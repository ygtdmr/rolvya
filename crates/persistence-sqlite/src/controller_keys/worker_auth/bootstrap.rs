use super::*;
use worker_control::CredentialScopeUse;
impl WorkerSecurity {
    pub async fn prepare_bootstrap(
        &self,
        r: &PrepareBootstrap,
    ) -> Result<BootstrapTicket, WorkerAuthError> {
        let mut read = self.store.pool.begin().await?;
        let t = self.now()?;
        let scope = self
            .scope(&mut read, &r.lease, t, CredentialScopeUse::Bootstrap)
            .await?;
        read.commit().await?;
        let controller = self.controller();
        let check = IdentityCheck {
            controller: &controller,
            stage: VerificationStage::Prepare,
            request: &r.request_id,
            evidence: &r.evidence,
            binding: &scope.binding,
            challenge: None,
            credential: None,
            provider: &scope.provider,
            account: &scope.account,
            pool: &scope.pool,
            command_digest: &scope.command_digest,
        };
        let grant = self.identity.verify(&check, t).await?;
        self.grant_current(&check, &grant, self.now()?)?;
        if !grant.identity.capabilities.supports(&scope.profile) {
            return Err(WorkerAuthError::Denied);
        }
        let challenge = BootstrapChallengeId::parse(new_id()?)?;
        let enrollment = WorkerEnrollmentId::parse(new_id()?)?;
        let secret = BootstrapSecret(bearer()?);
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let now = self.now()?;
        if now < t
            || self
                .scope(&mut tx, &r.lease, now, CredentialScopeUse::Bootstrap)
                .await?
                != scope
        {
            return Err(WorkerAuthError::Denied);
        }
        let old=sqlx::query("SELECT enrollment_id,state FROM worker_enrollments WHERE project_id=? AND attempt_id=?")
            .bind(scope.binding.task.plan.project_id.as_str()).bind(scope.binding.attempt_id.as_str()).fetch_optional(&mut *tx).await?;
        if let Some(old) = old {
            let id = WorkerEnrollmentId::parse(old.try_get("enrollment_id")?)?;
            let record = self.enrollment(&mut tx, &id).await?;
            if record.request_id != r.request_id
                || record.evidence != r.evidence
                || old.try_get::<&str, _>("state")? != "pending"
                || now >= record.challenge_expires_at
            {
                return Err(WorkerAuthError::Conflict);
            }
            self.grant_current(&check, &grant, self.now()?)?;
            tx.commit().await?;
            return Ok(BootstrapTicket {
                id: record.challenge,
                enrollment: id,
                expires_at: record.challenge_expires_at,
                secret: None,
            });
        }
        let valid_until = scope.valid_until.min(grant.identity.valid_until);
        let expires_at = (now + 300).min(valid_until);
        if expires_at <= now {
            return Err(WorkerAuthError::Expired);
        }
        let record = EnrollmentRecord {
            format: 1,
            controller: controller.clone(),
            id: enrollment.clone(),
            challenge: challenge.clone(),
            scope: scope.clone(),
            identity: grant.identity.clone(),
            request_id: r.request_id.clone(),
            evidence: r.evidence.clone(),
            created_at: now,
            challenge_expires_at: expires_at,
            valid_until,
        };
        let text = encode(&record)?;
        let signature = self.mac(b"enrollment", &[text.as_bytes()]);
        one(sqlx::query(
            "INSERT INTO worker_enrollments VALUES(?,?,?,?,?,?,?,?,?,?,'pending',1,0,0,?,?)",
        )
        .bind(enrollment.as_str())
        .bind(scope.binding.task.plan.project_id.as_str())
        .bind(scope.binding.attempt_id.as_str())
        .bind(scope.binding.worker_id.as_str())
        .bind(scope.provider.as_str())
        .bind(scope.account.as_str())
        .bind(scope.pool.as_str())
        .bind(record.identity.execution.as_str())
        .bind(text)
        .bind(signature.as_slice())
        .bind(now)
        .bind(valid_until)
        .execute(&mut *tx)
        .await?
        .rows_affected())?;
        let commitment = self.mac(
            b"bootstrap-secret",
            &[
                challenge.as_str().as_bytes(),
                enrollment.as_str().as_bytes(),
                secret.0.0.as_bytes(),
            ],
        );
        one(sqlx::query(
            "INSERT INTO worker_bootstrap_challenges VALUES(?,?,?,?,?,?,'pending',NULL)",
        )
        .bind(challenge.as_str())
        .bind(enrollment.as_str())
        .bind(r.request_id.as_str())
        .bind(commitment.as_slice())
        .bind(now)
        .bind(expires_at)
        .execute(&mut *tx)
        .await?
        .rows_affected())?;
        self.event(&mut tx, &record, &r.request_id, "prepared", now)
            .await?;
        self.checkpoint(CommitCheckpoint::BeforePrepare)?;
        let end = self.now()?;
        if end < now
            || end >= expires_at
            || self
                .scope(&mut tx, &r.lease, end, CredentialScopeUse::Bootstrap)
                .await?
                != scope
        {
            return Err(WorkerAuthError::Denied);
        }
        self.grant_current(&check, &grant, end)?;
        tx.commit().await?;
        self.checkpoint(CommitCheckpoint::AfterPrepare)?;
        Ok(BootstrapTicket {
            id: challenge,
            enrollment,
            expires_at,
            secret: Some(secret),
        })
    }
    pub(super) async fn enrollment(
        &self,
        c: &mut SqliteConnection,
        id: &WorkerEnrollmentId,
    ) -> Result<EnrollmentRecord, WorkerAuthError> {
        let row = sqlx::query("SELECT * FROM worker_enrollments WHERE enrollment_id=?")
            .bind(id.as_str())
            .fetch_optional(c)
            .await?
            .ok_or(WorkerAuthError::Denied)?;
        let text: &str = row.try_get("record")?;
        let signature: Vec<u8> = row.try_get("record_mac")?;
        if text.len() > 16384 || !equal(&signature, &self.mac(b"enrollment", &[text.as_bytes()])) {
            return Err(WorkerAuthError::Integrity);
        }
        let r: EnrollmentRecord =
            serde_json::from_str(text).map_err(|_| WorkerAuthError::Integrity)?;
        r.scope
            .binding
            .validate()
            .map_err(|_| WorkerAuthError::Integrity)?;
        if r.format != 1
            || r.controller != self.controller()
            || r.id != *id
            || row.try_get::<&str, _>("project_id")?
                != r.scope.binding.task.plan.project_id.as_str()
            || row.try_get::<&str, _>("attempt_id")? != r.scope.binding.attempt_id.as_str()
            || row.try_get::<&str, _>("worker_id")? != r.scope.binding.worker_id.as_str()
            || row.try_get::<&str, _>("provider")? != r.scope.provider.as_str()
            || row.try_get::<&str, _>("account")? != r.scope.account.as_str()
            || row.try_get::<&str, _>("pool")? != r.scope.pool.as_str()
            || row.try_get::<&str, _>("execution_identity")? != r.identity.execution.as_str()
            || row.try_get::<i64, _>("created_at")? != r.created_at
            || row.try_get::<i64, _>("valid_until")? != r.valid_until
        {
            return Err(WorkerAuthError::Integrity);
        }
        Ok(r)
    }
    /// Delivery is a separate trusted callback after commit, never a raw export.
    /// Lost preparation responses cannot reconstruct the challenge secret.
    pub async fn deliver_bootstrap(
        &self,
        r: &PrepareBootstrap,
        ticket: &BootstrapTicket,
    ) -> Result<(), WorkerAuthError> {
        if ticket.secret.is_none() {
            return Err(WorkerAuthError::Unavailable);
        }
        let mut tx = self.store.pool.begin().await?;
        let record = self.enrollment(&mut tx, &ticket.enrollment).await?;
        let now = self.now()?;
        let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM worker_bootstrap_challenges WHERE challenge_id=? AND enrollment_id=? AND state='pending')").bind(ticket.id.as_str()).bind(ticket.enrollment.as_str()).fetch_one(&mut *tx).await?;
        if record.lease() != r.lease
            || record.request_id != r.request_id
            || record.evidence != r.evidence
            || record.challenge != ticket.id
            || record.challenge_expires_at != ticket.expires_at
            || now < record.created_at
            || now >= ticket.expires_at
            || !pending
        {
            return Err(WorkerAuthError::Denied);
        }
        let expected = self.mac(
            b"bootstrap-secret",
            &[
                ticket.id.as_str().as_bytes(),
                ticket.enrollment.as_str().as_bytes(),
                ticket
                    .secret
                    .as_ref()
                    .ok_or(WorkerAuthError::Unavailable)?
                    .0
                    .0
                    .as_bytes(),
            ],
        );
        let stored: Vec<u8> = sqlx::query_scalar(
            "SELECT commitment FROM worker_bootstrap_challenges WHERE challenge_id=?",
        )
        .bind(ticket.id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if !equal(&expected, &stored) {
            return Err(WorkerAuthError::Denied);
        }
        self.scope(&mut tx, &record.lease(), now, CredentialScopeUse::Bootstrap)
            .await?;
        tx.commit().await?;
        self.transport.bootstrap(r, ticket)
    }
}
