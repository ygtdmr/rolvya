---
title: Rolvya Decision Closure and Remaining Verification Gates
status: Decisions settled; engineering and external evidence pending
category: project
last-reviewed: 2026-10-25
tags: [decisions, implementation, release-gates]
---

# Decision closure and remaining verification gates

## Read this first: a decision is not an implementation task

**The Owner identified an error on 2026-10-25:** several choices already approved in previous conversations/ADRs were repeatedly described as open because implementation or tests were unfinished. This register corrects that separation. **Do not reopen an approved selection merely because code, deployment, version pinning, external entitlement, or acceptance evidence is missing.** Owner approval is required only if changing a previously approved constraint or introducing an action that itself requires approval (such as paid cloud work, external account connection, protected ownership/budget change).

The numbered OQ identifiers below are **historical issue categories** carried forward for traceability, **not 13 pending Owner decisions**. The assistant has selected implementation defaults for them in [one engineering pack](implementation-defaults-and-release-gates.md). Engineering-only details are resolved by implementation and test, not by asking the Owner the same question again.

## Binding decisions already closed

| Former OQ topic | Decision already settled — do not re-ask | Remaining work (NOT an open architecture vote) |
| --- | --- | --- |
| **OQ-004: human login** | Password **or** verified-email Magic Link/OTP primary login; optional TOTP only, never mandatory; role/invite policy, Owner action-specific fresh proof and approved timing/replay rules — [ADRs 0024, 0033–0035](../decisions/0033-configurable-password-email-login-and-totp.md) | Session implementation, throttle/risk tuning, account recovery, security tests — [Issue #2](https://github.com/ygtdmr/rolvya/issues/2) |
| **OQ-005: secrets** | Rust Secret Broker and isolated multiple accounts; encrypted SQLite; automatic protected local master key; separately encrypted off-host recovery kit and recovery secret; **XChaCha20-Poly1305, 256-bit KEK and wrapped per-secret DEKs** — [ADRs 0037/0038/0044–0047](../decisions/0047-secret-broker-xchacha20-key-hierarchy.md) | Crypto schema, file provisioning, recovery kit mechanism/storage, worker attestation and real restore tests — [Issue #2](https://github.com/ygtdmr/rolvya/issues/2) |
| **OQ-008: CLI/API architecture** | Separate CLI-subscription and API-key access, provider/account-specific isolated remote sessions, multiple accounts, no disguised identities, no silent CLI→paid API fallback — [ADR 0038](../decisions/0038-multi-account-isolated-cli-and-api-access.md) | **Externally verifiable provider entitlement** for specific unattended/hosted subscription integration remains a hard release gate; blocked until verified — [Issue #3](https://github.com/ygtdmr/rolvya/issues/3) |
| **OQ-010: real cloud order** | **Fake first → AWS ECS Fargate first real worker → Google Cloud Run Jobs → enrolled remote VPS**, no Hetzner first, no local PC — [ADR 0041](../decisions/0041-aws-fargate-first-real-worker.md) | AWS region/size/IAM, billing cap and cancellation measurements — [Issue #3](https://github.com/ygtdmr/rolvya/issues/3) |
| **OQ-011: worker attempts** | Controller owns retries; **3 total attempts by default (including the initial run)**, configurable at project preflight; 15-min worker token, 30-sec heartbeat, 90-sec lease — [ADR 0019](../decisions/0019-centralized-worker-retries.md), [worker auth](../architecture/worker-authentication-policy.md) | Implement transient backoff/classification, provider stop and billing reconciliation — [Issue #1](https://github.com/ygtdmr/rolvya/issues/1) |
| **OQ-012: context** | Compact context with mandatory acceptance/security constraints, upfront model/effort and fixed DAG — [context efficiency contract](../architecture/token-context-efficiency.md) | Benchmark and enforce context manifests/token budgets — [Issue #1](https://github.com/ygtdmr/rolvya/issues/1) |
| **OQ-013: frontend stack/UX** | Exactly five top-level sections; **React/TypeScript/Vite/React Flow + Rust Axum REST/SSE**, one minified application JS embedding CSS, separate SW; external font/icon assets, browser/i18n/a11y guarantees — [ADRs 0042](../decisions/0042-five-section-web-panel-navigation.md) / [0043](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md) | **Visual UI design, shared UI Elements, font/icon direction and motion rules now delegated and settled by ADR 0048 and the Connected Control spec.** Frontend source creation is still not started/authorized by this documentation request; browser implementation/tests remain work — [Issue #4](https://github.com/ygtdmr/rolvya/issues/4) |
| **OQ-014: backup policy** | Consistent backup every **15 min**, seven-day frequent and 30-day daily retention; encrypted off-server snapshots and key separation — [ADR 0015](../decisions/0015-backup-recovery.md) | Measure RPO/RTO, restore cadence and off-host key recovery, not reselect backup frequency — [Issue #4](https://github.com/ygtdmr/rolvya/issues/4) |
| **OQ-015: review/CI** | Human security authority and MIT license preserved; engineering defaults include Rust/test/secret/notice merge checks — [engineering pack](implementation-defaults-and-release-gates.md) | Implement branch protection, CI and verify actual runs — [Issue #4](https://github.com/ygtdmr/rolvya/issues/4) |
| **OQ-016: monitoring/audit** | 90-day panel history, one-year routine retention, at least three-year critical finance/security retention and protected audit integrity — [ADR 0030](../decisions/0030-project-activity-audit-log.md) | Implement durable in-panel inbox, optionally configured mail, audit anchoring/archive retrieval — [Issue #4](https://github.com/ygtdmr/rolvya/issues/4) |
| **OQ-018: other remote Git** | Remote Git only; **GitHub App first** — [ADRs 0003](../decisions/0003-remote-git-only.md) / [0008](../decisions/0008-github-authentication.md) | **Explicitly deferred** additional Git/self-hosted adapters; not a first-release decision blocker — [Issue #4](https://github.com/ygtdmr/rolvya/issues/4) |
| **OQ-019: brand assets** | Name/accepted logo identity, navy/blue/gray **no gold**, MIT and tagline approved — [brand decisions](../product/brand-history-license-governance.md) | Actual original approved image/vector and precise assets/trademark details intentionally deferred; never invent or redraw approved logo — [Issue #4](https://github.com/ygtdmr/rolvya/issues/4) |
| **OQ-020: scheduling fairness** | Dynamic concurrency without arbitrary 20-worker cap; approved fairness/aging and Owner-selected performance/cost modes — [ADRs 0021](../decisions/0021-scheduler-selection-modes.md), [0022](../decisions/0022-dynamic-worker-concurrency.md), [0023](../decisions/0023-cross-project-scheduling-fairness.md) | Implement and test fairness formula with initial parameters from [engineering pack](implementation-defaults-and-release-gates.md) — [Issue #1](https://github.com/ygtdmr/rolvya/issues/1) |

### Previously closed OQ categories (also not to be reopened)

- **OQ-001:** Rust-only backend — [ADR 0004](../decisions/0004-control-plane-language.md).
- **OQ-002:** SQLite sole authoritative DB; Axum/SQLx/WAL defaults already selected — [ADRs 0005](../decisions/0005-authoritative-database.md), [0012](../decisions/0012-rest-api-framework.md), [0013](../decisions/0013-sqlite-access-library.md), [0014](../decisions/0014-sqlite-initial-configuration.md).
- **OQ-003:** Atomic SQLite admission using `BEGIN IMMEDIATE`, lease, reservation and durable outbox before external dispatch — [ADR 0031](../decisions/0031-atomic-task-budget-admission.md).
- **OQ-006:** Worker-initiated HTTPS — [ADR 0006](../decisions/0006-worker-communication.md).
- **OQ-007:** GitHub App for remote Git — [ADR 0008](../decisions/0008-github-authentication.md).
- **OQ-009:** In-flight budget bounded-or-stop rule — [ADR 0010](../decisions/0010-inflight-budget.md).
- **OQ-017:** Non-executable group parents; only planned leaves run workers — [ADR 0009](../decisions/0009-parent-aggregation.md).

## Only genuinely outstanding external actions/evidence

These **are NOT re-opened owner product/architecture selections**:

1. **Provider permissions (OQ-008):** official Anthropic/OpenAI authorization/entitlement for this particular third-party, hosted, unattended CLI mode may require provider-specific registration/consent. Until verified, keep subscription-backed remote execution **disabled**; separately authorized API-key mode may be eligible. Do not claim any subscription account is officially licensed based on documentation alone.
2. **Live AWS/Git integration:** real account permissions, installation of the GitHub App, effective IAM limits, provider billing/termination tests and explicit **Owner-approved spend** are missing. No account or expense is implicitly approved by an architecture ADR.
3. **Technical proof:** writing Rust code, compiling, passing tests, first-run encryption, worker attestation and restoring backups are implementation work. See [Issues #1–#4](https://github.com/ygtdmr/rolvya/issues). Test failures may require engineering revisions but do not invalidate prior decisions without a clear reason and explicit change request.
4. **Deliberately deferred Owner design/input:** final UI appearance, font and icon supplier and provenance of original logo are not v0.1 fake-core prerequisites; **frontend source creation was explicitly deferred**. Do not repeatedly ask for them now.

## Policy against decision re-opening

**Order of authority:** latest explicit Owner statement → non-superseded approved ADR → approved product specification → engineering implementation defaults → historical proposal. If a later document conflicts with approved ADRs, correct the *later document*, not the approved decision. Before requesting any new decision, first search the [ADR register](../decisions/README.md) and this closed-decision map; if it is already decided, implement or test it instead. **Do not convert pending implementation into pending Owner approval.** Preserve strict authorization for financial, access and credential operations.

The [pre-implementation gap checklist](pre-implementation-gap-closure-review.md) is a dated historical assessment, not a second active open-decision authority. This file is the **single current resolution index**; prior long-form notes can be recovered in Git history if needed.

**Current evidence status:** architectural choices approved, selected engineering defaults recorded, **no verified backend/frontend implementation or cloud deployment**. Use [project status](status.md) and the GitHub issues to track code and tests.
