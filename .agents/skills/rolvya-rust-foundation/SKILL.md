---
name: rolvya-rust-foundation
description: Implement or refactor Rolvya Rust domain, immutable plans, typed identifiers, graph validation, and initial Cargo workspace code. Use for Rust core tasks; not for database transactions, UI, or cloud deployment.
---

# Rolvya Rust foundation

1. Read root `AGENTS.md`, `docs/project/status.md`, the relevant approved ADRs, and the specific domain/task contracts. Treat planned examples as proposals, not implemented APIs. Do not reopen approved decisions.
2. Use Graphify before investigating existing code: run `graphify query` and targeted `graphify explain` or `graphify path` where relevant. Confirm its findings against files. After code changes run `graphify update .`; do not commit generated graphs.
3. Scope one reviewable Rust component at a time. Use only Rust for the owned backend and do not add a second authoritative database. Keep domain types independent of transports, external providers, and persistence implementations.
4. Prefer opaque, noninterchangeable IDs, checked integer minor-unit money with currency identity, explicit errors, validated constructors, and deterministic results. Do not authorize spending with floating-point arithmetic.
5. Keep immutable plan content separate from mutable execution/attempt state. A main planner fixes the complete DAG before activation; running workers never create new tasks. Treat non-leaf parents as non-executable groups and validate explicit dependencies independently of hierarchy.
6. Reject cycles, dangling/cross-plan references, duplicate IDs, invalid states, and ambiguous optional-child behavior with clear diagnostics. Use deterministic fixtures and property/boundary tests rather than only happy-path examples.
7. Before introducing or changing Rust toolchains/crates, inspect existing manifests and approved constraints; lock exact tested versions. Do not scaffold all proposed crates at once.
8. Run applicable `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --locked` when a workspace exists. Record commands, outcomes, unsupported checks, file paths, and known gaps in the handoff.

Relevant contracts: `docs/architecture/rust-workspace.md`, `docs/development/initial-implementation-task-packages.md`, `docs/architecture/task-model.md`, and ADRs 0001, 0004, 0009.
