---
title: Secret Broker Key Bootstrap, Rotation and Disaster Recovery Proposal
status: Approved recovery principle; implementation proposed
category: architecture
last-reviewed: 2026-10-25
tags: [security, secrets, encryption, bootstrap, rotation, recovery, sqlite, p0]
---

# Secret Broker — first setup, safe rotation and recovery proposal

## Approval boundary

**The encrypted off-host recovery kit with a separately retained recovery secret is Owner-approved in [ADR 0046](../decisions/0046-off-host-encrypted-master-key-recovery.md).** Bootstrap, crypto choices, storage medium, rotation and operational details below remain Proposed. Previously approved binding decisions are:
- [ADR 0044](../decisions/0044-encrypted-sqlite-secret-storage.md): encrypt secret records before persistence in the one authoritative SQLite database.
- [ADR 0045](../decisions/0045-automatic-local-master-key-unlock.md): on every normal start, the Rust Secret Broker automatically reads an **existing** protected key file on the remote controller host; no manual unlocking and no silently regenerated key.
- [ADR 0037](../decisions/0037-central-secret-broker-task-scoped-access.md): account/project/attempt/budget/lease authorization and brokered secrets; [ADR 0038](../decisions/0038-multi-account-isolated-cli-and-api-access.md): separately isolated Claude/Codex CLI accounts and API-key modes.

The Owner subsequently approved the off-host encrypted recovery kit and separate emergency recovery secret. This does not authorize a particular provider, cipher, key wrapping procedure, upload or spending. No filesystem, database, live key or provider resource exists because of this proposal.

## Three-stage recommended operator experience

| When | Proposed behavior | Essential protections |
| --- | --- | --- |
| **First authorized installation only** | One-time setup generates high-entropy master/key-encryption material through a reviewed operating-system cryptographic random source and atomically writes the existing-key file on the remote controller host. | Explicit trusted setup permission; exclusive no-follow creation with strict ownership/permissions; durable flush; no stdout/env/CLI history/repository logs; disallow silently initializing over existing database state, stale installation markers or an unrecognized key. |
| **Normal startup/restart** | Rust reads the already-provisioned protected file automatically and verifies key identity/format before enabling access to encrypted credentials. | Keep approved [ADR 0045](../decisions/0045-automatic-local-master-key-unlock.md): missing/mismatched/key-permission failure **blocks**, with no replacement generation or paid task dispatch requiring secrets. |
| **Controlled key rotation / host-loss recovery** | Rotation is version-aware and recoverable. A separately protected **encrypted recovery copy** of the required key material lives **off the controller host and outside ordinary SQLite backups**. Restoring after real host loss is a privileged, audited recovery operation, **not** an additional manual step for routine startup. | No loss of prior ciphertext access, old matching SQLite backup/key pairs remain restorable for their retention period, and host compromise triggers provider-credential revocation as well as master-key replacement. |

The [separate technical crypto/bootstrap proposal](secret-broker-cryptography-bootstrap-proposal.md) evaluates 256-bit OS-generated master keys, XChaCha20-Poly1305, independently wrapped per-secret DEKs and a first-run guarded installation state machine. **These exact cryptographic choices are not yet Owner-approved.** The off-host encrypted recovery kit principle remains Approved ADR 0046.

## Proposal A — key creation and validation

1. Only an **explicitly authorized first-time setup** can create a new root key; the first ordinary startup must **never** treat an absent key as authorization to generate a new one.
2. Use the system CSPRNG and a vetted Rust crypto library; choose exact AEAD, key sizes, file format and key wrapping only with implementation review. Never derive a master key from time, project name, machine ID or a hard-coded value.
3. Treat on-disk key creation as an exclusive, atomic, crash-safe operation with a dedicated non-root process and OS-restricted parent directory/file. Check ownership, permissions and unsafe symlink/path substitution; write and sync before treating initialization as complete.
4. Keep a nonsecret installation/key-version identifier and explicit initialized state to detect preexisting encrypted credentials and file/database mismatches without storing plaintext key bytes in SQLite. On errors, fail safely; do not expose enough information to support enumeration of secrets.
5. Prevent key leakage through backup agents, process command line/environment, core dumps where practicable, test fixtures, frontend, application logs or worker/bootstrap scripts. Full host/root compromise remains a documented residual risk.

## Proposal B — safe versioned rotation

1. Rotate on verified compromise/suspected exposure and planned maintenance under a documented future lifecycle policy; **do not automatically rotate on every startup** or set an arbitrary calendar interval without evidence.
2. Require fresh Owner-authorized, auditable approval for a privileged rotation request; rate-limit and serialize rotation attempts. Evaluate **separate encryption data keys and key-encryption keys (DEK/KEK)** and versioned ciphertext to minimize bulk rewrites; exact cryptographic envelope layout remains open.
3. Acquire/write new key material safely, preserve required decryptability of existing and archived encrypted records, use durable checkpoints/transactions and perform staged re-encryption/rewrapping with validation. Keep the prior protected key version only for the explicitly necessary restore/read window; do not silently destroy retained backups' required keys.
4. Under incomplete migration, crash, database lock, disk-full or key mismatch, fail closed for unsafe operations and resume/rollback from durable state without data loss. Never declare a rotated key 'active' while files/database/backup manifests disagree.
5. Rotation of *local encryption keys* does **not revoke copied Claude/Codex sessions or upstream API keys**. A breach requires separate provider-side revocation/rotation and scoped halt/reconciliation.

## Proposal C — independently protected emergency recovery

- **Recommend an encrypted recovery kit** containing the necessary versioned key material and verified metadata, protected by a strong recovery secret, stored **in a separate trusted location** from the controller and ordinary SQLite snapshot storage; **do not** replicate the usable unprotected root key into the DB backup. The specific encryption/derivation method, delivery path, custody medium and recovery identity proof remain to be chosen and validated.
- Create/update recovery material after initial setup and key changes; provide verifiable, redacted success/error status **without printing or auto-exposing recovery plaintext**. No automatic upload to an unapproved cloud provider, no provider-specific recurring fee or account connection is authorized by this proposal.
- Before restoring, an authenticated operator intentionally puts the system into recovery/maintenance, denies new paid dispatch, fences or reconciles active remote jobs, verifies backup/ciphertext and matches correct key version. Recover the protected key file onto a clean trusted remote host and test decryption with **synthetic** records before reconnecting real accounts.
- Restore should validate protected SQLite state, costs/leases, Git refs, audit integrity and pending remote charges. If key or backup cannot be verified, hold rather than generating a replacement or claiming repaired credentials.
- Keep normal restarts **unattended**. Emergency recovery may involve Owner/operator intervention and access to a separate recovery secret. A design that is both unattended after total controller loss *and* uses no independent trusted key custody cannot promise safe secret recovery.
- Encrypt and protect the recovery kit; decide how its unlock secret is independently retained and recovered. Losing both the live key and its recovery protection may make ciphertext **permanently unrecoverable**. Backup recoverability must be tested on an isolated remote host before production with real credentials.

## Design alternatives still to choose

| Option | Benefit | Limitation |
| --- | --- | --- |
| **Recommended: encrypted off-host recovery kit with separately held recovery secret** | No always-on cloud dependency or routine manual unlock | Requires secure emergency custody and occasional verified restore drill |
| Unprotected copy of master key inside DB backup | Apparently simple | **Forbidden by ADR 0044/0045:** defeats storage separation |
| No recovery material | No backup-custody work | Permanent loss of all protected credentials after controller key loss |
| Mandatory hosted KMS/Secrets Manager | Central enterprise controls | Not required by current architecture; extra cost/identity/dependency and would need Owner scope approval |

## Validation plan (future checks, not executed)

- First-time authorized setup succeeds once and atomically; simultaneous setup and interrupted setup never overwrite keys or encrypt with conflicting keys.
- Normal restart loads the existing key without interaction; an absent, corrupt, substituted or permission-unsafe key blocks; no spontaneous replacement key appears.
- A controlled versioned rotation persists across a forced crash at each stage, old archived encrypted backups remain recoverable, and stale worker access never bypasses project/account authorization.
- Restore a matched SQLite backup and separately retrieved encrypted recovery kit on a clean remote host, with synthetic credentials, without manual involvement in **subsequent normal restarts**.
- Wrong recovery secret, tampered recovery kit, missing version, compromised host, backup mismatch and underfunded recovery actions fail safely, preserve incident evidence and expose no plaintext.
- An intentional provider credential revocation follows compromise of a worker or its upstream account; rotating only local ciphertext keys is never mistaken for that revocation.

## Pending explicit Owner choice

**Resolved by ADR 0046:** Use an encrypted off-host recovery kit with a separately protected recovery secret, needed only for emergency restore. Normal startup remains automatic. Exact custody, cipher, initial provisioning and rotation method remain open.

**Still P0:** exact encryption algorithm/library, safe key-file bootstrap path, recovery-kit wrapping/custody method and threat model, rotation checkpoints, provider CLI entitlement OQ-008 and remote worker attestation. Do not mark OQ-005 resolved by this proposal.

## Sources and related docs

- [OWASP Cryptographic Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Cryptographic_Storage_Cheat_Sheet.html)
- [OWASP Secrets Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Secrets_Management_Cheat_Sheet.html)
- [OWASP Key Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Key_Management_Cheat_Sheet.html)
- [ADR 0045 — Auto-unlock](../decisions/0045-automatic-local-master-key-unlock.md)
- [Secret storage/key custody](secret-storage-key-custody-decision-proposal.md)
- [Backup/recovery operations](../operations/backup-recovery.md)
- [Open questions](../project/open-questions.md)
