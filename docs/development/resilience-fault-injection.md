# R10 resilience and crash-consistency evidence

R10 extends the existing R04–R09 Rust/SQLite tests rather than implementing another scheduler, ledger or provider. PR #19 is confirmed merged at main `9d7ca7430e8a1143d3c72ef475da0ab42c97d3e3`, with the same tree as the verified R09 head. Work starts from that main commit on `codex/resilience-fault-injection`.

## Executable harness

[`tests/resilience`](../../crates/persistence-sqlite/tests/resilience/mod.rs) is compiled only through the persistence library's private `cfg(test)` module. It uses safe, connection-local SQLx update/commit/progress hooks on disposable databases. No public fault API, runtime feature, new dependency or published migration changes are introduced. The existing R09 fixture is shared through [`common/synthetic.rs`](../../crates/persistence-sqlite/tests/common/synthetic.rs); its original 34 test bodies are preserved.

- Typed fault points pause victims at an acknowledged boundary. The parent sends actual SIGKILL and checks signal 9 on Unix, then reopens SQLite. A disposable-directory sentinel/path check limits victims to owned temporary databases. Victim cleanup and readiness deadlines are bounded.
- The independent state oracle checks SQLite/FK integrity, frozen plan fingerprint/node/edge counts, monotonic fences, one outbox and reservation per attempt, no parent attempts, durable logical provider operations and preserved accepted manifests. It sums all four settled/held/provisional categories using `i128` and compares the real ledger projection, uncertainty and headroom.
- Fake clocks, stable operation identities, explicit rendezvous/notification gates and four fixed seeds replace timing sleeps. Every randomized prefix prints its seed/index/action on failure. Serialized actions and prefix truncation support reproducible diagnosis; there is no automatic property-test shrinker.
- Tests use the actual R06 admission, R08 dispatcher/message processor and R09 coordinator/validator. They never manually mark tasks successful or create an alternative state authority.

## New evidence and existing coverage

There are **22 new tests: 21 behavioral regressions plus one inventory consistency test**. The [machine-readable inventory](../../crates/persistence-sqlite/tests/resilience/coverage.json) distinguishes new cases from existing evidence, names executable tests, maps F1–F13 and applicable T-010–T-030, and lists unverified boundaries. Its references and all new behavioral-test entries are checked by a Rust test. Recorded local outcomes do not replace final-head CI or certify production behavior.

| Boundary | New verification |
| --- | --- |
| Admission: before acquisition, eligibility read, write lock, headroom calculation, reservation, attempt, fence, audit, outbox, before/after commit | **11 SIGKILL cases**. Precommit authority is wholly absent; committed attempt/hold/outbox persist and admission replay has no extra effect. Independent WAL readers cannot see staged authority. |
| Dispatch claim, effective synthetic launch before acknowledgement, message-journal write, result commit before acknowledgement | **4 SIGKILL cases**. Original operation/receipt is recovered; no second effective launch. |
| Leaf validation commit, group commit, group commit before acknowledgement | **3 SIGKILL cases**. Uncommitted acceptance does not satisfy dependencies; committed acceptance survives, siblings and artifact bytes remain valid. |
| `VACUUM INTO` after output pages exist; partial passive checkpoint before application acknowledgement | **2 SIGKILL cases**. Source remains valid; partial copy is not certified. A pinned WAL reader forces a bounded busy checkpoint; releasing it permits truncation and consistent reopen. |
| Outbox, result receipt and trusted acceptance inserts | Actual `SQLITE_FULL` (code 13), safely induced with per-connection `max_page_count` and a bounded 4 MiB statement. Prior staged writes roll back. No host filesystem exhaustion. |
| SQLite interruption and commit veto | Actual engine interruption (code 9) and commit-hook rollback; no partial financial/admission authority after reopen. |
| Eligibility read versus a later charge; repeated financial and fencing races | Controlled read/charge rendezvous, eight admission/charge races and eight heartbeat/fence races. Authority is rechecked; stale/revoked replay cannot resurrect it or release holds. |
| Delayed provider lookup | A writer can acquire `BEGIN IMMEDIATE` while lookup waits. Expiry or fencing during the wait prevents unsafe continuation. |
| Bounded adversarial sequences | Four seeds (`1`, `7`, `0x5eed`, `0x9e3779b97f4a7c15`), 48 operations each: pass, clock advance, reopen, duplicate, cancellation, validation, cost report, scoped synthetic approval, worker approval denial, provider observation and revocation. Invariants checked after **192 prefixes**. |
| Hidden provider-native retry and delayed cost | A deliberately faulty adapter records **two durable remote execution IDs** under one approved operation, reports unknown exposure, then reports an idempotent late provisional charge. C remains 10, H remains 7, P becomes 7; new admission is blocked, including after restart. No extra controller attempt is authorized. |
| Backup and artifact attacks | Full G/A/B/C snapshot restores graph, budget, holds, fences, audit, exact message digests/acknowledgements and accepted bytes. Racing billing yields one complete four-category revision. Missing/modified bytes fail independent verification even when physical integrity passes; a damaged header rejects opening without resetting data. |

Existing R04 tests already cover duplicate/unauthorized/stale approvals, exact currency/arithmetic and overflow, duplicate invoices, late charges after reconciliation, incomplete reconciliation and append-only history. R06 already covers competing reservations, statement-by-statement aborts, scope/capability guards, three total attempts and persisted retry delays. R08 already covers contradictory/reordered provider evidence, digest conflicts, durable replay/revocation, concurrent dispatch claims, cancellation without termination and reservation preservation. R09 already covers nested groups, immutable accepted artifacts, invalid input commitments, concurrent acceptance, independent A/B execution, blocked C and reconciled transient retry. These tests remain active; R10 does not copy them to inflate coverage.

## Demonstrated defect and minimal fix

A new test first failed because `SyntheticCoordinator::deliver` counted an ignored delivery-projection update as progress while its event remained `pending`. That can falsely report progress and repeatedly revisit the same projection. The fix checks affected rows and, when zero, reads the saved projection: a still-pending event returns an integrity error. A projection already committed by a racing coordinator is allowed. The durable R08 message journal remains authoritative. Removing the injected fault and replaying completes the same three logical operations without duplicate effects. No schema, budget policy or authorization changes are required.

## Backup/recovery boundary

`PlanStore::snapshot` already supports consistent private SQLite copies. R10 verifies isolated restoration and semantic artifact checks; it does not build a disaster-recovery service. Ordinary `PlanStore::open` cannot distinguish an operator-restored old snapshot from an ordinary controller restart, nor discover later external effects itself.

The stale-restore test therefore **explicitly pauses synthetic admission through revoked authority before resuming**, inspects later provider evidence, and records unknown exposure. Unpausing still leaves unsafe work blocked and preserves accepted A. This exercises the conservative operator procedure from [ADR 0015](../decisions/0015-backup-recovery.md); it is not an automated authenticated restore-activation gate. Such a gate, encrypted off-host backups, recovery kits, external billing reconciliation and operational RPO/RTO remain unimplemented.

## Verification

The workspace now has **270 passing Rust tests**, including the 22 R10 tests and all 34 unchanged R09 orchestration tests. Exact final-head GitHub CI evidence belongs in the PR. Required local commands:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo check --workspace --all-targets --all-features --locked
python3 .github/scripts/ci.py rust
python3 .github/scripts/ci.py repository
node --test --test-isolation=none .github/scripts/integrate.test.cjs
python3 -m unittest discover -s .github/scripts -p '*_test.py'
bash .github/scripts/check-migrations.sh
cargo test -p rolvya-persistence-sqlite --lib resilience --locked -- --nocapture
```

Workflow linting, repository inventory, redacted secret scanning and Graphify query/path/explain/update are also run. Graphify's unavailable SQL parser does not replace direct SQL inspection, replaying all five migrations or executable integrity/transaction tests. Graph/database/build artifacts remain ignored. The first regression's expected red result and initial harness errors were resolved; no failing tests or safety assertions were deleted.

One focused run on Linux x86_64 (3 available CPUs, 8 GiB cgroup memory limit, pinned Rust 1.99.1) finished its 22 tests in **12.00 seconds**; command wall time including compilation was **14.70 seconds**. The pinned-reader checkpoint returned busy after **5.007 seconds**, respecting the configured five-second timeout. Across 53 observations, reopen plus integrity took **9.44–24.50 ms**; one complete restore plus semantic verification took **48.74 ms**. These are synthetic observations, not production throughput, memory or durability guarantees.

An unfiltered workspace secret scan flagged 20 generic-rule matches in ignored Graphify generated prose and content-hash metadata. A clean export of the tracked tree passed with zero findings; the PR commit range is scanned separately. No secret-scan rule or CI exclusion is weakened.

## Evidence limits and next step

All resources are disposable SQLite files and synthetic operations. `SQLITE_FULL` proves engine behavior under a logical size limit, not physical host ENOSPC; SIGKILL/checkpoint tests do not prove storage-hardware or power-loss guarantees. The hidden-retry case requires provider evidence of the extra execution/uncertainty: an adapter cannot detect an undisclosed remote retry without observations. Reservations are accounting authority, never proof of live provider enforcement.

Production authentication/token issuance, worker isolation, HTTPS transport, provider spending limits/cancellation/billing, real Git publication, Secret Broker recovery and off-host disaster recovery remain outside this evidence. No infrastructure, frontend, new issues, permissions/protection changes or paid provider operations. This sensitive PR requires Owner review and must not be self-approved or merged. **Next: R11 independent verification, release-gate evidence and core implementation handoff.**
