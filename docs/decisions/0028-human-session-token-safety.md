---
title: "ADR 0028 — Secure Human Session Renewal, Credential Rotation and Revocation"
status: Approved
category: decisions
last-reviewed: 2026-10-22
tags: [authentication, session, tokens, security, replay, revocation, oidc]
---

# ADR 0028 — Secure human session renewal and token theft protection

- **Status:** Approved
- **Date:** 2026-10-22
- **Decision:** Rolvya must protect human UI sessions against credential theft using secure, unobtrusive renewal, appropriately supported credential rotation/replay defense, and server-enforced per-device revocation.

## Context and user approval

The owner approved (2026-10-22) the proposed *core security mechanisms*: **renew session credentials securely**, **detect reuse of invalidated renewal credentials wherever the issuer provides suitable signals**, and **let users revoke individual device sessions**. This supplements [ADR 0025](0025-oidc-passkey-authentication.md), [ADR 0026](0026-account-recovery.md) and [ADR 0027](0027-risk-based-session-management.md) and honors the owner's goal of high security without frequent visible logins.

A proposed implementation document existed first: [Human session rotation and theft-response contract](../architecture/session-token-rotation-and-theft-response.md). Its detailed implementation choices are **not automatically approved** by this ADR.

## Approved requirements

1. **Secure, unobtrusive renewal:** Normal authorized web interactions should continue without unnecessary login interruptions. Renewal must be revocable, bounded by risk and assessed session lifetimes, and must not silently increase project roles, provider access or budget authority.
2. **Secret containment:** Never expose human authentication secrets to AI prompts, worker environments, debug logs or public URLs. Browser-side handling must resist routine JavaScript exfiltration (e.g., server-managed HttpOnly cookies under the chosen web implementation). Use protected server-side credential storage and secure HTTPS.
3. **Credential rotation:** Rotate or otherwise defend renewal/session credentials according to their issuer and client capability, with a server-side invalidation/generation model where Rolvya owns the credential. Avoid a reusable stolen renewal credential remaining valid indefinitely. Browser cookie/session rotation is different from **OIDC-provider-issued refresh token rotation**, whose policy and capabilities must be verified at that provider.
4. **Reuse/replay handling:** Where a previously consumed or revoked credential is presented, reject it and investigate a possible compromise. If an OIDC provider offers refresh-token rotation/reuse-detection or sender-constrained proof, integrate its supported mechanism. Never claim *all* stolen current tokens or cookies can be detected; false positives due to concurrent browser requests need safe treatment.
5. **Per-device server-side revocation:** Users can see and terminate individual logged-in Rolvya sessions or all sessions. Revocation must be enforced by the Rust backend, including expired/revoked generations, and must survive restarts; simply removing an entry from a web page is insufficient.
6. **Incident response:** On convincing token theft/replay or account recovery, invalidate affected sessions/credential families according to risk, require fresh strong sign-in for restoration, record the event, notify the legitimate user through verified channels and invalidate pending sensitive approvals. Keep credentials and personally sensitive audit data protected.
7. **Privileged action separation:** Budget increases, new critical permissions, role elevation and credential-provider changes require a freshly verified **action-bound** password or new email challenge (with voluntarily enabled factor respected), exact one-use confirmation and server-side checks under [ADRs 0033–0034](0034-owner-action-bound-reauthentication.md). **OIDC/passkeys are not mandatory v0.1 login methods and TOTP is never mandatory.** A renewed ordinary session or spoken instruction is **not** proof of authorization.
8. **Quiet runtime and independent workers:** Under [ADR 0035](0035-scoped-human-incident-containment.md), targeted denial of a suspicious critical Owner action and revocation of compromised human sessions do **not** by themselves stop unrelated, independently authorized remote jobs; workers have distinct task-scoped identities, immutable DAG rules and hard per-project spending gates. Background jobs/polling never extend the **approved seven-day real-human-inactivity web baseline** of [ADR 0027](0027-risk-based-session-management.md). Do not infer a permanent session or blanket monthly forced logout.
9. **Scope:** The approved system is human web-session safety; a mobile app or Jarvis-like voice client is **an uncommitted future idea** and is not a prerequisite or authorized build task.

## Important boundary and caveats

- **Rolvya server sessions vs upstream IdP:** Rolvya owns its session identifiers and server-side revocation; an OIDC identity provider controls its own refresh tokens, support for rotation/reuse signals, and upstream logout. Rolvya cannot promise to rotate or revoke an unsupported IdP-issued credential by fiat.
- **Threat model limitation:** A currently valid stolen cookie can remain useful until blocked by another defense, server revocation, step-up or expiry. Rotating a token is not a proof that theft is detectable. Use layered controls.
- **Multi-tab safety:** Concurrent legitimate renewals, network retries and delayed responses must be considered. An outdated request is not automatic proof of malicious reuse; fail closed for privilege expansion without unreasonably disrupting routine users.

## Candidate engineering approach — NOT approved implementation specifics

The detailed [Proposed Rust/BFF and SQLite session design](../architecture/session-token-rotation-and-theft-response.md) considers opaque HttpOnly session cookies, keyed hashes of handles, protected token storage, generation-based atomic rotation, bounded race handling, IdP refresh-token replay response, CSRF safeguards, device inventory and low-overhead cleanup. These are **implementation candidates** that require technical review and tests before the system can claim protection.

Specific identity-provider selection, exact session/step-up lifetimes beyond already approved policy, whether upstream refresh tokens are stored, OIDC assurance mappings, secret backend, session schema, cryptographic libraries, cookie refresh algorithm, data retention and notification channels are **still open**.

## Future acceptance checks — no tests run

- Old or revoked cookie/session generation cannot authorize a new request after termination or approved rotation.
- User can revoke session A without automatically revoking distinct session B, and can revoke all.
- Concurrent legitimate renewals and dropped responses cannot revive revoked credentials or silently elevate permissions.
- Replayed IdP refresh token triggers issuer-supported reuse handling; where unavailable, the product does not imply detection.
- Privileged budget approval cannot be authorized by a renewed basic session without verified strong step-up.
- Idle human web session expires under approved policy even if AI remote workers continue running.
- No user token appears in model context, worker process, logs or raw public URLs.
- A stolen **current** cookie's residual risk is documented rather than falsely claimed impossible.

Existing specified future tests: [AC-041–AC-048](../product/acceptance-criteria.md); they are **not implemented or passed**.

## Alternatives and consequences

- **Rejected:** indefinite bearer renewal with no server revocation.
- **Rejected:** silent low-assurance email/password fallback and independent authorization by agents.
- **Chosen:** layered, provider-capability-aware rotation/replay defense and per-device revocation without unnecessary login prompts.
- **Consequences:** requires sound credential lifecycle, SQLite-transactional session state and careful IdP conformance tests. It does **not** require implementing optional mobile or voice experiences.
