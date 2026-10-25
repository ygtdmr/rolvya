---
name: rolvya-sqlite-integrity
description: Design, implement, or review Rolvya Rust and SQLite persistence, schema migrations, transactional budget admission, crash recovery, and financial integrity. Use when touching database or spending-authority code; not for generic Rust domain work.
---

# Rolvya SQLite integrity

1. Start with `AGENTS.md`, approved ADRs 0002, 0005, 0010 and applicable newer decisions, `docs/architecture/rust-sqlite-backend.md`, `docs/architecture/sqlite-schema-draft.md`, and `docs/architecture/budget-reservation-reconciliation.md`. Draft SQL is not an automatically authorized migration.
2. Query Graphify for existing persistence and caller relationships, then inspect real SQL/Rust source. Do not infer implementation from documentation. Refresh Graphify after edits.
3. Keep SQLite as the sole authoritative state store, accessed only by the trusted Rust backend. Enforce foreign keys for every connection; examine WAL, busy behavior and bounded connection use. Do not use a network-filesystem shared writer.
4. Implement budget/attempt admission as one atomic SQLite transaction: prerequisite check, authorized ceiling, pessimistic exposure/reservation, attempt and fencing epoch, audit, and dispatch outbox. Never start a billable job before commit, and never hold a write transaction across external network awaits.
5. Keep a durable append-only financial/audit trail with distinct AI API, compute, storage and transfer categories. Fail closed when available budget or external billability is uncertain. No automatic overrun, and only an explicitly authenticated human can raise a ceiling.
6. Treat secrets as protected data: honor approved encrypted SQLite secret-broker and off-DB master-key contracts. Never log or write plaintext key material, regenerate a missing master key during restart, or invent an unauthorized recovery mechanism.
7. Before a migration, verify the reviewed schema and CI replay gate. Test fresh creation, rollback/preflight, conflicting writes, process interruption, foreign-key violations, duplicate callbacks, fencing/replay and late charges with isolated disposable databases.
8. Deliver a concise list of invariants covered, exact repeatable test commands/results, rollback/recovery limitations, and any unverified security assumptions. Never call an untested migration production-safe.

Relevant contracts: `docs/architecture/sqlite-operations.md` if present, `docs/operations/sqlite-operations.md`, `docs/development/rust-sqlite-test-matrix.md`, and ADRs 0044–0047 for protected secret records.
