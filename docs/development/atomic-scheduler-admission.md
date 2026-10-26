# R06 atomic admission, lease fencing and dispatch intent

Status: **Implemented Rust/SQLite library; synthetic verification only.** Based on main `7922f32278a84e3d711e45a41b550977964aaf77`, the confirmed merged [PR #15](https://github.com/ygtdmr/rolvya/pull/15). This extends the existing plan store and financial ledger without new dependencies, services, workflow permissions or architecture decisions.

Authority: [ADR 0031](../decisions/0031-atomic-task-budget-admission.md), [hard budget ADR 0002](../decisions/0002-hard-budget-approval.md), [bounded-or-stop ADR 0010](../decisions/0010-inflight-budget.md), [group-only ADR 0009](../decisions/0009-parent-aggregation.md), [worker timing policy](../architecture/worker-authentication-policy.md) and [engineering defaults](../project/implementation-defaults-and-release-gates.md). Existing foundations: [plan persistence](sqlite-plan-activation.md), [financial ledger](financial-ledger.md) and [task packages](initial-implementation-task-packages.md).

## Authoritative transaction

`rolvya_persistence_sqlite::admission` adds scoped APIs on the existing `PlanStore`. `admit` performs full immutable-plan/canonical/normalized-graph validation **before** acquiring the writer, then starts `BEGIN IMMEDIATE`. Frozen structure guards preserve that validated snapshot. Inside the transaction it rechecks the active activation receipt, fingerprint and exact static task definition, current runtime eligibility, executable-leaf kind, accepted explicit predecessors, expected fencing epoch, conflicting attempts, current authorization/assignment and unexpired capability.

Dependency acceptance is a durable, fingerprint-scoped record joined to authoritative succeeded state; setting a raw runtime flag cannot satisfy it. Indexed SQL checks cover all explicit predecessors and group children, avoiding a separate query per predecessor/child or a whole-graph traversal under the writer lock. Hierarchy adds no hidden execution dependencies.

The existing ledger rules are reused through a **crate-private transaction helper that cannot commit**. The public standalone financial API still wraps the same logic in its own transaction. Admission rechecks the approved currency/ceiling, settled/held/provisional exposure, unknown liabilities and proposed conservative bound inside its writer transaction. A standalone previously booked hold cannot be adopted as new admission. No forecast or percentage buffer grants headroom.

The transaction atomically writes:

1. Four-category ledger reservation and its idempotency identity.
2. Admission audit event, immutable existing-task attempt and one-use capability identity.
3. Separate attempt state and incremented task fencing epoch/current attempt.
4. Running runtime state and durable outbox intent.

All writes check affected rows and complete receipt/financial application. Ignored statements, constraint failures, stale data, insufficient budget, unknown exposure, capability/authorization revocation or expiry before commit roll back together. The result is returned only after commit. The module invokes no network/provider operation and accepts no launch callback. Existing WAL, FULL synchronization, four-connection limit, foreign keys, recursive triggers and five-second busy wait remain unchanged; no automatic busy retry is added.

## Interfaces and trust boundary

- `admit`: immutable task/fingerprint, expected epoch, scoped request/actor and candidate `ExecutionPermit`; returns the original committed receipt for an identical authorized replay. Replays never create another attempt, extend the lease or authorize a launch. Changed payload/action under a key fails closed.
- `pending_dispatches`: bounded project-scoped keyset metadata, 1–100 rows. It may show an expired pending operation; metadata is not launch authority.
- `claim_dispatch`: rechecks current fence, lease, immutable assignment, authorization, capability deadline and category-specific remaining reservation coverage. It permits already-reserved bounded dispatch at zero *new* headroom, but rejects unknown/over-ceiling exposure or released/insufficient holds. Exactly one claim commits; only then is a non-Clone/non-Deserialize `DispatchPermit` returned. It carries the original assignment, target, lease deadline and **(project, attempt)** operation key. A repeated claim returns `AlreadyClaimed`, including after a lost acknowledgement. There is no blind automatic reissue.
- `accept_task`: trusted independent validation boundary, not a worker result endpoint. An executable leaf requires a current lease and committed dispatch claim; its executor cannot validate its own result even under a permissive policy. A group has no attempt and requires accepted children plus separately verified group acceptance. Success/evidence commit together and remain immutable. Static plan content/hash is unchanged.
- `fence_attempt`: verifies the exact current task/attempt/epoch, advances the epoch and revokes current authority, even after lease expiry. It preserves financial holds and completed siblings. Fencing is not provider termination or a release of liability.
- `reconcile_fenced`: stores trusted external/Git reconciliation and classified transient retry eligibility. A new admission separately requires final financial reconciliation, a fresh capability, expected current epoch, no unknown liabilities, budget and all other gates. Engineering defaults enforce three total attempts, 10/30-second minimum retry delays and stable 0–2-second jitter from the old epoch. Counters/delays survive restart; no autonomous retry loop is implemented.
- `check_lease`: scoped current-fence/expiry metadata validation for an already authenticated controller boundary. It does not authenticate a worker, grant secret access or authorize spending.

`SchedulerPolicy` is mandatory and has no permissive production implementation. It extends the existing assignment policy and receives the complete scoped action. Checks run synchronously inside the write transaction and again before commit. A real implementation must verify authenticated actor/session/roles, exact evidence/project/task binding, current entitlement, provider/account/pool/capacity/isolation constraints, conservative enforceable full cost bounds including minimums/retries, revocation and independent acceptance/reconciliation. Policy code must remain bounded and perform no I/O or external side effects; collect evidence beforehand and reject stale/uncertain evidence. Unique one-use capability identities prevent reuse across attempts/projects, but do not implement a global worker-capacity allocator.

Permit IDs, actor strings, hashes and timestamps are **not authentication, human approval or provider-limit proof**. Synthetic tests use explicitly named scoped policies/fake clocks and no actual credentials. Real authentication, worker enrollment/tokens, independent artifact verification, external capacity and provider billing evidence remain integration work. `SystemClock` reads controller UTC; operational time must be trustworthy and nondecreasing. Times before issuance and regressions during an operation reject. Approved initial leases last at most 90 seconds, also capped by capability expiry. The 30-second heartbeat and 15-minute access-token flows are not implemented here.

## Durable schema and recovery

Forward migration `0003_scheduler_admission.sql` adds scheduler audit events, immutable attempts, separate attempt states/fences, append-only acceptance/recovery records, outbox intents and claims. Scoped foreign keys bind tasks, attempts, financial events and audit. SQL guards reject active group attempts, conflicting live attempts, invalid fence/state transitions, history mutation/replacement and regression of accepted work. Ordinary DML protection assumes trusted schema/OS custody; an administrator able to drop triggers/rewrite files is outside that guarantee. This is not cryptographically anchored audit history.

Migration tests replay all three versions, upgrade a real v2 financial database without changing balances, inject a failing v3 and verify rollback before valid repair, and retain existing checksum/unknown-schema/snapshot tests. No published migration is changed. Verify a consistent backup before production migration; no down/reset path or production migration was performed.

Crash tests use actual admission API calls. A test policy terminates a subprocess at precommit after all writes; SQLite recovery leaves no attempt/reservation/outbox. Another subprocess exits immediately after commit; reopen yields one persistent intent and the same receipt. A third commits a dispatch claim, writes a deterministic fake provider effect keyed by project/attempt, then exits without acknowledgement. The parent looks up that fake effect and proves a second claim cannot launch it again. This demonstrates safe failure/reconciliation boundaries, **not** a real provider lookup implementation or exactly-once third-party billing.

The future dispatcher must revalidate current authority before external use, retain stable project/attempt operation identity, and reconcile any ambiguous launch before replay. Fenced attempts cannot authorize new results or release newer attempts' reservations. Real worker cancellation, heartbeat/renewal, bootstrapping, message sequencing, provider retry idempotency and full outbox recovery are R07/R08 work. No provider can launch through this library alone.

## Executed verification and acceptance coverage

Rust 1.99.1 and SQLite 3.46.0 in Codex Cloud; all amounts/identities/providers are synthetic. **135 workspace Rust tests pass**, including 27 R06 integration tests and eight migration tests. The existing CI replay gate repeats those eight migration tests. No paid API, live worker, infrastructure, production credentials, new issue, merge or protection change is involved.

| Case | Evidence and limits |
| --- | --- |
| T-010 | Separate scheduler pools race against insufficient funds: one complete attempt/reservation/outbox, no loser effects. Duplicate admissions return one receipt. |
| T-011 | Existing exact/negative-money tests; R06 overflow, currency mismatch and invalid epoch/capability inputs reject without authority. |
| T-012 | Busy writer fails after the bounded wait while WAL readers continue; no implicit retry/new intent. |
| T-013 | Old epochs/cross-task/project leases cannot dispatch, validate, fence a newer attempt or release its hold. Real worker token endpoints remain future work. |
| T-014 | Actual pre/post-commit process exit proves rollback or durable intent/receipt recovery. Complete dispatcher reconciliation remains R08. |
| T-015 | Claimed fake launch/lost acknowledgement: fake effect lookup and refusal to reissue claim. Real provider lookup/recovery is not implemented. |
| T-018 | Unknown exposure, including zero reported usage, blocks admission and dispatch claim without changing the ceiling. |
| T-025 | Group stays incomplete with one missing required child; even accepted children require independent group acceptance before a group-dependent leaf admits. Real artifact verification remains a trusted policy obligation. |
| T-026 | Group admission rejects; only fixed executable leaves receive attempts. |
| T-028 | No headroom blocks new work while a separately reserved/covered dispatch may claim. Actual running-provider continuation/stop enforcement is not implemented. |

Fault injection uses `RAISE(ABORT)` and `RAISE(IGNORE)` at every admission write stage, including partial four-category entries, fence/runtime updates and outbox. Tests cover concurrent claims, immutable completed work, delayed reconciliation/backoff, three-attempt limits across restart, precommit revocation/expiry, forged capability/scope and refusal to adopt a preexisting standalone hold. RED→GREEN evidence includes absent outbox behavior and cross-project operation-key collision.

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

Graphify query/path/explain guided investigation; `graphify update .` refreshes Rust relationships. SQL AST extraction is unavailable (`tree_sitter_sql` missing); migration SQL is inspected and executed directly. Generated graph/build/database artifacts remain ignored.

This is a sensitive admission/financial/migration PR requiring Owner manual review. It changes no workflow, permission, protection or routine-source policy. Next dependency-ready coding: **R07 deterministic fake remote-worker control contracts**, then **R08 durable dispatcher/message replay and heartbeat/renewal**, before R09 fake end-to-end orchestration. No external spend is needed for those synthetic milestones.
