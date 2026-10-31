//! Independently scoped verifier execution. No public trust-root installation.
//! Production establishment remains unavailable until remote isolation is verified.
use super::*;
use super::{
    store::{decode, same},
    work::*,
};
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FargateProofReference {
    pub launch: Digest,
    pub revision: u64,
    pub commitment: Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierExecution {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fargate: Option<FargateProofReference>,
    pub version: u16,
    pub installation: String,
    pub binding: WorkerBinding,
    pub work: Digest,
    pub artifact: Digest,
    pub resulting_tree: Digest,
    pub policy: Digest,
    pub execution: String,
    pub subject: String,
    pub image: Digest,
    pub public_key: Vec<u8>,
    pub issued_at: i64,
    pub expires_at: i64,
    /// This is a classification, never caller-installed approval.
    pub authority: String,
}
impl VerifierExecution {
    fn trusted(&self) -> Result<(), OrchestrationError> {
        #[cfg(test)]
        if self.authority == "private-offline-process-v1"
            && self.subject.starts_with("private-offline-verifier-")
        {
            return Ok(());
        }
        Err(OrchestrationError::VerificationUnavailable)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticatedReport {
    pub grant_sha256: Digest,
    pub resulting_tree: Digest,
    pub report: VerificationReport,
    pub signature: Vec<u8>,
}
impl std::fmt::Debug for AuthenticatedReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AuthenticatedReport [redacted]")
    }
}
fn signed_bytes(domain: &str, v: &impl Serialize) -> Result<Vec<u8>, OrchestrationError> {
    let bytes = encode(v, 65536)?;
    let mut out = domain.as_bytes().to_vec();
    out.push(0);
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(&bytes);
    Ok(out)
}
impl AuthenticatedReport {
    pub(crate) fn sign(
        g: &VerifierExecution,
        r: VerificationReport,
        key: &Ed25519KeyPair,
    ) -> Result<Self, OrchestrationError> {
        if key.public_key().as_ref() != g.public_key {
            return Err(OrchestrationError::ScopeMismatch);
        }
        let mut e = Self {
            grant_sha256: commitment(g)?,
            resulting_tree: g.resulting_tree.clone(),
            report: r,
            signature: vec![],
        };
        e.signature = key.sign(&e.message()?).as_ref().to_vec();
        Ok(e)
    }
    fn message(&self) -> Result<Vec<u8>, OrchestrationError> {
        signed_bytes(
            "rolvya/independent-coding-report/v1",
            &(&self.grant_sha256, &self.resulting_tree, &self.report),
        )
    }
    fn verify(&self, g: &VerifierExecution) -> Result<(), OrchestrationError> {
        if self.grant_sha256 != commitment(g)?
            || self.resulting_tree != g.resulting_tree
            || self.report.verifier != g.subject
            || self.report.environment != g.image.as_str()
            || self.report.execution != g.execution
            || self.report.binding != g.binding
            || self.report.artifact_sha256 != g.artifact
            || self.report.policy_sha256 != g.policy
            || self.report.started_at < g.issued_at
            || self.report.completed_at >= g.expires_at
        {
            return Err(OrchestrationError::ScopeMismatch);
        }
        UnparsedPublicKey::new(&ED25519, &g.public_key)
            .verify(&self.message()?, &self.signature)
            .map_err(|_| OrchestrationError::InvalidArtifact)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VerifierOperation {
    Claim {},
    Source {
        work: Digest,
        file: u16,
        offset: u32,
    },
    Artifact {
        offset: u32,
    },
    Report {
        envelope: Box<AuthenticatedReport>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierRequest {
    pub version: u16,
    pub execution: String,
    pub request_id: RequestId,
    pub operation: VerifierOperation,
    pub signature: Vec<u8>,
}
impl VerifierRequest {
    pub(crate) fn sign(
        &mut self,
        key: &Ed25519KeyPair,
        channel: &[u8; 32],
    ) -> Result<(), OrchestrationError> {
        self.signature = key.sign(&self.message(channel)?).as_ref().to_vec();
        Ok(())
    }
    fn message(&self, channel: &[u8; 32]) -> Result<Vec<u8>, OrchestrationError> {
        signed_bytes(
            "rolvya/verifier-https-request/v1",
            &(
                self.version,
                &self.execution,
                &self.request_id,
                &self.operation,
                channel,
            ),
        )
    }
    fn verify(&self, g: &VerifierExecution, channel: &[u8; 32]) -> Result<(), OrchestrationError> {
        if self.version != 1
            || self.execution != g.execution
            || self.signature.len() != 64
            || g.public_key.len() != 32
        {
            return Err(OrchestrationError::ScopeMismatch);
        }
        RequestId::new(self.request_id.as_str())?;
        UnparsedPublicKey::new(&ED25519, &g.public_key)
            .verify(&self.message(channel)?, &self.signature)
            .map_err(|_| OrchestrationError::ScopeMismatch)
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VerifierReply {
    Assignment {
        grant: Box<VerifierExecution>,
        work: Box<CodingWork>,
    },
    Source {
        chunk: SourceChunk,
    },
    Artifact {
        offset: u32,
        total: u32,
        bytes: Vec<u8>,
        sha256: Digest,
    },
    Recorded {
        evidence_sha256: Digest,
    },
}
impl SyntheticCoordinator {
    /// No live scheduling, reservation or attestation is fabricated by this API.
    pub async fn prepare_independent_verifier(
        &self,
        _l: &LeaseRef,
    ) -> Result<VerifierExecution, OrchestrationError> {
        Err(OrchestrationError::VerificationUnavailable)
    }
    pub(crate) async fn verifier_grant_in(
        &self,
        c: &mut SqliteConnection,
        id: &str,
    ) -> Result<VerifierExecution, OrchestrationError> {
        if id.is_empty() || id.len() > 120 {
            return Err(OrchestrationError::ScopeMismatch);
        }
        let r=sqlx::query("SELECT project_id,attempt_id,body,seal FROM coding_verifier_executions WHERE execution_id=?").bind(id).fetch_optional(&mut *c).await?.ok_or(OrchestrationError::VerificationUnavailable)?;
        let g: VerifierExecution = decode(&r.get::<Vec<u8>, _>(2), 16384)?;
        self.coding_authority()?
            .verify(b"verifier-grant", &g, &r.get::<Vec<u8>, _>(3))?;
        if g.authority == "private-fargate-verifier-v1" || g.authority == "aws-fargate-verifier-v1"
        {
            crate::controller_keys::worker_auth::aws_fargate::execution::verifier::grant_in(
                c,
                &self.coding_authority()?.key,
                &g,
                g.issued_at,
                true,
            )
            .await
            .map_err(|_| OrchestrationError::VerificationUnavailable)?;
        } else {
            g.trusted()?;
        }
        if g.execution != id
            || g.installation != self.coding_authority()?.installation()
            || g.binding.task.plan != *self.plan_key()
            || g.binding.task.plan.project_id.as_str() != r.get::<String, _>(0)
            || g.binding.attempt_id.as_str() != r.get::<String, _>(1)
        {
            return Err(OrchestrationError::ScopeMismatch);
        }
        g.binding.validate()?;
        Ok(g)
    }
    async fn verifier_current_in(
        &self,
        c: &mut SqliteConnection,
        g: &VerifierExecution,
        policy: &dyn SchedulerPolicy,
        now: i64,
    ) -> Result<(), OrchestrationError> {
        if g.authority == "private-fargate-verifier-v1" || g.authority == "aws-fargate-verifier-v1"
        {
            crate::controller_keys::worker_auth::aws_fargate::execution::verifier::grant_in(
                c,
                &self.coding_authority()?.key,
                g,
                now,
                false,
            )
            .await
            .map_err(|_| OrchestrationError::VerificationUnavailable)?;
        }
        if now < g.issued_at || now >= g.expires_at {
            return Err(OrchestrationError::VerificationUnavailable);
        }
        let observed: Option<i64> = sqlx::query_scalar(
            "SELECT max(recorded_at) FROM coding_verifier_requests WHERE execution_id=?",
        )
        .bind(&g.execution)
        .fetch_one(&mut *c)
        .await?;
        if observed.is_some_and(|t| now < t) {
            return Err(OrchestrationError::VerificationUnavailable);
        }
        let revoked: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM coding_verifier_revocations WHERE execution_id=?)",
        )
        .bind(&g.execution)
        .fetch_one(&mut *c)
        .await?;
        if revoked {
            return Err(OrchestrationError::VerificationUnavailable);
        }
        let l = LeaseRef {
            task: g.binding.task.clone(),
            attempt_id: g.binding.attempt_id.clone(),
            epoch: g.binding.epoch,
        };
        let scope = crate::worker_control::credential_scope(
            c,
            &l,
            policy,
            now,
            crate::worker_control::CredentialScopeUse::Control,
        )
        .await?;
        same(&scope.binding, &g.binding)?;
        // Revocation of the original coding execution invalidates pending authority.
        let disabled:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM worker_enrollments WHERE project_id=? AND attempt_id=? AND state<>'active')").bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).fetch_one(&mut *c).await?;
        if disabled {
            return Err(OrchestrationError::VerificationUnavailable);
        }
        let w = self.coding_work_in(c, &l, now).await?;
        same(&w.identity, &g.work)?;
        let raw = self.coding_raw(c, &l).await?;
        let (r, _, ctx) = self.structural(c, &l, &raw).await?;
        let (_, receipt, _) = crate::worker_control::coding_receipt_in(c, &l).await?;
        if receipt != g.artifact
            || hash(&raw) != g.artifact.as_str()
            || r.resulting_tree_sha256 != g.resulting_tree
            || commitment(&ctx.verification)? != g.policy
            || r.verification_execution != g.execution
        {
            return Err(OrchestrationError::InvalidArtifact);
        }
        Ok(())
    }
    pub(crate) async fn verifier_request_binding_in(
        &self,
        c: &mut SqliteConnection,
        r: &VerifierRequest,
        channel: &[u8; 32],
    ) -> Result<WorkerBinding, OrchestrationError> {
        let grant = self.verifier_grant_in(c, &r.execution).await?;
        r.verify(&grant, channel)?;
        Ok(grant.binding)
    }
    pub(crate) async fn verifier_request_in(
        &self,
        c: &mut SqliteConnection,
        r: &VerifierRequest,
        channel: &[u8; 32],
        policy: &dyn SchedulerPolicy,
        now: i64,
    ) -> Result<VerifierReply, OrchestrationError> {
        let g = self.verifier_grant_in(c, &r.execution).await?;
        r.verify(&g, channel)?;
        self.verifier_current_in(c, &g, policy, now).await?;
        let l = LeaseRef {
            task: g.binding.task.clone(),
            attempt_id: g.binding.attempt_id.clone(),
            epoch: g.binding.epoch,
        };
        let digest = commitment(&(r.version, &r.execution, &r.request_id, &r.operation))?;
        let old=sqlx::query("SELECT digest,response_sha256 FROM coding_verifier_requests WHERE execution_id=? AND request_id=?").bind(&g.execution).bind(r.request_id.as_str()).fetch_optional(&mut *c).await?;
        if let Some(row) = &old {
            same(&row.get::<String, _>(0), &digest.as_str().to_owned())?;
        }
        let reply = match &r.operation {
            VerifierOperation::Claim {} => VerifierReply::Assignment {
                grant: Box::new(g.clone()),
                work: Box::new(self.coding_work_in(c, &l, now).await?),
            },
            VerifierOperation::Source { work, file, offset } => VerifierReply::Source {
                chunk: self
                    .coding_chunk_in(c, &l, work, *file, *offset, now)
                    .await?,
            },
            VerifierOperation::Artifact { offset } => {
                let raw = self.coding_raw(c, &l).await?;
                let o = *offset as usize;
                if !o.is_multiple_of(SOURCE_CHUNK_BYTES) || o >= raw.len() {
                    return Err(OrchestrationError::ScopeMismatch);
                }
                VerifierReply::Artifact {
                    offset: *offset,
                    total: raw.len() as u32,
                    bytes: raw[o..(o + SOURCE_CHUNK_BYTES).min(raw.len())].to_vec(),
                    sha256: g.artifact.clone(),
                }
            }
            VerifierOperation::Report { envelope } => {
                envelope.verify(&g)?;
                let raw = self.coding_raw(c, &l).await?;
                let (result, _, ctx) = self.structural(c, &l, &raw).await?;
                self.report(&result, &raw, &ctx, &envelope.report)?;
                self.chronology(c, &l, &envelope.report).await?;
                if envelope.report.completed_at > now {
                    return Err(OrchestrationError::InvalidArtifact);
                }
                VerifierReply::Recorded {
                    evidence_sha256: commitment(&envelope.report)?,
                }
            }
        };
        let response = commitment(&reply)?;
        encode(&reply, 96 * 1024)?;
        if let Some(row) = old {
            same(&row.get::<String, _>(1), &response.as_str().to_owned())?;
        } else {
            one(
                sqlx::query("INSERT INTO coding_verifier_requests VALUES(?,?,?,?,?)")
                    .bind(&g.execution)
                    .bind(r.request_id.as_str())
                    .bind(digest.as_str())
                    .bind(response.as_str())
                    .bind(now)
                    .execute(&mut *c)
                    .await?
                    .rows_affected(),
            )?;
        }
        if let VerifierOperation::Report { envelope } = &r.operation {
            let bytes = encode(envelope, 49152)?;
            if let Some(old) = sqlx::query_scalar::<_, Vec<u8>>(
                "SELECT envelope FROM coding_verifier_reports WHERE execution_id=?",
            )
            .bind(&g.execution)
            .fetch_optional(&mut *c)
            .await?
            {
                same(&old, &bytes)?;
            } else {
                let raw = self.coding_raw(c, &l).await?;
                self.store_coding_report_in(c, &l, &envelope.report, &raw)
                    .await?;
                let seal = self
                    .coding_authority()?
                    .seal(b"verifier-report", envelope)?;
                one(
                    sqlx::query("INSERT INTO coding_verifier_reports VALUES(?,?,?,?,?,?,?)")
                        .bind(&g.execution)
                        .bind(l.task.plan.project_id.as_str())
                        .bind(l.attempt_id.as_str())
                        .bind(r.request_id.as_str())
                        .bind(commitment(&envelope.report)?.as_str())
                        .bind(bytes)
                        .bind(seal)
                        .execute(&mut *c)
                        .await?
                        .rows_affected(),
                )?;
            }
        }
        Ok(reply)
    }
    pub(super) async fn authenticated_report_in(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
        report: &VerificationReport,
    ) -> Result<(), OrchestrationError> {
        let r=sqlx::query("SELECT execution_id,envelope,seal FROM coding_verifier_reports WHERE project_id=? AND attempt_id=?").bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).fetch_optional(&mut *c).await?;
        if let Some(r) = r {
            let g = self.verifier_grant_in(c, &r.get::<String, _>(0)).await?;
            let e: AuthenticatedReport = decode(&r.get::<Vec<u8>, _>(1), 49152)?;
            self.coding_authority()?
                .verify(b"verifier-report", &e, &r.get::<Vec<u8>, _>(2))?;
            e.verify(&g)?;
            same(&e.report, report)?;
        } else {
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM coding_verifier_executions WHERE project_id=? AND attempt_id=?)").bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).fetch_one(c).await?;
            if exists {
                return Err(OrchestrationError::VerificationUnavailable);
            }
        }
        Ok(())
    }
    pub(super) async fn verifier_acceptance_current_in(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
        policy: &dyn SchedulerPolicy,
        now: i64,
    ) -> Result<(), OrchestrationError> {
        let done: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM task_acceptances WHERE project_id=? AND attempt_id=?)",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_one(&mut *c)
        .await?;
        if done {
            return Ok(());
        }
        let id:Option<String>=sqlx::query_scalar("SELECT execution_id FROM coding_verifier_executions WHERE project_id=? AND attempt_id=?").bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).fetch_optional(&mut *c).await?;
        if let Some(id) = id {
            let g = self.verifier_grant_in(c, &id).await?;
            self.verifier_current_in(c, &g, policy, now).await?;
        }
        Ok(())
    }
    #[cfg(test)]
    pub(crate) async fn fixture_verifier_execution(
        &self,
        l: &LeaseRef,
        subject: String,
        image: Digest,
        public_key: Vec<u8>,
        policy: &impl SchedulerPolicy,
        now: i64,
    ) -> Result<VerifierExecution, OrchestrationError> {
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let w = self.coding_work_in(&mut tx, l, now).await?;
        let raw = self.coding_raw(&mut tx, l).await?;
        let (r, _, ctx) = self.structural(&mut tx, l, &raw).await?;
        let g = VerifierExecution {
            fargate: None,
            version: 1,
            installation: ctx.installation,
            binding: r.binding,
            work: w.identity,
            artifact: Digest::new(hash(&raw))?,
            resulting_tree: r.resulting_tree_sha256,
            policy: commitment(&ctx.verification)?,
            execution: r.verification_execution,
            subject,
            image,
            public_key,
            issued_at: now,
            expires_at: w.expires_at.min(now + 120),
            authority: "private-offline-process-v1".into(),
        };
        g.trusted()?;
        self.verifier_current_in(&mut tx, &g, policy, now).await?;
        let seal = self.coding_authority()?.seal(b"verifier-grant", &g)?;
        one(
            sqlx::query("INSERT INTO coding_verifier_executions VALUES(?,?,?,?,?)")
                .bind(l.task.plan.project_id.as_str())
                .bind(l.attempt_id.as_str())
                .bind(&g.execution)
                .bind(encode(&g, 16384)?)
                .bind(seal)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        tx.commit().await?;
        Ok(g)
    }
}

impl std::fmt::Debug for VerifierReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VerifierReply [bounded content redacted]")
    }
}

impl SyntheticCoordinator {
    // A proposal contains no trusted execution. Only the sealed Fargate delivery
    // path may persist a grant after current provider inspection and possession.
    pub(crate) async fn fargate_verifier_proposal_in(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
        now: i64,
    ) -> Result<(CodingWork, VerifierExecution), OrchestrationError> {
        let w = self.coding_work_in(c, l, now).await?;
        let raw = self.coding_raw(c, l).await?;
        let (r, _, ctx) = self.structural(c, l, &raw).await?;
        let (_, receipt, _) = crate::worker_control::coding_receipt_in(c, l).await?;
        if receipt.as_str() != hash(&raw) {
            return Err(OrchestrationError::InvalidArtifact);
        }
        let g = VerifierExecution {
            fargate: None,
            version: 1,
            installation: ctx.installation,
            binding: r.binding,
            work: w.identity.clone(),
            artifact: receipt,
            resulting_tree: r.resulting_tree_sha256,
            policy: commitment(&ctx.verification)?,
            execution: r.verification_execution,
            subject: "pending-fargate-possession".into(),
            image: commitment(&ctx.verification.commands)?,
            public_key: vec![],
            issued_at: now,
            expires_at: w.expires_at.min(
                now.checked_add(120)
                    .ok_or(OrchestrationError::LimitExceeded)?,
            ),
            authority: "aws-fargate-verifier-v1".into(),
        };
        Ok((w, g))
    }
    pub(crate) async fn install_fargate_verifier_in(
        &self,
        c: &mut SqliteConnection,
        g: &VerifierExecution,
        now: i64,
    ) -> Result<(), OrchestrationError> {
        // The caller is the private AWS path; authenticate its durable authority
        // again rather than trusting a DTO or the call-site alone.
        crate::controller_keys::worker_auth::aws_fargate::execution::verifier::grant_in(
            c,
            &self.coding_authority()?.key,
            g,
            now,
            false,
        )
        .await
        .map_err(|_| OrchestrationError::VerificationUnavailable)?;
        if let Some(row) = sqlx::query(
            "SELECT body,seal FROM coding_verifier_executions WHERE project_id=? AND attempt_id=?",
        )
        .bind(g.binding.task.plan.project_id.as_str())
        .bind(g.binding.attempt_id.as_str())
        .fetch_optional(&mut *c)
        .await?
        {
            let old: VerifierExecution = decode(&row.get::<Vec<u8>, _>(0), 16384)?;
            self.coding_authority()?
                .verify(b"verifier-grant", &old, &row.get::<Vec<u8>, _>(1))?;
            same(&old, g)?;
            return Ok(());
        }
        one(
            sqlx::query("INSERT INTO coding_verifier_executions VALUES(?,?,?,?,?)")
                .bind(g.binding.task.plan.project_id.as_str())
                .bind(g.binding.attempt_id.as_str())
                .bind(&g.execution)
                .bind(encode(g, 16384)?)
                .bind(self.coding_authority()?.seal(b"verifier-grant", g)?)
                .execute(c)
                .await?
                .rows_affected(),
        )
    }
}
