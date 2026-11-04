//! Qualification evidence is not consent, admission, a credential or settlement.
//! All production effect authorities remain closed until independent external
//! issuers and effective enforcement have been separately qualified.
use super::{ControllerBootstrap, ControllerKey};
use crate::{PlanStore, RequestId};
use rolvya_domain::ids::ProjectId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
mod account;
pub use account::*;
mod configuration;
mod exposure;
mod store;
pub use configuration::*;
pub use exposure::*;
#[cfg(test)]
pub(in crate::controller_keys) use store::SimulationAuthority;
pub(crate) use store::panel_summary;
pub use store::{QualificationReport, QualificationStore, QualificationSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualificationError {
    InvalidInput,
    Scope,
    UntrustedIssuer,
    Expired,
    Revoked,
    Conflict,
    Integrity,
    Storage,
    Unbounded,
}
impl std::fmt::Display for QualificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "qualification: {self:?}")
    }
}
impl std::error::Error for QualificationError {}
impl From<sqlx::Error> for QualificationError {
    fn from(_: sqlx::Error) -> Self {
        Self::Storage
    }
}
type E = QualificationError;
fn digest<T: Serialize>(domain: &str, v: &T) -> Result<String, E> {
    let mut h = Sha256::new();
    h.update(domain.as_bytes());
    h.update([0]);
    h.update(serde_json::to_vec(v).map_err(|_| E::InvalidInput)?);
    Ok(format!("{:x}", h.finalize()))
}
fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn atom(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:/".contains(&b))
}

pub fn workload_commitment(
    w: &super::worker_auth::aws_fargate::FargateWorkload,
) -> Result<String, E> {
    w.validate().map_err(|_| E::Scope)?;
    digest("rolvya/qualified-workload/v1", w)
}
pub(crate) fn execution_commitment(
    b: &rolvya_domain::worker::WorkerBinding,
    d: &rolvya_domain::worker::Digest,
    p: &rolvya_domain::worker::ExecutionProfile,
) -> Result<String, E> {
    digest("rolvya/qualified-execution/v1", &(b, d, p))
}
pub(crate) fn launch_exposure_commitment(
    e: &super::worker_auth::aws_fargate::execution::ExposureEnvelope,
    m: &rolvya_domain::money::CostBreakdown,
    event: i64,
    bootstrap: &super::worker_auth::aws_fargate::execution::BootstrapAllocation,
) -> Result<String, E> {
    digest("rolvya/qualified-exposure/v1", &(e, m, event, bootstrap))
}
pub(crate) fn model_execution_commitment(
    w: &crate::orchestration::coding::work::CodingWork,
    operation: &rolvya_domain::worker::Digest,
) -> Result<String, E> {
    digest("rolvya/qualified-model-call/v1", &(w, operation))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    AwsIdentity,
    EffectiveIam,
    NetworkIsolation,
    WorkloadIsolation,
    BootstrapExclusivity,
    ProviderEntitlement,
    CostEnforcement,
    VerifierIsolation,
    Reconciliation,
}
pub const REQUIRED: [Domain; 9] = [
    Domain::AwsIdentity,
    Domain::EffectiveIam,
    Domain::NetworkIsolation,
    Domain::WorkloadIsolation,
    Domain::BootstrapExclusivity,
    Domain::ProviderEntitlement,
    Domain::CostEnforcement,
    Domain::VerifierIsolation,
    Domain::Reconciliation,
];
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLevel {
    ConfigurationInspected,
    Simulated,
    ExternallyObserved,
    IndependentlyQualified,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationClass {
    Inspection,
    CodingLaunch,
    VerifierLaunch,
    ModelCall,
}
/// Untrusted input until compared with current controller state. This type has
/// no dispatch methods and cannot construct an execution authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationScope {
    pub installation: String,
    pub project: ProjectId,
    pub infrastructure_account: String,
    pub partition: super::worker_auth::aws_fargate::Partition,
    pub region: String,
    pub aws_account: super::worker_auth::aws_fargate::AwsAccountId,
    pub task_definition: super::worker_auth::aws_fargate::TaskDefinitionArn,
    pub image_digest: String,
    pub infrastructure_version: i64,
    pub infrastructure_credential_version: i64,
    pub configuration_sha256: String,
    pub workload_sha256: String,
    pub execution_sha256: String,
    pub exposure_sha256: String,
    pub connection: String,
    pub connection_version: i64,
    pub grant_version: i64,
    pub credential_version: i64,
    pub financial_revision: u64,
    pub operation: OperationClass,
    pub operation_id: String,
    pub attempt: String,
    pub epoch: u64,
}
impl QualificationScope {
    pub fn commitment(&self) -> Result<String, E> {
        if self.partition != self.task_definition.partition()
            || self.region != self.task_definition.region()
            || self.aws_account != self.task_definition.account()
            || !self.image_digest.strip_prefix("sha256:").is_some_and(hash)
            || self.installation.len() != 32
            || !self
                .installation
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || [
                &self.configuration_sha256,
                &self.workload_sha256,
                &self.execution_sha256,
                &self.exposure_sha256,
            ]
            .iter()
            .any(|s| !hash(s))
            || [
                &self.infrastructure_account,
                &self.connection,
                &self.operation_id,
                &self.attempt,
            ]
            .iter()
            .any(|s| !atom(s))
            || [
                self.infrastructure_version,
                self.infrastructure_credential_version,
                self.connection_version,
                self.grant_version,
                self.credential_version,
            ]
            .iter()
            .any(|v| *v < 1)
            || self.financial_revision == 0
            || self.epoch == 0
            || self.epoch > i64::MAX as u64
        {
            return Err(E::InvalidInput);
        }
        digest("rolvya/qualification-scope/v1", self)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceStatement {
    pub version: u16,
    pub request: RequestId,
    pub scope: QualificationScope,
    pub domain: Domain,
    pub level: EvidenceLevel,
    pub evidence_sha256: String,
    pub source_revision: String,
    pub observed_at: i64,
    pub expires_at: i64,
}
impl EvidenceStatement {
    fn validate(&self, now: i64) -> Result<(), E> {
        self.scope.commitment()?;
        if self.version != 1 || !hash(&self.evidence_sha256) || !hash(&self.source_revision) {
            return Err(E::InvalidInput);
        }
        if self.observed_at < 0
            || now < self.observed_at
            || now >= self.expires_at
            || self
                .expires_at
                .checked_sub(self.observed_at)
                .is_none_or(|v| v > 86400)
        {
            return Err(E::Expired);
        }
        Ok(())
    }
}
/// Signature bytes are data. Production callers cannot nominate a verification
/// key or promote a simulation by supplying a different level field.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedEvidence {
    pub statement: EvidenceStatement,
    pub issuer: String,
    pub signature: String,
}

#[cfg(test)]
mod tests;
