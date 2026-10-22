---
title: "ADR 0026: Secure human account recovery"
status: Approved
date: 2026-10-22
---

# ADR 0026: Secure human account recovery

## Context
Rolvya uses OIDC-first sign-in with passkey-capable identity providers (ADR 0025), and project Owner, Editor, Viewer roles (ADR 0024). Device or authenticator loss must not silently weaken access to projects, secrets, budgets or privileged actions.

## Decision
- Encourage enrollment of at least one independent backup passkey or phishing-resistant hardware authenticator before loss occurs. Do not assume synced passkeys are independent recovery factors.
- For federated identities, primary account recovery remains with the configured OIDC identity provider. Rolvya must revalidate the recovered identity using immutable provider issuer and subject identifiers, not email address alone.
- Never implement a Rolvya password reset, email-only privilege restoration or an unverified self-service shortcut that bypasses the identity provider's protections in the initial release.
- Treat recovery and addition/replacement of authenticators as high-risk security events. Revoke or rotate Rolvya sessions where technically feasible, notify the user over existing verified channels, record auditable events without secrets, and require fresh strong authentication before privileged operations.
- Owner privileges, including budget increases and security-setting changes, must **not automatically become usable merely because an external account was recovered**. Require a verified, recent, sufficiently strong authentication event before privileged access is restored; when assurance cannot be established, suspend sensitive operations until an approved recovery review succeeds.
- Support a controlled exceptional recovery pathway for lockouts and identity-provider failures, with independently verified evidence and multiple authorized approvals where practical (for example another already-authenticated Owner), and a risk-based delay for critical changes. A second Owner's approval alone is not proof of identity. Avoid a permanent lockout where a deployment has only one Owner; require a predefined, audited administrator emergency process and never a hidden backdoor.
- Recovery codes, if offered by a chosen identity provider, are handled by that provider. Rolvya must not treat possession of a recovery code alone as equivalent to passkey phishing-resistance or sufficient evidence for immediate Owner-level sensitive actions.
- Identity provider changes and account re-linking must be reviewed as privileged events; prevent takeover by email reassignment or unverified OIDC subject changes.

## Implementation status
Approved architecture, **not implemented**. Exact assurance claim mapping, recovery approval quorum, time delays, notifications, break-glass process and self-hosted deployment constraints require security design and testing before production.

## Related
- [ADR 0024 — Project human roles](0024-project-human-roles.md)
- [ADR 0025 — OIDC and passkeys](0025-oidc-passkey-authentication.md)
