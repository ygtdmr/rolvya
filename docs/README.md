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

- [Two-conversation decision coverage audit (2026-10-22)](project/two-conversation-coverage-audit.md)

- [Historical conversation-to-document audit](project/historical-decision-audit.md)
- [Brand, repository and MIT positioning](product/brand-positioning.md)
- [CLI/API mode constraints](architecture/model-access.md)
- [Infrastructure candidates and historical cost examples](operations/infrastructure-options.md)

## Product and quality specifications now available

- [Product vision and success principles](product/vision.md)
- [Brand evolution and asset provenance](product/brand-history-license-governance.md)
- [Product scope, exclusions and MVP candidates](product/scope.md)

- [Product requirements](product/requirements.md)
- [Acceptance criteria](product/acceptance-criteria.md)
- [Owner panel and user journeys](product/user-flows.md)
- [Owner-approved five-section web panel navigation and proposed screen details](product/initial-web-panel-structure.md)
- [ADR 0042 — Approved web panel navigation](decisions/0042-five-section-web-panel-navigation.md)
- [ADR 0048 — Owner-delegated unified UI design, motion and elements](decisions/0048-owner-delegated-cohesive-ui-design.md)
- [Connected Control — UI design tokens, UI Elements catalog, animations, modern CSS, responsive quality and validation](product/connected-control-design-system.md)
- [Approved frontend core stack — React/Vite, DAG and Axum/SSE](architecture/web-panel-technical-stack-proposal.md)
- [ADR 0044 — Approved encrypted SQLite Secret Broker storage](decisions/0044-encrypted-sqlite-secret-storage.md)
- [ADR 0045 — Approved automatic protected local key loading](decisions/0045-automatic-local-master-key-unlock.md)
- [ADR 0046 — Approved encrypted off-host emergency recovery](decisions/0046-off-host-encrypted-master-key-recovery.md)
- [Encrypted SQLite, automatic local key and remaining recovery/rotation details](architecture/secret-storage-key-custody-decision-proposal.md)
- [Approved emergency recovery and proposed key-bootstrap/rotation implementation](architecture/secret-key-bootstrap-rotation-recovery-proposal.md)
- [ADR 0047 — Approved XChaCha20-Poly1305, 256-bit KEK and per-secret DEKs](decisions/0047-secret-broker-xchacha20-key-hierarchy.md)
- [Cryptography and first-run bootstrap contract — approved baseline, implementation open](architecture/secret-broker-cryptography-bootstrap-proposal.md)
- [Approved ADR 0043 — Single application JS and browser platform boundary](decisions/0043-single-bundle-browser-platform-and-ux-boundary.md)
- [Browser platform asset checklist and production build verification](architecture/web-platform-artifact-and-capability-contract.md)
- [UX and accessibility contract — without selected visual design](product/web-ux-and-accessibility-contract.md)
- [Testing and verification strategy](development/testing.md)
- [Frontend source structure and single-application-JS build gates (proposed implementation plan)](development/web-frontend-source-and-build-plan.md)

## Architecture specifications now available

- [System overview](architecture/overview.md)
- [Main orchestrator responsibilities](architecture/orchestrator.md)
- [Retry escalation and recovery policy](architecture/retry-escalation-policy.md)
- [Plan activation and version boundaries](architecture/plan-activation-and-versioning.md)
- [Approved group-parent aggregation implementation contract](architecture/parent-task-aggregation.md)
- [Hierarchy and task/attempt model](architecture/task-model.md)
- [Planning and execution agent contracts](architecture/agents.md)
- [DAG planning](architecture/dag-planning.md)
- [Execution lifecycle](architecture/execution.md)
- [Scheduler state machine](architecture/scheduler.md)
- [Multi-provider scheduler eligibility and selection](architecture/scheduler-selection-policy.md)
- [Worker control protocol](architecture/worker-protocol.md)
- [Remote worker transport design](architecture/worker-transport-design.md)
- [Multi-provider Rust worker adapter contract](architecture/multi-provider-adapter-contract.md)
- [Approved one-provider first-working development sequence](decisions/0040-single-provider-first-working-version.md)
- [Staged rollout: AWS Fargate first, then Cloud Run Jobs and remote VPS](architecture/initial-remote-worker-rollout-proposal.md)
- [Approved ADR 0041 — AWS ECS Fargate as first real worker](decisions/0041-aws-fargate-first-real-worker.md)
- [Approved AWS ECS Fargate substitution for Hetzner](decisions/0039-aws-ecs-fargate-replaces-hetzner.md)
- [Proposed worker wire contract v0](architecture/worker-wire-contract-draft.md)
- [Failure classification and recovery](architecture/failure-recovery.md)
- [Resumable historical storage migration state machine — proposed](architecture/storage-migration-state-machine.md)
- [Monitoring and audit architecture](architecture/observability.md)
- [Remote workers and models](architecture/workers.md)
- [Budget and costs](architecture/budget-costs.md)
- [Budget reservation and reconciliation](architecture/budget-reservation-reconciliation.md)
- [Worked three-scenario, four-category cost examples](architecture/cost-scenario-worked-examples.md)
- [Approved bounded in-flight worker policy contract](architecture/inflight-cost-policy.md)
- [Security model](architecture/security.md)
- [Project identity and authorization](architecture/identity-authorization.md)
- [Human approval state machine](architecture/approval-workflow.md)
- [Remote GitHub integration](architecture/integration-github.md)
- [GitHub App security design](architecture/github-app-integration-design.md)
- [Remote Git publication contract](architecture/git-remote-integration-contract.md)
- [Persistence and data model](architecture/data-model.md)
- [Rust + SQLite backend contract](architecture/rust-sqlite-backend.md)
- [Proposed Rust workspace boundaries](architecture/rust-workspace.md)
- [SQLite schema and atomic admission draft](architecture/sqlite-schema-draft.md)
- [Model entitlement and CLI/API access](architecture/model-access-entitlement-contract.md)
- [Approved multi-account isolated CLI and API access](decisions/0038-multi-account-isolated-cli-and-api-access.md)
- [Agent context and memory](architecture/context-memory.md)
- [Planner and worker token efficiency](architecture/token-context-efficiency.md)
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

- [Pre-implementation gap closure and full-project review](project/pre-implementation-gap-closure-review.md) — Live P0/P1 decision queue and end-to-end review checklist.

## Engineering and agent execution specifications

- [S2-04 verified worker bootstrap and scoped credentials](development/verified-worker-bootstrap.md)
- [S2-05 provider/account entitlement authority](development/provider-account-entitlements.md)
- [S2-06A trusted worker HTTPS transport](development/worker-https-transport.md)
- [S2-03C project authorization and exact-action Owner approvals](development/human-owner-authorization.md)
- [S2-03B human sessions and voluntary TOTP](development/human-sessions-totp.md)
- [S2-02 encrypted SQLite Secret Broker](development/encrypted-secret-broker.md)
- [S2-01 guarded controller key initialization](development/controller-key-bootstrap.md)
- [R11 independent core verification, release gates and Stage 2 handoff](development/core-verification-handoff.md)

- [R10 executable fault injection and resilience evidence](development/resilience-fault-injection.md)

- [R09 synthetic DAG orchestration and independent artifact validation](development/synthetic-dag-orchestration.md)

- [R08 durable synthetic dispatch, lease renewal and crash recovery](development/durable-worker-recovery.md)

- [Engineering standards](development/standards.md)
- [AI agent handoff contract](development/agent-handoffs.md)
- [Review and ADR governance](development/review-process.md)
- [Repository CI, routine PR integration and one-time activation](development/pull-request-integration.md)
- [Initial Rust + SQLite implementation task packages](development/initial-implementation-task-packages.md)
- [Rust + SQLite core test matrix](development/rust-sqlite-test-matrix.md)
- [Remote integration verification matrix](development/remote-integration-test-matrix.md)
- [Closed decision map and outstanding implementation/evidence gates](project/open-questions.md)

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
- [Remote worker provisioning and billing lifecycle](operations/remote-worker-lifecycle-contract.md)

## Architecture evaluation and implementation planning

- [Approved ADR 0004 — Rust-only backend](decisions/0004-control-plane-language.md)
- [Approved ADR 0005 — SQLite database](decisions/0005-authoritative-database.md)
- [Approved ADR 0006 — HTTPS worker communication](decisions/0006-worker-communication.md)
- [Historical hosting options (AWS-first settled by ADR 0041)](decisions/0007-remote-worker-hosting.md)
- [Approved ADR 0008 — GitHub App authentication](decisions/0008-github-authentication.md)
- [Approved ADR 0009 — Group-only parent tasks](decisions/0009-parent-aggregation.md)
- [Approved ADR 0010 — Bounded worker continuation or stop-on-risk](decisions/0010-inflight-budget.md)
- [Approved ADR 0011 — Extensible multi-provider worker infrastructure](decisions/0011-multi-provider-worker-infrastructure.md)
- [Evidence-driven implementation slices](project/implementation-slices.md)
- [Pre-implementation decision readiness](project/decision-readiness.md)
- [Transport, Git and hosting owner decision pack](project/transport-git-hosting-decision-pack.md)
- [Product delivery roadmap](project/roadmap.md)
- [Milestone definitions and evidence gates](project/milestones.md)
- [Architecture glossary](reference/glossary.md)
- [Current Claude/Codex CLI and API terms evidence](reference/provider-cli-api-policy-evidence.md)

**Approved 2026-10-21:** ADR 0004 (Rust), 0005 (SQLite), 0006 (worker HTTPS), 0008 (GitHub App), 0009 (group-only parents), 0010 (bounded-or-stop) and 0011 (multi-provider worker adapters). **AWS Fargate-first is settled by ADR 0041**; earlier ADR 0007 is historical cost/vendor exploration. Concrete IAM, cost and protocol tests are unfinished implementation, not open architectural selection. See the [closed-decision map and remaining evidence gates](project/open-questions.md).

## Project event history and audit

- [Resumable migration and crash-safe recovery design](architecture/storage-migration-state-machine.md) — Approved safeguards, Proposed implementation.
- [ADR 0030 — Project Activity Log and protected audit history](decisions/0030-project-activity-audit-log.md) — Approved policy, not implemented.
- [Observability and event architecture](architecture/observability.md) — Proposed implementation details.

## Human authentication and session safety

- [P0 human authentication, recovery and Owner step-up implementation proposal](architecture/human-authentication-implementation-contract.md) — **Proposed** contract with owner-approval gates; no runtime implementation.
- [Owner privileged reauthentication without mandatory TOTP](architecture/owner-non-totp-reauthentication-design.md) — **ADR 0034 approves the core flow**; duration, risk, schema and recovery implementation remain Proposed.
- [ADR 0034 — Action-bound Owner reauthentication](decisions/0034-owner-action-bound-reauthentication.md) — **Approved**, including no mandatory TOTP and no automatic suspicious-action approval.
- [ADR 0035 — Scoped suspicious-action containment and worker continuity](decisions/0035-scoped-human-incident-containment.md) — **Approved**; implementation/risk thresholds remain P0.

- [ADR 0033 — Current login methods: password/email + authenticator-app TOTP](decisions/0033-configurable-password-email-login-and-totp.md) — Approved, superseding initial-login rules in ADR 0032 and earlier OIDC-only wording.
- [ADR 0025 — Conditional federation and passkey safeguards](decisions/0025-oidc-passkey-authentication.md) — Approved where OIDC is separately enabled; not required for v0.1.
- [ADR 0026 — Secure account recovery, including current password-reset rules](decisions/0026-account-recovery.md) — Approved as reconciled with ADR 0033.
- [ADR 0027 — Low-friction risk-based web sessions](decisions/0027-risk-based-session-management.md) — Approved; possible mobile/voice clients **not** committed.
- [ADR 0028 — Human session credential rotation, replay defense and revocation](decisions/0028-human-session-token-safety.md) — Approved security requirements; implementation not built.
- [Proposed session rotation and theft-response implementation](architecture/session-token-rotation-and-theft-response.md) — Proposed, not implemented.
- [Product scope: future ideas are not commitments](product/scope.md).

## Recorded architecture decisions

- [ADR 0001 — Fixed upfront DAG](decisions/0001-fixed-upfront-dag.md)
- [ADR 0002 — Hard budget approval](decisions/0002-hard-budget-approval.md)
- [ADR 0003 — Remote Git only](decisions/0003-remote-git-only.md)
- [ADR 0021 — Scheduler modes: economical, balanced (default) and performance](decisions/0021-scheduler-selection-modes.md)
- [ADR 0022 — Dynamic worker parallelism and optional preflight cap](decisions/0022-dynamic-worker-concurrency.md)
- [ADR 0023 — Cross-project fair scheduling, priority and aging](decisions/0023-cross-project-scheduling-fairness.md)
- [ADR 0024 — Owner, Editor and Viewer human roles](decisions/0024-project-human-roles.md)
- [ADR 0029 — Owner-only project invitations and secure ownership transfer](decisions/0029-owner-controlled-project-invitations.md)

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
