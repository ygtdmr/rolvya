---
title: Cross-Project Scheduling Fairness and Aging
status: Approved
category: decisions
last-reviewed: 2026-10-22
tags: [scheduler, fairness, priority, concurrency, budgets, tenancy]
---

# ADR 0023 — Cross-project scheduling fairness and aging

- **Status:** Approved
- **Date:** 2026-10-22
- **Owner decision:** Share *eligible, authorized shared* worker capacity fairly across active projects, preserve preflight project priority, and give waiting work increasing consideration so large projects cannot starve smaller ones.

## Context

Rolvya supports arbitrarily many independent executable task leaves across multiple projects and providers. A single project with 100 ready leaves must not monopolize a **shared** eligible worker pool indefinitely while another project has 10 ready leaves. Conversely, projects may have deliberately different owner-selected priorities, provider permissions, reserved/dedicated capacity and available budgets. Fairness must not imply accessing another owner's resources or overriding paid-work admission controls.

Prior [ADR 0022](0022-dynamic-worker-concurrency.md) approved dynamic parallel capacity with an optional per-project preflight cap. [ADR 0021](0021-scheduler-selection-modes.md) approved Economical/Balanced/Performance ranking. Neither fixed a cross-project queueing policy.

## Approved policy

1. **Fair access to shared capacity.** Among ready, eligible projects competing for a *shared* authorized pool, schedule in a way that prevents a large ready queue from consuming every future admission. Consider distinct project demand, prior allocation and wait age, not just raw number of ready leaves. When there is ample capacity, all eligible independent leaves can proceed without artificial serialization.
2. **Preflight priority respected.** The owner can set a project priority in the preflight authorization snapshot. Scheduling takes this priority into account while still preventing indefinite starvation of lower-priority *eligible* projects. Priority does not override security, model entitlement, remote Git conflict, budget or user-set concurrency limits.
3. **Aging to prevent starvation.** Eligible work that waits longer gains scheduling consideration. Exact time-to-boost, weighting, service guarantees and tie-breakers require design and load testing; no numerical fairness algorithm is silently approved here.
4. **Only compare truly shareable resources.** One owner's cloud account, model quota, remote VPS, secret, budget, data region or isolation class is not automatically available to other projects. Dedicated pools, incompatible model access and disallowed providers are excluded before fairness ranking. Shared capacity must have explicit authorization and tenant isolation.
5. **Independent budget admission.** Every billable attempt requires its own project-approved bounded reservation within the correct SQLite ledger; a project with insufficient headroom cannot use another project's unused funds. A fairness score never implies permission to spend.
6. **Respect dependency and execution identity.** Only existing validated, ready executable leaves compete. Parent group tasks do not run. Retry attempts follow their own configured caps and cannot create new DAG nodes. A task is not preempted or killed solely because another project's fairness score rises; safe queue decisions govern *new* admissions.
7. **Do not spam users.** Fairness selection and routine queueing operate automatically under approved preflight choices. Project owners see queue state and reasons when useful; ask for human intervention only when a genuinely new budget, permission or scope decision is required.
8. **Auditable decisions.** Persist queue age/eligibility, effective configured priority, pool scope, candidate selection reason, cap/headroom blockers and allocation decision without raw credentials. Retain evidence for diagnosing starvation or tenant unfairness.

## Example (illustrative, not a promised quota)

Projects A and B share one permitted pool with a current capacity of 4 worker slots. A has 100 ready executable leaves; B has 10. At equal preflight priority and with both having valid funding/permissions, the scheduler should distribute subsequent admissions across both while demand remains, rather than assigning every slot forever to A. This example **does not approve a fixed 2:2 split**, a maximum wait time, or a universal worker count. If B's model/region is unsupported or B's budget has no headroom, B cannot receive a paid slot from that pool until its blocker is resolved.

## Rejected interpretations

- **Pure FIFO across every leaf without project awareness:** high-volume projects may monopolize shared capacity.
- **Absolute strict priority without aging:** potentially infinite starvation for lower-priority projects.
- **Guaranteed equal share regardless of eligibility:** violates security, provider ownership, configured project priorities and bounded budgets.
- **Preempt and cancel validated running work solely to force equality:** risks lost progress, billable cancellation delay and duplicate side effects.

## Engineering candidates (Proposed, not owner-approved parameters)

A weighted deficit round-robin or similar project-aware fair scheduler, with bounded priority aging, per-provider-pool queues and admission snapshots, is a candidate. Enforce deterministic decisions where possible; ensure fairness across shared eligible resources while distinguishing dedicated hosts from pooled capacity. Benchmarks should guide batching, starvation thresholds, SQLite contention, distribution and controller CPU/RAM.

## Acceptance cases — specified, not executed

- A: 100 ready leaves, B: 10, both eligible/equal priority, shared capacity constrained → B receives admission opportunities before A drains all 100.
- Prioritized A and eligible lower-priority B → A gets appropriate preference, but B cannot be indefinitely ignored solely due to lower priority.
- Pool belongs exclusively to project A → no capacity is allocated to B without owner-authorized sharing.
- B has no valid budget reservation, or is blocked by model/region/sandbox policy → never admit B due to fairness pressure.
- A has a preflight max-concurrency cap → fair scheduling honors that cap, but does not substitute a universal hard-coded cap.
- C depends on an unfinished leaf → no queue aging/priority promotes C before prerequisite validation.
- Provider outage, capacity change or controller restart → persist and reconstruct queue/accounting state without rerunning completed work.
- Multi-project stress tests at 100/500/1000 synthetic workers → measure service distribution, p95 queue latency, starvation, RSS/CPU, SQLite write contention, cost and task completion.
- Economical/Balanced/Performance preference still chooses suitable hosts *within* each authorized project/pool; it never overrides queue admission gates or fairness principle.

## Open implementation parameters

Priority levels and whether weights are user-visible, aging formula and bounds, fairness window, persistent scheduler queue schema, tie-breaking, queue batching and any service-level targets remain **Proposed**. This owner approval does **not** select a particular scheduling algorithm or authorize a billing/tenant-policy exception.

Related: [scheduler selection policy](../architecture/scheduler-selection-policy.md), [project preflight policy](../product/preflight-quiet-runtime-policy.md), [dynamic concurrency](0022-dynamic-worker-concurrency.md), [hard budget](0002-hard-budget-approval.md).
