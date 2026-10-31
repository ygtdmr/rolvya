//! Provider/account eligibility is controller authority, never worker configuration.
//! Public construction cannot verify provider entitlement or consume credentials.
use super::{ControllerBootstrap, ControllerKey};
use crate::{admission::*, finance::EvidenceRef, *};
use hmac::{Hmac, Mac};
use rolvya_domain::{ids::AttemptId, plan::*};
use rolvya_plan_validator::AssignmentDenial;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};
use std::{future::Future, pin::Pin, sync::Arc};
pub(crate) mod gate;
mod reconciliation;
pub(in crate::controller_keys) mod store;
use reconciliation::ReconciliationAuthority;
pub use reconciliation::{CapacityRelease, CapacityReleaseReceipt};
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EligibilityReason {
    Eligible,
    AwaitingProviderVerification,
    AwaitingUserConsent,
    UnsupportedIntegration,
    ExpiredCredentials,
    RevokedConnection,
    QuotaExhausted,
    ConcurrencyUnavailable,
    UnsupportedModelEffort,
    ProviderPolicyChanged,
    AccountIdentityMismatch,
    InsufficientBudgetAuthority,
    UncertainFinancialExposure,
}
impl EligibilityReason {
    pub fn retryable(self) -> bool {
        matches!(self, Self::QuotaExhausted | Self::ConcurrencyUnavailable)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryError {
    InvalidInput,
    Unavailable,
    Denied,
    Conflict,
    Integrity,
    Busy,
    Storage,
    KeyUnavailable,
    Eligibility(EligibilityReason),
}
impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "provider registry: {self:?}")
    }
}
impl std::error::Error for RegistryError {}
impl From<sqlx::Error> for RegistryError {
    fn from(e: sqlx::Error) -> Self {
        if matches!(StoreError::from(e), StoreError::Busy) {
            Self::Busy
        } else {
            Self::Storage
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionId(String);
impl ConnectionId {
    pub fn parse(s: String) -> Result<Self, RegistryError> {
        atom(&s)?;
        Ok(Self(s))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn generate() -> Result<Self, RegistryError> {
        Ok(Self(hex(
            &super::secret_broker::os_random::<16>().map_err(|_| RegistryError::Unavailable)?[..]
        )))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialReference {
    pub connection: String,
    pub account: String,
    pub provider: String,
    pub record: String,
    pub purpose: String,
    pub version: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionDraft {
    pub id: ConnectionId,
    pub provider: String,
    pub account: String,
    pub mode: AccessMode,
    pub integration: String,
    pub deployment: String,
    pub credential: CredentialReference,
    pub models: Vec<String>,
}
impl ConnectionDraft {
    fn validate(&self) -> Result<(), RegistryError> {
        for s in [
            &self.id.0,
            &self.provider,
            &self.account,
            &self.integration,
            &self.deployment,
            &self.credential.connection,
            &self.credential.account,
            &self.credential.provider,
            &self.credential.record,
            &self.credential.purpose,
        ] {
            atom(s)?;
        }
        if self.models.is_empty() || self.models.len() > 32 {
            return Err(RegistryError::InvalidInput);
        }
        for m in &self.models {
            atom(m)?;
        }
        if self.credential.version < 1
            || self.credential.account != self.account
            || self.credential.provider != self.provider
        {
            return Err(RegistryError::InvalidInput);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConnectionChange {
    Enroll {
        draft: ConnectionDraft,
    },
    Grant {
        expected_grant_version: i64,
        connection: ConnectionId,
        expected_version: i64,
        credential: CredentialReference,
    },
    RemoveGrant {
        connection: ConnectionId,
        expected_version: i64,
        expected_grant_version: i64,
    },
    Revoke {
        connection: ConnectionId,
        expected_version: i64,
    },
    BeginRefresh {
        connection: ConnectionId,
        expected_version: i64,
        operation_id: String,
    },
    Permissions {
        connection: ConnectionId,
        expected_version: i64,
        models: Vec<String>,
    },
    Rotate {
        expected_grant_version: i64,
        connection: ConnectionId,
        expected_version: i64,
        credential: CredentialReference,
    },
}
impl ConnectionChange {
    fn connection(&self) -> &ConnectionId {
        match self {
            Self::Enroll { draft } => &draft.id,
            Self::Grant { connection, .. }
            | Self::Revoke { connection, .. }
            | Self::RemoveGrant { connection, .. }
            | Self::Rotate { connection, .. }
            | Self::BeginRefresh { connection, .. }
            | Self::Permissions { connection, .. } => connection,
        }
    }
    fn expected(&self) -> i64 {
        match self {
            Self::Enroll { .. } => 0,
            Self::Grant {
                expected_version, ..
            }
            | Self::RemoveGrant {
                expected_version, ..
            }
            | Self::Revoke {
                expected_version, ..
            }
            | Self::Rotate {
                expected_version, ..
            }
            | Self::BeginRefresh {
                expected_version, ..
            }
            | Self::Permissions {
                expected_version, ..
            } => *expected_version,
        }
    }
    fn grant_expected(&self) -> Option<i64> {
        match self {
            Self::Grant {
                expected_grant_version,
                ..
            }
            | Self::Rotate {
                expected_grant_version,
                ..
            }
            | Self::RemoveGrant {
                expected_grant_version,
                ..
            } => Some(*expected_grant_version),
            _ => None,
        }
    }
    pub(crate) fn validate(&self) -> Result<(), RegistryError> {
        atom(self.connection().as_str())?;
        if self.grant_expected().is_some_and(|v| v < 0) {
            return Err(RegistryError::InvalidInput);
        }
        if let Self::Enroll { draft } = self {
            draft.validate()?;
        } else if self.expected() < 1 {
            return Err(RegistryError::InvalidInput);
        }
        if let Self::Grant { credential, .. } | Self::Rotate { credential, .. } = self {
            for s in [
                &credential.connection,
                &credential.account,
                &credential.provider,
                &credential.record,
                &credential.purpose,
            ] {
                atom(s)?;
            }
            if credential.version < 1 {
                return Err(RegistryError::InvalidInput);
            }
        }
        if let Self::BeginRefresh { operation_id, .. } = self {
            atom(operation_id)?;
        }
        if let Self::Permissions { models, .. } = self {
            if models.is_empty() || models.len() > 32 {
                return Err(RegistryError::InvalidInput);
            }
            for m in models {
                atom(m)?;
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelCapability {
    pub model: String,
    pub revision: String,
    pub efforts: Vec<Effort>,
    pub cpu_millis: u32,
    pub memory_mib: u32,
    pub timeout_seconds: u32,
    pub infrastructure_provider: String,
    pub infrastructure_account: String,
    pub pool: String,
    pub region: String,
    pub resource_class: String,
    pub quota_units: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum EvidenceSource {
    Provider,
    #[cfg(test)]
    Synthetic,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PolicyClass {
    ApiGenerallySupported,
    NativeClientOnly,
    ThirdPartySpecificallyPermitted,
    RegistrationRequired,
    Disallowed,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AccountPermission {
    VerifiedApi,
    VerifiedHostedCli,
    NativeLoginOnly,
    AwaitingProof,
    Revoked,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CliIsolation {
    home_identity: String,
    revision: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::controller_keys) struct Entitlement {
    policy: PolicyClass,
    permission: AccountPermission,
    cli: Option<CliIsolation>,
    quota_unit: String,
    source: EvidenceSource,
    evidence: EvidenceRef,
    provider: String,
    account: String,
    mode: AccessMode,
    integration: String,
    deployment: String,
    source_url: String,
    revision: String,
    policy_version: i64,
    scope_digest: String,
    verified_against: i64,
    collected_at: i64,
    expires_at: i64,
    credential_expires_at: i64,
    verifier: String,
    decision: EligibilityReason,
    models: Vec<ModelCapability>,
    concurrency: u32,
    quota_window: String,
    quota_ceiling: i64,
    refresh_serialized: bool,
    refresh_operation: Option<String>,
}
impl Entitlement {
    pub(in crate::controller_keys) fn deadline(&self) -> i64 {
        self.expires_at.min(self.credential_expires_at)
    }
}
/// Immutable requested verification scope; this data is not verification authority.
#[derive(Debug, Clone)]
pub struct ProviderVerificationRequest {
    pub connection: ConnectionId,
    pub expected_version: i64,
    pub evidence: EvidenceRef,
}
/// Redacted facts only. Recorded verification is not a current eligibility grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionMetadata {
    pub connection: ConnectionId,
    pub provider: String,
    pub account: String,
    pub mode: AccessMode,
    pub connection_version: i64,
    pub grant_version: i64,
    pub revoked: bool,
    pub consented_models: Vec<String>,
    pub recorded_verification_deadline: Option<i64>,
}
struct VerificationCheck<'a> {
    connection: &'a ConnectionId,
    version: i64,
    record: &'a ConnectionRecord,
    grant: &'a ProjectGrant,
    evidence: &'a EvidenceRef,
    now: i64,
}
type VerificationFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Entitlement, RegistryError>> + Send + 'a>>;
trait EntitlementVerifier: Send + Sync {
    fn verify<'a>(&'a self, c: &'a VerificationCheck<'a>) -> VerificationFuture<'a>;
    fn current(
        &self,
        c: &VerificationCheck<'_>,
        e: &Entitlement,
        now: i64,
    ) -> Result<(), RegistryError>;
}
impl VerificationCheck<'_> {
    fn validate(&self) -> Result<(), RegistryError> {
        if self.connection != &self.record.id
            || self.version != self.record.version
            || self.grant.connection != *self.connection
            || self.grant.revoked
            || self.evidence.as_str().is_empty()
            || self.now < 0
        {
            return Err(RegistryError::Denied);
        }
        Ok(())
    }
}
struct Unavailable;
impl EntitlementVerifier for Unavailable {
    fn verify<'a>(&'a self, c: &'a VerificationCheck<'a>) -> VerificationFuture<'a> {
        Box::pin(async move {
            c.validate()?;
            Err(RegistryError::Unavailable)
        })
    }
    fn current(
        &self,
        _: &VerificationCheck<'_>,
        _: &Entitlement,
        _: i64,
    ) -> Result<(), RegistryError> {
        Err(RegistryError::Unavailable)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::controller_keys) struct ConnectionRecord {
    id: ConnectionId,
    format: u32,
    installation: String,
    owner: Option<String>,
    provider: String,
    account: String,
    mode: AccessMode,
    integration: String,
    deployment: String,
    version: i64,
    revoked: bool,
    models: Vec<String>,
    entitlement: Option<Entitlement>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectGrant {
    consent_version: i64,
    models: Vec<String>,
    revoked: bool,
    verified_connection_version: Option<i64>,
    entitlement: Option<Entitlement>,
    connection: ConnectionId,
    project: ProjectId,
    version: i64,
    credential: CredentialReference,
    consent: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposal {
    id: String,
    format: u32,
    actor: String,
    project: ProjectId,
    change: ConnectionChange,
    created: i64,
    expires: i64,
}
/// Holds a separately loaded opaque existing KEK; offers no plaintext export,
/// provider verifier override, or production credential-consumption implementation.
pub struct ProviderRegistry {
    store: PlanStore,
    key: ControllerKey,
    verifier: Arc<dyn EntitlementVerifier>,
    reconciliation: Arc<dyn ReconciliationAuthority>,
}
impl ProviderRegistry {
    pub async fn open(b: &ControllerBootstrap) -> Result<Self, RegistryError> {
        Ok(Self {
            store: b.store.clone(),
            key: b
                .load_existing()
                .await
                .map_err(|_| RegistryError::KeyUnavailable)?,
            verifier: Arc::new(Unavailable),
            reconciliation: Arc::new(Unavailable),
        })
    }
    pub fn policy<P: SchedulerPolicy>(self: &Arc<Self>, inner: P) -> RegistryPolicy<P> {
        RegistryPolicy {
            registry: self.clone(),
            inner,
        }
    }
    fn scope_digest(
        &self,
        id: &ConnectionId,
        r: &ConnectionRecord,
        g: &ProjectGrant,
    ) -> Result<String, RegistryError> {
        Ok(hex(&Sha256::digest(encode(&(
            1,
            id,
            r.version,
            &r.installation,
            &r.owner,
            &r.provider,
            &r.account,
            r.mode,
            &r.integration,
            &r.deployment,
            &r.models,
            (&g.project, g.consent_version, &g.credential, &g.models),
        ))?)))
    }
    fn mac(&self, domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
        mac(&self.key, domain, parts)
    }
}
/// Composes existing independent scheduling/budget policy with opaque DB-backed
/// registry authority. It cannot override a denial from either policy.
pub struct RegistryPolicy<P> {
    registry: Arc<ProviderRegistry>,
    inner: P,
}
#[cfg(test)]
impl<P> RegistryPolicy<P> {
    pub(crate) fn fixture_reuse<Q>(&self, inner: Q) -> RegistryPolicy<Q> {
        RegistryPolicy {
            registry: self.registry.clone(),
            inner,
        }
    }
}
impl<P: SchedulerPolicy> AssignmentPolicy for RegistryPolicy<P> {
    fn check(&self, a: &ExecutionAssignment) -> Result<(), AssignmentDenial> {
        self.inner.check(a)
    }
}
impl<P: SchedulerPolicy> SchedulerPolicy for RegistryPolicy<P> {
    fn permits(&self, o: &PrincipalId, a: SchedulerAction<'_>, t: i64) -> bool {
        self.inner.permits(o, a, t)
    }
    fn provider_registry(&self) -> Option<&ProviderRegistry> {
        Some(&self.registry)
    }
}
fn atom(s: &str) -> Result<(), RegistryError> {
    if s.is_empty()
        || s.len() > 64
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:/".contains(&b))
    {
        Err(RegistryError::InvalidInput)
    } else {
        Ok(())
    }
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn mac(key: &ControllerKey, domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(&key.material[..])
        .expect("HMAC accepts every key length");
    for p in [
        b"rolvya/provider-registry/v1".as_slice(),
        &key.metadata.installation.0[..],
        &key.metadata.key.0[..],
        &key.metadata.key_version.to_be_bytes()[..],
        domain,
    ]
    .into_iter()
    .chain(parts.iter().copied())
    {
        m.update(&(p.len() as u64).to_be_bytes());
        m.update(p);
    }
    m.finalize().into_bytes().into()
}
fn encode<T: Serialize>(v: &T) -> Result<String, RegistryError> {
    let s = serde_json::to_string(v).map_err(|_| RegistryError::Integrity)?;
    if s.len() > 16384 {
        Err(RegistryError::InvalidInput)
    } else {
        Ok(s)
    }
}
fn one(n: u64) -> Result<(), RegistryError> {
    if n == 1 {
        Ok(())
    } else {
        Err(RegistryError::Integrity)
    }
}
impl<P: SchedulerPolicy + crate::worker_simulation::WorkerAuthority>
    crate::worker_simulation::WorkerAuthority for RegistryPolicy<P>
{
    fn permits(
        &self,
        p: &crate::worker_simulation::PeerEvidence,
        b: &rolvya_domain::worker::WorkerBinding,
        o: crate::worker_simulation::WorkerOperation,
        c: Option<&rolvya_domain::worker::DeclaredCapabilities>,
        t: i64,
    ) -> bool {
        crate::worker_simulation::WorkerAuthority::permits(&self.inner, p, b, o, c, t)
    }
}
impl<P: crate::orchestration::SyntheticExecutionAuthority>
    crate::orchestration::SyntheticExecutionAuthority for RegistryPolicy<P>
{
    fn actor(&self) -> PrincipalId {
        self.inner.actor()
    }
    fn assignment(
        &self,
        t: &PlannedTask,
        n: u8,
        now: i64,
    ) -> Result<crate::orchestration::SyntheticAssignment, crate::orchestration::OrchestrationError>
    {
        self.inner.assignment(t, n, now)
    }
    fn peer(
        &self,
        b: &rolvya_domain::worker::WorkerBinding,
    ) -> crate::worker_simulation::PeerEvidence {
        self.inner.peer(b)
    }
}
#[cfg(test)]
pub(in crate::controller_keys) use tests::{
    fixture_limit, fixture_owner_verifier, fixture_registry, fixture_registry_for, fixture_reopen,
};
