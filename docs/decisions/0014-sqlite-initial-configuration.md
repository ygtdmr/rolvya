# ADR 0014 — Initial SQLite configuration

Status: **Approved**
Date: 2026-10-22

## Decision

For Rolvya's single authoritative SQLite database accessed using SQLx, start with:

- **WAL journal mode** for concurrent readers and writer.
- **Maximum 4 database connections** in the control-plane pool as an initial tunable configuration, not an assumed throughput optimum.
- **5-second SQLite busy timeout** on connections as an initial tunable configuration; distinct from HTTP request timeout and overall retry policy.
- **Short atomic transactions**, particularly for attempt admission, lease/fence changes, budget reservations and durable outbox.
- **Regular, recoverable backups to secure remote storage**, with periodic restore verification; backup frequency, retention, encryption and recovery objectives remain open implementation parameters.

These defaults were explicitly approved by the project owner on 2026-10-22. They are provisional operational settings subject to tests, not measured optimums or guarantees.

## Safety and durability requirements

- SQLite's one-writer-at-a-time constraint applies despite WAL and four connections. Do not hold write transactions while making cloud/API requests or running agents.
- Use SQLite-aware online backup mechanisms or supported consistent snapshot processes; **do not simply copy a live database file without accounting for WAL/checkpoint state**.
- Determine and test `synchronous` policy, WAL checkpoint frequency, `foreign_keys=ON`, storage durability, migration/backup consistency, corruption recovery and disk-space exhaustion. Do not silently choose unsafe durability settings for budget ledger data.
- Persist authoritative lease/fencing and financial records. Performance optimizations must not omit correctness-critical writes.
- Benchmark 100, 500 and 1,000 simulated remote workers on 2–4 GB control-plane hardware and test connection contention, busy-timeout handling, crash recovery, budget invariants, and successful restores.

## Open implementation parameters

Backup cadence/retention, RPO/RTO, encryption/key management, synchronous and checkpoint policy, HTTP timeouts and pool tuning based on measured load remain open; changing the architecture or owner-approved spending rules requires separate approval.

Related: [SQLx choice](0013-sqlite-access-library.md), [SQLite choice](0005-authoritative-database.md), [Rust + SQLite architecture](../architecture/rust-sqlite-backend.md).
