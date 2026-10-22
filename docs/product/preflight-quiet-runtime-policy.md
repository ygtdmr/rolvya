---
title: Preflight Decisions and Quiet Runtime Policy
status: Approved
category: product
last-reviewed: 2026-10-22
tags: [ux, approvals, planning, runtime, autonomy, budget]
---

# Preflight decisions and quiet runtime policy

## Approved principle (2026-10-22)

**Rolvya must minimize interruptions during execution.** Before activating a plan or launching any billable worker, the controller and main planning agent gather *all reasonably foreseeable decisions, permissions, boundaries and necessary clarifications* in a consolidated preflight phase. Once the user approves that scope, operation should proceed autonomously within it. Ordinary failures, model/effort changes, remote worker/provider selection, safe retries, validation, and recovery are managed by the main agent under the approved plan and Rust scheduler guardrails; **do not ask the user for every attempt or routine fallback**.

This policy does **not** permit silent expansion of budgets, access rights, task graph, or project scope. It also does not imply that every future incident is predictable.

## Preflight checklist / resulting immutable authorization snapshot

Ask questions in logical batches; prefer defaults and explicit trade-offs to repetitive prompts. Resolve before plan activation:
- Project objective, deliverables, required acceptance criteria, relevant context and external integrations.
- Full DAG and scope approval; required/optional work semantics when relevant.
- Allowed model families, specific models, effort ranges, CLI/API access modes, supported entitlements, acceptable automatic fallback ladder, and cost constraints.
- Permitted provider accounts, worker regions/resource classes, remote-host permissions, external effects, GitHub repositories, branches and publishing/merge authority.
- Optional owner-selected **project priority** for fair scheduling across authorized shared worker pools. Lower-priority eligible projects must not be starved indefinitely; [ADR 0023](../decisions/0023-cross-project-scheduling-fairness.md) governs aging and fairness. Priority cannot authorize another project's resources.
- Scheduler preference (**Economical**, **Balanced** default or **Performance**) and any optional positive maximum number of concurrent worker attempts. **No arbitrary global/default concurrency ceiling** is assumed if none is specified; automatic capacity remains limited by actual quotas, risk and budget under [ADR 0022](../decisions/0022-dynamic-worker-concurrency.md).
- Total hard budget cap and time/resource bounds; expected/pessimistic estimates separated into AI API, worker compute, storage and transfer. Per-attempt reservations must fit remaining budget; automatic percentage overruns prohibited.
- Retry bounds, diagnostic authority, allowed context/prompt repairs, permitted checkpoints/artifact reuse and expected validation.
- Security/data-handling constraints, secrets, data residency, side-effect controls and conditions that require explicit approval.
- The actual set of unanswered required questions must be resolved before *billable* activation. Do not assert the user has approved answers they have not given.

Persist the approved policy snapshot, owner identity, timestamp and plan hash in SQLite; workers receive only their narrowly authorized task context and never permission to enlarge these limits.

## Approved human authorization boundaries (ADR 0024)

The three project-scoped human roles are **Owner**, **Editor** and **Viewer**, as approved in [ADR 0024](../decisions/0024-project-human-roles.md). The Owner approves preflight plan/scope and hard budget changes. Editors may start or manage a **previously owner-approved** run inside its permitted execution envelope, but cannot increase ceiling or grant new critical model/provider/Git/security authority. Viewers see only authorized read-only status/results. Human roles do not confer permissions on AI main agents or remote workers. Do not prompt the Owner for each routine Editor-started job or main-agent retry when its approvals already exist.

## Quiet runtime and escalation

- **Runtime normal:** main agent receives failure telemetry, diagnoses cause, adjusts attempt-scoped strategy only within authorized model/effort/provider/resource/retry bounds, and preserves verified outputs; scheduler verifies budget and security for each new attempt. Independent leaves continue; dependent leaves wait.
- **Runtime notification:** dashboard may show live status and audit log unobtrusively. Avoid conversational questions or push notifications for routine retries.
- **Critical blocker requiring approval:** remaining authorized budget insufficient; exposure cannot be bounded; new scope or DAG changes necessary; need for new credentials/provider account/repository rights; security incident requiring owner action; mandated external legal/compliance approval; exhausting authorized recovery options where completion requires a new authorization. Pause **only the affected new billable work** (or fence ongoing work on stop-on-risk), preserve completed work, and request one consolidated actionable decision.
- **Safety:** independently enforce stop-on-risk and revocation immediately, without waiting for a user response when continued spending or privilege would be unsafe. An incident may be reported even if user cannot act immediately.
- **Completion:** provide concise outcome, relevant tests, costs and exceptions; avoid unnecessary intermediate questions.

## Non-negotiable compatibility

Approved ADR 0009: group parents only aggregate validated descendants, never launch paid worker tasks. Approved ADR 0010: bounded in-flight spend may continue only while reserved and enforceable; otherwise stop and reconcile. Approved ADR 0011: provider-neutral orchestration, no user's personal computer integration. Fixed upfront DAG cannot gain runtime task nodes. Retried attempt may differ in authorized model/effort/provider, but must retain same task identity, acceptance criteria and explicit fresh cost reservation.

## Acceptance examples

1. Preflight authorizes Codex and Claude Sonnet/Opus with effort range and cost cap. Codex fails: main agent chooses eligible Claude model and effort within policy and budget; **no new question**.
2. A transient provider error with confirmed no prior paid job and remaining allowed retry count: retry automatically within cap.
3. A new provider would require a new cloud account or an unapproved location: do **not** switch silently; pause affected attempt and ask permission.
4. Hard budget exhausted: no new billable attempt, even if the model predicts it will succeed; present forecast and explicit approval request.
5. Unexpected security compromise: revoke/fence first, preserve evidence; alert appropriately, without treating silence as approval.
6. Retry options exhausted: main agent diagnoses and consolidates alternatives before requesting an actionable decision, rather than forwarding every worker error.

## Remaining details

Concrete approval UX, question grouping, critical alert channels, service-level defaults and numeric retry caps remain implementation decisions; they are not silently approved by this product principle.

Related: [retry policy](../architecture/retry-escalation-policy.md), [model entitlement](../architecture/model-access-entitlement-contract.md), [bounded spending ADR](../decisions/0010-inflight-budget.md), [worker auth](../architecture/worker-authentication-policy.md).
