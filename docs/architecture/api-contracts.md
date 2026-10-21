---
title: API and Event Contracts
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [api, events, contracts]
---

# API and event contracts

## Status

This is a **contract inventory**, not a deployed API or approved OpenAPI schema. Protocol (HTTP, gRPC, messaging), URL paths and authentication scheme remain undecided.

## Proposed operations and invariants

| Operation | Inputs | Output / invariant |
| --- | --- | --- |
| Create project | owner, intent, constraints, remote repository | Stable project identity, auditable ownership |
| Propose plan | project, request, model configuration | Versioned complete DAG draft, three scenario estimates |
| Validate/activate plan | plan version, validation report, authorization | Immutable approved plan; reject cycles and missing acceptance criteria |
| Fetch ready work | project/worker capability | Only dependency-satisfied tasks, subject to capacity and cost gate |
| Reserve and lease attempt | task, worker, expected maximum exposure, idempotency key | Atomic budget admission and fenced lease |
| Heartbeat/report progress | attempt, lease token, state, redacted metrics | Reject stale/replayed leases |
| Submit result | attempt, artifact manifest, tests, usage | Durable result pending validation; idempotent |
| Verify/complete task | task, evidence, authorization | Validated terminal state and successor eligibility |
| Request budget increase | project, proposed new ceiling, reason | Pending approval **without raising ceiling** |
| Approve/deny budget | pending request, authenticated human actor | Auditable decision; only approval may raise ceiling |
| Report costs | project, period, category, scenario/actual | API, compute, storage, transfer separate |
| Read event stream | project, cursor | Ordered, redacted events with stable pagination |
| Cancel/pause/resume | project/attempt, reason, actor | Consistent state and budget semantics |

## Standard envelope candidates

Commands should carry `project_id`, `request_id`, expected version (optimistic concurrency), actor identity and correlation ID. Task messages must bind `plan_id`, `task_id`, `attempt_id`, `lease_token` and output hashes. Use idempotency keys for remote Git writes and financial reservation actions.

## Contract error families

Authorization denied; invalid/expired lease; stale version; DAG validation error; dependency blocked; worker capacity unavailable; insufficient approved budget; provider failure; verification failure; duplicate operation; and transient retryable error. Errors must not leak raw secrets.

## Test scenarios

- Missing predecessor denies activation.
- Two concurrent reservations cannot collectively overspend.
- Replay of completion with same idempotency key causes no second commit.
- Stale attempt cannot overwrite a newer validated result.
- Budget notification or acknowledgement never counts as approval.
- Unauthorized worker cannot access another project's context.

## Open decisions

Transport and schema format, versioning, paging, streaming, authn/authz, rate limits, webhook semantics, worker polling vs push, and retry SLAs.

Related: [data model](data-model.md), [execution](execution.md), [budget](budget-costs.md), [security](security.md).
