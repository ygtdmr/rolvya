---
title: Product Scope and Non-Goals
status: Approved
category: product
last-reviewed: 2026-10-23
tags: [scope, mvp, non-goals, phases]
---

# Product scope and non-goals

## Status and intent

**Owner-approved Rolvya v0.1 MVP feature boundary (2026-10-23)**. This approves the capabilities below for the first usable release, **not** exact UI/provider implementations, cloud costs, milestone dates or a claim that code exists. Approved constraints remain binding regardless of which features ship first: complete immutable DAG planned up front, **non-executable group parents and executable leaves**, separate hierarchy/dependencies, dependent tasks wait, remote execution only, hard owner budget gate and **bounded-or-stop in-flight worker spending**, Rust backend only, SQLite database only, **HTTPS remote worker communication**, **GitHub App** for GitHub and **multi-provider remote worker adapter architecture**. Refer to [ADRs](../decisions/README.md).

## Candidate initial vertical slice

The first technical demonstration should prove **correctness of orchestration** with fake providers before adding live expense or real credentials:

1. Rust main control process on a **remote** development host; SQLite for persistence.
2. Single complete synthetic plan with hierarchical groups and dependency edges; validate graph before activation and freeze plan hash.
3. Structured per-task goal, context, model/effort and acceptance criteria (fake models for simulation).
4. Two independent simulated workers can execute concurrently; a dependent task waits for both validated artifacts.
5. Atomic SQLite budget reservation, attempt fencing and durable outbox/ledger.
6. Explicit owner-approved ceiling changes exercised with test identities; blocked billable work never starts automatically.
7. Crash/replay tests preserving successful artifacts and rejecting old worker results.

No live cloud, API key, subscription session or production GitHub App needs to be active in this *synthetic slice*. Its success does not by itself satisfy a releasable MVP.

## Approved first-working provider sequence (2026-10-24)

[ADR 0040](../decisions/0040-single-provider-first-working-version.md) approves a staged build: synthetic fake-worker tests, then an initial real **Google Cloud Run Jobs** adapter for the **first working version**, followed incrementally by **Amazon ECS on AWS Fargate** and user-enrolled remote VPS. This is **not** a downgrade of the previously approved functional v0.1 MVP requirements below: a fake-only demonstration is not a public release, and one real provider is not by itself a certification of complete secure production operations. Concrete regions, CPU/RAM, costs, provider eligibility, implementation and release signoff remain unselected.

## Approved Rolvya v0.1 public MVP capabilities

| Capability | Approved v0.1 outcome | Implementation dependency / still-open choice |
| --- | --- | --- |
| Create and configure project | Register user goal, remote Git repo, resource constraints and budget | Identity UX, supported provider policy |
| Up-front planning | Full versioned DAG and task hierarchy; per-task model+effort | Planner model and access mode |
| Scheduler | Ready queue, dependency checks, isolation, lease fencing | Rust + SQLite transactional implementation |
| Remote worker | Securely launch/provision a real remote executor, support CLI/API mode where permitted | HTTPS worker transport **Approved**, multi-provider architecture **Approved**; first working real adapter Cloud Run Jobs approved ADR 0040; production configuration and additional adapter rollout still open |
| Remote Git | Read approved repo, publish isolated task result and obtain reviewable refs | GitHub App **Approved** for GitHub; actual permission matrix and protected branch/merge policy open |
| Verification | Tests/artifact provenance, failure classification, same-task retries | Result schema, validator policy |
| Budget | Approved ceiling, holds/actuals, four categories and 3 scenarios, pause/approval | In-flight policy, cost source confidence |
| Owner panel | Project DAG, blockers, worker state, artifacts, charges and alerts | Frontend framework, identity, accessibility |
| Operations | Reproducible remote deployment, backups, audit and recovery | SQLite WAL and backup defaults already approved; secrets, restore objectives and release policy still open |

**All nine capabilities above are in the approved v0.1 feature boundary.** Specifically mandatory: goal/project configuration, complete up-front frozen DAG including hierarchy and per-task model/effort, dependency-aware concurrent real remote workers, secure permitted model execution, remote GitHub App reviewable outputs, independent result verification, hard budget accounting and explicit Owner approval, and a basic web panel. Minimal recovery, secure human/worker identity, audit and backup safeguards are release-critical supporting requirements. The approved multi-provider architecture does not imply every provider must be fully implemented at initial launch; actual provider release subset is a separately governed rollout decision. No date, working implementation or live provider authorization is asserted.

## Scope explicitly excluded by prior approval

- Local Agent, Local Sync, direct Git access or repository synchronization on the user's personal computer.
- Worker agents creating additional task nodes or launching unauthorized subagents during execution.
- Rewriting active plan topology to solve a failure instead of retrying the same task and escalating new requirements.
- Percentage-based automatic budget overruns, hidden billable worker provisioning or silent subscription-to-API fallback.
- A non-Rust Rolvya backend or non-SQLite authoritative database.
- Describing diagrams or draft schema as tested production services.

## Explicitly deferred beyond the v0.1 MVP

- Native mobile client, voice/Jarvis-style assistant, and optional advanced integrations are **not required for v0.1**. They would need separate owner-approved scope, security and schedule.
- Expanded provider catalog, advanced dashboards and deeper nonessential customization can follow after the first usable secure release; do not silently downgrade approved v0.1 core functionality.

## Potential later features (not commitments)

Multiple remote Git providers including self-hosted, broader cloud adapter coverage, team roles and enterprise auth, richer graph exploration, provider pricing dashboards, advanced fairness/scheduling, automatic source conflict analysis, optional long-term result reuse and expanded observability integrations. Their inclusion requires a separate product decision, acceptance criteria and development plan.

## Future client possibilities — not approved product scope

A possible **mobile client** and a possible Jarvis-like **voice conversational assistant** were discussed only as ideas, not commitments, and are explicitly **out of scope for approved v0.1**. They have **no approved release date, development work package, budget, MVP requirement or implementation authority**. Neither is a prerequisite for secure web sessions or Rolvya's core remote orchestrator. If later requested and separately approved, evaluate client-specific authentication, device secret storage, voice-initiated action authorization, consent, threat model and session continuity without relaxing Owner budget/permission step-up requirements. The current baseline is [web risk-based sessions](../decisions/0027-risk-based-session-management.md).

## Nonfunctional acceptance dimensions

- Resource budget: prioritize low control-plane RAM/CPU; benchmark on the intended remote VPS, do not assume 20 workers fit 4GB.
- Security: short-lived scoped identities, no model-visible secrets, protected remote Git merging.
- Reliability: SQLite transaction integrity, stale attempt fencing, replay-safe external writes, verified backup restore.
- Correctness: no runtime task creation, no dependency bypass, no admission without approved cost headroom.
- Transparency: cost forecasts vs charges visibly separated, missing provider data marked unknown.
- Licensing: MIT copyright/permission notice compliance, compatible third-party dependency licenses and provider/tool terms.

## Scope-change rule

Changes to a draft plan can be proposed/validated before activation. Once activated, a new requirement requiring additional tasks needs an **explicitly authorized new plan/version** under a still-open migration policy—not a hidden added child. Product scope decisions and technical implementation choices must be recorded separately.

## Required approval and traceability

**The v0.1 MVP feature boundary is Owner-approved.** Before implementation and release, remaining authentication/hosting/remote Git details, provider-specific limits for the approved in-flight budget behavior, review/merge UX and third-party licensing/operational assumptions still require appropriate approval and evidence. Each feature maps to [requirements](requirements.md), [acceptance criteria](acceptance-criteria.md), [implementation slices](../project/implementation-slices.md) and [project status](../project/status.md).

Related: [vision](vision.md), [user journeys](user-flows.md), [agent roles](../architecture/agents.md), [decision readiness](../project/decision-readiness.md).
