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

Six additional **Proposed** specifications were added: [budget reservation and reconciliation](../architecture/budget-reservation-reconciliation.md), [worker transport design](../architecture/worker-transport-design.md), [project identity and authorization](../architecture/identity-authorization.md), [GitHub App remote write security](../architecture/github-app-integration-design.md), [worker provider evaluation](../operations/worker-provider-evaluation.md), and [decision readiness](decision-readiness.md). They examine hidden cloud retries, SQLite admission, GitHub token/ref separation, authenticated human approvals and measurable provider cost. **No live provider configuration, production credentials or benchmark results have been created by this documentation work.** At the time of that earlier checkpoint ADRs 0006–0008 were Proposed; **subsequently ADR 0006 (HTTPS) and ADR 0008 (GitHub App) were Approved**, along with ADR 0011 (multi-provider). ADR 0007 remains Proposed only for initial concrete providers.

## Core orchestration and delivery documentation (2026-10-21)

Ten new **Proposed** specifications were committed: [main orchestrator](../architecture/orchestrator.md), [hierarchical task model](../architecture/task-model.md), [planning and worker agent contracts](../architecture/agents.md), [human approval workflow](../architecture/approval-workflow.md), [Rust workspace design](../architecture/rust-workspace.md), [product vision](../product/vision.md), [product scope](../product/scope.md), [delivery roadmap](roadmap.md), [milestone evidence gates](milestones.md) and [technical glossary](../reference/glossary.md). The documents separate main-agent planning from Rust enforcement, attempt retries from new DAG tasks, approval requests from applied human approvals, technical demos from product MVP, and proposed delivery gates from actual implementation status.

**These additions are documentation only.** The backend is still Rust-only by approved ADR 0004 and SQLite-only by approved ADR 0005. No executable software, SQLite migrations, live worker, GitHub App, or UI is verified. Group-only parents and bounded-or-stop in-flight policy were **subsequently approved** in ADR 0009 and ADR 0010. Activation UX, optional-child semantics, Rust crate choices, provider-specific stop controls, HTTP/worker transport, hosting, authentication and MVP boundaries remain Open/Proposed.

## Activation, hierarchical aggregation and first Rust/SQLite work packages (2026-10-21)

Seven new documentation files were committed: [plan activation and versioning](../architecture/plan-activation-and-versioning.md), [parent task aggregation options](../architecture/parent-task-aggregation.md), [in-flight worker budget policy options](../architecture/inflight-cost-policy.md), [initial Rust and SQLite implementation task packages](../development/initial-implementation-task-packages.md), [Rust and SQLite verification matrix](../development/rust-sqlite-test-matrix.md), and initially Proposed [ADR 0009 — parent aggregation](../decisions/0009-parent-aggregation.md) and [ADR 0010 — in-flight worker budget](../decisions/0010-inflight-budget.md), **both subsequently Approved by the owner on 2026-10-21**.

These define candidate activation guards, distinct group vs dependency semantics, conservative billing choices, twelve future engineering packages (R00–R11) and 24 detailed future test cases (T-001–T-024). **No actual engineering task DAG has been activated; no backend implementation, migration, CI test or paid provider trial has been verified.** Rust-only backend and SQLite-only authoritative database remain approved. ADRs 0006–0008 and plan-version supersession/activation UX remain Proposed/Open. ADRs 0009–0010 are Approved; only their operational parameters remain to be specified and tested.

## Owner approval checkpoint — parent execution and in-flight spending (2026-10-21)

The owner explicitly approved [ADR 0009 — group-only non-executable parent tasks](../decisions/0009-parent-aggregation.md) and [ADR 0010 — bounded continuation of in-flight workers with stop-on-risk](../decisions/0010-inflight-budget.md). Detailed contracts now reflect verified required-child aggregation, explicit prerequisites, and provider-backed budget continuation or cancellation/reconciliation. The technical specs and test catalogue are updated to differentiate the **approved policies** from optional-child, actual cloud caps, cancellation latency and schema implementation details that are still Open. **No Rust code, SQLite migrations, cloud billing controls or worker tests are verified as implemented.**

## Remote transport, Git publication and hosting implementation contracts (2026-10-21)

Five new **Proposed** documentation artifacts have been committed: [worker wire contract v0](../architecture/worker-wire-contract-draft.md), [trusted remote Git publication contract](../architecture/git-remote-integration-contract.md), [remote worker provider lifecycle and cancellation](../operations/remote-worker-lifecycle-contract.md), [remaining transport/Git/hosting owner decision pack](transport-git-hosting-decision-pack.md), and [remote integration test matrix RI-001 through RI-030](../development/remote-integration-test-matrix.md).

The new drafts detail message identity, SQLite lease/outbox and duplicate delivery, GitHub token-vs-branch authorization, cloud auto-retry/idempotency, provider stop-on-risk and late cost reconciliation. Relevant GitHub, Cloud Run Jobs and AWS ECS official documents were checked as engineering context. **This earlier checkpoint is superseded:** ADR 0006 (HTTPS) and ADR 0008 (GitHub App) are now Approved, and ADR 0011 requires multi-provider infrastructure. ADR 0007 remains Proposed for first provider/region selection. No cloud jobs, GitHub App installation, Rust endpoints, real invoices or passing tests were created by this documentation work.

## Owner-approved HTTPS, GitHub App and multi-provider remote worker architecture (2026-10-21)

The project owner approved [ADR 0006 — remote-worker-initiated HTTPS control](../decisions/0006-worker-communication.md) and [ADR 0008 — GitHub App authentication](../decisions/0008-github-authentication.md). The owner additionally approved an **extensible multi-provider worker architecture** in new [ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md). A [Rust multi-provider adapter contract](../architecture/multi-provider-adapter-contract.md) proposes provider capability, launch, cancellation, cost accounting, worker identity and cross-provider attempt rules.

This **supersedes earlier chronological notes above** that described ADR 0006 or ADR 0008 as Proposed; those were accurate when written but are not the current decision status. **ADR 0007 stays Proposed only for first cloud vendors, regions and benchmark/evidence selection**, not for whether Rolvya may support multiple providers. No GitHub App installation, HTTPS API server, working provider adapter, live cloud benchmark, billed resource or Rust/SQLite implementation is verified. Next open decisions include concrete GitHub permissions and merge policy, worker enrollment/TLS/schema and initial provider rollout.

## Two-conversation documentation coverage audit and detailed gap remediation (2026-10-22)

An audit of **available** Rolvya historical chat excerpts/summaries, historical diagram descriptions and the GitHub repository is recorded in [two-conversation coverage audit](two-conversation-coverage-audit.md). **A full line-by-line export of both conversations was not accessible**, so total verbatim completeness is **not certified**. At the audit start the repository contained **90 tracked files: 89 Markdown plus LICENSE**. The documents did not constitute working Rust software.

The audit identified important under-specified or previously summary-only areas. Detailed **Proposed** documents were added for [Claude/Codex CLI vs API entitlement](../architecture/model-access-entitlement-contract.md), [multi-provider scheduler placement dimensions](../architecture/scheduler-selection-policy.md), [failure retry/escalation and nonbinding historical 1/3/10 minute examples](../architecture/retry-escalation-policy.md), [planner and worker token/context efficiency](../architecture/token-context-efficiency.md), [worked hypothetical 40-task cost scenarios](../architecture/cost-scenario-worked-examples.md), [brand/gold-palette/MIT correction provenance](../product/brand-history-license-governance.md), and [dated official provider access policy evidence](../reference/provider-cli-api-policy-evidence.md).

These additions preserve approved architecture but **do not finalize** CLI entitlement, subscription automation permission, retry counts, Spot/On-Demand fallback, scheduler weights, concrete cloud providers, logo source assets or unresolved technical parameters. Neither UI acceptance/Obsidian rendering nor live cloud/provider tests were performed. No original complete chat export or original logo master was available to this audit.

## Next documentation tasks
1. For **literal complete two-chat coverage**, acquire full original chat exports and any original architecture/logo assets, then compare every message against [the evidence matrix](two-conversation-coverage-audit.md). Continue keeping unrecoverable details explicitly unknown.
2. Resolve remaining open **implementation** choices (Rust crates/tooling, SQLite WAL/migrations, HTTPS schema/identity, exact GitHub App permissions/merge policy, first cloud provider rollout, subscription CLI entitlement, provider cost caps and UI) before dependent work. Rust, SQLite, HTTPS, GitHub App and multi-provider architecture selections are already Approved.
3. Decompose acceptance criteria into implementation slices with measured performance limits and proof-of-completion checks; user journeys and criteria are drafted, not implemented.
4. Keep this file synchronized with actual commits, tests and releases.

**Rule:** Do not promote a planned item to implemented without a linked code artifact and verification evidence.
