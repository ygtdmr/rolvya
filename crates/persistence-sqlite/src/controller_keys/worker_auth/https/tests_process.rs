//! Actual process boundaries. Only the controller receives database/key paths.
use super::*;
use std::{
    io::{BufRead, Write},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};
const DRIVER: &str = "controller_keys::worker_auth::https::tests::process::https_process_fixture";
struct Process {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}
impl Process {
    async fn start(role: &str, config: serde_json::Value) -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", DRIVER, "--nocapture"])
            .env("ROLVYA_HTTPS_TEST_PROCESS", role)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut p = Self {
            child,
            input,
            output,
        };
        p.send(&config).await;
        p
    }
    async fn send(&mut self, value: &impl Serialize) {
        let mut bytes = Zeroizing::new(serde_json::to_vec(value).unwrap());
        bytes.push(b'\n');
        self.input.write_all(&bytes).await.unwrap();
        self.input.flush().await.unwrap();
    }
    async fn receive(&mut self) -> serde_json::Value {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let mut line = Zeroizing::new(String::new());
                let n = self.output.read_line(&mut line).await.unwrap();
                assert!(n > 0, "fixture child closed unexpectedly");
                assert!(line.len() <= wire::MAX_HTTP_BODY * 2);
                if let Some(line) = line.strip_prefix("ROLVYA_TEST ") {
                    return serde_json::from_str(line).unwrap();
                }
            }
        })
        .await
        .expect("bounded process fixture response")
    }
    async fn finish(mut self) {
        drop(self.input);
        let status = tokio::time::timeout(Duration::from_secs(20), self.child.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(status.success(), "fixture child failed");
    }
}
fn emit(value: &impl Serialize) {
    println!("ROLVYA_TEST {}", serde_json::to_string(value).unwrap());
    std::io::stdout().flush().unwrap();
}
fn line(input: &mut impl BufRead) -> Option<Zeroizing<String>> {
    let mut s = Zeroizing::new(String::new());
    let n = std::io::Read::take(&mut *input, (wire::MAX_HTTP_BODY * 2 + 1) as u64)
        .read_line(&mut s)
        .unwrap();
    assert!(s.len() <= wire::MAX_HTTP_BODY * 2);
    if n == 0 { None } else { Some(s) }
}
#[derive(Serialize, Deserialize)]
struct WorkerConfig {
    url: String,
    ca: Vec<u8>,
    challenge: BootstrapChallengeId,
    secret: String,
    proof: Attestation,
    binding: WorkerBinding,
    request: RequestId,
}
#[derive(Serialize, Deserialize)]
struct ControllerConfig {
    database: String,
    keys: String,
    certificate: Vec<u8>,
    tls_key: Vec<u8>,
    provider_public: Vec<u8>,
    now: i64,
    fault: Option<String>,
    port: Option<u16>,
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn https_process_fixture() {
    let Ok(role) = std::env::var("ROLVYA_HTTPS_TEST_PROCESS") else {
        return;
    };
    let mut input = std::io::BufReader::new(std::io::stdin());
    let initial = line(&mut input).unwrap();
    if role == "worker" {
        let config: WorkerConfig = serde_json::from_str(&initial).unwrap();
        let secret = BootstrapSecret::parse(config.secret).unwrap();
        let mut client = WorkerHttpClient::with_tls(
            ControllerEndpoint::parse(&config.url).unwrap(),
            tls::fixture_client(CertificateDer::from(config.ca)),
        )
        .unwrap();
        client
            .bootstrap(
                config.request,
                config.challenge,
                &secret,
                config.proof,
                &config.binding,
            )
            .await
            .unwrap();
        let a = client
            .assignment(
                RequestId::new(format!("assignment-{}", config.binding.attempt_id.as_str()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let script = FakeWorker::new(a.binding, a.capabilities, a.seed)
            .unwrap()
            .with_inputs(a.inputs)
            .unwrap()
            .script(FakeBehavior::Success, synthetic::T)
            .unwrap();
        emit(
            &serde_json::json!({"ready":std::process::id(),"times":script.events.iter().map(|e|e.at).collect::<Vec<_>>()}),
        );
        while let Some(command) = line(&mut input) {
            let index: usize = serde_json::from_str(&command).unwrap();
            let SimulatedInput::Message(bytes) = &script.events[index].input else {
                panic!("message fixture expected")
            };
            let m: WorkerMessage = serde_json::from_slice(bytes).unwrap();
            let artifact = if matches!(m.body, WorkerBody::SubmitResult { .. }) {
                Some(script.artifact.as_ref().unwrap().bytes.as_slice())
            } else {
                None
            };
            match client.message(m, artifact).await {
                Ok(r) => emit(&serde_json::json!({"receipt":r})),
                Err(e) => emit(&serde_json::json!({"error":e.to_string()})),
            };
        }
    } else if role == "controller" {
        let config: ControllerConfig = serde_json::from_str(&initial).unwrap();
        let store = PlanStore::open(Path::new(&config.database)).await.unwrap();
        let b = ControllerBootstrap::open(
            store.clone(),
            Path::new(&config.keys),
            ServiceIdentity::current(),
            &[],
        )
        .await
        .unwrap();
        let registry = provider_accounts::fixture_reopen(&b).await;
        let policy = Arc::new(registry.policy(synthetic::Authority::default()));
        let clock = Arc::new(DeterministicClock::new(config.now).unwrap());
        let mut security = WorkerSecurity::open(&b).await.unwrap();
        security.policy = policy;
        security.clock = clock.clone();
        let transport =
            context::HttpTransport::new(context::ResourceLimits::default(), clock.clone());
        security.transport = transport.clone();
        security.identity = Arc::new(SignedProvider {
            public: config.provider_public,
            transport: transport.clone(),
            prepare: Mutex::new(BTreeMap::new()),
            pause: Mutex::new(None),
        });
        if let Some(fault) = config.fault {
            let (selected, target) = match fault.as_str() {
                "before-message" => (CommitCheckpoint::BeforeMessage, 1),
                "after-message" => (CommitCheckpoint::AfterMessage, 1),
                "before-result" => (CommitCheckpoint::BeforeMessage, 6),
                "after-result" => (CommitCheckpoint::AfterMessage, 6),
                "after-bootstrap" => (CommitCheckpoint::AfterBootstrap, 1),
                "after-renew" => (CommitCheckpoint::AfterRenew, 1),
                _ => panic!("private fixture fault"),
            };
            let count = std::sync::atomic::AtomicU64::new(0);
            security.hook = Box::new(move |point| {
                if point == selected
                    && count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1 == target
                {
                    let _ = std::process::Command::new("kill")
                        .args(["-KILL", &std::process::id().to_string()])
                        .status();
                    std::process::abort();
                }
                Ok(())
            });
        }
        let identity = tls::ServerIdentity::fixture(
            vec![CertificateDer::from(config.certificate)],
            PrivatePkcs8KeyDer::from(config.tls_key).into(),
        );
        let server = Arc::new(HttpsWorkerServer {
            secure_fargate: None,
            security,
            transport,
            identity,
        });
        let listener = TcpListener::bind(("127.0.0.1", config.port.unwrap_or(0)))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let (shutdown, stopped) = oneshot::channel();
        let running = server.clone();
        let task = tokio::spawn(async move {
            running
                .serve(listener, async {
                    let _ = stopped.await;
                })
                .await
                .unwrap();
        });
        emit(&serde_json::json!({"ready":std::process::id(),"port":port}));
        while let Some(command) = line(&mut input) {
            let v: serde_json::Value = serde_json::from_str(&command).unwrap();
            if v.get("stop").is_some() {
                break;
            }
            if let Some(t) = v.get("time").and_then(|n| n.as_i64()) {
                clock.advance_to(t).unwrap();
            }
            emit(&serde_json::json!({"ok":true}));
        }
        let _ = shutdown.send(());
        task.await.unwrap();
        store.close().await;
    } else {
        panic!("unknown private test role");
    }
}
impl Fixture {
    fn controller_config(
        &self,
        now: i64,
        fault: Option<&str>,
        port: Option<u16>,
    ) -> serde_json::Value {
        serde_json::to_value(ControllerConfig {
            database: self
                .root
                .path()
                .join("data/state.sqlite")
                .to_str()
                .unwrap()
                .into(),
            keys: self.root.path().join("keys").to_str().unwrap().into(),
            certificate: self.cert.cert.to_vec(),
            tls_key: self.cert.key.clone(),
            provider_public: self.key.public_key().as_ref().to_vec(),
            now,
            fault: fault.map(str::to_owned),
            port,
        })
        .unwrap()
    }
    async fn network_worker(
        &self,
        s: &HttpsWorkerServer,
        v: &SignedProvider,
        task: &str,
        url: &str,
    ) -> (Process, serde_json::Value) {
        let (boot, secret, proof, binding, request) = self.prepare(s, v, task).await;
        let config = WorkerConfig {
            url: url.into(),
            ca: self.cert.cert.to_vec(),
            challenge: boot.id,
            secret: secret.0.0.to_string(),
            proof,
            binding,
            request,
        };
        let mut p = Process::start("worker", serde_json::to_value(config).unwrap()).await;
        let ready = p.receive().await;
        (p, ready)
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn separate_worker_processes_execute_frozen_dag_over_tls() {
    let f = Fixture::new().await;
    let fingerprint = f.co.fingerprint().to_owned();
    let (s, v) = f.server().await;
    let (url, stop, task) = listen(s.clone()).await;
    let (mut a, ra) = f.network_worker(&s, &v, "task-a", &url).await;
    let (mut b, rb) = f.network_worker(&s, &v, "task-b", &url).await;
    assert_ne!(ra["ready"], rb["ready"]);
    assert_ne!(ra["ready"].as_u64().unwrap(), u64::from(std::process::id()));
    assert_eq!(f.co.snapshot().await.unwrap().running, 2);
    assert_eq!(f.co.snapshot().await.unwrap().waiting_dependencies, 1);
    for index in 0..6 {
        let at = ra["times"][index]
            .as_i64()
            .unwrap()
            .max(rb["times"][index].as_i64().unwrap());
        f.clock.advance_to(at).unwrap();
        a.send(&index).await;
        b.send(&index).await;
        let (x, y) = tokio::join!(a.receive(), b.receive());
        assert!(
            x.get("receipt").is_some() && y.get("receipt").is_some(),
            "both worker processes advance through TLS"
        );
        if index == 2 {
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM worker_sessions WHERE json_extract(state,'$.phase')='Executing'").fetch_one(&f.store.pool).await.unwrap(),2,"both independently authenticated worker sessions execute simultaneously");
        }
        if index < 5 {
            assert_eq!(f.co.snapshot().await.unwrap().accepted, 0);
        }
    }
    assert_eq!(f.co.snapshot().await.unwrap().received_unvalidated, 2);
    assert_eq!(f.co.snapshot().await.unwrap().accepted, 0);
    let validator = f.co.validator();
    let (la, lb) = (f.lease("task-a").await, f.lease("task-b").await);
    let (x, y) = tokio::join!(
        validator.validate_leaf(&la, f.policy.as_ref(), f.clock.as_ref()),
        validator.validate_leaf(&lb, f.policy.as_ref(), f.clock.as_ref())
    );
    x.unwrap();
    y.unwrap();
    assert_eq!(f.co.snapshot().await.unwrap().accepted, 2);
    let blocked =
        f.co.pass(
            f.policy.as_ref(),
            f.clock.as_ref(),
            PassLimits {
                admissions: 4,
                events: 0,
                validations: 0,
                groups: 0,
            },
        )
        .await
        .unwrap();
    assert!(blocked.admitted.is_empty());
    let group = TaskRef {
        plan: f.co.plan_key().clone(),
        task_id: TaskId::new("group-g").unwrap(),
    };
    validator
        .aggregate(&group, f.policy.as_ref(), f.clock.as_ref())
        .await
        .unwrap();
    let next =
        f.co.pass(
            f.policy.as_ref(),
            f.clock.as_ref(),
            PassLimits {
                admissions: 1,
                events: 0,
                validations: 0,
                groups: 0,
            },
        )
        .await
        .unwrap();
    assert_eq!(next.admitted.len(), 1);
    assert_eq!(next.admitted[0].task.task_id.as_str(), "task-c");
    let (mut c, rc) = f.network_worker(&s, &v, "task-c", &url).await;
    // C's synthetic lifecycle starts at its admission time, not at the original
    // plan's T. Use independent event barriers; no timing sleeps.
    for index in 0..6 {
        let at = (synthetic::T + 50) + (rc["times"][index].as_i64().unwrap() - synthetic::T);
        f.clock.advance_to(at).unwrap();
        c.send(&index).await;
        assert!(c.receive().await.get("receipt").is_some());
    }
    validator
        .validate_leaf(
            &f.lease("task-c").await,
            f.policy.as_ref(),
            f.clock.as_ref(),
        )
        .await
        .unwrap();
    assert_eq!(f.co.snapshot().await.unwrap().accepted, 4);
    let restored = f
        .store
        .load_active(
            &f.co.plan_key().project_id,
            f.policy.as_ref(),
            Limits::default(),
        )
        .await
        .unwrap();
    assert_eq!(restored.revision.fingerprint(), fingerprint);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_dependencies")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        1
    );
    let budget = f
        .store
        .financial_snapshot(&f.co.plan_key().project_id)
        .await
        .unwrap();
    assert_eq!(budget.ceiling, synthetic::money(100));
    assert_eq!(
        budget.held,
        rolvya_domain::money::CostBreakdown {
            ai_api: synthetic::money(3),
            compute: synthetic::money(6),
            storage: synthetic::money(9),
            transfer: synthetic::money(3)
        }
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_attempts")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM synthetic_operations")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM tasks")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        4
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_attempts WHERE task_id='group-g'")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        0
    );
    a.finish().await;
    b.finish().await;
    c.finish().await;
    stop.send(()).unwrap();
    task.await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sigkill_before_and_after_network_receipt_commit_reopens_without_duplicate_effect() {
    for fault in ["before-message", "after-message"] {
        let f = Fixture::new().await;
        let (s, v) = f.server().await;
        let (boot, secret, proof, binding, id) = f.prepare(&s, &v, "task-a").await;
        let mut controller = Process::start(
            "controller",
            f.controller_config(synthetic::T, Some(fault), None),
        )
        .await;
        let ready = controller.receive().await;
        let port = ready["port"].as_u64().unwrap() as u16;
        let url = format!("https://localhost:{port}/");
        let mut client = f.cert.client(&url);
        client
            .bootstrap(id, boot.id, &secret, proof, &binding)
            .await
            .unwrap();
        let a = client
            .assignment(RequestId::new("crash-assignment").unwrap())
            .await
            .unwrap();
        let script = FakeWorker::new(a.binding, a.capabilities, a.seed)
            .unwrap()
            .script(FakeBehavior::Success, synthetic::T)
            .unwrap();
        let SimulatedInput::Message(bytes) = &script.events[0].input else {
            unreachable!()
        };
        let message: WorkerMessage = serde_json::from_slice(bytes).unwrap();
        assert!(client.message(message.clone(), None).await.is_err());
        let status = tokio::time::timeout(Duration::from_secs(10), controller.child.wait())
            .await
            .unwrap()
            .unwrap();
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(status.signal(), Some(9));
        let committed = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_messages")
            .fetch_one(&f.store.pool)
            .await
            .unwrap();
        assert_eq!(committed, if fault == "after-message" { 1 } else { 0 });
        let mut resumed = Process::start(
            "controller",
            f.controller_config(synthetic::T, None, Some(port)),
        )
        .await;
        resumed.receive().await;
        let first = client.message(message.clone(), None).await.unwrap();
        let again = client.message(message, None).await.unwrap();
        assert_eq!(first, again);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_messages")
                .fetch_one(&f.store.pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_attempts")
                .fetch_one(&f.store.pool)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM synthetic_operations")
                .fetch_one(&f.store.pool)
                .await
                .unwrap(),
            2
        );
        resumed.send(&serde_json::json!({"stop":true})).await;
        resumed.finish().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sigkill_at_result_commit_preserves_receipt_artifact_and_accepted_sibling() {
    for fault in ["before-result", "after-result"] {
        let f = Fixture::new().await;
        let (local, v) = f.server().await;
        let (boot, secret, proof, binding, id) = f.prepare(&local, &v, "task-a").await;
        let mut server = Process::start(
            "controller",
            f.controller_config(synthetic::T, Some(fault), None),
        )
        .await;
        let ready = server.receive().await;
        let port = ready["port"].as_u64().unwrap() as u16;
        let url = format!("https://localhost:{port}/");
        let mut client = f.cert.client(&url);
        client
            .bootstrap(id, boot.id, &secret, proof, &binding)
            .await
            .unwrap();
        let a = client
            .assignment(RequestId::new("result-crash-assignment").unwrap())
            .await
            .unwrap();
        let script = FakeWorker::new(a.binding, a.capabilities, a.seed)
            .unwrap()
            .script(FakeBehavior::Success, synthetic::T)
            .unwrap();
        for e in &script.events[..5] {
            server.send(&serde_json::json!({"time":e.at})).await;
            server.receive().await;
            let SimulatedInput::Message(bytes) = &e.input else {
                unreachable!()
            };
            client
                .message(serde_json::from_slice(bytes).unwrap(), None)
                .await
                .unwrap();
        }
        server
            .send(&serde_json::json!({"time":synthetic::T+50}))
            .await;
        server.receive().await;
        let SimulatedInput::Message(bytes) = &script.events[5].input else {
            unreachable!()
        };
        let result: WorkerMessage = serde_json::from_slice(bytes).unwrap();
        let artifact = &script.artifact.as_ref().unwrap().bytes;
        assert!(
            client
                .message(result.clone(), Some(artifact))
                .await
                .is_err()
        );
        let status = server.child.wait().await.unwrap();
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(status.signal(), Some(9));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_messages")
                .fetch_one(&f.store.pool)
                .await
                .unwrap(),
            if fault == "after-result" { 6 } else { 5 }
        );
        assert_eq!(f.co.snapshot().await.unwrap().accepted, 0);
        let mut restarted = Process::start(
            "controller",
            f.controller_config(synthetic::T + 50, None, Some(port)),
        )
        .await;
        restarted.receive().await;
        let one = client
            .message(result.clone(), Some(artifact))
            .await
            .unwrap();
        assert_eq!(one, client.message(result, Some(artifact)).await.unwrap());
        f.clock.advance_to(synthetic::T + 50).unwrap();
        f.co.validator()
            .validate_leaf(
                &f.lease("task-a").await,
                f.policy.as_ref(),
                f.clock.as_ref(),
            )
            .await
            .unwrap();
        assert_eq!(f.co.snapshot().await.unwrap().accepted, 1);
        // Reopening and unrelated B fencing cannot erase accepted A's bytes.
        f.store
            .fence_attempt(
                &admission::FenceRequest {
                    lease: f.lease("task-b").await,
                    request_id: RequestId::new("unrelated-worker-failure").unwrap(),
                    actor: common::owner(),
                    evidence: synthetic::e("synthetic-unrelated-failure"),
                },
                f.policy.as_ref(),
                f.clock.as_ref(),
            )
            .await
            .unwrap();
        assert_eq!(f.co.snapshot().await.unwrap().accepted, 1);
        assert_eq!(
            sqlx::query_scalar::<_, Vec<u8>>(
                "SELECT content FROM synthetic_artifacts WHERE attempt_id=?"
            )
            .bind(binding.attempt_id.as_str())
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
            *artifact
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_attempts")
                .fetch_one(&f.store.pool)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM synthetic_operations")
                .fetch_one(&f.store.pool)
                .await
                .unwrap(),
            2
        );
        restarted.send(&serde_json::json!({"stop":true})).await;
        restarted.finish().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lost_bootstrap_issuance_response_cannot_retrieve_or_issue_another_generation() {
    let f = Fixture::new().await;
    let (s, v) = f.server().await;
    let (boot, secret, proof, binding, id) = f.prepare(&s, &v, "task-a").await;
    let mut child = Process::start(
        "controller",
        f.controller_config(synthetic::T, Some("after-bootstrap"), None),
    )
    .await;
    let ready = child.receive().await;
    let port = ready["port"].as_u64().unwrap() as u16;
    let url = format!("https://localhost:{port}/");
    let mut c = f.cert.client(&url);
    assert_eq!(
        c.bootstrap(
            id.clone(),
            boot.id.clone(),
            &secret,
            proof.clone(),
            &binding
        )
        .await,
        Err(ClientError::CredentialDeliveryUnknown)
    );
    assert!(c.metadata().is_none());
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(child.child.wait().await.unwrap().signal(), Some(9));
    let mut resumed = Process::start(
        "controller",
        f.controller_config(synthetic::T, None, Some(port)),
    )
    .await;
    resumed.receive().await;
    assert!(
        c.bootstrap(id, boot.id, &secret, proof, &binding)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_credentials")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT max(current_generation) FROM worker_enrollments")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        1
    );
    resumed.send(&serde_json::json!({"stop":true})).await;
    resumed.finish().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lost_renewal_response_retires_old_generation_without_token_retrieval() {
    let f = Fixture::new().await;
    let (s, v) = f.server().await;
    let (boot, secret, proof, binding, id) = f.prepare(&s, &v, "task-a").await;
    let mut child = Process::start(
        "controller",
        f.controller_config(synthetic::T, Some("after-renew"), None),
    )
    .await;
    let ready = child.receive().await;
    let port = ready["port"].as_u64().unwrap() as u16;
    let url = format!("https://localhost:{port}/");
    let mut c = f.cert.client(&url);
    c.bootstrap(id, boot.id, &secret, proof, &binding)
        .await
        .unwrap();
    let a = c
        .assignment(RequestId::new("renewal-crash-assignment").unwrap())
        .await
        .unwrap();
    let script = FakeWorker::new(a.binding, a.capabilities, a.seed)
        .unwrap()
        .script(FakeBehavior::Success, synthetic::T)
        .unwrap();
    for e in &script.events[..4] {
        child.send(&serde_json::json!({"time":e.at})).await;
        child.receive().await;
        let SimulatedInput::Message(bytes) = &e.input else {
            unreachable!()
        };
        c.message(serde_json::from_slice(bytes).unwrap(), None)
            .await
            .unwrap();
    }
    child
        .send(&serde_json::json!({"time":synthetic::T+60}))
        .await;
    child.receive().await;
    let old = c.fixture_stolen_credential();
    let m = c.metadata().unwrap();
    let mut tx = f.store.pool.begin().await.unwrap();
    let record = s.security.enrollment(&mut tx, &m.enrollment).await.unwrap();
    tx.commit().await.unwrap();
    let request = RequestId::new("renewal-lost-ack").unwrap();
    let evidence = synthetic::e("fresh-signed-renewal");
    let proof = sign(
        &f.key,
        record
            .check(VerificationStage::Renew, &request, &evidence, Some(m))
            .scope(),
    );
    assert_eq!(
        c.renew(request, proof).await,
        Err(ClientError::CredentialDeliveryUnknown)
    );
    assert!(c.metadata().is_none());
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(child.child.wait().await.unwrap().signal(), Some(9));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT max(current_generation) FROM worker_enrollments")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        2
    );
    let mut resumed = Process::start(
        "controller",
        f.controller_config(synthetic::T + 60, None, Some(port)),
    )
    .await;
    resumed.receive().await;
    assert_eq!(
        old.assignment(RequestId::new("obsolete-after-restart").unwrap())
            .await
            .unwrap_err(),
        ClientError::Rejected(ErrorCode::StaleVersion)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_credentials")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_attempts")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        2
    );
    resumed.send(&serde_json::json!({"stop":true})).await;
    resumed.finish().await;
}
