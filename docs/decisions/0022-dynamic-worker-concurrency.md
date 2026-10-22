---
title: Dynamic Remote Worker Concurrency
status: Approved
category: decisions
last-reviewed: 2026-10-22
tags: [scheduler, concurrency, workers, budget, capacity, preflight]
---

# ADR 0022 — Dynamic remote worker concurrency

- **Status:** Approved
- **Date:** 2026-10-22
- **Owner decision:** No universally hard-coded concurrent-worker limit; dynamic scheduling subject to preflight owner ceilings and verified operational constraints

## Context

Rolvya must maximize useful parallel execution of **independent and ready executable task leaves** while a small (2–4 GB target) remote Rust control plane coordinates distributed remote compute. Historical diagrams with 20 workers are examples, **not** an authorized production maximum. Setting a single fixed value such as 10 or 20 for all projects would waste eligible capacity and delay independent work. Unrestricted launching, conversely, can violate quotas, exceed budgets, increase SQLite/heartbeat contention and create remote Git conflicts.

## Approved decision

1. **Dynamic rather than hard-coded concurrency.** The Rust scheduler continually estimates how many existing *executable leaves* can safely run at once across authorized serverless, cloud VM and user-enrolled remote-VPS pools. The number changes as jobs complete, dependencies become validated, quotas shift, hosts recover and budget headroom changes. No automatic creation of new DAG nodes.
2. **Optional owner-configured cap.** During project preflight, the owner may set a positive maximum concurrent worker/attempt count. If left unset, no arbitrary project-wide numerical cap is imposed by product policy. The scheduler **still** respects actual safe limits from hardware, provider quotas, model access/concurrency entitlements, authorization, lease control, and finite budget.
3. **Eligibility first.** Only ready leaves with verified dependencies, compatible scoped Git side effects, permitted model/effort/access mode, authorized provider/region, sufficient isolation/resources and reliably bounded full cost exposure can be admitted.
4. **Admission is authoritative and atomic.** In SQLite, under a serialized write transaction, re-evaluate active attempt count, optional project cap, budget headroom, lease/fencing constraints and durable outbox intent before each new paid launch. Never rely on a stale external capacity snapshot. A free remote CPU slot does not authorize spending.
5. **Selection preference remains owner-defined.** Respect [ADR 0021](0021-scheduler-selection-modes.md): Economical, Balanced (default), Performance. In economical or balanced modes, a newly provisioned paid host must have a reasoned cost/time advantage; **do not** maximize active workers irrespective of value. Idle/warm VM reservations and auto-scaling are billable and require prior authorization.
6. **Stable adaptation.** Employ measured provider quotas/capacity, failure rates, queue age, cold-start latency, model rate limits and controller/SQLite contention. Scale up/down without oscillation or hot polling. Exact numerical thresholds, ramp rates, fairness algorithms and control loops are engineering parameters, not approved defaults.
7. **Do not disturb productive work.** When budget or provider capacity shrinks, stop admitting new billable attempts. Previously admitted work continues only inside provable pre-reserved exposure ([ADR 0010](0010-inflight-budget.md)); otherwise stop-on-risk. Preserve verified artifacts; only prerequisite-dependent leaves wait.
8. **Scope restrictions.** No local personal-computer worker, no unapproved provider/model switch, no model subscription usage that violates entitlement, no separate paid group-parent worker, and no provider-controlled hidden retries. Retry is a new attempt of the **same** leaf inside the preflight-configured attempt cap ([ADR 0019](0019-centralized-worker-retries.md)).

## Practical example (illustrative, not performance evidence)

A project has 100 independent ready executable leaves. If compatible authorized provider/model capacity is 60 simultaneous attempts, all financial/security gates pass, and the optional project maximum is not lower, the scheduler **may** admit up to 60; the others wait for safe capacity. An optional preflight cap of 30 would limit admission to 30 even if 60 are otherwise possible. A provider quota change to 15 reduces *new* admissions; it does not itself prove already running work must be killed. If only 20 cost-bounded reservations fit the project ceiling, admit **at most 20**, even when host capacity is 60.

Total *useful* parallelism cannot exceed independently eligible leaves; network/SQLite performance, per-provider fairness, resource-class matching and Git path conflicts may reduce it further. Not all 100 tasks should necessarily launch in the cheapest-to-run configuration if the expected total cost/time is worse.

## Consequences and alternatives

- **Rejected:** fixed, universal worker limit such as 20 based on an illustrative diagram.
- **Rejected:** unbounded paid autoscaling solely because extra tasks exist, without provider/entitlement/cost checks.
- **Chosen:** project-configurable optional concurrency ceiling combined with measured dynamic safe admission.
- **Cost:** capacity telemetry, admission synchronization, queue fairness, throttling/anti-thrashing logic and accurate cost bounds require testing and implementation.

## Proposed implementation/acceptance tests (NOT executed)

- 100 independent leaves / synthetic eligible capacity 60 / no project cap → scheduler may admit no more than 60, subject to all other gates; cap 30 → at most 30.
- Hard ceiling funds 20 but capacity is 60 → at most 20 new billable attempts admitted. Concurrent Rust scheduler races never double-allocate last reservation.
- DAG has 60 ready and 40 blocked dependents → blocked leaves cannot launch even with 100 idle workers.
- Provider quota drops / model account rate-limits / SQLite becomes busy → slow or stop new admissions, preserve validated work and safe in-flight attempts.
- Already-funded attempt remains bounded, another new attempt is denied → do not kill the bounded attempt solely to enforce a no-new-admissions decision.
- Economical/Balanced/Performance modes all honor the *same* security, capability, concurrency and budget limits.
- One provider fails; other authorized compatible providers retain running independent tasks; no DAG mutation or unapproved model switch.
- Benchmark control-plane CPU/RSS, SQLite writer latency, heartbeat/write rates, outbox lag, p95/p99 API latency, fairness and cost under 100/500/1000 simulated workers on the remote 2–4 GB target.
- Verify no worker process or Git synchronization is performed on the user's personal computer.

## Open engineering parameters

Exact provider polling/capacity discovery periods, controller admission batch size, aging/fairness strategy, scale-up/down hysteresis, model concurrency-limit discovery, financial exposure caps per resource class, backpressure thresholds and benchmarking-based SLOs remain Proposed. **These are not invitations to re-ask the approved dynamic-concurrency product decision.**

Related: [scheduler design](../architecture/scheduler-selection-policy.md), [provider adapters](../architecture/multi-provider-adapter-contract.md), [preflight quiet runtime](../product/preflight-quiet-runtime-policy.md), [immutable DAG](0001-fixed-upfront-dag.md).
