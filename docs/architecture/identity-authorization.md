---
title: Identity, Authorization and Human Approval Model
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [identity, rbac, authorization, approvals, security]
---

# Identity, authorization and human approval model

## Status and scope

**Proposed security architecture, not deployed authentication.** The user approved a **hard human approval gate for budget increases**, a fixed DAG, remote workers only and remote Git providers only. They have **not** selected an identity provider, authentication standard, role model or tenant topology. Rust is the approved backend; SQLite is authoritative state. The implementation **must not infer authority** from possession of a task lease, a webhook, an LLM message, an API callback or UI notification acknowledgement.

## Trust principals

| Principal | Identity source to select | Allowed responsibility | Forbidden |
| --- | --- | --- | --- |
| Project owner | Authenticated human session, bound to project owner ID | Configure projects; issue explicit budget approval; authorize consequential ADR/merge changes | Implicit approval by page refresh or copied token |
| Invited reviewer | Authenticated delegated human, role assignment | Examine code, tests, audit and Git integration subject to approved policy | Automatic project ceiling changes unless separately delegated by explicit owner policy |
| Main planner/orchestrator | Rust backend internal/service identity | Propose full DAG, request validation, schedule and manage attempts | Approve budget on behalf of human; mutate activated DAG |
| Remote worker | Enrolled short-lived remote instance identity | Claim authorized attempt, heartbeat and submit bounded artifacts | Write SQLite, add tasks, raise budget, bypass merge or access other projects |
| Provider adapter | Dedicated service identity bound to allowed project/provider | Issue approved cloud/Git actions through controls | Broad unmanaged cloud or repository administration |
| GitHub installation | Per-installation/project-authorized GitHub App identity (proposed) | Read designated repos; publish isolated refs subject to GitHub permissions | Assume a token itself restricts to branch namespace |
| Public/unauthenticated viewer | None | Read explicitly public documentation only | See private logs, remote refs, task context, charges or secrets |

Roles are illustrative; exact RBAC/ABAC policy and whether reviewer/owner roles are combined need owner approval.

## Authorization decision inputs

Any privileged action must check `actor_id`, `actor_type`, authenticated session/worker identity, project membership, requested action, target resource and expected version/lease epoch. Cost-affecting actions additionally check approved ceiling, provider estimate/limit and budget reservations. Git operations check repository installation, pinned base SHA, allowed remote ref, and merge authority. Plan activation must verify the entire graph and approved constraints.

Enforce server-side policy **on every request and outbox execution**, not just in a UI disabled button. Revalidate when an action waits in an outbox; permissions can be revoked after initial scheduling.

## Minimum proposed authorization matrix

| Action | Owner | Reviewer | Controller | Worker |
| --- | --- | --- | --- | --- |
| Read own project DAG/results | Yes | Only granted | Scoped | Only assigned task |
| Propose/upfront-validate plan | Request | Review | Yes | No |
| Activate validated plan | Approval policy TBD | No | Only with authorized plan gate | No |
| Admit budget-covered attempt | No direct lease | No | Yes, in SQLite transaction | No |
| Increase approved cost ceiling | **Explicit authenticated approval** | No by default | No, only verifies and records authenticated approval | **No** |
| Change global Rust/SQLite decision | Explicit new ADR approval | Propose | No | No |
| Read task context | Project-scoped | Scope-dependent | Yes | Own fenced attempt only |
| Publish isolated remote Git ref | Through controlled integration | Review | Scoped adapter | Only under delegated attempt |
| Merge protected remote branch | Per approved merge policy | Per approved merge policy | Controlled, requires verified authority | No |

## Human budget approval flow (proposal)

1. Controller detects insufficient headroom and persists **pending** increase request with amount/currency, existing ceiling, justification, pending tasks and four cost categories.
2. Owner reviews an authenticated, project-bound confirmation showing total *new* ceiling, **increment**, existing settled/reserved/unknown exposure and optimistic/expected/pessimistic scenarios.
3. Owner deliberately confirms the exact new ceiling; a durable SQLite transaction records actor, time, request ID, prior/new value and reason.
4. Scheduler rechecks DAG, capacity, authorization and budget; approval does not automatically dispatch an unsafe task.
5. Rejected, expired or abandoned approval leaves the ceiling unchanged. Multiple concurrent approval requests need version checks to prevent lost updates.

This workflow is proposed implementation detail; explicit approval and no automatic overrun are already approved requirements.

## Threats, mitigations and tests

- **Worker impersonation:** short-lived credentials and cryptographic identity (mechanism TBD), attempt-scoped capabilities and fencing.
- **Cross-project ID probing:** project-bound SQL queries/policy; ignore user-supplied foreign IDs even if syntactically valid.
- **Approval replay/CSRF/stale session:** explicit human action, request nonce/idempotency and server-side version checking; authentication/CSRF mechanism to be chosen.
- **Prompt-injected repository text:** never treat external text as a privileged human instruction.
- **Credential exfiltration:** secrets held outside model-visible context; scrub logs and rotate/revoke on incident.
- **App installation revoked:** re-check before Git write; fence tasks and preserve completed work.
- **Duplicate budget click:** one approved change with unique request ID, never multiple additive increases.
- **Backend outage:** no automated override; SQLite approval history remains authoritative.

All protected operations require auditable traces with no raw credential content. Policy service unavailable means no new privileged/billable admission.

## Decisions needed

Identity provider/session architecture; tenant model; enrollment and worker attestation; roles/delegation; protected Git branch merge approval; approval UI/2FA requirements; audit retention; secret manager and project isolation enforcement.

The [human approval workflow](approval-workflow.md) defines proposed request/decision/application state separation; a displayed approval request never itself authorizes billable admission.

Related: [security](security.md), [GitHub App design](github-app-integration-design.md), [budget reconciliation](budget-reservation-reconciliation.md), [API contracts](api-contracts.md), [open questions](../project/open-questions.md).
