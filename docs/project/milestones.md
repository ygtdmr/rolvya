---
title: Rolvya Milestone Definitions and Evidence Gates
status: Proposed
category: project
last-reviewed: 2026-10-21
tags: [milestones, verification, acceptance, releases]
---

# Rolvya milestone definitions and evidence gates

## Ground rules

Milestones below are **proposed future deliverables**, **not completed work**, funded commitments, issue tickets or due dates. Confirm actual status using [project status](status.md), not this table. Software design can be approved without implementation; code can exist without having been independently verified.

## Milestone catalog

| ID | Milestone | Deliverable and independent verification | Current status |
| --- | --- | --- | --- |
| M0 | Approved architecture and risk gates | Rust/SQLite recorded; remaining transport, Git authorization, worker hosting and budget in-flight policies resolved before dependent paid work | **Partially specified** |
| M1 | Fixed DAG validator | Versioned hierarchy and dependency graph; reject cycles, dangling refs, runtime graph mutation | **Not verified** |
| M2 | SQLite financial ledger and durable outbox | Atomic lease/reservation and explicit human approvals; two simultaneous contenders cannot over-admit budget | **Not verified** |
| M3 | Rust scheduler and attempt lifecycle | Prerequisite guards, independent fake-worker concurrency, stale lease fencing and retry of same task ID | **Not verified** |
| M4 | Remote fake-worker protocol | Authenticated synthetic remote workers, duplicate/reordered calls, restart and backpressure scenarios | **Not verified** |
| M5 | Remote provider with authorized spend | Selected provider launches/cleans worker; measured invoice vs reservation and retry behavior | **Not verified** |
| M6 | Remote Git protected integration | Scoped repo access, isolated ref publication, conflict and revoke tests; no local desktop sync | **Not verified** |
| M7 | Owner budget/DAG panel | View plan/dependencies/attempts; explicit approvals, blocked status, artifacts and cost uncertainty | **Not verified** |
| M8 | Resilience and recoverability | Backup/restore, provider unknown-write reconciliation, security/financial auditing, simulated outages | **Not verified** |
| M9 | Public release candidate | CI/reproducible build, independent acceptance suite, security review, MIT/dependency and operator docs | **Not verified** |

“Partially specified” for M0 refers to **documentation**, not working software. Rust and SQLite are approved; other decisions and operational controls remain unverified.

## Proof-of-completion evidence template

Each milestone completion report must contain:

```yaml
milestone_id: Mx
decision_status: proposed-or-approved
delivery_status: not-started-or-in-progress-or-implemented-or-verified
remote_git_commit: replace-with-actual-commit
verified_on: replace-with-real-date
test_commands: []
test_results: []
requirements: []
acceptance_ids: []
budget_authorization_ref: null
actual_costs:
  ai_api: unknown
  compute: unknown
  storage: unknown
  transfer: unknown
known_limitations: []
owner_or_reviewer_approval: pending
```

The sample is a **template**, not a real verified report; never copy placeholder values into a completed milestone.

## Exit gates in detail

- **M1:** graph hash unchanged by attempts; hierarchy vs prerequisites not conflated; random DAG property tests.
- **M2:** SQLite failure/retry and exact integer ledger; no automatic percentage overrun; cost approval click replay safety.
- **M3:** only dependency-validated tasks admitted; worker exit does not equal accepted task; previous sibling artifacts remain after task failure.
- **M4:** stale worker cannot write authoritative task success or alter approved ceiling.
- **M5:** real billable attempts must have explicit test budget approval, provider cap/timeout awareness, invoice-lag and hidden-retry accounting.
- **M6:** GitHub token/repo permissions distinct from branch rules; remote output provenance and merge gate confirmed.
- **M7:** owner sees accurate task state and separated optimistic/expected/pessimistic forecasts; unknown charges visible.
- **M8:** restoration rechecks remote side effects and approved spending before resuming.
- **M9:** status says **Verified** only with a real commit, tests, environment and independent review.

## How to update actual progress

When code or an external environment exists, keep this spec as a milestone definition and write observed progress to [project status](status.md), including specific verified commits and test evidence. An agent handoff must distinguish tests *run* from *not run*. Substantive requirement changes need approval and ADR updates when applicable.

## Pending owner choices

Milestone priority/order, actual release scope, dates, initial provider selection, reviewer/merge policies, SLOs and launch criteria.

Related: [roadmap](roadmap.md), [implementation slices](implementation-slices.md), [requirements](../product/requirements.md), [acceptance criteria](../product/acceptance-criteria.md), [testing](../development/testing.md), [review process](../development/review-process.md).
