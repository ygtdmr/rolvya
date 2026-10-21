---
title: Budget Enforcement and Cost Estimation
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [costs, budget, approvals]
---
# Budget enforcement and cost estimation

## Approved policy: hard stop
A project has a hard budget limit. Before starting **any new billable task**, the scheduler must ensure sufficient approved remaining budget. If insufficient, pause new billable task admission, notify the user, preserve completed work and wait for **explicit approval to increase the budget**. There is **no automatic percentage overrun**. An approval event must state project, amount/currency, actor and time, and be auditable. A refusal keeps the project paused for billable scheduling.

## Cost categories (approved)
Track AI API/model charges, remote cloud worker compute, storage and data transfer **separately**. Estimates include optimistic, expected and pessimistic scenarios. Clearly label actual spending, forecast spending and reserved exposure; never present estimates as invoices.

## Candidate budget ledger (design, not final schema)
- Approved ceiling and currency, change/approval history.
- Actual settled usage by category and provider.
- Outstanding committed/reserved charges per running attempt.
- Forecast additional exposure by task and scenario.
- Available budget = approved ceiling minus recorded actual and enforceable reservations; final accounting requires reconciliation and conservative assumptions.
- Alerts, pause reason and resume authorization.

## Scheduling race control
Two simultaneous ready tasks must not independently spend the same remaining budget. Use serialized/transactional admission and reservation, idempotent usage reporting and reconciliation. Provider invoices may arrive late; handle charges not instantly observable. For cost sources with no reliable upper bound, do not treat an optimistic estimate as guaranteed admission safety: require conservative limits, hard provider caps or explicit policy.

## Cost estimate inputs
For each task: model/API token range, chosen effort, anticipated retries, worker size and seconds, start-up/idle charges, storage volume/time and outgoing bytes. Keep provider prices and pricing dates as external configuration rather than hard-coded unverified constants.

## UX
Show estimated optimistic/expected/pessimistic overall and category-level costs, ceiling, spent/reserved/remaining, the exact reason admission paused, completed tasks, and an explicit budget increase action. Approval must not be inferred from a notification acknowledgment.

## Open technical decisions
Currency conversion, taxes, pricing source freshness, reservation release policy, prepaid/CLI subscription attribution, **provider-specific enforcement of Approved bounded-or-stop ADR 0010**, cost correction/refunds, multi-project budget isolation, offline provider metering and detailed SQLite transaction implementation. **The in-flight policy choice itself is closed.**

## Required future acceptance tests
- Two concurrent admissions never collectively reserve beyond remaining budget.
- Insufficient remaining budget blocks new billable work and retains successful outputs.
- Notification alone never raises a ceiling.
- Explicit ceiling increase creates auditable approval then allows scheduler reevaluation.
- Cost reports distinguish four categories and three forecast scenarios.
- No configured percentage buffer bypasses the hard gate.

Detailed **Proposed** accounting mechanics—including SQLite atomic holds, unknown provider invoices and cloud automatic retries, implementing **Approved ADR 0010**—are in [budget reservation and reconciliation](budget-reservation-reconciliation.md). A transparent hypothetical 40-task, four-category, three-scenario example and separate hard admission calculations are in [worked cost examples](cost-scenario-worked-examples.md). The hard explicit-approval policy itself is **Approved**.

Related: [overview](overview.md), [execution](execution.md), [decision register](../decisions/README.md).
