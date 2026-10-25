# ADR 0047 — XChaCha20-Poly1305 and versioned KEK/DEK cryptography for SQLite secrets

Status: **Approved — initial cryptographic architecture and guarded first setup**  
Date: 2026-10-25  
Owner decision: “tamamdır”, explicitly accepting the proposed 256-bit OS-generated key, XChaCha20-Poly1305, RustCrypto, individually wrapped per-secret data keys, and guarded first-run provisioning.

## Context

[ADR 0037](0037-central-secret-broker-task-scoped-access.md) establishes the trusted Rust Secret Broker; [ADR 0044](0044-encrypted-sqlite-secret-storage.md) selects authenticated-encrypted SQLite records; [ADR 0045](0045-automatic-local-master-key-unlock.md) requires automatic subsequent startup from a protected controller-host key file; [ADR 0046](0046-off-host-encrypted-master-key-recovery.md) requires an encrypted, off-host recovery kit protected by a separately held emergency recovery secret. OQ-005/P0 still required a concrete initial cryptographic and bootstrap direction.

## Approved decision

1. **Secret-row AEAD:** use **XChaCha20-Poly1305** with 256-bit keys and 24-byte per-operation nonces for authenticated encryption of protected secret records and for wrapping data keys. Never reuse a nonce with the same key. The exact serialization and migration format require implementation review.
2. **Rust libraries:** start with the reviewed RustCrypto `chacha20poly1305` implementation providing `XChaCha20Poly1305`; draw cryptographically secure bytes from the OS using Rust `getrandom` or its vetted equivalent. Lock/test actual crate versions during implementation; no custom cipher is permitted.
3. **Key hierarchy:** the trusted Rust controller holds a random 256-bit **key-encryption key (KEK)** in the separately protected file already approved by ADR 0045. Each independent protected secret record has a fresh random 256-bit **data-encryption key (DEK)**, which is itself wrapped under the KEK with authenticated encryption and key-version metadata. The clear KEK and DEKs are never persisted in SQLite, ordinary backups, app bundles or prompts.
4. **Record binding:** AEAD associated authenticated data must bind stable project/connection identity, record purpose/field and version to ciphertext and its wrapped DEK, preventing unauthorized swapping between accounts/projects. Rust must separately enforce human/worker/attempt/lease and budget authorization **before** decryption. Reject malformed metadata, mismatched key versions or invalid authentication tags without trying another account or key.
5. **One-time controlled bootstrap:** generate the initial random KEK **only** during a clearly authorized first-time setup on a fresh installation. Use atomic/exclusive file creation with strong ownership/permission checks and durable initialization state, rejecting symlinks and unsafe replacements. A missing or corrupted key on an **existing** installation is never a cue for automatic regeneration. Subsequent ordinary restarts read the existing key with **no manual unlock**.
6. **Versioned change and recovery:** cryptographic record/key identifiers must support future staged KEK rewrapping without silently breaking existing or retained backup snapshots. The off-host encrypted recovery kit and its separate emergency secret remain mandatory per ADR 0046. Choosing this cipher for SQLite records **does not automatically choose** the emergency recovery password/KDF/wrapping technique or kit storage provider.
7. **Deployment/security boundary:** a running controller fully compromised together with its live KEK may expose protected credentials. Local key rotation does not revoke stolen provider API keys or CLI sessions; upstream revocation/incident policy remains separate. XChaCha20-Poly1305 choice does **not** claim a FIPS-validated cryptographic module or full SQLite file encryption.

## Implementation choices still open — OQ-005/P0

Exact Rust dependency versions, on-disk and SQL ciphertext layout, zeroization behavior, file location and service account, concurrency/crash-safe setup reconciliation, nonce handling, key rotation checkpoint scheme, secure recovery-kit encryption and custodian location, backup restore drills, worker attestation and final threat testing. These details cannot be silently labeled production-tested by this ADR. The first version does not require AWS Secrets Manager or an additional authoritative database.

## Required future verification (not executed)

- Two competing/interruptible first-time setups never create conflicting KEKs; existing encrypted state with missing key always fails closed without overwrite.
- Different project/account ciphertext, swapped wrapped DEKs, invalid tags, wrong nonces/versions and corrupted files do not decrypt or leak protected data.
- SQLite/WAL/ordinary DB backups contain encrypted secret material only; no plaintext KEK/DEK/credentials.
- Normal restart is automatic; an encrypted off-host recovery kit permits explicitly authorized emergency restore with separate recovery secret and matching key version, followed by automatic normal restart.
- Rotation preserves valid historical backup decryption and never reauthorizes expired leases, account sessions or insufficient-budget operations.

## Implementation status

**Approved documentation-level technical selection only.** No encryption code, actual KEK, DEK, key file, SQLite schema, backup, live provider secret or passing security test has been created or verified.

Related: [crypto/bootstrap technical contract](../architecture/secret-broker-cryptography-bootstrap-proposal.md), [key lifecycle plan](../architecture/secret-key-bootstrap-rotation-recovery-proposal.md), [OQ-005](../project/open-questions.md), [tests](../product/acceptance-criteria.md), [status](../project/status.md).
