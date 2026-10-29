//! S2-04 worker control authority. Provider attestation and HTTPS are unavailable
//! by default; no worker ID, evidence reference or request object is a grant.
use super::{ControllerBootstrap, ControllerKey};
use crate::{
    PlanStore, RequestId, StoreError,
    admission::{self, Clock, LeaseRef, SchedulerAction, SchedulerPolicy, SystemClock},
    finance::EvidenceRef,
    worker_control,
    worker_simulation::{PeerEvidence, WorkerAuthority, WorkerError, WorkerOperation},
};
use hmac::{Hmac, Mac};
use rolvya_domain::worker::*;
use rolvya_plan_validator::{AssignmentDenial, AssignmentPolicy};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use sqlx::{Row, SqliteConnection};
use std::{fmt, future::Future, pin::Pin, sync::Arc};
use zeroize::Zeroizing;
mod bootstrap;
mod control;
mod credentials;
mod revocation;
pub use revocation::{WorkerRevocation, WorkerRevocationReceipt};
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerAuthError {
    InvalidInput,
    Unavailable,
    Denied,
    Conflict,
    Expired,
    Integrity,
    FinancialRisk,
    Protocol,
    Busy,
    Storage,
    KeyUnavailable,
    Randomness,
    ResourceLimit,
}
impl fmt::Display for WorkerAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "worker authentication: {self:?}")
    }
}
impl std::error::Error for WorkerAuthError {}
impl From<sqlx::Error> for WorkerAuthError {
    fn from(e: sqlx::Error) -> Self {
        match StoreError::from(e) {
            StoreError::Busy => Self::Busy,
            _ => Self::Storage,
        }
    }
}
impl From<WorkerError> for WorkerAuthError {
    fn from(e: WorkerError) -> Self {
        match e {
            WorkerError::Storage(StoreError::Busy) => Self::Busy,
            WorkerError::Storage(_) => Self::Storage,
            WorkerError::FinancialRisk => Self::FinancialRisk,
            WorkerError::Protocol(_) => Self::Protocol,
            WorkerError::ResourceLimit => Self::ResourceLimit,
            WorkerError::ReplayConflict => Self::Conflict,
            WorkerError::Admission(admission::AdmissionError::Expired) => Self::Expired,
            _ => Self::Denied,
        }
    }
}
macro_rules! locator {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub struct $name(String);
        impl $name {
            pub fn parse(value: String) -> Result<Self, WorkerAuthError> {
                if value.len() != 32
                    || !value
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(WorkerAuthError::InvalidInput);
                }
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}
locator!(WorkerEnrollmentId);
locator!(BootstrapChallengeId);
locator!(WorkerCredentialId);

/// Incoming bearer only. No serialization, clone or public export path.
/// ```compile_fail
/// use rolvya_persistence_sqlite::controller_keys::worker_auth::WorkerCredential;
/// fn leak(c: &WorkerCredential) { let _=serde_json::to_string(c); }
/// ```
pub struct WorkerCredential(Zeroizing<String>);
impl WorkerCredential {
    pub fn parse(value: String) -> Result<Self, WorkerAuthError> {
        let value = Zeroizing::new(value);
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(WorkerAuthError::InvalidInput);
        }
        Ok(Self(value))
    }
}
impl fmt::Debug for WorkerCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WorkerCredential([redacted])")
    }
}
pub struct BootstrapSecret(WorkerCredential);
impl BootstrapSecret {
    pub fn parse(value: String) -> Result<Self, WorkerAuthError> {
        Ok(Self(WorkerCredential::parse(value)?))
    }
}
impl fmt::Debug for BootstrapSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("BootstrapSecret([redacted])")
    }
}
#[derive(Debug, Clone)]
pub struct WorkerRequestContext {
    pub request_id: RequestId,
    pub transport_evidence: EvidenceRef,
}
#[derive(Debug, Clone)]
pub struct PrepareBootstrap {
    pub lease: LeaseRef,
    pub request_id: RequestId,
    pub evidence: EvidenceRef,
}
#[derive(Debug)]
pub struct BootstrapTicket {
    pub id: BootstrapChallengeId,
    pub enrollment: WorkerEnrollmentId,
    pub expires_at: i64,
    secret: Option<BootstrapSecret>,
}
pub struct BootstrapClaim<'a> {
    pub id: &'a BootstrapChallengeId,
    pub secret: &'a BootstrapSecret,
    pub evidence: &'a EvidenceRef,
    pub context: &'a WorkerRequestContext,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerCredentialMetadata {
    pub id: WorkerCredentialId,
    pub enrollment: WorkerEnrollmentId,
    pub binding: WorkerBinding,
    pub generation: i64,
    pub issued_at: i64,
    pub expires_at: i64,
}
#[derive(Debug)]
pub struct WorkerCredentialTicket {
    pub metadata: WorkerCredentialMetadata,
    secret: WorkerCredential,
    response_request: RequestId,
    response_operation: WorkerOperation,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum VerificationStage {
    Prepare,
    Bootstrap,
    Renew,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifiedWorkerIdentity {
    subject: EvidenceRef,
    execution: EvidenceRef,
    capabilities: DeclaredCapabilities,
    permissions: Vec<WorkerPermission>,
    valid_until: i64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WorkerPermission {
    Register,
    Claim,
    AcceptLease,
    Heartbeat,
    Progress,
    SubmitResult,
    StopQuery,
    StopAck,
    Renew,
}
impl WorkerPermission {
    fn operation(self) -> WorkerOperation {
        match self {
            Self::Register => WorkerOperation::Register,
            Self::Claim => WorkerOperation::Claim,
            Self::AcceptLease => WorkerOperation::AcceptLease,
            Self::Heartbeat => WorkerOperation::Heartbeat,
            Self::Progress => WorkerOperation::Progress,
            Self::SubmitResult => WorkerOperation::SubmitResult,
            Self::StopQuery => WorkerOperation::StopQuery,
            Self::StopAck => WorkerOperation::StopAck,
            Self::Renew => WorkerOperation::RenewCredential,
        }
    }
}
struct IdentityCheck<'a> {
    controller: &'a ControllerIdentity,
    stage: VerificationStage,
    request: &'a RequestId,
    evidence: &'a EvidenceRef,
    binding: &'a WorkerBinding,
    challenge: Option<&'a BootstrapChallengeId>,
    credential: Option<&'a WorkerCredentialMetadata>,
    provider: &'a EvidenceRef,
    account: &'a EvidenceRef,
    pool: &'a EvidenceRef,
    command_digest: &'a str,
}
struct IdentityGrant {
    identity: VerifiedWorkerIdentity,
    scope: VerifiedIdentityScope,
    not_before: i64,
    expires_at: i64,
}
/// Verified statement/controller-side correlation, not reflected worker claims.
#[derive(Clone, PartialEq, Eq)]
struct VerifiedIdentityScope {
    controller: ControllerIdentity,
    stage: VerificationStage,
    request: RequestId,
    evidence: EvidenceRef,
    binding: WorkerBinding,
    challenge: Option<BootstrapChallengeId>,
    credential: Option<WorkerCredentialMetadata>,
    provider: EvidenceRef,
    account: EvidenceRef,
    pool: EvidenceRef,
    command_digest: String,
}
impl IdentityCheck<'_> {
    fn scope(&self) -> VerifiedIdentityScope {
        VerifiedIdentityScope {
            controller: self.controller.clone(),
            stage: self.stage,
            request: self.request.clone(),
            evidence: self.evidence.clone(),
            binding: self.binding.clone(),
            challenge: self.challenge.cloned(),
            credential: self.credential.cloned(),
            provider: self.provider.clone(),
            account: self.account.clone(),
            pool: self.pool.clone(),
            command_digest: self.command_digest.to_owned(),
        }
    }
}
type VerificationFuture<'a> =
    Pin<Box<dyn Future<Output = Result<IdentityGrant, WorkerAuthError>> + Send + 'a>>;
/// Private, reviewed provider integrations only. External verification happens
/// without a SQLite write lock; current() must be bounded and perform no I/O.
trait IdentityAuthority: Send + Sync {
    fn verify<'a>(&'a self, check: &'a IdentityCheck<'a>, now: i64) -> VerificationFuture<'a>;
    fn current(
        &self,
        check: &IdentityCheck<'_>,
        grant: &IdentityGrant,
        now: i64,
    ) -> Result<(), WorkerAuthError>;
}
trait WorkerTransport: Send + Sync {
    fn verify(
        &self,
        context: &WorkerRequestContext,
        binding: &WorkerBinding,
        operation: WorkerOperation,
        now: i64,
    ) -> Result<(), WorkerAuthError>;
    fn bootstrap(
        &self,
        request: &PrepareBootstrap,
        ticket: &BootstrapTicket,
    ) -> Result<(), WorkerAuthError>;
    fn credential(
        &self,
        context: &WorkerRequestContext,
        ticket: &WorkerCredentialTicket,
    ) -> Result<(), WorkerAuthError>;
}
trait RevocationAuthority: Send + Sync {
    fn verify(
        &self,
        request: &WorkerRevocation,
        record: &EnrollmentRecord,
        now: i64,
    ) -> Result<(), WorkerAuthError>;
}
struct Unimplemented;
impl IdentityAuthority for Unimplemented {
    fn verify<'a>(&'a self, _: &'a IdentityCheck<'a>, _: i64) -> VerificationFuture<'a> {
        Box::pin(async { Err(WorkerAuthError::Unavailable) })
    }
    fn current(
        &self,
        _: &IdentityCheck<'_>,
        _: &IdentityGrant,
        _: i64,
    ) -> Result<(), WorkerAuthError> {
        Err(WorkerAuthError::Unavailable)
    }
}
impl WorkerTransport for Unimplemented {
    fn verify(
        &self,
        _: &WorkerRequestContext,
        _: &WorkerBinding,
        _: WorkerOperation,
        _: i64,
    ) -> Result<(), WorkerAuthError> {
        Err(WorkerAuthError::Unavailable)
    }
    fn bootstrap(&self, _: &PrepareBootstrap, _: &BootstrapTicket) -> Result<(), WorkerAuthError> {
        Err(WorkerAuthError::Unavailable)
    }
    fn credential(
        &self,
        _: &WorkerRequestContext,
        _: &WorkerCredentialTicket,
    ) -> Result<(), WorkerAuthError> {
        Err(WorkerAuthError::Unavailable)
    }
}
impl AssignmentPolicy for Unimplemented {
    fn check(&self, _: &rolvya_domain::plan::ExecutionAssignment) -> Result<(), AssignmentDenial> {
        Err(AssignmentDenial::UnauthorizedAccessMode)
    }
}
impl SchedulerPolicy for Unimplemented {
    fn permits(&self, _: &crate::PrincipalId, _: SchedulerAction<'_>, _: i64) -> bool {
        false
    }
}
impl RevocationAuthority for Unimplemented {
    fn verify(
        &self,
        _: &WorkerRevocation,
        _: &EnrollmentRecord,
        _: i64,
    ) -> Result<(), WorkerAuthError> {
        Err(WorkerAuthError::Unavailable)
    }
}
/// No public verifier/policy overrides or credential-export callbacks. A future
/// provider/HTTPS integration must implement the private reviewed boundaries.
pub struct WorkerSecurity {
    store: PlanStore,
    key: ControllerKey,
    identity: Arc<dyn IdentityAuthority>,
    transport: Arc<dyn WorkerTransport>,
    policy: Arc<dyn SchedulerPolicy>,
    clock: Arc<dyn Clock>,
    revocation: Arc<dyn RevocationAuthority>,
    #[cfg(test)]
    hook: Box<dyn Fn(CommitCheckpoint) -> Result<(), WorkerAuthError> + Send + Sync>,
}
impl WorkerSecurity {
    fn controller(&self) -> ControllerIdentity {
        ControllerIdentity {
            installation: hex(&self.key.metadata.installation.0),
            key: hex(&self.key.metadata.key.0),
            key_version: self.key.metadata.key_version,
            format: self.key.metadata.format_version,
        }
    }
    pub async fn open(bootstrap: &ControllerBootstrap) -> Result<Self, WorkerAuthError> {
        let key = bootstrap
            .load_existing()
            .await
            .map_err(|_| WorkerAuthError::KeyUnavailable)?;
        Ok(Self {
            store: bootstrap.store.clone(),
            key,
            identity: Arc::new(Unimplemented),
            transport: Arc::new(Unimplemented),
            policy: Arc::new(Unimplemented),
            clock: Arc::new(SystemClock),
            revocation: Arc::new(Unimplemented),
            #[cfg(test)]
            hook: Box::new(|_| Ok(())),
        })
    }
    fn now(&self) -> Result<i64, WorkerAuthError> {
        let now = self.clock.now().map_err(|_| WorkerAuthError::Unavailable)?;
        if !(0..i64::MAX - 4000).contains(&now) {
            return Err(WorkerAuthError::Unavailable);
        }
        Ok(now)
    }
    fn checkpoint(&self, p: CommitCheckpoint) -> Result<(), WorkerAuthError> {
        #[cfg(test)]
        {
            (self.hook)(p)
        }
        #[cfg(not(test))]
        {
            let _ = p;
            Ok(())
        }
    }
    fn mac(&self, domain: &[u8], fields: &[&[u8]]) -> [u8; 32] {
        let mut h = <Hmac<Sha256> as Mac>::new_from_slice(&self.key.material[..])
            .expect("HMAC accepts every key length");
        h.update(b"rolvya/worker-authority/v1\0");
        for part in [
            &self.key.metadata.installation.0[..],
            &self.key.metadata.key.0[..],
            &self.key.metadata.key_version.to_be_bytes()[..],
            domain,
        ]
        .into_iter()
        .chain(fields.iter().copied())
        {
            h.update(&(part.len() as u64).to_be_bytes());
            h.update(part);
        }
        h.finalize().into_bytes().into()
    }
    async fn scope(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
        t: i64,
        usage: worker_control::CredentialScopeUse,
    ) -> Result<worker_control::CredentialScope, WorkerAuthError> {
        Ok(worker_control::credential_scope(c, l, self.policy.as_ref(), t, usage).await?)
    }
    async fn event(
        &self,
        c: &mut SqliteConnection,
        r: &EnrollmentRecord,
        request: &RequestId,
        kind: &str,
        t: i64,
    ) -> Result<i64, WorkerAuthError> {
        Ok(sqlx::query_scalar("INSERT INTO worker_security_events(project_id,attempt_id,enrollment_id,request_id,kind,recorded_at) VALUES(?,?,?,?,?,?) RETURNING sequence")
            .bind(r.scope.binding.task.plan.project_id.as_str()).bind(r.scope.binding.attempt_id.as_str()).bind(r.id.as_str()).bind(request.as_str()).bind(kind).bind(t).fetch_one(c).await?)
    }
    fn grant_current(
        &self,
        check: &IdentityCheck<'_>,
        g: &IdentityGrant,
        t: i64,
    ) -> Result<(), WorkerAuthError> {
        g.identity
            .capabilities
            .validate()
            .map_err(|_| WorkerAuthError::Denied)?;
        if g.scope != check.scope()
            || g.identity.capabilities.evidence.is_none()
            || g.identity.permissions.is_empty()
            || g.identity.permissions.len() > 9
            || g.identity
                .permissions
                .iter()
                .enumerate()
                .any(|(i, p)| g.identity.permissions[..i].contains(p))
            || t < g.not_before
            || t >= g.expires_at
            || t >= g.identity.valid_until
        {
            return Err(WorkerAuthError::Denied);
        }
        self.identity.current(check, g, t)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommitCheckpoint {
    BeforePrepare,
    AfterPrepare,
    BeforeBootstrap,
    AfterBootstrap,
    BeforeRenew,
    AfterRenew,
    BeforeMessage,
    AfterMessage,
    BeforeRevoke,
    AfterRevoke,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnrollmentRecord {
    format: u32,
    controller: ControllerIdentity,
    id: WorkerEnrollmentId,
    challenge: BootstrapChallengeId,
    scope: worker_control::CredentialScope,
    identity: VerifiedWorkerIdentity,
    request_id: RequestId,
    evidence: EvidenceRef,
    created_at: i64,
    challenge_expires_at: i64,
    valid_until: i64,
}
impl EnrollmentRecord {
    fn lease(&self) -> LeaseRef {
        LeaseRef {
            task: self.scope.binding.task.clone(),
            attempt_id: self.scope.binding.attempt_id.clone(),
            epoch: self.scope.binding.epoch,
        }
    }
    fn check<'a>(
        &'a self,
        stage: VerificationStage,
        request: &'a RequestId,
        evidence: &'a EvidenceRef,
        credential: Option<&'a WorkerCredentialMetadata>,
    ) -> IdentityCheck<'a> {
        IdentityCheck {
            controller: &self.controller,
            stage,
            request,
            evidence,
            binding: &self.scope.binding,
            challenge: if stage == VerificationStage::Bootstrap {
                Some(&self.challenge)
            } else {
                None
            },
            credential,
            provider: &self.scope.provider,
            account: &self.scope.account,
            pool: &self.scope.pool,
            command_digest: &self.scope.command_digest,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ControllerIdentity {
    installation: String,
    key: String,
    key_version: u64,
    format: u32,
}
fn one(n: u64) -> Result<(), WorkerAuthError> {
    if n == 1 {
        Ok(())
    } else {
        Err(WorkerAuthError::Integrity)
    }
}
fn encode<T: Serialize>(v: &T) -> Result<String, WorkerAuthError> {
    let text = serde_json::to_string(v).map_err(|_| WorkerAuthError::Integrity)?;
    if text.len() > 16384 {
        return Err(WorkerAuthError::InvalidInput);
    }
    Ok(text)
}
fn equal(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    a.ct_eq(b).into()
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}
fn random<const N: usize>() -> Result<Zeroizing<[u8; N]>, WorkerAuthError> {
    super::secret_broker::os_random().map_err(|_| WorkerAuthError::Randomness)
}
fn new_id() -> Result<String, WorkerAuthError> {
    Ok(hex(&random::<16>()?[..]))
}
fn bearer() -> Result<WorkerCredential, WorkerAuthError> {
    WorkerCredential::parse(hex(&random::<32>()?[..]))
}
fn no_bearer_reference(secret: &WorkerCredential, fields: &[&[u8]]) -> Result<(), WorkerAuthError> {
    if fields
        .iter()
        .any(|f| f.windows(secret.0.len()).any(|w| w == secret.0.as_bytes()))
    {
        Err(WorkerAuthError::InvalidInput)
    } else {
        Ok(())
    }
}

#[cfg(test)]
pub(in crate::controller_keys) use tests::FixtureWorker;
