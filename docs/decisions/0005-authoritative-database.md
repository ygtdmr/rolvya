---
title: "ADR 0005: SQLite as the Authoritative Database"
status: Approved
category: decision
last-reviewed: 2026-10-21
tags: [adr, sqlite, database, persistence, transactions]
---

# ADR 0005: SQLite as the authoritative database

## Status and approval

**Approved by the project owner on 2026-10-21.** This supersedes the former *Proposed* SQLite-versus-PostgreSQL evaluation in this ADR. The owner explicitly decided: **Rolvya will use SQLite.** Database engine selection is closed.

## Context

Rolvya needs a lightweight and efficient single authoritative database for the immutable plan DAG, tasks/attempts, lease epochs, approval history, costs and artifacts metadata. The control plane is intended for a small remote VPS, with heavy coding/build work running on isolated remote workers.

## Decision

1. **SQLite is the only authoritative database engine for Rolvya.** Do not introduce PostgreSQL or a second relational/NoSQL engine as a fallback or parallel source of truth without a new explicitly approved ADR.
2. Use **one authoritative database instance per deployment** as the current architecture model. SQLite's database storage should be accessed through the Rust backend's persistence layer, **not directly by remote workers** over the network.
3. Store the active immutable DAG, task and attempt states, approvals, reservations, ledger entries, worker state and audit references in SQLite. Binary logs/build artifacts may live in separate remote object/artifact storage, with provenance stored in SQLite; artifact storage is not a second authoritative database.
4. Budget reservation and attempt lease admission must be **atomic and serialized**; never rely on multiple independent reads/writes for project budget safety.
5. SQLite-specific details including **WAL mode, busy timeout, foreign keys, synchronous/durability settings, transactions, pool size, indexes, migration tooling and snapshot/backup mechanism** require technical validation. WAL is a recommended candidate, **not a separate user-approved configuration**.
6. Any alternative database in the future requires explicit owner approval, a superseding ADR, integrity/migration tests and documented impact. This is not a planned automatic scale fallback.

## Why SQLite was selected

The owner finalized SQLite; its embedded deployment style aligns with the preferred single-instance, low-footprint remote control plane. SQLite is not automatically faster for all workloads, and concurrent writers still serialize. The team must design around SQLite's write contention and reliable backup discipline rather than silently swapping engines under load.

## SQLite-specific implementation contract

- **Single-writer coordination:** create the lease and cost reservation within one SQLite write transaction; recheck dependencies, budget and fencing state within the same serialized admission boundary.
- **Foreign keys:** enable and test foreign key enforcement on **every connection**. Validate plan/task/dependency/attempt relationships.
- **Busy/lock handling:** bound waits, use carefully scoped transactions, prefer short write transactions and fail safely rather than spin or double-reserve.
- **WAL consideration:** evaluate WAL on a locally attached filesystem. SQLite documents one writer at a time and that WAL does not operate as intended across network filesystems; avoid sharing a live database file via network filesystem between workers.
- **Durability:** decide and test sync/checkpoint/backups against explicit RPO/RTO; SQLite transactions alone do not prove restore viability.
- **Schema evolution:** versioned migrations, preflight checks, rollback/forward plan and restore tests.
- **Money/accounting:** store exact integer minor units or another precise checked representation, enforce project currency semantics, append-only financial events and immutable approvals. Never use binary floating-point to authorize spending.
- **Read paths:** index dependency and attempt/lease lookups, and measure large DAG reads on the small control host.

## Critical transaction invariant

Let `C` = approved ceiling, `S` = settled costs, `R` = enforceable outstanding reservations. For conservative exposure `e`, admit billable work only if `e <= C - S - R`, within the **same atomic serialized SQLite transaction** that persists a fenced attempt and reservation.

Invoice lag, provider-side billing minimums and in-flight charges can still create real overrun risks. The owner's **no automatic percentage overrun** policy remains mandatory; conservative reservations/provider limits and reconciliation must be designed and disclosed.

## Validation and recovery gates

1. Two concurrent admissions competing for insufficient budget cannot both succeed.
2. Crash after reservation but before remote dispatch resumes from durable state/outbox without duplicates.
3. FK checks reject cross-plan references and invalid dependency links.
4. Stale attempt cannot modify newer successful state or budget reservation.
5. Large DAG reads, bursty heartbeat writes and audit append performance remain within measured control VPS capacity.
6. Backup taken during supported write load can be restored and reconciled with Git/artifact refs and ledger.
7. Workers never directly open or copy the live SQLite database file.

## Rejected or superseded alternatives

PostgreSQL and other database engines were considered in an earlier draft. They are **not approved fallbacks** after this decision; benchmarking should optimize SQLite, not reopen the database choice without the owner's request.

## References

- [SQLite transactions](https://www.sqlite.org/lang_transaction.html)
- [SQLite WAL mode and filesystem limitations](https://www.sqlite.org/wal.html)
- [SQLite foreign keys](https://www.sqlite.org/foreignkeys.html)
- [SQLite online backup API](https://www.sqlite.org/backup.html)

Related: [ADR 0004 — Rust-only backend](0004-control-plane-language.md), [data model](../architecture/data-model.md), [Rust + SQLite backend contract](../architecture/rust-sqlite-backend.md), [scheduler](../architecture/scheduler.md), [budget](../architecture/budget-costs.md), [backup recovery](../operations/backup-recovery.md).
