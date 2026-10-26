use super::*;
use rolvya_domain::money::{CostBreakdown, Money};
struct RiskOnly;
impl FinancialAuthorization for RiskOnly {
    fn permits(&self, _: &PrincipalId, r: &FinancialRequest, _: i64) -> bool {
        matches!(
            r.command,
            FinancialCommand::Report {
                uncertain: true,
                ..
            }
        )
    }
}
pub(super) async fn mark_unknown(
    c: &mut SqliteConnection,
    cmd: &LaunchCommand,
    o: &ProviderObservation,
    _t: i64,
) -> Result<(), WorkerError> {
    let p = &cmd.binding.task.plan.project_id;
    let budget = finance::snapshot(c, p)
        .await
        .map_err(AdmissionError::from)?;
    let liability = LiabilityId::new(cmd.binding.attempt_id.as_str())?;
    let balance = finance::liability(c, p, &liability, budget.ceiling.currency())
        .await
        .map_err(AdmissionError::from)?
        .ok_or(StoreError::Integrity)?;
    if balance.uncertain {
        return Ok(());
    }
    let zero = Money::new(0, budget.ceiling.currency()).map_err(|_| StoreError::Integrity)?;
    let request = FinancialRequest {
        project: p.clone(),
        request_id: RequestId::new(format!("risk-{}", &hash(&json(o)?)[..48]))?,
        actor: admission::owner(c, p).await?,
        reason: "Trusted provider observation cannot bound remaining exposure".into(),
        command: FinancialCommand::Report {
            liability,
            expected_revision: balance.revision,
            unreserved: CostBreakdown {
                ai_api: zero,
                compute: zero,
                storage: zero,
                transfer: zero,
            },
            uncertain: true,
            evidence: o.evidence.clone(),
        },
    };
    finance::apply_in(c, &request, &RiskOnly)
        .await
        .map_err(AdmissionError::from)?;
    Ok(())
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryCause {
    Expired,
    Risk,
    Revoked,
    Canceled,
    Fenced,
}
impl RecoveryCause {
    fn name(self) -> &'static str {
        match self {
            Self::Expired => "expired",
            Self::Risk => "risk",
            Self::Revoked => "revoked",
            Self::Canceled => "canceled",
            Self::Fenced => "fenced",
        }
    }
}
impl OutboxDispatcher {
    /// Expiration scan uses the lease index and a bounded keyset. Risk/revocation
    /// events use recover_attempt directly; caller supplies trusted fence authority.
    pub async fn expired(
        &self,
        project: &ProjectId,
        clock: &impl Clock,
        limit: u16,
    ) -> Result<Vec<LeaseRef>, WorkerError> {
        if !(1..=100).contains(&limit) {
            return Err(WorkerError::ResourceLimit);
        }
        let t = admission::at(clock)?;
        let rows=sqlx::query("SELECT a.plan_id,a.version,a.task_id,a.attempt_id,a.epoch FROM worker_leases l JOIN task_attempts a USING(project_id,attempt_id) JOIN attempt_states s USING(project_id,attempt_id) WHERE l.project_id=? AND l.lease_until<=? AND s.state='admitted' ORDER BY l.lease_until,l.attempt_id LIMIT ?").bind(project.as_str()).bind(t).bind(limit).fetch_all(&self.store.pool).await?;
        rows.into_iter()
            .map(|r| {
                Ok(LeaseRef {
                    task: rolvya_domain::plan::TaskRef {
                        plan: rolvya_domain::plan::PlanKey {
                            project_id: project.clone(),
                            plan_id: PlanId::new(r.get::<String, _>(0))
                                .map_err(|_| StoreError::Integrity)?,
                            version: PlanVersion::new(r.get::<i64, _>(1) as u64)
                                .map_err(|_| StoreError::Integrity)?,
                        },
                        task_id: TaskId::new(r.get::<String, _>(2))
                            .map_err(|_| StoreError::Integrity)?,
                    },
                    attempt_id: AttemptId::new(r.get::<String, _>(3))
                        .map_err(|_| StoreError::Integrity)?,
                    epoch: r.get::<i64, _>(4) as u64,
                })
            })
            .collect()
    }
    /// Fencing + recovery audit + dispatch block commit together. No hold release,
    /// replacement attempt, provider termination claim or automatic retry occurs.
    pub async fn recover_attempt<P: SchedulerPolicy>(
        &self,
        r: &FenceRequest,
        cause: RecoveryCause,
        policy: &P,
        clock: &impl Clock,
    ) -> Result<u64, WorkerError> {
        let l = &r.lease;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let t = admission::at(clock)?;
        let owner = admission::owner(&mut tx, &l.task.plan.project_id).await?;
        if !policy.permits(&owner, SchedulerAction::Fence(r), t) {
            return Err(WorkerError::Unauthorized);
        }
        // Scope verification occurs even on replay.
        let (_, until, status) = admission::stored_admission(&mut tx, l).await?;
        let old: Option<i64> = sqlx::query_scalar(
            "SELECT next_epoch FROM worker_recoveries WHERE project_id=? AND attempt_id=?",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(epoch) = old {
            tx.commit().await?;
            return Ok(epoch as u64);
        }
        if cause == RecoveryCause::Expired && t < until {
            return Err(WorkerError::InvalidState);
        }
        let next = if status == "fenced" {
            l.epoch.checked_add(1).ok_or(WorkerError::ResourceLimit)?
        } else {
            admission::fence_in(&mut tx, r, policy, t).await?.0
        };
        let e = audit(
            &mut tx,
            l,
            "recovery",
            t,
            &serde_json::json!({"reason":cause,"next_epoch":next,"evidence":r.evidence}),
        )
        .await?;
        affected(
            sqlx::query("INSERT INTO worker_recoveries VALUES(?,?,?,?,?)")
                .bind(l.task.plan.project_id.as_str())
                .bind(l.attempt_id.as_str())
                .bind(i64::try_from(next).map_err(|_| WorkerError::ResourceLimit)?)
                .bind(cause.name())
                .bind(e)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM dispatch_operations WHERE project_id=? AND attempt_id=?)",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if exists {
            let (_, state, revision) = command(&mut tx, l).await?;
            if !matches!(
                state,
                DispatchState::Finished | DispatchState::NoEffect | DispatchState::Blocked
            ) {
                transition(&mut tx, l, DispatchState::Blocked, revision, e).await?;
            }
        }
        let end = admission::at(clock)?;
        if end < t || !policy.permits(&owner, SchedulerAction::Fence(r), end) {
            return Err(WorkerError::Unauthorized);
        }
        tx.commit().await?;
        Ok(next)
    }
}
/// Legal-hold/retention authority is external to worker inputs. No permissive
/// production implementation exists. Archive/legal deletion remains a later gate.
pub trait RetentionAuthority: Send + Sync {
    fn permits(&self, project: &ProjectId, now: i64) -> bool;
}
impl DurableWorkerController {
    /// Bounded receipt-body compaction, conservatively after 1096 days (>=3 years),
    /// final financial reconciliation, provider termination/no-effect evidence and
    /// retired authority. Identity digests, audit links and compacted tombstones stay.
    pub async fn compact_receipts(
        &self,
        project: &ProjectId,
        authority: &impl RetentionAuthority,
        clock: &impl Clock,
        limit: u16,
    ) -> Result<u64, WorkerError> {
        if !(1..=100).contains(&limit) {
            return Err(WorkerError::ResourceLimit);
        }
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let t = admission::at(clock)?;
        if !authority.permits(project, t) {
            return Err(WorkerError::Unauthorized);
        }
        let cutoff = t
            .checked_sub(1096 * 86400)
            .ok_or(WorkerError::InvalidTime)?;
        let rows=sqlx::query("SELECT m.attempt_id,m.message_id FROM worker_messages m JOIN worker_receipts r USING(project_id,attempt_id,message_id) JOIN attempt_states s USING(project_id,attempt_id) JOIN dispatch_operations d USING(project_id,attempt_id) WHERE m.project_id=? AND m.recorded_at<=? AND s.state IN ('fenced','succeeded') AND d.state IN ('finished','no_effect') ORDER BY m.recorded_at,m.attempt_id,m.message_id LIMIT ?").bind(project.as_str()).bind(cutoff).bind(limit).fetch_all(&mut *tx).await?;
        let mut count = 0;
        for row in rows {
            let attempt =
                AttemptId::new(row.get::<String, _>(0)).map_err(|_| StoreError::Integrity)?;
            let lease_row=sqlx::query("SELECT plan_id,version,task_id,epoch FROM task_attempts WHERE project_id=? AND attempt_id=?").bind(project.as_str()).bind(attempt.as_str()).fetch_one(&mut *tx).await?;
            let l = LeaseRef {
                task: rolvya_domain::plan::TaskRef {
                    plan: rolvya_domain::plan::PlanKey {
                        project_id: project.clone(),
                        plan_id: PlanId::new(lease_row.get::<String, _>(0))
                            .map_err(|_| StoreError::Integrity)?,
                        version: PlanVersion::new(lease_row.get::<i64, _>(1) as u64)
                            .map_err(|_| StoreError::Integrity)?,
                    },
                    task_id: TaskId::new(lease_row.get::<String, _>(2))
                        .map_err(|_| StoreError::Integrity)?,
                },
                attempt_id: attempt,
                epoch: lease_row.get::<i64, _>(3) as u64,
            };
            let budget = finance::snapshot(&mut tx, project)
                .await
                .map_err(AdmissionError::from)?;
            let balance = finance::liability(
                &mut tx,
                project,
                &LiabilityId::new(l.attempt_id.as_str())?,
                budget.ceiling.currency(),
            )
            .await
            .map_err(AdmissionError::from)?
            .ok_or(StoreError::Integrity)?;
            if !balance.finalized || balance.uncertain || budget.uncertain_liabilities > 0 {
                continue;
            }
            let id: String = row.get(1);
            let e = audit(
                &mut tx,
                &l,
                "compact",
                t,
                &serde_json::json!({"message_id":id,"policy":"critical-minimum-1096-days"}),
            )
            .await?;
            affected(
                sqlx::query("INSERT INTO worker_compactions VALUES(?,?,?,?)")
                    .bind(project.as_str())
                    .bind(l.attempt_id.as_str())
                    .bind(&id)
                    .bind(e)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
            count += sqlx::query(
                "DELETE FROM worker_receipts WHERE project_id=? AND attempt_id=? AND message_id=?",
            )
            .bind(project.as_str())
            .bind(l.attempt_id.as_str())
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        }
        let end = admission::at(clock)?;
        if end < t || !authority.permits(project, end) {
            return Err(WorkerError::Unauthorized);
        }
        tx.commit().await?;
        Ok(count)
    }
}
#[derive(Debug)]
pub struct RecoveryOutcome {
    pub lease: LeaseRef,
    pub result: Result<u64, WorkerError>,
}
impl OutboxDispatcher {
    /// One bounded recovery pass; scheduling/backoff belongs to the caller. Each
    /// victim is rechecked and fenced in its own short authoritative transaction.
    /// A concurrent valid heartbeat may make a scanned lease ineligible to expire.
    pub async fn recover_expired<P: SchedulerPolicy>(
        &self,
        project: &ProjectId,
        actor: &PrincipalId,
        policy: &P,
        clock: &impl Clock,
        limit: u16,
    ) -> Result<Vec<RecoveryOutcome>, WorkerError> {
        let leases = self.expired(project, clock, limit).await?;
        let mut outcomes = Vec::with_capacity(leases.len());
        for lease in leases {
            let request = FenceRequest {
                lease: lease.clone(),
                request_id: identity("expire", &lease)?,
                actor: actor.clone(),
                evidence: EvidenceRef::new("persisted-lease-expiration")?,
            };
            let result = self
                .recover_attempt(&request, RecoveryCause::Expired, policy, clock)
                .await;
            outcomes.push(RecoveryOutcome { lease, result });
        }
        Ok(outcomes)
    }
}
