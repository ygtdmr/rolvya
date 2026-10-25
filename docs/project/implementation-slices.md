---
title: Evidence-Driven Implementation Slices
status: Proposed
category: project
last-reviewed: 2026-10-21
tags: [roadmap, planning, tests, implementation]
---

# Evidence-driven implementation slices

## Purpose and caveat

Translate current Rolvya architecture and accepted constraints into an **ordered implementation proposal** that an AI coding agent can execute after the owner approves the relevant technology ADRs. This is a **project development roadmap**, **not** the immutable task DAG for any specific user project. Creating future development milestones does not authorize runtime generation of new tasks inside a Rolvya execution plan.

No code implementation, worker deployment, provider integration or passing CI is implied by the presence of this document.

The [decision readiness checklist](decision-readiness.md) separates approved architecture from transport/provider/security approvals, and the [worker provider evaluation](../operations/worker-provider-evaluation.md) specifies billable benchmark requirements.

**Updated approved choices (2026-10-21):** [ADR 0006](../decisions/0006-worker-communication.md) fixes worker-initiated HTTPS; [ADR 0008](../decisions/0008-github-authentication.md) fixes GitHub App; [ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md) requires extensible multi-provider Rust adapters. **[ADR 0041](../decisions/0041-aws-fargate-first-real-worker.md) already selects AWS ECS Fargate as the first real adapter**, after synthetic workers; [ADR 0007](../decisions/0007-remote-worker-hosting.md) is an earlier historical exploration. Provider regions and benchmark numbers are engineering validation, not an open provider-selection decision. See [multi-provider adapter contract](../architecture/multi-provider-adapter-contract.md). Exact identity, permissions and wire/configuration remain implementation choices.

## Latest approved first working provider (2026-10-24)

[ADR 0041](../decisions/0041-aws-fargate-first-real-worker.md) refines [ADR 0040](../decisions/0040-single-provider-first-working-version.md): start with synthetic fake workers, then **Amazon ECS on AWS Fargate** as the first real remote-worker adapter, add **Google Cloud Run Jobs** and user-enrolled remote VPS later. The provider-neutral Rust interfaces, approved MVP feature requirements, explicit financial approval and verified CLI/API account entitlements remain compulsory. **No live paid AWS testing or provider provisioning has been approved.**

## Mandatory foundations before implementation

1. **Rust-only backend and SQLite database are approved** by [ADR 0004](../decisions/0004-control-plane-language.md) and [ADR 0005](../decisions/0005-authoritative-database.md). Worker-initiated HTTPS is already approved in [ADR 0006](../decisions/0006-worker-communication.md) and GitHub App authentication in [ADR 0008](../decisions/0008-github-authentication.md). Initial provider categories and priorities are approved in [ADRs 0017–0018](../decisions/README.md); concrete provider regions, budget limits and identity/wire implementation remain open in the [decision register](open-questions.md).
2. **Implement Approved ADR 0010**: previously admitted workers can continue only within enforceably bounded pre-reserved exposure, otherwise stop-on-risk; assess actual provider-side bill limits and delayed invoices. Approved invariant: **no percentage overrun, explicit human approval for higher ceiling**.
3. **Implement and verify previously approved** security/identity and Git merge authority policies, including tenant isolation; seek authorization only for actual privileged actions, outside-provider eligibility or changes to approved constraints.
4. Establish a sandboxed remote development/test environment using fake provider adapters first; no user's local computer worker or repository access.
5. Define measurable acceptance/benchmark thresholds and capture exact toolchain versions in the handoff.

See the [Rust + SQLite backend implementation contract](../architecture/rust-sqlite-backend.md) for approved engine boundaries, atomic admission requirements and proposed SQLite lifecycle. The [SQLite operations](../operations/sqlite-operations.md) document covers the safety and recovery requirements.

## Suggested dependency-aware sequence

```mermaid
flowchart TD
  A[Approve remaining critical transport/identity/security ADRs] --> B[Rust domain types and immutable DAG validator]
  A --> C[Persistence, audit and transactional budget ledger]
  B --> D[Deterministic scheduler and task state machine]
  C --> D
  C --> E[Worker lease protocol and fake remote adapter]
  D --> F[Remote fake-worker end-to-end orchestration]
  E --> F
  F --> G[Real isolated worker provider and model mode adapter]
  F --> H[Remote Git provider sandbox and integration verification]
  G --> I[Combined model, cost and recovery tests]
  H --> I
  I --> J[Owner panel and approval workflows]
  J --> K[Security, backup and deployment hardening]
```

The diagram's edges are **development dependencies**, not an actual activated Rolvya task graph. Parallelize nonconflicting implementation work after prerequisites are approved and isolated.

Candidate packages R00–R11, with explicit future agent handoffs, are in [initial Rust/SQLite engineering tasks](../development/initial-implementation-task-packages.md); acceptance/fault cases T-001–T-030 are in [test matrix](../development/rust-sqlite-test-matrix.md). These are **not** an activated Rolvya DAG. **Group-only parent aggregation** ([Approved ADR 0009](../decisions/0009-parent-aggregation.md)) and **bounded-or-stop running worker spending** ([Approved ADR 0010](../decisions/0010-inflight-budget.md)) are mandatory. Optional-child behavior, provider-specific limits and detailed implementation interfaces remain open.

## Slices and acceptance gates

| Slice | Outcome | Minimum evidence to complete |
| --- | --- | --- |
| S0 — Decision gates | Rust + SQLite recorded as approved; owner-approved remaining transport/hosting/identity ADRs, threat boundaries and benchmark envelope | Explicit approval history, reasons, known costs and rejected alternatives |
| S1 — Rust core domain & validator | Rust project/plan/task/edge types; immutable active DAG and graph checks | Reject cycles, unknown IDs, mutable active nodes; hierarchy not equal dependency |
| S2 — SQLite persistence & budget ledger | SQLite schema/migrations, durable attempts, events, approvals and atomic admission | Simultaneous reservations never exceed approved headroom; crash replay safe |
| S3 — Deterministic scheduler | Ready queue, dependency guards, attempt state and fencing | Independent tasks parallel-eligible; dependents wait for validation |
| S4 — Remote fake-worker protocol | Authenticated lease, heartbeat, results, idempotency using fake executors | Stale/duplicate callback rejected; simulated partition recovery |
| S5 — Minimal E2E run | Test project with 3–5 tasks across two branches of a DAG | Verified artifacts, source hashes, attempt histories and accurate cost state |
| S6 — Real remote executor adapter | Launch/cleanup on selected remote provider, model mode validation | CPU/RAM profile, correct cleanup, externally measured bill/cost exposure |
| S7 — Remote Git sandbox | Install/connect one approved remote provider, isolate refs, conflict gating | Ref provenance, stale-base rejection, no local desktop sync |
| S8 — Resilience & cost reconciliation | Recovery from ambiguous push, process loss, delayed billing | No duplicate remote writes/charges; preserved sibling work |
| S9 — Owner panel & hard approvals | Readable DAG, task attempts, budget and explicit ceiling increase | Denial preserves pause; approval audited; no automatic overrun |
| S10 — Release hardening | CI, adversarial tests, backup/restore, privacy, docs and packaging | Independent verification, tests with exact environment, rollback procedure |

All slice names are **proposed engineering breakdown**, not runtime task nodes. Each coding job inside a slice must be described with task goal, scope, model/effort, accepted context, time estimate, budget exposure and acceptance evidence.

## Incremental demo definitions

- **Demo A (after S1–S3):** single control process can validate a DAG and safely schedule simulated tasks without live providers.
- **Demo B (after S4–S5):** two fake remote workers complete independent tasks; a dependent task waits; logs/verified results survive restart.
- **Demo C (after S6–S8):** isolated remote worker makes reviewable changes to a throwaway remote Git repository with hard budget reservation and reconciliation.
- **Demo D (after S9–S10):** owner sees task graph and costs, approves or denies budget increase, and inspects verified artifacts and audit evidence.

## Evidence pack per slice

Record requirement/acceptance IDs from [PR/AC inventory](../product/requirements.md) and [acceptance criteria](../product/acceptance-criteria.md), code commit, remote branch, toolchain/OS, exact test commands/results, workload seed/CPU/RAM, simulated vs invoiced costs, redacted logs, changed docs, risks, unresolved ADRs and reviewer signoff. Follow [agent handoff](../development/agent-handoffs.md) and [review process](../development/review-process.md).

## Stop/go conditions

Do not advance from fake providers to paid live cloud/model testing without explicit authorized spend and a confirmed provider access mode. Do not mark S9 completed if a budget notification can bypass owner approval. Do not publish a release if migration, security, rollback or artifact provenance remains unverified.

## Known gaps

Formal timeline, remaining owner choices, framework dependency versions, SQLite schema and unapproved durability/checkpoint tuning, tenant model, performance SLOs, exact API schema, provider-specific retries/timeouts, official CLI automation permissions and provider credentials remain unresolved. **Axum, SQLx and the initial WAL/pool/busy-timeout settings are already Approved** (ADRs 0012–0014); do not reopen these. **Rust and SQLite choices are fixed.** Track them in [open questions](open-questions.md).

Related: [documentation index](../README.md), [project status](status.md), [open questions](open-questions.md), [architecture overview](../architecture/overview.md), [testing](../development/testing.md).
