//! Versioned, flat planned content. These are draft data contracts; validation
//! produces an immutable borrowed snapshot in `rolvya-plan-validator`.
use crate::{ids::*, money::CostScenarios};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanKey {
    pub project_id: ProjectId,
    pub plan_id: PlanId,
    pub version: PlanVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRef {
    pub plan: PlanKey,
    pub task_id: TaskId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootPolicy {
    Forest,
    SingleRoot,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Group,
    ExecutableLeaf,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Low,
    Medium,
    High,
    ExtraHigh,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    ApiKey,
    CliSession,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextReference {
    pub reference: String,
    pub sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactExpectation {
    pub predecessor: TaskRef,
    pub name: String,
    pub sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceRequirements {
    pub cpu_millis: u32,
    pub memory_mib: u32,
    pub timeout_seconds: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WriteScope {
    ReadOnly {},
    Remote {
        repository: String,
        git_ref: String,
        paths: Vec<String>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionAssignment {
    pub model: String,
    pub effort: Effort,
    pub access_mode: AccessMode,
    /// Opaque connection identity, never credential contents.
    pub connection_ref: String,
    pub resources: ResourceRequirements,
    pub costs: CostScenarios,
    pub write_scope: WriteScope,
    pub policy_refs: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedTask {
    pub id: TaskId,
    pub plan: PlanKey,
    pub parent: Option<TaskRef>,
    pub dependencies: Vec<TaskRef>,
    pub kind: TaskKind,
    /// Only required children are supported in canonical format v1.
    pub required: bool,
    pub goal: String,
    pub instructions: String,
    pub scope: Vec<String>,
    pub exclusions: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub context: ContextReference,
    pub upstream_artifacts: Vec<ArtifactExpectation>,
    pub execution: Option<ExecutionAssignment>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanContent {
    pub schema_version: u16,
    pub key: PlanKey,
    pub root_policy: RootPolicy,
    pub tasks: Vec<PlannedTask>,
}
