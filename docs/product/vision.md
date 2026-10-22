---
title: Rolvya Product Vision and Principles
status: Proposed
category: product
last-reviewed: 2026-10-21
tags: [vision, goals, product, open-source]
---

# Rolvya product vision and principles

## Established identity

**Project:** Rolvya. **Approved tagline:** “One Plan. Many Agents.” **Approved short description:** “Open-source orchestration for AI coding agents, with dependency-aware task planning and parallel execution.” Official project repository: `ygtdmr/rolvya`. Licensing: MIT. Brand colors: navy/blue/gray; no gold. These are established decisions recorded in [brand positioning](brand-positioning.md) and the [decision register](../decisions/README.md).

This vision elaboration is **Proposed** product guidance, not evidence that the product exists as working software.

## Problem

A single AI coding session may struggle to organize a large engineering goal, track dependencies, preserve useful work across model failures and run truly independent implementation steps concurrently. Ad-hoc multi-agent approaches can waste tokens through repeated planning, lose context, make conflicting Git changes, or incur uncontrolled cloud/model costs.

Rolvya aims to be a **small, self-hostable orchestration control plane** that receives a project goal, prepares a complete dependency-aware plan once, dispatches bounded execution attempts on remote worker resources, validates results and exposes auditable progress/costs to a human owner.

## Product promise (future capability)

A user should be able to:

1. Connect an authorized **remote** Git repository and configure eligible model access modes.
2. Supply an engineering goal, required constraints, exclusions and an approved project budget.
3. Inspect a **complete up-front** hierarchy and dependency DAG with per-task model and effort, requirements, estimated resources and three cost scenarios.
4. Run independent eligible tasks concurrently on remote workers; dependent tasks wait for validated predecessors.
5. View where every task stands, which attempt produced what, where remote code lives and what was actually tested.
6. Recover worker/model failures by reattempting existing tasks without rewriting the active DAG.
7. Control new billable work using a hard explicit budget approval gate; preserve successful artifacts on pause.
8. Review verified outputs, remote Git integration evidence and realistic actual-versus-estimated cost reports.

Every point above is **design intent**, not a claim of shipped functionality.

## Core principles: approved vs proposed

| Principle | Status | Product implication |
| --- | --- | --- |
| Single main planning agent and immutable complete task graph | Approved | No reactive runtime task invention |
| Workers execute rather than plan | Approved | Fixed scope and reproducible attempt handoffs |
| Dependency-safe concurrency | Approved | Resource available alone is insufficient for eligibility |
| User-owned hard budget approvals | Approved | No automatic percentage overrun |
| Remote Git and remote worker infrastructure only | Approved | No Local Agent, desktop repository sync or personal-computer access |
| Rust-only backend and SQLite authoritative database | Approved | Efficient small remote control-plane architecture |
| Small remote control VPS with many scalable remote workers | Architectural direction | Measure rather than assert 2–4GB scale or number of workers |
| Open-source and self-hostable | Approved open-source identity, proposed deployment experience | MIT-licensed source, reproducible builds and straightforward operations |
| Provider-agnostic adapters | Proposed architecture | Avoid unnecessary lock-in, validate costs and terms |
| Readable user-facing graph and financial telemetry | Proposed product experience | Clear blockers and reliable project status |

## Primary users (proposed)

- An individual developer who wants to supervise a larger code project using multiple specialized AI agents.
- A technically capable maintainer operating a self-hosted, small VPS control plane with remote worker capacity.
- A team/organization experimenting with auditable AI-assisted development, subject to future multi-user security choices.

The target audience, supported team size and business model require further research and approval. MIT permits commercial reuse with its copyright/permission notice; see [brand positioning](brand-positioning.md).

## Success measurements to establish

- Time to obtain a **validated** final change, not just speed of worker starts.
- Proportion of dependency-valid scheduling events and absence of runtime DAG mutation.
- Recoverability after crashed workers/control processes; preservation of validated work.
- Spend attribution completeness and number of unapproved billable admissions (target should be zero).
- Controller RAM/CPU against the chosen small VPS environment.
- Token/model usage per *accepted task*, not per generated line of code.
- Incidence of conflicting remote Git writes and failed integrations.
- User understanding of approvals, task state and uncertain provider charges.

Numerical SLOs, benchmarks and time-to-completion claims are **not yet approved**.

## Core tradeoffs and product risks

A fully fixed graph is predictable but less adaptive to unexpected discoveries; meaningful scope expansion needs a new plan/version approval mechanism. Many workers improve concurrency only where dependencies and conflicts permit. Low cloud prices are not enough if startup, idle capacity, hidden retries or transfers dominate. Subscription-based agent CLI operation on unattended remote servers requires provider-specific eligibility verification.

## Explicitly outside the immediate promise

No personal-computer Local Agent or Git synchronization, no auto-approved budget buffer, no worker-initiated subtask creation, no guaranteed fixed cloud price, no guaranteed cross-provider CLI entitlement, and no automatic merge into protected remote branches without approved policy.

## Evidence and delivery

Before declaring a feature implemented, record code commit, test suite results, remote provider evidence where applicable and the actual [project status](../project/status.md). Product details and release composition should be agreed in [scope](scope.md) and [milestones](../project/milestones.md).

Related: [requirements](requirements.md), [acceptance criteria](acceptance-criteria.md), [user journeys](user-flows.md), [architecture overview](../architecture/overview.md), [decisions](../decisions/README.md).
