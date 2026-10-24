---
title: "ADR 0025: OIDC-first human authentication with phishing-resistant passkeys"
status: Approved
date: 2026-10-22
---

# ADR 0025: OIDC-first human authentication with phishing-resistant passkeys

## Context
Rolvya needs secure human sign-in for the approved Owner, Editor and Viewer roles (ADR 0024), with secure authorization and account recovery. The subsequent [ADR 0033](0033-configurable-password-email-login-and-totp.md) explicitly permits password authentication; the former passwordless-only constraint is superseded. The project owner explicitly approved the OIDC + passkey-capable + strong session security approach on 2026-10-22 and emphasized maximizing authentication security.

## Decision
- **Initial Rolvya v0.1 human sign-in is governed by [ADR 0033](0033-configurable-password-email-login-and-totp.md): password or verified-email Magic Link/OTP, with optional app-based TOTP.** OIDC is a conditional federated authentication extension, not a mandatory initial entry point.
- For future explicitly enabled federated sign-in, prefer phishing-resistant passkeys (FIDO2/WebAuthn) where the provider proves this assurance. TOTP is not phishing-resistant and initial v0.1 methods do not imply a passkey implementation.
- If federated OIDC is later enabled, use Authorization Code flow with PKCE (S256), state/nonce as applicable, and strict issuer, audience, signature, expiry and redirect URI validation; forbid implicit flow and untrusted algorithms.
- Treat the Rust backend as the authority for project roles and authorization. Identity-provider login does not grant Owner privileges by itself. Apply least privilege and server-side authorization checks to all sensitive operations.
- Prefer a secure server-managed session with Secure, HttpOnly, SameSite cookies; protect against CSRF, session fixation, token leakage and replay. Rotate/invalidate sessions appropriately, enforce idle and absolute expiration, and provide logout/revocation behavior.
- Require recent strong reauthentication/step-up for sensitive actions such as approving a budget increase, changing critical security settings or managing access. Reject or defer high-risk changes if the required authentication assurance cannot be verified; do not assume that an ordinary OIDC token proves recent passkey authentication.
- Maintain auditable sign-in, failed login, privileged-change and session events without exposing credentials or tokens; apply rate limiting, provider key rotation validation and secure secret storage.
- **ADR 0033 supersedes the former ADR 0032 login-method selection:** initial methods are password and verified-email Magic Link/OTP, with optional authenticator-app TOTP 2FA. Password verifiers must be strongly salted and memory-hard; no plaintext passwords. Google/GitHub social login is not a mandatory v0.1 method. Email-only proof cannot substitute for independently verified strong Owner step-up. Recovery and account linking must not silently downgrade authentication assurance.
- Deployment uses HTTPS and trusted origin/redirect configuration. Compatible external OIDC providers for self-hosted instances remain a future configurable option, not a required v0.1 login method.

## Implementation status and boundaries
**The general federation and strong-session safeguards remain binding when applicable. The OIDC-first initial-login mandate is superseded by ADR 0033.** Account recovery, precise step-up assurance, session duration and future IdP conformance remain pending implementation/security validation.

## Consequences and alternatives
Future federation may reduce password handling and offer phishing-resistant authentication, while the approved v0.1 password route needs its own secure verifier and recovery controls. A custom weak password mechanism is forbidden; a secure password-verifier implementation or trusted managed auth service may be selected under ADR 0033.

## Related decisions
- [ADR 0024 — Project human roles](0024-project-human-roles.md)
- [Identity and authorization architecture](../architecture/identity-authorization.md)

## Subsequent Owner-approved methods (current 2026-10-24)

[ADR 0033](0033-configurable-password-email-login-and-totp.md) replaces ADR 0032: password and email verification-only login (Magic Link/OTP), with optional authenticator-app TOTP and Owner-controlled method configuration. The GitHub App repository integration remains distinct from human sign-in. Google/GitHub social sign-in is not a required initial method. The trusted OIDC/broker integration and high-assurance human step-up implementation remain unverified.
