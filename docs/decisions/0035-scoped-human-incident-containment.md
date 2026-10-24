---
title: "ADR 0035 — Scoped containment of suspicious Owner actions"
status: Approved
date: 2026-10-24
category: decisions
tags: [security, incident-response, owner, sessions, workers, budget]
---

# ADR 0035 — Scoped containment of suspicious Owner actions

**Status: Approved — explicit Owner confirmation on 2026-10-24.**

## Context

[ADR 0034](0034-owner-action-bound-reauthentication.md) already requires fresh, single-use, transaction-bound approval for sensitive Owner operations, with **no mandatory TOTP**. [ADR 0028](0028-human-session-token-safety.md) requires safe human-session theft response. Remote workers have **separate identities, leases, hard budget gates and independently approved work** ([ADRs 0006](0006-worker-communication.md), [0010](0010-inflight-budget.md), [0031](0031-atomic-task-budget-admission.md)). Suspected compromise of an Owner's human session must not automatically terminate unrelated, previously approved coding work.

The Owner explicitly approved the proposed scoped response: **when there is credible evidence of account takeover or dangerous reuse, stop or hold the relevant sensitive human operation and record the incident. Do not automatically stop unrelated normal account activity or previously authorized, budget-bounded workers merely because of that human risk signal.**

## Approved policy

1. **Scope the initial denial/hold to the unsafe privileged operation.** A suspicious request to increase a budget, transfer Owner authority or change credentials/permissions must not commit while relevant proof, current human session, account ownership or audit integrity is in doubt. Denial/hold must take place **before** any irreversible or billable effect.
2. **Correlate affected authorizations.** If multiple pending privileged approvals depend on the **same compromised/replayed credential, session generation or account recovery state**, invalidate those related approvals as well; do not leave a different malicious pending Owner action usable. Scope by affected identity/security authority, project and proof.
3. **Protect compromised human sessions, separately from worker scheduling.** For convincing human-session theft/replay, reject the presented token and revoke the **affected session or credential family**, as required by ADR 0028. If evidence instead establishes an **account-wide** compromise, protective account/session-wide revocation can be necessary. This is not an indiscriminate global worker stop. Distinguish a user's valid session termination from any independent previously authorized execution.
4. **Preserve unrelated ordinary activity.** Do not globally suspend other unrelated users, projects or valid low-risk sessions solely because one attempted Owner action is suspicious. Normal permitted activity proceeds where the relevant session/authority remains valid and risk is isolated. A specific session revoked for theft may no longer be used by its holder; this is a targeted security response, not a product-wide pause.
5. **Preserve separately authorized remote work and artifacts.** Validly admitted, independent remote workers continue **only inside their pre-reserved enforceable exposure**. Existing validated outputs and budget accounting remain durable. No automatic worker cancellation or newly invented retry/spend authorization arises merely from a suspicious human approval request.
6. **Escalate worker controls only for independent machine/budget compromise.** If investigation reveals worker identities, provider API keys, GitHub App permissions, project data isolation, infrastructure integrity or enforceable spending caps are themselves compromised, use the **affected** worker/security/financial stop-on-risk and fencing controls in their own approved policies. The human-action-only decision never prevents a necessary independent stop.
7. **Record and communicate accurately.** Persist redacted, project/access-scoped incident and action outcome data, including actor, affected action and proof/session references, evaluated policy decision, disposition and appropriate notification intent. Do **not** log passwords, OTPs, complete links, session cookies or provider secrets; do not claim notice delivery before evidence exists.
8. **Protected restoration, no hidden TOTP mandate.** A denied/held Owner action can be retried only with a **new** action-specific approval and securely verified state after resolution. Recover a stolen session or account through separately protected methods. Never automatically force TOTP enrollment, grant extra Owner authority, require all co-Owners to sign, raise budget or bypass the final-Owner invariant. Approved 5-minute action approval, 10-minute email challenge, five failed email OTP attempt cap and single-use remain unchanged.

## Evidence boundaries

Examples of **potentially credible** events: confirmed use of a revoked session, an unambiguous one-time credential replay, independently supported account/session takeover, or a compromised credential still linked to a pending critical operation. **Signals** such as an unfamiliar IP, new device, location change, unusual budget value or a recent legitimate password recovery **are not automatically proof of compromise in isolation**. Exact evidentiary thresholds, false-positive handling, risk states, incident-review authority, hold expiry, unlock/cooldown, notification delivery and identity-recovery steps are **P0 implementation choices**, not approved numbers/algorithms in this ADR.

If a required auth/policy/SQLite transaction cannot be verified, the critical action **fails closed**. An independent worker continues only if its existing credential, lease, project isolation and enforceable reserved budget remain verifiably safe. A failure in the shared budget-admission authority can separately block **new** billable admissions regardless of this decision.

## Alternatives rejected

- **Stop all agents or pause every project on any suspicious human sign-in:** rejected; needless loss of permitted work, possible extra cloud costs and unnecessary global disruption.
- **Ignore linked pending approvals while denying just one HTTP request:** rejected; stale/replayed credential state must not authorize a different privileged effect.
- **Ignore confirmed session theft to avoid interrupting ordinary UX:** rejected; targeted server-side revocation is already approved by ADR 0028.
- **Force TOTP enrollment to clear a hold:** rejected under ADR 0033.
- **Treat existing worker activity or AI output as substitute Owner consent:** rejected under hard-budget approval gates.

## Future acceptance scenarios — not executed

- Suspicious/replayed critical Owner authorization is denied or held before the budget/role/credential change commits.
- Other pending authorizations tied to the same stolen credential/session generation are invalidated.
- Confirmed stolen human session is revoked; an unrelated, valid human session and unrelated users/projects are not automatically disabled.
- Pre-admitted remote worker with unaffected secret, scoped lease and enforceable reserved budget **continues**; existing results remain available; a human incident never expands the ceiling.
- Independent compromise of worker token/provider key/budget enforceability triggers existing scoped worker stop-on-risk despite the general continuity rule.
- Evidence-only new IP/device change cannot be hardcoded as conclusive takeover without a documented risk policy.
- Security incident outcome and necessary notifications are durable, access controlled, redacted and not falsely marked delivered.
- Held privileged action requires fresh proof and current authorization for any subsequent attempt; no compulsory 2FA enrollment.

## Deliberately still open

Risk/compromise evidence thresholds, incident evaluator design, hold state machine, support workflow, recovery credential process and delay, exact alert routing and rate limits remain under OQ-004. **This decision is not an implemented detection system and does not authorize executing real provider calls.**

Related: [Owner reauthentication](0034-owner-action-bound-reauthentication.md), [human-session theft protection](0028-human-session-token-safety.md), [account recovery](0026-account-recovery.md), [incident implementation design](../architecture/owner-non-totp-reauthentication-design.md), [open P0 decisions](../project/open-questions.md), [acceptance tests](../product/acceptance-criteria.md).
