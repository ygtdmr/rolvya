use super::*;
use crate::worker_simulation::{decode_worker_message, encode_worker_message};
use worker_control::CredentialScopeUse;
struct Permit<'a> {
    peer: &'a EvidenceRef,
    identity: &'a VerifiedWorkerIdentity,
    binding: &'a WorkerBinding,
    since: i64,
    until: i64,
}
impl WorkerAuthority for Permit<'_> {
    fn permits(
        &self,
        peer: &PeerEvidence,
        binding: &WorkerBinding,
        op: WorkerOperation,
        declared: Option<&DeclaredCapabilities>,
        now: i64,
    ) -> bool {
        peer.reference == *self.peer
            && *binding == *self.binding
            && now >= self.since
            && now < self.until
            && self
                .identity
                .permissions
                .iter()
                .any(|p| p.operation() == op)
            && declared.map_or(
                matches!(op, WorkerOperation::StopQuery | WorkerOperation::StopAck),
                |c| *c == self.identity.capabilities,
            )
    }
}
struct ControllerClock<'a>(&'a dyn Clock);
impl Clock for ControllerClock<'_> {
    fn now(&self) -> Result<i64, admission::AdmissionError> {
        self.0.now()
    }
}
impl WorkerSecurity {
    /// Authenticate and apply through the existing durable journal in the SAME
    /// writer transaction. No cached permit escapes this operation.
    pub async fn receive(
        &self,
        credential: &WorkerCredential,
        bytes: &[u8],
        context: &WorkerRequestContext,
    ) -> Result<ControllerMessage, WorkerAuthError> {
        self.receive_receipt(credential, bytes, context)
            .await
            .map(|(message, _)| message)
    }
    /// Transport disposition is derived inside the same writer transaction as
    /// receipt processing; a preliminary cache lookup is not authoritative.
    pub(super) async fn receive_receipt(
        &self,
        credential: &WorkerCredential,
        bytes: &[u8],
        context: &WorkerRequestContext,
    ) -> Result<(ControllerMessage, bool), WorkerAuthError> {
        if bytes.is_empty() || bytes.len() > MAX_MESSAGE_BYTES {
            return Err(WorkerAuthError::Protocol);
        }
        no_bearer_reference(
            credential,
            &[
                bytes,
                context.request_id.as_str().as_bytes(),
                context.transport_evidence.as_str().as_bytes(),
            ],
        )?;
        let message = decode_worker_message(bytes)?;
        // Check decoded canonical data as well: JSON Unicode escapes must not
        // smuggle a bearer into a persisted ID or protocol field.
        no_bearer_reference(credential, &[&encode_worker_message(&message)?])?;
        let result = self
            .apply_authenticated(credential, &message, context)
            .await;
        if let Err(error) = &result
            && !matches!(
                error,
                WorkerAuthError::Busy
                    | WorkerAuthError::Storage
                    | WorkerAuthError::Unavailable
                    | WorkerAuthError::KeyUnavailable
            )
        {
            self.audit_authenticated_denial(
                credential,
                context,
                crate::worker_simulation::controller::operation(&message.body),
            )
            .await?;
        }
        result
    }
    async fn apply_authenticated(
        &self,
        credential: &WorkerCredential,
        message: &WorkerMessage,
        context: &WorkerRequestContext,
    ) -> Result<(ControllerMessage, bool), WorkerAuthError> {
        let op = crate::worker_simulation::controller::operation(&message.body);
        let stop = matches!(op, WorkerOperation::StopQuery | WorkerOperation::StopAck);
        let usage = || {
            if stop {
                CredentialScopeUse::Stop
            } else {
                CredentialScopeUse::Control
            }
        };
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let now = self.now()?;
        let current = self.current_credential(&mut tx, credential, now).await?;
        let record = &current.enrollment;
        if message.binding != record.scope.binding {
            return Err(WorkerAuthError::Denied);
        }
        self.transport
            .verify(context, &record.scope.binding, op, now)?;
        if self.scope(&mut tx, &record.lease(), now, usage()).await? != record.scope {
            return Err(WorkerAuthError::Integrity);
        }
        let old_lease = admission::stored_admission(&mut tx, &record.lease())
            .await
            .map_err(WorkerError::from)?
            .1;
        let canonical = encode_worker_message(message)?;
        let digest = format!("{:x}", Sha256::digest(&canonical));
        let old=sqlx::query("SELECT digest,message_id,credential_id FROM worker_credential_requests WHERE enrollment_id=? AND request_id=?")
            .bind(record.id.as_str()).bind(context.request_id.as_str()).fetch_optional(&mut *tx).await?;
        if let Some(old) = &old
            && (old.try_get::<&str, _>("digest")? != digest
                || old.try_get::<&str, _>("message_id")? != message.message_id.as_str()
                || old.try_get::<&str, _>("credential_id")? != current.metadata.id.as_str())
        {
            return Err(WorkerAuthError::Conflict);
        }
        if old.is_none() {
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM worker_credential_requests WHERE enrollment_id=?",
            )
            .bind(record.id.as_str())
            .fetch_one(&mut *tx)
            .await?;
            if count >= 8192 {
                return Err(WorkerAuthError::ResourceLimit);
            }
        }
        let peer = PeerEvidence {
            reference: EvidenceRef::new(format!("credential-{}", current.metadata.id.as_str()))
                .map_err(|_| WorkerAuthError::Integrity)?,
        };
        let permit = Permit {
            peer: &peer.reference,
            identity: &record.identity,
            binding: &record.scope.binding,
            since: current.metadata.issued_at,
            until: current.metadata.expires_at,
        };
        let result = worker_control::receive_in(
            &mut tx,
            &peer,
            &canonical,
            &permit,
            self.policy.as_ref(),
            &ControllerClock(self.clock.as_ref()),
        )
        .await?;
        if old.is_none() {
            let event = self
                .event(&mut tx, record, &context.request_id, "message", now)
                .await?;
            one(
                sqlx::query("INSERT INTO worker_credential_requests VALUES(?,?,?,?,?,?,?,?)")
                    .bind(record.id.as_str())
                    .bind(context.request_id.as_str())
                    .bind(current.metadata.id.as_str())
                    .bind(record.scope.binding.task.plan.project_id.as_str())
                    .bind(record.scope.binding.attempt_id.as_str())
                    .bind(message.message_id.as_str())
                    .bind(digest)
                    .bind(event)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
        }
        self.checkpoint(CommitCheckpoint::BeforeMessage)?;
        let end = self.now()?;
        // An in-transaction heartbeat must not validate itself against its own
        // newly extended lease after the original authority has already expired.
        if end < now || (!stop && end >= old_lease) {
            return Err(WorkerAuthError::Expired);
        }
        let end_current = self.current_credential(&mut tx, credential, end).await?;
        if end_current.metadata != current.metadata
            || self.scope(&mut tx, &record.lease(), end, usage()).await? != record.scope
        {
            return Err(WorkerAuthError::Denied);
        }
        self.transport
            .verify(context, &record.scope.binding, op, end)?;
        tx.commit().await?;
        self.checkpoint(CommitCheckpoint::AfterMessage)?;
        Ok((result, old.is_some()))
    }
    /// Only a verified current token plus trusted transport can attribute a
    /// denial to a project. Unknown network traffic remains a future gateway gate.
    /// Rejections are idempotent and bounded to 128 records per enrollment.
    async fn audit_authenticated_denial(
        &self,
        credential: &WorkerCredential,
        context: &WorkerRequestContext,
        op: WorkerOperation,
    ) -> Result<(), WorkerAuthError> {
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let now = self.now()?;
        let current = match self.current_credential(&mut tx, credential, now).await {
            Ok(c) => c,
            Err(
                WorkerAuthError::Denied | WorkerAuthError::Expired | WorkerAuthError::Integrity,
            ) => return Ok(()),
            Err(e) => return Err(e),
        };
        if self
            .transport
            .verify(context, &current.enrollment.scope.binding, op, now)
            .is_err()
        {
            return Ok(());
        }
        let r = &current.enrollment;
        sqlx::query("INSERT OR IGNORE INTO worker_security_events(project_id,attempt_id,enrollment_id,request_id,kind,recorded_at) SELECT ?,?,?,?,'denied',? WHERE (SELECT count(*) FROM worker_security_events WHERE enrollment_id=? AND kind='denied')<128")
            .bind(r.scope.binding.task.plan.project_id.as_str()).bind(r.scope.binding.attempt_id.as_str()).bind(r.id.as_str()).bind(context.request_id.as_str()).bind(now).bind(r.id.as_str()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
