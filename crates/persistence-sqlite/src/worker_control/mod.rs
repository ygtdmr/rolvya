//! R08 durable control of synthetic workers. No production authentication or provider.
mod dispatch;
mod messages;
mod provider;
mod recovery;
use crate::{
    admission::{self, *},
    finance::{self, *},
    worker_simulation::{controller::Session, *},
    *,
};
pub use dispatch::*;
pub use messages::*;
pub use provider::*;
pub use recovery::*;
use rolvya_domain::{ids::*, worker::*};
use rolvya_plan_validator::Limits;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchState {
    Claimed,
    Launching,
    Reconcile,
    Active,
    Finished,
    NoEffect,
    Blocked,
}
impl DispatchState {
    fn name(self) -> &'static str {
        match self {
            Self::Claimed => "claimed",
            Self::Launching => "launching",
            Self::Reconcile => "reconcile",
            Self::Active => "active",
            Self::Finished => "finished",
            Self::NoEffect => "no_effect",
            Self::Blocked => "blocked",
        }
    }
    fn parse(s: &str) -> Result<Self, WorkerError> {
        serde_json::from_value(serde_json::Value::String(s.into()))
            .map_err(|_| StoreError::Integrity.into())
    }
}
/// Stable identity is project + R06 attempt ID. A ticket alone confers no authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchTicket {
    lease: LeaseRef,
}
impl DispatchTicket {
    pub fn lease(&self) -> &LeaseRef {
        &self.lease
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchCommand {
    binding: WorkerBinding,
    profile: ExecutionProfile,
    assignment: rolvya_domain::plan::ExecutionAssignment,
    admission: AdmissionRequest,
    financial_event: i64,
    initial_lease_until: i64,
}
impl LaunchCommand {
    pub fn binding(&self) -> &WorkerBinding {
        &self.binding
    }
    pub fn profile(&self) -> &ExecutionProfile {
        &self.profile
    }
    pub fn operation_key(&self) -> OperationKey {
        OperationKey {
            project: self.binding.task.plan.project_id.clone(),
            attempt: self.binding.attempt_id.clone(),
        }
    }
    pub fn digest(&self) -> Result<String, WorkerError> {
        Ok(hash(&json(self)?))
    }
    fn lease(&self) -> LeaseRef {
        LeaseRef {
            task: self.binding.task.clone(),
            attempt_id: self.binding.attempt_id.clone(),
            epoch: self.binding.epoch,
        }
    }
    fn session(&self) -> Session {
        Session {
            binding: self.binding.clone(),
            lease: self.lease(),
            profile: self.profile.clone(),
            maximum: self.admission.permit.maximum,
            financial_event: self.financial_event,
            lease_until: self.initial_lease_until,
            phase: SimulatedPhase::Attached,
            capabilities: None,
            offer_id: None,
            last_sequence: 0,
            journal: BTreeMap::new(),
            stop: None,
            result: None,
            progress: None,
        }
    }
}
/// Constructed only after durable launch-marker commit. Not deserializable or cloneable.
/// A future provider must implement scoped idempotency, lookup and enforceable bounds.
pub struct LaunchAuthorization {
    command: LaunchCommand,
}
impl LaunchAuthorization {
    pub fn command(&self) -> &LaunchCommand {
        &self.command
    }
}
fn hash(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
fn json(v: &impl Serialize) -> Result<String, WorkerError> {
    let s = serde_json::to_string(v).map_err(|_| StoreError::Integrity)?;
    if s.len() > 16384 {
        return Err(WorkerError::ResourceLimit);
    }
    Ok(s)
}
fn identity(prefix: &str, l: &LeaseRef) -> Result<RequestId, WorkerError> {
    Ok(RequestId::new(format!(
        "{prefix}-{}",
        &hash(&json(l)?)[..48]
    ))?)
}
async fn audit(
    c: &mut SqliteConnection,
    l: &LeaseRef,
    kind: &str,
    t: i64,
    payload: &impl Serialize,
) -> Result<i64, WorkerError> {
    Ok(sqlx::query_scalar("INSERT INTO worker_events(project_id,attempt_id,kind,recorded_at,payload) VALUES(?,?,?,?,?) RETURNING sequence")
        .bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).bind(kind).bind(t).bind(json(payload)?).fetch_one(c).await?)
}
async fn command(
    c: &mut SqliteConnection,
    l: &LeaseRef,
) -> Result<(LaunchCommand, DispatchState, i64), WorkerError> {
    let row=sqlx::query("SELECT command,digest,state,revision FROM dispatch_operations WHERE project_id=? AND attempt_id=?").bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).fetch_optional(c).await?.ok_or(WorkerError::UnknownAttempt)?;
    let text: String = row.get(0);
    let cmd: LaunchCommand = serde_json::from_str(&text).map_err(|_| StoreError::Integrity)?;
    if cmd.lease() != *l || cmd.digest()? != row.get::<String, _>(1) {
        return Err(WorkerError::ScopeMismatch);
    }
    cmd.binding.validate()?;
    Ok((
        cmd,
        DispatchState::parse(&row.get::<String, _>(2))?,
        row.get(3),
    ))
}
async fn session(
    c: &mut SqliteConnection,
    l: &LeaseRef,
    cmd: &LaunchCommand,
) -> Result<Session, WorkerError> {
    let row = sqlx::query(
        "SELECT state,last_sequence FROM worker_sessions WHERE project_id=? AND attempt_id=?",
    )
    .bind(l.task.plan.project_id.as_str())
    .bind(l.attempt_id.as_str())
    .fetch_optional(c)
    .await?
    .ok_or(WorkerError::UnknownAttempt)?;
    let s: Session =
        serde_json::from_str(&row.get::<String, _>(0)).map_err(|_| StoreError::Integrity)?;
    if s.binding != cmd.binding
        || s.lease != *l
        || s.profile != cmd.profile
        || s.maximum != cmd.admission.permit.maximum
        || s.financial_event != cmd.financial_event
        || i64::try_from(s.last_sequence).ok() != Some(row.get(1))
    {
        return Err(StoreError::Integrity.into());
    }
    Ok(s)
}
async fn financial_guard(c: &mut SqliteConnection, cmd: &LaunchCommand) -> Result<(), WorkerError> {
    let p = &cmd.binding.task.plan.project_id;
    let budget = finance::snapshot(c, p)
        .await
        .map_err(AdmissionError::from)?;
    if budget.uncertain_liabilities != 0 || budget.over_ceiling {
        return Err(WorkerError::FinancialRisk);
    }
    let balance = finance::liability(
        c,
        p,
        &LiabilityId::new(cmd.binding.attempt_id.as_str())?,
        budget.ceiling.currency(),
    )
    .await
    .map_err(AdmissionError::from)?
    .ok_or(StoreError::Integrity)?;
    let held = [
        balance.held.ai_api,
        balance.held.compute,
        balance.held.storage,
        balance.held.transfer,
    ];
    let max = cmd.admission.permit.maximum;
    let max = [max.ai_api, max.compute, max.storage, max.transfer];
    if balance.uncertain
        || balance.finalized
        || (0..4).any(|i| {
            held[i].currency() != max[i].currency() || held[i].minor_units() < max[i].minor_units()
        })
    {
        return Err(WorkerError::FinancialRisk);
    }
    Ok(())
}
async fn current<P: SchedulerPolicy + ?Sized>(
    c: &mut SqliteConnection,
    cmd: &LaunchCommand,
    policy: &P,
    t: i64,
) -> Result<(), WorkerError> {
    let l = cmd.lease();
    if admission::live(c, &l, t).await? != cmd.admission {
        return Err(StoreError::Integrity.into());
    }
    let seen: i64 = sqlx::query_scalar(
        "SELECT observed_at FROM worker_leases WHERE project_id=? AND attempt_id=?",
    )
    .bind(l.task.plan.project_id.as_str())
    .bind(l.attempt_id.as_str())
    .fetch_one(&mut *c)
    .await?;
    if t < seen {
        return Err(WorkerError::InvalidTime);
    }
    // Indexed immutable-scope check. Admission/claim already validated the complete
    // frozen DAG; heartbeat processing never reloads or copies the full plan.
    let row=sqlx::query("SELECT p.fingerprint,json_extract(t.definition,'$.context.sha256'),json_extract(t.definition,'$.execution'),r.state FROM tasks t JOIN plan_versions p USING(project_id,plan_id,version) JOIN task_runtime r USING(project_id,plan_id,version,task_id) JOIN activation_events e ON e.sequence=p.activated_event AND e.project_id=p.project_id AND e.plan_id=p.plan_id AND e.version=p.version AND e.fingerprint=p.fingerprint WHERE t.project_id=? AND t.plan_id=? AND t.version=? AND t.task_id=? AND p.state='active' AND t.kind='executable_leaf'")
        .bind(l.task.plan.project_id.as_str()).bind(l.task.plan.plan_id.as_str()).bind(version(&l.task.plan)?).bind(l.task.task_id.as_str()).fetch_optional(&mut *c).await?.ok_or(AdmissionError::NotEligible)?;
    let assignment: rolvya_domain::plan::ExecutionAssignment =
        serde_json::from_str(&row.get::<String, _>(2)).map_err(|_| StoreError::Integrity)?;
    if row.get::<String, _>(0) != cmd.binding.plan_hash.as_str()
        || row.get::<String, _>(1) != cmd.binding.context_hash.as_str()
        || row.get::<String, _>(3) != "running"
        || assignment != cmd.assignment
    {
        return Err(AdmissionError::NotEligible.into());
    }
    admission::validated_dependencies(c, &l.task, &cmd.admission.fingerprint).await?;
    policy
        .check(&assignment)
        .map_err(|_| AdmissionError::Capability)?;
    let request = DispatchRequest {
        lease: l.clone(),
        request_id: identity("dispatch", &l)?,
        actor: cmd.admission.actor.clone(),
    };
    let owner = admission::owner(c, &l.task.plan.project_id).await?;
    if !policy.permits(
        &owner,
        SchedulerAction::Dispatch {
            request: &request,
            admission: &cmd.admission,
        },
        t,
    ) {
        return Err(WorkerError::Unauthorized);
    }
    financial_guard(c, cmd).await
}
async fn transition(
    c: &mut SqliteConnection,
    l: &LeaseRef,
    state: DispatchState,
    revision: i64,
    event: i64,
) -> Result<(), WorkerError> {
    let n=sqlx::query("UPDATE dispatch_operations SET state=?,revision=revision+1,event_sequence=? WHERE project_id=? AND attempt_id=? AND revision=?")
        .bind(state.name()).bind(event).bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).bind(revision).execute(c).await?.rows_affected();
    if n != 1 {
        return Err(WorkerError::InvalidState);
    }
    Ok(())
}

fn affected(n: u64) -> Result<(), WorkerError> {
    if n == 1 {
        Ok(())
    } else {
        Err(StoreError::Integrity.into())
    }
}

/// Reuse current R06/R08 authority inside the broker's existing write transaction.
/// This returns no credential or fabricated authentication evidence.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn broker_guard(
    c: &mut SqliteConnection,
    binding: &WorkerBinding,
    peer: &PeerEvidence,
    authority: &dyn WorkerAuthority,
    policy: &dyn SchedulerPolicy,
    now: i64,
    connection: &str,
    account: &str,
    provider: &str,
    mode: &str,
) -> Result<(), WorkerError> {
    let lease = LeaseRef {
        task: binding.task.clone(),
        attempt_id: binding.attempt_id.clone(),
        epoch: binding.epoch,
    };
    let (cmd, state, _) = command(c, &lease).await?;
    let session = session(c, &lease, &cmd).await?;
    if state != DispatchState::Active
        || session.phase != SimulatedPhase::Executing
        || session.binding != *binding
        || !authority.permits(
            peer,
            binding,
            WorkerOperation::UseCredential,
            session.capabilities.as_ref(),
            now,
        )
    {
        return Err(WorkerError::Unauthorized);
    }
    let actual_mode = match cmd.assignment.access_mode {
        rolvya_domain::plan::AccessMode::ApiKey => "api_key",
        rolvya_domain::plan::AccessMode::CliSession => "cli_subscription",
    };
    if cmd.assignment.connection_ref != connection
        || cmd.admission.permit.account.as_str() != account
        || cmd.admission.permit.provider.as_str() != provider
        || actual_mode != mode
    {
        return Err(WorkerError::ScopeMismatch);
    }
    current(c, &cmd, policy, now).await
}
