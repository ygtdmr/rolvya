---
title: Owner-controlled Project Invitations and Secure Ownership Changes
status: Approved
category: decisions
last-reviewed: 2026-10-22
tags: [auth, identity, rbac, invitations, owner, editor, viewer, transfer]
---

# ADR 0029 — Owner-controlled project invitations and secure ownership changes

- **Status:** Approved
- **Date:** 2026-10-22
- **Owner decision:** Only a project **Owner** can invite new project members; invitations are expiring, one-use, and require verified OIDC identity before becoming project access. Owner elevation or ownership transfer is a separately protected, strong-reauthentication action.

## Context

Rolvya already has [Owner / Editor / Viewer](0024-project-human-roles.md), [OIDC-first identity with passkey-capable providers](0025-oidc-passkey-authentication.md), [secure account recovery](0026-account-recovery.md), and [risk-based strong step-up for critical changes](0027-risk-based-session-management.md). An invitation link cannot be allowed to bypass those protections or silently grant cloud costs, GitHub credentials, another project's resources, or ownership.

## Approved human permission rules

1. **Owner-only invitations:** Only an authenticated human currently authorized as this project's **Owner** may create, revoke or manage project invitations and grant initial role assignments. An Editor may operate already-approved work but **cannot** invite anyone, including a Viewer. A Viewer may only read specifically allowed data.
2. **Scoped initial roles:** A normal invitation targets one explicit project and a role no greater than **Editor or Viewer**. Granting **Owner** authority is a distinct high-risk elevation/ownership workflow, not an unchecked flag on an ordinary invite. Never silently inherit organization/other-project access.
3. **Time-limited, one-use invitation:** Every invitation expires **24 hours after issuance (approved default)**, can be revoked before redemption, and is consumed **at most once**. Creating multiple invitation links must not cause accidental duplicated role grants or privilege escalation.
4. **Authentication before membership:** A link alone is not proof of identity. Acceptance requires completing trusted **OIDC login** and binding the new membership to that authenticated person's durable provider issuer and subject (`iss` + `sub`, or an appropriately verified equivalent). If a specific recipient was targeted, verify permitted recipient binding against trusted provider identity evidence; an email address alone is **not** adequate as a permanent account key or proof of Owner authority. On mismatch, fail closed and provide a safe way to request a new invite.
5. **Owner elevation and transfer:** Any promotion to Owner and any transfer of ownership require a separately authorized, audited, intentional operation with **recent strong reauthentication** from the acting Owner. The receiving principal **must authenticate through trusted OIDC and explicitly accept** the ownership responsibility before transfer can complete. **Do not** infer an Owner's consent from an Editor clicking a link or from an agent's response. **Keep the initiating Owner's authority unchanged until the recipient's verified acceptance and the whole transfer successfully commits atomically**; reject a failed or expired transfer without removing the prior Owner. Protect against orphaned projects / accidental loss of the last Owner.
6. **Server-side enforcement:** Rust checks invitation creator and redeemer authorization, project, configured role, identity, expiry, revocation, nonce/one-use state, and current project membership. **SQLite must atomically** consume an invitation and grant membership with concurrency-safe idempotency. UI hiding and email-link secrecy are insufficient.
7. **Hard financial and security gates unaffected:** New Editor membership does not confer the ability to increase budget, change critical provider/Git/security scope, or activate an unapproved DAG. Worker, main-agent and GitHub App identities are distinct system principals and can never redeem human invitations.
8. **Audit and notification:** Record issuance, delivery attempt where relevant, redemption, rejection, expiration, revocation and Owner changes without putting raw invite tokens in logs. Notify affected people over verified channels for material access changes. Allow an Owner to inspect and revoke pending invitations and project memberships.
9. **Quiet operation:** Invitations are initiated explicitly by an Owner, not by an autonomous worker. Routine authorized execution remains uninterrupted, but role grants and Owner changes never occur silently.

## Design boundaries — NOT yet owner-approved implementation values

- Recipient targeting or re-send UX, invitation email delivery service, project-owner plurality, acceptance screens, verified-email matching policy, notification retention and rate-limit thresholds.
- Exact transfer request lifetime, any additional Owner signoff if present, cooldown/recovery process, self-hosted IdP linking and final transfer UX. **The current Owner's strong reauthentication, recipient's independent authentication and explicit acceptance, and preservation of current Owner authority until atomic completion are Approved.**
- Atomic schema for pending invites, token hashing at rest, email enumeration defenses and race resolution. These are proposed engineering concerns, not verified code.

## Security testing / future acceptance (no tests executed)

- Editor or Viewer attempts to invite any role, revoke another member, or promote themselves → reject and audit.
- Owner issues time-limited Editor/Viewer invite; untrusted link bearer alone accesses project → reject until trusted OIDC login, recipient checks and valid redemption.
- Same invite redeemed concurrently twice → **exactly one** membership grant; no double assignment.
- Invite redeemed at or after its 24-hour expiry, revoked/altered invite or invalid project/role → no access grant; clear error without account enumeration.
- Invite addressed to Alice is opened under a mismatched authenticated identity → deny; do not grant to a changed-email holder without verified account binding.
- Member is removed after invite creation; stale roles, queued actions and cached permissions cannot restore access.
- Owner promotion without recent strong assurance or with unverified target → deny; ordinary Editor invite cannot be promoted by changing request payload.
- Owner transfer cannot silently leave the project ownerless; require independent recipient acceptance and reauthentication by initiating Owner, and preserve prior Owner until final transaction completes.
- Inviting Editor or Viewer never creates a new DAG node, raises the ceiling, or grants GitHub App/private AI keys.

## Alternatives and consequences

- **Rejected:** invitations from any Editor, permanent reusable bearer invites, email-link-only login, self-serve Owner promotion.
- **Chosen:** deliberate Owner-only project-scoped, one-use OIDC-verified invitations with separate Owner elevation/transfer.
- **Cost:** backend membership controls, expiring invitation state and audited high-risk operations; no added user interruptions for normal coding workflow.

## Related contracts

- [ADR 0024: roles](0024-project-human-roles.md)
- [ADR 0025: OIDC/passkey](0025-oidc-passkey-authentication.md)
- [ADR 0027: session step-up](0027-risk-based-session-management.md)
- [Identity and authorization](../architecture/identity-authorization.md)
- [Product acceptance criteria](../product/acceptance-criteria.md)
