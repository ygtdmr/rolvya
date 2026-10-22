---
title: "ADR 0002: Hard Project Budget Approval Gate"
status: Approved
category: decision
last-reviewed: 2026-10-21
tags: [adr, budget, approvals]
---

# ADR 0002: Hard project budget approval gate

## Status and authority

**Approved product policy** from the 2026-10-20 decision checkpoint. Implementation design and provider billing enforcement remain proposed.

## Context

Concurrent agents and remotely billed workers can create cost exposure. A warning alone cannot serve as authorization to spend beyond a user's stated project ceiling.

## Decision

Before starting new billable work, Rolvya checks available **human-approved** budget. When insufficient, scheduling of new billable tasks pauses, the user is alerted, and completed work remains available. Resumption of billable admission requires explicit approval to increase the ceiling. No automatic percentage-based overrun is permitted. Cost tracking separates AI API, cloud worker compute, storage and data transfer. Estimates report optimistic, expected and pessimistic scenarios separately from settled actuals and reservations.

## Consequences

Require atomic reservation/admission across concurrent tasks; auditable approval actors and amounts; idempotent cost events; delayed-billing reconciliation; recoverable pause/resume. Acknowledge that provider invoices can lag, and design conservative exposure limits rather than promising a physically impossible hard billing guarantee without provider caps.

## Not decided

Accounting currency, tax, exchange-rate policies, in-flight worker behavior, metering accuracy and provider-side caps.

## Alternatives considered

Automatic percentage overrun was rejected by the user. Exact budgeting implementation alternatives remain open.

Related: [budget architecture](../architecture/budget-costs.md), [data model](../architecture/data-model.md), [acceptance criteria](../product/acceptance-criteria.md).

## Multi-Owner authorization refinement (Approved 2026-10-22)

When a project has multiple Owners, **one individually authenticated Owner**, after recent verified strong reauthentication, may explicitly authorize an increase to the hard project ceiling. A unanimous signature is **not** required. The old and new ceiling, actor, transaction/version and approval evidence are auditable; **other Owners receive notifications** after a successful increase without serving as extra approvers. Missing notification delivery does not grant additional spending authority or negate an otherwise valid committed approval; retry and audit delivery. Preserve all original atomic reservation and no-percent-overrun rules. See [ADR 0029](0029-owner-controlled-project-invitations.md).
