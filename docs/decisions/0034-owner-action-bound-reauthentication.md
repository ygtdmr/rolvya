---
title: "ADR 0034 — Owner action-bound reauthentication without mandatory TOTP"
status: Approved
date: 2026-10-24
category: decisions
tags: [owner, human-auth, reauthentication, transactions, budget, security]
---

# ADR 0034 — Owner action-bound reauthentication without mandatory TOTP

**Status: Approved — explicit Owner approval on 2026-10-24.**

## Context and authority

[ADR 0033](0033-configurable-password-email-login-and-totp.md) approves password login and verified-email Magic Link/OTP; authenticator-app TOTP is **voluntary for all humans and all actions**, even for an Owner increasing the budget or changing ownership. ADRs [0024](0024-project-human-roles.md), [0027](0027-risk-based-session-management.md), [0028](0028-human-session-token-safety.md) and [0029](0029-owner-controlled-project-invitations.md) already require server-side Owner authorization, protected sessions and freshly verified human approval for sensitive actions.

The Owner **approved the fundamental non-TOTP security model** after reviewing [the proposed design](../architecture/owner-non-totp-reauthentication-design.md): reverify an Owner through their current password or a **new** verified-email challenge, bind a one-use approval to the **exact** proposed privileged operation, and **stop automatic approval in suspicious/compromised circumstances**. The design's **numeric timeouts, risk thresholds and physical schema were not approved** by that confirmation.

## Approved decision

1. **No mandatory 2FA:** No human, including an Owner, is required to enroll or enable TOTP to sign in or perform privileged actions. Existing voluntarily enrolled TOTP protection cannot be silently disabled or bypassed by switching to an inferior credential/recovery route.
2. **Distinct fresh reauthentication:** Viewing a signed-in session, having opened an old Magic Link, receiving a notification or possessing a worker/provider credential is **not** reauthentication. For each **sensitive Owner operation**, the server requires new proof of the actor's presently bound primary credential:
   - **Password-based account:** check the **current password again**, securely verified against the stored memory-hard password verifier.
   - **Verified-email-only account:** send and validate a **new, one-time, short-lived, action/session-bound Magic Link or email OTP**. An existing successful email login is insufficient.
   - **Voluntary TOTP:** respect existing enrolled account protection where applicable, without imposing enrollment on someone without TOTP.
   - A single password or email challenge remains **single-factor proof**. It is **not** phishing-resistant MFA or automatically NIST AAL2.
3. **Approve the exact intended action:** Present critical changed values before explicit affirmative confirmation: for a budget ceiling increase, project ID/name, existing/new total ceiling and currency; for ownership/permissions, the identified recipient and precise authority changes; for credential/provider settings, the exact affected permissions. Bind the fresh proof and confirmation to immutable actor/account, session, project, action, target version and payload identity.
4. **One-use transaction gate:** Create a strictly bounded authorization that is **unique to a single intended operation**; never a reusable blanket 'Owner recently authenticated' bypass. At the final Rust/SQLite write, atomically re-check active Owner membership, current session/security state, pending action, exact payload/version and unused reauthentication proof. Commit the authorized change, proof consumption, audit record and durable notification intent together; replay/reordered/concurrent attempts must not duplicate effects. Any separately billable worker dispatch still obeys [ADR 0031](0031-atomic-task-budget-admission.md) and the project hard-budget gate.
5. **Risk must not auto-approve:** With credible compromise, stale/changed credentials, unauthorized actor, missing mail for email-only verification, inconsistent state, revoked session or changed action payload, **do not complete the sensitive operation automatically**. Deny or hold the particular action, notify/audit according to risk, and offer a separately protected recovery/security-review route. Holding a risky action **never** means mandating TOTP enrollment.
6. **Preserve existing governance:** One authenticated, freshly reverified Owner may explicitly raise a project budget ceiling; all other active Owners receive in-app notifications after successful commit. No silent spending overrun, no blanket co-Owner unanimity. Ownership transfers require separate recipient identity proof/acceptance and transactionally preserve at least one Owner. Deployment administrator identity must not be treated as project Owner membership.
7. **No unsafe mail fallback:** Without working outbound mail, an email-only proof cannot be treated as completed. Existing [ADR 0033](0033-configurable-password-email-login-and-totp.md) password-reset UI instructs the user to contact the instance administrator; administration cannot view or arbitrarily set passwords or bypass enrolled protections.

## Alternatives rejected

- **Require TOTP for budget/Owner operations:** explicitly rejected by the Owner on 2026-10-24; an opt-in second factor may improve an individual's security but is never a prerequisite for these operations.
- **Rely on the ordinary active session or a generic recent-authenticated flag:** rejected, because it cannot authorize the intended **specific transaction** or protect against stolen sessions and later-modified values.
- **Treat email delivery as MFA or an independent device possession factor:** rejected. Email sign-in is an approved feature but not a claim of phishing-resistant multi-factor assurance.
- **Block every legitimate Owner without TOTP indefinitely:** rejected; an explicitly verified non-TOTP path is required. This does **not** imply a risky or demonstrably compromised action must be executed.

## Boundaries: NOT decided by this ADR

- **No numerical defaults approved:** action-proof lifetime (a five-minute window was a candidate), Magic Link/OTP time-to-live, attempt limits, resend rate/lockout, password-hash tuning, recovery cooldown and session absolute lifetime remain P0 implementation and risk-policy decisions.
- No specific risk-score algorithm, device fingerprint requirement, automatic account-wide freeze duration, mandatory external audit/quorum, universal service-admin powers or cryptographic key provider was approved.
- The concrete SQLite schema, HTTP endpoint names, hash canonicalization format, proof token handling, fraud investigation workflow, account-recovery/first-Owner bootstrap and project-vs-deployment authorization split remain proposed engineering details.
- Existing **seven days without real user activity** baseline (ADR 0027) and **24-hour default invitation lifetime** (ADR 0029) remain approved; this ADR does not reset or overwrite them.
- This is a **decision document, not working code or an executed security test**.

## Minimum future negative and integration tests

- A legitimate Owner **without TOTP** can carry out a non-suspicious, freshly authenticated and explicitly confirmed budget/ownership action.
- Current password is re-entered or **new dedicated** Magic Link/OTP used; ordinary login, background activity and prior email proof are insufficient.
- An enrolled TOTP user's protection cannot be silently bypassed by a less protected login/recovery path.
- A changed ceiling/currency, recipient, project, owner role, session or security version invalidates authorization; token reuse/concurrent requests have at most one effect.
- A revoked/compromised session, credible takeover indicators, no-mail email-only flow, or SQLite/audit failure blocks unauthorized/unsafe execution.
- An Owner budget increase records one auditable effect, preserves hard approval gates and creates durable notices for other Owners; no unconditional co-Owner quorum.
- A deployment operator lacking project Owner role cannot approve that project's budget or transfer its Owner role.

Related: [ADR 0033](0033-configurable-password-email-login-and-totp.md), [human auth P0 contract](../architecture/human-authentication-implementation-contract.md), [Owner action-specific engineering design](../architecture/owner-non-totp-reauthentication-design.md), [open questions](../project/open-questions.md), [acceptance criteria](../product/acceptance-criteria.md).

## External references (engineering evidence, not legal/compliance certification)

- [OWASP Authentication Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html) — reauthentication for sensitive changes.
- [OWASP Transaction Authorization Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Transaction_Authorization_Cheat_Sheet.html) — per-action, bounded transaction authorization and execution checks.
- [NIST SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b/authenticators/) — email is not an accepted out-of-band authenticator for NIST assurance; Rolvya's approved email login must not be mislabeled as multi-factor.
