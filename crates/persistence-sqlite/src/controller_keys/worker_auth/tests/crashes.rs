use super::*;
use std::io::{BufRead, Read, Write};
use zeroize::Zeroize;

fn point(operation: &str, after: bool) -> CommitCheckpoint {
    match (operation, after) {
        ("prepare", false) => CommitCheckpoint::BeforePrepare,
        ("prepare", true) => CommitCheckpoint::AfterPrepare,
        ("bootstrap", false) => CommitCheckpoint::BeforeBootstrap,
        ("bootstrap", true) => CommitCheckpoint::AfterBootstrap,
        ("renew", false) => CommitCheckpoint::BeforeRenew,
        ("renew", true) => CommitCheckpoint::AfterRenew,
        ("message", false) => CommitCheckpoint::BeforeMessage,
        ("message", true) => CommitCheckpoint::AfterMessage,
        ("revoke", false) => CommitCheckpoint::BeforeRevoke,
        ("revoke", true) => CommitCheckpoint::AfterRevoke,
        _ => panic!("unsupported test checkpoint"),
    }
}
#[tokio::test]
async fn subprocess_driver() {
    let Ok(operation) = std::env::var("ROLVYA_S204_TEST_OPERATION") else {
        return;
    };
    let path = PathBuf::from(std::env::var_os("ROLVYA_S204_TEST_DATABASE").unwrap());
    let keys = PathBuf::from(std::env::var_os("ROLVYA_S204_TEST_KEYS").unwrap());
    let root = path.parent().unwrap().parent().unwrap();
    assert_eq!(keys.parent().unwrap(), root);
    assert_eq!(
        fs::read(root.join("s204-disposable-test-only")).unwrap(),
        b"scoped synthetic worker crash fixture"
    );
    let mut input = Zeroizing::new(String::new());
    std::io::stdin()
        .lock()
        .take(16385)
        .read_to_string(&mut input)
        .unwrap();
    assert!(input.len() <= 16384);
    let mut fields: BTreeMap<String, String> = serde_json::from_str(&input).unwrap();
    let binding: WorkerBinding = serde_json::from_str(&fields["binding"]).unwrap();
    assert_eq!(binding.task.plan.project_id.as_str(), "project");
    assert_eq!(binding.worker_id.as_str(), "synthetic-worker");
    assert_eq!(binding.attempt_id.as_str(), "worker-attempt-a");
    let digest = fields["digest"].clone();
    let now: i64 = fields["now"].parse().unwrap();
    let raw = Zeroizing::new(fields.remove("raw").unwrap());
    let metadata: Option<WorkerCredentialMetadata> = if fields["metadata"].is_empty() {
        None
    } else {
        Some(serde_json::from_str(&fields["metadata"]).unwrap())
    };
    let challenge = if fields["challenge"].is_empty() {
        None
    } else {
        Some(BootstrapChallengeId::parse(fields["challenge"].clone()).unwrap())
    };
    for value in fields.values_mut() {
        value.zeroize()
    }
    drop(fields);
    let store = PlanStore::open(&path).await.unwrap();
    let bootstrap =
        ControllerBootstrap::open(store.clone(), &keys, ServiceIdentity::current(), &[])
            .await
            .unwrap();
    let mut security = WorkerSecurity::open(&bootstrap).await.unwrap();
    let lease = LeaseRef {
        task: binding.task.clone(),
        attempt_id: binding.attempt_id.clone(),
        epoch: binding.epoch,
    };
    assert_eq!(
        DurableWorkerController::new(store.clone())
            .binding(&lease)
            .await
            .unwrap(),
        binding
    );
    let actual: String = sqlx::query_scalar(
        "SELECT digest FROM dispatch_operations WHERE project_id=? AND attempt_id=?",
    )
    .bind(binding.task.plan.project_id.as_str())
    .bind(binding.attempt_id.as_str())
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_eq!(digest, actual);
    let clock = Arc::new(DeterministicClock::new(now).unwrap());
    security.clock = clock;
    security.policy = Arc::new(synthetic::Authority::default());
    let identity = Arc::new(SyntheticIdentity::default());
    let transport = Arc::new(SyntheticTransport::default());
    let revocation = Arc::new(SyntheticRevocation::default());
    let id = format!("crash-{operation}");
    let request_id = RequestId::new(&id).unwrap();
    let evidence = synthetic::e(&format!("proof-{operation}"));
    let stage = match operation.as_str() {
        "prepare" => VerificationStage::Prepare,
        "renew" => VerificationStage::Renew,
        _ => VerificationStage::Bootstrap,
    };
    identity.0.lock().unwrap().insert(
        id.clone(),
        Allowed {
            controller: security.controller(),
            stage,
            request: request_id.clone(),
            evidence: evidence.clone(),
            binding: binding.clone(),
            digest,
            challenge: challenge.clone(),
            credential: if operation == "renew" {
                metadata.clone()
            } else {
                None
            },
            account: synthetic::e("synthetic-account"),
        },
    );
    let op = match operation.as_str() {
        "renew" => WorkerOperation::RenewCredential,
        "message" => WorkerOperation::Heartbeat,
        _ => WorkerOperation::Register,
    };
    let context = WorkerRequestContext {
        request_id: request_id.clone(),
        transport_evidence: transport_evidence(&id),
    };
    transport
        .0
        .lock()
        .unwrap()
        .insert(id, (binding.clone(), op));
    if let Some(m) = &metadata {
        revocation.0.lock().unwrap().insert(
            request_id.as_str().into(),
            (m.enrollment.clone(), 1, evidence.clone(), binding.clone()),
        );
    }
    security.identity = identity;
    security.transport = transport;
    security.revocation = revocation;
    let checkpoint = std::env::var("ROLVYA_S204_TEST_CHECKPOINT").unwrap();
    let expected = point(&operation, checkpoint == "after");
    security.hook = Box::new(move |p| {
        if p == expected {
            println!("S204_READY");
            std::io::stdout().flush().unwrap();
            loop {
                std::thread::park();
            }
        }
        Ok(())
    });
    match operation.as_str() {
        "prepare" => {
            security
                .prepare_bootstrap(&PrepareBootstrap {
                    lease,
                    request_id,
                    evidence,
                })
                .await
                .unwrap();
        }
        "bootstrap" => {
            security
                .claim_bootstrap(BootstrapClaim {
                    id: challenge.as_ref().unwrap(),
                    secret: &BootstrapSecret::parse(raw.to_string()).unwrap(),
                    evidence: &evidence,
                    context: &context,
                })
                .await
                .unwrap();
        }
        "renew" => {
            security
                .renew_credential(
                    &WorkerCredential::parse(raw.to_string()).unwrap(),
                    &evidence,
                    &context,
                )
                .await
                .unwrap();
        }
        "message" => {
            let m = WorkerMessage {
                protocol_version: PROTOCOL_VERSION,
                message_id: MessageId::new("message-4").unwrap(),
                binding,
                sequence: 4,
                body: WorkerBody::Heartbeat {},
            };
            security
                .receive(
                    &WorkerCredential::parse(raw.to_string()).unwrap(),
                    &encode_worker_message(&m).unwrap(),
                    &context,
                )
                .await
                .unwrap();
        }
        "revoke" => {
            security
                .revoke_worker(&WorkerRevocation {
                    enrollment: metadata.unwrap().enrollment,
                    expected_security_epoch: 1,
                    request_id,
                    evidence,
                })
                .await
                .unwrap();
        }
        _ => panic!("unsupported synthetic test operation"),
    }
    panic!("synthetic checkpoint was not reached");
}
struct KillOnDrop(std::process::Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn kill_at(f: &Fixture, operation: &str, after: bool, fields: &mut BTreeMap<String, String>) {
    fs::write(
        f.root.path().join("s204-disposable-test-only"),
        b"scoped synthetic worker crash fixture",
    )
    .unwrap();
    let input = Zeroizing::new(serde_json::to_string(fields).unwrap());
    for value in fields.values_mut() {
        value.zeroize()
    }
    assert!(input.len() < 16384);
    let mut child = KillOnDrop(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "controller_keys::worker_auth::tests::crashes::subprocess_driver",
                "--nocapture",
            ])
            .env("ROLVYA_S204_TEST_OPERATION", operation)
            .env(
                "ROLVYA_S204_TEST_CHECKPOINT",
                if after { "after" } else { "before" },
            )
            .env("ROLVYA_S204_TEST_DATABASE", &f.path)
            .env("ROLVYA_S204_TEST_KEYS", &f.keys)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    child
        .0
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.0.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        for line in std::io::BufReader::new(output).lines() {
            if line.is_ok_and(|s| s == "S204_READY") {
                let _ = tx.send(());
                break;
            }
        }
    });
    if rx.recv_timeout(std::time::Duration::from_secs(20)).is_err() {
        let _ = child.0.kill();
        let _ = child.0.wait();
        let _ = reader.join();
        panic!("synthetic worker child did not reach {operation}/{after}")
    }
    child.0.kill().unwrap();
    let status = child.0.wait().unwrap();
    assert_eq!(
        std::os::unix::process::ExitStatusExt::signal(&status),
        Some(9)
    );
    reader.join().unwrap();
}
#[tokio::test]
async fn ten_sigkill_checkpoints_preserve_bootstrap_rotation_messages_fences_and_holds() {
    for operation in ["prepare", "bootstrap", "renew", "message", "revoke"] {
        for after in [false, true] {
            let f = Fixture::new().await;
            let mut boot = None;
            let mut token = None;
            if operation == "bootstrap" {
                boot = Some(
                    f.security
                        .prepare_bootstrap(&f.request("prepare"))
                        .await
                        .unwrap(),
                );
            } else if matches!(operation, "renew" | "message" | "revoke") {
                let t = f.issued().await;
                f.executing(&t).await;
                token = Some(t);
            }
            if operation == "renew" {
                f.clock.advance_to(synthetic::T + 60).unwrap();
            } else if operation == "message" {
                f.clock.advance_to(synthetic::T + 30).unwrap();
            }
            let raw = boot
                .as_ref()
                .and_then(|b| b.secret.as_ref())
                .map(|b| b.0.0.to_string())
                .or_else(|| {
                    token
                        .as_ref()
                        .filter(|_| operation != "revoke")
                        .map(|t| t.secret.0.to_string())
                })
                .unwrap_or_default();
            let mut fields = BTreeMap::from([
                ("binding".into(), serde_json::to_string(&f.binding).unwrap()),
                ("digest".into(), f.digest.clone()),
                ("now".into(), f.clock.now().unwrap().to_string()),
                ("raw".into(), raw),
                (
                    "metadata".into(),
                    token
                        .as_ref()
                        .map(|t| serde_json::to_string(&t.metadata).unwrap())
                        .unwrap_or_default(),
                ),
                (
                    "challenge".into(),
                    boot.as_ref()
                        .map(|b| b.id.as_str().to_owned())
                        .unwrap_or_default(),
                ),
            ]);
            kill_at(&f, operation, after, &mut fields);
            let reopened = f.reopened().await;
            reopened.store.check_integrity().await.unwrap();
            match operation {
                "prepare" => {
                    assert_eq!(
                        f.rows("worker_bootstrap_challenges").await,
                        i64::from(after)
                    );
                    let mut r = f.request("crash-prepare");
                    r.evidence = synthetic::e("proof-prepare");
                    f.identity
                        .0
                        .lock()
                        .unwrap()
                        .get_mut(r.request_id.as_str())
                        .unwrap()
                        .evidence = r.evidence.clone();
                    let ticket = reopened.prepare_bootstrap(&r).await.unwrap();
                    assert_eq!(ticket.secret.is_none(), after);
                    assert_eq!(f.rows("worker_bootstrap_challenges").await, 1);
                    assert_eq!(f.rows("worker_credentials").await, 0);
                }
                "bootstrap" => {
                    assert_eq!(f.rows("worker_credentials").await, i64::from(after));
                    let boot = boot.as_ref().unwrap();
                    let c = f.context("fresh-bootstrap", WorkerOperation::Register);
                    let e = synthetic::e("new-independent-bootstrap-proof");
                    f.grant(VerificationStage::Bootstrap, &c, &e, Some(&boot.id), None);
                    let result = reopened
                        .claim_bootstrap(BootstrapClaim {
                            id: &boot.id,
                            secret: boot.secret.as_ref().unwrap(),
                            evidence: &e,
                            context: &c,
                        })
                        .await;
                    assert_eq!(result.is_err(), after);
                    assert_eq!(f.rows("worker_credentials").await, 1);
                }
                "renew" => {
                    let t = token.as_ref().unwrap();
                    assert_eq!(f.rows("worker_credentials").await, 1 + i64::from(after));
                    let m = f.message(4, WorkerBody::Heartbeat {});
                    let c = f.context("after-crash-message", WorkerOperation::Heartbeat);
                    let result = reopened
                        .receive(&t.secret, &encode_worker_message(&m).unwrap(), &c)
                        .await;
                    assert_eq!(result.is_err(), after);
                    let (c, e) = f.renewal(t, "fresh-renewal");
                    assert_eq!(
                        reopened.renew_credential(&t.secret, &e, &c).await.is_err(),
                        after
                    );
                }
                "message" => {
                    assert_eq!(f.rows("worker_messages").await, 3 + i64::from(after));
                    assert_eq!(
                        DurableWorkerController::new(f.store.clone())
                            .lease_until(&f.lease)
                            .await
                            .unwrap(),
                        synthetic::T + if after { 120 } else { 90 }
                    );
                    let t = token.as_ref().unwrap();
                    let c = f.context("after-crash-message", WorkerOperation::Heartbeat);
                    let m = f.message(4, WorkerBody::Heartbeat {});
                    let ack = reopened
                        .receive(&t.secret, &encode_worker_message(&m).unwrap(), &c)
                        .await
                        .unwrap();
                    assert_eq!(
                        reopened
                            .receive(&t.secret, &encode_worker_message(&m).unwrap(), &c)
                            .await
                            .unwrap(),
                        ack
                    );
                    assert_eq!(f.rows("worker_messages").await, 4);
                }
                "revoke" => {
                    let t = token.as_ref().unwrap();
                    assert_eq!(f.rows("worker_revocations").await, i64::from(after));
                    assert_eq!(
                        sqlx::query_scalar::<_, String>("SELECT state FROM attempt_states")
                            .fetch_one(&f.store.pool)
                            .await
                            .unwrap(),
                        if after { "fenced" } else { "admitted" }
                    );
                    let c = f.context("after-crash-message", WorkerOperation::Heartbeat);
                    assert_eq!(
                        reopened
                            .receive(
                                &t.secret,
                                &encode_worker_message(&f.message(4, WorkerBody::Heartbeat {}))
                                    .unwrap(),
                                &c
                            )
                            .await
                            .is_err(),
                        after
                    );
                    let mut r = f.revoke_request(t, "crash-revoke");
                    r.evidence = synthetic::e("proof-revoke");
                    f.revocation
                        .0
                        .lock()
                        .unwrap()
                        .get_mut(r.request_id.as_str())
                        .unwrap()
                        .2 = r.evidence.clone();
                    let receipt = reopened.revoke_worker(&r).await.unwrap();
                    assert_eq!(reopened.revoke_worker(&r).await.unwrap(), receipt);
                }
                _ => unreachable!(),
            }
            let budget = f
                .store
                .financial_snapshot(&f.binding.task.plan.project_id)
                .await
                .unwrap();
            assert_eq!(budget.held, synthetic::costs());
            assert_eq!(budget.ceiling.minor_units(), 100);
            assert_eq!(f.rows("task_attempts").await, 1);
            assert_eq!(f.rows("dispatch_outbox").await, 1);
            assert_eq!(f.rows("synthetic_operations").await, 1);
            assert_eq!(f.rows("task_acceptances").await, 0);
        }
    }
}
