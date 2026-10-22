---
title: Human Session Renewal, Token Rotation and Theft Response
status: Proposed
category: architecture
last-reviewed: 2026-10-22
tags: [security, oidc, oauth, session, tokens, rotation, replay, csrf, account]
---

# Human session renewal, token rotation and theft response — implementation proposal

## Scope and authority

**Proposed implementation details, NOT implemented or individually approved.** This is a security-engineering design for the **already Approved** OIDC-first human sign-in ([ADR 0025](../decisions/0025-oidc-passkey-authentication.md)), secure account recovery ([ADR 0026](../decisions/0026-account-recovery.md)), risk-based user session continuity ([ADR 0027](../decisions/0027-risk-based-session-management.md)), and project-scoped Owner/Editor/Viewer roles ([ADR 0024](../decisions/0024-project-human-roles.md)). Work on any future mobile app or Jarvis-like voice interface is **NOT** authorized or required; both are optional concepts requiring a separate owner product decision.

This policy concerns **human UI sessions**. Remote workers' task-scoped HTTPS claim credentials, OIDC issuer signing keys, GitHub installation tokens, and provider API credentials are distinct credential classes with separate lifecycle rules; **never interchange them**.

## Threats and objectives

- **Stolen cookie/session handle:** attacker impersonates a user on the web unless server-side revocation and privilege gates intervene. Rotation reduces exposure but cannot guarantee detection of a stolen *current* cookie.
- **Stolen OAuth/OIDC refresh token:** attacker may silently regain access unless the identity provider supports replay detection via sender constraint or refresh token rotation.
- **Reuse of invalidated credentials:** indicates replay or a concurrency/late-response race; never automatically infer *which* actor is legitimate.
- **Session fixation, CSRF, XSS, cache/log leaks, identity-provider compromise, lost or shared device, credential revocation while a privileged operation is queued.**
- **UX:** active authorized web users remain logged in through secure renewal; no unexpected login dialog for ordinary authorized operations. Critical budget/permission actions require fresh verified step-up. Background worker processing does **not** count as human presence.

## Proposed baseline — backend-for-frontend (BFF) on Rust

1. Use **OIDC Authorization Code + PKCE** with strict issuer, subject, audience, redirect, state, nonce and cryptographic validation; require HTTPS and a configurable audited trustworthy OIDC provider. Authorization/login decision in provider; project role/permissions and approval stay in Rust and SQLite.
2. Browser receives **only an opaque, unpredictable, first-party session cookie**, with `Secure`, `HttpOnly`, appropriate `SameSite`, host-only scope/Path and preferably `__Host-` naming when compatible. No OIDC access token or refresh token in `localStorage`, JS, URL, browser logs or worker model context. Enforce CSRF defenses, origin checks, CSP and output encoding; secure cookies alone do not remove CSRF or XSS.
3. Rust holds an authoritative server-side session record (project permissions always checked separately), limited device metadata, server-enforced expiration and revocation state. In persistent storage, keep **only keyed hashes** of high-entropy opaque session handles, with the hashing key protected outside the main DB, to reduce impact of SQLite backup compromise. If OIDC provider refresh tokens are truly needed, encrypt them at rest using distinct protected keys/key versions, and never write them to analytics, crash dumps or backups without encryption.
4. Limit cached authorization and refresh privileges to authorized scopes/client/resource. Prefer not to retain an OAuth refresh token when a secure IdP sign-in renewal mechanism suffices for the user experience.
5. Session renewal is server-mediated; **a renewal must not silently expand privileges**. Rotate the opaque session identifier after successful authentication, privilege changes and defined renewal events. Keep server-side revocation/role version checks on every sensitive action, including queued approvals and outbox effects.
6. User-visible inactivity is measured only from authenticated real user activity (meaningful interaction or trusted user-originated request, not merely polling); approved web policy: **eligible to expire after 7 days without real user activity**. Authentication/session max lifetime and chosen renewal rules require further security review. No blanket hard 30-day forced logout product rule. Existing project automation continues on independent, scoped service credentials, not the browser session.

## Proposed rotation and replay detection

- **OIDC provider refresh tokens:** configure and verify provider-supported refresh-token **rotation or sender-constrained** issuance, particularly for public clients, as required in OAuth 2.0 Security BCP [RFC 9700](https://www.rfc-editor.org/rfc/rfc9700.html#section-4.14). Do not pretend Rolvya can forcibly rotate tokens owned by an IdP that doesn't support it. If provider capabilities are insufficient for the assessed risk, disable unattended refresh for that configuration or require interactive reauthentication.
- **Rolvya sessions:** generate a fresh high-entropy cookie identifier when renewing. Update the session generation and hash in an **atomic SQLite transaction**. Previous identifiers must expire/be invalidated under a carefully bounded implementation strategy. A server-side revocation flag/epoch blocks known stolen, terminated or older identifiers. Cache invalidation must not resurrect revoked sessions.
- **Duplicate concurrent tab requests:** coordinate renewal per session and use transaction/version checks; ensure late responses cannot resurrect an older session generation. A narrowly bounded race-handling mechanism, if employed, must **not** turn a revoked identifier into an unrestricted bypass. Test under dropped responses and multi-tab navigation.
- **Replay of an already consumed refresh token (where supported by IdP):** treat as a suspected credential compromise; block further refresh in the affected grant/token family, revoke affected Rolvya sessions, request upstream revocation where supported, log and alert, and require a fresh strong login. Differentiate expected concurrency from suspicious reuse with evidence, but never silently allow a confirmed replay.
- **Stolen active cookie:** rotation alone may not reveal active theft. Pair with server-side revocation, rate limits, anomaly signals, additional step-up for sensitive operations, short-lived privileged authorization and recorded security events. Do not rely solely on IP binding or passive browser fingerprinting (which may break legitimate traveling users and is forgeable).

## Step-up and compromise handling

1. Sensitive actions: budget increase, Owner/editor role/permission changes, new external provider/Git authorization, plan-scope approval and other explicit high-risk effects require recently verified **strong authentication assurance** from the trusted IdP, checked by Rust against supported `auth_time`/`acr`/`amr` or an equivalent verified provider-specific contract. Do **not** treat presence of arbitrary claims as proof that passkey authentication occurred.
2. Step-up approval is specific to actor, project, intended action and expiry; verify again at execution. A valid session alone never authorizes more budget or silently changes a frozen task DAG.
3. On device loss, reported theft, suspicious token reuse or forced security recovery: revoke **affected device/session family**; for systemic account compromise, revoke all human sessions and pending privileged grants, notify through verified channels, audit and require reauthentication before resuming sensitive work. Don't automatically cancel unrelated, already-approved remote workers unless their own authority/secrets are at risk; preserve correct billing and forensic evidence.
4. Self-hosted OIDC provider outage or revoked OIDC client: deny sensitive new access; retain existing authorized workflow records, independent bounded worker execution and safe shutdown rules. Never fall back silently to weaker local-password or email-only login.

## Deployment/operational constraints

- Store secret keys through dedicated encrypted secret facilities under least-privilege Rust service identity; support key versioning, rotation and incident revocation. Avoid leaking plaintext to SQLite WAL, normal backups, logs, support tickets or task context.
- Track per-device session ID, creation, last *human* activity, server-side expiry, generation, revoked time/reason and security-event reference. The exact SQLite schema/crypto crate/secret manager remain to be chosen.
- Ensure account recovery and sign-out invalidate appropriate local sessions even when the IdP's own web session cannot be revoked; UI must explain the distinction.
- Use monotonic/conservative server timing and reliable eventual expiration/cleanup without expensive full-table scans or busy polling on 2–4 GB remote controller.

## Proposed negative and load tests — not executed

1. JS/XSS tries to read an HttpOnly cookie or fetch tokens from local storage → none present; still validate CSRF and content injection defenses.
2. Session fixation or privilege elevation attempts reuse an old session ID → rejected after authenticated elevation/rotation.
3. Two browser tabs renew at once; one response is lost → no duplicate session family, stale generation escalation, or unbounded forced logout; defined retry behavior tested.
4. Already-redeemed provider refresh token is replayed → if IdP rotation is supported, entire affected grant is invalidated and a fresh high-assurance sign-in is required.
5. Malicious actor steals current cookie before rotation → access may be possible until detection, revocation or expiry; **step-up denies sensitive approval**. Document this residual risk.
6. Logout on device A revokes A promptly, not other sessions unless "all devices" chosen; server checks stale cookie denial.
7. A legitimate owner remains active over multiple days; session renewal remains unobtrusive within reviewed limits; seven days of *no human interaction* expires web session regardless of background worker/polling.
8. A stale Editor permission cached before revocation cannot approve privileged queued work; server reauthorizes at actual effect.
9. OIDC provider outage or token revocation mid-approval → affected new privilege escalation fails closed without resetting budget cap.
10. 100/500/1000 simulated human session renewals and worker claim load → measure SQLite write contention, latency, CPU/RSS and races; no fabricated performance claims.
11. Debug logs, backups and worker contexts contain no raw session handles, IdP refresh tokens, signing keys or other secret material.
12. Session termination/compromise does not accidentally disable independently authorized fixed-DAG worker execution or permit newly unbudgeted dispatch.

## Explicitly open for implementation approval

OIDC provider capabilities and onboarding test, encrypted secret backend, whether IdP refresh tokens are retained at all, sender-constraining support, cookie renewal window/race protocol, *precise* idle/absolute duration choices (aside from owner-approved 7-day real-human-idle baseline), privileged step-up recency, risk signals, session cap per user, and the browser UI for device list/revocation. **Do not mark this proposal as an approved ADR or implemented code.**

## Authoritative external security guidance

- [IETF RFC 9700 — OAuth 2.0 Security Best Current Practice](https://www.rfc-editor.org/rfc/rfc9700.html), especially replay defense and refresh token guidance §4.14
- [OWASP Session Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html), especially session ID rotation, expiration and invalidation
- [MDN — Session Management](https://developer.mozilla.org/en-US/docs/Web/Security/Authentication/Session_management)
