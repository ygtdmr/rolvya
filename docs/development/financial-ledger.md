# R04 financial ledger implementation and verification

Status: **Implemented library foundation; no scheduler admission, live billing or production deployment.** Based on main `07f8aefae1fe089d9359c7f65379220a7d633189` (merged [PR #14](https://github.com/ygtdmr/rolvya/pull/14)). R04 is delivered separately so spending authority and reconciliation can be reviewed before R06 adds attempts, leases, fencing and dispatch.

Binding policy: [hard budget ADR 0002](../decisions/0002-hard-budget-approval.md), [bounded-or-stop ADR 0010](../decisions/0010-inflight-budget.md), [atomic admission ADR 0031](../decisions/0031-atomic-task-budget-admission.md) and [exact-action Owner proof ADR 0034](../decisions/0034-owner-action-bound-reauthentication.md). See [accounting contract](../architecture/budget-reservation-reconciliation.md), [approval workflow](../architecture/approval-workflow.md), [engineering defaults](../project/implementation-defaults-and-release-gates.md) and [task packages](initial-implementation-task-packages.md). No approved architecture decision changes.

## Implemented boundary

`rolvya_persistence_sqlite::finance` extends the existing `PlanStore` and its private SQLx pool. It reuses domain `Money`, `Currency`, `CostBreakdown`, project IDs, principal IDs and request IDs. No crate, dependency, network client, authentication service, new provider or alternate database is introduced. Connection limits, WAL, FULL synchronization, foreign keys, recursive triggers, bounded busy wait and sanitized storage errors remain unchanged.

The API exposes:

- `apply_financial(request, authorization)`: serialized `BEGIN IMMEDIATE` mutation and durable idempotent receipt. A mandatory synchronous `FinancialAuthorization: Send + Sync` checks the entire action after obtaining the writer lock and again before commit. It must use authenticated identities and actual evidence; no permissive production policy is provided.
- `financial_snapshot(project)`: consistent approved ceiling/version, four-category settled/held/provisional amounts, unknown-liability count, headroom and over-ceiling status.
- `financial_liability(project, liability)`: current expected revision, category balances, uncertainty and finalization.
- `financial_history(project, after_sequence, limit)`: bounded keyset pagination (1–100 events), preserving actor, timestamp, request, rationale and non-secret evidence references in versioned JSON.

Reads are trusted control-plane library calls, like existing plan reads. A future API must enforce project read permission; possession of a project ID is not authorization.

## Approvals and accounting invariants

An `Approve` action initializes or explicitly increases the **total** approved ceiling. It requires the current stored project Owner, a matching expected budget version, a non-secret exact-action proof reference and creation timestamp. The trusted policy must verify explicit human confirmation, fresh reauthentication, action/project/amount/currency/session binding, revocation and risk controls. The library checks the approved maximum five-minute window at application and immediately before commit; a proof at 300 seconds, from the future or with negative time is rejected. Proof references are consumed atomically and globally single-use. Identical already-committed deliveries may return their original receipt after current authorization; they never authorize another effect.

A principal string, stored proof reference, GitHub account identity, notification acknowledgement or permissive callback is **not** human authentication. A non-Owner cannot approve even if a caller policy returns true. Real login, session/step-up verification, invitations, multiple-Owner membership and notification delivery remain later security/application integration. The current project store has one Owner; no independent human approval is claimed by synthetic tests.

The initial currency is supplied by the explicitly approved ceiling and becomes immutable for that project. All values are nonnegative exact signed-64-bit integer minor units. There is no default accounting currency, floating-point admission arithmetic, FX conversion, tax policy, percentage overrun or automatic ceiling increase. Existing three-scenario forecasts remain separate domain data and never grant spend.

Every liability revision contains exactly four normalized positions: `ai_api`, `compute`, `storage`, `transfer`. Their **cumulative balances**, rather than additive copies of invoices, form an append-only ledger; current project exposure sums only the latest revision of each liability:

```text
headroom = approved ceiling - settled - held - additional unreserved provisional exposure
```

Negative headroom is shown as zero with `over_ceiling=true`. Checked Rust sums and SQLite integer aggregation reject overflow; no wrapped or floating-point value can authorize a hold. Individual reported charges still persist if their project aggregate exceeds integer capacity; snapshot/admission then fails closed, while paginated history and individual positions remain available for diagnosis.

## Liability transitions and idempotency

- `Reserve`: records a positive four-category financial hold after checking current approved headroom and **all** unknown liabilities inside the writer transaction. The policy must independently verify the referenced enforceable maximum, minimum charges, remaining duration, hidden retries and evidence freshness. A forecast or timeout target alone is insufficient. Liability identity cannot be reused to reset a hold.
- `Report`: records cumulative **additional unreserved** exposure or uncertainty, including a newly discovered external liability. Costs already covered by a hold must not be reported again here. Reports cannot reduce provisional exposure, release holds, clear uncertainty or authorize spending. A delayed zero-usage report leaves holds intact. New reporting requires an existing approved-currency account; missing account/budget fails explicitly, never implies zero-cost permission.
- `Reconcile`: requires the exact current liability revision and authoritative evidence for cumulative settled charges and remaining held/provisional balances. It consumes covered exposure without double counting, cannot increase a hold or provisional balance, and cannot leave settled money available in the same hold. Newly discovered additional unreserved exposure uses a separate `Report`. `Final` requires zero remaining hold/provisional liability. Timeouts, worker completion and cancellation acknowledgements do not automatically release money.
- Unexpected charges, including a late invoice after finalization, are recorded even above the approved ceiling. They force uncertainty and require a **separate fresh** reconciliation before further holds. Confirming those charges never raises the ceiling; over-ceiling exposure continues to block holds. R04 reports risk, but has no worker privileges to fence or provider cancellation path.

Requests are bound to the exact project, actor, command, amounts, expected version, evidence references and rationale. Repeated identical keys return one receipt; changed content is rejected. Billing evidence identities are also unique per project so changing only a client request key cannot reapply a provider event. Stale/different concurrent invoice revisions conflict rather than overwrite newer state. A successful replay verifies the full archived approval or four-position liability application, not merely an event header.

Refund/negative-correction semantics are not selected here: cumulative settled amounts cannot decrease. The existing pure `Correction` type does not authorize ledger refunds. Genuine refunds/FX need a separately reviewed accounting contract; they are rejected rather than silently netted against spend.

## Storage, recovery and trust

Forward migration `0002_financial_ledger.sql` adds immutable accounting currency, ordered approvals, event headers, liability revisions and exact category positions. SQL constraints enforce project-scoped relationships, unique requests/proofs/billing events, sequential revisions, nonnegative balances and valid categories. Triggers reject update, delete and `INSERT OR REPLACE` against financial history under the existing recursive-trigger setting. Each API mutation verifies affected rows and its durable application before commit; failures roll back account initialization, proof consumption, events and balances together.

This protects ordinary DML and API invariants under trusted OS/schema custody. It is not tamper-proof against an administrator who can drop triggers or replace the DB. Cryptographic audit checkpoints, retention/archive and separately encrypted off-host backups remain unimplemented. Take and verify a consistent backup before any production migration; migrations are forward-only with checksum checks, no financial reset/down operation. Tests upgrade a real v1 DB and prove a failed v2 rolls back before a valid upgrade. No production DB was used.

Crash tests exit subprocesses without cleanup during uncommitted financial writes and immediately after a real API commit. Restart/replay observes rollback or the same durable receipt. Statement injection uses both `RAISE(ABORT)` and zero-row `RAISE(IGNORE)` between writes. Busy-writer tests preserve WAL readers and deny writes after the bounded wait. These are process/SQL failure tests; actual power cuts, disk exhaustion, hostile host access, hardware durability and billing-provider behavior remain unverified.

## Verification and review scope

Repeatable commands (Rust toolchain 1.99.1, SQLite 3.46.0 in the Cloud test environment):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo check --workspace --all-targets --all-features --locked
python3 .github/scripts/ci.py rust
python3 .github/scripts/ci.py inventory
python3 .github/scripts/ci.py repository
node --test --test-isolation=none .github/scripts/integrate.test.cjs
python3 -m unittest discover -s .github/scripts -p '*_test.py'
```

The R04 changes add 34 financial integration tests, one exact-proof-window unit test and one v1/v2 migration test: **107 workspace Rust tests total**, with seven migration tests repeated by the existing CI replay gate. Cases include simultaneous holds (32 independently spawned requests against one ceiling), duplicate/stale approvals and invoices, worker/unauthenticated/revoked authority, cross-project/currency isolation, unknown/late/over-ceiling/overflow exposure, illicit release/refund/hold transitions, raw SQL mutation/replacement, ignored writes, precommit policy revocation, abrupt process exit, bounded busy failure and snapshot restore. Red-to-green observations include missing-budget denial, incomplete-receipt false success retaining already-settled money in a hold and reopening uncertainty after finalization even with zero reported usage. Applicable coverage is the **ledger portion** of T-010/011/012/017/018/022/030/035; no complete attempt/admission/lease/outbox acceptance is claimed.

Graphify query/path/explain guided investigation and `graphify update .` refreshes Rust relationships. SQL AST extraction remains unavailable (`tree_sitter_sql` missing); both migrations are inspected and executed directly. Generated graph, build and DB artifacts remain ignored.

This is a **sensitive financial-authority and migration PR** for the Owner's manual review. No workflow, protection, permission, source allowlist or routine merging setting changes. No issues, approvals, merges, external credentials, paid provider calls or infrastructure are created.

## Next coding milestone: R06

Reuse these financial rules **inside the same** `BEGIN IMMEDIATE` transaction as active immutable-plan/leaf eligibility, independently verified dependency acceptance, authorization, attempt creation, lease/fencing epoch, audit and durable dispatch outbox. Do not call `apply_financial` and then launch or create an attempt in a second transaction: a standalone financial receipt is **not** an admission token. No external dispatch may occur until that complete transaction commits. R06 must add stale-lease rejection, crash/fault tests across each admission stage and reconcilable stable launch identity; R07/R08 then add deterministic fake worker and dispatch replay. No external spend is needed for those synthetic milestones.
