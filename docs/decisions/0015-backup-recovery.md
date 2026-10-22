# ADR 0015 — Backup and Disaster Recovery Policy

Status: **Approved**
Date: 2026-10-22

## Decision
Rolvya's single authoritative SQLite database uses:
- A **consistent automated backup every 15 minutes**.
- A **daily recovery point every 24 hours**.
- Retention of frequent recovery points for the **most recent 7 days** and daily recovery points for the **most recent 30 days**. Avoid interpreting these as requiring deletion of the daily points within the 7-day window.
- **Encrypted off-server remote storage** independent of the control host.
- **Periodic restore tests**; the specific test cadence and recovery objectives are implementation parameters.

These times are initial policy defaults approved by the owner, not proof that 15-minute data-loss bounds are always attainable. Failed backups must be monitored and surfaced.

## SQLite backup integrity
- Use a SQLite-supported online backup procedure (or verified consistent database snapshot) that handles WAL coherently. Do **not** simply copy a live main database file while ignoring WAL/checkpoint state.
- Version and verify backup manifests/checksums, encryption, restoreability, schema/migration compatibility and access permissions.
- Secure separate storage and key management; prevent workers from accessing controller backups.

## Restart and recovery
1. On ordinary controller restart, **prefer the intact current database**; inspect its integrity and reconcile in-flight operations. Do not automatically replace it with an older backup.
2. If corruption or loss requires restoration, pause **new billable launches**; restore the latest *verified* consistent snapshot, only after authorized operator/recovery policy permits it.
3. Reconcile each active task/attempt and provider job by durable operation identity: externally running jobs, results/artifact evidence, Git refs, lease epochs, unconfirmed cancellation and delayed invoices. Fence stale leases and prevent duplicate paid launches; do not automatically rerun everything.
4. Reconcile the approved budget ledger with actual provider usage and uncertain in-flight exposure before resuming billable dispatch. Unknown exposure is **not** treated as zero, and budget is never raised automatically (ADR 0010).
5. Preserve previously validated artifacts and completed work, and ensure dependent DAG execution remains blocked until prerequisites are verified.
6. Resume only after integrity, authorization, lease, budget and external side-effect checks succeed.

## Verification scenarios
- Concurrent WAL writes while a backup is taken; restored database passes integrity tests.
- Main server reboot with live remote workers; controller recognizes existing attempts and does not start duplicates.
- Restore 15 minutes behind with already-launched provider jobs; inspect/reconcile first, never blindly re-dispatch.
- Late bill for a cancelled worker; charge remains visible and correctly budgeted.
- Missing/corrupt backup, failed upload, lost encryption key, incompatible schema; recovery blocks with actionable alert.
- Periodic restore into a disposable isolated environment without production credentials, ensuring usable DAG and financial records.

## Open implementation details
Backup provider, storage-region/data-retention policies, backup and restoration tooling, encryption-key rotation, detailed checkpoint frequency, specific restore-test schedule, exact RPO/RTO objectives, incident approvals, and fallback storage remain undecided.

Related: [SQLite configuration](0014-sqlite-initial-configuration.md), [budget policy](0010-inflight-budget.md), [retry policy](../architecture/retry-escalation-policy.md).
