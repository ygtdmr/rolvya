//! R06 authoritative transactions. No provider/network operations live here.
//! Policy/evidence and clock are trusted controller dependencies, never worker input.
use crate::{finance::*, *};
use rolvya_domain::{ids::AttemptId, money::CostBreakdown, plan::*, runtime::*};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionError {
    Storage(StoreError),
    Finance(FinanceError),
    InvalidInput,
    Forbidden,
    NotEligible,
    DependencyIncomplete,
    StaleFence,
    Conflict,
    Expired,
    Capability,
    ReplayConflict,
    AlreadyClaimed,
    RetryBlocked,
}
impl fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Storage(_) => "scheduler storage operation failed",
            Self::Finance(_) => "financial authority denied admission",
            Self::InvalidInput => "invalid scheduler request",
            Self::Forbidden => "scheduler action is not authorized",
            Self::NotEligible => "task is not an eligible active executable leaf",
            Self::DependencyIncomplete => "independent prerequisite acceptance is incomplete",
            Self::StaleFence => "attempt fencing identity is stale",
            Self::Conflict => "task or operation already has conflicting authority",
            Self::Expired => "lease or capability expired",
            Self::Capability => "execution capability is not valid",
            Self::ReplayConflict => "request identity is bound to a different action",
            Self::AlreadyClaimed => "dispatch was already claimed; reconcile external outcome",
            Self::RetryBlocked => "prior attempt requires reconciliation or retry is not allowed",
        })
    }
}
impl std::error::Error for AdmissionError {}
impl From<StoreError> for AdmissionError {
    fn from(e: StoreError) -> Self {
        Self::Storage(e)
    }
}
impl From<FinanceError> for AdmissionError {
    fn from(e: FinanceError) -> Self {
        Self::Finance(e)
    }
}
impl From<sqlx::Error> for AdmissionError {
    fn from(e: sqlx::Error) -> Self {
        Self::Storage(e.into())
    }
}
/// Controller clock. SystemClock is the production implementation; fake clocks are
/// only trusted synthetic fixtures. Never substitute a client-submitted timestamp.
pub trait Clock: Send + Sync {
    fn now(&self) -> Result<i64, AdmissionError>;
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Result<i64, AdmissionError> {
        Ok(crate::finance::now()?)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPermit {
    /// One-use trusted capacity/authorization evidence identity, not a bearer token.
    pub id: EvidenceRef,
    pub provider: EvidenceRef,
    pub account: EvidenceRef,
    pub pool: EvidenceRef,
    pub executor: PrincipalId,
    pub maximum: CostBreakdown,
    pub valid_until: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionRequest {
    pub task: TaskRef,
    pub fingerprint: String,
    pub expected_epoch: u64,
    pub request_id: RequestId,
    pub actor: PrincipalId,
    pub permit: ExecutionPermit,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseRef {
    pub task: TaskRef,
    pub attempt_id: AttemptId,
    pub epoch: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationRequest {
    pub task: TaskRef,
    pub fingerprint: String,
    pub lease: Option<LeaseRef>,
    pub request_id: RequestId,
    pub actor: PrincipalId,
    pub evidence: EvidenceRef,
    pub manifest_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FenceRequest {
    pub lease: LeaseRef,
    pub request_id: RequestId,
    pub actor: PrincipalId,
    pub evidence: EvidenceRef,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryRequest {
    pub lease: LeaseRef,
    pub request_id: RequestId,
    pub actor: PrincipalId,
    pub evidence: EvidenceRef,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchRequest {
    pub lease: LeaseRef,
    pub request_id: RequestId,
    pub actor: PrincipalId,
}
/// Full action context. Implementations must verify authenticated identities,
/// current scoped roles, capacity/isolation/entitlement, provider-enforced bounds
/// (all four categories, minimums/retries), expiry and revocation. Validation needs
/// independent artifact/acceptance evidence. Recovery needs verified provider/Git
/// reconciliation and a classified transient failure. No permissive default exists.
pub enum SchedulerAction<'a> {
    Admit {
        request: &'a AdmissionRequest,
        task: &'a PlannedTask,
    },
    Dispatch {
        request: &'a DispatchRequest,
        admission: &'a AdmissionRequest,
    },
    Validate {
        request: &'a ValidationRequest,
        task: &'a PlannedTask,
    },
    Fence(&'a FenceRequest),
    Recover(&'a RecoveryRequest),
}
pub trait SchedulerPolicy: AssignmentPolicy + Send + Sync {
    /// Bounded synchronous check inside the write transaction and before commit;
    /// no I/O, external side effects, or unverified model/worker claims.
    fn permits(&self, owner: &PrincipalId, action: SchedulerAction<'_>, now: i64) -> bool;
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionReceipt {
    pub lease: LeaseRef,
    pub lease_until: i64,
    pub financial_event: i64,
    pub audit_event: i64,
    pub outbox_sequence: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchIntent {
    pub sequence: i64,
    pub lease: LeaseRef,
}
/// Constructed only after claim COMMIT. Intentionally neither Clone nor Deserialize.
/// Stable operation identity is mandatory for future provider lookup/idempotency;
/// this value does not promise exactly-once external execution or billing.
#[derive(Debug)]
pub struct DispatchPermit {
    lease: LeaseRef,
    assignment: ExecutionAssignment,
    target: ExecutionPermit,
    lease_until: i64,
}
impl DispatchPermit {
    pub fn lease(&self) -> &LeaseRef {
        &self.lease
    }
    pub fn operation_id(&self) -> (&ProjectId, &AttemptId) {
        (&self.lease.task.plan.project_id, &self.lease.attempt_id)
    }
    pub fn assignment(&self) -> &ExecutionAssignment {
        &self.assignment
    }
    pub fn lease_until(&self) -> i64 {
        self.lease_until
    }
    pub fn target(&self) -> &ExecutionPermit {
        &self.target
    }
    pub fn attempt(&self) -> AttemptRecord {
        AttemptRecord {
            id: self.lease.attempt_id.clone(),
            task: self.lease.task.clone(),
            assignment: self.assignment.clone(),
            status: AttemptStatus::Created,
            lease_epoch: self.lease.epoch,
        }
    }
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn number(n: u64) -> Result<i64, AdmissionError> {
    i64::try_from(n).map_err(|_| AdmissionError::InvalidInput)
}
fn one(n: u64) -> Result<(), AdmissionError> {
    if n == 1 {
        Ok(())
    } else {
        Err(StoreError::Integrity.into())
    }
}
pub(crate) fn at(clock: &impl Clock) -> Result<i64, AdmissionError> {
    let t = clock.now()?;
    if t < 0 {
        Err(AdmissionError::InvalidInput)
    } else {
        Ok(t)
    }
}
fn payload(kind: &str, request: &impl Serialize) -> Result<String, AdmissionError> {
    let s = serde_json::to_string(&serde_json::json!({"format":1,"kind":kind,"request":request}))
        .map_err(|_| AdmissionError::InvalidInput)?;
    if s.len() > 32768 {
        return Err(AdmissionError::InvalidInput);
    }
    Ok(s)
}
pub(crate) async fn owner(
    c: &mut SqliteConnection,
    p: &ProjectId,
) -> Result<PrincipalId, AdmissionError> {
    let s: Option<String> =
        sqlx::query_scalar("SELECT owner_actor FROM projects WHERE project_id=?")
            .bind(p.as_str())
            .fetch_optional(c)
            .await?;
    Ok(PrincipalId::new(s.ok_or(StoreError::NotFound)?)?)
}
pub(crate) async fn existing(
    c: &mut SqliteConnection,
    p: &ProjectId,
    key: &RequestId,
    body: &str,
) -> Result<Option<i64>, AdmissionError> {
    let row = sqlx::query(
        "SELECT sequence,payload FROM scheduler_events WHERE project_id=? AND request_id=?",
    )
    .bind(p.as_str())
    .bind(key.as_str())
    .fetch_optional(c)
    .await?;
    row.map(|r| {
        if r.get::<String, _>(1) == body {
            Ok(r.get(0))
        } else {
            Err(AdmissionError::ReplayConflict)
        }
    })
    .transpose()
}
pub(crate) async fn event(
    c: &mut SqliteConnection,
    p: &ProjectId,
    key: &RequestId,
    kind: &str,
    actor: &PrincipalId,
    t: i64,
    body: &str,
) -> Result<i64, AdmissionError> {
    Ok(sqlx::query_scalar("INSERT INTO scheduler_events(project_id,request_id,kind,actor,recorded_at,payload) VALUES (?,?,?,?,?,?) RETURNING sequence")
        .bind(p.as_str()).bind(key.as_str()).bind(kind).bind(actor.as_str()).bind(t).bind(body).fetch_one(c).await?)
}
/// Prepared immutable content is fully validated before acquiring the writer.
/// Inside the writer we recheck active receipt, fingerprint and exact static task;
/// frozen-row guards preserve the validated graph while concurrent authority changes.
async fn recheck(
    c: &mut SqliteConnection,
    plan: &StoredPlan,
    task: &PlannedTask,
) -> Result<(), AdmissionError> {
    let k = &task.plan;
    let row=sqlx::query("SELECT p.state,p.fingerprint,t.definition FROM plan_versions p JOIN tasks t USING(project_id,plan_id,version) JOIN activation_events a ON a.sequence=p.activated_event AND a.project_id=p.project_id AND a.plan_id=p.plan_id AND a.version=p.version AND a.fingerprint=p.fingerprint WHERE p.project_id=? AND p.plan_id=? AND p.version=? AND t.task_id=?")
        .bind(k.project_id.as_str()).bind(k.plan_id.as_str()).bind(version(k)?).bind(task.id.as_str()).fetch_optional(&mut *c).await?.ok_or(AdmissionError::NotEligible)?;
    if !plan.active
        || row.get::<String, _>(0) != "active"
        || row.get::<String, _>(1) != plan.revision.fingerprint()
        || row.get::<String, _>(2)
            != serde_json::to_string(task).map_err(|_| AdmissionError::InvalidInput)?
    {
        return Err(AdmissionError::NotEligible);
    }
    Ok(())
}
pub(crate) fn task_in<'a>(
    plan: &'a StoredPlan,
    reference: &TaskRef,
    fingerprint: &str,
) -> Result<&'a PlannedTask, AdmissionError> {
    if !valid_hash(fingerprint)
        || plan.content.key != reference.plan
        || plan.revision.fingerprint() != fingerprint
    {
        return Err(AdmissionError::NotEligible);
    }
    plan.content
        .tasks
        .iter()
        .find(|t| t.id == reference.task_id)
        .ok_or(AdmissionError::NotEligible)
}
async fn state(c: &mut SqliteConnection, t: &TaskRef) -> Result<String, AdmissionError> {
    sqlx::query_scalar("SELECT state FROM task_runtime WHERE project_id=? AND plan_id=? AND version=? AND task_id=?")
        .bind(t.plan.project_id.as_str()).bind(t.plan.plan_id.as_str()).bind(version(&t.plan)?).bind(t.task_id.as_str()).fetch_optional(c).await?.ok_or(StoreError::Integrity.into())
}
async fn accepted(
    c: &mut SqliteConnection,
    t: &TaskRef,
    hash: &str,
) -> Result<bool, AdmissionError> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM task_acceptances a JOIN task_runtime r USING(project_id,plan_id,version,task_id) WHERE a.project_id=? AND a.plan_id=? AND a.version=? AND a.task_id=? AND a.fingerprint=? AND r.state='succeeded')")
        .bind(t.plan.project_id.as_str()).bind(t.plan.plan_id.as_str()).bind(version(&t.plan)?).bind(t.task_id.as_str()).bind(hash).fetch_one(c).await?)
}
async fn dependencies(
    c: &mut SqliteConnection,
    t: &PlannedTask,
    hash: &str,
) -> Result<(), AdmissionError> {
    validated_dependencies(
        c,
        &TaskRef {
            plan: t.plan.clone(),
            task_id: t.id.clone(),
        },
        hash,
    )
    .await
}
pub(crate) async fn validated_dependencies(
    c: &mut SqliteConnection,
    t: &TaskRef,
    hash: &str,
) -> Result<(), AdmissionError> {
    let missing:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM task_dependencies d LEFT JOIN task_acceptances a ON a.project_id=d.project_id AND a.plan_id=d.plan_id AND a.version=d.version AND a.task_id=d.predecessor_id AND a.fingerprint=? LEFT JOIN task_runtime r ON r.project_id=d.project_id AND r.plan_id=d.plan_id AND r.version=d.version AND r.task_id=d.predecessor_id WHERE d.project_id=? AND d.plan_id=? AND d.version=? AND d.successor_id=? AND (a.task_id IS NULL OR r.task_id IS NULL OR r.state<>'succeeded'))")
        .bind(hash).bind(t.plan.project_id.as_str()).bind(t.plan.plan_id.as_str()).bind(version(&t.plan)?).bind(t.task_id.as_str()).fetch_one(c).await?;
    if missing {
        Err(AdmissionError::DependencyIncomplete)
    } else {
        Ok(())
    }
}

async fn set_runtime(
    c: &mut SqliteConnection,
    t: &TaskRef,
    value: &str,
) -> Result<(), AdmissionError> {
    one(sqlx::query("UPDATE task_runtime SET state=? WHERE project_id=? AND plan_id=? AND version=? AND task_id=?")
        .bind(value).bind(t.plan.project_id.as_str()).bind(t.plan.plan_id.as_str()).bind(version(&t.plan)?).bind(t.task_id.as_str()).execute(c).await?.rows_affected())
}
async fn fence_value(
    c: &mut SqliteConnection,
    t: &TaskRef,
) -> Result<(i64, Option<String>), AdmissionError> {
    let r=sqlx::query("SELECT epoch,current_attempt FROM task_fences WHERE project_id=? AND plan_id=? AND version=? AND task_id=?")
        .bind(t.plan.project_id.as_str()).bind(t.plan.plan_id.as_str()).bind(version(&t.plan)?).bind(t.task_id.as_str()).fetch_optional(c).await?;
    Ok(r.map(|r| (r.get(0), r.get(1))).unwrap_or((0, None)))
}
pub(crate) async fn stored_admission(
    c: &mut SqliteConnection,
    lease: &LeaseRef,
) -> Result<(AdmissionRequest, i64, String), AdmissionError> {
    let r=sqlx::query("SELECT e.payload,l.lease_until,s.state,a.plan_id,a.version,a.task_id,a.epoch FROM task_attempts a JOIN scheduler_events e ON e.sequence=a.admission_event JOIN attempt_states s USING(project_id,attempt_id) JOIN worker_leases l USING(project_id,attempt_id) WHERE a.project_id=? AND a.attempt_id=?")
        .bind(lease.task.plan.project_id.as_str()).bind(lease.attempt_id.as_str()).fetch_optional(c).await?.ok_or(AdmissionError::StaleFence)?;
    if r.get::<String, _>(3) != lease.task.plan.plan_id.as_str()
        || r.get::<i64, _>(4) != version(&lease.task.plan)?
        || r.get::<String, _>(5) != lease.task.task_id.as_str()
        || r.get::<i64, _>(6) != number(lease.epoch)?
    {
        return Err(AdmissionError::StaleFence);
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Record {
        format: u32,
        kind: String,
        request: AdmissionRequest,
    }
    let record: Record =
        serde_json::from_str(&r.get::<String, _>(0)).map_err(|_| StoreError::Integrity)?;
    if record.format != 1
        || record.kind != "admit"
        || record.request.task != lease.task
        || record.request.expected_epoch.checked_add(1) != Some(lease.epoch)
        || record.request.request_id.as_str() != lease.attempt_id.as_str()
    {
        return Err(StoreError::Integrity.into());
    }
    Ok((record.request, r.get(1), r.get(2)))
}
pub(crate) async fn live(
    c: &mut SqliteConnection,
    l: &LeaseRef,
    t: i64,
) -> Result<AdmissionRequest, AdmissionError> {
    let (r, until, status) = stored_admission(c, l).await?;
    let (epoch, current) = fence_value(c, &l.task).await?;
    if number(l.epoch)? != epoch
        || current.as_deref() != Some(l.attempt_id.as_str())
        || status != "admitted"
    {
        return Err(AdmissionError::StaleFence);
    }
    let issued: i64 = sqlx::query_scalar(
        "SELECT admitted_at FROM task_attempts WHERE project_id=? AND attempt_id=?",
    )
    .bind(l.task.plan.project_id.as_str())
    .bind(l.attempt_id.as_str())
    .fetch_one(c)
    .await?;
    if t < issued || t >= until || t >= r.permit.valid_until {
        return Err(AdmissionError::Expired);
    }
    Ok(r)
}
async fn receipt(
    c: &mut SqliteConnection,
    r: &AdmissionRequest,
) -> Result<AdmissionReceipt, AdmissionError> {
    let a=sqlx::query("SELECT a.epoch,a.lease_until,a.financial_event,a.admission_event,o.sequence FROM task_attempts a JOIN attempt_states s USING(project_id,attempt_id) JOIN dispatch_outbox o USING(project_id,attempt_id) WHERE a.project_id=? AND a.attempt_id=?")
        .bind(r.task.plan.project_id.as_str()).bind(r.request_id.as_str()).fetch_optional(&mut *c).await?.ok_or(StoreError::Integrity)?;
    let l = LeaseRef {
        task: r.task.clone(),
        attempt_id: AttemptId::new(r.request_id.as_str())
            .map_err(|_| AdmissionError::InvalidInput)?,
        epoch: a.get::<i64, _>(0) as u64,
    };
    if stored_admission(c, &l).await?.0 != *r {
        return Err(StoreError::Integrity.into());
    }
    let positions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM financial_positions WHERE event_sequence=?")
            .bind(a.get::<i64, _>(2))
            .fetch_one(&mut *c)
            .await?;
    if positions != 4 {
        return Err(StoreError::Integrity.into());
    }
    finance::verify_application(c, a.get(2), &financial_request(r)?).await?;
    Ok(AdmissionReceipt {
        lease: l,
        lease_until: a.get(1),
        financial_event: a.get(2),
        audit_event: a.get(3),
        outbox_sequence: a.get(4),
    })
}
fn financial_request(r: &AdmissionRequest) -> Result<FinancialRequest, AdmissionError> {
    Ok(FinancialRequest {
        project: r.task.plan.project_id.clone(),
        request_id: r.request_id.clone(),
        actor: r.actor.clone(),
        reason: "Atomic scheduler admission".into(),
        command: FinancialCommand::Reserve {
            liability: LiabilityId::new(r.request_id.as_str())?,
            maximum: r.permit.maximum,
            bound_evidence: r.permit.id.clone(),
        },
    })
}
struct ReservationOnly<'a>(&'a FinancialRequest);
impl FinancialAuthorization for ReservationOnly<'_> {
    fn permits(&self, _: &PrincipalId, r: &FinancialRequest, _: i64) -> bool {
        r == self.0 && matches!(r.command, FinancialCommand::Reserve { .. })
    }
}
fn check_capability(r: &AdmissionRequest, t: i64) -> Result<(), AdmissionError> {
    r.permit
        .maximum
        .validate()
        .map_err(|_| AdmissionError::Capability)?;
    if r.permit.valid_until <= t {
        return Err(AdmissionError::Expired);
    }
    Ok(())
}
impl PlanStore {
    pub async fn admit<P: SchedulerPolicy>(
        &self,
        r: &AdmissionRequest,
        policy: &P,
        clock: &impl Clock,
        limits: Limits,
    ) -> Result<AdmissionReceipt, AdmissionError> {
        let plan = self.load(&r.task.plan, policy, limits).await?;
        let task = task_in(&plan, &r.task, &r.fingerprint)?;
        let assignment = task.execution.as_ref().ok_or(AdmissionError::NotEligible)?;
        if task.kind != TaskKind::ExecutableLeaf {
            return Err(AdmissionError::NotEligible);
        }
        let body = payload("admit", r)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let t = at(clock)?;
        let owner = owner(&mut tx, &r.task.plan.project_id).await?;
        recheck(&mut tx, &plan, task).await?;
        if !policy.permits(&owner, SchedulerAction::Admit { request: r, task }, t) {
            return Err(AdmissionError::Forbidden);
        }
        if existing(&mut tx, &r.task.plan.project_id, &r.request_id, &body)
            .await?
            .is_some()
        {
            let value = receipt(&mut tx, r).await?;
            tx.commit().await?;
            return Ok(value);
        }
        check_capability(r, t)?;
        policy
            .check(assignment)
            .map_err(|_| AdmissionError::Capability)?;
        if !matches!(
            state(&mut tx, &r.task).await?.as_str(),
            "planned" | "waiting" | "ready"
        ) {
            return Err(AdmissionError::NotEligible);
        }
        dependencies(&mut tx, task, &r.fingerprint).await?;
        let (epoch, current) = fence_value(&mut tx, &r.task).await?;
        if epoch != number(r.expected_epoch)? {
            return Err(AdmissionError::StaleFence);
        }
        if current.is_some() {
            return Err(AdmissionError::Conflict);
        }
        let next = epoch.checked_add(1).ok_or(AdmissionError::InvalidInput)?;
        let prior=sqlx::query("SELECT a.attempt_id,s.state,recovery.not_before,dispatch.state AS dispatch_state FROM task_attempts a JOIN attempt_states s USING(project_id,attempt_id) LEFT JOIN attempt_recovery recovery USING(project_id,attempt_id) LEFT JOIN dispatch_operations dispatch USING(project_id,attempt_id) WHERE a.project_id=? AND a.plan_id=? AND a.version=? AND a.task_id=? ORDER BY a.epoch")
            .bind(r.task.plan.project_id.as_str()).bind(r.task.plan.plan_id.as_str()).bind(version(&r.task.plan)?).bind(r.task.task_id.as_str()).fetch_all(&mut *tx).await?;
        if prior.len() >= 3 {
            return Err(AdmissionError::RetryBlocked);
        }
        for p in &prior {
            if p.get::<String, _>(1) != "fenced"
                || p.get::<Option<i64>, _>(2).is_none_or(|after| t < after)
                || p.get::<Option<String>, _>(3)
                    .is_some_and(|s| !matches!(s.as_str(), "finished" | "no_effect"))
            {
                return Err(AdmissionError::RetryBlocked);
            }
            let balance = finance::liability(
                &mut tx,
                &r.task.plan.project_id,
                &LiabilityId::new(p.get::<String, _>(0))?,
                r.permit.maximum.ai_api.currency(),
            )
            .await?
            .ok_or(StoreError::Integrity)?;
            if !balance.finalized
                || balance.uncertain
                || balance
                    .held
                    .total()
                    .map_err(FinanceError::from)?
                    .minor_units()
                    != 0
                || balance
                    .provisional
                    .total()
                    .map_err(FinanceError::from)?
                    .minor_units()
                    != 0
            {
                return Err(AdmissionError::RetryBlocked);
            }
        }
        let used: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM task_attempts WHERE capability_id=?)")
                .bind(r.permit.id.as_str())
                .fetch_one(&mut *tx)
                .await?;
        if used {
            return Err(AdmissionError::Capability);
        }
        let financial = financial_request(r)?;
        let preexisting: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM financial_events WHERE project_id=? AND request_id=?)",
        )
        .bind(r.task.plan.project_id.as_str())
        .bind(r.request_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if preexisting {
            return Err(AdmissionError::Conflict);
        }
        let booked = finance::apply_in(&mut tx, &financial, &ReservationOnly(&financial)).await?;
        // A preexisting standalone hold must never be adopted as new admission.
        let financial_owner: i64 =
            sqlx::query_scalar("SELECT count(*) FROM task_attempts WHERE financial_event=?")
                .bind(booked.sequence)
                .fetch_one(&mut *tx)
                .await?;
        if financial_owner != 0 {
            return Err(AdmissionError::Conflict);
        }
        let audit = event(
            &mut tx,
            &r.task.plan.project_id,
            &r.request_id,
            "admit",
            &r.actor,
            t,
            &body,
        )
        .await?;
        let until = t
            .checked_add(90)
            .ok_or(AdmissionError::InvalidInput)?
            .min(r.permit.valid_until);
        one(
            sqlx::query("INSERT INTO task_attempts VALUES (?,?,?,?,?,?,?,?,?,?,?)")
                .bind(r.task.plan.project_id.as_str())
                .bind(r.request_id.as_str())
                .bind(r.task.plan.plan_id.as_str())
                .bind(version(&r.task.plan)?)
                .bind(r.task.task_id.as_str())
                .bind(next)
                .bind(booked.sequence)
                .bind(audit)
                .bind(r.permit.id.as_str())
                .bind(t)
                .bind(until)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        one(
            sqlx::query("INSERT INTO attempt_states VALUES (?,?,'admitted',NULL)")
                .bind(r.task.plan.project_id.as_str())
                .bind(r.request_id.as_str())
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        if epoch == 0 {
            one(
                sqlx::query("INSERT INTO task_fences VALUES (?,?,?,?,0,NULL)")
                    .bind(r.task.plan.project_id.as_str())
                    .bind(r.task.plan.plan_id.as_str())
                    .bind(version(&r.task.plan)?)
                    .bind(r.task.task_id.as_str())
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
        }
        one(sqlx::query("UPDATE task_fences SET epoch=?,current_attempt=? WHERE project_id=? AND plan_id=? AND version=? AND task_id=? AND epoch=? AND current_attempt IS NULL")
            .bind(next).bind(r.request_id.as_str()).bind(r.task.plan.project_id.as_str()).bind(r.task.plan.plan_id.as_str()).bind(version(&r.task.plan)?).bind(r.task.task_id.as_str()).bind(epoch).execute(&mut *tx).await?.rows_affected())?;
        set_runtime(&mut tx, &r.task, "running").await?;
        let _: i64 = sqlx::query_scalar(
            "INSERT INTO dispatch_outbox(project_id,attempt_id) VALUES (?,?) RETURNING sequence",
        )
        .bind(r.task.plan.project_id.as_str())
        .bind(r.request_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        let value = receipt(&mut tx, r).await?;
        let end = at(clock)?;
        if end < t
            || end >= until
            || !policy.permits(&owner, SchedulerAction::Admit { request: r, task }, end)
            || policy.check(assignment).is_err()
        {
            return Err(AdmissionError::Capability);
        }
        let _ = live(&mut tx, &value.lease, end).await?;
        tx.commit().await?;
        Ok(value)
    }
    /// Non-authoritative bounded metadata. Only claim_dispatch returns a permit.
    pub async fn pending_dispatches(
        &self,
        project: &ProjectId,
        after: i64,
        limit: u16,
    ) -> Result<Vec<DispatchIntent>, AdmissionError> {
        if after < 0 || !(1..=100).contains(&limit) {
            return Err(AdmissionError::InvalidInput);
        }
        let rows=sqlx::query("SELECT o.sequence,a.plan_id,a.version,a.task_id,a.attempt_id,a.epoch FROM dispatch_outbox o JOIN task_attempts a USING(project_id,attempt_id) JOIN attempt_states s USING(project_id,attempt_id) JOIN task_fences f ON f.project_id=a.project_id AND f.plan_id=a.plan_id AND f.version=a.version AND f.task_id=a.task_id AND f.current_attempt=a.attempt_id AND f.epoch=a.epoch LEFT JOIN dispatch_claims c USING(project_id,attempt_id) WHERE o.project_id=? AND o.sequence>? AND s.state='admitted' AND c.attempt_id IS NULL ORDER BY o.sequence LIMIT ?")
            .bind(project.as_str()).bind(after).bind(i64::from(limit)).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|r| {
                Ok(DispatchIntent {
                    sequence: r.get(0),
                    lease: LeaseRef {
                        task: TaskRef {
                            plan: PlanKey {
                                project_id: project.clone(),
                                plan_id: rolvya_domain::ids::PlanId::new(r.get::<String, _>(1))
                                    .map_err(|_| StoreError::Integrity)?,
                                version: rolvya_domain::ids::PlanVersion::new(
                                    r.get::<i64, _>(2) as u64
                                )
                                .map_err(|_| StoreError::Integrity)?,
                            },
                            task_id: rolvya_domain::ids::TaskId::new(r.get::<String, _>(3))
                                .map_err(|_| StoreError::Integrity)?,
                        },
                        attempt_id: AttemptId::new(r.get::<String, _>(4))
                            .map_err(|_| StoreError::Integrity)?,
                        epoch: r.get::<i64, _>(5) as u64,
                    },
                })
            })
            .collect()
    }
    /// One-time claim, committed before returning a permit. Never automatically
    /// reissues a claimed operation after timeout/crash; R08 must reconcile it.
    pub async fn claim_dispatch<P: SchedulerPolicy>(
        &self,
        r: &DispatchRequest,
        policy: &P,
        clock: &impl Clock,
        limits: Limits,
    ) -> Result<DispatchPermit, AdmissionError> {
        let mut read = self.pool.begin().await?;
        let (admission, lease_until, _) = stored_admission(&mut read, &r.lease).await?;
        read.commit().await?;
        let plan = self.load(&r.lease.task.plan, policy, limits).await?;
        let task = task_in(&plan, &r.lease.task, &admission.fingerprint)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let t = at(clock)?;
        let owner = owner(&mut tx, &r.lease.task.plan.project_id).await?;
        recheck(&mut tx, &plan, task).await?;
        if !policy.permits(
            &owner,
            SchedulerAction::Dispatch {
                request: r,
                admission: &admission,
            },
            t,
        ) {
            return Err(AdmissionError::Forbidden);
        }
        let current = live(&mut tx, &r.lease, t).await?;
        if current != admission {
            return Err(StoreError::Integrity.into());
        }
        policy
            .check(task.execution.as_ref().ok_or(AdmissionError::NotEligible)?)
            .map_err(|_| AdmissionError::Capability)?;
        let claimed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM dispatch_claims WHERE project_id=? AND attempt_id=?)",
        )
        .bind(r.lease.task.plan.project_id.as_str())
        .bind(r.lease.attempt_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if claimed {
            return Err(AdmissionError::AlreadyClaimed);
        }
        let body = payload("dispatch", r)?;
        if existing(&mut tx, &r.lease.task.plan.project_id, &r.request_id, &body)
            .await?
            .is_some()
        {
            return Err(AdmissionError::AlreadyClaimed);
        }
        let snapshot = finance::snapshot(&mut tx, &r.lease.task.plan.project_id).await?;
        if snapshot.uncertain_liabilities > 0 {
            return Err(FinanceError::UncertainExposure.into());
        }
        if snapshot.over_ceiling {
            return Err(FinanceError::InsufficientBudget.into());
        }
        let balance = finance::liability(
            &mut tx,
            &r.lease.task.plan.project_id,
            &LiabilityId::new(r.lease.attempt_id.as_str())?,
            snapshot.ceiling.currency(),
        )
        .await?
        .ok_or(StoreError::Integrity)?;
        let held = [
            balance.held.ai_api,
            balance.held.compute,
            balance.held.storage,
            balance.held.transfer,
        ];
        let max = [
            admission.permit.maximum.ai_api,
            admission.permit.maximum.compute,
            admission.permit.maximum.storage,
            admission.permit.maximum.transfer,
        ];
        if balance.uncertain
            || balance.finalized
            || (0..4).any(|i| {
                held[i].currency() != max[i].currency()
                    || held[i].minor_units() < max[i].minor_units()
            })
        {
            return Err(FinanceError::InsufficientBudget.into());
        }
        let audit = event(
            &mut tx,
            &r.lease.task.plan.project_id,
            &r.request_id,
            "dispatch",
            &r.actor,
            t,
            &body,
        )
        .await?;
        one(sqlx::query("INSERT INTO dispatch_claims VALUES (?,?,?)")
            .bind(r.lease.task.plan.project_id.as_str())
            .bind(r.lease.attempt_id.as_str())
            .bind(audit)
            .execute(&mut *tx)
            .await?
            .rows_affected())?;
        let end = at(clock)?;
        if end < t
            || !policy.permits(
                &owner,
                SchedulerAction::Dispatch {
                    request: r,
                    admission: &admission,
                },
                end,
            )
        {
            return Err(AdmissionError::Forbidden);
        }
        live(&mut tx, &r.lease, end).await?;
        policy
            .check(task.execution.as_ref().ok_or(AdmissionError::NotEligible)?)
            .map_err(|_| AdmissionError::Capability)?;
        let permit = DispatchPermit {
            lease: r.lease.clone(),
            assignment: task.execution.clone().ok_or(AdmissionError::NotEligible)?,
            target: admission.permit,
            lease_until,
        };
        tx.commit().await?;
        Ok(permit)
    }
}
impl PlanStore {
    /// Accept only independently verified evidence. This is not a worker result API.
    pub async fn accept_task<P: SchedulerPolicy>(
        &self,
        r: &ValidationRequest,
        policy: &P,
        clock: &impl Clock,
        limits: Limits,
    ) -> Result<i64, AdmissionError> {
        if !valid_hash(&r.manifest_sha256) {
            return Err(AdmissionError::InvalidInput);
        }
        let plan = self.load(&r.task.plan, policy, limits).await?;
        let task = task_in(&plan, &r.task, &r.fingerprint)?;
        if r.lease.as_ref().is_some_and(|l| l.task != r.task) {
            return Err(AdmissionError::StaleFence);
        }
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let t = at(clock)?;
        let owner = owner(&mut tx, &r.task.plan.project_id).await?;
        recheck(&mut tx, &plan, task).await?;
        if !policy.permits(&owner, SchedulerAction::Validate { request: r, task }, t) {
            return Err(AdmissionError::Forbidden);
        }
        let body = payload("validate", r)?;
        if let Some(seq) = existing(&mut tx, &r.task.plan.project_id, &r.request_id, &body).await? {
            let saved:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM task_acceptances WHERE project_id=? AND plan_id=? AND version=? AND task_id=? AND event_sequence=?)")
                .bind(r.task.plan.project_id.as_str()).bind(r.task.plan.plan_id.as_str()).bind(version(&r.task.plan)?).bind(r.task.task_id.as_str()).bind(seq).fetch_one(&mut *tx).await?;
            if !saved || !accepted(&mut tx, &r.task, &r.fingerprint).await? {
                return Err(StoreError::Integrity.into());
            }
            tx.commit().await?;
            return Ok(seq);
        }
        dependencies(&mut tx, task, &r.fingerprint).await?;
        if task.kind == TaskKind::Group {
            if r.lease.is_some() {
                return Err(AdmissionError::NotEligible);
            }
            let missing:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tasks child LEFT JOIN task_acceptances a ON a.project_id=child.project_id AND a.plan_id=child.plan_id AND a.version=child.version AND a.task_id=child.task_id AND a.fingerprint=? LEFT JOIN task_runtime r ON r.project_id=child.project_id AND r.plan_id=child.plan_id AND r.version=child.version AND r.task_id=child.task_id WHERE child.project_id=? AND child.plan_id=? AND child.version=? AND child.parent_task_id=? AND (a.task_id IS NULL OR r.task_id IS NULL OR r.state<>'succeeded'))")
                .bind(&r.fingerprint).bind(r.task.plan.project_id.as_str()).bind(r.task.plan.plan_id.as_str()).bind(version(&r.task.plan)?).bind(r.task.task_id.as_str()).fetch_one(&mut *tx).await?;
            if missing {
                return Err(AdmissionError::DependencyIncomplete);
            }
            if !matches!(
                state(&mut tx, &r.task).await?.as_str(),
                "planned" | "waiting" | "ready"
            ) {
                return Err(AdmissionError::NotEligible);
            }
        } else {
            let l = r.lease.as_ref().ok_or(AdmissionError::StaleFence)?;
            let admission = live(&mut tx, l, t).await?;
            if admission.permit.executor == r.actor {
                return Err(AdmissionError::Forbidden);
            }
            let claimed: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM dispatch_claims WHERE project_id=? AND attempt_id=?)",
            )
            .bind(r.task.plan.project_id.as_str())
            .bind(l.attempt_id.as_str())
            .fetch_one(&mut *tx)
            .await?;
            if !claimed || state(&mut tx, &r.task).await? != "running" {
                return Err(AdmissionError::NotEligible);
            }
        }
        let seq = event(
            &mut tx,
            &r.task.plan.project_id,
            &r.request_id,
            "validate",
            &r.actor,
            t,
            &body,
        )
        .await?;
        one(
            sqlx::query("INSERT INTO task_acceptances VALUES (?,?,?,?,?,?,?,?,?,?)")
                .bind(r.task.plan.project_id.as_str())
                .bind(r.task.plan.plan_id.as_str())
                .bind(version(&r.task.plan)?)
                .bind(r.task.task_id.as_str())
                .bind(r.lease.as_ref().map(|l| l.attempt_id.as_str()))
                .bind(r.lease.as_ref().map(|l| number(l.epoch)).transpose()?)
                .bind(&r.fingerprint)
                .bind(&r.manifest_sha256)
                .bind(r.evidence.as_str())
                .bind(seq)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        if let Some(l) = &r.lease {
            one(sqlx::query("UPDATE attempt_states SET state='succeeded' WHERE project_id=? AND attempt_id=? AND state='admitted'").bind(r.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).execute(&mut *tx).await?.rows_affected())?;
        }
        set_runtime(&mut tx, &r.task, "succeeded").await?;
        let end = at(clock)?;
        if end < t || !policy.permits(&owner, SchedulerAction::Validate { request: r, task }, end) {
            return Err(AdmissionError::Forbidden);
        }
        if let Some(l) = &r.lease {
            let (admission, until, _) = stored_admission(&mut tx, l).await?;
            if end >= until || end >= admission.permit.valid_until {
                return Err(AdmissionError::Expired);
            }
        }
        if !accepted(&mut tx, &r.task, &r.fingerprint).await? {
            return Err(StoreError::Integrity.into());
        }
        tx.commit().await?;
        Ok(seq)
    }
    /// Fence current authority, including an expired lease, without releasing any
    /// money or asserting that the external worker stopped. No network cancellation.
    pub async fn fence_attempt<P: SchedulerPolicy>(
        &self,
        r: &FenceRequest,
        policy: &P,
        clock: &impl Clock,
    ) -> Result<u64, AdmissionError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let t = at(clock)?;
        let owner = owner(&mut tx, &r.lease.task.plan.project_id).await?;
        let (next, replay) = fence_in(&mut tx, r, policy, t).await?;
        let end = at(clock)?;
        if end < t || !policy.permits(&owner, SchedulerAction::Fence(r), end) {
            return Err(AdmissionError::Forbidden);
        }
        let (stored, current) = fence_value(&mut tx, &r.lease.task).await?;
        if !replay && (stored != number(next)? || current.is_some()) {
            return Err(StoreError::Integrity.into());
        }
        tx.commit().await?;
        Ok(next)
    }
    /// Record independently verified external/Git reconciliation and classified
    /// transient retry eligibility. Financial finalization remains separately
    /// required. No automatic retry; next admission rechecks all current gates.
    pub async fn reconcile_fenced<P: SchedulerPolicy>(
        &self,
        r: &RecoveryRequest,
        policy: &P,
        clock: &impl Clock,
    ) -> Result<i64, AdmissionError> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let t = at(clock)?;
        let p = &r.lease.task.plan.project_id;
        let owner = owner(&mut tx, p).await?;
        if !policy.permits(&owner, SchedulerAction::Recover(r), t) {
            return Err(AdmissionError::Forbidden);
        }
        let body = payload("recover", r)?;
        let (_, _, status) = stored_admission(&mut tx, &r.lease).await?;
        if status != "fenced" {
            return Err(AdmissionError::RetryBlocked);
        }
        if let Some(seq) = existing(&mut tx, p, &r.request_id, &body).await? {
            let after:Option<i64>=sqlx::query_scalar("SELECT not_before FROM attempt_recovery WHERE project_id=? AND attempt_id=? AND event_sequence=?").bind(p.as_str()).bind(r.lease.attempt_id.as_str()).bind(seq).fetch_optional(&mut *tx).await?;
            let after = after.ok_or(StoreError::Integrity)?;
            tx.commit().await?;
            return Ok(after);
        }
        let (epoch, current) = fence_value(&mut tx, &r.lease.task).await?;
        if current.is_some()
            || epoch
                != number(r.lease.epoch)?
                    .checked_add(1)
                    .ok_or(AdmissionError::InvalidInput)?
        {
            return Err(AdmissionError::StaleFence);
        }
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM task_attempts WHERE project_id=? AND plan_id=? AND version=? AND task_id=?").bind(p.as_str()).bind(r.lease.task.plan.plan_id.as_str()).bind(version(&r.lease.task.plan)?).bind(r.lease.task.task_id.as_str()).fetch_one(&mut *tx).await?;
        if !(1..3).contains(&count) {
            return Err(AdmissionError::RetryBlocked);
        }
        let fenced_at: i64 = sqlx::query_scalar(
            "SELECT fenced_at FROM attempt_states WHERE project_id=? AND attempt_id=?",
        )
        .bind(p.as_str())
        .bind(r.lease.attempt_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if t < fenced_at {
            return Err(AdmissionError::Expired);
        }
        // Engineering defaults: 3 total attempts, 10s then 30s minimum delay;
        // stable bounded 0..2s jitter, never reset on restart.
        let delay = if count == 1 { 10 } else { 30 };
        let not_before = fenced_at
            .checked_add(delay + number(r.lease.epoch)? % 3)
            .ok_or(AdmissionError::InvalidInput)?;
        let seq = event(&mut tx, p, &r.request_id, "recover", &r.actor, t, &body).await?;
        one(sqlx::query("INSERT INTO attempt_recovery VALUES (?,?,?,?)")
            .bind(p.as_str())
            .bind(r.lease.attempt_id.as_str())
            .bind(not_before)
            .bind(seq)
            .execute(&mut *tx)
            .await?
            .rows_affected())?;
        let end = at(clock)?;
        if end < t || !policy.permits(&owner, SchedulerAction::Recover(r), end) {
            return Err(AdmissionError::Forbidden);
        }
        tx.commit().await?;
        Ok(not_before)
    }
    /// Authorization-free metadata check for an already authenticated controller
    /// boundary; this does not authenticate workers or authorize financial writes.
    pub async fn check_lease(
        &self,
        lease: &LeaseRef,
        clock: &impl Clock,
    ) -> Result<(), AdmissionError> {
        let mut tx = self.pool.begin().await?;
        live(&mut tx, lease, at(clock)?).await?;
        tx.commit().await?;
        Ok(())
    }
}

pub(crate) async fn fence_in<P: SchedulerPolicy>(
    c: &mut SqliteConnection,
    r: &FenceRequest,
    policy: &P,
    t: i64,
) -> Result<(u64, bool), AdmissionError> {
    let p = &r.lease.task.plan.project_id;
    let owner = owner(&mut *c, p).await?;
    if !policy.permits(&owner, SchedulerAction::Fence(r), t) {
        return Err(AdmissionError::Forbidden);
    }
    let body = payload("fence", r)?;
    let (_, _, status) = stored_admission(&mut *c, &r.lease).await?;
    if existing(&mut *c, p, &r.request_id, &body).await?.is_some() {
        if status != "fenced" {
            return Err(StoreError::Integrity.into());
        }
        return Ok((
            r.lease
                .epoch
                .checked_add(1)
                .ok_or(AdmissionError::InvalidInput)?,
            true,
        ));
    }
    let (epoch, current) = fence_value(&mut *c, &r.lease.task).await?;
    if epoch != number(r.lease.epoch)?
        || current.as_deref() != Some(r.lease.attempt_id.as_str())
        || status != "admitted"
    {
        return Err(AdmissionError::StaleFence);
    }
    let next = epoch.checked_add(1).ok_or(AdmissionError::InvalidInput)?;
    event(&mut *c, p, &r.request_id, "fence", &r.actor, t, &body).await?;
    one(sqlx::query("UPDATE task_fences SET epoch=?,current_attempt=NULL WHERE project_id=? AND plan_id=? AND version=? AND task_id=? AND epoch=? AND current_attempt=?")
            .bind(next).bind(p.as_str()).bind(r.lease.task.plan.plan_id.as_str()).bind(version(&r.lease.task.plan)?).bind(r.lease.task.task_id.as_str()).bind(epoch).bind(r.lease.attempt_id.as_str()).execute(&mut *c).await?.rows_affected())?;
    one(sqlx::query("UPDATE attempt_states SET state='fenced',fenced_at=? WHERE project_id=? AND attempt_id=? AND state='admitted'").bind(t).bind(p.as_str()).bind(r.lease.attempt_id.as_str()).execute(&mut *c).await?.rows_affected())?;
    set_runtime(&mut *c, &r.lease.task, "waiting").await?;
    Ok((next as u64, false))
}
