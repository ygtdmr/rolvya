# Project status

Current authoritative decision cross-check: 2026-10-25 (see current ADRs; earlier sections are chronological historical checkpoints).

## Phase
**Specification / repository foundation.** No executable Rolvya orchestration implementation is verified by this document.

## Verified repository artifacts
- `LICENSE` — MIT License text present on `main`.
- Root `README.md` — introductory project description and explicitly pre-implementation status.
- `AGENTS.md` and initial `docs/` foundation — repository guidance and planning documents, not implementation.

## Established project decisions
- Project name Rolvya; open-source AI coding-agent orchestration.
- GitHub repository `ygtdmr/rolvya` (public, `main`); current source license MIT.
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

An audit of **available** Rolvya historical chat excerpts/summaries, historical diagram descriptions and the GitHub repository is recorded in [two-conversation coverage audit](two-conversation-coverage-audit.md). **A full line-by-line export of both conversations was not accessible**, so total verbatim completeness is **not certified**. At the audit start the repository contained **82 tracked files: 81 Markdown plus LICENSE**. The documents did not constitute working Rust software.

The audit identified important under-specified or previously summary-only areas. Detailed **Proposed** documents were added for [Claude/Codex CLI vs API entitlement](../architecture/model-access-entitlement-contract.md), [multi-provider scheduler placement dimensions](../architecture/scheduler-selection-policy.md), [failure retry/escalation and nonbinding historical 1/3/10 minute examples](../architecture/retry-escalation-policy.md), [planner and worker token/context efficiency](../architecture/token-context-efficiency.md), [worked hypothetical 40-task cost scenarios](../architecture/cost-scenario-worked-examples.md), [brand and design provenance](../product/brand-history-license-governance.md), and [dated official provider access policy evidence](../reference/provider-cli-api-policy-evidence.md).

These additions preserve approved architecture but **do not finalize** CLI entitlement, subscription automation permission, retry counts, Spot/On-Demand fallback, scheduler weights, concrete cloud providers, logo source assets or unresolved technical parameters. Neither UI acceptance/Obsidian rendering nor live cloud/provider tests were performed. No original complete chat export or original logo master was available to this audit.

## Full current Markdown file-link structural check (2026-10-22)

At the end of the historical documentation remediation, current repo tree verification showed **90 tracked files: 89 Markdown plus LICENSE**. A static audit fetched all **89 Markdown files**, evaluated **1,214 file-relative Markdown link occurrences** against repository file targets, and found **no missing file targets** or odd Markdown code-fence delimiter counts. A missing YAML frontmatter in this status note is permitted under the project documentation policy. Fragment-anchor resolution, Obsidian GUI behavior, rendered Mermaid/YAML parsing, external URL health and actual software execution were **not tested**; do not equate structural source checks with those outcomes.

## Approved resumable storage migration policy — specification checkpoint (2026-10-22)

Owner approval extends [ADR 0030](../decisions/0030-project-activity-audit-log.md): historical storage migration must preserve durably verified partial progress, resume only unfinished/unverified approved objects after safe reconciliation, stop admitting billable work when approved headroom is insufficient, independently verify full manifest and trusted checkpoints before recording success, and **never automatically delete the original source**. Source cleanup remains a distinct strongly authenticated Owner-approved operation under retention and legal safeguards.

The new [storage migration state-machine document](../architecture/storage-migration-state-machine.md) is **Proposed technical implementation**; [AC-086–AC-098](../product/acceptance-criteria.md) are future acceptance scenarios. No actual provider transfers, Rust implementation, SQLite schema migration, automated tests or spend occurred in this documentation-only update.

## Approved bounded retry for historical storage migration — specification checkpoint (2026-10-22)

The Owner approved **finite automatic retry with bounded exponential backoff for transient migration-transfer failures only**, recorded in [ADR 0030](../decisions/0030-project-activity-audit-log.md) and the [Proposed storage migration state-machine design](../architecture/storage-migration-state-machine.md). Ambiguous provider outcomes must be reconciled before paid retries; security/authorization violations, independent integrity mismatches, exhausted retry limits and insufficient/unbounded approved exposure **must not** trigger automated transfer retries. Source copies and durably verified progress remain protected. Storage operation retry policy is distinct from the approved three-total-attempt default for AI coding workers.

[AC-099–AC-108](../product/acceptance-criteria.md) define **10 additional specified, unexecuted tests**; the acceptance catalogue now covers AC-001–AC-108. **Subsequently approved: 4 total storage-transfer attempts by default (1 initial + at most 3 automatic retries), distinct from the 3-total AI coding worker default.** **The 5/10/20-second base waits were subsequently approved**; jitter distribution, provider error mapping, storage-operation boundaries, Rust/SQLite implementation and CI remain **open/not implemented**. This work changed documentation only; no remote storage API calls, transfer costs, infrastructure deployments or executable verification tests were performed.

## Approved storage-transfer attempt ceiling — decision checkpoint (2026-10-22)

The Owner approved **4 total attempts per logical storage-transfer operation by default** (initial attempt plus up to 3 automatic retries) under [ADR 0030](../decisions/0030-project-activity-audit-log.md). After attempt 4 fails, a fifth automatic attempt is forbidden; safe durable counters and replay fencing must prevent restart bypass. [AC-109](../product/acceptance-criteria.md) explicitly specifies this boundary as a future test. The separate AI coding-worker default remains **3 total attempts**, unchanged. The **5/10/20-second base-delay cadence was subsequently Approved**, while exact jitter bounds and storage operation granularity remain unapproved. This is documentation-only and does not indicate implemented retry logic or executed tests.

## Approved storage retry backoff cadence — decision checkpoint (2026-10-22)

The Owner approved a **5 → 10 → 20 second base delay** before the three allowed automatic retries of a historical-storage transfer, under the previously approved **4 total attempts** ceiling. Small bounded jitter may adjust actual execution time; a provider's valid mandatory longer wait (for example `Retry-After`) takes priority. Neither the base delays nor elapsed wall-clock time bypass the project hard-budget, independent verification, provider permissions or original-source preservation safeguards. See [ADR 0030](../decisions/0030-project-activity-audit-log.md) and the [Proposed migration scheduler](../architecture/storage-migration-state-machine.md).

[AC-110–AC-112](../product/acceptance-criteria.md) are future acceptance scenarios, bringing the catalogue to **AC-001–AC-112**. The exact jitter distribution and bound, provider-header parsing, clock semantics and low-level scheduling remain **open/unimplemented**. No provider jobs, infrastructure, CI tests or billable transfers were executed as part of this documentation change.

## Next documentation tasks
1. For **literal complete two-chat coverage**, acquire full original chat exports and any original architecture/logo assets, then compare every message against [the evidence matrix](two-conversation-coverage-audit.md). Continue keeping unrecoverable details explicitly unknown.
2. Resolve remaining open **implementation** choices (Rust crates/tooling, SQLite WAL/migrations, HTTPS schema/identity, exact GitHub App permissions/merge policy, first cloud provider rollout, subscription CLI entitlement, provider cost caps and UI) before dependent work. Rust, SQLite, HTTPS, GitHub App and multi-provider architecture selections are already Approved.
3. Decompose acceptance criteria into implementation slices with measured performance limits and proof-of-completion checks; user journeys and criteria are drafted, not implemented.
4. Keep this file synchronized with actual commits, tests and releases.

**Rule:** Do not promote a planned item to implemented without a linked code artifact and verification evidence.

## Current project source license

The authoritative [`LICENSE`](../../LICENSE) is the **MIT License**. Documentation and source-release guidance must use SPDX identifier `MIT` and retain its copyright/permission notice in qualifying distributions. Third-party dependency notices and independent trademark/brand policy remain separate. This repository is still in the specification phase; no product implementation or release is verified by this license change.

## Gap closure before full architecture review (2026-10-23)

The Owner asked to resolve remaining gaps before a complete project walkthrough. The [gap closure and full review register](pre-implementation-gap-closure-review.md) now separates already approved decisions from unresolved P0/P1 engineering/security/product gates and outlines an eleven-part review sequence. This checkpoint corrected stale SQLite WAL/pool/busy settings and HTTPS/GitHub App approval-status references. **No new architecture policy was silently approved and no executable Rolvya code or CI tests were created.**

## Owner-approved v0.1 product feature boundary (2026-10-23)

The Owner approved the proposed first usable v0.1 scope: up-front immutable DAG planning and agent model/effort assignment; dependency-aware parallel remote workers; secure authorized model execution; GitHub App-backed remote Git and verified outputs; hard budget tracking with explicit Owner gates; project configuration; basic web panel; and essential authentication, audit and recovery safeguards. Mobile, voice assistant and optional advanced integrations are deferred. See [product scope](../product/scope.md) and [gap closure register](pre-implementation-gap-closure-review.md). Provider rollout subset, exact security/worker/DB contracts and performance figures remain open. **This is a product-scope decision only; no feature has been implemented or exercised.**

## Approved atomic task and budget admission (2026-10-23)

The Owner approved [ADR 0031](../decisions/0031-atomic-task-budget-admission.md), closing the **OQ-003 architecture decision**. A short SQLite `BEGIN IMMEDIATE` transaction must recheck executable leaf readiness, current budget and liabilities, then atomically create a same-task attempt, lease fencing generation, financial hold, audit and durable outbox before commit. No billable provider launch may happen before commit. [T-031–T-035](../development/rust-sqlite-test-matrix.md) specify additional future cases; **no Rust implementation, migration, running tests or billable work has occurred**.

## Owner-approved passwordless human authentication (2026-10-23)

[ADR 0032](../decisions/0032-passwordless-human-sign-in.md) approves **Google**, **GitHub**, and **verified-email one-time link/code** as v0.1 human sign-in. User passwords, password recovery and fallback are explicitly excluded. Existing OIDC/passkey and Owner recent strong reauthentication protections remain binding; email-only proof cannot confer high-risk Owner approvals. **OQ-004 remains P0 for broker/provider protocol, immutable identity linking, assurance, sessions and secure delivery/anti-abuse implementation.** No actual human auth provider or tests were deployed.

## Owner-revised human login and TOTP policy (2026-10-24)

[ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md) **supersedes the passwordless-only ADR 0032 selection**. Primary human sign-in choices are **email/password** and **email-verified Magic Link/OTP**; **app-based TOTP 2FA** is optional and SMS is not needed. At initial setup the Owner selects at least one primary method, and later may enable additional methods with strong reauthentication. Google/GitHub social sign-in is not part of the required v0.1 methods; remote repository GitHub App integration remains approved. Secure password hashing, email delivery, TOTP key storage and recovery, Owner method administration and acceptance tests remain **unimplemented**. Earlier dated passwordless notes are preserved as historical checkpoints, not active policy.

## Current documentation consistency checkpoint (2026-10-24)

The active sign-in source is [ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md): Owner-enabled password login or verified-email Magic Link/OTP, optional authenticator-app TOTP and no SMS. Earlier OIDC-only/passwordless-only mandates are superseded; [ADRs 0025–0027 and 0029](../decisions/README.md) preserve their security requirements while honoring stable internal account identity, member invites and ownership changes under these sign-in choices. Without configured/working email delivery, password-reset UX instructs the user to contact the instance administrator; account recovery still requires secure verified handling. [AC-125–127](../product/acceptance-criteria.md) add future regression checks.

**Already approved, not pending votes:** Rust/SQLite/Axum/SQLx, initial SQLite WAL / max four pool connections / five-second busy timeout; atomic `BEGIN IMMEDIATE` task/budget admission; worker HTTPS, 15-minute worker token, 30-second heartbeat, 90-second lease and default three total AI-worker attempts; backup every 15 minutes and daily points with seven-day frequent / thirty-day daily retention; first development adapter priorities **Amazon ECS on AWS Fargate**, Cloud Run Jobs and enrolled remote VPS (latest ADR 0041) (Hetzner replaced by ADR 0039); and Activity Log 90-day active view, one-year routine / minimum three-year critical retention. Exact schemas, provider risks, remaining security implementations, RPO/RTO and algorithms stay open. **This cleanup changes documentation only: no Rust code, CI runs, deployed login/worker, recovery procedure or budget integration has been verified.**

## Owner clarified TOTP enrollment is optional even for privileged operations (2026-10-24)

The Owner explicitly rejected requiring authenticator-app TOTP for budget increases, Owner transfers and other sensitive operations. [ADR 0033](../decisions/0033-configurable-password-email-login-and-totp.md) now states that **TOTP remains voluntary for all humans and all Owner operations**. Maintain fresh reauthentication, explicit server-side Owner authorization, financial hard gates, audit/alerts and risk checks; implement an appropriate non-TOTP route for Owners without enrolled 2FA rather than treating optional TOTP as mandatory. Never misrepresent fresh single-factor verification as multi-factor or phishing-resistant. [P0 implementation contract](../architecture/human-authentication-implementation-contract.md) tracks the still-open exact risk/step-up design; [AC-128–130](../product/acceptance-criteria.md) add planned regression cases. No runtime implementation or security tests were executed.

## Owner-approved scoped security incident handling (2026-10-24)

[ADR 0035](../decisions/0035-scoped-human-incident-containment.md) approves **targeted hold/deny of affected high-risk human actions**, revoking credibly stolen related human session proofs, preserving unrelated authorized activity and **not automatically cancelling independently approved remote workers** whose credentials/isolation and reserved cost caps remain enforceably safe. A separate worker/provider/budget compromise still triggers its own fenced stop-on-risk. No mandatory TOTP; evidence thresholds, alert workflow and administrator account recovery are still P0. [AC-140–144](../product/acceptance-criteria.md) are specified tests, not run.

## Approved transaction-bound Owner reauthentication (2026-10-24)

[ADR 0034](../decisions/0034-owner-action-bound-reauthentication.md) records the Owner's explicit approval for **fresh password re-entry or a new single-use transaction-bound email Magic Link/OTP**, display of exact changed values, explicit **single-operation** confirmation, server-side current Owner/session checks, atomic application and **hold/deny when credible compromise is detected**. TOTP remains entirely voluntary, including for Owner budget increases and transfers. The Owner **subsequently approved** the **5-minute** privileged-action proof window, **10-minute** email Magic Link/OTP validity, **5 unsuccessful email OTP attempts per challenge**, and **one-use** consumption; remaining anti-abuse/risk thresholds, bootstrap and recovery still need P0 decision and tests. [Engineering implementation sketch](../architecture/owner-non-totp-reauthentication-design.md) remains Proposed for those details. Nothing has been deployed or verified.

## Owner-approved multi-account CLI session architecture (2026-10-24)

[ADR 0038](../decisions/0038-multi-account-isolated-cli-and-api-access.md) adds explicit **simultaneous multiple Claude Code/Codex CLI accounts with genuine supported provider login and separate remote authentication/execution homes**, alongside separately selected API-key mode. This follows the user's Paperclip-like connection requirement without any personal-computer agent. Per-account authorization, protected grants, provider quotas and hard budget admission apply; no silent another-account or paid-API fallback. **Provider-specific third-party hosted subscription entitlement and concrete token-handling implementations remain OQ-008/P0**, not approved by the UI/design choice. [AC-145–149](../product/acceptance-criteria.md) are future tests, not executed.

## AWS worker provider priority revision — Owner approved 2026-10-24

[ADR 0039](../decisions/0039-aws-ecs-fargate-replaces-hetzner.md) changes **only priority 2** of the previously approved initial worker adapters from Hetzner Cloud to **Amazon ECS using AWS Fargate RunTask**. **Historical sequence in ADR 0039:** Google Cloud Run Jobs, ECS on Fargate, user-enrolled remote VPS; **latest order superseded by ADR 0041**. AWS Batch is not chosen because Rolvya already owns scheduling, retries and budget approvals; EC2 remains a future eligible adapter for tasks incompatible with Fargate privileged/container limits. [Proposed staged rollout](../architecture/initial-remote-worker-rollout-proposal.md) remains Proposed for release sequencing. **No paid AWS resources, benchmarks or deploys were started**; exact region, size, image, IAM and measured upper cost remain P1 OQ-010.

## First-working single real-provider rollout — Owner approved 2026-10-24

[ADR 0040](../decisions/0040-single-provider-first-working-version.md) approves fake-worker correctness testing first, followed by **Google Cloud Run Jobs** as the originally chosen first real adapter; **that provider order was superseded by ADR 0041**. The current order is AWS Fargate first, then Cloud Run Jobs, then remote VPS. This preserves the approved multi-provider architecture and does not independently change approved public v0.1 features, guarantee production safety or authorize cloud charges. Region, provider billing bounds, third-party CLI/API legal eligibility and public release gates still require separate validation. New acceptance scenarios [AC-150–152](../product/acceptance-criteria.md) remain unexecuted. No backend code or deployment was created by the documentation update.

## Latest Owner change — AWS Fargate first real provider (2026-10-24)

[ADR 0041](../decisions/0041-aws-fargate-first-real-worker.md) supersedes the earlier Google-first development order referenced in historical status entries: **fake providers first; Amazon ECS on AWS Fargate first real integration; Google Cloud Run Jobs second; user-enrolled remote VPS third**. The [ADR 0040](../decisions/0040-single-provider-first-working-version.md) one-real-provider initial working version principle remains approved. This is a development/adapter priority and does not approve AWS spending, deployment, public production certification, provider-specific CLI rights, region, task sizing or other implementation thresholds. The 3 cloud/remote adapters remain development goals. No new provider jobs or code have been run.

## Owner-approved first-version panel navigation (2026-10-25)

[ADR 0042](../decisions/0042-five-section-web-panel-navigation.md) approves exactly five high-level web panel navigation sections: **Projects**, **Execution**, **Costs & Approvals**, **Connections**, and **Results & Activity**. The [panel specification](../product/initial-web-panel-structure.md) marks the navigation approved while keeping detailed screen UX, frontend framework, live updates and initial plan-activation consent as Proposed/Open **OQ-013/P1**. No web panel code, UI test, deployed service or additional AWS spending was created or verified by this documentation change.

## Web panel implementation stack candidate — Proposed 2026-10-25

The [web panel technical stack proposal](../architecture/web-panel-technical-stack-proposal.md) compares a static React + TypeScript/Vite frontend, React Flow for **read-only dependency DAG visualization** alongside a distinct accessible task hierarchy, and REST plus Server-Sent Events through the already approved Rust Axum + SQLite/SQLx backend. **The technology selection is NOT Owner-approved.** It remains OQ-013/P1; no web application, Node runtime service, backend endpoint, graph test, UI benchmark or deployment was created, and no cloud spend was authorized.

## Owner-approved one-bundle browser frontend and UX boundary (2026-10-25)

[ADR 0043](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md) approves React + TypeScript/Vite with React Flow for the **read-only activated dependency DAG**, separate accessible task hierarchy and Rust Axum REST/SSE. Production output requires **exactly one minified application JavaScript asset containing compiled CSS**, with a **separate `sw.js`** for Service Worker and external manifest/icons/locales assets as applicable; no embedded fonts or icons. The [browser output, PWA, localization and browser capability catalog](../architecture/web-platform-artifact-and-capability-contract.md) and [UX/accessibility contract](../product/web-ux-and-accessibility-contract.md) specify future verification. **No font family, icon provider or final UI aesthetic was selected; exact design remains open under OQ-013**. This later approval **supersedes the preceding 2026-10-25 Proposed stack checkpoint**. No application code, one-JS build, asset files, UI test, browser certification or AWS deployment has been created or verified; no new spending was authorized.

## Frontend source layout and build verification plan (2026-10-25)

A [Proposed F0–F4 implementation plan](../development/web-frontend-source-and-build-plan.md) now identifies a future `apps/web/` modular React/Vite source layout, explicit `?inline` CSS assembly with React Flow stylesheet, current Rolldown one-JS chunk control, separate `sw.js`, CSP nonce/hash verification and a final `dist/` artifact inspector. F0–F1 are the suggested first implementation slice; later fake data, Rust API integration and browser/PWA tests follow. **No `apps/web` directory, bundle, CI workflow, browser test result or production deployment was created or verified by this document.** UI fonts, icon provider, component visuals and final layout remain undecided; approved ADR 0043 requirements remain binding.

## Next P0 decision pack — Secret Broker key custody (2026-10-25)

A [Proposed](../architecture/secret-storage-key-custody-decision-proposal.md) decision pack compares a managed secret service with envelope-encrypted SQLite plus separate root key custody, preserving the already-approved central Rust Secret Broker (ADR 0037), multiple isolated CLI/API connections (ADR 0038), worker lease checks and hard budgets. **The actual storage provider, encryption key location and security acceptance are NOT Owner-approved or implemented.** No accounts were connected, secrets stored or billable services started.

## Secret Broker initial storage selected — Owner approved 2026-10-25

[ADR 0044](../decisions/0044-encrypted-sqlite-secret-storage.md) approves **Rust-brokered, application-layer authenticated-encrypted credential records in the existing authoritative SQLite database** for the first Rolvya implementation. AWS Secrets Manager and external Vault are not initially required. **Root/master keys must be protected outside SQLite and its ordinary backups**, while **their concrete custody/provisioning/recovery** and provider worker identity/cryptographic implementation remain **OQ-005/P0**. A ciphertext-only SQLite backup does not imply full-database encryption or protection if the live host/root key is compromised. [Storage/key-custody follow-up](../architecture/secret-storage-key-custody-decision-proposal.md) distinguishes resolved choice from open details. This is an **approved design, not built or tested code**; no key was generated or stored, and no secret manager or cloud charge was initiated.

## Secret Broker automatic master-key loading — Owner approved 2026-10-25

[ADR 0045](../decisions/0045-automatic-local-master-key-unlock.md) approves **unattended restart** using a separate, access-restricted master/key-encryption file on the **remote controller host** for encrypted SQLite credential records (ADR 0044). Normal startup does not ask the Owner to manually unseal the key; invalid/missing key must block secret use without regenerating it or destroying encrypted rows. Owner-only OS file permissions and separation from ordinary DB backups are mandatory. **Exact first-run setup, crypto/key rotation, separately safeguarded recovery and remote worker identity remain OQ-005/P0** and unimplemented. A controller-host compromise that exposes the live key remains a residual risk. No actual key file, ciphertext, test, deploy or paid cloud resource was created.

## Key creation, rotation and emergency recovery — Proposed 2026-10-25

The [first-time setup, staged rotation and encrypted off-host recovery-kit design](../architecture/secret-key-bootstrap-rotation-recovery-proposal.md) is documented for Owner review. The existing approval remains **encrypted SQLite secrets (ADR 0044)** and **automatic ordinary startup from the controller-host protected key file (ADR 0045)**. One-time CSPRNG generation with guarded initialization, version-aware re-encryption and an independently safeguarded, encrypted off-host disaster recovery kit are **recommended options, NOT yet Owner-approved**. Key-file implementation and exact crypto/recovery custody remain **OQ-005/P0**. No source files, backup, cryptographic material, live provider connection, paid resource or restore test was created.

## Emergency encrypted key recovery approved (2026-10-25)

[ADR 0046](../decisions/0046-off-host-encrypted-master-key-recovery.md) approves an encrypted off-controller-host recovery kit, stored separately from the SQLite backup, protected by a separately retained recovery secret. Ordinary startup stays automatic under ADR 0045. Exact provider, encryption parameters, initial setup, rotation and verified restore remain OQ-005/P0. This is documentation only: no recovery kit, encryption code or test was created.
