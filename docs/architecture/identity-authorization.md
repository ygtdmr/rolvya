---
title: Identity, Authorization and Human Approval Model
status: Proposed
category: architecture
last-reviewed: 2026-10-22
tags: [identity, rbac, authorization, approvals, security]
---

# Identity, authorization and human approval model

## Status and scope

**Implemented library boundary:** [S2-03C implementation notes](../development/human-owner-authorization.md) identify the current Rust/SQLite roles, invitations, independent recipient acceptance and transaction-bound budget/governance effects. The broader deployment/UX design below remains a specification. Production human enrollment, legacy-project ownership binding, browser and mail adapters are unavailable by default; no deployed authentication or permission service is claimed.

**Proposed security architecture, not deployed authentication.** The user approved a **hard human approval gate for budget increases**, a fixed DAG, remote workers only and remote Git providers only. **ADR 0024 approved three project-scoped human roles: Owner, Editor and Viewer.** **Conditional federated OIDC/passkey safeguards remain in ADR 0025; secure recovery is Approved ADR 0026; low-friction risk-based web session management is Approved ADR 0027.** **ADR 0033 approves password or verified-email primary login (Magic Link/OTP), plus optional authenticator-app TOTP 2FA; no SMS. Owner selects at least one primary method initially and can later enable more.** Trusted broker/provider protocol, email delivery, assurance mapping, secure token/renewal implementation, concrete tenancy topology and remaining fine-grained authorization rules are still unselected. Rust is the approved backend; SQLite is authoritative state. The implementation **must not infer authority** from possession of a task lease, a webhook, an LLM message, an API callback or UI notification acknowledgement.

## Approved human roles and separate system principals

[ADR 0024 — Owner, Editor, Viewer](../decisions/0024-project-human-roles.md) is the binding minimum human role definition. A project's Owner may explicitly authorize or raise its budget ceiling, approve consequential scope and critical permissions. An Editor may initiate/review **already owner-approved** work within preflight limits, but cannot approve higher spending, broaden provider/Git/secret permissions or silently replace the activated immutable DAG. A Viewer may only read authorized project information. Roles are **project-scoped** and are checked server-side at each privileged action, not inferred from a visible UI control. Risk-based sessions are Approved ADR 0027, with conditional OIDC/passkey safeguards in ADR 0025; core credential renewal/rotation/replay protection and device-scoped revocation are **Approved ADR 0028**. Their concrete Rust session/token implementation contract remains [Proposed](session-token-rotation-and-theft-response.md).

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

[ADR 0029](../decisions/0029-owner-controlled-project-invitations.md) settles the product security rules: **only a current project Owner may issue project invitations**; an Editor cannot invite even Viewers. Normal Editor/Viewer invitations are explicitly project/role scoped, **expire**, are **single-use**, and do not grant membership until verified **Owner-enabled password or verified-email login plus their voluntarily enabled TOTP, if any**, with applicable recipient binding checks. An invite link/email by itself is insufficient authorization.

Owner promotion or ownership transfer is a **separate high-risk action** requiring verified recent strong reauthentication by the initiating Owner, independent login through an enabled primary method and any voluntarily enrolled TOTP, plus **explicit acceptance by the recipient**. The current Owner remains authorized until the final transfer commits atomically; failed/expired transfers do not remove that Owner. Identity is bound to a stable internal human account ID and verified credential binding; future federation also verifies IdP issuer+subject rather than email alone. Rust must enforce the project role, targeted invite and one-time redemption in **atomic SQLite state changes**, including replay and concurrent acceptance, and re-check permissions on queued effects. The **approved default invitation lifetime is 24 hours**; email service, transfer handoff UX, account/recipient matching, membership schema and owner-quorum protocol remain Proposed.

## Approved multiple-Owner projects

A project may have **more than one Owner** under [ADR 0029](../decisions/0029-owner-controlled-project-invitations.md). Every Owner must independently authenticate as their own durable human account through a supported enabled method and any voluntarily enrolled TOTP; federation, when enabled, additionally verifies issuer+subject. New Owner promotion requires strong reauthentication by an existing Owner plus independent recipient authentication and explicit acceptance. Rust/SQLite must prohibit concurrent removal/demotion of the **last remaining Owner**. The presence of multiple Owners does not implicitly change the separately approved hard-budget gate or authorize agents to grant human roles. Co-Owner voting/quorum requirements beyond the two-party grant/transfer remain open.

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

## P0 implementation elaboration (proposal only)

See [human authentication/recovery/Owner step-up implementation contract](human-authentication-implementation-contract.md) for proposed Rust/SQLite entities, first setup, password/email login, optional TOTP, no-email administrator support, session handling, risk/step-up protection and thirteen-plus negative tests. **No new Owner product policy is approved by that draft.** Especially unresolved: which fresh non-TOTP reauthentication with risk controls permits a high-risk Owner action without forcing enrollment; what independent evidence permits bootstrap/recovery; and who administers service-wide options vs project preferences.

## Approved action-specific Owner approval (ADR 0034)

[ADR 0034](../decisions/0034-owner-action-bound-reauthentication.md) approves **fresh current password** (password-enabled account) or **new transaction-bound email Magic Link/OTP** (email-only account), an explicit confirmation of the exact sensitive change, an **atomic one-use proof and permission check** in Rust/SQLite, and **denial/hold when compromise is credibly suspected**. No Owner must enroll TOTP. **Approved**: 5-minute exact-action proof lifetime, 10-minute email Magic Link/OTP TTL, five failed email OTP submissions per challenge, single-use token/action consumption. Other rate limits, risk thresholds and account recovery remain unselected engineering details.

## Owner clarification (approved 2026-10-24)

**TOTP is entirely opt-in**, including for project Owners, budget increases, ownership changes and security operations ([ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md)). A recent user-initiated reauthentication and explicit Owner authorization still protect privileged actions, but risk-safe non-TOTP verification must be available for users who chose not to enroll. Never portray a single password or email challenge as MFA. Details of non-TOTP step-up, fraud/risk refusal and account recovery remain P0 engineering work.

## Decisions needed

Identity/session **implementation and method-specific assurance mapping** (current methods approved ADR 0033, conditional OIDC safeguards ADR 0025, web session policy ADR 0027); tenant model; enrollment and worker attestation; **role membership persistence, invitation delivery and transfer workflow specifics plus finer Editor scopes** (human roles Approved ADR 0024; invitation/Owner transfer protections Approved ADR 0029); protected Git branch merge approval; approval UI/2FA requirements; audit retention; secret manager and project isolation enforcement.

The [human approval workflow](approval-workflow.md) defines proposed request/decision/application state separation; a displayed approval request never itself authorizes billable admission.

Related: [security](security.md), [GitHub App design](github-app-integration-design.md), [budget reconciliation](budget-reservation-reconciliation.md), [API contracts](api-contracts.md), [open questions](../project/open-questions.md).

## Approved human login configuration (ADR 0033)

[ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md) **supersedes ADR 0032**. Approved primary human login options: email/password and password-free verified email via Magic Link or OTP. Authenticator-app TOTP 2FA is an additional optional factor, **not standalone primary login**; no SMS required. Owner selects at least one primary login method at initial setup and may later enable other methods under recent strong authorization. Protect password verifiers with memory-hard salted hashing and secure recovery; encrypt TOTP seeds; enforce one-time email challenges, anti-enumeration, role checks and high-risk Owner step-up. Google/GitHub social login is no longer an initial v0.1 requirement; GitHub App repo authorization is separate.

### Approved default email sign-in experience (2026-10-23)

Magic Link remains the default email-only entry option; one-time email OTP is an equally supported alternative. ADR 0033 also permits password sign-in and authenticator-app TOTP. Both require short-lived atomic one-use challenges, browser/session binding protections, enumeration resistance, rate limiting and safe account linking. Neither method alone substitutes for strong Owner reauthentication. See current [ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md). This is an approved UX policy, not deployed authentication.

### Unconfigured email service: administrator-assisted password recovery

The current [ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md) requires a privacy-preserving **contact the instance administrator** instruction when password reset email cannot be delivered; the UI must not pretend a message was sent. Administrator-assisted recovery requires independent identity checks, narrowly scoped auditable recovery, retained TOTP/Owner safeguards and appropriate session revocation. **Email-only primary login cannot be enabled without verified working outbound email.** The exact administrator recovery implementation remains P0; [AC-125–127](../product/acceptance-criteria.md) are future tests only.
