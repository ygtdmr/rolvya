---
title: Rolvya Delivery Roadmap
status: Proposed
category: project
last-reviewed: 2026-10-21
tags: [roadmap, phases, delivery, milestones]
---

# Rolvya delivery roadmap

## Status and planning distinction

This is a **proposed software development roadmap**, not an approved date-based schedule, and **not** a runtime Rolvya task DAG. The user-approved product rule requires each *actual Rolvya project execution* to have one fully preplanned immutable task graph. Developers may refine the **future product delivery roadmap** between releases with owner approval; that does not authorize changing the graph of an already activated project.

The backend is definitively Rust, authoritative DB SQLite, worker control **HTTPS**, GitHub authentication **GitHub App**, and remote worker architecture **multi-provider** (Approved ADRs 0004, 0005, 0006, 0008 and 0011). **The v0.1 feature boundary is now Owner-approved** in [product scope](../product/scope.md). Which concrete cloud adapters ship first, exact credentials/protocol implementation and frontend stack remain open. No implementation milestone is marked complete without evidence.

## Delivery phases

| Phase | Goal | Entry gate | Exit evidence |
| --- | --- | --- | --- |
| P0 — Decision readiness | Detail approved Rust/SQLite, worker HTTPS, GitHub App and multi-provider contracts; resolve remaining identity, wire, branch and first-provider rollout details | Approved ADRs 0004/0005/0006/0008/0009/0010/0011 | Security/permission matrix, threat model, no unapproved live spend |
| P1 — Rust+SQLite deterministic core | Immutable graph, task/attempt domain, budget admission, outbox and persistence | Defined domain and SQLite transaction contract | Property and concurrency tests on fake workload, crash replay |
| P2 — Fake remote-worker integration | Validate network and execution protocol without paid providers | P1 passes, authenticated test channel | Independent fake tasks complete, dependent task waits, stale responses fenced |
| P3 — Real remote worker and Git sandbox | Run an isolated remote worker, publish reviewable remote Git refs | Provider/legal access, live-spend approval and remote Git authorization | Actual job and Git refs, measured cost, clean worker termination, no local sync |
| P4 — Owner panel and workflow | Make state, approvals, logs/artifacts and costs intelligible | Trusted state APIs, identity policy | UX/security tests for budget pause, approval and exact result provenance |
| P5 — Hardening, public readiness | Security, backups, CI, docs, license checks, release artifacts | Verified P1–P4 slices | Independent E2E suite, recoverability drill, auditable release signoff |

The sequence is a *suggested dependency order*; unrelated development tasks within a phase can run concurrently after common contracts and ownership are defined.

## Critical cross-phase gates

**Cost:** All new billable launches, live provider experiments, idle pool resources and live model calls require explicit spending approval; cloud provider retry policy must not create hidden attempts.

**Immutable plan:** Main planner produces all hierarchical nodes and dependency edges before activation. Failure retry targets the same `task_id` and new `attempt_id`; workers never insert tasks.

**Provider eligibility:** User-owned CLI subscriptions are not automatically licensed for unattended remote cloud execution. Verify model/provider rules before enabling real adapters.

**Git security:** Remote-only; **GitHub App is Approved** as GitHub authentication, while protected integration writes require separate authorization, independent validation and an owner-authorized installation/permission setup. No App is installed by this documentation.

**Production reliability:** SQLite lock/busy, WAL and checkpoint, backups, crash consistency, audit, and delayed billing must be proven in tests rather than inferred.

## Proposed product MVP vs technical demos

A **technical demo** may use fake models, synthetic repositories and stub owner identities to validate algorithms with zero cloud charges. A **publicly usable MVP** needs secure real remote worker and remote Git integrations, transparent spend controls, identity/approval UX and recovery evidence. The **v0.1 feature boundary is now approved**; its technical implementation and per-provider release readiness still need independent evidence. See [product scope](../product/scope.md).

## Planning artifacts and agent workflow

For every development phase, require a scoped plan that includes task ownership, remote Git base commit, acceptance IDs, tests, cost exposure and handoff. AI agents should follow [engineering standards](../development/standards.md), [agent handoffs](../development/agent-handoffs.md), [review](../development/review-process.md) and documented [implementation slices](implementation-slices.md). Do not mark provider/CI tests as run merely because a Markdown checklist exists.

## Remaining scheduling choices

The **public v0.1 MVP feature boundary is Approved**. Release cadence, initial cloud adapter subset/regions, production deployment target, support policy, numerical performance SLOs and milestone dates remain **Open**; multi-user roles/permissions policies already have approved ADRs, but implementation is not ready.

Related: [milestones](milestones.md), [decision readiness](decision-readiness.md), [implementation slices](implementation-slices.md), [open questions](open-questions.md), [actual status](status.md).
