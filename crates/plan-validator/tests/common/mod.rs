#![allow(dead_code)]
use rolvya_domain::{ids::*, money::*, plan::*};
use rolvya_plan_validator::{AssignmentDenial, AssignmentPolicy};

pub struct SyntheticPolicy;
impl AssignmentPolicy for SyntheticPolicy {
    fn check(&self, assignment: &ExecutionAssignment) -> Result<(), AssignmentDenial> {
        if assignment.model != "synthetic/model" {
            return Err(AssignmentDenial::UnknownModel);
        }
        if assignment.effort != Effort::Medium {
            return Err(AssignmentDenial::UnsupportedEffort);
        }
        if assignment.access_mode != AccessMode::ApiKey
            || assignment.connection_ref != "synthetic-connection"
        {
            return Err(AssignmentDenial::UnauthorizedAccessMode);
        }
        Ok(())
    }
}
pub fn task(id: &str, key: &PlanKey) -> PlannedTask {
    let zero = Money::new(0, Currency::new("USD").unwrap()).unwrap();
    let breakdown = CostBreakdown {
        ai_api: zero,
        compute: zero,
        storage: zero,
        transfer: zero,
    };
    PlannedTask {
        id: TaskId::new(id).unwrap(),
        plan: key.clone(),
        parent: None,
        dependencies: vec![],
        kind: TaskKind::ExecutableLeaf,
        required: true,
        goal: "Synthetic bounded task".into(),
        instructions: "Exercise pure domain behavior only".into(),
        scope: vec!["crates/example".into()],
        exclusions: vec!["live providers".into()],
        acceptance_criteria: vec!["deterministic test passes".into()],
        context: ContextReference {
            reference: "synthetic/context".into(),
            sha256: "1".repeat(64),
        },
        upstream_artifacts: vec![],
        execution: Some(ExecutionAssignment {
            model: "synthetic/model".into(),
            effort: Effort::Medium,
            access_mode: AccessMode::ApiKey,
            connection_ref: "synthetic-connection".into(),
            resources: ResourceRequirements {
                cpu_millis: 1000,
                memory_mib: 512,
                timeout_seconds: 60,
            },
            costs: CostScenarios {
                optimistic: breakdown,
                expected: breakdown,
                pessimistic: breakdown,
            },
            write_scope: WriteScope::ReadOnly {},
            policy_refs: vec!["synthetic/no-spend".into()],
        }),
    }
}
pub fn plan() -> PlanContent {
    let key = PlanKey {
        project_id: ProjectId::new("project").unwrap(),
        plan_id: PlanId::new("plan").unwrap(),
        version: PlanVersion::new(1).unwrap(),
    };
    PlanContent {
        schema_version: 1,
        tasks: vec![task("task-a", &key)],
        key,
        root_policy: RootPolicy::Forest,
    }
}
pub fn task_ref(task: &PlannedTask) -> TaskRef {
    TaskRef {
        plan: task.plan.clone(),
        task_id: task.id.clone(),
    }
}
pub fn group(id: &str, key: &PlanKey) -> PlannedTask {
    let mut result = task(id, key);
    result.kind = TaskKind::Group;
    result.execution = None;
    result
}
