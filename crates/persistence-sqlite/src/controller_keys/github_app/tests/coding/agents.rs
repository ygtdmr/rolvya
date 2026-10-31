use super::*;
use crate::controller_keys::worker_auth::{
    https::{
        agent::{self, Adapter, AgentOperation},
        tests::CodingNetwork,
    },
    *,
};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coding_agent_effect_denied_before_accepted_lease() {
    let f = Fixture::new_coding_execution().await;
    let leases = f.coding_admit().await;
    let mut co = f
        .bootstrap
        .coding_coordinator(&f.project, &f.policy, Limits::default())
        .await
        .unwrap();
    co.fixture_agent_enable(0);
    let net = CodingNetwork::new(
        &f.bootstrap,
        Arc::new(f.policy.fixture_reuse(synthetic::Authority::default())),
        f.clock.clone(),
        co,
    )
    .await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "https://localhost:{}/",
        listener.local_addr().unwrap().port()
    );
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = net.server.clone();
    let serving = tokio::spawn(async move {
        server
            .serve(listener, async {
                let _ = stopped.await;
            })
            .await
            .unwrap()
    });
    let config = net.bootstrap(&leases[0]).await;
    let mut client = CodingNetwork::worker(&url, net.ca.clone());
    let binding = serde_json::from_value(config["binding"].clone()).unwrap();
    client
        .bootstrap(
            serde_json::from_value(config["request"].clone()).unwrap(),
            serde_json::from_value(config["challenge"].clone()).unwrap(),
            &BootstrapSecret::parse(config["secret"].as_str().unwrap().into()).unwrap(),
            serde_json::from_value(config["proof"].clone()).unwrap(),
            &binding,
        )
        .await
        .unwrap();
    let w = client
        .coding_work(
            crate::RequestId::new("pre-lease-source").unwrap(),
            f.clock.now().unwrap(),
        )
        .await
        .unwrap();
    let answer = agent::fixture::step(
        &client,
        AgentOperation::Start {
            work: w.identity,
            step: 1,
            adapter: Adapter::OpenAiApi,
            request_sha256: commitment_bytes(b"request"),
        },
    )
    .await;
    assert!(
        answer.is_err(),
        "an active bearer before accepted lease must not authorize model effects"
    );
    assert_eq!(f.count("coding_agent_calls").await, 0);
    let _ = stop.send(());
    serving.await.unwrap();
}
#[tokio::test]
async fn coding_agent_api_transport_cannot_be_substituted_across_attempts() {
    let f = Fixture::new_coding_mode("agents-pair").await;
    let leases = f.coding_admit().await;
    let mut tx = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let a =
        f.co.coding_work_in(&mut tx, &leases[0], f.clock.now().unwrap())
            .await
            .unwrap();
    let b =
        f.co.coding_work_in(&mut tx, &leases[1], f.clock.now().unwrap())
            .await
            .unwrap();
    tx.commit().await.unwrap();
    assert!(agent::fixture::swapped_transport_denied(
        &a,
        &b,
        f.clock.now().unwrap()
    ));
}

struct Session {
    url: String,
    ca: Vec<u8>,
    f: Fixture,
    lease: LeaseRef,
    worker: crate::controller_keys::worker_auth::https::coding_worker::CodingWorker,
    stop: tokio::sync::oneshot::Sender<()>,
    serving: tokio::task::JoinHandle<()>,
}
impl Session {
    async fn open(maximum: Option<i64>) -> Self {
        let f = Fixture::new_coding_mode("agents-openai").await;
        let leases = f.coding_admit().await;
        let mut co = f
            .bootstrap
            .coding_coordinator(&f.project, &f.policy, Limits::default())
            .await
            .unwrap();
        if let Some(n) = maximum {
            co.fixture_agent_enable(n)
        }
        if let Ok(point) = std::env::var("ROLVYA_G105_CRASH_POINT") {
            co.fixture_coding_hook(Box::new(move |p| {
                if p == point {
                    let _ = std::process::Command::new("kill")
                        .args(["-KILL", &std::process::id().to_string()])
                        .status();
                    unreachable!("SIGKILL");
                }
            }));
        }
        let net = CodingNetwork::new(
            &f.bootstrap,
            Arc::new(f.policy.fixture_reuse(synthetic::Authority {
                coding_agents: true,
                ..Default::default()
            })),
            f.clock.clone(),
            co,
        )
        .await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "https://localhost:{}/",
            listener.local_addr().unwrap().port()
        );
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let server = net.server.clone();
        let serving = tokio::spawn(async move {
            server
                .serve(listener, async {
                    let _ = stopped.await;
                })
                .await
                .unwrap()
        });
        let config = net.bootstrap(&leases[0]).await;
        let mut client = CodingNetwork::worker(&url, net.ca.clone());
        let binding = serde_json::from_value(config["binding"].clone()).unwrap();
        client
            .bootstrap(
                serde_json::from_value(config["request"].clone()).unwrap(),
                serde_json::from_value(config["challenge"].clone()).unwrap(),
                &BootstrapSecret::parse(config["secret"].as_str().unwrap().into()).unwrap(),
                serde_json::from_value(config["proof"].clone()).unwrap(),
                &binding,
            )
            .await
            .unwrap();
        let worker =
            crate::controller_keys::worker_auth::https::coding_worker::CodingWorker::begin(
                client,
                synthetic::caps(),
                f.clock.now().unwrap(),
            )
            .await
            .unwrap();
        Self {
            url,
            ca: net.ca.clone(),
            f,
            lease: leases[0].clone(),
            worker,
            stop,
            serving,
        }
    }
    fn start(&self, step: u16) -> AgentOperation {
        AgentOperation::Start {
            work: self.worker.work().identity.clone(),
            step,
            adapter: Adapter::OpenAiApi,
            request_sha256: commitment_bytes(format!("request-{step}").as_bytes()),
        }
    }
    fn finish(&self, step: u16, completed: bool) -> AgentOperation {
        AgentOperation::Finish {
            work: self.worker.work().identity.clone(),
            step,
            evidence: agent::ExecutionEvidence {
                adapter: Adapter::OpenAiApi,
                response_sha256: Some(commitment_bytes(b"synthetic-response")),
                usage: agent::Usage {
                    input_tokens: Some(10),
                    output_tokens: Some(5),
                },
                completed,
            },
        }
    }
    async fn step(&self, op: AgentOperation) -> Result<agent::StepReceipt, agent::AgentError> {
        agent::fixture::worker_step(&self.worker, op).await
    }
    async fn close(self) {
        let _ = self.stop.send(());
        self.serving.await.unwrap();
    }
}
#[tokio::test]
async fn coding_agent_journal_current_scope_replay_and_budget() {
    let mut s = Session::open(Some(1)).await;
    let before = s.f.store.financial_snapshot(&s.f.project).await.unwrap();
    let mut wrong = s.start(1);
    if let AgentOperation::Start { adapter, .. } = &mut wrong {
        *adapter = Adapter::AnthropicApi;
    }
    assert!(s.step(wrong).await.is_err());
    assert_eq!(s.f.count("coding_agent_calls").await, 0);
    assert!(s.step(s.start(1)).await.unwrap().execute);
    assert!(!s.step(s.start(1)).await.unwrap().execute);
    let mut conflicting = s.start(1);
    if let AgentOperation::Start { request_sha256, .. } = &mut conflicting {
        *request_sha256 = commitment_bytes(b"conflicting");
    }
    assert!(s.step(conflicting).await.is_err());
    assert!(
        s.step(s.start(2)).await.is_err(),
        "missing response cannot authorize another request"
    );
    s.step(s.finish(1, true)).await.unwrap();
    s.step(s.finish(1, true)).await.unwrap();
    assert!(s.step(s.finish(1, false)).await.is_err());
    assert!(
        s.step(s.start(2)).await.is_err(),
        "remaining original AI hold cannot cover second maximum"
    );
    assert_eq!(s.f.count("coding_agent_calls").await, 1);
    assert_eq!(s.f.count("coding_agent_observations").await, 1);
    let r = s.f.coding_result(&s.lease).await;
    s.worker
        .submit(r.changes, r.verification_execution)
        .await
        .unwrap();
    assert!(
        s.step(s.start(2)).await.is_err(),
        "terminal worker cannot create model authority"
    );
    assert_eq!(
        s.f.store.financial_snapshot(&s.f.project).await.unwrap(),
        before
    );
    s.close().await;
}
#[tokio::test]
async fn coding_agent_journal_concurrent_start_has_one_effect() {
    let s = Session::open(Some(0)).await;
    let (a, b) = tokio::join!(s.step(s.start(1)), s.step(s.start(1)));
    assert_eq!(
        [a, b]
            .iter()
            .filter(|r| r.as_ref().is_ok_and(|p| p.execute))
            .count(),
        1
    );
    assert_eq!(s.f.count("coding_agent_calls").await, 1);
    assert!(!s.step(s.start(1)).await.unwrap().execute);
    s.step(s.finish(1, false)).await.unwrap();
    assert!(s.step(s.start(2)).await.is_err());
    s.close().await;
}
#[tokio::test]
async fn coding_agent_journal_ignored_writes_roll_back_and_do_not_retry() {
    for stage in ["calls", "observations"] {
        let s = Session::open(Some(0)).await;
        if stage == "observations" {
            s.step(s.start(1)).await.unwrap();
        }
        sqlx::query(&format!("CREATE TRIGGER private_agent_ignore BEFORE INSERT ON coding_agent_{stage} BEGIN SELECT RAISE(IGNORE); END")).execute(&s.f.store.pool).await.unwrap();
        let answer = s
            .step(if stage == "calls" {
                s.start(1)
            } else {
                s.finish(1, true)
            })
            .await;
        assert!(answer.is_err());
        assert_eq!(s.f.count(&format!("coding_agent_{stage}")).await, 0);
        sqlx::query("DROP TRIGGER private_agent_ignore")
            .execute(&s.f.store.pool)
            .await
            .unwrap();
        if stage == "calls" {
            assert!(s.step(s.start(1)).await.unwrap().execute);
        } else {
            assert!(!s.step(s.start(1)).await.unwrap().execute);
            s.step(s.finish(1, true)).await.unwrap();
        }
        for table in ["coding_agent_calls", "coding_agent_observations"] {
            assert!(
                sqlx::query(&format!("DELETE FROM {table}"))
                    .execute(&s.f.store.pool)
                    .await
                    .is_err()
                    || s.f.count(table).await == 0
            );
        }
        s.f.store.check_integrity().await.unwrap();
        s.close().await;
    }
}
#[tokio::test]
async fn coding_agent_unconfigured_authority_expiry_and_context_are_denied() {
    let s = Session::open(None).await;
    assert!(s.step(s.start(1)).await.is_err());
    assert_eq!(s.f.count("coding_agent_calls").await, 0);
    s.close().await;
    let s = Session::open(Some(0)).await;
    assert!(agent::fixture::context_is_bounded(s.worker.work()));
    assert!(agent::fixture::swapped_transport_denied(
        s.worker.work(),
        s.worker.work(),
        s.worker.work().expires_at
    ));
    s.f.clock.advance_to(s.worker.work().expires_at).unwrap();
    assert!(s.step(s.start(1)).await.is_err());
    assert_eq!(s.f.count("coding_agent_calls").await, 0);
    s.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coding_agent_crash_child() {
    let Ok(point) = std::env::var("ROLVYA_G105_CRASH_POINT") else {
        return;
    };
    let s = Session::open(Some(0)).await;
    let sibling = s.f.lease("task-b").await;
    let result = s.f.coding_result(&sibling).await;
    s.f.coding_receive(&sibling, &result).await;
    s.f.coding_verify(&sibling, &result).await;
    s.f.coding_accept(&sibling).await;
    let balance = s.f.store.financial_snapshot(&s.f.project).await.unwrap();
    std::fs::write(std::env::var("ROLVYA_G105_CHECKPOINT").unwrap(),serde_json::to_vec(&json!({"root":s.f.root.path(),"project":s.f.project,"lease":s.lease,"held":balance.held.total().unwrap().minor_units(),"uncertain":balance.uncertain_liabilities})).unwrap()).unwrap();
    assert!(s.step(s.start(1)).await.unwrap().execute);
    assert!(point.starts_with("provider-observe"));
    s.step(s.finish(1, true)).await.unwrap();
    panic!("unreached private crash point");
}
#[tokio::test]
async fn coding_agent_sigkill_intent_and_observation_commits_preserve_holds() {
    use std::os::unix::process::ExitStatusExt;
    for (point, calls, observations) in [
        ("provider-start-before", 0, 0),
        ("provider-start-after", 1, 0),
        ("provider-observe-before", 1, 0),
        ("provider-observe-after", 1, 1),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let checkpoint = directory.path().join("checkpoint.json");
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(45),
            tokio::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "controller_keys::github_app::tests::coding::agents::coding_agent_crash_child",
                    "--nocapture",
                ])
                .env("ROLVYA_G105_CRASH_POINT", point)
                .env("ROLVYA_G105_CHECKPOINT", &checkpoint)
                .kill_on_drop(true)
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            output.status.signal(),
            Some(9),
            "{point}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let info: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&checkpoint).unwrap()).unwrap();
        let root = std::path::PathBuf::from(info["root"].as_str().unwrap());
        let store = PlanStore::open(&root.join("data/state.sqlite"))
            .await
            .unwrap();
        store.check_integrity().await.unwrap();
        for (table, n) in [
            ("coding_agent_calls", calls),
            ("coding_agent_observations", observations),
            ("task_acceptances", 1),
        ] {
            assert_eq!(
                sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
                    .fetch_one(&store.pool)
                    .await
                    .unwrap(),
                n,
                "{point}:{table}"
            );
        }
        let project = ProjectId::new(info["project"].as_str().unwrap()).unwrap();
        let balance = store.financial_snapshot(&project).await.unwrap();
        assert_eq!(
            balance.held.total().unwrap().minor_units(),
            info["held"].as_i64().unwrap()
        );
        assert_eq!(
            balance.uncertain_liabilities,
            info["uncertain"].as_u64().unwrap()
        );
        let boot = ControllerBootstrap::open(
            store.clone(),
            &root.join("keys"),
            ServiceIdentity::current(),
            &[],
        )
        .await
        .unwrap();
        let registry = crate::controller_keys::provider_accounts::fixture_reopen(&boot).await;
        let policy = registry.policy(synthetic::Authority {
            coding_agents: true,
            ..Default::default()
        });
        let co = boot
            .coding_coordinator(&project, &policy, Limits::default())
            .await
            .unwrap();
        let l: LeaseRef = serde_json::from_value(info["lease"].clone()).unwrap();
        let sibling = TaskRef {
            plan: l.task.plan.clone(),
            task_id: TaskId::new("task-b").unwrap(),
        };
        co.validator().verify_accepted(&sibling).await.unwrap();
        assert!(co.validator().verify_accepted(&l.task).await.is_err());
        // Reopening installs no fixture provider authority; an ordinary restart cannot
        // revive a private trust root or authorize any additional external operation.
        let mut tx = store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        let w = co.coding_work_in(&mut tx, &l, NOW).await.unwrap();
        assert!(
            co.coding_agent_in(
                &mut tx,
                &l,
                &AgentOperation::Start {
                    work: w.identity,
                    step: 1,
                    adapter: Adapter::OpenAiApi,
                    request_sha256: commitment_bytes(b"request-1")
                },
                NOW
            )
            .await
            .is_err()
        );
        tx.rollback().await.unwrap();
        store.close().await;
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[tokio::test]
async fn coding_agent_transport_rejects_cleartext_before_connection() {
    let s = Session::open(Some(0)).await;
    assert!(agent::fixture::cleartext_denied(s.worker.work()).await);
    s.close().await;
}

#[tokio::test]
async fn coding_agent_transport_validates_hostname_ca_and_denies_redirect() {
    let s = Session::open(Some(0)).await;
    agent::fixture::tls_boundaries_denied(s.worker.work()).await;
    s.close().await;
}

#[tokio::test]
async fn coding_agent_lost_provider_response_records_uncertainty_without_retry() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = Session::open(Some(0)).await;
    let (listener, endpoint) = agent::fixture::provider_listener().await;
    let acceptor = endpoint.acceptor.clone();
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = count.clone();
    let provider = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = acceptor.accept(socket).await.unwrap();
        let mut raw = Vec::new();
        let mut b = [0; 1024];
        loop {
            let n = socket.read(&mut b).await.unwrap();
            assert!(n > 0);
            raw.extend_from_slice(&b[..n]);
            assert!(raw.len() < 131072);
            if let Some(end) = raw.windows(4).position(|v| v == b"\r\n\r\n") {
                let head = std::str::from_utf8(&raw[..end]).unwrap();
                let size = head
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .map(|n| n.parse::<usize>().unwrap())
                    })
                    .unwrap();
                if raw.len() >= end + 4 + size {
                    break;
                }
            }
        }
        observed.fetch_add(1, Ordering::SeqCst);
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 9999\r\nConnection: close\r\n\r\n{}").await.unwrap();
    });
    let before = s.f.store.financial_snapshot(&s.f.project).await.unwrap();
    assert!(
        agent::fixture::execute_endpoint(&mut s.worker, endpoint.clone(), s.f.clock.now().unwrap())
            .await
            .is_err()
    );
    provider.await.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(s.f.count("coding_agent_calls").await, 1);
    assert_eq!(s.f.count("coding_agent_observations").await, 1);
    let body: Vec<u8> = sqlx::query_scalar("SELECT body FROM coding_agent_observations")
        .fetch_one(&s.f.store.pool)
        .await
        .unwrap();
    let e: agent::ExecutionEvidence = serde_json::from_slice(&body).unwrap();
    assert!(!e.completed);
    assert!(
        serde_json::to_value(&e).unwrap()["response_sha256"].is_null(),
        "unobserved response has no fabricated content hash"
    );
    assert_eq!(e.usage.input_tokens, None);
    assert!(
        agent::fixture::execute_endpoint(&mut s.worker, endpoint, s.f.clock.now().unwrap())
            .await
            .is_err()
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(s.f.count("coding_artifacts").await, 0);
    assert_eq!(s.f.count("task_acceptances").await, 0);
    assert_eq!(
        s.f.store.financial_snapshot(&s.f.project).await.unwrap(),
        before
    );
    s.close().await;
}

#[tokio::test]
async fn coding_agent_provider_failures_never_retry_or_accept() {
    for (status, body, expected) in [
        (
            401,
            json!({"error":{"code":"expired"}}),
            agent::AgentError::AuthenticationExpired,
        ),
        (
            403,
            json!({"error":{"type":"permission_error"}}),
            agent::AgentError::AuthenticationExpired,
        ),
        (
            429,
            json!({"error":{"code":"insufficient_quota"}}),
            agent::AgentError::Quota,
        ),
        (
            429,
            json!({"error":{"type":"rate_limit_error"}}),
            agent::AgentError::RateLimited,
        ),
        (
            503,
            json!({"error":{"code":"unavailable"}}),
            agent::AgentError::ProviderUnavailable,
        ),
        (
            200,
            json!({"worker_says_success":true}),
            agent::AgentError::UnsupportedModel,
        ),
    ] {
        let mut s = Session::open(Some(0)).await;
        let before = s.f.store.financial_snapshot(&s.f.project).await.unwrap();
        assert!(agent::fixture::host_substitutions_denied(
            s.worker.work(),
            s.f.clock.now().unwrap()
        ));
        let (answer, calls) =
            agent::fixture::execute_status(&mut s.worker, status, body, s.f.clock.now().unwrap())
                .await;
        assert_eq!(answer.err(), Some(expected));
        assert_eq!(calls, 1);
        assert_eq!(s.f.count("coding_agent_calls").await, 1);
        assert_eq!(s.f.count("task_acceptances").await, 0);
        assert_eq!(s.f.count("coding_artifacts").await, 0);
        assert_eq!(
            s.f.store.financial_snapshot(&s.f.project).await.unwrap(),
            before
        );
        s.close().await;
    }
}

#[tokio::test]
async fn coding_agent_legacy_work_and_request_dtos_do_not_establish_authority() {
    use crate::controller_keys::secret_broker as b;
    let s = Session::open(Some(0)).await;
    let original = s.worker.work();
    let mut legacy = original.clone();
    legacy.model_account = None;
    legacy.identity = Digest::new("0".repeat(64)).unwrap();
    legacy.identity = commitment(&legacy).unwrap();
    let encoded = serde_json::to_vec(&legacy).unwrap();
    assert!(
        !String::from_utf8(encoded.clone())
            .unwrap()
            .contains("model_account")
    );
    let decoded: crate::orchestration::coding::work::CodingWork =
        serde_json::from_slice(&encoded).unwrap();
    decoded
        .validate(&original.binding, s.f.clock.now().unwrap())
        .unwrap();
    assert_eq!(commitment(&decoded).unwrap(), commitment(&legacy).unwrap());
    let model = original.model_account.as_ref().unwrap();
    let request = b::SecretUseRequest {
        scope: b::RecordScope {
            connection: b::ConnectionScope {
                project: s.f.project.clone(),
                connection: b::ConnectionId::new(model.credential_connection.clone()).unwrap(),
                account: b::AccountId::new(model.account.clone()).unwrap(),
                provider: b::ProviderId::new(model.provider.clone()).unwrap(),
                mode: b::ProviderMode::ApiKey,
            },
            record: b::RecordId::new(model.credential_record.clone()).unwrap(),
            purpose: b::SecretPurpose::new(model.credential_purpose.clone()).unwrap(),
        },
        version: b::SecretVersion::new(model.credential_version as u64).unwrap(),
        request_id: crate::RequestId::new("untrusted-model-request").unwrap(),
        operation: b::CredentialOperation::ProxyModelRequest,
        worker: None,
    };
    let before = s.f.count("secret_events").await;
    assert_eq!(
        agent::RemoteCoding::from_verified_fargate(
            &s.f.bootstrap,
            &request,
            Adapter::OpenAiApi,
            original
        )
        .await
        .err(),
        Some(agent::AgentError::Unavailable)
    );
    let broker = b::SecretBroker::open(&s.f.bootstrap).await.unwrap();
    assert!(
        broker
            .model_transport(&request, Adapter::OpenAiApi, original)
            .await
            .is_err()
    );
    assert_eq!(s.f.count("secret_events").await, before);
    s.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coding_agent_successful_model_with_wrong_code_fails_independent_verification() {
    use ring::signature::KeyPair;
    let mut s = Session::open(Some(0)).await;
    let before = s.f.store.financial_snapshot(&s.f.project).await.unwrap();
    let result = agent::fixture::execute_value(
        &mut s.worker,
        Adapter::OpenAiApi,
        s.f.clock.now().unwrap(),
        99,
    )
    .await;
    assert_eq!(s.f.count("coding_artifacts").await, 1);
    assert_eq!(s.f.count("task_acceptances").await, 0);
    let key =
        ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
    let pair = ring::signature::Ed25519KeyPair::from_pkcs8(key.as_ref()).unwrap();
    let policy = &s.worker.work().context.verification;
    let image = commitment(
        &policy
            .commands
            .iter()
            .map(|c| {
                (
                    c[0].clone(),
                    commitment_bytes(&std::fs::read(&c[0]).unwrap()),
                )
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let grant =
        s.f.co
            .fixture_verifier_execution(
                &s.lease,
                "private-offline-verifier-bad-provider-output".into(),
                image,
                pair.public_key().as_ref().to_vec(),
                &s.f.policy,
                s.f.clock.now().unwrap(),
            )
            .await
            .unwrap();
    assert_eq!(grant.execution, result.verification_execution);
    let mut verifier=super::execution::Process::new("verifier",json!({"ca":s.ca,"url":s.url,"now":s.f.clock.now().unwrap(),"execution":grant.execution,"pkcs":key.as_ref(),"expect_failure":true})).await;
    let pending = verifier.recv().await;
    s.f.clock
        .advance_to(
            pending["completed_at"]
                .as_i64()
                .unwrap()
                .max(s.f.clock.now().unwrap()),
        )
        .unwrap();
    verifier.send(&json!({"submit":true})).await;
    assert_eq!(verifier.recv().await["failed"], true);
    verifier.finish().await;
    assert!(
        s.f.co
            .accept_coding_result(&s.lease, &s.f.policy, s.f.clock.as_ref())
            .await
            .is_err()
    );
    assert!(s.f.remote.prepare_coding(&s.f.co, &s.lease).await.is_err());
    assert_eq!(s.f.count("task_acceptances").await, 0);
    assert_eq!(s.f.count("coding_verifications").await, 0);
    assert_eq!(
        s.f.store.financial_snapshot(&s.f.project).await.unwrap(),
        before
    );
    s.close().await;
}
