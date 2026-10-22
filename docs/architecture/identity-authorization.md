---
title: Identity, Authorization and Human Approval Model
status: Proposed
category: architecture
last-reviewed: 2026-10-22
tags: [identity, rbac, authorization, approvals, security]
---

# Identity, authorization and human approval model

## Status and scope

**Proposed security architecture, not deployed authentication.** The user approved a **hard human approval gate for budget increases**, a fixed DAG, remote workers only and remote Git providers only. **ADR 0024 approved three project-scoped human roles: Owner, Editor and Viewer.** **OIDC-first sign-in and passkey-capable providers are Approved ADR 0025; secure recovery is Approved ADR 0026; low-friction risk-based web session management is Approved ADR 0027.** Specific IdP vendors, assurance mapping, secure token/renewal implementation, concrete tenancy topology and remaining fine-grained authorization rules are still unselected. Rust is the approved backend; SQLite is authoritative state. The implementation **must not infer authority** from possession of a task lease, a webhook, an LLM message, an API callback or UI notification acknowledgement.

## Approved human roles and separate system principals

[ADR 0024 — Owner, Editor, Viewer](../decisions/0024-project-human-roles.md) is the binding minimum human role definition. A project's Owner may explicitly authorize or raise its budget ceiling, approve consequential scope and critical permissions. An Editor may initiate/review **already owner-approved** work within preflight limits, but cannot approve higher spending, broaden provider/Git/secret permissions or silently replace the activated immutable DAG. A Viewer may only read authorized project information. Roles are **project-scoped** and are checked server-side at each privileged action, not inferred from a visible UI control. OIDC/passkey and risk-based session **principles** are Approved ADRs 0025–0027; core credential renewal/rotation/replay protection and device-scoped revocation are **Approved ADR 0028**. Their concrete Rust session/token implementation contract remains [Proposed](session-token-rotation-and-theft-response.md).

| Principal | Identity source | Allowed responsibility | Forbidden |
| --- | --- | --- | --- |
| **Owner** | Authenticated, project-bound human session | Explicit budget approval, critical project governance/access, preflight plan and scope authorization | Implicit approval from silence, copied token or UI event |
| **Editor** | Authenticated project-bound membership | Begin/manage an already-approved run within existing ceiling/policy; inspect permitted results | Increase ceiling, elevate roles, widen sensitive provider/Git scope, approve unapproved DAG |
| **Viewer** | Authenticated project-bound membership | Read permitted status/results only | Launch, alter, approve, or access secrets |
| **Main planner/orchestrator** | Rust backend service identity | Propose fixed plans and manage authorized same-task recovery | Approve human budget, mutate activated DAG |
| **Remote worker** | Verified short-lived identity scoped to current attempt | Claim, heartbeat and submit own bounded artifacts | Write SQLite, create tasks, act as project human, access other projects |
| **Provider adapter** | Dedicated service identity bound to allowed project/provider | Execute pre-reserved actions through policy controls | Unmanaged cross-tenant resources or broad cloud administration |
| **GitHub installation** | Approved GitHub App installation/token | Limited remote Git integration on authorized refs | Treat installation permission as automatic protected-ref merge authority |
| **Public/unauthenticated** | None | Public repository documentation only | Private projects, logs, task context, costs, credentials |

No fourth initial human role (such as an independent **Reviewer**) has been approved. Reviewer workflows may be covered by appropriately permissioned existing roles; fine-grained review/merge authorities are not automatically implied by Editor.

## Approved invitation and ownership controls (ADR 0029)

[ADR 0029](../decisions/0029-owner-controlled-project-invitations.md) settles the product security rules: **only a current project Owner may issue project invitations**; an Editor cannot invite even Viewers. Normal Editor/Viewer invitations are explicitly project/role scoped, **expire**, are **single-use**, and do not grant membership until trusted **OIDC authentication** and applicable recipient binding checks succeed. An invite link/email by itself is insufficient authorization.

Owner promotion or ownership transfer is a **separate high-risk action** requiring verified recent strong reauthentication by the initiating Owner, independent OIDC login and **explicit acceptance by the recipient**. The current Owner remains authorized until the final transfer commits atomically; failed/expired transfers do not remove that Owner. Credential identity is based on stable IdP issuer+subject rather than email alone. Rust must enforce the project role, targeted invite and one-time redemption in **atomic SQLite state changes**, including replay and concurrent acceptance, and re-check permissions on queued effects. The **approved default invitation lifetime is 24 hours**; email service, transfer handoff UX, IdP recipient matching, membership schema and owner-quorum protocol remain Proposed.

## Approved multiple-Owner projects

A project may have **more than one Owner** under [ADR 0029](../decisions/0029-owner-controlled-project-invitations.md). Every Owner must authenticate as their own verified OIDC principal. New Owner promotion requires strong reauthentication by an existing Owner plus independent recipient authentication and explicit acceptance. Rust/SQLite must prohibit concurrent removal/demotion of the **last remaining Owner**. The presence of multiple Owners does not implicitly change the separately approved hard-budget gate or authorize agents to grant human roles. Co-Owner voting/quorum requirements beyond the two-party grant/transfer remain open.

## Authorization decision inputs

Any privileged action must check `actor_id`, `actor_type`, authenticated session/worker identity, project membership, requested action, target resource and expected version/lease epoch. Cost-affecting actions additionally check approved ceiling, provider estimate/limit and budget reservations. Git operations check repository installation, pinned base SHA, allowed remote ref, and merge authority. Plan activation must verify the entire graph and approved constraints.

Enforce server-side policy **on every request and outbox execution**, not just in a UI disabled button. Revalidate when an action waits in an outbox; permissions can be revoked after initial scheduling.

## Approved minimum human permissions, proposed implementation detail

| Action | Owner | Editor | Viewer | Controller | Worker |
| --- | --- | --- | --- | --- | --- |
| Read authorized project DAG/results | Yes | Yes | Read-only | Project-scoped | Assigned attempt only |
| Authorize new preflight plan/scope | Yes | Propose only | No | Validate but no human approval | No |
| Start/resume an already owner-approved run | Yes | Yes within delegated policy | No | Budget/cap/lease gate | No |
| Admit a billable attempt | Not a direct lease | Not a direct lease | No | **Yes**, only after atomic authorized reservation | No |
| Increase hard budget ceiling | **Explicit authenticated approval** | **No** | **No** | Records only genuine owner approval | **No** |
| Grant/revoke critical project access, provider account or sensitive Git scope | Owner authorization | **No unilateral grant** | No | Enforce | No |
| Read model/provider raw secrets | Not via ordinary app UI | No | No | Scoped broker/secret store only | No raw general secrets |
| Publish remote isolated ref | Through allowed integration | No implied direct branch authority | No | Trusted publisher under current scope | Scoped evidence, not branch merge |
| Merge protected branch | Only if separately authorized GitHub policy | Not implied by role | No | Verify independent merge rule | No |

**Editors cannot invite any project members (Approved ADR 0029)**. Whether Editors may edit descriptions or request a protected merge is **not yet settled**. Treat uncertain privilege as denied until specified; an Editor pressing Start is not authority to create a new plan or exceed preflight spending limits.

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

## Approved project audit visibility

[ADR 0030](../decisions/0030-project-activity-audit-log.md) requires project-scoped, protected activity history. Owner/Editor may read authorized audit data, Viewer may only access an approved read-only subset. No ordinary role may edit/delete history. Sensitive identities, detailed security evidence and credentials require finer least-privilege checks and redaction. Human actions and agent/worker effects must have distinguishable, truthful actor attribution; a log entry alone does not authorize spending or a privileged change.

## Decisions needed

Exact OIDC provider selection, identity/session **implementation and assurance mapping** (OIDC and passkey capability themselves Approved ADR 0025; human session policy Approved ADR 0027); tenant model; enrollment and worker attestation; **role membership persistence, invitation delivery and transfer workflow specifics plus finer Editor scopes** (human roles Approved ADR 0024; invitation/Owner transfer protections Approved ADR 0029); protected Git branch merge approval; approval UI/2FA requirements; audit retention; secret manager and project isolation enforcement.

The [human approval workflow](approval-workflow.md) defines proposed request/decision/application state separation; a displayed approval request never itself authorizes billable admission.

Related: [security](security.md), [GitHub App design](github-app-integration-design.md), [budget reconciliation](budget-reservation-reconciliation.md), [API contracts](api-contracts.md), [open questions](../project/open-questions.md).
