use super::common::owner;
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
async fn cap(store: &PlanStore, pages: i64) {
    for mut c in connections(store).await {
        let n: i64 = sqlx::query_scalar(&format!("PRAGMA max_page_count={pages}"))
            .fetch_one(&mut *c)
            .await
            .unwrap();
        assert_eq!(n, pages);
    }
}
async fn pressure(store: &PlanStore, table: &str) {
    sqlx::query("CREATE TABLE r10_pressure(value BLOB)")
        .execute(&store.pool)
        .await
        .unwrap();
    let pages: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    cap(store, pages + 16).await;
    let err = sqlx::query("INSERT INTO r10_pressure VALUES(zeroblob(4194304))")
        .execute(&store.pool)
        .await
        .unwrap_err();
    assert_eq!(
        err.as_database_error().unwrap().code().as_deref(),
        Some("13"),
        "expected actual SQLITE_FULL"
    );
    sqlx::query(&format!("CREATE TRIGGER r10_pressure_gate BEFORE INSERT ON {table} BEGIN INSERT INTO r10_pressure VALUES(zeroblob(4194304)); END")).execute(&store.pool).await.unwrap();
}
async fn clear_pressure(store: &PlanStore) {
    cap(store, 1_000_000).await;
    sqlx::query("DROP TRIGGER r10_pressure_gate")
        .execute(&store.pool)
        .await
        .unwrap();
    sqlx::query("DROP TABLE r10_pressure")
        .execute(&store.pool)
        .await
        .unwrap();
}
#[tokio::test]
async fn sqlite_full_at_outbox_rolls_back_every_prior_financial_and_attempt_write() {
    let f = fixture(100).await;
    let before = AuthorityImage::read(&f.store, &f.content.key).await;
    pressure(&f.store, "dispatch_outbox").await;
    assert!(
        f.store
            .admit(
                &standalone(&f),
                &Authority::default(),
                &DeterministicClock::new(T).unwrap(),
                Limits::default()
            )
            .await
            .is_err()
    );
    assert_eq!(AuthorityImage::read(&f.store, &f.content.key).await, before);
    clear_pressure(&f.store).await;
    f.store.close().await;
    let s = reopen(&f.path).await;
    s.admit(
        &standalone(&f),
        &Authority::default(),
        &DeterministicClock::new(T).unwrap(),
        Limits::default(),
    )
    .await
    .unwrap();
    assert_eq!(AuthorityImage::read(&s, &f.content.key).await.attempts, 1);
}
#[tokio::test]
async fn sqlite_full_at_result_receipt_does_not_leave_session_or_journal_half_applied() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    for (i, t) in [0, 1, 2, 30, 42].into_iter().enumerate() {
        clock.advance_to(T + t).unwrap();
        co.pass(
            &a,
            &clock,
            PassLimits {
                admissions: if i == 0 { 2 } else { 0 },
                validations: 0,
                groups: 0,
                ..PassLimits::default()
            },
        )
        .await
        .unwrap();
    }
    let image = AuthorityImage::read(&f.store, &f.content.key).await;
    pressure(&f.store, "worker_receipts").await;
    clock.advance_to(T + 50).unwrap();
    let result = co
        .pass(
            &a,
            &clock,
            PassLimits {
                admissions: 0,
                validations: 0,
                groups: 0,
                ..PassLimits::default()
            },
        )
        .await;
    let report = result.unwrap();
    assert_eq!(report.blocked.len(), 2);
    assert_eq!(report.snapshot.received_unvalidated, 0);
    assert_eq!(AuthorityImage::read(&f.store, &f.content.key).await, image);
    assert_eq!(count(&f, "worker_messages").await, 10);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM worker_sessions WHERE json_extract(state,'$.result') IS NOT NULL"
        )
        .fetch_one(&f.store.pool)
        .await
        .unwrap(),
        0
    );
    clear_pressure(&f.store).await;
    f.store.close().await;
    let s = reopen(&f.path).await;
    // Rejected delivery projections can be inspected; do not fabricate an ACK or
    // erase the fault. The same real worker frames remain legal at the authority.
    let rows: Vec<Vec<u8>> =
        sqlx::query_scalar("SELECT payload FROM synthetic_worker_events WHERE ordinal=5")
            .fetch_all(&s.pool)
            .await
            .unwrap();
    let controller = DurableWorkerController::new(s.clone());
    for bytes in rows {
        let m = decode_worker_message(&bytes).unwrap();
        controller
            .receive(
                &a.peer(&m.binding),
                &bytes,
                &a,
                &a,
                &clock,
                Limits::default(),
            )
            .await
            .unwrap();
    }
    let co =
        SyntheticCoordinator::open(s.clone(), &f.content.key.project_id, &a, Limits::default())
            .await
            .unwrap();
    assert_eq!(
        co.run(&a, &clock, RunLimits::default())
            .await
            .unwrap()
            .status,
        RunStatus::Complete
    );
    assert_eq!(AuthorityImage::read(&s, &f.content.key).await.effects, 3);
}
#[tokio::test]
async fn sqlite_full_at_acceptance_preserves_candidate_evidence_but_not_false_success() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    phase(&co, &a, &clock, 2, 0, 0).await;
    let before = AuthorityImage::read(&f.store, &f.content.key).await;
    pressure(&f.store, "task_acceptances").await;
    assert!(
        co.validator()
            .validate_leaf(&lease(&f, "task-a").await, &a, &clock)
            .await
            .is_err()
    );
    assert_eq!(AuthorityImage::read(&f.store, &f.content.key).await, before);
    clear_pressure(&f.store).await;
    assert_eq!(
        co.run(&a, &clock, RunLimits::default())
            .await
            .unwrap()
            .status,
        RunStatus::Complete
    );
}
#[tokio::test]
async fn engine_interrupt_and_commit_veto_leave_no_partial_admission() {
    for veto in [false, true] {
        let f = fixture(100).await;
        let before = AuthorityImage::read(&f.store, &f.content.key).await;
        if !veto {
            let mut c = f.store.pool.acquire().await.unwrap();
            c.lock_handle()
                .await
                .unwrap()
                .set_progress_handler(1, || false);
            let error = sqlx::query("SELECT 1").execute(&mut *c).await.unwrap_err();
            assert_eq!(
                error.as_database_error().unwrap().code().as_deref(),
                Some("9")
            );
            c.lock_handle().await.unwrap().remove_progress_handler();
        }
        let armed = Arc::new(AtomicBool::new(false));
        for mut c in connections(&f.store).await {
            if veto {
                c.lock_handle().await.unwrap().set_commit_hook(|| false);
            } else {
                let signal = armed.clone();
                c.lock_handle().await.unwrap().set_update_hook(move |r| {
                    if r.table == "financial_events" {
                        signal.store(true, Ordering::SeqCst);
                    }
                });
                let signal = armed.clone();
                c.lock_handle()
                    .await
                    .unwrap()
                    .set_progress_handler(1, move || !signal.load(Ordering::SeqCst));
            }
        }
        assert!(
            f.store
                .admit(
                    &standalone(&f),
                    &Authority::default(),
                    &DeterministicClock::new(T).unwrap(),
                    Limits::default()
                )
                .await
                .is_err()
        );
        for mut c in connections(&f.store).await {
            let mut h = c.lock_handle().await.unwrap();
            h.remove_progress_handler();
            h.remove_update_hook();
            h.remove_commit_hook();
        }
        f.store.close().await;
        let s = reopen(&f.path).await;
        assert_eq!(AuthorityImage::read(&s, &f.content.key).await, before);
        s.admit(
            &standalone(&f),
            &Authority::default(),
            &DeterministicClock::new(T).unwrap(),
            Limits::default(),
        )
        .await
        .unwrap();
    }
}
#[tokio::test]
async fn eligibility_read_cannot_admit_against_headroom_changed_before_revalidation() {
    if let Some((path, point)) = child_case() {
        let s = PlanStore::open(path).await.unwrap();
        let plan = s
            .load_active(
                &ProjectId::new("project").unwrap(),
                &Authority::default(),
                Limits::default(),
            )
            .await
            .unwrap();
        let p = Probe {
            base: Authority::default(),
            point,
            used: AtomicBool::new(false),
            resume: true,
        };
        assert!(
            s.admit(
                &admission_request(
                    &plan.content.key,
                    plan.revision.fingerprint(),
                    "fault-attempt",
                    "task-a"
                ),
                &p,
                &DeterministicClock::new(T).unwrap(),
                Limits::default()
            )
            .await
            .is_err()
        );
        return;
    }
    let f = fixture(100).await;
    f.store
        .apply_financial(
            &FinancialRequest {
                project: f.content.key.project_id.clone(),
                request_id: RequestId::new("existing-hold").unwrap(),
                actor: owner(),
                reason: "Scoped synthetic liability".into(),
                command: FinancialCommand::Reserve {
                    liability: LiabilityId::new("existing").unwrap(),
                    maximum: CostBreakdown {
                        ai_api: money(1),
                        ..zero()
                    },
                    bound_evidence: e("existing-bound"),
                },
            },
            &Finance,
        )
        .await
        .unwrap();
    let mut v = Victim::start(
        &f,
        FaultPoint::EligibilityRead,
        "resilience::storage::eligibility_read_cannot_admit_against_headroom_changed_before_revalidation",
    );
    v.ready();
    f.store
        .apply_financial(
            &FinancialRequest {
                project: f.content.key.project_id.clone(),
                request_id: RequestId::new("racing-charge").unwrap(),
                actor: owner(),
                reason: "Synthetic accrued exposure, not a real invoice".into(),
                command: FinancialCommand::Report {
                    liability: LiabilityId::new("existing").unwrap(),
                    expected_revision: 1,
                    unreserved: CostBreakdown {
                        ai_api: money(95),
                        ..zero()
                    },
                    uncertain: false,
                    evidence: e("racing-charge-evidence"),
                },
            },
            &Finance,
        )
        .await
        .unwrap();
    v.resume();
    let image = AuthorityImage::read(&f.store, &f.content.key).await;
    assert_eq!(image.attempts, 0);
    assert_eq!(image.positions[4], 1);
    assert_eq!(image.positions[8], 95);
    assert_eq!(image.ceiling, 100);
}
#[tokio::test]
async fn pinned_wal_reader_and_partial_checkpoint_survive_process_kill() {
    if let Some((path, point)) = child_case() {
        let s = PlanStore::open(path).await.unwrap();
        let row = sqlx::query("PRAGMA wal_checkpoint(PASSIVE)")
            .fetch_one(&s.pool)
            .await
            .unwrap();
        assert!(
            row.get::<i64, _>(1) > row.get::<i64, _>(2),
            "parent must pin uncheckpointed WAL"
        );
        marker(point);
    }
    let f = fixture(100).await;
    let mut conn = f.store.pool.acquire().await.unwrap();
    let mut reader = conn.begin().await.unwrap();
    sqlx::query("SELECT count(*) FROM financial_events")
        .fetch_one(&mut *reader)
        .await
        .unwrap();
    f.store
        .admit(
            &standalone(&f),
            &Authority::default(),
            &DeterministicClock::new(T).unwrap(),
            Limits::default(),
        )
        .await
        .unwrap();
    let image = AuthorityImage::read(&f.store, &f.content.key).await;
    let mut v = Victim::start(
        &f,
        FaultPoint::Checkpoint,
        "resilience::storage::pinned_wal_reader_and_partial_checkpoint_survive_process_kill",
    );
    v.ready();
    v.kill();
    let mut check = f.store.pool.acquire().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA busy_timeout")
            .fetch_one(&mut *check)
            .await
            .unwrap(),
        5000
    );
    let start = std::time::Instant::now();
    let result = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .fetch_one(&mut *check)
        .await
        .unwrap();
    let elapsed = start.elapsed();
    assert_eq!(result.get::<i64, _>(0), 1);
    assert!(
        elapsed >= std::time::Duration::from_secs(4)
            && elapsed < std::time::Duration::from_secs(20)
    );
    println!("R10 checkpoint bounded busy observation: {elapsed:?}");
    reader.rollback().await.unwrap();
    drop(conn);
    let result = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .fetch_one(&mut *check)
        .await
        .unwrap();
    assert_eq!(result.get::<i64, _>(0), 0);
    drop(check);
    f.store.close().await;
    let s = reopen(&f.path).await;
    assert_eq!(AuthorityImage::read(&s, &f.content.key).await, image);
}
