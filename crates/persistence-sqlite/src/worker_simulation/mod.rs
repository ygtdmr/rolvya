//! R07 in-process simulation, not authenticated remote execution or durable R08 replay.
pub(crate) mod controller;
mod fake;
use crate::{StoreError, admission::AdmissionError};
pub use controller::*;
pub use fake::*;
use rolvya_domain::worker::*;
use std::fmt;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerError {
    Protocol(ProtocolError),
    Admission(AdmissionError),
    Storage(StoreError),
    UnknownAttempt,
    Unauthorized,
    ScopeMismatch,
    ContextMismatch,
    StaleEpoch,
    InvalidState,
    OutOfOrder { expected: u64 },
    ReplayConflict,
    ResourceLimit,
    InvalidTime,
    FinancialRisk,
}
impl From<ProtocolError> for WorkerError {
    fn from(v: ProtocolError) -> Self {
        Self::Protocol(v)
    }
}
impl From<AdmissionError> for WorkerError {
    fn from(v: AdmissionError) -> Self {
        Self::Admission(v)
    }
}
impl From<StoreError> for WorkerError {
    fn from(v: StoreError) -> Self {
        Self::Storage(v)
    }
}
impl From<sqlx::Error> for WorkerError {
    fn from(v: sqlx::Error) -> Self {
        Self::Storage(v.into())
    }
}
impl fmt::Display for WorkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Protocol(_) => "worker protocol rejected",
            Self::Admission(_) => "attempt authority rejected",
            Self::Storage(_) => "worker authority storage check failed",
            Self::UnknownAttempt => "unknown dispatched attempt",
            Self::Unauthorized => "worker identity or capability evidence rejected",
            Self::ScopeMismatch => "worker scope does not match assignment",
            Self::ContextMismatch => "immutable worker context does not match",
            Self::StaleEpoch => "worker fencing epoch is stale",
            Self::InvalidState => "worker lifecycle transition rejected",
            Self::OutOfOrder { .. } => "worker sequence is out of order",
            Self::ReplayConflict => "message identity was reused with different content",
            Self::ResourceLimit => "simulation resource limit exceeded",
            Self::InvalidTime => "simulation time is invalid",
            Self::FinancialRisk => "reservation coverage or billing exposure is uncertain",
        })
    }
}
impl std::error::Error for WorkerError {}
pub fn decode_worker_message(bytes: &[u8]) -> Result<WorkerMessage, WorkerError> {
    if bytes.is_empty() || bytes.len() > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::LimitExceeded.into());
    }
    let message: WorkerMessage =
        serde_json::from_slice(bytes).map_err(|_| ProtocolError::Malformed)?;
    message.validate()?;
    Ok(message)
}
pub fn encode_worker_message(message: &WorkerMessage) -> Result<Vec<u8>, WorkerError> {
    message.validate()?;
    let bytes = serde_json::to_vec(message).map_err(|_| ProtocolError::Malformed)?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::LimitExceeded.into());
    }
    Ok(bytes)
}
pub fn encode_controller_message(message: &ControllerMessage) -> Result<Vec<u8>, WorkerError> {
    message.validate()?;
    let bytes = serde_json::to_vec(message).map_err(|_| ProtocolError::Malformed)?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::LimitExceeded.into());
    }
    Ok(bytes)
}
pub fn decode_controller_message(bytes: &[u8]) -> Result<ControllerMessage, WorkerError> {
    if bytes.is_empty() || bytes.len() > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::LimitExceeded.into());
    }
    let message: ControllerMessage =
        serde_json::from_slice(bytes).map_err(|_| ProtocolError::Malformed)?;
    message.validate()?;
    Ok(message)
}
