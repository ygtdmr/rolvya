---
title: Verification and Testing Strategy
status: Proposed
category: development
last-reviewed: 2026-10-21
tags: [testing, ci, reliability, security]
---

# Verification and testing strategy

## Status and quality gate

**Rust core, plan persistence and financial-ledger library tests are implemented; full application/worker/browser tests remain future work.** A [repository CI and integration foundation](pull-request-integration.md) now provides documentation, secret scanning and merge-policy tests, with explicit activation contracts for future Rust/frontend checks. Its local tests and GitHub activation evidence are distinct from the future product tests below. Test requirements derive from [product acceptance criteria](../product/acceptance-criteria.md), [execution](../architecture/execution.md), [budget](../architecture/budget-costs.md) and [security](../architecture/security.md).

The [initial Rust/SQLite verification matrix](rust-sqlite-test-matrix.md) catalogs detailed future fixtures, concurrency tests and fault-injection points; [engineering task packages](initial-implementation-task-packages.md) assign candidate ownership and dependencies. These are planning artifacts, **not tests that have already run**.

Repository integration regression tests exercise pinned shared-account provenance without external identity proofs, advisory CODEOWNERS, general/CODEOWNER/last-push approval deadlocks including overlapping rules, future source registration against temporary real boundaries/tests, language-job omissions, strict checks, isolated protected smoke scope, forged/stale receipts and security-tree changes. Temporary Rust/frontend fixtures test classification only, not the application. Passing unit tests or historical #7 CI cannot activate routine merging; #7 must be retargeted/refreshed and actually squash-merged by the controller under verified main protection. No test or shared-account action claims independent human authorization.


The [remote integration test matrix](remote-integration-test-matrix.md) defines RI-001–RI-030 (future, not executed) across remote authentication, duplicate worker messages, Git branch protection, provider hidden retries, spending bounds and billing reconciliation. Use [proposed wire](../architecture/worker-wire-contract-draft.md), [remote Git](../architecture/git-remote-integration-contract.md) and [remote worker lifecycle](../operations/remote-worker-lifecycle-contract.md) contracts only under the eventual selected ADRs.

The [conversation coverage audit](../project/two-conversation-coverage-audit.md) links historic user statements and illustrated examples to normative current documents. Its new acceptance cases AC-031–AC-036 test historical mode, scheduler, retry, cost, context and brand requirements; no tests are represented as executed. See [CLI entitlement](../architecture/model-access-entitlement-contract.md), [scheduler selection](../architecture/scheduler-selection-policy.md), [retry](../architecture/retry-escalation-policy.md), [token efficiency](../architecture/token-context-efficiency.md), [cost examples](../architecture/cost-scenario-worked-examples.md).

## Executed Rust foundation boundary

R01/R02 now have executable tests under `crates/domain/tests` and `crates/plan-validator/tests`, plus typed-ID unit/compile-fail tests. [The implementation guide](rust-core-foundation.md) lists commands, deterministic fixtures, limits and the v1 fingerprint contract. T-001–004, PA-01–02 and structural PA-07 are covered here; T-005/PA-05 remain partial until persisted attempts and activation exist. Later transaction, worker, browser and cloud cases below remain specified future checks.

## Executed SQLite plan-store boundary

R03/R05 tests under `crates/persistence-sqlite` cover scoped/per-connection foreign keys, immutable active structure, concurrent/idempotent activation, stale revision rejection, SQL rollback, process-exit recovery, local snapshot restore and migration replay/checksums. [The storage guide](sqlite-plan-activation.md) maps the applicable T/PA cases and distinguishes them from deferred financial admission, attempts/fencing, remote backups and production failure testing. The CI migration replay script executes real disposable-database tests.

## Executed R04 financial boundary

The [financial ledger guide](financial-ledger.md) records 33 financial integration tests, one proof-window unit test and an additional v1/v2 migration test (106 workspace Rust tests; seven migration replay tests also run through the existing CI gate). These prove four-category exact accounting, serialized conservative holds, explicit versioned Owner approvals, one-use proof references, denied authority, idempotent/stale billing, uncertainty and late charges, no double counting, integer overflow rejection, rollback, abrupt-process recovery and local snapshot restore. R04 is separate from R06: no attempts, lease/fencing, dependency admission or dispatch outbox is implemented or claimed tested.

## Test layers

| Layer | Focus | Examples |
| --- | --- | --- |
| Static/specification | Documentation links, frontmatter validity, diagram syntax, ADR and requirement consistency | Broken paths; contradictory approved decisions |
| Unit | DAG validator, task state machine, context packer, cost math, policy authorization | Cycles, invalid transitions, missing context, overflow |
| Property-based | Invariants over arbitrary DAGs and budget sequences | Acyclicity, no new runtime nodes, reservations ≤ approved remaining |
| Database/transaction | Uniqueness, migration, durability, admission serialization | Concurrent leases, simultaneous reservations, recovery |
| Adapter/contract | CLI/API modes, cloud workers, remote Git, provider usage | Fake provider failures, replay, pagination, idempotency |
| Integration | Planner to scheduler to worker to artifact validator and remote Git | Dependencies, safe parallelism, isolated workspaces |
| End-to-end | Project goal to approved execution and auditable result | Happy path, budget pause/resume, failure recovery |
| Security/fault | Secret exfiltration, prompt injection, compromised worker, network outage | Isolation, least privilege, redaction, expiry/fencing |
| Performance | Small-VPS control-plane overhead, scheduling latency, worker throughput | Multi-worker load and endurance benchmarks |

## Required invariants

1. A graph must be fully formed and valid **before** execution; no new task records for an active plan.
2. Predecessors must complete and validate before dependent tasks start; independent tasks may run concurrently.
3. One active leased attempt controls a task; stale attempts cannot overwrite newer outcomes.
4. Costs and reservations must never circumvent the owner-approved ceiling by automatic buffer.
5. Completed work, audit trails and prior budget approvals survive crashes and budget pauses.
6. No personal-computer repository access, Local Agent or Local Sync.
7. No raw secrets in source, prompts, logs, results or recorded fixtures.

## Harness design

Use deterministic fake model/CLI adapters, fake remote Git provider, virtual worker clocks and controllable cloud billing streams. Simulate late usage, duplicated callbacks, interrupted lease renewals and unreliable networks. Reserve live-provider tests for explicit controlled environments with an approved project cost cap. Never rely on a live provider to test denied approvals.

The [remote worker transport design](../architecture/worker-transport-design.md), [GitHub App security design](../architecture/github-app-integration-design.md) and [budget reconciliation](../architecture/budget-reservation-reconciliation.md) add negative tests for provider automatic retries, ambiguous remote write outcomes, protected-branch access and unknown delayed invoices. Such tests are future requirements, **not executed evidence**.

## Example scenarios

- Thousands of randomly generated small DAGs must reject cycles and accept valid dependency order.
- Two concurrent budgets reservations competing for a single remaining allowance must serialize correctly.
- A worker crash just before artifact acknowledgement must not duplicate a Git commit on restart.
- A retry may change model/effort while the task ID and original graph are unchanged.
- Provider costs arriving late must be reconciled; remaining exposure must not be misrepresented as guaranteed safe.
- A malicious README text instructing the worker to reveal secrets must be treated as untrusted content.

## Historical archive migration tests (specified, not run)

[AC-078–AC-108](../product/acceptance-criteria.md) include storage target switches, Owner-only historical migrations, independent source cleanup and **resumable transfer/recovery**. Implement deterministic fake-adapter, Rust and SQLite tests using the [proposed storage migration design](../architecture/storage-migration-state-machine.md): interrupted 1,000-object manifest with 800 independently verified entries, duplicate/stale callbacks, provider upload succeeded but acknowledgement lost, inconsistent source snapshot, missing/corrupt integrity checkpoint, allowlist revocation, budget exhaustion, unbounded in-flight exposure, late provider billing and cancellation.

Assert that previously verified entries are reused only with valid verification evidence; full completion never follows from partial success, and **original sources are never automatically deleted**. Under [ADR 0030](../decisions/0030-project-activity-audit-log.md), test bounded exponential backoff for transient timeout, rate limit/provider cool-down, finite retry exhaustion, persistent wait/retry state across restart, reconciliation before duplicate paid upload, and **no automatic retry** for integrity mismatch, revoked permissions or insufficient authorized budget. Verify that storage retries are distinct from the three-total-attempt default for AI coding workers. Unapproved resumes and new billable work beyond a hard budget limit must be rejected. **The storage-transfer default is 4 total attempts (1 initial + at most 3 automatic retries)**, unlike the separate 3-total AI worker task default. Verify attempt 5 is denied after exhaustion even after restart/replayed callback ([AC-109](../product/acceptance-criteria.md)). **Approved base backoff timing: 5/10/20 seconds** before the three automatic retries, with small bounded jitter and respect for longer provider waits. Add future tests [AC-110–AC-112](../product/acceptance-criteria.md) for cadence, provider delay precedence and crash-safe retry scheduling. Exact jitter amplitude/distribution and per-provider implementation values remain open. These fixtures, tests and CI jobs are **not implemented or executed**.

## Rust + SQLite stack verification (proposed)

The project owner has **approved Rust as the only Rolvya-owned backend language** ([ADR 0004](../decisions/0004-control-plane-language.md)) and **SQLite as the only authoritative database engine** ([ADR 0005](../decisions/0005-authoritative-database.md)). CI/review should reject unauthorized non-Rust backend services or additional authoritative databases. Test SQLite write serialization, FK enforcement on each connection, journal/backup consistency, lease fencing, atomic reservations, disk-full and busy-lock failures. Validate Rust domain/DTO separation, safe error handling, allocation/CPU profile and remote worker isolation. No tests are claimed to have run. See [Rust + SQLite implementation contract](../architecture/rust-sqlite-backend.md) and [SQLite operations](../operations/sqlite-operations.md).

## CI and release evidence (proposed)

CI stages: formatting/lint → unit/property tests → schema/migration tests → contract/integration suites → security scans → controlled end-to-end suites → docs checks. Each stage reports exact command, environment and artifacts. Merge/release gates, supported platforms and performance budgets need decisions before enforcement.

## Reporting and handoff

For every change record tests run/not run, expected vs actual results, failing cases, residual risks, usage and affected requirement IDs. Update [project status](../project/status.md) only on evidence; do not mark a feature implemented merely because tests are documented.
