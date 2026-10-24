---
title: Pre-Implementation Gap Closure and Full Project Review
status: Open
category: project
last-reviewed: 2026-10-23
tags: [readiness, gaps, review, implementation, approvals]
---

# Pre-implementation gap closure and full project review

## Purpose and evidence boundary

The project Owner requested **resolve the remaining gaps before a comprehensive, end-to-end review of Rolvya**. This file is the durable tracking register for that sequence, **not an assertion that all open choices have been resolved**. Current source license: **MIT**, see [LICENSE](../../LICENSE). The active Git commit history was rewritten after that licensing decision; prior historical SHA references must not be treated as current branch commits. The owner-approved architecture, product requirements and open questions are authoritative.

All rows below are **documentation/decision or future engineering tasks**. No running orchestrator, executable scheduler, provider integration, passing CI or real billing enforcement is verified.

## Completed housekeeping in this review

- Reconciled [open questions](open-questions.md): [ADR 0014](../decisions/0014-sqlite-initial-configuration.md) already selects SQLite **WAL**, maximum **four control-plane pool connections**, **five-second SQLite busy timeout**. Exact schema and unselected checkpoint/durability tuning are still open.
- Reconciled [decision readiness](decision-readiness.md): Rust, SQLite, Axum, SQLx, worker-initiated HTTPS, GitHub App and multi-provider architecture are selected; provider auth, wire schemas, measurable limits and integration still require work.
- Reconciled [implementation slices](implementation-slices.md): removed stale reference to worker HTTPS and GitHub App as Proposed choices.
- The active licensing policy is **MIT**. Full Git-host retention, third-party forks/caches and any previously granted historical license rights cannot be guaranteed erased merely by a force-push.

## Recently approved v0.1 product scope

On 2026-10-23 the Owner approved the proposed **v0.1 first usable release** boundaries: complete up-front DAG planning; parallel and dependency-safe remote agents; project configuration; model/effort selection; hard budget tracking/approval; remote GitHub integration; verified outputs; basic web panel; and necessary secure runtime/operations. Native mobile, conversational voice assistant and optional advanced integrations remain deferred. Exact technology/provider choices, legal access checks, costs and all executable implementation are **not** approved merely by this product-scope approval.

## Gap closure queue

| Order | Work item | Current status | Closure evidence | Source |
| --- | --- | --- | --- | --- |
| 0 | Ensure document consistency with approved ADRs and current MIT release policy | **Current decision references reconciled 2026-10-24; historical snapshots retained** | All tracked Markdown audited against approved ADRs, internal links checked and discrepancies resolved | [Decisions](../decisions/README.md), [status](status.md) |
| 1 | Confirm public MVP boundaries vs technical fake-only demo | **Approved 2026-10-23 — implementation pending** | v0.1 first-release feature matrix and explicit exclusions recorded; test mappings still to be verified | [Product scope](../product/scope.md) |
| 2 | Atomic SQLite lease, budget reservation, fence and durable outbox design | **Approved 2026-10-23 — implementation/testing pending** | [ADR 0031](../decisions/0031-atomic-task-budget-admission.md) selects `BEGIN IMMEDIATE` and post-commit dispatch; schema, failure tests and real proofs still needed | [OQ-003 closed](open-questions.md), [backend](../architecture/rust-sqlite-backend.md) |
| 3 | Human identity provider and project-scoped authorization implementation | **Login methods updated in ADR 0033, P0 implementation/security still open** | Password or verified-email Magic Link/OTP + optional authenticator-app TOTP (no SMS); Owner configures initial/enabled methods; verify subject binding, session safety, Owner step-up, role matrix and revocation | [OQ-004](open-questions.md), [ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md) |
| 4 | Worker identity, remote secrets and isolation | **P0 Open — contract and threat model needed** | Trust boundary, signed/short-lived tokens, secret lease/revocation, isolation negative tests | [OQ-005](open-questions.md), [secrets](../operations/secrets.md) |
| 5 | Claude/Codex CLI subscriptions versus official API worker entitlement | **P0 Open — fresh provider evidence required** | Confirm allowed unattended remote modes, version/date of terms, credentials isolation and fail-closed no-silent-fallback | [OQ-008](open-questions.md), [model access](../architecture/model-access-entitlement-contract.md) |
| 6 | First worker regions, images, cost ceilings and safe cancellation | **P1 Open — evidence and permitted benchmark spend needed** | Measured startup/billing/cancellation/isolation per Cloud Run Jobs, Hetzner and remote VPS | [OQ-010](open-questions.md), [ADR 0018](../decisions/0018-initial-worker-adapters.md) |
| 7 | GitHub App permission matrix and verified merge controls | **P1 Open — configuration and security tests needed** | Least-privilege installation, signed webhooks, protected ref and human merge authority checks | [Git contract](../architecture/git-remote-integration-contract.md) |
| 8 | Worker timing/health, scheduler fairness, context and model selection | **P1 Open — only remaining algorithms/benchmarks** | Approved 15-min token / 30-sec heartbeat / 90-sec lease and 3 total worker attempts preserved; retry delays, escalation, fairness weights, load and token/cost benchmarks remain | [OQ-011/012/020](open-questions.md) |
| 9 | Backup recovery targets and telemetry/alert delivery | **P1 Open — measured restores and policy needed** | RPO/RTO, key handling, legal retention, restore drills and reliable Owner alerts | [OQ-014/016](open-questions.md) |
| 10 | Web UI stack, approval UX, test/CI gates and MIT notice checks | **P1 Open — release contract and tests needed** | UX/permission acceptance, CI security tests, reviewed versioned release | [OQ-013/015](open-questions.md) |
| 11 | Storage migration jitter and provider-specific protocol mechanics | **Engineering parameter open; not a blocker for fake-only orchestrator demonstration** | Bounded jitter algorithm + provider cool-down, persistence and reconciliation tests; 4 total attempts and 5/10/20-second base waits already approved | [ADR 0030](../decisions/0030-project-activity-audit-log.md) |
| 12 | Original approved logo master and trademark/brand asset policy | **P2 Open; not a fake-core blocker** | Provenance-verified source asset and explicit asset-use decision | [OQ-019](open-questions.md) |

**No autonomous approval:** Close a consequential P0/P1 product or security decision only after explicit Owner confirmation and record the decision in the canonical ADR and linked tests. Do not silently change approved immutable DAG, no-local-machine, hard-budget or MIT policies. Real cloud/model benchmarks require explicit spend authorization.

## Full-project review — to perform after gap closure

Review and reconcile, in this order:

1. Product mission, positioning, MIT/branding, primary users, boundaries, MVP and explicitly deferred features.
2. Trust/security and authorization: human identity, project roles, Owner control, worker identity, secrets, tenant boundaries.
3. Main agent/planning contract: entire hierarchical DAG planned and frozen up front; groups vs leaves; task model/effort selection; memory/token budgets.
4. Scheduler: readiness, dependency validation, multi-provider placement, dynamic parallelism, fairness, leases, retries and failure recovery.
5. Economic correctness: cost scenarios/categories, financial reservations, unknown/late bills, hard approval gate, no percentage overrun.
6. Cloud workers, legal CLI/API entitlement, HTTPS protocol, provider lifecycle, resource/cancellation safety.
7. Remote Git: GitHub App, repo permissions, write isolation, branch protections, verified results.
8. Rust/SQLite storage, API schema, WAL/busy/backup setup, audit records, integrity checkpoints, historical provider migration.
9. Panel UX: project graph, worker progress, budgets, Owner approvals, Activity Log and incident alerts.
10. Delivery: fake-provider prototype, real integrations, test/CI evidence, performance, security review, operations, release, MIT notices.
11. Decision traceability: each approved ADR vs open question, requirement, acceptance test, owner approval and actual implementation proof.

**Review exit criteria:** present a consolidated overview, end-to-end execution example, component diagram, all decision statuses, unresolved technical risks, budget assumptions, and milestone/acceptance map. Distinguish **Approved**, **Proposed**, **Implemented**, and **Verified** explicitly. Do not certify historical chat completeness without the original transcripts/assets.

## Implementation readiness ordering

Start with Owner-approved MVP scope and P0 contracts, then safe isolated remote Rust/SQLite synthetic tests. Do not require a live provider or GitHub App installation merely to prove DAG parsing and budget invariants. Do not represent this document as executable code, a running task DAG, or completed acceptance evidence.

Related: [status](status.md), [open questions](open-questions.md), [roadmap](roadmap.md), [implementation slices](implementation-slices.md), [initial engineering tasks](../development/initial-implementation-task-packages.md), [acceptance criteria](../product/acceptance-criteria.md).
