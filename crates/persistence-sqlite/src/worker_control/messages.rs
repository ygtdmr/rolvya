use super::*;
use crate::worker_simulation::controller::operation;
/// Durable controller without a process-local session or receipt cache.
#[derive(Clone)]
pub struct DurableWorkerController {
    pub(super) store: PlanStore,
}
impl DurableWorkerController {
    pub fn new(store: PlanStore) -> Self {
        Self { store }
    }
    pub async fn binding(&self, l: &LeaseRef) -> Result<WorkerBinding, WorkerError> {
        let mut tx = self.store.pool.begin().await?;
        let (cmd, _, _) = command(&mut tx, l).await?;
        let s = session(&mut tx, l, &cmd).await?;
        tx.commit().await?;
        Ok(s.binding)
    }
    pub async fn lease_until(&self, l: &LeaseRef) -> Result<i64, WorkerError> {
        let mut tx = self.store.pool.begin().await?;
        let (_, until, _) = admission::stored_admission(&mut tx, l).await?;
        tx.commit().await?;
        Ok(until)
    }
    pub async fn received_outcome(
        &self,
        l: &LeaseRef,
    ) -> Result<Option<ExecutionOutcome>, WorkerError> {
        let mut tx = self.store.pool.begin().await?;
        let (cmd, _, _) = command(&mut tx, l).await?;
        let s = session(&mut tx, l, &cmd).await?;
        tx.commit().await?;
        Ok(s.result)
    }
    /// The current verifier must validate fresh channel identity, capability
    /// evidence, revocation and expiry. An ID string never authenticates a worker.
    pub async fn receive<P: SchedulerPolicy>(
        &self,
        peer: &PeerEvidence,
        bytes: &[u8],
        authority: &impl WorkerAuthority,
        policy: &P,
        clock: &impl Clock,
        _limits: Limits,
    ) -> Result<ControllerMessage, WorkerError> {
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let message = decode_worker_message(bytes)?;
        if credential_managed(&mut tx, &message.binding).await? {
            return Err(WorkerError::Unauthorized);
        }
        let response = receive_in(&mut tx, peer, bytes, authority, policy, clock).await?;
        tx.commit().await?;
        Ok(response)
    }
}

// Internal composition seam: caller owns the authoritative transaction. No public
// credential approval callback, new journal or separate commit is introduced.
pub(crate) async fn receive_in<P: SchedulerPolicy + ?Sized>(
    tx: &mut SqliteConnection,
    peer: &PeerEvidence,
    bytes: &[u8],
    authority: &impl WorkerAuthority,
    policy: &P,
    clock: &impl Clock,
) -> Result<ControllerMessage, WorkerError> {
    let m = decode_worker_message(bytes)?;
    let l = LeaseRef {
        task: m.binding.task.clone(),
        attempt_id: m.binding.attempt_id.clone(),
        epoch: m.binding.epoch,
    };
    let t = admission::at(clock)?;
    let (cmd, dispatch_state, _) = command(&mut *tx, &l).await?;
    let mut s = session(&mut *tx, &l, &cmd).await?;
    if s.binding.worker_id != m.binding.worker_id {
        return Err(WorkerError::Unauthorized);
    }
    if s.binding != m.binding {
        return Err(WorkerError::ContextMismatch);
    }
    let declared = match &m.body {
        WorkerBody::Register { capabilities } => Some(capabilities),
        _ => s.capabilities.as_ref(),
    };
    let op = operation(&m.body);
    if !authority.permits(peer, &s.binding, op, declared, t) {
        return Err(WorkerError::Unauthorized);
    }
    let stop_query = matches!(m.body, WorkerBody::GetStopInstruction {});
    let stop_ack = matches!(m.body, WorkerBody::StopAcknowledged { .. });
    // No old receipt can resurrect execution privileges. Retired stop commands
    // are the only scoped exception, and still require the current peer verifier.
    let current_check = current(&mut *tx, &cmd, policy, t).await;
    let healthy = current_check.is_ok() && dispatch_state == DispatchState::Active;
    if !healthy {
        if !stop_query && !(stop_ack && s.stop.is_some()) {
            current_check?;
            return Err(WorkerError::InvalidState);
        }
        if s.stop.is_none() {
            let reason = match &current_check {
                Err(WorkerError::Admission(AdmissionError::Expired)) => StopReason::LeaseExpired,
                Err(WorkerError::Admission(AdmissionError::StaleFence)) => StopReason::Fenced,
                Err(WorkerError::FinancialRisk) => StopReason::BudgetRisk,
                Err(WorkerError::Unauthorized) => StopReason::AuthorityRevoked,
                Err(error) => return Err(error.clone()),
                Ok(()) => StopReason::BudgetRisk,
            };
            s.stop = Some(StopInstruction {
                instruction_id: m.message_id.clone(),
                reason,
                next_epoch: None,
            });
            s.phase = SimulatedPhase::Stopped;
        }
    }
    let canonical =
        String::from_utf8(encode_worker_message(&m)?).map_err(|_| StoreError::Integrity)?;
    let digest = hash(&canonical);
    let old=sqlx::query("SELECT m.digest,r.response FROM worker_messages m LEFT JOIN worker_receipts r USING(project_id,attempt_id,message_id) WHERE m.project_id=? AND m.attempt_id=? AND m.message_id=?")
            .bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).bind(m.message_id.as_str()).fetch_optional(&mut *tx).await?;
    if let Some(old) = old {
        if old.get::<String, _>(0) != digest {
            return Err(WorkerError::ReplayConflict);
        }
        let response: Option<String> = old.get(1);
        let response =
            decode_controller_message(response.ok_or(WorkerError::InvalidState)?.as_bytes())?;
        if !healthy
            && !matches!(
                response.body,
                ControllerBody::Stop { .. }
                    | ControllerBody::Acknowledged {
                        receipt: WorkerReceipt {
                            kind: ReceiptKind::StoppedReported,
                            ..
                        }
                    }
            )
        {
            return Err(WorkerError::InvalidState);
        }
        let end = admission::at(clock)?;
        if end < t || !authority.permits(peer, &s.binding, op, declared, end) {
            return Err(WorkerError::Unauthorized);
        }
        if healthy {
            current(&mut *tx, &cmd, policy, end).await?;
        }
        return Ok(response);
    }
    let expected = s
        .last_sequence
        .checked_add(1)
        .ok_or(WorkerError::ResourceLimit)?;
    if m.sequence != expected {
        return Err(WorkerError::OutOfOrder { expected });
    }
    // Bounded on-disk journal per admitted attempt, no unbounded RAM journal.
    if expected > 4096 {
        return Err(WorkerError::ResourceLimit);
    }
    s.lease_until = admission::stored_admission(&mut *tx, &l).await?.1;
    let previous_until = s.lease_until;
    let response = s.apply(&m, t, &Ok(()), &Ok(()))?;
    let mut until = s.lease_until;
    if matches!(m.body, WorkerBody::Heartbeat {}) {
        // The SchedulerPolicy's current Dispatch decision verifies the original
        // enforceable maximum for the entire interval. Hold coverage is checked
        // inside this transaction; no extra cost or token authority is created.
        until = until.max(
            t.checked_add(i64::from(LEASE_DURATION_SECONDS))
                .ok_or(WorkerError::InvalidTime)?
                .min(cmd.admission.permit.valid_until),
        );
    }
    let e=audit(&mut *tx,&l,"message",t,&serde_json::json!({"message_id":m.message_id,"digest":digest,"sequence":m.sequence,"lease_until":until})).await?;
    s.lease_until = until;
    affected(sqlx::query("UPDATE worker_sessions SET state=?,last_sequence=?,event_sequence=? WHERE project_id=? AND attempt_id=? AND last_sequence=?")
            .bind(json(&s)?).bind(i64::try_from(m.sequence).map_err(|_|WorkerError::ResourceLimit)?).bind(e).bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).bind(i64::try_from(m.sequence-1).map_err(|_|WorkerError::ResourceLimit)?).execute(&mut *tx).await?.rows_affected())?;
    affected(
        sqlx::query("INSERT INTO worker_messages VALUES(?,?,?,?,?,?,?)")
            .bind(l.task.plan.project_id.as_str())
            .bind(l.attempt_id.as_str())
            .bind(m.message_id.as_str())
            .bind(m.sequence as i64)
            .bind(digest)
            .bind(e)
            .bind(t)
            .execute(&mut *tx)
            .await?
            .rows_affected(),
    )?;
    let response_text = String::from_utf8(encode_controller_message(&response)?)
        .map_err(|_| StoreError::Integrity)?;
    affected(
        sqlx::query("INSERT INTO worker_receipts VALUES(?,?,?,?)")
            .bind(l.task.plan.project_id.as_str())
            .bind(l.attempt_id.as_str())
            .bind(m.message_id.as_str())
            .bind(response_text)
            .execute(&mut *tx)
            .await?
            .rows_affected(),
    )?;
    if healthy {
        affected(sqlx::query("UPDATE worker_leases SET lease_until=?,observed_at=? WHERE project_id=? AND attempt_id=?").bind(until).bind(t).bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).execute(&mut *tx).await?.rows_affected())?;
    }
    let end = admission::at(clock)?;
    let declared = match &m.body {
        WorkerBody::Register { capabilities } => Some(capabilities),
        _ => s.capabilities.as_ref(),
    };
    if end < t || !authority.permits(peer, &s.binding, op, declared, end) {
        return Err(WorkerError::Unauthorized);
    }
    if healthy {
        if end >= previous_until {
            return Err(AdmissionError::Expired.into());
        }
        current(&mut *tx, &cmd, policy, end).await?;
    }
    Ok(response)
}
