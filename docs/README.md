# Rolvya documentation index

**Status:** Documentation foundation in progress. The existence of a section or path in this map does **not** mean its specification is complete.

## How to read this documentation

Start at [the repository README](../README.md), read [AGENTS.md](../AGENTS.md) if you are a coding agent, and consult [current status](project/status.md) plus the [decision register](decisions/README.md) before starting an implementation task.

## Obsidian navigation

Open the repository root as an Obsidian Vault so links to the root README and AGENTS.md remain reachable. Begin with the [architecture overview](architecture/overview.md), follow the related-document links, and consult the [decision register](decisions/README.md). Use ordinary relative Markdown links so the same navigation works on GitHub without an Obsidian plugin.

See the [compatibility audit](development/obsidian-audit.md) for the inspected files, checks, and remaining limitations.

## Documentation format

All Markdown documentation follows the [Obsidian-compatible documentation policy](development/documentation-policy.md): GitHub-compatible links, portable YAML metadata where useful, and optional Mermaid diagrams. GitHub remains the source of truth.

## Product and quality specifications now available

- [Product requirements](product/requirements.md)
- [Acceptance criteria](product/acceptance-criteria.md)
- [Testing and verification strategy](development/testing.md)

## Architecture specifications now available

- [System overview](architecture/overview.md)
- [DAG planning](architecture/dag-planning.md)
- [Execution lifecycle](architecture/execution.md)
- [Remote workers and models](architecture/workers.md)
- [Budget and costs](architecture/budget-costs.md)
- [Security model](architecture/security.md)
- [Persistence and data model](architecture/data-model.md)
- [Agent context and memory](architecture/context-memory.md)
- [API and event contracts](architecture/api-contracts.md)

These are **proposed architectural specifications**, not verified implementations or evidence that every prior decision has been recovered.

## Planned documentation map

| Area | Intended files and coverage |
| --- | --- |
| `product/` | `vision.md`, [`requirements.md`](product/requirements.md), `scope.md`, `user-flows.md`, [`acceptance-criteria.md`](product/acceptance-criteria.md) |
| `architecture/` | `overview.md`, `orchestrator.md`, `task-model.md`, `dag-planning.md`, `scheduler.md`, `agents.md`, `execution.md`, `workers.md`, [`context-memory.md`](architecture/context-memory.md), `integration-github.md`, [`data-model.md`](architecture/data-model.md), [`api-contracts.md`](architecture/api-contracts.md), `security.md`, `observability.md`, `failure-recovery.md`, `budget-costs.md` |
| `operations/` | `deployment.md`, `infrastructure.md`, `configuration.md`, `secrets.md`, `backup-recovery.md`, `runbooks.md` |
| `development/` | `standards.md`, [`testing.md`](development/testing.md), `review-process.md`, `agent-handoffs.md`, [`documentation-policy.md`](development/documentation-policy.md) |
| `project/` | [`status.md`](project/status.md), `roadmap.md`, `milestones.md`, `open-questions.md` |
| `decisions/` | [`README.md`](decisions/README.md), numbered Architecture Decision Records (ADRs) |
| `reference/` | glossary, diagrams, examples, and interface references |

The paths above are a **planned inventory**, not links to files that already exist. Create individual documents as their requirements are verified. Avoid empty placeholder documents masquerading as completed specifications.

## Coverage checklist

Documentation must eventually cover:
- Product goals, explicit non-goals, personas, workflows, and acceptance criteria.
- Upfront multi-level task decomposition; parent/child/leaf tasks; DAG dependencies; ready queues; model and effort assignment.
- Parallel scheduling, worker selection/capacity, cloud execution, interfaces, health checks, logs, metrics, retry and fallback policy.
- Agent coordination, context/memory, outputs and result aggregation, testing and final validation.
- GitHub remote repository integration, authentication, authorization, threat model, data isolation, and secret handling.
- Cost breakdown, scenario estimates, hard approval gates, pausing, and resuming.
- Data models, APIs, interface contracts, reliability, recovery, deployment and operations.
- Design decisions with evidence, rejected alternatives, migration implications, and owner approval.
- Tested implementation state versus proposed behavior, including agent handoff and pending decisions.

## Document lifecycle and quality
Use explicit labels: **Approved**, **Proposed**, **Open**, **Implemented**, **Verified** (do not conflate approval with implementation). Record last-reviewed date and related ADR/issue where appropriate. One authoritative document per topic; link instead of duplicating normative requirements.
