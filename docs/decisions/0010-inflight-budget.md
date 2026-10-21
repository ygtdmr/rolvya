---
title: "ADR 0010: In-Flight Worker Spending Policy"
status: Proposed
category: decision
last-reviewed: 2026-10-21
tags: [adr, budget, workers, provider, safety]
---

# ADR 0010: In-flight worker spending policy

## Decision status

**Proposed — owner approval required.** The owner has already approved [ADR 0002](0002-hard-budget-approval.md): hard admission gate; explicit authenticated human authorization to raise a ceiling; no percentage-based automatic overrun; preserve verified work. This ADR does **not** reopen that policy. It addresses the **unresolved behavior of already-running billable work** near the approved cap, amid delayed provider billing.

## Decision to make

After a worker attempt was admitted with an initial cost reservation, what happens when remaining headroom becomes insufficient, invoices arrive late or the running attempt's future exposure is uncertain?

## Options

| Candidate | Mechanism | Benefit | Risk |
| --- | --- | --- | --- |
| A: Proactive stop-on-risk | When exposure exceeds a verified safe bound, request provider cancellation and fence worker authority | Stronger protection against future unapproved costs | Provider minimum charges and late invoices still occur; interrupts work |
| B: Continue only inside enforceable reserved bound | Running worker may finish while hard provider caps and pre-reserved exposure remain sufficient | Better completion rate | Needs proven API/model/compute caps and careful accounting |
| C: Unlimited finish then bill | Let paid worker continue regardless of approved remaining exposure | Little interruption logic | **Incompatible with strong budget safeguards** without an explicit new authorization; not recommended |

## Working recommendation (not approved)

Consider combining **B for bounded attempts** with **A when remaining exposure becomes unknowable or cannot be enforced**. Require known hard provider duration/compute limits where available, conservative reservations, ongoing reconciliation and checkpoint preservation. Never claim that a Rust/SQLite application estimate guarantees the external invoice cannot exceed the cap.

Disable or explicitly authorize cloud provider automatic retries that could start additional paid instances outside one Rolvya attempt. Handle SQLite budget and lease admission atomically before provider launch; distinguish costs of API, compute, storage and transfer.

## Effects on task state and approval UX

- Insufficient budget **always** blocks admission of a new billable attempt.
- Already verified artifacts remain accessible.
- If an active attempt is stopped, fence the lease, request cloud cancellation and reconcile remote Git refs and late charges before retry.
- A pending warning or budget increase request is not an approved increase; only a durable authenticated approval changes the ceiling.
- When provider billing is unknown, show **unknown** exposure and halt unsafe new work.
- Retry remains a new **attempt of an existing task**, never a new task node.

## Acceptance tests

Simulate metering lag, unexpected minimum billing, provider auto-retry, in-flight cancellation, stale lease, post-cancellation charge, unacknowledged remote Git push, and explicit owner denial/approval. Every outcome must show actual-vs-reserved exposure, audit history and no hidden approval buffer.

## Owner decision record

- Decision: **PENDING**
- Approved by/date: **PENDING**
- Policy for bounded running attempts: **PENDING**
- Policy for unbounded/unknown exposure: **PENDING**
- Cancellation, artifact and billing reconciliation rules: **PENDING**

Detailed options: [in-flight policy design](../architecture/inflight-cost-policy.md). Accounting foundations: [budget reconciliation](../architecture/budget-reservation-reconciliation.md), [approved budget ADR](0002-hard-budget-approval.md), [owner approval workflow](../architecture/approval-workflow.md), [open questions](../project/open-questions.md).
