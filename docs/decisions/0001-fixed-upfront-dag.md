---
title: "ADR 0001: Fixed Upfront DAG and Worker Roles"
status: Approved
category: decision
last-reviewed: 2026-10-21
tags: [adr, dag, agents]
---

# ADR 0001: Fixed upfront DAG and worker roles

## Status and authority

**Approved product decision**, based on previously confirmed user direction; formal implementation details remain proposed. Historical approval date is not independently verified.

## Context

Unbounded agent autonomy and runtime task decomposition would make dependencies, cost forecasts and scheduling less predictable. Rolvya instead needs a complete reviewable plan before execution.

## Decision

A single main planning agent creates the entire multi-level hierarchy, explicit dependency graph, instructions, model and effort assignments **before** execution. The DAG is fixed for the active run: no new task nodes may be introduced at runtime. Workers execute assigned tasks without spawning their own subordinate planners. Independent eligible tasks execute in parallel; prerequisites must be satisfied before dependents start. A failed task can have a new **attempt** using another worker/model/effort without becoming a new task.

## Consequences

A validator must reject cycles and missing edges; task identity must survive retries; hierarchy and dependency edges must remain distinct; state changes and result aggregation need explicit rules. A materially different plan requires a separately approved versioning policy, not silent runtime graph editing.

## Not decided

Exact schema, scheduler algorithm, plan revision/approval workflow, effort nomenclature and retry limits.

## Alternatives considered

Dynamic runtime task creation was discussed and explicitly rejected. Specific scheduling/graph storage alternatives have not been chosen.

Related: [DAG contract](../architecture/dag-planning.md), [execution](../architecture/execution.md), [requirements](../product/requirements.md).
