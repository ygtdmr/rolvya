use super::*;
use crate::{admission::*, finance::*, *};
use rolvya_domain::{ids::*, worker::*};
use rolvya_plan_validator::{AssignmentPolicy, Limits};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Reference supplied by a controller transport boundary, never a message field.
/// It is not a credential or proof until a trusted verifier validates its binding.
#[derive(Clone)]
pub struct PeerEvidence {
    pub reference: EvidenceRef,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerOperation {
    Register,
    Claim,
    AcceptLease,
    Heartbeat,
    Progress,
    SubmitResult,
    StopQuery,
    StopAck,
    Disconnect,
    /// Separate broker authority; heartbeat permission never implies credential use.
    UseCredential,
    /// Separate credential lifecycle operation; never renews a worker lease.
    RenewCredential,
}
pub(crate) fn operation(body: &WorkerBody) -> WorkerOperation {
    match body {
        WorkerBody::Register { .. } => WorkerOperation::Register,
        WorkerBody::Claim {} => WorkerOperation::Claim,
        WorkerBody::AcceptLease { .. } => WorkerOperation::AcceptLease,
        WorkerBody::Heartbeat {} => WorkerOperation::Heartbeat,
        WorkerBody::Progress { .. } => WorkerOperation::Progress,
        WorkerBody::SubmitResult { .. } => WorkerOperation::SubmitResult,
        WorkerBody::GetStopInstruction {} => WorkerOperation::StopQuery,
        WorkerBody::StopAcknowledged { .. } => WorkerOperation::StopAck,
    }
}
/// Mandatory trusted identity/capability verifier. Evidence must bind the channel,
/// exact worker/project/attempt and allowed operation; declarations are not proof.
/// No I/O, credentials, or network endpoints are implemented by this simulation.
pub trait WorkerAuthority: Send + Sync {
    fn permits(
        &self,
        peer: &PeerEvidence,
        binding: &WorkerBinding,
        operation: WorkerOperation,
        declared: Option<&DeclaredCapabilities>,
        now: i64,
    ) -> bool;
}
/// Explicit fail-closed boundary for production authentication, still unimplemented.
pub struct UnimplementedAuthentication;
impl WorkerAuthority for UnimplementedAuthentication {
    fn permits(
        &self,
        _: &PeerEvidence,
        _: &WorkerBinding,
        _: WorkerOperation,
        _: Option<&DeclaredCapabilities>,
        _: i64,
    ) -> bool {
        false
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulationLimits {
    pub attempts: u16,
    pub messages_per_attempt: u16,
    pub scheduled_events: u16,
}
impl Default for SimulationLimits {
    fn default() -> Self {
        Self {
            attempts: 16,
            messages_per_attempt: 64,
            scheduled_events: 512,
        }
    }
}
impl SimulationLimits {
    pub fn validate(self) -> Result<(), WorkerError> {
        if !(1..=128).contains(&self.attempts)
            || !(1..=256).contains(&self.messages_per_attempt)
            || !(1..=4096).contains(&self.scheduled_events)
        {
            Err(WorkerError::ResourceLimit)
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimulatedPhase {
    Attached,
    Registered,
    LeaseOffered,
    Executing,
    ResultReceived,
    Failed,
    Disconnected,
    Stopped,
}
#[derive(Clone)]
pub(crate) struct JournalEntry {
    message: WorkerMessage,
    response: ControllerMessage,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Session {
    pub(crate) binding: WorkerBinding,
    pub(crate) lease: LeaseRef,
    pub(crate) profile: ExecutionProfile,
    pub(crate) maximum: rolvya_domain::money::CostBreakdown,
    pub(crate) financial_event: i64,
    pub(crate) lease_until: i64,
    pub(crate) phase: SimulatedPhase,
    pub(crate) capabilities: Option<DeclaredCapabilities>,
    pub(crate) offer_id: Option<MessageId>,
    pub(crate) last_sequence: u64,
    #[serde(skip)]
    pub(crate) journal: BTreeMap<MessageId, JournalEntry>,
    pub(crate) stop: Option<StopInstruction>,
    pub(crate) result: Option<ExecutionOutcome>,
    pub(crate) progress: Option<(u32, u32)>,
}
/// Ephemeral protocol state only. SQLite remains authority for the admitted task,
/// financial hold, dispatch claim and fencing. Restart/replay is intentionally R08.
pub struct SimulationController {
    store: PlanStore,
    limits: SimulationLimits,
    sessions: BTreeMap<(ProjectId, AttemptId), Session>,
}
fn key(binding: &WorkerBinding) -> (ProjectId, AttemptId) {
    (
        binding.task.plan.project_id.clone(),
        binding.attempt_id.clone(),
    )
}
fn time(clock: &impl Clock) -> Result<i64, WorkerError> {
    let t = clock.now()?;
    if t < 0 {
        Err(WorkerError::InvalidTime)
    } else {
        Ok(t)
    }
}
fn receipt(m: &WorkerMessage, t: i64, kind: ReceiptKind) -> WorkerReceipt {
    WorkerReceipt {
        message_id: m.message_id.clone(),
        sequence: m.sequence,
        observed_at: t,
        kind,
    }
}
fn reply(m: &WorkerMessage, body: ControllerBody) -> ControllerMessage {
    ControllerMessage {
        protocol_version: PROTOCOL_VERSION,
        response_to: m.message_id.clone(),
        binding: m.binding.clone(),
        body,
    }
}
impl SimulationController {
    pub fn new(store: PlanStore, limits: SimulationLimits) -> Result<Self, WorkerError> {
        limits.validate()?;
        Ok(Self {
            store,
            limits,
            sessions: BTreeMap::new(),
        })
    }
    /// Requires a consumed, committed R06 dispatch permit. No API attaches arbitrary
    /// worker-supplied lease metadata or adopts an unclaimed outbox operation.
    pub async fn attach(
        &mut self,
        permit: DispatchPermit,
        policy: &impl AssignmentPolicy,
        clock: &impl Clock,
        limits: Limits,
    ) -> Result<WorkerBinding, WorkerError> {
        if self.sessions.len() >= usize::from(self.limits.attempts) {
            return Err(WorkerError::ResourceLimit);
        }
        self.store.check_lease(permit.lease(), clock).await?;
        let plan = self
            .store
            .load(&permit.lease().task.plan, policy, limits)
            .await?;
        let task = plan
            .content
            .tasks
            .iter()
            .find(|t| t.id == permit.lease().task.task_id)
            .ok_or(WorkerError::ScopeMismatch)?;
        if !plan.active
            || task.execution.as_ref() != Some(permit.assignment())
            || task.kind != rolvya_domain::plan::TaskKind::ExecutableLeaf
        {
            return Err(WorkerError::ScopeMismatch);
        }
        let binding = WorkerBinding {
            task: permit.lease().task.clone(),
            attempt_id: permit.lease().attempt_id.clone(),
            worker_id: WorkerId::new(permit.target().executor.as_str())
                .map_err(|_| WorkerError::ScopeMismatch)?,
            epoch: permit.lease().epoch,
            plan_hash: Digest::new(plan.revision.fingerprint())?,
            context_hash: Digest::new(&task.context.sha256)?,
        };
        binding.validate()?;
        let k = key(&binding);
        if self.sessions.contains_key(&k) {
            return Err(WorkerError::InvalidState);
        }
        let financial_event:i64=sqlx::query_scalar("SELECT a.financial_event FROM task_attempts a JOIN dispatch_claims c USING(project_id,attempt_id) WHERE a.project_id=? AND a.attempt_id=? AND a.epoch=?").bind(binding.task.plan.project_id.as_str()).bind(binding.attempt_id.as_str()).bind(i64::try_from(binding.epoch).map_err(|_|WorkerError::ScopeMismatch)?).fetch_optional(&self.store.pool).await?.ok_or(WorkerError::ScopeMismatch)?;
        let profile = ExecutionProfile {
            model: ModelName::new(&permit.assignment().model)?,
            effort: permit.assignment().effort,
            access_mode: permit.assignment().access_mode,
            resources: permit.assignment().resources.clone(),
        };
        let session = Session {
            binding: binding.clone(),
            lease: permit.lease().clone(),
            profile,
            maximum: permit.target().maximum,
            financial_event,
            lease_until: permit.lease_until(),
            phase: SimulatedPhase::Attached,
            capabilities: None,
            offer_id: None,
            last_sequence: 0,
            journal: BTreeMap::new(),
            stop: None,
            result: None,
            progress: None,
        };
        self.financial_guard(&session).await?;
        self.sessions.insert(k, session);
        Ok(binding)
    }
    async fn financial_guard(&self, s: &Session) -> Result<(), WorkerError> {
        let budget = self
            .store
            .financial_snapshot(&s.binding.task.plan.project_id)
            .await
            .map_err(AdmissionError::from)?;
        if budget.uncertain_liabilities != 0 || budget.over_ceiling {
            return Err(WorkerError::FinancialRisk);
        }
        let balance = self
            .store
            .financial_liability(
                &s.binding.task.plan.project_id,
                &LiabilityId::new(s.binding.attempt_id.as_str())?,
            )
            .await
            .map_err(AdmissionError::from)?;
        let held = [
            balance.held.ai_api,
            balance.held.compute,
            balance.held.storage,
            balance.held.transfer,
        ];
        let max = [
            s.maximum.ai_api,
            s.maximum.compute,
            s.maximum.storage,
            s.maximum.transfer,
        ];
        if balance.uncertain
            || balance.finalized
            || (0..4).any(|i| {
                held[i].currency() != max[i].currency()
                    || held[i].minor_units() < max[i].minor_units()
            })
        {
            return Err(WorkerError::FinancialRisk);
        }
        Ok(())
    }
    pub fn phase(&self, b: &WorkerBinding) -> Result<SimulatedPhase, WorkerError> {
        let s = self
            .sessions
            .get(&key(b))
            .ok_or(WorkerError::UnknownAttempt)?;
        if s.binding != *b {
            return Err(WorkerError::ScopeMismatch);
        }
        Ok(s.phase)
    }
    /// Untrusted received outcome, not independently accepted evidence.
    pub fn received_outcome(
        &self,
        b: &WorkerBinding,
    ) -> Result<Option<&ExecutionOutcome>, WorkerError> {
        let s = self
            .sessions
            .get(&key(b))
            .ok_or(WorkerError::UnknownAttempt)?;
        if s.binding != *b {
            return Err(WorkerError::ScopeMismatch);
        }
        Ok(s.result.as_ref())
    }
    pub fn receipt_count(&self, b: &WorkerBinding) -> Result<usize, WorkerError> {
        let s = self
            .sessions
            .get(&key(b))
            .ok_or(WorkerError::UnknownAttempt)?;
        if s.binding != *b {
            return Err(WorkerError::ScopeMismatch);
        }
        Ok(s.journal.len())
    }
    pub async fn receive(
        &mut self,
        peer: &PeerEvidence,
        bytes: &[u8],
        authority: &impl WorkerAuthority,
        clock: &impl Clock,
    ) -> Result<ControllerMessage, WorkerError> {
        let message = decode_worker_message(bytes)?;
        let k = key(&message.binding);
        let t = time(clock)?;
        let s = self.sessions.get(&k).ok_or(WorkerError::UnknownAttempt)?;
        if message.binding.worker_id != s.binding.worker_id {
            return Err(WorkerError::Unauthorized);
        }
        let declared = match &message.body {
            WorkerBody::Register { capabilities } => Some(capabilities),
            _ => s.capabilities.as_ref(),
        };
        if !authority.permits(peer, &s.binding, operation(&message.body), declared, t) {
            return Err(WorkerError::Unauthorized);
        }
        if message.binding.task != s.binding.task {
            return Err(WorkerError::ScopeMismatch);
        }
        if message.binding.epoch != s.binding.epoch {
            return Err(WorkerError::StaleEpoch);
        }
        if message.binding.plan_hash != s.binding.plan_hash
            || message.binding.context_hash != s.binding.context_hash
        {
            return Err(WorkerError::ContextMismatch);
        }
        let stop_query = matches!(message.body, WorkerBody::GetStopInstruction {});
        let stop_ack = matches!(message.body, WorkerBody::StopAcknowledged { .. });
        let authority_check = self.store.check_lease(&s.lease, clock).await;
        let risk = if authority_check.is_ok() {
            self.financial_guard(s).await
        } else {
            Ok(())
        };
        if !stop_query && !(stop_ack && s.stop.is_some()) {
            authority_check.as_ref().map_err(Clone::clone)?;
            risk.as_ref().map_err(Clone::clone)?;
        }
        if let Some(old) = s.journal.get(&message.message_id) {
            if stop_query && !matches!(old.response.body, ControllerBody::Stop { .. }) {
                authority_check.as_ref().map_err(Clone::clone)?;
                risk.as_ref().map_err(Clone::clone)?;
                if s.stop.is_some() {
                    return Err(WorkerError::InvalidState);
                }
            }
            if old.message != message {
                return Err(WorkerError::ReplayConflict);
            }
            return Ok(old.response.clone());
        }
        let expected = s
            .last_sequence
            .checked_add(1)
            .ok_or(WorkerError::ResourceLimit)?;
        if message.sequence != expected {
            return Err(WorkerError::OutOfOrder { expected });
        }
        if s.journal.len() >= usize::from(self.limits.messages_per_attempt) {
            return Err(WorkerError::ResourceLimit);
        }
        let s = self
            .sessions
            .get_mut(&k)
            .ok_or(WorkerError::UnknownAttempt)?;
        let response = s.apply(&message, t, &authority_check, &risk)?;
        s.last_sequence = message.sequence;
        s.journal.insert(
            message.message_id.clone(),
            JournalEntry {
                message,
                response: response.clone(),
            },
        );
        Ok(response)
    }
    /// Trusted controller cancellation uses real R06 fencing. A worker cannot submit
    /// this request through the worker protocol and no financial hold is released.
    pub async fn cancel<P: SchedulerPolicy>(
        &mut self,
        request: &FenceRequest,
        policy: &P,
        clock: &impl Clock,
    ) -> Result<ControllerMessage, WorkerError> {
        let k = (
            request.lease.task.plan.project_id.clone(),
            request.lease.attempt_id.clone(),
        );
        let s = self.sessions.get(&k).ok_or(WorkerError::UnknownAttempt)?;
        if s.lease != request.lease {
            return Err(WorkerError::ScopeMismatch);
        }
        let epoch = self.store.fence_attempt(request, policy, clock).await?;
        let instruction = StopInstruction {
            instruction_id: MessageId::new(request.request_id.as_str())
                .map_err(|_| WorkerError::ScopeMismatch)?,
            reason: StopReason::ControllerCancellation,
            next_epoch: Some(epoch),
        };
        let s = self
            .sessions
            .get_mut(&k)
            .ok_or(WorkerError::UnknownAttempt)?;
        s.phase = SimulatedPhase::Stopped;
        s.stop = Some(instruction.clone());
        Ok(ControllerMessage {
            protocol_version: PROTOCOL_VERSION,
            response_to: instruction.instruction_id.clone(),
            binding: s.binding.clone(),
            body: ControllerBody::Stop { instruction },
        })
    }
    pub async fn disconnect(
        &mut self,
        peer: &PeerEvidence,
        binding: &WorkerBinding,
        authority: &impl WorkerAuthority,
        clock: &impl Clock,
    ) -> Result<(), WorkerError> {
        let t = time(clock)?;
        let s = self
            .sessions
            .get(&key(binding))
            .ok_or(WorkerError::UnknownAttempt)?;
        if s.binding != *binding {
            return Err(WorkerError::ScopeMismatch);
        }
        if !authority.permits(
            peer,
            &s.binding,
            WorkerOperation::Disconnect,
            s.capabilities.as_ref(),
            t,
        ) {
            return Err(WorkerError::Unauthorized);
        }
        self.store.check_lease(&s.lease, clock).await?;
        if !matches!(
            s.phase,
            SimulatedPhase::Executing | SimulatedPhase::Registered | SimulatedPhase::LeaseOffered
        ) {
            return Err(WorkerError::InvalidState);
        }
        self.sessions
            .get_mut(&key(binding))
            .ok_or(WorkerError::UnknownAttempt)?
            .phase = SimulatedPhase::Disconnected;
        Ok(())
    }
}

impl Session {
    /// Shared deterministic R07 transition logic. Durability is supplied by its caller.
    pub(crate) fn apply(
        &mut self,
        message: &WorkerMessage,
        t: i64,
        authority_check: &Result<(), AdmissionError>,
        risk: &Result<(), WorkerError>,
    ) -> Result<ControllerMessage, WorkerError> {
        let s = &*self;
        let mut phase = s.phase;
        let mut new_caps = None;
        let mut offer = None;
        let mut result = None;
        let mut progress = s.progress;
        let mut stop = None;
        let body = match &message.body {
            WorkerBody::Register { capabilities } if s.phase == SimulatedPhase::Attached => {
                if capabilities.evidence.is_none() || !capabilities.supports(&s.profile) {
                    return Err(ProtocolError::InvalidCapabilities.into());
                }
                new_caps = Some(capabilities.clone());
                phase = SimulatedPhase::Registered;
                ControllerBody::Registered {
                    receipt: receipt(message, t, ReceiptKind::Registered),
                }
            }
            WorkerBody::Claim {} if s.phase == SimulatedPhase::Registered => {
                phase = SimulatedPhase::LeaseOffered;
                offer = Some(message.message_id.clone());
                ControllerBody::LeaseAssignment {
                    offer: LeaseOffer {
                        offer_id: message.message_id.clone(),
                        profile: s.profile.clone(),
                        financial_event: s.financial_event,
                        lease_until: s.lease_until,
                        timing: WorkerTiming::default(),
                    },
                }
            }
            WorkerBody::AcceptLease { offer_id }
                if s.phase == SimulatedPhase::LeaseOffered
                    && s.offer_id.as_ref() == Some(offer_id) =>
            {
                phase = SimulatedPhase::Executing;
                ControllerBody::Acknowledged {
                    receipt: receipt(message, t, ReceiptKind::LeaseAccepted),
                }
            }
            WorkerBody::Heartbeat {} if s.phase == SimulatedPhase::Executing => {
                ControllerBody::Acknowledged {
                    receipt: receipt(message, t, ReceiptKind::HeartbeatObserved),
                }
            }
            WorkerBody::Progress {
                completed, total, ..
            } if s.phase == SimulatedPhase::Executing => {
                if s.progress.is_some_and(|(old, previous_total)| {
                    *completed < old || *total != previous_total
                }) {
                    return Err(WorkerError::InvalidState);
                }
                progress = Some((*completed, *total));
                ControllerBody::Acknowledged {
                    receipt: receipt(message, t, ReceiptKind::ProgressObserved),
                }
            }
            WorkerBody::SubmitResult { outcome } if s.phase == SimulatedPhase::Executing => {
                let kind = match outcome {
                    ExecutionOutcome::Succeeded { .. } => {
                        phase = SimulatedPhase::ResultReceived;
                        ReceiptKind::ResultReceived
                    }
                    ExecutionOutcome::Failed { .. } => {
                        phase = SimulatedPhase::Failed;
                        ReceiptKind::FailureReported
                    }
                };
                result = Some(outcome.clone());
                ControllerBody::Acknowledged {
                    receipt: receipt(message, t, kind),
                }
            }
            WorkerBody::GetStopInstruction {} => {
                let reason = if let Some(instruction) = &s.stop {
                    Some(instruction.reason)
                } else {
                    match &authority_check {
                        Err(AdmissionError::Expired) => Some(StopReason::LeaseExpired),
                        Err(AdmissionError::StaleFence) => Some(StopReason::Fenced),
                        Err(error) => return Err(WorkerError::Admission(error.clone())),
                        Ok(()) => {
                            if risk.is_err() {
                                Some(StopReason::BudgetRisk)
                            } else {
                                None
                            }
                        }
                    }
                };
                if let Some(reason) = reason {
                    let instruction = s.stop.clone().unwrap_or(StopInstruction {
                        instruction_id: message.message_id.clone(),
                        reason,
                        next_epoch: None,
                    });
                    stop = Some(instruction.clone());
                    phase = SimulatedPhase::Stopped;
                    ControllerBody::Stop { instruction }
                } else {
                    ControllerBody::Acknowledged {
                        receipt: receipt(message, t, ReceiptKind::InstructionObserved),
                    }
                }
            }
            WorkerBody::StopAcknowledged { instruction_id }
                if s.phase == SimulatedPhase::Stopped
                    && s.stop
                        .as_ref()
                        .is_some_and(|stop| stop.instruction_id == *instruction_id) =>
            {
                ControllerBody::Acknowledged {
                    receipt: receipt(message, t, ReceiptKind::StoppedReported),
                }
            }
            _ => return Err(WorkerError::InvalidState),
        };
        let response = reply(message, body);
        encode_controller_message(&response)?;
        let s = self;
        s.phase = phase;
        s.progress = progress;
        if let Some(c) = new_caps {
            s.capabilities = Some(c)
        }
        if let Some(id) = offer {
            s.offer_id = Some(id)
        }
        if let Some(outcome) = result {
            s.result = Some(outcome)
        }
        if let Some(instruction) = stop {
            s.stop = Some(instruction)
        }
        s.last_sequence = message.sequence;
        Ok(response)
    }
}
