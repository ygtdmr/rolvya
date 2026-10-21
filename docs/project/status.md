# Project status

Last verified documentation checkpoint: 2026-10-21

## Phase
**Specification / repository foundation.** No executable Rolvya orchestration implementation is verified by this document.

## Verified repository artifacts
- `LICENSE` — GNU MIT version 3 text present on `main`.
- Root `README.md` — introductory project description and explicitly pre-implementation status.
- `AGENTS.md` and initial `docs/` foundation — repository guidance and planning documents, not implementation.

## Established project decisions
- Project name Rolvya; open-source AI coding-agent orchestration.
- GitHub repository `ygtdmr/rolvya` (public, `main`); MIT selected.
- Dependency-aware up-front DAG planning and parallel execution of independent tasks are core architectural intent.
- Development will be performed by AI coding agents under the owner's direction; thorough documentation and handoffs required.
- Budget is a hard human approval gate, not an automatic overrun allowance.
- No Local Agent/Local Sync or direct personal-computer repository access/synchronization.

See [decision register](../decisions/README.md) for nuances.

## Not yet verified as implemented
- Orchestrator, scheduler, worker runtime or adapters, UI, API, database or cloud infrastructure.
- Production GitHub integrations, cost enforcement, approval workflow, or automated testing.
- Deployment, releases, security and performance properties.

## Architecture documentation progress (2026-10-21)
The initial six proposed specifications were added: [overview](../architecture/overview.md), [DAG planning](../architecture/dag-planning.md), [execution](../architecture/execution.md), [workers](../architecture/workers.md), [budget and costs](../architecture/budget-costs.md), and [security](../architecture/security.md). These documents specify intended behavior only. Three additional proposed specifications were added on 2026-10-21: [persistence and data model](../architecture/data-model.md), [agent context and memory](../architecture/context-memory.md), and [API and event contracts](../architecture/api-contracts.md). These remain design documents, not implemented services. Requirements coverage and historical decision reconciliation remain incomplete.

## Product and verification specifications (2026-10-21)
Three proposed documents were added: [product requirements](../product/requirements.md), [acceptance criteria](../product/acceptance-criteria.md) and [testing strategy](../development/testing.md). They establish traceable future behavior and verification cases, **not** completed implementation or executed tests.

## Git integration and recorded ADRs (2026-10-21)
The proposed [remote Git integration specification](../architecture/integration-github.md) and three approved-decision ADRs were documented: [fixed upfront DAG](../decisions/0001-fixed-upfront-dag.md), [hard budget approvals](../decisions/0002-hard-budget-approval.md), and [remote Git only](../decisions/0003-remote-git-only.md). These capture decisions and planned contracts, not implemented integrations.

## Historical conversation reconciliation (2026-10-21)
An initial [conversation-to-document decision audit](historical-decision-audit.md) was added, with companion [brand positioning](../product/brand-positioning.md), [CLI/API mode](../architecture/model-access.md) and [infrastructure options](../operations/infrastructure-options.md) documentation. This reconciles the available archived conversation context and earlier diagram descriptions; it is **not** a verified line-by-line audit of every historical message. Remaining evidence gaps and open technology choices are recorded rather than guessed.

## Operations specifications (2026-10-21)
Three additional proposed specifications were recorded: [deployment](../operations/deployment.md), [worker lifecycle](../operations/worker-lifecycle.md), and [operational secrets](../operations/secrets.md). These are unimplemented contracts, not a running deployment; provider, persistence, runtime and security technology choices remain open.

## Scheduler, worker protocol and recovery specifications (2026-10-21)

Three additional proposed specifications were recorded: [scheduler state machine](../architecture/scheduler.md), [worker control protocol](../architecture/worker-protocol.md), and [failure recovery](../architecture/failure-recovery.md). They define proposed transitions, logical messages, failure handling and test scenarios. No executable scheduler, communications service or recovery mechanism has been verified; actual transport, timing, retry limits and approval details remain open.

## Engineering, operations, product-flow and decision backlog (2026-10-21)

Nine additional **proposed** Markdown specifications were saved: [engineering standards](../development/standards.md), [agent handoff protocol](../development/agent-handoffs.md), [change review](../development/review-process.md), [observability and audit](../architecture/observability.md), [backup and disaster recovery](../operations/backup-recovery.md), [incident runbooks](../operations/runbooks.md), [owner panel user journeys](../product/user-flows.md), [operational configuration](../operations/configuration.md) and [open decisions](open-questions.md). All describe future behavior; none constitutes evidence that code, CI, production infrastructure, security controls or monitoring actually runs.

## Next documentation tasks
1. Recover all prior approved architectural detail, confirm sources, and convert it into topic-specific specifications and ADRs.
2. Resolve open questions (technologies, interfaces, deployment boundaries, worker providers, identity/security, persistence and approval UX) before implementation depends on them.
3. Decompose acceptance criteria into implementation slices with measured performance limits and proof-of-completion checks; user journeys and criteria are drafted, not implemented.
4. Keep this file synchronized with actual commits, tests and releases.

**Rule:** Do not promote a planned item to implemented without a linked code artifact and verification evidence.
