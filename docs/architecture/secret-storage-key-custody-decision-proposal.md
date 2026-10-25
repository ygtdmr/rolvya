---
title: Approved Encrypted SQLite and Automatic Local Key Custody
status: Approved store and local unlock; recovery implementation proposed
category: architecture
last-reviewed: 2026-10-25
tags: [security, secrets, kms, credential-store, worker, p0]
---

# Secret Broker encrypted SQLite and automatic local key unlock — approved

## Status and question

**Approved initial store:** [ADR 0044](../decisions/0044-encrypted-sqlite-secret-storage.md) selects **application-layer encrypted secret records in SQLite** as the first Secret Broker backend. [ADR 0037](../decisions/0037-central-secret-broker-task-scoped-access.md) still governs least-privilege broker access and [ADR 0038](../decisions/0038-multi-account-isolated-cli-and-api-access.md) governs independent CLI/API accounts. **[ADR 0045](../decisions/0045-automatic-local-master-key-unlock.md) approves automatic loading of an existing protected master key file on the remote controller host**; [ADR 0047](../decisions/0047-secret-broker-xchacha20-key-hierarchy.md) subsequently approves XChaCha20-Poly1305, 256-bit OS KEK and wrapped per-secret DEKs. OQ-005/P0 remains open for detailed crypto serialization/versioning, provisioning/rotation/recovery and worker bootstrap. No secret manager, credential, paid resource or encryption code has been deployed.

The store **and startup strategy** are settled: encrypted SQLite records and **automatic, noninteractive loading of a protected local controller-host key file**. The exact file path, key lifecycle and separately protected recovery mechanism remain open. A short-lived worker control token is never a substitute for AI provider credentials. The planned AWS Fargate first worker does not automatically require all secrets to be stored in AWS or allow an AWS-only control-plane architecture.

## Proposed provider-neutral boundary

- Keep SQLite the **only authoritative project/task/budget database**, storing connection identities, owner/project grants, allowed access modes, secret references, audit and revocation state. Do **not** store raw unencrypted API keys, password/session grants or GitHub App private keys in those fields.
- The initial Rust `SecretStore` implementation persists **authenticated encrypted credential records in SQLite**. Keep a small internal interface to avoid entangling business policy with encryption; this does **not** require implementing or shipping other backends. Root key material is held in a **separate access-restricted controller-host file** outside SQLite and its ordinary backups and loaded automatically during restart (ADR 0045). The exact path and first-time provisioning are open.
- Only the trusted Rust Secret Broker can resolve or decrypt a connection's secret. Every use must verify authenticated actor/connection, project scope, provider mode, fixed task and attempt, worker identity/lease epoch, account eligibility, explicit budget reservation and cancellation status.
- Prefer **brokered API calls** where provider support permits; only release CLI session material in an explicitly permitted isolated, short-lived remote runtime with per-connection protected home, redacted logs and cleanup. Raw upstream static credentials can be exfiltrated from a compromised worker even if its controller token is short-lived.
- CLI subscriber terms, unattended-hosted rights, auth refresh and model/account quota remain separately gated under OQ-008; no official rights can be assumed merely from a successful login.

## Previously evaluated physical stores (selection now approved)

| Candidate | Benefits | Limits / risks |
| --- | --- | --- |
| **A. Hosted secrets manager first** (e.g. AWS Secrets Manager) | External lifecycle, IAM/access audit and provider-managed infrastructure; fits first real AWS adapter | Request/secret charges, cloud lock-in for first concrete adapter, must solve controller identity from non-AWS control VPS, not automatically a least-privilege worker entitlement |
| **B. Envelope-encrypted credential records inside SQLite** + encryption root stored **outside** SQLite/repository/backups | Minimal operational dependencies, single authoritative SQLite instance, provider-neutral, can be economical for small self-host deployments | Higher responsibility for crypto correctness, key access, rotation and disaster recovery; same-host filesystem/process compromise can expose key + ciphertext; external KMS or tightly protected operator-supplied root is still needed |
| **C. External secrets-management server** (e.g. dedicated Vault service) | Broad independent features for large installations, multi-provider | More memory, administration, recovery/key-unseal and always-on cost; not preferred initial default on constrained 2–4 GB controller |

**Owner decision:** choose **option B, encrypted SQLite credential records with the master/key-encryption key kept outside SQLite, ordinary backups and source code** ([ADR 0044](../decisions/0044-encrypted-sqlite-secret-storage.md)). Option A (AWS Secrets Manager) and option C (Vault) are **not first-version requirements**. A future alternate backend needs its own explicit scope/decision. The later ADR 0045 selects the initial **protected local key-file source with automatic restart loading**, without deciding concrete key file location, encryption primitive or a complete recovery workflow. It does not imply whole-file SQLite encryption.

## Necessary implementation safeguards

1. Use well-reviewed authenticated encryption with per-record authenticated metadata binding to connection/provider/project/key version. Do not invent custom encryption algorithms. Keys and raw secrets must never be present in prompts, normal logs, source, frontend bundles, task DAGs or public audit exports.
2. Root key/KEK must remain outside encrypted ciphertext and ordinary SQLite backups, and must have a recoverable but separately protected restore path. Plan key rotation, retained old versions during staged re-encryption, locked invalid/old key behavior and tested recovery of older backup points. A backup without its corresponding key is not a usable restore.
3. An external managed secret store is **not** implemented initially. If separately approved in future, give only the trusted broker narrow IAM permissions. For initial encrypted SQLite, decrypt inside trusted Rust broker only after validating attempt, project and account authorization; do not distribute root key to workers.
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
- Protect SQLite backups/storage and account for their actual storage/transfer charges, if any, within the approved project cost policy; no Secrets Manager recurring fee is assumed. Any future billable external backend requires explicit approval.
- Real audits, provider isolation and cloud IAM tests are executed **only after separately approved test expenses and authorized accounts**.

## Remaining implementation and security decisions

**Already Owner-approved:** first secrets backend is encrypted SQLite ([ADR 0044](../decisions/0044-encrypted-sqlite-secret-storage.md)); its root key is automatically read at startup from a separate OS-protected controller-host file ([ADR 0045](../decisions/0045-automatic-local-master-key-unlock.md)). Manual unlock on every restart and mandatory managed KMS are not required.

**Still OQ-005/P0:** exact protected path and process user, safe first-run provisioning details, exact Rust crate versions and record format, rotation/versioning, independently protected key recovery and backup restore, host compromise and remote worker identity. Missing/invalid keys must block secrets without silently overwriting ciphertext. **Do not claim protection from a fully compromised host.** The encrypted off-host recovery kit approved in [ADR 0046](../decisions/0046-off-host-encrypted-master-key-recovery.md) must be implemented and proven before production with real connected credentials, without requiring manual entry at normal restart.

## References

- [Approved ADR 0037 — Central Secret Broker](../decisions/0037-central-secret-broker-task-scoped-access.md)
- [Approved ADR 0038 — Multi-account CLI and API](../decisions/0038-multi-account-isolated-cli-and-api-access.md)
- [Worker authentication and lease policy](worker-authentication-policy.md)
- [Open OQ-005](../project/open-questions.md)
- [Operational secrets](../operations/secrets.md)
- [OWASP Secrets Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html)
- [AWS Secrets Manager cross-account authorization](https://docs.aws.amazon.com/secretsmanager/latest/userguide/auth-and-access_examples_cross.html)
