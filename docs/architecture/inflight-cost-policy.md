---
title: Bounded In-Flight Worker Execution Contract
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [budget, billing, cancellation, workers, approval]
---

# Bounded in-flight worker execution contract

## Approved policy and design boundary

**Approved by owner on 2026-10-21:** [ADR 0010](../decisions/0010-inflight-budget.md) establishes **continue only inside an enforceably bounded, pre-reserved exposure; otherwise stop-on-risk** for already-running billable worker attempts. [ADR 0002](../decisions/0002-hard-budget-approval.md) still requires a hard stop on **new** billable work when available authorization is insufficient, explicit human approval to increase the ceiling, no percentage overrun and preservation of verified results.

This document describes **proposed mechanics**; cloud cancellation commands, billing caps, alert thresholds and timing are not deployed. A stop request does not guarantee a third-party provider's last invoice equals internal reservations.

## Financial guard

Use SQLite authority for approved ceiling, settled costs, held reservations, provisional accrual and conservative unknown liabilities, separated by AI API, compute, storage and transfer. The three forecast scenarios are estimates only and do not grant authorization.

A running worker may continue **only if** its own remaining provider-enforced maximum and all other project liabilities remain inside already approved, non-double-counted exposure. A missing invoice may be labeled **unknown** yet continuation is defensible only if a credible, independently enforceable maximum still bounds that uncertainty. If upper exposure can no longer be established, **request cancellation and fence authority**, and block new risky paid admissions.

## Decision algorithm (candidate)

```text
on cost, worker, provider, reservation, or cancellation signal:
    read verified latest approved ceiling + SQLite liabilities
    for each active paid attempt:
        evaluate provider enforcement, maximum remaining spend, hold coverage
        if continuation stays inside authoritative approved reserved exposure:
            continue WITHIN that existing bound (no fresh authorization)
        else:
            persist stop_on_risk + audit event
            fence lease and prevent additional privileged model/worker calls
            request remote provider cancellation/termination
            preserve validated/checkpointed artifacts
            reconcile Git refs, cloud instance state and delayed charges
    for any candidate NEW paid attempt:
        admit only via separate atomic SQLite budget/lease/outbox transaction
```

Do not interpret provider-side `cancel accepted` as `billing has stopped`. Poll/reconcile until external state is known, and do not release possible charges merely because a worker has no heartbeat. Bounded completion and provider cancellation rules must be tested on chosen provider before paid deployment.

## Operational state table

| Signal | Existing paid attempt | New billable admission | Owner-facing UI |
| --- | --- | --- | --- |
| Remaining reserved maximum fully supported and enforceable | May continue inside prior approved bound | Independently check headroom and all other gates | Show reservation, settled and provisional usage |
| Insufficient headroom to reserve another attempt | Continue only if its existing bound still holds | **Reject new paid attempt** | Alert and show pending tasks |
| Billing feed delayed but provider hard cap and total liability bound verifiable | May continue within that provable cap | Avoid speculative admission where exposure is uncertain | Mark delayed charges as **unknown** |
| Reserved maximum exceeded or future cap cannot be enforced | **Stop-on-risk**, fence and reconcile | Block risky new paid attempts | Show stopping, possible final charges |
| Provider launches unreserved retry or new billable container | Halt/contain and reconcile extra exposure; reject stale authority | Block and investigate | Warn of billing drift |
| Owner explicitly approves a larger total ceiling | Recheck every bound and authorization; no automatic unlimited run | Scheduler may re-evaluate | Log exact approval event |
| Attempt stopped after possible Git write | Reconcile remote ref and artifact ownership before a new attempt | Block unsafe duplicate launch | Show unverified external effects |

## Provider adapter obligations

- Before paid launch, establish effective maximum compute runtime, resource billing interval, AI API/CLI usage controls where provider offers them, storage and transfer exposure.
- Reserve conservative **full exposure**, not just an expected mean; use accurate provider-specific billing minima and refund behavior.
- Disable/reconcile automatic provider retries that could create unreserved paid attempts.
- Support provider-side cancellation/termination, remote worker cooperative stop and credentials/lease fencing.
- Identify late invoices, remaining liabilities and possibly successful Git writes even after interruption.
- Preserve already validated artifacts; partial work can be checkpointed only under security/retention rules and must not be called verified.
- Do not assume internal estimates can absolutely prevent provider invoice overruns.

## Verification cases

1. A budget-blocked new task cannot start while a separately bounded reserved attempt finishes.
2. Loss of cost-bound enforceability triggers persisted stop-on-risk, lease fencing and external cancel request.
3. Delayed invoice + still-verifiable hard maximum displays uncertainty without inventing zero spend.
4. Delayed invoice + no verifiable remaining maximum triggers stop-on-risk.
5. Duplicate stop acknowledgement or late worker result does not reopen fenced attempt.
6. Cloud auto-retry and post-cancel minimum charges remain visible and reconciled.
7. Already accepted task artifacts survive stop, budget pause and retry of an existing task.
8. Only authenticated explicit human approval can increase budget ceiling.

## Still-open engineering choices

Provider-specific enforceable caps and confidence model; cancellation latency, interval and post-stop charges; API token quota behavior; accounting currency conversion, taxes, partial artifact storage and alert delivery. **The Bounded-or-Stop decision itself is approved.**

Related: [ADR 0010](../decisions/0010-inflight-budget.md), [budget reconciliation](budget-reservation-reconciliation.md), [scheduler](scheduler.md), [human approvals](approval-workflow.md), [provider comparison](../operations/worker-provider-evaluation.md), [tests](../development/rust-sqlite-test-matrix.md).
