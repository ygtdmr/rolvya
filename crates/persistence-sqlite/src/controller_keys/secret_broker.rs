//! Encrypted local authority. Production authentication, entitlement and consumers
//! are unavailable: all public operations deny until trusted implementations exist.
use super::{ControllerBootstrap, ControllerKey};
use crate::{
    PlanStore, PrincipalId, RequestId,
    admission::*,
    worker_simulation::{PeerEvidence, WorkerAuthority},
};
use rolvya_domain::{ids::ProjectId, worker::WorkerBinding};
use std::{fmt, sync::Arc};
use zeroize::Zeroizing;
mod crypto;
mod store;
#[cfg(test)]
mod tests;
pub const MAX_SECRET_BYTES: usize = 65536;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerError {
    InvalidInput,
    TooLarge,
    UnavailableAuthority,
    Forbidden,
    NotFound,
    Revoked,
    StaleVersion,
    ReplayConflict,
    Integrity,
    UnsupportedVersion,
    Randomness,
    Storage,
    Busy,
    KeyUnavailable,
    AttemptAuthority,
}
impl fmt::Display for BrokerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "secret broker: {self:?}")
    }
}
impl std::error::Error for BrokerError {}
impl From<sqlx::Error> for BrokerError {
    fn from(e: sqlx::Error) -> Self {
        match crate::StoreError::from(e) {
            crate::StoreError::Busy => Self::Busy,
            _ => Self::Storage,
        }
    }
}
macro_rules! id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, BrokerError> {
                let s = value.into();
                if s.is_empty()
                    || s.len() > 64
                    || !s
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-".contains(&c))
                    || !s.as_bytes()[0].is_ascii_alphanumeric()
                {
                    return Err(BrokerError::InvalidInput);
                }
                Ok(Self(s))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}
id!(ConnectionId);
id!(AccountId);
id!(ProviderId);
id!(RecordId);
id!(SecretPurpose);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderMode {
    ApiKey,
    CliSubscription,
}
impl ProviderMode {
    fn name(self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::CliSubscription => "cli_subscription",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionScope {
    pub project: ProjectId,
    pub connection: ConnectionId,
    pub account: AccountId,
    pub provider: ProviderId,
    pub mode: ProviderMode,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordScope {
    pub connection: ConnectionScope,
    pub record: RecordId,
    pub purpose: SecretPurpose,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecretVersion(i64);
impl SecretVersion {
    pub fn new(v: u64) -> Result<Self, BrokerError> {
        let n = i64::try_from(v).map_err(|_| BrokerError::InvalidInput)?;
        if n == 0 {
            return Err(BrokerError::InvalidInput);
        }
        Ok(Self(n))
    }
    pub fn get(self) -> u64 {
        self.0 as u64
    }
    fn next(self) -> Result<Self, BrokerError> {
        Self::new(self.get().checked_add(1).ok_or(BrokerError::InvalidInput)?)
    }
}
/// Non-cloneable, non-serializable and redacted. Input only; no public plaintext getter.
/// ```compile_fail
/// use rolvya_persistence_sqlite::secret_broker::SecretValue;
/// fn leak(v:&SecretValue) { let _=serde_json::to_string(v); }
/// ```
pub struct SecretValue(Zeroizing<Vec<u8>>);
impl SecretValue {
    pub fn new(value: Vec<u8>) -> Result<Self, BrokerError> {
        let value = Zeroizing::new(value);
        if value.len() > MAX_SECRET_BYTES {
            return Err(BrokerError::TooLarge);
        }
        Ok(Self(value))
    }
}
impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue([REDACTED])")
    }
}
#[derive(Debug, Clone)]
pub struct AdminRequest {
    pub actor: PrincipalId,
    pub request_id: RequestId,
}
#[derive(Debug, Clone)]
pub struct PutSecret {
    pub scope: RecordScope,
    pub expected: Option<SecretVersion>,
    pub administration: AdminRequest,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerReceipt {
    pub event_sequence: i64,
    pub version: SecretVersion,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretMetadata {
    pub scope: RecordScope,
    pub version: SecretVersion,
    pub revoked: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BrokerAction {
    CreateConnection = 1,
    ReadMetadata = 2,
    Store = 3,
    Replace = 4,
    Revoke = 5,
    Use = 6,
    VerifyIntegrity = 7,
    ExportRecovery = 8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialOperation {
    /// Bounded controller-local check; no network or billable effect.
    LocalIntegrityInspection,
    ProxyModelRequest,
    RefreshConnection,
}
#[derive(Clone)]
pub struct SecretUseRequest {
    pub scope: RecordScope,
    pub version: SecretVersion,
    pub request_id: RequestId,
    pub operation: CredentialOperation,
    pub worker: Option<AttemptSecretScope>,
}
#[derive(Clone)]
pub struct AttemptSecretScope {
    pub binding: WorkerBinding,
    pub peer: PeerEvidence,
}
// Sealed integration seams. Only trusted modules inside this crate can install real
// authorities in S2-03/04/05. There is no public with_authority / development grant.
struct AdminContext<'a> {
    request: &'a AdminRequest,
    scope: &'a ConnectionScope,
    record: Option<&'a RecordScope>,
    action: BrokerAction,
    version: SecretVersion,
    commitment: &'a [u8; 32],
}
trait HumanAuthority: Send + Sync {
    fn check(&self, c: &AdminContext<'_>) -> Result<(), BrokerError>;
}
trait UseAuthority: Send + Sync {
    fn check(&self, r: &SecretUseRequest, now: i64) -> Result<(), BrokerError>;
}
trait EntitlementAuthority: Send + Sync {
    fn check(
        &self,
        c: &ConnectionScope,
        operation: CredentialOperation,
        now: i64,
    ) -> Result<(), BrokerError>;
}
/// Synchronous, bounded, no external effects. Future network consumers need their
/// own reviewed dispatch/authorization protocol; this is not a generic callback API.
trait TrustedConsumer: Send + Sync {
    fn consume(&self, r: &SecretUseRequest, value: &SecretValue) -> Result<(), BrokerError>;
}
struct Unimplemented;
impl HumanAuthority for Unimplemented {
    fn check(&self, c: &AdminContext<'_>) -> Result<(), BrokerError> {
        let _ = (
            c.request,
            c.scope,
            c.record,
            c.action,
            c.version,
            c.commitment,
        );
        Err(BrokerError::UnavailableAuthority)
    }
}
impl UseAuthority for Unimplemented {
    fn check(&self, _: &SecretUseRequest, _: i64) -> Result<(), BrokerError> {
        Err(BrokerError::UnavailableAuthority)
    }
}
impl EntitlementAuthority for Unimplemented {
    fn check(
        &self,
        _: &ConnectionScope,
        _: CredentialOperation,
        _: i64,
    ) -> Result<(), BrokerError> {
        Err(BrokerError::UnavailableAuthority)
    }
}
impl TrustedConsumer for Unimplemented {
    fn consume(&self, _: &SecretUseRequest, _: &SecretValue) -> Result<(), BrokerError> {
        Err(BrokerError::UnavailableAuthority)
    }
}
impl rolvya_plan_validator::AssignmentPolicy for Unimplemented {
    fn check(
        &self,
        _: &rolvya_domain::plan::ExecutionAssignment,
    ) -> Result<(), rolvya_plan_validator::AssignmentDenial> {
        Err(rolvya_plan_validator::AssignmentDenial::UnauthorizedAccessMode)
    }
}
impl SchedulerPolicy for Unimplemented {
    fn permits(&self, _: &PrincipalId, _: SchedulerAction<'_>, _: i64) -> bool {
        false
    }
}
impl WorkerAuthority for Unimplemented {
    fn permits(
        &self,
        _: &PeerEvidence,
        _: &WorkerBinding,
        _: crate::worker_simulation::WorkerOperation,
        _: Option<&rolvya_domain::worker::DeclaredCapabilities>,
        _: i64,
    ) -> bool {
        false
    }
}
#[cfg(test)]
type TestEntropy = Box<dyn Fn(&mut [u8]) -> Result<(), BrokerError> + Send + Sync>;
pub(super) fn os_random<const N: usize>() -> Result<Zeroizing<[u8; N]>, BrokerError> {
    // The vetted safe API exposes initialized bytes only after OS randomness
    // succeeds. Wipe even partially filled scratch on error.
    let mut scratch = Zeroizing::new([std::mem::MaybeUninit::uninit(); N]);
    let bytes = getrandom::fill_uninit(scratch.as_mut()).map_err(|_| BrokerError::Randomness)?;
    let bytes = (&*bytes).try_into().map_err(|_| BrokerError::Randomness)?;
    Ok(Zeroizing::new(bytes))
}
pub struct SecretBroker {
    store: PlanStore,
    key: ControllerKey,
    human: Arc<dyn HumanAuthority>,
    user: Arc<dyn UseAuthority>,
    entitlement: Arc<dyn EntitlementAuthority>,
    consumer: Arc<dyn TrustedConsumer>,
    policy: Arc<dyn SchedulerPolicy>,
    worker: Arc<dyn WorkerAuthority>,
    clock: Arc<dyn Clock>,
    #[cfg(test)]
    entropy: TestEntropy,
    #[cfg(test)]
    hook: Box<dyn Fn(Checkpoint) -> Result<(), BrokerError> + Send + Sync>,
}
impl SecretBroker {
    pub async fn open(bootstrap: &ControllerBootstrap) -> Result<Self, BrokerError> {
        let key = bootstrap
            .load_existing()
            .await
            .map_err(|_| BrokerError::KeyUnavailable)?;
        Ok(Self {
            store: bootstrap.store.clone(),
            key,
            human: Arc::new(Unimplemented),
            user: Arc::new(Unimplemented),
            entitlement: Arc::new(Unimplemented),
            consumer: Arc::new(Unimplemented),
            policy: Arc::new(Unimplemented),
            worker: Arc::new(Unimplemented),
            clock: Arc::new(SystemClock),
            #[cfg(test)]
            entropy: Box::new(|b| getrandom::fill(b).map_err(|_| BrokerError::Randomness)),
            #[cfg(test)]
            hook: Box::new(|_| Ok(())),
        })
    }
    fn random<const N: usize>(&self) -> Result<Zeroizing<[u8; N]>, BrokerError> {
        #[cfg(test)]
        {
            let mut bytes = Zeroizing::new([0u8; N]);
            (self.entropy)(bytes.as_mut())?;
            Ok(bytes)
        }
        #[cfg(not(test))]
        {
            os_random()
        }
    }
    fn checkpoint(&self, p: Checkpoint) -> Result<(), BrokerError> {
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
    fn now(&self) -> Result<i64, BrokerError> {
        self.clock.now().map_err(|_| BrokerError::AttemptAuthority)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Checkpoint {
    BeforeEncryption,
    AfterEncryption,
    BeforeCommit,
    AfterCommit,
}
