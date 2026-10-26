use super::common::owner;
use super::*;
#[tokio::test]
async fn consistent_snapshot_restores_full_dag_ledger_messages_and_accepted_bytes() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    assert_eq!(
        co.run(&a, &clock, RunLimits::default())
            .await
            .unwrap()
            .status,
        RunStatus::Complete
    );
    let before = AuthorityImage::read(&f.store, &f.content.key).await;
    let receipts = count(&f, "worker_messages").await;
    let messages: Vec<(String, String, String, i64, String, String)> = sqlx::query_as(
        "SELECT m.project_id,m.attempt_id,m.message_id,m.sequence,m.digest,r.response FROM worker_messages m JOIN worker_receipts r USING(project_id,attempt_id,message_id) ORDER BY m.attempt_id,m.sequence",
    ).fetch_all(&f.store.pool).await.unwrap();
    assert_eq!(messages.len() as i64, receipts);
    let audits = count(&f, "scheduler_events").await;
    let fences: Vec<(String, i64)> =
        sqlx::query_as("SELECT task_id,epoch FROM task_fences ORDER BY task_id")
            .fetch_all(&f.store.pool)
            .await
            .unwrap();
    let output = f.directory.path().join("consistent.sqlite");
    f.store.snapshot(&output).await.unwrap();
    assert_eq!(
        f.store.snapshot(&output).await.unwrap_err(),
        StoreError::SnapshotExists
    );
    let start = std::time::Instant::now();
    let restored = reopen(&output).await;
    let co = SyntheticCoordinator::open(
        restored.clone(),
        &f.content.key.project_id,
        &a,
        Limits::default(),
    )
    .await
    .unwrap();
    co.validator().verify_all().await.unwrap();
    assert_eq!(
        AuthorityImage::read(&restored, &f.content.key).await,
        before
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_messages")
            .fetch_one(&restored.pool)
            .await
            .unwrap(),
        receipts
    );
    assert_eq!(sqlx::query_as::<_, (String, String, String, i64, String, String)>(
        "SELECT m.project_id,m.attempt_id,m.message_id,m.sequence,m.digest,r.response FROM worker_messages m JOIN worker_receipts r USING(project_id,attempt_id,message_id) ORDER BY m.attempt_id,m.sequence",
    ).fetch_all(&restored.pool).await.unwrap(), messages);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM scheduler_events")
            .fetch_one(&restored.pool)
            .await
            .unwrap(),
        audits
    );
    assert_eq!(
        sqlx::query_as::<_, (String, i64)>(
            "SELECT task_id,epoch FROM task_fences ORDER BY task_id"
        )
        .fetch_all(&restored.pool)
        .await
        .unwrap(),
        fences
    );
    assert_eq!(
        co.run(&a, &clock, RunLimits::default())
            .await
            .unwrap()
            .status,
        RunStatus::Complete
    );
    assert_eq!(
        AuthorityImage::read(&restored, &f.content.key).await,
        before
    );
    println!(
        "R10 complete restore+semantic verification observation: {:?}",
        start.elapsed()
    );
}
#[tokio::test]
async fn online_snapshot_racing_billing_preserves_one_consistent_four_category_revision() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    phase(&co, &a, &clock, 1, 16, 0).await;
    let l = lease(&f, "task-a").await;
    let before = AuthorityImage::read(&f.store, &f.content.key).await;
    let output = f.directory.path().join("racing.sqlite");
    let bill = FinancialRequest {
        project: f.content.key.project_id.clone(),
        request_id: RequestId::new("snapshot-bill").unwrap(),
        actor: owner(),
        reason: "Synthetic concurrent usage observation".into(),
        command: FinancialCommand::Report {
            liability: LiabilityId::new(l.attempt_id.as_str()).unwrap(),
            expected_revision: 1,
            unreserved: CostBreakdown {
                ai_api: money(2),
                compute: money(3),
                storage: money(4),
                transfer: money(5),
            },
            uncertain: false,
            evidence: e("snapshot-bill-evidence"),
        },
    };
    let (snapshot, receipt) = tokio::join!(
        f.store.snapshot(&output),
        f.store.apply_financial(&bill, &Finance)
    );
    snapshot.unwrap();
    receipt.unwrap();
    let after = AuthorityImage::read(&f.store, &f.content.key).await;
    let restored = reopen(&output).await;
    let actual = AuthorityImage::read(&restored, &f.content.key).await;
    assert!(
        actual == before || actual == after,
        "snapshot mixed ledger revisions"
    );
    let co = SyntheticCoordinator::open(restored, &f.content.key.project_id, &a, Limits::default())
        .await
        .unwrap();
    co.validator()
        .verify_accepted(&scope(&f, "task-a"))
        .await
        .unwrap();
}
#[tokio::test]
async fn sigkill_inside_vacuum_into_copy_preserves_source_and_never_certifies_partial_backup() {
    if let Some((path, point)) = child_case() {
        let store = PlanStore::open(&path).await.unwrap();
        let output = path.parent().unwrap().join("interrupted.sqlite");
        for mut c in connections(&store).await {
            let output = output.clone();
            c.lock_handle()
                .await
                .unwrap()
                .set_progress_handler(100, move || {
                    if std::fs::metadata(&output).is_ok_and(|m| m.len() > 4096) {
                        marker(point);
                    }
                    true
                });
        }
        store.snapshot(&output).await.unwrap();
        panic!("copy fault must interrupt before success acknowledgement");
    }
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    co.run(&a, &clock, RunLimits::default()).await.unwrap();
    // Bounded disposable copy data forces actual output page writes during VACUUM.
    sqlx::query("CREATE TABLE r10_backup_copy_data(payload BLOB)")
        .execute(&f.store.pool)
        .await
        .unwrap();
    let mut tx = f.store.pool.begin().await.unwrap();
    for _ in 0..1024 {
        sqlx::query("INSERT INTO r10_backup_copy_data VALUES(zeroblob(8192))")
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
    let before = AuthorityImage::read(&f.store, &f.content.key).await;
    let mut v = Victim::start(
        &f,
        FaultPoint::Snapshot,
        "resilience::backup::sigkill_inside_vacuum_into_copy_preserves_source_and_never_certifies_partial_backup",
    );
    v.ready();
    v.kill();
    f.store.close().await;
    let s = reopen(&f.path).await;
    assert_eq!(AuthorityImage::read(&s, &f.content.key).await, before);
    let co = SyntheticCoordinator::open(s, &f.content.key.project_id, &a, Limits::default())
        .await
        .unwrap();
    co.validator().verify_all().await.unwrap();
    let output = f.directory.path().join("interrupted.sqlite");
    assert!(output.metadata().unwrap().len() > 4096);
    // Open/integrity alone is never a restore certificate: require the expected
    // project image and hashes too. A partial copy may fail at any of these gates.
    let certified = if let Ok(partial) = PlanStore::open(&output).await {
        if let Ok(co) =
            SyntheticCoordinator::open(partial, &f.content.key.project_id, &a, Limits::default())
                .await
        {
            co.validator().verify_all().await.is_ok()
        } else {
            false
        }
    } else {
        false
    };
    assert!(
        !certified,
        "mid-copy fixture unexpectedly reached a complete semantic certificate"
    );
}
#[tokio::test]
async fn missing_or_corrupt_artifacts_in_disposable_restore_are_not_validated_by_integrity_check() {
    for missing in [false, true] {
        let f = fixture(100).await;
        let a = Authority::default();
        let co = open(&f, &a).await;
        let clock = DeterministicClock::new(T).unwrap();
        co.run(&a, &clock, RunLimits::default()).await.unwrap();
        let output = f.directory.path().join("damaged.sqlite");
        f.store.snapshot(&output).await.unwrap();
        let mut c = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(&output)
                .foreign_keys(true),
        )
        .await
        .unwrap();
        sqlx::query(if missing {
            "DROP TRIGGER immutable_synthetic_artifacts_delete"
        } else {
            "DROP TRIGGER immutable_synthetic_artifacts_update"
        })
        .execute(&mut c)
        .await
        .unwrap();
        sqlx::query(if missing {"DELETE FROM synthetic_artifacts WHERE attempt_id=(SELECT attempt_id FROM task_attempts WHERE task_id='task-a')"}else {"UPDATE synthetic_artifacts SET content=X'7B7D' WHERE attempt_id=(SELECT attempt_id FROM task_attempts WHERE task_id='task-a')"}).execute(&mut c).await.unwrap();
        c.close().await.unwrap();
        let restored = reopen(&output).await;
        let co =
            SyntheticCoordinator::open(restored, &f.content.key.project_id, &a, Limits::default())
                .await
                .unwrap();
        assert!(co.validator().verify_all().await.is_err());
        assert!(co.run(&a, &clock, RunLimits::default()).await.is_err());
        // Original accepted bytes and authority are untouched by corruption of the copy.
        open(&f, &a).await.validator().verify_all().await.unwrap();
        assert_eq!(count(&f, "task_acceptances").await, 4);
    }
}
#[tokio::test]
async fn stale_restore_requires_external_reconciliation_before_any_new_admission() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    phase(&co, &a, &clock, 1, 16, 0).await;
    let original = AuthorityImage::read(&f.store, &f.content.key).await;
    let la = lease(&f, "task-a").await;
    let output = f.directory.path().join("older.sqlite");
    f.store.snapshot(&output).await.unwrap();
    co.run(&a, &clock, RunLimits::default()).await.unwrap();
    let remote = SyntheticProvider::new(f.store.clone());
    remote
        .observe(
            &OperationKey {
                project: la.task.plan.project_id.clone(),
                attempt: la.attempt_id.clone(),
            },
            1,
            ProviderState::Unknown,
            true,
            &e("external-ahead-of-snapshot"),
        )
        .await
        .unwrap();
    let restored = reopen(&output).await;
    let paused = Authority {
        revoked: true,
        ..Authority::default()
    };
    let co = SyntheticCoordinator::open(
        restored.clone(),
        &f.content.key.project_id,
        &a,
        Limits::default(),
    )
    .await
    .unwrap();
    // ADR 0015's operator pause is explicit: a file alone cannot detect time travel.
    let result = co
        .pass_with_adapter(&paused, &remote, &clock, PassLimits::default())
        .await
        .unwrap();
    assert!(!result.blocked.is_empty());
    assert_eq!(
        AuthorityImage::read(&restored, &f.content.key).await,
        original
    );
    assert_eq!(
        OutboxDispatcher::new(restored.clone())
            .reconcile(&la, &remote, &a, &clock, Limits::default())
            .await
            .unwrap(),
        DispatchState::Blocked
    );
    let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
    assert_eq!(r.status, RunStatus::Blocked);
    assert_eq!(r.snapshot.accepted, 1);
    let image = AuthorityImage::read(&restored, &f.content.key).await;
    image.preserves(&original);
    assert_eq!(image.uncertain, 1);
    assert_eq!(image.attempts, 1);
    assert_eq!(image.ceiling, 100);
}
#[tokio::test]
async fn physical_corruption_of_disposable_snapshot_rejects_open_instead_of_resetting_data() {
    let f = fixture(100).await;
    let output = f.directory.path().join("corrupt-header.sqlite");
    f.store.snapshot(&output).await.unwrap();
    use std::io::{Seek, Write};
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(&output)
        .unwrap();
    file.seek(std::io::SeekFrom::Start(0)).unwrap();
    file.write_all(b"damaged snapshot!").unwrap();
    file.sync_all().unwrap();
    drop(file);
    assert!(PlanStore::open(&output).await.is_err());
    assert_eq!(
        f.store
            .load_active(
                &f.content.key.project_id,
                &Authority::default(),
                Limits::default()
            )
            .await
            .unwrap()
            .revision,
        f.revision
    );
}
