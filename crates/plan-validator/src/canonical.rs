use rolvya_domain::plan::{PlanContent, WriteScope};
use serde::Serialize;

pub(crate) fn encode(plan: &PlanContent) -> Result<Vec<u8>, serde_json::Error> {
    // All listed Vec fields are semantic sets in format v1. Text inside each
    // element is preserved byte-for-byte. Struct declaration order is part of v1;
    // no maps/floats or runtime records enter this encoding.
    let mut normalized = plan.clone();
    normalized.tasks.sort_by(|a, b| a.id.cmp(&b.id));
    for task in &mut normalized.tasks {
        task.dependencies.sort();
        task.scope.sort();
        task.exclusions.sort();
        task.acceptance_criteria.sort();
        task.upstream_artifacts.sort_by(|a, b| {
            (&a.predecessor, &a.name, &a.sha256).cmp(&(&b.predecessor, &b.name, &b.sha256))
        });
        if let Some(execution) = &mut task.execution {
            execution.policy_refs.sort();
            if let WriteScope::Remote { paths, .. } = &mut execution.write_scope {
                paths.sort();
            }
        }
    }
    #[derive(Serialize)]
    struct Canonical<'a> {
        format: &'static str,
        plan: &'a PlanContent,
    }
    serde_json::to_vec(&Canonical {
        format: crate::CANONICAL_FORMAT,
        plan: &normalized,
    })
}
