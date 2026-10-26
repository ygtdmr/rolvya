use super::*;
#[tokio::test]
async fn all_four_connections_enforce_foreign_keys_durability_and_recursive_guards() {
    let dir = tempfile::tempdir().unwrap();
    let store = PlanStore::open(dir.path().join("test.sqlite"))
        .await
        .unwrap();
    let mut connections = Vec::new();
    for _ in 0..4 {
        connections.push(store.pool.acquire().await.unwrap());
    }
    for connection in &mut connections {
        for (pragma, expected) in [
            ("foreign_keys", 1),
            ("synchronous", 2),
            ("busy_timeout", 5000),
            ("recursive_triggers", 1),
            ("wal_autocheckpoint", 1000),
        ] {
            assert_eq!(
                sqlx::query_scalar::<_, i64>(&format!("PRAGMA {pragma}"))
                    .fetch_one(&mut **connection)
                    .await
                    .unwrap(),
                expected
            );
        }
        assert_eq!(
            sqlx::query_scalar::<_, String>("PRAGMA journal_mode")
                .fetch_one(&mut **connection)
                .await
                .unwrap(),
            "wal"
        );
        assert!(
            sqlx::query("INSERT INTO task_dependencies VALUES ('absent','plan',1,'a','b')")
                .execute(&mut **connection)
                .await
                .is_err()
        );
    }
    eprintln!(
        "SQLite {}: WAL/FULL, 4 connections, 5000ms busy timeout, FK/recursive guards checked",
        sqlx::query_scalar::<_, String>("SELECT sqlite_version()")
            .fetch_one(&mut *connections[0])
            .await
            .unwrap()
    );
}
#[test]
fn storage_identifier_and_version_ranges_do_not_truncate() {
    assert!(PrincipalId::new("BAD").is_err());
    assert!(RequestId::new("").is_err());
    let key = PlanKey {
        project_id: ProjectId::new("project").unwrap(),
        plan_id: rolvya_domain::ids::PlanId::new("plan").unwrap(),
        version: rolvya_domain::ids::PlanVersion::new(u64::MAX).unwrap(),
    };
    assert_eq!(version(&key), Err(StoreError::VersionOutOfRange));
}
