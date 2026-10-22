---
title: Historical Conversation Decision Audit
status: Proposed
category: project
last-reviewed: 2026-10-21
tags: [history, decisions, traceability, audit]
---

# Historical conversation decision audit

## Scope and evidence limits

This audit reconciles available context from the Rolvya main discussion (2026-10-19–20), subsequent repository-documentation discussions (2026-10-20–21), retained conversation summaries, and three earlier architecture diagram descriptions against repository specifications. It is **not** a verbatim inspection of every message in every historical chat, and is **not** a declaration that historical coverage is complete. Where evidence does not show a user approval, a design example stays **Explored**, not **Approved**.

For current source-of-truth priorities, see [decision register](../decisions/README.md), [requirements](../product/requirements.md) and [project status](status.md).

## Reconciliation matrix

| Historical topic | Evidence / decision state | Canonical documentation | Gap addressed or remaining |
| --- | --- | --- | --- |
| Brand Rolvya, selected wordless logo and navy/blue/gray, no gold | Approved | [Brand and project positioning](../product/brand-positioning.md) | Detailed brand intent added; image asset provenance not validated |
| Public personal GitHub repository `ygtdmr/rolvya`, `main`, MIT | Approved | [Decision register](../decisions/README.md) | Existing |
| Short description and tagline; avoid repeating tagline | Approved | [Brand positioning](../product/brand-positioning.md) | Added display guidance |
| Future potential company/organization; concerns about commercial competitors | Direction/concern; business/legal strategy open | [Brand positioning](../product/brand-positioning.md) | Separates MIT source-license terms from potential trademark strategy |
| One main agent fixes unlimited-depth task hierarchy and DAG before execution; workers do not plan | Approved | [DAG planning](../architecture/dag-planning.md), [ADR 0001](../decisions/0001-fixed-upfront-dag.md) | Existing |
| Parallel only when dependencies satisfied; hierarchy distinct from dependency | Approved | [DAG](../architecture/dag-planning.md), [execution](../architecture/execution.md) | Existing |
| Model and effort chosen per task; failed attempts may be reassigned | Approved design intent | [Workers](../architecture/workers.md), [execution](../architecture/execution.md) | Existing; provider-specific mapping remains open |
| Subscription-only → Claude CLI where permitted; API key → API mode | Previously stated product constraint; provider licensing unresolved | [Model access](../architecture/model-access.md) | Added explicit decision boundary and checks |
| Medium-size prompts/tasks; ~10 minute worker target | Planning preference, not SLA | [Context](../architecture/context-memory.md), [workers](../architecture/workers.md) | Existing; formal budget/performance limits remain open |
| Reduce token spending during planning and execution | Approved design objective | [Context](../architecture/context-memory.md), [model access](../architecture/model-access.md) | Added controls and open questions |
| Modest 2–4GB remote control VPS; workers remote, not personal computer | Approved architecture direction, sizing discussed not benchmarked | [Workers](../architecture/workers.md), [infrastructure history](../operations/infrastructure-options.md), [ADR 0003](../decisions/0003-remote-git-only.md) | Provider comparisons and RAM risk added |
| Same VPS, third-party cloud, user-supplied remote servers | Explored alternatives; remote cloud preferred | [Infrastructure options](../operations/infrastructure-options.md) | Added explicit alternatives |
| AWS ECS Fargate, Google Cloud Run, SSH/API remote hosts and spot fallback | Diagram examples, not binding selections | [Infrastructure options](../operations/infrastructure-options.md) | Added non-binding status |
| Illustrative 20 concurrent workers and per-day provider costs | Example only, not verified present-day price/limit | [Infrastructure options](../operations/infrastructure-options.md) | Archived examples with warnings |
| Single efficient database instance, prioritize speed/energy | Strong design preference, engine undecided | [Data model](../architecture/data-model.md) | Existing, physical design open |
| Hard project budget approval and no percentage overrun; four cost classes, three scenarios | Approved | [Budget](../architecture/budget-costs.md), [ADR 0002](../decisions/0002-hard-budget-approval.md) | Existing |
| Remote Git only; no Local Agent or personal computer sync | Approved exclusion | [Git integration](../architecture/integration-github.md), [ADR 0003](../decisions/0003-remote-git-only.md) | Existing |
| Completely AI-agent-developed; durable agent-readable specs and handoffs | Approved process requirement | [AGENTS.md](../../AGENTS.md), [testing](../development/testing.md) | Existing; detailed handoff template remains pending |
| English Markdown, GitHub source of truth, Obsidian-compatible navigation | Approved | [Documentation policy](../development/documentation-policy.md) | Existing; app UI test deliberately deferred |
| Diagram: Rust orchestrator; Claude Haiku/Sonnet/Opus, Codex named models | Diagram concepts/examples, not technology commitment | [Infrastructure options](../operations/infrastructure-options.md), [model access](../architecture/model-access.md) | Added status clarification |

## High-priority uncompleted specifications

1. [Deployment and infrastructure](../operations/infrastructure-options.md) still needs concrete deployment, recovery and secrets contracts, provider selection and benchmark-backed capacity decisions.
2. The model/CLI adapter interface needs provider-by-provider entitlement and terms verification, explicit user configuration and error behavior.
3. Scheduler state machine, worker protocol, context format, Git merge authority and API transport require precise implementable schemas.
4. UI workflows, identity/roles and project approval experience require explicit design and acceptance tests.
5. Database engine, indexing implementation, migrations, RPO/RTO and retention are not selected.
6. Full historical coverage requires source-by-source comparison with complete original transcripts and any additional diagrams; no invented decisions should fill missing evidence.

## Decision update after the historical checkpoint (2026-10-21)

The owner **subsequently approved Rust as the sole backend language** ([ADR 0004](../decisions/0004-control-plane-language.md)) and **SQLite as the sole authoritative database** ([ADR 0005](../decisions/0005-authoritative-database.md)). Entries above describing an undecided engine or historical Rust sketches reflect the earlier conversation state, **not the current architecture**. See the [Rust + SQLite backend contract](../architecture/rust-sqlite-backend.md). Framework/crates, SQLite WAL/migration settings and runtime infrastructure remain open details.

## Superseding evidence-mapping audit (2026-10-22)

A more detailed traceability matrix is now available at [Two-conversation coverage audit](two-conversation-coverage-audit.md). It covers later owner decisions **Approved ADR 0006 (HTTPS), ADR 0008 (GitHub App), ADR 0009 (group-only parents), ADR 0010 (bounded in-flight worker continuation or stop-on-risk), and ADR 0011 (multi-provider infrastructure)** in addition to Rust/SQLite. Earlier notes in this file saying that database engine, worker transport, GitHub authorization or parent-group rules were open are **historical snapshots, not current decisions**.

The deeper audit also records previously under-detailed user discussions about **model subscription/API eligibility, scheduler ranking and cloud hardware, retry/escalation diagrams, main-planner and worker token optimization, synthetic multi-scenario billing arithmetic and brand design evolution**. The corresponding topic specifications are linked from the new matrix. It still **does not claim full line-by-line access to both original chat exports**.

Historical diagrams also used individual illustrative model-duration figures (for example, a larger 30–60-minute planning activity alongside short ~2-minute worker examples) and named main-agent/orchestrator roles. A past mention of **“Rhynda”** as a question about who plans is **not evidence of a finalized component or product name**. Such examples must not override the Approved ~10-minute worker sizing preference, fixed up-front plan, non-executable group parents or settled Rolvya brand.

## Audit maintenance

When another historical decision is recovered, record: approximate discussion date, claim, user approval evidence, current status, normative destination, verification/test linkage and unresolved implications. Do not copy illustrative pricing or provider capacity into binding requirements.
