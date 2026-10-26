mod common;
use common::*;
use rolvya_domain::{ids::*, plan::*, worker::*};
use rolvya_persistence_sqlite::{
    admission::*, finance::*, orchestration::*, worker_control::*, worker_simulation::*, *,
};
use rolvya_plan_validator::Limits;
#[path = "common/synthetic.rs"]
mod synthetic;
use synthetic::*;
#[tokio::test]
async fn independent_leaves_run_before_group_and_integration_acceptance() {
    let f = fixture(100).await;
    let authority = Authority::default();
    let clock = DeterministicClock::new(T).unwrap();
    let coordinator = SyntheticCoordinator::open(
        f.store.clone(),
        &f.content.key.project_id,
        &authority,
        Limits::default(),
    )
    .await
    .unwrap();
    let first = coordinator
        .pass(&authority, &clock, PassLimits::default())
        .await
        .unwrap();
    assert_eq!(first.admitted.len(), 2);
    let snapshot = coordinator.snapshot().await.unwrap();
    assert_eq!(snapshot.running, 2);
    assert_eq!(snapshot.accepted, 0);
    assert_eq!(snapshot.waiting_dependencies, 1);
    let completed = coordinator
        .run(&authority, &clock, RunLimits::default())
        .await
        .unwrap();
    assert_eq!(completed.status, RunStatus::Complete);
    assert_eq!(completed.snapshot.accepted, 4);
    let raw = f.raw().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_attempts")
            .fetch_one(&raw)
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM synthetic_operations")
            .fetch_one(&raw)
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_acceptances")
            .fetch_one(&raw)
            .await
            .unwrap(),
        4
    );
    assert_eq!(
        f.store
            .load_active(&f.content.key.project_id, &authority, Limits::default())
            .await
            .unwrap()
            .revision
            .fingerprint(),
        f.revision.fingerprint()
    );
}
#[tokio::test]
async fn received_results_do_not_complete_tasks_and_group_requires_separate_validation() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    phase(&co, &a, &clock, 2, 0, 0).await;
    let snapshot = co.snapshot().await.unwrap();
    assert_eq!(snapshot.received_unvalidated, 2);
    assert_eq!(snapshot.accepted, 0);
    assert_eq!(count(&f, "synthetic_validations").await, 0);
    let first = co
        .validator()
        .validate_leaf(&lease(&f, "task-a").await, &a, &clock)
        .await
        .unwrap();
    assert!(
        co.validator()
            .aggregate(&scope(&f, "group-g"), &a, &clock)
            .await
            .is_err()
    );
    let again = co
        .validator()
        .validate_leaf(&lease(&f, "task-a").await, &a, &clock)
        .await
        .unwrap();
    assert_eq!(first, again);
    co.validator()
        .validate_leaf(&lease(&f, "task-b").await, &a, &clock)
        .await
        .unwrap();
    assert_eq!(co.snapshot().await.unwrap().accepted, 2);
    assert_eq!(count(&f, "task_attempts").await, 2);
    // Neither child validation implicitly certifies its parent.
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM synthetic_validations WHERE task_id='group-g'"
        )
        .fetch_one(&f.raw().await)
        .await
        .unwrap(),
        0
    );
    let g = co
        .validator()
        .aggregate(&scope(&f, "group-g"), &a, &clock)
        .await
        .unwrap();
    assert_eq!(
        co.validator()
            .aggregate(&scope(&f, "group-g"), &a, &clock)
            .await
            .unwrap(),
        g
    );
    assert_eq!(count(&f, "task_acceptances").await, 3);
    assert_eq!(count(&f, "task_attempts").await, 2);
    co.pass(&a, &clock, PassLimits::default()).await.unwrap();
    assert_eq!(count(&f, "task_attempts").await, 3);
}
#[tokio::test]
async fn integration_admission_cannot_bypass_unvalidated_group() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    phase(&co, &a, &clock, 2, 16, 0).await;
    let task = f
        .content
        .tasks
        .iter()
        .find(|t| t.id.as_str() == "task-c")
        .unwrap();
    let assignment = a.assignment(task, 1, T + 50).unwrap();
    let request = AdmissionRequest {
        task: scope(&f, "task-c"),
        fingerprint: f.revision.fingerprint().into(),
        expected_epoch: 0,
        request_id: RequestId::new("premature-c").unwrap(),
        actor: owner(),
        permit: assignment.permit,
    };
    assert_eq!(
        f.store
            .admit(&request, &a, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::DependencyIncomplete
    );
    assert_eq!(count(&f, "task_acceptances").await, 2);
    assert_eq!(count(&f, "task_attempts").await, 2);
}
#[tokio::test]
async fn concurrent_leaf_validation_and_group_callbacks_commit_once() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    phase(&co, &a, &clock, 2, 0, 0).await;
    let (la, lb) = (lease(&f, "task-a").await, lease(&f, "task-b").await);
    let validator = co.validator();
    let (ra, rb) = tokio::join!(
        validator.validate_leaf(&la, &a, &clock),
        validator.validate_leaf(&lb, &a, &clock)
    );
    ra.unwrap();
    rb.unwrap();
    let g = scope(&f, "group-g");
    let (r1, r2) = tokio::join!(
        validator.aggregate(&g, &a, &clock),
        validator.aggregate(&g, &a, &clock)
    );
    assert_eq!(r1.unwrap(), r2.unwrap());
    assert_eq!(count(&f, "task_acceptances").await, 3);
}
#[tokio::test]
async fn permanent_failure_preserves_accepted_sibling_and_blocks_descendants() {
    let f = fixture(100).await;
    let mut a = Authority::default();
    a.behaviors.insert(
        TaskId::new("task-b").unwrap(),
        FakeBehavior::PermanentFailure,
    );
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
    assert_eq!(r.status, RunStatus::Blocked);
    assert_eq!(r.snapshot.accepted, 1);
    assert_eq!(count(&f, "task_attempts").await, 2);
    assert!(r.blocked.iter().any(|b| matches!(
        b.reason,
        OrchestrationError::ExecutionFailed(FailureClass::Permanent)
    )));
    assert!(
        co.validator()
            .aggregate(&scope(&f, "group-g"), &a, &clock)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn insufficient_budget_does_not_discard_accepted_a_or_raise_ceiling() {
    let f = fixture(7).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
    assert_eq!(r.status, RunStatus::Blocked);
    assert_eq!(r.snapshot.accepted, 1);
    assert_eq!(count(&f, "task_attempts").await, 1);
    let balance = f
        .store
        .financial_snapshot(&f.content.key.project_id)
        .await
        .unwrap();
    assert_eq!(balance.ceiling, money(7));
    assert_eq!(balance.held, costs());
    assert_eq!(count(&f, "budget_approvals").await, 1);
}
#[tokio::test]
async fn unsupported_group_criteria_cannot_be_inferred_from_child_success() {
    let f = fixture_with(100, |p| {
        p.tasks
            .iter_mut()
            .find(|t| t.id.as_str() == "group-g")
            .unwrap()
            .acceptance_criteria = vec!["missing-independent-evaluation".into()]
    })
    .await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
    assert_eq!(r.status, RunStatus::Blocked);
    assert_eq!(r.snapshot.accepted, 2);
    assert!(
        r.blocked
            .iter()
            .any(|b| b.reason == OrchestrationError::UnsupportedCriteria)
    );
    assert_eq!(count(&f, "task_attempts").await, 2);
}
#[tokio::test]
async fn forged_stale_malformed_or_reordered_output_never_validates_group() {
    for behavior in [
        FakeBehavior::WrongContext,
        FakeBehavior::CrossProject,
        FakeBehavior::CrossAttempt,
        FakeBehavior::StaleEpoch,
        FakeBehavior::Impersonate,
        FakeBehavior::Malformed,
        FakeBehavior::OutOfOrder,
        FakeBehavior::Cancelled,
        FakeBehavior::MissingCapabilityEvidence,
        FakeBehavior::InvalidCapabilityEvidence,
    ] {
        let f = fixture(100).await;
        let mut a = Authority::default();
        a.behaviors.insert(TaskId::new("task-b").unwrap(), behavior);
        let co = open(&f, &a).await;
        let clock = DeterministicClock::new(T).unwrap();
        let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
        assert_eq!(r.snapshot.accepted, 1, "{behavior:?}");
        assert_ne!(r.status, RunStatus::Complete, "{behavior:?}");
        assert_eq!(count(&f, "task_attempts").await, 2, "{behavior:?}");
    }
}
#[tokio::test]
async fn actual_artifact_bytes_and_integration_provenance_are_independently_verified() {
    for (task_id, behavior, expected) in [
        ("task-b", FakeBehavior::InvalidArtifact, 1),
        ("task-c", FakeBehavior::MissingInputs, 3),
    ] {
        let f = fixture(100).await;
        let mut a = Authority::default();
        a.behaviors.insert(TaskId::new(task_id).unwrap(), behavior);
        let co = open(&f, &a).await;
        let clock = DeterministicClock::new(T).unwrap();
        let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
        assert_eq!(r.status, RunStatus::Blocked);
        assert_eq!(r.snapshot.accepted, expected);
        // A valid-looking successful receipt/hash has been accepted as untrusted input.
        let l = lease(&f, task_id).await;
        assert!(matches!(
            rolvya_persistence_sqlite::worker_control::DurableWorkerController::new(
                f.store.clone()
            )
            .received_outcome(&l)
            .await
            .unwrap(),
            Some(ExecutionOutcome::Succeeded { .. })
        ));
        assert!(co.validator().validate_leaf(&l, &a, &clock).await.is_err());
    }
}
#[tokio::test]
async fn restart_checkpoints_recover_same_artifacts_without_duplicate_effective_work() {
    for mode in [
        "a-accepted",
        "running",
        "children-accepted",
        "group-accepted",
    ] {
        let f = fixture(100).await;
        let a = Authority::default();
        let co = open(&f, &a).await;
        let clock = DeterministicClock::new(T).unwrap();
        if mode == "running" {
            co.pass(&a, &clock, PassLimits::default()).await.unwrap();
        } else {
            phase(
                &co,
                &a,
                &clock,
                if mode == "a-accepted" { 1 } else { 2 },
                16,
                if mode == "group-accepted" { 16 } else { 0 },
            )
            .await;
        }
        let before = co.snapshot().await.unwrap();
        let financial = f
            .store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap();
        f.store.close().await;
        let restored = PlanStore::open(&f.path).await.unwrap();
        let co = SyntheticCoordinator::open(
            restored.clone(),
            &f.content.key.project_id,
            &a,
            Limits::default(),
        )
        .await
        .unwrap();
        assert_eq!(co.snapshot().await.unwrap(), before);
        assert_eq!(
            restored
                .financial_snapshot(&f.content.key.project_id)
                .await
                .unwrap(),
            financial
        );
        let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
        assert_eq!(r.status, RunStatus::Complete, "{mode}");
        assert_eq!(count(&f, "synthetic_operations").await, 3);
        assert_eq!(count(&f, "task_attempts").await, 3);
        assert_eq!(count(&f, "task_acceptances").await, 4);
    }
}
struct CrashAdapter {
    provider: rolvya_persistence_sqlite::worker_control::SyntheticProvider,
}
impl rolvya_persistence_sqlite::worker_control::LaunchAdapter for CrashAdapter {
    async fn inspect(
        &self,
        c: &rolvya_persistence_sqlite::worker_control::LaunchCommand,
    ) -> Result<rolvya_persistence_sqlite::worker_control::ProviderObservation, WorkerError> {
        self.provider.inspect(c).await
    }
    async fn launch(
        &self,
        a: &rolvya_persistence_sqlite::worker_control::LaunchAuthorization,
        clock: &impl Clock,
    ) -> Result<rolvya_persistence_sqlite::worker_control::LaunchResult, WorkerError> {
        let result = self.provider.launch(a, clock).await?;
        if a.command().binding().task.task_id.as_str() == "task-c" {
            std::process::exit(17);
        }
        Ok(result)
    }
}
#[tokio::test]
async fn subprocess_restart_at_each_orchestration_boundary_recovers_without_relaunch() {
    use rolvya_persistence_sqlite::worker_control::*;
    if let Ok(mode) = std::env::var("ROLVYA_R09_CRASH") {
        let store = PlanStore::open(std::env::var("ROLVYA_R09_DB").unwrap())
            .await
            .unwrap();
        let a = Authority::default();
        let co = SyntheticCoordinator::open(
            store.clone(),
            &ProjectId::new("project").unwrap(),
            &a,
            Limits::default(),
        )
        .await
        .unwrap();
        let clock = DeterministicClock::new(T).unwrap();
        if mode == "running" {
            co.pass(&a, &clock, PassLimits::default()).await.unwrap();
        } else {
            phase(
                &co,
                &a,
                &clock,
                if mode == "a-accepted" { 1 } else { 2 },
                16,
                if matches!(mode.as_str(), "group-accepted" | "c-lost-ack") {
                    16
                } else {
                    0
                },
            )
            .await;
            if mode == "c-lost-ack" {
                co.pass_with_adapter(
                    &a,
                    &CrashAdapter {
                        provider: SyntheticProvider::new(store),
                    },
                    &clock,
                    PassLimits::default(),
                )
                .await
                .unwrap();
                panic!("C launch fault not reached");
            }
        }
        std::process::exit(17);
    }
    for mode in [
        "a-accepted",
        "running",
        "children-accepted",
        "group-accepted",
        "c-lost-ack",
    ] {
        let f = fixture(100).await;
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "subprocess_restart_at_each_orchestration_boundary_recovers_without_relaunch",
                "--nocapture",
            ])
            .env("ROLVYA_R09_CRASH", mode)
            .env("ROLVYA_R09_DB", &f.path)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(17), "{mode}");
        let a = Authority::default();
        let store = PlanStore::open(&f.path).await.unwrap();
        let co = SyntheticCoordinator::open(
            store.clone(),
            &f.content.key.project_id,
            &a,
            Limits::default(),
        )
        .await
        .unwrap();
        let clock = DeterministicClock::new(if mode == "running" { T } else { T + 50 }).unwrap();
        let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
        assert_eq!(r.status, RunStatus::Complete, "{mode}");
        assert_eq!(count(&f, "synthetic_operations").await, 3, "{mode}");
        assert_eq!(count(&f, "task_attempts").await, 3, "{mode}");
        assert_eq!(count(&f, "task_acceptances").await, 4, "{mode}");
        store.check_integrity().await.unwrap();
    }
}
#[tokio::test]
async fn nested_groups_roll_up_bottom_up_without_parent_attempts_or_new_edges() {
    let f = fixture_with(100, |p| {
        let mut h = group("group-h", &p.key);
        h.acceptance_criteria = vec![GROUP_CRITERION.into()];
        p.tasks
            .iter_mut()
            .find(|t| t.id.as_str() == "group-g")
            .unwrap()
            .parent = Some(task_ref(&h));
        p.tasks
            .iter_mut()
            .find(|t| t.id.as_str() == "task-c")
            .unwrap()
            .dependencies = vec![task_ref(&h)];
        p.tasks.push(h);
    })
    .await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    phase(&co, &a, &clock, 2, 1, 16).await;
    assert_eq!(co.snapshot().await.unwrap().accepted, 1);
    assert!(
        co.validator()
            .aggregate(&scope(&f, "group-h"), &a, &clock)
            .await
            .is_err()
    );
    let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
    assert_eq!(r.status, RunStatus::Complete);
    assert_eq!(r.snapshot.accepted, 5);
    assert_eq!(count(&f, "task_attempts").await, 3);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_dependencies WHERE version=2")
            .fetch_one(&f.raw().await)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        f.store
            .load_active(&f.content.key.project_id, &a, Limits::default())
            .await
            .unwrap()
            .revision
            .fingerprint(),
        co.fingerprint()
    );
}
#[tokio::test]
async fn duplicate_callbacks_and_worker_events_after_restart_preserve_one_logical_success() {
    let f = fixture(100).await;
    let mut a = Authority::default();
    a.behaviors
        .insert(TaskId::new("task-a").unwrap(), FakeBehavior::Duplicate);
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    co.run(&a, &clock, RunLimits::default()).await.unwrap();
    let la = lease(&f, "task-a").await;
    let accepted = co.validator().validate_leaf(&la, &a, &clock).await.unwrap();
    f.store.close().await;
    let restored = PlanStore::open(&f.path).await.unwrap();
    let co = SyntheticCoordinator::open(restored, &f.content.key.project_id, &a, Limits::default())
        .await
        .unwrap();
    assert_eq!(
        co.validator().validate_leaf(&la, &a, &clock).await.unwrap(),
        accepted
    );
    assert_eq!(
        co.run(&a, &clock, RunLimits::default())
            .await
            .unwrap()
            .status,
        RunStatus::Complete
    );
    assert_eq!(count(&f, "task_acceptances").await, 4);
    assert_eq!(count(&f, "synthetic_operations").await, 3);
}
#[tokio::test]
async fn transient_retry_requires_separate_termination_billing_and_existing_backoff() {
    let f = fixture(100).await;
    let mut a = Authority::default();
    a.behaviors.insert(
        TaskId::new("task-b").unwrap(),
        FakeBehavior::TransientFailure,
    );
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    let blocked = co.run(&a, &clock, RunLimits::default()).await.unwrap();
    assert_eq!(blocked.status, RunStatus::Blocked);
    assert_eq!(blocked.snapshot.accepted, 1);
    let old_b = lease(&f, "task-b").await;
    let first_a = co
        .validator()
        .validate_leaf(&lease(&f, "task-a").await, &a, &clock)
        .await
        .unwrap();
    assert_eq!(
        f.store
            .financial_liability(
                &f.content.key.project_id,
                &LiabilityId::new(old_b.attempt_id.as_str()).unwrap()
            )
            .await
            .unwrap()
            .held,
        costs()
    );
    // Independent synthetic driver supplies provider termination and zero-charge
    // evidence. A failure callback alone did not release funds or authorize retry.
    SyntheticProvider::new(f.store.clone())
        .observe(
            &OperationKey {
                project: old_b.task.plan.project_id.clone(),
                attempt: old_b.attempt_id.clone(),
            },
            1,
            ProviderState::Finished,
            false,
            &e("synthetic-provider-terminated-b"),
        )
        .await
        .unwrap();
    synthetic_final(&f, &old_b).await;
    a.behaviors.remove(&TaskId::new("task-b").unwrap());
    let completed = co.run(&a, &clock, RunLimits::default()).await.unwrap();
    assert_eq!(completed.status, RunStatus::Complete);
    let new_b = lease(&f, "task-b").await;
    assert_ne!(old_b.attempt_id, new_b.attempt_id);
    assert!(new_b.epoch > old_b.epoch);
    assert_eq!(count(&f, "task_attempts").await, 4);
    assert_eq!(count(&f, "synthetic_operations").await, 4);
    assert_eq!(
        co.validator()
            .validate_leaf(&lease(&f, "task-a").await, &a, &clock)
            .await
            .unwrap(),
        first_a
    );
    assert_eq!(count(&f, "task_acceptances").await, 4);
}
#[tokio::test]
async fn unknown_exposure_or_revoked_admission_preserves_accepted_work() {
    for revoked in [false, true] {
        let f = fixture(100).await;
        let mut a = Authority::default();
        let co = open(&f, &a).await;
        let clock = DeterministicClock::new(T).unwrap();
        phase(&co, &a, &clock, 1, 16, 16).await;
        let l = lease(&f, "task-a").await;
        if revoked {
            a.revoked = true;
        } else {
            f.store
                .apply_financial(
                    &FinancialRequest {
                        project: f.content.key.project_id.clone(),
                        request_id: RequestId::new("unknown-exposure").unwrap(),
                        actor: owner(),
                        reason: "Synthetic provider exposure unavailable".into(),
                        command: FinancialCommand::Report {
                            liability: LiabilityId::new(l.attempt_id.as_str()).unwrap(),
                            expected_revision: 1,
                            unreserved: zero(),
                            uncertain: true,
                            evidence: e("synthetic-unknown-exposure"),
                        },
                    },
                    &Finance,
                )
                .await
                .unwrap();
        }
        let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
        assert_eq!(r.status, RunStatus::Blocked);
        assert_eq!(r.snapshot.accepted, 1);
        assert_eq!(count(&f, "task_attempts").await, 1);
        assert_eq!(count(&f, "task_acceptances").await, 1);
        assert_eq!(
            f.store
                .financial_snapshot(&f.content.key.project_id)
                .await
                .unwrap()
                .ceiling,
            money(100)
        );
    }
}
#[tokio::test]
async fn unknown_synthetic_operation_blocks_without_blind_launch_or_reservation_release() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    let adapter = SyntheticProvider::new(f.store.clone())
        .with_behavior(SyntheticLaunchBehavior::UnknownBeforeEffect);
    co.pass_with_adapter(&a, &adapter, &clock, PassLimits::default())
        .await
        .unwrap();
    let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
    assert_eq!(r.status, RunStatus::Blocked);
    assert_eq!(r.snapshot.accepted, 0);
    assert_eq!(count(&f, "synthetic_operations").await, 0);
    assert_eq!(count(&f, "task_attempts").await, 2);
    assert_eq!(
        f.store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap()
            .held
            .total()
            .unwrap(),
        money(14)
    );
}
#[tokio::test]
async fn expired_execution_cannot_submit_success_after_restart() {
    let f = fixture(100).await;
    let mut a = Authority::default();
    a.behaviors.insert(
        TaskId::new("task-b").unwrap(),
        FakeBehavior::Delayed { seconds: 100 },
    );
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
    assert_eq!(r.status, RunStatus::Blocked);
    assert_eq!(r.snapshot.accepted, 1);
    clock.advance_to(T + 150).unwrap();
    let later = co.pass(&a, &clock, PassLimits::default()).await.unwrap();
    assert_eq!(later.snapshot.accepted, 1);
    assert_eq!(count(&f, "task_attempts").await, 2);
    assert!(
        co.validator()
            .validate_leaf(&lease(&f, "task-b").await, &a, &clock)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn immutable_plan_nodes_edges_and_financial_positions_survive_end_to_end_execution() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    let fingerprint = f.revision.fingerprint().to_owned();
    let nodes = count(&f, "tasks").await;
    let edges = count(&f, "task_dependencies").await;
    let first = co.pass(&a, &clock, PassLimits::default()).await.unwrap();
    assert_eq!(first.admitted.len(), 2);
    let raw = f.raw().await;
    let scopes:Vec<String>=sqlx::query_scalar("SELECT task_id FROM task_attempts a JOIN attempt_states s USING(project_id,attempt_id) WHERE s.state='admitted' ORDER BY task_id").fetch_all(&raw).await.unwrap();
    assert_eq!(scopes, vec!["task-a", "task-b"]);
    co.run(&a, &clock, RunLimits::default()).await.unwrap();
    assert_eq!(count(&f, "tasks").await, nodes);
    assert_eq!(count(&f, "task_dependencies").await, edges);
    assert_eq!(
        f.store
            .load_active(&f.content.key.project_id, &a, Limits::default())
            .await
            .unwrap()
            .revision
            .fingerprint(),
        fingerprint
    );
    let reserved:i64=sqlx::query_scalar("SELECT count(*) FROM task_attempts a JOIN financial_events e ON e.sequence=a.financial_event AND e.project_id=a.project_id JOIN liability_revisions l ON l.event_sequence=e.sequence AND l.liability_id=a.attempt_id WHERE e.kind='reserve'").fetch_one(&raw).await.unwrap();
    assert_eq!(reserved, 3);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM financial_positions")
            .fetch_one(&raw)
            .await
            .unwrap(),
        12
    );
    let budget = f
        .store
        .financial_snapshot(&f.content.key.project_id)
        .await
        .unwrap();
    assert_eq!(budget.ceiling, money(100));
    assert_eq!(budget.held.total().unwrap(), money(21));
    assert_eq!(budget.settled, zero());
}
#[tokio::test]
async fn job_transaction_faults_recover_existing_dispatch_without_partial_scripts() {
    for table in [
        "synthetic_jobs",
        "synthetic_artifacts",
        "synthetic_worker_events",
    ] {
        for mode in ["ABORT", "IGNORE"] {
            let f = fixture(100).await;
            let a = Authority::default();
            let co = open(&f, &a).await;
            let clock = DeterministicClock::new(T).unwrap();
            let raw = f.raw().await;
            let raise = if mode == "ABORT" {
                "RAISE(ABORT,'injected')"
            } else {
                "RAISE(IGNORE)"
            };
            sqlx::query(&format!(
                "CREATE TRIGGER injected BEFORE INSERT ON {table} BEGIN SELECT {raise}; END"
            ))
            .execute(&raw)
            .await
            .unwrap();
            let r = co.pass(&a, &clock, PassLimits::default()).await.unwrap();
            assert!(!r.blocked.is_empty());
            for name in [
                "synthetic_jobs",
                "synthetic_artifacts",
                "synthetic_worker_events",
            ] {
                assert_eq!(count(&f, name).await, 0, "{table}/{mode}/{name}");
            }
            assert_eq!(count(&f, "synthetic_operations").await, 2);
            assert_eq!(count(&f, "task_acceptances").await, 0);
            sqlx::query("DROP TRIGGER injected")
                .execute(&raw)
                .await
                .unwrap();
            assert_eq!(
                co.run(&a, &clock, RunLimits::default())
                    .await
                    .unwrap()
                    .status,
                RunStatus::Complete
            );
            assert_eq!(count(&f, "synthetic_operations").await, 3);
        }
    }
}
#[tokio::test]
async fn validation_storage_fault_never_marks_task_success() {
    for mode in ["ABORT", "IGNORE"] {
        let f = fixture(100).await;
        let a = Authority::default();
        let co = open(&f, &a).await;
        let clock = DeterministicClock::new(T).unwrap();
        phase(&co, &a, &clock, 2, 0, 0).await;
        let raw = f.raw().await;
        let raise = if mode == "ABORT" {
            "RAISE(ABORT,'injected')"
        } else {
            "RAISE(IGNORE)"
        };
        sqlx::query(&format!("CREATE TRIGGER injected BEFORE INSERT ON synthetic_validations BEGIN SELECT {raise}; END")).execute(&raw).await.unwrap();
        assert!(
            co.validator()
                .validate_leaf(&lease(&f, "task-a").await, &a, &clock)
                .await
                .is_err()
        );
        assert_eq!(co.snapshot().await.unwrap().accepted, 0);
        assert_eq!(count(&f, "synthetic_validations").await, 0);
        sqlx::query("DROP TRIGGER injected")
            .execute(&raw)
            .await
            .unwrap();
        assert_eq!(
            co.run(&a, &clock, RunLimits::default())
                .await
                .unwrap()
                .status,
            RunStatus::Complete
        );
    }
}
#[tokio::test]
async fn accepted_artifact_and_manifest_are_immutable_and_recheckable_after_restart() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    co.run(&a, &clock, RunLimits::default()).await.unwrap();
    let c = scope(&f, "task-c");
    let sha = co.validator().verify_accepted(&c).await.unwrap();
    let raw = f.raw().await;
    for sql in [
        "UPDATE synthetic_artifacts SET content=X'00'",
        "DELETE FROM synthetic_artifacts",
        "UPDATE synthetic_validations SET sha256=printf('%064d',0)",
        "DELETE FROM synthetic_validations",
        "DELETE FROM synthetic_jobs",
    ] {
        assert!(sqlx::query(sql).execute(&raw).await.is_err(), "{sql}");
    }
    raw.close().await;
    f.store.close().await;
    let restored = PlanStore::open(&f.path).await.unwrap();
    let co = SyntheticCoordinator::open(restored, &f.content.key.project_id, &a, Limits::default())
        .await
        .unwrap();
    assert_eq!(co.validator().verify_accepted(&c).await.unwrap(), sha);
    co.validator().verify_all().await.unwrap();
}
#[tokio::test]
async fn finite_work_limits_pause_instead_of_busy_polling() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    assert!(
        co.run(
            &a,
            &clock,
            RunLimits {
                passes: 0,
                ..RunLimits::default()
            }
        )
        .await
        .is_err()
    );
    assert!(
        co.pass(
            &a,
            &clock,
            PassLimits {
                events: 257,
                ..PassLimits::default()
            }
        )
        .await
        .is_err()
    );
    let r = co
        .run(
            &a,
            &clock,
            RunLimits {
                passes: 1,
                ..RunLimits::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(r.status, RunStatus::Paused);
    assert_eq!(r.passes, 1);
    assert_eq!(r.snapshot.running, 2);
    assert_eq!(
        co.run(&a, &clock, RunLimits::default())
            .await
            .unwrap()
            .status,
        RunStatus::Complete
    );
}
#[tokio::test]
async fn completed_execution_with_unknown_finance_is_blocked_without_erasing_artifacts() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    co.run(&a, &clock, RunLimits::default()).await.unwrap();
    let la = lease(&f, "task-a").await;
    f.store
        .apply_financial(
            &FinancialRequest {
                project: f.content.key.project_id.clone(),
                request_id: RequestId::new("late-unknown").unwrap(),
                actor: owner(),
                reason: "Synthetic late exposure remains unresolved".into(),
                command: FinancialCommand::Report {
                    liability: LiabilityId::new(la.attempt_id.as_str()).unwrap(),
                    expected_revision: 1,
                    unreserved: zero(),
                    uncertain: true,
                    evidence: e("synthetic-late-unknown"),
                },
            },
            &Finance,
        )
        .await
        .unwrap();
    let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
    assert_eq!(r.status, RunStatus::Blocked);
    assert_eq!(r.snapshot.accepted, 4);
    assert_eq!(r.snapshot.uncertain_liabilities, 1);
    co.validator().verify_all().await.unwrap();
}
#[tokio::test]
async fn forged_artifact_hash_cannot_substitute_for_hashing_actual_bytes() {
    let f = fixture(100).await;
    let mut a = Authority::default();
    a.behaviors.insert(
        TaskId::new("task-b").unwrap(),
        FakeBehavior::ForgedArtifactDigest,
    );
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
    assert_eq!(r.status, RunStatus::Blocked);
    assert_eq!(r.snapshot.accepted, 1);
    assert_eq!(
        co.validator()
            .validate_leaf(&lease(&f, "task-b").await, &a, &clock)
            .await
            .unwrap_err(),
        OrchestrationError::InvalidArtifact
    );
}
#[tokio::test]
async fn retry_cap_survives_orchestration_restart_and_does_not_add_a_fourth_attempt() {
    let f = fixture(100).await;
    let mut a = Authority::default();
    a.behaviors.insert(
        TaskId::new("task-b").unwrap(),
        FakeBehavior::TransientFailure,
    );
    let clock = DeterministicClock::new(T).unwrap();
    let mut co = open(&f, &a).await;
    for index in 1..=3 {
        assert_eq!(
            co.run(&a, &clock, RunLimits::default())
                .await
                .unwrap()
                .status,
            RunStatus::Blocked
        );
        let b = lease(&f, "task-b").await;
        SyntheticProvider::new(f.store.clone())
            .observe(
                &OperationKey {
                    project: b.task.plan.project_id.clone(),
                    attempt: b.attempt_id.clone(),
                },
                1,
                ProviderState::Finished,
                false,
                &e(&format!("termination-{index}")),
            )
            .await
            .unwrap();
        synthetic_final(&f, &b).await;
        co = SyntheticCoordinator::open(
            PlanStore::open(&f.path).await.unwrap(),
            &f.content.key.project_id,
            &a,
            Limits::default(),
        )
        .await
        .unwrap();
    }
    let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
    assert_eq!(r.status, RunStatus::Blocked);
    assert_eq!(r.snapshot.accepted, 1);
    assert_eq!(count(&f, "task_attempts").await, 4);
    assert_eq!(count(&f, "synthetic_operations").await, 4);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_attempts WHERE task_id='task-b'")
            .fetch_one(&f.raw().await)
            .await
            .unwrap(),
        3
    );
}
#[tokio::test]
async fn foreign_validation_scope_and_parent_execution_are_rejected() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    phase(&co, &a, &clock, 2, 0, 0).await;
    let mut l = lease(&f, "task-a").await;
    l.task.plan.project_id = ProjectId::new("other-project").unwrap();
    assert_eq!(
        co.validator()
            .validate_leaf(&l, &a, &clock)
            .await
            .unwrap_err(),
        OrchestrationError::ScopeMismatch
    );
    let g = scope(&f, "group-g");
    let request = AdmissionRequest {
        task: g.clone(),
        fingerprint: f.revision.fingerprint().into(),
        expected_epoch: 0,
        request_id: RequestId::new("illegal-group").unwrap(),
        actor: owner(),
        permit: a
            .assignment(
                f.content
                    .tasks
                    .iter()
                    .find(|t| t.id.as_str() == "task-a")
                    .unwrap(),
                1,
                T,
            )
            .unwrap()
            .permit,
    };
    assert_eq!(
        f.store
            .admit(&request, &a, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::NotEligible
    );
    assert_eq!(count(&f, "task_attempts").await, 2);
    assert_eq!(count(&f, "task_acceptances").await, 0);
}
#[tokio::test]
async fn unsupported_execution_scope_or_criteria_cannot_launch_synthetic_work() {
    for remote in [false, true] {
        let f = fixture_with(100, |p| {
            let task = p
                .tasks
                .iter_mut()
                .find(|t| t.id.as_str() == "task-a")
                .unwrap();
            if remote {
                task.execution.as_mut().unwrap().write_scope = WriteScope::Remote {
                    repository: "synthetic/project".into(),
                    git_ref: "refs/heads/synthetic".into(),
                    paths: vec!["src/a".into()],
                };
            } else {
                task.acceptance_criteria = vec!["unknown-predicate".into()];
            }
        })
        .await;
        let a = Authority::default();
        let co = open(&f, &a).await;
        let clock = DeterministicClock::new(T).unwrap();
        let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
        assert_eq!(r.status, RunStatus::Blocked);
        assert_eq!(r.snapshot.accepted, 1);
        assert_eq!(count(&f, "task_attempts").await, 1);
        assert_eq!(count(&f, "synthetic_operations").await, 1);
    }
}
#[tokio::test]
async fn bounded_sixteen_leaf_workload_records_real_evidence_and_timing() {
    let f = fixture_with(1000, |p| {
        let group = p
            .tasks
            .iter()
            .find(|t| t.id.as_str() == "group-g")
            .unwrap()
            .clone();
        let c = p
            .tasks
            .iter()
            .find(|t| t.id.as_str() == "task-c")
            .unwrap()
            .clone();
        p.tasks = (0..16)
            .map(|i| {
                let mut t = task(&format!("leaf-{i:02}"), &p.key);
                t.parent = Some(task_ref(&group));
                t.acceptance_criteria = vec![ARTIFACT_CRITERION.into()];
                t
            })
            .collect();
        p.tasks.extend([group, c]);
    })
    .await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    let started = std::time::Instant::now();
    let first = co.pass(&a, &clock, PassLimits::default()).await.unwrap();
    assert_eq!(first.snapshot.running, 16);
    let report = co.run(&a, &clock, RunLimits::default()).await.unwrap();
    let elapsed = started.elapsed();
    assert_eq!(report.status, RunStatus::Complete);
    assert_eq!(report.snapshot.accepted, 18);
    assert_eq!(count(&f, "task_attempts").await, 17);
    assert_eq!(count(&f, "worker_messages").await, 102);
    assert_eq!(count(&f, "synthetic_operations").await, 17);
    let bytes: i64 = sqlx::query_scalar("SELECT sum(length(content)) FROM synthetic_artifacts")
        .fetch_one(&f.raw().await)
        .await
        .unwrap();
    println!(
        "R09 synthetic debug observation: 18 tasks, 17 workers (16 simultaneously active), 102 messages, {bytes} artifact bytes, {} passes, {elapsed:?}",
        report.passes + 1
    );
}
#[tokio::test]
async fn independent_coordinators_do_not_duplicate_admission_events_or_acceptances() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co1 = open(&f, &a).await;
    let co2 = SyntheticCoordinator::open(
        PlanStore::open(&f.path).await.unwrap(),
        &f.content.key.project_id,
        &a,
        Limits::default(),
    )
    .await
    .unwrap();
    let clock = DeterministicClock::new(T).unwrap();
    let (a1, a2) = tokio::join!(
        co1.pass(&a, &clock, PassLimits::default()),
        co2.pass(&a, &clock, PassLimits::default())
    );
    a1.unwrap();
    a2.unwrap();
    assert_eq!(count(&f, "task_attempts").await, 2);
    assert_eq!(count(&f, "synthetic_operations").await, 2);
    for offset in [1, 2, 30, 41, 42, 50] {
        clock.advance_to(T + offset).unwrap();
        let limits = PassLimits {
            admissions: 0,
            ..PassLimits::default()
        };
        let (r1, r2) = tokio::join!(co1.pass(&a, &clock, limits), co2.pass(&a, &clock, limits));
        r1.unwrap();
        r2.unwrap();
    }
    assert_eq!(count(&f, "task_acceptances").await, 3);
    assert_eq!(count(&f, "worker_messages").await, 12);
    assert_eq!(
        co1.run(&a, &clock, RunLimits::default())
            .await
            .unwrap()
            .status,
        RunStatus::Complete
    );
    assert_eq!(count(&f, "task_attempts").await, 3);
    assert_eq!(count(&f, "task_acceptances").await, 4);
}
#[tokio::test]
async fn unrecognized_legacy_attempt_is_not_reconstructed_as_new_synthetic_work() {
    let f = fixture(100).await;
    let a = Authority::default();
    let clock = DeterministicClock::new(T).unwrap();
    let task = f
        .content
        .tasks
        .iter()
        .find(|t| t.id.as_str() == "task-a")
        .unwrap();
    let receipt = f
        .store
        .admit(
            &AdmissionRequest {
                task: scope(&f, "task-a"),
                fingerprint: f.revision.fingerprint().into(),
                expected_epoch: 0,
                request_id: RequestId::new("legacy-r08-attempt").unwrap(),
                actor: owner(),
                permit: a.assignment(task, 1, T).unwrap().permit,
            },
            &a,
            &clock,
            Limits::default(),
        )
        .await
        .unwrap();
    let d = OutboxDispatcher::new(f.store.clone());
    let ticket = d
        .claim(&receipt.lease, &owner(), &a, &clock, Limits::default())
        .await
        .unwrap();
    d.dispatch(
        &ticket,
        &SyntheticProvider::new(f.store.clone()),
        &a,
        &clock,
        Limits::default(),
    )
    .await
    .unwrap();
    let co = open(&f, &a).await;
    let r = co
        .pass(
            &a,
            &clock,
            PassLimits {
                admissions: 0,
                ..PassLimits::default()
            },
        )
        .await
        .unwrap();
    assert!(
        r.blocked
            .iter()
            .any(|b| b.reason == OrchestrationError::Conflict)
    );
    assert_eq!(count(&f, "synthetic_jobs").await, 0);
    assert_eq!(count(&f, "synthetic_operations").await, 1);
}
#[tokio::test]
async fn real_v4_worker_data_survives_v5_upgrade_and_failed_v5_rolls_back() {
    for failing in [false, true] {
        let f = fixture(100).await;
        let a = Authority::default();
        let co = open(&f, &a).await;
        let clock = DeterministicClock::new(T).unwrap();
        for (index, at) in [0, 1, 2, 30, 42].into_iter().enumerate() {
            clock.advance_to(T + at).unwrap();
            co.pass(
                &a,
                &clock,
                PassLimits {
                    admissions: if index == 0 { 2 } else { 0 },
                    ..PassLimits::default()
                },
            )
            .await
            .unwrap();
        }
        let before = f
            .store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap();
        let messages = count(&f, "worker_messages").await;
        assert_eq!(messages, 10);
        f.store.close().await;
        let raw = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&f.path)
                .foreign_keys(false),
        )
        .await
        .unwrap();
        for table in [
            "synthetic_worker_events",
            "synthetic_artifacts",
            "synthetic_jobs",
            "synthetic_validations",
        ] {
            sqlx::query(&format!("DROP TABLE {table}"))
                .execute(&raw)
                .await
                .unwrap();
        }
        for index in ["orchestration_runtime_state", "orchestration_task_kind"] {
            sqlx::query(&format!("DROP INDEX {index}"))
                .execute(&raw)
                .await
                .unwrap();
        }
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version=5")
            .execute(&raw)
            .await
            .unwrap();
        if failing {
            let scripts = tempfile::tempdir().unwrap();
            for (name, body) in [
                (
                    "0001_plan_store.sql",
                    include_str!("../migrations/0001_plan_store.sql"),
                ),
                (
                    "0002_financial_ledger.sql",
                    include_str!("../migrations/0002_financial_ledger.sql"),
                ),
                (
                    "0003_scheduler_admission.sql",
                    include_str!("../migrations/0003_scheduler_admission.sql"),
                ),
                (
                    "0004_durable_worker_control.sql",
                    include_str!("../migrations/0004_durable_worker_control.sql"),
                ),
            ] {
                std::fs::write(scripts.path().join(name), body).unwrap();
            }
            std::fs::write(
                scripts.path().join("0005_synthetic_orchestration.sql"),
                format!(
                    "{}\nSELECT missing FROM absent;",
                    include_str!("../migrations/0005_synthetic_orchestration.sql")
                ),
            )
            .unwrap();
            assert!(
                sqlx::migrate::Migrator::new(scripts.path())
                    .await
                    .unwrap()
                    .run(&raw)
                    .await
                    .is_err()
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM sqlite_schema WHERE name='synthetic_jobs'"
                )
                .fetch_one(&raw)
                .await
                .unwrap(),
                0
            );
        }
        raw.close().await;
        let restored = PlanStore::open(&f.path).await.unwrap();
        restored.check_integrity().await.unwrap();
        assert_eq!(
            restored
                .financial_snapshot(&f.content.key.project_id)
                .await
                .unwrap(),
            before
        );
        assert_eq!(count(&f, "worker_messages").await, messages);
        assert_eq!(count(&f, "synthetic_jobs").await, 0);
        assert_eq!(count(&f, "synthetic_operations").await, 2);
    }
}
#[tokio::test]
async fn redacted_observability_is_bounded_and_distinguishes_evidence_from_success() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    let before = co.tasks(None, 100).await.unwrap();
    assert_eq!(
        before
            .iter()
            .filter(|t| t.phase == TaskPhase::Ready)
            .count(),
        2
    );
    assert!(
        before
            .iter()
            .any(|t| t.task.task_id.as_str() == "task-c"
                && t.phase == TaskPhase::WaitingDependencies)
    );
    let first = co.tasks(None, 2).await.unwrap();
    let second = co.tasks(Some(&first[1].task.task_id), 2).await.unwrap();
    assert_eq!(first.len() + second.len(), 4);
    assert!(co.tasks(None, 101).await.is_err());
    phase(&co, &a, &clock, 2, 0, 0).await;
    let received = co.tasks(None, 100).await.unwrap();
    assert_eq!(
        received
            .iter()
            .filter(|t| t.phase == TaskPhase::ReceivedUnvalidated)
            .count(),
        2
    );
    assert!(received.iter().all(|t| t.manifest_sha256.is_none()));
    co.run(&a, &clock, RunLimits::default()).await.unwrap();
    let done = co.tasks(None, 100).await.unwrap();
    assert!(
        done.iter()
            .all(|t| t.phase == TaskPhase::Validated && t.manifest_sha256.is_some())
    );
    assert!(
        done.iter()
            .filter(|t| t.kind == TaskKind::Group)
            .all(|t| t.lease.is_none())
    );
}
#[tokio::test]
async fn declared_artifact_expectation_mismatch_blocks_integration_before_admission() {
    let f = fixture_with(100, |p| {
        let group_ref = task_ref(p.tasks.iter().find(|t| t.id.as_str() == "group-g").unwrap());
        p.tasks
            .iter_mut()
            .find(|t| t.id.as_str() == "task-c")
            .unwrap()
            .upstream_artifacts
            .push(ArtifactExpectation {
                predecessor: group_ref,
                name: "aggregate.json".into(),
                sha256: "0".repeat(64),
            });
    })
    .await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
    assert_eq!(r.status, RunStatus::Blocked);
    assert_eq!(r.snapshot.accepted, 3);
    assert_eq!(count(&f, "task_attempts").await, 2);
    assert!(
        r.blocked
            .iter()
            .any(|b| b.reason == OrchestrationError::InvalidInputs)
    );
}
#[tokio::test]
async fn worker_transient_label_without_independent_classification_cannot_authorize_retry() {
    let f = fixture(100).await;
    let a = Authority::default();
    let co = open(&f, &a).await;
    let clock = DeterministicClock::new(T).unwrap();
    for (index, offset) in [0, 1, 2, 30, 42].into_iter().enumerate() {
        clock.advance_to(T + offset).unwrap();
        co.pass(
            &a,
            &clock,
            PassLimits {
                admissions: if index == 0 { 2 } else { 0 },
                validations: 0,
                groups: 0,
                ..PassLimits::default()
            },
        )
        .await
        .unwrap();
    }
    let b = lease(&f, "task-b").await;
    let controller = DurableWorkerController::new(f.store.clone());
    let binding = controller.binding(&b).await.unwrap();
    let forged = WorkerMessage {
        protocol_version: PROTOCOL_VERSION,
        message_id: MessageId::new("self-classified-transient").unwrap(),
        binding: binding.clone(),
        sequence: 6,
        body: WorkerBody::SubmitResult {
            outcome: ExecutionOutcome::Failed {
                class: FailureClass::Transient,
            },
        },
    };
    controller
        .receive(
            &a.peer(&binding),
            &encode_worker_message(&forged).unwrap(),
            &a,
            &a,
            &clock,
            Limits::default(),
        )
        .await
        .unwrap();
    co.pass(
        &a,
        &clock,
        PassLimits {
            admissions: 0,
            events: 0,
            validations: 0,
            groups: 0,
        },
    )
    .await
    .unwrap();
    SyntheticProvider::new(f.store.clone())
        .observe(
            &OperationKey {
                project: b.task.plan.project_id.clone(),
                attempt: b.attempt_id.clone(),
            },
            1,
            ProviderState::Finished,
            false,
            &e("independent-termination"),
        )
        .await
        .unwrap();
    synthetic_final(&f, &b).await;
    let r = co.run(&a, &clock, RunLimits::default()).await.unwrap();
    assert_eq!(r.status, RunStatus::Blocked);
    assert_eq!(r.snapshot.accepted, 1);
    assert_eq!(count(&f, "task_attempts").await, 2);
}
