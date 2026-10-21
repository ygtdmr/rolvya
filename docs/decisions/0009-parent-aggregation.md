---
title: "ADR 0009: Parent Task Completion and Aggregation"
status: Proposed
category: decision
last-reviewed: 2026-10-21
tags: [adr, dag, hierarchy, aggregation]
---

# ADR 0009: Parent task completion and aggregation

## Decision status

**Proposed — owner approval required.** The owner approved arbitrarily deep hierarchy plus an explicit dependency DAG, but **did not yet choose** whether parent/group tasks execute code or how they become validated succeeded. Until approved, the system must reject ambiguous activation or preserve the plan in draft.

## Decision to make

When a task has children, should it be:
1. A **group-only parent**, aggregating separately planned executable leaf tasks; or
2. An **executable parent**, independently assigned an attempt and also aggregating child results?

And when an explicit prerequisite edge targets a parent, exactly what result satisfies that edge?

## Options and tradeoffs

| Option | Strengths | Weaknesses |
| --- | --- | --- |
| A: Group-only parent, executable leaves | Predictable scheduling; group never consumes worker or new budget; clear plan freezing | Parent-specific work needs an explicit planned leaf or integration task |
| B: Executable parent | Supports parent-level integration/code generation | Requires extra semantics for parent own attempt, descendant ordering, costs, failure and validation |
| C: Infer dependencies automatically from hierarchy | Easy to describe | Obscures approved distinction between decomposition and prerequisites; risk of accidental serialization |

## Working recommendation (not approved)

Choose **A** for the first version, and require a planned executable integration/validation leaf for work that conceptually belongs to a parent. Every required child must be validated and the group's declared acceptance predicate must pass before parent success; do not equate a worker exit code with validation.

Do not permit implicit hierarchy-driven dependency edges; dependencies must be explicitly described **before plan activation**. Optional children, canceled descendants, edges pointing at groups and parent-level acceptance predicates must be defined in the approved contract.

## Consequences if approved

- The main agent must emit a `group` vs `executable` distinction for all planned tasks.
- Only executable leaves may be leased to workers in the first version.
- Rust state aggregation derives group results from authoritative SQLite child outcomes and validation evidence.
- A child failure or budget pause does not erase successful siblings.
- A dependency targeting a group is satisfied only after the **group result is independently validated**, never after incomplete or self-reported child status.
- A genuine need for new tasks after activation requires a separately authorized plan version; no new runtime nodes.

## Verification / approval evidence

Parent cycle and DAG-cycle negative tests; concurrent child completions produce one group transition; failed required child blocks parent success; dependency on a group waits; active graph hash remains constant; no parent worker attempt is scheduled under option A. See [detailed policy design](../architecture/parent-task-aggregation.md).

## Owner decision record

- Decision: **PENDING**
- Approved by/date: **PENDING**
- Chosen policy: **PENDING**
- Optional descendants semantics: **PENDING**
- Parent dependency readiness: **PENDING**

Related: [ADR 0001](0001-fixed-upfront-dag.md), [task model](../architecture/task-model.md), [plan activation](../architecture/plan-activation-and-versioning.md), [open questions](../project/open-questions.md).
