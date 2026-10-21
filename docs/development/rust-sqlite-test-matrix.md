---
title: Rust SQLite Core Verification Matrix
status: Proposed
category: development
last-reviewed: 2026-10-21
tags: [rust, sqlite, tests, acceptance, safety]
---

# Rust and SQLite core verification matrix

## Status and test evidence policy

**Proposed future test specification only. No Rust tests, CI pipeline, SQLite migration or fault test is represented as executed.** Rust-only backend and SQLite-only authoritative DB are approved decisions. The scenarios below make core invariants testable for [initial work packages](initial-implementation-task-packages.md). Source requirements: [ADR 0001](../decisions/0001-fixed-upfront-dag.md), [ADR 0002](../decisions/0002-hard-budget-approval.md), [ADR 0004](../decisions/0004-control-plane-language.md), [ADR 0005](../decisions/0005-authoritative-database.md), [product acceptance criteria](../product/acceptance-criteria.md).

## Deterministic fixture catalogue

| Fixture | Planned topology and runtime | Expected outcome |
| --- | --- | --- |
| F1: parallel two-branch | A and B independent; C depends on A,B | A/B eligible concurrently; C waits until **both validated** |
| F2: hierarchy vs DAG | Parent P groups A,B; only A → C explicitly | Parent relation never secretly makes B → C |
| F3: dependency cycle | A → B → C → A | Activation rejected, no provider launches |
| F4: missing/outside predecessor | A depends on unknown task or task in another plan | Reject and explain ID/scope |
| F5: parent cycle | P parent of Q, Q parent of P | Reject before activation |
| F6: failed retry | A attempt1 fails, attempt2 is reassigned | Same A task ID and immutable hash, distinct attempt IDs |
| F7: budget race | Project remaining 10 units, two proposed holds of 7 each | At most one atomic admission succeeds |
| F8: late invoice | Old attempt charge arrives after reserve release | Reconcile without double counting; flag unknown risk |
| F9: stale worker | A attempt1 epoch 1 reports success after epoch 2 issued | Reject stale authority; no false task success |
| F10: conflicting Git writes | Independent tasks both propose protected branch write | Block or use declared isolated refs; never silent race |
| F11: unauthorized approval | Worker/fake user submits increased ceiling | Reject; store denial audit only |
| F12: unimplemented group rule | Edge points to group without approved aggregation | Activation blocked, not guessed |
| F13: provider auto-retry | Fake provider secretly launches twice | Detect second execution/spend; no silently approved attempt |

All money and billing amounts in fixtures are synthetic exact integer minor units. Use stable fixture IDs, seeds and content hashes; never include secrets or real user repositories.

## Detailed core test cases

| ID | Layer | Precondition and operation | Pass condition |
| --- | --- | --- | --- |
| T-001 | Unit | Duplicate task ID or conflicting parent ID | Validator rejects deterministically |
| T-002 | Property | Random directed graph with one injected back-edge cycle | Cycle rejected before activation |
| T-003 | Property | Random valid DAG with hierarchy-only parent edges | Topological order uses only explicit dependencies |
| T-004 | Unit | Invalid model/effort/access mode or missing acceptance | Plan cannot activate |
| T-005 | State | Attempt changes execution model after failure | Static plan hash unchanged; retry has new attempt ID |
| T-006 | SQLite | Every pool connection gets invalid FK insert | Rejected; no connection silently disables FKs |
| T-007 | SQLite | Cross-plan dependency/parent FK | Rejected under compound constraints or verified transaction |
| T-008 | SQLite | Draft activation concurrently submitted twice | Exactly one active version under proposed single-active policy |
| T-009 | SQLite | Write task/edge after activation | Denied by authoritative storage guard, including out-of-band write attack simulation |
| T-010 | SQLite | Two reserve+lease transactions use same last budget | At most one accepted; no partial outbox row for loser |
| T-011 | SQLite | Attempt integer overflow / negative authorized charge | Transaction rejects, no provider launch |
| T-012 | SQLite | `SQLITE_BUSY` and bounded retry | No duplicate attempt or reservation; safe timeout/failure |
| T-013 | State | Lease replay from old epoch | No authoritative update or reservation release |
| T-014 | Replay | Crash immediately after durable outbox commit | Restart reconciles launch intent without double-effective job |
| T-015 | Replay | Crash after possible remote success, before acknowledgement | Remote operation looked up; no blind repeated irreversible write |
| T-016 | Budget | UI notification dismissed | Ceiling and blocked admission unchanged |
| T-017 | Budget | Explicit authenticated new ceiling with duplicate request ID | Exactly one durable application |
| T-018 | Budget | Invoice delayed, exposure unknown | No unsafe billable attempt; ledger shows unknown |
| T-019 | Artifact | Successful A result followed by unrelated B failure | A artifact and validation remain after pause/restart |
| T-020 | Security | Worker tries direct SQLite access or protected remote Git branch | Denied by access architecture |
| T-021 | Security | Prompt-injected README requests budget or credential action | Treated as untrusted data, privileged action denied |
| T-022 | Operations | SQLite consistent online backup then isolated restore | FK/integrity checks, graph, approvals and artifacts refs verified |
| T-023 | Performance | Synthetic thousands-node DAG and independent ready queue | Measured CPU/RSS/query p95; **no invented pass threshold** |
| T-024 | Boundary | Rolvya backend has non-Rust service or alternate authoritative DB | Review/build gate rejects forbidden stack |

Tests under a *proposed* group or plan-activation policy should be marked **blocked** until owner approval; they must not be “passed” by silently choosing an implementation.

## Required fault-injection points

For a future SQLite admission, inject crash or I/O error:
1. Before transaction starts.
2. After computing eligible prerequisites, before budget recheck.
3. After write lock obtained and before reservation.
4. After reservation but before attempt/outbox rows.
5. After attempt/outbox writes but before commit.
6. After commit but before cloud launch.
7. After external launch but before provider execution ID persisted.
8. After result receipt but before verification/audit.
9. During backup/checkpoint and when available disk space becomes insufficient.

At every point verify the authoritative SQLite state, current attempt epoch, budget headroom, outstanding holds, outbox operations and remote provider reconciliation. Do not claim exactly-once provider billing: ensure **detect/reconcile** and fail closed when ambiguous.

## Property-based invariants

- Graph node count and active plan hash are stable for all valid state changes and failed attempts.
- No task becomes eligible before every required predecessor is validated under the chosen group policy.
- `settled + held + unreserved_uncertain + new_exposure` must not be admitted beyond the human-approved ceiling; duplicates must not create new authorized exposure.
- Only one current fenced attempt has authority for a given task; stale workers cannot force success.
- Owner-approved ceiling changes are human-authenticated, version-checked and auditable.
- All new paid launch intents have persisted corresponding attempt and funds reservation; no hidden provider auto retry is treated as free.
- Cross-project or untrusted task/model input never grants access to other projects, credentials or protected refs.

## Performance benchmarking protocol

Run synthetic test variants on one small remote test VPS (2 GB and 4 GB configurations were discussed as targets, not proven deployment limits) and compare identical Rust/SQLite workloads. Measure warm/idle memory, peak CPU, SQLite database/WAL disk growth, lock contention, p50/p95/p99 admission time, ready-queue delay and checkpoint behavior; set pass thresholds **after** baseline measurement and owner approval. Profile Rust allocation and SQLite writes separately from expensive user-project compilation executed on remote workers.

## Evidence format for agent handoff

```yaml
test_case: T-010
implementation_commit: pending
rust_toolchain: pending
sqlite_version: pending
remote_test_environment: pending
exact_command: not-run
result: not-run
worker_provider: fake
artifact_hashes: []
observed_budget_exposure: synthetic-only
reviewer: pending
```

This is a **template, not a run report**. For every actual test include command, seed/input, toolchain/OS, observed result, logs/artifact references and reason for skipped/blocked cases. A documented test plan is not evidence of working software.

## Exit criteria for early engineering slices

- R01–R02: domain types and DAG validators implement and **actually pass** T-001–T-005 and relevant property tests.
- R03–R06: SQLite admissions demonstrate T-006–T-012 and T-016–T-018, respecting undecided policy gates.
- R07–R09: fake remote worker and outbox demonstrate T-013–T-015 and T-019.
- R10: recovery/security tests and incident evidence for T-020–T-022.
- R11: performance tests T-023 and stack boundary check T-024, with independent review and actual commit identifiers.

Related: [implementation task packages](initial-implementation-task-packages.md), [testing strategy](testing.md), [SQLite operations](../operations/sqlite-operations.md), [plan activation](../architecture/plan-activation-and-versioning.md), [group policy](../architecture/parent-task-aggregation.md), [budget policy options](../architecture/inflight-cost-policy.md), [project status](../project/status.md).
