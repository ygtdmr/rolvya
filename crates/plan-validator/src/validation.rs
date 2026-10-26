//! Bounded, deterministic, side-effect-free validation of draft plan content.
use rolvya_domain::{ids::TaskId, plan::*};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// External evidence belongs to a trusted caller; no model entitlement is
/// inferred by the graph validator and there is no permissive default policy.
/// Implementations must be bounded and side-effect-free over previously verified
/// capability evidence. Fetch external evidence before calling validation.
pub trait AssignmentPolicy {
    fn check(&self, assignment: &ExecutionAssignment) -> Result<(), AssignmentDenial>;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentDenial {
    UnknownModel,
    UnsupportedEffort,
    UnauthorizedAccessMode,
}

/// Validation work limits, not product nesting limits. Callers must also bound
/// input decoding; this API receives an already allocated typed plan.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_tasks: usize,
    pub max_edges: usize,
    pub max_text_bytes: usize,
    pub max_conflict_work: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_tasks: 10_000,
            max_edges: 100_000,
            max_text_bytes: 32 * 1024 * 1024,
            max_conflict_work: 1_000_000,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    EmptyPlan,
    UnsupportedSchema,
    ResourceLimit,
    DuplicateTask,
    CrossPlanReference,
    MissingParent,
    SelfParent,
    ParentCycle,
    MissingDependency,
    SelfDependency,
    DuplicateDependency,
    DependencyCycle,
    CompletionDeadlock,
    ExecutableParent,
    EmptyGroup,
    GroupExecution,
    MissingAssignment,
    UnsupportedOptionalChild,
    EmptyField,
    InvalidContext,
    InvalidResources,
    InvalidCosts,
    InvalidAssignment,
    UnknownModel,
    UnsupportedEffort,
    UnauthorizedAccessMode,
    InvalidRootCount,
    InvalidWriteScope,
    WriteConflict,
    InvalidArtifact,
    DuplicateArtifact,
    DuplicateValue,
    CanonicalEncoding,
}
impl DiagnosticCode {
    pub fn message(self) -> &'static str {
        match self {
            Self::EmptyPlan => "a plan needs at least one task",
            Self::UnsupportedSchema => "unsupported plan schema version",
            Self::ResourceLimit => "validation exceeds the configured resource limit",
            Self::DuplicateTask => "task identifiers must be unique",
            Self::CrossPlanReference => "reference belongs to another project, plan or version",
            Self::MissingParent => "parent task does not exist",
            Self::SelfParent => "a task cannot be its own parent",
            Self::ParentCycle => "task participates in a hierarchy cycle",
            Self::MissingDependency => "predecessor task does not exist",
            Self::SelfDependency => "a task cannot depend on itself",
            Self::DuplicateDependency => "predecessor is declared more than once",
            Self::DependencyCycle => "task participates in an explicit dependency cycle",
            Self::CompletionDeadlock => {
                "required group completion and dependencies form a deadlock"
            }
            Self::ExecutableParent => "an executable task cannot have children",
            Self::EmptyGroup => "a group must have required children",
            Self::GroupExecution => "a group cannot have an execution assignment",
            Self::MissingAssignment => "an executable task requires a planned assignment",
            Self::UnsupportedOptionalChild => {
                "optional-child semantics are unsupported in schema v1"
            }
            Self::EmptyField => "required planned content is empty or contains control characters",
            Self::InvalidContext => "context requires a reference and lowercase SHA-256 digest",
            Self::InvalidResources => "execution resources and timeout must be positive",
            Self::InvalidCosts => "cost estimates violate currency, amount or scenario invariants",
            Self::InvalidAssignment => "model or connection reference has invalid syntax",
            Self::UnknownModel => "trusted policy does not recognize the planned model",
            Self::UnsupportedEffort => "trusted policy rejects the planned effort",
            Self::UnauthorizedAccessMode => {
                "trusted policy rejects the planned connection or access mode"
            }
            Self::InvalidRootCount => "single-root plans must have exactly one root",
            Self::InvalidWriteScope => {
                "remote write scope must have canonical repository, ref and relative paths"
            }
            Self::WriteConflict => "overlapping remote writes lack explicit dependency sequencing",
            Self::InvalidArtifact => {
                "artifact expectation must name a direct predecessor and valid digest"
            }
            Self::DuplicateArtifact => "artifact expectation is duplicated",
            Self::DuplicateValue => "set-like planned content contains duplicate values",
            Self::CanonicalEncoding => "canonical plan encoding failed",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub task: Option<TaskId>,
    pub related: Option<TaskId>,
    pub field: &'static str,
}

fn issue(
    code: DiagnosticCode,
    task: Option<&TaskId>,
    related: Option<&TaskId>,
    field: &'static str,
) -> Diagnostic {
    Diagnostic {
        code,
        task: task.cloned(),
        related: related.cloned(),
        field,
    }
}
fn sorted_errors(mut errors: Vec<Diagnostic>) -> Vec<Diagnostic> {
    errors.sort();
    errors.dedup();
    errors
}
fn nonempty(value: &str) -> bool {
    !value.trim().is_empty()
        && !value
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
}
fn token(value: &str) -> bool {
    value
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._/-".contains(&b))
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}
fn repository(value: &str) -> bool {
    let parts: Vec<_> = value.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && *part != "."
                && *part != ".."
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
        })
}
fn git_ref(value: &str) -> bool {
    value.strip_prefix("refs/heads/").is_some_and(|tail| {
        path(tail)
            && !tail.contains("..")
            && !tail.ends_with('.')
            && tail
                .split('/')
                .all(|part| !part.starts_with('.') && !part.ends_with(".lock"))
    })
}
fn list_checks(
    values: &[String],
    required: bool,
    task: &TaskId,
    field: &'static str,
    errors: &mut Vec<Diagnostic>,
) {
    if (required && values.is_empty()) || values.iter().any(|value| !nonempty(value)) {
        errors.push(issue(DiagnosticCode::EmptyField, Some(task), None, field));
    }
    if values.iter().collect::<BTreeSet<_>>().len() != values.len() {
        errors.push(issue(
            DiagnosticCode::DuplicateValue,
            Some(task),
            None,
            field,
        ));
    }
}

// Preflight bounds allocations by tasks/relationships/collection entries and
// total user-controlled text. Saturating arithmetic makes hostile lengths fail.
// Budget diagnostics use plan-level context: early exit must not let task input
// order select a different error when multiple budgets are exhausted.
fn preflight(plan: &PlanContent, limits: Limits) -> Result<(), Vec<Diagnostic>> {
    if plan.tasks.len() > limits.max_tasks {
        return Err(vec![issue(
            DiagnosticCode::ResourceLimit,
            None,
            None,
            "tasks",
        )]);
    }
    let mut entries = 0usize;
    let mut bytes = 0usize;
    let mut add = |text: &str| -> Result<(), Vec<Diagnostic>> {
        bytes = bytes.saturating_add(text.len());
        if bytes > limits.max_text_bytes {
            Err(vec![issue(
                DiagnosticCode::ResourceLimit,
                None,
                None,
                "plan",
            )])
        } else {
            Ok(())
        }
    };
    for task in &plan.tasks {
        entries = entries
            .saturating_add(task.dependencies.len())
            .saturating_add(usize::from(task.parent.is_some()))
            .saturating_add(task.upstream_artifacts.len())
            .saturating_add(task.scope.len())
            .saturating_add(task.exclusions.len())
            .saturating_add(task.acceptance_criteria.len());
        if entries > limits.max_edges {
            return Err(vec![issue(
                DiagnosticCode::ResourceLimit,
                None,
                None,
                "plan",
            )]);
        }
        for value in [
            &task.goal,
            &task.instructions,
            &task.context.reference,
            &task.context.sha256,
        ] {
            add(value)?;
        }
        for value in task
            .scope
            .iter()
            .chain(&task.exclusions)
            .chain(&task.acceptance_criteria)
        {
            add(value)?;
        }
        for artifact in &task.upstream_artifacts {
            add(&artifact.name)?;
            add(&artifact.sha256)?;
        }
        if let Some(assignment) = &task.execution {
            add(&assignment.model)?;
            add(&assignment.connection_ref)?;
            entries = entries.saturating_add(assignment.policy_refs.len());
            if entries > limits.max_edges {
                return Err(vec![issue(
                    DiagnosticCode::ResourceLimit,
                    None,
                    None,
                    "plan",
                )]);
            }
            for value in &assignment.policy_refs {
                add(value)?;
            }
            if let WriteScope::Remote {
                repository,
                git_ref,
                paths,
            } = &assignment.write_scope
            {
                add(repository)?;
                add(git_ref)?;
                entries = entries.saturating_add(paths.len());
                if entries > limits.max_edges {
                    return Err(vec![issue(
                        DiagnosticCode::ResourceLimit,
                        None,
                        None,
                        "plan",
                    )]);
                }
                for value in paths {
                    add(value)?;
                }
            }
        }
    }
    Ok(())
}

pub fn check(
    plan: &PlanContent,
    policy: &impl AssignmentPolicy,
    limits: Limits,
) -> Result<Vec<TaskId>, Vec<Diagnostic>> {
    preflight(plan, limits)?;
    let mut errors = Vec::new();
    if plan.schema_version != 1 {
        errors.push(issue(
            DiagnosticCode::UnsupportedSchema,
            None,
            None,
            "schema_version",
        ));
    }
    if plan.tasks.is_empty() {
        errors.push(issue(DiagnosticCode::EmptyPlan, None, None, "tasks"));
    }
    let mut counts = BTreeMap::new();
    for task in &plan.tasks {
        *counts.entry(&task.id).or_insert(0usize) += 1;
    }
    for (id, count) in &counts {
        if *count > 1 {
            errors.push(issue(DiagnosticCode::DuplicateTask, Some(id), None, "id"));
        }
    }
    // Never select an arbitrary representative of conflicting duplicate records.
    if !errors.is_empty() {
        return Err(sorted_errors(errors));
    }
    let mut tasks: Vec<_> = plan.tasks.iter().collect();
    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    let ids: BTreeMap<_, _> = tasks
        .iter()
        .enumerate()
        .map(|(index, task)| (&task.id, index))
        .collect();
    let mut children = vec![Vec::new(); tasks.len()];
    let mut dependencies = vec![Vec::new(); tasks.len()];
    let mut roots = 0;
    let mut plan_currency = None;
    for (index, task) in tasks.iter().enumerate() {
        let id = Some(&task.id);
        if task.plan != plan.key {
            errors.push(issue(DiagnosticCode::CrossPlanReference, id, None, "plan"));
        }
        if !task.required {
            errors.push(issue(
                DiagnosticCode::UnsupportedOptionalChild,
                id,
                None,
                "required",
            ));
        }
        if !nonempty(&task.goal) {
            errors.push(issue(DiagnosticCode::EmptyField, id, None, "goal"));
        }
        if !nonempty(&task.instructions) {
            errors.push(issue(DiagnosticCode::EmptyField, id, None, "instructions"));
        }
        list_checks(&task.scope, true, &task.id, "scope", &mut errors);
        list_checks(&task.exclusions, false, &task.id, "exclusions", &mut errors);
        list_checks(
            &task.acceptance_criteria,
            true,
            &task.id,
            "acceptance_criteria",
            &mut errors,
        );
        if !nonempty(&task.context.reference) || !digest(&task.context.sha256) {
            errors.push(issue(DiagnosticCode::InvalidContext, id, None, "context"));
        }
        if let Some(parent) = &task.parent {
            if parent.plan != plan.key {
                errors.push(issue(
                    DiagnosticCode::CrossPlanReference,
                    id,
                    Some(&parent.task_id),
                    "parent",
                ));
            } else if parent.task_id == task.id {
                errors.push(issue(DiagnosticCode::SelfParent, id, id, "parent"));
            } else if let Some(&parent_index) = ids.get(&parent.task_id) {
                children[parent_index].push(index);
            } else {
                errors.push(issue(
                    DiagnosticCode::MissingParent,
                    id,
                    Some(&parent.task_id),
                    "parent",
                ));
            }
        } else {
            roots += 1;
        }
        let mut seen_dependencies = BTreeSet::new();
        for predecessor in &task.dependencies {
            if !seen_dependencies.insert(predecessor) {
                errors.push(issue(
                    DiagnosticCode::DuplicateDependency,
                    id,
                    Some(&predecessor.task_id),
                    "dependencies",
                ));
            }
            if predecessor.plan != plan.key {
                errors.push(issue(
                    DiagnosticCode::CrossPlanReference,
                    id,
                    Some(&predecessor.task_id),
                    "dependencies",
                ));
            } else if predecessor.task_id == task.id {
                errors.push(issue(
                    DiagnosticCode::SelfDependency,
                    id,
                    id,
                    "dependencies",
                ));
            } else if let Some(&predecessor_index) = ids.get(&predecessor.task_id) {
                dependencies[predecessor_index].push(index);
            } else {
                errors.push(issue(
                    DiagnosticCode::MissingDependency,
                    id,
                    Some(&predecessor.task_id),
                    "dependencies",
                ));
            }
        }
        let mut seen_artifacts = BTreeSet::new();
        for artifact in &task.upstream_artifacts {
            if !seen_artifacts.insert((&artifact.predecessor, &artifact.name)) {
                errors.push(issue(
                    DiagnosticCode::DuplicateArtifact,
                    id,
                    Some(&artifact.predecessor.task_id),
                    "upstream_artifacts",
                ));
            }
            if artifact.predecessor.plan != plan.key
                || !seen_dependencies.contains(&artifact.predecessor)
                || !nonempty(&artifact.name)
                || !digest(&artifact.sha256)
            {
                errors.push(issue(
                    DiagnosticCode::InvalidArtifact,
                    id,
                    Some(&artifact.predecessor.task_id),
                    "upstream_artifacts",
                ));
            }
        }
        match (task.kind, &task.execution) {
            (TaskKind::Group, Some(_)) => {
                errors.push(issue(DiagnosticCode::GroupExecution, id, None, "execution"))
            }
            (TaskKind::ExecutableLeaf, None) => errors.push(issue(
                DiagnosticCode::MissingAssignment,
                id,
                None,
                "execution",
            )),
            (TaskKind::ExecutableLeaf, Some(assignment)) => {
                if !token(&assignment.model) || !token(&assignment.connection_ref) {
                    errors.push(issue(
                        DiagnosticCode::InvalidAssignment,
                        id,
                        None,
                        "execution",
                    ));
                }
                list_checks(
                    &assignment.policy_refs,
                    true,
                    &task.id,
                    "policy_refs",
                    &mut errors,
                );
                let resources = &assignment.resources;
                if resources.cpu_millis == 0
                    || resources.memory_mib == 0
                    || resources.timeout_seconds == 0
                {
                    errors.push(issue(
                        DiagnosticCode::InvalidResources,
                        id,
                        None,
                        "resources",
                    ));
                }
                if assignment.costs.validate().is_err() {
                    errors.push(issue(DiagnosticCode::InvalidCosts, id, None, "costs"));
                }
                let currency = assignment.costs.optimistic.ai_api.currency();
                if plan_currency.is_some_and(|previous| previous != currency) {
                    errors.push(issue(
                        DiagnosticCode::InvalidCosts,
                        id,
                        None,
                        "costs.currency",
                    ));
                }
                plan_currency.get_or_insert(currency);
                if let Err(denial) = policy.check(assignment) {
                    let code = match denial {
                        AssignmentDenial::UnknownModel => DiagnosticCode::UnknownModel,
                        AssignmentDenial::UnsupportedEffort => DiagnosticCode::UnsupportedEffort,
                        AssignmentDenial::UnauthorizedAccessMode => {
                            DiagnosticCode::UnauthorizedAccessMode
                        }
                    };
                    errors.push(issue(code, id, None, "execution"));
                }
                if let WriteScope::Remote {
                    repository: repo,
                    git_ref: target,
                    paths,
                } = &assignment.write_scope
                {
                    if !repository(repo)
                        || !git_ref(target)
                        || paths.is_empty()
                        || !paths.iter().all(|value| path(value))
                    {
                        errors.push(issue(
                            DiagnosticCode::InvalidWriteScope,
                            id,
                            None,
                            "write_scope",
                        ));
                    }
                    list_checks(paths, true, &task.id, "write_scope.paths", &mut errors);
                }
            }
            _ => {}
        }
    }
    for (index, task) in tasks.iter().enumerate() {
        if task.kind == TaskKind::ExecutableLeaf && !children[index].is_empty() {
            errors.push(issue(
                DiagnosticCode::ExecutableParent,
                Some(&task.id),
                None,
                "kind",
            ));
        }
        if task.kind == TaskKind::Group && children[index].is_empty() {
            errors.push(issue(
                DiagnosticCode::EmptyGroup,
                Some(&task.id),
                None,
                "kind",
            ));
        }
    }
    if plan.root_policy == RootPolicy::SingleRoot && roots != 1 {
        errors.push(issue(
            DiagnosticCode::InvalidRootCount,
            None,
            None,
            "root_policy",
        ));
    }
    if !errors.is_empty() {
        return Err(sorted_errors(errors));
    }
    for graph in [&mut dependencies, &mut children] {
        for edges in graph {
            edges.sort_unstable();
            edges.dedup();
        }
    }
    for index in cycle_members(&children) {
        errors.push(issue(
            DiagnosticCode::ParentCycle,
            Some(&tasks[index].id),
            None,
            "parent",
        ));
    }
    let explicit_order = topological(&dependencies);
    if explicit_order.len() != tasks.len() {
        for index in cycle_members(&dependencies) {
            errors.push(issue(
                DiagnosticCode::DependencyCycle,
                Some(&tasks[index].id),
                None,
                "dependencies",
            ));
        }
    }
    if !errors.is_empty() {
        return Err(sorted_errors(errors));
    }
    // Child completion precedes group completion, solely for deadlock analysis.
    // This never becomes an implicit dependency in the returned plan order.
    let mut completion = dependencies.clone();
    for (parent, descendants) in children.iter().enumerate() {
        for &child in descendants {
            completion[child].push(parent);
        }
    }
    for edges in &mut completion {
        edges.sort_unstable();
        edges.dedup();
    }
    for index in cycle_members(&completion) {
        errors.push(issue(
            DiagnosticCode::CompletionDeadlock,
            Some(&tasks[index].id),
            None,
            "completion",
        ));
    }
    if !errors.is_empty() {
        return Err(sorted_errors(errors));
    }
    write_conflicts(&tasks, &completion, limits.max_conflict_work, &mut errors);
    if !errors.is_empty() {
        return Err(sorted_errors(errors));
    }
    Ok(explicit_order
        .into_iter()
        .map(|index| tasks[index].id.clone())
        .collect())
}

fn topological(graph: &[Vec<usize>]) -> Vec<usize> {
    let mut indegree = vec![0usize; graph.len()];
    for edges in graph {
        for &next in edges {
            indegree[next] += 1;
        }
    }
    let mut ready: BTreeSet<_> = indegree
        .iter()
        .enumerate()
        .filter_map(|(i, &degree)| (degree == 0).then_some(i))
        .collect();
    let mut order = Vec::with_capacity(graph.len());
    while let Some(node) = ready.pop_first() {
        order.push(node);
        for &next in &graph[node] {
            indegree[next] -= 1;
            if indegree[next] == 0 {
                ready.insert(next);
            }
        }
    }
    order
}

// Iterative Kosaraju: identify actual strongly connected cycle members rather
// than incorrectly labeling every downstream blocked task as part of a cycle.
fn cycle_members(graph: &[Vec<usize>]) -> Vec<usize> {
    let mut visited = vec![false; graph.len()];
    let mut finish = Vec::with_capacity(graph.len());
    for root in 0..graph.len() {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut stack = vec![(root, 0usize)];
        while let Some((node, cursor)) = stack.last_mut() {
            if *cursor < graph[*node].len() {
                let next = graph[*node][*cursor];
                *cursor += 1;
                if !visited[next] {
                    visited[next] = true;
                    stack.push((next, 0));
                }
            } else {
                let (node, _) = stack.pop().unwrap_or((root, 0));
                finish.push(node);
            }
        }
    }
    let mut reverse = vec![Vec::new(); graph.len()];
    for (node, edges) in graph.iter().enumerate() {
        for &next in edges {
            reverse[next].push(node);
        }
    }
    visited.fill(false);
    let mut cycles = Vec::new();
    for &root in finish.iter().rev() {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut component = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            component.push(node);
            for &next in &reverse[node] {
                if !visited[next] {
                    visited[next] = true;
                    stack.push(next);
                }
            }
        }
        if component.len() > 1 || graph[root].contains(&root) {
            cycles.extend(component);
        }
    }
    cycles.sort_unstable();
    cycles
}

fn overlaps(a: &str, b: &str) -> bool {
    a == b
        || a.strip_prefix(b).is_some_and(|tail| tail.starts_with('/'))
        || b.strip_prefix(a).is_some_and(|tail| tail.starts_with('/'))
}
fn spend(work: &mut usize) -> Result<(), ()> {
    if *work == 0 {
        Err(())
    } else {
        *work -= 1;
        Ok(())
    }
}
fn reachable(
    graph: &[Vec<usize>],
    start: usize,
    target: usize,
    work: &mut usize,
) -> Result<bool, ()> {
    *work = work.checked_sub(graph.len()).ok_or(())?;
    let mut visited = vec![false; graph.len()];
    visited[start] = true;
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        spend(work)?;
        for &next in &graph[node] {
            spend(work)?;
            if next == target {
                return Ok(true);
            }
            if !visited[next] {
                visited[next] = true;
                stack.push(next);
            }
        }
    }
    Ok(false)
}
struct Writer<'a> {
    index: usize,
    paths: &'a [String],
}
fn write_conflicts(
    tasks: &[&PlannedTask],
    graph: &[Vec<usize>],
    mut work: usize,
    errors: &mut Vec<Diagnostic>,
) {
    let mut targets: BTreeMap<(&str, &str), Vec<Writer<'_>>> = BTreeMap::new();
    for (index, task) in tasks.iter().enumerate() {
        if let Some(ExecutionAssignment {
            write_scope:
                WriteScope::Remote {
                    repository,
                    git_ref,
                    paths,
                },
            ..
        }) = &task.execution
        {
            targets
                .entry((repository, git_ref))
                .or_default()
                .push(Writer { index, paths });
        }
    }
    for writers in targets.values() {
        for (offset, writer) in writers.iter().enumerate() {
            let (left, paths) = (writer.index, writer.paths);
            for other in &writers[offset + 1..] {
                let (right, other_paths) = (other.index, other.paths);
                let mut check = || -> Result<bool, ()> {
                    spend(&mut work)?;
                    let mut overlap = false;
                    for a in paths {
                        for b in other_paths {
                            spend(&mut work)?;
                            overlap |= overlaps(a, b);
                        }
                    }
                    if !overlap {
                        return Ok(false);
                    }
                    Ok(!reachable(graph, left, right, &mut work)?
                        && !reachable(graph, right, left, &mut work)?)
                };
                match check() {
                    Ok(true) => errors.push(issue(
                        DiagnosticCode::WriteConflict,
                        Some(&tasks[left].id),
                        Some(&tasks[right].id),
                        "write_scope",
                    )),
                    Ok(false) => {}
                    Err(()) => {
                        errors.push(issue(
                            DiagnosticCode::ResourceLimit,
                            None,
                            None,
                            "conflict_work",
                        ));
                        return;
                    }
                }
            }
        }
    }
}
