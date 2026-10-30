//! Actual SIGKILL at the existing issuer's transaction checkpoints. Private AWS
//! observations are synthetic; SQLite, keys, accepted TLS and one-use writes are real.
use super::*;
use std::{
    io::{Read, Write},
    os::unix::process::ExitStatusExt,
    process::Stdio,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
};
const DRIVER: &str =
    "controller_keys::worker_auth::https::tests::secure_aws::process::secure_aws_process_fixture";
#[derive(Serialize, Deserialize)]
struct Config {
    database: std::path::PathBuf,
    keys: std::path::PathBuf,
    plan: ProtectedDelivery,
    prepare: bool,
    after: bool,
}
#[tokio::test]
async fn secure_aws_process_fixture() {
    if std::env::var("ROLVYA_SECURE_AWS_TEST_PROCESS").as_deref() != Ok("synthetic-disposable") {
        return;
    }
    let mut input = String::new();
    std::io::stdin()
        .take(65537)
        .read_to_string(&mut input)
        .unwrap();
    assert!(input.len() <= 65536);
    let c: Config = serde_json::from_str(&input).unwrap();
    let store = PlanStore::open(&c.database).await.unwrap();
    let bootstrap =
        ControllerBootstrap::open(store.clone(), &c.keys, ServiceIdentity::current(), &[])
            .await
            .unwrap();
    let registry = provider_accounts::fixture_registry_for(
        &bootstrap,
        synthetic::T,
        AWS_PROVIDER,
        "aws-account-a",
    )
    .await;
    let policy = Arc::new(registry.policy(super::super::aws::AwsAuthority::default()));
    let clock = Arc::new(DeterministicClock::new(synthetic::T).unwrap());
    let http = native::fixture::AwsFixture::new(&c.plan.execution);
    let (identity, _) = native::fixture::open(
        &bootstrap,
        policy.clone(),
        clock.clone(),
        &c.plan.execution,
        http.http(),
    )
    .await;
    let world = secure_fixture::World::new(&c.plan);
    let service = secure_fixture::service(identity, world);
    let cert = Certificate::new("localhost", false);
    let mut security = WorkerSecurity::open(&bootstrap).await.unwrap();
    security.policy = policy;
    security.clock = clock;
    let transport = context::HttpTransport::new(ResourceLimits::default(), security.clock.clone());
    security.transport = transport.clone();
    security.identity = Arc::new(native::PossessionIdentity {
        service: service.clone(),
        transport: transport.clone(),
    });
    let target = match (c.prepare, c.after) {
        (true, false) => CommitCheckpoint::BeforePrepare,
        (true, true) => CommitCheckpoint::AfterPrepare,
        (false, false) => CommitCheckpoint::BeforeBootstrap,
        (false, true) => CommitCheckpoint::AfterBootstrap,
    };
    security.hook = Box::new(move |p| {
        if p == target {
            rustix::process::kill_process(rustix::process::getpid(), rustix::process::Signal::KILL)
                .unwrap();
            unreachable!();
        }
        Ok(())
    });
    let server = Arc::new(HttpsWorkerServer {
        security,
        transport,
        identity: cert.server(),
        secure_fargate: Some(service),
    });
    if c.prepare {
        let r = PrepareBootstrap {
            lease: c.plan.execution.lease(),
            request_id: RequestId::new("secure-prepare").unwrap(),
            evidence: synthetic::e("secure-prepare-evidence"),
        };
        let _ = server.prepare_fargate_bootstrap(&r, &c.plan).await;
        panic!("prepare checkpoint not reached");
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "https://localhost:{}/",
        listener.local_addr().unwrap().port()
    );
    println!(
        "SECURE_FIXTURE {}",
        serde_json::json!({"url":url,"ca":cert.cert.as_ref()})
    );
    std::io::stdout().flush().unwrap();
    server
        .serve(listener, std::future::pending::<()>())
        .await
        .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sigkill_native_prepare_and_consumption_reopen_atomically_with_holds() {
    for prepare in [true, false] {
        for after in [false, true] {
            let n = NativeFixture::new().await;
            let material = if prepare {
                let r = PrepareBootstrap {
                    lease: n.expected.lease(),
                    request_id: RequestId::new("secure-prepare").unwrap(),
                    evidence: synthetic::e("secure-prepare-evidence"),
                };
                n.server
                    .secure_fargate
                    .as_ref()
                    .unwrap()
                    .register_plan(&r, &n.plan)
                    .await
                    .unwrap();
                None
            } else {
                let c = n.prepare().await;
                Some(n.material(&c).await)
            };
            let before =
                n.f.store
                    .financial_snapshot(&n.expected.binding.task.plan.project_id)
                    .await
                    .unwrap();
            let config = Config {
                database: n.f.root.path().join("data/state.sqlite"),
                keys: n.f.root.path().join("keys"),
                plan: n.plan.clone(),
                prepare,
                after,
            };
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", DRIVER, "--nocapture"])
                .env("ROLVYA_SECURE_AWS_TEST_PROCESS", "synthetic-disposable")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let mut input = child.stdin.take().unwrap();
            input
                .write_all(&serde_json::to_vec(&config).unwrap())
                .await
                .unwrap();
            drop(input);
            if let Some(material) = material {
                let mut stdout = BufReader::new(child.stdout.take().unwrap());
                let public = tokio::time::timeout(std::time::Duration::from_secs(20), async {
                    loop {
                        let mut line = String::new();
                        assert!(
                            stdout.read_line(&mut line).await.unwrap() > 0,
                            "native controller fixture closed before listening"
                        );
                        assert!(line.len() < 16384);
                        if let Some(json) = line.strip_prefix("SECURE_FIXTURE ") {
                            break serde_json::from_str::<serde_json::Value>(json).unwrap();
                        }
                    }
                })
                .await
                .unwrap();
                let ca: Vec<u8> = serde_json::from_value(public["ca"].clone()).unwrap();
                let cert = Certificate {
                    cert: CertificateDer::from(ca),
                    key: Vec::new(),
                };
                let mut client = cert.client(public["url"].as_str().unwrap());
                assert!(
                    material
                        .authenticate(&mut client, RequestId::new("killed-native-claim").unwrap())
                        .await
                        .is_err()
                );
            }
            let output =
                tokio::time::timeout(std::time::Duration::from_secs(20), child.wait_with_output())
                    .await
                    .unwrap()
                    .unwrap();
            assert_eq!(
                output.status.signal(),
                Some(9),
                "actual SIGKILL checkpoint required, prepare={prepare}, after={after}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let reopened = PlanStore::open(&config.database).await.unwrap();
            reopened.check_integrity().await.unwrap();
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM worker_credentials")
                .fetch_one(&reopened.pool)
                .await
                .unwrap();
            assert_eq!(count, i64::from(!prepare && after));
            if prepare {
                assert_eq!(
                    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM aws_protected_challenges")
                        .fetch_one(&reopened.pool)
                        .await
                        .unwrap(),
                    i64::from(after)
                );
            } else {
                assert_eq!(
                    sqlx::query_scalar::<_, String>("SELECT state FROM aws_protected_challenges")
                        .fetch_one(&reopened.pool)
                        .await
                        .unwrap(),
                    if after { "consumed" } else { "delivered" }
                );
            }
            assert_eq!(
                reopened
                    .financial_snapshot(&n.expected.binding.task.plan.project_id)
                    .await
                    .unwrap(),
                before
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_attempts")
                    .fetch_one(&reopened.pool)
                    .await
                    .unwrap(),
                2
            );
            ControllerBootstrap::open(
                reopened.clone(),
                &config.keys,
                ServiceIdentity::current(),
                &[],
            )
            .await
            .unwrap()
            .load_existing()
            .await
            .unwrap();
            reopened.close().await;
        }
    }
}
