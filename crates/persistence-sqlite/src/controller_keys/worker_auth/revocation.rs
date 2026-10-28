use super::*;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerRevocation {
    pub enrollment: WorkerEnrollmentId,
    pub expected_security_epoch: i64,
    pub request_id: RequestId,
    pub evidence: EvidenceRef,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerRevocationReceipt {
    pub enrollment: WorkerEnrollmentId,
    pub security_epoch: i64,
    pub fencing_epoch: Option<u64>,
    pub audit_event: i64,
}
impl WorkerSecurity {
    /// Controller incident authority is separate from worker possession/identity.
    /// Fencing and credential retirement commit together; no provider is stopped
    /// here and no financial reservation is released.
    pub async fn revoke_worker(
        &self,
        r: &WorkerRevocation,
    ) -> Result<WorkerRevocationReceipt, WorkerAuthError> {
        if !(1..i64::MAX).contains(&r.expected_security_epoch) {
            return Err(WorkerAuthError::InvalidInput);
        }
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let t = self.now()?;
        let record = self.enrollment(&mut tx, &r.enrollment).await?;
        self.revocation.verify(r, &record, t)?;
        let old = sqlx::query("SELECT * FROM worker_revocations WHERE enrollment_id=?")
            .bind(r.enrollment.as_str())
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(old) = old {
            if old.try_get::<&str, _>("request_id")? != r.request_id.as_str()
                || old.try_get::<&str, _>("evidence")? != r.evidence.as_str()
                || old.try_get::<i64, _>("expected_security_epoch")? != r.expected_security_epoch
            {
                return Err(WorkerAuthError::Conflict);
            }
            let receipt = WorkerRevocationReceipt {
                enrollment: r.enrollment.clone(),
                security_epoch: old.try_get("security_epoch")?,
                fencing_epoch: old
                    .try_get::<Option<i64>, _>("fencing_epoch")?
                    .map(|v| v as u64),
                audit_event: old.try_get("event_sequence")?,
            };
            let end = self.now()?;
            if end < t {
                return Err(WorkerAuthError::Denied);
            }
            self.revocation.verify(r, &record, end)?;
            tx.commit().await?;
            return Ok(receipt);
        }
        let current = sqlx::query(
            "SELECT state,security_epoch FROM worker_enrollments WHERE enrollment_id=?",
        )
        .bind(r.enrollment.as_str())
        .fetch_one(&mut *tx)
        .await?;
        let epoch: i64 = current.try_get("security_epoch")?;
        let retired = current.try_get::<&str, _>("state")? == "revoked";
        if epoch != r.expected_security_epoch
            && !(retired && epoch == r.expected_security_epoch + 1)
        {
            return Err(WorkerAuthError::Conflict);
        }
        let (_, _, status) = admission::stored_admission(&mut tx, &record.lease())
            .await
            .map_err(WorkerError::from)?;
        let request = admission::FenceRequest {
            lease: record.lease(),
            request_id: r.request_id.clone(),
            actor: admission::stored_admission(&mut tx, &record.lease())
                .await
                .map_err(WorkerError::from)?
                .0
                .actor,
            evidence: r.evidence.clone(),
        };
        let fencing_epoch = if status == "admitted" {
            Some(
                worker_control::recover_in(
                    &mut tx,
                    &request,
                    worker_control::RecoveryCause::Revoked,
                    self.policy.as_ref(),
                    t,
                )
                .await?,
            )
        } else if status == "fenced" {
            Some(
                record
                    .scope
                    .binding
                    .epoch
                    .checked_add(1)
                    .ok_or(WorkerAuthError::Integrity)?,
            )
        } else {
            None
        };
        let final_state = sqlx::query(
            "SELECT state,security_epoch FROM worker_enrollments WHERE enrollment_id=?",
        )
        .bind(r.enrollment.as_str())
        .fetch_one(&mut *tx)
        .await?;
        let final_epoch: i64 = final_state.try_get("security_epoch")?;
        if final_state.try_get::<&str, _>("state")? != "revoked"
            || final_epoch != if retired { epoch } else { epoch + 1 }
        {
            return Err(WorkerAuthError::Integrity);
        }
        let event = self
            .event(&mut tx, &record, &r.request_id, "revoked", t)
            .await?;
        one(
            sqlx::query("INSERT INTO worker_revocations VALUES(?,?,?,?,?,?,?)")
                .bind(r.enrollment.as_str())
                .bind(r.request_id.as_str())
                .bind(r.evidence.as_str())
                .bind(r.expected_security_epoch)
                .bind(final_epoch)
                .bind(fencing_epoch.map(|e| e as i64))
                .bind(event)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        self.checkpoint(CommitCheckpoint::BeforeRevoke)?;
        let end = self.now()?;
        if end < t {
            return Err(WorkerAuthError::Denied);
        }
        self.revocation.verify(r, &record, end)?;
        if status == "admitted" {
            let owner = admission::owner(&mut tx, &record.scope.binding.task.plan.project_id)
                .await
                .map_err(WorkerError::from)?;
            if !self
                .policy
                .permits(&owner, SchedulerAction::Fence(&request), end)
            {
                return Err(WorkerAuthError::Denied);
            }
        }
        tx.commit().await?;
        self.checkpoint(CommitCheckpoint::AfterRevoke)?;
        Ok(WorkerRevocationReceipt {
            enrollment: r.enrollment.clone(),
            security_epoch: final_epoch,
            fencing_epoch,
            audit_event: event,
        })
    }
}
