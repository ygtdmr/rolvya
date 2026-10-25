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
- [ADR 0006](../decisions/0006-worker-communication.md): Remote workers initiate HTTPS claim, heartbeat and result communication; Rust/SQLite retains authority.
- [ADR 0008](../decisions/0008-github-authentication.md): GitHub access uses GitHub App installations and trusted Rust integration, with protected ref controls.
- [ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md): Multiple provider adapters may run remote workers; no single-cloud lock-in.
- [ADR 0009](../decisions/0009-parent-aggregation.md): Non-executable group parents aggregate independently validated child results; only planned executable leaves run workers.
- [ADR 0010](../decisions/0010-inflight-budget.md): In-flight billable workers continue only within enforceably bounded, pre-reserved exposure; otherwise initiate stop-on-risk.

## Open engineering implementation and rollout gates

| Gate | Candidate approach | Evidence before approval |
| --- | --- | --- |
| HTTPS protocol configuration | **Approved ADR 0006** | Finalize worker identity, Rust stack, URL/schema, lease and load tests; choice of HTTPS is closed |
| First provider rollout | **AWS Fargate first is approved ADR 0041**, after fake workers; ADR 0007 is historical provider comparison | Compare specific authorized vendors/regions; measured startup, cost, quotas and security |
| GitHub App configuration | **Approved ADR 0008** | Finalize App permissions, key storage, webhook validation, branch protections and human merge authorization |
| In-flight budget implementation details | **Approved policy ADR 0010**; [budget design](../architecture/budget-reservation-reconciliation.md) | Validate provider-specific caps, cancellation timing, unknown bills, artifact preservation; policy choice itself is closed |
| Human and worker authorization | [Identity design](../architecture/identity-authorization.md) | Test owner approval, actor roles, project separation and audit |
| SQLite operational configuration | **Approved [ADR 0014](../decisions/0014-sqlite-initial-configuration.md): WAL, maximum four SQLx pool connections, five-second SQLite busy timeout** | Test atomic task/lease/budget transactions, locking, durability/checkpoint choices, backup consistency and recovery; approved defaults are not performance benchmarks |

Approved: Rust, SQLite, HTTPS worker control, GitHub App authentication and multi-provider infrastructure (plus fixed DAG/group/budget policies). **Still open:** remaining crate versions, wire details, identity-provider/worker permissions, initial provider regions and measured cost limits, and live deployment. **Axum (ADR 0012), SQLx (ADR 0013) and the initial SQLite settings (ADR 0014) are already Approved.** Approved design does not imply implementation.

## Proposed implementation sequence

1. Implement **approved** HTTPS, GitHub App and multi-provider contracts with explicit authentication/permission design; enforce bounded-or-stop spending and group-only parents without inventing provider caps.
2. Use Rust and SQLite with synthetic tasks to validate an immutable DAG, transactional reservations and lease fencing.
3. Test remote fake workers with duplicate results, network interruptions and provider launch uncertainty.
4. Run separately authorized and budget-capped provider benchmarks using the [evaluation protocol](../operations/worker-provider-evaluation.md).
5. Validate GitHub integration on a dedicated remote test repository.
6. Implement approval UI and operational controls, then collect independent test evidence before any release.

The [transport/Git/hosting implementation and rollout pack](transport-git-hosting-decision-pack.md) records the approved choices and remaining implementation gates, while the [remote integration test matrix](../development/remote-integration-test-matrix.md) defines the negative and recovery evidence required before approval and implementation.

## Readiness definition

A design may be approved without being implemented. The architecture is currently in specification phase. Production readiness requires tested software, reviewed security rules, reproducible deployments, verified billing behavior and documented recovery procedures.

The [delivery roadmap](roadmap.md) and [milestone definitions](milestones.md) distinguish future engineering goals from verified project status. The product MVP scope itself remains proposed in [product scope](../product/scope.md).

Related: [implementation slices](implementation-slices.md), [open decisions](open-questions.md), [project status](status.md), [documentation index](../README.md).
