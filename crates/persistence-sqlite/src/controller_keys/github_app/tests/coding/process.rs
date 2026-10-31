use super::*;
use crate::controller_keys::{ControllerBootstrap, ServiceIdentity, provider_accounts};
use std::os::unix::process::ExitStatusExt;
const DRIVER: &str = "controller_keys::github_app::tests::coding::process::coding_crash_child";
#[tokio::test]
async fn coding_crash_child() {
    let Ok(point) = std::env::var("ROLVYA_CODING_CRASH_POINT") else {
        return;
    };
    let mut f = Fixture::new_coding().await;
    let leases = f.coding_admit().await;
    for l in &leases {
        let r = f.coding_result(l).await;
        f.coding_receive(l, &r).await;
        f.coding_verify(l, &r).await;
    }
    // Preserve a fully accepted sibling before interrupting A.
    f.coding_accept(&leases[1]).await;
    std::fs::write(std::env::var("ROLVYA_CODING_CRASH_REPORT").unwrap(),serde_json::to_vec(&json!({"root":f.root.path(),"public":hex(&f.world.public),"project":f.project,"lease":leases[0],"held":f.store.financial_snapshot(&f.project).await.unwrap().held.total().unwrap().minor_units(),"uncertain":f.store.financial_snapshot(&f.project).await.unwrap().uncertain_liabilities})).unwrap()).unwrap();
    let kill = |p: &str, point: &str| {
        if p == point {
            let status = std::process::Command::new("kill")
                .args(["-KILL", &std::process::id().to_string()])
                .status()
                .unwrap();
            assert!(status.success());
            unreachable!("SIGKILL cannot acknowledge");
        }
    };
    let a = point.clone();
    f.co.fixture_coding_hook(Box::new(move |p| kill(p, &a)));
    f.remote.hook = Box::new(move |p| kill(p, &point));
    f.coding_accept(&leases[0]).await;
    let p = f.remote.prepare_coding(&f.co, &leases[0]).await.unwrap();
    let _ = f.remote.publish(&f.co, &f.project, &p.operation).await;
    panic!("private coding crash point not reached");
}
#[tokio::test]
async fn coding_sigkill_acceptance_intent_effect_and_receipt_reopen_without_duplicates() {
    for (point, accepted, intents, effects) in [
        ("accept-before", 1, 0, 0),
        ("accept-uncommitted", 1, 0, 0),
        ("accept-after", 2, 0, 0),
        ("intent-before", 2, 0, 0),
        ("intent-after", 2, 1, 0),
        ("effect-before", 2, 1, 0),
        ("effect-after", 2, 1, 1),
        ("receipt-before", 2, 1, 1),
        ("receipt-after", 2, 1, 1),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let report = dir.path().join("checkpoint.json");
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(45),
            tokio::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", DRIVER, "--nocapture"])
                .env("ROLVYA_CODING_CRASH_POINT", point)
                .env("ROLVYA_CODING_CRASH_REPORT", &report)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
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
            serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
        let root = std::path::PathBuf::from(info["root"].as_str().unwrap());
        let store = PlanStore::open(&root.join("data/state.sqlite"))
            .await
            .unwrap();
        store.check_integrity().await.unwrap();
        let count = |table: &str| format!("SELECT count(*) FROM {table}");
        for (table, n) in [
            ("task_attempts", 2),
            ("coding_artifacts", 2),
            ("coding_verifications", 2),
            ("task_acceptances", accepted),
            ("coding_decisions", accepted),
            ("git_publications", intents),
        ] {
            assert_eq!(
                sqlx::query_scalar::<_, i64>(&count(table))
                    .fetch_one(&store.pool)
                    .await
                    .unwrap(),
                n,
                "{point}: {table}"
            );
        }
        let boot = ControllerBootstrap::open(
            store.clone(),
            &root.join("keys"),
            ServiceIdentity::current(),
            &[],
        )
        .await
        .unwrap();
        let registry = provider_accounts::fixture_reopen(&boot).await;
        let policy = registry.policy(synthetic::Authority::default());
        let project = ProjectId::new(info["project"].as_str().unwrap()).unwrap();
        let co = boot
            .coding_coordinator(&project, &policy, Limits::default())
            .await
            .unwrap();
        let world = fixture::World::reopen(
            &root.join("remote-git.sqlite"),
            http::unhex(info["public"].as_str().unwrap()).unwrap(),
        )
        .await;
        assert_eq!(world.ref_count().await, effects, "{point}");
        let (origin, server) = world.server().await;
        let mut remote = GitHubRemote::open(&boot).await.unwrap();
        remote.clock = Arc::new(DeterministicClock::new(NOW).unwrap());
        remote.http = http::Api::fixture(origin);
        let a: LeaseRef = serde_json::from_value(info["lease"].clone()).unwrap();
        let sibling = TaskRef {
            plan: a.task.plan.clone(),
            task_id: TaskId::new("task-b").unwrap(),
        };
        co.validator().verify_accepted(&sibling).await.unwrap();
        if accepted == 1 {
            assert!(co.validator().verify_accepted(&a.task).await.is_err());
            assert_eq!(
                remote.prepare_coding(&co, &a).await.unwrap_err(),
                GitError::Unaccepted
            );
        } else {
            co.validator().verify_accepted(&a.task).await.unwrap();
            if intents == 1 {
                let op: String = sqlx::query_scalar("SELECT operation_id FROM git_publications")
                    .fetch_one(&store.pool)
                    .await
                    .unwrap();
                let before = world.attempts.load(Ordering::SeqCst);
                let r = remote.reconcile(&co, &project, &op).await;
                if effects == 1 {
                    assert_eq!(r.unwrap().state, PublicationState::Published);
                } else {
                    assert_eq!(r.unwrap().state, PublicationState::Unknown);
                }
                assert_eq!(world.attempts.load(Ordering::SeqCst), before);
            }
        }
        assert_eq!(world.ref_count().await, effects);
        let budget = store.financial_snapshot(&project).await.unwrap();
        assert_eq!(
            budget.held.total().unwrap().minor_units(),
            info["held"].as_i64().unwrap()
        );
        assert_eq!(
            budget.uncertain_liabilities,
            info["uncertain"].as_u64().unwrap()
        );
        server.abort();
        world.db.close().await;
        store.close().await;
        std::fs::remove_dir_all(root).unwrap();
    }
}
