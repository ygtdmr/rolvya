---
title: Verification and Testing Strategy
status: Proposed
category: development
last-reviewed: 2026-10-21
tags: [testing, ci, reliability, security]
---

# Verification and testing strategy

## Status and quality gate

**Not implemented:** no claim is made that the CI pipeline or tests described here currently exist. Test requirements derive from [product acceptance criteria](../product/acceptance-criteria.md), [execution](../architecture/execution.md), [budget](../architecture/budget-costs.md) and [security](../architecture/security.md).

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

## Example scenarios

- Thousands of randomly generated small DAGs must reject cycles and accept valid dependency order.
- Two concurrent budgets reservations competing for a single remaining allowance must serialize correctly.
- A worker crash just before artifact acknowledgement must not duplicate a Git commit on restart.
- A retry may change model/effort while the task ID and original graph are unchanged.
- Provider costs arriving late must be reconciled; remaining exposure must not be misrepresented as guaranteed safe.
- A malicious README text instructing the worker to reveal secrets must be treated as untrusted content.

## Rust + SQLite stack verification (proposed)

The project owner has **approved Rust as the only Rolvya-owned backend language** ([ADR 0004](../decisions/0004-control-plane-language.md)) and **SQLite as the only authoritative database engine** ([ADR 0005](../decisions/0005-authoritative-database.md)). CI/review should reject unauthorized non-Rust backend services or additional authoritative databases. Test SQLite write serialization, FK enforcement on each connection, journal/backup consistency, lease fencing, atomic reservations, disk-full and busy-lock failures. Validate Rust domain/DTO separation, safe error handling, allocation/CPU profile and remote worker isolation. No tests are claimed to have run. See [Rust + SQLite implementation contract](../architecture/rust-sqlite-backend.md) and [SQLite operations](../operations/sqlite-operations.md).

## CI and release evidence (proposed)

CI stages: formatting/lint → unit/property tests → schema/migration tests → contract/integration suites → security scans → controlled end-to-end suites → docs checks. Each stage reports exact command, environment and artifacts. Merge/release gates, supported platforms and performance budgets need decisions before enforcement.

## Reporting and handoff

For every change record tests run/not run, expected vs actual results, failing cases, residual risks, usage and affected requirement IDs. Update [project status](../project/status.md) only on evidence; do not mark a feature implemented merely because tests are documented.
