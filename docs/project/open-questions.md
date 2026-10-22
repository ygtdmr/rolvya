---
title: Open Decisions and Architecture Questions
status: Open
category: project
last-reviewed: 2026-10-22
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
| OQ-008 | P0 | Provider-valid subscription CLI vs API usage on unattended third-party remote workers? | Check official current terms, owner entitlement, secure CLI/OAuth delegation, no silent paid fallback | [CLI/API entitlement contract](../architecture/model-access-entitlement-contract.md), [dated official evidence](../reference/provider-cli-api-policy-evidence.md) |
| OQ-010 | P1 | Which first-provider **regions, images, sandbox settings and cost guarantees** are viable? | First adapter priorities **Approved ADR 0018**: Cloud Run Jobs, Hetzner Cloud and user remote VPS; validate each one's actual cost bounds, isolation and billing | [ADR 0018](../decisions/0018-initial-worker-adapters.md), [ADR 0007](../decisions/0007-remote-worker-hosting.md) |
| OQ-011 | P1 | Worker lease TTL, heartbeat/retry limits/timeouts and model/resource escalation? | Historical 1/3/10 minute & three-attempt graphic is nonbinding; test idempotency, provider costs, fair retry policy | [Retry and escalation](../architecture/retry-escalation-policy.md) |
| OQ-012 | P1 | Planner and worker context packaging, token quotas and handoffs? | Minimize tokens in **both** phases, retain required constraints, version hashes and prior verified artifacts | [Token efficiency](../architecture/token-context-efficiency.md) |
| OQ-013 | P1 | UI stack, roles and plan approval interactions? | Budget approval security, task graph readability and accessibility | [User flows](../product/user-flows.md) |
| OQ-014 | P1 | Backup RPO/RTO, encryption and retention? | Risk to approvals/ledger/artifacts, restore exercises, storage cost | [Backups](../operations/backup-recovery.md) |
| OQ-015 | P1 | CI merge gates, review roles and release process? | Test scope, human security signoff, branch protection, MIT compliance | [Review](../development/review-process.md) |
| OQ-016 | P1 | Alert delivery, monitoring stack and audit retention? | Low overhead, integrity, tenant privacy, noise controls | [Observability](../architecture/observability.md) |
| OQ-018 | P2 | Non-GitHub self-hosted remote Git adapters? | Provider compatibility and authorization, never local computer sync | [Git integration](../architecture/integration-github.md) |

| OQ-019 | P2 | Which exact accepted logo source file/SVG, final no-gold HEX values and brand-use policy are authoritative? | Obtain original approved asset and provenance; avoid redrawing, superseded brass and trademark assumptions | [Brand history](../product/brand-history-license-governance.md) |
| OQ-020 | P1 | Which *numeric* priority/aging weights, fairness algorithm, provider fallbacks and measured backpressure thresholds are safe? | [ADR 0022](../decisions/0022-dynamic-worker-concurrency.md) **Approved** dynamic concurrency; [ADR 0023](../decisions/0023-cross-project-scheduling-fairness.md) **Approved** cross-project fair sharing and aging. Only algorithm details, fairness parameters and thresholds are open | [Selection policy](../architecture/scheduler-selection-policy.md) |

## Recently approved remote integration architecture — no longer open

- **OQ-006 closed 2026-10-21:** [ADR 0006 — Approved](../decisions/0006-worker-communication.md). Worker control uses remote-worker-initiated HTTPS pull/claim/heartbeat/result with SQLite durable outbox; exact Rust library, authentication, payload and timing remain implementation choices.
- **OQ-007 (GitHub auth selection) closed 2026-10-21:** [ADR 0008 — Approved](../decisions/0008-github-authentication.md). GitHub integration uses GitHub App installations with a trusted Rust Git publisher. Exact endpoint grants, merge authority and owner permissions remain open security details covered by OQ-004/005/015.
- **Multi-provider architecture Approved:** [ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md) requires extensible Rust adapters supporting different remote providers, not a single cloud implementation. **OQ-010 remains open for regional/deployment settings, provider setup, billing evidence and benchmarking; the first three adapter priorities were approved in ADR 0018.**

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
| OQ-010 | [ADR 0007: first provider rollout](../decisions/0007-remote-worker-hosting.md) with [Approved ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md) | Benchmark initial provider adapters on billable minimums, latency, isolation, region and four cost classes; extensibility itself is settled |

The [implementation slice proposal](implementation-slices.md) explicitly blocks production coding on unresolved critical choices. ADR 0007 remains `status: Proposed` for initial provider rollout. ADR 0006 (HTTPS), ADR 0008 (GitHub App) and ADR 0011 (multi-provider) are now `status: Approved`; only their concrete contracts/enrollment and security details remain to be implemented.

## Decision package for remaining technology ADRs

The [transport / Git / worker hosting decision pack](transport-git-hosting-decision-pack.md) records **Approved ADR 0006 (HTTPS), ADR 0008 (GitHub App) and ADR 0011 (multi-provider architecture)**, while tracking the **still-open OQ-010 / Proposed ADR 0007** initial provider rollout. Its associated drafts are the [versioned worker wire contract](../architecture/worker-wire-contract-draft.md), [remote Git publisher contract](../architecture/git-remote-integration-contract.md), [worker lifecycle and cost reconciliation](../operations/remote-worker-lifecycle-contract.md) and [remote integration test suite](../development/remote-integration-test-matrix.md). Their detailed wire/configuration schemas remain **Proposed**, while the core transport/authentication/multi-provider architecture choices are **Approved**. Paid benchmarks require independent explicit test-budget authorization.

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

## Dynamic worker concurrency — now Approved

[ADR 0022](../decisions/0022-dynamic-worker-concurrency.md): no arbitrary fixed worker limit; optional owner-selected per-project concurrency ceiling at preflight, otherwise adapt to independently ready leaves, live quotas, security, model entitlement, SQLite controller capacity and conservative authorized spending. Exact thresholds, fair-share and control-loop tuning remain open engineering parameters; do not re-ask the product-policy choice.

## Cross-project fairness — now Approved

[ADR 0023](../decisions/0023-cross-project-scheduling-fairness.md): large ready queues must not indefinitely starve other eligible projects on **authorized shared pools**. Project preflight priority is respected, waiting work gains consideration over time, and per-project security, quotas and budget remain independent. Exact weighted fairness/aging algorithm is still open and must be measured.

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

Previous conversation summaries and diagrams may omit specific wording. The [historical decision audit](historical-decision-audit.md) and [2026-10-22 two-conversation evidence matrix](two-conversation-coverage-audit.md) document recovered evidence and its limits; full original chat exports were **not accessible** for literal message-by-message verification. Missing provenance must be marked unknown, not invented.

Related: [documentation index](../README.md), [project status](status.md), [review process](../development/review-process.md), [decision register](../decisions/README.md).
