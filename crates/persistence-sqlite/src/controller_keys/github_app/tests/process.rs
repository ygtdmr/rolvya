use super::*;
use crate::controller_keys::provider_accounts;
use crate::{
    controller_keys::ServiceIdentity, orchestration::SyntheticCoordinator, resilience::synthetic,
    worker_simulation::DeterministicClock,
};
use std::os::unix::process::ExitStatusExt;
const DRIVER: &str = "controller_keys::github_app::tests::process::git_crash_child";
#[tokio::test]
async fn git_crash_child() {
    let Ok(point) = std::env::var("ROLVYA_GIT_CRASH_POINT") else {
        return;
    };
    let mut f = Fixture::new().await;
    let report = std::env::var("ROLVYA_GIT_CRASH_REPORT").unwrap();
    std::fs::write(
        report,
        serde_json::to_vec(
            &json!({"root":f.root.path(),"public":hex(&f.world.public),"project":f.project}),
        )
        .unwrap(),
    )
    .unwrap();
    f.remote.hook = Box::new(move |p| {
        if p == point {
            let status = std::process::Command::new("kill")
                .args(["-KILL", &std::process::id().to_string()])
                .status()
                .unwrap();
            assert!(status.success());
            unreachable!("SIGKILL returns no acknowledgment");
        }
    });
    let p = f.prepare("task-a").await.unwrap();
    let _ = f.remote.publish(&f.co, &f.project, &p.operation).await;
    panic!("private crash point not reached");
}
#[tokio::test]
async fn sigkill_intent_submission_effect_and_receipt_boundaries_reopen_without_duplicate_refs() {
    for (point, intents, effects) in [
        ("intent-before", 0, 0),
        ("intent-after", 1, 0),
        ("submit-before", 1, 0),
        ("submit-after", 1, 0),
        ("effect-before", 1, 0),
        ("effect-after", 1, 1),
        ("receipt-before", 1, 1),
        ("receipt-after", 1, 1),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let report = dir.path().join("report.json");
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(45),
            tokio::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", DRIVER, "--nocapture"])
                .env("ROLVYA_GIT_CRASH_POINT", point)
                .env("ROLVYA_GIT_CRASH_REPORT", &report)
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
            serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
        let root = std::path::PathBuf::from(info["root"].as_str().unwrap());
        let store = PlanStore::open(&root.join("data/state.sqlite"))
            .await
            .unwrap();
        store.check_integrity().await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM git_publications")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(count, intents, "{point}");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_acceptances")
                .fetch_one(&store.pool)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_attempts")
                .fetch_one(&store.pool)
                .await
                .unwrap(),
            2
        );
        let bootstrap = ControllerBootstrap::open(
            store.clone(),
            &root.join("keys"),
            ServiceIdentity::current(),
            &[],
        )
        .await
        .unwrap();
        let world = fixture::World::reopen(
            &root.join("remote-git.sqlite"),
            http::unhex(info["public"].as_str().unwrap()).unwrap(),
        )
        .await;
        assert_eq!(world.ref_count().await, effects, "{point}");
        let (origin, task) = world.server().await;
        let registry = provider_accounts::fixture_reopen(&bootstrap).await;
        let policy = registry.policy(synthetic::Authority::default());
        let project =
            rolvya_domain::ids::ProjectId::new(info["project"].as_str().unwrap()).unwrap();
        let co = SyntheticCoordinator::open(
            store.clone(),
            &project,
            &policy,
            rolvya_plan_validator::Limits::default(),
        )
        .await
        .unwrap();
        let mut remote = GitHubRemote::open(&bootstrap).await.unwrap();
        remote.clock = Arc::new(DeterministicClock::new(NOW).unwrap());
        remote.http = http::Api::fixture(origin);
        if intents == 1 {
            let op: String = sqlx::query_scalar("SELECT operation_id FROM git_publications")
                .fetch_one(&store.pool)
                .await
                .unwrap();
            let result = remote.reconcile(&co, &project, &op).await.unwrap();
            assert_eq!(
                result.state,
                if effects == 1 {
                    PublicationState::Published
                } else {
                    PublicationState::Unknown
                }
            );
            assert_eq!(
                world.attempts.load(Ordering::SeqCst),
                0,
                "recovery reads only at {point}"
            );
            let _ = remote.publish(&co, &project, &op).await;
            assert_eq!(
                world.attempts.load(Ordering::SeqCst),
                0,
                "uncertain replay never writes at {point}"
            );
        }
        task.abort();
        store.close().await;
        world.db.close().await;
        std::fs::remove_dir_all(root).unwrap();
    }
}
