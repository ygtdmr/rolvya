mod common;
use common::*;
use rolvya_domain::{ids::*, plan::*, runtime::*};
use rolvya_plan_validator::*;

fn hash(plan: &PlanContent) -> PlanFingerprint {
    validate(plan, &SyntheticPolicy, Limits::default())
        .unwrap()
        .fingerprint()
}
#[test]
fn static_content_changes_the_fingerprint() {
    let p = plan();
    let first = hash(&p);
    let mut changed = p.clone();
    changed.tasks[0]
        .goal
        .push_str(" additional acceptance scope");
    assert_ne!(first, hash(&changed));
}
#[test]
fn all_unordered_collections_have_canonical_order() {
    let mut p = plan();
    let b = task("task-b", &p.key);
    let mut c = task("task-c", &p.key);
    c.dependencies = vec![task_ref(&p.tasks[0]), task_ref(&b)];
    c.upstream_artifacts = c
        .dependencies
        .iter()
        .map(|predecessor| ArtifactExpectation {
            predecessor: predecessor.clone(),
            name: "result".into(),
            sha256: "2".repeat(64),
        })
        .collect();
    c.scope = vec!["second".into(), "first".into()];
    c.exclusions = vec!["cloud".into(), "credentials".into()];
    c.acceptance_criteria = vec!["test".into(), "inspect".into()];
    c.execution.as_mut().unwrap().policy_refs = vec!["no-spend".into(), "no-network".into()];
    p.tasks.extend([b, c]);
    let original = validate(&p, &SyntheticPolicy, Limits::default()).unwrap();
    let mut permuted = p.clone();
    permuted.tasks.reverse();
    for node in &mut permuted.tasks {
        node.dependencies.reverse();
        node.scope.reverse();
        node.exclusions.reverse();
        node.acceptance_criteria.reverse();
        node.upstream_artifacts.reverse();
        node.execution.as_mut().unwrap().policy_refs.reverse();
    }
    let permuted = validate(&permuted, &SyntheticPolicy, Limits::default()).unwrap();
    assert_eq!(original.fingerprint(), permuted.fingerprint());
    assert_eq!(original.canonical_bytes(), permuted.canonical_bytes());
    assert_eq!(original.topological_order(), permuted.topological_order());
    let parsed: serde_json::Value = serde_json::from_slice(original.canonical_bytes()).unwrap();
    assert_eq!(parsed["format"], "rolvya-plan/v1");
    assert_eq!(parsed["plan"]["tasks"].as_array().unwrap().len(), 3);
}
#[test]
fn attempts_retries_and_runtime_assignment_changes_do_not_enter_static_content() {
    let p = plan();
    let initial = hash(&p);
    let mut attempt = AttemptRecord {
        id: AttemptId::new("attempt-1").unwrap(),
        task: task_ref(&p.tasks[0]),
        assignment: p.tasks[0].execution.clone().unwrap(),
        status: AttemptStatus::Running,
        lease_epoch: 1,
    };
    attempt.status = AttemptStatus::Failed;
    attempt.lease_epoch = 2;
    attempt.id = AttemptId::new("attempt-2").unwrap();
    attempt.assignment.model = "synthetic/fallback".into();
    assert_eq!(initial, hash(&p));
    let encoded = validate(&p, &SyntheticPolicy, Limits::default()).unwrap();
    let text = std::str::from_utf8(encoded.canonical_bytes()).unwrap();
    for runtime_field in [
        "attempt-2",
        "lease_epoch",
        "synthetic/fallback",
        "actual_cost",
    ] {
        assert!(!text.contains(runtime_field));
    }
    assert_eq!(attempt.task.task_id, p.tasks[0].id);
}
#[test]
fn wire_round_trip_preserves_valid_content_and_fingerprint() {
    let p = plan();
    let decoded: PlanContent = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
    assert_eq!(hash(&p), hash(&decoded));
    let mut wire = serde_json::to_value(&p).unwrap();
    wire["runtime_status"] = serde_json::json!("running");
    assert!(serde_json::from_value::<PlanContent>(wire).is_err());
}

struct SyntheticAllAssignments;
impl AssignmentPolicy for SyntheticAllAssignments {
    fn check(&self, _: &ExecutionAssignment) -> Result<(), AssignmentDenial> {
        Ok(())
    }
}
#[test]
fn every_static_assignment_and_identity_dimension_is_fingerprinted() {
    type Mutation = Box<dyn Fn(&mut PlanContent)>;
    let mutations: Vec<Mutation> = vec![
        Box::new(|p| {
            p.key.project_id = ProjectId::new("other-project").unwrap();
            p.tasks[0].plan = p.key.clone();
        }),
        Box::new(|p| {
            p.key.plan_id = PlanId::new("other-plan").unwrap();
            p.tasks[0].plan = p.key.clone();
        }),
        Box::new(|p| {
            p.key.version = PlanVersion::new(2).unwrap();
            p.tasks[0].plan = p.key.clone();
        }),
        Box::new(|p| p.root_policy = RootPolicy::SingleRoot),
        Box::new(|p| p.tasks[0].id = TaskId::new("other-task").unwrap()),
        Box::new(|p| p.tasks[0].instructions.push_str(" inspect changes")),
        Box::new(|p| p.tasks[0].scope.push("tests".into())),
        Box::new(|p| p.tasks[0].exclusions.push("production".into())),
        Box::new(|p| {
            p.tasks[0]
                .acceptance_criteria
                .push("reviewed result".into())
        }),
        Box::new(|p| p.tasks[0].context.reference.push_str("-2")),
        Box::new(|p| p.tasks[0].context.sha256 = "2".repeat(64)),
        Box::new(|p| p.tasks[0].execution.as_mut().unwrap().model = "synthetic/other".into()),
        Box::new(|p| p.tasks[0].execution.as_mut().unwrap().effort = Effort::High),
        Box::new(|p| p.tasks[0].execution.as_mut().unwrap().access_mode = AccessMode::CliSession),
        Box::new(|p| {
            p.tasks[0].execution.as_mut().unwrap().connection_ref = "synthetic-other".into()
        }),
        Box::new(|p| p.tasks[0].execution.as_mut().unwrap().resources.cpu_millis = 2000),
        Box::new(|p| p.tasks[0].execution.as_mut().unwrap().resources.memory_mib = 1024),
        Box::new(|p| {
            p.tasks[0]
                .execution
                .as_mut()
                .unwrap()
                .resources
                .timeout_seconds = 120
        }),
        Box::new(|p| {
            p.tasks[0]
                .execution
                .as_mut()
                .unwrap()
                .policy_refs
                .push("synthetic/review".into())
        }),
        Box::new(|p| {
            p.tasks[0].execution.as_mut().unwrap().write_scope = WriteScope::Remote {
                repository: "owner/repo".into(),
                git_ref: "refs/heads/codex/task".into(),
                paths: vec!["src".into()],
            };
        }),
    ];
    let original = plan();
    let original_hash = validate(&original, &SyntheticAllAssignments, Limits::default())
        .unwrap()
        .fingerprint();
    for mutate in mutations {
        let mut changed = original.clone();
        mutate(&mut changed);
        assert_ne!(
            original_hash,
            validate(&changed, &SyntheticAllAssignments, Limits::default())
                .unwrap()
                .fingerprint()
        );
    }
    use rolvya_domain::money::{Currency, Money};
    for category in 0..4 {
        let mut changed = original.clone();
        let pessimistic = &mut changed.tasks[0]
            .execution
            .as_mut()
            .unwrap()
            .costs
            .pessimistic;
        let amount = Money::new(1, Currency::new("USD").unwrap()).unwrap();
        match category {
            0 => pessimistic.ai_api = amount,
            1 => pessimistic.compute = amount,
            2 => pessimistic.storage = amount,
            _ => pessimistic.transfer = amount,
        }
        assert_ne!(
            original_hash,
            validate(&changed, &SyntheticAllAssignments, Limits::default())
                .unwrap()
                .fingerprint()
        );
    }
}
#[test]
fn static_relationships_and_remote_path_order_have_stable_fingerprints() {
    let mut p = plan();
    p.tasks.push(task("task-b", &p.key));
    let base = hash(&p);
    let predecessor = task_ref(&p.tasks[0]);
    p.tasks[1].dependencies.push(predecessor.clone());
    assert_ne!(base, hash(&p));
    let base = hash(&p);
    p.tasks[1].upstream_artifacts.push(ArtifactExpectation {
        predecessor,
        name: "source".into(),
        sha256: "3".repeat(64),
    });
    assert_ne!(base, hash(&p));
    p.tasks[0].execution.as_mut().unwrap().write_scope = WriteScope::Remote {
        repository: "owner/repo".into(),
        git_ref: "refs/heads/codex/task".into(),
        paths: vec!["src".into(), "tests".into()],
    };
    let base = hash(&p);
    if let WriteScope::Remote { paths, .. } =
        &mut p.tasks[0].execution.as_mut().unwrap().write_scope
    {
        paths.reverse();
    }
    assert_eq!(base, hash(&p));
}

#[test]
fn canonical_format_v1_golden_vector() {
    let p = plan();
    let checked = validate(&p, &SyntheticPolicy, Limits::default()).unwrap();
    assert_eq!(
        checked.fingerprint().to_string(),
        "4681e94df9500d3cffd8b76b611d3d9b9e9d629e0f52e25b286f48fc32b1c135"
    );
}
