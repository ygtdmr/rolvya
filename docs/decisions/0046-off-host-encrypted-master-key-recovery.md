# ADR 0046 — Encrypted off-host key recovery

Status: **Approved — recovery architecture**  
Date: 2026-10-25  
Owner approval: The Owner accepted an encrypted emergency recovery copy stored independently of the controller server, protected with a separately held recovery secret.

## Decision

1. The existing approved Rust Secret Broker and encrypted SQLite storage ([ADR 0044](0044-encrypted-sqlite-secret-storage.md)) must have an **encrypted recovery copy** of the key material necessary for restoring backed-up encrypted credentials.
2. Store the encrypted recovery copy **outside the controller server** and separately from normal database backups. Protect it with an **independently retained recovery secret**. Never package either plaintext key or its recovery secret into SQLite backups, repository, logs, containers, workers, or browser files.
3. Normal restart remains automatic using the protected controller-host key file under [ADR 0045](0045-automatic-local-master-key-unlock.md). The separate recovery secret is accessed **only during deliberate emergency restore**, never on ordinary startup.
4. Recovery must verify the encrypted database backup and matching key version, use a clean authorized remote host, hold risky new dispatch until remote attempts/costs are reconciled, and require independent successful restore testing before production. If recovery material is missing or invalid, fail safely; **never regenerate keys to overwrite encrypted records**.
5. After key rotation, retain required recoverability for appropriately retained older snapshots and update/verify the emergency recovery copy. Rotating encryption keys does **not** revoke any compromised provider API key or CLI login; upstream revocation is separate.
6. No mandatory paid secrets manager, cloud account, new database, or personal-computer agent is introduced. Paid external storage remains subject to the existing budget approval gate.

## Open implementation details

The exact cryptographic algorithms and Rust libraries, key provisioning, off-host recovery medium/provider, recovery-secret custody, rotation mechanics, permission checks and restore workflow remain **OQ-005/P0**. Neither the specific external storage location nor automatic uploading is approved by this ADR. [Implementation options](../architecture/secret-key-bootstrap-rotation-recovery-proposal.md) remain proposals where not already covered by explicit decisions.

## Verification state

This is a documented approval only: no encrypted kit, database, encryption key, tests or live cloud resources have been created. Future tests must verify restore from a matching encrypted SQLite backup using the separately held recovery secret and that subsequent ordinary restarts remain automatic.

Related: [backup and restore](../operations/backup-recovery.md), [secrets](../operations/secrets.md), [open questions](../project/open-questions.md), [project status](../project/status.md).
