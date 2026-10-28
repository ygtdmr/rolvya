use super::*;
#[tokio::test]
async fn populated_v10_upgrade_and_failed_v11_preserve_human_and_worker_authority_without_auto_enrollment()
 {
    for fail_first in [false, true] {
        let root = temporary();
        let path = root.path().join("legacy.sqlite");
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
        options.open(&path).unwrap();
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(&path)
                    .foreign_keys(true)
                    .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                    .synchronous(sqlx::sqlite::SqliteSynchronous::Full)
                    .busy_timeout(std::time::Duration::from_millis(5000))
                    .pragma("recursive_triggers", "ON"),
            )
            .await
            .unwrap();
        let scripts = temporary();
        for entry in fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations")).unwrap() {
            let e = entry.unwrap();
            if e.file_name().to_str().unwrap() < "0011" {
                fs::copy(e.path(), scripts.path().join(e.file_name())).unwrap();
            }
        }
        sqlx::migrate::Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&pool)
            .await
            .unwrap();
        // Test-only adapter over an actual v10 schema, so existing R01–R08 APIs
        // construct the historical plan/financial/dispatch records before upgrade.
        let store = PlanStore { pool: pool.clone() };
        let content = common::plan();
        let authority = synthetic::Authority::default();
        let valid =
            rolvya_plan_validator::validate(&content, &authority, Limits::default()).unwrap();
        store
            .create_project(&content.key.project_id, &common::owner())
            .await
            .unwrap();
        let revision = store
            .save_draft(&valid, None, &common::owner(), &common::OwnerPolicy)
            .await
            .unwrap();
        store
            .activate(
                &revision,
                &common::request(),
                &common::owner(),
                &common::OwnerPolicy,
                &authority,
                Limits::default(),
            )
            .await
            .unwrap();
        store
            .apply_financial(
                &FinancialRequest {
                    project: content.key.project_id.clone(),
                    request_id: RequestId::new("historical-budget").unwrap(),
                    actor: common::owner(),
                    reason: "Historical synthetic schema fixture".into(),
                    command: FinancialCommand::Approve {
                        expected_version: 0,
                        new_ceiling: synthetic::money(100),
                        proof: OwnerProof {
                            reference: synthetic::e("historical-budget-proof"),
                            verified_at: SystemClock.now().unwrap(),
                        },
                    },
                },
                &synthetic::Finance,
            )
            .await
            .unwrap();
        let user = "a".repeat(32);
        let session = "b".repeat(32);
        let clock = DeterministicClock::new(synthetic::T).unwrap();
        let lease = store
            .admit(
                &admission::AdmissionRequest {
                    task: TaskRef {
                        plan: content.key.clone(),
                        task_id: TaskId::new("task-a").unwrap(),
                    },
                    fingerprint: revision.fingerprint().into(),
                    expected_epoch: 0,
                    request_id: RequestId::new("historical-attempt").unwrap(),
                    actor: common::owner(),
                    permit: admission::ExecutionPermit {
                        id: synthetic::e("historical-capacity"),
                        provider: synthetic::e("synthetic"),
                        account: synthetic::e("synthetic-account"),
                        pool: synthetic::e("synthetic-pool"),
                        executor: crate::PrincipalId::new(&user).unwrap(),
                        maximum: synthetic::costs(),
                        valid_until: synthetic::T + 3600,
                    },
                },
                &authority,
                &clock,
                Limits::default(),
            )
            .await
            .unwrap()
            .lease;
        let dispatcher = OutboxDispatcher::new(store.clone());
        let ticket = dispatcher
            .claim(
                &lease,
                &common::owner(),
                &authority,
                &clock,
                Limits::default(),
            )
            .await
            .unwrap();
        dispatcher
            .dispatch(
                &ticket,
                &SyntheticProvider::new(store.clone()),
                &authority,
                &clock,
                Limits::default(),
            )
            .await
            .unwrap();
        // Inert historical human rows: these are not verified sessions or binding
        // MACs. Matching human/worker ID strings must not create worker credentials.
        sqlx::query("INSERT INTO human_users VALUES(?,'active',1,0,10000)")
            .bind(&user)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO human_emails VALUES(?,'historical@example.invalid',1,1,1)")
            .bind(&user)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO human_login_policy VALUES(1,1,1,1)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO human_authentications VALUES('historical-primary',?,'magic_link',1,1,1,10000,0,'historical-email',zeroblob(32))").bind(&user).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO human_sessions VALUES(?,?,'historical-primary',1,1,1,1,'magic_link','primary',zeroblob(32),'historical-device',10000,7786000,10000,10000,1,NULL,zeroblob(32))").bind(&session).bind(&user).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO human_project_bindings VALUES(?,?,'owner','historical-binding',10000,zeroblob(32))").bind(content.key.project_id.as_str()).bind(&user).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO human_memberships VALUES(?,?,'owner','active',1)")
            .bind(content.key.project_id.as_str())
            .bind(&user)
            .execute(&pool)
            .await
            .unwrap();
        let before = store
            .financial_snapshot(&content.key.project_id)
            .await
            .unwrap();
        let command: String = sqlx::query_scalar("SELECT command FROM dispatch_operations")
            .fetch_one(&pool)
            .await
            .unwrap();
        let fingerprint: String = sqlx::query_scalar("SELECT fingerprint FROM plan_versions")
            .fetch_one(&pool)
            .await
            .unwrap();
        let next = scripts.path().join("0011_worker_credential_authority.sql");
        let migration = include_str!("../../../../migrations/0011_worker_credential_authority.sql");
        if fail_first {
            fs::write(
                &next,
                format!("{migration}\nSELECT absent_column FROM absent_table;"),
            )
            .unwrap();
            assert!(
                sqlx::migrate::Migrator::new(scripts.path())
                    .await
                    .unwrap()
                    .run(&pool)
                    .await
                    .is_err()
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                10
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM sqlite_schema WHERE name='worker_enrollments'"
                )
                .fetch_one(&pool)
                .await
                .unwrap(),
                0
            );
            assert_eq!(
                store
                    .financial_snapshot(&content.key.project_id)
                    .await
                    .unwrap()
                    .held,
                before.held
            );
        }
        fs::write(&next, migration).unwrap();
        sqlx::migrate::Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&pool)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                .fetch_one(&pool)
                .await
                .unwrap(),
            11
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_enrollments")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_credentials")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT command FROM dispatch_operations")
                .fetch_one(&pool)
                .await
                .unwrap(),
            command
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT fingerprint FROM plan_versions")
                .fetch_one(&pool)
                .await
                .unwrap(),
            fingerprint
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT session_id FROM human_sessions")
                .fetch_one(&pool)
                .await
                .unwrap(),
            session
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT user_id FROM human_memberships WHERE role='owner'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            user
        );
        assert_eq!(
            store
                .financial_snapshot(&content.key.project_id)
                .await
                .unwrap()
                .held,
            before.held
        );
        pool.close().await;
        let reopened = PlanStore::open(&path).await.unwrap();
        reopened.check_integrity().await.unwrap();
        assert_eq!(
            reopened
                .financial_snapshot(&content.key.project_id)
                .await
                .unwrap()
                .held,
            synthetic::costs()
        );
    }
}
