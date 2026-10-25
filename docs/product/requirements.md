---
title: Product Requirements
status: Proposed
category: product
last-reviewed: 2026-10-21
tags: [requirements, orchestration, agents]
---

# Product requirements

## Purpose and authority

Rolvya is an open-source orchestration system for AI coding agents. This specification separates **previously agreed constraints** from proposed implementation details. No working product is implied. Consult the [decision register](../decisions/README.md) and [project status](../project/status.md).

## Agreed product invariants

| ID | Requirement | Verification expectation |
| --- | --- | --- |
| PR-001 | A single main planning agent creates the complete task hierarchy and dependency DAG before execution. | Reject execution of incomplete or cyclic plans. |
| PR-002 | Execution never creates additional task nodes. Failed attempts can be retried or reassigned to another worker/model/effort without mutating the original task identity. | Compare active DAG identities before and after retries. |
| PR-003 | Worker agents execute bounded assigned tasks only; they neither plan the project nor spawn new subtask trees. | Worker contract has no task-creation privilege. |
| PR-004 | Only dependency-satisfied work is eligible; independent tasks run concurrently when capacity and conflict-safety permit. | Scheduler tests cover blocked and parallel tasks. |
| PR-005 | Tasks carry goal, instructions, context, model, effort and execution mode; history, output and evidence must be retained. | Inspect planned and executed task records. |
| PR-006 | Run coding agents on **remote** workers (cloud or user-provided remote server); orchestration must accommodate a small control VPS (2–4 GB RAM was discussed). | Infrastructure design separates control-plane and remote execution. |
| PR-007 | No Local Agent, Local Sync, direct personal-computer Git access, or synchronization with repositories on the user's own computer. Git integration uses remote services, including self-hosted services. | No local-host connector in interfaces or code. |
| PR-008 | A project budget is a hard admission gate: insufficient remaining funds prevent new billable work until explicit user approval raises the ceiling. Never permit an automatic percentage overrun; preserve completed work. | Concurrent reservations, denial, approval and resume tests. |
| PR-009 | Estimate optimistic, expected and pessimistic costs, distinguishing AI API, cloud compute, storage and transfer. | Cost report preserves all three scenarios and four categories. |
| PR-010 | The main planner assigns model, effort and authorized access mode per task. **ADR 0038** requires separate API-key and genuine CLI session modes; multiple independently authorized Claude/Codex accounts each have isolated credential/runtime homes and may run concurrently where provider terms permit. No personal-computer integration, provider spoofing, hidden paid fallback or quota evasion. | Verify per-connection permission, isolation, parallel eligible runs, quotas, provider eligibility and distinct subscription/API billing. [ADR 0038](../decisions/0038-multi-account-isolated-cli-and-api-access.md). |
| PR-011 | Worker tasks should be moderately scoped, with ~10-minute runtime as a planning target, not a guarantee or hard timeout. | Estimates and runtime metrics remain separate. |
| PR-012 | Persist authoritative project/task/attempt/budget states in a single database instance where practical; prioritize efficiency. | Recovery and budget integrity tests after restart. |
| PR-013 | Use Obsidian-compatible, GitHub-readable Markdown documentation; agent handoffs and current project status must be maintained. | Relative links and historical decisions are traceable. |
| PR-014 | All Rolvya-owned backend services and backend logic are implemented **only in Rust**. | Backend build and source inspection show no alternative-language backend services; approved [ADR 0004](../decisions/0004-control-plane-language.md). |
| PR-015 | The sole authoritative database engine is **SQLite**, accessed through the Rust backend. | SQLite transactions, foreign keys, restore and budget-admission tests; approved [ADR 0005](../decisions/0005-authoritative-database.md). |
| PR-016 | Parent tasks are **non-executable groups**. Only planned executable leaf tasks receive worker attempts. Group success requires independently validated required descendants and applicable acceptance criteria. | Reject leasing a group; group dependency waits for validated aggregate; [Approved ADR 0009](../decisions/0009-parent-aggregation.md). |
| PR-017 | Already-running billable attempts continue only within enforceably bounded, **pre-reserved** exposure; otherwise issue stop-on-risk/fencing/cancellation and reconcile provider charges. | Block unbounded execution and new unfunded admissions, retain artifacts; [Approved ADR 0010](../decisions/0010-inflight-budget.md). |
| PR-018 | Worker control uses authenticated **worker-initiated HTTPS** pull/claim/heartbeat/result messaging, with SQLite-authoritative state. | Reject forged/stale claims and duplicated messages; [Approved ADR 0006](../decisions/0006-worker-communication.md). |
| PR-019 | GitHub remote repository access uses **GitHub App**, with trusted Rust publication and separate protected ref/merge authorization. | Scoped installation/ref rules, token isolation and revocation evidence; [Approved ADR 0008](../decisions/0008-github-authentication.md). |
| PR-020 | Remote worker infrastructure provides an **extensible, multi-provider Rust adapter architecture**, including the ability to add other remote cloud/host implementations without changing the task DAG or budget authority. | Conformance with two synthetic providers, independent remote execution and failover controls; [Approved ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md). |
| PR-028 | **First working version rollout:** prove correctness with synthetic fake workers, then integrate **AWS ECS on Fargate** as the first real remote provider. **Google Cloud Run Jobs** and user-enrolled remote VPS follow incrementally; preserve the provider-neutral interface and unchanged hard budget, worker identity, model entitlement and remote-only safeguards. | Validate fake end-to-end first, one safe authorized AWS ECS Fargate attempt second, and compatibility of later provider additions; no public release/paid test implied. [Approved ADRs 0040](../decisions/0040-single-provider-first-working-version.md) and [0041](../decisions/0041-aws-fargate-first-real-worker.md); AC-150–152. |

## Approved browser frontend and packaging requirements (ADR 0043)

| ID | Requirement | Verification expectation |
| --- | --- | --- |
| PR-029 | First-version browser frontend uses **React + TypeScript, Vite, React Flow read-only activated DAG, and Rust Axum REST/SSE**, retaining a distinct accessible group/task hierarchy and server-authoritative SQLite data. | Graph vs hierarchy and overlay identity tests, REST/SSE reconnect/permission tests; [ADR 0043](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md), AC-153–AC-154. |
| PR-030 | The production distribution ships **exactly one minified content-hashed application JS file with compiled CSS inside**, **no separate production CSS or other app JS chunks**, while separately shipping required `sw.js`, manifest, external font/icon and other browser assets. **Never inline/bundle fonts or icons**; font/icon provider and all specific UI aesthetics remain unselected. | Machine-readable dist audit, CSP-proven stylesheet mount, cache/rollback, absence of font/icon glyph/Base64 payloads and no extra executable app JS; [ADR 0043](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md), [build contract](../architecture/web-platform-artifact-and-capability-contract.md), AC-155–AC-157. |
| PR-031 | Deliver relevant installable PWA manifest/icons/service worker, browser-file catalog, secure headers, correct MIME/caching, offline **non-billable** shell and graceful supported/unsupported API flows. Only enable optional browser permissions when product flows justify them. | PWA/install/upgrade/rollback/offline and security header tests across supported browsers; never cache credentials/sensitive APIs or queue paid approvals; AC-158–AC-160. |
| PR-032 | UX is accessible and localizable by design, targeting **WCAG 2.2 AA**, keyboard/screen-reader and reduced-motion support, i18n/l10n with locale, RTL and Intl formatting, responsive/mobile fallbacks, clear budget/worker provenance and no mouse-only DAG operation. **Do not pick UI font/icon family, aesthetic layout or visual design prematurely.** | Manual+automated a11y, RTL/pseudolocale, focus/zoom, device/browser tests and deferred-design audit; [UX contract](web-ux-and-accessibility-contract.md), AC-161–AC-164. |

These are **approved future requirements**, not evidence that any frontend files, build or browser test already exist.

## Core end-to-end user flow (proposed)

1. Create a project and bind an authorized **remote** repository.
2. Supply project goal, scope, credentials by secure reference, allowed execution modes, desired budget and constraints.
3. Main planner produces the entire hierarchy, immutable dependency DAG, proposed model/effort allocations and three cost forecasts.
4. Validate graph and constraints. Present material questions and human approvals before accepting an execution version. **Plan approval UX remains open**; never infer budget approval.
5. Schedule ready tasks after dependency, concurrency, resource and budget admission checks; lease to isolated remote workers.
6. Collect logs, test evidence, artifacts and cost usage; validate outputs and integrate remote changes safely.
7. For failure, diagnose and retry/reassign an existing task within policy. Do not append new task nodes. For insufficient budget, stop admitting new billable tasks and await explicit ceiling increase.
8. Report results, remaining risks, unverified items, costs and handoff to next agent.

## Approved human project access

| ID | Requirement | Verification expectation |
| --- | --- | --- |
| PR-021 | Each project supports **Owner, Editor and Viewer** human roles. Owner exclusively authorizes hard-budget increases and critical permission/plan scope changes; Editor may start previously owner-approved work and examine permitted results but cannot expand the approved scope; Viewer sees only permitted read-only status/results. | Project-scoped RBAC negative tests and preflight/budget approval audit; see [ADR 0024](../decisions/0024-project-human-roles.md). |

## Approved session security requirements

| ID | Requirement | Verification expectation |
| --- | --- | --- |
| PR-022 | Human web sessions use secure unobtrusive renewal, conditional IdP-capability-aware rotation/replay protection when federation is enabled, plus local session rotation/replay defense, explicit server-side per-device revocation, and strong verified reauthentication for critical budget/access actions. Backend work never counts as human activity; mobile and voice remain uncommitted future ideas. | Negative tests for stolen/revoked sessions, legitimate concurrent renewals, issuer supported replay protection, independent workers and Owner step-up; [Approved ADR 0028](../decisions/0028-human-session-token-safety.md). |

## Approved invitation and ownership security

| ID | Requirement | Verification expectation |
| --- | --- | --- |
| PR-023 | **Owner-only** project invitations for Editor or Viewer, with expiry, one-use redemption and verified recipient identity through an enabled primary login method and any TOTP voluntarily enrolled by that user; link possession alone cannot grant access. Multiple individually authenticated Owners are allowed per project. Owner grants/transfers require separate recent strong step-up, receiving Owner authentication/acceptance, and transactional prohibition of last-Owner removal. Membership actions are project-scoped and auditably enforced by Rust/SQLite. | Negative tests for Editor/Viewer invitations, expired/replayed/mismatched invites, concurrent redemption, cross-project privilege grants and unverified Owner elevation; [Approved ADR 0029](../decisions/0029-owner-controlled-project-invitations.md). |

## Approved v0.1 human login and no-mail recovery

| ID | Requirement | Verification expectation |
| --- | --- | --- |
| PR-025 | The deployment Owner must initially enable at least one primary method: **email/password** or **email-only Magic Link/OTP**; the Owner may later enable the other after appropriate strong verification. App-based **TOTP 2FA is completely opt-in, including for Owners and privileged actions**; it is additional, never primary, and **SMS is not required**. Secure password verifiers, one-use email challenges, protected factor enrollment and verified role checks are mandatory. Earlier Google/GitHub/OIDC-only human login requirements are superseded for v0.1 by ADR 0033; remote GitHub App integration is separate. | Verify authorized setup/method changes, real credential binding, password/email challenge security and TOTP without SMS. [Approved ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md), AC-113–AC-124 and approved timing/replay regressions AC-136–AC-139. |
| PR-026 | With absent/nonfunctional outbound mail, password-reset UX must tell the user to **contact the instance administrator** without pretending a reset email was sent or revealing account existence. Admin help requires independently verified identity and controlled, audited recovery without bypassing TOTP/Owner safeguards. Email verification cannot be the only primary login method unless outbound mail actually works. | No-email fallback, controlled recovery and invalid email-only startup configuration are specified in AC-125–AC-127; not yet implemented. [ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md). |

| PR-027 | **Critical Owner actions require new, action-specific reauthentication and one-use approval**, using current password or a new session/action-bound verified-email Magic Link/OTP for accounts without enrolled TOTP. TOTP remains optional for everybody, including for sensitive Owner operations. Show exact budget/role/permission changes before confirmation; recheck current session/role/target/payload at commit, protect audit and durable notifications, and deny/hold automatic authorization on credible account compromise. A single factor is never claimed to be MFA. Existing Owner budget hard gates and independent worker admission remain in force. | Negative tests for stolen/stale session, new password/email proof, changed payload, replay, concurrent approval, mistaken system administrator powers, email outage and voluntary TOTP safeguards. **[Approved ADR 0034](../decisions/0034-owner-action-bound-reauthentication.md)** and **[ADR 0035](../decisions/0035-scoped-human-incident-containment.md)**; acceptance cases AC-119, AC-128–AC-143. **ADR 0034 now approves** 5-minute one-action proof, 10-minute email challenge, five failed email OTP attempts/challenge and one-use consumption. Other limits/risk thresholds remain unselected; no tests executed. |

## Approved project Activity Log

| ID | Requirement | Verification expectation |
| --- | --- | --- |
| PR-024 | Project-scoped chronological **Activity Log** for budget/permissions, agent/task/worker, failed and security/system events; truthful provenance, role-aware viewing, secret redaction, crash-safe persistence and mandatory cryptographic verification of active and archived history. Independent provider-neutral trusted checkpoints (not S3-only), administrator-controlled provider/endpoint allowlisting and project Owner selection among allowed targets. **Switching storage provider affects future writes only**, never automatically copies old archives/checkpoints; historical migration requires a distinct Owner approval and hard budget/security checks, proceeds in **staged resumable units**, preserves previously independently verified progress through interruptions, reconciles uncertain provider writes/charges before retry, uses **4 total storage-transfer attempts by default (initial + at most 3 automatic retries)** with **5/10/20-second base waits** (small bounded jitter and provider-mandated longer cool-down) for transient failures only, blocks automatic retries on integrity/security/permission violations or exhausted limits, and **pauses new billable work** rather than overrunning approved limits. Whole-manifest independent integrity/checkpoint validation and durable completion evidence are mandatory. **Successfully migrated source data is not automatically deleted**: separate strongly authenticated Owner approval, independent destination/integrity verification, scoped/auditable cleanup and retention/legal-hold checks are mandatory. Provide **90 days active panel access**, **1 year total routine retention**, and **at least 3 years critical finance/security retention**, subject to law; ordinary roles cannot rewrite/delete history. | Verify role and cross-project denial, crash/replay deduplication, confidential-data redaction, storage cutover/migration refusal, interrupted/resumed transfer, uncertain billing, integrity failure and explicit source-cleanup acceptance cases; [Approved ADR 0030](../decisions/0030-project-activity-audit-log.md). |

## Approved encrypted SQLite credential storage (ADR 0044)

| ID | Requirement | Verification expectation |
| --- | --- | --- |
| PR-033 | The initial trusted Rust Secret Broker encrypts credential records with authenticated application-layer encryption **before SQLite persistence**. SQLite remains the sole authoritative DB; unencrypted API keys, CLI authentication grants and master/root keys never enter DB rows, logs, task memory, source or ordinary backups. The **master key is provisioned and recoverable outside SQLite/backups**, while its exact custody mechanism and cryptographic implementation remain OQ-005/P0. Initial support does **not** require AWS Secrets Manager/Vault or full-file SQLite encryption. | Validate ciphertext-only SQLite/WAL/backup artifacts, cross-account authorization, tamper/missing-key denial, separate root-key recovery and rotation, and no accidental alternate credential backend. [ADR 0044](../decisions/0044-encrypted-sqlite-secret-storage.md); AC-165–AC-167. |

These are future checks, not evidence of implemented encryption or completed key recovery.

## Approved automatic Secret Broker master-key loading (ADR 0045)

| ID | Requirement | Verification expectation |
| --- | --- | --- |
| PR-034 | Initial Rust Secret Broker **automatically loads its existing master/key-encryption key from a separate, access-restricted file on the remote controller host** at ordinary startup. No Owner manual unlock, mandatory external KMS or AWS secret manager is required. File is outside SQLite, ordinary DB backups, code, worker images and browser assets; only the dedicated non-root service can read it. A missing/invalid key **fails closed** without automatic replacement or wiping encrypted credentials. Exact first-run provisioning and separately protected key recovery/rotation remain OQ-005/P0. | Verify unattended restart, strict file/directory permissions, missing/invalid/broadly accessible file denial, no implicit key regeneration, encrypted backup isolation and controlled separate key recovery. [ADR 0045](../decisions/0045-automatic-local-master-key-unlock.md); AC-168–AC-170. |

**Approved design, not proof of key generation or tested implementation.**

## Approved encrypted off-host recovery (ADR 0046)

| ID | Requirement | Verification expectation |
| --- | --- | --- |
| PR-035 | A separately retained **encrypted off-controller-host recovery copy** of the SQLite Secret Broker key material must exist, protected by an independent emergency recovery secret, separate from the ordinary database backups. Normal restarts remain automatic and never ask for emergency recovery credentials. Exact backup provider and cryptographic wrapping remain OQ-005/P0. | Restore encrypted records with a matching recovery kit to a trusted remote host; deny wrong key, tampered kit or mismatched version without data loss or paid dispatch. [ADR 0046](../decisions/0046-off-host-encrypted-master-key-recovery.md), AC-171–AC-173. |

These are approved requirements but remain unimplemented.

## Non-functional constraints (targets; thresholds not yet approved)

- Fast scheduling and state reads on constrained control hardware; minimize unnecessary polling and database scans.
- Durable state transitions, atomic budget reservations, replay-safe operations and crash recovery.
- Least-privilege, short-lived remote credentials, isolation of code execution, secret redaction and auditable approvals.
- Token-aware context packaging with compact references; no unnecessary repeated prompts.
- Observable resource consumption, latency, retries, task outcomes and provider-specific costs.

## Exclusions and open product questions

Not included: personal-computer worker/sync integration, runtime expansion of the task graph, automatic budget overrun. Still open: UI flow, supported providers and subscription eligibility, tenancy, how plan amendments are authorized, detailed prioritization, SLA and performance objectives, exact deployment stack, pricing-provider selection, licensing compliance and operational limits.

## Traceability

Technical elaboration: [Rust + SQLite backend](../architecture/rust-sqlite-backend.md), [DAG](../architecture/dag-planning.md), [execution](../architecture/execution.md), [workers](../architecture/workers.md), [budget](../architecture/budget-costs.md), [data model](../architecture/data-model.md), [security](../architecture/security.md). Testing: [acceptance criteria](acceptance-criteria.md), [test strategy](../development/testing.md).
