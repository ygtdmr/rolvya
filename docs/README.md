# Rolvya documentation index

**Status:** Documentation foundation in progress. The existence of a section or path in this map does **not** mean its specification is complete.

## How to read this documentation

Start at [the repository README](../README.md), read [AGENTS.md](../AGENTS.md) if you are a coding agent, and consult [current status](project/status.md) plus the [decision register](decisions/README.md) before starting an implementation task.

## Obsidian navigation

Open the repository root as an Obsidian Vault so links to the root README and AGENTS.md remain reachable. Begin with the [architecture overview](architecture/overview.md), follow the related-document links, and consult the [decision register](decisions/README.md). Use ordinary relative Markdown links so the same navigation works on GitHub without an Obsidian plugin.

See the [compatibility audit](development/obsidian-audit.md) for the inspected files, checks, and remaining limitations.

## Documentation format

All Markdown documentation follows the [Obsidian-compatible documentation policy](development/documentation-policy.md): GitHub-compatible links, portable YAML metadata where useful, and optional Mermaid diagrams. GitHub remains the source of truth.

## Historical decision reconciliation

- [Historical conversation-to-document audit](project/historical-decision-audit.md)
- [Brand, repository and MIT positioning](product/brand-positioning.md)
- [CLI/API mode constraints](architecture/model-access.md)
- [Infrastructure candidates and historical cost examples](operations/infrastructure-options.md)

## Product and quality specifications now available

- [Product vision and success principles](product/vision.md)
- [Product scope, exclusions and MVP candidates](product/scope.md)

- [Product requirements](product/requirements.md)
- [Acceptance criteria](product/acceptance-criteria.md)
- [Owner panel and user journeys](product/user-flows.md)
- [Testing and verification strategy](development/testing.md)

## Architecture specifications now available

- [System overview](architecture/overview.md)
- [Main orchestrator responsibilities](architecture/orchestrator.md)
- [Hierarchy and task/attempt model](architecture/task-model.md)
- [Planning and execution agent contracts](architecture/agents.md)
- [DAG planning](architecture/dag-planning.md)
- [Execution lifecycle](architecture/execution.md)
- [Scheduler state machine](architecture/scheduler.md)
- [Worker control protocol](architecture/worker-protocol.md)
- [Remote worker transport design](architecture/worker-transport-design.md)
- [Failure classification and recovery](architecture/failure-recovery.md)
- [Monitoring and audit architecture](architecture/observability.md)
- [Remote workers and models](architecture/workers.md)
- [Budget and costs](architecture/budget-costs.md)
- [Budget reservation and reconciliation](architecture/budget-reservation-reconciliation.md)
- [Security model](architecture/security.md)
- [Project identity and authorization](architecture/identity-authorization.md)
- [Human approval state machine](architecture/approval-workflow.md)
- [Remote GitHub integration](architecture/integration-github.md)
- [GitHub App security design](architecture/github-app-integration-design.md)
- [Persistence and data model](architecture/data-model.md)
- [Rust + SQLite backend contract](architecture/rust-sqlite-backend.md)
- [Proposed Rust workspace boundaries](architecture/rust-workspace.md)
- [SQLite schema and atomic admission draft](architecture/sqlite-schema-draft.md)
- [Agent context and memory](architecture/context-memory.md)
- [API and event contracts](architecture/api-contracts.md)

These are **proposed architectural specifications**, not verified implementations or evidence that every prior decision has been recovered.

## Planned documentation map

| Area | Intended files and coverage |
| --- | --- |
| `product/` | [vision.md](product/vision.md), [requirements.md](product/requirements.md), [scope.md](product/scope.md), [user-flows.md](product/user-flows.md), [acceptance-criteria.md](product/acceptance-criteria.md) |
| `architecture/` | `overview.md`, [orchestrator.md](architecture/orchestrator.md), [task-model.md](architecture/task-model.md), `dag-planning.md`, `scheduler.md`, [agents.md](architecture/agents.md), `execution.md`, `workers.md`, [`context-memory.md`](architecture/context-memory.md), `integration-github.md`, [`data-model.md`](architecture/data-model.md), [`api-contracts.md`](architecture/api-contracts.md), `security.md`, [observability.md](architecture/observability.md), `failure-recovery.md`, `budget-costs.md` |
| `operations/` | `deployment.md`, `infrastructure.md`, [configuration.md](operations/configuration.md), [secrets.md](operations/secrets.md), [backup-recovery.md](operations/backup-recovery.md), [runbooks.md](operations/runbooks.md) |
| `development/` | [standards.md](development/standards.md), [testing.md](development/testing.md), [review-process.md](development/review-process.md), [agent-handoffs.md](development/agent-handoffs.md), [`documentation-policy.md`](development/documentation-policy.md) |
| `project/` | [status.md](project/status.md), [roadmap.md](project/roadmap.md), [milestones.md](project/milestones.md), [open-questions.md](project/open-questions.md), [implementation-slices.md](project/implementation-slices.md) |
| `decisions/` | [`README.md`](decisions/README.md), numbered Architecture Decision Records (ADRs) |
| `reference/` | [glossary.md](reference/glossary.md), additional diagrams, examples, and interface references |

The paths above are a **planned inventory**, not links to files that already exist. Create individual documents as their requirements are verified. Avoid empty placeholder documents masquerading as completed specifications.

## Engineering and agent execution specifications

- [Engineering standards](development/standards.md)
- [AI agent handoff contract](development/agent-handoffs.md)
- [Review and ADR governance](development/review-process.md)
- [Open decisions and architectural questions](project/open-questions.md)

## Operations specifications now available

- [Deployment architecture](operations/deployment.md)
- [Remote worker lifecycle](operations/worker-lifecycle.md)
- [Operational secrets](operations/secrets.md)
- [Operational configuration](operations/configuration.md)
- [Backup and disaster recovery](operations/backup-recovery.md)
- [SQLite operations and recovery](operations/sqlite-operations.md)
- [Incident response runbooks](operations/runbooks.md)
- [Infrastructure alternatives](operations/infrastructure-options.md)
- [Worker provider benchmark protocol](operations/worker-provider-evaluation.md)

## Architecture evaluation and implementation planning

- [Approved ADR 0004 — Rust-only backend](decisions/0004-control-plane-language.md)
- [Approved ADR 0005 — SQLite database](decisions/0005-authoritative-database.md)
- [Proposed ADR 0006 — Worker communication](decisions/0006-worker-communication.md)
- [Proposed ADR 0007 — Remote worker hosting](decisions/0007-remote-worker-hosting.md)
- [Proposed ADR 0008 — GitHub authentication](decisions/0008-github-authentication.md)
- [Evidence-driven implementation slices](project/implementation-slices.md)
- [Pre-implementation decision readiness](project/decision-readiness.md)
- [Product delivery roadmap](project/roadmap.md)
- [Milestone definitions and evidence gates](project/milestones.md)
- [Architecture glossary](reference/glossary.md)

**ADR 0004 and ADR 0005 are approved (2026-10-21); ADRs 0006–0008 remain proposed.** The [owner's open technical decisions](project/open-questions.md) must be resolved explicitly; only Rust and SQLite are approved stack selections; other platform/provider choices await explicit approval.

## Recorded architecture decisions

- [ADR 0001 — Fixed upfront DAG](decisions/0001-fixed-upfront-dag.md)
- [ADR 0002 — Hard budget approval](decisions/0002-hard-budget-approval.md)
- [ADR 0003 — Remote Git only](decisions/0003-remote-git-only.md)

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
