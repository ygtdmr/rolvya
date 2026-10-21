---
title: Remote Integration Security Reliability and Cost Test Matrix
status: Proposed
category: development
last-reviewed: 2026-10-21
tags: [integration, tests, github, worker, cloud, security]
---

# Remote integration security, reliability and cost test matrix

## Status and scope

**Proposed acceptance/test design; no tests listed here are claimed to have been run.** Supplements the [Rust + SQLite core test matrix](rust-sqlite-test-matrix.md) with **untrusted remote worker communications, Git publication, provider launch, billing and identity** behavior.

Binding decisions: Rust-only backend, SQLite-only authoritative database, completely fixed up-front DAG, executable leaves only, independent validation, remote Git/worker only, hard human budget approval and bounded-or-stop in-flight spending. Pending: worker transport ([ADR 0006](../decisions/0006-worker-communication.md)), hosting ([ADR 0007](../decisions/0007-remote-worker-hosting.md)), GitHub auth ([ADR 0008](../decisions/0008-github-authentication.md)). Until approved, test harnesses may **simulate** these candidates but must not claim the vendor was chosen.

## Deterministic fixture domains

- **Projects P1/P2:** distinct users/roles, scoped remote repositories R1/R2, owner-approved synthetic minor-unit budgets.
- **Plan H:** group G with executable leaves A,B; explicit dependency `A → V`, `B → V` and integration leaf V; no group worker attempt.
- **Attempt A1/A2:** A1 fails or is fenced, A2 retries original task A with incremented epoch and separate reservation.
- **Cloud fake:** responds success, failure, timeout-before-create, timeout-after-create, provider physical auto-retry, stale cost feed and cancel-pending.
- **Remote Git fake:** pinned base, isolated branch, protected main, diverged base, lost-response-after-write.
- **Worker fake:** authenticated/untrusted agent messages with duplicate sequence, forged project, expired lease and malicious stdout/README.

All fixtures must run in isolated approved **remote** test infrastructure or as in-process deterministic Rust simulation, never a user's personal-computer Local Agent. No real provider keys or personal repository data are needed for initial tests.

## Test cases

| ID | Area | Injected condition / operation | Evidence required to pass |
| --- | --- | --- | --- |
| RI-001 | Protocol | Worker claims group G | Denied without lease or provider launch |
| RI-002 | Protocol | Worker claims executable A without durable reservation/outbox | Denied; no paid launch |
| RI-003 | Identity | Worker P1 presents an A2 attempt ID belonging to P2 | Rejected; sanitized audit; no cross-project metadata |
| RI-004 | Fencing | A1 heartbeat/result arrives after A2 epoch created | A1 remains fenced; A2/task status not overwritten |
| RI-005 | Replay | Duplicate progress/result `message_id` and sequence | Single logical event/effect; deterministic acknowledgment |
| RI-006 | Network | Worker disconnected after legitimate claim | Safe lease expiry/reconcile; no new task node |
| RI-007 | Protocol load | Many idle fake workers poll | Controller RSS/CPU and request throughput measured; rate/backoff enforced |
| RI-008 | Budget | Ready task B has insufficient approved exposure | No new provider call, SQLite reservation or lease; correct UI blocker |
| RI-009 | In-flight | Running A remains provably bounded and reserved while B budget blocked | A continues only under its existing bound; B not admitted |
| RI-010 | In-flight | Provider stop limit disappears / cannot be verified | Stop-on-risk, lease fencing, cancel request, preserved validated artifacts |
| RI-011 | Billing | A provider's invoice delayed after cancellation | Unknown/pending liability visible; not silently zeroed |
| RI-012 | Cloud launch | Provider launches but HTTP response is lost | Inspect existing provider operation; no blind second paid launch |
| RI-013 | Cloud auto-retry | Fake provider starts unreserved second physical execution | Detect/contain and reconcile extra liability; block unsafe admissions |
| RI-014 | Cloud cleanup | Container exits but billable storage/log resources persist | Residual resource detected, billed category attributed and cleanup audited |
| RI-015 | Git identity | Installation/repository mismatch | Ref publication rejected |
| RI-016 | Git security | Worker asks publisher to write protected `main` | Trusted publisher denies; no broad token leaks to worker |
| RI-017 | Git conflict | Target ref SHA changed after validation | No force overwrite/merge; explicit conflict/review state |
| RI-018 | Git replay | Ref publication successful, API response lost | Reconcile actual remote ref SHA before retry; no duplicate effect |
| RI-019 | Git revocation | Installation removed or permission downgraded | Stop future privileged operations, preserve existing verified artifacts |
| RI-020 | Prompt injection | README requests Git token or budget approval | Treated as untrusted source text, never granted privileges |
| RI-021 | Acceptance | Worker exits successfully but artifact/test evidence absent | Task unverified; group G cannot become successful |
| RI-022 | Group aggregation | Required A/B validated and group acceptance passes, followed by duplicate callback | One group validated success transition; original fixed plan hash |
| RI-023 | Approval | Owner dismisses a budget alert without confirming ceiling | No new authorized amount or billable dispatch |
| RI-024 | Authorization | Forged worker command to change attempt model or create child task | Reject; immutable original plan preserved |
| RI-025 | Capacity | Cloud limit reached during outbox dispatch | Durable pending/blocked attempt state; no silent budget overrun or duplicate |
| RI-026 | Entitlement | Selected CLI subscription mode invalid for unattended remote host | Block with evidence, no silent paid API fallback |
| RI-027 | Failure recovery | A1 crashes after possible Git write and cloud charge | Reconcile Git/provider/ledger before A2; same task ID |
| RI-028 | Audit | Provider charge correction and duplicate billing notification | Append-only audited correction, no duplicate logical charge |
| RI-029 | Performance | Representative independent DAG fan-out + fake workers | Report measured p50/p95/p99, RSS/CPU, SQLite BUSY, outbox lag and test seed |
| RI-030 | Spend control | Paid benchmark requested without owner-authorized spend envelope | Deny; fake-only tests remain available |

Tests requiring final wire, auth or provider selection are marked **BLOCKED PENDING ADR**, not failed or passed. A test requiring real cloud charges needs a **separately approved benchmark budget** and documented limits.

## Stage gates

### Stage I — No paid providers

Use Rust+SQLite and deterministic fake worker, cloud, Git, billing and identity adapters. Verify RI-001–RI-014, RI-020–RI-024, RI-027–RI-030 using simulated providers where reasonable. Real HTTP endpoints are optional at this stage because ADR 0006 is not selected.

### Stage II — Candidate transport and remote Git sandbox

After owner approval of the relevant ADR(s), run authenticated worker protocol and **throwaway remote repository** tests, including revocation, protected-branch defense, stale worker and ambiguous ref writes. Token scopes must match current official GitHub endpoint permission docs; no production repo or protected branch is changed by a doc.

### Stage III — Authorized live cloud benchmark

After explicitly capped owner-approved test spend, repeat equivalent synthetic workloads on candidate remote worker providers. Capture provider job refs, actual time/rates, cost categories, billing delays, retries, cancellation outcomes, images, environment, permissions, CPU/RAM and latency. Do not choose a provider or claim a hard cap based only on forecast.

### Stage IV — Release verification

Only after real code exists, combine fixed-DAG validation, group roll-up, budget approval/stop-on-risk, remote Git isolation, cloud worker lifecycle, backup/restore and security tests. Provide tested Rust compiler/SQLite versions, source commits and reproducible commands.

## Evidence record (template, not executed)

```yaml
case_id: RI-012
status: not-run
environment: pending
backend_commit: pending
rust_toolchain: pending
sqlite_version: pending
provider: fake
credential_mode: test-only
request_and_operation_ids: []
test_command: not-run
expected_behavior: inspect existing launch rather than duplicate it
observed_result: not-run
cost_synthetic_or_invoiced: synthetic-only
artifacts_and_redacted_logs: []
reviewer: pending
```

## Links and coverage

- [Product requirements](../product/requirements.md) PR-001–PR-017 (approved requirements).
- [Product acceptance](../product/acceptance-criteria.md) AC-001–AC-026 (plus later additions).
- [Worker wire draft](../architecture/worker-wire-contract-draft.md) and [logical protocol](../architecture/worker-protocol.md).
- [Remote Git publication](../architecture/git-remote-integration-contract.md).
- [Worker provisioning and billing](../operations/remote-worker-lifecycle-contract.md).
- [Decision pack](../project/transport-git-hosting-decision-pack.md).
- [Approved ADR 0009](../decisions/0009-parent-aggregation.md) and [Approved ADR 0010](../decisions/0010-inflight-budget.md).

**Do not confuse a detailed test matrix with working software or passing CI.**
