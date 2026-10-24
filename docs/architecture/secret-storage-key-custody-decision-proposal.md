---
title: Secret Broker Storage and Key Custody — Decision Proposal
status: Proposed
category: architecture
last-reviewed: 2026-10-25
tags: [security, secrets, kms, credential-store, worker, p0]
---

# Secret Broker physical storage and key custody — proposal

## Status and question

**Proposed, not yet Owner-approved.** The central Rust-side Secret Broker and least-privilege attempt-scoped credential access have already been **Approved** in [ADR 0037](../decisions/0037-central-secret-broker-task-scoped-access.md), including CLI/API account separation in [ADR 0038](../decisions/0038-multi-account-isolated-cli-and-api-access.md). **OQ-005/P0** remains open for concrete storage encryption, key custody, provider deployment and worker bootstrap. This draft does not create an actual secret, cloud bill or provider login.

The next decision is **where encrypted credential material lives, who controls master encryption keys, and which backend is supported first**. A short-lived worker control token is never a substitute for AI provider credentials. The planned AWS Fargate first worker does not automatically require all secrets to be stored in AWS or allow an AWS-only control-plane architecture.

## Proposed provider-neutral boundary

- Keep SQLite the **only authoritative project/task/budget database**, storing connection identities, owner/project grants, allowed access modes, secret references, audit and revocation state. Do **not** store raw unencrypted API keys, password/session grants or GitHub App private keys in those fields.
- Put actual secret storage and envelope encryption behind a Rust `SecretStore`/key-custody interface. The store may be an approved hosted manager or self-hosted encrypted records **without becoming a second authoritative task database**.
- Only the trusted Rust Secret Broker can resolve or decrypt a connection's secret. Every use must verify authenticated actor/connection, project scope, provider mode, fixed task and attempt, worker identity/lease epoch, account eligibility, explicit budget reservation and cancellation status.
- Prefer **brokered API calls** where provider support permits; only release CLI session material in an explicitly permitted isolated, short-lived remote runtime with per-connection protected home, redacted logs and cleanup. Raw upstream static credentials can be exfiltrated from a compromised worker even if its controller token is short-lived.
- CLI subscriber terms, unattended-hosted rights, auth refresh and model/account quota remain separately gated under OQ-008; no official rights can be assumed merely from a successful login.

## Candidate physical stores (choice still open)

| Candidate | Benefits | Limits / risks |
| --- | --- | --- |
| **A. Hosted secrets manager first** (e.g. AWS Secrets Manager) | External lifecycle, IAM/access audit and provider-managed infrastructure; fits first real AWS adapter | Request/secret charges, cloud lock-in for first concrete adapter, must solve controller identity from non-AWS control VPS, not automatically a least-privilege worker entitlement |
| **B. Envelope-encrypted credential records inside SQLite** + encryption root stored **outside** SQLite/repository/backups | Minimal operational dependencies, single authoritative SQLite instance, provider-neutral, can be economical for small self-host deployments | Higher responsibility for crypto correctness, key access, rotation and disaster recovery; same-host filesystem/process compromise can expose key + ciphertext; external KMS or tightly protected operator-supplied root is still needed |
| **C. External secrets-management server** (e.g. dedicated Vault service) | Broad independent features for large installations, multi-provider | More memory, administration, recovery/key-unseal and always-on cost; not preferred initial default on constrained 2–4 GB controller |

**Recommended policy for Owner review:** approve **a provider-neutral SecretStore interface**, and offer both hosted-manager storage and envelope-encrypted self-hosted storage **only after each implementation passes its own security and restore gates**. Avoid a universal mandatory paid provider. If an initial MVP must choose exactly one concrete production backend, select it based on the actual control-plane deployment and verifiable key custody, not solely on Fargate being the first worker. A secrets backend is not a second authority for plan/budget/task state. This is a recommendation **not** a chosen priority/order.

## Necessary implementation safeguards

1. Use well-reviewed authenticated encryption with per-record authenticated metadata binding to connection/provider/project/key version. Do not invent custom encryption algorithms. Keys and raw secrets must never be present in prompts, normal logs, source, frontend bundles, task DAGs or public audit exports.
2. Root key/KEK must remain outside encrypted ciphertext and ordinary SQLite backups, and must have a recoverable but separately protected restore path. Plan key rotation, retained old versions during staged re-encryption, locked invalid/old key behavior and tested recovery of older backup points. A backup without its corresponding key is not a usable restore.
3. Hosted secrets manager access must be performed by trusted broker identity with narrow IAM policies and access verification. Do not grant a worker blanket `GetSecretValue` to all user connections; if a specific worker direct credential is unavoidable, limit access by exact attempt/project/expiry where supported and revoke on stop.
4. Every grant is auditable without exposing secret content; track authorized access, provider account, connection and attempt. Revoke/disconnect and incident containment are explicit actions; deleting a local blob does **not** revoke an upstream copied credential.
5. A failed secret store, failed decrypt, compromised identity, unverified provider-entitlement or missing cost reservation is an explicit **block**, never authorization to use a previously cached/other user's credential or silently fall back CLI→API.
6. Keep existing approved worker token **15 minutes**, heartbeat **30 seconds**, lease **90 seconds**, and scoped stop-on-risk/budget hard gates. These values govern worker authorization, not upstream model token lifespan.
7. Encrypted CLI account sessions may require provider-specific refresh serialization; prohibit concurrent mutation of a shared auth home across remote workers. Test account isolation and withdrawal while workers are in flight.
8. Instrument secret-access errors without leakage, TLS-bound endpoints, administrative emergency rotation, incident alerts, and key-custody restore drills. Do not send provider credentials through the browser JS bundle after initial authorized account connection.

## Acceptance evidence before closing OQ-005

- Cross-project/cross-account stolen worker token cannot read another connection; revoked/expired lease is denied even while a worker is running.
- Unauthorized API fallback and unknown CLI subscription rights block the attempt.
- A key is rotated and a prior compatible encrypted backup is restored with the correct archived key; backups alone never expose plaintext.
- Broker restart and store outage do not accidentally authorize paid work or leak plaintext.
- Compromised worker that accessed an unscoped upstream key is treated as a real compromise requiring upstream revocation/rotation.
- Confirm whether secrets-backend usage and storage charges exist in approved budgets, with deterministic blocked-on-insufficient-budget behavior if they create new spend.
- Real audits, provider isolation and cloud IAM tests are executed **only after separately approved test expenses and authorized accounts**.

## Needed Owner decision

1. Is **backend-neutral support** with a selectable managed/self-hosted secret store preferred over making AWS Secrets Manager mandatory?
2. Which **first deployable secret backend** is preferred after evaluating control-plane location, operator key availability, expected connection count and real recurring costs?
3. Which trusted location will retain/recover master keys, with explicit host-compromise residual-risk acceptance?

Do not mark the physical store, concrete encryption root or deployment identity **Approved** until the Owner decides. Next implementer may write more detailed risk checks without claiming those choices were made.

## References

- [Approved ADR 0037 — Central Secret Broker](../decisions/0037-central-secret-broker-task-scoped-access.md)
- [Approved ADR 0038 — Multi-account CLI and API](../decisions/0038-multi-account-isolated-cli-and-api-access.md)
- [Worker authentication and lease policy](worker-authentication-policy.md)
- [Open OQ-005](../project/open-questions.md)
- [Operational secrets](../operations/secrets.md)
- [OWASP Secrets Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html)
- [AWS Secrets Manager cross-account authorization](https://docs.aws.amazon.com/secretsmanager/latest/userguide/auth-and-access_examples_cross.html)
