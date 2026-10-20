# Rolvya documentation index

**Status:** Documentation foundation in progress. The existence of a section or path in this map does **not** mean its specification is complete.

## How to read this documentation

Start at [the repository README](../README.md), read [AGENTS.md](../AGENTS.md) if you are a coding agent, and consult [current status](project/status.md) plus the [decision register](decisions/README.md) before starting an implementation task.

## Planned documentation map

| Area | Intended files and coverage |
| --- | --- |
| `product/` | `vision.md`, `requirements.md`, `scope.md`, `user-flows.md`, `acceptance-criteria.md` |
| `architecture/` | `overview.md`, `orchestrator.md`, `task-model.md`, `dag-planning.md`, `scheduler.md`, `agents.md`, `execution.md`, `workers.md`, `context-memory.md`, `integration-github.md`, `data-model.md`, `api-contracts.md`, `security.md`, `observability.md`, `failure-recovery.md`, `budget-costs.md` |
| `operations/` | `deployment.md`, `infrastructure.md`, `configuration.md`, `secrets.md`, `backup-recovery.md`, `runbooks.md` |
| `development/` | `standards.md`, `testing.md`, `review-process.md`, `agent-handoffs.md`, `documentation-policy.md` |
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
