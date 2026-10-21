---
title: Rolvya Two-Conversation Historical Requirements Coverage Audit
status: Proposed
category: project
last-reviewed: 2026-10-22
tags: [audit, traceability, history, conversations, documentation]
---

# Rolvya two-conversation historical requirements coverage audit

## Executive result

**Conclusion: The Rolvya repository records the core owner-approved architecture well, but it is not defensible to claim that every single detail from both chats was already documented or that every original message has been inspected.** This pass identified summary-only topics, superseded decisions that needed clearer chronology, gaps in detailed CLI/API eligibility, scheduler selection, example cost calculations, retry/escalation and token-efficiency tactics, plus outstanding brand asset/legal provenance and concrete implementation choices. New documents were added for those gaps.

**Source access limitation:** The relevant conversations are the original **Rolvya Main Conversation** (project naming/design/architecture and initial repository establishment, approximately 2026-10-19–20) and the later **Rolvya project documentation continuation** (2026-10-20–22). This audit can use conversation content currently present, retained recoverable conversation summaries and facts from prior exchanges, a historical audit already in GitHub, and descriptions of previously supplied architecture diagrams. **The original complete, line-by-line transcripts of both conversations were not retrievable in this audit**. Accordingly **no 100%-complete beginning-to-end transcript verification is claimed**. A literal exhaustive audit would additionally require the user's chat exports/transcripts and any original diagram assets.

**Repository baseline examined:** at the start of this audit, the live `main` Git tree contained **90 tracked files: 89 Markdown files plus LICENSE**. These are specifications, not a compiled backend or running workers. GitHub remained the source of truth. The current body of work adds further Markdown specs and corrects existing references, while keeping source-code/deployment verification explicitly absent.

## Evidence and precedence

- **E — Owner-explicit:** user's direct accepted choice or prohibition preserved in retrievable conversation context and/or an Approved ADR. Binding.
- **H — Historical preference:** a specific user request discussed prior to current finalized decisions. Preserve but do not automatically elevate to a universal runtime rule.
- **D — Diagram/explored alternative:** examples from earlier diagrams or assistant comparisons, sometimes including prices/timers/model names. Record as *illustrative*, not binding.
- **P — Implementation proposal:** structured technical design to be implemented/tested/approved later, not automatically an owner choice.
- **V — Verified implementation:** would require code and reproduced tests. **No V claims** were established by this documentation audit.

Latest explicit owner decision supersedes incompatible earlier creative/technical variants. **Approved ADR** is design authority, not evidence a feature shipped. Use [decision register](../decisions/README.md) and [project status](status.md) for current statuses, not an older dated checkpoint.

## 1. Project identity, delivery and brand coverage

| ID | Recovered historical subject / decision | Evidence | Current canonical coverage | Audit assessment |
| --- | --- | --- | --- | --- |
| B-01 | Name **Rolvya**, coined word, no asserted literal meaning | E | [Brand](../product/brand-positioning.md), [evolution](../product/brand-history-license-governance.md) | Covered |
| B-02 | Earlier names Dagyro and Velnqo declined | H | [Brand evolution](../product/brand-history-license-governance.md) | **Added historical rejection** |
| B-03 | GitHub/X name-conflict search prioritized; no blanket verified trademark/domain ownership | H | [Brand evolution](../product/brand-history-license-governance.md) | **Added caveat** |
| B-04 | Old-money/elegant/golden-ratio design explorations | H/D | [Brand evolution](../product/brand-history-license-governance.md) | **Added chronology; not current mandate** |
| B-05 | Interim gold/brass palette and previously listed HEX values | H | [Brand evolution](../product/brand-history-license-governance.md) | **Added as superseded only** |
| B-06 | Final no-gold, wordless geometric mark; navy/light-blue/gray; no redesign | E | [Current brand](../product/brand-positioning.md), [evolution](../product/brand-history-license-governance.md) | Covered; exact final HEX/logo master **not verified** |
| B-07 | Accepted short description and slogan; refrain from repeating slogan in root README | E | [Brand](../product/brand-positioning.md), [README](../../README.md) | Covered |
| B-08 | Public `ygtdmr/rolvya`, `main`, initially personal account | E | [README](../../README.md), [decision register](../decisions/README.md) | Covered |
| B-09 | Future organization/company/trademark considered, not authorized migration | H | [Brand](../product/brand-positioning.md), [evolution](../product/brand-history-license-governance.md) | Covered as open |
| B-10 | Commercial competitor concern; discussed MIT/Apache/MIT | E/H | [Brand evolution](../product/brand-history-license-governance.md) | **Added detailed history** |
| B-11 | Approved **GNU MIT**; earlier assistant's **MIT** mention was a mistake | E | [LICENSE](../../LICENSE), [brand evolution](../product/brand-history-license-governance.md) | Covered, explicit correction |
| B-12 | User will not manually code; AI coding agents will implement under human decisions | E | [AGENTS](../../AGENTS.md), [standards](../development/standards.md) | Covered |
| B-13 | README/docs in English and compatible with GitHub and Obsidian | E | [Docs policy](../development/documentation-policy.md) | Covered; **Obsidian UI not tested** |

## 2. Planning, tasks, agent behavior and recovery

| ID | Historical discussion or approved behavior | Evidence | Current documentation | Audit assessment |
| --- | --- | --- | --- | --- |
| A-01 | **One main agent** creates entire plan before execution | E | [ADR 0001](../decisions/0001-fixed-upfront-dag.md), [orchestrator](../architecture/orchestrator.md) | Covered |
| A-02 | Arbitrary-depth hierarchy and distinct explicit dependency DAG | E | [Task model](../architecture/task-model.md), [DAG](../architecture/dag-planning.md) | Covered |
| A-03 | All task IDs and dependency edges fixed at activation; no runtime new tasks | E | [Activation](../architecture/plan-activation-and-versioning.md), [ADR 0001](../decisions/0001-fixed-upfront-dag.md) | Covered |
| A-04 | Planner preassigns task model/effort/instructions/acceptance | E | [Agents](../architecture/agents.md), [task model](../architecture/task-model.md) | Covered |
| A-05 | Independent ready tasks run in parallel; dependent tasks wait | E | [Scheduler](../architecture/scheduler.md) | Covered |
| A-06 | Parent only groups/rolls up verified descendants; executable leaves only | E | [Approved ADR 0009](../decisions/0009-parent-aggregation.md) | Covered |
| A-07 | Workers only execute, may run code/tests/commands on remote hosts; no subtask delegation | E | [Agent contracts](../architecture/agents.md), [worker protocol](../architecture/worker-protocol.md) | Covered |
| A-08 | Main agent diagnoses failures, can propose new attempt/model/effort/worker | E | [Recovery](../architecture/failure-recovery.md), [retry policy](../architecture/retry-escalation-policy.md) | Covered; exact retry limits open |
| A-09 | No existing-task retry may mutate original DAG; preserve successful siblings | E | [Activation](../architecture/plan-activation-and-versioning.md), [recovery](../architecture/retry-escalation-policy.md) | Covered |
| A-10 | Older diagrams' three attempts and example 1/3/10-minute backoff | D | [Retry history and nonbinding examples](../architecture/retry-escalation-policy.md) | **Added; NOT approved constants** |
| A-11 | Retry may scale model or worker CPU/RAM or switch authorized cloud | E/D | [Retry policy](../architecture/retry-escalation-policy.md), [scheduler selection](../architecture/scheduler-selection-policy.md) | Detailed; fallback criteria open |
| A-12 | Completion requires independent verification, not worker exit status | E/P | [Agent handoffs](../development/agent-handoffs.md), [validator](../architecture/orchestrator.md) | Covered |
| A-13 | New requirements requiring extra tasks trigger separate plan-version process | E/P | [Activation/versioning](../architecture/plan-activation-and-versioning.md) | **Version supersession UX still open** |
| A-14 | Detailed handoff preserves work, source refs, tests, logs and attempts | E/P | [Agent handoffs](../development/agent-handoffs.md) | Covered |

## 3. Model and token economy

| ID | Historical subject | Evidence | Current documentation | Audit assessment |
| --- | --- | --- | --- | --- |
| M-01 | Both **Claude Code CLI** and **Codex CLI** discussed as interfaces | H | [Model access](../architecture/model-access.md), [entitlement](../architecture/model-access-entitlement-contract.md) | Covered |
| M-02 | Historical preference: subscription-only/no API key → Claude CLI **where eligible** | H | [Entitlement contract](../architecture/model-access-entitlement-contract.md) | **Expanded with decision tree** |
| M-03 | API key present → API-key mode, subject to authorization and selected model | H | [Entitlement contract](../architecture/model-access-entitlement-contract.md) | Detailed; mixed-mode precedence open |
| M-04 | Unattended/cloud worker subscription rights not assumed | P / external policy | [Official policy evidence](../reference/provider-cli-api-policy-evidence.md) | **Added dated official reference** |
| M-05 | Per-task model and effort examples: Claude Opus/Sonnet/Haiku, Codex 0.1 low/medium | D | [Model access](../architecture/model-access.md), [infra history](../operations/infrastructure-options.md) | Models examples only, not guaranteed catalogs |
| M-06 | Keep prompts moderate length and worker tasks ~10 minutes | H | [Token efficiency](../architecture/token-context-efficiency.md) | Covered as target, not SLA |
| M-07 | Save tokens in main-agent upfront planning, not just worker execution | E/H | [Token efficiency](../architecture/token-context-efficiency.md) | **Expanded significantly** |
| M-08 | Save tokens during worker runtime via compact memory, handoffs, referenced artifacts | E/H | [Token efficiency](../architecture/token-context-efficiency.md), [context](../architecture/context-memory.md) | Detailed; exact original approved checklist not fully retrievable |
| M-09 | No silent unapproved model/provider/effort or subscription→paid API fallback | E/P | [Entitlement contract](../architecture/model-access-entitlement-contract.md), [agent contracts](../architecture/agents.md) | Covered |
| M-10 | Track token usage and cost without conflating subscription with API token billing | E/P | [Cost examples](../architecture/cost-scenario-worked-examples.md), [provider evidence](../reference/provider-cli-api-policy-evidence.md) | Expanded |

## 4. Worker infrastructure, scheduler and Git

| ID | Historical subject | Evidence | Canonical documentation | Audit assessment |
| --- | --- | --- | --- | --- |
| I-01 | Control server remote, 2–4 GB RAM suggested | E/H | [Infrastructure](../operations/infrastructure-options.md), [deployment](../operations/deployment.md) | Covered, **not measured** |
| I-02 | High independent-task concurrency; initial thought 10 Codex + 10 Claude on 4 GB | H/D | [Infrastructure](../operations/infrastructure-options.md) | Example only, not supported limit |
| I-03 | Third-party cloud workers and user-supplied remote host credentials/SSH/API paths | E/H | [Approved multi-provider ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md), [adapter contract](../architecture/multi-provider-adapter-contract.md) | Covered, exact host enrollment open |
| I-04 | **No personal-computer Local Agent, Local Sync or local Git** | E | [ADR 0003](../decisions/0003-remote-git-only.md) | Covered |
| I-05 | Remote Git including self-hosted providers, GitHub initial integration | E/H | [Remote Git](../architecture/integration-github.md) | Covered; non-GitHub auth open |
| I-06 | Approved GitHub **App** and trusted Rust branch publishing | E | [ADR 0008](../decisions/0008-github-authentication.md), [Git contract](../architecture/git-remote-integration-contract.md) | Covered; no App installed |
| I-07 | Worker-to-controller **HTTPS pull/claim**, progress and result | E | [ADR 0006](../decisions/0006-worker-communication.md), [wire draft](../architecture/worker-wire-contract-draft.md) | Covered; no endpoint implemented |
| I-08 | Multi-cloud support, do not hardcode single vendor | E | [ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md) | Covered; concrete adapters unimplemented |
| I-09 | Cloud Run/Fargate/remote VPS, spot/on-demand variants | D/H | [Provider evaluation](../operations/worker-provider-evaluation.md), [infrastructure history](../operations/infrastructure-options.md) | Recorded as candidates |
| I-10 | Scheduler considers priority, duration, CPU/RAM, model, capacity, cost, region/latency | D/H | [Selection policy](../architecture/scheduler-selection-policy.md) | **Expanded with hard eligibility gates** |
| I-11 | Spot→On-Demand fallback appears in earlier graphic | D | [Selection policy](../architecture/scheduler-selection-policy.md) | **Marked unapproved automatic fallback** |
| I-12 | Earlier 20-worker cap and illustration $3.33/day | D | [Infrastructure history](../operations/infrastructure-options.md) | Recorded, never actual quote/limit |
| I-13 | Worker life cycle auth/health/retry/logs/metrics and dynamic capacity | D/P | [Provider lifecycle](../operations/remote-worker-lifecycle-contract.md), [observability](../architecture/observability.md) | Covered at design level |
| I-14 | Users can connect their own remote servers, not desktops | E | [ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md) | Covered |
| I-15 | Backend **Rust only**, authoritative database **SQLite only** | E | [ADR 0004](../decisions/0004-control-plane-language.md), [ADR 0005](../decisions/0005-authoritative-database.md) | Covered |
| I-16 | One efficient low-energy DB with strong transactions/indexes/backup | H/E | [SQLite schema](../architecture/sqlite-schema-draft.md), [operations](../operations/sqlite-operations.md) | Detailed, operational tuning still proposed |

## 5. Budget, costs, human controls and operations

| ID | Historical subject | Evidence | Canonical documentation | Audit assessment |
| --- | --- | --- | --- | --- |
| C-01 | Explicit **hard project spending ceiling**, no automatic percentage overrun | E | [ADR 0002](../decisions/0002-hard-budget-approval.md) | Covered |
| C-02 | Insufficient remaining budget stops **new billable** attempts and alerts owner | E | [Budget reconciliation](../architecture/budget-reservation-reconciliation.md) | Covered |
| C-03 | Explicit authenticated human approval to increase ceiling; prior work retained | E | [Approval workflow](../architecture/approval-workflow.md) | Covered |
| C-04 | Running worker may finish **only if pre-reserved and bounded**, otherwise stop-on-risk | E | [ADR 0010](../decisions/0010-inflight-budget.md) | Covered |
| C-05 | Four categories: AI API / remote compute / storage / data transfer | E | [Budget](../architecture/budget-costs.md), [worked examples](../architecture/cost-scenario-worked-examples.md) | Covered |
| C-06 | Three scenarios: optimistic / expected / pessimistic | E | [Worked examples](../architecture/cost-scenario-worked-examples.md) | Covered with synthetic calculation |
| C-07 | User requested example cost for N tasks + worker runtimes | H | [Worked 40-task scenario](../architecture/cost-scenario-worked-examples.md) | **Added detailed hypothetical arithmetic** |
| C-08 | Cloud providers compared for economic selection; AWS was explored, not approved as universal best | H | [Provider evaluation](../operations/worker-provider-evaluation.md) | Covered; real measured comparison missing |
| C-09 | Dynamic capacity, provider retry costs, early cancel, late invoices, reservations | H/P | [Budget reservation](../architecture/budget-reservation-reconciliation.md), [worker lifecycle](../operations/remote-worker-lifecycle-contract.md) | Covered at design level |
| C-10 | Owner dashboard showing status, resources, estimates/actuals, failures/retries and alerts | H/D | [User flows](../product/user-flows.md), [observability](../architecture/observability.md) | Covered design, UI mock/implementation not verified |
| C-11 | Backup/recovery, secrets, monitoring, safe incident response | P | [Runbooks](../operations/runbooks.md), [backup](../operations/backup-recovery.md) | Covered as proposals |
| C-12 | One authority across multiple worker providers for ledger/fencing | E | [Multi-provider](../architecture/multi-provider-adapter-contract.md), [data model](../architecture/data-model.md) | Covered |
| C-13 | No unbounded retry, unapproved model escalation, paid cloud auto-retry | E/P | [Recovery](../architecture/retry-escalation-policy.md), [provider lifecycle](../operations/remote-worker-lifecycle-contract.md) | Covered |
| C-14 | Keep cost forecasts separate from enforceable provider cap and actual final bill | E/P | [Cost examples](../architecture/cost-scenario-worked-examples.md) | Expanded |

## 6. Repository documentation and implementation evidence

| ID | Requirement | Evidence | Canonical record | Assessment |
| --- | --- | --- | --- | --- |
| D-01 | All source/docs written for future AI agents; human does not write code | E | [AGENTS](../../AGENTS.md), [handoffs](../development/agent-handoffs.md) | Covered |
| D-02 | Conventional commits/branch review and precise tests | P | [Standards](../development/standards.md), [review process](../development/review-process.md) | Proposed, not fully owner-selected tooling |
| D-03 | Markdown relative links, YAML and Mermaid, Obsidian compatibility | E | [Documentation policy](../development/documentation-policy.md) | Covered, UI app untested |
| D-04 | All design decisions marked Approved/Proposed/Open/Implemented/Verified correctly | E | [ADRs](../decisions/README.md), [status](status.md) | Improved; live audit recommended each change |
| D-05 | Architecture, API, schema, operations, runbook and CI/test docs handoff-ready | E/P | [Docs index](../README.md) and related specs | Extensive, several interfaces still Proposed |
| D-06 | Actual code/tests separate from documented future work | E | [Project status](status.md), [milestones](milestones.md) | Covered; no compiled backend or validated deployment |
| D-07 | Two project chats **entirely and literally** checked | E user request | This audit | **NOT PROVEN** due to lack of full transcript export |
| D-08 | Original brand artwork/architecture diagrams byte-for-byte compared | H | [Brand history](../product/brand-history-license-governance.md), this audit | **NOT PROVEN**; artifact/source completeness missing |

## Findings: what was deficient before this audit

1. **Model entitlement/configuration:** previous [model access overview](../architecture/model-access.md) was concise; lacked a full two-mode decision flow, per-provider verification state, failure behaviors and safe process/credential handoff. **Expanded** in [detailed contract](../architecture/model-access-entitlement-contract.md) and [official-policy evidence](../reference/provider-cli-api-policy-evidence.md).
2. **Scheduler placement logic:** prior [scheduler spec](../architecture/scheduler.md) emphasized state/SQLite transactions but largely deferred ranking, hardware, region, Spot/On-Demand and multi-provider selection. **Expanded** in [selection policy](../architecture/scheduler-selection-policy.md), leaving numerical weights unapproved.
3. **Retry/escalation history:** old diagrams' 1/3/10-minute and three-attempt illustrations were not sufficiently separated from production behavior. **Expanded** in [recovery policy](../architecture/retry-escalation-policy.md), explicitly nonbinding.
4. **Planning + runtime token optimization:** existing [context spec](../architecture/context-memory.md) was broad; detailed staged prompt/diagnostic/caching techniques and negative tests were underspecified. **Expanded** in [token-efficiency contract](../architecture/token-context-efficiency.md); original complete accepted list remains unverified.
5. **Budget worked example:** four categories/three scenarios were specified but no transparent arithmetic example with concurrency, separate reserved exposure and a $-limited hard gate was prominent. **Added** [40-task synthetic scenarios](../architecture/cost-scenario-worked-examples.md).
6. **Brand/legal evolution:** final no-gold logo/MIT were captured, but rejected names, intermediate gold palette, MIT correction, missing final SVG/palette and trademark/competition concerns lacked a single detailed, time-ordered reference. **Added** [brand and license provenance](../product/brand-history-license-governance.md).
7. **Open vs implemented:** Rust, SQLite, HTTPS, GitHub App, group-only parents, bounded stop and multi-provider **are Approved architecture**, but provider accounts, exact permission sets, cloud adapter(s), migrations, executable code, actual invoices and E2E tests were **not verified**. Do not promote to “Implemented” merely because the documentation is extensive.
8. **Historical snapshots:** older notes occasionally still said choices were open after later user approval. Canonical decision/status docs must explicitly mark supersession or correct the stale claim; see [decision register](../decisions/README.md).

## Remaining requirements that cannot safely be “filled in” as approved

| Area | What is missing | Why not invented |
| --- | --- | --- |
| Original conversation fidelity | Full exported beginning-to-end transcripts from both chats | Retained excerpts/summaries do not prove every original sentence/reversal |
| Historical diagrams | Original image bytes, editable asset/provenance and any text not visible in summaries | Diagram excerpts may not reflect owner approvals |
| Brand | Original accepted logo file, final exact no-gold HEX set, SVG, usage license, X/GitHub/domain/trademark clearance | Earlier gold palette is superseded; missing art cannot be generated as “the original” |
| CLI/subscriptions | Provider-by-provider current remote automation/third-party terms and user entitlements | A subscription may not authorize third-party unattended workers |
| Scheduling | Numeric fairness/priority weights, true max parallelism, region/spot policy, warm pools | Diagram values were examples |
| Retry | Exact retries, 1/3/10 vs exponential backoff, model escalation and timeout caps | Historic graphic is not binding |
| Product UX | Explicit plan activation consent, account/RBAC policy, protected merge policy, frontend framework | Owner has not selected details |
| GitHub | Actual App ID/config, scopes, signed webhooks, key rotation, branch protection | Approval is design-only; no installation performed |
| Cloud | First supported provider list/regions, paid benchmark ceiling, enforceable provider spend caps | Multi-provider architecture Approved; rollout ADR 0007 still Proposed |
| SQLite/Rust | Real schema migrations, crate/runtime versions, WAL/backup settings and measured resource use | Design drafts are not code |
| Release | Real test runs, CI, verification of backup restore, review, measured reliability | No verified implementation in source examined |

## Recovery plan for a literal two-chat completion claim

1. Obtain user-supplied exports of **both complete Rolvya chats** (and original architecture diagrams/logo assets if they are relevant); do not assume summaries are complete.
2. Segment messages chronologically, with source conversation and message ordinal. Record each **user-approved**, **rejected**, **superseded** and **merely explored** technical/business decision independently.
3. Match each source statement to a normative document, test/ADR and current status. Add `source_message` references only after actual transcripts supply the IDs.
4. Resolve contradictions by **latest explicit user authorization**, never by the assistant's prior suggestion or a graphic's illustrative values.
5. Run exhaustive relative link / Markdown metadata checks and, when feasible, GitHub/Obsidian rendering, Mermaid parse and actual code/tests. This audit does not claim those tools were run simply because the documents have diagrams.
6. Publish remaining action list and verified coverage counts **based on the true total of source statements**, not an invented percentage.

## Change-control and safety notes

- Do not modify the original final logo, add gold, or migrate the public repository account.
- Never substitute MIT or a noncommercial restriction for MIT without a separate owner license decision.
- Do not add a personal-computer worker, local sync or unapproved DAG runtime tasks.
- Do not interpret a historical cloud/model/time/price example as a binding product limit.
- Do not enable live paid providers without explicit separate spending authorization; no auto-raise or percentage overrun.
- Never give broad GitHub write credentials or API keys to untrusted worker prompts.
- Track all future corrections in [status](status.md) and [open questions](open-questions.md) with **specific test evidence** when implemented.

Related: [historical audit](historical-decision-audit.md), [status](status.md), [decision register](../decisions/README.md), [documentation index](../README.md), [testing](../development/testing.md).
