---
title: System Architecture Overview
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [architecture, orchestration, dag]
---
# System architecture overview

## Purpose and status
Rolvya is an open-source orchestration system for AI coding agents. **Architecture intent, not implemented functionality.** Only explicitly approved decisions are binding; detailed interfaces and stack choices remain subject to ADR review.

## Approved invariants
1. One coordinating main agent produces the full hierarchical task plan **before execution**. During runtime no new task nodes are generated.
2. The plan is a directed acyclic graph (DAG): a task starts only when all required predecessors have successfully satisfied their completion criteria. Independent ready tasks may run concurrently.
3. Workers execute bounded tasks, rather than spawning or planning subordinate agents. Per-task instructions include scope, context, model and effort.
4. Insufficient remaining budget stops admission of **new billable work** until explicit owner approval to increase the budget. Never silently permit a percentage overrun. Preserve completed work.
5. Integration with Git repositories is through **remote providers** (including self-hosted servers). No Local Agent, Local Sync or access/synchronization to repositories on the user's personal computer.
6. Documentation is maintained in portable Markdown readable by GitHub and Obsidian.

## Logical component map
```mermaid
flowchart TD
  U[User / approval interface] --> O[Control plane / orchestrator]
  O --> P[Upfront planning agent]
  P --> G[Persisted, validated task DAG]
  G --> S[Dependency-aware scheduler]
  S --> B[Budget admission gate]
  B --> Q[Ready / leased task queue]
  Q --> W1[Cloud worker A]
  Q --> W2[Remote worker B]
  W1 --> R[Results and artifacts]
  W2 --> R
  R --> V[Verification and aggregation]
  V --> O
  O <--> DB[(Single authoritative database)]
  W1 <--> RG[Remote Git repository service]
  W2 <--> RG
```
Arrows represent intended logical control/data flows, not finalized network connections or protocol contracts.

## Component responsibilities
- **User interface and approvals:** capture project intent, review plans, show task/budget state, explicitly approve budget increases and significant decisions. UI stack and UX are open.
- **Planning agent:** decomposes top-level work into nested tasks; fixes dependency edges, goals, constraints, model/effort assignments and completion criteria before run admission.
- **DAG validator:** rejects cycles, missing references, contradictory parent/dependency semantics and unbounded or invalid task assignments.
- **Scheduler:** maintains ready/waiting/running/terminal states; admits independent tasks when dependencies, resource limits and budget permit; ensures controlled concurrency.
- **Worker control and adapters:** provision/use remote compute, deliver scoped instructions, execute CLI agents or API-based models under approved credential and resource policies, and collect results.
- **Persistence:** one database instance stores project, task graph, execution state, instructions/context, attempts, approvals and cost ledger. Schema/database engine are not yet approved.
- **Validation and integration:** verify work, manage compatible remote Git changes and aggregate outputs. Merge-conflict and test-gate rules require dedicated contracts.
- **Observability and recovery:** durable events, logs, lifecycle/heartbeats, retries and reconciliation; exact fault policy remains open.

## Deployment constraints and options
The control plane is intended to work on a modest remote VPS (roughly 2–4 GB RAM was discussed). Workers can use cloud providers or user-supplied **remote** servers; same-VPS execution was discussed but is not the preferred scaling assumption. AWS, Google Cloud and other options are candidates, **not selected providers**. Local personal-computer workers are excluded.

## Known planning targets, not guarantees
Task granularity was discussed as medium complexity, usually targeting no more than about 10 minutes per worker task. Many independent tasks may execute simultaneously; no fixed 20-worker limit or hardware capacity has been approved. Resource quotas and model limits must determine actual concurrency.

## Interfaces awaiting specification
Planner output schema; DAG validation rules; task state-machine transitions; worker lease protocol; cloud lifecycle; model adapters and CLI versus API policy; context packaging; remote Git permissions; artifact format; budget reservation/settlement; identity/secrets; retries, reconciliation and audit event contracts.

## Documentation navigation
See [DAG planning](dag-planning.md), [execution and scheduling](execution.md), [budget and costs](budget-costs.md), [workers and models](workers.md), [security](security.md) and the [decision register](../decisions/README.md). Read the [project status](../project/status.md) for what is actually implemented.
