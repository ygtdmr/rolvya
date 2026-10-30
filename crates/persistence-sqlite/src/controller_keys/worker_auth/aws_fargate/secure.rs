//! Existing S2-04 is the sole credential issuer. This module supplies scoped
//! protected-delivery and possession evidence, never a public verifier override.
use super::{super::*, protected, store::AdministrativeOperation, *};
use crate::{
    admission::{LeaseRef, SchedulerPolicy},
    controller_keys::ControllerBootstrap,
    worker_control::CredentialScopeUse,
};
use rolvya_domain::ids::CapabilityRef;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::{Row, SqliteConnection};
use std::{future::Future, pin::Pin, sync::Arc};
type AwsFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AwsIdentityError>> + Send + 'a>>;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProofOrigin {
    IndependentlyVerifiedAws,
    #[cfg(test)]
    SyntheticAwsProtocolFixture,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeliveryVerification {
    format: u16,
    origin: ProofOrigin,
    revision: EvidenceRef,
    plan_digest: String,
    collected_at: i64,
    expires_at: i64,
}
/// Sealed independent effective-IAM/role-exclusivity gate. A generated policy,
/// Owner consent, STS response or copied ARN cannot construct production success.
trait RestrictedDistribution: Send + Sync {
    fn verify<'a>(
        &'a self,
        plan: &'a ProtectedDelivery,
        now: i64,
    ) -> AwsFuture<'a, DeliveryVerification>;
    fn publish<'a>(
        &'a self,
        plan: &'a ProtectedDelivery,
        material: &'a DeliveredPossession,
    ) -> AwsFuture<'a, ()>;
    fn current(&self, plan: &ProtectedDelivery, now: i64) -> Result<(), AwsIdentityError>;
}
struct UnavailableDistribution {
    identity: Arc<FargateIdentity>,
}
impl RestrictedDistribution for UnavailableDistribution {
    fn current(&self, _: &ProtectedDelivery, _: i64) -> Result<(), AwsIdentityError> {
        Err(AwsIdentityError::SecureDeliveryUnverified)
    }
    fn verify<'a>(
        &'a self,
        _: &'a ProtectedDelivery,
        _: i64,
    ) -> AwsFuture<'a, DeliveryVerification> {
        Box::pin(async { Err(AwsIdentityError::SecureDeliveryUnverified) })
    }
    fn publish<'a>(
        &'a self,
        p: &'a ProtectedDelivery,
        material: &'a DeliveredPossession,
    ) -> AwsFuture<'a, ()> {
        Box::pin(async move {
            self.current(p, self.identity.now()?)?;
            let now = self.identity.now()?;
            let mut tx = self
                .identity
                .guard
                .store
                .pool
                .begin_with("BEGIN IMMEDIATE")
                .await?;
            let request = RequestId::new(format!("secret-{}", material.challenge.id.as_str()))
                .map_err(|_| AwsIdentityError::Integrity)?;
            let scope = credential_use::VerifiedInfrastructureUse::authorize(
                &mut tx,
                &self.identity.guard.key,
                &p.execution.binding.task.plan.project_id,
                &p.execution.account_ref,
                &request,
                credential_use::InfrastructureOperation::ProtectedDelivery,
                now,
            )
            .await?;
            let loaded = crate::controller_keys::secret_broker::aws_consumer::consume(
                &mut tx,
                &self.identity.guard.key,
                &scope,
                now,
            )
            .await?;
            let versions:Vec<String>=sqlx::query_scalar("SELECT secret_version FROM aws_protected_challenges WHERE project_id=? AND attempt_id=? ORDER BY generation LIMIT 64").bind(p.execution.binding.task.plan.project_id.as_str()).bind(p.execution.binding.attempt_id.as_str()).fetch_all(&mut *tx).await?;
            tx.commit().await?;
            self.current(p, self.identity.now()?)?;
            super::secrets_sdk::SecretSdk::new(p, loaded.credentials)?
                .publish(material, &versions)
                .await
        })
    }
}
/// Opaque one-use material produced only by the protected resource consumer.
/// No Clone, Serialize or secret export; Debug is redacted. Not AWS-signed attestation.
pub struct DeliveredPossession {
    challenge: PossessionChallenge,
    secret: BootstrapSecret,
}
impl std::fmt::Debug for DeliveredPossession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeliveredPossession([redacted])")
    }
}
impl DeliveredPossession {
    pub(super) fn payload(&self) -> Result<zeroize::Zeroizing<String>, AwsIdentityError> {
        let mut body = zeroize::Zeroizing::new(String::from("{\"format\":1,\"challenge\":"));
        body.push_str(
            &serde_json::to_string(&self.challenge).map_err(|_| AwsIdentityError::Integrity)?,
        );
        body.push_str(",\"secret\":\"");
        body.push_str(&self.secret.0.0);
        body.push_str("\"}");
        Ok(body)
    }
    #[cfg(test)]
    pub(super) fn parse(
        plan: &ProtectedDelivery,
        bytes: &[u8],
        now: i64,
    ) -> Result<Self, AwsIdentityError> {
        if bytes.len() > 16384 {
            return Err(AwsIdentityError::ResourceLimit);
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Payload<'a> {
            format: u16,
            challenge: PossessionChallenge,
            secret: &'a str,
        }
        let value: Payload<'_> =
            serde_json::from_slice(bytes).map_err(|_| AwsIdentityError::Integrity)?;
        value.challenge.validate(now)?;
        if value.format != 1 || proof_delivery(&value.challenge) != *plan {
            return Err(AwsIdentityError::ScopeMismatch);
        }
        Ok(Self {
            challenge: value.challenge,
            secret: BootstrapSecret::parse(value.secret.to_owned()).map_err(store::auth_error)?,
        })
    }
    pub fn challenge(&self) -> &PossessionChallenge {
        &self.challenge
    }
    pub async fn authenticate(
        self,
        client: &mut https::client::WorkerHttpClient,
        request: RequestId,
    ) -> Result<(), https::client::ClientError> {
        match self.challenge.operation {
            PossessionOperation::Bootstrap => {
                client
                    .possession_bootstrap(request, &self.challenge, &self.secret)
                    .await
            }
            PossessionOperation::Renew => {
                client
                    .possession_renew(request, &self.challenge, &self.secret)
                    .await
            }
        }
    }
}
pub struct SecureFargateBootstrap {
    pub(super) identity: Arc<FargateIdentity>,
    distribution: Arc<dyn RestrictedDistribution>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckedDelivery {
    format: u16,
    plan_digest: String,
    verification: DeliveryVerification,
    inspection_revision: u64,
    observed_at: i64,
    expires_at: i64,
}
struct ProtectedRecord {
    challenge: PossessionChallenge,
    enrollment: WorkerEnrollmentId,
    state: String,
    revision: i64,
    secret: BootstrapSecret,
}
fn plan_digest(p: &ProtectedDelivery) -> Result<String, AwsIdentityError> {
    Ok(hex(&Sha256::digest(
        serde_json::to_vec(p).map_err(|_| AwsIdentityError::Integrity)?,
    )))
}
fn proof_delivery(c: &PossessionChallenge) -> ProtectedDelivery {
    ProtectedDelivery {
        format: 1,
        execution: c.execution.clone(),
        secret: c.secret.clone(),
        version_id: c.secret_version.clone(),
        expires_at: c.expires_at,
    }
}
impl SecureFargateBootstrap {
    pub async fn open(
        b: &ControllerBootstrap,
        p: Arc<dyn SchedulerPolicy>,
    ) -> Result<Arc<Self>, AwsIdentityError> {
        let identity = FargateIdentity::open(b, p).await?;
        Ok(Arc::new(Self {
            distribution: Arc::new(UnavailableDistribution {
                identity: identity.clone(),
            }),
            identity,
        }))
    }
    fn now(&self) -> Result<i64, AwsIdentityError> {
        self.identity.now()
    }
    async fn authority_in(
        &self,
        c: &mut SqliteConnection,
        p: &ProtectedDelivery,
        request: &RequestId,
        now: i64,
        usage: CredentialScopeUse,
    ) -> Result<worker_control::CredentialScope, AwsIdentityError> {
        self.distribution.current(p, now)?;
        let e = &p.execution;
        let account = self
            .identity
            .account(c, &e.binding.task.plan.project_id, &e.account_ref)
            .await?;
        self.identity.account_current(&account, e, now)?;
        let stored = self
            .identity
            .execution_optional(c, &e.lease())
            .await?
            .ok_or(AwsIdentityError::UnknownExecution)?;
        if stored.expected != *e {
            return Err(AwsIdentityError::ScopeMismatch);
        }
        self.identity
            .authority
            .authorize_in(
                c,
                AdministrativeOperation::Inspect {
                    account: &account,
                    execution: e,
                    request,
                },
                now,
            )
            .await?;
        self.identity
            .credential_reference(c, &account.project, &account.workload, &account.credential)
            .await?;
        let scope = self
            .identity
            .guard
            .scope(c, &e.lease(), now, usage)
            .await
            .map_err(store::auth_error)?;
        if scope.binding != e.binding
            || scope.provider.as_str() != AWS_PROVIDER
            || scope.account != e.account_ref
            || scope.command_digest != e.command_digest.as_str()
        {
            return Err(AwsIdentityError::ScopeMismatch);
        }
        Ok(scope)
    }
    pub(in crate::controller_keys::worker_auth) async fn register_plan(
        &self,
        r: &PrepareBootstrap,
        p: &ProtectedDelivery,
    ) -> Result<(), AwsIdentityError> {
        p.validate()?;
        if r.lease != p.execution.lease() {
            return Err(AwsIdentityError::ScopeMismatch);
        }
        let now = self.now()?;
        if now >= p.expires_at {
            return Err(AwsIdentityError::ExpiredEvidence);
        }
        // Effective role exclusivity is checked before any secret can be generated.
        let verified = self.distribution.verify(p, now).await?;
        self.check_fact(p, &verified, now)?;
        let mut tx = self
            .identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let end = self.now()?;
        if end < now || end >= p.expires_at {
            return Err(AwsIdentityError::ExpiredEvidence);
        }
        self.check_fact(p, &verified, end)?;
        self.authority_in(
            &mut tx,
            p,
            &r.request_id,
            end,
            CredentialScopeUse::Bootstrap,
        )
        .await?;
        let e = &p.execution;
        let existing = self.load_plan_optional(&mut tx, &e.lease()).await?;
        if let Some(old) = existing {
            if old != *p {
                return Err(AwsIdentityError::Conflict);
            }
            tx.commit().await?;
            return Ok(());
        }
        let text = serde_json::to_string(p).map_err(|_| AwsIdentityError::Integrity)?;
        if text.len() > 32768 {
            return Err(AwsIdentityError::ResourceLimit);
        }
        let tag = self
            .identity
            .guard
            .mac(b"aws-attempt-role", &[text.as_bytes()]);
        if sqlx::query("INSERT INTO aws_attempt_roles VALUES(?,?,?,?,?,?,?,?)")
            .bind(e.binding.task.plan.project_id.as_str())
            .bind(e.binding.attempt_id.as_str())
            .bind(e.account_ref.as_str())
            .bind(e.account_version as i64)
            .bind(p.task_role().as_str())
            .bind(p.secret.as_str())
            .bind(text)
            .bind(tag.as_slice())
            .execute(&mut *tx)
            .await?
            .rows_affected()
            != 1
        {
            return Err(AwsIdentityError::Storage);
        }
        tx.commit().await?;
        Ok(())
    }
    async fn load_plan_optional(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
    ) -> Result<Option<ProtectedDelivery>, AwsIdentityError> {
        let row = sqlx::query(
            "SELECT record,record_mac FROM aws_attempt_roles WHERE project_id=? AND attempt_id=?",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_optional(c)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let text: &str = row.try_get("record")?;
        if text.len() > 32768
            || !equal(
                row.try_get::<&[u8], _>("record_mac")?,
                &self
                    .identity
                    .guard
                    .mac(b"aws-attempt-role", &[text.as_bytes()]),
            )
        {
            return Err(AwsIdentityError::Integrity);
        }
        let plan: ProtectedDelivery =
            serde_json::from_str(text).map_err(|_| AwsIdentityError::Integrity)?;
        plan.validate()?;
        if plan.execution.lease() != *l {
            return Err(AwsIdentityError::ScopeMismatch);
        }
        Ok(Some(plan))
    }
    fn check_fact(
        &self,
        p: &ProtectedDelivery,
        v: &DeliveryVerification,
        now: i64,
    ) -> Result<(), AwsIdentityError> {
        if v.format != 1
            || v.plan_digest != plan_digest(p)?
            || now < v.collected_at
            || now >= v.expires_at
            || v.expires_at - v.collected_at > 15
        {
            return Err(AwsIdentityError::SecureDeliveryUnverified);
        }
        Ok(())
    }
    async fn observe(
        &self,
        p: &ProtectedDelivery,
        request: &RequestId,
    ) -> Result<(), AwsIdentityError> {
        let lease = p.execution.lease();
        let observed = self.identity.inspect(&lease, request).await?;
        let a = observed.assessment.as_ref().map_err(|e| *e)?;
        if a.lifecycle != EcsLifecycle::Running
            || !a.configuration_correlated
            || observed.control_denial.is_some()
        {
            return Err(AwsIdentityError::TaskNotRunning);
        }
        let now = self.now()?;
        let verification = self.distribution.verify(p, now).await?;
        self.check_fact(p, &verification, now)?;
        let mut tx = self
            .identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let end = self.now()?;
        self.authority_in(&mut tx, p, request, end, CredentialScopeUse::Control)
            .await?;
        if self.identity.latest(&mut tx, &lease).await?.as_ref() != Some(&observed) {
            return Err(AwsIdentityError::Conflict);
        }
        self.check_fact(p, &verification, end)?;
        let r = CheckedDelivery {
            format: 1,
            plan_digest: plan_digest(p)?,
            verification,
            inspection_revision: observed.revision,
            observed_at: end,
            expires_at: end.saturating_add(15).min(p.expires_at),
        };
        let text = serde_json::to_string(&r).map_err(|_| AwsIdentityError::Integrity)?;
        let tag = self.identity.guard.mac(
            b"aws-delivery-check",
            &[
                lease.task.plan.project_id.as_str().as_bytes(),
                lease.attempt_id.as_str().as_bytes(),
                request.as_str().as_bytes(),
                text.as_bytes(),
            ],
        );
        let old = self.delivery_check(&mut tx, &lease, request).await?;
        if let Some(old) = old {
            if old.plan_digest != r.plan_digest
                || old.inspection_revision != r.inspection_revision
                || old.verification.revision != r.verification.revision
                || end < old.observed_at
                || end >= old.expires_at
            {
                return Err(AwsIdentityError::Conflict);
            }
            tx.commit().await?;
            return Ok(());
        }
        if sqlx::query("INSERT INTO aws_delivery_checks VALUES(?,?,?,?,?,?,?,?)")
            .bind(lease.task.plan.project_id.as_str())
            .bind(lease.attempt_id.as_str())
            .bind(request.as_str())
            .bind(r.inspection_revision as i64)
            .bind(text)
            .bind(tag.as_slice())
            .bind(end)
            .bind(r.expires_at)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            != 1
        {
            return Err(AwsIdentityError::Storage);
        }
        tx.commit().await?;
        Ok(())
    }
    async fn delivery_check(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
        request: &RequestId,
    ) -> Result<Option<CheckedDelivery>, AwsIdentityError> {
        let row=sqlx::query("SELECT record,record_mac FROM aws_delivery_checks WHERE project_id=? AND attempt_id=? AND request_id=?").bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).bind(request.as_str()).fetch_optional(c).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let text: &str = row.try_get("record")?;
        if text.len() > 8192
            || !equal(
                row.try_get::<&[u8], _>("record_mac")?,
                &self.identity.guard.mac(
                    b"aws-delivery-check",
                    &[
                        l.task.plan.project_id.as_str().as_bytes(),
                        l.attempt_id.as_str().as_bytes(),
                        request.as_str().as_bytes(),
                        text.as_bytes(),
                    ],
                ),
            )
        {
            return Err(AwsIdentityError::Integrity);
        }
        Ok(Some(
            serde_json::from_str(text).map_err(|_| AwsIdentityError::Integrity)?,
        ))
    }
    async fn checked_in(
        &self,
        c: &mut SqliteConnection,
        p: &ProtectedDelivery,
        request: &RequestId,
        now: i64,
        usage: CredentialScopeUse,
    ) -> Result<worker_control::CredentialScope, AwsIdentityError> {
        let scope = self.authority_in(c, p, request, now, usage).await?;
        let row = self
            .delivery_check(c, &p.execution.lease(), request)
            .await?
            .ok_or(AwsIdentityError::SecureDeliveryUnverified)?;
        if row.format != 1
            || row.plan_digest != plan_digest(p)?
            || now < row.observed_at
            || now >= row.expires_at
        {
            return Err(AwsIdentityError::ExpiredEvidence);
        }
        self.check_fact(p, &row.verification, now)?;
        let latest = self
            .identity
            .latest(c, &p.execution.lease())
            .await?
            .ok_or(AwsIdentityError::UnknownExecution)?;
        if latest.revision != row.inspection_revision
            || latest.control_denial.is_some()
            || !latest
                .assessment
                .as_ref()
                .is_ok_and(|a| a.configuration_correlated && a.lifecycle == EcsLifecycle::Running)
        {
            return Err(AwsIdentityError::TaskNotRunning);
        }
        Ok(scope)
    }
    async fn load_protected(
        &self,
        c: &mut SqliteConnection,
        id: &BootstrapChallengeId,
    ) -> Result<ProtectedRecord, AwsIdentityError> {
        let lengths=sqlx::query("SELECT length(record),length(record_mac),length(key_id),length(data_nonce),length(wrap_nonce),length(wrapped_dek),length(ciphertext) FROM aws_protected_challenges WHERE challenge_id=?").bind(id.as_str()).fetch_optional(&mut *c).await?.ok_or(AwsIdentityError::UnknownExecution)?;
        for (i, n) in [(1, 32i64), (2, 16), (3, 24), (4, 24), (5, 48), (6, 80)] {
            if lengths.try_get::<i64, _>(i)? != n {
                return Err(AwsIdentityError::Integrity);
            }
        }
        if !(1..=32768).contains(&lengths.try_get::<i64, _>(0)?) {
            return Err(AwsIdentityError::Integrity);
        }
        let row = sqlx::query("SELECT * FROM aws_protected_challenges WHERE challenge_id=?")
            .bind(id.as_str())
            .fetch_one(c)
            .await?;
        let text: &str = row.try_get("record")?;
        if !equal(
            row.try_get::<&[u8], _>("record_mac")?,
            &self
                .identity
                .guard
                .mac(b"aws-protected-challenge", &[text.as_bytes()]),
        ) || row.try_get::<&[u8], _>("key_id")? != self.identity.guard.key.metadata.key.0
            || row.try_get::<i64, _>("key_version")?
                != self.identity.guard.key.metadata.key_version as i64
        {
            return Err(AwsIdentityError::Integrity);
        }
        let challenge: PossessionChallenge =
            serde_json::from_str(text).map_err(|_| AwsIdentityError::Integrity)?;
        let b = &challenge.execution.binding;
        if challenge.id != *id
            || row.try_get::<&str, _>("project_id")? != b.task.plan.project_id.as_str()
            || row.try_get::<&str, _>("attempt_id")? != b.attempt_id.as_str()
            || row.try_get::<&str, _>("secret_version")? != challenge.secret_version
            || row.try_get::<i64, _>("generation")? != challenge.generation
            || row.try_get::<&str, _>("operation")?
                != if challenge.operation == PossessionOperation::Bootstrap {
                    "bootstrap"
                } else {
                    "renew"
                }
            || row.try_get::<Option<&str>, _>("previous_credential")?
                != challenge.credential.as_ref().map(|id| id.as_str())
            || row.try_get::<i64, _>("created_at")? != challenge.issued_at
            || row.try_get::<i64, _>("expires_at")? != challenge.expires_at
        {
            return Err(AwsIdentityError::Integrity);
        }
        let envelope = protected::Envelope {
            dn: row
                .try_get::<Vec<u8>, _>("data_nonce")?
                .try_into()
                .map_err(|_| AwsIdentityError::Integrity)?,
            wn: row
                .try_get::<Vec<u8>, _>("wrap_nonce")?
                .try_into()
                .map_err(|_| AwsIdentityError::Integrity)?,
            wrapped: row.try_get("wrapped_dek")?,
            ciphertext: row.try_get("ciphertext")?,
        };
        let secret = protected::open(&self.identity.guard.key, &challenge, &envelope)?;
        Ok(ProtectedRecord {
            challenge,
            enrollment: WorkerEnrollmentId::parse(row.try_get("enrollment_id")?)
                .map_err(store::auth_error)?,
            state: row.try_get("state")?,
            revision: row.try_get("revision")?,
            secret,
        })
    }
}
fn denied(error: AwsIdentityError) -> WorkerAuthError {
    let _ = error;
    WorkerAuthError::Denied
}
fn lease_for(c: &IdentityCheck<'_>) -> LeaseRef {
    LeaseRef {
        task: c.binding.task.clone(),
        attempt_id: c.binding.attempt_id.clone(),
        epoch: c.binding.epoch,
    }
}
fn signature(s: &str) -> Result<[u8; 32], AwsIdentityError> {
    if s.len() != 64
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(AwsIdentityError::TaskPossessionUnverified);
    }
    let mut out = [0u8; 32];
    for (i, b) in s.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let digit = |v: u8| if v <= b'9' { v - b'0' } else { v - b'a' + 10 };
        out[i] = digit(b[0]) * 16 + digit(b[1]);
    }
    Ok(out)
}
impl SecureFargateBootstrap {
    pub(in crate::controller_keys::worker_auth) async fn prepare_renewal(
        &self,
        security: &WorkerSecurity,
        bearer: &WorkerCredential,
        context: &WorkerRequestContext,
    ) -> Result<PossessionChallenge, WorkerAuthError> {
        let now = self.now().map_err(denied)?;
        let mut tx = security.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let current = security.current_credential(&mut tx, bearer, now).await?;
        let r = &current.enrollment;
        security.transport.verify(
            context,
            &r.scope.binding,
            WorkerOperation::RenewCredential,
            now,
        )?;
        if now < current.metadata.issued_at + 60
            || current.metadata.generation >= 64
            || !r.identity.permissions.contains(&WorkerPermission::Renew)
        {
            return Err(WorkerAuthError::Denied);
        }
        let plan = self
            .load_plan_optional(&mut tx, &r.lease())
            .await
            .map_err(denied)?
            .ok_or(WorkerAuthError::Denied)?;
        let scope = self
            .authority_in(
                &mut tx,
                &plan,
                &context.request_id,
                now,
                CredentialScopeUse::Renew,
            )
            .await
            .map_err(denied)?;
        if scope != r.scope {
            return Err(WorkerAuthError::Denied);
        }
        let previous:Option<String>=sqlx::query_scalar("SELECT challenge_id FROM aws_protected_challenges WHERE enrollment_id=? AND operation='renew' AND generation=?").bind(r.id.as_str()).bind(current.metadata.generation+1).fetch_optional(&mut *tx).await?;
        let challenge = if let Some(id) = previous {
            let old = self
                .load_protected(&mut tx, &BootstrapChallengeId::parse(id)?)
                .await
                .map_err(denied)?;
            if old.challenge.credential.as_ref() != Some(&current.metadata.id)
                || !matches!(old.state.as_str(), "pending" | "uncertain" | "delivered")
            {
                return Err(WorkerAuthError::Denied);
            }
            old.challenge.validate(now).map_err(denied)?;
            old.challenge
        } else {
            let secret = BootstrapSecret(super::super::bearer()?);
            let challenge = PossessionChallenge {
                format: 1,
                id: BootstrapChallengeId::parse(new_id()?)?,
                operation: PossessionOperation::Renew,
                execution: plan.execution,
                credential: Some(current.metadata.id.clone()),
                generation: current.metadata.generation + 1,
                issued_at: now,
                expires_at: now
                    .saturating_add(120)
                    .min(current.metadata.expires_at)
                    .min(r.valid_until),
                secret: plan.secret,
                secret_version: new_id()?,
            };
            self.insert_proof(&mut tx, r, &challenge, &context.request_id, &secret)
                .await
                .map_err(denied)?;
            challenge
        };
        // No external publication under the writer lock. Replays reuse exactly
        // the committed encrypted material/version, never a second secret.
        tx.commit().await?;
        self.publish(&challenge.id, &context.request_id)
            .await
            .map_err(denied)
    }
    async fn audit(
        &self,
        c: &mut SqliteConnection,
        id: &BootstrapChallengeId,
        request: &RequestId,
        kind: &str,
        now: i64,
    ) -> Result<(), AwsIdentityError> {
        let revision: i64 = sqlx::query_scalar(
            "SELECT revision FROM aws_protected_challenges WHERE challenge_id=?",
        )
        .bind(id.as_str())
        .fetch_one(&mut *c)
        .await?;
        if sqlx::query("INSERT INTO aws_delivery_events(challenge_id,request_id,kind,recorded_at,reference) VALUES(?,?,?,?,?)").bind(id.as_str()).bind(request.as_str()).bind(kind).bind(now).bind(format!("protected-proof-revision-{revision}")).execute(c).await?.rows_affected()!=1{return Err(AwsIdentityError::Storage);}
        Ok(())
    }
    async fn insert_proof(
        &self,
        c: &mut SqliteConnection,
        record: &EnrollmentRecord,
        challenge: &PossessionChallenge,
        request: &RequestId,
        secret: &BootstrapSecret,
    ) -> Result<(), AwsIdentityError> {
        challenge.validate(challenge.issued_at)?;
        let envelope = protected::seal(&self.identity.guard.key, challenge, secret)?;
        let text = serde_json::to_string(challenge).map_err(|_| AwsIdentityError::Integrity)?;
        if text.len() > 32768 {
            return Err(AwsIdentityError::ResourceLimit);
        }
        let tag = self
            .identity
            .guard
            .mac(b"aws-protected-challenge", &[text.as_bytes()]);
        let b = &challenge.execution.binding;
        if sqlx::query("INSERT INTO aws_protected_challenges(challenge_id,project_id,attempt_id,enrollment_id,operation,previous_credential,generation,request_id,secret_version,record,record_mac,key_id,key_version,data_nonce,wrap_nonce,wrapped_dek,ciphertext,created_at,expires_at,state,revision,consumed_credential) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'pending',1,NULL)")
            .bind(challenge.id.as_str()).bind(b.task.plan.project_id.as_str()).bind(b.attempt_id.as_str()).bind(record.id.as_str()).bind(if challenge.operation==PossessionOperation::Bootstrap{"bootstrap"}else{"renew"}).bind(challenge.credential.as_ref().map(|v|v.as_str())).bind(challenge.generation).bind(request.as_str()).bind(&challenge.secret_version).bind(text).bind(tag.as_slice()).bind(self.identity.guard.key.metadata.key.0.as_slice()).bind(self.identity.guard.key.metadata.key_version as i64).bind(envelope.dn.as_slice()).bind(envelope.wn.as_slice()).bind(envelope.wrapped).bind(envelope.ciphertext).bind(challenge.issued_at).bind(challenge.expires_at).execute(&mut *c).await?.rows_affected()!=1{return Err(AwsIdentityError::Storage);}
        self.audit(c, &challenge.id, request, "prepared", challenge.issued_at)
            .await
    }
    async fn prepare_in(
        &self,
        c: &mut SqliteConnection,
        r: &EnrollmentRecord,
        secret: &BootstrapSecret,
        now: i64,
    ) -> Result<(), AwsIdentityError> {
        let plan = self
            .load_plan_optional(c, &r.lease())
            .await?
            .ok_or(AwsIdentityError::SecureDeliveryUnverified)?;
        self.checked_in(c, &plan, &r.request_id, now, CredentialScopeUse::Bootstrap)
            .await?;
        let proof = PossessionChallenge {
            format: 1,
            id: r.challenge.clone(),
            operation: PossessionOperation::Bootstrap,
            execution: plan.execution.clone(),
            credential: None,
            generation: 1,
            issued_at: r.created_at,
            expires_at: r.challenge_expires_at.min(plan.expires_at),
            secret: plan.secret,
            secret_version: plan.version_id,
        };
        self.insert_proof(c, r, &proof, &r.request_id, secret).await
    }
    fn identity(
        &self,
        p: &ProtectedDelivery,
        scope: &worker_control::CredentialScope,
    ) -> Result<VerifiedWorkerIdentity, AwsIdentityError> {
        let task = &p.execution.task;
        let hash = hex(&Sha256::digest(task.as_str().as_bytes()));
        Ok(VerifiedWorkerIdentity {
            subject: EvidenceRef::new(format!("aws-task-{}", &hash[..40]))
                .map_err(|_| AwsIdentityError::Integrity)?,
            execution: EvidenceRef::new(format!("aws-run-{}", task.task_id()))
                .map_err(|_| AwsIdentityError::Integrity)?,
            capabilities: DeclaredCapabilities {
                models: vec![scope.profile.model.clone()],
                efforts: vec![scope.profile.effort],
                access_modes: vec![scope.profile.access_mode],
                cpu_millis: p.execution.workload.cpu_units * 1000 / 1024,
                memory_mib: p.execution.workload.memory_mib,
                evidence: Some(
                    CapabilityRef::new(format!(
                        "aws-approved-{}",
                        &p.execution.operation_token[..32]
                    ))
                    .map_err(|_| AwsIdentityError::Integrity)?,
                ),
            },
            permissions: vec![
                WorkerPermission::Register,
                WorkerPermission::Claim,
                WorkerPermission::AcceptLease,
                WorkerPermission::Heartbeat,
                WorkerPermission::Progress,
                WorkerPermission::SubmitResult,
                WorkerPermission::StopQuery,
                WorkerPermission::StopAck,
                WorkerPermission::Renew,
            ],
            valid_until: p.execution.authorization_until,
        })
    }
    fn scope_matches(
        &self,
        c: &IdentityCheck<'_>,
        p: &ProtectedDelivery,
    ) -> Result<(), AwsIdentityError> {
        if c.controller != &self.identity.guard.controller()
            || c.binding != &p.execution.binding
            || c.account != &p.execution.account_ref
            || c.provider.as_str() != AWS_PROVIDER
            || c.command_digest != p.execution.command_digest.as_str()
        {
            return Err(AwsIdentityError::ScopeMismatch);
        }
        Ok(())
    }
    fn check_claim(
        &self,
        c: &IdentityCheck<'_>,
        r: &ProtectedRecord,
        attestation: &https::wire::Attestation,
        channel: &[u8; 32],
        now: i64,
    ) -> Result<(), AwsIdentityError> {
        let proof = &r.challenge;
        self.scope_matches(c, &proof_delivery(proof))?;
        proof.validate(now)?;
        if r.state != "delivered"
            || attestation.reference.as_str() != proof.id.as_str()
            || c.evidence != &attestation.reference
            || serde_json::from_str::<PossessionChallenge>(&attestation.statement)
                .map_err(|_| AwsIdentityError::Integrity)?
                != *proof
        {
            return Err(AwsIdentityError::TaskPossessionUnverified);
        }
        match c.stage {
            VerificationStage::Bootstrap => {
                if proof.operation != PossessionOperation::Bootstrap
                    || c.challenge != Some(&proof.id)
                    || c.credential.is_some()
                {
                    return Err(AwsIdentityError::ScopeMismatch);
                }
            }
            VerificationStage::Renew => {
                let old = c.credential.ok_or(AwsIdentityError::ScopeMismatch)?;
                if proof.operation != PossessionOperation::Renew
                    || proof.credential.as_ref() != Some(&old.id)
                    || old.generation.checked_add(1) != Some(proof.generation)
                    || old.enrollment != r.enrollment
                {
                    return Err(AwsIdentityError::ScopeMismatch);
                }
            }
            _ => return Err(AwsIdentityError::ScopeMismatch),
        }
        proof.verify(
            &r.secret,
            c.request,
            channel,
            &signature(&attestation.signature)?,
            now,
        )
    }
    async fn verify_request(
        &self,
        c: &IdentityCheck<'_>,
        transport: &https::context::HttpTransport,
        now: i64,
    ) -> Result<IdentityGrant, AwsIdentityError> {
        let lease = lease_for(c);
        let mut read = self.identity.guard.store.pool.begin().await?;
        let plan = if c.stage == VerificationStage::Prepare {
            let p = self
                .load_plan_optional(&mut read, &lease)
                .await?
                .ok_or(AwsIdentityError::SecureDeliveryUnverified)?;
            self.scope_matches(c, &p)?;
            p
        } else {
            let (channel, attestation) = transport
                .accepted_proof(c, now)
                .map_err(store::auth_error)?;
            let id = BootstrapChallengeId::parse(c.evidence.as_str().to_owned())
                .map_err(store::auth_error)?;
            let row = self.load_protected(&mut read, &id).await?;
            self.check_claim(c, &row, &attestation, &channel, now)?;
            proof_delivery(&row.challenge)
        };
        read.commit().await?;
        self.observe(&plan, c.request).await?;
        let mut read = self.identity.guard.store.pool.begin().await?;
        let end = self.now()?;
        let scope = self
            .checked_in(
                &mut read,
                &plan,
                c.request,
                end,
                if c.stage == VerificationStage::Prepare {
                    CredentialScopeUse::Bootstrap
                } else if c.stage == VerificationStage::Renew {
                    CredentialScopeUse::Renew
                } else {
                    CredentialScopeUse::Bootstrap
                },
            )
            .await?;
        let checked = self
            .delivery_check(&mut read, &lease, c.request)
            .await?
            .ok_or(AwsIdentityError::SecureDeliveryUnverified)?;
        read.commit().await?;
        Ok(IdentityGrant {
            identity: self.identity(&plan, &scope)?,
            scope: c.scope(),
            not_before: checked.observed_at,
            expires_at: checked.expires_at.min(checked.verification.expires_at),
        })
    }
    async fn current_in(
        &self,
        conn: &mut SqliteConnection,
        c: &IdentityCheck<'_>,
        grant: &IdentityGrant,
        transport: &https::context::HttpTransport,
        now: i64,
    ) -> Result<(), AwsIdentityError> {
        let lease = lease_for(c);
        if c.stage == VerificationStage::Prepare {
            let plan = self
                .load_plan_optional(conn, &lease)
                .await?
                .ok_or(AwsIdentityError::SecureDeliveryUnverified)?;
            let scope = self
                .checked_in(conn, &plan, c.request, now, CredentialScopeUse::Bootstrap)
                .await?;
            if grant.identity != self.identity(&plan, &scope)? {
                return Err(AwsIdentityError::ScopeMismatch);
            }
            return Ok(());
        }
        let (channel, attestation) = transport
            .accepted_proof(c, now)
            .map_err(store::auth_error)?;
        let id = BootstrapChallengeId::parse(c.evidence.as_str().to_owned())
            .map_err(store::auth_error)?;
        let row = self.load_protected(conn, &id).await?;
        self.check_claim(c, &row, &attestation, &channel, now)?;
        let plan = proof_delivery(&row.challenge);
        let scope = self
            .checked_in(
                conn,
                &plan,
                c.request,
                now,
                if c.stage == VerificationStage::Renew {
                    CredentialScopeUse::Renew
                } else {
                    CredentialScopeUse::Bootstrap
                },
            )
            .await?;
        if grant.identity != self.identity(&plan, &scope)? {
            return Err(AwsIdentityError::ScopeMismatch);
        }
        // The issuer has staged its own credential and one-use transition. The
        // native proof must be consumed in this very same transaction.
        let token=sqlx::query("SELECT c.credential_id FROM worker_credentials c JOIN worker_enrollments e USING(enrollment_id) WHERE c.enrollment_id=? AND c.generation=? AND e.state='active' AND e.current_generation=c.generation").bind(row.enrollment.as_str()).bind(row.challenge.generation).fetch_optional(&mut *conn).await?.ok_or(AwsIdentityError::Conflict)?;
        let token: &str = token.try_get("credential_id")?;
        if row.challenge.operation == PossessionOperation::Bootstrap {
            let used:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM worker_bootstrap_challenges WHERE challenge_id=? AND enrollment_id=? AND state='consumed' AND credential_id=?)").bind(id.as_str()).bind(row.enrollment.as_str()).bind(token).fetch_one(&mut *conn).await?;
            if !used {
                return Err(AwsIdentityError::Conflict);
            }
        }
        if sqlx::query("UPDATE aws_protected_challenges SET state='consumed',revision=revision+1,consumed_credential=? WHERE challenge_id=? AND state='delivered' AND revision=?").bind(token).bind(id.as_str()).bind(row.revision).execute(&mut *conn).await?.rows_affected()!=1{return Err(AwsIdentityError::Conflict);}
        self.audit(conn, &id, c.request, "consumed", now).await?;
        self.audit(conn, &id, c.request, "cleanup_requested", now)
            .await
    }
    pub(in crate::controller_keys::worker_auth) async fn publish(
        &self,
        id: &BootstrapChallengeId,
        request: &RequestId,
    ) -> Result<PossessionChallenge, AwsIdentityError> {
        let mut read = self.identity.guard.store.pool.begin().await?;
        let row = self.load_protected(&mut read, id).await?;
        let p = proof_delivery(&row.challenge);
        let now = self.now()?;
        row.challenge.validate(now)?;
        self.authority_in(
            &mut read,
            &p,
            request,
            now,
            if row.challenge.operation == PossessionOperation::Renew {
                CredentialScopeUse::Renew
            } else {
                CredentialScopeUse::Bootstrap
            },
        )
        .await?;
        if row.state == "delivered" {
            read.commit().await?;
            return Ok(row.challenge);
        }
        if !matches!(row.state.as_str(), "pending" | "uncertain") || row.revision >= 16 {
            return Err(AwsIdentityError::Revoked);
        }
        read.commit().await?;
        let verified = self.distribution.verify(&p, self.now()?).await?;
        self.check_fact(&p, &verified, self.now()?)?;
        // Persist each bounded attempt BEFORE the external effect. A post-effect
        // SQLite failure cannot erase this count or justify a fresh secret.
        let mut intent = self
            .identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let before = self.now()?;
        let current = self.load_protected(&mut intent, id).await?;
        if current.revision != row.revision
            || current.state != row.state
            || current.challenge != row.challenge
        {
            return Err(AwsIdentityError::Conflict);
        }
        row.challenge.validate(before)?;
        self.check_fact(&p, &verified, before)?;
        self.authority_in(
            &mut intent,
            &p,
            request,
            before,
            if row.challenge.operation == PossessionOperation::Renew {
                CredentialScopeUse::Renew
            } else {
                CredentialScopeUse::Bootstrap
            },
        )
        .await?;
        if sqlx::query("UPDATE aws_protected_challenges SET state='uncertain',revision=revision+1 WHERE challenge_id=? AND revision=? AND state IN ('pending','uncertain')").bind(id.as_str()).bind(row.revision).execute(&mut *intent).await?.rows_affected()!=1{return Err(AwsIdentityError::Conflict);}
        self.audit(&mut intent, id, request, "publish_requested", before)
            .await?;
        intent.commit().await?;
        let reserved_revision = row.revision + 1;
        let material = DeliveredPossession {
            challenge: row.challenge.clone(),
            secret: row.secret,
        };
        let outcome = self.distribution.publish(&p, &material).await;
        let mut tx = self
            .identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let current = self.load_protected(&mut tx, id).await?;
        let end = self.now()?;
        if current.revision != reserved_revision
            || current.state != "uncertain"
            || current.challenge != row.challenge
        {
            return Err(AwsIdentityError::Conflict);
        }
        let eligible = self
            .authority_in(
                &mut tx,
                &p,
                request,
                end,
                if row.challenge.operation == PossessionOperation::Renew {
                    CredentialScopeUse::Renew
                } else {
                    CredentialScopeUse::Bootstrap
                },
            )
            .await
            .and_then(|_| self.check_fact(&p, &verified, end));
        let state = if outcome.is_ok() && eligible.is_ok() {
            "delivered"
        } else {
            "uncertain"
        };
        if sqlx::query("UPDATE aws_protected_challenges SET state=?,revision=revision+1 WHERE challenge_id=? AND revision=? AND state IN ('pending','uncertain')").bind(state).bind(id.as_str()).bind(reserved_revision).execute(&mut *tx).await?.rows_affected()!=1{return Err(AwsIdentityError::Conflict);}
        self.audit(&mut tx, id, request, state, end).await?;
        tx.commit().await?;
        eligible?;
        outcome?;
        Ok(row.challenge)
    }
}
pub(in crate::controller_keys::worker_auth) struct PossessionIdentity {
    pub(in crate::controller_keys::worker_auth) service: Arc<SecureFargateBootstrap>,
    pub(in crate::controller_keys::worker_auth) transport: Arc<https::context::HttpTransport>,
}
impl IdentityAuthority for PossessionIdentity {
    fn control_in<'a>(
        &'a self,
        c: &'a mut SqliteConnection,
        r: &'a EnrollmentRecord,
        request: &'a RequestId,
        now: i64,
        usage: CredentialScopeUse,
    ) -> IdentityCommitFuture<'a> {
        Box::pin(async move {
            let p = self
                .service
                .load_plan_optional(c, &r.lease())
                .await
                .map_err(denied)?
                .ok_or(WorkerAuthError::Denied)?;
            if matches!(usage, CredentialScopeUse::Stop) {
                if p.execution.binding != r.scope.binding
                    || p.execution.command_digest.as_str() != r.scope.command_digest
                {
                    return Err(WorkerAuthError::Denied);
                }
                let scope = self
                    .service
                    .identity
                    .guard
                    .scope(c, &r.lease(), now, CredentialScopeUse::Stop)
                    .await?;
                if scope != r.scope {
                    return Err(WorkerAuthError::Denied);
                }
                return Ok(());
            }
            let scope = self
                .service
                .authority_in(c, &p, request, now, usage)
                .await
                .map_err(denied)?;
            if scope != r.scope {
                return Err(WorkerAuthError::Denied);
            }
            // Latest known STOPPED/uncertain observation always dominates an old
            // token. Absence is not proof that the external task is running.
            let latest = self
                .service
                .identity
                .latest(c, &r.lease())
                .await
                .map_err(denied)?
                .ok_or(WorkerAuthError::Denied)?;
            if latest.control_denial.is_some()
                || !latest.assessment.is_ok_and(|a| {
                    a.lifecycle == EcsLifecycle::Running && a.configuration_correlated
                })
            {
                return Err(WorkerAuthError::Denied);
            }
            Ok(())
        })
    }
    fn verify<'a>(&'a self, c: &'a IdentityCheck<'a>, now: i64) -> VerificationFuture<'a> {
        Box::pin(async move {
            self.service
                .verify_request(c, &self.transport, now)
                .await
                .map_err(denied)
        })
    }
    fn current(
        &self,
        c: &IdentityCheck<'_>,
        _: &IdentityGrant,
        now: i64,
    ) -> Result<(), WorkerAuthError> {
        if c.provider.as_str() != AWS_PROVIDER
            || c.controller != &self.service.identity.guard.controller()
        {
            return Err(WorkerAuthError::Denied);
        }
        if c.stage != VerificationStage::Prepare {
            self.transport.accepted_channel(c, now)?;
        }
        Ok(())
    }
    fn current_in<'a>(
        &'a self,
        c: &'a mut SqliteConnection,
        check: &'a IdentityCheck<'a>,
        g: &'a IdentityGrant,
        now: i64,
    ) -> IdentityCommitFuture<'a> {
        Box::pin(async move {
            self.service
                .current_in(c, check, g, &self.transport, now)
                .await
                .map_err(denied)
        })
    }
    fn prepared_in<'a>(
        &'a self,
        c: &'a mut SqliteConnection,
        r: &'a EnrollmentRecord,
        s: &'a BootstrapSecret,
        now: i64,
    ) -> IdentityCommitFuture<'a> {
        Box::pin(async move { self.service.prepare_in(c, r, s, now).await.map_err(denied) })
    }
}

#[cfg(test)]
#[path = "secure_fixture.rs"]
pub(in crate::controller_keys::worker_auth) mod fixture;
