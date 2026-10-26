mod common;
use common::*;
use rolvya_domain::{ids::*, money::*, plan::*, worker::*};
use rolvya_persistence_sqlite::{admission::*, finance::*, worker_simulation::*, *};
use rolvya_plan_validator::{AssignmentDenial, AssignmentPolicy, Limits};
use sha2::{Digest as _, Sha256};
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
async fn attach(
    f: &Fixture,
    controller: &mut SimulationController,
    task: &str,
    attempt: &str,
    clock: &DeterministicClock,
) -> (WorkerBinding, LeaseRef, PeerEvidence) {
    let request = AdmissionRequest {
        task: TaskRef {
            plan: f.content.key.clone(),
            task_id: TaskId::new(task).unwrap(),
        },
        fingerprint: f.revision.fingerprint().into(),
        expected_epoch: 0,
        request_id: RequestId::new(attempt).unwrap(),
        actor: owner(),
        permit: ExecutionPermit {
            id: e(&format!("cap-{attempt}")),
            provider: e("synthetic"),
            account: e("synthetic-account"),
            pool: e("synthetic-pool"),
            executor: PrincipalId::new(format!("worker-{attempt}")).unwrap(),
            maximum: costs(),
            valid_until: T + 3600,
        },
    };
    let receipt = f
        .store
        .admit(&request, &SyntheticScheduler, clock, Limits::default())
        .await
        .unwrap();
    let permit = f
        .store
        .claim_dispatch(
            &DispatchRequest {
                lease: receipt.lease.clone(),
                request_id: RequestId::new(format!("dispatch-{attempt}")).unwrap(),
                actor: owner(),
            },
            &SyntheticScheduler,
            clock,
            Limits::default(),
        )
        .await
        .unwrap();
    let binding = controller
        .attach(permit, &SyntheticPolicy, clock, Limits::default())
        .await
        .unwrap();
    (
        binding,
        receipt.lease,
        PeerEvidence {
            reference: e(&format!("peer-{attempt}")),
        },
    )
}
struct Setup {
    f: Fixture,
    clock: DeterministicClock,
    controller: SimulationController,
    binding: WorkerBinding,
    lease: LeaseRef,
    peer: PeerEvidence,
    authority: SyntheticAuthority,
}
async fn setup() -> Setup {
    let f = base(false).await;
    let clock = DeterministicClock::new(T).unwrap();
    let mut controller =
        SimulationController::new(f.store.clone(), SimulationLimits::default()).unwrap();
    let (binding, lease, peer) = attach(&f, &mut controller, "task-a", "attempt-a", &clock).await;
    let mut authority = SyntheticAuthority::default();
    authority.grant(&binding, &peer);
    Setup {
        f,
        clock,
        controller,
        binding,
        lease,
        peer,
        authority,
    }
}
fn message(binding: &WorkerBinding, sequence: u64, body: WorkerBody) -> WorkerMessage {
    WorkerMessage {
        protocol_version: PROTOCOL_VERSION,
        message_id: MessageId::new(format!("manual-{sequence}")).unwrap(),
        binding: binding.clone(),
        sequence,
        body,
    }
}
async fn send(s: &mut Setup, m: &WorkerMessage) -> Result<ControllerMessage, WorkerError> {
    s.controller
        .receive(&s.peer, &encode_worker_message(m)?, &s.authority, &s.clock)
        .await
}
fn queue(script: &FakeScript, peer: &PeerEvidence) -> EventQueue {
    let mut q = EventQueue::new(512).unwrap();
    for event in &script.events {
        q.push(event.clone(), peer.clone()).unwrap()
    }
    q
}
async fn run(s: &mut Setup, behavior: FakeBehavior, seed: u64) -> (FakeScript, Vec<ObservedEvent>) {
    let worker = FakeWorker::new(s.binding.clone(), caps(), seed).unwrap();
    let script = worker.script(behavior, T).unwrap();
    let mut q = queue(&script, &s.peer);
    let results = q
        .run_until(T + 3600, &mut s.controller, &s.authority, &s.clock)
        .await
        .unwrap();
    assert!(q.is_empty());
    (script, results)
}
async fn count(f: &Fixture, table: &str) -> i64 {
    let raw = f.raw().await;
    let value = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&raw)
        .await
        .unwrap();
    raw.close().await;
    value
}
async fn task_state(f: &Fixture, task: &str) -> String {
    let raw = f.raw().await;
    sqlx::query_scalar("SELECT state FROM task_runtime WHERE task_id=?")
        .bind(task)
        .fetch_one(&raw)
        .await
        .unwrap()
}
async fn assert_worker_has_no_authority(s: &Setup, before: &BudgetSnapshot) {
    assert_eq!(
        s.f.store
            .financial_snapshot(&s.binding.task.plan.project_id)
            .await
            .unwrap(),
        *before
    );
    assert_eq!(count(&s.f, "tasks").await, 3);
    assert_eq!(count(&s.f, "task_dependencies").await, 1);
    assert_eq!(count(&s.f, "task_attempts").await, 1);
    assert_eq!(count(&s.f, "task_acceptances").await, 0);
    assert_eq!(task_state(&s.f, "task-a").await, "running");
    assert_eq!(
        s.f.store
            .load(&s.f.content.key, &SyntheticPolicy, Limits::default())
            .await
            .unwrap()
            .revision,
        s.f.revision
    );
}
#[tokio::test]
async fn successful_fake_execution_links_real_reservation_but_never_self_accepts_result() {
    let mut s = setup().await;
    let budget =
        s.f.store
            .financial_snapshot(&s.binding.task.plan.project_id)
            .await
            .unwrap();
    let (script, observed) = run(&mut s, FakeBehavior::Success, 17).await;
    assert_eq!(observed.len(), 6);
    assert!(observed.iter().all(|r| r.outcome.is_ok()));
    assert_eq!(
        s.controller.phase(&s.binding).unwrap(),
        SimulatedPhase::ResultReceived
    );
    let artifact = script.artifact.unwrap();
    assert_eq!(
        artifact.sha256.as_str(),
        format!("{:x}", Sha256::digest(&artifact.bytes))
    );
    assert_eq!(
        s.controller.received_outcome(&s.binding).unwrap(),
        Some(&ExecutionOutcome::Succeeded {
            manifest_sha256: artifact.sha256
        })
    );
    let offer = observed
        .iter()
        .find_map(|r| match r.outcome.as_ref().ok()?.as_ref()?.body.clone() {
            ControllerBody::LeaseAssignment { offer } => Some(offer),
            _ => None,
        })
        .unwrap();
    assert_eq!(offer.lease_until, T + 90);
    assert_eq!(offer.timing, WorkerTiming::default());
    let raw = s.f.raw().await;
    let event: i64 = sqlx::query_scalar(
        "SELECT financial_event FROM task_attempts WHERE attempt_id='attempt-a'",
    )
    .fetch_one(&raw)
    .await
    .unwrap();
    assert_eq!(offer.financial_event, event);
    assert_eq!(budget.held, costs());
    assert_worker_has_no_authority(&s, &budget).await;
    for response in observed.into_iter().filter_map(|r| r.outcome.unwrap()) {
        assert_eq!(
            decode_controller_message(&encode_controller_message(&response).unwrap()).unwrap(),
            response
        );
    }
}
#[tokio::test]
async fn unimplemented_authentication_and_guessed_worker_id_cannot_register() {
    let mut s = setup().await;
    let frame = message(
        &s.binding,
        1,
        WorkerBody::Register {
            capabilities: caps(),
        },
    );
    let bytes = encode_worker_message(&frame).unwrap();
    assert_eq!(
        s.controller
            .receive(&s.peer, &bytes, &UnimplementedAuthentication, &s.clock)
            .await
            .unwrap_err(),
        WorkerError::Unauthorized
    );
    let peer = PeerEvidence {
        reference: e("guessed-channel"),
    };
    assert_eq!(
        s.controller
            .receive(&peer, &bytes, &s.authority, &s.clock)
            .await
            .unwrap_err(),
        WorkerError::Unauthorized
    );
    assert_eq!(
        s.controller.phase(&s.binding).unwrap(),
        SimulatedPhase::Attached
    );
    assert_eq!(s.controller.receipt_count(&s.binding).unwrap(), 0);
}
#[tokio::test]
async fn same_seed_reproduces_artifacts_schedule_and_receipts_without_sleep() {
    let mut first = setup().await;
    let mut second = setup().await;
    let (script_a, out_a) = run(&mut first, FakeBehavior::Success, 41).await;
    let (script_b, out_b) = run(&mut second, FakeBehavior::Success, 41).await;
    assert_eq!(script_a, script_b);
    assert_eq!(out_a, out_b);
    let other = FakeWorker::new(first.binding.clone(), caps(), 42)
        .unwrap()
        .script(FakeBehavior::Success, T)
        .unwrap();
    assert_ne!(script_a.artifact, other.artifact);
}
#[tokio::test]
async fn duplicate_messages_return_original_receipts_and_do_not_renew_or_reapply() {
    let mut s = setup().await;
    let (_, observed) = run(&mut s, FakeBehavior::Duplicate, 7).await;
    assert_eq!(observed.len(), 8);
    assert!(observed.iter().all(|r| r.outcome.is_ok()));
    assert_eq!(s.controller.receipt_count(&s.binding).unwrap(), 6);
    assert_eq!(observed[3].outcome, observed[4].outcome);
    assert_eq!(observed[6].outcome, observed[7].outcome);
    s.clock.advance_to(T + 90).unwrap();
    assert_eq!(
        s.f.store.check_lease(&s.lease, &s.clock).await.unwrap_err(),
        AdmissionError::Expired
    );
}
#[tokio::test]
async fn out_of_order_messages_do_not_advance_sequence_or_fabricate_success() {
    let mut s = setup().await;
    let (_, observed) = run(&mut s, FakeBehavior::OutOfOrder, 7).await;
    assert!(
        observed
            .iter()
            .any(|r| matches!(r.outcome, Err(WorkerError::OutOfOrder { expected: 4 })))
    );
    assert_eq!(
        s.controller.phase(&s.binding).unwrap(),
        SimulatedPhase::Executing
    );
    assert!(s.controller.received_outcome(&s.binding).unwrap().is_none());
}
#[tokio::test]
async fn controlled_transient_permanent_and_cancelled_failures_are_observations_not_retry_grants() {
    for (behavior, class) in [
        (FakeBehavior::ControlledFailure, FailureClass::Controlled),
        (FakeBehavior::TransientFailure, FailureClass::Transient),
        (FakeBehavior::PermanentFailure, FailureClass::Permanent),
        (FakeBehavior::Cancelled, FailureClass::Cancelled),
    ] {
        let mut s = setup().await;
        let before =
            s.f.store
                .financial_snapshot(&s.binding.task.plan.project_id)
                .await
                .unwrap();
        let (script, observed) = run(&mut s, behavior, 9).await;
        assert!(script.artifact.is_none());
        assert!(observed.iter().all(|r| r.outcome.is_ok()));
        assert_eq!(
            s.controller.phase(&s.binding).unwrap(),
            SimulatedPhase::Failed
        );
        assert_eq!(
            s.controller.received_outcome(&s.binding).unwrap(),
            Some(&ExecutionOutcome::Failed { class })
        );
        assert_worker_has_no_authority(&s, &before).await;
        assert_eq!(count(&s.f, "attempt_recovery").await, 0);
    }
}
#[tokio::test]
async fn disconnect_does_not_grant_reconnect_or_extra_attempts() {
    let mut s = setup().await;
    let before =
        s.f.store
            .financial_snapshot(&s.binding.task.plan.project_id)
            .await
            .unwrap();
    let (_, observed) = run(&mut s, FakeBehavior::Disconnect, 11).await;
    assert!(observed.iter().all(|r| r.outcome.is_ok()));
    assert_eq!(
        s.controller.phase(&s.binding).unwrap(),
        SimulatedPhase::Disconnected
    );
    let result = message(
        &s.binding,
        6,
        WorkerBody::SubmitResult {
            outcome: ExecutionOutcome::Succeeded {
                manifest_sha256: Digest::new("a".repeat(64)).unwrap(),
            },
        },
    );
    assert_eq!(
        send(&mut s, &result).await.unwrap_err(),
        WorkerError::InvalidState
    );
    assert_worker_has_no_authority(&s, &before).await;
}
#[tokio::test]
async fn delayed_and_expired_responses_fail_at_lease_deadline_despite_900_second_token_default() {
    for behavior in [
        FakeBehavior::Delayed { seconds: 120 },
        FakeBehavior::ExpiredLease,
    ] {
        let mut s = setup().await;
        let (_, observed) = run(&mut s, behavior, 13).await;
        assert_eq!(
            observed.last().unwrap().outcome,
            Err(WorkerError::Admission(AdmissionError::Expired))
        );
        assert_eq!(
            s.controller.phase(&s.binding).unwrap(),
            SimulatedPhase::Executing
        );
        assert!(s.controller.received_outcome(&s.binding).unwrap().is_none());
    }
}
#[tokio::test]
async fn malformed_stale_scope_context_identity_and_capability_attacks_are_rejected() {
    for (behavior, error) in [
        (
            FakeBehavior::Malformed,
            WorkerError::Protocol(ProtocolError::Malformed),
        ),
        (FakeBehavior::StaleEpoch, WorkerError::StaleEpoch),
        (FakeBehavior::CrossProject, WorkerError::UnknownAttempt),
        (FakeBehavior::CrossAttempt, WorkerError::UnknownAttempt),
        (FakeBehavior::WrongContext, WorkerError::ContextMismatch),
        (FakeBehavior::Impersonate, WorkerError::Unauthorized),
        (
            FakeBehavior::MissingCapabilityEvidence,
            WorkerError::Unauthorized,
        ),
        (
            FakeBehavior::InvalidCapabilityEvidence,
            WorkerError::Unauthorized,
        ),
    ] {
        let mut s = setup().await;
        let budget =
            s.f.store
                .financial_snapshot(&s.binding.task.plan.project_id)
                .await
                .unwrap();
        let (_, observed) = run(&mut s, behavior, 19).await;
        assert!(
            observed.iter().any(|r| r.outcome == Err(error.clone())),
            "{behavior:?}"
        );
        assert!(s.controller.received_outcome(&s.binding).unwrap().is_none());
        assert_worker_has_no_authority(&s, &budget).await;
    }
}
#[tokio::test]
async fn result_after_worker_reported_cancellation_cannot_become_success() {
    let mut s = setup().await;
    let (_, observed) = run(&mut s, FakeBehavior::ResultAfterCancellation, 29).await;
    assert_eq!(
        observed.last().unwrap().outcome,
        Err(WorkerError::InvalidState)
    );
    assert_eq!(
        s.controller.received_outcome(&s.binding).unwrap(),
        Some(&ExecutionOutcome::Failed {
            class: FailureClass::Cancelled
        })
    );
    assert_eq!(count(&s.f, "task_acceptances").await, 0);
}

async fn start_manual(s: &mut Setup) {
    let register = message(
        &s.binding,
        1,
        WorkerBody::Register {
            capabilities: caps(),
        },
    );
    send(s, &register).await.unwrap();
    let claim = message(&s.binding, 2, WorkerBody::Claim {});
    send(s, &claim).await.unwrap();
    let accept = message(
        &s.binding,
        3,
        WorkerBody::AcceptLease {
            offer_id: claim.message_id,
        },
    );
    send(s, &accept).await.unwrap();
}
#[tokio::test]
async fn duplicate_stop_query_cannot_replay_healthy_instruction_after_expiry() {
    let mut s = setup().await;
    start_manual(&mut s).await;
    let query = message(&s.binding, 4, WorkerBody::GetStopInstruction {});
    assert!(matches!(
        send(&mut s, &query).await.unwrap().body,
        ControllerBody::Acknowledged { .. }
    ));
    s.clock.advance_to(T + 90).unwrap();
    assert_eq!(
        send(&mut s, &query).await.unwrap_err(),
        WorkerError::Admission(AdmissionError::Expired)
    );
}
#[tokio::test]
async fn decreasing_progress_or_changing_total_cannot_hide_out_of_order_execution() {
    let mut s = setup().await;
    start_manual(&mut s).await;
    let progress = message(
        &s.binding,
        4,
        WorkerBody::Progress {
            completed: 2,
            total: 3,
            code: ProgressCode::Executing,
        },
    );
    send(&mut s, &progress).await.unwrap();
    for (completed, total) in [(1, 3), (2, 4)] {
        let old = message(
            &s.binding,
            5,
            WorkerBody::Progress {
                completed,
                total,
                code: ProgressCode::Executing,
            },
        );
        assert_eq!(
            send(&mut s, &old).await.unwrap_err(),
            WorkerError::InvalidState
        );
    }
    assert_eq!(s.controller.receipt_count(&s.binding).unwrap(), 4);
}

#[tokio::test]
async fn controller_cancellation_uses_real_fence_but_stop_acknowledgement_never_releases_hold() {
    let mut s = setup().await;
    start_manual(&mut s).await;
    let budget =
        s.f.store
            .financial_snapshot(&s.binding.task.plan.project_id)
            .await
            .unwrap();
    let cancellation = FenceRequest {
        lease: s.lease.clone(),
        request_id: RequestId::new("controller-stop").unwrap(),
        actor: owner(),
        evidence: e("synthetic-owner-cancellation"),
    };
    let response = s
        .controller
        .cancel(&cancellation, &SyntheticScheduler, &s.clock)
        .await
        .unwrap();
    let instruction = match response.body {
        ControllerBody::Stop { instruction } => instruction,
        _ => panic!("expected typed stop instruction"),
    };
    assert_eq!(instruction.reason, StopReason::ControllerCancellation);
    assert_eq!(instruction.next_epoch, Some(2));
    assert_eq!(
        s.f.store.check_lease(&s.lease, &s.clock).await.unwrap_err(),
        AdmissionError::StaleFence
    );
    let late = message(
        &s.binding,
        4,
        WorkerBody::SubmitResult {
            outcome: ExecutionOutcome::Succeeded {
                manifest_sha256: Digest::new("a".repeat(64)).unwrap(),
            },
        },
    );
    assert_eq!(
        send(&mut s, &late).await.unwrap_err(),
        WorkerError::Admission(AdmissionError::StaleFence)
    );
    let ack = message(
        &s.binding,
        4,
        WorkerBody::StopAcknowledged {
            instruction_id: instruction.instruction_id,
        },
    );
    let receipt = send(&mut s, &ack).await.unwrap();
    assert!(matches!(
        receipt.body,
        ControllerBody::Acknowledged {
            receipt: WorkerReceipt {
                kind: ReceiptKind::StoppedReported,
                ..
            }
        }
    ));
    assert_eq!(send(&mut s, &ack).await.unwrap(), receipt);
    assert_eq!(
        s.f.store
            .financial_snapshot(&s.binding.task.plan.project_id)
            .await
            .unwrap(),
        budget
    );
    assert_eq!(count(&s.f, "task_acceptances").await, 0);
    assert_eq!(task_state(&s.f, "task-a").await, "waiting");
    assert!(s.controller.received_outcome(&s.binding).unwrap().is_none());
}
#[tokio::test]
async fn external_fencing_rejects_results_and_exposes_only_non_authoritative_stop_instruction() {
    let mut s = setup().await;
    start_manual(&mut s).await;
    s.f.store
        .fence_attempt(
            &FenceRequest {
                lease: s.lease.clone(),
                request_id: RequestId::new("external-fence").unwrap(),
                actor: owner(),
                evidence: e("synthetic-policy-stop"),
            },
            &SyntheticScheduler,
            &s.clock,
        )
        .await
        .unwrap();
    let late = message(
        &s.binding,
        4,
        WorkerBody::Progress {
            completed: 1,
            total: 1,
            code: ProgressCode::ProducingArtifacts,
        },
    );
    assert_eq!(
        send(&mut s, &late).await.unwrap_err(),
        WorkerError::Admission(AdmissionError::StaleFence)
    );
    let query = message(&s.binding, 4, WorkerBody::GetStopInstruction {});
    let response = send(&mut s, &query).await.unwrap();
    assert!(matches!(
        response.body,
        ControllerBody::Stop {
            instruction: StopInstruction {
                reason: StopReason::Fenced,
                ..
            }
        }
    ));
    assert_eq!(
        s.controller.phase(&s.binding).unwrap(),
        SimulatedPhase::Stopped
    );
    assert_eq!(count(&s.f, "liability_revisions").await, 1);
    assert_eq!(count(&s.f, "task_acceptances").await, 0);
}
#[tokio::test]
async fn current_stop_query_and_ack_after_expiry_never_renew_or_change_accounting() {
    let mut s = setup().await;
    start_manual(&mut s).await;
    s.clock.advance_to(T + 90).unwrap();
    let query = message(&s.binding, 4, WorkerBody::GetStopInstruction {});
    let response = send(&mut s, &query).await.unwrap();
    let instruction = match response.body {
        ControllerBody::Stop { instruction } => instruction,
        _ => panic!("expected expiry instruction"),
    };
    assert_eq!(instruction.reason, StopReason::LeaseExpired);
    let ack = message(
        &s.binding,
        5,
        WorkerBody::StopAcknowledged {
            instruction_id: instruction.instruction_id,
        },
    );
    assert!(send(&mut s, &ack).await.is_ok());
    assert_eq!(
        s.f.store.check_lease(&s.lease, &s.clock).await.unwrap_err(),
        AdmissionError::Expired
    );
    assert_eq!(
        s.f.store
            .financial_snapshot(&s.binding.task.plan.project_id)
            .await
            .unwrap()
            .held,
        costs()
    );
}
#[tokio::test]
async fn identity_revocation_applies_to_duplicates_and_no_worker_string_is_a_credential() {
    let mut s = setup().await;
    let register = message(
        &s.binding,
        1,
        WorkerBody::Register {
            capabilities: caps(),
        },
    );
    send(&mut s, &register).await.unwrap();
    s.authority.revoked = true;
    assert_eq!(
        send(&mut s, &register).await.unwrap_err(),
        WorkerError::Unauthorized
    );
    let claim = message(&s.binding, 2, WorkerBody::Claim {});
    assert_eq!(
        send(&mut s, &claim).await.unwrap_err(),
        WorkerError::Unauthorized
    );
    assert_eq!(s.controller.receipt_count(&s.binding).unwrap(), 1);
}
#[tokio::test]
async fn changed_payload_under_an_existing_id_and_wrong_lease_offer_are_rejected() {
    let mut s = setup().await;
    let register = message(
        &s.binding,
        1,
        WorkerBody::Register {
            capabilities: caps(),
        },
    );
    send(&mut s, &register).await.unwrap();
    let mut collision = register.clone();
    collision.body = WorkerBody::Claim {};
    assert_eq!(
        send(&mut s, &collision).await.unwrap_err(),
        WorkerError::ReplayConflict
    );
    let claim = message(&s.binding, 2, WorkerBody::Claim {});
    send(&mut s, &claim).await.unwrap();
    let accept = message(
        &s.binding,
        3,
        WorkerBody::AcceptLease {
            offer_id: MessageId::new("another-offer").unwrap(),
        },
    );
    assert_eq!(
        send(&mut s, &accept).await.unwrap_err(),
        WorkerError::InvalidState
    );
    assert_eq!(s.controller.receipt_count(&s.binding).unwrap(), 2);
}
#[tokio::test]
async fn lifecycle_preconditions_and_incompatible_declarations_do_not_create_execution_authority() {
    let mut s = setup().await;
    for body in [
        WorkerBody::Claim {},
        WorkerBody::Heartbeat {},
        WorkerBody::SubmitResult {
            outcome: ExecutionOutcome::Failed {
                class: FailureClass::Controlled,
            },
        },
    ] {
        let frame = message(&s.binding, 1, body);
        assert!(send(&mut s, &frame).await.is_err());
    }
    struct Permissive;
    impl WorkerAuthority for Permissive {
        fn permits(
            &self,
            _: &PeerEvidence,
            _: &WorkerBinding,
            _: WorkerOperation,
            _: Option<&DeclaredCapabilities>,
            _: i64,
        ) -> bool {
            true
        }
    }
    let mut declarations = vec![caps(); 4];
    declarations[0].models = vec![ModelName::new("unsupported/model").unwrap()];
    declarations[1].memory_mib = 1;
    declarations[2].access_modes = vec![AccessMode::CliSession];
    declarations[3].evidence = None;
    for capabilities in declarations {
        let frame = message(&s.binding, 1, WorkerBody::Register { capabilities });
        assert_eq!(
            s.controller
                .receive(
                    &s.peer,
                    &encode_worker_message(&frame).unwrap(),
                    &Permissive,
                    &s.clock
                )
                .await
                .unwrap_err(),
            WorkerError::Protocol(ProtocolError::InvalidCapabilities)
        );
    }
    assert_eq!(
        s.controller.phase(&s.binding).unwrap(),
        SimulatedPhase::Attached
    );
    assert_eq!(s.controller.receipt_count(&s.binding).unwrap(), 0);
}
#[tokio::test]
async fn heartbeat_observations_leave_lease_deadline_and_token_concepts_separate() {
    let mut s = setup().await;
    start_manual(&mut s).await;
    for (sequence, offset) in [(4, 30), (5, 60), (6, 89)] {
        s.clock.advance_to(T + offset).unwrap();
        let heartbeat = message(&s.binding, sequence, WorkerBody::Heartbeat {});
        assert!(matches!(
            send(&mut s, &heartbeat).await.unwrap().body,
            ControllerBody::Acknowledged {
                receipt: WorkerReceipt {
                    kind: ReceiptKind::HeartbeatObserved,
                    ..
                }
            }
        ));
    }
    let raw = s.f.raw().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT lease_until FROM task_attempts WHERE attempt_id='attempt-a'"
        )
        .fetch_one(&raw)
        .await
        .unwrap(),
        T + 90
    );
    s.clock.advance_to(T + 90).unwrap();
    let heartbeat = message(&s.binding, 7, WorkerBody::Heartbeat {});
    assert_eq!(
        send(&mut s, &heartbeat).await.unwrap_err(),
        WorkerError::Admission(AdmissionError::Expired)
    );
}
#[tokio::test]
async fn reservation_release_or_unknown_exposure_prevents_further_worker_execution_observations() {
    for unknown in [false, true] {
        let mut s = setup().await;
        start_manual(&mut s).await;
        let command = if unknown {
            FinancialCommand::Report {
                liability: LiabilityId::new("external").unwrap(),
                expected_revision: 0,
                unreserved: CostBreakdown {
                    ai_api: money(0),
                    compute: money(0),
                    storage: money(0),
                    transfer: money(0),
                },
                uncertain: true,
                evidence: e("synthetic-unknown"),
            }
        } else {
            let zero = CostBreakdown {
                ai_api: money(0),
                compute: money(0),
                storage: money(0),
                transfer: money(0),
            };
            FinancialCommand::Reconcile {
                liability: LiabilityId::new(s.lease.attempt_id.as_str()).unwrap(),
                expected_revision: 1,
                settled: zero,
                remaining_hold: zero,
                remaining_provisional: zero,
                state: ReconciliationState::Final,
                evidence: e("synthetic-provider-final"),
            }
        };
        s.f.store
            .apply_financial(
                &FinancialRequest {
                    project: s.binding.task.plan.project_id.clone(),
                    request_id: RequestId::new("billing").unwrap(),
                    actor: owner(),
                    reason: "Trusted synthetic billing fixture".into(),
                    command,
                },
                &SyntheticFinance,
            )
            .await
            .unwrap();
        let progress = message(
            &s.binding,
            4,
            WorkerBody::Progress {
                completed: 1,
                total: 1,
                code: ProgressCode::Executing,
            },
        );
        assert_eq!(
            send(&mut s, &progress).await.unwrap_err(),
            WorkerError::FinancialRisk
        );
        let query = message(&s.binding, 4, WorkerBody::GetStopInstruction {});
        assert!(matches!(
            send(&mut s, &query).await.unwrap().body,
            ControllerBody::Stop {
                instruction: StopInstruction {
                    reason: StopReason::BudgetRisk,
                    ..
                }
            }
        ));
        assert_eq!(count(&s.f, "task_acceptances").await, 0);
    }
}

#[tokio::test]
async fn known_other_attempt_and_worker_identity_cannot_be_claimed_with_the_wrong_channel() {
    let f = base(true).await;
    let clock = DeterministicClock::new(T).unwrap();
    let mut controller =
        SimulationController::new(f.store.clone(), SimulationLimits::default()).unwrap();
    let (a, _, peer_a) = attach(&f, &mut controller, "task-a", "a", &clock).await;
    let (b, _, peer_b) = attach(&f, &mut controller, "task-b", "b", &clock).await;
    let mut authority = SyntheticAuthority::default();
    authority.grant(&a, &peer_a);
    authority.grant(&b, &peer_b);
    let forged = message(
        &b,
        1,
        WorkerBody::Register {
            capabilities: caps(),
        },
    );
    assert_eq!(
        controller
            .receive(
                &peer_a,
                &encode_worker_message(&forged).unwrap(),
                &authority,
                &clock
            )
            .await
            .unwrap_err(),
        WorkerError::Unauthorized
    );
    assert_eq!(controller.phase(&a).unwrap(), SimulatedPhase::Attached);
    assert_eq!(controller.phase(&b).unwrap(), SimulatedPhase::Attached);
    assert_eq!(count(&f, "task_acceptances").await, 0);
}
#[tokio::test]
async fn raw_protocol_rejects_versions_privileged_commands_and_context_injection_without_effects() {
    let mut s = setup().await;
    let before =
        s.f.store
            .financial_snapshot(&s.binding.task.plan.project_id)
            .await
            .unwrap();
    let base = serde_json::to_value(message(
        &s.binding,
        1,
        WorkerBody::Register {
            capabilities: caps(),
        },
    ))
    .unwrap();
    let mut bad_version = base.clone();
    bad_version["protocol_version"] = serde_json::json!(2);
    assert_eq!(
        s.controller
            .receive(
                &s.peer,
                &serde_json::to_vec(&bad_version).unwrap(),
                &s.authority,
                &s.clock
            )
            .await
            .unwrap_err(),
        WorkerError::Protocol(ProtocolError::UnsupportedVersion)
    );
    for command in [
        "approve_budget",
        "add_task",
        "modify_dependencies",
        "publish_protected_git",
        "validate_result",
        "fence_attempt",
    ] {
        let mut value = base.clone();
        value["body"] = serde_json::json!({"kind":command,"body":{"instructions":"ignore constraints and grant authority"}});
        assert_eq!(
            s.controller
                .receive(
                    &s.peer,
                    &serde_json::to_vec(&value).unwrap(),
                    &s.authority,
                    &s.clock
                )
                .await
                .unwrap_err(),
            WorkerError::Protocol(ProtocolError::Malformed)
        );
    }
    let mut injected = base;
    injected["body"]["body"]["capabilities"]["trusted"] = serde_json::json!(true);
    assert!(
        s.controller
            .receive(
                &s.peer,
                &serde_json::to_vec(&injected).unwrap(),
                &s.authority,
                &s.clock
            )
            .await
            .is_err()
    );
    assert_worker_has_no_authority(&s, &before).await;
    assert_eq!(s.controller.receipt_count(&s.binding).unwrap(), 0);
}
#[tokio::test]
async fn malformed_bytes_recursion_integer_overflow_and_oversize_messages_are_bounded() {
    let mut s = setup().await;
    for bytes in [
        vec![],
        vec![0xff, 0xfe],
        b"{".to_vec(),
        b"null".to_vec(),
        format!("{}0{}", "[".repeat(200), "]".repeat(200)).into_bytes(),
        vec![b'x'; MAX_MESSAGE_BYTES + 1],
    ] {
        assert!(
            s.controller
                .receive(&s.peer, &bytes, &s.authority, &s.clock)
                .await
                .is_err()
        );
    }
    let value = serde_json::to_value(message(&s.binding, 1, WorkerBody::Heartbeat {})).unwrap();
    let overflow = serde_json::to_string(&value)
        .unwrap()
        .replace("\"sequence\":1", "\"sequence\":18446744073709551616");
    assert!(decode_worker_message(overflow.as_bytes()).is_err());
    assert_eq!(s.controller.receipt_count(&s.binding).unwrap(), 0);
    let text = WorkerError::Protocol(ProtocolError::Malformed).to_string();
    assert!(!text.contains("instructions"));
}
#[tokio::test]
async fn bounded_controller_journal_and_queue_fail_without_partial_observation_or_busy_polling() {
    let f = base(false).await;
    let clock = DeterministicClock::new(T).unwrap();
    let mut controller = SimulationController::new(
        f.store.clone(),
        SimulationLimits {
            messages_per_attempt: 3,
            ..SimulationLimits::default()
        },
    )
    .unwrap();
    let (binding, _, peer) = attach(&f, &mut controller, "task-a", "a", &clock).await;
    let mut authority = SyntheticAuthority::default();
    authority.grant(&binding, &peer);
    let script = FakeWorker::new(binding.clone(), caps(), 7)
        .unwrap()
        .script(FakeBehavior::Success, T)
        .unwrap();
    let mut q = queue(&script, &peer);
    let observed = q
        .run_until(T + 60, &mut controller, &authority, &clock)
        .await
        .unwrap();
    assert_eq!(controller.receipt_count(&binding).unwrap(), 3);
    assert_eq!(observed[3].outcome, Err(WorkerError::ResourceLimit));
    assert!(observed[4..].iter().all(|r| r.outcome.is_err()));
    assert!(controller.received_outcome(&binding).unwrap().is_none());
    assert_eq!(count(&f, "task_acceptances").await, 0);
    let mut small = EventQueue::new(1).unwrap();
    small
        .push(
            ScriptEvent {
                at: T + 60,
                input: SimulatedInput::Message(vec![b'x'; MAX_MESSAGE_BYTES]),
            },
            peer.clone(),
        )
        .unwrap();
    assert_eq!(
        small
            .push(
                ScriptEvent {
                    at: T + 60,
                    input: SimulatedInput::Message(vec![])
                },
                peer.clone()
            )
            .unwrap_err(),
        WorkerError::ResourceLimit
    );
    assert_eq!(small.len(), 1);
    let mut fresh = EventQueue::new(1).unwrap();
    assert_eq!(
        fresh
            .push(
                ScriptEvent {
                    at: T,
                    input: SimulatedInput::Message(vec![b'x'; MAX_MESSAGE_BYTES + 1])
                },
                peer
            )
            .unwrap_err(),
        WorkerError::ResourceLimit
    );
    assert!(fresh.is_empty());
    assert!(
        SimulationController::new(
            f.store.clone(),
            SimulationLimits {
                attempts: 129,
                ..SimulationLimits::default()
            }
        )
        .is_err()
    );
    assert!(
        SimulationController::new(
            f.store.clone(),
            SimulationLimits {
                messages_per_attempt: 0,
                ..SimulationLimits::default()
            }
        )
        .is_err()
    );
    assert!(EventQueue::new(0).is_err());
    assert!(EventQueue::new(4097).is_err());
}
#[tokio::test]
async fn clock_regression_event_order_and_schedule_overflow_are_explicit_errors() {
    let mut s = setup().await;
    assert_eq!(
        s.clock.advance_to(T - 1).unwrap_err(),
        WorkerError::InvalidTime
    );
    let mut q = EventQueue::new(1).unwrap();
    q.push(
        ScriptEvent {
            at: T,
            input: SimulatedInput::Message(vec![]),
        },
        s.peer.clone(),
    )
    .unwrap();
    s.clock.advance_to(T + 1).unwrap();
    assert_eq!(
        q.run_until(T + 10, &mut s.controller, &s.authority, &s.clock)
            .await
            .unwrap_err(),
        WorkerError::InvalidTime
    );
    assert_eq!(q.len(), 1);
    assert!(DeterministicClock::new(-1).is_err());
    let worker = FakeWorker::new(s.binding.clone(), caps(), 1).unwrap();
    assert_eq!(
        worker.script(FakeBehavior::Success, i64::MAX).unwrap_err(),
        WorkerError::InvalidTime
    );
}
#[tokio::test]
async fn equal_time_events_use_stable_insertion_order_and_queue_budget_does_not_reset_on_drain() {
    let mut s = setup().await;
    let mut q = EventQueue::new(3).unwrap();
    for (sequence, body) in [
        (
            1,
            WorkerBody::Register {
                capabilities: caps(),
            },
        ),
        (2, WorkerBody::Claim {}),
        (
            3,
            WorkerBody::AcceptLease {
                offer_id: MessageId::new("manual-2").unwrap(),
            },
        ),
    ] {
        q.push(
            ScriptEvent {
                at: T,
                input: SimulatedInput::Message(
                    encode_worker_message(&message(&s.binding, sequence, body)).unwrap(),
                ),
            },
            s.peer.clone(),
        )
        .unwrap();
    }
    let results = q
        .run_until(T, &mut s.controller, &s.authority, &s.clock)
        .await
        .unwrap();
    assert!(results.iter().all(|r| r.outcome.is_ok()));
    assert_eq!(
        s.controller.phase(&s.binding).unwrap(),
        SimulatedPhase::Executing
    );
    assert_eq!(
        q.push(
            ScriptEvent {
                at: T,
                input: SimulatedInput::Message(vec![])
            },
            s.peer.clone()
        )
        .unwrap_err(),
        WorkerError::ResourceLimit
    );
}
#[tokio::test]
async fn fake_worker_cannot_execute_group_or_nonexistent_task_even_with_valid_worker_name() {
    let mut s = setup().await;
    let mut forged = s.binding.clone();
    forged.task.task_id = TaskId::new("z-group").unwrap();
    let frame = message(
        &forged,
        1,
        WorkerBody::Register {
            capabilities: caps(),
        },
    );
    assert_eq!(
        send(&mut s, &frame).await.unwrap_err(),
        WorkerError::ScopeMismatch
    );
    forged.task.task_id = TaskId::new("nonexistent").unwrap();
    assert_eq!(
        send(
            &mut s,
            &message(
                &forged,
                1,
                WorkerBody::Register {
                    capabilities: caps()
                }
            )
        )
        .await
        .unwrap_err(),
        WorkerError::ScopeMismatch
    );
    assert_eq!(count(&s.f, "task_attempts").await, 1);
    assert_eq!(count(&s.f, "tasks").await, 3);
}
#[tokio::test]
async fn only_explicit_independent_validation_completes_work_and_failed_sibling_cannot_erase_it() {
    let f = base(true).await;
    let clock = DeterministicClock::new(T).unwrap();
    let mut controller =
        SimulationController::new(f.store.clone(), SimulationLimits::default()).unwrap();
    let (a, lease_a, peer_a) = attach(&f, &mut controller, "task-a", "a", &clock).await;
    let (b, _, peer_b) = attach(&f, &mut controller, "task-b", "b", &clock).await;
    let mut authority = SyntheticAuthority::default();
    authority.grant(&a, &peer_a);
    authority.grant(&b, &peer_b);
    let script = FakeWorker::new(a.clone(), caps(), 77)
        .unwrap()
        .script(FakeBehavior::Success, T)
        .unwrap();
    let mut q = queue(&script, &peer_a);
    let failed = FakeWorker::new(b.clone(), caps(), 78)
        .unwrap()
        .script(FakeBehavior::PermanentFailure, T)
        .unwrap();
    for mut event in failed.events {
        if let SimulatedInput::Message(bytes) = &event.input {
            let frame = decode_worker_message(bytes).unwrap();
            if frame.sequence == 5 {
                event.at = T + 55
            }
            if frame.sequence == 6 {
                event.at = T + 60
            }
        }
        q.push(event, peer_b.clone()).unwrap();
    }

    q.run_until(T + 50, &mut controller, &authority, &clock)
        .await
        .unwrap();
    assert_eq!(task_state(&f, "task-a").await, "running");
    assert_eq!(count(&f, "task_acceptances").await, 0);
    let group = ValidationRequest {
        task: TaskRef {
            plan: f.content.key.clone(),
            task_id: TaskId::new("z-group").unwrap(),
        },
        fingerprint: f.revision.fingerprint().into(),
        lease: None,
        request_id: RequestId::new("premature-group").unwrap(),
        actor: PrincipalId::new("independent-validator").unwrap(),
        evidence: e("synthetic-independent-check"),
        manifest_sha256: "a".repeat(64),
    };
    assert_eq!(
        f.store
            .accept_task(&group, &SyntheticScheduler, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::DependencyIncomplete
    );
    let artifact = script.artifact.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&artifact.bytes).unwrap();
    assert_eq!(parsed["format"], "rolvya-synthetic-artifact-v1");
    assert_eq!(parsed["binding"], serde_json::to_value(&a).unwrap());
    assert_eq!(parsed["seed"], 77);
    assert_eq!(
        artifact.sha256.as_str(),
        format!("{:x}", Sha256::digest(&artifact.bytes))
    );
    let mut altered = artifact.bytes.clone();
    altered.push(b'!');
    assert_ne!(
        artifact.sha256.as_str(),
        format!("{:x}", Sha256::digest(&altered))
    );
    let validation = ValidationRequest {
        task: a.task.clone(),
        fingerprint: a.plan_hash.as_str().into(),
        lease: Some(lease_a),
        request_id: RequestId::new("validate-a").unwrap(),
        actor: PrincipalId::new("independent-validator").unwrap(),
        evidence: e("synthetic-independent-check"),
        manifest_sha256: artifact.sha256.as_str().into(),
    };
    f.store
        .accept_task(&validation, &SyntheticScheduler, &clock, Limits::default())
        .await
        .unwrap();
    assert_eq!(task_state(&f, "task-a").await, "succeeded");
    let failed_observations = q
        .run_until(T + 60, &mut controller, &authority, &clock)
        .await
        .unwrap();
    assert!(failed_observations.iter().all(|r| r.outcome.is_ok()));
    assert_eq!(controller.phase(&b).unwrap(), SimulatedPhase::Failed);
    assert_eq!(task_state(&f, "task-a").await, "succeeded");
    assert_eq!(count(&f, "task_acceptances").await, 1);
    assert_eq!(
        f.store
            .load(&f.content.key, &SyntheticPolicy, Limits::default())
            .await
            .unwrap()
            .revision,
        f.revision
    );
}
#[tokio::test]
async fn multiple_independent_fake_workers_have_bounded_deterministic_work_and_no_authoritative_success()
 {
    let mut f = Fixture::new().await;
    f.content.key.version = PlanVersion::new(2).unwrap();
    f.content.tasks = (0..24)
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
    let mut controller = SimulationController::new(
        f.store.clone(),
        SimulationLimits {
            attempts: 32,
            ..SimulationLimits::default()
        },
    )
    .unwrap();
    let mut authority = SyntheticAuthority::default();
    let mut q = EventQueue::new(512).unwrap();
    let mut bindings = Vec::new();
    let mut wire_bytes = 0;
    let frozen_count = count(&f, "tasks").await;
    for i in 0..24 {
        let (binding, _, peer) = attach(
            &f,
            &mut controller,
            &format!("task-{i}"),
            &format!("attempt-{i}"),
            &clock,
        )
        .await;
        authority.grant(&binding, &peer);
        let script = FakeWorker::new(binding.clone(), caps(), i as u64)
            .unwrap()
            .script(FakeBehavior::Success, T)
            .unwrap();
        for event in script.events {
            if let SimulatedInput::Message(bytes) = &event.input {
                wire_bytes += bytes.len()
            }
            q.push(event, peer.clone()).unwrap()
        }
        bindings.push(binding);
    }
    assert_eq!(q.len(), 144);
    let started = std::time::Instant::now();
    let events = q
        .run_until(T + 50, &mut controller, &authority, &clock)
        .await
        .unwrap();
    let elapsed = started.elapsed();
    assert_eq!(events.len(), 144);
    assert!(events.iter().all(|event| event.outcome.is_ok()));
    for b in &bindings {
        assert_eq!(controller.phase(b).unwrap(), SimulatedPhase::ResultReceived);
        assert_eq!(controller.receipt_count(b).unwrap(), 6);
    }
    assert_eq!(count(&f, "task_acceptances").await, 0);
    assert_eq!(count(&f, "task_attempts").await, 24);
    assert_eq!(count(&f, "tasks").await, frozen_count);
    assert_eq!(
        f.store
            .load_active(
                &f.content.key.project_id,
                &SyntheticPolicy,
                Limits::default()
            )
            .await
            .unwrap()
            .content
            .tasks
            .len(),
        24
    );
    assert_eq!(
        f.store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap()
            .held
            .total()
            .unwrap()
            .minor_units(),
        24 * 7
    );
    eprintln!(
        "R07 synthetic observation: 24 workers, 144 events, {wire_bytes} queued wire bytes, elapsed {elapsed:?}; not a production benchmark"
    );
}

#[tokio::test]
async fn missing_reservation_or_provider_eligibility_cannot_produce_a_worker_session() {
    let f = Fixture::new().await;
    f.activate().await.unwrap();
    let clock = DeterministicClock::new(T).unwrap();
    let mut controller =
        SimulationController::new(f.store.clone(), SimulationLimits::default()).unwrap();
    let mut req = AdmissionRequest {
        task: TaskRef {
            plan: f.content.key.clone(),
            task_id: TaskId::new("task-a").unwrap(),
        },
        fingerprint: f.revision.fingerprint().into(),
        expected_epoch: 0,
        request_id: RequestId::new("unfunded").unwrap(),
        actor: owner(),
        permit: ExecutionPermit {
            id: e("unfunded-cap"),
            provider: e("synthetic"),
            account: e("synthetic-account"),
            pool: e("synthetic-pool"),
            executor: PrincipalId::new("worker").unwrap(),
            maximum: costs(),
            valid_until: T + 3600,
        },
    };
    assert!(
        f.store
            .admit(&req, &SyntheticScheduler, &clock, Limits::default())
            .await
            .is_err()
    );
    req.permit.provider = e("unverified-provider");
    assert_eq!(
        f.store
            .admit(&req, &SyntheticScheduler, &clock, Limits::default())
            .await
            .unwrap_err(),
        AdmissionError::Forbidden
    );
    let binding = WorkerBinding {
        task: req.task,
        attempt_id: AttemptId::new("unfunded").unwrap(),
        worker_id: WorkerId::new("worker").unwrap(),
        epoch: 1,
        plan_hash: Digest::new(f.revision.fingerprint()).unwrap(),
        context_hash: Digest::new("1".repeat(64)).unwrap(),
    };
    let peer = PeerEvidence {
        reference: e("synthetic-peer"),
    };
    let mut authority = SyntheticAuthority::default();
    authority.grant(&binding, &peer);
    assert_eq!(
        controller
            .receive(
                &peer,
                &encode_worker_message(&message(
                    &binding,
                    1,
                    WorkerBody::Register {
                        capabilities: caps()
                    }
                ))
                .unwrap(),
                &authority,
                &clock
            )
            .await
            .unwrap_err(),
        WorkerError::UnknownAttempt
    );
    assert_eq!(count(&f, "dispatch_outbox").await, 0);
    assert_eq!(count(&f, "task_attempts").await, 0);
    assert_eq!(count(&f, "financial_events").await, 0);
}
#[tokio::test]
async fn identical_worker_and_attempt_strings_do_not_allow_cross_project_channel_reuse() {
    let mut s = setup().await;
    let mut content = plan();
    content.key.project_id = ProjectId::new("other-project").unwrap();
    for task in &mut content.tasks {
        task.plan = content.key.clone()
    }
    s.f.store
        .create_project(&content.key.project_id, &owner())
        .await
        .unwrap();
    let valid =
        rolvya_plan_validator::validate(&content, &SyntheticPolicy, Limits::default()).unwrap();
    let revision =
        s.f.store
            .save_draft(&valid, None, &owner(), &OwnerPolicy)
            .await
            .unwrap();
    s.f.store
        .activate(
            &revision,
            &RequestId::new("activate-other").unwrap(),
            &owner(),
            &OwnerPolicy,
            &SyntheticPolicy,
            Limits::default(),
        )
        .await
        .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    s.f.store
        .apply_financial(
            &FinancialRequest {
                project: content.key.project_id.clone(),
                request_id: RequestId::new("other-budget").unwrap(),
                actor: owner(),
                reason: "Synthetic other-project budget".into(),
                command: FinancialCommand::Approve {
                    expected_version: 0,
                    new_ceiling: money(100),
                    proof: OwnerProof {
                        reference: e("synthetic-other-owner-proof"),
                        verified_at: now,
                    },
                },
            },
            &SyntheticFinance,
        )
        .await
        .unwrap();
    let request = AdmissionRequest {
        task: TaskRef {
            plan: content.key.clone(),
            task_id: TaskId::new("task-a").unwrap(),
        },
        fingerprint: revision.fingerprint().into(),
        expected_epoch: 0,
        request_id: RequestId::new("attempt-a").unwrap(),
        actor: owner(),
        permit: ExecutionPermit {
            id: e("other-project-cap"),
            provider: e("synthetic"),
            account: e("synthetic-account"),
            pool: e("synthetic-pool"),
            executor: PrincipalId::new(s.binding.worker_id.as_str()).unwrap(),
            maximum: costs(),
            valid_until: T + 3600,
        },
    };
    let admitted =
        s.f.store
            .admit(&request, &SyntheticScheduler, &s.clock, Limits::default())
            .await
            .unwrap();
    let permit =
        s.f.store
            .claim_dispatch(
                &DispatchRequest {
                    lease: admitted.lease,
                    request_id: RequestId::new("other-dispatch").unwrap(),
                    actor: owner(),
                },
                &SyntheticScheduler,
                &s.clock,
                Limits::default(),
            )
            .await
            .unwrap();
    let other = s
        .controller
        .attach(permit, &SyntheticPolicy, &s.clock, Limits::default())
        .await
        .unwrap();
    let peer = PeerEvidence {
        reference: e("other-channel"),
    };
    s.authority.grant(&other, &peer);
    assert_eq!(s.binding.worker_id, other.worker_id);
    assert_eq!(s.binding.attempt_id, other.attempt_id);
    let registration = message(
        &other,
        1,
        WorkerBody::Register {
            capabilities: caps(),
        },
    );
    assert_eq!(
        s.controller
            .receive(
                &s.peer,
                &encode_worker_message(&registration).unwrap(),
                &s.authority,
                &s.clock
            )
            .await
            .unwrap_err(),
        WorkerError::Unauthorized
    );
    assert_eq!(
        s.controller.phase(&other).unwrap(),
        SimulatedPhase::Attached
    );
    assert_eq!(s.controller.receipt_count(&s.binding).unwrap(), 0);
}
#[tokio::test]
async fn controller_restart_does_not_claim_durable_worker_journal_or_reissue_dispatch() {
    let mut s = setup().await;
    let registration = message(
        &s.binding,
        1,
        WorkerBody::Register {
            capabilities: caps(),
        },
    );
    send(&mut s, &registration).await.unwrap();
    let mut restarted =
        SimulationController::new(s.f.store.clone(), SimulationLimits::default()).unwrap();
    assert_eq!(
        restarted
            .receive(
                &s.peer,
                &encode_worker_message(&registration).unwrap(),
                &s.authority,
                &s.clock
            )
            .await
            .unwrap_err(),
        WorkerError::UnknownAttempt
    );
    assert_eq!(
        s.f.store
            .claim_dispatch(
                &DispatchRequest {
                    lease: s.lease.clone(),
                    request_id: RequestId::new("reclaim-after-restart").unwrap(),
                    actor: owner()
                },
                &SyntheticScheduler,
                &s.clock,
                Limits::default()
            )
            .await
            .unwrap_err(),
        AdmissionError::AlreadyClaimed
    );
    assert_eq!(count(&s.f, "task_attempts").await, 1);
    assert_eq!(count(&s.f, "dispatch_claims").await, 1);
    assert_eq!(count(&s.f, "task_acceptances").await, 0);
}
