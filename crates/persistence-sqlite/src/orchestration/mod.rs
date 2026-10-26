//! R09 bounded synthetic orchestration, not a cloud service or new state authority.
mod artifacts;
mod coordinator;
mod observability;
use crate::{
    admission::{self, *},
    finance::*,
    worker_control::*,
    worker_simulation::*,
    *,
};
pub use artifacts::*;
pub use observability::*;
use rolvya_domain::{ids::*, plan::*, worker::*};
use rolvya_plan_validator::Limits;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
/// Concrete synthetic predicates only; unknown/ordinary prose criteria fail closed.
pub const ARTIFACT_CRITERION: &str = "synthetic/artifact-v1";
pub const INPUTS_CRITERION: &str = "synthetic/inputs-v1";
pub const GROUP_CRITERION: &str = "synthetic/required-artifacts-v1";
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrchestrationError {
    Storage(StoreError),
    Admission(AdmissionError),
    Worker(WorkerError),
    InvalidArtifact,
    MissingArtifact,
    MissingResult,
    UnsupportedCriteria,
    InvalidInputs,
    ScopeMismatch,
    LimitExceeded,
    Conflict,
    RecoveryRequired,
    FailureClassification,
    Disconnected,
    ExecutionFailed(FailureClass),
}
impl From<StoreError> for OrchestrationError {
    fn from(e: StoreError) -> Self {
        Self::Storage(e)
    }
}
impl From<sqlx::Error> for OrchestrationError {
    fn from(e: sqlx::Error) -> Self {
        Self::Storage(e.into())
    }
}
impl From<AdmissionError> for OrchestrationError {
    fn from(e: AdmissionError) -> Self {
        Self::Admission(e)
    }
}
impl From<WorkerError> for OrchestrationError {
    fn from(e: WorkerError) -> Self {
        Self::Worker(e)
    }
}
impl From<rolvya_domain::worker::ProtocolError> for OrchestrationError {
    fn from(e: rolvya_domain::worker::ProtocolError) -> Self {
        Self::Worker(e.into())
    }
}
impl fmt::Display for OrchestrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidArtifact => "synthetic artifact integrity or provenance rejected",
            Self::MissingArtifact => "synthetic artifact is unavailable",
            Self::MissingResult => "no successful durable result received",
            Self::UnsupportedCriteria => "synthetic acceptance criteria unsupported or incomplete",
            Self::FailureClassification => {
                "worker failure classification lacks independent synthetic evidence"
            }
            Self::InvalidInputs => "validated predecessor evidence does not match output",
            Self::ScopeMismatch => "synthetic evidence scope mismatch",
            Self::LimitExceeded => "bounded synthetic work limit exceeded",
            Self::Conflict => "conflicting durable synthetic evidence",
            _ => "orchestration authority or storage operation rejected",
        })
    }
}
impl std::error::Error for OrchestrationError {}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn encode(v: &impl Serialize, max: usize) -> Result<Vec<u8>, OrchestrationError> {
    let b = serde_json::to_vec(v).map_err(|_| StoreError::Integrity)?;
    if b.len() > max {
        return Err(OrchestrationError::LimitExceeded);
    }
    Ok(b)
}
fn one(n: u64) -> Result<(), OrchestrationError> {
    if n == 1 {
        Ok(())
    } else {
        Err(StoreError::Integrity.into())
    }
}
fn id(prefix: &str, v: &impl Serialize) -> Result<RequestId, OrchestrationError> {
    Ok(RequestId::new(format!(
        "{prefix}-{}",
        &hash(&encode(v, 65536)?)[..40]
    ))?)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyntheticAssignment {
    pub permit: ExecutionPermit,
    pub capabilities: DeclaredCapabilities,
    pub seed: u64,
    pub behavior: FakeBehavior,
}
/// Trusted synthetic-driver boundary. No default grants, credentials or real
/// authentication are provided. Original R06/R08 policy checks remain mandatory.
pub trait SyntheticExecutionAuthority: SchedulerPolicy + WorkerAuthority {
    fn actor(&self) -> PrincipalId;
    fn assignment(
        &self,
        task: &PlannedTask,
        ordinal: u8,
        now: i64,
    ) -> Result<SyntheticAssignment, OrchestrationError>;
    fn peer(&self, binding: &WorkerBinding) -> PeerEvidence;
}
#[derive(Debug, Clone, Copy)]
pub struct PassLimits {
    pub admissions: u16,
    pub events: u16,
    pub validations: u16,
    pub groups: u16,
}
impl Default for PassLimits {
    fn default() -> Self {
        Self {
            admissions: 16,
            events: 64,
            validations: 16,
            groups: 16,
        }
    }
}
impl PassLimits {
    fn check(self) -> Result<(), OrchestrationError> {
        if self.admissions > 100 || self.events > 256 || self.validations > 100 || self.groups > 100
        {
            Err(OrchestrationError::LimitExceeded)
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct RunLimits {
    pub passes: u16,
    pub work: PassLimits,
}
impl Default for RunLimits {
    fn default() -> Self {
        Self {
            passes: 128,
            work: PassLimits::default(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskBlock {
    pub task: TaskRef,
    pub reason: OrchestrationError,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunSnapshot {
    pub total: u64,
    pub ready: u64,
    pub running: u64,
    pub accepted: u64,
    pub waiting_dependencies: u64,
    pub received_unvalidated: u64,
    pub recovery_required: u64,
    pub uncertain_liabilities: u64,
}
#[derive(Debug, Clone, Default)]
pub struct PassReport {
    pub admitted: Vec<LeaseRef>,
    pub delivered: u16,
    pub validated: Vec<TaskRef>,
    pub aggregated: Vec<TaskRef>,
    pub blocked: Vec<TaskBlock>,
    pub next_wake: Option<i64>,
    pub snapshot: RunSnapshot,
}
impl PassReport {
    fn progress(&self) -> bool {
        !self.admitted.is_empty()
            || self.delivered > 0
            || !self.validated.is_empty()
            || !self.aggregated.is_empty()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Complete,
    Blocked,
    Paused,
}
#[derive(Debug, Clone)]
pub struct RunReport {
    pub status: RunStatus,
    pub passes: u16,
    pub snapshot: RunSnapshot,
    pub blocked: Vec<TaskBlock>,
    pub project_block: Option<OrchestrationError>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    format: u8,
    lease: LeaseRef,
    binding: WorkerBinding,
    assignment: SyntheticAssignment,
    inputs: Vec<SyntheticInput>,
}
/// One validated frozen plan snapshot per controller instance; all mutable
/// authority is read from SQLite. No persistent in-memory scheduling queue.
pub struct SyntheticCoordinator {
    store: PlanStore,
    plan: StoredPlan,
    positions: BTreeMap<TaskId, usize>,
    children: BTreeMap<TaskId, Vec<TaskId>>,
    limits: Limits,
}
