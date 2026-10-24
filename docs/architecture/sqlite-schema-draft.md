---
title: SQLite Schema and Transaction Blueprint
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [sqlite, rust, schema, migrations, transactions]
---

# SQLite schema and transaction blueprint

## Status and authority

**Approved infrastructure choices:** Rolvya backend **Rust only** ([ADR 0004](../decisions/0004-control-plane-language.md)); authoritative database **SQLite only** ([ADR 0005](../decisions/0005-authoritative-database.md)). **This SQL schema is Proposed**, not a migration that has been executed, reviewed or shipped. The table names, identifiers, state enums, `synchronous`/checkpoint configuration, schema/migration details and safe lock/retry implementation are not yet finalized; **WAL, SQLx, four maximum pool connections, five-second busy timeout and `BEGIN IMMEDIATE` budget admission are already approved** (ADRs 0013, 0014, 0031).

## Goals and constraints

The schema stores a fully validated immutable DAG, hierarchy vs dependency edges, per-task model/effort, task contexts, attempts and fencing epochs, remote worker state, audit/outbox, exact-budget reservations and human ceiling approvals. It must preserve completed work across process crashes and budget pauses. Remote workers never open the live SQLite file; all authoritative writes go through a Rust backend transaction boundary.

## Identity, money and time representation

- The samples use opaque `TEXT` identities for readability. UUID/ULID/byte encoding is a separate implementation choice.
- A project has one accounting currency in this draft (e.g. ISO currency string). Multi-currency conversion/exchange timing needs a separate design/approval.
- Money is stored in **integer minor units** with range and overflow checks; never use floating-point for admission. Negative entries are allowed only as separately audited corrections in an append-only cost ledger.
- Times below use ISO-8601 UTC `TEXT` placeholders. Choosing integer microseconds or a monotonic event sequence remains a physical-schema decision; timestamps alone cannot fence stale workers.

## Illustrative core migration (SQL draft, not production)

```sql
-- Enable on EACH Rust-controlled SQLite connection.
PRAGMA foreign_keys = ON;

CREATE TABLE projects (
  project_id TEXT PRIMARY KEY,
  owner_id TEXT NOT NULL,
  name TEXT NOT NULL,
  state TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE plan_versions (
  plan_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(project_id),
  version_no INTEGER NOT NULL CHECK (version_no >= 1),
  graph_hash TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('draft', 'validated', 'active', 'closed')),
  created_at TEXT NOT NULL,
  activated_at TEXT,
  UNIQUE(project_id, version_no),
  UNIQUE(project_id, plan_id)
);

CREATE TABLE tasks (
  task_id TEXT PRIMARY KEY,
  plan_id TEXT NOT NULL REFERENCES plan_versions(plan_id),
  parent_task_id TEXT,
  goal TEXT NOT NULL,
  scope_ref TEXT,
  criteria_ref TEXT NOT NULL,
  model_id TEXT NOT NULL,
  effort TEXT NOT NULL,
  access_mode TEXT NOT NULL,
  state TEXT NOT NULL,
  fence_epoch INTEGER NOT NULL DEFAULT 0 CHECK (fence_epoch >= 0),
  context_hash TEXT NOT NULL,
  UNIQUE(plan_id, task_id),
  FOREIGN KEY (plan_id, parent_task_id)
    REFERENCES tasks(plan_id, task_id)
);

CREATE TABLE task_dependencies (
  plan_id TEXT NOT NULL REFERENCES plan_versions(plan_id),
  predecessor_id TEXT NOT NULL,
  successor_id TEXT NOT NULL,
  PRIMARY KEY (plan_id, predecessor_id, successor_id),
  CHECK (predecessor_id <> successor_id),
  FOREIGN KEY (plan_id, predecessor_id) REFERENCES tasks(plan_id, task_id),
  FOREIGN KEY (plan_id, successor_id) REFERENCES tasks(plan_id, task_id)
);

CREATE TABLE project_budgets (
  project_id TEXT PRIMARY KEY REFERENCES projects(project_id),
  currency TEXT NOT NULL,
  ceiling_minor INTEGER NOT NULL CHECK (ceiling_minor >= 0),
  version_no INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE budget_approvals (
  approval_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(project_id),
  actor_id TEXT NOT NULL,
  old_ceiling_minor INTEGER NOT NULL,
  new_ceiling_minor INTEGER NOT NULL,
  approved_at TEXT NOT NULL,
  request_id TEXT NOT NULL UNIQUE,
  CHECK (old_ceiling_minor >= 0 AND new_ceiling_minor > old_ceiling_minor)
);

CREATE TABLE attempts (
  attempt_id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES tasks(task_id),
  worker_id TEXT NOT NULL,
  lease_epoch INTEGER NOT NULL CHECK (lease_epoch > 0),
  state TEXT NOT NULL,
  lease_until TEXT,
  started_at TEXT,
  ended_at TEXT,
  UNIQUE (task_id, lease_epoch)
);

CREATE TABLE reservations (
  reservation_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(project_id),
  attempt_id TEXT NOT NULL UNIQUE REFERENCES attempts(attempt_id),
  amount_minor INTEGER NOT NULL CHECK (amount_minor >= 0),
  currency TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('held', 'reconciled', 'released')),
  created_at TEXT NOT NULL
);

CREATE TABLE cost_entries (
  entry_id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(project_id),
  attempt_id TEXT REFERENCES attempts(attempt_id),
  category TEXT NOT NULL CHECK (category IN ('ai_api', 'compute', 'storage', 'transfer')),
  amount_minor INTEGER NOT NULL,
  currency TEXT NOT NULL,
  source_operation_id TEXT NOT NULL,
  recorded_at TEXT NOT NULL,
  UNIQUE (project_id, category, source_operation_id)
);

CREATE TABLE dispatch_outbox (
  outbox_id TEXT PRIMARY KEY,
  attempt_id TEXT NOT NULL REFERENCES attempts(attempt_id),
  operation_key TEXT NOT NULL UNIQUE,
  state TEXT NOT NULL,
  created_at TEXT NOT NULL,
  dispatched_at TEXT
);

CREATE INDEX idx_plan_task_states ON tasks(plan_id, state);
CREATE INDEX idx_dependencies_successor ON task_dependencies(plan_id, successor_id);
CREATE INDEX idx_attempts_lease ON attempts(state, lease_until);
CREATE INDEX idx_reservations_exposure ON reservations(project_id, state);
CREATE INDEX idx_cost_entries_project ON cost_entries(project_id, category);
CREATE INDEX idx_dispatch_outbox_pending ON dispatch_outbox(state, created_at);
```

### Important omissions deliberately left for follow-up migrations

This subset does **not** yet implement `workers`, `task_contexts`, `artifacts`, `remote_repositories`, `integration_operations`, `execution_events`, cost forecast scenarios, project isolation checks, time/event sequencing or cryptographic provenance. Those remain mandatory logical entities in [data-model.md](data-model.md); **do not implement only the example DDL and claim a complete Rolvya data model**. Test compound foreign keys and the actual SQL engine behavior with each migration.

## Invariants requiring Rust services and/or additional constraints

The sample CHECK constraints and foreign keys do **not** prove complete policy enforcement. The Rust application and chosen migrations must additionally ensure:

1. Only a whole, validated acyclic graph transitions to `active`; all plan-owned tasks and dependency edges become immutable thereafter.
2. Task hierarchy has no parent cycles and cannot cross plans; dependency cycles require graph traversal validation, not simple FK checks.
3. All project-scoped foreign references agree on project and active plan identity; do not trust worker-provided IDs.
4. At most one current fenced/leased attempt may report authoritative progress per task; prevent conflicting active attempts using a tested conditional constraint or serialized transaction policy.
5. Only explicitly authenticated human approvals may increase a ceiling; preserve approval records and ledger entries immutably.
6. Budget reservation exposure includes already-settled charges, outstanding holds, relevant provider minimum charges and unknown late invoices conservatively.
7. Cost correction cannot delete history; write a separately authorized compensating entry and preserve provenance. Idempotency must avoid conflating distinct provider charges with a repeated callback.
8. The outbox record and attempted lease/reservation must be in **one SQLite write transaction** before starting cloud billing.

## Atomic admission pattern (proposal)

```text
request task T, eligible worker W, conservative exposure E
BEGIN IMMEDIATE                       # candidate transaction mode
  validate active immutable plan hash, predecessor validation, worker and access policy
  verify T is eligible and currently has no authoritative running attempt
  compute spent and outstanding holds by approved project currency
  verify E <= approved_ceiling - spent - held
  increment T's fencing epoch
  create new Attempt(A, epoch), Reservation(A,E), Outbox(A, operation_key)
COMMIT
dispatcher asynchronously launches the remote worker with operation_key
on failure: reconcile provider, fence old epoch, settle reservation, retry SAME task if permitted
```

If a transaction aborts, **no partial lease or cloud dispatch** is permitted. The exact SQL isolation, savepoints, conditional UPDATE, constraints and Rust driver behavior must be validated experimentally. An outbox avoids losing intent after a commit; **it does not guarantee exactly-once provider execution**.

## SQLite-specific concurrency and backup tests

- Run competing reservations in different Rust tasks/SQLite connections, including low remaining budget and integer overflow.
- Simulate WAL checkpoint pressure, long-running readers, `SQLITE_BUSY`, failed commit, low disk and process termination at every transaction stage.
- Verify foreign keys on each new connection and test composite dependencies.
- Restore a consistent SQLite backup, then reconcile Git refs, provider worker state, reserved/settled cost and artifact hashes.
- Ensure a stopped or untrusted remote worker cannot open the authoritative DB file, introduce new DAG nodes or change approval ceilings.
- Confirm actual controller RAM/CPU and SQLite file/WAL footprint under realistic simulated scheduling and billing volumes.

## Pending decisions

DDL completeness, table naming, identity/time format, SQLite `synchronous`/checkpoint tuning (WAL mode already approved), safe retry/backoff, index selection, crate and migration tooling, backup RPO/RTO, final authorization model and the exact in-flight billing limit policy. **Backend language Rust and database engine SQLite are not pending.**

## References

- [SQLite foreign keys](https://www.sqlite.org/foreignkeys.html)
- [SQLite transactions](https://www.sqlite.org/lang_transaction.html)
- [SQLite WAL](https://www.sqlite.org/wal.html)

Related: [approved Rust ADR](../decisions/0004-control-plane-language.md), [approved SQLite ADR](../decisions/0005-authoritative-database.md), [backend design](rust-sqlite-backend.md), [scheduler](scheduler.md), [budget](budget-costs.md), [SQLite operations](../operations/sqlite-operations.md), [test strategy](../development/testing.md).
