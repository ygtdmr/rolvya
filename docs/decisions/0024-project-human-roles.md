---
title: Owner Editor Viewer Project Roles
status: Approved
category: decisions
last-reviewed: 2026-10-22
tags: [rbac, authorization, owner, editor, viewer, project, budget]
---

# ADR 0024 — Project human roles: Owner, Editor, Viewer

- **Status:** Approved
- **Date:** 2026-10-22
- **Owner decision:** Three project-scoped human roles: **Owner**, **Editor**, **Viewer**.

## Context

Rolvya should minimize user interruptions through a preflight authorization snapshot followed by a quiet autonomous runtime. Multiple people may be involved in one project, but their privileges must not implicitly authorize new money, credentials, scopes, provider accounts or changes to an activated immutable task DAG. Workers and the main agent have separate machine identities and cannot inherit the Owner role.

## Approved human roles

- **Owner:** project owner and final human authority for approving/increasing its hard budget, changing critical security permissions and project governance, and approving a new plan/scope when necessary. Human actions require authenticated, deliberate confirmation and an auditable SQLite record.
- **Editor:** may initiate or manage **already approved, policy-compliant project execution** and review its outcomes; may work within delegated project permissions. **Cannot** raise or authorize raising the budget ceiling, grant critical access, widen allowed model/provider/region/Git scopes, or silently replace the activated plan.
- **Viewer:** may only inspect the project status, DAG and **authorized** results/metrics according to data visibility rules. Cannot launch, modify or approve execution, spend or permissions. Never sees raw secrets merely due to Viewer membership.

This approval establishes the **role distinction and minimum rights**. It does **not** authorize Editors to activate new unapproved plans or initiate billable work outside the existing owner-approved preflight budget and access envelope. Exact editing privileges, owner transfer, invitation flows and granular merge/review authority are future authorization specifications.

## Baseline human permission matrix

| Operation | Owner | Editor | Viewer |
| --- | --- | --- | --- |
| Read permitted project status/results | Yes | Yes | Yes, read-only and scoped |
| Start/resume previously owner-approved eligible work inside approved cap | Yes | Yes | No |
| Inspect failed attempts and validation evidence | Yes | Yes (authorized scope) | Read-only if permitted |
| Request new permission/ceiling/plan change | Yes | May propose/request; no approval | No |
| Explicitly approve **higher budget ceiling** | **Yes, authenticated** | **No** | **No** |
| Change critical project security permissions/provider/repository scopes | **Owner authorization required** | **No** | **No** |
| Approve new plan/scope or immutable DAG supersession | **Owner authorization required** | **No** | **No** |
| Invite users, assign or elevate roles | Owner-controlled; exact mechanics TBD | No unilateral privilege escalation | No |
| Change protected-branch merge policy / grant merge authority | Owner-controlled; GitHub protection applies | Not implied by Editor status | No |
| View raw AI API secrets, GitHub App private keys, worker bootstrap keys | Not through ordinary project UI; dedicated secret administration separately governed | No | No |

The server must enforce these checks **per project and action**, regardless of frontend state, SDK path or agent/model output. An Owner's human approval cannot be inferred from a model response, an Editor action, a webhook or an unchecked API token. Being Editor or Viewer on project A confers **no** authority on project B.

## Separation of system identities

- **Main agent:** diagnoses failures and may choose authorized same-task fallback model/effort/provider; never approves a human budget increase.
- **Rust scheduler:** authoritatively checks project, identity, immutable plan, permitted capacity, provider entitlement, model, budget reservation and leases before admission.
- **Remote worker:** short-lived task/attempt-scoped identity; never acquires a human role, accesses another project or grants itself Git merge/budget approval.
- **GitHub App:** an independent, restricted integration identity; GitHub permissions and branch-protection rules remain authoritative, not an Editor-role shortcut.

## Required security/UX behavior

1. Preflight presents consequential owner choices together. After approval, routine execution and retries do not spam the owner for confirmation.
2. Editor clicking Run on an eligible, **already authorized** project does not bypass required owner preflight approval or spawn a new DAG.
3. If new cost, scope or privilege is necessary, save a pending approval request and **pause only affected new billable work**; no silent override. Keep independent eligible work and verified artifacts.
4. Record actor, role, project, policy/plan version and decision outcome for privileged actions, without exposing credentials.
5. Deny by default on missing, revoked or ambiguous membership; recheck roles at every sensitive action and at execution of queued side effects.

## Verification requirements (not executed)

- An authenticated Editor starts a previously approved run inside the remaining budget; Rust scheduler separately authorizes each attempt.
- An Editor attempts to raise the cap, install a broader GitHub App scope, add a provider account or activate an unapproved plan → denied, auditable request for Owner approval as appropriate.
- A Viewer tries to start/pause/mutate a run or view project secrets → denied; allowed status/results remain read-only.
- An Editor from project A cannot read or start project B without independent project membership.
- A worker/main-agent prompt or webhook tries to grant itself Owner privileges → rejected.
- A role is revoked after a job is queued but before a privileged external action → server revalidates and denies it when authorization is no longer valid.
- Owner approves budget increase using a versioned, explicit authenticated action; race/replay does not grant the increase twice.
- The authenticated owner and valid preflight envelope remain required regardless of scheduler fairness/concurrency mode.

## Remaining open engineering questions

Identity provider, authentication/session/MFA, project membership schema, whether owners may delegate *non-budget* permissions, invitation/revocation and owner-transfer flows, audit retention, UI presentation, plan approval workflows and exact Editor document-edit rights remain open. **The three role choices and hard budget separation are settled; do not re-ask them.**

Related: [Identity/authorization contract](../architecture/identity-authorization.md), [quiet runtime and preflight](../product/preflight-quiet-runtime-policy.md), [budget approval](0002-hard-budget-approval.md), [GitHub App](0008-github-authentication.md).
