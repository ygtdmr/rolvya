---
title: Owner High-Risk Reauthentication Without Mandatory TOTP
status: Proposed
category: architecture
last-reviewed: 2026-10-24
tags: [p0, authentication, owner, security, transactions, email, password]
---

# Owner privileged reauthentication without mandatory TOTP — proposed implementation

> **Status: MIXED — fundamental action-specific reauthentication flow and 5-minute approval / 10-minute email challenge / 5-failed-email-OTP-attempt / one-use limits APPROVED in [ADR 0034](../decisions/0034-owner-action-bound-reauthentication.md); concrete implementation, physical schema, other rate limits, risk thresholds and recovery procedures remain PROPOSED.** Voluntary TOTP and the hard-budget/Owner requirements are unchanged. No Rust authentication code or tests are claimed.

## Governing decisions and constraints

- [ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md): initial methods are password or verified-email Magic Link (default) / OTP (alternative); **TOTP is opt-in, never required for Owner actions**. No SMS or initial Google/GitHub human social-login requirement.
- [ADR 0027](../decisions/0027-risk-based-session-management.md) and [ADR 0028](../decisions/0028-human-session-token-safety.md): per-device server-revocable sessions, recent reauthentication for sensitive operations, background jobs cannot extend the seven-day real-user idle window; no blanket forced logout every 30 days.
- [ADR 0029](../decisions/0029-owner-controlled-project-invitations.md): one strongly reverified active Owner is sufficient for an explicit project budget increase; other Owners get durable in-app notification. Ownership transfer is separately protected, with independently authenticated recipient consent and last-Owner invariant.
- [ADR 0026](../decisions/0026-account-recovery.md): no outgoing mail means **contact the instance administrator**, not a fake password reset; administrator cannot choose/read user password, grant Owner, or silently bypass an enrolled authenticator.
- [ADR 0031](../decisions/0031-atomic-task-budget-admission.md): paid worker admission reserves worst-case exposure and commits SQLite attempt/lease/outbox before any provider launch; an Owner's ceiling approval is a distinct protected transaction and does not itself launch paid work.

## Explicit risk and standards limits

A fresh password alone or a fresh email challenge alone **is still single-factor authentication**; neither is automatically phishing-resistant or NIST AAL2. In particular [NIST SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b/authenticators/) states email is not an accepted out-of-band authentication factor for its assurance framework. Rolvya may still offer Owner-approved email login; **do not misrepresent its assurance**. The design uses layered server-side authorization, transaction binding, short-lived proof and conservative incident handling, not a false MFA claim.

[OWASP Authentication](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html) advises reauthentication for sensitive changes; [OWASP Transaction Authorization](https://cheatsheetseries.owasp.org/cheatsheets/Transaction_Authorization_Cheat_Sheet.html) advises presenting significant transaction data, per-operation authorization and checking authorization at execution; [OWASP Forgot Password](https://cheatsheetseries.owasp.org/cheatsheets/Forgot_Password_Cheat_Sheet.html) advises uniform responses and expiring single-use recovery tokens. These are engineering references, not proof the design is implemented or fully standards-compliant.

## Approved fundamental flow (ADR 0034) with proposed engineering realization

1. **Prepare, do not execute.** Rust verifies a valid server-side session, current project/role, account not frozen and unchanged preflight policy. Create a short-lived `pending_privileged_action` record with an opaque ID, immutable canonical payload hash, `actor_user_id`, session identity/generation, project/deployment scope, action name, target version, state, expiry and risk snapshot.
2. **Show exactly what will change.** For a budget increase, show old/new **total ceiling**, currency and project; for ownership transfer show recipient immutable account identity and role consequences; for credential/provider changes show explicit permission differences. A changed payload or version invalidates the pending authorization. No approval via an alert dismissal, background worker or just opening an email link.
3. **Authenticate freshly according to the user's actual enabled credential**, not an imposed TOTP enrollment. Password-enabled principal may enter their **current** password again, verified by Rust against the salted memory-hard verifier. A verified-email-only principal must complete a **new, dedicated, transaction-scoped one-time email challenge** through Magic Link or OTP. Verification proves control of that **single** factor again, **not** a second factor. If the user voluntarily enrolled TOTP, respect its existing enabled protection without forcing un-enrolled users to adopt it. A principal with more than one method must not silently bypass a previously enrolled stronger method by choosing a weaker fallback.
4. **Explicitly confirm** the exact pending action after successful reauthentication. The server stores at most one short-lived, single-use, session/action/payload-bound `privileged_authorization` and consumes it when applying the exact intended change. A login challenge or reauth success may **not** grant reusable general Owner powers to execute unrelated transactions.
5. **Apply under SQLite atomic consistency.** Within a short write transaction re-check current active Owner membership and any project/deployment administrator boundary, session generation and security epoch, pending-action expiry/unused state, credential/reauth evidence, target version and immutable payload digest. Atomically consume authorization, apply the owner-authorized change and record the audit and mandatory post-commit notification intent. Rollback consumes neither budget ceiling nor the approval. For subsequent paid execution, **ADR 0031 admission is a separate transaction** that rechecks the new committed ceiling and reserved liabilities.
6. **Notify and monitor** the acting Owner and other eligible Owners through durable in-app events; optional email only where configured. Never report an approval or notification as delivered before the relevant committed state is known.

## Reauthentication decision matrix (candidate, not production policy)

| Account/operation state | Proposed route | Reason and safe behavior |
| --- | --- | --- |
| Password is enabled, no TOTP enrolled | Ask for current password *again* within action-bound flow | Single-factor fresh proof + authorization confirmation; apply rate limits, session/role checks and anomaly controls; **no compulsory 2FA** |
| Verified-email only, mail operational, no TOTP enrolled | New purpose-bound Magic Link/OTP with explicit action confirmation | Inbox-controlled single-factor proof. Bind to existing browser/session and exact action; do not call MFA; suspicious behavior can still trigger denial/recovery |
| Both password and verified-email enabled | Use current authorized credential binding without silently downgrading; require account-owner confirmation for method switching | Avoid an attacker using a newly linked or compromised mailbox as a silent bypass |
| User voluntarily enrolled TOTP | Preserve the enabled factor according to that account's protection policy | Never strip enrolled protection via method switch, recovery or administrator shortcut |
| Mail is absent and account has a valid password | Fresh current password + confirmation; if forgotten, show **contact administrator** | No fake reset messages. Administrator-assisted recovery requires independent evidence and audit |
| Verified-email is the sole primary method but mail is unavailable | No mail challenge can complete; reject unsafe new high-risk action and initiate repair/recovery route | Instance setup should have refused email-only activation without operational mail. Do not invent a hidden fallback |
| Recently recovered/renamed account, changed address/password/TOTP, compromised session, suspicious role change | Revoke existing pending approvals; risk-review before a new high-risk request | Do not reuse old evidence; optionally delay or suspend action pending independent review without silently imposing TOTP |
| Non-Owner / revoked Owner / wrong project | Deny before and again at commit | Identity proof cannot grant membership, rewrite the immutable DAG or raise budgets |
| Ownership transfer | Initiating Owner's fresh action-specific proof; separate target's independently authenticated explicit acceptance | Commit atomically; never remove last Owner or infer assent from forwarded invite |
| Budget ceiling increase with several Owners | One active Owner's verified explicit action, audit and durable notice to other active Owners | Do not invent co-Owner unanimity or hidden automatic overruns |

## Proposed records and replay boundaries

```text
pending_privileged_actions(
  action_id, actor_user_id, actor_session_id, session_generation,
  deployment_id, project_id, action_type, payload_digest,
  target_version, created_at, expires_at, state, risk_epoch
)

privileged_proofs(
  proof_id, action_id, actor_user_id, method,
  credential_binding_id, session_generation,
  verified_at, expires_at, consumed_at, fail_reason
)

privileged_effects(
  action_id UNIQUE, approval_version, effect_version,
  actor_user_id, project_id, committed_at, audit_event_id
)
```

All names and types are **illustrative, not approved schema migrations**. Use replay-safe one-use digests for high-entropy email challenge tokens; treat short numeric OTPs as low entropy and protect their server-side verifiers against offline guessing. Bind step-up proof to **current session generation, credential binding version, immutable action payload, target entity version, role version and security/risk epoch**. A changed ceiling/role/payload or logout/recovery invalidates it. No API token, raw password, OTP, Magic Link, session ID, recovery secret or TOTP seed goes into an AI prompt, worker, generic audit event or outbound user notification.

**Approved ADR 0034:** the fresh critical-action approval expires **5 minutes after proof creation**; issued email Magic Link/OTP challenges expire **10 minutes after issuance**; **5 unsuccessful email OTP attempts per challenge** are permitted (the fifth exhausts it); and email challenges and transaction authorizations are **single-use**. All expiry/attempt/consumption checks must be atomic and time-tested. Specific resend/account/IP abuse windows, code length/entropy, TOTP-app-code policy, risk thresholds and recovery delays are **still unselected**.

## Abuse and denial handling — fail safely without mandatory 2FA

- Risk signals: newly initialized device/session, recent credentials/factor/address change, suspicious login/recovery, replayed challenge, unusual spending change, changed target permissions, known stolen session or impossible audit state. IP/device/geographic heuristics are signals, **not** independent authentication factors and not reliable evidence of identity.
- Under credible compromise, the controller can hold the **specific high-risk action** and require independent incident review/recovery. This is **not** a requirement to enroll TOTP; retaining optional 2FA must not make a compromised account automatically spend-authorized.
- On outage of SQLite/auth proof/policy engine, deny new privileged changes and new billable admission; preserve existing verified work and bounded preauthorized worker policy.
- Never approve by refreshing an old session, showing "Yes" without credential proof, clicking a previewed link, resending the same OTP, or reusing a consumed action authorization.
- Allow safe non-TOTP recovery after independent identity verification, bounded approval and auditable credential re-enrollment; the **administrator is not by default the same principal or authority as project Owner**.
- Password reset or email-only login with an already compromised mailbox is **not** independent ownership assurance. Where risk cannot be adequately controlled with the selected methods, fail closed and disclose that limitation rather than claiming strong MFA security.

## Security regression cases — future tests, not run

| ID | Scenario | Required outcome |
| --- | --- | --- |
| STEP-T01 | Owner without TOTP increases ceiling after fresh password and explicit exact-amount approval | Allowed **only if risk checks pass**, with once-only approval; never prompt TOTP enrollment |
| STEP-T02 | Email-only Owner confirms action via new Magic Link/OTP | Existing session/action binding, single use and exact payload verified; no MFA/AAL2 claim |
| STEP-T03 | Stolen session with no current password or fresh mailbox challenge | Cannot approve sensitive operation solely from cookie |
| STEP-T04 | Action is edited after proof (new budget/role/recipient), or previous project version changes | Prior proof invalid; no altered effect |
| STEP-T05 | Browser retries the same POST or concurrent requests race | At most one committed action/audit/effect; no double ceiling increase |
| STEP-T06 | User logs out, rotates credential, recovers account or loses Owner membership after challenge | Reject any stale pending authorization even if email link arrives later |
| STEP-T07 | User voluntarily enabled TOTP but attacker attempts email fallback to bypass it | Deny unauthorized downgrade, protect enrollment and safe recovery |
| STEP-T08 | Outgoing mail fails for email-only Owner or password reset | No fake sent-email result; block inaccessible challenge and offer actual admin support route |
| STEP-T09 | Compromised email evidence/recent factor change and high-risk action | Risk hold or independent review; never conflate single-factor proof with independent MFA |
| STEP-T10 | Admin account attempts budget change without project Owner membership | Deny; operator cannot impersonate project Owner |
| STEP-T11 | Recipient accepts transfer but initiating Owner loses membership concurrently | Atomic compare-and-swap prevents unauthorized or ownerless transfer |
| STEP-T12 | Budget approval succeeds while durable other-Owner notification delivery is delayed | Budget change stays auditable, notification intent remains retryable, no false read receipt |
| STEP-T13 | Commit succeeds then process crashes before sending HTTP response | Durable idempotency prevents duplicate approval on replay |
| STEP-T14 | Worker replays a human challenge/authorization token | Reject machine-to-human role confusion and revoke exposed proof |

## What still needs explicit Owner/security review

1. Exact risk signals and mandatory evidence for routine versus unusually risky Owner approvals, including when to hold/alert rather than automatically execute. **The principle of refusing automatic approval under credible compromise is approved ADR 0034.**
2. **Already approved:** 5-minute one-action proof life, 10-minute email link/OTP issuance TTL, up to 5 failed email OTP verifications per challenge and one-use consumption. **Still open:** per-account/IP and resend rate limits, password/TOTP-app-code limits, and account-recovery cooldown before privileges can be used.
3. Trust boundary between deployment administrator and project Owner, particularly bootstrap and sole-Owner recovery; no hidden administrator override of hard financial gates.
4. How to compare authentication strength between enrolled methods while preserving voluntary TOTP and preventing downgrade.
5. Exact transaction schema and cookie/authorization/step-up HTTP interfaces, key custody and incident response thresholds.

**Status:** Voluntary TOTP and the **fundamental new password/email proof + exact one-use transaction confirmation + suspicious-operation denial/hold** are **Approved in ADRs 0033–0034**. The above record layout, other numeric limits, risk algorithms, transport details and recovery implementation remain **Proposed** until approved/tested; ADR 0034 already approves the four explicit challenge/approval parameters. OQ-004 remains P0. Reference: [human auth overview](human-authentication-implementation-contract.md), [identity design](identity-authorization.md), [session proposal](session-token-rotation-and-theft-response.md), [ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md), [acceptance criteria](../product/acceptance-criteria.md).
