---
title: "ADR 0004: Control-Plane Implementation Language"
status: Proposed
category: decision
last-reviewed: 2026-10-21
tags: [adr, rust, go, performance, control-plane]
---

# ADR 0004: Control-plane implementation language

## Status and authority

**Proposed — NOT approved.** Rust appeared in previous architecture sketches, but the user has not finalized a language. Rolvya values a low-overhead, energy-efficient control server on a small remote VPS (roughly 2–4 GB RAM discussed), with many independent remote workers and no personal-computer agent.

## Decision question

Which language/runtime should implement the authoritative scheduler, API, durable state transitions, provider adapters and resource-aware orchestration?

## Decision drivers

1. Small predictable RAM/CPU footprint when many attempts and heartbeats are tracked; avoid busy polling.
2. Durable concurrency semantics, strict types around money/identity/state, and testable fault behavior.
3. Safe untrusted input handling, remote worker integration, TLS/networking and maintainable async control flow.
4. Contributor familiarity and AI-agent reliability for code generation, testing and review.
5. Operational complexity, binary release footprint, cross-platform builds and dependency/security update burden.

## Candidate comparison (qualitative, to be measured)

| Criterion | Rust | Go | Higher-level dynamic runtime |
| --- | --- | --- | --- |
| Runtime/memory model | Native binary; explicit ownership, no tracing GC | Native binary; garbage collector, simple concurrency | VM/interpreter overhead varies substantially |
| Concurrency ergonomics | Async ecosystem; requires careful cancellation design | Goroutines and channels; simple network services | Mature async options; memory/CPU varies |
| Data/type safety | Strong static typing; compiler enforces ownership | Strong static typing; GC simplifies ownership | Often more dynamic validation at boundaries |
| Build/development complexity | Higher; compiler feedback valuable but compile times cost | Generally lower build/tool complexity | Fast iteration; runtime-specific tradeoffs |
| Operational fit on tiny VPS | Promising; **not benchmarked for Rolvya** | Promising; **not benchmarked for Rolvya** | Possible, but profile required |
| Cost/energy | Unknown until representative load | Unknown until representative load | Unknown until representative load |

These are directional tradeoffs, **not measured Rolvya benchmarks**. “Low-level” is a design preference, not a demand for C or unsafe code.

## Working recommendation (subject to approval)

**Evaluate Rust first**, with **Go as a serious comparator**. Rust fits the earlier compact-control-plane direction and enables explicit memory ownership; Go may materially reduce implementation complexity without violating the measured 2–4 GB goal. Reject neither before an apples-to-apples benchmark. Do not interpret this section as authorization for an agent to scaffold the project.

## Required selection experiment

Implement *disposable remote prototypes*, not production features, for both finalists:

- Load a synthetic fixed DAG, run dependency readiness queries and apply multiple simulated completion events.
- Run concurrent **simulated** atomic budget admissions with fixed currency/integer units.
- Schedule many fake remote attempts with retry/fencing/heartbeats and no actual billable worker provider.
- Serialize/validate context/event envelopes and expose a minimal authenticated health endpoint.
- Measure idle and loaded resident memory, peak CPU, throughput, p50/p95/p99 scheduling latency, startup time, executable/build size, defect/debug time and static-analysis findings.
- Test restarts, cancellation and malformed/prompt-injected messages.
- Use the same hardware, dependency graph, fake provider and workload seeds for both languages.

**Decision gate:** choose only after benchmark data plus maintainer/owner approval and a migration assessment. Do not invent target p95 or RAM thresholds: set them explicitly using the chosen VPS size and worker scale.

## Consequences if Rust is selected

Use typed domain values for task/attempt identity, money with explicit precision, immutable plan revision, fence epochs and risk-aware cancellation; ban unsafe code except reviewed justified modules; keep adapters isolated. Document async runtime, database driver, TLS/certificate policy, migrations and supported operating systems in subsequent ADRs.

## Alternatives and migration

Go remains feasible and may be a fallback if Rust complexity or delivery friction outweighs measured benefits. Isolate external messages, stored logical schema and worker adapter contracts from the implementation language to permit future migration. No language choice may authorize a Local Agent, mutable runtime DAG or relaxed budget gate.

## Decision ownership

Owner approval **pending**; benchmark results **not yet available**. Related open question: [OQ-001](../project/open-questions.md). Related: [overview](../architecture/overview.md), [scheduler](../architecture/scheduler.md), [testing](../development/testing.md), [deployment](../operations/deployment.md).
