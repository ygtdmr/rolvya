# ADR 0013 — SQLite access library

- Status: **Approved**
- Date: 2026-10-22

## Decision
Rolvya's Rust control plane will use **SQLx** to access its single authoritative **SQLite** database. The user approved SQLx on 2026-10-22. This builds on the approved Rust, SQLite and Axum choices.

## Rationale
SQLx integrates with the async Rust control plane and provides pooled connections and transactional APIs. Compared with rusqlite it reduces the need to build and operate a custom thread bridge. This does not establish that SQLx is faster or uses less RAM.

## Constraints
- Workers never directly access the SQLite database.
- Budget authorization, attempt reservation and launch outbox intents must commit atomically; use short transactions and reconcile external side effects.
- SQLite has one writer at a time. Bound connections and write contention, rather than assuming more pooled connections improve throughput.
- Keep lease/fencing records durable; heartbeat optimizations must preserve correctness.
- Test under 2–4 GB controller hardware, including 100, 500 and 1,000 simulated workers. Measure p95/p99 latency, CPU/RAM, transaction contention, errors and budget invariants.

## Remaining engineering choices
**Subsequent ADR 0014 approved WAL, at most four control-plane pool connections and a five-second busy timeout.** Exact `synchronous` and checkpoint policy, migration strategy and restore implementation/validation remain to be finalized. No performance benchmark or production deployment is claimed here.
