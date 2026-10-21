---
title: AI Agent Task Handoffs
status: Proposed
category: development
last-reviewed: 2026-10-21
tags: [agents, handoff, traceability, execution]
---

# AI agent task handoffs

## Why the handoff is a first-class artifact

Rolvya relies on AI coding agents working on scoped tasks and possibly resuming after failures or provider changes. A handoff must provide enough **verified, compact context** to continue without re-reading an entire conversation. It is not permission to create new nodes in an active plan.

Canonical task and attempt behavior: [DAG](../architecture/dag-planning.md), [scheduler](../architecture/scheduler.md), [context](../architecture/context-memory.md), [worker protocol](../architecture/worker-protocol.md).

## Who reports what

- **Main planning agent:** up-front plan version/hash, task hierarchy, explicit dependencies, goal/constraints, selected model and effort per task, acceptance criteria and three cost scenarios. All task nodes are fixed before run activation.
- **Remote execution worker:** scoped work performed, source/base and resulting remote Git refs, concrete artifacts, commands/tests actually run, runtime/cost measurements, failures and unresolved items. No new project planning.
- **Validator/reviewer:** independent evidence against acceptance criteria, integration conflicts, security/documentation findings and a documented accept/reject outcome.
- **Orchestrator:** authoritative state, attempt identity, lease fencing, retry/reassignment, approval and cost reservation event references; preserves succeeded siblings when a task fails.

## Minimum handoff record

| Field | Mandatory meaning |
| --- | --- |
| project_id / plan_id / plan_hash | Immutable provenance of the planned graph |
| task_id / attempt_id | Logical scope distinct from one execution attempt |
| upstream_dependencies | IDs and validated output refs actually consumed |
| objective, acceptance_criteria | Exactly what the agent was authorized to do and how success is decided |
| scope and forbidden actions | Paths/operations allowed; local desktop access and task creation forbidden |
| worker, model, effort, access_mode | Execution identity and settings, including any approved fallback |
| base_git_ref and result_git_ref | Provider, repository, branch and commit SHA, plus merge status |
| changed_files and artifact_manifest | Paths, object hashes, test reports and source/result references |
| tests_run / tests_not_run | Exact commands and result summaries; never inferred successes |
| decisions, assumptions, blockers | Distinguish approved decision from worker hypothesis |
| failures, attempts and recovery | Typed reasons, prior attempt refs, idempotency/reconciliation notes |
| budget and usage | Authorized reservation, actual/estimated by AI API/compute/storage/transfer |
| status and next_action | Proposed, validated, blocked or failed; explicit recipient action |

Sensitive values, raw tokens and private repository content must never be embedded directly.

## Example human-readable handoff (fictional illustration)

```yaml
project_id: example-project
plan_id: example-plan-v1
plan_hash: example-hash
task_id: example-task-04
attempt_id: example-attempt-02
execution:
  model: planner-assigned-model
  effort: planner-assigned-effort
  access_mode: approved-mode
source:
  provider: remote-git-provider
  repository: example-org/example-repo
  base_commit: example-base-sha
result:
  remote_ref: refs/heads/agent/example-task-04
  commit: example-result-sha
  integration: pending-review
verification:
  status: unverified
  tests_run: []
  tests_not_run:
    - reason: example-harness-not-available
budget:
  reservation_ref: example-reservation
  actual_usage: unknown
next_action: independent-review
```

The sample is *not* a canonical schema or evidence of any real commit. A machine-readable schema and redaction policy need an ADR.

## Retry/reassignment rules

A retry is a new **attempt of the same task**. Carry forward original acceptance criteria, last validated predecessor refs, previous failure class, known unsafe external effects, remaining budget and approved model/effort changes. Fence old attempts; reconcile ambiguous remote writes before replay. Do not silently rewrite task scope or append subtasks.

If a dependency is invalidated by re-integration, mark downstream verification at risk and escalate to the orchestrator; never claim the original DAG changed. Any new plan version requires an explicit approval workflow still to be defined.

## Reviewable completion checklist

- Exact requirement IDs and acceptance evidence included
- Changed source and remote Git refs identify real, existing objects
- Validation independent of worker self-report
- Tests run vs skipped visibly distinct; vulnerabilities/limitations listed
- No secrets or disallowed personal-computer integration
- Approved remaining budget respected; all costs attributed or explicitly unknown
- Documentation/ADR references updated without contradicting the source of truth
- Artifact retention, cleanup and merge state clear
- A fresh agent could reproduce next steps from the report alone

## Open decisions

Structured handoff serialization, size caps, artifact retention, redaction tooling, exact validator role, handoff storage and UI display, plan-version supersession and human approval workflow.

Related: [review process](review-process.md), [engineering standards](standards.md), [test strategy](testing.md), [data model](../architecture/data-model.md), [project status](../project/status.md).
