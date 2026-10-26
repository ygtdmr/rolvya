//! Versioned transport-neutral worker data. Declarations and receipts are untrusted
//! observations, never authentication, budget authority or result acceptance.
use crate::{ids::*, plan::*};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_MESSAGE_BYTES: usize = 8192;
pub const MAX_MODELS: usize = 16;
pub const ACCESS_TOKEN_TTL_SECONDS: u32 = 900;
pub const HEARTBEAT_INTERVAL_SECONDS: u32 = 30;
pub const LEASE_DURATION_SECONDS: u32 = 90;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    UnsupportedVersion,
    InvalidMessage,
    InvalidBinding,
    InvalidCapabilities,
    LimitExceeded,
    Malformed,
}
impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnsupportedVersion => "unsupported worker protocol version",
            Self::InvalidMessage => "invalid worker message",
            Self::InvalidBinding => "invalid worker scope",
            Self::InvalidCapabilities => "invalid declared capabilities",
            Self::LimitExceeded => "worker protocol resource limit exceeded",
            Self::Malformed => "malformed worker protocol payload",
        })
    }
}
impl std::error::Error for ProtocolError {}
pub fn validate_version(v: u16) -> Result<(), ProtocolError> {
    if v == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(ProtocolError::UnsupportedVersion)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Digest(String);
impl Digest {
    pub fn new(s: impl Into<String>) -> Result<Self, ProtocolError> {
        let s = s.into();
        if s.len() != 64
            || !s
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(ProtocolError::InvalidBinding);
        }
        Ok(Self(s))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for Digest {
    type Error = ProtocolError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(s)
    }
}
impl From<Digest> for String {
    fn from(v: Digest) -> Self {
        v.0
    }
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ModelName(String);
impl fmt::Debug for ModelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ModelName(..)")
    }
}
impl ModelName {
    pub fn new(s: impl Into<String>) -> Result<Self, ProtocolError> {
        let s = s.into();
        if s.is_empty()
            || s.len() > 128
            || !s
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/._-:".contains(&b))
        {
            return Err(ProtocolError::InvalidCapabilities);
        }
        Ok(Self(s))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for ModelName {
    type Error = ProtocolError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(s)
    }
}
impl From<ModelName> for String {
    fn from(v: ModelName) -> Self {
        v.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerBinding {
    pub task: TaskRef,
    pub attempt_id: AttemptId,
    pub worker_id: WorkerId,
    pub epoch: u64,
    pub plan_hash: Digest,
    pub context_hash: Digest,
}
impl WorkerBinding {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.epoch == 0
            || self.epoch > i64::MAX as u64
            || self.task.plan.version.get() > i64::MAX as u64
        {
            Err(ProtocolError::InvalidBinding)
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclaredCapabilities {
    pub models: Vec<ModelName>,
    pub efforts: Vec<Effort>,
    pub access_modes: Vec<AccessMode>,
    pub cpu_millis: u32,
    pub memory_mib: u32,
    /// Non-secret evidence reference. A declaration alone never verifies it.
    pub evidence: Option<CapabilityRef>,
}
fn unique<T: PartialEq>(items: &[T]) -> bool {
    items
        .iter()
        .enumerate()
        .all(|(i, v)| !items[..i].contains(v))
}
impl DeclaredCapabilities {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.models.is_empty()
            || self.models.len() > MAX_MODELS
            || self.efforts.is_empty()
            || self.efforts.len() > 4
            || self.access_modes.is_empty()
            || self.access_modes.len() > 2
            || self.cpu_millis == 0
            || self.memory_mib == 0
            || !unique(&self.models)
            || !unique(&self.efforts)
            || !unique(&self.access_modes)
        {
            return Err(ProtocolError::InvalidCapabilities);
        }
        Ok(())
    }
    pub fn supports(&self, p: &ExecutionProfile) -> bool {
        self.models.contains(&p.model)
            && self.efforts.contains(&p.effort)
            && self.access_modes.contains(&p.access_mode)
            && self.cpu_millis >= p.resources.cpu_millis
            && self.memory_mib >= p.resources.memory_mib
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionProfile {
    pub model: ModelName,
    pub effort: Effort,
    pub access_mode: AccessMode,
    pub resources: ResourceRequirements,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    Controlled,
    Transient,
    Permanent,
    Cancelled,
    Disconnected,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "body",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ExecutionOutcome {
    Succeeded { manifest_sha256: Digest },
    Failed { class: FailureClass },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressCode {
    Starting,
    Executing,
    ProducingArtifacts,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "body",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum WorkerBody {
    Register {
        capabilities: DeclaredCapabilities,
    },
    Claim {},
    AcceptLease {
        offer_id: MessageId,
    },
    Heartbeat {},
    Progress {
        completed: u32,
        total: u32,
        code: ProgressCode,
    },
    SubmitResult {
        outcome: ExecutionOutcome,
    },
    GetStopInstruction {},
    StopAcknowledged {
        instruction_id: MessageId,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerMessage {
    pub protocol_version: u16,
    pub message_id: MessageId,
    pub binding: WorkerBinding,
    pub sequence: u64,
    pub body: WorkerBody,
}
impl WorkerMessage {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_version(self.protocol_version)?;
        self.binding.validate()?;
        if self.sequence == 0 {
            return Err(ProtocolError::InvalidMessage);
        }
        match &self.body {
            WorkerBody::Register { capabilities } => capabilities.validate()?,
            WorkerBody::Progress {
                completed, total, ..
            } if *total == 0 || completed > total => return Err(ProtocolError::InvalidMessage),
            _ => {}
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptKind {
    Registered,
    LeaseAccepted,
    HeartbeatObserved,
    ProgressObserved,
    ResultReceived,
    FailureReported,
    StoppedReported,
    InstructionObserved,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerReceipt {
    pub message_id: MessageId,
    pub sequence: u64,
    pub observed_at: i64,
    pub kind: ReceiptKind,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerTiming {
    pub access_token_ttl_seconds: u32,
    pub heartbeat_interval_seconds: u32,
    pub lease_duration_seconds: u32,
}
impl Default for WorkerTiming {
    fn default() -> Self {
        Self {
            access_token_ttl_seconds: ACCESS_TOKEN_TTL_SECONDS,
            heartbeat_interval_seconds: HEARTBEAT_INTERVAL_SECONDS,
            lease_duration_seconds: LEASE_DURATION_SECONDS,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseOffer {
    pub offer_id: MessageId,
    pub profile: ExecutionProfile,
    pub financial_event: i64,
    pub lease_until: i64,
    pub timing: WorkerTiming,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    ControllerCancellation,
    Fenced,
    LeaseExpired,
    BudgetRisk,
    AuthorityRevoked,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopInstruction {
    pub instruction_id: MessageId,
    pub reason: StopReason,
    pub next_epoch: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "body",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ControllerBody {
    Registered { receipt: WorkerReceipt },
    LeaseAssignment { offer: LeaseOffer },
    Acknowledged { receipt: WorkerReceipt },
    Stop { instruction: StopInstruction },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerMessage {
    pub protocol_version: u16,
    pub response_to: MessageId,
    pub binding: WorkerBinding,
    pub body: ControllerBody,
}
impl ControllerMessage {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_version(self.protocol_version)?;
        self.binding.validate()?;
        match &self.body {
            ControllerBody::Registered { receipt } | ControllerBody::Acknowledged { receipt } => {
                if receipt.sequence == 0
                    || receipt.observed_at < 0
                    || receipt.message_id != self.response_to
                {
                    return Err(ProtocolError::InvalidMessage);
                }
            }
            ControllerBody::LeaseAssignment { offer } => {
                if offer.offer_id != self.response_to
                    || offer.financial_event <= 0
                    || offer.lease_until < 0
                    || offer.timing != WorkerTiming::default()
                    || offer.profile.resources.cpu_millis == 0
                    || offer.profile.resources.memory_mib == 0
                    || offer.profile.resources.timeout_seconds == 0
                {
                    return Err(ProtocolError::InvalidMessage);
                }
            }
            ControllerBody::Stop { instruction } => {
                if instruction
                    .next_epoch
                    .is_some_and(|epoch| epoch <= self.binding.epoch || epoch > i64::MAX as u64)
                {
                    return Err(ProtocolError::InvalidBinding);
                }
            }
        }
        Ok(())
    }
}
