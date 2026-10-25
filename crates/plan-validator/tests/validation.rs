mod common;
use common::*;
use rolvya_domain::plan::*;
use rolvya_plan_validator::{DiagnosticCode, Limits, validate};

#[test]
fn rejects_missing_parent_before_freezing() {
    let mut input = plan();
    input.tasks[0].parent = Some(task_ref(&task("missing", &input.key)));
    let errors = validate(&input, &SyntheticPolicy, Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.code == DiagnosticCode::MissingParent)
    );
}

fn errors(input: &PlanContent) -> Vec<rolvya_plan_validator::Diagnostic> {
    validate(input, &SyntheticPolicy, Limits::default()).unwrap_err()
}
fn has(input: &PlanContent, code: DiagnosticCode) {
    assert!(
        errors(input).iter().any(|error| error.code == code),
        "missing diagnostic {code:?}"
    );
}
fn write(task: &mut PlannedTask, target: &str, paths: &[&str]) {
    task.execution.as_mut().unwrap().write_scope = WriteScope::Remote {
        repository: "synthetic/repo".into(),
        git_ref: target.into(),
        paths: paths.iter().map(|s| (*s).into()).collect(),
    };
}

#[test]
fn valid_single_task_and_hierarchy_do_not_imply_dependency_edges() {
    let mut input = plan();
    assert_eq!(
        validate(&input, &SyntheticPolicy, Limits::default())
            .unwrap()
            .topological_order(),
        &[input.tasks[0].id.clone()]
    );
    let parent = group("a-group", &input.key);
    input.tasks[0].parent = Some(task_ref(&parent));
    input.tasks.push(parent);
    let checked = validate(&input, &SyntheticPolicy, Limits::default()).unwrap();
    assert_eq!(
        checked.topological_order()[0].as_str(),
        "a-group",
        "no child-before-parent edge in explicit order"
    );
    input.root_policy = RootPolicy::SingleRoot;
    assert!(validate(&input, &SyntheticPolicy, Limits::default()).is_ok());
}

#[test]
fn fan_in_fan_out_and_group_dependencies_are_valid() {
    let mut input = plan();
    let a = task("a", &input.key);
    let b = task("b", &input.key);
    let mut c = task("c", &input.key);
    c.dependencies = vec![task_ref(&b), task_ref(&a)];
    let mut d = task("d", &input.key);
    d.dependencies = vec![task_ref(&a)];
    input.tasks = vec![d, c, b, a];
    let result = validate(&input, &SyntheticPolicy, Limits::default()).unwrap();
    assert_eq!(
        result
            .topological_order()
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "c", "d"]
    );
    let g = group("group", &input.key);
    input.tasks[0].parent = Some(task_ref(&g));
    input.tasks[1].dependencies.push(task_ref(&g));
    input.tasks.push(g);
    assert!(validate(&input, &SyntheticPolicy, Limits::default()).is_ok());
}

#[test]
fn empty_schema_root_and_optional_contracts_fail_closed() {
    let mut input = plan();
    input.tasks.clear();
    has(&input, DiagnosticCode::EmptyPlan);
    let mut input = plan();
    input.schema_version = 2;
    has(&input, DiagnosticCode::UnsupportedSchema);
    let mut input = plan();
    input.tasks.push(task("b", &input.key));
    input.root_policy = RootPolicy::SingleRoot;
    has(&input, DiagnosticCode::InvalidRootCount);
    let mut input = plan();
    input.tasks[0].required = false;
    has(&input, DiagnosticCode::UnsupportedOptionalChild);
}

#[test]
fn duplicate_conflicting_definitions_are_order_independent() {
    let mut input = plan();
    let mut duplicate = input.tasks[0].clone();
    duplicate.goal = "different".into();
    input.tasks.push(duplicate);
    let expected = errors(&input);
    assert_eq!(expected.len(), 1);
    assert_eq!(expected[0].code, DiagnosticCode::DuplicateTask);
    input.tasks.reverse();
    assert_eq!(errors(&input), expected);
}

#[test]
fn rejects_missing_self_duplicate_and_cross_scope_references() {
    let mut input = plan();
    input.tasks[0].parent = Some(task_ref(&input.tasks[0]));
    has(&input, DiagnosticCode::SelfParent);
    let mut input = plan();
    let reference = task_ref(&input.tasks[0]);
    input.tasks[0].dependencies.push(reference);
    has(&input, DiagnosticCode::SelfDependency);
    let mut input = plan();
    input.tasks[0]
        .dependencies
        .push(task_ref(&task("missing", &input.key)));
    has(&input, DiagnosticCode::MissingDependency);
    let mut input = plan();
    let b = task("b", &input.key);
    input.tasks[0].dependencies = vec![task_ref(&b), task_ref(&b)];
    input.tasks.push(b);
    has(&input, DiagnosticCode::DuplicateDependency);
    for dimension in 0..3 {
        let mut input = plan();
        let mut foreign = input.key.clone();
        match dimension {
            0 => foreign.project_id = rolvya_domain::ids::ProjectId::new("other-project").unwrap(),
            1 => foreign.plan_id = rolvya_domain::ids::PlanId::new("other-plan").unwrap(),
            _ => foreign.version = rolvya_domain::ids::PlanVersion::new(2).unwrap(),
        }
        input.tasks[0].plan = foreign.clone();
        has(&input, DiagnosticCode::CrossPlanReference);
        input.tasks[0].plan = input.key.clone();
        input.tasks[0].parent = Some(TaskRef {
            plan: foreign.clone(),
            task_id: input.tasks[0].id.clone(),
        });
        has(&input, DiagnosticCode::CrossPlanReference);
        input.tasks[0].parent = None;
        let reference = TaskRef {
            plan: foreign,
            task_id: input.tasks[0].id.clone(),
        };
        input.tasks[0].dependencies.push(reference);
        has(&input, DiagnosticCode::CrossPlanReference);
    }
}

#[test]
fn exact_cycle_members_exclude_merely_blocked_successors() {
    let mut input = plan();
    let mut a = task("a", &input.key);
    let mut b = task("b", &input.key);
    let mut c = task("c", &input.key);
    a.dependencies.push(task_ref(&b));
    b.dependencies.push(task_ref(&a));
    c.dependencies.push(task_ref(&b));
    input.tasks = vec![c, b, a];
    let result = errors(&input);
    assert_eq!(
        result
            .iter()
            .filter(|d| d.code == DiagnosticCode::DependencyCycle)
            .map(|d| d.task.as_ref().unwrap().as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    let mut a = group("a", &input.key);
    let mut b = group("b", &input.key);
    a.parent = Some(task_ref(&b));
    b.parent = Some(task_ref(&a));
    input.tasks = vec![b, a];
    has(&input, DiagnosticCode::ParentCycle);
}

#[test]
fn group_completion_deadlock_is_detected_without_rewriting_dependencies() {
    let mut input = plan();
    let g = group("group", &input.key);
    input.tasks[0].parent = Some(task_ref(&g));
    input.tasks[0].dependencies.push(task_ref(&g));
    input.tasks.push(g);
    has(&input, DiagnosticCode::CompletionDeadlock);
    assert_eq!(input.tasks[0].dependencies.len(), 1);
}

#[test]
fn group_and_leaf_role_inconsistencies_are_rejected() {
    let mut input = plan();
    input.tasks[0].kind = TaskKind::Group;
    has(&input, DiagnosticCode::GroupExecution);
    input.tasks[0].execution = None;
    has(&input, DiagnosticCode::EmptyGroup);
    let mut input = plan();
    let mut child = task("child", &input.key);
    child.parent = Some(task_ref(&input.tasks[0]));
    input.tasks.push(child);
    has(&input, DiagnosticCode::ExecutableParent);
    let mut input = plan();
    input.tasks[0].execution = None;
    has(&input, DiagnosticCode::MissingAssignment);
}

#[test]
fn executable_completeness_and_structural_assignments_are_enforced() {
    for field in 0..9 {
        let mut input = plan();
        let task = &mut input.tasks[0];
        let code = match field {
            0 => {
                task.goal.clear();
                DiagnosticCode::EmptyField
            }
            1 => {
                task.instructions.clear();
                DiagnosticCode::EmptyField
            }
            2 => {
                task.scope.clear();
                DiagnosticCode::EmptyField
            }
            3 => {
                task.acceptance_criteria.clear();
                DiagnosticCode::EmptyField
            }
            4 => {
                task.context.sha256 = "F".repeat(64);
                DiagnosticCode::InvalidContext
            }
            5 => {
                task.context.reference.clear();
                DiagnosticCode::InvalidContext
            }
            6 => {
                task.execution.as_mut().unwrap().resources.cpu_millis = 0;
                DiagnosticCode::InvalidResources
            }
            7 => {
                task.execution.as_mut().unwrap().model = "bad model".into();
                DiagnosticCode::InvalidAssignment
            }
            _ => {
                task.execution.as_mut().unwrap().policy_refs.clear();
                DiagnosticCode::EmptyField
            }
        };
        has(&input, code);
    }
    let mut input = plan();
    let duplicate = input.tasks[0].scope[0].clone();
    input.tasks[0].scope.push(duplicate);
    has(&input, DiagnosticCode::DuplicateValue);
}

#[test]
fn external_capability_policy_must_accept_exact_planned_model_effort_and_mode() {
    let mut input = plan();
    input.tasks[0].execution.as_mut().unwrap().model = "unknown/model".into();
    has(&input, DiagnosticCode::UnknownModel);
    let mut input = plan();
    input.tasks[0].execution.as_mut().unwrap().effort = Effort::High;
    has(&input, DiagnosticCode::UnsupportedEffort);
    let mut input = plan();
    input.tasks[0].execution.as_mut().unwrap().access_mode = AccessMode::CliSession;
    has(&input, DiagnosticCode::UnauthorizedAccessMode);
    let mut input = plan();
    input.tasks[0].execution.as_mut().unwrap().connection_ref = "other-connection".into();
    has(&input, DiagnosticCode::UnauthorizedAccessMode);
}

#[test]
fn artifacts_require_known_direct_predecessor_and_unique_name_digest() {
    let mut input = plan();
    let a = task("a", &input.key);
    let artifact = ArtifactExpectation {
        predecessor: task_ref(&a),
        name: "result".into(),
        sha256: "2".repeat(64),
    };
    input.tasks[0].upstream_artifacts.push(artifact.clone());
    input.tasks.push(a.clone());
    has(&input, DiagnosticCode::InvalidArtifact);
    input.tasks[0].dependencies.push(task_ref(&a));
    assert!(validate(&input, &SyntheticPolicy, Limits::default()).is_ok());
    input.tasks[0].upstream_artifacts.push(artifact);
    has(&input, DiagnosticCode::DuplicateArtifact);
    input.tasks[0].upstream_artifacts.pop();
    input.tasks[0].upstream_artifacts[0].sha256.clear();
    has(&input, DiagnosticCode::InvalidArtifact);
}

#[test]
fn currencies_cannot_silently_mix_across_tasks() {
    use rolvya_domain::money::*;
    let mut input = plan();
    let mut b = task("b", &input.key);
    let euro = Money::new(0, Currency::new("EUR").unwrap()).unwrap();
    let breakdown = CostBreakdown {
        ai_api: euro,
        compute: euro,
        storage: euro,
        transfer: euro,
    };
    b.execution.as_mut().unwrap().costs = CostScenarios {
        optimistic: breakdown,
        expected: breakdown,
        pessimistic: breakdown,
    };
    input.tasks.push(b);
    has(&input, DiagnosticCode::InvalidCosts);
}

#[test]
fn diagnostics_remain_stable_under_invalid_input_and_edge_permutation() {
    let mut input = plan();
    let mut a = task("a", &input.key);
    let mut b = task("b", &input.key);
    a.parent = Some(task_ref(&task("unknown-parent", &input.key)));
    b.acceptance_criteria.clear();
    a.dependencies = vec![
        task_ref(&task("unknown-a", &input.key)),
        task_ref(&task("unknown-z", &input.key)),
    ];
    input.tasks = vec![b, a];
    let expected = errors(&input);
    input.tasks.reverse();
    for task in &mut input.tasks {
        task.dependencies.reverse();
    }
    assert_eq!(errors(&input), expected);
}

#[test]
fn unplanned_overlapping_remote_writes_require_dependency_sequencing() {
    let mut input = plan();
    let mut b = task("b", &input.key);
    write(&mut input.tasks[0], "refs/heads/isolated", &["src"]);
    write(&mut b, "refs/heads/isolated", &["src/lib.rs"]);
    input.tasks.push(b);
    has(&input, DiagnosticCode::WriteConflict);
    let reference = task_ref(&input.tasks[0]);
    input.tasks[1].dependencies.push(reference);
    assert!(validate(&input, &SyntheticPolicy, Limits::default()).is_ok());
    input.tasks[1].dependencies.clear();
    write(&mut input.tasks[1], "refs/heads/other", &["src/lib.rs"]);
    assert!(validate(&input, &SyntheticPolicy, Limits::default()).is_ok());
    write(
        &mut input.tasks[1],
        "refs/heads/isolated",
        &["src-other/lib.rs"],
    );
    assert!(
        validate(&input, &SyntheticPolicy, Limits::default()).is_ok(),
        "prefix needs segment boundary"
    );
}

#[test]
fn transitive_dependencies_and_verified_group_completion_serialize_writes() {
    let mut input = plan();
    let mut middle = task("middle", &input.key);
    let mut last = task("last", &input.key);
    middle.dependencies.push(task_ref(&input.tasks[0]));
    last.dependencies.push(task_ref(&middle));
    write(&mut input.tasks[0], "refs/heads/isolated", &["src"]);
    write(&mut last, "refs/heads/isolated", &["src"]);
    input.tasks.extend([middle, last]);
    assert!(validate(&input, &SyntheticPolicy, Limits::default()).is_ok());
    let g = group("group", &input.key);
    input.tasks[0].parent = Some(task_ref(&g));
    input.tasks[2].dependencies = vec![task_ref(&g)];
    input.tasks.push(g);
    assert!(validate(&input, &SyntheticPolicy, Limits::default()).is_ok());
}

#[test]
fn malformed_remote_targets_and_aliases_do_not_hide_write_conflicts() {
    for target in [
        "main",
        "refs/heads/../main",
        "refs/heads/.hidden",
        "refs/heads/a.lock",
        "refs/heads/a..b",
        "refs/heads/a.",
        "refs/heads/a b",
    ] {
        let mut input = plan();
        write(&mut input.tasks[0], target, &["src"]);
        has(&input, DiagnosticCode::InvalidWriteScope);
    }
    for value in [
        "../src",
        "/src",
        "src//lib.rs",
        "src/",
        "src/./file.rs",
        "src\\file.rs",
    ] {
        let mut input = plan();
        write(&mut input.tasks[0], "refs/heads/isolated", &[value]);
        has(&input, DiagnosticCode::InvalidWriteScope);
    }
    let mut input = plan();
    write(&mut input.tasks[0], "refs/heads/isolated", &["src"]);
    if let WriteScope::Remote { repository, .. } =
        &mut input.tasks[0].execution.as_mut().unwrap().write_scope
    {
        *repository = "Synthetic/repo".into();
    }
    has(&input, DiagnosticCode::InvalidWriteScope);
}

#[test]
fn resource_limits_bound_collections_text_and_conflict_search() {
    let input = plan();
    let limits = Limits {
        max_tasks: 0,
        ..Limits::default()
    };
    assert_eq!(
        validate(&input, &SyntheticPolicy, limits).unwrap_err()[0].code,
        DiagnosticCode::ResourceLimit
    );
    let limits = Limits {
        max_text_bytes: 1,
        ..Limits::default()
    };
    assert_eq!(
        validate(&input, &SyntheticPolicy, limits).unwrap_err()[0].code,
        DiagnosticCode::ResourceLimit
    );
    let mut input = plan();
    input.tasks[0].scope = vec![String::new(); 11];
    let limits = Limits {
        max_edges: 10,
        ..Limits::default()
    };
    assert_eq!(
        validate(&input, &SyntheticPolicy, limits).unwrap_err()[0].code,
        DiagnosticCode::ResourceLimit
    );
    let mut input = plan();
    let mut b = task("b", &input.key);
    write(&mut input.tasks[0], "refs/heads/isolated", &["src"]);
    write(&mut b, "refs/heads/isolated", &["src"]);
    input.tasks.push(b);
    let limits = Limits {
        max_conflict_work: 0,
        ..Limits::default()
    };
    assert_eq!(
        validate(&input, &SyntheticPolicy, limits).unwrap_err()[0].code,
        DiagnosticCode::ResourceLimit
    );
}

#[test]
fn deep_hierarchy_and_large_dag_use_iterative_bounded_algorithms() {
    let mut input = plan();
    input.tasks.clear();
    for n in 0..10_000 {
        let mut node = if n == 9_999 {
            task(&format!("node-{n:05}"), &input.key)
        } else {
            group(&format!("node-{n:05}"), &input.key)
        };
        if n > 0 {
            node.parent = Some(TaskRef {
                plan: input.key.clone(),
                task_id: rolvya_domain::ids::TaskId::new(format!("node-{:05}", n - 1)).unwrap(),
            });
        }
        input.tasks.push(node);
    }
    let started = std::time::Instant::now();
    assert!(validate(&input, &SyntheticPolicy, Limits::default()).is_ok());
    eprintln!(
        "10,000-node deep hierarchy validate+canonicalize+hash: {:?}",
        started.elapsed()
    );
    input.tasks.clear();
    for n in 0..5_000 {
        let mut node = task(&format!("node-{n:05}"), &input.key);
        if n > 0 {
            node.dependencies.push(TaskRef {
                plan: input.key.clone(),
                task_id: rolvya_domain::ids::TaskId::new(format!("node-{:05}", n - 1)).unwrap(),
            });
        }
        input.tasks.push(node);
    }
    let started = std::time::Instant::now();
    assert!(validate(&input, &SyntheticPolicy, Limits::default()).is_ok());
    eprintln!(
        "5,000-node dependency DAG validate+canonicalize+hash: {:?}",
        started.elapsed()
    );
}

#[test]
fn seeded_dags_preserve_order_and_injected_back_edges_are_rejected() {
    let mut seed = 0x5eed_u64;
    for _ in 0..24 {
        let mut input = plan();
        input.tasks = (0..48)
            .map(|n| task(&format!("node-{n:03}"), &input.key))
            .collect();
        for n in 1..input.tasks.len() {
            let reference = task_ref(&input.tasks[n - 1]);
            input.tasks[n].dependencies.push(reference);
            for predecessor in 0..n.saturating_sub(1) {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                if seed >> 60 == 0 {
                    let reference = task_ref(&input.tasks[predecessor]);
                    input.tasks[n].dependencies.push(reference);
                }
            }
        }
        let expected = validate(&input, &SyntheticPolicy, Limits::default())
            .unwrap()
            .fingerprint();
        for i in (1..input.tasks.len()).rev() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            input.tasks.swap(i, (seed as usize) % (i + 1));
        }
        for node in &mut input.tasks {
            node.dependencies.reverse();
        }
        assert_eq!(
            validate(&input, &SyntheticPolicy, Limits::default())
                .unwrap()
                .fingerprint(),
            expected
        );
        let last = input
            .tasks
            .iter()
            .find(|t| t.id.as_str() == "node-047")
            .unwrap()
            .clone();
        let first = input
            .tasks
            .iter_mut()
            .find(|t| t.id.as_str() == "node-000")
            .unwrap();
        first.dependencies.push(task_ref(&last));
        has(&input, DiagnosticCode::DependencyCycle);
    }
}

#[test]
fn punctuation_only_assignments_are_structurally_invalid_even_before_policy() {
    let mut input = plan();
    input.tasks[0].execution.as_mut().unwrap().model = "/".into();
    has(&input, DiagnosticCode::InvalidAssignment);
}

#[test]
fn exhausted_budget_diagnostics_are_independent_of_task_order() {
    let mut input = plan();
    input.tasks[0].goal = "x".repeat(50);
    let mut other = task("other", &input.key);
    other.scope = (0..20).map(|n| format!("path-{n}")).collect();
    input.tasks.push(other);
    let limits = Limits {
        max_edges: 10,
        max_text_bytes: 20,
        ..Limits::default()
    };
    let expected = validate(&input, &SyntheticPolicy, limits).unwrap_err();
    input.tasks.reverse();
    assert_eq!(
        expected,
        validate(&input, &SyntheticPolicy, limits).unwrap_err()
    );
}
