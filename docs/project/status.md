# Project status

Last verified documentation checkpoint: 2026-10-20

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

## Next documentation tasks
1. Recover all prior approved architectural detail, confirm sources, and convert it into topic-specific specifications and ADRs.
2. Resolve open questions (technologies, interfaces, deployment boundaries, worker providers, identity/security, persistence and approval UX) before implementation depends on them.
3. Define acceptance criteria and testable vertical slices.
4. Keep this file synchronized with actual commits, tests and releases.

**Rule:** Do not promote a planned item to implemented without a linked code artifact and verification evidence.
