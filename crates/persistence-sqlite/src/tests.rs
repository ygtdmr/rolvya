use super::*;

#[tokio::test]
async fn close_drains_connection_returned_after_the_initial_idle_scan() {
    use std::{
        future::Future,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        task::{Context, Poll, Waker},
    };
    use tokio::sync::Notify;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("close.sqlite");
    let armed = Arc::new(AtomicBool::new(false));
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let callback = (armed.clone(), entered.clone(), release.clone());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .after_release(move |_, _| {
            let (armed, entered, release) = callback.clone();
            Box::pin(async move {
                if armed.swap(false, Ordering::SeqCst) {
                    entered.notify_one();
                    release.notified().await;
                }
                Ok(true)
            })
        })
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
                .synchronous(SqliteSynchronous::Full)
                .disable_statement_logging(),
        )
        .await
        .unwrap();
    let store = PlanStore { pool };
    let mut connection = store.pool.acquire().await.unwrap();
    sqlx::query("CREATE TABLE sentinel(value INTEGER NOT NULL)")
        .execute(&mut *connection)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sentinel VALUES(42)")
        .execute(&mut *connection)
        .await
        .unwrap();
    // Hold a normal return after SQLx's closed check, modelling its default ping
    // await. Pool::close has already scanned idle entries when release resumes.
    armed.store(true, Ordering::SeqCst);
    drop(connection);
    entered.notified().await;
    let mut closing = Box::pin(store.close());
    assert!(matches!(
        closing
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Pending
    ));
    assert!(store.pool.is_closed());
    release.notify_one();
    closing.await;
    assert_eq!(
        store.pool.size(),
        0,
        "awaited store close must drain the late idle return"
    );
    let mut reopened = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .connect()
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT value FROM sentinel")
            .fetch_one(&mut reopened)
            .await
            .unwrap(),
        42
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
            .fetch_one(&mut reopened)
            .await
            .unwrap(),
        "ok"
    );
    reopened.close().await.unwrap();
}
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
