# Decision register

This register separates **approved user decisions** from **earlier design concepts** and unresolved choices. It is not a claim that the platform has been implemented.

## Confirmed decisions (2026-10-20 checkpoint)
| ID | Status | Decision | Implication |
| --- | --- | --- | --- |
| D-001 | Approved | Rolvya name, accepted logo and navy/blue/gray visual identity; no redesign requested. | Preserve brand identity. |
| D-002 | Approved | Public `ygtdmr/rolvya` repository, default branch `main`. | GitHub-hosted project source of truth. |
| D-003 | Approved | GNU MIT license. | Do not silently replace with MIT or another license. |
| D-004 | Approved | "Open-source orchestration for AI coding agents, with dependency-aware task planning and parallel execution." | Approved short description. |
| D-005 | Approved | "One Plan. Many Agents." tagline. | Use sparingly; not repeatedly in one document. |
| D-006 | Approved | Upfront dependency-aware DAG task planning and parallelization of independent tasks. | Scheduler must enforce dependencies. |
| D-007 | Approved | Entire coding implementation by AI agents, under human architectural and priority direction. | Agent-continuable, complete documentation and evidence-based handoffs. |
| D-008 | Approved | Budget is a hard approval gate. | Pause new billable tasks when funds insufficient; require explicit increase; preserve completed work; no percentage overrun. |
| D-009 | Approved | Cost planning separates AI API, cloud workers, storage and transfer, with optimistic/expected/pessimistic scenarios. | Design estimators around these categories. |
| D-010 | Approved | No Local Agent, Local Sync, or direct access/synchronization with Git repositories on user's personal machine. | Support remote Git services rather than personal-computer access. |
| D-011 | Approved 2026-10-21 | Backend implementation language is **Rust only**. | All Rolvya-owned backend services and logic must be Rust; see [ADR 0004](0004-control-plane-language.md). |
| D-012 | Approved 2026-10-21 | Authoritative database engine is **SQLite only**. | Rust persistence layer and remote-worker API boundary; see [ADR 0005](0005-authoritative-database.md). |
| D-013 | Approved 2026-10-21 | Worker control communication is worker-initiated **HTTPS pull/claim**. | Authenticated remote workers, durable SQLite outbox; [ADR 0006](0006-worker-communication.md). |
| D-014 | Approved 2026-10-21 | **GitHub App** is the GitHub repository authorization method. | Rust trusted publisher, scoped installation, protected refs; [ADR 0008](0008-github-authentication.md). |
| D-015 | Approved 2026-10-21 | Remote worker architecture supports **multiple provider adapters**. | No cloud lock-in; common provider-neutral Rust interfaces; [ADR 0011](0011-multi-provider-worker-infrastructure.md). |

## Previously explored architecture; validate before treating details as binding
- Main orchestrator decomposes hierarchical tasks, selects AI models and effort levels, manages queue, dependencies and contextual memory.
- Worker scheduler considers priority, duration, CPU/RAM, model constraints, capacity, costs and region/latency.
- Cloud worker candidates included AWS ECS Fargate, Google Cloud Run, and separately managed remote servers; a Rust orchestrator on a small VPS was depicted in an earlier diagram.
- Planned flow: queue → parallel workers → results and parent-task aggregation → final tests and validation.
- Candidate operational concerns: worker lifecycle, health, authentication, retries, logs, metrics and cost reporting.
- An earlier diagram referenced a local VPS and an illustrative 20-worker limit. These are **not confirmed production limits**, and a VPS-hosted orchestrator does **not** authorize a personal-computer Local Agent.

## Outstanding decisions / research
- Detailed product flows, Rust framework/crates and UI stack, SQLite physical schema/WAL configuration, persistence transaction tuning and data isolation. Rust and SQLite themselves are **approved**, not open choices.
- Graph/task implementation contracts, scheduler fairness, **multi-provider adapter implementation**, failure/retry/idempotency behavior.
- Initial remote provider lineup/region and exact per-provider identity, permissions and cost caps (multi-provider architecture is Approved).
- Concrete authentication/authorization, secrets, threat model, API schemas, budget enforcement and approval UX.
- Roadmap milestones, test strategy, support and release process.
- Legal/commercial implications of MIT and a separate future trademark policy. MIT does not generally prohibit commercial competing use; brand restrictions must not be falsely described as MIT requirements.

## Detailed approved ADRs

- [ADR 0001 — Fixed upfront DAG and worker roles](0001-fixed-upfront-dag.md)
- [ADR 0002 — Hard project budget approval gate](0002-hard-budget-approval.md)
- [ADR 0003 — Remote Git only](0003-remote-git-only.md)

See the [historical conversation audit](../project/historical-decision-audit.md) for a topic-by-topic reconciliation of available prior discussions, including exploratory cloud options and brand/legal concerns.

## Technology ADRs — approved and pending

**Approved technology decisions:**

- [ADR 0004 — Rust-only backend](0004-control-plane-language.md) — **Approved 2026-10-21**.
- [ADR 0005 — SQLite authoritative database](0005-authoritative-database.md) — **Approved 2026-10-21**.
- [ADR 0006 — Worker-initiated HTTPS control](0006-worker-communication.md) — **Approved 2026-10-21**.
- [ADR 0008 — GitHub App authorization](0008-github-authentication.md) — **Approved 2026-10-21**.
- [ADR 0011 — Extensible multi-provider remote workers](0011-multi-provider-worker-infrastructure.md) — **Approved 2026-10-21**.

**Additional owner-approved operational ADRs (2026-10-21):**

- [ADR 0009 — Group-only parent tasks and verified aggregation](0009-parent-aggregation.md) — **Approved**.
- [ADR 0010 — Bounded in-flight worker execution or stop-on-risk](0010-inflight-budget.md) — **Approved**.

**Additional owner-approved REST, storage and execution policies (2026-10-22):**

- [ADR 0012 — Axum REST API](0012-rest-api-framework.md).
- [ADR 0013 — SQLx SQLite access](0013-sqlite-access-library.md).
- [ADR 0014 — Initial SQLite WAL and pool settings](0014-sqlite-initial-configuration.md).
- [ADR 0015 — Encrypted remote backup and restoration policy](0015-backup-recovery.md).
- [ADR 0016 — S3-compatible storage adapter first](0016-s3-first-backup-adapter.md).
- [ADR 0017 — Three remote worker host categories](0017-worker-host-categories.md).
- [ADR 0018 — Cloud Run Jobs, Hetzner Cloud and user remote VPS adapter priorities](0018-initial-worker-adapters.md).
- [ADR 0019 — Centralized retries](0019-centralized-worker-retries.md).
- [ADR 0020 — Provider-aware isolation](0020-provider-aware-worker-isolation.md).
- [ADR 0021 — Economical / Balanced / Performance scheduler modes](0021-scheduler-selection-modes.md).
- [ADR 0022 — Dynamic concurrency and optional user cap](0022-dynamic-worker-concurrency.md).
- [ADR 0023 — Cross-project fairness, preflight priority and queue aging](0023-cross-project-scheduling-fairness.md).
- [ADR 0024 — Owner, Editor and Viewer project roles](0024-project-human-roles.md).
- [ADR 0025 — OIDC-first sign-in, passkey support and strong sessions](0025-oidc-passkey-authentication.md).
- [ADR 0026 — Secure account recovery and Owner protection](0026-account-recovery.md).
- [ADR 0027 — Risk-based session management](0027-risk-based-session-management.md).

These are **Approved designs, not claims of implemented or production-verified software**. Detailed region/cost/adapter validation remains separate.

**Remaining proposed provider rollout decision:**

- [ADR 0007 — Initial remote worker providers, regions and cost comparison](0007-remote-worker-hosting.md) — **Proposed**; it does not reopen approved multi-provider support.


Selection gates, unresolved questions and evidence requirements are in the [open-decision register](../project/open-questions.md). Implementation order after approval is in the [proposed implementation slices](../project/implementation-slices.md).

## ADR process
Create `NNNN-short-decision-title.md` for consequential decisions. Include context, options, chosen decision, owner/approval date, consequences, rejected alternatives, related requirements, migration notes and supersession links. Never mark an item approved just because an earlier assistant suggested it.

Review this register against original conversations and diagrams before claiming historical coverage is complete.
