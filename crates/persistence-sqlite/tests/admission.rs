mod common;
use common::*;
#[tokio::test]
async fn empty_outbox_has_no_launch_authority() {
    let f = Fixture::new().await;
    assert!(
        f.store
            .pending_dispatches(&f.content.key.project_id, 0, 10)
            .await
            .unwrap()
            .is_empty()
    );
}
use rolvya_domain::{ids::*, money::*, plan::*};
use rolvya_persistence_sqlite::{admission::*, finance::*, *};
use rolvya_plan_validator::{AssignmentDenial, AssignmentPolicy, Limits};
use std::sync::atomic::{AtomicI64, Ordering};
const TEST_TIME: i64 = 1_800_000_000;
struct TestClock(AtomicI64);
impl TestClock {
    fn new() -> Self {
        Self(AtomicI64::new(TEST_TIME))
    }
    fn set(&self, t: i64) {
        self.0.store(t, Ordering::SeqCst)
    }
}
impl Clock for TestClock {
    fn now(&self) -> Result<i64, AdmissionError> {
        Ok(self.0.load(Ordering::SeqCst))
    }
}
fn money(n: i64) -> Money {
    Money::new(n, Currency::new("USD").unwrap()).unwrap()
}
fn costs(a: [i64; 4]) -> CostBreakdown {
    CostBreakdown {
        ai_api: money(a[0]),
        compute: money(a[1]),
        storage: money(a[2]),
        transfer: money(a[3]),
    }
}
fn e(s: &str) -> EvidenceRef {
    EvidenceRef::new(s).unwrap()
}
struct Synthetic;
impl AssignmentPolicy for Synthetic {
    fn check(&self, a: &ExecutionAssignment) -> Result<(), AssignmentDenial> {
        SyntheticPolicy.check(a)
    }
}
impl SchedulerPolicy for Synthetic {
    fn permits(&self, o: &PrincipalId, action: SchedulerAction<'_>, _: i64) -> bool {
        match action {
            SchedulerAction::Admit { request: r, .. } => {
                r.actor == *o
                    && r.permit.provider.as_str() == "synthetic"
                    && r.permit.account.as_str() == "synthetic-account"
                    && r.permit.pool.as_str() == "synthetic-pool"
                    && r.permit.executor.as_str() == "synthetic-worker"
                    && r.permit.id.as_str() == format!("cap-{}", r.request_id.as_str())
                    && r.permit.valid_until == TEST_TIME + 3600
            }
            SchedulerAction::Dispatch { request: r, .. } => r.actor == *o,
            SchedulerAction::Validate { request: r, .. } => {
                r.actor.as_str() == "independent-validator"
                    && r.evidence.as_str().starts_with("synthetic-validation")
            }
            SchedulerAction::Fence(r) => r.actor == *o,
            SchedulerAction::Recover(r) => {
                r.actor == *o && r.evidence.as_str().starts_with("synthetic-reconciled")
            }
        }
    }
}
struct FinancialFixture;
impl FinancialAuthorization for FinancialFixture {
    fn permits(&self, o: &PrincipalId, r: &FinancialRequest, _: i64) -> bool {
        r.actor == *o
    }
}
async fn fund(f: &Fixture, amount: i64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    f.store
        .apply_financial(
            &FinancialRequest {
                project: f.content.key.project_id.clone(),
                request_id: RequestId::new("budget").unwrap(),
                actor: owner(),
                reason: "Synthetic no-cost test budget".into(),
                command: FinancialCommand::Approve {
                    expected_version: 0,
                    new_ceiling: money(amount),
                    proof: OwnerProof {
                        reference: e("synthetic-owner-proof"),
                        verified_at: now,
                    },
                },
            },
            &FinancialFixture,
        )
        .await
        .unwrap();
}
async fn ready(independent: bool, amount: i64) -> Fixture {
    let mut f = Fixture::new().await;
    if independent {
        f.content.key.version = PlanVersion::new(2).unwrap();
        for task in &mut f.content.tasks {
            task.plan = f.content.key.clone();
            if let Some(p) = &mut task.parent {
                p.plan = f.content.key.clone()
            }
            task.dependencies.clear();
        }
        let valid =
            rolvya_plan_validator::validate(&f.content, &Synthetic, Limits::default()).unwrap();
        f.revision = f
            .store
            .save_draft(&valid, Some(&f.revision), &owner(), &OwnerPolicy)
            .await
            .unwrap();
    }
    f.activate().await.unwrap();
    fund(&f, amount).await;
    f
}
fn build(
    key: &PlanKey,
    hash: &str,
    task: &str,
    request: &str,
    maximum: [i64; 4],
) -> AdmissionRequest {
    AdmissionRequest {
        task: TaskRef {
            plan: key.clone(),
            task_id: TaskId::new(task).unwrap(),
        },
        fingerprint: hash.into(),
        expected_epoch: 0,
        request_id: RequestId::new(request).unwrap(),
        actor: owner(),
        permit: ExecutionPermit {
            id: e(&format!("cap-{request}")),
            provider: e("synthetic"),
            account: e("synthetic-account"),
            pool: e("synthetic-pool"),
            executor: PrincipalId::new("synthetic-worker").unwrap(),
            maximum: costs(maximum),
            valid_until: TEST_TIME + 3600,
        },
    }
}
fn r(f: &Fixture, task: &str, key: &str, cost: [i64; 4]) -> AdmissionRequest {
    build(&f.content.key, f.revision.fingerprint(), task, key, cost)
}
fn dispatch(lease: &LeaseRef, key: &str) -> DispatchRequest {
    DispatchRequest {
        lease: lease.clone(),
        request_id: RequestId::new(key).unwrap(),
        actor: owner(),
    }
}
fn validation(f: &Fixture, task: &str, lease: Option<&LeaseRef>, key: &str) -> ValidationRequest {
    ValidationRequest {
        task: TaskRef {
            plan: f.content.key.clone(),
            task_id: TaskId::new(task).unwrap(),
        },
        fingerprint: f.revision.fingerprint().into(),
        lease: lease.cloned(),
        request_id: RequestId::new(key).unwrap(),
        actor: PrincipalId::new("independent-validator").unwrap(),
        evidence: e("synthetic-validation"),
        manifest_sha256: "b".repeat(64),
    }
}
fn fencing(lease: &LeaseRef, key: &str) -> FenceRequest {
    FenceRequest {
        lease: lease.clone(),
        request_id: RequestId::new(key).unwrap(),
        actor: owner(),
        evidence: e("synthetic-stop-request"),
    }
}
async fn n(f: &Fixture, table: &str) -> i64 {
    let raw = f.raw().await;
    let value = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&raw)
        .await
        .unwrap();
    raw.close().await;
    value
}
async fn empty_admission(f: &Fixture) {
    for table in [
        "task_attempts",
        "attempt_states",
        "task_fences",
        "scheduler_events",
        "dispatch_outbox",
        "dispatch_claims",
        "liability_revisions",
        "financial_positions",
    ] {
        assert_eq!(n(f, table).await, 0, "{table}")
    }
    assert_eq!(n(f, "financial_events").await, 1)
}
async fn admit(f: &Fixture, r: &AdmissionRequest, clock: &TestClock) -> AdmissionReceipt {
    f.store
        .admit(r, &Synthetic, clock, Limits::default())
        .await
        .unwrap()
}
async fn complete(f: &Fixture, task: &str, key: &str, clock: &TestClock) -> AdmissionReceipt {
    let a = admit(f, &r(f, task, key, [1, 0, 0, 0]), clock).await;
    f.store
        .claim_dispatch(
            &dispatch(&a.lease, &format!("claim-{key}")),
            &Synthetic,
            clock,
            Limits::default(),
        )
        .await
        .unwrap();
    f.store
        .accept_task(
            &validation(f, task, Some(&a.lease), &format!("validate-{key}")),
            &Synthetic,
            clock,
            Limits::default(),
        )
        .await
        .unwrap();
    a
}
#[tokio::test]
async fn admission_commits_attempt_fence_finance_audit_and_durable_intent_together() {
    let f = ready(false, 100).await;
    let clock = TestClock::new();
    let req = r(&f, "task-a", "admit-a", [10, 20, 30, 40]);
    let a = admit(&f, &req, &clock).await;
    assert_eq!(a.lease.epoch, 1);
    assert_eq!(a.lease_until, TEST_TIME + 90);
    assert_eq!(
        f.store
            .financial_snapshot(&req.task.plan.project_id)
            .await
            .unwrap()
            .headroom,
        money(0)
    );
    for table in [
        "task_attempts",
        "attempt_states",
        "task_fences",
        "scheduler_events",
        "dispatch_outbox",
        "liability_revisions",
    ] {
        assert_eq!(n(&f, table).await, 1, "{table}")
    }
    assert_eq!(n(&f, "financial_positions").await, 4);
    assert_eq!(
        f.store
            .pending_dispatches(&req.task.plan.project_id, 0, 10)
            .await
            .unwrap()[0]
            .lease,
        a.lease
    );
    let reopened = PlanStore::open(&f.path).await.unwrap();
    assert_eq!(
        reopened
            .admit(&req, &Synthetic, &clock, Limits::default())
            .await
            .unwrap(),
        a
    );
    assert_eq!(
        reopened
            .load_active(&req.task.plan.project_id, &Synthetic, Limits::default())
            .await
            .unwrap()
            .revision,
        f.revision
    );
}
#[tokio::test]
async fn concurrent_duplicate_requests_have_one_effect_and_receipt() {
    let f = ready(false, 100).await;
    let clock = TestClock::new();
    let req = r(&f, "task-a", "a", [70, 0, 0, 0]);
    let (a, b) = tokio::join!(
        f.store.admit(&req, &Synthetic, &clock, Limits::default()),
        f.store.admit(&req, &Synthetic, &clock, Limits::default())
    );
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(n(&f, "task_attempts").await, 1);
    assert_eq!(n(&f, "dispatch_outbox").await, 1);
}
#[tokio::test]
async fn independent_concurrent_schedulers_cannot_reserve_the_same_last_headroom() {
    let f = ready(true, 100).await;
    let clock = TestClock::new();
    let a = r(&f, "task-a", "a", [70, 0, 0, 0]);
    let b = r(&f, "task-b", "b", [70, 0, 0, 0]);
    let other = PlanStore::open(&f.path).await.unwrap();
    let (a, b) = tokio::join!(
        f.store.admit(&a, &Synthetic, &clock, Limits::default()),
        other.admit(&b, &Synthetic, &clock, Limits::default())
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(
        matches!(
            a,
            Err(AdmissionError::Finance(FinanceError::InsufficientBudget))
        ) || matches!(
            b,
            Err(AdmissionError::Finance(FinanceError::InsufficientBudget))
        )
    );
    assert_eq!(n(&f, "task_attempts").await, 1);
    assert_eq!(n(&f, "dispatch_outbox").await, 1);
    assert_eq!(n(&f, "scheduler_events").await, 1);
}
#[tokio::test]
async fn all_ineligible_or_untrusted_admissions_leave_no_chargeable_intent() {
    let f = ready(false, 100).await;
    let clock = TestClock::new();
    let group = r(&f, "z-group", "group", [1, 0, 0, 0]);
    assert!(
        f.store
            .admit(&group, &Synthetic, &clock, Limits::default())
            .await
            .is_err()
    );
    let dependent = r(&f, "task-b", "blocked", [1, 0, 0, 0]);
    assert_eq!(
        f.store
            .admit(&dependent, &Synthetic, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::DependencyIncomplete
    );
    let mut variants = vec![r(&f, "task-a", "bad", [1, 0, 0, 0]); 7];
    variants[0].fingerprint = "a".repeat(64);
    variants[1].expected_epoch = 1;
    variants[2].task.task_id = TaskId::new("absent").unwrap();
    variants[3].actor = PrincipalId::new("worker").unwrap();
    variants[4].permit.provider = e("untrusted");
    variants[5].permit.valid_until = TEST_TIME;
    variants[6].task.plan.project_id = ProjectId::new("other").unwrap();
    for req in variants {
        assert!(
            f.store
                .admit(&req, &Synthetic, &clock, Limits::default())
                .await
                .is_err()
        );
        empty_admission(&f).await;
    }
}
#[tokio::test]
async fn raw_succeeded_status_is_not_validated_dependency_evidence() {
    let f = ready(false, 100).await;
    let raw = f.raw().await;
    sqlx::query("UPDATE task_runtime SET state='succeeded' WHERE task_id='task-a'")
        .execute(&raw)
        .await
        .unwrap();
    assert_eq!(
        f.store
            .admit(
                &r(&f, "task-b", "b", [1, 0, 0, 0]),
                &Synthetic,
                &TestClock::new(),
                Limits::default()
            )
            .await
            .unwrap_err(),
        AdmissionError::DependencyIncomplete
    );
    empty_admission(&f).await;
}
#[tokio::test]
async fn accepted_leaf_dependencies_and_group_aggregation_require_independent_evidence() {
    let f = ready(false, 100).await;
    let clock = TestClock::new();
    complete(&f, "task-a", "a", &clock).await;
    assert_eq!(
        f.store
            .accept_task(
                &validation(&f, "z-group", None, "early-group"),
                &Synthetic,
                &clock,
                Limits::default()
            )
            .await
            .unwrap_err(),
        AdmissionError::DependencyIncomplete
    );
    complete(&f, "task-b", "b", &clock).await;
    let group = validation(&f, "z-group", None, "group-ok");
    let first = f
        .store
        .accept_task(&group, &Synthetic, &clock, Limits::default())
        .await
        .unwrap();
    assert_eq!(
        first,
        f.store
            .accept_task(&group, &Synthetic, &clock, Limits::default())
            .await
            .unwrap()
    );
    assert_eq!(n(&f, "task_acceptances").await, 3);
    assert_eq!(n(&f, "task_attempts").await, 2);
    assert_eq!(
        f.store
            .load(&f.content.key, &Synthetic, Limits::default())
            .await
            .unwrap()
            .revision,
        f.revision
    );
}
#[tokio::test]
async fn worker_cannot_self_validate_and_acceptance_requires_committed_dispatch_claim() {
    let f = ready(false, 100).await;
    let clock = TestClock::new();
    let a = admit(&f, &r(&f, "task-a", "a", [1, 0, 0, 0]), &clock).await;
    let v = validation(&f, "task-a", Some(&a.lease), "validate");
    assert_eq!(
        f.store
            .accept_task(&v, &Synthetic, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::NotEligible
    );
    f.store
        .claim_dispatch(
            &dispatch(&a.lease, "claim"),
            &Synthetic,
            &clock,
            Limits::default(),
        )
        .await
        .unwrap();
    struct Permissive;
    impl AssignmentPolicy for Permissive {
        fn check(&self, a: &ExecutionAssignment) -> Result<(), AssignmentDenial> {
            Synthetic.check(a)
        }
    }
    impl SchedulerPolicy for Permissive {
        fn permits(&self, _: &PrincipalId, _: SchedulerAction<'_>, _: i64) -> bool {
            true
        }
    }
    let mut forged = v.clone();
    forged.actor = PrincipalId::new("synthetic-worker").unwrap();
    assert_eq!(
        f.store
            .accept_task(&forged, &Permissive, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::Forbidden
    );
    assert_eq!(n(&f, "task_acceptances").await, 0);
}
#[tokio::test]
async fn dispatch_claim_is_one_time_and_no_launch_occurs_for_a_failed_claim() {
    let f = ready(false, 100).await;
    let clock = TestClock::new();
    let a = admit(&f, &r(&f, "task-a", "a", [1, 0, 0, 0]), &clock).await;
    let req = dispatch(&a.lease, "claim");
    let permit = f
        .store
        .claim_dispatch(&req, &Synthetic, &clock, Limits::default())
        .await
        .unwrap();
    assert_eq!(
        permit.operation_id(),
        (&a.lease.task.plan.project_id, &a.lease.attempt_id)
    );
    assert_eq!(permit.attempt().lease_epoch, 1);
    assert_eq!(
        f.store
            .claim_dispatch(&req, &Synthetic, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::AlreadyClaimed
    );
    assert!(
        f.store
            .pending_dispatches(&f.content.key.project_id, 0, 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(n(&f, "dispatch_claims").await, 1);
}
#[tokio::test]
async fn fencing_and_expiry_never_release_funds_or_resurrect_authority() {
    let f = ready(false, 100).await;
    let clock = TestClock::new();
    let a = admit(&f, &r(&f, "task-a", "a", [50, 0, 0, 0]), &clock).await;
    clock.set(TEST_TIME - 1);
    assert_eq!(
        f.store.check_lease(&a.lease, &clock).await.unwrap_err(),
        AdmissionError::Expired
    );
    clock.set(TEST_TIME + 90);
    assert_eq!(
        f.store.check_lease(&a.lease, &clock).await.unwrap_err(),
        AdmissionError::Expired
    );
    assert_eq!(
        f.store
            .claim_dispatch(
                &dispatch(&a.lease, "expired"),
                &Synthetic,
                &clock,
                Limits::default()
            )
            .await
            .unwrap_err(),
        AdmissionError::Expired
    );
    let fence = fencing(&a.lease, "fence");
    assert_eq!(
        f.store
            .fence_attempt(&fence, &Synthetic, &clock)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        f.store
            .fence_attempt(&fence, &Synthetic, &clock)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        f.store.check_lease(&a.lease, &clock).await.unwrap_err(),
        AdmissionError::StaleFence
    );
    assert_eq!(
        f.store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap()
            .held
            .total()
            .unwrap(),
        money(50)
    );
    let mut retry = r(&f, "task-a", "retry", [1, 0, 0, 0]);
    retry.expected_epoch = 2;
    assert_eq!(
        f.store
            .admit(&retry, &Synthetic, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::RetryBlocked
    );
}
#[tokio::test]
async fn financial_unknowns_block_new_admission_but_no_headroom_does_not_block_covered_dispatch() {
    let f = ready(true, 50).await;
    let clock = TestClock::new();
    let a = admit(&f, &r(&f, "task-a", "a", [50, 0, 0, 0]), &clock).await;
    assert_eq!(
        f.store
            .admit(
                &r(&f, "task-b", "b", [1, 0, 0, 0]),
                &Synthetic,
                &clock,
                Limits::default()
            )
            .await
            .unwrap_err(),
        AdmissionError::Finance(FinanceError::InsufficientBudget)
    );
    f.store
        .claim_dispatch(
            &dispatch(&a.lease, "claim"),
            &Synthetic,
            &clock,
            Limits::default(),
        )
        .await
        .unwrap();
    let unknown = FinancialRequest {
        project: f.content.key.project_id.clone(),
        request_id: RequestId::new("unknown").unwrap(),
        actor: owner(),
        reason: "Synthetic unknown external billing".into(),
        command: FinancialCommand::Report {
            liability: LiabilityId::new("external").unwrap(),
            expected_revision: 0,
            unreserved: costs([0; 4]),
            uncertain: true,
            evidence: e("synthetic-unknown"),
        },
    };
    f.store
        .apply_financial(&unknown, &FinancialFixture)
        .await
        .unwrap();
    assert_eq!(
        f.store
            .admit(
                &r(&f, "task-b", "b", [1, 0, 0, 0]),
                &Synthetic,
                &clock,
                Limits::default()
            )
            .await
            .unwrap_err(),
        AdmissionError::Finance(FinanceError::UncertainExposure)
    );
    assert_eq!(n(&f, "task_attempts").await, 1);
    assert_eq!(n(&f, "dispatch_outbox").await, 1);
}

async fn close_financial_hold(f: &Fixture, lease: &LeaseRef, key: &str) {
    f.store
        .apply_financial(
            &FinancialRequest {
                project: lease.task.plan.project_id.clone(),
                request_id: RequestId::new(key).unwrap(),
                actor: owner(),
                reason: "Synthetic independently reconciled external execution".into(),
                command: FinancialCommand::Reconcile {
                    liability: LiabilityId::new(lease.attempt_id.as_str()).unwrap(),
                    expected_revision: 1,
                    settled: costs([0; 4]),
                    remaining_hold: costs([0; 4]),
                    remaining_provisional: costs([0; 4]),
                    state: ReconciliationState::Final,
                    evidence: e(key),
                },
            },
            &FinancialFixture,
        )
        .await
        .unwrap();
}
fn recovery(lease: &LeaseRef, key: &str) -> RecoveryRequest {
    RecoveryRequest {
        lease: lease.clone(),
        request_id: RequestId::new(key).unwrap(),
        actor: owner(),
        evidence: e("synthetic-reconciled-external-effects"),
    }
}
#[tokio::test]
async fn retry_requires_financial_and_external_reconciliation_and_stale_epoch_cannot_affect_new_attempt()
 {
    let f = ready(false, 100).await;
    let clock = TestClock::new();
    let first = r(&f, "task-a", "first", [50, 0, 0, 0]);
    let a = admit(&f, &first, &clock).await;
    let old = fencing(&a.lease, "fence-a");
    f.store
        .fence_attempt(&old, &Synthetic, &clock)
        .await
        .unwrap();
    let mut next = r(&f, "task-a", "second", [40, 0, 0, 0]);
    next.expected_epoch = 2;
    close_financial_hold(&f, &a.lease, "bill-final-a").await;
    assert_eq!(
        f.store
            .admit(&next, &Synthetic, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::RetryBlocked
    );
    let recover = recovery(&a.lease, "recover-a");
    let after = f
        .store
        .reconcile_fenced(&recover, &Synthetic, &clock)
        .await
        .unwrap();
    assert_eq!(after, TEST_TIME + 11);
    assert_eq!(
        after,
        f.store
            .reconcile_fenced(&recover, &Synthetic, &clock)
            .await
            .unwrap()
    );
    assert_eq!(
        f.store
            .admit(&next, &Synthetic, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::RetryBlocked
    );
    clock.set(after);
    let b = admit(&f, &next, &clock).await;
    assert_eq!(b.lease.epoch, 3);
    let stale = fencing(&a.lease, "stale-fence");
    assert_eq!(
        f.store
            .fence_attempt(&stale, &Synthetic, &clock)
            .await
            .unwrap_err(),
        AdmissionError::StaleFence
    );
    assert_eq!(
        f.store.check_lease(&a.lease, &clock).await.unwrap_err(),
        AdmissionError::StaleFence
    );
    f.store.check_lease(&b.lease, &clock).await.unwrap();
    assert_eq!(
        f.store
            .claim_dispatch(
                &dispatch(&a.lease, "stale-dispatch"),
                &Synthetic,
                &clock,
                Limits::default()
            )
            .await
            .unwrap_err(),
        AdmissionError::StaleFence
    );
    assert_eq!(
        f.store
            .accept_task(
                &validation(&f, "task-a", Some(&a.lease), "stale-validation"),
                &Synthetic,
                &clock,
                Limits::default()
            )
            .await
            .unwrap_err(),
        AdmissionError::StaleFence
    );
    assert_eq!(
        f.store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap()
            .held
            .total()
            .unwrap(),
        money(40)
    );
    assert_eq!(
        f.store
            .admit(&first, &Synthetic, &clock, Limits::default())
            .await
            .unwrap(),
        a
    );
    assert_eq!(n(&f, "task_attempts").await, 2);
}
#[tokio::test]
async fn unknown_prior_hold_blocks_retry_even_after_external_reconciliation() {
    let f = ready(false, 100).await;
    let clock = TestClock::new();
    let a = admit(&f, &r(&f, "task-a", "a", [50, 0, 0, 0]), &clock).await;
    f.store
        .fence_attempt(&fencing(&a.lease, "fence"), &Synthetic, &clock)
        .await
        .unwrap();
    let after = f
        .store
        .reconcile_fenced(&recovery(&a.lease, "recover"), &Synthetic, &clock)
        .await
        .unwrap();
    clock.set(after);
    let mut retry = r(&f, "task-a", "retry", [1, 0, 0, 0]);
    retry.expected_epoch = 2;
    assert_eq!(
        f.store
            .admit(&retry, &Synthetic, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::RetryBlocked
    );
    assert_eq!(
        f.store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap()
            .held
            .total()
            .unwrap(),
        money(50)
    );
}
#[tokio::test]
async fn three_total_attempts_and_retry_delays_persist_across_restart() {
    let f = ready(false, 100).await;
    let clock = TestClock::new();
    let mut epoch = 0;
    for index in 1..=3 {
        let key = format!("attempt-{index}");
        let mut req = r(&f, "task-a", &key, [1, 0, 0, 0]);
        req.expected_epoch = epoch;
        let a = admit(&f, &req, &clock).await;
        epoch = f
            .store
            .fence_attempt(
                &fencing(&a.lease, &format!("fence-{index}")),
                &Synthetic,
                &clock,
            )
            .await
            .unwrap();
        close_financial_hold(&f, &a.lease, &format!("bill-{index}")).await;
        let recover = recovery(&a.lease, &format!("recover-{index}"));
        if index < 3 {
            let after = f
                .store
                .reconcile_fenced(&recover, &Synthetic, &clock)
                .await
                .unwrap();
            assert!(after >= clock.now().unwrap() + if index == 1 { 10 } else { 30 });
            clock.set(after);
        } else {
            assert_eq!(
                f.store
                    .reconcile_fenced(&recover, &Synthetic, &clock)
                    .await
                    .unwrap_err(),
                AdmissionError::RetryBlocked
            );
        }
    }
    let store = PlanStore::open(&f.path).await.unwrap();
    let mut req = r(&f, "task-a", "fourth", [1, 0, 0, 0]);
    req.expected_epoch = epoch;
    assert_eq!(
        store
            .admit(&req, &Synthetic, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::RetryBlocked
    );
    assert_eq!(n(&f, "task_attempts").await, 3);
}
#[tokio::test]
async fn transaction_faults_at_every_admission_write_leave_no_partial_authority() {
    for table in [
        "financial_events",
        "liability_revisions",
        "financial_positions",
        "scheduler_events",
        "task_attempts",
        "attempt_states",
        "task_fences",
        "task_runtime",
        "dispatch_outbox",
    ] {
        for mode in ["ABORT", "IGNORE"] {
            let f = ready(false, 100).await;
            let raw = f.raw().await;
            let verb = if table == "task_runtime" {
                "UPDATE"
            } else {
                "INSERT"
            };
            let condition = if table == "financial_positions" {
                "WHEN NEW.category='storage'"
            } else {
                ""
            };
            let raise = if mode == "ABORT" {
                "RAISE(ABORT,'synthetic injected failure')"
            } else {
                "RAISE(IGNORE)"
            };
            sqlx::query(&format!("CREATE TRIGGER injected BEFORE {verb} ON {table} {condition} BEGIN SELECT {raise}; END")).execute(&raw).await.unwrap();
            let result = f
                .store
                .admit(
                    &r(&f, "task-a", "attempt", [10, 20, 30, 40]),
                    &Synthetic,
                    &TestClock::new(),
                    Limits::default(),
                )
                .await;
            assert!(result.is_err(), "{table}/{mode}");
            empty_admission(&f).await;
            sqlx::query("DROP TRIGGER injected")
                .execute(&raw)
                .await
                .unwrap();
            admit(
                &f,
                &r(&f, "task-a", "attempt", [10, 20, 30, 40]),
                &TestClock::new(),
            )
            .await;
        }
    }
}
#[tokio::test]
async fn ignored_fence_advance_and_dispatch_claim_roll_back_without_launch_permission() {
    let f = ready(false, 100).await;
    let raw = f.raw().await;
    sqlx::query(
        "CREATE TRIGGER injected BEFORE UPDATE ON task_fences BEGIN SELECT RAISE(IGNORE); END",
    )
    .execute(&raw)
    .await
    .unwrap();
    assert!(
        f.store
            .admit(
                &r(&f, "task-a", "a", [1, 0, 0, 0]),
                &Synthetic,
                &TestClock::new(),
                Limits::default()
            )
            .await
            .is_err()
    );
    empty_admission(&f).await;
    sqlx::query("DROP TRIGGER injected")
        .execute(&raw)
        .await
        .unwrap();
    let a = admit(&f, &r(&f, "task-a", "a", [1, 0, 0, 0]), &TestClock::new()).await;
    sqlx::query(
        "CREATE TRIGGER injected BEFORE INSERT ON dispatch_claims BEGIN SELECT RAISE(IGNORE); END",
    )
    .execute(&raw)
    .await
    .unwrap();
    let mut launches = 0;
    if f.store
        .claim_dispatch(
            &dispatch(&a.lease, "claim"),
            &Synthetic,
            &TestClock::new(),
            Limits::default(),
        )
        .await
        .is_ok()
    {
        launches += 1
    }
    assert_eq!(launches, 0);
    assert_eq!(n(&f, "dispatch_claims").await, 0);
    assert_eq!(n(&f, "scheduler_events").await, 1);
}
#[tokio::test]
async fn concurrent_dispatch_claims_issue_only_one_committed_permit() {
    let f = ready(false, 100).await;
    let clock = TestClock::new();
    let a = admit(&f, &r(&f, "task-a", "a", [1, 0, 0, 0]), &clock).await;
    let x = dispatch(&a.lease, "claim-x");
    let y = dispatch(&a.lease, "claim-y");
    let (a, b) = tokio::join!(
        f.store
            .claim_dispatch(&x, &Synthetic, &clock, Limits::default()),
        f.store
            .claim_dispatch(&y, &Synthetic, &clock, Limits::default())
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert!(
        matches!(a, Err(AdmissionError::AlreadyClaimed))
            || matches!(b, Err(AdmissionError::AlreadyClaimed))
    );
    assert_eq!(n(&f, "dispatch_claims").await, 1);
}
#[tokio::test]
async fn changed_idempotency_payload_or_reused_capability_cannot_authorize_another_attempt() {
    let f = ready(true, 100).await;
    let clock = TestClock::new();
    let req = r(&f, "task-a", "a", [10, 0, 0, 0]);
    admit(&f, &req, &clock).await;
    let mut changed = req.clone();
    changed.permit.maximum = costs([11, 0, 0, 0]);
    assert_eq!(
        f.store
            .admit(&changed, &Synthetic, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::ReplayConflict
    );
    struct Always;
    impl AssignmentPolicy for Always {
        fn check(&self, a: &ExecutionAssignment) -> Result<(), AssignmentDenial> {
            Synthetic.check(a)
        }
    }
    impl SchedulerPolicy for Always {
        fn permits(&self, _: &PrincipalId, _: SchedulerAction<'_>, _: i64) -> bool {
            true
        }
    }
    let mut other = r(&f, "task-b", "b", [10, 0, 0, 0]);
    other.permit.id = req.permit.id.clone();
    assert_eq!(
        f.store
            .admit(&other, &Always, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::Capability
    );
    assert_eq!(n(&f, "task_attempts").await, 1);
}
#[tokio::test]
async fn revocation_before_commit_rolls_back_all_staged_admission_writes() {
    use std::sync::atomic::AtomicUsize;
    struct Revoked(AtomicUsize);
    impl AssignmentPolicy for Revoked {
        fn check(&self, a: &ExecutionAssignment) -> Result<(), AssignmentDenial> {
            Synthetic.check(a)
        }
    }
    impl SchedulerPolicy for Revoked {
        fn permits(&self, _: &PrincipalId, _: SchedulerAction<'_>, _: i64) -> bool {
            self.0.fetch_add(1, Ordering::SeqCst) == 0
        }
    }
    let f = ready(false, 100).await;
    assert!(
        f.store
            .admit(
                &r(&f, "task-a", "a", [1, 0, 0, 0]),
                &Revoked(AtomicUsize::new(0)),
                &TestClock::new(),
                Limits::default()
            )
            .await
            .is_err()
    );
    empty_admission(&f).await;
}
#[tokio::test]
async fn completed_work_and_static_plan_survive_failed_sibling_admission_and_fencing() {
    let f = ready(false, 2).await;
    let clock = TestClock::new();
    let completed = complete(&f, "task-a", "a", &clock).await;
    assert!(
        f.store
            .admit(
                &r(&f, "task-b", "b", [10, 0, 0, 0]),
                &Synthetic,
                &clock,
                Limits::default()
            )
            .await
            .is_err()
    );
    assert_eq!(
        f.store
            .fence_attempt(&fencing(&completed.lease, "bad-fence"), &Synthetic, &clock)
            .await
            .unwrap_err(),
        AdmissionError::StaleFence
    );
    assert_eq!(n(&f, "task_acceptances").await, 1);
    assert_eq!(
        f.store
            .load_active(&f.content.key.project_id, &Synthetic, Limits::default())
            .await
            .unwrap()
            .revision,
        f.revision
    );
    let raw = f.raw().await;
    assert!(
        sqlx::query("UPDATE task_runtime SET state='waiting' WHERE task_id='task-a'")
            .execute(&raw)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn writer_contention_fails_closed_while_readers_see_no_uncommitted_intent() {
    use std::time::{Duration, Instant};
    let f = ready(false, 100).await;
    let raw = f.raw().await;
    let tx = raw.begin_with("BEGIN IMMEDIATE").await.unwrap();
    assert!(
        f.store
            .pending_dispatches(&f.content.key.project_id, 0, 10)
            .await
            .unwrap()
            .is_empty()
    );
    let start = Instant::now();
    assert_eq!(
        tokio::time::timeout(
            Duration::from_secs(12),
            f.store.admit(
                &r(&f, "task-a", "a", [1, 0, 0, 0]),
                &Synthetic,
                &TestClock::new(),
                Limits::default()
            )
        )
        .await
        .unwrap()
        .unwrap_err(),
        AdmissionError::Storage(StoreError::Busy)
    );
    assert!(start.elapsed() >= Duration::from_secs(4));
    tx.rollback().await.unwrap();
    empty_admission(&f).await;
}

#[tokio::test]
async fn dispatch_operation_identity_includes_project_scope() {
    let f = ready(false, 100).await;
    let clock = TestClock::new();
    let a = admit(&f, &r(&f, "task-a", "shared-attempt", [1, 0, 0, 0]), &clock).await;
    let first = f
        .store
        .claim_dispatch(
            &dispatch(&a.lease, "claim"),
            &Synthetic,
            &clock,
            Limits::default(),
        )
        .await
        .unwrap();
    let mut other = Fixture::new().await;
    other.content.key.project_id = ProjectId::new("other-project").unwrap();
    for task in &mut other.content.tasks {
        task.plan = other.content.key.clone();
        if let Some(parent) = &mut task.parent {
            parent.plan = other.content.key.clone()
        }
        for dep in &mut task.dependencies {
            dep.plan = other.content.key.clone()
        }
    }
    other
        .store
        .create_project(&other.content.key.project_id, &owner())
        .await
        .unwrap();
    let valid =
        rolvya_plan_validator::validate(&other.content, &Synthetic, Limits::default()).unwrap();
    other.revision = other
        .store
        .save_draft(&valid, None, &owner(), &OwnerPolicy)
        .await
        .unwrap();
    other.activate().await.unwrap();
    fund(&other, 100).await;
    let b = admit(
        &other,
        &r(&other, "task-a", "shared-attempt", [1, 0, 0, 0]),
        &clock,
    )
    .await;
    let second = other
        .store
        .claim_dispatch(
            &dispatch(&b.lease, "claim"),
            &Synthetic,
            &clock,
            Limits::default(),
        )
        .await
        .unwrap();
    assert_ne!(first.operation_id(), second.operation_id());
}

#[tokio::test]
async fn real_api_process_crashes_before_commit_after_commit_and_after_claim_preserve_safe_recovery()
 {
    use std::sync::atomic::AtomicUsize;
    if let Ok(mode) = std::env::var("ROLVYA_ADMISSION_CRASH_MODE") {
        let path = std::env::var("ROLVYA_ADMISSION_CRASH_DB").unwrap();
        let store = PlanStore::open(&path).await.unwrap();
        let clock = TestClock::new();
        let loaded = store
            .load_active(
                &ProjectId::new("project").unwrap(),
                &Synthetic,
                Limits::default(),
            )
            .await
            .unwrap();
        let request = build(
            &loaded.content.key,
            loaded.revision.fingerprint(),
            "task-a",
            "crash-attempt",
            [10, 0, 0, 0],
        );
        struct ExitBeforeCommit(AtomicUsize);
        impl AssignmentPolicy for ExitBeforeCommit {
            fn check(&self, a: &ExecutionAssignment) -> Result<(), AssignmentDenial> {
                Synthetic.check(a)
            }
        }
        impl SchedulerPolicy for ExitBeforeCommit {
            fn permits(&self, o: &PrincipalId, a: SchedulerAction<'_>, t: i64) -> bool {
                if self.0.fetch_add(1, Ordering::SeqCst) == 1 {
                    std::process::exit(17)
                }
                Synthetic.permits(o, a, t)
            }
        }
        if mode == "before-commit" {
            store
                .admit(
                    &request,
                    &ExitBeforeCommit(AtomicUsize::new(0)),
                    &clock,
                    Limits::default(),
                )
                .await
                .unwrap();
            panic!("fault hook was not reached")
        }
        let receipt = store
            .admit(&request, &Synthetic, &clock, Limits::default())
            .await
            .unwrap();
        if mode == "after-claim" {
            let permit = store
                .claim_dispatch(
                    &dispatch(&receipt.lease, "crash-claim"),
                    &Synthetic,
                    &clock,
                    Limits::default(),
                )
                .await
                .unwrap();
            // Deterministic fake external side effect, keyed by project+attempt.
            // File creation is deliberately after the committed permit is returned.
            use std::io::Write;
            let fake_path = std::path::Path::new(&path).with_extension("synthetic-provider-effect");
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(fake_path)
                .unwrap();
            file.write_all(
                serde_json::to_string(&permit.operation_id())
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
            file.sync_all().unwrap();
        }
        // Lost acknowledgement: no Drop/connection cleanup runs.
        std::process::exit(17);
    }
    let f = ready(false, 100).await;
    let clock = TestClock::new();
    for mode in ["before-commit", "after-commit", "after-claim"] {
        let status=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","real_api_process_crashes_before_commit_after_commit_and_after_claim_preserve_safe_recovery","--nocapture"]).env("ROLVYA_ADMISSION_CRASH_MODE",mode).env("ROLVYA_ADMISSION_CRASH_DB",&f.path).status().unwrap();
        assert_eq!(status.code(), Some(17));
        let reopened = PlanStore::open(&f.path).await.unwrap();
        reopened.check_integrity().await.unwrap();
        if mode == "before-commit" {
            empty_admission(&f).await;
            assert!(
                reopened
                    .pending_dispatches(&f.content.key.project_id, 0, 10)
                    .await
                    .unwrap()
                    .is_empty()
            );
        } else {
            assert_eq!(n(&f, "task_attempts").await, 1);
            assert_eq!(n(&f, "dispatch_outbox").await, 1);
            assert_eq!(
                f.store
                    .financial_snapshot(&f.content.key.project_id)
                    .await
                    .unwrap()
                    .held
                    .total()
                    .unwrap(),
                money(10)
            );
            let receipt = reopened
                .admit(
                    &r(&f, "task-a", "crash-attempt", [10, 0, 0, 0]),
                    &Synthetic,
                    &clock,
                    Limits::default(),
                )
                .await
                .unwrap();
            if mode == "after-claim" {
                assert_eq!(
                    reopened
                        .claim_dispatch(
                            &dispatch(&receipt.lease, "repeat-after-crash"),
                            &Synthetic,
                            &clock,
                            Limits::default()
                        )
                        .await
                        .unwrap_err(),
                    AdmissionError::AlreadyClaimed
                );
                let effect =
                    std::fs::read_to_string(f.path.with_extension("synthetic-provider-effect"))
                        .unwrap();
                assert_eq!(
                    effect,
                    serde_json::to_string(&(&f.content.key.project_id, &receipt.lease.attempt_id))
                        .unwrap()
                );
                assert_eq!(n(&f, "dispatch_claims").await, 1);
            } else {
                assert_eq!(
                    reopened
                        .pending_dispatches(&f.content.key.project_id, 0, 10)
                        .await
                        .unwrap()
                        .len(),
                    1
                );
            }
        }
        reopened.close().await;
    }
}
#[tokio::test]
async fn group_dependency_stays_blocked_until_children_and_group_acceptance_are_validated() {
    let mut f = Fixture::new().await;
    f.content.key.version = PlanVersion::new(2).unwrap();
    for task in &mut f.content.tasks {
        task.plan = f.content.key.clone();
        if let Some(parent) = &mut task.parent {
            parent.plan = f.content.key.clone()
        }
        for dep in &mut task.dependencies {
            dep.plan = f.content.key.clone()
        }
    }
    let mut c = task("task-c", &f.content.key);
    let group = f
        .content
        .tasks
        .iter()
        .find(|t| t.kind == TaskKind::Group)
        .unwrap();
    c.dependencies.push(task_ref(group));
    f.content.tasks.push(c);
    let valid = rolvya_plan_validator::validate(&f.content, &Synthetic, Limits::default()).unwrap();
    f.revision = f
        .store
        .save_draft(&valid, Some(&f.revision), &owner(), &OwnerPolicy)
        .await
        .unwrap();
    f.activate().await.unwrap();
    fund(&f, 100).await;
    let clock = TestClock::new();
    let req = r(&f, "task-c", "c", [1, 0, 0, 0]);
    assert_eq!(
        f.store
            .admit(&req, &Synthetic, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::DependencyIncomplete
    );
    complete(&f, "task-a", "a", &clock).await;
    assert!(
        f.store
            .accept_task(
                &validation(&f, "z-group", None, "early-group"),
                &Synthetic,
                &clock,
                Limits::default()
            )
            .await
            .is_err()
    );
    complete(&f, "task-b", "b", &clock).await;
    assert_eq!(
        f.store
            .admit(&req, &Synthetic, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::DependencyIncomplete
    );
    f.store
        .accept_task(
            &validation(&f, "z-group", None, "group-acceptance"),
            &Synthetic,
            &clock,
            Limits::default(),
        )
        .await
        .unwrap();
    admit(&f, &req, &clock).await;
    assert_eq!(n(&f, "task_attempts").await, 3);
}
#[tokio::test]
async fn negative_scope_mutations_permissions_and_unknown_exposure_do_not_issue_a_dispatch_permit()
{
    let f = ready(false, 100).await;
    let clock = TestClock::new();
    let a = admit(&f, &r(&f, "task-a", "a", [50, 0, 0, 0]), &clock).await;
    let mut stale = a.lease.clone();
    stale.epoch += 1;
    let mut wrong_task = a.lease.clone();
    wrong_task.task.task_id = TaskId::new("task-b").unwrap();
    let mut wrong_project = a.lease.clone();
    wrong_project.task.plan.project_id = ProjectId::new("other").unwrap();
    for lease in [stale, wrong_task, wrong_project] {
        assert!(f.store.check_lease(&lease, &clock).await.is_err());
    }
    let mut bad = dispatch(&a.lease, "bad");
    bad.actor = PrincipalId::new("untrusted-worker").unwrap();
    assert_eq!(
        f.store
            .claim_dispatch(&bad, &Synthetic, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::Forbidden
    );
    let mut stop = fencing(&a.lease, "bad-fence");
    stop.actor = bad.actor.clone();
    assert_eq!(
        f.store
            .fence_attempt(&stop, &Synthetic, &clock)
            .await
            .unwrap_err(),
        AdmissionError::Forbidden
    );
    let mut recover = recovery(&a.lease, "bad-recover");
    recover.actor = bad.actor;
    assert_eq!(
        f.store
            .reconcile_fenced(&recover, &Synthetic, &clock)
            .await
            .unwrap_err(),
        AdmissionError::Forbidden
    );
    f.store
        .apply_financial(
            &FinancialRequest {
                project: f.content.key.project_id.clone(),
                request_id: RequestId::new("unknown").unwrap(),
                actor: owner(),
                reason: "Synthetic ambiguous provider bill".into(),
                command: FinancialCommand::Report {
                    liability: LiabilityId::new("external").unwrap(),
                    expected_revision: 0,
                    unreserved: costs([0; 4]),
                    uncertain: true,
                    evidence: e("synthetic-uncertain"),
                },
            },
            &FinancialFixture,
        )
        .await
        .unwrap();
    assert_eq!(
        f.store
            .claim_dispatch(
                &dispatch(&a.lease, "blocked-by-unknown"),
                &Synthetic,
                &clock,
                Limits::default()
            )
            .await
            .unwrap_err(),
        AdmissionError::Finance(FinanceError::UncertainExposure)
    );
    assert_eq!(n(&f, "dispatch_claims").await, 0);
    assert_eq!(n(&f, "scheduler_events").await, 1);
}
#[tokio::test]
async fn invalid_money_epoch_overflow_and_capability_expiry_at_commit_do_not_create_authority() {
    let f = ready(false, 100).await;
    let clock = TestClock::new();
    let mut bad = r(&f, "task-a", "overflow", [i64::MAX, 1, 0, 0]);
    assert!(
        f.store
            .admit(&bad, &Synthetic, &clock, Limits::default())
            .await
            .is_err()
    );
    bad.permit.maximum = costs([1, 0, 0, 0]);
    bad.expected_epoch = u64::MAX;
    assert!(
        f.store
            .admit(&bad, &Synthetic, &clock, Limits::default())
            .await
            .is_err()
    );
    bad.expected_epoch = 0;
    bad.permit.maximum.ai_api = Money::new(1, Currency::new("EUR").unwrap()).unwrap();
    assert!(
        f.store
            .admit(&bad, &Synthetic, &clock, Limits::default())
            .await
            .is_err()
    );
    struct Expires(std::sync::atomic::AtomicUsize);
    impl Clock for Expires {
        fn now(&self) -> Result<i64, AdmissionError> {
            Ok(if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                TEST_TIME
            } else {
                TEST_TIME + 90
            })
        }
    }
    assert!(
        f.store
            .admit(
                &r(&f, "task-a", "expired", [1, 0, 0, 0]),
                &Synthetic,
                &Expires(std::sync::atomic::AtomicUsize::new(0)),
                Limits::default()
            )
            .await
            .is_err()
    );
    empty_admission(&f).await;
}
#[tokio::test]
async fn pending_outbox_pagination_is_bounded_and_reservations_cannot_be_adopted_from_another_transaction()
 {
    let f = ready(true, 100).await;
    let clock = TestClock::new();
    let req = r(&f, "task-a", "preexisting", [1, 0, 0, 0]);
    let finance = FinancialRequest {
        project: req.task.plan.project_id.clone(),
        request_id: req.request_id.clone(),
        actor: req.actor.clone(),
        reason: "Atomic scheduler admission".into(),
        command: FinancialCommand::Reserve {
            liability: LiabilityId::new(req.request_id.as_str()).unwrap(),
            maximum: req.permit.maximum,
            bound_evidence: req.permit.id.clone(),
        },
    };
    f.store
        .apply_financial(&finance, &FinancialFixture)
        .await
        .unwrap();
    assert_eq!(
        f.store
            .admit(&req, &Synthetic, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::Conflict
    );
    assert_eq!(n(&f, "task_attempts").await, 0);
    let a = admit(&f, &r(&f, "task-a", "a", [1, 0, 0, 0]), &clock).await;
    let b = admit(&f, &r(&f, "task-b", "b", [1, 0, 0, 0]), &clock).await;
    let first = f
        .store
        .pending_dispatches(&f.content.key.project_id, 0, 1)
        .await
        .unwrap();
    assert_eq!(first[0].lease, a.lease);
    assert_eq!(
        f.store
            .pending_dispatches(&f.content.key.project_id, first[0].sequence, 1)
            .await
            .unwrap()[0]
            .lease,
        b.lease
    );
    for (after, limit) in [(-1, 1), (0, 0), (0, 101)] {
        assert_eq!(
            f.store
                .pending_dispatches(&f.content.key.project_id, after, limit)
                .await
                .unwrap_err(),
            AdmissionError::InvalidInput
        );
    }
}
