---
title: "ADR 0005: Authoritative Database Engine"
status: Proposed
category: decision
last-reviewed: 2026-10-21
tags: [adr, sqlite, postgresql, database, transactions]
---

# ADR 0005: Authoritative database engine

## Status and authority

**Proposed — NOT approved.** The requirement is a *single authoritative database instance* where practical, prioritizing low energy, fast requests and durable concurrent admission. Neither SQLite nor PostgreSQL has been approved. Database location on the 2–4 GB control VPS versus separate infrastructure is also open.

## What the database must protect

- Immutable activated DAG versions and all hierarchy/dependency edges.
- Task vs attempt separation, worker leases and fencing tokens.
- Atomic or equivalently serialized **task + cost reservation** admission.
- Human-approved ceilings and immutable approval history; four billing categories and three scenario forecasts.
- Stable idempotency/outbox records for possibly duplicated remote provider operations.
- Success artifacts and audit history preserved across restart/budget pause.

## Alternatives

| Criterion | SQLite with WAL | PostgreSQL, single instance |
| --- | --- | --- |
| Deployment | Embedded DB file, minimal extra service | Separate DB process, pool and admin/backup operations |
| Concurrency | Many readers with a **single concurrent writer** in WAL | Multiple concurrent transactions; locking/isolation mechanisms |
| Contention | Write-lock contention can throttle fast concurrent state updates | Row-level concurrency and SERIALIZABLE option; contention/deadlocks still possible |
| Network access | Avoid direct remote workers connecting to SQLite file; one controller API | Client/server connections with authorization; remote workers should still use controller API |
| Failover | File/snapshot/WAL backup and restore need careful discipline | WAL/backup/migration stack and DBA overhead |
| Cost/RAM | Usually modest for a single control process | Must measure buffers, connections, maintenance and optional managed DB price |
| Operational risk | Network filesystem WAL and careless file copying unsafe | Connection count, transaction retries, lock ordering and replication complexity |

SQLite's official WAL docs state that WAL supports readers alongside a single writer and does not work across network filesystems; PostgreSQL documents SERIALIZABLE isolation and explicit lock modes. See source links below. These are general capabilities, not performance measurements for Rolvya.

## Working recommendation and conditional path

**Candidate A — SQLite WAL for a small one-controller proof of concept:** lower operational footprint, all writes via one authoritative control process, modest measured write contention; test crash/restore rigorously.

**Candidate B — PostgreSQL single instance for concurrency-heavy production:** preferable **if** many simultaneous reservations, lease renewals and events create unacceptable SQLite contention and memory/ops budget permits a Postgres server. A single Postgres instance still satisfies “one authoritative database”; it need not imply a cluster.

**No final selection until measurements.** Strong budget correctness is mandatory with either engine. Simplicity is valuable but not more valuable than serializable reservation safety.

## Transaction invariant and example

Let `C` = human-approved ceiling; `S` = committed/settled charges; `R` = all enforceable outstanding reservations. For a proposed attempt with conservative reserved exposure `e`, admit only if `e <= C - S - R` **inside the same serialized atomic operation** that creates the attempt lease and records the reservation. This is a logical safety test; currency conversion, delayed invoices, cancellation and actual provider hard caps still require policy and reconciliation.

Required cases:
1. Two concurrent attempts competing for insufficient remainder: at most one reserves.
2. Crash after reserve but before remote dispatch: restart finds pending outbox event without duplicate charges.
3. Stale worker reply cannot close a newer attempt's reservation.
4. Late invoice correction cannot be silently ignored or counted twice.
5. Approved ceiling update requires authenticated human decision; no environment variable can bypass it.
6. Reconstruct plan/task state after rollback and backup restore without losing verified artifacts.

Use integer minor monetary units **or another explicitly precise currency representation**. Avoid floating-point rounding for ledger invariants.

## Performance and sizing trial

Repeat identical workloads on the actual target VPS sizing and a realistic external database option. Test at least: many tiny writes per second, large DAG readiness queries, concurrent transactions, long-running readers, backup/restore during activity, restart, network latency, disk sync costs and retention. Measure p95/p99 admission, lock/retry frequency, write-ahead log growth, memory RSS, backup size, recovery duration and monthly infrastructure cost. Numbers are **not yet measured**.

## Security, backup and future migration

Remote workers cannot issue uncontrolled SQL; only authenticated control APIs handle admissions. Encrypt and back up appropriately, verify restores, migrate schema using versioned idempotent steps. Keep repository, plan, approvals and artifact references portable across engines via an explicit domain persistence interface; a later migration requires independent ADR and data-integrity proof.

## Open decision

Selection, version, hosting, isolation level/lock discipline, indexing, connection pool, recovery objectives and invoice settlement model remain open.

## Sources

- [SQLite WAL behavior and single-writer semantics](https://www.sqlite.org/wal.html)
- [PostgreSQL transaction isolation](https://www.postgresql.org/docs/current/transaction-iso.html)
- [PostgreSQL explicit locking](https://www.postgresql.org/docs/current/explicit-locking.html)

Related: [OQ-002 and OQ-003](../project/open-questions.md), [logical data model](../architecture/data-model.md), [scheduler](../architecture/scheduler.md), [budget](../architecture/budget-costs.md), [recovery](../operations/backup-recovery.md).
