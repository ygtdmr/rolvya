---
title: Open Decisions and Architecture Questions
status: Open
category: project
last-reviewed: 2026-10-24
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
| OQ-004 | P0 | Finalize numerical/security implementation for already approved human login, voluntary TOTP and transaction-bound Owner reauthentication; sessions and membership/recovery safeguards | **ADR 0033 supersedes ADR 0032: password login or verified-email Magic Link/OTP are approved primary options; authenticator-app TOTP is **fully optional for all users and Owner actions**, not standalone; no SMS. Owner must initially enable at least one primary method and may later enable others.** Remaining: secure provider/broker protocol, trusted subject binding, email delivery/account-IP throttling, account linking, **ADR 0034 approved 5m action proof, 10m email link/OTP, 5 failed email OTP attempts and one-use proof**, **ADR 0035 approved scoped incident containment**, remaining risk thresholds, sessions and membership/invite schema | [ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md), [P0 auth contract](../architecture/human-authentication-implementation-contract.md), [ADR 0034](../decisions/0034-owner-action-bound-reauthentication.md), [Owner reauthentication implementation proposal](../architecture/owner-non-totp-reauthentication-design.md), [identity](../architecture/identity-authorization.md) |
| OQ-005 | P0 | Secrets storage and remote worker identity? | **ADR 0037 approves central Secret Broker and minimum task-scoped credential access**; concrete secret store, broker interface, host attestation, extraction risk and revocation implementation still require validation | [ADR 0037](../decisions/0037-central-secret-broker-task-scoped-access.md), [Secrets](../operations/secrets.md) |
| OQ-008 | P0 | Provider-valid **approved multi-account CLI and API-key** execution on unattended remote workers? | **ADR 0038 approves** genuine provider CLI sign-in, multiple isolated Claude/Codex accounts and separate API keys. Still verify hosted/third-party entitlement, approved login and token handling, quotas/concurrency, and no silent account/API fallback. | [ADR 0038](../decisions/0038-multi-account-isolated-cli-and-api-access.md), [CLI/API contract](../architecture/model-access-entitlement-contract.md), [official evidence](../reference/provider-cli-api-policy-evidence.md) |
| OQ-010 | P1 | Which first-provider **regions, images, sandbox settings and cost guarantees** are viable? | **ADR 0040 approves fake-first, then one Cloud Run Jobs adapter for the first working version; AWS ECS on Fargate and user remote VPS added incrementally.**  First adapter priorities **Approved ADR 0018**: Cloud Run Jobs, **Amazon ECS on AWS Fargate** and user remote VPS ([ADR 0039](../decisions/0039-aws-ecs-fargate-replaces-hetzner.md) replaces Hetzner); validate each one's actual cost bounds, isolation and billing | [ADR 0018](../decisions/0018-initial-worker-adapters.md), [ADR 0007](../decisions/0007-remote-worker-hosting.md), [staged rollout proposal](../architecture/initial-remote-worker-rollout-proposal.md), [approved AWS replacement](../decisions/0039-aws-ecs-fargate-replaces-hetzner.md), [approved first-working rollout](../decisions/0040-single-provider-first-working-version.md) |
| OQ-011 | P1 | Remaining worker retry delay/escalation, provider timeout and resource/model reassignment parameters? | **Already approved:** 15-minute worker token, 30-second heartbeat, 90-second lease, and 3 total AI worker attempts by default (ADR 0019). Old 1/3/10-minute backoff diagram nonbinding; remaining: retry delays, failure classification, provider costs/stop timing and safe escalation | [Retry and escalation](../architecture/retry-escalation-policy.md) |
| OQ-012 | P1 | Planner and worker context packaging, token quotas and handoffs? | Minimize tokens in **both** phases, retain required constraints, version hashes and prior verified artifacts | [Token efficiency](../architecture/token-context-efficiency.md) |
| OQ-013 | P1 | UI stack, panel navigation and plan approval interactions? | [Five-section first-version layout](../product/initial-web-panel-structure.md) **Proposed**, not approved: Projects, Execution, Costs & Approvals, Connections, Results & Activity. Still choose frontend stack, precise approval UX, graph/accessibility details; approved Owner/budget/session security is unchanged. | [User flows](../product/user-flows.md), [web panel proposal](../product/initial-web-panel-structure.md) |
| OQ-014 | P1 | Backup RPO/RTO, encryption/key custody and restore-test cadence? | **Already approved ADR 0015:** 15-minute backups, 24-hour daily recovery points, retention of frequent points for 7 days and daily points for 30 days. Remaining: measured RPO/RTO, encryption keys, restore tests, provider region and cost | [Backups](../operations/backup-recovery.md) |
| OQ-015 | P1 | CI merge gates, review roles and release process? | Test scope, human security signoff, branch protection, MIT notice compliance | [Review](../development/review-process.md) |
| OQ-016 | P1 | Alert delivery, monitoring, audit integrity anchoring and archive retrieval? | **Already approved ADR 0030:** 90-day panel-accessible activity, 1-year routine and minimum 3-year critical finance/security retention, independent trusted checkpoints. Remaining: alert delivery, crypto/key custody, retrieval, noise and privacy controls | [Observability](../architecture/observability.md) |
| OQ-018 | P2 | Non-GitHub self-hosted remote Git adapters? | Provider compatibility and authorization, never local computer sync | [Git integration](../architecture/integration-github.md) |

| OQ-019 | P2 | Which exact accepted logo source file/SVG, final no-gold HEX values and brand-use policy are authoritative? | Obtain original approved asset and provenance; avoid redrawing, superseded brass and trademark assumptions | [Brand history](../product/brand-history-license-governance.md) |
| OQ-020 | P1 | Which *numeric* priority/aging weights, fairness algorithm, provider fallbacks and measured backpressure thresholds are safe? | [ADR 0022](../decisions/0022-dynamic-worker-concurrency.md) **Approved** dynamic concurrency; [ADR 0023](../decisions/0023-cross-project-scheduling-fairness.md) **Approved** cross-project fair sharing and aging. Only algorithm details, fairness parameters and thresholds are open | [Selection policy](../architecture/scheduler-selection-policy.md) |

## Closed atomic admission decision

- **OQ-003 closed 2026-10-23:** [ADR 0031](../decisions/0031-atomic-task-budget-admission.md) approves a short SQLite `BEGIN IMMEDIATE` transaction that rechecks ready executable task/dependencies and authorized budget, atomically creates the same-task attempt, fenced lease, reservation, audit and durable outbox; paid dispatch occurs **only after commit**. Exact schema, unknown billing/driver behavior, concurrency tuning and live verification remain open implementation work.

## Scoped suspicious-action containment — Approved ADR 0035 (2026-10-24)

[ADR 0035](../decisions/0035-scoped-human-incident-containment.md) approves hold/deny of affected critical Owner operations on credible takeover/replay, invalidation of other pending approvals sharing compromised authority and revocation of affected stolen human sessions. **Unrelated authorized human use and already-approved workers with valid isolated credentials and enforceable reserved budgets are not automatically stopped**. Independent worker/provider/budget compromise still invokes its own scoped stop-on-risk. Risk evidence thresholds, incident-review/unlock mechanics, notifications and administrator-assisted recovery remain **OQ-004/P0**.

## Approved challenge/approval limits — OQ-004 partially resolved (2026-10-24)

[ADR 0034](../decisions/0034-owner-action-bound-reauthentication.md) now also approves: **5-minute action-specific authorization after new proof**, **10-minute email Magic Link/OTP from issue**, **at most 5 failed email OTP submissions per challenge**, and **one-use** challenge/privileged approval. Do not reopen these defaults as undecided. Remaining OQ-004/P0 includes risk detection, account/IP/re-issuance throttles, recovery, sessions, identity bindings and tests.

## Approved human login methods — OQ-004 partially resolved

[ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md) supersedes the earlier passwordless-only rule. **Password or verified-email Magic Link/OTP** are supported primary methods; **authenticator-app TOTP** is additional optional 2FA (no SMS). First setup requires at least one primary method, and only an authorized Owner can enable further methods. Google/GitHub social sign-in is not initial v0.1 scope. The allowed methods and voluntary TOTP are selected. **ADR 0034 now approves the fresh password/email, exact one-use, risk-aware Owner step-up principle.** Still OQ-004/P0: concrete proof expiry, challenge/lockout limits, risk thresholds, sessions, identity binding, account recovery, membership/invite schema and implementation/testing. Google/GitHub social login is **not** initial v0.1 scope; do not imply a mandatory GitHub human OIDC integration. Do not close the entire OQ-004 engineering gate.

## Recently approved remote integration architecture — no longer open

- **OQ-006 closed 2026-10-21:** [ADR 0006 — Approved](../decisions/0006-worker-communication.md). Worker control uses remote-worker-initiated HTTPS pull/claim/heartbeat/result with SQLite durable outbox; exact Rust library/auth implementation, payload and long-poll timing remain choices; **15-minute worker token, 30-second heartbeat and 90-second lease are already approved**.
- **OQ-007 (GitHub auth selection) closed 2026-10-21:** [ADR 0008 — Approved](../decisions/0008-github-authentication.md). GitHub integration uses GitHub App installations with a trusted Rust Git publisher. Exact endpoint grants, merge authority and owner permissions remain open security details covered by OQ-004/005/015.
- **Multi-provider architecture Approved:** [ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md) requires extensible Rust adapters supporting different remote providers, not a single cloud implementation. **OQ-010 remains open for regional/deployment settings, provider setup, billing evidence and benchmarking; the first three adapter priorities were approved in ADR 0018.**

## Recently approved execution policies — no longer open

- **OQ-009 closed 2026-10-21:** [ADR 0010 — Approved](../decisions/0010-inflight-budget.md). Already-running billable attempts continue only within verifiably enforceable pre-reserved exposure; if full exposure cannot safely be bounded, initiate stop-on-risk, fence/cancel and reconcile. Provider-specific caps, cancellation timing, billing lag and refund rules remain implementation details.
- **OQ-017 closed 2026-10-21:** [ADR 0009 — Approved](../decisions/0009-parent-aggregation.md). Parent nodes are non-executable groups; only planned executable leaves receive workers. Verified required child outputs and group acceptance drive success. Optional-child semantics and group predicate serialization remain open implementation details.

## Approved technology choices — no longer open

- **OQ-001 closed 2026-10-21:** Rust is the **only** Rolvya backend language ([ADR 0004 — Approved](../decisions/0004-control-plane-language.md)). Rust frameworks/crates and performance benchmarks remain open implementation details.
- **OQ-002 closed 2026-10-21:** SQLite is the **only** authoritative database engine ([ADR 0005 — Approved](../decisions/0005-authoritative-database.md)). **ADR 0014 already approves initial WAL, maximum four pool connections and a five-second SQLite busy timeout; ADR 0013 approves SQLx.** Atomic admission transaction pattern is **approved ADR 0031**, while its Rust/SQLite implementation, exact schema, `synchronous`/checkpoint tuning, restore tests and RPO/RTO remain open. Backup cadence/retention are **approved ADR 0015**.

## Candidate decision packages (not approved)

| Open question | Proposed ADR | Evidence required before owner approval |
| --- | --- | --- |
| OQ-003 (closed) | [Approved ADR 0031: atomic admission](../decisions/0031-atomic-task-budget-admission.md) and [backend contract](../architecture/rust-sqlite-backend.md) | **Policy settled**; implement and test lease/reservation/outbox atomicity, WAL/busy behavior, crash replay and billing consistency |
| OQ-010 | [ADR 0007: first provider rollout](../decisions/0007-remote-worker-hosting.md) with [Approved ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md) | Benchmark initial provider adapters on billable minimums, latency, isolation, region and four cost classes; extensibility itself is settled |

The [implementation slice proposal](implementation-slices.md) explicitly blocks production coding on unresolved critical choices. ADR 0007 remains `status: Proposed` for initial provider rollout. ADR 0006 (HTTPS), ADR 0008 (GitHub App) and ADR 0011 (multi-provider) are now `status: Approved`; only their concrete contracts/enrollment and security details remain to be implemented.

## Decision package for remaining technology ADRs

The [transport / Git / worker hosting decision pack](transport-git-hosting-decision-pack.md) records **Approved ADR 0006 (HTTPS), ADR 0008 (GitHub App) and ADR 0011 (multi-provider architecture)**, while tracking the **still-open OQ-010 / Proposed ADR 0007** initial provider rollout. Its associated drafts are the [versioned worker wire contract](../architecture/worker-wire-contract-draft.md), [remote Git publisher contract](../architecture/git-remote-integration-contract.md), [worker lifecycle and cost reconciliation](../operations/remote-worker-lifecycle-contract.md) and [remote integration test suite](../development/remote-integration-test-matrix.md). Their detailed wire/configuration schemas remain **Proposed**, while the core transport/authentication/multi-provider architecture choices are **Approved**. Paid benchmarks require independent explicit test-budget authorization.

## Detailed evaluation specifications

- [Budget authorization and provider invoice reconciliation](../architecture/budget-reservation-reconciliation.md) implements the **Approved** bounded-or-stop running-work policy conceptually; actual provider limits and accounting details remain open.
- [Worker transport design](../architecture/worker-transport-design.md) proposes message shapes and failure tests. **HTTPS transport is already approved ADR 0006**; only implementation/wire details are Proposed.
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

## Human project role model — now Approved

[ADR 0024](../decisions/0024-project-human-roles.md): initial project human roles are **Owner**, **Editor** and **Viewer**. Editors may start previously authorized work and inspect results but cannot raise budget or grant critical security authority; Viewers have read-only access to permitted data. No separate Reviewer role is approved. Authentication/session/MFA, invite/revocation and fine-grained authorization implementation remain **OQ-004**, not a reopening of the three-role policy.

## Authentication and session design — approved principles vs implementation proposals

- [ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md) approves current password/email/TOTP methods; ADR 0025 governs optional federated OIDC/passkey protections; [ADR 0026](../decisions/0026-account-recovery.md) approved security-preserving recovery; [ADR 0027](../decisions/0027-risk-based-session-management.md) approved secure, user-friendly session renewal, risk-based step-up and device-based revocation. **No automatic mandatory logout every 30 days**; 7-day real-human-inactivity web baseline is approved.
- [ADR 0028](../decisions/0028-human-session-token-safety.md) **Approved** the basic secure renewal, issuer-capability-aware rotation/replay safeguards and per-device server-side revocation. The [human session implementation contract](../architecture/session-token-rotation-and-theft-response.md) remains **Proposed**: exact cookie/rotation algorithms, provider-specific replay, assurance verification, step-up lifetime and encrypted storage must be tested/decided.
- Mobile app and Jarvis-style voice are **hypothetical ideas only**, not approved roadmap commitments, and need no implementation.

## Project invitation and ownership policy — Approved

[ADR 0029](../decisions/0029-owner-controlled-project-invitations.md): only Owner invites; normal Editor/Viewer invitations expire and are one-use and require independently verified login through an enabled method (and their voluntarily enabled TOTP, if any); changing/assigning Owner authority follows an independent high-assurance procedure. **The 24-hour default invitation validity is Approved (ADR 0029).** Only technical parameters—recipient binding, invitations delivery/revocation UI, transfer timeout, exceptional recovery and precise last-Owner transaction implementation—remain open; **two-party authenticated approval, multiple Owners per project, independent identities, no early Owner removal and a transactional last-Owner guard are Approved**. Do not re-ask the product policy.

## Multi-Owner budget approvals — Approved

With multiple project Owners, **one authenticated, strongly reverified Owner may explicitly approve a budget increase**, and the system notifies other Owners after commit. **Unanimous approval is not required**; notification receipts are not a budget gate. All project hard-budget and SQLite reservation rules still apply; see [ADR 0029](../decisions/0029-owner-controlled-project-invitations.md). **Mandatory in-app notification, with email optional by recipient preference, is Approved ADR 0029.** Notification data model, delivery provider and retry mechanics remain technical open questions.

## Project Activity Log — Approved

[ADR 0030](../decisions/0030-project-activity-audit-log.md) **Approved:** project Activity Log with crash-safe append-only application records, redaction, independently protected cryptographic checkpoints through extensible administrator-allowlisted storage adapters, project Owner target selection, 90-day active access, 1-year routine and minimum 3-year critical retention subject to law. Switching provider affects **future writes only**. Historical migration requires a **distinct Owner approval and hard budget admission**, is **staged and resumable after interruptions**, preserves independently verified progress and all original source data, and requires **independent full-manifest/checkpoint verification** before completion. **Four total attempts per logical storage-transfer operation by default (initial + at most three automatic retries), 5/10/20-second base waits, small bounded jitter and priority for a provider-required longer wait are approved for classified transient connection/provider faults only**; persist counters and wait state, honor provider throttling, reconcile uncertain side effects before reissue, and stop automatically on repeated failure or integrity/security/permission/budget blocks. New billable copy/retry work pauses on insufficient approved exposure without automatic ceiling increase; uncertain external writes/charges must be reconciled. Even successful migration **never** auto-deletes old data: separate strongly authenticated Owner cleanup approval and legal/retention checks remain mandatory. **Open (implementation only):** [state machine](../architecture/storage-migration-state-machine.md) exact enum/transitions, consistency/manifest representation, crypto/checkpoint cadence, adapter probes, **exact jitter magnitude/distribution, provider cool-down parsing, operation granularity and provider-specific error classification (four attempts and 5/10/20-second bases already approved)**, chunk sizing, cost bounds and billing reconciliation, archival implementation, lawful erasure, monitoring and storage cleanup UX. Documentation describes requirements and Proposed mechanics; **nothing is implemented or verified**.

## Decisions that are not open

These have already been approved and must not be reopened by a worker without owner consent: Rust-only backend and SQLite-only authoritative database; non-executable group parents and independently validated aggregate success; running workers limited to pre-reserved enforceable exposure or stop-on-risk; one main agent creates a fully fixed task hierarchy and DAG before runtime; workers execute, not plan; dependencies govern parallelism; hard budget pause with explicit approval and no percentage overrun; preserve completed work; separate cost categories/scenarios; remote Git only; no Local Agent or personal-computer sync; MIT License.

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
