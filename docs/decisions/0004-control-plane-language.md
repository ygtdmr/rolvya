---
title: "ADR 0004: Rust-Only Backend"
status: Approved
category: decision
last-reviewed: 2026-10-21
tags: [adr, rust, backend, architecture]
---

# ADR 0004: Rust-only backend

## Status and approval

**Approved by the project owner on 2026-10-21.** This supersedes the former *Proposed* Rust-versus-Go evaluation in this ADR. The owner explicitly decided: **Rolvya's backend is written only in Rust.** Language selection is closed.

## Context

Rolvya is an open-source orchestration system with an immutable up-front task DAG, independent parallel worker execution, authoritative state management and a hard human-controlled budget. The control plane is intended to work on a modest remote VPS (roughly 2–4 GB RAM was discussed; not performance-verified). Backend components must remain efficient, safe, auditable and suitable for remote-agent development.

## Decision

1. **Rust is the only implementation language for Rolvya-owned backend services and control-plane logic.** This includes orchestration, planner coordination, DAG validation, scheduling, task/attempt lifecycle, budget/approval enforcement, persistence access, API/backend endpoints, remote-worker control, Git/provider adapters, reconciliation and audit logic.
2. **Do not introduce Go, Python, Node.js/TypeScript or another language as a parallel Rolvya backend service or alternative backend implementation** without a new explicit owner-approved superseding ADR.
3. SQL is permitted for SQLite schema, queries and migrations; configuration and data serialization languages (e.g., YAML, JSON) do not count as additional backend implementation languages.
4. External coding-agent CLIs, provider binaries and programs/build scripts belonging to **user projects on remote workers** are not Rolvya-owned backend implementations and are not prohibited by this language decision. The frontend/UI language and framework are **not determined by this ADR**.
5. Rust code must enforce all existing architecture invariants: immutable activated DAG, workers executing rather than replanning, remote Git only and hard approved-budget gates.

## Rationale

The owner chose Rust decisively. The architecture prefers efficient control-plane resource usage, explicit type safety and reliable concurrency. **These are motivations, not claims of measured Rolvya performance.** Rust's chosen status no longer depends on a Rust-versus-Go performance contest.

## Consequences for implementation

- Use a Rust workspace/module structure with separate domain (task/attempt/budget), application services (scheduler, approval, validation), persistence, worker protocol, provider adapters and API boundaries.
- Use strong domain types for plan IDs, task IDs, attempt IDs, fencing epochs, monetary values and authorized actions; avoid mixing task/attempt identity or floats in critical ledger decisions.
- Prefer safe Rust; justify and independently review any `unsafe` blocks.
- Specify an async runtime, HTTP stack, SQLite driver, migration tool, CLI process supervisor, tracing and cryptographic dependencies **in separate proposed implementation decisions**; Rust selection alone does not approve crates or versions.
- Run resource, cancellation, fault-injection and concurrency benchmarks on the intended 2–4 GB remote control host, even though alternative language benchmarking is no longer required.
- Preserve compatibility with remote workers running third-party agents and user project commands.

## Rejected or superseded alternatives

Earlier ADR drafts compared Rust and Go and considered higher-level runtimes. Those comparisons were exploratory and **are not live alternatives** after the owner's Rust-only decision. Changing the backend language requires explicit owner approval and a new ADR.

## Verification requirements

- Every Rolvya-owned backend executable/service has Rust source and Rust build provenance.
- Repository review rejects new non-Rust backend services unless explicitly superseded.
- Rust components pass DAG immutability, atomic budget admission, stale lease, restart and remote Git idempotency tests.
- Performance evidence verifies actual resource constraints rather than inferring them from language choice.

## Remaining open decisions

Rust edition/toolchain version, async runtime/crate choices, project module layout, compile/release pipeline, resource limits and supporting frontend stack.

Related: [ADR 0005 — SQLite](0005-authoritative-database.md), [data model](../architecture/data-model.md), [architecture overview](../architecture/overview.md), [Rust and SQLite backend contract](../architecture/rust-sqlite-backend.md), [open questions](../project/open-questions.md).
