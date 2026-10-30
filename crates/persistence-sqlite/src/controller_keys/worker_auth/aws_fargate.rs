//! AWS-specific inspection and correlation, separate from permission to launch
//! or proof that an HTTP caller possesses a particular ECS task's identity.
mod inspection;
mod model;
mod possession;
mod protected;
mod secure;
pub(super) use secure::PossessionIdentity;
pub use secure::{DeliveredPossession, SecureFargateBootstrap};
pub(in crate::controller_keys) mod consent;
pub(in crate::controller_keys) mod credential_use;
mod delivery;
pub use delivery::ProtectedDelivery;
pub use possession::{PossessionChallenge, PossessionOperation};
mod sdk;
mod secrets_sdk;
mod store;
mod trusted;
pub use inspection::{InspectionAssessment, ProductionGate};
pub use model::*;
pub use store::{
    AWS_PROVIDER, AccountEnrollment, ExecutionRegistration, FargateIdentity, InspectionRecord,
};
pub use trusted::ControllerVerificationReceipt;
#[cfg(test)]
pub(super) use trusted::{fixture_account_expiry, fixture_open as reviewed_fixture_open};
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AwsIdentityError {
    InvalidInput,
    UnsupportedPartition,
    UnconfiguredAccount,
    UnverifiedControllerAuthority,
    AccountMismatch,
    ScopeMismatch,
    TaskNotVisible,
    TaskNotRunning,
    TaskStopped,
    UnknownExecution,
    TaskDefinitionMismatch,
    RoleMismatch,
    ImageMismatch,
    IsolationMismatch,
    TaskPossessionUnverified,
    ChallengeBindingMismatch,
    SecureDeliveryUnverified,
    ExpiredEvidence,
    Revoked,
    Conflict,
    FinancialRisk,
    Throttled,
    Transient,
    Storage,
    Busy,
    KeyUnavailable,
    Integrity,
    LiveLaunchDisabled,
    ResourceLimit,
}
impl std::fmt::Display for AwsIdentityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AWS worker identity: {self:?}")
    }
}
impl std::error::Error for AwsIdentityError {}

impl From<sqlx::Error> for AwsIdentityError {
    fn from(e: sqlx::Error) -> Self {
        if matches!(crate::StoreError::from(e), crate::StoreError::Busy) {
            Self::Busy
        } else {
            Self::Storage
        }
    }
}

/// Concrete AWS correlation adapter. No accepted task-possession construction is
/// available yet, so it cannot manufacture an S2-04 IdentityGrant.
pub(super) struct HttpsIdentity {
    pub(super) identity: std::sync::Arc<FargateIdentity>,
    pub(super) transport: std::sync::Arc<super::https::context::HttpTransport>,
}
impl super::IdentityAuthority for HttpsIdentity {
    fn verify<'a>(
        &'a self,
        c: &'a super::IdentityCheck<'a>,
        now: i64,
    ) -> super::VerificationFuture<'a> {
        Box::pin(async move {
            let _accepted_tls_channel = self.transport.accepted_channel(c, now)?;
            self.identity
                .check_https(c, now)
                .await
                .map_err(|_| super::WorkerAuthError::Denied)?;
            // Defense in depth: even a future observation refactor cannot grant
            // worker authority without a reviewed proof and protected delivery.
            Err(super::WorkerAuthError::Unavailable)
        })
    }
    fn current(
        &self,
        _: &super::IdentityCheck<'_>,
        _: &super::IdentityGrant,
        _: i64,
    ) -> Result<(), super::WorkerAuthError> {
        Err(super::WorkerAuthError::Unavailable)
    }
}
#[cfg(test)]
pub(in crate::controller_keys::worker_auth) use store::fixture;
#[cfg(test)]
pub(in crate::controller_keys) fn fixture_workload() -> FargateWorkload {
    sdk::tests::expected("task-a").workload
}

#[cfg(test)]
pub(in crate::controller_keys::worker_auth) use secure::fixture as secure_fixture;
