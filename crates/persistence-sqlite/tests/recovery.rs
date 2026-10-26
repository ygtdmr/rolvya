mod common;
use common::*;
use rolvya_persistence_sqlite::*;
use rolvya_plan_validator::Limits;
use std::time::{Duration, Instant};

#[tokio::test]
async fn busy_writer_fails_closed_after_bounded_wait_and_wal_readers_continue() {
    let f = Fixture::new().await;
    let raw = f.raw().await;
    let mut tx = raw.begin_with("BEGIN IMMEDIATE").await.unwrap();
    sqlx::query("INSERT INTO task_runtime VALUES ('project','plan',1,'task-a','waiting')")
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(
        !f.store
            .load(&f.content.key, &SyntheticPolicy, Limits::default())
            .await
            .unwrap()
            .active
    );
    let started = Instant::now();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(12), f.activate())
            .await
            .unwrap()
            .unwrap_err(),
        StoreError::Busy
    );
    assert!(started.elapsed() >= Duration::from_secs(4));
    tx.rollback().await.unwrap();
    f.activate().await.unwrap();
}

#[tokio::test]
async fn process_exit_before_commit_rolls_back_and_after_commit_replays_receipt() {
    if let Ok(mode) = std::env::var("ROLVYA_TEST_PROCESS_MODE") {
        let path = std::env::var("ROLVYA_TEST_DB").unwrap();
        if mode == "uncommitted" {
            use sqlx::sqlite::SqliteConnectOptions;
            let pool = sqlx::SqlitePool::connect_with(
                SqliteConnectOptions::new()
                    .filename(&path)
                    .foreign_keys(true),
            )
            .await
            .unwrap();
            let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
            sqlx::query("INSERT INTO task_runtime VALUES ('project','plan',1,'task-a','waiting')")
                .execute(&mut *tx)
                .await
                .unwrap();
            // Exit without Drop/rollback: tests SQLite's recovery of an interrupted writer.
            std::process::exit(17);
        }
        let store = PlanStore::open(&path).await.unwrap();
        let p = plan();
        // The parent stores a group fixture: recover the actual canonical content.
        let loaded = store
            .load(&p.key, &SyntheticPolicy, Limits::default())
            .await
            .unwrap();
        store
            .activate(
                &loaded.revision,
                &request(),
                &owner(),
                &OwnerPolicy,
                &SyntheticPolicy,
                Limits::default(),
            )
            .await
            .unwrap();
        // Simulate a lost acknowledgement after the real API's commit.
        std::process::exit(17);
    }
    let f = Fixture::new().await;
    for mode in ["uncommitted", "committed"] {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "process_exit_before_commit_rolls_back_and_after_commit_replays_receipt",
                "--nocapture",
            ])
            .env("ROLVYA_TEST_PROCESS_MODE", mode)
            .env("ROLVYA_TEST_DB", &f.path)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(17));
        let loaded = f
            .store
            .load(&f.content.key, &SyntheticPolicy, Limits::default())
            .await
            .unwrap();
        assert_eq!(loaded.active, mode == "committed");
        if mode == "uncommitted" {
            let raw = f.raw().await;
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_runtime")
                    .fetch_one(&raw)
                    .await
                    .unwrap(),
                0
            );
        }
    }
    let a = f.activate().await.unwrap();
    assert_eq!(a, f.activate().await.unwrap());
}
