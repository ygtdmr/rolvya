---
title: "ADR 0025: OIDC-first human authentication with phishing-resistant passkeys"
status: Approved
date: 2026-10-22
---

# ADR 0025: OIDC-first human authentication with phishing-resistant passkeys

## Context
Rolvya needs secure human sign-in for the approved Owner, Editor and Viewer roles (ADR 0024), without implementing or storing a homegrown password database. The project owner explicitly approved the OIDC + passkey-capable + strong session security approach on 2026-10-22 and emphasized maximizing authentication security.

## Decision
- Use standards-based **OpenID Connect (OIDC)** as the primary human sign-in architecture with configurable trusted identity providers, including suitable self-hosted providers.
- Prefer and support **phishing-resistant passkeys (FIDO2/WebAuthn)** through the chosen identity provider. OIDC itself does not guarantee a passkey; provider and authentication-context capabilities must be checked. Do not falsely label every OIDC sign-in phishing-resistant.
- Use Authorization Code flow with **PKCE (S256)**, state and nonce checks where applicable, strict issuer, audience, signature, expiration and redirect URI validation; forbid implicit flow and untrusted algorithms.
- Treat the Rust backend as the authority for project roles and authorization. Identity-provider login does not grant Owner privileges by itself. Apply least privilege and server-side authorization checks to all sensitive operations.
- Prefer a secure server-managed session with Secure, HttpOnly, SameSite cookies; protect against CSRF, session fixation, token leakage and replay. Rotate/invalidate sessions appropriately, enforce idle and absolute expiration, and provide logout/revocation behavior.
- Require recent strong reauthentication/step-up for sensitive actions such as approving a budget increase, changing critical security settings or managing access. Reject or defer high-risk changes if the required authentication assurance cannot be verified; do not assume that an ordinary OIDC token proves recent passkey authentication.
- Maintain auditable sign-in, failed login, privileged-change and session events without exposing credentials or tokens; apply rate limiting, provider key rotation validation and secure secret storage.
- **ADR 0032 additionally approves passwordless-only human sign-in for v0.1:** Google, GitHub and verified-email one-time link/code. Do not develop a Rolvya-local password store, password form or password-based recovery/fallback. Email-only proof cannot substitute for independently verified strong Owner step-up. Recovery and account linking must not silently downgrade authentication assurance.
- Deployment must use HTTPS and trusted origin/redirect configuration. Preserve support for self-hosted instances via compatible OIDC providers.

## Implementation status and boundaries
**Architecture approved; implementation and provider conformance tests are pending.** Specific identity providers, session durations, fallback/recovery policy and concrete assurance claim enforcement remain to be evaluated and tested, without weakening these guardrails.

## Consequences and alternatives
This reduces password-handling burden and supports phishing-resistant sign-in, but transfers parts of authentication assurance and uptime to chosen identity providers. Alternatives of inventing local password auth and requiring one specific consumer login provider are rejected for v1.

## Related decisions
- [ADR 0024 — Project human roles](0024-project-human-roles.md)
- [Identity and authorization architecture](../architecture/identity-authorization.md)

## Subsequent Owner-approved methods (2026-10-23)

[ADR 0032](0032-passwordless-human-sign-in.md) specifies Google, GitHub, and verified-email one-time link/code, with **no password login or recovery**. GitHub's consumer login integration needs validated identity provenance and cannot assume a GitHub OAuth token is itself an OIDC ID token. The trusted OIDC/broker integration and high-assurance human step-up implementation remain unverified.
