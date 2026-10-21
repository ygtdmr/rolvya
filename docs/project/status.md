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

## Technology-choice evaluation checkpoint (2026-10-21)

Five ADR drafts were originally recorded for [control-plane language](../decisions/0004-control-plane-language.md), [authoritative database](../decisions/0005-authoritative-database.md), [remote worker communication](../decisions/0006-worker-communication.md), [remote worker hosting](../decisions/0007-remote-worker-hosting.md) and [GitHub authentication](../decisions/0008-github-authentication.md). **Subsequently, ADR 0004 (Rust-only backend) and ADR 0005 (SQLite database) were approved by the owner on 2026-10-21; ADRs 0006–0008 remain proposed.** An [evidence-driven implementation slice roadmap](implementation-slices.md) lists dependencies, entry/exit conditions and fake-provider tests. Official vendor documentation informed option comparison, **not** measured Rolvya benchmarks, approved prices or deployed providers.

**Open:** owner decisions, physical schema, protocol, credentials, deployment setup, performance targets and CI. No compiled control plane, running database, live worker, production GitHub App or end-to-end test is verified.

## Owner-approved backend and database stack (2026-10-21)

The project owner explicitly chose **Rust as the only Rolvya-owned backend language** ([ADR 0004 — Approved](../decisions/0004-control-plane-language.md)) and **SQLite as the only authoritative database engine** ([ADR 0005 — Approved](../decisions/0005-authoritative-database.md)). The [Rust + SQLite implementation contract](../architecture/rust-sqlite-backend.md) and [SQLite operations](../operations/sqlite-operations.md) specify proposed module responsibilities, atomic task/budget admission, crash consistency and recovery. The [SQLite schema blueprint](../architecture/sqlite-schema-draft.md) adds illustrative SQL and atomic admission tests; it is **Proposed**, not an applied migration. Other technology ADRs (worker transport, cloud hosting, GitHub authentication), Rust crate choices and SQLite configuration remain **Proposed/Open**; these decisions are not proof of any compiled Rust code or running SQLite DB.

## Security, budget and remote execution contracts (2026-10-21)

Six additional **Proposed** specifications were added: [budget reservation and reconciliation](../architecture/budget-reservation-reconciliation.md), [worker transport design](../architecture/worker-transport-design.md), [project identity and authorization](../architecture/identity-authorization.md), [GitHub App remote write security](../architecture/github-app-integration-design.md), [worker provider evaluation](../operations/worker-provider-evaluation.md), and [decision readiness](decision-readiness.md). They examine hidden cloud retries, SQLite admission, GitHub token/ref separation, authenticated human approvals and measurable provider cost. **No live provider configuration, production credentials or benchmark results have been created by this documentation work.** ADR 0006 (transport), 0007 (hosting) and 0008 (GitHub auth) remain Proposed; only Rust backend and SQLite database have been approved as technical choices.

## Core orchestration and delivery documentation (2026-10-21)

Ten new **Proposed** specifications were committed: [main orchestrator](../architecture/orchestrator.md), [hierarchical task model](../architecture/task-model.md), [planning and worker agent contracts](../architecture/agents.md), [human approval workflow](../architecture/approval-workflow.md), [Rust workspace design](../architecture/rust-workspace.md), [product vision](../product/vision.md), [product scope](../product/scope.md), [delivery roadmap](roadmap.md), [milestone evidence gates](milestones.md) and [technical glossary](../reference/glossary.md). The documents separate main-agent planning from Rust enforcement, attempt retries from new DAG tasks, approval requests from applied human approvals, technical demos from product MVP, and proposed delivery gates from actual implementation status.

**These additions are documentation only.** The backend is still Rust-only by approved ADR 0004 and SQLite-only by approved ADR 0005. No executable software, SQLite migrations, live worker, GitHub App, or UI is verified. Parent-group aggregation, activation policy, Rust crate choices, in-flight worker budget behavior, HTTP/worker transport, hosting, authentication and MVP boundaries are still Open/Proposed.

## Next documentation tasks
1. Recover all prior approved architectural detail, confirm sources, and convert it into topic-specific specifications and ADRs.
2. Resolve remaining open questions (Rust crates/tooling, SQLite configuration, worker transport/providers, identities and approvals, deployment boundaries and UI) before implementation depends on them; Rust and SQLite selection are settled.
3. Decompose acceptance criteria into implementation slices with measured performance limits and proof-of-completion checks; user journeys and criteria are drafted, not implemented.
4. Keep this file synchronized with actual commits, tests and releases.

**Rule:** Do not promote a planned item to implemented without a linked code artifact and verification evidence.
