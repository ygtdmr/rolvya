mod common;
use common::*;
use rolvya_persistence_sqlite::*;
use rolvya_plan_validator::Limits;
use sqlx::migrate::Migrator;

#[tokio::test]
async fn migrations_replay_reopen_and_consistent_snapshot_restore() {
    let f = Fixture::new().await;
    let receipt = f.activate().await.unwrap();
    let snapshot = f.directory.path().join("snapshot.sqlite");
    f.store.snapshot(&snapshot).await.unwrap();
    assert_eq!(
        f.store.snapshot(&snapshot).await.unwrap_err(),
        StoreError::SnapshotExists
    );
    let restored = PlanStore::open(&snapshot).await.unwrap();
    restored.check_integrity().await.unwrap();
    let plan = restored
        .load(&f.content.key, &SyntheticPolicy, Limits::default())
        .await
        .unwrap();
    assert!(plan.active);
    assert_eq!(plan.revision, f.revision);
    assert_eq!(
        restored
            .activate(
                &f.revision,
                &request(),
                &owner(),
                &OwnerPolicy,
                &SyntheticPolicy,
                Limits::default()
            )
            .await
            .unwrap(),
        receipt
    );
    restored.close().await;
    let reopened = PlanStore::open(&snapshot).await.unwrap();
    reopened.check_integrity().await.unwrap();
    let raw = f.raw().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations WHERE success=1")
            .fetch_one(&raw)
            .await
            .unwrap(),
        8
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT sqlite_version()")
            .fetch_one(&raw)
            .await
            .unwrap()
            .split('.')
            .next(),
        Some("3")
    );
}
#[tokio::test]
async fn checksum_mismatch_and_unknown_migration_versions_fail_without_resetting_data() {
    for alteration in [
        "UPDATE _sqlx_migrations SET checksum=X'00'",
        "UPDATE _sqlx_migrations SET version=999 WHERE version=1",
    ] {
        let f = Fixture::new().await;
        let raw = f.raw().await;
        sqlx::query(alteration).execute(&raw).await.unwrap();
        raw.close().await;
        f.store.close().await;
        assert_eq!(
            PlanStore::open(&f.path).await.unwrap_err(),
            StoreError::Migration
        );
        let raw = f.raw().await;
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM tasks")
                .fetch_one(&raw)
                .await
                .unwrap(),
            3
        );
    }
}
#[tokio::test]
async fn unknown_schema_is_not_silently_adopted() {
    let f = Fixture::new().await;
    f.store.close().await;
    let raw = f.raw().await;
    sqlx::query("DROP TABLE _sqlx_migrations")
        .execute(&raw)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA journal_mode=DELETE")
            .fetch_one(&raw)
            .await
            .unwrap(),
        "delete"
    );
    raw.close().await;
    assert_eq!(
        PlanStore::open(&f.path).await.unwrap_err(),
        StoreError::UnknownDatabase
    );
    let raw = f.raw().await;
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA journal_mode")
            .fetch_one(&raw)
            .await
            .unwrap(),
        "delete",
        "refusing an unrelated schema must not convert its journal"
    );
}

#[tokio::test]
async fn failed_migration_rolls_back_ddl_and_data() {
    let f = Fixture::new().await;
    let raw = f.raw().await;
    let scripts = tempfile::tempdir().unwrap();
    std::fs::write(scripts.path().join("0009_injected_failure.sql"),"CREATE TABLE should_rollback(value TEXT); INSERT INTO should_rollback VALUES ('temporary'); SELECT absent_column FROM absent_table;").unwrap();
    // Replay real migrations, then verify failing v9 executes and rolls back atomically.
    std::fs::write(
        scripts.path().join("0001_plan_store.sql"),
        include_str!("../migrations/0001_plan_store.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0002_financial_ledger.sql"),
        include_str!("../migrations/0002_financial_ledger.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0003_scheduler_admission.sql"),
        include_str!("../migrations/0003_scheduler_admission.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0004_durable_worker_control.sql"),
        include_str!("../migrations/0004_durable_worker_control.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0005_synthetic_orchestration.sql"),
        include_str!("../migrations/0005_synthetic_orchestration.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0006_controller_key_bootstrap.sql"),
        include_str!("../migrations/0006_controller_key_bootstrap.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0007_secret_broker.sql"),
        include_str!("../migrations/0007_secret_broker.sql"),
    )
    .unwrap();
    std::fs::write(
        scripts.path().join("0008_human_primary_authentication.sql"),
        include_str!("../migrations/0008_human_primary_authentication.sql"),
    )
    .unwrap();
    let migrator = Migrator::new(scripts.path()).await.unwrap();
    let error = migrator.run(&raw).await.unwrap_err();
    assert!(
        matches!(error, sqlx::migrate::MigrateError::ExecuteMigration(_, 9)),
        "must execute the injected next migration, not reject migration metadata"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM sqlite_schema WHERE name='should_rollback'"
        )
        .fetch_one(&raw)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM tasks")
            .fetch_one(&raw)
            .await
            .unwrap(),
        3
    );
    f.store.close().await;
    raw.close().await;
    assert!(PlanStore::open(&f.path).await.is_ok());
}
#[tokio::test]
async fn foreign_key_corruption_is_detected_at_open() {
    let f = Fixture::new().await;
    let raw = f.raw().await;
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&raw)
        .await
        .unwrap();
    sqlx::query("INSERT INTO task_dependencies VALUES ('missing','plan',1,'a','b')")
        .execute(&raw)
        .await
        .unwrap();
    raw.close().await;
    f.store.close().await;
    assert_eq!(
        PlanStore::open(&f.path).await.unwrap_err(),
        StoreError::Integrity
    );
}
#[tokio::test]
async fn private_paths_and_snapshot_non_overwrite_are_enforced() {
    let f = Fixture::new().await;
    assert_eq!(
        f.store.snapshot(&f.path).await.unwrap_err(),
        StoreError::SnapshotExists
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        assert_eq!(
            std::fs::metadata(&f.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let link = f.directory.path().join("linked.sqlite");
        symlink(&f.path, &link).unwrap();
        assert_eq!(
            PlanStore::open(link).await.unwrap_err(),
            StoreError::InvalidPath
        );
        f.store.close().await;
        std::fs::set_permissions(&f.path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            PlanStore::open(&f.path).await.unwrap_err(),
            StoreError::InvalidPath
        );
    }
}

#[tokio::test]
async fn real_v1_upgrade_preserves_projects_and_failed_v2_leaves_v1_intact() {
    use sqlx::sqlite::SqliteConnectOptions;
    for inject_failure in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("upgrade.sqlite");
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&path).unwrap();
        let raw = sqlx::SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .foreign_keys(true),
        )
        .await
        .unwrap();
        let scripts = tempfile::tempdir().unwrap();
        std::fs::write(
            scripts.path().join("0001_plan_store.sql"),
            include_str!("../migrations/0001_plan_store.sql"),
        )
        .unwrap();
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects(project_id,owner_actor) VALUES ('legacy','owner')")
            .execute(&raw)
            .await
            .unwrap();
        if inject_failure {
            std::fs::write(
                scripts.path().join("0002_financial_ledger.sql"),
                format!(
                    "{}\nSELECT absent_column FROM absent_table;",
                    include_str!("../migrations/0002_financial_ledger.sql")
                ),
            )
            .unwrap();
            assert!(
                Migrator::new(scripts.path())
                    .await
                    .unwrap()
                    .run(&raw)
                    .await
                    .is_err()
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM sqlite_schema WHERE name='financial_events'"
                )
                .fetch_one(&raw)
                .await
                .unwrap(),
                0
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                    .fetch_one(&raw)
                    .await
                    .unwrap(),
                1
            );
        }
        raw.close().await;
        let upgraded = PlanStore::open(&path).await.unwrap();
        upgraded.check_integrity().await.unwrap();
        upgraded.close().await;
        let raw = sqlx::SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .foreign_keys(true),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT owner_actor FROM projects WHERE project_id='legacy'"
            )
            .fetch_one(&raw)
            .await
            .unwrap(),
            "owner"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations WHERE success=1")
                .fetch_one(&raw)
                .await
                .unwrap(),
            8
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM financial_events")
                .fetch_one(&raw)
                .await
                .unwrap(),
            0
        );
    }
}

#[tokio::test]
async fn v2_financial_data_survives_v3_and_failed_v3_rolls_back_before_repair() {
    use rolvya_persistence_sqlite::finance::*;
    use sqlx::sqlite::SqliteConnectOptions;
    for failing in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("v2.sqlite");
        let mut file = std::fs::OpenOptions::new();
        file.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            file.mode(0o600);
        }
        file.open(&db).unwrap();
        let raw = sqlx::SqlitePool::connect_with(
            SqliteConnectOptions::new().filename(&db).foreign_keys(true),
        )
        .await
        .unwrap();
        let scripts = tempfile::tempdir().unwrap();
        for (name, sql) in [
            (
                "0001_plan_store.sql",
                include_str!("../migrations/0001_plan_store.sql"),
            ),
            (
                "0002_financial_ledger.sql",
                include_str!("../migrations/0002_financial_ledger.sql"),
            ),
        ] {
            std::fs::write(scripts.path().join(name), sql).unwrap();
        }
        Migrator::new(scripts.path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects(project_id,owner_actor) VALUES ('legacy','owner')")
            .execute(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO financial_accounts VALUES ('legacy','USD')")
            .execute(&raw)
            .await
            .unwrap();
        let event:i64=sqlx::query_scalar("INSERT INTO financial_events(project_id,request_id,actor,recorded_at,kind,payload) VALUES ('legacy','approval','owner',1,'approval','{}') RETURNING sequence").fetch_one(&raw).await.unwrap();
        sqlx::query("INSERT INTO budget_approvals VALUES (?,'legacy',1,0,100,'legacy-proof',1)")
            .bind(event)
            .execute(&raw)
            .await
            .unwrap();
        let event:i64=sqlx::query_scalar("INSERT INTO financial_events(project_id,request_id,actor,recorded_at,kind,payload) VALUES ('legacy','reserve','owner',1,'reserve','{}') RETURNING sequence").fetch_one(&raw).await.unwrap();
        sqlx::query("INSERT INTO liability_revisions VALUES (?,'legacy','hold',1,0,0)")
            .bind(event)
            .execute(&raw)
            .await
            .unwrap();
        for category in ["ai_api", "compute", "storage", "transfer"] {
            sqlx::query("INSERT INTO financial_positions VALUES (?,?,0,10,0)")
                .bind(event)
                .bind(category)
                .execute(&raw)
                .await
                .unwrap();
        }
        if failing {
            std::fs::write(
                scripts.path().join("0003_scheduler_admission.sql"),
                format!(
                    "{}\nSELECT missing FROM absent;",
                    include_str!("../migrations/0003_scheduler_admission.sql")
                ),
            )
            .unwrap();
            assert!(
                Migrator::new(scripts.path())
                    .await
                    .unwrap()
                    .run(&raw)
                    .await
                    .is_err()
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM sqlite_schema WHERE name='task_attempts'"
                )
                .fetch_one(&raw)
                .await
                .unwrap(),
                0
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                    .fetch_one(&raw)
                    .await
                    .unwrap(),
                2
            );
        }
        raw.close().await;
        let store = PlanStore::open(&db).await.unwrap();
        let project = rolvya_domain::ids::ProjectId::new("legacy").unwrap();
        let balance = store.financial_snapshot(&project).await.unwrap();
        assert_eq!(balance.ceiling.minor_units(), 100);
        assert_eq!(balance.held.total().unwrap().minor_units(), 40);
        assert_eq!(balance.headroom.minor_units(), 60);
        assert_eq!(balance.version, 1);
        assert_eq!(
            store
                .financial_liability(&project, &LiabilityId::new("hold").unwrap())
                .await
                .unwrap()
                .revision,
            1
        );
        store.check_integrity().await.unwrap();
    }
}
