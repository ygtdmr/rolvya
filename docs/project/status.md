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

## Next documentation tasks
1. Recover all prior approved architectural detail, confirm sources, and convert it into topic-specific specifications and ADRs.
2. Resolve open questions (technologies, interfaces, deployment boundaries, worker providers, identity/security, persistence and approval UX) before implementation depends on them.
3. Define acceptance criteria and testable vertical slices.
4. Keep this file synchronized with actual commits, tests and releases.

**Rule:** Do not promote a planned item to implemented without a linked code artifact and verification evidence.
