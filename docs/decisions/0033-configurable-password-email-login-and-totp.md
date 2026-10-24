# ADR 0033 — Configurable password and verified-email login with authenticator-app 2FA

Status: **Approved**  
Date: 2026-10-24  
Supersedes: [ADR 0032](0032-passwordless-human-sign-in.md) (previous passwordless-only/Google/GitHub login policy), and conflicting login-method mandates in [ADR 0025](0025-oidc-passkey-authentication.md).

## Owner-approved product decision

Rolvya's initial human login methods are:

1. **Email address and password** authentication.
2. **Email verification-only login**, with **Magic Link and one-time email OTP** both supported (Magic Link remains the default email-only UX, OTP is an alternative).
3. **Authenticator-app 2FA** as an optional extra security layer after primary authentication, using standard app-generated time-based one-time codes (TOTP); **no SMS integration is required**.

On first setup, the Owner **must enable at least one primary login method** (password or email verification). The remaining primary login method can be enabled later by the Owner. 2FA is an **additional factor, not a third standalone login method**; it may be enabled subsequently and must never be allowed to replace the only working primary method. This separation is required to avoid a 2FA-only account that cannot sign in.

**Google and GitHub social sign-in are not required or approved as initial Rolvya v0.1 login options under this superseding decision.** GitHub App authorization for remote repository integration remains an independent approved system integration and is unaffected. Configurable external OIDC/passkey capabilities are not silently removed from the security architecture, but introducing additional human login providers in the initial UI requires a new Owner scope decision.

## Authentication and configuration safeguards

- Password authentication uses a unique per-user salted, memory-hard verifier such as **Argon2id** with appropriate tuned parameters, no plaintext or reversible password storage, safe reset flows requiring verified account possession, enumeration protection, breached-password screening if feasible, and request throttling. Passwords are never present in logs or sent to AI agents.
- Email-only Magic Links/OTPs are unpredictable, brief-lived, scoped to the pending login, single-use, atomically consumed and rate-limited; prevent reuse, mailbox enumeration, credential leaks, and unsafe cross-device links. Email-only login does not require setting a password.
- TOTP authenticator apps use interoperable RFC 6238-style tokens; enrollment requires an authenticated user and verification of a generated code before activation. Store TOTP secrets encrypted at rest with separately managed encryption keys; throttle attempts, prevent replay within the accepted window, and provide narrowly scoped, single-use, safely stored recovery codes with secure revocation/re-enrollment.
- **Owner-only security configuration:** Only an authenticated Owner with appropriate recent strong reauthentication may enable an additional allowed primary method, change project/deployment login-method policy, or set a policy requiring TOTP. Changes are audited and affected users informed. Never disable the last usable login method or lock out existing users without a verified migration/recovery path; no agent/Editor/Viewer can change global login policy or weaken project security.
- A login method being enabled for an instance/project does not automatically enroll an existing account into it: password setup and TOTP enrollment require account-owner identity verification. Do not silently merge accounts by matching email strings.
- **High-risk Owner step-up still applies:** managing Owners, expanding spending limits, secrets or Git/provider privileges requires a recently verified strong step-up. A plain password alone or inbox possession alone is not automatically phishing-resistant. TOTP provides an additional factor but is not phishing-resistant; retain risk-based denial, alerts and secure recovery when appropriate.
- Role authorization (Owner/Editor/Viewer), project isolation, sessions, reset and recovery, TLS, audit and machine-identity separation remain mandatory. Email verification is required when establishing/recovering an address; 2FA never uses SMS in the initial scope.

## Password reset when outbound email is not configured (Owner-approved 2026-10-24)

- When the deployment's outbound email service is **not configured or unavailable**, the password-reset screen must not claim that a reset message was sent or generate an unusable email link. Explain that self-service email-based password reset is unavailable and instruct the user to **contact the instance administrator** through the configured support channel. Do not reveal whether the submitted account exists.
- Administrator assistance is a **secure recovery workflow**, not a right to read passwords, set an arbitrary replacement password, bypass TOTP or grant an Owner role. Require independent identity verification, recorded authorization, single-use bounded recovery or password re-enrollment, session/token revocation as appropriate and audit. The exact non-email recovery method remains an implementation/security design question.
- If verified-email login is enabled as the **only** primary sign-in method, initial setup must check operational email delivery before allowing activation; missing email configuration cannot produce an unusable sole method. Likewise password-reset recovery readiness and a reachable administrator contact route should be validated for deployments using password sign-in.
- The recovery instructions must not disclose secrets or private account information, and must not silently weaken the approved multi-Owner/last-Owner and step-up controls.

## Implementation decisions still open

Concrete email delivery provider, instance-versus-project policy inheritance, password strength/breach checking and hashing tuning, TOTP secret key management and recovery policy, session cookies/expiration, assurance mapping, UI details, login challenges and abuse throttles. These require security review and tests, but **do not reopen which initial login methods the Owner approved**.

## Acceptance requirements (future, not executed)

- Initial setup refuses a configuration without either password or verified-email login.
- Owner can enable a disabled primary method after strong authorization; Editor, Viewer or an agent cannot. Enabling a method does not bypass enrollment and recovery safeguards.
- Password signup/login/reset works without storing/revealing plaintext; attempts are rate-limited and sessions are protected.
- Magic Link and OTP both work without a password; expired/reused codes/links fail safely.
- Authenticator-app TOTP enrollment, login prompt, replay protection, loss/recovery and revocation are tested; no SMS provider is needed.
- TOTP-only as a primary method is invalid; changing policy cannot strand all account holders.
- High-risk Owner budget/ownership actions cannot be authorized solely through weak factors or stale sessions.

**State:** Owner-approved architecture and v0.1 feature policy, **not implemented or tested**.

Related: [ADR 0024 roles](0024-project-human-roles.md), [ADR 0025 identity principles](0025-oidc-passkey-authentication.md), [ADR 0026 recovery](0026-account-recovery.md), [ADR 0029 invitations](0029-owner-controlled-project-invitations.md), [identity architecture](../architecture/identity-authorization.md).
