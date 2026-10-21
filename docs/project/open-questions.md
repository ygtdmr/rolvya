---
title: Open Decisions and Architecture Questions
status: Open
category: project
last-reviewed: 2026-10-21
tags: [decisions, roadmap, unresolved, adr]
---

# Open decisions and architecture questions

## Purpose and interpretation

This register tracks **unresolved choices** that must not be silently converted into implemented requirements. It is a planning instrument, not evidence a particular provider, framework or algorithm has been approved. Approved constraints remain in the [decision register](../decisions/README.md), [requirements](../product/requirements.md) and numbered ADRs.

Each question should ultimately have an owner decision, dated rationale, alternative evaluation, security/cost consequences and a linked ADR or requirement update.

## Priority classification

**P0:** needed before implementation depends on the choice, particularly for security, persistence, authorization and budget integrity. **P1:** needed before connected functionality or first release. **P2:** may remain a documented deferred enhancement. These priority labels are proposed for review.

| ID | Priority | Question | Candidate directions / decision criteria | Related specification |
| --- | --- | --- | --- | --- |
| OQ-001 | P0 | Main orchestrator implementation language/runtime? | Rust or other efficient compiled runtime; measured memory, deployment and maintainability | [Architecture](../architecture/overview.md) |
| OQ-002 | P0 | Single authoritative DB engine and durability model? | SQLite / PostgreSQL / alternative; concurrent admissions, crash safety, 2–4 GB control VPS | [Data model](../architecture/data-model.md) |
| OQ-003 | P0 | Atomic task lease + budget reservation transaction? | DB transaction/isolation, idempotent outbox, fencing epochs, delayed charges | [Scheduler](../architecture/scheduler.md) |
| OQ-004 | P0 | Authentication, project roles and authorization? | Role policy, audit identity, tenant separation, human budget approval | [Security](../architecture/security.md) |
| OQ-005 | P0 | Secrets storage and remote worker identity? | Secret-manager references, ephemeral worker identity, revocation, sandbox boundary | [Secrets](../operations/secrets.md) |
| OQ-006 | P0 | Worker communication protocol and queue transport? | HTTP polling, RPC, queues; leases, late callbacks, duplication and failure testing | [Worker protocol](../architecture/worker-protocol.md) |
| OQ-007 | P0 | GitHub/provider authentication and merge authority? | GitHub App vs delegated OAuth; least privilege, isolated branches, review gates | [Git integration](../architecture/integration-github.md) |
| OQ-008 | P0 | Provider-valid CLI subscription versus API access rules? | Terms, supported automation, tokens and billing by provider/model | [Model access](../architecture/model-access.md) |
| OQ-009 | P0 | Hard cost exposure policy for running attempts and delayed invoices? | Provider-side caps, conservative reservations, pause and reconciliation | [Budget](../architecture/budget-costs.md) |
| OQ-010 | P1 | Cloud worker providers and container/sandbox setup? | Cost vs startup/CPU isolation, reliability, region and user-provided remote servers | [Infrastructure](../operations/infrastructure-options.md) |
| OQ-011 | P1 | Worker lease TTL, heartbeat/retry and timeouts? | Benchmark-backed defaults, fenced attempts, quotas and non-idempotent writes | [Recovery](../architecture/failure-recovery.md) |
| OQ-012 | P1 | Context packaging, token quotas and task handoff format? | Provider-specific adapters, compact references, provenance, truncation detection | [Context](../architecture/context-memory.md) |
| OQ-013 | P1 | UI stack, roles and plan approval interactions? | Budget approval security, task graph readability and accessibility | [User flows](../product/user-flows.md) |
| OQ-014 | P1 | Backup RPO/RTO, encryption and retention? | Risk to approvals/ledger/artifacts, restore exercises, storage cost | [Backups](../operations/backup-recovery.md) |
| OQ-015 | P1 | CI merge gates, review roles and release process? | Test scope, human security signoff, branch protection, MIT compliance | [Review](../development/review-process.md) |
| OQ-016 | P1 | Alert delivery, monitoring stack and audit retention? | Low overhead, integrity, tenant privacy, noise controls | [Observability](../architecture/observability.md) |
| OQ-017 | P1 | Parent task aggregation and failed descendant semantics? | Explicit hierarchy separate from dependency edges; deterministic verification | [DAG](../architecture/dag-planning.md) |
| OQ-018 | P2 | Non-GitHub self-hosted remote Git adapters? | Provider compatibility and authorization, never local computer sync | [Git integration](../architecture/integration-github.md) |

## Decisions that are not open

These have already been approved and must not be reopened by a worker without owner consent: one main agent creates a fully fixed task hierarchy and DAG before runtime; workers execute, not plan; dependencies govern parallelism; hard budget pause with explicit approval and no percentage overrun; preserve completed work; separate cost categories/scenarios; remote Git only; no Local Agent or personal-computer sync; GNU MIT license.

## How to resolve a question

1. Gather up-to-date technical evidence and, when applicable, provider terms/pricing; record sources and assumptions.
2. Evaluate at least realistic alternative and rejected direction against cost, RAM/CPU, reliability, security and complexity.
3. Propose an ADR with impact, migration risk and acceptance tests.
4. Obtain explicit human approval when consequential; preserve approval actor/date.
5. Update the decision register, requirement IDs, specs and tests together.
6. Implement only through an authorized planned task; do not mutate the active DAG to resolve a choice.

## Coverage limitations

Previous conversation summaries and diagrams may omit specific wording. The [historical decision audit](historical-decision-audit.md) documents the currently available evidence and its limitations; missing provenance must be marked unknown, not invented.

Related: [documentation index](../README.md), [project status](status.md), [review process](../development/review-process.md), [decision register](../decisions/README.md).
