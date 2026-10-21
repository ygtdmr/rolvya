---
title: Decision Readiness and Approval Gates
status: Proposed
category: project
last-reviewed: 2026-10-21
tags: [decisions, approvals, roadmap]
---

# Decision readiness and approval gates

## Purpose

Track which architectural decisions are approved and which are still proposals. This checklist is documentation only. It does not represent deployed infrastructure or a running application.

## Approved product decisions

- [ADR 0001](../decisions/0001-fixed-upfront-dag.md): One main planning agent creates a complete fixed task graph before execution. Worker agents execute assigned tasks; they do not add new tasks.
- [ADR 0002](../decisions/0002-hard-budget-approval.md): Project spending uses a hard admission gate. An explicit owner approval is required before increasing the ceiling. No automatic percentage overrun.
- [ADR 0003](../decisions/0003-remote-git-only.md): Only remote Git providers and remote worker environments; no personal-computer repository synchronization.
- [ADR 0004](../decisions/0004-control-plane-language.md): The Rolvya backend is written exclusively in Rust.
- [ADR 0005](../decisions/0005-authoritative-database.md): SQLite is the only authoritative database engine.
- [ADR 0009](../decisions/0009-parent-aggregation.md): Non-executable group parents aggregate independently validated child results; only planned executable leaves run workers.
- [ADR 0010](../decisions/0010-inflight-budget.md): In-flight billable workers continue only within enforceably bounded, pre-reserved exposure; otherwise initiate stop-on-risk.

## Engineering decisions awaiting owner approval

| Gate | Candidate approach | Evidence before approval |
| --- | --- | --- |
| Worker communication | [ADR 0006](../decisions/0006-worker-communication.md): worker-initiated HTTPS | Validate network retry, fencing, resource consumption and authentication |
| Worker hosting | [ADR 0007](../decisions/0007-remote-worker-hosting.md): compare remote provider choices | Obtain authorized benchmark results for cost, startup time, security and availability |
| GitHub authentication | [ADR 0008](../decisions/0008-github-authentication.md): GitHub App approach | Validate repository scopes, branch protection, revocation and merge authority |
| In-flight budget implementation details | **Approved policy ADR 0010**; [budget design](../architecture/budget-reservation-reconciliation.md) | Validate provider-specific caps, cancellation timing, unknown bills, artifact preservation; policy choice itself is closed |
| Human and worker authorization | [Identity design](../architecture/identity-authorization.md) | Test owner approval, actor roles, project separation and audit |
| SQLite operational configuration | [SQLite operations](../operations/sqlite-operations.md) | Test transaction locking, WAL option, backup consistency and recovery |

Approving one technology choice does not automatically approve another. Rust and SQLite are final selections; specific crate versions, network transports and cloud providers are not.

## Proposed implementation sequence

1. Record owner decisions on communication, authorization and GitHub access; implement **approved** bounded-or-stop running-work policy and group-only parent aggregation without inventing provider caps.
2. Use Rust and SQLite with synthetic tasks to validate an immutable DAG, transactional reservations and lease fencing.
3. Test remote fake workers with duplicate results, network interruptions and provider launch uncertainty.
4. Run separately authorized and budget-capped provider benchmarks using the [evaluation protocol](../operations/worker-provider-evaluation.md).
5. Validate GitHub integration on a dedicated remote test repository.
6. Implement approval UI and operational controls, then collect independent test evidence before any release.

## Readiness definition

A design may be approved without being implemented. The architecture is currently in specification phase. Production readiness requires tested software, reviewed security rules, reproducible deployments, verified billing behavior and documented recovery procedures.

The [delivery roadmap](roadmap.md) and [milestone definitions](milestones.md) distinguish future engineering goals from verified project status. The product MVP scope itself remains proposed in [product scope](../product/scope.md).

Related: [implementation slices](implementation-slices.md), [open decisions](open-questions.md), [project status](status.md), [documentation index](../README.md).
