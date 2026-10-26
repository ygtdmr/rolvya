# SQLite plan persistence and durable activation

`rolvya-persistence-sqlite` implements the R03/R05 **plan-store boundary** on top of the verified [Rust foundation](rust-core-foundation.md). It uses approved SQLx/SQLite, WAL, at most four pooled connections and a five-second busy timeout ([ADRs 0013–0014](../decisions/0014-sqlite-initial-configuration.md)). No scheduler, billable admission, authentication service or worker is supplied.

## Public boundary

`PlanStore::open` creates or reopens one controller-owned local database. Reuse one store (clones share its pool); opening separate stores creates separate pools and is used in tests to model competing clients. The default synchronization policy is **FULL**, with SQLite's 1,000-page automatic WAL checkpoint. These are conservative engineering defaults, not measured production settings. SQLite still has one writer. A separate six-second pool-acquisition timeout bounds connection starvation; it does not replace the five-second SQLite busy timeout. There are no automatic blind retries.

On Unix, new DB/snapshot files are mode 0600; a final-component symlink, nonregular file or existing group/world-accessible DB is rejected. The configured directory and ancestor paths must be trusted, private local storage; this is not a defense against a hostile local administrator or filesystem replacement race. Network filesystem detection, OS ownership/ACL provisioning, secure hosting and backup destinations are operator responsibilities. Remote workers never receive this handle or open the DB.

Project creation is a trusted bootstrap-management call. `PrincipalId` and `RequestId` are distinct, bounded, canonical identifiers. No caller-supplied principal string authenticates a human. `PlanAuthorization` is mandatory for saving/activating and must evaluate previously authenticated, action-specific consent without network I/O inside the transaction. The current persisted Owner must match the actor; this is rechecked under the write transaction. Authentication, fresh Owner proof, roles and ownership transfer remain later security integration work. Read methods similarly belong behind the trusted application's authorization boundary.

- `save_draft`: accepts an R02 `ValidatedPlan` and compares the expected current draft revision under `BEGIN IMMEDIATE`. Each version is an append-only snapshot; edits require a new version. Exact duplicate saves are read-only/idempotent. Competing stale writers cannot replace the latest pointer. Saving a new draft after activation is blocked; supersession/closing is not implemented.
- `load`: reads a transactionally consistent plan snapshot, revalidates v1 canonical bytes and fingerprint with the supplied assignment policy/limits, and checks every normalized task and dependency against that payload.
- `load_active`: discovers the project's durable active version after restart without remembering its original key.
- `activate`: compares current draft key/hash, revalidates the stored complete graph and current assignment policy, and atomically writes initial runtime rows, an append-only activation receipt and active state. Exactly one plan per project may be active. No remote operation occurs.

SQLite stores plan versions as positive signed 64-bit integers. Larger domain `u64` versions fail with `VersionOutOfRange`, without truncation. Canonical payloads are limited to 64 MiB in both Rust and SQL; callers must additionally use appropriate R02 validation budgets and bound upstream decoding. Capability and authorization policies must be bounded and side-effect-free. Full graph verification takes place inside activation's transaction to prevent stale graph/role decisions; large-plan activation contention still needs production measurements.

## Storage and immutable activation

The first migration contains projects, plan versions, normalized tasks/dependencies, separate task runtime and activation events. It deliberately does **not** copy the draft schema's budget, approval, reservation, attempt, worker, secret or outbox examples. Financial/attempt uniqueness and transactional admission belong to R04/R06, not placeholder tables in this milestone.

Composite foreign keys include project, plan, version and task scope. Parent FKs are deferred to support arbitrarily ordered nested task insertion. Foreign keys, recursive triggers, WAL and FULL synchronization are configured on **every** pooled connection and tested across all four. Recursive triggers are important: ordinary SQLite `INSERT OR REPLACE` must not bypass frozen-row delete guards.

After activation, SQL triggers block insert/update/delete/replacement of the plan, static tasks and dependency edges, and preserve runtime identities and the activation audit. Runtime status is stored separately and may change without changing the static fingerprint. No public arbitrary runtime-write/lease/dispatch API is exposed. Future scheduler methods must enforce task-kind, acceptance, fencing and lifecycle authorization; these storage rows alone do not prove group success or legitimate execution.

Activation request identities are scoped per project and bound to the exact revision, fingerprint and actor. A matching replay returns the same durable receipt only after checking that the corresponding active state and complete runtime initialization exist. Reusing an ID for different content fails. Replays still require current Owner authorization, but do not repeat assignment eligibility checks or create a new activation; revoked execution eligibility must be enforced again at future admission. Receipts are ordered local audit metadata, not cryptographically anchored audit proofs or dispatch intents.

Protection assumes the trusted SQLite schema/connection settings and OS custody remain intact. It is tested against ordinary out-of-band DML, including replacements; a database administrator able to drop triggers or rewrite the database is outside that guarantee. Loads fail on canonical/hash/normalized-graph inconsistency. This milestone does not implement authenticated tamper-evident backups.

## Migrations, snapshots and recovery

Migrations are embedded at compile time, versioned and checksum-verified by SQLx. The build script tracks the migration directory. Startup recognizes schema/migration metadata **read-only before changing journal settings**, refuses unrecognized databases or unknown/changed/failed revisions, then applies pending migrations and checks SQLite integrity/foreign keys. Never edit a published migration; add a reviewed forward migration. No destructive down/reset operation is provided. Existing deployments need a verified backup, compatible-binary rollout and an explicit forward-fix/restore plan before migration. No production database was migrated in this task.

`PlanStore::snapshot` uses SQLite's supported `VACUUM INTO` consistent snapshot, rather than copying a live WAL database. The destination must not exist and is created privately. Failure leaves the destination for diagnosis; do not treat it as a valid backup. Snapshot restoration is tested by reopening a separate file, replaying migrations/checks and recovering the same active hash and receipt. This is a **local consistency foundation**, not the approved encrypted/off-host scheduled backup service, retention policy or measured RPO/RTO. Real disk-full, abrupt host/power loss and storage hardware guarantees remain unverified.

Fault tests abort a statement after runtime insertion and verify full rollback, then retry successfully. A subprocess exits without transaction cleanup before commit; another exits immediately after the real activation commit to model a lost acknowledgement. Reopening demonstrates rollback or recovery of the same receipt. These tests cover process failure, not a power cut or all SQLite I/O failure modes. Busy-writer tests retain WAL readers and deny the write after the configured wait without creating activation effects.

## Verification and acceptance

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo check --workspace --all-targets --all-features --locked
bash .github/scripts/check-migrations.sh
python3 .github/scripts/ci.py inventory
python3 .github/scripts/ci.py repository
python3 .github/scripts/ci.py rust
node --test --test-isolation=none .github/scripts/integrate.test.cjs
python3 -m unittest discover -s .github/scripts -p '*_test.py'
```

The migration replay gate is required by the existing CI once `.sql` files appear; the new script runs actual migration replay, failure rollback, checksum/version rejection and snapshot restore tests. No workflow permissions, protection rules or automatic-source classifications change. The required new `.github` migration gate changes the security tree; any older automatic-merge smoke receipt must be refreshed through the existing runbook after Owner review, without changing live mode or bypassing gates. SQLx supports SQLite only in the compiled feature graph; optional other-driver metadata in Cargo.lock is not an alternate backend.

Graphify refreshed Rust relationships; SQL AST extraction was unavailable because `tree_sitter_sql` is not installed. Migration SQL was inspected and executed directly in the tests above.

Evidence covers T-006/T-007 (per-connection/scoped FKs), T-008 (single active plan/concurrent activation), T-009 and PA-06 (static write rejection), PA-03/04 (idempotency/stale revision), the stored-runtime portion of PA-05, bounded contention, atomic statement rollback and the local consistency portion of T-022. Complete ledger/attempt recovery, worker fencing, financial audit reconciliation and remote encrypted restore remain future requirements.

Next is R04/R06: exact financial ledger/approvals and hard-budget admission, with reservation, attempt/fence, audit and dispatch outbox committed together under the approved `BEGIN IMMEDIATE` contract. No spend or dispatch permission follows merely from an active plan.
