//! Private harmless offline processes; not remote sandbox/attestation evidence.
use super::*;
use crate::controller_keys::worker_auth::{
    https::{tests::CodingNetwork, verifier::*},
    *,
};
use crate::orchestration::coding::execution::*;
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::{
    io::{BufRead, Write},
    process::Stdio,
    time::Duration,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use zeroize::Zeroizing;
const DRIVER: &str =
    "controller_keys::github_app::tests::coding::execution::coding_execution_process";
fn image(policy: &VerificationPolicy) -> Digest {
    commitment(
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
    .unwrap()
}
fn pkcs() -> Vec<u8> {
    Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
        .unwrap()
        .as_ref()
        .to_vec()
}
fn emit(v: &impl Serialize) {
    println!("G103 {}", serde_json::to_string(v).unwrap());
    std::io::stdout().flush().unwrap();
}
fn line(r: &mut impl BufRead) -> Option<Zeroizing<String>> {
    let mut s = Zeroizing::new(String::new());
    let n = std::io::Read::take(&mut *r, 256 * 1024)
        .read_line(&mut s)
        .unwrap();
    assert!(s.len() < 256 * 1024);
    if n == 0 { None } else { Some(s) }
}
struct Process {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}
impl Process {
    async fn new(role: &str, config: serde_json::Value) -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", DRIVER, "--nocapture"])
            .env("ROLVYA_G103_PROCESS", role)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
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
    async fn send(&mut self, v: &impl Serialize) {
        let mut b = Zeroizing::new(serde_json::to_vec(v).unwrap());
        b.push(b'\n');
        self.input.write_all(&b).await.unwrap();
        self.input.flush().await.unwrap();
    }
    async fn recv(&mut self) -> serde_json::Value {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let mut s = Zeroizing::new(String::new());
                assert!(
                    self.output.read_line(&mut s).await.unwrap() > 0,
                    "child unexpectedly closed"
                );
                assert!(s.len() < 256 * 1024);
                if let Some(s) = s.strip_prefix("G103 ") {
                    return serde_json::from_str(s).unwrap();
                }
            }
        })
        .await
        .unwrap()
    }
    async fn finish(mut self) {
        drop(self.input);
        let status = tokio::time::timeout(Duration::from_secs(20), self.child.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(status.success());
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coding_execution_process() {
    let Ok(role) = std::env::var("ROLVYA_G103_PROCESS") else {
        return;
    };
    let mut input = std::io::BufReader::new(std::io::stdin());
    let config: serde_json::Value = serde_json::from_str(&line(&mut input).unwrap()).unwrap();
    let ca: Vec<u8> = serde_json::from_value(config["ca"].clone()).unwrap();
    let url = config["url"].as_str().unwrap();
    let now = config["now"].as_i64().unwrap();
    if role == "coding" {
        let mut client = CodingNetwork::worker(url, ca);
        let binding: WorkerBinding =
            serde_json::from_value(config["bootstrap"]["binding"].clone()).unwrap();
        client
            .bootstrap(
                serde_json::from_value(config["bootstrap"]["request"].clone()).unwrap(),
                serde_json::from_value(config["bootstrap"]["challenge"].clone()).unwrap(),
                &BootstrapSecret::parse(config["bootstrap"]["secret"].as_str().unwrap().into())
                    .unwrap(),
                serde_json::from_value(config["bootstrap"]["proof"].clone()).unwrap(),
                &binding,
            )
            .await
            .unwrap();
        let mut worker =
            crate::controller_keys::worker_auth::https::coding_worker::CodingWorker::begin(
                client,
                synthetic::caps(),
                now,
            )
            .await
            .unwrap();
        let work = worker.work().clone();
        let source = worker.source().clone();
        let changes = if binding.task.task_id.as_str() == "task-a" {
            vec![
                FileChange::Modify {
                    path: "src/alpha.rs".into(),
                    original_sha256: commitment_bytes(
                        &source
                            .files
                            .iter()
                            .find(|f| f.path == "src/alpha.rs")
                            .unwrap()
                            .bytes,
                    ),
                    content: "pub fn alpha()->u8{11}\r\n".into(),
                },
                FileChange::Add {
                    path: "src/added.rs".into(),
                    content: "pub const ADDED:u8=4;\n".into(),
                },
                FileChange::Delete {
                    path: "src/obsolete.rs".into(),
                    original_sha256: commitment_bytes(
                        &source
                            .files
                            .iter()
                            .find(|f| f.path == "src/obsolete.rs")
                            .unwrap()
                            .bytes,
                    ),
                },
            ]
        } else {
            vec![FileChange::Modify {
                path: "src/beta.rs".into(),
                original_sha256: commitment_bytes(
                    &source
                        .files
                        .iter()
                        .find(|f| f.path == "src/beta.rs")
                        .unwrap()
                        .bytes,
                ),
                content: "pub fn beta()->u8{22}\n".into(),
            }]
        };
        let result = worker
            .submit(
                changes,
                format!("independent-{}", binding.attempt_id.as_str()),
            )
            .await
            .unwrap();
        let raw = serde_json::to_vec(&result).unwrap();
        emit(
            &serde_json::json!({"pid":std::process::id(),"artifact":commitment_bytes(&raw),"work":work.identity,"source":tree_commitment(&source.files).unwrap(),"result":result}),
        );
    } else if role == "verifier" {
        let exec = config["execution"].as_str().unwrap().to_owned();
        let client = if config.get("protected").is_some() {
            let plan: crate::controller_keys::worker_auth::aws_fargate::ProtectedDelivery =
                serde_json::from_value(config["delivery"].clone()).unwrap();
            let credential=crate::controller_keys::worker_auth::aws_fargate::execution::verifier::VerifierCredential::fixture_parse(&plan,config["protected"].as_str().unwrap().as_bytes(),now).unwrap();
            credential
                .fixture_tls(
                    crate::controller_keys::worker_auth::https::client::ControllerEndpoint::parse(
                        url,
                    )
                    .unwrap(),
                    crate::controller_keys::worker_auth::https::tls::fixture_client(
                        rustls::pki_types::CertificateDer::from(ca),
                    ),
                )
                .unwrap()
        } else {
            CodingNetwork::verifier(
                url,
                ca,
                exec,
                serde_json::from_value(config["pkcs"].clone()).unwrap(),
            )
        };
        let inputs = client.inputs(now).await.unwrap();
        let root = tempfile::tempdir().unwrap();
        let host = RemoteExecution::fixture(root.path().join("candidate"), &inputs, now);
        let report = client.execute(&inputs, host).await.unwrap();
        emit(
            &serde_json::json!({"completed_at":report.report.completed_at,"execution":inputs.grant.execution}),
        );
        let confirm: serde_json::Value = serde_json::from_str(&line(&mut input).unwrap()).unwrap();
        assert_eq!(confirm["submit"], true);
        let answer = client.submit(report.clone()).await.unwrap();
        let replay = client.submit(report).await.unwrap();
        assert_eq!(answer, replay);
        emit(&serde_json::json!({"pid":std::process::id(),"evidence":answer}));
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coding_work_tls_two_worker_two_verifier_processes_r09_git() {
    coding_work_end_to_end(false).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fargate_verifier_tls_two_worker_two_execution_r09_git() {
    coding_work_end_to_end(true).await;
}
async fn coding_work_end_to_end(fargate: bool) {
    let f = if fargate {
        Fixture::new_coding_fargate().await
    } else {
        Fixture::new_coding_execution().await
    };
    let leases = f.coding_admit().await;
    let before = f.store.financial_snapshot(&f.project).await.unwrap();
    let fingerprint = f.co.fingerprint().to_owned();
    let co = f
        .bootstrap
        .coding_coordinator(&f.project, &f.policy, Limits::default())
        .await
        .unwrap();
    let net = CodingNetwork::new(
        &f.bootstrap,
        Arc::new(f.policy.fixture_reuse(synthetic::Authority {
            fargate,
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
            .unwrap();
    });
    let a = net.bootstrap(&leases[0]).await;
    let b = net.bootstrap(&leases[1]).await;
    let mut pa = Process::new(
        "coding",
        json!({"ca":net.ca,"url":url,"now":f.clock.now().unwrap(),"bootstrap":a}),
    )
    .await;
    let mut pb = Process::new(
        "coding",
        json!({"ca":net.ca,"url":url,"now":f.clock.now().unwrap(),"bootstrap":b}),
    )
    .await;
    let (a, b) = tokio::join!(pa.recv(), pb.recv());
    assert_ne!(a["pid"], b["pid"]);
    assert_ne!(a["artifact"], b["artifact"]);
    pa.finish().await;
    pb.finish().await;
    assert_eq!(f.count("task_acceptances").await, 0);
    assert!(f.remote.prepare_coding(&f.co, &leases[0]).await.is_err());
    let mut keys = Vec::new();
    let mut configs = Vec::new();
    let mut aws_executions = Vec::new();
    for (i, l) in leases.iter().enumerate() {
        if fargate {
            let mut execution = super::fargate::prepare(&f, l).await;
            execution
                .adapter
                .launch_verifier(l, f.clock.as_ref())
                .await
                .unwrap();
            execution.distribution(l).await;
            execution.adapter.authorize_verifier(l).await.unwrap();
            assert!(
                execution
                    .credential(l, f.clock.now().unwrap(), true)
                    .await
                    .is_err()
            );
            let (delivery, payload) = execution.task_payload(l, f.clock.now().unwrap()).await;
            let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
            keys.push(
                base64::engine::general_purpose::STANDARD
                    .decode(value["pkcs8"].as_str().unwrap())
                    .unwrap(),
            );
            configs.push(json!({"ca":net.ca,"url":url,"now":f.clock.now().unwrap(),"execution":format!("independent-{}",l.attempt_id.as_str()),"protected":payload.as_str(),"delivery":delivery}));
            aws_executions.push(execution);
        } else {
            let key = pkcs();
            let pair = Ed25519KeyPair::from_pkcs8(&key).unwrap();
            f.co.fixture_verifier_execution(
                l,
                format!("private-offline-verifier-{i}"),
                image(&f.contexts[&l.task.task_id].verification),
                pair.public_key().as_ref().to_vec(),
                &f.policy,
                f.clock.now().unwrap(),
            )
            .await
            .unwrap();
            configs.push(json!({"ca":net.ca,"url":url,"now":f.clock.now().unwrap(),"execution":format!("independent-{}",l.attempt_id.as_str()),"pkcs":key}));
            keys.push(key);
        }
    }
    let mut va = Process::new("verifier", configs.remove(0)).await;
    let mut vb = Process::new("verifier", configs.remove(0)).await;
    let (pending_a, pending_b) = tokio::join!(va.recv(), vb.recv());
    let completed = pending_a["completed_at"]
        .as_i64()
        .unwrap()
        .max(pending_b["completed_at"].as_i64().unwrap());
    f.clock
        .advance_to(completed.max(f.clock.now().unwrap()))
        .unwrap();
    va.send(&json!({"submit":true})).await;
    vb.send(&json!({"submit":true})).await;
    let (ra, rb) = tokio::join!(va.recv(), vb.recv());
    for r in [&ra, &rb] {
        assert_ne!(r["pid"], a["pid"]);
        assert_ne!(r["pid"], b["pid"]);
    }
    assert_ne!(ra["pid"], rb["pid"]);
    va.finish().await;
    vb.finish().await;
    for l in &leases {
        f.coding_accept(l).await;
    }
    let (pa, pb) = tokio::join!(
        f.remote.prepare_coding(&f.co, &leases[0]),
        f.remote.prepare_coding(&f.co, &leases[1])
    );
    let pa = pa.unwrap();
    let pb = pb.unwrap();
    let (ra, rb) = tokio::join!(
        f.remote.publish(&f.co, &f.project, &pa.operation),
        f.remote.publish(&f.co, &f.project, &pb.operation)
    );
    let ra = ra.unwrap();
    let rb = rb.unwrap();
    assert_eq!(ra.state, PublicationState::Published);
    assert_eq!(rb.state, PublicationState::Published);
    assert_ne!(ra.reference, rb.reference);
    for (receipt, result) in [(&ra, &a["result"]), (&rb, &b["result"])] {
        let r: CodingResult = serde_json::from_value(result.clone()).unwrap();
        let source = f
            .remote
            .read_pinned(&f.project, &r.base_commit)
            .await
            .unwrap();
        let expected = apply_changes(
            &source,
            &r.changes,
            &f.contexts[&r.binding.task.task_id].paths,
        )
        .unwrap()
        .into_iter()
        .map(|f| (f.path, f.bytes))
        .collect::<BTreeMap<_, _>>();
        assert_eq!(published_files(&f, receipt).await, expected);
    }
    for execution in &aws_executions {
        assert_eq!(execution.effects().await, 1);
    }
    assert_eq!(f.co.fingerprint(), fingerprint);
    let after = f.store.financial_snapshot(&f.project).await.unwrap();
    assert_eq!(before.held, after.held);
    assert_eq!(before.ceiling, after.ceiling);
    assert_eq!(f.count("task_attempts").await, 2);
    assert_eq!(f.count("coding_verifier_executions").await, 2);
    assert_eq!(f.count("coding_verifier_reports").await, 2);
    assert_eq!(f.count("task_acceptances").await, 2);
    assert_eq!(f.co.snapshot().await.unwrap().waiting_dependencies, 1);
    f.co.validator()
        .aggregate(
            &TaskRef {
                plan: f.co.plan_key().clone(),
                task_id: TaskId::new("group-g").unwrap(),
            },
            &f.policy,
            f.clock.as_ref(),
        )
        .await
        .unwrap();
    let pass =
        f.co.pass(
            &f.policy,
            f.clock.as_ref(),
            PassLimits {
                admissions: 1,
                events: 64,
                validations: 16,
                groups: 0,
            },
        )
        .await
        .unwrap();
    assert_eq!(pass.admitted.len(), 1);
    assert_eq!(pass.admitted[0].task.task_id.as_str(), "task-c");
    let c_start = f.clock.now().unwrap();
    for offset in [1, 2, 30, 42, 50] {
        f.clock.advance_to(c_start + offset).unwrap();
        f.co.pass(
            &f.policy,
            f.clock.as_ref(),
            PassLimits {
                admissions: 0,
                events: 64,
                validations: 16,
                groups: 0,
            },
        )
        .await
        .unwrap();
    }
    let c = f.lease("task-c").await;
    f.co.validator().verify_accepted(&c.task).await.unwrap();
    let group = TaskRef {
        plan: f.co.plan_key().clone(),
        task_id: TaskId::new("group-g").unwrap(),
    };
    let group_hash = f.co.validator().verify_accepted(&group).await.unwrap();
    let raw: Vec<u8> =
        sqlx::query_scalar("SELECT content FROM synthetic_artifacts WHERE attempt_id=?")
            .bind(c.attempt_id.as_str())
            .fetch_one(&f.store.pool)
            .await
            .unwrap();
    let c_result: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(
        c_result["inputs"][0]["manifest_sha256"],
        group_hash.as_str()
    );
    assert_eq!(f.co.fingerprint(), fingerprint);
    assert_eq!(f.count("task_attempts").await, 3);
    assert_eq!(f.count("coding_verifier_executions").await, 2);
    assert_eq!(
        f.store
            .financial_snapshot(&f.project)
            .await
            .unwrap()
            .ceiling,
        before.ceiling
    );
    let snapshot = f.root.path().join("data/coding-execution-snapshot.sqlite");
    f.store.snapshot(&snapshot).await.unwrap();
    let mut protected_values = keys.clone();
    protected_values.extend(keys.iter().map(|k| {
        base64::engine::general_purpose::STANDARD
            .encode(k)
            .into_bytes()
    }));
    protected_values.push(f.remote.key.material.to_vec());
    protected_values.push(fixture::signing_key().0.clone());
    protected_values.push(f.webhook.clone());
    for file in [
        f.root.path().join("data/state.sqlite"),
        f.root.path().join("data/state.sqlite-wal"),
        snapshot,
    ] {
        if file.exists() {
            let bytes = std::fs::read(file).unwrap();
            for key in &protected_values {
                assert!(
                    !bytes.windows(key.len()).any(|w| w == key),
                    "private verifier/controller/Git credentials cannot enter ordinary persistence"
                );
            }
        }
    }
    let protected: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM refs WHERE name IN ('refs/heads/main','refs/heads/master')",
    )
    .fetch_one(&f.world.db)
    .await
    .unwrap();
    assert_eq!(protected, 1);
    let main: String = sqlx::query_scalar("SELECT sha FROM refs WHERE name='refs/heads/main'")
        .fetch_one(&f.world.db)
        .await
        .unwrap();
    assert_eq!(main, f.world.base.as_str());
    stop.send(()).unwrap();
    serving.await.unwrap();
}

pub(crate) fn fixture_command(task: &str) -> Vec<String> {
    static HARNESS: std::sync::OnceLock<(tempfile::TempDir, std::path::PathBuf)> =
        std::sync::OnceLock::new();
    let (_, path) = HARNESS.get_or_init(|| {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("harness.rs");
        let bin = root.path().join("verification-harness");
        let harness=r#"
fn main(){
 let task=std::env::args().nth(1).unwrap();
 if task=="fixture-descendant" {std::fs::write("descendant.pid",std::process::id().to_string()).unwrap();std::thread::sleep(std::time::Duration::from_secs(30));return;}

 if task=="task-a" {
  assert_eq!(std::fs::read("src/alpha.rs").unwrap(),b"pub fn alpha()->u8{11}\r\n");
  assert_eq!(std::fs::read("src/added.rs").unwrap(),b"pub const ADDED:u8=4;\n");
  assert!(!std::path::Path::new("src/obsolete.rs").exists());
 }else{
  assert_eq!(std::fs::read("src/beta.rs").unwrap(),b"pub fn beta()->u8{22}\n");
  assert_eq!(std::fs::read("src/shared.rs").unwrap(),b"pub const SHARED:u8=3;\n");
 }
 let mode=std::env::args().nth(2).unwrap_or_default();
 if mode=="environment" {assert!(std::env::vars().next().is_none());}
 if mode=="extra" {std::fs::write("src/unauthorized.rs",b"unexpected").unwrap();}
 if mode=="timeout-child" { let _child=std::process::Command::new(std::env::current_exe().unwrap()).arg("fixture-descendant").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap(); std::thread::sleep(std::time::Duration::from_secs(30)); }
 if mode=="timeout" {std::thread::sleep(std::time::Duration::from_secs(10));}
 if mode=="output" {print!("{}","x".repeat(65536));}
 if mode=="crash" {std::process::exit(22);}
 let out=std::env::temp_dir().join(format!("rolvya-fixed-rust-tests-{}",std::process::id()));
 std::fs::create_dir(&out).unwrap();
 let binary=out.join("candidate-tests");
 let tests=if task=="task-a"{"tests/alpha_test.rs"}else{"tests/beta_test.rs"};
 assert!(std::process::Command::new(__RUSTC__).env("PATH","/usr/bin:/bin").args(["--edition=2024","--test",tests,"-o"]).arg(&binary).status().unwrap().success());
 assert!(std::process::Command::new(&binary).arg("--test-threads=1").status().unwrap().success());
 std::fs::remove_dir_all(out).unwrap();
 println!("ROLVYA_COMPLETED_TEST {}-test",task);
}
"#;
        let rustc=std::process::Command::new("rustup").args(["which","rustc"]).output().unwrap();assert!(rustc.status.success());
        let rustc=String::from_utf8(rustc.stdout).unwrap();let rustc=rustc.trim();
        std::fs::write(&src,harness.replace("__RUSTC__",&format!("{rustc:?}"))).unwrap();
        assert!(
            std::process::Command::new("rustc")
                .args(["--edition=2024", "-O"])
                .arg(&src)
                .arg("-o")
                .arg(&bin)
                .status()
                .unwrap()
                .success()
        );
        assert!(std::fs::metadata(&bin).unwrap().len() < 128 * 1024 * 1024);
        (root, bin)
    });
    vec![path.to_str().unwrap().into(), task.into()]
}

async fn ready_inputs() -> (Fixture, LeaseRef, VerificationInputs, VerifierHttpClient) {
    ready_inputs_mode("").await
}
async fn ready_inputs_mode(
    mode: &str,
) -> (Fixture, LeaseRef, VerificationInputs, VerifierHttpClient) {
    let f = Fixture::new_coding_mode(mode).await;
    let leases = f.coding_admit().await;
    let l = leases[0].clone();
    let r = f.coding_result(&l).await;
    f.coding_receive(&l, &r).await;
    let key = pkcs();
    let pair = Ed25519KeyPair::from_pkcs8(&key).unwrap();
    let g =
        f.co.fixture_verifier_execution(
            &l,
            "private-offline-verifier-local".into(),
            image(&f.contexts[&l.task.task_id].verification),
            pair.public_key().as_ref().to_vec(),
            &f.policy,
            f.clock.now().unwrap(),
        )
        .await
        .unwrap();
    let mut tx = f.store.pool.begin().await.unwrap();
    let work =
        f.co.coding_work_in(&mut tx, &l, f.clock.now().unwrap())
            .await
            .unwrap();
    tx.commit().await.unwrap();
    let source = f
        .remote
        .read_pinned(&f.project, &r.base_commit)
        .await
        .unwrap();
    let files = apply_changes(&source, &r.changes, &work.context.paths).unwrap();
    let inputs = VerificationInputs {
        grant: g.clone(),
        work,
        source,
        artifact: serde_json::to_vec(&r).unwrap(),
        candidate: r,
        files,
    };
    let client = VerifierHttpClient::new(
        crate::controller_keys::worker_auth::https::client::ControllerEndpoint::parse(
            "https://localhost/",
        )
        .unwrap(),
        g.execution,
        Zeroizing::new(key),
    )
    .unwrap();
    (f, l, inputs, client)
}
#[tokio::test]
async fn coding_runner_rejects_mutated_inputs_before_filesystem_effect() {
    let (f, _l, mut inputs, client) = ready_inputs().await;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("candidate");
    let host = RemoteExecution::fixture(path.clone(), &inputs, f.clock.now().unwrap());
    inputs.files.push(SourceFile {
        path: "src/unauthorized.rs".into(),
        mode: "100644".into(),
        sha: crate::controller_keys::github_app::coding_blob(b"extra"),
        bytes: b"extra".to_vec(),
    });
    assert!(
        client.execute(&inputs, host).await.is_err(),
        "a valid host capability must not authorize caller-mutated input files"
    );
    assert!(
        !path.exists(),
        "substitution must be rejected before materialization"
    );
}
#[tokio::test]
async fn coding_runner_rejects_mutated_frozen_argv_before_execution() {
    let (f, _l, mut inputs, client) = ready_inputs().await;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("candidate");
    let host = RemoteExecution::fixture(path.clone(), &inputs, f.clock.now().unwrap());
    inputs.work.context.verification.commands[0][1] = "task-b".into();
    assert!(
        client.execute(&inputs, host).await.is_err(),
        "policy arguments cannot be substituted after authority creation"
    );
    assert!(!path.exists());
}
async fn record(
    f: &Fixture,
    l: &LeaseRef,
    inputs: &VerificationInputs,
    client: &VerifierHttpClient,
) {
    let root = tempfile::tempdir().unwrap();
    let envelope = client
        .execute(
            inputs,
            RemoteExecution::fixture(
                root.path().join("candidate"),
                inputs,
                f.clock.now().unwrap(),
            ),
        )
        .await
        .unwrap();
    f.clock
        .advance_to(envelope.report.completed_at.max(f.clock.now().unwrap()))
        .unwrap();
    let request = client.fixture_signed_request(
        VerifierOperation::Report {
            envelope: Box::new(envelope),
        },
        [5; 32],
    );
    let mut tx = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    f.co.verifier_request_in(
        &mut tx,
        &request,
        &[5; 32],
        &f.policy,
        f.clock.now().unwrap(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let _ = l;
}
#[tokio::test]
async fn coding_acceptance_rejects_verifier_expiring_in_final_commit_window() {
    for point in ["accept-before", "accept-uncommitted"] {
        let (mut f, l, inputs, client) = ready_inputs().await;
        record(&f, &l, &inputs, &client).await;
        let clock = f.clock.clone();
        let expiry = inputs.grant.expires_at;
        f.co.fixture_coding_hook(Box::new(move |p| {
            if p == point {
                clock.advance_to(expiry).unwrap();
            }
        }));
        assert!(
            f.co.accept_coding_result(&l, &f.policy, f.clock.as_ref())
                .await
                .is_err(),
            "expired verifier must not commit at {point}"
        );
        assert_eq!(f.count("task_acceptances").await, 0);
        assert_eq!(f.count("coding_decisions").await, 0);
    }
}

#[tokio::test]
async fn coding_runner_rejects_unexpected_files_created_by_test_command() {
    let (f, _, inputs, client) = ready_inputs_mode("extra").await;
    let root = tempfile::tempdir().unwrap();
    assert!(
        client
            .execute(
                &inputs,
                RemoteExecution::fixture(
                    root.path().join("candidate"),
                    &inputs,
                    f.clock.now().unwrap()
                )
            )
            .await
            .is_err(),
        "a successful approved test must not hide unexpected tree mutations"
    );
}

async fn envelope(
    f: &Fixture,
    inputs: &VerificationInputs,
    client: &VerifierHttpClient,
) -> AuthenticatedReport {
    let root = tempfile::tempdir().unwrap();
    let envelope = client
        .execute(
            inputs,
            RemoteExecution::fixture(
                root.path().join("candidate"),
                inputs,
                f.clock.now().unwrap(),
            ),
        )
        .await
        .unwrap();
    f.clock
        .advance_to(envelope.report.completed_at.max(f.clock.now().unwrap()))
        .unwrap();
    envelope
}
async fn apply_request(
    f: &Fixture,
    client: &VerifierHttpClient,
    op: VerifierOperation,
) -> Result<VerifierReply, OrchestrationError> {
    let request = client.fixture_signed_request(op, [9; 32]);
    let mut tx = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let reply =
        f.co.verifier_request_in(
            &mut tx,
            &request,
            &[9; 32],
            &f.policy,
            f.clock.now().unwrap(),
        )
        .await?;
    tx.commit().await.unwrap();
    Ok(reply)
}
#[tokio::test]
async fn coding_verifier_wrong_tls_key_execution_and_report_signature_denied() {
    let (f, _, inputs, client) = ready_inputs().await;
    let mut r = client.fixture_signed_request(VerifierOperation::Claim {}, [9; 32]);
    for variant in 0..4 {
        let mut bad = r.clone();
        if variant == 0 {
            bad.signature[0] ^= 1;
        }
        if variant == 1 {
            bad.execution = "unregistered".into();
        }
        if variant == 2 {
            bad.version = 99;
        }
        let mut tx = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        let channel = if variant == 3 { [8; 32] } else { [9; 32] };
        assert!(
            f.co.verifier_request_in(&mut tx, &bad, &channel, &f.policy, f.clock.now().unwrap())
                .await
                .is_err()
        );
    }
    r.signature.clear();
    assert_eq!(f.count("coding_verifier_requests").await, 0);
    let mut e = envelope(&f, &inputs, &client).await;
    e.signature[0] ^= 1;
    assert!(
        apply_request(
            &f,
            &client,
            VerifierOperation::Report {
                envelope: Box::new(e)
            }
        )
        .await
        .is_err()
    );
    assert_eq!(f.count("coding_verifications").await, 0);
    assert_eq!(f.count("task_acceptances").await, 0);
}
#[tokio::test]
async fn coding_verifier_policy_artifact_tests_status_outputs_and_chronology_are_rechecked() {
    let (f, _, inputs, client) = ready_inputs().await;
    let e = envelope(&f, &inputs, &client).await;
    for variant in 0..9 {
        let mut r = e.report.clone();
        match variant {
            0 => r.artifact_sha256 = Digest::new("f".repeat(64)).unwrap(),
            1 => r.policy_sha256 = Digest::new("f".repeat(64)).unwrap(),
            2 => r.tests_completed.clear(),
            3 => r.terminal = VerificationTerminal::Interrupted,
            4 => r.exit_code = 42,
            5 => r.outputs[0].content.push_str("forged"),
            6 => r.started_at -= 1,
            7 => r.completed_at = inputs.grant.expires_at,
            _ => r.binding.worker_id = WorkerId::new("other-worker").unwrap(),
        }
        let signed = client.fixture_sign_report(&inputs.grant, r);
        assert!(
            apply_request(
                &f,
                &client,
                VerifierOperation::Report {
                    envelope: Box::new(signed)
                }
            )
            .await
            .is_err(),
            "variant {variant}"
        );
        assert_eq!(f.count("coding_verifier_requests").await, 0);
        assert_eq!(f.count("coding_verifications").await, 0);
    }
}
#[tokio::test]
async fn coding_report_replay_conflict_ignored_write_and_rollback_preserve_single_effect() {
    let (f, l, inputs, client) = ready_inputs().await;
    let e = envelope(&f, &inputs, &client).await;
    sqlx::query("CREATE TRIGGER reject_report BEFORE INSERT ON coding_verifier_reports BEGIN SELECT RAISE(IGNORE); END;").execute(&f.store.pool).await.unwrap();
    assert!(
        apply_request(
            &f,
            &client,
            VerifierOperation::Report {
                envelope: Box::new(e.clone())
            }
        )
        .await
        .is_err()
    );
    assert_eq!(f.count("coding_verifier_requests").await, 0);
    assert_eq!(f.count("coding_verifications").await, 0);
    sqlx::query("DROP TRIGGER reject_report")
        .execute(&f.store.pool)
        .await
        .unwrap();
    let first = apply_request(
        &f,
        &client,
        VerifierOperation::Report {
            envelope: Box::new(e.clone()),
        },
    )
    .await
    .unwrap();
    let second = apply_request(
        &f,
        &client,
        VerifierOperation::Report {
            envelope: Box::new(e.clone()),
        },
    )
    .await
    .unwrap();
    assert_eq!(commitment(&first).unwrap(), commitment(&second).unwrap());
    let mut r = e.report.clone();
    r.outputs[0].identity = "conflicting-output".into();
    let changed = client.fixture_sign_report(&inputs.grant, r);
    assert!(
        apply_request(
            &f,
            &client,
            VerifierOperation::Report {
                envelope: Box::new(changed)
            }
        )
        .await
        .is_err()
    );
    assert_eq!(f.count("coding_verifications").await, 1);
    assert_eq!(f.count("coding_verifier_reports").await, 1);
    f.coding_accept(&l).await;
    f.coding_accept(&l).await;
    assert!(
        apply_request(
            &f,
            &client,
            VerifierOperation::Report {
                envelope: Box::new(e)
            }
        )
        .await
        .is_err(),
        "retired verifier cannot create fresh authority; committed evidence remains readable"
    );
    assert_eq!(f.count("task_acceptances").await, 1);
}
#[tokio::test]
async fn coding_work_source_truncation_tampering_scope_and_expiry_rejected() {
    let (f, _, inputs, _) = ready_inputs().await;
    let w = &inputs.work;
    let mut chunks = Vec::new();
    for (i, file) in inputs.source.files.iter().enumerate() {
        for (n, b) in file.bytes.chunks(16384).enumerate() {
            chunks.push(crate::orchestration::coding::work::SourceChunk {
                work: w.identity.clone(),
                file: i as u16,
                offset: (n * 16384) as u32,
                bytes: b.to_vec(),
                sha256: commitment_bytes(b),
            });
        }
    }
    assert_eq!(
        tree_commitment(&w.reconstruct(&chunks).unwrap().files).unwrap(),
        w.context.source_sha256
    );
    for variant in 0..5 {
        let mut c = chunks.clone();
        match variant {
            0 => {
                c.pop();
            }
            1 => c[0].bytes[0] ^= 1,
            2 => c[0].offset = 1,
            3 => c[0].work = Digest::new("f".repeat(64)).unwrap(),
            _ => c.push(c[0].clone()),
        }
        assert!(w.reconstruct(&c).is_err());
    }
    let mut bad = w.clone();
    bad.profile.resources.memory_mib += 1;
    assert!(bad.validate(&w.binding, f.clock.now().unwrap()).is_err());
    assert!(w.validate(&w.binding, w.expires_at).is_err());
    bad = w.clone();
    bad.version = 99;
    assert!(bad.validate(&w.binding, f.clock.now().unwrap()).is_err());
}
#[tokio::test]
async fn coding_runner_timeout_output_exhaustion_crash_and_failure_never_accept() {
    for mode in ["timeout", "output", "crash"] {
        let (f, _, inputs, client) = ready_inputs_mode(mode).await;
        let root = tempfile::tempdir().unwrap();
        let started = std::time::Instant::now();
        let r = client
            .execute(
                &inputs,
                RemoteExecution::fixture(
                    root.path().join("candidate"),
                    &inputs,
                    f.clock.now().unwrap(),
                ),
            )
            .await;
        if mode == "crash" {
            let e = r.unwrap();
            assert_eq!(e.report.terminal, VerificationTerminal::Failed);
            assert_eq!(e.report.exit_code, 22);
            assert!(
                apply_request(
                    &f,
                    &client,
                    VerifierOperation::Report {
                        envelope: Box::new(e)
                    }
                )
                .await
                .is_err()
            );
        } else {
            assert!(r.is_err());
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(f.count("coding_verifications").await, 0);
        assert_eq!(f.count("task_acceptances").await, 0);
    }
}
#[tokio::test]
async fn coding_production_verifier_establishment_and_worker_self_report_remain_denied() {
    let (f, l, inputs, client) = ready_inputs().await;
    assert_eq!(
        f.co.prepare_independent_verifier(&l).await.unwrap_err(),
        OrchestrationError::VerificationUnavailable
    );
    assert!(RemoteExecution::from_verified_fargate().is_err());
    let e = envelope(&f, &inputs, &client).await;
    assert_eq!(
        f.co.verify_coding_execution(&l, &e.report, f.clock.as_ref())
            .await
            .unwrap_err(),
        OrchestrationError::VerificationUnavailable
    );
    assert_eq!(f.count("task_acceptances").await, 0);
}

#[tokio::test]
async fn coding_recovery_hold_blocks_restored_verifier_and_source_without_releasing_money() {
    let (f, l, inputs, client) = ready_inputs().await;
    record(&f, &l, &inputs, &client).await;
    let before = f.store.financial_snapshot(&f.project).await.unwrap();
    let snapshot = f.root.path().join("data/restore.sqlite");
    f.store.snapshot(&snapshot).await.unwrap();
    let restored = PlanStore::open(&snapshot).await.unwrap();
    let boot = ControllerBootstrap::open(
        restored.clone(),
        &f.root.path().join("keys"),
        ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let co = boot
        .coding_coordinator(&f.project, &f.policy, Limits::default())
        .await
        .unwrap();
    co.hold_coding_after_restore(f.clock.as_ref())
        .await
        .unwrap();
    co.hold_coding_after_restore(f.clock.as_ref())
        .await
        .unwrap();
    assert!(
        co.accept_coding_result(&l, &f.policy, f.clock.as_ref())
            .await
            .is_err()
    );
    let mut tx = restored.pool.begin().await.unwrap();
    assert!(
        co.coding_work_in(&mut tx, &l, f.clock.now().unwrap())
            .await
            .is_err()
    );
    drop(tx);
    assert_eq!(
        restored.financial_snapshot(&f.project).await.unwrap(),
        before
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM coding_verifier_executions")
            .fetch_one(&restored.pool)
            .await
            .unwrap(),
        1
    );
}

pub(super) async fn network(
    f: &Fixture,
    co: SyntheticCoordinator,
) -> (
    CodingNetwork,
    String,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
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
    let task = tokio::spawn(async move {
        server
            .serve(listener, async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    (net, url, stop, task)
}
async fn boot_worker(
    net: &CodingNetwork,
    url: &str,
    l: &LeaseRef,
) -> crate::controller_keys::worker_auth::https::client::WorkerHttpClient {
    let b = net.bootstrap(l).await;
    let mut client = CodingNetwork::worker(url, net.ca.clone());
    client
        .bootstrap(
            serde_json::from_value(b["request"].clone()).unwrap(),
            serde_json::from_value(b["challenge"].clone()).unwrap(),
            &BootstrapSecret::parse(b["secret"].as_str().unwrap().into()).unwrap(),
            serde_json::from_value(b["proof"].clone()).unwrap(),
            &serde_json::from_value(b["binding"].clone()).unwrap(),
        )
        .await
        .unwrap();
    client
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coding_tls_cross_attempt_source_forgery_revocation_and_independent_role_denials() {
    let f = Fixture::new_coding_execution().await;
    let leases = f.coding_admit().await;
    let co = f
        .bootstrap
        .coding_coordinator(&f.project, &f.policy, Limits::default())
        .await
        .unwrap();
    let (net, url, stop, task) = network(&f, co).await;
    let a = boot_worker(&net, &url, &leases[0]).await;
    let b = boot_worker(&net, &url, &leases[1]).await;
    let wa = a
        .coding_work(crate::RequestId::new("wa").unwrap(), f.clock.now().unwrap())
        .await
        .unwrap();
    let wb = b
        .coding_work(crate::RequestId::new("wb").unwrap(), f.clock.now().unwrap())
        .await
        .unwrap();
    assert!(a.coding_source(&wb).await.is_err());
    assert!(b.coding_source(&wa).await.is_err());
    let source = a.coding_source(&wa).await.unwrap();
    assert_eq!(
        tree_commitment(&source.files).unwrap(),
        wa.context.source_sha256
    );
    let unknown =
        CodingNetwork::verifier(&url, net.ca.clone(), "nonexistent-verifier".into(), pkcs());
    assert!(unknown.inputs(f.clock.now().unwrap()).await.is_err());
    // Exact Owner/security epoch revocation blocks disclosure, leaving both holds.
    let before = f.store.financial_snapshot(&f.project).await.unwrap();
    sqlx::query("UPDATE human_users SET security_epoch=security_epoch+1 WHERE user_id=(SELECT actor FROM git_bindings WHERE project_id=? ORDER BY version DESC LIMIT 1)")
        .bind(f.project.as_str())
        .execute(&f.store.pool)
        .await
        .unwrap();
    assert!(a.coding_source(&wa).await.is_err());
    assert!(b.coding_source(&wb).await.is_err());
    assert_eq!(
        f.store.financial_snapshot(&f.project).await.unwrap(),
        before
    );
    assert_eq!(f.count("task_acceptances").await, 0);
    stop.send(()).unwrap();
    task.await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coding_verifier_actual_tls_exporter_and_copied_worker_scope_cannot_authenticate() {
    let (f, _l, inputs, client) = ready_inputs().await;
    // The original grant has a different private key from this client.
    let co = f
        .bootstrap
        .coding_coordinator(&f.project, &f.policy, Limits::default())
        .await
        .unwrap();
    let (net, url, stop, task) = network(&f, co).await;
    let copied =
        CodingNetwork::verifier(&url, net.ca.clone(), inputs.grant.execution.clone(), pkcs());
    assert!(copied.inputs(f.clock.now().unwrap()).await.is_err());
    assert_eq!(f.count("coding_verifier_requests").await, 0);
    let valid = client.fixture_endpoint(&url, net.ca.clone());
    valid.inputs(f.clock.now().unwrap()).await.unwrap();
    let before = f.count("coding_verifier_requests").await;
    assert!(valid.fixture_wrong_tls().await.is_err());
    assert_eq!(f.count("coding_verifier_requests").await, before);
    stop.send(()).unwrap();
    task.await.unwrap();
}

#[tokio::test]
async fn coding_verification_environment_is_empty_and_working_directory_cannot_be_substituted() {
    let (f, _, mut inputs, client) = ready_inputs_mode("environment").await;
    let root = tempfile::tempdir().unwrap();
    let good = envelope(&f, &inputs, &client).await;
    assert_eq!(
        good.report.exit_code, 0,
        "{}",
        good.report.outputs[0].content
    );
    let path = root.path().join("attempt");
    let host = RemoteExecution::fixture(path.clone(), &inputs, f.clock.now().unwrap());
    inputs.files[0].path = "../outside.rs".into();
    assert!(client.execute(&inputs, host).await.is_err());
    assert!(!path.exists());
    assert!(!root.path().join("outside.rs").exists());
}
#[tokio::test]
async fn coding_concurrent_report_delivery_repeated_and_sqlite_contention_have_one_effect() {
    for _repetition in 0..3 {
        let (f, l, inputs, client) = ready_inputs().await;
        let e = envelope(&f, &inputs, &client).await;
        let lock = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        assert!(
            tokio::time::timeout(
                Duration::from_millis(100),
                apply_request(
                    &f,
                    &client,
                    VerifierOperation::Report {
                        envelope: Box::new(e.clone())
                    }
                )
            )
            .await
            .is_err()
        );
        lock.rollback().await.unwrap();
        assert_eq!(f.count("coding_verifications").await, 0);
        let (a, b) = tokio::join!(
            apply_request(
                &f,
                &client,
                VerifierOperation::Report {
                    envelope: Box::new(e.clone())
                }
            ),
            apply_request(
                &f,
                &client,
                VerifierOperation::Report {
                    envelope: Box::new(e.clone())
                }
            )
        );
        assert!(a.is_ok() && b.is_ok());
        assert_eq!(f.count("coding_verifier_reports").await, 1);
        assert_eq!(f.count("coding_verifier_requests").await, 1);
        let (a, b) = tokio::join!(
            f.co.accept_coding_result(&l, &f.policy, f.clock.as_ref()),
            f.co.accept_coding_result(&l, &f.policy, f.clock.as_ref())
        );
        assert!(a.is_ok() && b.is_ok());
        assert_eq!(f.count("task_acceptances").await, 1);
        assert_eq!(f.count("task_attempts").await, 2);
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coding_execution_crash_process() {
    let Ok(point) = std::env::var("ROLVYA_G103_CRASH") else {
        return;
    };
    let f = Fixture::new_coding_execution().await;
    let leases = f.coding_admit().await;
    let sibling = f.coding_result(&leases[1]).await;
    f.coding_receive(&leases[1], &sibling).await;
    f.coding_verify(&leases[1], &sibling).await;
    f.coding_accept(&leases[1]).await;
    let reporting = point.starts_with("report-");
    let mut verifier_key = None;
    let mut execution = None;
    if reporting {
        let r = f.coding_result(&leases[0]).await;
        f.coding_receive(&leases[0], &r).await;
        let key = pkcs();
        let pair = Ed25519KeyPair::from_pkcs8(&key).unwrap();
        let g =
            f.co.fixture_verifier_execution(
                &leases[0],
                "private-offline-verifier-crash".into(),
                image(&f.contexts[&leases[0].task.task_id].verification),
                pair.public_key().as_ref().to_vec(),
                &f.policy,
                f.clock.now().unwrap(),
            )
            .await
            .unwrap();
        execution = Some(g.execution);
        verifier_key = Some(key);
    }
    let before = f.store.financial_snapshot(&f.project).await.unwrap();
    std::fs::write(std::env::var("ROLVYA_G103_CHECKPOINT").unwrap(),serde_json::to_vec(&json!({"root":f.root.path(),"project":f.project,"fingerprint":f.co.fingerprint(),"held":before.held.total().unwrap().minor_units(),"uncertain":before.uncertain_liabilities,"lease":leases[0],"sibling":leases[1],"now":f.clock.now().unwrap()})).unwrap()).unwrap();
    let mut co = f
        .bootstrap
        .coding_coordinator(&f.project, &f.policy, Limits::default())
        .await
        .unwrap();
    co.fixture_coding_hook(Box::new(move |p| {
        if p == point {
            let _ = std::process::Command::new("kill")
                .args(["-KILL", &std::process::id().to_string()])
                .status();
            unreachable!("SIGKILL has no acknowledgement");
        }
    }));
    let (net, url, _stop, _task) = network(&f, co).await;
    if reporting {
        let client = CodingNetwork::verifier(
            &url,
            net.ca.clone(),
            execution.unwrap(),
            verifier_key.unwrap(),
        );
        let inputs = client.inputs(f.clock.now().unwrap()).await.unwrap();
        let root = tempfile::tempdir().unwrap();
        let e = client
            .execute(
                &inputs,
                RemoteExecution::fixture(
                    root.path().join("candidate"),
                    &inputs,
                    f.clock.now().unwrap(),
                ),
            )
            .await
            .unwrap();
        f.clock
            .advance_to(e.report.completed_at.max(f.clock.now().unwrap()))
            .unwrap();
        let _ = client.submit(e).await;
    } else {
        let client = boot_worker(&net, &url, &leases[0]).await;
        let _ = client
            .coding_work(
                crate::RequestId::new("crash-work").unwrap(),
                f.clock.now().unwrap(),
            )
            .await;
    }
    panic!("intended crash boundary not reached");
}
#[tokio::test]
async fn coding_sigkill_work_delivery_and_authenticated_report_commit_reopen() {
    use std::os::unix::process::ExitStatusExt;
    const CRASH: &str =
        "controller_keys::github_app::tests::coding::execution::coding_execution_crash_process";
    for (point, work, reports) in [
        ("work-before", 0, 0),
        ("work-after", 1, 0),
        ("report-before", 1, 0),
        ("report-after", 1, 1),
    ] {
        let root = tempfile::tempdir().unwrap();
        let checkpoint = root.path().join("checkpoint.json");
        let output = tokio::time::timeout(
            Duration::from_secs(45),
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", CRASH, "--nocapture"])
                .env("ROLVYA_G103_CRASH", point)
                .env("ROLVYA_G103_CHECKPOINT", &checkpoint)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
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
            serde_json::from_slice(&std::fs::read(checkpoint).unwrap()).unwrap();
        let path = std::path::PathBuf::from(info["root"].as_str().unwrap());
        let store = PlanStore::open(&path.join("data/state.sqlite"))
            .await
            .unwrap();
        store.check_integrity().await.unwrap();
        for (table, n) in [
            ("task_attempts", 2),
            ("task_acceptances", 1),
            ("coding_work", work),
            ("coding_verifier_reports", reports),
            ("coding_verifications", 1 + reports),
        ] {
            let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
                .fetch_one(&store.pool)
                .await
                .unwrap();
            assert_eq!(count, n, "{point}: {table}");
        }
        let boot = ControllerBootstrap::open(
            store.clone(),
            &path.join("keys"),
            ServiceIdentity::current(),
            &[],
        )
        .await
        .unwrap();
        boot.load_existing().await.unwrap();
        let registry = crate::controller_keys::provider_accounts::fixture_reopen(&boot).await;
        let policy = registry.policy(synthetic::Authority::default());
        let p: ProjectId = serde_json::from_value(info["project"].clone()).unwrap();
        let co = boot
            .coding_coordinator(&p, &policy, Limits::default())
            .await
            .unwrap();
        assert_eq!(co.fingerprint(), info["fingerprint"].as_str().unwrap());
        let sibling: LeaseRef = serde_json::from_value(info["sibling"].clone()).unwrap();
        co.validator().verify_accepted(&sibling.task).await.unwrap();
        let balance = store.financial_snapshot(&p).await.unwrap();
        assert_eq!(
            balance.held.total().unwrap().minor_units(),
            info["held"].as_i64().unwrap()
        );
        assert_eq!(
            balance.uncertain_liabilities,
            info["uncertain"].as_i64().unwrap() as u64
        );
        let l: LeaseRef = serde_json::from_value(info["lease"].clone()).unwrap();
        assert_eq!(
            co.prepare_independent_verifier(&l).await.unwrap_err(),
            OrchestrationError::VerificationUnavailable
        );
        if reports == 1 {
            // Recover the committed signed report after a lost response. No new
            // verifier authority, report, attempt or financial hold is required.
            let observed: Option<i64> =
                sqlx::query_scalar("SELECT max(recorded_at) FROM coding_verifier_requests")
                    .fetch_one(&store.pool)
                    .await
                    .unwrap();
            let clock =
                DeterministicClock::new(observed.unwrap_or(info["now"].as_i64().unwrap())).unwrap();
            co.accept_coding_result(&l, &policy, &clock).await.unwrap();
            co.accept_coding_result(&l, &policy, &clock).await.unwrap();
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM coding_verifier_executions")
                    .fetch_one(&store.pool)
                    .await
                    .unwrap(),
                1
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_acceptances")
                    .fetch_one(&store.pool)
                    .await
                    .unwrap(),
                2
            );
        }
        store.close().await;
        drop(co);
        drop(boot);
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[tokio::test]
async fn coding_committed_verifier_evidence_cannot_be_accepted_under_rewound_clock() {
    let (f, l, inputs, client) = ready_inputs().await;
    let original = f.clock.now().unwrap();
    f.clock.advance_to(original + 2).unwrap();
    record(&f, &l, &inputs, &client).await;
    let backwards = DeterministicClock::new(original).unwrap();
    assert!(
        f.co.accept_coding_result(&l, &f.policy, &backwards)
            .await
            .is_err(),
        "committed verification cannot precede its own authoritative observation"
    );
    assert_eq!(f.count("task_acceptances").await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coding_authenticated_failed_verifier_report_records_only_redacted_rejection() {
    let (f, _, inputs, client) = ready_inputs_mode("crash").await;
    let co = f
        .bootstrap
        .coding_coordinator(&f.project, &f.policy, Limits::default())
        .await
        .unwrap();
    let (net, url, stop, task) = network(&f, co).await;
    let client = client.fixture_endpoint(&url, net.ca.clone());
    let root = tempfile::tempdir().unwrap();
    let report = client
        .execute(
            &inputs,
            RemoteExecution::fixture(
                root.path().join("candidate"),
                &inputs,
                f.clock.now().unwrap(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(report.report.exit_code, 22);
    assert!(client.submit(report).await.is_err());
    assert_eq!(f.count("coding_verifications").await, 0);
    assert_eq!(f.count("task_acceptances").await, 0);
    let reason: String =
        sqlx::query_scalar("SELECT reason FROM coding_rejections WHERE phase='verification'")
            .fetch_one(&f.store.pool)
            .await
            .expect("authenticated failure needs durable redacted evidence");
    assert_eq!(reason, "invalid");
    stop.send(()).unwrap();
    task.await.unwrap();
}

#[tokio::test]
async fn fargate_verifier_timeout_terminates_ordinary_descendant_group() {
    let (f, _, inputs, client) = ready_inputs_mode("timeout-child").await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("candidate");
    assert!(
        client
            .execute(
                &inputs,
                RemoteExecution::fixture(root.clone(), &inputs, f.clock.now().unwrap())
            )
            .await
            .is_err()
    );
    let pid: u32 = std::fs::read_to_string(root.join("descendant.pid"))
        .unwrap()
        .parse()
        .unwrap();
    let mut active = true;
    for _ in 0..100 {
        active = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .is_ok_and(|s| s.split_once(") ").is_some_and(|(_, v)| !v.starts_with('Z')));
        if !active {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    if active {
        let _ = rustix::process::kill_process(
            rustix::process::Pid::from_raw(pid as i32).unwrap(),
            rustix::process::Signal::KILL,
        );
    }
    assert!(
        !active,
        "a timed-out verifier command left an ordinary descendant executing"
    );
}
