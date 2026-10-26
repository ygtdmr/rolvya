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
        1
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
        "UPDATE _sqlx_migrations SET version=999",
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
    std::fs::write(scripts.path().join("0002_injected_failure.sql"),"CREATE TABLE should_rollback(value TEXT); INSERT INTO should_rollback VALUES ('temporary'); SELECT absent_column FROM absent_table;").unwrap();
    // This fixture intentionally includes the real v1, then a failing v2.
    std::fs::write(
        scripts.path().join("0001_plan_store.sql"),
        include_str!("../migrations/0001_plan_store.sql"),
    )
    .unwrap();
    let migrator = Migrator::new(scripts.path()).await.unwrap();
    assert!(migrator.run(&raw).await.is_err());
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
