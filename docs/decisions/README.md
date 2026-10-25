# Decision register

This register separates **approved user decisions** from **earlier design concepts** and unresolved choices. It is not a claim that the platform has been implemented.

## Confirmed decisions (current project policy)
| ID | Status | Decision | Implication |
| --- | --- | --- | --- |
| D-001 | Approved | Rolvya name, accepted logo and navy/blue/gray visual identity; no redesign requested. | Preserve brand identity. |
| D-002 | Approved | Public `ygtdmr/rolvya` repository, default branch `main`. | GitHub-hosted project source of truth. |
| D-003 | Approved | MIT License (SPDX: `MIT`) is the project source license. | Include copyright and permission notice; further changes require explicit owner approval. |
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
- Detailed product flows, remaining Rust crate versions and UI stack, SQLite physical schema, `synchronous`/checkpoint tuning, persistence implementation and data isolation. **Rust, SQLite, Axum, SQLx, initial WAL/pool/busy defaults and `BEGIN IMMEDIATE` admission are already approved** (ADRs 0004/0005/0012–0014/0031).
- Graph/task implementation contracts, scheduler fairness, **multi-provider adapter implementation**, failure/retry/idempotency behavior.
- Initial remote provider lineup/region and exact per-provider identity, permissions and cost caps (multi-provider architecture is Approved).
- Concrete authentication/authorization, secrets, threat model, API schemas, budget enforcement and approval UX.
- Roadmap milestones, test strategy, support and release process.
- Legal implications of MIT licensing and future trademark/brand-use policy. MIT permits commercial reuse of code; trademark rights are distinct.

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
- [ADR 0018 — Current initial adapter priorities: **AWS ECS Fargate, Google Cloud Run Jobs, remote VPS**](0018-initial-worker-adapters.md) — after Hetzner replacement [ADR 0039](0039-aws-ecs-fargate-replaces-hetzner.md) and AWS-first [ADR 0041](0041-aws-fargate-first-real-worker.md).
- [ADR 0019 — Centralized retries](0019-centralized-worker-retries.md).
- [ADR 0020 — Provider-aware isolation](0020-provider-aware-worker-isolation.md).
- [ADR 0021 — Economical / Balanced / Performance scheduler modes](0021-scheduler-selection-modes.md).
- [ADR 0022 — Dynamic concurrency and optional user cap](0022-dynamic-worker-concurrency.md).
- [ADR 0023 — Cross-project fairness, preflight priority and queue aging](0023-cross-project-scheduling-fairness.md).
- [ADR 0024 — Owner, Editor and Viewer project roles](0024-project-human-roles.md).
- [ADR 0025 — Conditional OIDC/passkey security extension](0025-oidc-passkey-authentication.md) — federation not mandatory in initial v0.1 (ADR 0033).
- [ADR 0026 — Secure account recovery and Owner protection](0026-account-recovery.md).
- [ADR 0027 — Risk-based session management](0027-risk-based-session-management.md).
- [ADR 0028 — Secure human session renewal, credential rotation and per-device revocation](0028-human-session-token-safety.md).
- [ADR 0029 — Owner-only invitations and protected ownership changes](0029-owner-controlled-project-invitations.md).
- [ADR 0030 — Protected project activity and audit history](0030-project-activity-audit-log.md).
- [ADR 0031 — Atomic task admission and budget reservation](0031-atomic-task-budget-admission.md).
- [ADR 0032 — Earlier passwordless-only login selection](0032-passwordless-human-sign-in.md) — **Superseded by ADR 0033**.
- [ADR 0033 — Configurable password/email login and authenticator-app TOTP 2FA](0033-configurable-password-email-login-and-totp.md) — **Approved 2026-10-24**.
- [ADR 0034 — Owner action-bound reauthentication without mandatory TOTP](0034-owner-action-bound-reauthentication.md) — **Approved 2026-10-24**; the four core time/attempt limits approved; other abuse/risk and recovery mechanics still open.
- [ADR 0037 — Central Secret Broker and task-scoped access](0037-central-secret-broker-task-scoped-access.md) — **Approved 2026-10-24**; concrete credential and isolation implementations remain open.
- [ADR 0038 — Isolated multi-account CLI sessions and API-key mode](0038-multi-account-isolated-cli-and-api-access.md) — **Approved 2026-10-24** architecture; provider-specific hosted subscription eligibility remains P0.
- [ADR 0039 — Amazon ECS on AWS Fargate replaces Hetzner Cloud priority](0039-aws-ecs-fargate-replaces-hetzner.md) — **Approved 2026-10-24**; concrete region/task cost and live adapter validation remain open.
- [ADR 0040 — One real provider for first working Rolvya version](0040-single-provider-first-working-version.md) — **Approved 2026-10-24** for fake-first + single-real-adapter, but its original Google-first selection was superseded by ADR 0041.
- [ADR 0041 — **AWS ECS Fargate first real integration**](0041-aws-fargate-first-real-worker.md) — **Approved 2026-10-24**; fake workers → AWS Fargate → Google Cloud Run Jobs → user-enrolled remote VPS. No cloud spend authorized.
- [ADR 0042 — First-version five-section web panel navigation](0042-five-section-web-panel-navigation.md) — **Approved 2026-10-25** for the five top-level navigation sections.
- [ADR 0043 — Single minified app JS, embedded CSS, external fonts/icons and browser/UX completeness](0043-single-bundle-browser-platform-and-ux-boundary.md) — **Approved 2026-10-25** for core React/Vite/React Flow/REST+SSE stack and output/browser constraints. UI aesthetics, concrete provider/locale versions and first activation consent remain OQ-013.
- [ADR 0044 — Encrypted SQLite for initial Rust Secret Broker credential storage](0044-encrypted-sqlite-secret-storage.md) — **Approved 2026-10-25**; the key is kept outside SQLite/backup; ADR 0045 settles automatic loading, while crypto implementation and recovery stay OQ-005/P0.
- [ADR 0045 — Automatic unlock from a protected controller-host master-key file](0045-automatic-local-master-key-unlock.md) — **Approved 2026-10-25**; no manual entry on normal startup, no silently regenerated missing key; exact provisioning, rotation and separate recovery remain open.
- [ADR 0035 — Scoped containment of suspicious Owner actions](0035-scoped-human-incident-containment.md) — **Approved 2026-10-24**; protect compromised human credentials while preserving separately authorized safe worker execution.

These are **Approved designs, not claims of implemented or production-verified software**. Detailed region/cost/adapter validation remains separate.

**Remaining proposed provider rollout decision:**

- [ADR 0007 — Initial remote worker providers, regions and cost comparison](0007-remote-worker-hosting.md) — **Proposed**; it does not reopen approved multi-provider support.


Selection gates, unresolved questions and evidence requirements are in the [open-decision register](../project/open-questions.md). Implementation order after approval is in the [proposed implementation slices](../project/implementation-slices.md).

## ADR process
Create `NNNN-short-decision-title.md` for consequential decisions. Include context, options, chosen decision, owner/approval date, consequences, rejected alternatives, related requirements, migration notes and supersession links. Never mark an item approved just because an earlier assistant suggested it.

Review this register against original conversations and diagrams before claiming historical coverage is complete.
