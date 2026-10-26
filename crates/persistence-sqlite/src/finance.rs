//! R04 accounting on the existing SQLite authority. Holds never admit execution.
//!
//! All mutations serialize with BEGIN IMMEDIATE. Authentication, verified human
//! consent and provider evidence belong to a trusted caller policy, not IDs in a
//! request. Only references to that evidence are stored here, never bearer proofs.
//! No financial operation invokes providers, creates attempts or dispatches work.
use crate::*;
use rolvya_domain::money::{CostBreakdown, Currency, Money, MoneyError};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

identity!(LiabilityId);
identity!(EvidenceRef);
const CATEGORIES: [&str; 4] = ["ai_api", "compute", "storage", "transfer"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinanceError {
    Storage(StoreError),
    InvalidInput,
    CurrencyMismatch,
    Arithmetic,
    StaleVersion,
    ProofExpired,
    ProofConsumed,
    InsufficientBudget,
    UncertainExposure,
    InvalidTransition,
}
impl fmt::Display for FinanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Storage(_) => "financial storage operation failed",
            Self::InvalidInput => "invalid financial request",
            Self::CurrencyMismatch => "accounting currencies do not match",
            Self::Arithmetic => "financial arithmetic exceeds integer capacity",
            Self::StaleVersion => "financial revision is stale",
            Self::ProofExpired => "exact-action Owner proof is outside its validity window",
            Self::ProofConsumed => "Owner proof was already consumed",
            Self::InsufficientBudget => "approved budget cannot cover the proposed exposure",
            Self::UncertainExposure => "unreconciled exposure blocks new financial holds",
            Self::InvalidTransition => "financial state transition is not permitted",
        })
    }
}
impl std::error::Error for FinanceError {}
impl From<StoreError> for FinanceError {
    fn from(e: StoreError) -> Self {
        Self::Storage(e)
    }
}
impl From<sqlx::Error> for FinanceError {
    fn from(e: sqlx::Error) -> Self {
        // SQLite SUM uses exact integers; an overflowing aggregate fails closed.
        if matches!(&e, sqlx::Error::Database(db) if db.message()=="integer overflow") {
            Self::Arithmetic
        } else {
            Self::Storage(e.into())
        }
    }
}
impl From<MoneyError> for FinanceError {
    fn from(e: MoneyError) -> Self {
        if e == MoneyError::CurrencyMismatch {
            Self::CurrencyMismatch
        } else {
            Self::Arithmetic
        }
    }
}

/// Non-secret reference to a freshly verified, explicitly confirmed exact action.
/// Authenticity, actor/session binding and revocation must be checked by the policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OwnerProof {
    pub reference: EvidenceRef,
    pub verified_at: i64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ReconciliationState {
    /// Remaining liabilities have a verified conservative bound.
    Bounded,
    /// External exposure cannot currently be bounded; block new holds.
    Uncertain,
    /// Authoritative evidence confirms no remaining liability.
    Final,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum FinancialCommand {
    Approve {
        expected_version: u64,
        new_ceiling: Money,
        proof: OwnerProof,
    },
    /// Financial hold only, never a dispatch permit. A trusted policy must verify
    /// the enforceable maximum including minimum charges and hidden retries.
    Reserve {
        liability: LiabilityId,
        maximum: CostBreakdown,
        bound_evidence: EvidenceRef,
    },
    /// Cumulative *additional unreserved* liability, not usage already in a hold.
    /// Cannot release existing exposure or clear uncertainty. Version zero creates
    /// an observed liability; billing reports never authorize new spending.
    Report {
        liability: LiabilityId,
        expected_revision: u64,
        unreserved: CostBreakdown,
        uncertain: bool,
        evidence: EvidenceRef,
    },
    /// Cumulative confirmed charges and separately evidenced remaining exposure.
    /// Only trusted reconciliation evidence may reduce holds/provisional exposure.
    /// Refunds/FX are deliberately unsupported; settled amounts cannot decrease.
    Reconcile {
        liability: LiabilityId,
        expected_revision: u64,
        settled: CostBreakdown,
        remaining_hold: CostBreakdown,
        remaining_provisional: CostBreakdown,
        state: ReconciliationState,
        evidence: EvidenceRef,
    },
}
impl FinancialCommand {
    fn kind(&self) -> &'static str {
        match self {
            Self::Approve { .. } => "approval",
            Self::Reserve { .. } => "reserve",
            Self::Report { .. } => "report",
            Self::Reconcile { .. } => "reconcile",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FinancialRequest {
    pub project: ProjectId,
    pub request_id: RequestId,
    pub actor: PrincipalId,
    /// Bounded audit rationale; never include credentials, tokens or customer data.
    pub reason: String,
    pub command: FinancialCommand,
}
/// Mandatory trusted control-plane policy. Runs inside the write transaction and
/// immediately before commit; must be bounded, synchronous and side-effect-free.
/// Verify the entire request: actual authenticated identity, current roles/session,
/// explicit action-bound human consent for approvals, evidence scope and freshness
/// for provider bounds/releases/billing, including project and liability binding.
/// A worker or a matching Owner string is never sufficient authorization.
pub trait FinancialAuthorization: Send + Sync {
    fn permits(&self, owner: &PrincipalId, request: &FinancialRequest, now: i64) -> bool;
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinancialReceipt {
    pub sequence: i64,
    pub request_id: RequestId,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiabilityBalance {
    pub revision: u64,
    pub settled: CostBreakdown,
    pub held: CostBreakdown,
    pub provisional: CostBreakdown,
    pub uncertain: bool,
    pub finalized: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetSnapshot {
    pub version: u64,
    pub ceiling: Money,
    pub settled: CostBreakdown,
    pub held: CostBreakdown,
    pub provisional: CostBreakdown,
    pub uncertain_liabilities: u64,
    pub headroom: Money,
    pub over_ceiling: bool,
}
impl BudgetSnapshot {
    fn require_hold(&self, maximum: CostBreakdown) -> Result<(), FinanceError> {
        let total = maximum.total()?;
        if total.currency() != self.ceiling.currency() {
            return Err(FinanceError::CurrencyMismatch);
        }
        if self.uncertain_liabilities != 0 {
            return Err(FinanceError::UncertainExposure);
        }
        if self.over_ceiling || total.minor_units() > self.headroom.minor_units() {
            return Err(FinanceError::InsufficientBudget);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerEvent {
    pub sequence: i64,
    pub request_id: RequestId,
    pub actor: PrincipalId,
    pub recorded_at: i64,
    /// Versioned serialized request; includes evidence references, never proof tokens.
    pub request_json: String,
}
fn now() -> Result<i64, FinanceError> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| FinanceError::InvalidInput)?
            .as_secs(),
    )
    .map_err(|_| FinanceError::InvalidInput)
}
fn proof_current(proof: &OwnerProof, at: i64) -> Result<(), FinanceError> {
    if proof.verified_at < 0 || at < proof.verified_at || at - proof.verified_at >= 300 {
        return Err(FinanceError::ProofExpired);
    }
    Ok(())
}
fn integer_version(v: u64) -> Result<i64, FinanceError> {
    i64::try_from(v).map_err(|_| FinanceError::Arithmetic)
}
fn amounts(cost: CostBreakdown) -> [i64; 4] {
    [
        cost.ai_api.minor_units(),
        cost.compute.minor_units(),
        cost.storage.minor_units(),
        cost.transfer.minor_units(),
    ]
}
fn costs(a: [i64; 4], c: Currency) -> Result<CostBreakdown, FinanceError> {
    Ok(CostBreakdown {
        ai_api: Money::new(a[0], c)?,
        compute: Money::new(a[1], c)?,
        storage: Money::new(a[2], c)?,
        transfer: Money::new(a[3], c)?,
    })
}
fn zero(c: Currency) -> Result<CostBreakdown, FinanceError> {
    costs([0; 4], c)
}
fn require_currency(cost: CostBreakdown, currency: Currency) -> Result<(), FinanceError> {
    if cost.total()?.currency() != currency {
        return Err(FinanceError::CurrencyMismatch);
    }
    Ok(())
}
fn require_one(rows: u64) -> Result<(), FinanceError> {
    if rows == 1 {
        Ok(())
    } else {
        Err(StoreError::Integrity.into())
    }
}
async fn account(c: &mut SqliteConnection, project: &ProjectId) -> Result<Currency, FinanceError> {
    let value: String =
        sqlx::query_scalar("SELECT currency FROM financial_accounts WHERE project_id=?")
            .bind(project.as_str())
            .fetch_optional(c)
            .await?
            .ok_or(StoreError::NotFound)?;
    Currency::new(&value).map_err(|_| StoreError::Integrity.into())
}
async fn ceiling(
    c: &mut SqliteConnection,
    project: &ProjectId,
) -> Result<(i64, i64), FinanceError> {
    let row = sqlx::query("SELECT version,new_ceiling FROM budget_approvals WHERE project_id=? ORDER BY version DESC LIMIT 1")
        .bind(project.as_str()).fetch_optional(c).await?;
    Ok(row.map(|r| (r.get(0), r.get(1))).unwrap_or((0, 0)))
}
async fn liability(
    c: &mut SqliteConnection,
    project: &ProjectId,
    id: &LiabilityId,
    currency: Currency,
) -> Result<Option<LiabilityBalance>, FinanceError> {
    let row = sqlx::query("SELECT event_sequence,version,uncertain,finalized FROM liability_revisions WHERE project_id=? AND liability_id=? ORDER BY version DESC LIMIT 1")
        .bind(project.as_str()).bind(id.as_str()).fetch_optional(&mut *c).await?;
    let Some(row) = row else { return Ok(None) };
    let rows = sqlx::query("SELECT category,settled_minor,held_minor,provisional_minor FROM financial_positions WHERE event_sequence=?")
        .bind(row.get::<i64,_>(0)).fetch_all(&mut *c).await?;
    if rows.len() != 4 {
        return Err(StoreError::Integrity.into());
    }
    let mut settled = [0; 4];
    let mut held = [0; 4];
    let mut provisional = [0; 4];
    for r in rows {
        let idx = CATEGORIES
            .iter()
            .position(|name| *name == r.get::<String, _>(0))
            .ok_or(StoreError::Integrity)?;
        settled[idx] = r.get(1);
        held[idx] = r.get(2);
        provisional[idx] = r.get(3);
    }
    Ok(Some(LiabilityBalance {
        revision: row.get::<i64, _>(1) as u64,
        settled: costs(settled, currency)?,
        held: costs(held, currency)?,
        provisional: costs(provisional, currency)?,
        uncertain: row.get(2),
        finalized: row.get(3),
    }))
}
const CURRENT: &str = "SELECT r.event_sequence FROM liability_revisions r WHERE r.project_id=? AND NOT EXISTS (SELECT 1 FROM liability_revisions newer WHERE newer.project_id=r.project_id AND newer.liability_id=r.liability_id AND newer.version>r.version)";
async fn snapshot(
    c: &mut SqliteConnection,
    project: &ProjectId,
) -> Result<BudgetSnapshot, FinanceError> {
    let currency = account(c, project).await?;
    let (version, cap) = ceiling(c, project).await?;
    if version == 0 {
        return Err(StoreError::Integrity.into());
    }
    let malformed: i64=sqlx::query_scalar(&format!("SELECT count(*) FROM liability_revisions WHERE event_sequence IN ({CURRENT}) AND (SELECT count(*) FROM financial_positions p WHERE p.event_sequence=liability_revisions.event_sequence)<>4"))
        .bind(project.as_str()).fetch_one(&mut *c).await?;
    if malformed != 0 {
        return Err(StoreError::Integrity.into());
    }
    let rows=sqlx::query(&format!("SELECT category,sum(settled_minor),sum(held_minor),sum(provisional_minor) FROM financial_positions WHERE event_sequence IN ({CURRENT}) GROUP BY category"))
        .bind(project.as_str()).fetch_all(&mut *c).await?;
    let mut settled = [0; 4];
    let mut held = [0; 4];
    let mut provisional = [0; 4];
    for r in rows {
        let idx = CATEGORIES
            .iter()
            .position(|name| *name == r.get::<String, _>(0))
            .ok_or(StoreError::Integrity)?;
        settled[idx] = r.get(1);
        held[idx] = r.get(2);
        provisional[idx] = r.get(3);
    }
    let uncertain: i64=sqlx::query_scalar(&format!("SELECT count(*) FROM liability_revisions WHERE event_sequence IN ({CURRENT}) AND uncertain=1"))
        .bind(project.as_str()).fetch_one(&mut *c).await?;
    let settled = costs(settled, currency)?;
    let held = costs(held, currency)?;
    let provisional = costs(provisional, currency)?;
    let exposure = settled
        .total()?
        .checked_add(held.total()?)?
        .checked_add(provisional.total()?)?
        .minor_units();
    Ok(BudgetSnapshot {
        version: version as u64,
        ceiling: Money::new(cap, currency)?,
        settled,
        held,
        provisional,
        uncertain_liabilities: uncertain as u64,
        headroom: Money::new(cap.saturating_sub(exposure).max(0), currency)?,
        over_ceiling: exposure > cap,
    })
}
impl PlanStore {
    /// Trusted read boundary, like plan reads. Caller enforces project read access.
    pub async fn financial_snapshot(
        &self,
        project: &ProjectId,
    ) -> Result<BudgetSnapshot, FinanceError> {
        let mut tx = self.pool.begin().await?;
        let value = snapshot(&mut tx, project).await?;
        tx.commit().await?;
        Ok(value)
    }
    pub async fn financial_liability(
        &self,
        project: &ProjectId,
        id: &LiabilityId,
    ) -> Result<LiabilityBalance, FinanceError> {
        let mut tx = self.pool.begin().await?;
        let currency = account(&mut tx, project).await?;
        let value = liability(&mut tx, project, id, currency)
            .await?
            .ok_or(StoreError::NotFound)?;
        tx.commit().await?;
        Ok(value)
    }
    /// Bounded keyset pagination. Reads do not confer spending authorization.
    pub async fn financial_history(
        &self,
        project: &ProjectId,
        after: i64,
        limit: u16,
    ) -> Result<Vec<LedgerEvent>, FinanceError> {
        if after < 0 || !(1..=100).contains(&limit) {
            return Err(FinanceError::InvalidInput);
        }
        sqlx::query("SELECT sequence,request_id,actor,recorded_at,payload FROM financial_events WHERE project_id=? AND sequence>? ORDER BY sequence LIMIT ?")
            .bind(project.as_str()).bind(after).bind(i64::from(limit)).fetch_all(&self.pool).await?
            .into_iter().map(|r|Ok(LedgerEvent{sequence:r.get(0),request_id:RequestId::new(r.get::<String,_>(1))?,actor:PrincipalId::new(r.get::<String,_>(2))?,recorded_at:r.get(3),request_json:r.get(4)})).collect()
    }
    /// Commit an authorized financial operation. Identical deliveries return the
    /// original receipt only after current authorization is rechecked. Payload,
    /// actor, evidence and version changes under the same key are conflicts.
    /// R06 must compose the hold logic in its own single admission transaction;
    /// calling this API followed by a launch would NOT satisfy ADR 0031.
    pub async fn apply_financial(
        &self,
        request: &FinancialRequest,
        authorization: &dyn FinancialAuthorization,
    ) -> Result<FinancialReceipt, FinanceError> {
        if request.reason.trim().is_empty()
            || request.reason.len() > 512
            || request.reason.chars().any(char::is_control)
        {
            return Err(FinanceError::InvalidInput);
        }
        let payload = serde_json::to_string(&serde_json::json!({"format":1,"request":request}))
            .map_err(|_| FinanceError::InvalidInput)?;
        if payload.len() > 16384 {
            return Err(FinanceError::InvalidInput);
        }
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let owner: String =
            sqlx::query_scalar("SELECT owner_actor FROM projects WHERE project_id=?")
                .bind(request.project.as_str())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        let owner = PrincipalId::new(owner)?;
        let at = now()?;
        if !authorization.permits(&owner, request, at)
            || matches!(request.command, FinancialCommand::Approve { .. }) && owner != request.actor
        {
            return Err(StoreError::Forbidden.into());
        }
        if let Some(row) = sqlx::query(
            "SELECT sequence,payload FROM financial_events WHERE project_id=? AND request_id=?",
        )
        .bind(request.project.as_str())
        .bind(request.request_id.as_str())
        .fetch_optional(&mut *tx)
        .await?
        {
            if row.get::<String, _>(1) != payload {
                return Err(StoreError::IdempotencyConflict.into());
            }
            verify_application(&mut tx, row.get(0), request).await?;
            let receipt = FinancialReceipt {
                sequence: row.get(0),
                request_id: request.request_id.clone(),
            };
            tx.commit().await?;
            return Ok(receipt);
        }
        let currency = match &request.command {
            FinancialCommand::Approve { new_ceiling, .. } => {
                let present: Option<String> = sqlx::query_scalar(
                    "SELECT currency FROM financial_accounts WHERE project_id=?",
                )
                .bind(request.project.as_str())
                .fetch_optional(&mut *tx)
                .await?;
                if let Some(value) = present {
                    Currency::new(&value).map_err(|_| StoreError::Integrity)?
                } else {
                    require_one(
                        sqlx::query(
                            "INSERT INTO financial_accounts(project_id,currency) VALUES (?,?)",
                        )
                        .bind(request.project.as_str())
                        .bind(new_ceiling.currency().to_string())
                        .execute(&mut *tx)
                        .await?
                        .rows_affected(),
                    )?;
                    new_ceiling.currency()
                }
            }
            _ => account(&mut tx, &request.project).await?,
        };
        let billing_evidence = match &request.command {
            FinancialCommand::Report { evidence, .. }
            | FinancialCommand::Reconcile { evidence, .. } => Some(evidence.as_str()),
            _ => None,
        };
        if let Some(evidence) = billing_evidence {
            let used:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM financial_events WHERE project_id=? AND billing_evidence=?)").bind(request.project.as_str()).bind(evidence).fetch_one(&mut *tx).await?;
            if used {
                return Err(StoreError::IdempotencyConflict.into());
            }
        }
        let next = prepare_position(&mut tx, request, currency).await?;
        if let FinancialCommand::Approve {
            expected_version,
            new_ceiling,
            proof,
        } = &request.command
        {
            proof_current(proof, at)?;
            if new_ceiling.currency() != currency {
                return Err(FinanceError::CurrencyMismatch);
            }
            let (version, old) = ceiling(&mut tx, &request.project).await?;
            if integer_version(*expected_version)? != version {
                return Err(FinanceError::StaleVersion);
            }
            if version > 0 && new_ceiling.minor_units() <= old {
                return Err(FinanceError::InvalidTransition);
            }
            let used: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM budget_approvals WHERE proof_id=?)",
            )
            .bind(proof.reference.as_str())
            .fetch_one(&mut *tx)
            .await?;
            if used {
                return Err(FinanceError::ProofConsumed);
            }
        }
        let sequence:i64=sqlx::query_scalar("INSERT INTO financial_events(project_id,request_id,actor,recorded_at,kind,payload,billing_evidence) VALUES (?,?,?,?,?,?,?) RETURNING sequence")
            .bind(request.project.as_str()).bind(request.request_id.as_str()).bind(request.actor.as_str()).bind(at).bind(request.command.kind()).bind(&payload).bind(billing_evidence).fetch_one(&mut *tx).await?;
        if let FinancialCommand::Approve {
            new_ceiling, proof, ..
        } = &request.command
        {
            let (version, old) = ceiling(&mut tx, &request.project).await?;
            let next_version = version.checked_add(1).ok_or(FinanceError::Arithmetic)?;
            require_one(
                sqlx::query("INSERT INTO budget_approvals VALUES (?,?,?,?,?,?,?)")
                    .bind(sequence)
                    .bind(request.project.as_str())
                    .bind(next_version)
                    .bind(old)
                    .bind(new_ceiling.minor_units())
                    .bind(proof.reference.as_str())
                    .bind(proof.verified_at)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
        }
        if let Some((id, balance)) = next {
            require_one(
                sqlx::query("INSERT INTO liability_revisions VALUES (?,?,?,?,?,?)")
                    .bind(sequence)
                    .bind(request.project.as_str())
                    .bind(id.as_str())
                    .bind(integer_version(balance.revision)?)
                    .bind(balance.uncertain)
                    .bind(balance.finalized)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
            let settled = amounts(balance.settled);
            let held = amounts(balance.held);
            let provisional = amounts(balance.provisional);
            for (idx, category) in CATEGORIES.iter().enumerate() {
                require_one(
                    sqlx::query("INSERT INTO financial_positions VALUES (?,?,?,?,?)")
                        .bind(sequence)
                        .bind(category)
                        .bind(settled[idx])
                        .bind(held[idx])
                        .bind(provisional[idx])
                        .execute(&mut *tx)
                        .await?
                        .rows_affected(),
                )?;
            }
            // Verify the durable position, not just successful SQL completion.
            if liability(&mut tx, &request.project, &id, currency)
                .await?
                .as_ref()
                != Some(&balance)
            {
                return Err(StoreError::Integrity.into());
            }
        }
        verify_application(&mut tx, sequence, request).await?;
        let commit_at = now()?;
        if commit_at < at || !authorization.permits(&owner, request, commit_at) {
            return Err(StoreError::Forbidden.into());
        }
        if let FinancialCommand::Approve { proof, .. } = &request.command {
            proof_current(proof, commit_at)?;
        }
        tx.commit().await?;
        Ok(FinancialReceipt {
            sequence,
            request_id: request.request_id.clone(),
        })
    }
}
/// Check the complete committed application on replay, including archived
/// revisions. A matching event header by itself is never a success receipt.
async fn verify_application(
    c: &mut SqliteConnection,
    sequence: i64,
    request: &FinancialRequest,
) -> Result<(), FinanceError> {
    if let FinancialCommand::Approve {
        expected_version,
        new_ceiling,
        proof,
    } = &request.command
    {
        let version = integer_version(*expected_version)?
            .checked_add(1)
            .ok_or(FinanceError::Arithmetic)?;
        let matches: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM budget_approvals WHERE event_sequence=? AND project_id=? AND version=? AND new_ceiling=? AND proof_id=? AND proof_verified_at=?)")
            .bind(sequence).bind(request.project.as_str()).bind(version).bind(new_ceiling.minor_units()).bind(proof.reference.as_str()).bind(proof.verified_at).fetch_one(&mut *c).await?;
        let currency = account(c, &request.project).await?;
        if !matches || new_ceiling.currency() != currency {
            return Err(StoreError::Integrity.into());
        }
        return Ok(());
    }
    let (id, version) = match &request.command {
        FinancialCommand::Reserve { liability, .. } => (liability, 1),
        FinancialCommand::Report {
            liability,
            expected_revision,
            ..
        }
        | FinancialCommand::Reconcile {
            liability,
            expected_revision,
            ..
        } => (
            liability,
            integer_version(*expected_revision)?
                .checked_add(1)
                .ok_or(FinanceError::Arithmetic)?,
        ),
        FinancialCommand::Approve { .. } => return Err(StoreError::Integrity.into()),
    };
    let matches: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM liability_revisions WHERE event_sequence=? AND project_id=? AND liability_id=? AND version=?)")
        .bind(sequence).bind(request.project.as_str()).bind(id.as_str()).bind(version).fetch_one(&mut *c).await?;
    if !matches {
        return Err(StoreError::Integrity.into());
    }
    let rows = sqlx::query("SELECT category,settled_minor,held_minor,provisional_minor FROM financial_positions WHERE event_sequence=?")
        .bind(sequence).fetch_all(&mut *c).await?;
    if rows.len() != 4 {
        return Err(StoreError::Integrity.into());
    }
    for row in rows {
        let index = CATEGORIES
            .iter()
            .position(|name| *name == row.get::<String, _>(0))
            .ok_or(StoreError::Integrity)?;
        let position = [
            row.get::<i64, _>(1),
            row.get::<i64, _>(2),
            row.get::<i64, _>(3),
        ];
        let valid = match &request.command {
            FinancialCommand::Reserve { maximum, .. } => {
                position == [0, amounts(*maximum)[index], 0]
            }
            FinancialCommand::Report { unreserved, .. } => {
                position[2] == amounts(*unreserved)[index]
            }
            FinancialCommand::Reconcile {
                settled,
                remaining_hold,
                remaining_provisional,
                ..
            } => {
                position
                    == [
                        amounts(*settled)[index],
                        amounts(*remaining_hold)[index],
                        amounts(*remaining_provisional)[index],
                    ]
            }
            FinancialCommand::Approve { .. } => false,
        };
        if !valid {
            return Err(StoreError::Integrity.into());
        }
    }
    Ok(())
}
async fn prepare_position(
    c: &mut SqliteConnection,
    request: &FinancialRequest,
    currency: Currency,
) -> Result<Option<(LiabilityId, LiabilityBalance)>, FinanceError> {
    let empty = zero(currency)?;
    match &request.command {
        FinancialCommand::Approve { .. } => Ok(None),
        FinancialCommand::Reserve {
            liability: id,
            maximum,
            ..
        } => {
            require_currency(*maximum, currency)?;
            if maximum.total()?.minor_units() == 0 {
                return Err(FinanceError::InvalidInput);
            }
            if liability(c, &request.project, id, currency)
                .await?
                .is_some()
            {
                return Err(FinanceError::InvalidTransition);
            }
            snapshot(c, &request.project)
                .await?
                .require_hold(*maximum)?;
            Ok(Some((
                id.clone(),
                LiabilityBalance {
                    revision: 1,
                    settled: empty,
                    held: *maximum,
                    provisional: empty,
                    uncertain: false,
                    finalized: false,
                },
            )))
        }
        FinancialCommand::Report {
            liability: id,
            expected_revision,
            unreserved,
            uncertain,
            ..
        } => {
            require_currency(*unreserved, currency)?;
            let old = liability(c, &request.project, id, currency)
                .await?
                .unwrap_or(LiabilityBalance {
                    revision: 0,
                    settled: empty,
                    held: empty,
                    provisional: empty,
                    uncertain: false,
                    finalized: false,
                });
            if *expected_revision != old.revision {
                return Err(FinanceError::StaleVersion);
            }
            let values = amounts(*unreserved);
            let previous = amounts(old.provisional);
            if (0..4).any(|i| values[i] < previous[i]) {
                return Err(FinanceError::InvalidTransition);
            }
            let changed = *unreserved != old.provisional;
            Ok(Some((
                id.clone(),
                LiabilityBalance {
                    revision: old
                        .revision
                        .checked_add(1)
                        .ok_or(FinanceError::Arithmetic)?,
                    provisional: *unreserved,
                    uncertain: *uncertain || old.uncertain || old.finalized && changed,
                    finalized: old.finalized && !changed,
                    ..old
                },
            )))
        }
        FinancialCommand::Reconcile {
            liability: id,
            expected_revision,
            settled,
            remaining_hold,
            remaining_provisional,
            state,
            ..
        } => {
            for cost in [settled, remaining_hold, remaining_provisional] {
                require_currency(*cost, currency)?;
            }
            let old = liability(c, &request.project, id, currency)
                .await?
                .ok_or(StoreError::NotFound)?;
            if *expected_revision != old.revision {
                return Err(FinanceError::StaleVersion);
            }
            let settled_values = amounts(*settled);
            let held_values = amounts(*remaining_hold);
            let old_settled = amounts(old.settled);
            let old_held = amounts(old.held);
            let old_provisional = amounts(old.provisional);
            if (0..4).any(|i| settled_values[i] < old_settled[i] || held_values[i] > old_held[i]) {
                return Err(FinanceError::InvalidTransition);
            }
            let provisional_values = amounts(*remaining_provisional);
            for i in 0..4 {
                // Reconciliation consumes existing exposure. Newly discovered
                // additional unreserved exposure belongs in a separate Report.
                let available = i128::from(old_held[i]) + i128::from(old_provisional[i]);
                let spent = i128::from(settled_values[i]) - i128::from(old_settled[i]);
                let remaining = i128::from(held_values[i]) + i128::from(provisional_values[i]);
                if provisional_values[i] > old_provisional[i]
                    || spent <= available && remaining > available - spent
                {
                    return Err(FinanceError::InvalidTransition);
                }
            }
            if *state == ReconciliationState::Final
                && (remaining_hold.total()?.minor_units() != 0
                    || remaining_provisional.total()?.minor_units() != 0)
            {
                return Err(FinanceError::InvalidTransition);
            }
            // Record real unexpected charges even beyond the ceiling. They never
            // imply approval; force a separate reconciliation before new holds.
            let unexpected = (0..4).any(|i| {
                i128::from(settled_values[i])
                    > i128::from(old_settled[i])
                        + i128::from(old_held[i])
                        + i128::from(old_provisional[i])
            });
            let uncertain = *state == ReconciliationState::Uncertain || unexpected;
            Ok(Some((
                id.clone(),
                LiabilityBalance {
                    revision: old
                        .revision
                        .checked_add(1)
                        .ok_or(FinanceError::Arithmetic)?,
                    settled: *settled,
                    held: *remaining_hold,
                    provisional: *remaining_provisional,
                    uncertain,
                    finalized: *state == ReconciliationState::Final && !uncertain,
                },
            )))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_owner_proof_window_excludes_expiry_future_and_negative_time() {
        let proof = OwnerProof {
            reference: EvidenceRef::new("proof").unwrap(),
            verified_at: 1000,
        };
        assert!(proof_current(&proof, 1000).is_ok());
        assert!(proof_current(&proof, 1299).is_ok());
        assert_eq!(proof_current(&proof, 1300), Err(FinanceError::ProofExpired));
        assert_eq!(proof_current(&proof, 999), Err(FinanceError::ProofExpired));
        assert_eq!(
            proof_current(
                &OwnerProof {
                    verified_at: -1,
                    ..proof
                },
                0
            ),
            Err(FinanceError::ProofExpired)
        );
    }
}
