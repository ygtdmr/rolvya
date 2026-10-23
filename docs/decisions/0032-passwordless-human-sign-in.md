# ADR 0032 — Passwordless human sign-in and email verification

Status: **Approved**  
Date: 2026-10-23

## Owner decision

The Rolvya v0.1 human login offers **Google sign-in, GitHub sign-in, and verified-email passwordless sign-in**. **No Rolvya password registration, password field, password login, or password-based fallback/recovery** is permitted. Users may authenticate through a one-time email verification link or one-time email code. Trusted OIDC providers, passkey-capable authentication and strong reauthentication requirements from ADRs 0025–0029 remain binding. This is an approval of supported human methods, **not** permission to skip security checks.

## Security and identity invariants

- Google/GitHub authentication must verify trusted issuer and subject, strict redirect and callback integrity, PKCE/state/nonce as applicable, token signature/audience/time, and actual verified email evidence; a GitHub OAuth access token alone is not proof of a validated OIDC identity. Provider protocol details and any secure identity-broker integration remain engineering choices, and must be checked against provider capabilities before deployment.
- Passwordless email verification must prove current mailbox control with a short-lived, unpredictable, one-use challenge. Store only an irreversible server-side representation of the challenge; bind to intended action, session/transaction and recipient; enforce expiry, atomic one-use consumption, throttling, enumeration resistance, replay protection and safe redirects. Do not include login secrets in audit logs, analytics or external redirect query parameters.
- Verified email is **not** an immutable user/account key: store durable internal account IDs and provider-specific stable subject bindings. Linking an email-authenticated account to Google/GitHub or another identity requires verified control of both sides and protection against takeover, collisions and stale/reassigned mailboxes. Never auto-merge accounts just because the displayed email string matches.
- A successful email code or link can authenticate an ordinary account, **but email possession alone is not sufficient assurance for high-risk Owner actions**. Budget-ceiling increases, Owner elevation/transfer, security changes and account recovery must satisfy already-approved independent recent strong step-up rules (prefer verified phishing-resistant passkey or a comparably approved method). If assurance cannot be established, fail closed and offer a supported secure enrollment/recovery route rather than bypassing Owner safeguards.
- Human login is separate from GitHub App authorization of repositories, cloud worker/service identities and machine credentials. Roles are enforced by Rust/SQLite on every privileged operation; sign-in never automatically confers project membership or Owner authority.
- Self-hosted deployments may use trusted configurable OIDC identity brokers supporting the approved human methods; implementation must not force storing human passwords. No silent fallback to password auth.

## Open implementation questions

The specific email sending service, link-versus-code primary UX (both allowed), code entropy/expiration/throttle constants, anti-abuse measures, secure login/account-linking broker, provider credential scope and reliable assurance mapping are **not yet selected**. Google/GitHub availability, current OAuth/OIDC and passkey capabilities require vendor-compliant verification. Exact Owner step-up UX must be proved, not inferred from email verification.

## Future acceptance tests (specified, not executed)

- Google and GitHub logins produce distinct issuer/subject bindings and cannot gain membership from an unverified or spoofed email.
- Expired, reused, concurrently consumed, forwarded or brute-forced email challenges do not create a session or grant access.
- No password database, password UI, password reset or hidden password fallback exists.
- Forged callback and account-link collisions fail closed; verified linking requires control of both identities.
- Email-only session cannot approve Owner budget increases, ownership transfers or high-risk recovery without separately verified strong step-up.

**Implementation status:** Approved product/security policy only; no IdP, email sender, Rust auth implementation or acceptance test is running.

Related: [ADR 0025](0025-oidc-passkey-authentication.md), [ADR 0026](0026-account-recovery.md), [ADR 0027](0027-risk-based-session-management.md), [ADR 0029](0029-owner-controlled-project-invitations.md), [identity architecture](../architecture/identity-authorization.md).
