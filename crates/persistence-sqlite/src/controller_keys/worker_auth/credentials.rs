use super::*;
use worker_control::CredentialScopeUse;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialRecord {
    format: u32,
    metadata: WorkerCredentialMetadata,
    security_epoch: i64,
    request: RequestId,
    renewal: bool,
    authority_evidence: EvidenceRef,
}
struct Issuance<'a> {
    context: &'a WorkerRequestContext,
    generation: i64,
    epoch: i64,
    secret: WorkerCredential,
    now: i64,
    evidence: &'a EvidenceRef,
}
pub(super) struct CurrentCredential {
    pub enrollment: EnrollmentRecord,
    pub metadata: WorkerCredentialMetadata,
    pub epoch: i64,
}
impl WorkerSecurity {
    /// Requires a current bearer AND newly independently verified provider/host
    /// identity. Rotation never renews a lease, reservation or provider entitlement.
    pub async fn renew_credential(
        &self,
        credential: &WorkerCredential,
        evidence: &EvidenceRef,
        context: &WorkerRequestContext,
    ) -> Result<WorkerCredentialTicket, WorkerAuthError> {
        no_bearer_reference(
            credential,
            &[
                context.request_id.as_str().as_bytes(),
                context.transport_evidence.as_str().as_bytes(),
                evidence.as_str().as_bytes(),
            ],
        )?;
        let mut read = self.store.pool.begin().await?;
        let t = self.now()?;
        let old = self.current_credential(&mut read, credential, t).await?;
        let r = &old.enrollment;
        if t < old.metadata.issued_at + 60
            || old.metadata.generation >= 64
            || !r.identity.permissions.contains(&WorkerPermission::Renew)
        {
            return Err(WorkerAuthError::Denied);
        }
        self.transport.verify(
            context,
            &r.scope.binding,
            WorkerOperation::RenewCredential,
            t,
        )?;
        if self
            .scope(&mut read, &r.lease(), t, CredentialScopeUse::Renew)
            .await?
            != r.scope
        {
            return Err(WorkerAuthError::Integrity);
        }
        read.commit().await?;
        let check = r.check(
            VerificationStage::Renew,
            &context.request_id,
            evidence,
            Some(&old.metadata),
        );
        let grant = self.identity.verify(&check, t).await?;
        self.grant_current(&check, &grant, self.now()?)?;
        if grant.identity != r.identity {
            return Err(WorkerAuthError::Denied);
        }
        let secret = bearer()?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let now = self.now()?;
        let original = self.current_credential(&mut tx, credential, now).await?;
        if now < t || original.metadata != old.metadata || original.epoch != old.epoch {
            return Err(WorkerAuthError::Denied);
        }
        if self
            .scope(&mut tx, &r.lease(), now, CredentialScopeUse::Renew)
            .await?
            != r.scope
        {
            return Err(WorkerAuthError::Integrity);
        }
        let ticket = self
            .mint(
                &mut tx,
                r,
                Issuance {
                    context,
                    generation: old.metadata.generation + 1,
                    epoch: old.epoch,
                    secret,
                    now,
                    evidence,
                },
            )
            .await?;
        self.event(&mut tx, r, &context.request_id, "renew", now)
            .await?;
        self.checkpoint(CommitCheckpoint::BeforeRenew)?;
        let end = self.now()?;
        if end < now || end >= old.metadata.expires_at {
            return Err(WorkerAuthError::Expired);
        }
        self.grant_current_in(&mut tx, &check, &grant, end).await?;
        self.transport.verify(
            context,
            &r.scope.binding,
            WorkerOperation::RenewCredential,
            end,
        )?;
        let current = self
            .current_credential(&mut tx, &ticket.secret, end)
            .await?;
        if current.metadata != ticket.metadata
            || current.epoch != old.epoch
            || self
                .scope(&mut tx, &r.lease(), end, CredentialScopeUse::Renew)
                .await?
                != r.scope
        {
            return Err(WorkerAuthError::Denied);
        }
        tx.commit().await?;
        self.checkpoint(CommitCheckpoint::AfterRenew)?;
        Ok(ticket)
    }
    pub async fn claim_bootstrap(
        &self,
        claim: BootstrapClaim<'_>,
    ) -> Result<WorkerCredentialTicket, WorkerAuthError> {
        no_bearer_reference(
            &claim.secret.0,
            &[
                claim.context.request_id.as_str().as_bytes(),
                claim.context.transport_evidence.as_str().as_bytes(),
                claim.evidence.as_str().as_bytes(),
            ],
        )?;
        let mut read = self.store.pool.begin().await?;
        let t = self.now()?;
        let record = self.pending_challenge(&mut read, &claim, t).await?;
        self.transport.verify(
            claim.context,
            &record.scope.binding,
            WorkerOperation::Register,
            t,
        )?;
        self.scope(&mut read, &record.lease(), t, CredentialScopeUse::Bootstrap)
            .await?;
        read.commit().await?;
        let check = record.check(
            VerificationStage::Bootstrap,
            &claim.context.request_id,
            claim.evidence,
            None,
        );
        let grant = self.identity.verify(&check, t).await?;
        self.grant_current(&check, &grant, self.now()?)?;
        if grant.identity != record.identity {
            return Err(WorkerAuthError::Denied);
        }
        let secret = bearer()?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let now = self.now()?;
        if now < t || self.pending_challenge(&mut tx, &claim, now).await? != record {
            return Err(WorkerAuthError::Denied);
        }
        self.scope(&mut tx, &record.lease(), now, CredentialScopeUse::Bootstrap)
            .await?;
        let ticket = self
            .mint(
                &mut tx,
                &record,
                Issuance {
                    context: claim.context,
                    generation: 1,
                    epoch: 1,
                    secret,
                    now,
                    evidence: claim.evidence,
                },
            )
            .await?;
        one(sqlx::query("UPDATE worker_bootstrap_challenges SET state='consumed',credential_id=? WHERE challenge_id=? AND enrollment_id=? AND state='pending'")
            .bind(ticket.metadata.id.as_str()).bind(claim.id.as_str()).bind(record.id.as_str()).execute(&mut *tx).await?.rows_affected())?;
        self.event(
            &mut tx,
            &record,
            &claim.context.request_id,
            "bootstrap",
            now,
        )
        .await?;
        self.checkpoint(CommitCheckpoint::BeforeBootstrap)?;
        let end = self.now()?;
        if end < now || end >= record.challenge_expires_at || end >= ticket.metadata.expires_at {
            return Err(WorkerAuthError::Expired);
        }
        self.grant_current_in(&mut tx, &check, &grant, end).await?;
        self.transport.verify(
            claim.context,
            &record.scope.binding,
            WorkerOperation::Register,
            end,
        )?;
        self.scope(&mut tx, &record.lease(), end, CredentialScopeUse::Bootstrap)
            .await?;
        tx.commit().await?;
        self.checkpoint(CommitCheckpoint::AfterBootstrap)?;
        Ok(ticket)
    }
    async fn pending_challenge(
        &self,
        c: &mut SqliteConnection,
        claim: &BootstrapClaim<'_>,
        now: i64,
    ) -> Result<EnrollmentRecord, WorkerAuthError> {
        let row=sqlx::query("SELECT c.*,e.state AS enrollment_state FROM worker_bootstrap_challenges c JOIN worker_enrollments e USING(enrollment_id) WHERE challenge_id=?")
            .bind(claim.id.as_str()).fetch_optional(&mut *c).await?.ok_or(WorkerAuthError::Denied)?;
        let id = WorkerEnrollmentId::parse(row.try_get("enrollment_id")?)?;
        let record = self.enrollment(c, &id).await?;
        let commitment = self.mac(
            b"bootstrap-secret",
            &[
                claim.id.as_str().as_bytes(),
                id.as_str().as_bytes(),
                claim.secret.0.0.as_bytes(),
            ],
        );
        let stored: Vec<u8> = row.try_get("commitment")?;
        if record.challenge != *claim.id
            || row.try_get::<&str, _>("preparation_request")? != record.request_id.as_str()
            || row.try_get::<i64, _>("issued_at")? != record.created_at
            || row.try_get::<i64, _>("expires_at")? != record.challenge_expires_at
            || row.try_get::<&str, _>("state")? != "pending"
            || row.try_get::<&str, _>("enrollment_state")? != "pending"
            || now < record.created_at
            || now >= record.challenge_expires_at
            || now >= record.valid_until
            || !equal(&stored, &commitment)
        {
            return Err(WorkerAuthError::Denied);
        }
        Ok(record)
    }
    async fn mint(
        &self,
        c: &mut SqliteConnection,
        r: &EnrollmentRecord,
        issue: Issuance<'_>,
    ) -> Result<WorkerCredentialTicket, WorkerAuthError> {
        if !(1..=64).contains(&issue.generation) {
            return Err(WorkerAuthError::Denied);
        }
        let expires_at = (issue.now + i64::from(ACCESS_TOKEN_TTL_SECONDS)).min(r.valid_until);
        if expires_at <= issue.now {
            return Err(WorkerAuthError::Expired);
        }
        let metadata = WorkerCredentialMetadata {
            id: WorkerCredentialId::parse(new_id()?)?,
            enrollment: r.id.clone(),
            binding: r.scope.binding.clone(),
            generation: issue.generation,
            issued_at: issue.now,
            expires_at,
        };
        let record = CredentialRecord {
            format: 1,
            metadata: metadata.clone(),
            security_epoch: issue.epoch,
            request: issue.context.request_id.clone(),
            renewal: issue.generation > 1,
            authority_evidence: issue.evidence.clone(),
        };
        let text = encode(&record)?;
        let lookup = self.mac(b"credential-lookup", &[issue.secret.0.as_bytes()]);
        let signature = self.mac(b"credential-record", &[text.as_bytes(), &lookup]);
        one(
            sqlx::query("INSERT INTO worker_credentials VALUES(?,?,?,?,?,?,?,?)")
                .bind(metadata.id.as_str())
                .bind(r.id.as_str())
                .bind(issue.generation)
                .bind(lookup.as_slice())
                .bind(text)
                .bind(signature.as_slice())
                .bind(issue.now)
                .bind(expires_at)
                .execute(&mut *c)
                .await?
                .rows_affected(),
        )?;
        one(sqlx::query("UPDATE worker_enrollments SET state='active',current_generation=?,revision=revision+1 WHERE enrollment_id=? AND state IN ('pending','active') AND current_generation=? AND security_epoch=?")
            .bind(issue.generation).bind(r.id.as_str()).bind(issue.generation-1).bind(issue.epoch).execute(c).await?.rows_affected())?;
        Ok(WorkerCredentialTicket {
            metadata,
            secret: issue.secret,
            response_request: issue.context.request_id.clone(),
            response_operation: if issue.generation > 1 {
                WorkerOperation::RenewCredential
            } else {
                WorkerOperation::Register
            },
        })
    }
    pub(super) async fn current_credential(
        &self,
        c: &mut SqliteConnection,
        secret: &WorkerCredential,
        now: i64,
    ) -> Result<CurrentCredential, WorkerAuthError> {
        let lookup = self.mac(b"credential-lookup", &[secret.0.as_bytes()]);
        let row=sqlx::query("SELECT c.*,e.state,e.security_epoch,e.current_generation FROM worker_credentials c JOIN worker_enrollments e USING(enrollment_id) WHERE c.token_lookup=?")
            .bind(lookup.as_slice()).fetch_optional(&mut *c).await?.ok_or(WorkerAuthError::Denied)?;
        let text: &str = row.try_get("record")?;
        let sig: Vec<u8> = row.try_get("record_mac")?;
        if text.len() > 8192
            || !equal(
                &sig,
                &self.mac(b"credential-record", &[text.as_bytes(), &lookup]),
            )
        {
            return Err(WorkerAuthError::Integrity);
        }
        let record: CredentialRecord =
            serde_json::from_str(text).map_err(|_| WorkerAuthError::Integrity)?;
        let r = self.enrollment(c, &record.metadata.enrollment).await?;
        let m = &record.metadata;
        if record.format != 1
            || m.binding != r.scope.binding
            || row.try_get::<&str, _>("credential_id")? != m.id.as_str()
            || row.try_get::<&str, _>("enrollment_id")? != r.id.as_str()
            || row.try_get::<i64, _>("generation")? != m.generation
            || row.try_get::<i64, _>("issued_at")? != m.issued_at
            || row.try_get::<i64, _>("expires_at")? != m.expires_at
            || m.expires_at > r.valid_until
            || m.expires_at > m.issued_at + 900
            || row.try_get::<i64, _>("security_epoch")? != record.security_epoch
            || row.try_get::<i64, _>("current_generation")? != m.generation
            || row.try_get::<&str, _>("state")? != "active"
        {
            return Err(WorkerAuthError::Denied);
        }
        if now < m.issued_at || now >= m.expires_at {
            return Err(WorkerAuthError::Expired);
        }
        Ok(CurrentCredential {
            enrollment: r,
            metadata: record.metadata,
            epoch: record.security_epoch,
        })
    }
    /// Only the exact response ceremony can deliver this secret through a reviewed
    /// private HTTPS adapter. A dropped response never resurrects an old bearer.
    pub async fn deliver_credential(
        &self,
        ticket: &WorkerCredentialTicket,
        context: &WorkerRequestContext,
    ) -> Result<(), WorkerAuthError> {
        if ticket.response_request != context.request_id {
            return Err(WorkerAuthError::Denied);
        }
        let mut tx = self.store.pool.begin().await?;
        let now = self.now()?;
        let current = self
            .current_credential(&mut tx, &ticket.secret, now)
            .await?;
        if current.metadata != ticket.metadata {
            return Err(WorkerAuthError::Denied);
        }
        self.transport.verify(
            context,
            &ticket.metadata.binding,
            ticket.response_operation,
            now,
        )?;
        self.scope(
            &mut tx,
            &current.enrollment.lease(),
            now,
            CredentialScopeUse::Control,
        )
        .await?;
        tx.commit().await?;
        self.transport.credential(context, ticket)
    }
}
