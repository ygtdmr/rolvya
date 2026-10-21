---
title: Acceptance Criteria and Traceability
status: Proposed
category: product
last-reviewed: 2026-10-21
tags: [acceptance, requirements, tests]
---

# Acceptance criteria and traceability

This is a **future implementation test contract**, not evidence tests have run. Requirement identifiers refer to [product requirements](requirements.md).

| Test ID | Scenario and steps | Pass condition | Requirements |
| --- | --- | --- | --- |
| AC-001 | Submit a DAG with A→B→A or missing predecessor ID. | Entire plan rejected; no runnable jobs created. | PR-001 |
| AC-002 | Activate a valid DAG, cause a worker failure and then retry with another model. | Same task and graph identities retained; new attempt and audit record created. | PR-002, PR-005 |
| AC-003 | Have worker submit request to create a subtask. | Denied without modifying active DAG. | PR-002, PR-003 |
| AC-004 | Configure A prerequisite of C; B independent. | B may run while C remains blocked until A validates. | PR-004 |
| AC-005 | Ready tasks write overlapping remote Git files. | Integration isolation/conflict policy prevents silent overwrite. | PR-004, PR-007 |
| AC-006 | Run control plane on resource-constrained remote VPS with remote worker. | Remote dispatch and full state recovery work within a benchmark envelope to be approved. | PR-006, PR-012 |
| AC-007 | Inspect connector configuration and invoke repository operations. | Only remote endpoints are supported; no personal-computer Git paths or Local Sync. | PR-007 |
| AC-008 | Concurrently admit two tasks whose combined exposure exceeds available budget. | Transaction admits at most fully authorized exposure; no percent overrun. | PR-008 |
| AC-009 | Deny proposed budget increase, then approve a separate valid increase. | Denial leaves admission paused; only authenticated approval changes ceiling and enables reevaluation. | PR-008 |
| AC-010 | Consume budget mid-project. | Previously completed outputs remain available; newly billable tasks pause. | PR-008 |
| AC-011 | Generate cost estimate and accounting report. | AI API, compute, storage, transfer separated; optimistic/expected/pessimistic estimates distinct from actual spend. | PR-009 |
| AC-012 | Plan mixed-model work with access modes. | Task-level model/effort/mode retained and unauthorized modes rejected. | PR-005, PR-010 |
| AC-013 | Crash after provider writes but before local acknowledgement. | Reconciliation/idempotency prevents duplicate write and double reservation. | PR-008, PR-012 |
| AC-014 | Attempt to leak credentials via logs or model output. | Sensitive values redacted; access rejected and security event tracked. | PR-013 |
| AC-015 | Inspect docs in GitHub and repository-root Obsidian Vault. | Existing links resolve; proposed versus implemented status is explicit. | PR-013 |
| AC-016 | Inspect/build all Rolvya-owned backend services. | Backend services and application logic are Rust-only; no unapproved parallel Go/Python/Node backend. External user project tools remain permitted. | PR-014 |
| AC-017 | Initialize authoritative persistence and concurrently submit budget reservations, then crash/recover it. | SQLite is the only authoritative engine; transactional budget integrity, foreign keys, approved-ceiling protection and restored successful artifacts hold. | PR-008, PR-012, PR-015 |

## Additional provider and security acceptance cases

| Test ID | Scenario | Pass condition |
| --- | --- | --- |
| AC-018 | Cloud provider automatically restarts a failed worker container | Retry is disabled or financially accounted for as an authorized Rolvya attempt; no silent extra paid execution |
| AC-019 | GitHub token has Contents write but worker attempts protected-branch write | Worker cannot bypass trusted integration control or approved protected-branch rules |
| AC-020 | Owner dismisses an expense alert without approving a new ceiling | Approved ceiling and admission pause remain unchanged |
| AC-021 | Provider invoice is delayed or cannot be reconciled | Cost remains marked uncertain; new unsafe billable work is not admitted |
| AC-022 | Worker impersonates another project or submits stale lease epoch | Authorization fails and audit logs capture a sanitized denial |

Related designs: [budget reconciliation](../architecture/budget-reservation-reconciliation.md), [worker transport](../architecture/worker-transport-design.md), [identity](../architecture/identity-authorization.md) and [GitHub App isolation](../architecture/github-app-integration-design.md).

## Evidence rules

Each implementation claim needs source commit, repeatable test command, environment/version, result and known limitations. A proposed acceptance test is not a completed test. Automated checks should run in CI before merging behavior changes; security-critical behavior must include negative tests.

## Pending decisions

Performance thresholds, service reliability targets, supported worker providers, concrete authentication tests, timeout policy, UX/approval presentation and which tests gate initial release.

See [testing strategy](../development/testing.md), [project status](../project/status.md), and [decision register](../decisions/README.md).
