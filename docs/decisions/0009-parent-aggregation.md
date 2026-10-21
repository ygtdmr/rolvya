---
title: "ADR 0009: Group-Only Parent Tasks and Verified Aggregation"
status: Approved
category: decision
last-reviewed: 2026-10-21
tags: [adr, dag, hierarchy, aggregation]
---

# ADR 0009: Group-only parent tasks and verified aggregation

## Status and owner authorization

**Approved by the project owner on 2026-10-21.** The owner explicitly approved the previously recommended approach: **parent tasks only organize and aggregate results; execution belongs to planned executable child/leaf tasks.** This closes the fundamental OQ-017 group-vs-executable-parent decision. It extends [ADR 0001](0001-fixed-upfront-dag.md), which already fixed all task nodes and prerequisite edges before execution.

This ADR is a **binding architecture decision, not an implemented feature or a claim that tests passed**.

## Decision

1. **A task with children is a group node**, not a worker-executable task. It must not be leased, assigned an executing attempt, billed as a worker execution or used to spawn agents.
2. **Only preplanned executable leaf tasks are dispatched.** A parent's own code generation, integration, testing or synthesis that requires execution must be modeled as a **separate executable leaf** in the complete up-front plan.
3. **Hierarchy and scheduling prerequisites remain separate.** A parent-child link describes decomposition and **does not automatically add DAG dependency edges**. All prerequisite edges must be declared and validated before activation.
4. A group may be marked **validated succeeded** only when all its **required** child outcomes (including recursively required descendants) have been independently validated **and** any declared group-level acceptance criteria are demonstrably satisfied. A successful process exit or worker claim never suffices.
5. A task with an explicit incoming prerequisite from a group waits for the group's **validated aggregate success**. Group status must be derived by the trusted Rust backend from SQLite-backed evidence; no worker can self-certify the group.
6. If required descendants are incomplete, failed or canceled, group success is forbidden. Retryable failure may lead to another **attempt of that existing leaf**, not a new graph node; prior successful siblings and artifacts remain preserved.
7. Group status derivation and retries change runtime state only; the activated task graph, goal/scope and static plan hash remain immutable.

## Conservative initial interpretation and remaining product details

For the initial implementation, **treat children as required unless an owner-approved optional-child contract exists**. Do not silently treat an optional or canceled descendant as successful. If optional children are introduced, their selection and evidence semantics require explicit review; until then, a plan depending on ambiguous optional aggregation is held/rejected rather than guessed. A group may need an explicit acceptance predicate; the exact serialization, validator format, canceled/failed terminal state labels and root policy remain implementation details.

The owner's approval selects **group-only parents and independently validated roll-up**, not an unreviewed new task type, automatic dependency insertion or arbitrary planner changes at runtime.

## Why this model

- Clean and auditable separation between **task decomposition** and **execution scheduling**.
- Simple remote-worker permissions: only bounded executable leaves receive a lease/attempt.
- Prevents parent double-execution and model-generated subtasks.
- Supports arbitrarily nested group structure without implying a parent is an extra worker.
- Preserves previously validated artifacts when an unrelated descendant fails.

## Alternatives rejected for this version

- **Executable parents with children:** creates ambiguous dependency, cost and attempt semantics. Not approved.
- **Implicit descendant edges from hierarchy:** violates the explicit dependency DAG. Not approved.
- **Worker-submitted group success:** cannot establish independent validation. Forbidden.

## Implementation guardrails

- The complete plan validator classifies every node as group or executable leaf and rejects malformed parent/child references, cycles, missing criteria and inconsistent edge definitions.
- The scheduler leases executable leaves only and evaluates explicit validated predecessor outcomes.
- The Rust aggregation component reads verified SQLite task/child state and writes an idempotent group state/audit transition; it does not create tasks.
- An integration step that needs tools is a distinct planned executable leaf, with its own dependencies, model/effort, scope and acceptance criteria.
- Cross-project references and stale attempt events cannot trigger group success.
- A paused project retains successful leaf artifacts and validated group state.

## Acceptance examples

1. Group G contains independent executable A and B; A finishes successfully while B is running → G **not succeeded**.
2. A and B both validated, group criteria met → G **validated succeeded**, once.
3. B failed irrecoverably → G never appears successful regardless of A.
4. Explicit edge G → C → C cannot start before G is validated; no implied edges between A and B.
5. A planned integration leaf V may depend on A and B; G never runs V implicitly.
6. Duplicated child result or concurrent callbacks do not produce multiple group completion events.
7. Worker requests “add child task” → deny; active plan hash unchanged.

## Approval record

- **Decision:** Group-only parent tasks; execution only at planned executable leaves; independently validated aggregate completion.
- **Status:** Approved.
- **Approved by:** Project owner, explicit confirmation in conversation, 2026-10-21.
- **Open implementation details:** Optional-child support, group predicate serialization, failure/cancellation status enums, full-root layout and algorithmic performance limits.
- **Implementation state:** Not verified.

Related: [parent aggregation implementation contract](../architecture/parent-task-aggregation.md), [task model](../architecture/task-model.md), [plan activation](../architecture/plan-activation-and-versioning.md), [testing](../development/rust-sqlite-test-matrix.md).
