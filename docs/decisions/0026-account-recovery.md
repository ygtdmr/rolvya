---
title: "ADR 0026: Secure human account recovery"
status: Approved
date: 2026-10-22
---

# ADR 0026: Secure human account recovery

## Context
Rolvya initially supports email/password or verified-email Magic Link/OTP with optional app-based TOTP ([ADR 0033](0033-configurable-password-email-login-and-totp.md)), and Owner, Editor, Viewer roles (ADR 0024). OIDC/passkeys from ADR 0025 are optional conditional federation controls. Device or authenticator loss must not silently weaken access to projects, secrets, budgets or privileged actions.

## Decision
- Protect recovery from loss of an enrolled authenticator-app TOTP factor. Independent backup passkeys/hardware authenticators are optional for separately enabled future support, not a v0.1 login prerequisite.
- For federated identities, primary account recovery remains with the configured OIDC identity provider. Rolvya must revalidate the recovered identity using immutable provider issuer and subject identifiers, not email address alone.
- **Password reset is supported when password login is enabled (ADR 0033).** With operational mail, require bounded one-use verified recovery. **Without configured/working outbound email**, display privacy-preserving **contact the instance administrator** guidance rather than claiming an email was sent. Administrator assistance must independently verify identity, apply narrowly scoped auditable recovery, protect TOTP and revoke affected sessions as necessary. Mailbox ownership alone never restores critical Owner approval rights.
- Treat recovery and addition/replacement of authenticators as high-risk security events. Revoke or rotate Rolvya sessions where technically feasible, notify the user over existing verified channels, record auditable events without secrets, and require fresh strong authentication before privileged operations.
- Owner privileges, including budget increases and security-setting changes, must **not automatically become usable merely because any password, email or federated account was recovered**. Require a verified, recent, sufficiently strong authentication event before privileged access is restored; when assurance cannot be established, suspend sensitive operations until an approved recovery review succeeds.
- Support a controlled exceptional recovery pathway for lockouts, no-email configurations and optional identity-provider failures, with independently verified evidence and multiple authorized approvals where practical (for example another already-authenticated Owner), and a risk-based delay for critical changes. A second Owner's approval alone is not proof of identity. Avoid a permanent lockout where a deployment has only one Owner; require a predefined, audited administrator emergency process and never a hidden backdoor.
- Locally enrolled TOTP recovery codes require one-use protection, revocation, audit and safe re-enrollment. If future federation offers recovery codes, the IdP manages them. A recovery code alone is not phishing-resistant and cannot automatically authorize sensitive Owner actions.
- Account recovery and identity linking must bind a stable internal account ID and verified credentials; never merge by a matching email string or trust a reassigned mailbox. Optional federated issuer/subject changes require explicit verification.

## Implementation status
Approved architecture, **not implemented**. Exact assurance claim mapping, recovery approval quorum, time delays, notifications, break-glass process and self-hosted deployment constraints require security design and testing before production.

## Related
- [ADR 0024 — Project human roles](0024-project-human-roles.md)
- [ADR 0025 — OIDC and passkeys](0025-oidc-passkey-authentication.md)
