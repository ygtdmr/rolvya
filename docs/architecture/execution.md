---
title: Scheduling and Execution Lifecycle
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [scheduler, execution, reliability]
---
# Scheduling and execution lifecycle

## Confirmed behavior
The scheduler runs only tasks whose explicit prerequisites are met. Independent tasks may run in parallel. Main-agent planning precedes execution; worker agents execute assigned tasks without creating subtasks. Budget exhaustion halts admission of new billable tasks until explicit approval; completed work remains durable.

## Proposed task lifecycle (not an adopted API contract)
`planned → blocked / ready → reserved → running → validating → succeeded`. Alternate paths: `failed`, `retry_wait`, `canceled`, `paused_budget`. Transitions must be durable, authorized and audit logged; final names and edge cases are open. A task can be dependency-blocked and budget-blocked independently; avoid encoding two orthogonal constraints into one state.

## Scheduler decision order
1. Validate active immutable DAG version and dependency completion.
2. Determine ready leaf tasks and reject contradictory writes or unsafe parallel integration.
3. Determine worker capacity, model availability, quota and execution mode.
4. Estimate and reserve sufficient budget **before** authorizing new billable work.
5. Lease the task to exactly one active attempt; use an idempotency key/fencing token.
6. Execute in a remote isolated workspace, collect logs and outputs, and verify acceptance criteria.
7. Commit durable results; release or reconcile reservations and enable newly unblocked tasks.

Order is illustrative until formal state transitions, reservations and worker contracts are approved.

## Failures and recovery
Track distinct failure classes: model/context exhaustion, transient provider errors, worker crash/lease expiry, credential failure, invalid output, test failure, merge conflict, and budget denial. Retry only within a documented cap; a main orchestrator may reassign model/effort or worker under policy while keeping the original task node. Retries must not duplicate irreversible Git operations or charge reservations twice. Worker output must be treated as untrusted until validated.

## Integrating parallel changes
Prefer task-isolated remote branches/worktrees or equivalent cloud workspace separation. Shared artifact conflicts must be detected and resolved through a reviewable integration strategy. The project explicitly does not connect to repositories on the user's personal computer.

## Required telemetry
Persist scheduling decision, attempt identity, worker ID, model, effort, timestamps, state transitions, dependency eligibility, approval reference, estimated/reserved/actual cost, artifact references, validation result and redacted error categories.

## Open decisions
Scheduling fairness/priority, queue backend, execution timeouts, max concurrency, lease duration, retry caps/backoff, terminal-state policy, parent aggregation, cancellation semantics, merge authority, heartbeats, partial-result handling and provider failover.

## Acceptance tests to write later
Dependency blocking; independent parallel admission; crash/lease recovery without duplicate commits; retry preserving graph identity; budget denial with durable completed work; approval-gated resume; concurrent Git artifact conflict isolation.

Related: [overview](overview.md), [DAG planning](dag-planning.md), [workers](workers.md), [budget](budget-costs.md).
