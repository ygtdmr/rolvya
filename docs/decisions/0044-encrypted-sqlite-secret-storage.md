# ADR 0044 — Encrypted SQLite credential storage for the first Secret Broker

Status: **Approved — initial secrets storage choice**  
Date: 2026-10-25  
Owner decision: “Şifreli sqlite seçeneği ile ilerleyeyim.”

## Context

[ADR 0037](0037-central-secret-broker-task-scoped-access.md) approved a trusted Rust Secret Broker with attempt-scoped, least-privilege access. [ADR 0038](0038-multi-account-isolated-cli-and-api-access.md) approved multiple independently isolated Claude/Codex CLI connections alongside separately authorized API-key mode. The physical secret storage backend had not been selected (OQ-005/P0). The Owner chose the encrypted SQLite option over making a hosted secrets manager the initial dependency.

## Decision

1. **First supported secrets backend: SQLite with application-layer authenticated encryption of secret records.** The trusted **Rust Secret Broker** encrypts secret values **before inserting them** into SQLite and decrypts only after authorization checks. The existing SQLite database remains the **sole authoritative DB** for projects, task/budget state and encrypted connection records. No separate cloud secret store or Vault process is required for the first implementation.
2. **Do not confuse encrypted secret records with whole-database encryption.** Secret columns are ciphertext with versioned encryption metadata, not plaintext. The decision **does not** claim ordinary SQLite file pages, public metadata, indexes, SQLite WAL or all other database content are encrypted at rest. Full-file database encryption is a separate optional, future decision.
3. **Master key / key-encryption key lives outside SQLite, source code, images, logs, and regular database backups.** Store no master key or equivalent cleartext recovery material in the same database as the ciphertext. Require protected provisioning, rotation, offline/independently protected recovery and fail-closed operation on missing, invalid or compromised keys. The **exact master-key custody mechanism and startup identity remain OQ-005/P0** and are *not* implied by this storage choice.
4. **Use established reviewed cryptographic libraries**, randomized nonces and authenticated encryption (AEAD), ciphertext integrity, versioned key identifiers and authenticated binding of secret metadata to the intended connection/account/project/purpose. A per-secret data-encryption key and envelope wrapping scheme may be used if shown to improve rotation/recovery; **exact cipher, crate, serialization schema and envelope layout are not yet approved**. Do not invent cryptography.
5. **Access constraints remain binding:** Rust checks caller/worker identity, approved project and connected account, task/attempt, unexpired fenced lease, provider mode, budget authority and verified account entitlement before secret use. Workers normally receive brokered requests instead of a plaintext upstream key. Provider-supported CLI flows requiring local runtime credentials may receive only the minimum necessary material inside a separately authenticated, isolated *remote* environment. Do not store or pass raw credentials in DAG prompts, logs, audit, UI bundles or model contexts.
6. **Protect backup and key lifecycle separately.** Encrypt and authenticate secret data prior to writing to SQLite so backups, WAL and copied files contain ciphertext for protected fields. Plan rotation, key versions, recoverable old backups, key loss and host-compromise response. If the controller and its live decryption key are both compromised, encryption alone does **not** prevent plaintext recovery. Do not claim such a guarantee.
7. **Other secret backends are not first-version requirements.** Future optional manager/KMS adapters may be evaluated through a neutral internal interface, but this ADR **does not approve or require** AWS Secrets Manager, a second authoritative database, a running Vault service, or building two backends now. This selection **does not** change the already-approved first *worker* provider, AWS ECS Fargate (ADR 0041).

## Alternatives evaluated

- **Hosted secrets manager first (e.g. AWS Secrets Manager):** not selected as the first required store due to added provider dependency and potential ongoing charges. It can be separately proposed in the future.
- **Dedicated external Vault service:** not selected for v0.1 due to additional always-on infrastructure and key/unseal operations.
- **Plaintext SQLite secrets:** unacceptable.
- **Full-SQLite-file encryption alone:** not selected as a substitute for explicit secret lifecycle, purpose binding and Secret Broker authorization; may be evaluated as defense-in-depth later.

## Operational and implementation gates still open (OQ-005/P0)

- **Master key provisioning and custody:** protected service credential, file, hardware-backed/KMS or operator-controlled delivery; boot-time access, disaster recovery, identity/permissions and compromise tradeoffs require a separate concrete decision.
- Exact AEAD algorithm/library, rotation and data-encryption-key policy, salt/nonce/associated-data format, key identifiers, tamper handling, corruption detection and memory-clearing strategy.
- Authorization and per-account brokered connection implementation, provider-specific CLI token-refresh, session isolation, upstream revocation and hosted subscription terms (also OQ-008/P0).
- Locked-down host/service filesystem, decrypted temporary CLI home, secure transport, incident response and tested backup/restore.
- Physical SQL schema/migrations, how to prevent raw values in SQLite temporary/log/WAL artifacts, and interaction with redacted audit records.

## Future acceptance (not executed)

- An unauthorized project, task or stale lease fails before any decryption; account A cannot access account B.
- Backed-up/copied SQLite, including WAL/shm where applicable, reveals no plaintext **credential values**; missing root key prevents recovery.
- Reconnect, key rotation and restoring an old encrypted backup with a separately protected matching key are verified. Missing/compromised key blocks safely.
- Broker restart, malicious record replacement, wrong metadata/AAD, connection revocation and compromised worker do not silently recover other accounts' secrets.
- No paid AWS secret manager is provisioned, no provider credential is committed, and no change to API/CLI entitlement or hard spending is implied.

## Implementation status

**Documentation decision only.** No SQLite encryption schema, Secret Broker implementation, master-key provisioning, key rotation, live AWS resource, credential onboarding or security test has been created or verified by this ADR.

Related: [OQ-005 open implementation details](../project/open-questions.md), [storage alternatives and now-selected direction](../architecture/secret-storage-key-custody-decision-proposal.md), [operational secrets](../operations/secrets.md), [SQLite domain schema](../architecture/data-model.md), [backup and recovery](../operations/backup-recovery.md), [ADR 0037](0037-central-secret-broker-task-scoped-access.md), [ADR 0038](0038-multi-account-isolated-cli-and-api-access.md).
