use super::*;
use std::os::unix::process::ExitStatusExt;
const DRIVER: &str =
    "controller_keys::worker_auth::https::tests::lifecycle::process::lifecycle_crash_child";
#[tokio::test]
async fn lifecycle_crash_child() {
    let Ok(point) = std::env::var("ROLVYA_EXECUTION_CRASH_CHILD") else {
        return;
    };
    let f = FixtureRun::new().await;
    f.dispatch().await;
    if point.starts_with("stop-") {
        f.f.clock.advance_to(synthetic::T + 60).unwrap();
        let _ = f
            .adapter
            .stop(&f.expected.lease(), run::StopReason::RuntimeLimit)
            .await;
    }
    panic!("private SIGKILL checkpoint not reached");
}
#[tokio::test]
async fn sigkill_launch_receipt_stop_and_preparation_reopen_without_partial_authority() {
    for (point, requests, effects, stops) in [
        ("prepare-before", 0, 0, 0),
        ("prepare-after", 1, 0, 0),
        ("submit-before", 1, 0, 0),
        ("submit-after", 1, 0, 0),
        ("effect-after", 1, 1, 0),
        ("receipt-before", 1, 1, 0),
        ("receipt-after", 1, 1, 0),
        ("stop-before", 1, 1, 0),
        ("stop-after", 1, 1, 0),
        ("stop-effect-after", 1, 1, 1),
    ] {
        let report = temporary();
        let report_path = report.path().join("report.json");
        let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", DRIVER, "--nocapture"])
            .env("ROLVYA_EXECUTION_CRASH_CHILD", point)
            .env("ROLVYA_EXECUTION_CRASH_REPORT", &report_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let status = tokio::time::timeout(std::time::Duration::from_secs(30), child.wait())
            .await
            .expect("bounded child")
            .unwrap();
        assert_eq!(status.signal(), Some(9), "checkpoint {point}");
        let info: serde_json::Value =
            serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
        let root = std::path::PathBuf::from(info["root"].as_str().unwrap());
        let store = PlanStore::open(&root.join("data/state.sqlite"))
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
                .fetch_one(&store.pool)
                .await
                .unwrap(),
            "ok"
        );
        assert!(
            sqlx::query("PRAGMA foreign_key_check")
                .fetch_all(&store.pool)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM aws_launch_requests")
                .fetch_one(&store.pool)
                .await
                .unwrap(),
            requests,
            "{point}"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_attempts")
                .fetch_one(&store.pool)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_credentials")
                .fetch_one(&store.pool)
                .await
                .unwrap(),
            0
        );
        let before = store
            .financial_snapshot(&ProjectId::new("project").unwrap())
            .await
            .unwrap();
        assert_eq!(
            before.held,
            rolvya_domain::money::CostBreakdown {
                ai_api: synthetic::money(2),
                compute: synthetic::money(4),
                storage: synthetic::money(6),
                transfer: synthetic::money(2)
            }
        );
        let external = sqlx::SqlitePool::connect(&format!(
            "sqlite://{}",
            root.join("external-aws.sqlite").display()
        ))
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM effects")
                .fetch_one(&external)
                .await
                .unwrap(),
            effects,
            "{point}"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT coalesce(sum(stops),0) FROM effects")
                .fetch_one(&external)
                .await
                .unwrap(),
            stops,
            "{point}"
        );
        if point == "stop-after" {
            let expected: ExpectedExecution =
                serde_json::from_value(info["expected"].clone()).unwrap();
            let bootstrap = ControllerBootstrap::open(
                store.clone(),
                &root.join("keys"),
                ServiceIdentity::current(),
                &[],
            )
            .await
            .unwrap();
            let registry = provider_accounts::fixture_registry_for(
                &bootstrap,
                synthetic::T,
                AWS_PROVIDER,
                "aws-account-a",
            )
            .await;
            let policy = Arc::new(registry.policy(AwsAuthority::default()));
            let clock = Arc::new(DeterministicClock::new(synthetic::T + 60).unwrap());
            let http = aws::fixture::AwsFixture::new(&expected);
            let (identity, _) =
                aws::fixture::open(&bootstrap, policy, clock, &expected, http.http()).await;
            let world =
                run::fixture::World::new(&root.join("external-aws.sqlite"), vec![expected.clone()])
                    .await;
            // Private fixture explicitly re-establishes external inventory trust;
            // no corresponding production activation grant exists.
            let adapter = run::fixture::open(identity, world.clone(), true).await;
            assert_eq!(
                adapter
                    .stop(&expected.lease(), run::StopReason::RuntimeLimit)
                    .await
                    .unwrap()
                    .kind,
                "stop_ack"
            );
            assert_eq!(world.stop_count().await, 1);
            assert_eq!(world.count().await, 1);
            assert_eq!(
                store
                    .financial_snapshot(&expected.binding.task.plan.project_id)
                    .await
                    .unwrap()
                    .held,
                before.held
            );
        }
        external.close().await;
        store.pool.close().await;
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".tmp")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
