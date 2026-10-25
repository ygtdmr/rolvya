//! Mutable attempt observations are deliberately separate from static plan content.
//! These records do not implement authorization or a scheduler state machine.
use crate::{
    ids::AttemptId,
    plan::{ExecutionAssignment, TaskRef},
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Created,
    Running,
    Succeeded,
    Failed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptRecord {
    pub id: AttemptId,
    pub task: TaskRef,
    pub assignment: ExecutionAssignment,
    pub status: AttemptStatus,
    pub lease_epoch: u64,
}
