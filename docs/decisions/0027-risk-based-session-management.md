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


## Approved refinement — low-friction secure sessions (2026-10-22)
- Secure, revocable session renewal should keep routinely active **web users** signed in without repeated visible sign-in prompts. If a future voice interface is separately approved, its low-risk interactions should follow compatible security principles; this is **not** a feature commitment.
- Keep web sessions eligible for termination after **7 days without real user activity**. Backend jobs, scheduled agents, token refreshes, background polling and worker activity **do not** reset this user-idle clock.
- If a mobile app is **ever separately approved**, evaluate platform secure storage and revocable credentials, with separately risk-reviewed session lifetimes. No mobile app, mobile-specific credential store or voice interface is required or currently approved for delivery.
- **Do not impose a blanket mandatory logout every 30 days** as a product rule. Retain bounded credential/session lifetimes, revalidation on risk, server-side revocation and provider-enforced reauthentication as needed; do not infer infinite session validity.
- Keep sensitive operations behind verified step-up authentication. Spoken commands or voice similarity alone cannot authorize budget increases, role/security changes or other high-risk actions.
- Maintain device-specific session inventory and revocation. Logout or compromise of a human session must not automatically terminate previously authorized ongoing agent or remote-worker execution; these have separate machine credentials and budget/security gates.
- Token rotation, replay detection, secure storage and theft response need an explicit implementation policy and tests before launch.

## Scope clarification — optional future interfaces (2026-10-22)
Mobile applications and Jarvis-like voice interaction are **possible future concepts only**, not approved roadmap commitments, v1 requirements, or dependencies for implementing the web session architecture. The current session-security baseline must stand alone for the web product. If either interface is later approved, it should reuse appropriate risk-based security principles with a separate design and threat-model review; no work on those interfaces is required by this ADR.

## Open implementation details
Exact idle/absolute timeout values, maximum simultaneous sessions, trusted-device handling, recent-authentication window, session storage, high-risk action catalog, and OIDC assurance mapping are **not yet approved**. Choose them during security implementation with explicit review and tests. No session-management functionality is claimed implemented.

## Rationale and trade-offs
Balances usability against account takeover risk; server-side revocation and privileged reauthentication add state and integration complexity. Reject permanent sessions and password-only locally built authentication as v1 defaults.

## Related
- [ADR 0024 — Project human roles](0024-project-human-roles.md)
- [ADR 0025 — OIDC and passkeys](0025-oidc-passkey-authentication.md)
- [ADR 0026 — Secure account recovery](0026-account-recovery.md)
