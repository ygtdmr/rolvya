mod common;
use common::*;
use rolvya_domain::{ids::*, money::*, plan::*, worker::*};
use rolvya_persistence_sqlite::{
    admission::*, finance::*, worker_control::*, worker_simulation::*, *,
};
use rolvya_plan_validator::{AssignmentDenial, AssignmentPolicy, Limits};
use std::collections::BTreeMap;
const T: i64 = 1_800_000_000;
fn money(n: i64) -> Money {
    Money::new(n, Currency::new("USD").unwrap()).unwrap()
}
fn costs() -> CostBreakdown {
    CostBreakdown {
        ai_api: money(1),
        compute: money(2),
        storage: money(3),
        transfer: money(1),
    }
}
fn e(s: &str) -> EvidenceRef {
    EvidenceRef::new(s).unwrap()
}
fn caps() -> DeclaredCapabilities {
    DeclaredCapabilities {
        models: vec![ModelName::new("synthetic/model").unwrap()],
        efforts: vec![Effort::Medium],
        access_modes: vec![AccessMode::ApiKey],
        cpu_millis: 1000,
        memory_mib: 512,
        evidence: Some(CapabilityRef::new("synthetic-capability").unwrap()),
    }
}
struct SyntheticScheduler;
impl AssignmentPolicy for SyntheticScheduler {
    fn check(&self, a: &ExecutionAssignment) -> Result<(), AssignmentDenial> {
        SyntheticPolicy.check(a)
    }
}
impl SchedulerPolicy for SyntheticScheduler {
    fn permits(&self, owner: &PrincipalId, action: SchedulerAction<'_>, _: i64) -> bool {
        match action {
            SchedulerAction::Admit { request, .. } => {
                request.actor == *owner && request.permit.provider.as_str() == "synthetic"
            }
            SchedulerAction::Dispatch { request, .. } => request.actor == *owner,
            SchedulerAction::Validate { request, .. } => {
                request.actor.as_str() == "independent-validator"
                    && request.evidence.as_str() == "synthetic-independent-check"
            }
            SchedulerAction::Fence(r) => r.actor == *owner,
            SchedulerAction::Recover(r) => r.actor == *owner,
        }
    }
}
struct SyntheticFinance;
impl FinancialAuthorization for SyntheticFinance {
    fn permits(&self, owner: &PrincipalId, r: &FinancialRequest, _: i64) -> bool {
        r.actor == *owner
    }
}
#[derive(Default)]
struct SyntheticAuthority {
    grants: BTreeMap<(ProjectId, AttemptId), (WorkerBinding, EvidenceRef)>,
    revoked: bool,
}
impl SyntheticAuthority {
    fn grant(&mut self, b: &WorkerBinding, peer: &PeerEvidence) {
        self.grants.insert(
            (b.task.plan.project_id.clone(), b.attempt_id.clone()),
            (b.clone(), peer.reference.clone()),
        );
    }
}
impl WorkerAuthority for SyntheticAuthority {
    fn permits(
        &self,
        peer: &PeerEvidence,
        b: &WorkerBinding,
        operation: WorkerOperation,
        declared: Option<&DeclaredCapabilities>,
        _: i64,
    ) -> bool {
        !self.revoked
            && self
                .grants
                .get(&(b.task.plan.project_id.clone(), b.attempt_id.clone()))
                .is_some_and(|(scope, reference)| scope == b && reference == &peer.reference)
            && match declared {
                Some(c) => *c == caps(),
                None => matches!(
                    operation,
                    WorkerOperation::StopQuery | WorkerOperation::Disconnect
                ),
            }
    }
}
async fn base(independent: bool) -> Fixture {
    let mut f = Fixture::new().await;
    if independent {
        f.content.key.version = PlanVersion::new(2).unwrap();
        for t in &mut f.content.tasks {
            t.plan = f.content.key.clone();
            t.dependencies.clear();
            if let Some(parent) = &mut t.parent {
                parent.plan = f.content.key.clone()
            }
        }
        let valid =
            rolvya_plan_validator::validate(&f.content, &SyntheticPolicy, Limits::default())
                .unwrap();
        f.revision = f
            .store
            .save_draft(&valid, Some(&f.revision), &owner(), &OwnerPolicy)
            .await
            .unwrap();
    }
    activate_and_fund(f).await
}
async fn activate_and_fund(f: Fixture) -> Fixture {
    f.activate().await.unwrap();
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
                reason: "Explicit synthetic fixture only".into(),
                command: FinancialCommand::Approve {
                    expected_version: 0,
                    new_ceiling: money(10000),
                    proof: OwnerProof {
                        reference: e("synthetic-owner-proof"),
                        verified_at: now,
                    },
                },
            },
            &SyntheticFinance,
        )
        .await
        .unwrap();
    f
}

struct Setup {
    f: Fixture,
    clock: DeterministicClock,
    controller: DurableWorkerController,
    dispatcher: OutboxDispatcher,
    provider: SyntheticProvider,
    lease: LeaseRef,
    binding: WorkerBinding,
    peer: PeerEvidence,
    authority: SyntheticAuthority,
}
async fn admitted() -> (Fixture, DeterministicClock, LeaseRef) {
    admitted_until(T + 3600).await
}
async fn admitted_until(deadline: i64) -> (Fixture, DeterministicClock, LeaseRef) {
    let f = base(false).await;
    let clock = DeterministicClock::new(T).unwrap();
    let receipt = f
        .store
        .admit(
            &AdmissionRequest {
                task: TaskRef {
                    plan: f.content.key.clone(),
                    task_id: TaskId::new("task-a").unwrap(),
                },
                fingerprint: f.revision.fingerprint().into(),
                expected_epoch: 0,
                request_id: RequestId::new("attempt-a").unwrap(),
                actor: owner(),
                permit: ExecutionPermit {
                    id: e("cap-attempt-a"),
                    provider: e("synthetic"),
                    account: e("synthetic-account"),
                    pool: e("synthetic-pool"),
                    executor: PrincipalId::new("worker-attempt-a").unwrap(),
                    maximum: costs(),
                    valid_until: deadline,
                },
            },
            &SyntheticScheduler,
            &clock,
            Limits::default(),
        )
        .await
        .unwrap();
    (f, clock, receipt.lease)
}
async fn setup() -> Setup {
    setup_until(T + 3600).await
}
async fn setup_until(deadline: i64) -> Setup {
    let (f, clock, lease) = admitted_until(deadline).await;
    let dispatcher = OutboxDispatcher::new(f.store.clone());
    let provider = SyntheticProvider::new(f.store.clone());
    let ticket = dispatcher
        .claim(
            &lease,
            &owner(),
            &SyntheticScheduler,
            &clock,
            Limits::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        dispatcher
            .dispatch(
                &ticket,
                &provider,
                &SyntheticScheduler,
                &clock,
                Limits::default()
            )
            .await
            .unwrap(),
        DispatchState::Active
    );
    let controller = DurableWorkerController::new(f.store.clone());
    let binding = controller.binding(&lease).await.unwrap();
    let peer = PeerEvidence {
        reference: e("peer-attempt-a"),
    };
    let mut authority = SyntheticAuthority::default();
    authority.grant(&binding, &peer);
    Setup {
        f,
        clock,
        controller,
        dispatcher,
        provider,
        lease,
        binding,
        peer,
        authority,
    }
}
fn message(s: &Setup, sequence: u64, body: WorkerBody) -> WorkerMessage {
    WorkerMessage {
        protocol_version: PROTOCOL_VERSION,
        message_id: MessageId::new(format!("manual-{sequence}")).unwrap(),
        binding: s.binding.clone(),
        sequence,
        body,
    }
}
async fn send(s: &Setup, m: &WorkerMessage) -> Result<ControllerMessage, WorkerError> {
    s.controller
        .receive(
            &s.peer,
            &encode_worker_message(m)?,
            &s.authority,
            &SyntheticScheduler,
            &s.clock,
            Limits::default(),
        )
        .await
}
async fn executing(s: &Setup) {
    send(
        s,
        &message(
            s,
            1,
            WorkerBody::Register {
                capabilities: caps(),
            },
        ),
    )
    .await
    .unwrap();
    send(s, &message(s, 2, WorkerBody::Claim {})).await.unwrap();
    send(
        s,
        &message(
            s,
            3,
            WorkerBody::AcceptLease {
                offer_id: MessageId::new("manual-2").unwrap(),
            },
        ),
    )
    .await
    .unwrap();
}
#[tokio::test]
async fn durable_result_receipt_survives_restart_without_task_success() {
    let mut s = setup().await;
    executing(&s).await;
    let m = message(
        &s,
        4,
        WorkerBody::SubmitResult {
            outcome: ExecutionOutcome::Succeeded {
                manifest_sha256: Digest::new("2".repeat(64)).unwrap(),
            },
        },
    );
    let receipt = send(&s, &m).await.unwrap();
    s.f.store.close().await;
    let store = PlanStore::open(&s.f.path).await.unwrap();
    s.controller = DurableWorkerController::new(store);
    assert_eq!(send(&s, &m).await.unwrap(), receipt);
    let raw = s.f.raw().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_acceptances")
            .fetch_one(&raw)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_messages")
            .fetch_one(&raw)
            .await
            .unwrap(),
        4
    );
}
#[tokio::test]
async fn heartbeat_renews_authoritative_lease_once_with_current_authority() {
    let s = setup().await;
    executing(&s).await;
    s.clock.advance_to(T + 30).unwrap();
    let hb = message(&s, 4, WorkerBody::Heartbeat {});
    let receipt = send(&s, &hb).await.unwrap();
    assert_eq!(s.controller.lease_until(&s.lease).await.unwrap(), T + 120);
    s.clock.advance_to(T + 60).unwrap();
    assert_eq!(send(&s, &hb).await.unwrap(), receipt);
    assert_eq!(s.controller.lease_until(&s.lease).await.unwrap(), T + 120);
    s.clock.advance_to(T + 95).unwrap();
    s.f.store.check_lease(&s.lease, &s.clock).await.unwrap();
    s.clock.advance_to(T + 120).unwrap();
    assert_eq!(
        send(&s, &message(&s, 5, WorkerBody::Heartbeat {}))
            .await
            .unwrap_err(),
        WorkerError::Admission(AdmissionError::Expired)
    );
}
fn fence(s: &Setup, id: &str) -> FenceRequest {
    FenceRequest {
        lease: s.lease.clone(),
        request_id: RequestId::new(id).unwrap(),
        actor: owner(),
        evidence: e("synthetic-recovery-evidence"),
    }
}
async fn rows(f: &Fixture, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&f.raw().await)
        .await
        .unwrap()
}
async fn claimed(
    f: &Fixture,
    clock: &DeterministicClock,
    lease: &LeaseRef,
) -> (OutboxDispatcher, DispatchTicket) {
    let d = OutboxDispatcher::new(f.store.clone());
    let ticket = d
        .claim(
            lease,
            &owner(),
            &SyntheticScheduler,
            clock,
            Limits::default(),
        )
        .await
        .unwrap();
    (d, ticket)
}
#[tokio::test]
async fn lost_launch_ack_is_reconciled_after_restart_without_second_effect() {
    let (f, clock, lease) = admitted().await;
    let (d, ticket) = claimed(&f, &clock, &lease).await;
    let provider = SyntheticProvider::new(f.store.clone())
        .with_behavior(SyntheticLaunchBehavior::LoseAcknowledgement);
    assert_eq!(
        d.dispatch(
            &ticket,
            &provider,
            &SyntheticScheduler,
            &clock,
            Limits::default()
        )
        .await
        .unwrap(),
        DispatchState::Reconcile
    );
    assert_eq!(rows(&f, "synthetic_operations").await, 1);
    f.store.close().await;
    let store = PlanStore::open(&f.path).await.unwrap();
    let d = OutboxDispatcher::new(store.clone());
    let p = SyntheticProvider::new(store.clone());
    for _ in 0..3 {
        assert_eq!(
            d.reconcile(&lease, &p, &SyntheticScheduler, &clock, Limits::default())
                .await
                .unwrap(),
            DispatchState::Active
        );
    }
    assert!(
        DurableWorkerController::new(store)
            .binding(&lease)
            .await
            .is_ok()
    );
    assert_eq!(rows(&f, "synthetic_operations").await, 1);
    assert_eq!(rows(&f, "dispatch_observations").await, 1);
    assert_eq!(rows(&f, "worker_sessions").await, 1);
}
#[tokio::test]
async fn unknown_launch_without_effect_never_blindly_relaunches() {
    let (f, clock, lease) = admitted().await;
    let (d, ticket) = claimed(&f, &clock, &lease).await;
    let provider = SyntheticProvider::new(f.store.clone())
        .with_behavior(SyntheticLaunchBehavior::UnknownBeforeEffect);
    assert_eq!(
        d.dispatch(
            &ticket,
            &provider,
            &SyntheticScheduler,
            &clock,
            Limits::default()
        )
        .await
        .unwrap(),
        DispatchState::Reconcile
    );
    for _ in 0..3 {
        assert_eq!(
            d.dispatch(
                &ticket,
                &SyntheticProvider::new(f.store.clone()),
                &SyntheticScheduler,
                &clock,
                Limits::default()
            )
            .await
            .unwrap(),
            DispatchState::Reconcile
        );
    }
    assert_eq!(rows(&f, "synthetic_operations").await, 0);
    assert_eq!(
        f.store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap()
            .held
            .total()
            .unwrap(),
        costs().total().unwrap()
    );
}
#[tokio::test]
async fn failed_launch_before_effect_is_terminal_without_releasing_reservation() {
    let (f, clock, lease) = admitted().await;
    let (d, ticket) = claimed(&f, &clock, &lease).await;
    let p = SyntheticProvider::new(f.store.clone())
        .with_behavior(SyntheticLaunchBehavior::FailBeforeEffect);
    assert_eq!(
        d.dispatch(&ticket, &p, &SyntheticScheduler, &clock, Limits::default())
            .await
            .unwrap(),
        DispatchState::NoEffect
    );
    assert_eq!(
        d.dispatch(&ticket, &p, &SyntheticScheduler, &clock, Limits::default())
            .await
            .unwrap(),
        DispatchState::NoEffect
    );
    assert_eq!(rows(&f, "worker_sessions").await, 0);
    assert_eq!(
        f.store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap()
            .held,
        costs()
    );
}
#[tokio::test]
async fn concurrent_dispatchers_claim_one_operation_and_launch_once() {
    let (f, clock, lease) = admitted().await;
    let d1 = OutboxDispatcher::new(f.store.clone());
    let other = PlanStore::open(&f.path).await.unwrap();
    let d2 = OutboxDispatcher::new(other);
    let actor = owner();
    let (a, b) = tokio::join!(
        d1.claim(
            &lease,
            &actor,
            &SyntheticScheduler,
            &clock,
            Limits::default()
        ),
        d2.claim(
            &lease,
            &actor,
            &SyntheticScheduler,
            &clock,
            Limits::default()
        )
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    let p = SyntheticProvider::new(f.store.clone());
    let (a, b) = tokio::join!(
        d1.dispatch(&a, &p, &SyntheticScheduler, &clock, Limits::default()),
        d2.dispatch(&b, &p, &SyntheticScheduler, &clock, Limits::default())
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(
        d1.reconcile(&lease, &p, &SyntheticScheduler, &clock, Limits::default())
            .await
            .unwrap(),
        DispatchState::Active
    );
    for table in [
        "dispatch_operations",
        "dispatch_claims",
        "worker_sessions",
        "synthetic_operations",
    ] {
        assert_eq!(rows(&f, table).await, 1, "{table}");
    }
}
#[tokio::test]
async fn replay_rechecks_revocation_expiry_and_financial_safety() {
    let mut s = setup().await;
    executing(&s).await;
    let hb = message(&s, 4, WorkerBody::Heartbeat {});
    send(&s, &hb).await.unwrap();
    s.authority.revoked = true;
    assert_eq!(send(&s, &hb).await.unwrap_err(), WorkerError::Unauthorized);
    s.authority.revoked = false;
    let key = OperationKey {
        project: s.lease.task.plan.project_id.clone(),
        attempt: s.lease.attempt_id.clone(),
    };
    s.provider
        .observe(
            &key,
            1,
            ProviderState::Active,
            true,
            &e("unknown-late-exposure"),
        )
        .await
        .unwrap();
    assert_eq!(
        s.dispatcher
            .reconcile(
                &s.lease,
                &s.provider,
                &SyntheticScheduler,
                &s.clock,
                Limits::default()
            )
            .await
            .unwrap(),
        DispatchState::Blocked
    );
    assert_eq!(send(&s, &hb).await.unwrap_err(), WorkerError::FinancialRisk);
    let budget = s.f.store.financial_snapshot(&key.project).await.unwrap();
    assert_eq!(budget.uncertain_liabilities, 1);
    assert_eq!(budget.held, costs());
    s.dispatcher
        .recover_attempt(
            &fence(&s, "risk-stop"),
            RecoveryCause::Risk,
            &SyntheticScheduler,
            &s.clock,
        )
        .await
        .unwrap();
    assert!(s.f.store.check_lease(&s.lease, &s.clock).await.is_err());
    assert_eq!(rows(&s.f, "task_attempts").await, 1);
}
#[tokio::test]
async fn changed_content_replay_out_of_order_and_cross_scope_are_rejected_durably() {
    let s = setup().await;
    executing(&s).await;
    let m = message(
        &s,
        4,
        WorkerBody::Progress {
            completed: 1,
            total: 4,
            code: ProgressCode::Executing,
        },
    );
    send(&s, &m).await.unwrap();
    let mut changed = m.clone();
    changed.body = WorkerBody::Progress {
        completed: 2,
        total: 4,
        code: ProgressCode::Executing,
    };
    assert_eq!(
        send(&s, &changed).await.unwrap_err(),
        WorkerError::ReplayConflict
    );
    assert_eq!(
        send(&s, &message(&s, 6, WorkerBody::Heartbeat {}))
            .await
            .unwrap_err(),
        WorkerError::OutOfOrder { expected: 5 }
    );
    for mutate in 0..5 {
        let mut wrong = message(&s, 5, WorkerBody::Heartbeat {});
        match mutate {
            0 => wrong.binding.task.plan.project_id = ProjectId::new("other-project").unwrap(),
            1 => wrong.binding.attempt_id = AttemptId::new("other-attempt").unwrap(),
            2 => wrong.binding.worker_id = WorkerId::new("impersonated").unwrap(),
            3 => wrong.binding.context_hash = Digest::new("f".repeat(64)).unwrap(),
            _ => wrong.binding.epoch += 1,
        }
        assert!(send(&s, &wrong).await.is_err());
    }
    assert_eq!(rows(&s.f, "worker_messages").await, 4);
    assert_eq!(rows(&s.f, "task_acceptances").await, 0);
}
#[tokio::test]
async fn expired_recovery_fences_once_preserves_funds_and_rejects_stale_result() {
    let s = setup().await;
    executing(&s).await;
    s.clock.advance_to(T + 90).unwrap();
    assert_eq!(
        s.dispatcher
            .expired(&s.lease.task.plan.project_id, &s.clock, 1)
            .await
            .unwrap(),
        vec![s.lease.clone()]
    );
    for _ in 0..2 {
        assert_eq!(
            s.dispatcher
                .recover_attempt(
                    &fence(&s, "expired-stop"),
                    RecoveryCause::Expired,
                    &SyntheticScheduler,
                    &s.clock
                )
                .await
                .unwrap(),
            2
        );
    }
    assert!(
        s.dispatcher
            .expired(&s.lease.task.plan.project_id, &s.clock, 1)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        send(
            &s,
            &message(
                &s,
                4,
                WorkerBody::SubmitResult {
                    outcome: ExecutionOutcome::Succeeded {
                        manifest_sha256: Digest::new("2".repeat(64)).unwrap()
                    }
                }
            )
        )
        .await
        .is_err()
    );
    assert_eq!(
        s.f.store
            .financial_snapshot(&s.lease.task.plan.project_id)
            .await
            .unwrap()
            .held,
        costs()
    );
    assert_eq!(rows(&s.f, "worker_recoveries").await, 1);
    assert_eq!(rows(&s.f, "task_attempts").await, 1);
    // The provider still reports active after fencing. This is NOT termination.
    assert_eq!(
        s.dispatcher
            .reconcile(
                &s.lease,
                &s.provider,
                &SyntheticScheduler,
                &s.clock,
                Limits::default()
            )
            .await
            .unwrap(),
        DispatchState::Blocked
    );
}
#[tokio::test]
async fn cancellation_ack_is_not_termination_or_billing_finality() {
    let s = setup().await;
    executing(&s).await;
    s.dispatcher
        .recover_attempt(
            &fence(&s, "cancel"),
            RecoveryCause::Canceled,
            &SyntheticScheduler,
            &s.clock,
        )
        .await
        .unwrap();
    let stop = send(&s, &message(&s, 4, WorkerBody::GetStopInstruction {}))
        .await
        .unwrap();
    let ControllerBody::Stop { instruction } = stop.body else {
        panic!("expected stop")
    };
    send(
        &s,
        &message(
            &s,
            5,
            WorkerBody::StopAcknowledged {
                instruction_id: instruction.instruction_id,
            },
        ),
    )
    .await
    .unwrap();
    let k = OperationKey {
        project: s.lease.task.plan.project_id.clone(),
        attempt: s.lease.attempt_id.clone(),
    };
    s.provider
        .observe(
            &k,
            1,
            ProviderState::CancelAck,
            false,
            &e("provider-cancel-ack"),
        )
        .await
        .unwrap();
    assert_eq!(
        s.dispatcher
            .reconcile(
                &s.lease,
                &s.provider,
                &SyntheticScheduler,
                &s.clock,
                Limits::default()
            )
            .await
            .unwrap(),
        DispatchState::Blocked
    );
    let balance =
        s.f.store
            .financial_liability(&k.project, &LiabilityId::new(k.attempt.as_str()).unwrap())
            .await
            .unwrap();
    assert!(!balance.finalized);
    assert_eq!(balance.held, costs());
    assert!(
        send(&s, &message(&s, 6, WorkerBody::Heartbeat {}))
            .await
            .is_err()
    );
}
struct ExitAdapter {
    provider: SyntheticProvider,
    point: String,
}
impl LaunchAdapter for ExitAdapter {
    async fn inspect(&self, c: &LaunchCommand) -> Result<ProviderObservation, WorkerError> {
        self.provider.inspect(c).await
    }
    async fn launch(
        &self,
        a: &LaunchAuthorization,
        clock: &impl Clock,
    ) -> Result<LaunchResult, WorkerError> {
        if self.point == "before_effect" {
            std::process::exit(17);
        }
        let result = self.provider.launch(a, clock).await;
        std::process::exit(if result.is_ok() { 17 } else { 18 });
    }
}
#[tokio::test]
async fn subprocess_dispatch_crashes_recover_committed_identity_without_duplicate_launch() {
    if let Ok(mode) = std::env::var("ROLVYA_R08_DISPATCH_CRASH") {
        let path = std::env::var("ROLVYA_R08_DB").unwrap();
        let lease: LeaseRef =
            serde_json::from_str(&std::env::var("ROLVYA_R08_LEASE").unwrap()).unwrap();
        let store = PlanStore::open(path).await.unwrap();
        let d = OutboxDispatcher::new(store.clone());
        let clock = DeterministicClock::new(T).unwrap();
        if mode == "before_claim" {
            std::process::exit(17);
        }
        let ticket = d
            .claim(
                &lease,
                &owner(),
                &SyntheticScheduler,
                &clock,
                Limits::default(),
            )
            .await
            .unwrap();
        if mode == "after_claim" {
            std::process::exit(17);
        }
        let provider = SyntheticProvider::new(store).with_behavior(if mode == "after_effect" {
            SyntheticLaunchBehavior::LoseAcknowledgement
        } else {
            SyntheticLaunchBehavior::Normal
        });
        d.dispatch(
            &ticket,
            &ExitAdapter {
                provider,
                point: mode,
            },
            &SyntheticScheduler,
            &clock,
            Limits::default(),
        )
        .await
        .unwrap();
        panic!("fault point not reached");
    }
    for mode in [
        "before_claim",
        "after_claim",
        "before_effect",
        "after_effect",
        "after_ack",
    ] {
        let (f, clock, lease) = admitted().await;
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "subprocess_dispatch_crashes_recover_committed_identity_without_duplicate_launch",
                "--nocapture",
            ])
            .env("ROLVYA_R08_DISPATCH_CRASH", mode)
            .env("ROLVYA_R08_DB", &f.path)
            .env("ROLVYA_R08_LEASE", serde_json::to_string(&lease).unwrap())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(17), "{mode}");
        assert_eq!(
            rows(&f, "synthetic_operations").await,
            if matches!(mode, "after_effect" | "after_ack") {
                1
            } else {
                0
            },
            "{mode}"
        );
        let (d, ticket) = claimed(&f, &clock, &lease).await;
        let p = SyntheticProvider::new(f.store.clone());
        let state = d
            .dispatch(&ticket, &p, &SyntheticScheduler, &clock, Limits::default())
            .await
            .unwrap();
        if mode == "before_effect" {
            assert!(matches!(
                state,
                DispatchState::Launching | DispatchState::Reconcile
            ));
            assert_eq!(rows(&f, "synthetic_operations").await, 0);
        } else {
            assert_eq!(state, DispatchState::Active);
            assert_eq!(rows(&f, "synthetic_operations").await, 1);
        }
        f.store.check_integrity().await.unwrap();
    }
}
struct FailSecond<'a> {
    inner: &'a SyntheticAuthority,
    calls: std::sync::atomic::AtomicUsize,
    exit: bool,
}
impl WorkerAuthority for FailSecond<'_> {
    fn permits(
        &self,
        p: &PeerEvidence,
        b: &WorkerBinding,
        op: WorkerOperation,
        c: Option<&DeclaredCapabilities>,
        t: i64,
    ) -> bool {
        if self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) > 0 {
            if self.exit {
                std::process::exit(17);
            }
            return false;
        }
        self.inner.permits(p, b, op, c, t)
    }
}
#[tokio::test]
async fn revocation_before_message_commit_rolls_back_receipt_state_and_lease() {
    let s = setup().await;
    executing(&s).await;
    s.clock.advance_to(T + 30).unwrap();
    let hb = message(&s, 4, WorkerBody::Heartbeat {});
    let authority = FailSecond {
        inner: &s.authority,
        calls: std::sync::atomic::AtomicUsize::new(0),
        exit: false,
    };
    assert_eq!(
        s.controller
            .receive(
                &s.peer,
                &encode_worker_message(&hb).unwrap(),
                &authority,
                &SyntheticScheduler,
                &s.clock,
                Limits::default()
            )
            .await
            .unwrap_err(),
        WorkerError::Unauthorized
    );
    assert_eq!(s.controller.lease_until(&s.lease).await.unwrap(), T + 90);
    assert_eq!(rows(&s.f, "worker_messages").await, 3);
    send(&s, &hb).await.unwrap();
}
#[tokio::test]
async fn subprocess_result_crash_before_and_after_commit_preserves_exact_receipt() {
    if let Ok(mode) = std::env::var("ROLVYA_R08_MESSAGE_CRASH") {
        let path = std::env::var("ROLVYA_R08_DB").unwrap();
        let m: WorkerMessage =
            serde_json::from_str(&std::env::var("ROLVYA_R08_MESSAGE").unwrap()).unwrap();
        let store = PlanStore::open(path).await.unwrap();
        let controller = DurableWorkerController::new(store);
        let peer = PeerEvidence {
            reference: e("peer-attempt-a"),
        };
        let mut authority = SyntheticAuthority::default();
        authority.grant(&m.binding, &peer);
        let clock = DeterministicClock::new(T).unwrap();
        if mode == "before_commit" {
            controller
                .receive(
                    &peer,
                    &encode_worker_message(&m).unwrap(),
                    &FailSecond {
                        inner: &authority,
                        calls: std::sync::atomic::AtomicUsize::new(0),
                        exit: true,
                    },
                    &SyntheticScheduler,
                    &clock,
                    Limits::default(),
                )
                .await
                .unwrap();
            panic!("fault point not reached");
        }
        controller
            .receive(
                &peer,
                &encode_worker_message(&m).unwrap(),
                &authority,
                &SyntheticScheduler,
                &clock,
                Limits::default(),
            )
            .await
            .unwrap();
        std::process::exit(17);
    }
    for mode in ["before_commit", "after_commit"] {
        let s = setup().await;
        executing(&s).await;
        let m = message(
            &s,
            4,
            WorkerBody::SubmitResult {
                outcome: ExecutionOutcome::Succeeded {
                    manifest_sha256: Digest::new("2".repeat(64)).unwrap(),
                },
            },
        );
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "subprocess_result_crash_before_and_after_commit_preserves_exact_receipt",
                "--nocapture",
            ])
            .env("ROLVYA_R08_MESSAGE_CRASH", mode)
            .env("ROLVYA_R08_DB", &s.f.path)
            .env("ROLVYA_R08_MESSAGE", serde_json::to_string(&m).unwrap())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(17));
        assert_eq!(
            rows(&s.f, "worker_messages").await,
            if mode == "before_commit" { 3 } else { 4 }
        );
        let receipt = send(&s, &m).await.unwrap();
        assert_eq!(send(&s, &m).await.unwrap(), receipt);
        assert_eq!(rows(&s.f, "worker_messages").await, 4);
        assert_eq!(rows(&s.f, "task_acceptances").await, 0);
    }
}
#[tokio::test]
async fn heartbeat_and_fence_serialize_without_reviving_execution() {
    let s = setup().await;
    executing(&s).await;
    s.clock.advance_to(T + 30).unwrap();
    let m = message(&s, 4, WorkerBody::Heartbeat {});
    let r = fence(&s, "race-stop");
    let (heartbeat, stop) = tokio::join!(
        send(&s, &m),
        s.dispatcher
            .recover_attempt(&r, RecoveryCause::Canceled, &SyntheticScheduler, &s.clock)
    );
    assert_eq!(stop.unwrap(), 2);
    if heartbeat.is_ok() {
        assert_eq!(s.controller.lease_until(&s.lease).await.unwrap(), T + 120);
    }
    assert_eq!(
        s.f.store.check_lease(&s.lease, &s.clock).await.unwrap_err(),
        AdmissionError::StaleFence
    );
    assert!(send(&s, &m).await.is_err());
    assert_eq!(
        s.f.store
            .financial_snapshot(&s.lease.task.plan.project_id)
            .await
            .unwrap()
            .held,
        costs()
    );
}
struct InspectOverride {
    observation: ProviderObservation,
}
impl LaunchAdapter for InspectOverride {
    async fn inspect(&self, _: &LaunchCommand) -> Result<ProviderObservation, WorkerError> {
        Ok(self.observation.clone())
    }
    async fn launch(
        &self,
        _: &LaunchAuthorization,
        _: &impl Clock,
    ) -> Result<LaunchResult, WorkerError> {
        panic!("reconciliation must not launch")
    }
}
#[tokio::test]
async fn reordered_provider_observations_cannot_resurrect_old_execution_state() {
    let s = setup().await;
    let raw = s.f.raw().await;
    let text: String = sqlx::query_scalar("SELECT command FROM dispatch_operations")
        .fetch_one(&raw)
        .await
        .unwrap();
    let cmd: LaunchCommand = serde_json::from_str(&text).unwrap();
    let old = s.provider.inspect(&cmd).await.unwrap();
    s.provider
        .observe(
            &old.key,
            1,
            ProviderState::CancelAck,
            false,
            &e("cancel-observation"),
        )
        .await
        .unwrap();
    assert_eq!(
        s.dispatcher
            .reconcile(
                &s.lease,
                &s.provider,
                &SyntheticScheduler,
                &s.clock,
                Limits::default()
            )
            .await
            .unwrap(),
        DispatchState::Reconcile
    );
    assert_eq!(
        s.dispatcher
            .reconcile(
                &s.lease,
                &InspectOverride {
                    observation: old.clone()
                },
                &SyntheticScheduler,
                &s.clock,
                Limits::default()
            )
            .await
            .unwrap(),
        DispatchState::Reconcile
    );
    let mut conflict = old;
    conflict.state = ProviderState::Finished;
    assert_eq!(
        s.dispatcher
            .reconcile(
                &s.lease,
                &InspectOverride {
                    observation: conflict
                },
                &SyntheticScheduler,
                &s.clock,
                Limits::default()
            )
            .await
            .unwrap_err(),
        WorkerError::ReplayConflict
    );
}
#[tokio::test]
async fn repeated_result_and_cancel_race_never_completes_task_or_releases_hold() {
    let s = setup().await;
    executing(&s).await;
    let m = message(
        &s,
        4,
        WorkerBody::SubmitResult {
            outcome: ExecutionOutcome::Succeeded {
                manifest_sha256: Digest::new("2".repeat(64)).unwrap(),
            },
        },
    );
    let r = fence(&s, "result-race");
    let (result, cancel) = tokio::join!(
        send(&s, &m),
        s.dispatcher
            .recover_attempt(&r, RecoveryCause::Canceled, &SyntheticScheduler, &s.clock)
    );
    cancel.unwrap();
    assert!(send(&s, &m).await.is_err());
    assert_eq!(
        rows(&s.f, "worker_messages").await,
        if result.is_ok() { 4 } else { 3 }
    );
    assert_eq!(rows(&s.f, "task_acceptances").await, 0);
    assert_eq!(
        s.f.store
            .financial_snapshot(&s.lease.task.plan.project_id)
            .await
            .unwrap()
            .held,
        costs()
    );
}
#[tokio::test]
async fn sqlite_message_faults_roll_back_all_authority_relevant_writes() {
    for table in [
        "worker_events",
        "worker_sessions",
        "worker_messages",
        "worker_receipts",
        "worker_leases",
    ] {
        let s = setup().await;
        executing(&s).await;
        s.clock.advance_to(T + 30).unwrap();
        let raw = s.f.raw().await;
        let operation = if matches!(table, "worker_sessions" | "worker_leases") {
            "UPDATE"
        } else {
            "INSERT"
        };
        sqlx::query(&format!("CREATE TRIGGER injected BEFORE {operation} ON {table} BEGIN SELECT RAISE(ABORT,'injected'); END")).execute(&raw).await.unwrap();
        assert!(
            send(&s, &message(&s, 4, WorkerBody::Heartbeat {}))
                .await
                .is_err(),
            "{table}"
        );
        assert_eq!(
            s.controller.lease_until(&s.lease).await.unwrap(),
            T + 90,
            "{table}"
        );
        assert_eq!(rows(&s.f, "worker_messages").await, 3, "{table}");
        sqlx::query("DROP TRIGGER injected")
            .execute(&raw)
            .await
            .unwrap();
        send(&s, &message(&s, 4, WorkerBody::Heartbeat {}))
            .await
            .unwrap();
    }
}
#[tokio::test]
async fn outbox_claim_faults_rollback_claim_and_control_state_without_effect() {
    for table in ["dispatch_claims", "worker_events", "dispatch_operations"] {
        let (f, clock, lease) = admitted().await;
        let raw = f.raw().await;
        sqlx::query(&format!("CREATE TRIGGER injected BEFORE INSERT ON {table} BEGIN SELECT RAISE(ABORT,'injected'); END")).execute(&raw).await.unwrap();
        assert!(
            OutboxDispatcher::new(f.store.clone())
                .claim(
                    &lease,
                    &owner(),
                    &SyntheticScheduler,
                    &clock,
                    Limits::default()
                )
                .await
                .is_err()
        );
        for name in [
            "dispatch_claims",
            "worker_events",
            "dispatch_operations",
            "synthetic_operations",
        ] {
            assert_eq!(rows(&f, name).await, 0, "{table}: {name}");
        }
        assert_eq!(rows(&f, "task_attempts").await, 1);
        assert_eq!(rows(&f, "dispatch_outbox").await, 1);
    }
}
#[tokio::test]
async fn pending_batches_and_expired_scans_reject_unbounded_inputs() {
    let (f, clock, lease) = admitted().await;
    let d = OutboxDispatcher::new(f.store.clone());
    let p = &lease.task.plan.project_id;
    assert_eq!(
        d.pending(p, 0, 1).await.unwrap(),
        vec![DispatchIntent {
            sequence: 1,
            lease: lease.clone()
        }]
    );
    for limit in [0, 101, u16::MAX] {
        assert_eq!(
            d.pending(p, 0, limit).await.unwrap_err(),
            WorkerError::ResourceLimit
        );
        assert_eq!(
            d.expired(p, &clock, limit).await.unwrap_err(),
            WorkerError::ResourceLimit
        );
    }
    assert!(d.pending(p, -1, 1).await.is_err());
    assert_eq!(rows(&f, "dispatch_operations").await, 0);
}
#[tokio::test]
async fn legacy_claim_requires_reconciliation_instead_of_adoption_as_unlaunched() {
    let (f, clock, lease) = admitted().await;
    f.store
        .claim_dispatch(
            &DispatchRequest {
                lease: lease.clone(),
                request_id: RequestId::new("legacy").unwrap(),
                actor: owner(),
            },
            &SyntheticScheduler,
            &clock,
            Limits::default(),
        )
        .await
        .unwrap();
    let (d, ticket) = claimed(&f, &clock, &lease).await;
    assert_eq!(d.state(&lease).await.unwrap(), DispatchState::Reconcile);
    assert_eq!(
        d.dispatch(
            &ticket,
            &SyntheticProvider::new(f.store.clone()),
            &SyntheticScheduler,
            &clock,
            Limits::default()
        )
        .await
        .unwrap(),
        DispatchState::Reconcile
    );
    assert_eq!(rows(&f, "synthetic_operations").await, 0);
}
async fn original_request(f: &Fixture, lease: &LeaseRef) -> AdmissionRequest {
    let text:String=sqlx::query_scalar("SELECT e.payload FROM task_attempts a JOIN scheduler_events e ON e.sequence=a.admission_event WHERE a.project_id=? AND a.attempt_id=?").bind(lease.task.plan.project_id.as_str()).bind(lease.attempt_id.as_str()).fetch_one(&f.raw().await).await.unwrap();
    serde_json::from_value(
        serde_json::from_str::<serde_json::Value>(&text).unwrap()["request"].clone(),
    )
    .unwrap()
}
async fn finalize(s: &Setup, evidence: &str) {
    let p = s.lease.task.plan.project_id.clone();
    let l = LiabilityId::new(s.lease.attempt_id.as_str()).unwrap();
    let balance = s.f.store.financial_liability(&p, &l).await.unwrap();
    let zero = CostBreakdown {
        ai_api: money(0),
        compute: money(0),
        storage: money(0),
        transfer: money(0),
    };
    s.f.store
        .apply_financial(
            &FinancialRequest {
                project: p,
                request_id: RequestId::new(evidence).unwrap(),
                actor: owner(),
                reason: "Synthetic final billing evidence".into(),
                command: FinancialCommand::Reconcile {
                    liability: l,
                    expected_revision: balance.revision,
                    settled: zero,
                    remaining_hold: zero,
                    remaining_provisional: zero,
                    state: ReconciliationState::Final,
                    evidence: e(evidence),
                },
            },
            &SyntheticFinance,
        )
        .await
        .unwrap();
}
#[tokio::test]
async fn unresolved_dispatch_blocks_retry_even_with_permissive_reconciliation_fixture() {
    let s = setup().await;
    s.dispatcher
        .recover_attempt(
            &fence(&s, "stop-retry"),
            RecoveryCause::Canceled,
            &SyntheticScheduler,
            &s.clock,
        )
        .await
        .unwrap();
    // Deliberately over-permissive synthetic policy is NOT production evidence.
    // Even if a caller mistakenly clears finance, durable active dispatch must block.
    finalize(&s, "synthetic-final").await;
    s.f.store
        .reconcile_fenced(
            &RecoveryRequest {
                lease: s.lease.clone(),
                request_id: RequestId::new("retry-ready").unwrap(),
                actor: owner(),
                evidence: e("synthetic-transient"),
            },
            &SyntheticScheduler,
            &s.clock,
        )
        .await
        .unwrap();
    s.clock.advance_to(T + 11).unwrap();
    let mut request = original_request(&s.f, &s.lease).await;
    request.request_id = RequestId::new("attempt-b").unwrap();
    request.expected_epoch = 2;
    request.permit.id = e("cap-attempt-b");
    assert_eq!(
        s.f.store
            .admit(&request, &SyntheticScheduler, &s.clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::RetryBlocked
    );
}
#[tokio::test]
async fn lease_renewal_clips_to_enforceable_capability_deadline_without_more_hold() {
    let s = setup_until(T + 125).await;
    executing(&s).await;
    for (seq, at, expected) in [(4, 30, 120), (5, 60, 125), (6, 100, 125)] {
        s.clock.advance_to(T + at).unwrap();
        send(&s, &message(&s, seq, WorkerBody::Heartbeat {}))
            .await
            .unwrap();
        assert_eq!(
            s.controller.lease_until(&s.lease).await.unwrap(),
            T + expected
        );
        assert_eq!(
            s.f.store
                .financial_snapshot(&s.lease.task.plan.project_id)
                .await
                .unwrap()
                .held,
            costs()
        );
    }
    s.clock.advance_to(T + 125).unwrap();
    assert!(
        send(&s, &message(&s, 7, WorkerBody::Heartbeat {}))
            .await
            .is_err()
    );
}
struct ExpiringAuthority<'a> {
    inner: &'a SyntheticAuthority,
    until: i64,
}
impl WorkerAuthority for ExpiringAuthority<'_> {
    fn permits(
        &self,
        p: &PeerEvidence,
        b: &WorkerBinding,
        op: WorkerOperation,
        c: Option<&DeclaredCapabilities>,
        t: i64,
    ) -> bool {
        t < self.until && self.inner.permits(p, b, op, c, t)
    }
}
#[tokio::test]
async fn heartbeat_does_not_renew_transport_identity_or_authenticate_id_strings() {
    let s = setup().await;
    executing(&s).await;
    s.clock.advance_to(T + 30).unwrap();
    let m = message(&s, 4, WorkerBody::Heartbeat {});
    let bytes = encode_worker_message(&m).unwrap();
    let auth = ExpiringAuthority {
        inner: &s.authority,
        until: T + 45,
    };
    s.controller
        .receive(
            &s.peer,
            &bytes,
            &auth,
            &SyntheticScheduler,
            &s.clock,
            Limits::default(),
        )
        .await
        .unwrap();
    s.clock.advance_to(T + 46).unwrap();
    assert_eq!(
        s.controller
            .receive(
                &s.peer,
                &bytes,
                &auth,
                &SyntheticScheduler,
                &s.clock,
                Limits::default()
            )
            .await
            .unwrap_err(),
        WorkerError::Unauthorized
    );
    assert_eq!(
        s.controller
            .receive(
                &s.peer,
                &bytes,
                &UnimplementedAuthentication,
                &SyntheticScheduler,
                &s.clock,
                Limits::default()
            )
            .await
            .unwrap_err(),
        WorkerError::Unauthorized
    );
    assert_eq!(s.controller.lease_until(&s.lease).await.unwrap(), T + 120);
}
#[tokio::test]
async fn durable_time_high_water_prevents_clock_regression_after_restart() {
    let mut s = setup().await;
    executing(&s).await;
    s.clock.advance_to(T + 30).unwrap();
    send(&s, &message(&s, 4, WorkerBody::Heartbeat {}))
        .await
        .unwrap();
    s.clock = DeterministicClock::new(T + 20).unwrap();
    assert_eq!(
        send(&s, &message(&s, 5, WorkerBody::Heartbeat {}))
            .await
            .unwrap_err(),
        WorkerError::InvalidTime
    );
    assert_eq!(s.controller.lease_until(&s.lease).await.unwrap(), T + 120);
}
struct JumpClock(std::sync::atomic::AtomicUsize);
impl Clock for JumpClock {
    fn now(&self) -> Result<i64, AdmissionError> {
        Ok(
            if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                T + 30
            } else {
                T + 100
            },
        )
    }
}
#[tokio::test]
async fn renewal_cannot_commit_after_old_lease_has_expired() {
    let s = setup().await;
    executing(&s).await;
    let m = message(&s, 4, WorkerBody::Heartbeat {});
    assert_eq!(
        s.controller
            .receive(
                &s.peer,
                &encode_worker_message(&m).unwrap(),
                &s.authority,
                &SyntheticScheduler,
                &JumpClock(std::sync::atomic::AtomicUsize::new(0)),
                Limits::default()
            )
            .await
            .unwrap_err(),
        WorkerError::Admission(AdmissionError::Expired)
    );
    assert_eq!(s.controller.lease_until(&s.lease).await.unwrap(), T + 90);
    assert_eq!(rows(&s.f, "worker_messages").await, 3);
}
#[tokio::test]
async fn deterministic_r07_fake_worker_runs_through_durable_boundaries() {
    let s = setup().await;
    let mut script = FakeWorker::new(s.binding.clone(), caps(), 42)
        .unwrap()
        .script(FakeBehavior::Duplicate, T)
        .unwrap();
    let mut receipts = Vec::new();
    script.events.sort_by_key(|e| e.at);
    for event in script.events {
        s.clock.advance_to(event.at).unwrap();
        if let SimulatedInput::Message(bytes) = event.input {
            receipts.push(
                s.controller
                    .receive(
                        &s.peer,
                        &bytes,
                        &s.authority,
                        &SyntheticScheduler,
                        &s.clock,
                        Limits::default(),
                    )
                    .await
                    .unwrap(),
            );
        }
    }
    assert_eq!(receipts.len(), 8);
    assert_eq!(rows(&s.f, "worker_messages").await, 6);
    assert!(matches!(
        s.controller.received_outcome(&s.lease).await.unwrap(),
        Some(ExecutionOutcome::Succeeded { .. })
    ));
    assert_eq!(rows(&s.f, "tasks").await, 3);
    assert_eq!(rows(&s.f, "task_acceptances").await, 0);
    assert_eq!(
        s.f.store
            .financial_snapshot(&s.lease.task.plan.project_id)
            .await
            .unwrap()
            .ceiling,
        money(10000)
    );
    let artifact = script.artifact.unwrap();
    use sha2::{Digest as _, Sha256};
    assert_eq!(
        artifact.sha256.as_str(),
        format!("{:x}", Sha256::digest(&artifact.bytes))
    );
}
struct RetentionPermit(bool);
impl RetentionAuthority for RetentionPermit {
    fn permits(&self, _: &ProjectId, _: i64) -> bool {
        self.0
    }
}
#[tokio::test]
async fn retention_compaction_is_bounded_permissioned_and_keeps_identity_evidence() {
    let s = setup().await;
    executing(&s).await;
    let key = OperationKey {
        project: s.lease.task.plan.project_id.clone(),
        attempt: s.lease.attempt_id.clone(),
    };
    assert_eq!(
        s.controller
            .compact_receipts(&key.project, &RetentionPermit(true), &s.clock, 100)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        s.controller
            .compact_receipts(&key.project, &RetentionPermit(false), &s.clock, 100)
            .await
            .unwrap_err(),
        WorkerError::Unauthorized
    );
    s.provider
        .observe(
            &key,
            1,
            ProviderState::Finished,
            false,
            &e("termination-evidence"),
        )
        .await
        .unwrap();
    s.dispatcher
        .reconcile(
            &s.lease,
            &s.provider,
            &SyntheticScheduler,
            &s.clock,
            Limits::default(),
        )
        .await
        .unwrap();
    s.dispatcher
        .recover_attempt(
            &fence(&s, "retention-stop"),
            RecoveryCause::Canceled,
            &SyntheticScheduler,
            &s.clock,
        )
        .await
        .unwrap();
    s.clock.advance_to(T + 1096 * 86400).unwrap();
    // Outstanding finance still prevents cleanup after the minimum age.
    assert_eq!(
        s.controller
            .compact_receipts(&key.project, &RetentionPermit(true), &s.clock, 100)
            .await
            .unwrap(),
        0
    );
    finalize(&s, "final-retention").await;
    assert_eq!(
        s.controller
            .compact_receipts(&key.project, &RetentionPermit(true), &s.clock, 1)
            .await
            .unwrap(),
        1
    );
    assert_eq!(rows(&s.f, "worker_receipts").await, 2);
    assert_eq!(rows(&s.f, "worker_messages").await, 3);
    assert_eq!(
        s.controller
            .compact_receipts(&key.project, &RetentionPermit(true), &s.clock, 100)
            .await
            .unwrap(),
        2
    );
    assert_eq!(rows(&s.f, "worker_messages").await, 3);
    assert_eq!(rows(&s.f, "worker_compactions").await, 3);
    assert!(
        send(
            &s,
            &message(
                &s,
                1,
                WorkerBody::Register {
                    capabilities: caps()
                }
            )
        )
        .await
        .is_err()
    );
}
#[tokio::test]
async fn v3_upgrade_backfills_leases_preserves_claims_and_failed_v4_is_atomic() {
    for failing in [false, true] {
        let (f, clock, lease) = admitted().await;
        f.store
            .claim_dispatch(
                &DispatchRequest {
                    lease: lease.clone(),
                    request_id: RequestId::new("legacy").unwrap(),
                    actor: owner(),
                },
                &SyntheticScheduler,
                &clock,
                Limits::default(),
            )
            .await
            .unwrap();
        f.store.close().await;
        // Remove only R08's empty schema from this disposable fixture, yielding the
        // unchanged published v3 schema with a real R06 attempt, hold and claim.
        let raw = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&f.path)
                .foreign_keys(false),
        )
        .await
        .unwrap();
        sqlx::query("DROP TRIGGER initialize_worker_lease")
            .execute(&raw)
            .await
            .unwrap();
        for table in [
            "worker_compactions",
            "worker_receipts",
            "worker_messages",
            "worker_sessions",
            "worker_recoveries",
            "dispatch_observations",
            "synthetic_operations",
            "dispatch_operations",
            "worker_leases",
            "worker_events",
        ] {
            sqlx::query(&format!("DROP TABLE {table}"))
                .execute(&raw)
                .await
                .unwrap();
        }
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version=4")
            .execute(&raw)
            .await
            .unwrap();
        if failing {
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
                (
                    "0003_scheduler_admission.sql",
                    include_str!("../migrations/0003_scheduler_admission.sql"),
                ),
            ] {
                std::fs::write(scripts.path().join(name), sql).unwrap();
            }
            std::fs::write(
                scripts.path().join("0004_durable_worker_control.sql"),
                format!(
                    "{}\nSELECT missing FROM absent;",
                    include_str!("../migrations/0004_durable_worker_control.sql")
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
                    "SELECT count(*) FROM sqlite_schema WHERE name='worker_leases'"
                )
                .fetch_one(&raw)
                .await
                .unwrap(),
                0
            );
        }
        raw.close().await;
        let store = PlanStore::open(&f.path).await.unwrap();
        store.check_lease(&lease, &clock).await.unwrap();
        let controller = DurableWorkerController::new(store.clone());
        assert_eq!(controller.lease_until(&lease).await.unwrap(), T + 90);
        assert_eq!(
            store
                .financial_snapshot(&lease.task.plan.project_id)
                .await
                .unwrap()
                .held,
            costs()
        );
        let d = OutboxDispatcher::new(store.clone());
        let ticket = d
            .claim(
                &lease,
                &owner(),
                &SyntheticScheduler,
                &clock,
                Limits::default(),
            )
            .await
            .unwrap();
        assert_eq!(
            d.dispatch(
                &ticket,
                &SyntheticProvider::new(store.clone()),
                &SyntheticScheduler,
                &clock,
                Limits::default()
            )
            .await
            .unwrap(),
            DispatchState::Reconcile
        );
        store.check_integrity().await.unwrap();
    }
}
#[tokio::test]
async fn sqlite_contention_cannot_issue_receipt_or_change_lease() {
    let s = setup().await;
    executing(&s).await;
    s.clock.advance_to(T + 30).unwrap();
    let raw = s.f.raw().await;
    let tx = raw.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let result = send(&s, &message(&s, 4, WorkerBody::Heartbeat {})).await;
    assert_eq!(result.unwrap_err(), WorkerError::Storage(StoreError::Busy));
    assert_eq!(s.controller.lease_until(&s.lease).await.unwrap(), T + 90);
    assert_eq!(rows(&s.f, "worker_messages").await, 3);
    tx.rollback().await.unwrap();
    send(&s, &message(&s, 4, WorkerBody::Heartbeat {}))
        .await
        .unwrap();
}
#[tokio::test]
async fn provider_digest_conflict_cannot_become_a_second_launch() {
    let (f, clock, lease) = admitted().await;
    let (d, ticket) = claimed(&f, &clock, &lease).await;
    let raw = f.raw().await;
    sqlx::query("INSERT INTO synthetic_operations VALUES('project','attempt-a',?,'active',0,1,'conflicting-payload')").bind("0".repeat(64)).execute(&raw).await.unwrap();
    assert_eq!(
        d.dispatch(
            &ticket,
            &SyntheticProvider::new(f.store.clone()),
            &SyntheticScheduler,
            &clock,
            Limits::default()
        )
        .await
        .unwrap_err(),
        WorkerError::ReplayConflict
    );
    assert_eq!(rows(&f, "synthetic_operations").await, 1);
    assert_eq!(rows(&f, "worker_sessions").await, 0);
}
#[tokio::test]
async fn protocol_errors_and_missing_capability_do_not_consume_durable_sequence() {
    let s = setup().await;
    let mut cap = caps();
    cap.evidence = None;
    assert!(
        send(
            &s,
            &message(&s, 1, WorkerBody::Register { capabilities: cap })
        )
        .await
        .is_err()
    );
    for payload in [
        b"{".to_vec(),
        vec![b' '; MAX_MESSAGE_BYTES + 1],
        b"{\"protocol_version\":2}".to_vec(),
    ] {
        assert!(
            s.controller
                .receive(
                    &s.peer,
                    &payload,
                    &s.authority,
                    &SyntheticScheduler,
                    &s.clock,
                    Limits::default()
                )
                .await
                .is_err()
        );
    }
    assert_eq!(rows(&s.f, "worker_messages").await, 0);
    executing(&s).await;
}
#[tokio::test]
async fn reconciled_retries_keep_three_attempt_cap_and_delays_after_restart() {
    let mut s = setup().await;
    let mut request = original_request(&s.f, &s.lease).await;
    for index in 1..=3 {
        let key = OperationKey {
            project: s.lease.task.plan.project_id.clone(),
            attempt: s.lease.attempt_id.clone(),
        };
        s.provider
            .observe(
                &key,
                1,
                ProviderState::Finished,
                false,
                &e(&format!("finished-{index}")),
            )
            .await
            .unwrap();
        s.dispatcher
            .reconcile(
                &s.lease,
                &s.provider,
                &SyntheticScheduler,
                &s.clock,
                Limits::default(),
            )
            .await
            .unwrap();
        let next = s
            .dispatcher
            .recover_attempt(
                &fence(&s, &format!("fence-{index}")),
                RecoveryCause::Canceled,
                &SyntheticScheduler,
                &s.clock,
            )
            .await
            .unwrap();
        finalize(&s, &format!("bill-{index}")).await;
        let recovery = RecoveryRequest {
            lease: s.lease.clone(),
            request_id: RequestId::new(format!("recover-{index}")).unwrap(),
            actor: owner(),
            evidence: e("synthetic-classified-transient"),
        };
        let outcome =
            s.f.store
                .reconcile_fenced(&recovery, &SyntheticScheduler, &s.clock)
                .await;
        request.expected_epoch = next;
        request.request_id = RequestId::new(format!("attempt-{}", index + 1)).unwrap();
        request.permit.id = e(&format!("cap-{}", index + 1));
        if index == 3 {
            assert_eq!(outcome.unwrap_err(), AdmissionError::RetryBlocked);
            let reopened = PlanStore::open(&s.f.path).await.unwrap();
            assert_eq!(
                reopened
                    .admit(&request, &SyntheticScheduler, &s.clock, Limits::default())
                    .await
                    .unwrap_err(),
                AdmissionError::RetryBlocked
            );
        } else {
            let after = outcome.unwrap();
            let now = s.clock.now().unwrap();
            assert!((if index == 1 { 10..=12 } else { 30..=32 }).contains(&(after - now)));
            assert_eq!(
                s.f.store
                    .admit(&request, &SyntheticScheduler, &s.clock, Limits::default())
                    .await
                    .unwrap_err(),
                AdmissionError::RetryBlocked
            );
            s.clock.advance_to(after).unwrap();
            s.lease =
                s.f.store
                    .admit(&request, &SyntheticScheduler, &s.clock, Limits::default())
                    .await
                    .unwrap()
                    .lease;
            let (_, ticket) = claimed(&s.f, &s.clock, &s.lease).await;
            s.dispatcher
                .dispatch(
                    &ticket,
                    &s.provider,
                    &SyntheticScheduler,
                    &s.clock,
                    Limits::default(),
                )
                .await
                .unwrap();
        }
    }
    assert_eq!(rows(&s.f, "task_attempts").await, 3);
    assert_eq!(rows(&s.f, "synthetic_operations").await, 3);
}
#[tokio::test]
async fn verified_sibling_artifact_survives_risk_recovery_and_restart() {
    let mut s = setup().await;
    executing(&s).await;
    let manifest = "a".repeat(64);
    send(
        &s,
        &message(
            &s,
            4,
            WorkerBody::SubmitResult {
                outcome: ExecutionOutcome::Succeeded {
                    manifest_sha256: Digest::new(&manifest).unwrap(),
                },
            },
        ),
    )
    .await
    .unwrap();
    s.f.store
        .accept_task(
            &ValidationRequest {
                task: s.lease.task.clone(),
                fingerprint: s.binding.plan_hash.as_str().into(),
                lease: Some(s.lease.clone()),
                request_id: RequestId::new("verify-a").unwrap(),
                actor: PrincipalId::new("independent-validator").unwrap(),
                evidence: e("synthetic-independent-check"),
                manifest_sha256: manifest.clone(),
            },
            &SyntheticScheduler,
            &s.clock,
            Limits::default(),
        )
        .await
        .unwrap();
    let mut request = original_request(&s.f, &s.lease).await;
    request.task.task_id = TaskId::new("task-b").unwrap();
    request.request_id = RequestId::new("attempt-b").unwrap();
    request.permit.id = e("cap-b");
    s.lease =
        s.f.store
            .admit(&request, &SyntheticScheduler, &s.clock, Limits::default())
            .await
            .unwrap()
            .lease;
    let (_, ticket) = claimed(&s.f, &s.clock, &s.lease).await;
    s.dispatcher
        .dispatch(
            &ticket,
            &s.provider,
            &SyntheticScheduler,
            &s.clock,
            Limits::default(),
        )
        .await
        .unwrap();
    s.clock.advance_to(T + 90).unwrap();
    s.dispatcher
        .recover_attempt(
            &fence(&s, "expire-b"),
            RecoveryCause::Expired,
            &SyntheticScheduler,
            &s.clock,
        )
        .await
        .unwrap();
    s.f.store.close().await;
    let store = PlanStore::open(&s.f.path).await.unwrap();
    store.check_integrity().await.unwrap();
    let raw = s.f.raw().await;
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT manifest_sha256 FROM task_acceptances WHERE task_id='task-a'"
        )
        .fetch_one(&raw)
        .await
        .unwrap(),
        manifest
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM task_runtime WHERE task_id='task-a'")
            .fetch_one(&raw)
            .await
            .unwrap(),
        "succeeded"
    );
    assert_eq!(rows(&s.f, "tasks").await, 3);
}
#[tokio::test]
async fn terminal_provider_contradiction_blocks_and_records_unknown_exposure() {
    let s = setup().await;
    let key = OperationKey {
        project: s.lease.task.plan.project_id.clone(),
        attempt: s.lease.attempt_id.clone(),
    };
    s.provider
        .observe(
            &key,
            1,
            ProviderState::Finished,
            false,
            &e("finished-first"),
        )
        .await
        .unwrap();
    s.dispatcher
        .reconcile(
            &s.lease,
            &s.provider,
            &SyntheticScheduler,
            &s.clock,
            Limits::default(),
        )
        .await
        .unwrap();
    s.provider
        .observe(
            &key,
            2,
            ProviderState::Active,
            false,
            &e("impossible-restart"),
        )
        .await
        .unwrap();
    assert_eq!(
        s.dispatcher
            .reconcile(
                &s.lease,
                &s.provider,
                &SyntheticScheduler,
                &s.clock,
                Limits::default()
            )
            .await
            .unwrap(),
        DispatchState::Blocked
    );
    assert_eq!(
        s.f.store
            .financial_snapshot(&key.project)
            .await
            .unwrap()
            .uncertain_liabilities,
        1
    );
}
#[tokio::test]
async fn bounded_recovery_pass_fences_expired_attempt_and_rescan_is_empty() {
    let s = setup().await;
    s.clock.advance_to(T + 90).unwrap();
    let results = s
        .dispatcher
        .recover_expired(
            &s.lease.task.plan.project_id,
            &owner(),
            &SyntheticScheduler,
            &s.clock,
            1,
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].lease, s.lease);
    assert_eq!(results[0].result, Ok(2));
    assert!(
        s.dispatcher
            .recover_expired(
                &s.lease.task.plan.project_id,
                &owner(),
                &SyntheticScheduler,
                &s.clock,
                1
            )
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        s.f.store
            .financial_snapshot(&s.lease.task.plan.project_id)
            .await
            .unwrap()
            .held,
        costs()
    );
}
#[tokio::test]
async fn ignored_message_writes_are_detected_before_success_receipt() {
    for table in [
        "worker_events",
        "worker_sessions",
        "worker_messages",
        "worker_receipts",
        "worker_leases",
    ] {
        let s = setup().await;
        executing(&s).await;
        s.clock.advance_to(T + 30).unwrap();
        let raw = s.f.raw().await;
        let operation = if matches!(table, "worker_sessions" | "worker_leases") {
            "UPDATE"
        } else {
            "INSERT"
        };
        sqlx::query(&format!(
            "CREATE TRIGGER ignored BEFORE {operation} ON {table} BEGIN SELECT RAISE(IGNORE); END"
        ))
        .execute(&raw)
        .await
        .unwrap();
        assert!(
            send(&s, &message(&s, 4, WorkerBody::Heartbeat {}))
                .await
                .is_err(),
            "{table}"
        );
        assert_eq!(rows(&s.f, "worker_messages").await, 3, "{table}");
        assert_eq!(
            s.controller.lease_until(&s.lease).await.unwrap(),
            T + 90,
            "{table}"
        );
    }
}
#[tokio::test]
async fn bounded_eight_worker_durable_workload_with_restart() {
    let mut f = Fixture::new().await;
    f.content.key.version = PlanVersion::new(2).unwrap();
    f.content.tasks = (0..8)
        .map(|i| task(&format!("task-{i}"), &f.content.key))
        .collect();
    let valid =
        rolvya_plan_validator::validate(&f.content, &SyntheticPolicy, Limits::default()).unwrap();
    f.revision = f
        .store
        .save_draft(&valid, Some(&f.revision), &owner(), &OwnerPolicy)
        .await
        .unwrap();
    let f = activate_and_fund(f).await;
    let clock = DeterministicClock::new(T).unwrap();
    let d = OutboxDispatcher::new(f.store.clone());
    let p = SyntheticProvider::new(f.store.clone());
    let mut controller = DurableWorkerController::new(f.store.clone());
    let mut authority = SyntheticAuthority::default();
    let mut events = Vec::new();
    let mut leases = Vec::new();
    let mut wire_bytes = 0;
    for i in 0..8 {
        let receipt = f
            .store
            .admit(
                &AdmissionRequest {
                    task: TaskRef {
                        plan: f.content.key.clone(),
                        task_id: TaskId::new(format!("task-{i}")).unwrap(),
                    },
                    fingerprint: f.revision.fingerprint().into(),
                    expected_epoch: 0,
                    request_id: RequestId::new(format!("attempt-{i}")).unwrap(),
                    actor: owner(),
                    permit: ExecutionPermit {
                        id: e(&format!("cap-{i}")),
                        provider: e("synthetic"),
                        account: e("synthetic-account"),
                        pool: e("synthetic-pool"),
                        executor: PrincipalId::new(format!("worker-{i}")).unwrap(),
                        maximum: costs(),
                        valid_until: T + 3600,
                    },
                },
                &SyntheticScheduler,
                &clock,
                Limits::default(),
            )
            .await
            .unwrap();
        let ticket = d
            .claim(
                &receipt.lease,
                &owner(),
                &SyntheticScheduler,
                &clock,
                Limits::default(),
            )
            .await
            .unwrap();
        d.dispatch(&ticket, &p, &SyntheticScheduler, &clock, Limits::default())
            .await
            .unwrap();
        let binding = controller.binding(&receipt.lease).await.unwrap();
        let peer = PeerEvidence {
            reference: e(&format!("peer-{i}")),
        };
        authority.grant(&binding, &peer);
        for event in FakeWorker::new(binding, caps(), i)
            .unwrap()
            .script(FakeBehavior::Success, T)
            .unwrap()
            .events
        {
            if let SimulatedInput::Message(bytes) = event.input {
                wire_bytes += bytes.len();
                events.push((event.at, bytes, peer.clone()));
            }
        }
        leases.push(receipt.lease);
    }
    events.sort_by_key(|e| e.0);
    assert_eq!(events.len(), 48);
    let start = std::time::Instant::now();
    for (index, (at, bytes, peer)) in events.into_iter().enumerate() {
        if index == 24 {
            controller = DurableWorkerController::new(PlanStore::open(&f.path).await.unwrap());
        }
        clock.advance_to(at).unwrap();
        controller
            .receive(
                &peer,
                &bytes,
                &authority,
                &SyntheticScheduler,
                &clock,
                Limits::default(),
            )
            .await
            .unwrap();
    }
    let elapsed = start.elapsed();
    for lease in leases {
        assert!(matches!(
            controller.received_outcome(&lease).await.unwrap(),
            Some(ExecutionOutcome::Succeeded { .. })
        ));
    }
    assert_eq!(rows(&f, "worker_messages").await, 48);
    assert_eq!(rows(&f, "synthetic_operations").await, 8);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM tasks WHERE version=2")
            .fetch_one(&f.raw().await)
            .await
            .unwrap(),
        8
    );
    assert_eq!(rows(&f, "task_acceptances").await, 0);
    println!(
        "R08 synthetic debug observation: 8 workers, 48 durable messages, {wire_bytes} wire bytes, {elapsed:?}, SQLite {}",
        sqlx::query_scalar::<_, String>("SELECT sqlite_version()")
            .fetch_one(&f.raw().await)
            .await
            .unwrap()
    );
}
struct PausedAdapter {
    provider: SyntheticProvider,
    entered: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}
impl LaunchAdapter for PausedAdapter {
    async fn inspect(&self, c: &LaunchCommand) -> Result<ProviderObservation, WorkerError> {
        self.provider.inspect(c).await
    }
    async fn launch(
        &self,
        a: &LaunchAuthorization,
        clock: &impl Clock,
    ) -> Result<LaunchResult, WorkerError> {
        self.entered.notify_one();
        self.resume.notified().await;
        self.provider.launch(a, clock).await
    }
}
#[tokio::test]
async fn competing_scan_cannot_steal_inflight_marker_or_hold_sqlite_writer() {
    let (f, clock, lease) = admitted().await;
    let (d, ticket) = claimed(&f, &clock, &lease).await;
    let p = PausedAdapter {
        provider: SyntheticProvider::new(f.store.clone()),
        entered: tokio::sync::Notify::new(),
        resume: tokio::sync::Notify::new(),
    };
    let first = d.dispatch(&ticket, &p, &SyntheticScheduler, &clock, Limits::default());
    let second = async {
        p.entered.notified().await;
        // This write proves the suspended adapter is outside the controller's lock.
        let raw = f.raw().await;
        let tx = raw.begin_with("BEGIN IMMEDIATE").await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(
            d.dispatch(
                &ticket,
                &p.provider,
                &SyntheticScheduler,
                &clock,
                Limits::default()
            )
            .await
            .unwrap(),
            DispatchState::Launching
        );
        p.resume.notify_one();
    };
    let (result, ()) = tokio::join!(first, second);
    assert_eq!(result.unwrap(), DispatchState::Active);
    assert_eq!(rows(&f, "synthetic_operations").await, 1);
}
#[tokio::test]
async fn delayed_adapter_cannot_launch_after_lease_expiry_and_fencing() {
    let (f, clock, lease) = admitted().await;
    let (d, ticket) = claimed(&f, &clock, &lease).await;
    let p = PausedAdapter {
        provider: SyntheticProvider::new(f.store.clone()),
        entered: tokio::sync::Notify::new(),
        resume: tokio::sync::Notify::new(),
    };
    let launch = d.dispatch(&ticket, &p, &SyntheticScheduler, &clock, Limits::default());
    let expire = async {
        p.entered.notified().await;
        clock.advance_to(T + 90).unwrap();
        d.recover_expired(
            &lease.task.plan.project_id,
            &owner(),
            &SyntheticScheduler,
            &clock,
            1,
        )
        .await
        .unwrap()[0]
            .result
            .clone()
            .unwrap();
        p.resume.notify_one();
    };
    let (state, ()) = tokio::join!(launch, expire);
    assert_eq!(state.unwrap(), DispatchState::Blocked);
    assert_eq!(rows(&f, "synthetic_operations").await, 0);
    assert_eq!(
        f.store
            .financial_snapshot(&lease.task.plan.project_id)
            .await
            .unwrap()
            .held,
        costs()
    );
}
