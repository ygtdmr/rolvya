---
title: "ADR 0027: Risk-based human session management"
status: Approved
date: 2026-10-22
---

# ADR 0027: Risk-based human session management

## Context
Rolvya uses OIDC-first human authentication, passkey-capable identity providers (ADR 0025), secure account recovery (ADR 0026), and Owner/Editor/Viewer roles (ADR 0024). Requiring sign-in before every ordinary action harms usability, while unrestricted long-lived sessions endanger budgets and security settings.

## Approved decision
- **Low-friction normal use:** Authenticated users may perform permitted routine actions within a valid session without repeating login for every operation.
- **Step-up for sensitive actions:** Require recent strong reauthentication, with verified assurance from the chosen identity provider, before budget increases, changing critical security settings, managing user roles/access, and equivalent privileged operations. Do not equate a routine OIDC login with verified passkey usage.
- **Session inventory and remote termination:** Users can inspect their currently active Rolvya sessions (with device/browser details and last activity where feasible) and revoke individual other sessions or all sessions. Enforce revocation server-side, promptly, not just by removing browser cookies.
- **Secure server-managed sessions:** Prefer opaque session identifiers in Secure, HttpOnly and appropriate SameSite cookies over storing browser tokens in local storage; use CSRF defenses and session rotation where appropriate.
- **Expiration and risk controls:** Enforce idle and absolute timeouts, invalidate sessions after appropriate security events (including account recovery), and require reauthentication after suspicious activity. Apply server-side authorization for every request.
- **Provider boundary:** Revoking a Rolvya session does not necessarily revoke an external identity provider session. Explain this distinction in UI and use provider logout/revocation when available and safe.
- **Auditability:** Log session creation, step-up, revocation and suspicious events without leaking secrets.

## Open implementation details
Exact idle/absolute timeout values, maximum simultaneous sessions, trusted-device handling, recent-authentication window, session storage, high-risk action catalog, and OIDC assurance mapping are **not yet approved**. Choose them during security implementation with explicit review and tests. No session-management functionality is claimed implemented.

## Rationale and trade-offs
Balances usability against account takeover risk; server-side revocation and privileged reauthentication add state and integration complexity. Reject permanent sessions and password-only locally built authentication as v1 defaults.

## Related
- [ADR 0024 — Project human roles](0024-project-human-roles.md)
- [ADR 0025 — OIDC and passkeys](0025-oidc-passkey-authentication.md)
- [ADR 0026 — Secure account recovery](0026-account-recovery.md)
