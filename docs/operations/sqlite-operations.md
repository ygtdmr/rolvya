---
title: SQLite Operational Safety and Recovery
status: Proposed
category: operations
last-reviewed: 2026-10-21
tags: [sqlite, backup, wal, recovery, operations]
---

# SQLite operational safety and recovery

## Authority

**SQLite is approved and mandatory** as Rolvya's authoritative database ([ADR 0005](../decisions/0005-authoritative-database.md)); this operations specification proposes how to use it safely. No actual SQLite file, production backup or restored system has been verified. All control-plane backend logic is Rust ([ADR 0004](../decisions/0004-control-plane-language.md)).

## Topology and file ownership

Keep the database on a **remote controlled persistent volume** accessible to the Rust control process. Restrict OS permissions; do not put the DB in public/static web directories, committed Git files or worker images. Never have remote worker instances open the live SQLite file, especially over network filesystems.

For a SQLite connection pool, enforce busy handling and foreign keys on **every connection**. **WAL is already approved under ADR 0014**; treat the main database plus `-wal` and `-shm` state as an operational unit. Avoid copying a running database file with a naive filesystem copy; use SQLite's supported online-backup or snapshot mechanisms with verified consistency.

## Candidate production settings (must be benchmarked)

| Area | Candidate / invariant | Verification required |
| --- | --- | --- |
| Journal mode | WAL is a candidate for concurrent reading | Validate real filesystem semantics, checkpoint strategy and backups |
| Foreign keys | **Always ON per connection** | Negative FK and cross-plan tests |
| Writer concurrency | One serialized SQLite writer at a time | Budget reservation race and queue fairness |
| Busy timeout | Bounded, observable wait | No infinite retries, no duplicate admission |
| Transaction mode | Short write transactions; `BEGIN IMMEDIATE` a candidate | Serially recompute budget before lease creation |
| Synchronous/durability | Choose per measured safety and RPO | Kill/crash and restore experiments |
| Connection pool | Small bounded pool appropriate to 2–4GB VPS | RSS, lock contention, p95/p99 admission |
| WAL checkpoints | Monitor WAL size and checkpoint progress | Long reader handling, disk growth |
| Disk capacity | Reserve working headroom and alert early | Graceful read-only/fail-closed admission |
| Backup | SQLite-supported consistent snapshot | Restore ledger/DAG/artifacts and verify checksums |

## Backup and restore playbook

1. Halt or fence new billable admissions and record the current project event sequence.
2. Take a **consistent SQLite backup** using a supported online-backup path or a proven application pause with transactional snapshot; encrypt and store remotely with integrity metadata.
3. Preserve schema version, application binary version, object-store manifests, remote Git commit refs, budget approvals and unsettled reservation snapshot.
4. Test restoration into an isolated environment; run SQLite integrity and foreign-key checks and validate DAG uniqueness/acyclicity.
5. Reconcile remote provider worker launches, ambiguous Git writes, completed artifact hashes and late-billed charges; do not assume snapshot time matches provider billing time.
6. Fence stale leases and revoke compromised worker credentials before resuming.
7. Re-enable billable task admission **only** if the current approved ceiling and reservations remain consistent; a budget-paused project stays paused until explicit human approval.

**ADR 0015 already approves 15-minute automated backups, daily recovery points and retention of 7-day frequent / 30-day daily points.** RPO/RTO, restore-test cadence, remote provider pricing and cryptographic key management remain to be finalized.

## Migration requirements

Use versioned, idempotent Rust-controlled SQLite schema migrations with preflight disk/backup/schema checks. During migration, stop new billable admissions, prevent old executables from issuing incompatible writes, verify foreign key integrity and maintain rollback/restoration plans. Prefer forward-compatible additive changes; destructive migrations require explicit review and verified backups. Do not run SQL copied from a doc against production.

## Incident cases

| Incident | Immediate safe behavior |
| --- | --- |
| Locked DB / write contention | Bound retry, keep ledger authoritative, do not start unfunded attempts |
| Full disk / write failure | Fail closed on new billable admissions; preserve existing artifacts |
| Corrupt database or incomplete WAL | Isolate, assess, restore verified backup, reconcile external side effects |
| Backup exists but untested | Do not claim recoverability; run isolated restore |
| Lost worker with pending reservation | Fence old lease and reconcile costs before retry |
| Restore point older than provider invoices | Hold admission until spending/exposure reconciled |
| Untrusted worker tries DB access | Deny by network/OS boundaries and audit |

## Test evidence and monitoring

Measure transaction conflicts, busy timeouts, WAL/checkpoint size, SQLite integrity checks, query latency, restoration duration, database RSS/disk usage and backup age. Use simulated billing and Git adapters for destructive fault tests. Record platform/filesystem, SQLite version, Rust driver, config and exact restoration commands when selected.

## Open decisions

**Approved:** WAL enabled, five-second busy timeout, max four SQLx pool connections (ADR 0014) and 15-minute/daily backup cadence with 7-/30-day retention (ADR 0015). **Open:** checkpoint interval, `synchronous` durability, migration framework, backup destination/encryption/key custody, restore test cadence, RPO/RTO and recovery host placement.

Related: [backend contract](../architecture/rust-sqlite-backend.md), [data model](../architecture/data-model.md), [backup policy](backup-recovery.md), [incident runbooks](runbooks.md), [deployment](deployment.md).
