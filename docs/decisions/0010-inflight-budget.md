---
title: "ADR 0010: Bounded In-Flight Worker Execution and Stop-on-Risk"
status: Approved
category: decision
last-reviewed: 2026-10-21
tags: [adr, budget, workers, provider, safety]
---

# ADR 0010: Bounded in-flight worker execution and stop-on-risk

## Status and owner authorization

**Approved by the project owner on 2026-10-21.** The owner approved the previous recommended **combined policy**: an already authorized worker may continue **only while its full future billable exposure is demonstrably bounded and covered by a prior reservation; otherwise stop the worker when the safe bound cannot be maintained**.

This extends, **without weakening**, [ADR 0002](0002-hard-budget-approval.md): **hard gate on new billable admissions**, explicit authenticated human approval for increasing a ceiling, **no automatic percentage overrun**, preserve completed work and surface alerts. The binding policy does not mean an external cloud/API provider's eventual invoice is mathematically guaranteed never to exceed the limit.

## Decision

1. **Before any billable launch**, the Rust scheduler must atomically establish in SQLite an existing-task attempt, valid lease/fence, approved-budget reservation and durable dispatch intent. It must not admit new billable work whose safe total exposure does not fit the approved remaining ceiling.
2. **Continue an already-running worker only if its remaining potential billable exposure is demonstrably bounded and covered** by a conservative, enforceable reservation and applicable provider execution/time/usage limits. Continuation does not grant a new spending authorization.
3. **If that guarantee is lost or cannot be established**, stop further privileged execution: request provider cancellation/termination, revoke/fence the lease, block additional billable actions where possible, preserve verified/checkpointed work and begin Git/artifact/billing reconciliation. Provider stop acknowledgment is **not** assumed to end billing instantly.
4. **Insufficient headroom for a new attempt always prevents that new admission**, regardless of whether already-authorized bounded attempts continue.
5. **When billing information is missing or delayed**, show uncertain exposure rather than zero. If the remaining bounded exposure cannot be proven under all existing liabilities, trigger the stop-on-risk process; do not silently assume a limit remains valid.
6. **Automatic provider retries** that create new paid executions outside Rolvya's reservation/attempt flow must be disabled where supported, or explicitly accounted for and bounded under an approved policy. Never silently let a provider create unreserved workers.
7. A stopped worker's task remains in the fixed DAG; after the prior attempt is fenced and external effects reconciled, recovery may create a **new attempt of the same task** only under fresh admissibility and budget checks.
8. Only an **explicit authenticated owner approval** can raise the budget ceiling. Automated estimates, UI dismissals, model outputs, worker callbacks, notifications or hidden percentage buffers are not approvals.

## Practical guard and cost exposure

Keep separate category-aware accounting for **AI API, cloud compute, storage and transfer**, with optimistic/expected/pessimistic **forecasts** distinguished from enforceable reservations, incurred/provisional usage and settled invoices.

A candidate continuation check, evaluated against authoritative SQLite data and validated external limits, should establish:

```text
known settled charges
+ all still-liable holds (including the active attempt)
+ additional unreserved/uncertain exposure under conservative reconciliation
<= explicitly owner-approved project ceiling
```

For a running attempt, avoid **double counting** costs already covered by its hold. Its known provider-enforced remaining maximum must fit the **remaining portion of its own reserved exposure** plus other valid project liabilities. If that cannot be demonstrated, initiate stop-on-risk. Do not treat a ~10-minute task planning target as a provider-enforced time or money cap.

## Required continuation/stop behavior

| Condition | Running worker | New paid admissions |
| --- | --- | --- |
| Valid reservation, independently verifiable provider-enforced maximum and sufficient project exposure coverage | May continue within bound | Only if separate headroom permits and every other guard passes |
| No headroom for another task, but existing attempt remains fully bounded/reserved | Continue **only its previously authorized bound** | Block new paid attempts |
| Metering stale yet a credible enforceable maximum and liabilities are still provably bounded | May continue only inside bound; mark invoice data unknown and monitor | Block speculative admissions based on stale billing |
| Maximum cannot be proven, reservation no longer covers full risk, or uncontrolled provider retry occurs | **Request stop/fence and reconcile**; preserve validated/checkpointed outputs | Block until exposure reconciled or explicit new approval |
| Owner increases approved total ceiling explicitly | Re-evaluate against current provider and SQLite state; no automatic unlimited continuation | May reconsider, never bypass dependency/auth/lease checks |
| Stop requested, provider still running or billing | Maintain risk status, inspect/cancel/reconcile; do not claim stopped merely from API response | Block unsafe new admissions |

## Operational consequences and uncertainties

A provider may bill for minimum duration, non-cancelable work, late API usage, network transfer or retries even after a stop request. The product must **report actual discrepancies and not hide them**; Rust/SQLite internal admission control and reservations alone cannot enforce external billing systems. A provider adapter that cannot demonstrate an upper bound is **not eligible for unbounded paid operation** under this policy.

Support explicit worker-side cooperative stop plus provider-level cancellation and identity fencing. Ensure pending Git changes and completed artifacts are preserved, but never accept stale attempt claims of success. The exact stop latency, billing upper-bound computation and checkpoint implementation require provider-specific evidence, not invented SLAs.

## Alternatives rejected

- **Always kill every already-reserved worker immediately when a new task is budget-blocked:** unnecessary interruption when existing work is independently bounded and prepaid/reserved.
- **Allow an unbounded worker to finish and invoice later:** inconsistent with approved financial controls.
- **Automatic reserve/ceiling percentage overrun:** expressly forbidden by owner.

## Test cases and acceptance

1. New task exceeds remaining headroom; existing reserved/bounded worker continues while new admission is blocked.
2. Future exposure limit becomes unverifiable; cancellation/fencing issued and audit records a stop-on-risk event.
3. Billing feed delayed but hard provider cap still verified; mark invoice uncertainty and preserve enforceable limit checks.
4. Worker provider hidden auto-retry incurs a possible second charge; detect, request stop and reconcile.
5. Cancellation completes but invoice later charges minimum duration; actual cost visible and approval cap is not silently adjusted.
6. Repeated cancel/result/invoice callbacks are idempotent and stale worker cannot override a newer attempt.
7. Successful sibling artifacts survive interruption and budget pause.
8. Only explicit owner approval can increase ceiling, and it does not bypass other admission gates.

## Approval record

- **Decision:** Allow continuation **only within bounded, pre-reserved, enforceable exposure**; otherwise **stop-on-risk**.
- **Status:** Approved.
- **Approved by:** Project owner, explicit confirmation in conversation, 2026-10-21.
- **Open engineering parameters:** Specific provider caps/timeout, stop latency, metering freshness, quota strategies, checkpoint implementation, late charge/refund and multi-currency policies.
- **Implementation state:** Not verified.

Related: [in-flight execution contract](../architecture/inflight-cost-policy.md), [budget reconciliation](../architecture/budget-reservation-reconciliation.md), [scheduler](../architecture/scheduler.md), [worker evaluation](../operations/worker-provider-evaluation.md), [testing](../development/rust-sqlite-test-matrix.md).
