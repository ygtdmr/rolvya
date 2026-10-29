use super::*;
use std::io::{BufRead, Write};
fn pause() {
    println!("S205_READY");
    std::io::stdout().flush().unwrap();
    loop {
        std::thread::park();
    }
}
struct BeforeCommit(std::sync::atomic::AtomicUsize);
impl Clock for BeforeCommit {
    fn now(&self) -> Result<i64, AdmissionError> {
        if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1 {
            pause();
        }
        Ok(synthetic::T)
    }
}
#[tokio::test]
async fn subprocess_driver() {
    let Ok(path) = std::env::var("ROLVYA_S205_DISPOSABLE") else {
        return;
    };
    let root = std::path::PathBuf::from(path);
    assert_eq!(
        fs::read(root.join("s205-marker")).unwrap(),
        b"disposable provider authority fixture"
    );
    let store = PlanStore::open(root.join("data/state.sqlite"))
        .await
        .unwrap();
    let b = ControllerBootstrap::open(
        store.clone(),
        &root.join("keys"),
        ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let registry = fixture_reopen(&b).await;
    let policy = registry.policy(synthetic::Authority::default());
    let mut raw = String::new();
    std::io::stdin().read_line(&mut raw).unwrap();
    assert!(raw.len() < 16384);
    let request: AdmissionRequest = serde_json::from_str(&raw).unwrap();
    if std::env::var("ROLVYA_S205_AFTER_COMMIT").unwrap() == "yes" {
        store
            .admit(
                &request,
                &policy,
                &crate::worker_simulation::DeterministicClock::new(synthetic::T).unwrap(),
                Limits::default(),
            )
            .await
            .unwrap();
        pause();
    } else {
        store
            .admit(
                &request,
                &policy,
                &BeforeCommit(std::sync::atomic::AtomicUsize::new(0)),
                Limits::default(),
            )
            .await
            .unwrap();
        panic!("missed before-commit clock boundary");
    }
}
struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[tokio::test]
async fn sigkill_before_and_after_admission_commit_preserves_joint_financial_capacity_authority() {
    for after in [false, true] {
        let f = Fixture::new().await;
        let registry = f.seeded().await;
        fs::write(
            f._root.path().join("s205-marker"),
            b"disposable provider authority fixture",
        )
        .unwrap();
        let mut child = Child(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "controller_keys::provider_accounts::tests::crashes::subprocess_driver",
                    "--nocapture",
                ])
                .env("ROLVYA_S205_DISPOSABLE", f._root.path())
                .env("ROLVYA_S205_AFTER_COMMIT", if after { "yes" } else { "no" })
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap(),
        );
        writeln!(
            child.0.stdin.take().unwrap(),
            "{}",
            serde_json::to_string(&f.request).unwrap()
        )
        .unwrap();
        let output = child.0.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let reader = std::thread::spawn(move || {
            for line in std::io::BufReader::new(output).lines() {
                if line.is_ok_and(|s| s == "S205_READY") {
                    let _ = tx.send(());
                    break;
                }
            }
        });
        assert!(
            rx.recv_timeout(std::time::Duration::from_secs(20)).is_ok(),
            "child must reach the actual admission commit boundary"
        );
        child.0.kill().unwrap();
        let status = child.0.wait().unwrap();
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&status),
            Some(9)
        );
        reader.join().unwrap();
        let reopened = PlanStore::open(f._root.path().join("data/state.sqlite"))
            .await
            .unwrap();
        reopened.check_integrity().await.unwrap();
        for table in [
            "task_attempts",
            "dispatch_outbox",
            "provider_capacity_reservations",
        ] {
            assert_eq!(f.count(table).await, i64::from(after));
        }
        let held = reopened
            .financial_snapshot(&f.request.task.plan.project_id)
            .await
            .unwrap()
            .held
            .total()
            .unwrap()
            .minor_units();
        assert_eq!(held, if after { 7 } else { 0 });
        let policy = registry.policy(synthetic::Authority::default());
        reopened
            .admit(&f.request, &policy, &f.clock(), Limits::default())
            .await
            .unwrap();
        assert_eq!(f.count("task_attempts").await, 1);
        assert_eq!(f.count("provider_capacity_reservations").await, 1);
        assert_eq!(f.count("synthetic_operations").await, 0);
    }
}
