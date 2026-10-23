# ADR 0031 — Atomic task admission, lease and budget reservation

Status: **Approved**  
Date: 2026-10-23

## Context

Rolvya must never dispatch chargeable execution based on a stale task-readiness snapshot or a budget calculation performed outside the authoritative SQLite write transaction. The fixed DAG, project Owner's explicit hard budget ceiling, and all previously validated artifacts are invariant. The SQLite backend and SQLx are already approved (ADRs 0005 and 0013), as are initial WAL, four pool connections and five-second busy timeout (ADR 0014).

## Owner-approved decision (2026-10-23)

**Use a short SQLite `BEGIN IMMEDIATE` write transaction as the authoritative admission boundary.** Inside the **same transaction**:

1. Validate project, immutable active plan/version and executable leaf task identity; confirm validated dependencies, task eligibility, required authorization and the absence of a conflicting live attempt.
2. Read and **revalidate inside the transaction** the Owner-approved ceiling, settled/held/provisional financial exposure, unknown liabilities and the conservative upper bound of the proposed billable operation. Reject when headroom or a provider-enforceable bound is insufficient or uncertain.
3. Atomically create the attempt against the **existing task**, advance its lease/fencing epoch, record a conservative budget reservation and its idempotency keys, append required durable audit state and insert the **dispatch intent into the durable SQLite outbox**.
4. Commit. **Only after a successful commit** may the outbox dispatcher issue a billable remote launch; no worker starts on a transaction that fails or rolls back. Failed admission creates no billable intent.

The external cloud/Git/model call **must never occur inside** the SQLite transaction. Dispatch reconciliation after timeout/crash must use stable operation identity; a durable outbox does not guarantee exactly-once third-party billing. Ambiguous launches and charges are reconciled before replay, and the hard budget gate continues to apply. All provider-side hidden retries must be disabled or financially controlled; there is **no automatic ceiling overrun** and no runtime creation of new DAG nodes. Reject stale epochs and duplicate provider responses.

## Scope and implementation boundary

This decision settles **the admission transaction pattern and atomicity contract**, not the complete physical schema, authorization implementation, worker lease duration, provider cost-cap mechanism, currency conversions or SQLx code structure. WAL remains single-writer; keep the transaction short and limit SQLITE_BUSY retries to bounded safe attempts. Durable reservation and outbox records are part of the committed authority; network dispatch is separate and must fail closed on uncertain exposure.

## Required future verification (not yet executed)

- Two concurrent ready attempts competing for insufficient budget: at most one admits and commits.
- Crash before commit leaves no attempt, reservation or dispatch intent; crash after commit preserves one reconcilable dispatch intent.
- Duplicate client requests, replay after worker loss, stale fencing epoch and repeated outbox delivery never silently create extra effective billable attempts.
- Provider reports unknown/late charges: system blocks unsafe admissions without fabricating zero exposure.
- Dependency incomplete, group-only parent task, failed human authorization or revoked provider constraints: reject admission without billable side effects.
- Concurrent readers/writers and SQLITE_BUSY under approved WAL/pool/busy-timeout defaults preserve financial invariants.

**Implementation state:** Approved design only; no compiled Rust transaction, database migration, running CI or live-provider proof is asserted.

Related: [hard budget ADR 0002](0002-hard-budget-approval.md), [in-flight budget ADR 0010](0010-inflight-budget.md), [SQLite configuration ADR 0014](0014-sqlite-initial-configuration.md), [Rust/SQLite contract](../architecture/rust-sqlite-backend.md), [budget reconciliation](../architecture/budget-reservation-reconciliation.md), [test matrix](../development/rust-sqlite-test-matrix.md).
