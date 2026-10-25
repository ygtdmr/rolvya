---
title: Approved SQLite Secret Cryptography and First-Run Bootstrap Contract
status: Approved crypto/bootstrap baseline; implementation details proposed
category: architecture
last-reviewed: 2026-10-25
tags: [secrets, cryptography, rust, sqlite, bootstrap, keys, p0]
---

# Secret Broker cryptography and first-run bootstrap — approved baseline

## Scope and decision boundary

**Owner-approved:** [ADR 0047](../decisions/0047-secret-broker-xchacha20-key-hierarchy.md) selects 256-bit OS-generated KEK, XChaCha20-Poly1305, independently generated per-secret DEKs wrapped under KEK, and authorized one-time guarded setup. Exact data format, Rust package versions, recovery-kit cryptography, backup destination and security validation remain **Proposed/Open OQ-005/P0**. Existing approved requirements remain binding:
- [ADR 0037](../decisions/0037-central-secret-broker-task-scoped-access.md): trusted Rust Secret Broker, scoped secret access and provider-entitlement checks.
- [ADR 0044](../decisions/0044-encrypted-sqlite-secret-storage.md): application-layer encrypted SQLite credential records, not necessarily whole-file SQLite encryption.
- [ADR 0045](../decisions/0045-automatic-local-master-key-unlock.md): automatic normal restart using **existing** protected local controller-host key file, separate from SQLite and ordinary backups; no silent key replacement.
- [ADR 0046](../decisions/0046-off-host-encrypted-master-key-recovery.md): separately retained **encrypted** off-controller-host recovery kit and independent recovery secret, used only in deliberate emergency recovery. **No recovery storage provider, fee, upload path or exact recovery-secret format has been approved.**

This document specifies the **approved secret-row cipher and controlled first-time setup baseline**, with illustrative—not yet approved—data structures, file paths and failure-recovery implementation. Detailed key recovery wrapping, deployment identity and production signoff remain OQ-005/P0. No encryption is implemented or tested.

## Approved primitive and Rust dependency family

| Purpose | Approved baseline (version/details pending) | Explanation |
| --- | --- | --- |
| Root key / key-encryption key (KEK) | **256-bit (32-byte) random secret** | Generate **once** using the controller OS cryptographic random source; never derive from hostname, timestamp, username or weak password |
| Cryptographic randomness | Rust `getrandom` API using OS facilities | Fail setup on RNG failure; no fallback to predictable randomness |
| Cipher for secret records and key wrapping | **XChaCha20-Poly1305 AEAD** (256-bit key, **24-byte nonce**) | Authenticated encryption binds ciphertext and intended record metadata, practical random nonces for many small records; nonce must still never repeat under one key |
| Primary Rust implementation | RustCrypto `chacha20poly1305` | Maintained crate exposing `XChaCha20Poly1305`; lock exact versions and review dependency/license/audit status at implementation |
| Key hierarchy | One **KEK**, random independent per-secret **DEKs** (32 bytes each), with versioned wrapping | A change to the KEK can often rewrap DEKs without re-encrypting all secret contents; explicit metadata/version handling and migration checks required |
| In-memory handling | Tight Rust scope, best-effort zeroization where supported | Reduce exposure in memory/logs; not a guarantee against privileged host compromise |
| Emergency kit | **Existing approved off-host encrypted kit** (ADR 0046) | Candidate use of authenticated envelope encryption; wrapping key acquisition and separately stored recovery-secret format **still open** |

**Boundary:** XChaCha20-Poly1305 is broadly deployed but is **not itself NIST/FIPS-approved**. If a deployment later needs validated FIPS modules, investigate an appropriate validated AES-256-GCM implementation and separate Owner approval rather than asserting compliance. Do not change algorithm silently or promise formal audits without evidence.

## Proposed authenticated record layout (exact schema not approved)

Conceptual fields (not approved SQL DDL):
- `cipher_suite_version`, `key_id`, `wrapped_dek`, `wrap_nonce` (24 bytes), `data_nonce` (24 bytes), `ciphertext`, `tag`/combined AEAD output, and stable, validated connection/project/purpose identifiers.
- Encrypt secret bytes with a **new independent DEK per secret record** and new OS-generated data nonce. Wrap each DEK with the controller KEK and a distinct wrap nonce. Never use one nonce twice with the same key.
- Authenticate the record purpose, stable project/connection identifier, field type, serialization version and key-context identifiers as **AEAD associated data (AAD)**. A record copied to another project/account/purpose must fail integrity verification; avoid binding to mutable display names.
- Validate metadata and authorization **before** decryption. Reject missing/unknown cipher versions, malformed lengths, invalid tags and mismatched AAD **without** returning plaintext or a silent fallback to another account/key.
- Minimize plaintext residency and reject accidental logging, tracing, core-dump and SQLite temporary-file exposure. Only encrypted values enter DB/WAL/ordinary DB backup. No root KEK/DEK bytes in task DAG, prompt, frontend, log or Git repository.
- Do not mistake **encryption at rest** for worker isolation: a compromised controller with its usable KEK may still decrypt credentials. Exposed model-provider credentials require **upstream revocation**, not only KEK rewrapping.

## Guarded first-time setup baseline approved; precise crash-safe sequence proposed

A separate authorized **install/setup** action (not ordinary controller startup) controls all first-run initialization.

1. **Inspect existing state:** if the SQLite instance already contains initialized credentials, installation markers or unknown key metadata, **refuse automatic initialization**. Never interpret a missing key file on an existing installation as first-run setup.
2. **Authorize a new installation:** run under an explicitly trusted installer/service boundary on the **remote** controller host. Allocate a dedicated non-root service identity. Choose the exact key path and locked-down OS integration at implementation.
3. **Generate high-entropy root key:** obtain exactly 32 random bytes from the OS CSPRNG; abort on randomness failure. Avoid printing, shell arguments, environment variables, logs, world-readable configuration or repo files.
4. **Create the protected file safely:** exclusive no-follow creation with strict owner/parent permissions, file mode `0600` and directory mode `0700` (or stricter/equivalent); flush and validate file content/identity. Do not overwrite an existing key, follow a symlink, or use a temporary insecure file.
5. **Bind initialized DB and key identity:** persist an explicit nonsecret installation identifier, schema/key-version metadata and recoverable setup state. Filesystem and SQLite cannot be committed in one atomic transaction; document crash/restart and reconciliation cases rather than pretending otherwise.
6. **Prepare recovery before real secrets:** ADR 0046 requires a separately encrypted off-host recovery kit. Do not report the deployment **recoverable/production ready** until actual independently protected recovery media, key-version match and isolated restore test exist. A missing kit cannot be fixed by putting plaintext root key into ordinary database backups.
7. **Enable normal automatic start only after safe setup:** once fully initialized and verified, subsequent starts only **read** the existing protected key file (ADR 0045). Missing/invalid/mismatched keys **block** and preserve existing encrypted records; no replacement is generated on startup.

Rotations use version-aware key wrapping and a verified emergency recovery-kit update; [key lifecycle design](secret-key-bootstrap-rotation-recovery-proposal.md) describes the pending detailed crash-safe migration and version-retention strategy. There is no requirement to rotate automatically at every restart.

## Required failure-case checks

- Missing key + existing DB -> hard failure, no overwrite/new key.
- Interrupted bootstrap at any file/DB state -> explicit reconciled setup or manual audited recovery, never duplicate identities or silent migration.
- Two concurrent first-time installers -> at most one active root key and installation binding.
- Wrong file owner/mode, symlink, wrong key version, RNG failure, bad ciphertext/tag/AAD -> fail closed and keep data intact.
- Copy encrypted row to different project/account -> integrity failure.
- Change/root KEK rotation -> validate ability to decrypt both active records and authorized archived backup versions with corresponding retained recovery material.
- Real provider credential exposure -> separately revoke/rotate the upstream credential; merely encrypting a different local copy does not cure compromise.
- Existing approved budget/approval boundary remains enforced throughout install/recovery/worker dispatch.

## Remaining implementation decisions and already approved baseline

**Resolved by [ADR 0047](../decisions/0047-secret-broker-xchacha20-key-hierarchy.md):** use XChaCha20-Poly1305, OS-generated 256-bit KEK, separate random wrapped DEK for each protected record, and controlled once-only setup. This **does not approve** a recovery cloud provider, physical file path, recovery-password/KDF format, runtime credentials, automatic upload, licensed CLI use or spending.

Implementation still needs secure code review, dependency version locking, fuzz/property/failure tests, OS service hardening, encrypted-kit wrapping/custody and restoration details. OQ-005/P0 remains open until these pass.

## Verifiable references

- [RustCrypto ChaCha20Poly1305 documentation](https://docs.rs/chacha20poly1305/latest/chacha20poly1305/).
- [Rust getrandom — OS cryptographic randomness](https://docs.rs/getrandom/latest/getrandom/).
- [OWASP Cryptographic Storage](https://cheatsheetseries.owasp.org/cheatsheets/Cryptographic_Storage_Cheat_Sheet.html).
- [OWASP Key Management](https://cheatsheetseries.owasp.org/cheatsheets/Key_Management_Cheat_Sheet.html).

Related: [ADR 0044](../decisions/0044-encrypted-sqlite-secret-storage.md), [ADR 0045](../decisions/0045-automatic-local-master-key-unlock.md), [ADR 0046](../decisions/0046-off-host-encrypted-master-key-recovery.md), [OQ-005](../project/open-questions.md), [operations secrets](../operations/secrets.md), [project status](../project/status.md).
