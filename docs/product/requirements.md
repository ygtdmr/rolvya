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
| PR-010 | Main planner assigns model and effort per task; subscription/CLI versus API-key access was discussed as distinct modes. | Store mode explicitly; enforce provider terms after validation. |
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
| PR-025 | The deployment Owner must initially enable at least one primary method: **email/password** or **email-only Magic Link/OTP**; the Owner may later enable the other after appropriate strong verification. App-based **TOTP 2FA is completely opt-in, including for Owners and privileged actions**; it is additional, never primary, and **SMS is not required**. Secure password verifiers, one-use email challenges, protected factor enrollment and verified role checks are mandatory. Earlier Google/GitHub/OIDC-only human login requirements are superseded for v0.1 by ADR 0033; remote GitHub App integration is separate. | Verify authorized setup/method changes, real credential binding, password/email challenge security and TOTP without SMS. [Approved ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md), AC-113–AC-124. |
| PR-026 | With absent/nonfunctional outbound mail, password-reset UX must tell the user to **contact the instance administrator** without pretending a reset email was sent or revealing account existence. Admin help requires independently verified identity and controlled, audited recovery without bypassing TOTP/Owner safeguards. Email verification cannot be the only primary login method unless outbound mail actually works. | No-email fallback, controlled recovery and invalid email-only startup configuration are specified in AC-125–AC-127; not yet implemented. [ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md). |

## Approved project Activity Log

| ID | Requirement | Verification expectation |
| --- | --- | --- |
| PR-024 | Project-scoped chronological **Activity Log** for budget/permissions, agent/task/worker, failed and security/system events; truthful provenance, role-aware viewing, secret redaction, crash-safe persistence and mandatory cryptographic verification of active and archived history. Independent provider-neutral trusted checkpoints (not S3-only), administrator-controlled provider/endpoint allowlisting and project Owner selection among allowed targets. **Switching storage provider affects future writes only**, never automatically copies old archives/checkpoints; historical migration requires a distinct Owner approval and hard budget/security checks, proceeds in **staged resumable units**, preserves previously independently verified progress through interruptions, reconciles uncertain provider writes/charges before retry, uses **4 total storage-transfer attempts by default (initial + at most 3 automatic retries)** with **5/10/20-second base waits** (small bounded jitter and provider-mandated longer cool-down) for transient failures only, blocks automatic retries on integrity/security/permission violations or exhausted limits, and **pauses new billable work** rather than overrunning approved limits. Whole-manifest independent integrity/checkpoint validation and durable completion evidence are mandatory. **Successfully migrated source data is not automatically deleted**: separate strongly authenticated Owner approval, independent destination/integrity verification, scoped/auditable cleanup and retention/legal-hold checks are mandatory. Provide **90 days active panel access**, **1 year total routine retention**, and **at least 3 years critical finance/security retention**, subject to law; ordinary roles cannot rewrite/delete history. | Verify role and cross-project denial, crash/replay deduplication, confidential-data redaction, storage cutover/migration refusal, interrupted/resumed transfer, uncertain billing, integrity failure and explicit source-cleanup acceptance cases; [Approved ADR 0030](../decisions/0030-project-activity-audit-log.md). |

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
