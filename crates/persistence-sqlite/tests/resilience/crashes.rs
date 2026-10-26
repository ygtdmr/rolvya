use super::common::owner;
use super::*;
use std::sync::atomic::AtomicBool;
#[tokio::test]
async fn sigkill_at_admission_write_boundaries_has_no_partial_authority() {
    if let Some((path, point)) = child_case() {
        let store = PlanStore::open(path).await.unwrap();
        let plan = store
            .load_active(
                &ProjectId::new("project").unwrap(),
                &Authority::default(),
                Limits::default(),
            )
            .await
            .unwrap();
        if point == FaultPoint::BeforeAcquire {
            marker(point);
        }
        arm(&store, point).await;
        let policy = Probe {
            base: Authority::default(),
            point,
            used: AtomicBool::new(false),
            resume: false,
        };
        store
            .admit(
                &admission_request(
                    &plan.content.key,
                    plan.revision.fingerprint(),
                    "fault-attempt",
                    "task-a",
                ),
                &policy,
                &DeterministicClock::new(T).unwrap(),
                Limits::default(),
            )
            .await
            .unwrap();
        if point == FaultPoint::AfterCommit {
            marker(point);
        }
        panic!("typed fault boundary not reached: {point:?}");
    }
    for point in [
        FaultPoint::BeforeAcquire,
        FaultPoint::EligibilityRead,
        FaultPoint::WriteLock,
        FaultPoint::AfterHeadroom,
        FaultPoint::AfterReservation,
        FaultPoint::AttemptCreation,
        FaultPoint::FenceAdvance,
        FaultPoint::AuditInsertion,
        FaultPoint::OutboxInsertion,
        FaultPoint::BeforeCommit,
        FaultPoint::AfterCommit,
    ] {
        let f = fixture(100).await;
        let before = AuthorityImage::read(&f.store, &f.content.key).await;
        let mut victim = Victim::start(
            &f,
            point,
            "resilience::crashes::sigkill_at_admission_write_boundaries_has_no_partial_authority",
        );
        victim.ready();
        // An independent WAL reader never sees the child's uncommitted reservation.
        let visible = AuthorityImage::read(&f.store, &f.content.key).await;
        assert_eq!(
            visible.attempts,
            if point == FaultPoint::AfterCommit {
                1
            } else {
                0
            }
        );
        victim.kill();
        f.store.close().await;
        let restored = reopen(&f.path).await;
        let image = AuthorityImage::read(&restored, &f.content.key).await;
        image.preserves(&before);
        assert_eq!(image.ceiling, 100);
        if point == FaultPoint::AfterCommit {
            assert_eq!(image.attempts, 1);
            assert_eq!(image.effects, 0);
            assert_eq!(image.positions[4..8], [1, 2, 3, 1]);
            restored
                .admit(
                    &standalone(&f),
                    &Authority::default(),
                    &DeterministicClock::new(T).unwrap(),
                    Limits::default(),
                )
                .await
                .unwrap();
            assert_eq!(AuthorityImage::read(&restored, &f.content.key).await, image);
        } else {
            assert_eq!(image, before, "{point:?}");
        }
    }
}
#[tokio::test]
async fn sigkill_dispatch_and_result_gaps_reconcile_without_second_effect() {
    if let Some((path, point)) = child_case() {
        let store = PlanStore::open(path).await.unwrap();
        let a = Authority::default();
        let clock = DeterministicClock::new(
            if matches!(point, FaultPoint::MessageWrite | FaultPoint::ResultJournal) {
                T + 50
            } else {
                T
            },
        )
        .unwrap();
        if matches!(point, FaultPoint::MessageWrite | FaultPoint::ResultJournal) {
            if point == FaultPoint::MessageWrite {
                arm(&store, point).await;
            }
            let bytes:Vec<u8>=sqlx::query_scalar("SELECT e.payload FROM synthetic_worker_events e JOIN task_attempts a USING(project_id,attempt_id) WHERE a.task_id='task-a' AND e.ordinal=5").fetch_one(&store.pool).await.unwrap();
            let m = decode_worker_message(&bytes).unwrap();
            DurableWorkerController::new(store)
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
            marker(point);
        }
        let plan = store
            .load_active(&ProjectId::new("project").unwrap(), &a, Limits::default())
            .await
            .unwrap();
        let receipt = store
            .admit(
                &admission_request(
                    &plan.content.key,
                    plan.revision.fingerprint(),
                    "fault-attempt",
                    "task-a",
                ),
                &a,
                &clock,
                Limits::default(),
            )
            .await
            .unwrap();
        let d = OutboxDispatcher::new(store.clone());
        let ticket = d
            .claim(&receipt.lease, &owner(), &a, &clock, Limits::default())
            .await
            .unwrap();
        if point == FaultPoint::DispatchClaim {
            marker(point);
        }
        d.dispatch(
            &ticket,
            &EffectVictim {
                provider: SyntheticProvider::new(store),
            },
            &a,
            &clock,
            Limits::default(),
        )
        .await
        .unwrap();
        panic!("effect hook not reached");
    }
    for point in [
        FaultPoint::DispatchClaim,
        FaultPoint::LaunchEffect,
        FaultPoint::MessageWrite,
        FaultPoint::ResultJournal,
    ] {
        let f = fixture(100).await;
        let a = Authority::default();
        let clock = DeterministicClock::new(T).unwrap();
        if matches!(point, FaultPoint::MessageWrite | FaultPoint::ResultJournal) {
            let co = open(&f, &a).await;
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
        }
        let before = AuthorityImage::read(&f.store, &f.content.key).await;
        let mut victim = Victim::start(
            &f,
            point,
            "resilience::crashes::sigkill_dispatch_and_result_gaps_reconcile_without_second_effect",
        );
        victim.ready();
        victim.kill();
        f.store.close().await;
        let restored = reopen(&f.path).await;
        let image = AuthorityImage::read(&restored, &f.content.key).await;
        image.preserves(&before);
        if matches!(point, FaultPoint::MessageWrite | FaultPoint::ResultJournal) {
            let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM worker_messages")
                .fetch_one(&restored.pool)
                .await
                .unwrap();
            assert_eq!(
                receipts,
                if point == FaultPoint::ResultJournal {
                    11
                } else {
                    10
                }
            );
            let co = SyntheticCoordinator::open(
                restored.clone(),
                &f.content.key.project_id,
                &a,
                Limits::default(),
            )
            .await
            .unwrap();
            clock.advance_to(T + 50).unwrap();
            assert_eq!(
                co.run(&a, &clock, RunLimits::default())
                    .await
                    .unwrap()
                    .status,
                RunStatus::Complete
            );
            assert_eq!(
                AuthorityImage::read(&restored, &f.content.key)
                    .await
                    .effects,
                3
            );
        } else {
            let r = restored
                .admit(&standalone(&f), &a, &clock, Limits::default())
                .await
                .unwrap();
            let d = OutboxDispatcher::new(restored.clone());
            let ticket = d
                .claim(&r.lease, &owner(), &a, &clock, Limits::default())
                .await
                .unwrap();
            for _ in 0..3 {
                assert_eq!(
                    d.dispatch(
                        &ticket,
                        &SyntheticProvider::new(restored.clone()),
                        &a,
                        &clock,
                        Limits::default()
                    )
                    .await
                    .unwrap(),
                    DispatchState::Active
                );
            }
            let after = AuthorityImage::read(&restored, &f.content.key).await;
            assert_eq!(after.attempts, 1);
            assert_eq!(after.effects, 1);
            assert_eq!(after.positions[4..8], [1, 2, 3, 1]);
        }
    }
}
#[tokio::test]
async fn sigkill_validation_and_group_commit_preserve_independent_evidence() {
    if let Some((path, point)) = child_case() {
        let store = PlanStore::open(path).await.unwrap();
        let a = Authority::default();
        let co = SyntheticCoordinator::open(
            store.clone(),
            &ProjectId::new("project").unwrap(),
            &a,
            Limits::default(),
        )
        .await
        .unwrap();
        let clock = DeterministicClock::new(T + 50).unwrap();
        arm(&store, point).await;
        if point == FaultPoint::ValidationCommit {
            let row =
                sqlx::query("SELECT attempt_id,epoch FROM task_attempts WHERE task_id='task-a'")
                    .fetch_one(&store.pool)
                    .await
                    .unwrap();
            co.validator()
                .validate_leaf(
                    &LeaseRef {
                        task: TaskRef {
                            plan: co.plan_key().clone(),
                            task_id: TaskId::new("task-a").unwrap(),
                        },
                        attempt_id: AttemptId::new(row.get::<String, _>(0)).unwrap(),
                        epoch: row.get::<i64, _>(1) as u64,
                    },
                    &a,
                    &clock,
                )
                .await
                .unwrap();
        } else {
            co.validator()
                .aggregate(
                    &TaskRef {
                        plan: co.plan_key().clone(),
                        task_id: TaskId::new("group-g").unwrap(),
                    },
                    &a,
                    &clock,
                )
                .await
                .unwrap();
            if point == FaultPoint::GroupAfterCommit {
                marker(point);
            }
        }
        panic!("acceptance commit hook not reached");
    }
    for point in [
        FaultPoint::ValidationCommit,
        FaultPoint::GroupCommit,
        FaultPoint::GroupAfterCommit,
    ] {
        let f = fixture(100).await;
        let a = Authority::default();
        let co = open(&f, &a).await;
        let clock = DeterministicClock::new(T).unwrap();
        phase(
            &co,
            &a,
            &clock,
            2,
            if point == FaultPoint::ValidationCommit {
                0
            } else {
                16
            },
            0,
        )
        .await;
        let before = AuthorityImage::read(&f.store, &f.content.key).await;
        let mut victim = Victim::start(
            &f,
            point,
            "resilience::crashes::sigkill_validation_and_group_commit_preserve_independent_evidence",
        );
        victim.ready();
        victim.kill();
        f.store.close().await;
        let restored = reopen(&f.path).await;
        let image = AuthorityImage::read(&restored, &f.content.key).await;
        image.preserves(&before);
        assert_eq!(
            image.accepted.len(),
            if point == FaultPoint::GroupAfterCommit {
                3
            } else {
                before.accepted.len()
            }
        );
        let co = SyntheticCoordinator::open(
            restored.clone(),
            &f.content.key.project_id,
            &a,
            Limits::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            co.run(&a, &clock, RunLimits::default())
                .await
                .unwrap()
                .status,
            RunStatus::Complete
        );
        co.validator().verify_all().await.unwrap();
        let final_image = AuthorityImage::read(&restored, &f.content.key).await;
        assert_eq!(final_image.effects, 3);
        assert_eq!(final_image.accepted.len(), 4);
    }
}
