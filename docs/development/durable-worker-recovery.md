# R08 durable dispatch, messages and recovery

Status: **Implemented Rust/SQLite synthetic foundation; no live provider or authenticated service.** Based on main `5155acb73afc4b229eb84882840227445fc96e67`, confirmed merged [PR #17](https://github.com/ygtdmr/rolvya/pull/17), with R01–R07 present. Branch: `codex/durable-worker-recovery`.

[R09 synthetic orchestration](synthetic-dag-orchestration.md) now drives these interfaces end to end and persists/verifies artifact evidence. The R08 scope and historical limitations below describe the original slice.

This extends [R06 admission](atomic-scheduler-admission.md), the [R07 protocol/fake worker](fake-worker-contracts.md), [ADR 0031](../decisions/0031-atomic-task-budget-admission.md), [bounded spending ADR 0010](../decisions/0010-inflight-budget.md), [retry defaults](../project/implementation-defaults-and-release-gates.md) and [worker timing policy](../architecture/worker-authentication-policy.md). Approved architecture, frozen DAGs, financial ledger, dependencies and provider rollout are preserved. R09 orchestration is separate.

## Executable interfaces

`rolvya_persistence_sqlite::worker_control` contains:

- `OutboxDispatcher::pending`: indexed outbox keyset scan, at most 100 intents with sequence cursors. Read results are metadata, not launch permission.
- `claim`: full frozen-plan validation followed by a short `BEGIN IMMEDIATE` transaction rechecking current scope, dependencies, authorization, financial coverage and lease. The R06 claim, immutable compact launch command/digest and R08 dispatch record commit together. Existing R06 claims enter reconciliation because they may already have launched.
- `dispatch`: inspect the stable **project + attempt** operation key first. Only a newly claimed, still-authorized operation can commit a launch marker and produce a non-deserializable `LaunchAuthorization`. Adapter calls run outside controller transactions. Concurrent scans cannot steal an in-flight marker, launch again or interpret provider absence as cancellation of an outstanding request.
- `reconcile`: bind typed observations to the original command digest, stable evidence ID and monotonic provider revision. Duplicates are idempotent; changed evidence and conflicting payloads reject; old observations cannot restore newer privileges. Lost acknowledgements inspect prior operations. `Launching` means an effect may be in flight; `Reconcile` means acknowledgement/status is uncertain. Neither allows another launch. Stalled markers remain conservative until lookup/recovery establishes a safe outcome.
- `DurableWorkerController::receive`: bounded R07 decoding and the **same shared R07 lifecycle transitions**, with current peer/capability verifier, immutable scope/assignment, scheduler authorization, fence, lease and financial checks. Message digest/identity/sequence, original reply, session transition, result receipt and audit reference commit together. Identical duplicates return the original permitted receipt only after fresh authority checks; they do not apply progress or renew leases twice. Rejected input consumes no sequence or journal space.
- `received_outcome`: persisted untrusted result metadata. Receipt never calls independent validation or marks a task successful. Existing `accept_task` remains the independent acceptance boundary.
- `expired`, `recover_expired` and `recover_attempt`: bounded recovery passes plus explicit trusted cancellation/risk/revocation fencing. Fence advancement, recovery decision, audit and dispatch block commit together. Each scanned lease is rechecked, so a concurrent eligible heartbeat can prevent premature expiry. No replacement attempt, new node, financial release or assertion of provider termination is made.
- `SyntheticProvider`: provider-neutral `LaunchAdapter` implementation using separate SQLite transactions to simulate durable external idempotency and observations. Configurable launch failure before effect, lost acknowledgement and indeterminate outcome; revision-controlled observations model active/finished/unknown/cancellation-ack states and delayed exposure. This intentionally shares the disposable authoritative test DB; it is **not independent cloud evidence or a real-provider exactly-once guarantee**.

The caller schedules bounded passes and backoff. There is no busy-polling daemon, background task spawning or complete R09 execution loop. An unresolved marker is never automatically reset to claimable. Definitive no-effect/termination observations and separate trusted financial/external reconciliation are required before R06 can authorize another attempt.

For attempts enrolled through [S2-04 worker credentials](verified-worker-bootstrap.md), `WorkerSecurity::receive` wraps the same R08 journal and lifecycle in credential/current-authority validation inside one transaction. Legacy direct peer-policy calls reject enrolled attempts, and the Broker requires separate authority. Historical unenrolled synthetic fixtures remain supported. Production provider/HTTPS adapters remain default-deny.

## Lease and financial authority

Forward migration `0004` adds durable lease projections, dispatch state, sessions, append-only receipt identities/observations/recovery audit and the synthetic-provider stand-in. Migrations `0001`–`0003` remain unchanged. Existing attempts receive their original lease deadlines on upgrade. R06 checks and independent acceptance now read the authoritative renewed deadline; the immutable admission receipt retains its original deadline as historical evidence.

A new, correctly sequenced heartbeat renews atomically to `max(existing_until, min(controller_now + 90 seconds, original_capability_deadline))`. It must arrive and commit before the previous lease expires. Full original four-category hold coverage and current enforceable-bound authorization are rechecked; renewal cannot increase a hold, ceiling or approved exposure. The scheduler verifier must establish that the original bound covers the complete proposed continuation. **A reservation alone is not a provider limit.** Future integrations must fail closed when that evidence cannot be established. Current worker transport identity must remain independently valid; heartbeat renews no token. Defaults remain **900-second access-token lifetime, 30-second heartbeat interval, 90-second lease**.

Heartbeat processing performs indexed exact-attempt/task/dependency checks and does not reload the complete plan. Sessions retain compact execution references and hashes, not full task context. Payloads are at most 8 KiB; serialized internal commands/sessions at most 16 KiB; accepted journals at most 4,096 messages per attempt; scans/compaction at most 100 rows. Receipt identities live in SQLite, not an unbounded process cache. Accepted-message controller-time high water persists across restart and rejects regression.

Unknown exposure is recorded through the existing append-only ledger without releasing holds. Contradictory terminal provider observations block dispatch and record uncertainty. Expiration, disconnect, a stop instruction and a cancellation acknowledgement are **not** termination or final billing evidence. Recovery fences and preserves reservations; late billing remains subject to the existing ledger reconciliation. R06 now also rejects a retry while an R08 operation remains unresolved, even if a permissive synthetic caller clears a hold prematurely. The existing three-total-attempt cap, transient-failure gate, 10/30-second delays and bounded deterministic jitter remain in force across restart. Every new attempt still requires R06 admission and its own reservation.

[R11 independent review](core-verification-handoff.md) corrected two cross-layer gaps: explicit uncertainty now takes precedence over every provider-absence shortcut, and recording uncertainty preserves cumulative provisional costs in every category. New public-interface regressions verify blocked launch/heartbeat/sibling admission and durable restart/replay; no financial policy or published migration changed.

`compact_receipts` is a bounded, separately permissioned body-compaction mechanism: conservatively at least 1,096 days, retired authority, confirmed provider termination/no effect, final financial reconciliation and no project uncertainty are required. `RetentionAuthority` must check legal holds and applicable policy; there is no permissive production implementation. Identity digests, audit references and compaction tombstones remain. Financial/security audit evidence is never erased by this API. Archive export, cryptographic checkpoints and lawful audit deletion remain separate unimplemented gates under [ADR 0030](../decisions/0030-project-activity-audit-log.md).

## Verification evidence

`tests/worker_recovery.rs` adds **39 integration tests**. The existing R07 32 simulation tests continue to exercise the shared transition function. Tests use disposable SQLite, deterministic clocks/seeds and explicitly scoped synthetic verifiers; no production identity evidence or provider limits are fabricated.

Coverage includes:

- T-013: persisted deduplication, changed-content replay, strict sequences, scope/context/worker attacks, expired/revoked authority, stale epochs, heartbeat/fence and result/cancel races.
- T-014/T-015: actual subprocess exits before claim, after durable claim, before synthetic effect, after effect with lost acknowledgement, and after acknowledgement before local update; result-journal exits before/after commit; reopening and repeated scans. Failed/ignored writes roll back; ambiguous effects never trigger a blind second launch.
- T-018/T-028–030: full conservative holds, unknown/late exposure, terminal-provider contradictions, cancellation without termination, clipped renewal, no token renewal, retry delays/caps and unresolved-operation retry denial. Existing financial tests cover late invoice amounts, duplicate billing and ceiling variance; no real billing/cancellation enforcement is claimed.
- T-019: an independently accepted sibling manifest/runtime survives another attempt's expiry/recovery and SQLite reopening. Worker-reported results alone still produce zero acceptances.
- Upgrade v3→v4 with actual R06 attempts/claims/holds, injected migration failure, foreign-key/integrity replay; concurrent independent dispatcher pools, paused-adapter lock release, bounded SQLite contention, precommit revocation/expiry, permissioned retention and no graph/budget mutation.

A bounded debug workload exercises **8 independent workers, 48 durable messages, 22,648 wire bytes**, recreating the controller midway. One measured event-processing run took **180.057 ms** on the managed Linux x86_64 Cloud environment, Rust 1.99.1 / SQLite 3.46.0. This excludes setup and is an observation, not a production throughput, memory or 2–4 GiB deployment guarantee.

Exact verification commands (final results are recorded in the PR):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo check --workspace --all-targets --all-features --locked
python3 .github/scripts/ci.py rust
python3 .github/scripts/ci.py repository
node --test --test-isolation=none .github/scripts/integrate.test.cjs
python3 -m unittest discover -s .github/scripts -p '*_test.py'
bash .github/scripts/check-migrations.sh
```

Also run pinned actionlint, redacted Gitleaks and exact-head GitHub CI. Graphify is queried/explained alongside source and refreshed with `graphify update .`; generated artifacts remain ignored. Its SQL parser dependency is unavailable (`tree_sitter_sql`), so migration evidence comes from actual SQL execution/tests.

## Remaining boundaries

No production HTTPS authentication, token issuance/rotation, provider attestation, real enforceable spend limit, AWS cancellation, billing settlement, OS isolation, live model/CLI/Git execution, cryptographic audit checkpoints or deployment is implemented or verified. `UnimplementedAuthentication` still denies all worker messages. A real adapter needs trustworthy monotonic observations/idempotent operation lookup, enforced immutable scope/epochs/deadlines, bounded retries, authoritative billing and independent termination evidence. Synthetic SQLite idempotency cannot establish those guarantees for a remote provider.

**Zero paid-provider calls or live workers.** No infrastructure, issues, repository permissions, branch protection, merge policy or approved ADR changes. This is sensitive admission/migration/orchestration work for Owner manual review; no self-approval or merge. Next: **R09 synthetic DAG end-to-end orchestration**, dependency-aware parallelism, independently validated results, group aggregation and restart persistence.
