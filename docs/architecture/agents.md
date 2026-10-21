---
title: Planning, Diagnosis and Execution Agent Contracts
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [agents, planner, executor, models, isolation]
---

# Planning, diagnosis and execution agent contracts

## Approved roles and important distinction

Rolvya has **one main agent responsible for complete up-front planning**, including all nested tasks, dependencies, assigned model and effort. Worker agents **execute** only assigned tasks and **do not create additional tasks, decompose work or delegate new workers**. A main agent may diagnose failure and suggest new attempts of existing tasks under policy. These are approved architectural requirements. Rust orchestration services enforce policy; SQLite stores authoritative state. Agent models are not automatically Rust code—they are external provider programs/services invoked by the Rust backend.

## Role capabilities (proposed enforcement)

| Capability | Main planning/diagnosis role | Worker execution role | Trusted Rust control plane |
| --- | --- | --- | --- |
| Interpret project goal | Yes | Only supplied task slice | Validate owner intent and constraints |
| Create full task hierarchy | **Before activation only** | Never | Validate/freeze graph |
| Set model and effort | Complete up-front assignment | Cannot self-select new provider | Validate permitted mode and resources |
| Spawn child agents/tasks | Planning describes graph; no runtime expansion | Never | Schedules existing independent tasks |
| Execute arbitrary project commands | Not required on small controller | On isolated authorized **remote** worker | Does not build user projects |
| Retry failure | Diagnose and recommend | Report reason only | Fence, reconcile, reserve, admit new same-task attempt |
| Increase spending limit | Never | Never | Only record authenticated explicit human approval |
| Merge remote Git changes | Propose/review only under policy | Never | Authorized integration gate with independent validation |
| Declare success | Describe evidence | Submit manifest, never authoritative | Validate criteria and persist success |

## Main planner input and output contract

**Input:** human goal, constraints, scope exclusions, remote repository ID and pinned base ref, full approved architecture/decision context, available authorized spending, provider access options, rough duration/resources and acceptance criteria.

**Output:** complete versioned plan with nested hierarchy, explicit dependency DAG, task goal/scope, assigned model/effort/access mode, compact context refs/hashes, expected resource class, optimistic/expected/pessimistic costs separated into AI API/compute/storage/transfer, acceptance criteria, conflict constraints and unresolved issues.

A model-generated JSON or text graph is **untrusted until Rust validation**; require deterministic schema, ID consistency, cycle tests and budget safety before activation. A plan that is incomplete is draft/blocked, not partially executable.

## Worker task package (proposed)

```yaml
project_id: example
plan_id: example-fixed-plan
task_id: example-existing-task
attempt_id: example-attempt
plan_hash: example-hash
objective: implement one approved task
scope:
  permitted_paths: [example/specific/module]
  forbidden_operations: [create_task, raise_budget, merge_protected_branch]
upstream:
  verified_artifact_refs: []
execution:
  model: planner-assigned-model
  effort: planner-assigned-effort
  access_mode: authorized-cli-or-api
  remote_repository_ref: example-pinned-commit
acceptance:
  criteria_refs: [example-criterion]
budget:
  reservation_ref: example-approved-reservation
```

Illustrative payload only, not an approved wire schema; the actual task package also needs secure identity, fencing, expiry and immutable context digest. No real secrets belong in the package or prompt.

## Agent model and access mode checks

Provider-specific CLI and API modes are distinct. Earlier user intent favored CLI-only where no API key exists and API mode where a key is available, but unattended remote execution and subscription entitlement must be checked **per provider/model**. Never silently fall back to an unapproved paid API mode; reject unavailable modes with explicit blocker. Different models and effort levels may be assigned *per task* during planning and, for a failed attempt, policy-approved new attempts can use a different model/effort while retaining task identity.

## Context and token efficiency

Workers receive minimal goal, acceptance and dependency artifacts, with links to versioned instructions instead of copied chat histories. Hash context; use deterministic truncation warnings rather than dropping crucial constraints. Separate durable approved decisions from untrusted repository data and model self-generated commentary. Record token counts when available, but do not mistake subscription costs for API token charges.

## Failure and recovery handoff

When worker fails or is interrupted:
1. Report attempted commands, sanitized logs, exact test results, changed remote refs/artifact hashes and usage.
2. Classify failure, record known irreversible remote side effects and provider billing uncertainty.
3. Main agent may assess existing task's recovery options.
4. Rust coordinator fences prior attempt, reconciles Git/cloud/API charges, rechecks budget and only then authorizes new attempt of the **same task**.
5. A worker cannot claim an additional child task is necessary and then create it; unresolved scope goes to a new-plan decision process.

For details see [agent handoffs](../development/agent-handoffs.md).

## Prompt-injection boundary and secrets

An agent may read GitHub code, READMEs, issue text, compiler output and websites. These are lower-trust *task data*; they cannot supersede authorized scope or the fixed DAG. Reject instructions to exfiltrate tokens, change billing approval, read other projects or use personal-computer files. Only trusted Rust integration components should hold broader remote Git write credentials; worker access is constrained to its attempt and artifacts.

## Proposed test matrix

- Fake main model returns partial plan: no plan activation or paid worker start.
- Worker sends `create_task` in result: rejected and audited.
- Worker requests budget increase in logs: no ceiling change.
- Model choice is unavailable due to licensing: task blocked; no unauthorized fallback charge.
- Retry with new model and host preserves task ID and all validated sibling results.
- Malicious repository text cannot rewrite trust policy, allowed refs or model's task objective.
- Worker returns exit 0 but missing acceptance evidence: state remains unverified.

## Decisions not yet approved

Main planning model choice, provider access entitlement matrix, planner output schema, diagnosis frequency, UI for retries, model-selection heuristics, token budgets, maximum context and worker isolation technology.

Related: [orchestrator](orchestrator.md), [task model](task-model.md), [context memory](context-memory.md), [model access](model-access.md), [worker protocol](worker-protocol.md), [testing](../development/testing.md).
