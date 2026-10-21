---
title: Open Decisions and Architecture Questions
status: Open
category: project
last-reviewed: 2026-10-21
tags: [decisions, roadmap, unresolved, adr]
---

# Open decisions and architecture questions

## Purpose and interpretation

This register tracks **unresolved choices** that must not be silently converted into implemented requirements. It is a planning instrument, not evidence a particular provider, framework or algorithm has been approved. Approved constraints remain in the [decision register](../decisions/README.md), [requirements](../product/requirements.md) and numbered ADRs.

Each question should ultimately have an owner decision, dated rationale, alternative evaluation, security/cost consequences and a linked ADR or requirement update.

## Priority classification

**P0:** needed before implementation depends on the choice, particularly for security, persistence, authorization and budget integrity. **P1:** needed before connected functionality or first release. **P2:** may remain a documented deferred enhancement. These priority labels are proposed for review.

| ID | Priority | Question | Candidate directions / decision criteria | Related specification |
| --- | --- | --- | --- | --- |
| OQ-003 | P0 | SQLite-atomic task lease + budget reservation transaction? | Serialized SQLite write transaction, idempotent outbox, fencing epochs, delayed charges | [Scheduler](../architecture/scheduler.md) |
| OQ-004 | P0 | Authentication, project roles and authorization? | Role policy, audit identity, tenant separation, human budget approval | [Security](../architecture/security.md) |
| OQ-005 | P0 | Secrets storage and remote worker identity? | Secret-manager references, ephemeral worker identity, revocation, sandbox boundary | [Secrets](../operations/secrets.md) |
| OQ-006 | P0 | Worker communication protocol and queue transport? | HTTP polling, RPC, queues; leases, late callbacks, duplication and failure testing | [Worker protocol](../architecture/worker-protocol.md) |
| OQ-007 | P0 | GitHub/provider authentication and merge authority? | GitHub App vs delegated OAuth; least privilege, isolated branches, review gates | [Git integration](../architecture/integration-github.md) |
| OQ-008 | P0 | Provider-valid CLI subscription versus API access rules? | Terms, supported automation, tokens and billing by provider/model | [Model access](../architecture/model-access.md) |
| OQ-010 | P1 | Cloud worker providers and container/sandbox setup? | Cost vs startup/CPU isolation, reliability, region and user-provided remote servers | [Infrastructure](../operations/infrastructure-options.md) |
| OQ-011 | P1 | Worker lease TTL, heartbeat/retry and timeouts? | Benchmark-backed defaults, fenced attempts, quotas and non-idempotent writes | [Recovery](../architecture/failure-recovery.md) |
| OQ-012 | P1 | Context packaging, token quotas and task handoff format? | Provider-specific adapters, compact references, provenance, truncation detection | [Context](../architecture/context-memory.md) |
| OQ-013 | P1 | UI stack, roles and plan approval interactions? | Budget approval security, task graph readability and accessibility | [User flows](../product/user-flows.md) |
| OQ-014 | P1 | Backup RPO/RTO, encryption and retention? | Risk to approvals/ledger/artifacts, restore exercises, storage cost | [Backups](../operations/backup-recovery.md) |
| OQ-015 | P1 | CI merge gates, review roles and release process? | Test scope, human security signoff, branch protection, MIT compliance | [Review](../development/review-process.md) |
| OQ-016 | P1 | Alert delivery, monitoring stack and audit retention? | Low overhead, integrity, tenant privacy, noise controls | [Observability](../architecture/observability.md) |
| OQ-018 | P2 | Non-GitHub self-hosted remote Git adapters? | Provider compatibility and authorization, never local computer sync | [Git integration](../architecture/integration-github.md) |

## Recently approved execution policies — no longer open

- **OQ-009 closed 2026-10-21:** [ADR 0010 — Approved](../decisions/0010-inflight-budget.md). Already-running billable attempts continue only within verifiably enforceable pre-reserved exposure; if full exposure cannot safely be bounded, initiate stop-on-risk, fence/cancel and reconcile. Provider-specific caps, cancellation timing, billing lag and refund rules remain implementation details.
- **OQ-017 closed 2026-10-21:** [ADR 0009 — Approved](../decisions/0009-parent-aggregation.md). Parent nodes are non-executable groups; only planned executable leaves receive workers. Verified required child outputs and group acceptance drive success. Optional-child semantics and group predicate serialization remain open implementation details.

## Approved technology choices — no longer open

- **OQ-001 closed 2026-10-21:** Rust is the **only** Rolvya backend language ([ADR 0004 — Approved](../decisions/0004-control-plane-language.md)). Rust frameworks/crates and performance benchmarks remain open implementation details.
- **OQ-002 closed 2026-10-21:** SQLite is the **only** authoritative database engine ([ADR 0005 — Approved](../decisions/0005-authoritative-database.md)). SQLite transaction strategy, WAL configuration, schema, locking, backups and RPO/RTO are not yet approved.

## Candidate decision packages (not approved)

| Open question | Proposed ADR | Evidence required before owner approval |
| --- | --- | --- |
| OQ-003 | [Approved ADR 0005: SQLite engine](../decisions/0005-authoritative-database.md) and [backend contract](../architecture/rust-sqlite-backend.md) | Test SQLite transactional lease/reservation, WAL/busy behavior, crash replay and ledger consistency; engine choice is closed |
| OQ-006 | [ADR 0006: transport](../decisions/0006-worker-communication.md) | Remote HTTPS-pull vs alternatives under partitions, retries, auth and simulated load |
| OQ-010 | [ADR 0007: hosting](../decisions/0007-remote-worker-hosting.md) | Real workloads on two provider modes, regional billable minimums and full cost categories |
| OQ-007 | [ADR 0008: GitHub access](../decisions/0008-github-authentication.md) | Verified permission matrix, revocation, isolated refs and owner review |

The [implementation slice proposal](implementation-slices.md) explicitly blocks production coding on unresolved critical choices. These ADRs have `status: Proposed` until explicit owner authorization is recorded.

## Detailed evaluation specifications

- [Budget authorization and provider invoice reconciliation](../architecture/budget-reservation-reconciliation.md) implements the **Approved** bounded-or-stop running-work policy conceptually; actual provider limits and accounting details remain open.
- [Worker transport design](../architecture/worker-transport-design.md) proposes message shapes and failure tests. It does **not** approve HTTPS as the final transport.
- [Identity and authorization](../architecture/identity-authorization.md) describes proposed trust roles and explicit human approvals.
- [GitHub App integration design](../architecture/github-app-integration-design.md) distinguishes repository token permissions from protected-branch write enforcement.
- [Remote worker provider benchmark](../operations/worker-provider-evaluation.md) specifies cost and reliability comparisons with provider-side automatic retry checks.
- [Decision readiness checklist](decision-readiness.md) shows approved vs pending decisions and implementation entry gates.

## Explicit implementation readiness proposals

- [Plan activation/version boundaries](../architecture/plan-activation-and-versioning.md): freezes complete planned task DAG and records a scope-change escalation process; version-supersession consent remains open.
- [Parent group aggregation implementation contract](../architecture/parent-task-aggregation.md) and [Approved ADR 0009](../decisions/0009-parent-aggregation.md): group-only parents selected; optional-child and schema details remain open.
- [In-flight execution implementation contract](../architecture/inflight-cost-policy.md) and [Approved ADR 0010](../decisions/0010-inflight-budget.md): enforceably bounded continuation or stop-on-risk selected; provider-specific caps and stop timing remain open.
- [Initial Rust/SQLite engineering task packages](../development/initial-implementation-task-packages.md) and [test matrix](../development/rust-sqlite-test-matrix.md): proposals for future planning; not activated worker task nodes, not tested code.

## Decisions that are not open

These have already been approved and must not be reopened by a worker without owner consent: Rust-only backend and SQLite-only authoritative database; non-executable group parents and independently validated aggregate success; running workers limited to pre-reserved enforceable exposure or stop-on-risk; one main agent creates a fully fixed task hierarchy and DAG before runtime; workers execute, not plan; dependencies govern parallelism; hard budget pause with explicit approval and no percentage overrun; preserve completed work; separate cost categories/scenarios; remote Git only; no Local Agent or personal-computer sync; GNU MIT license.

## How to resolve a question

1. Gather up-to-date technical evidence and, when applicable, provider terms/pricing; record sources and assumptions.
2. Evaluate at least realistic alternative and rejected direction against cost, RAM/CPU, reliability, security and complexity.
3. Propose an ADR with impact, migration risk and acceptance tests.
4. Obtain explicit human approval when consequential; preserve approval actor/date.
5. Update the decision register, requirement IDs, specs and tests together.
6. Implement only through an authorized planned task; do not mutate the active DAG to resolve a choice.

## Coverage limitations

Previous conversation summaries and diagrams may omit specific wording. The [historical decision audit](historical-decision-audit.md) documents the currently available evidence and its limitations; missing provenance must be marked unknown, not invented.

Related: [documentation index](../README.md), [project status](status.md), [review process](../development/review-process.md), [decision register](../decisions/README.md).
