---
title: "Human Authentication, Recovery and Owner Step-up — P0 Implementation Contract"
status: Proposed
category: architecture
last-reviewed: 2026-10-24
tags: [p0, auth, password, magic-link, otp, totp, owner, sessions, security]
---

# Human authentication, recovery and Owner step-up — P0 implementation contract

> **STATUS: PROPOSED implementation design. No new authentication security policy is approved by the existence of this file. No Rust authentication code, email sender or security test has been executed.**

## Authority and provenance

- **Approved human primary methods** (ADR 0033): password; verified-email login with Magic Link as default and OTP as alternative. No Google/GitHub social login mandate and no SMS requirement. App-based TOTP 2FA is an optional additional factor, not a primary login method.
- **Approved enrollment/administration** (ADR 0033): Owner selects at least one primary login method at first setup; a suitably reauthenticated Owner can later enable permitted alternatives. **2026-10-24 clarification: TOTP is never required for Owner sensitive actions.** Do not disable the last working method or silently enroll users into new credentials.
- **Approved governance** (ADRs 0024/0029): project-scoped Owner, Editor and Viewer; only Owners invite; invitation default 24 hours, one-use, Owner transfer requires separate action, both humans independently authenticate, and SQLite protects the final Owner.
- **Approved sessions** (ADRs 0027/0028): server-revocable per-device sessions, secure renewal/replay defense, 7-day real-user-inactivity baseline; no automatic blanket logout each 30 days. Worker background traffic does not count as human interaction.
- **Approved recovery** (ADRs 0026/0033): secure password reset when available; if outgoing email is unavailable, show **contact the instance administrator**, never falsely claim delivery. Admin assistance requires independent identity proof, audit, safe session revocation and no TOTP/Owner bypass. Email-only sign-in cannot be the sole enabled primary method without functioning outgoing email.
- **Conditional future federation** (ADR 0025): if an Owner later explicitly approves OIDC/passkeys as human login, verify issuer/subject and assurance. It is **not** required for initial v0.1.
- **Separately scoped machines**: worker identity, provider credentials and GitHub App authorization do not authenticate a human or grant human roles.

### Security standards and the email-login limitation

[NIST SP 800-63B-4 (2025)](https://pages.nist.gov/800-63-4/sp800-63b/authenticators/) does **not** accept email as an out-of-band authenticator for its assurance framework. **This does not override the Owner's approved email Magic Link/OTP sign-in product feature.** It does mean that email possession by itself must **not** be labeled a device-bound possession factor, phishing-resistant authentication, or NIST AAL2/MFA. For high-risk Owner actions, require independently evidenced assurance under a separately approved policy. See [OWASP MFA](https://cheatsheetseries.owasp.org/cheatsheets/Multifactor_Authentication_Cheat_Sheet.html) and [OWASP Forgot Password](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html).

## Threat boundaries (must hold under any implementation)

1. **Stable identity**: internal immutable `user_id`; verified email addresses, password verifiers and future federated `issuer + subject` are linked **credentials**, not the user ID. Never auto-merge users because their email strings match or were reassigned.
2. **One authority**: Rust authorizes humans and roles; SQLite owns membership, session/recovery status, invitations and audit. A valid login is not project membership and never automatically confers Owner authority.
3. **Credential separation**: password secrets, email challenge plaintext, TOTP seeds, recovery codes, human session keys and provider/worker credentials never enter AI context or worker execution environments.
4. **One login-method policy**: distinguish deployment/system administrator's configured capabilities/operational mail health from each project Owner's permitted **subset** and enrollment requirements. The precise inheritance and bootstrap administration are **not approved yet**.
5. **Explicit assurance evidence**: persist human-auth method(s), verified factor enrollment and recent verified step-up with transaction/session binding. Do not infer strong assurance from email verification, ordinary password, an old session or a UI checkbox.
6. **Transactional revocation**: atomically check user active state, project membership/role version, session generation, security policy version and necessary high-risk reauthentication at the point of effect, including delayed approvals/outbox processing.

## Candidate internal account model (not final SQLite DDL)

| Conceptual record | Key and minimum protected fields | Atomic guard / behavior |
| --- | --- | --- |
| `users` | immutable user_id, disabled/locked state, created_at | No projects or roles inferred from sign-in |
| `user_emails` | user_id, normalized address, verified_at, change history | Verify address ownership; no email-only automatic account merge |
| `password_credentials` | user_id, versioned Argon2id verifier, rehash flags | Never retain plaintext or reversible password |
| `email_challenges` | challenge_id, keyed token/code digest, purpose, recipient, session nonce, expiry, max attempts, used_at | One live scoped one-use challenge; compare-and-consume in SQLite |
| `totp_credentials` | credential_id, encrypted seed, key_version, enrolled_at, activated_at, last accepted timestep | Reject replay; enrollment active only after verified first code |
| `recovery_credentials` | one-use code digests and invalidation events | Protect account recovery; only scoped guarded use |
| `human_sessions` | keyed opaque handle digest, user_id, device label, real_human_last_seen, auth_at, step_up_at, security_epoch, revoked_at | Per-device invalidation; bounded renewal; 7-day human inactivity |
| `login_policy` | deployment allowed methods, project-enabled subset, version, mail health, TOTP requirement | Fail startup or policy update if zero usable primary routes |
| `project_memberships` | project_id, user_id, role, version | Enforce role server-side; transactionally reject deletion of last Owner |
| `invitations` | nonce digest, project, invited recipient binding, proposed role, 24h expiry, consumed_at | Invite token alone not membership; exactly one redemption |
| `admin_recovery_cases` | case_id, initiating actor, independent identity evidence refs, approvals, audit, expires_at, used_at | No administrator password viewing, MFA bypass or arbitrary Owner restoration |
| `auth_audit_events` | actor, action, outcome, source, redacted metadata and integrity reference | No codes, links, passwords, TOTP seeds or session cookies |

**Important:** exact tables, IDs, encryption algorithms, credentials lifecycle and foreign keys are still unselected. This table guides later reviewed migrations, not an authorization to generate an incomplete schema.

## Candidate login flows (all proposed implementation mechanics)

### A — First setup and method configuration

1. Authenticated deployment bootstrap establishes the first independently verified human/Owner without publishing a reusable bootstrap secret; a safe one-time operator bootstrap ceremony is a **proposal**.
2. Verify at least one primary method is enabled: `password` or `verified_email`. Reject TOTP-only and zero-method configurations.
3. If verified email is the **only** method, check outbound sender health and readiness before activation. If password is enabled but no sender is available, expose a verified administrator support route.
4. The administrator controls deployment mail service and allowed authentication capabilities. The Owner can enable supported alternatives without disabling an existing safe route, subject to a version-checked, strongly reauthenticated, audited action. **Exact deployment-admin vs project-Owner division is awaiting review.**
5. Enabling a method does not enroll every account, bypass email verification or remove existing enrolled 2FA. Warn and test user migration/lockout before disabling any currently used route.

### B — Password login and reset

- Use salted memory-hard password verifiers (candidate Argon2id), bounded validation cost, leaked-password screening where practicable, response normalization and per-IP/per-identity throttling. No password material in logs.
- After successful password proof, if the user has voluntarily enrolled and enabled a TOTP factor, complete that factor before issuing a fully authenticated session; keep partial-auth state incapable of privileged actions.
- Password reset with configured operational mail uses a short-lived purpose-scoped, one-use challenge. After secure reset, require fresh login, invalidate affected sessions and outstanding challenges, notify through safe existing channels. Reset must never silently disable an enrolled second factor.
- **No outbound mail:** respond without account enumeration and provide **contact the instance administrator**. No claim that mail was sent. The administrator does not learn or choose the user's password; verified recovery permits user-controlled password re-enrollment with post-recovery session invalidation and audit. The concrete proof/quorum/one-time recovery credential flow is **open**.

### C — Email-only Magic Link (default) and email OTP (alternative)

- Require a verified address controlled through a short-lived, cryptographically strong challenge tied to purpose, recipient, instance and pending session.
- Magic Link confirmation in a different browser/device must not silently attach an unknown session; use safe handoff/challenge binding. Prevent open redirects, referrer/query-token leakage and link-scanner auto-consumption.
- OTP: rate-limit per email/IP/instance, bound tries and resend, compare a protected keyed digest and atomically mark consumed. Plain unsalted hashes of short numeric OTPs are susceptible to cheap offline guessing.
- Email route grants an ordinary session subject to any voluntarily enrolled TOTP. **Email possession alone is not MFA or automatically sufficient authorization for a critical Owner action.** A distinct fresh email challenge plus transaction-specific confirmation and independently assessed risk controls is a **proposed non-TOTP reauthentication path**, not a claim of MFA or a preapproved risk threshold. Failure to deliver must not reveal whether a user exists.

### D — Voluntary app-based TOTP setup and recovery

- TOTP is **strictly voluntary**, even for Owners and budget changes. Use interoperable RFC 6238 authenticators (QR/provisioning URI shown only to the verified user); activate only after the first valid user-entered code. Encrypt seeds at rest using a separately protected key; no seed/QR in AI prompts or logs.
- Enforce bounded drift window, request throttles, timestep replay prevention, and single-use protected recovery codes; replay protection must be atomic across concurrent connections.
- Factor removal/replacement requires separately verified high-assurance action; a currently active ordinary session or recently opened mailbox is not automatically sufficient.
- A lost device triggers protected recovery rather than "turn off 2FA" by support; this is especially critical for a sole Owner and mail-disabled deployment.

### E — Session, invitations and approvals

- Rust issues opaque, random, first-party secure HttpOnly SameSite session cookies over HTTPS; CSRF and origin protections required. Store server-side digest, revoke per device or all devices, rotate on authentication and privilege/risk events. No bearer credentials in localStorage.
- Real user interactions only refresh the approved seven-day inactivity window; backend jobs, token refresh, long-poll and remote workers do not. Absolute maximum session/step-up lifetime remains an open implementation policy.
- Invitation acceptance requires a valid human login through an enabled method (and their voluntarily enabled second factor, if any), a verified stable account binding, an unexpired one-use 24-hour invite and an atomic SQLite role grant. Invited Owner elevation/transfer has independent recipient acceptance, recent strong initiating Owner approval and preserved last-Owner membership.
- Financial approval or login-policy changes require server-side **fresh reauthentication and explicit human authorization**; a stale login, queued notification, signed link, TOTP entry on another account or worker output is never budget authorization. **TOTP enrollment is not required**; exact secure non-TOTP risk controls and protected initial Owner bootstrap are **P0 questions**.

## Approval-required choices (OQ-004 remains P0)

| ID | Exact outstanding choice | Candidate recommended direction; **not approved** |
| --- | --- | --- |
| AUTH-P0-01 | **High-risk Owner assurance with optional TOTP** | **Superseded by Owner clarification 2026-10-24:** never require TOTP enrollment for Owners or sensitive actions. Propose a fresh, transaction-bound password/email reauthentication mechanism with risk checks, protective delay/deny on suspected compromise, explicit Owner approval, audit and notification. If TOTP was voluntarily enabled, honor the enrolled protection; do not label one-factor proof MFA. Exact controls are still Proposed. |
| AUTH-P0-02 | **Deployment administrator vs project Owner** | Administrator controls service-wide allowed methods and SMTP health; each project Owner can enable only a permitted non-weaker subset. Both need separate scope and audit. |
| AUTH-P0-03 | **Email delivery and SMTP outage** | Permit an administrator-configured SMTP/transactional provider; health probe, safe fallback message and no-mail support contact. Need sender failure semantics and anti-enumeration design. |
| AUTH-P0-04 | **Challenge, TOTP and password parameters** | Versioned Argon2id tuning, email challenge/OTP lifetime and entropy, resend/throttle, TOTP drift/replay, key custody and recovery-code policy must be security-tested. No numeric constants in this document are new approvals. |
| AUTH-P0-05 | **Recovery, first Owner and lost 2FA** | Defined independently verified admin-assisted process; guard against support impersonation, sole-Owner lockout, compromised mailbox and new factor takeover; determine evidence/quorum/cooling policy. |
| AUTH-P0-06 | **Session and credential lifetime** | Web-only server sessions, seven-day actual-user idle rule already approved; choose maximum lifetime, step-up freshness period, revocation propagation and cross-tab renewal strategy. |
| AUTH-P0-07 | **Final authorization/schema contract** | Transaction-safe membership, invite redemption, account linking, factor changes and policy updates in authoritative Rust/SQLite; exact migrations and APIs are still proposed. |

## Security test design — not executed

| Scenario | Expected invariant |
| --- | --- |
| AUTH-T01: password/email/TOTP-only first-run configs | TOTP-only and no-primary rejected; email-only rejected when mail nonfunctional |
| AUTH-T02: wrong/duplicate/replayed email link or code; link scanner | No side-effect sign-in; one-use challenge and safe cross-device binding |
| AUTH-T03: stolen current/old session, parallel renewals | Server-side revocation/rotation/role changes cannot be bypassed |
| AUTH-T04: repeated TOTP timestep/concurrent code use | One accepted timestep at most, bounded attempts, no reusable codes |
| AUTH-T05: factor enrollment under compromised mailbox | No unverified immediate strong Owner approval from new factor |
| AUTH-T06: password reset with no SMTP | No fake delivery; non-enumerating admin-contact route; no automatic role/factor bypass |
| AUTH-T07: last-Owner transfer/invite race | No orphaned project, double redemption or cross-project authority |
| AUTH-T08: Owner budget approval with stale session or no fresh independently verified reauthentication | Reject privileged action and no billable admission; absence of voluntary TOTP enrollment alone must not bar a legitimate Owner using a safe approved non-TOTP route |
| AUTH-T09: lower role tries enabling login method or disabling another user's 2FA; admin tries requiring TOTP | Reject unauthorized policy changes and mandatory TOTP enrollment; audit without disclosing secrets |
| AUTH-T10: background worker polls for eight days without human activity | Cannot extend human seven-day idle window |
| AUTH-T11: email reassignment/duplicate address/account linking | Stable internal account identity and explicit verified linking |
| AUTH-T12: lost TOTP+mail outage for sole Owner | Safe, independently verified administrator path, no hardcoded bypass |
| AUTH-T13: malformed/oversized input, brute-force and rate-limit races | Bounded use of resources and no account enumeration |
| AUTH-T14: compromised DB snapshot | No plaintext passwords, OTP/TOTP seeds, session handles or recovery codes in snapshot |

## Explicit boundaries and references

- **Approved**: feature set and existing hard safeguards from ADRs 0024–0029, 0033, including Owner clarification 2026-10-24 that TOTP enrollment is always voluntary; current PR-025/026 and AC-113–130.
- **Proposed**: flows, records, challenge mechanics, admin policy inheritance, non-TOTP high-risk step-up risk checks, factor assurances, bootstrap and exact recovery.
- **Implemented/Verified**: **none**.
- Do not invent provider credentials, send emails, create user accounts, call billable providers, weaken Owner budget gates or conflate project Owner with deployment administrator.
- Related: [identity model](identity-authorization.md), [session proposal](session-token-rotation-and-theft-response.md), [open decisions](../project/open-questions.md), [acceptance tests](../product/acceptance-criteria.md), [ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md).
