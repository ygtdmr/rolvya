---
title: Remote Worker Wire Contract v0 Draft
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [worker, protocol, http, rust, sqlite, security]
---

# Remote worker wire contract v0 (draft)

## Status and scope

**Worker-initiated HTTPS transport is Approved in [ADR 0006](../decisions/0006-worker-communication.md).** This document remains a **Proposed wire/API schema**, not a running endpoint. It refines the [logical worker protocol](worker-protocol.md) and [transport design](worker-transport-design.md). The backend is Rust, authoritative DB SQLite; immutable DAG, group-only parents ([ADR 0009](../decisions/0009-parent-aggregation.md)), bounded-or-stop billing ([ADR 0010](../decisions/0010-inflight-budget.md)) and multi-provider worker hosts ([ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md)) are Approved.

The objective is to test exactly what messages the control plane must accept while keeping **transport-specific URL paths, codecs, timeouts and authentication mechanisms replaceable**.

## Actors and flow boundary

- **Owner/controller:** authenticates humans, validates entire plan and approves/records budgets. A worker has no such permissions.
- **Scheduler:** dispatches *existing executable leaves only*. No parent group receives a paid worker attempt.
- **Outbox dispatcher:** consumes a durable SQLite intent *after* an atomic attempt + budget reservation commit. It contacts a remote provider; possible provider success must be reconciled before retrying.
- **Remote worker:** starts on a remote host, authenticates its assigned attempt, receives only bounded context, heartbeats, submits evidence and stops on fencing.
- **Result validator:** verifies task acceptance evidence; receiving a result is never itself task success.

Workers do **not** access SQLite, create task nodes, run on the user's personal computer, raise budget, or merge protected remote branches. The controller must not give broad repository-write credentials to an untrusted agent.

## Proposed envelope v0

Every request and event carries or authenticates the following metadata:

| Field | Purpose / security check |
| --- | --- |
| `protocol_version` | Explicit compatible version (e.g. `draft-0`); reject unsupported versions |
| `message_id` | Unique logical message identity; dedupe in SQLite |
| `project_id`, `plan_id`, `plan_hash` | Immutable project-bound plan identity |
| `task_id` | Pre-existing executable-leaf task, never a group or a new node |
| `attempt_id`, `lease_epoch` | Current fenced attempt identity and monotonic authority |
| `worker_id`, `issued_at`, `expires_at` | Authenticated worker and controller-authoritative lease validity |
| `sequence_no` | Monotonic per-attempt event ordering, not client timestamp trust |
| `context_hash`, `artifact_manifest_hash` | Bind supplied task context and submitted outputs |
| `trace_id`, `operation_key` | Correlation and idempotency for external side effects |

**Do not** accept all identity fields at face value: correlate them with authenticated connection/session claims, active plan, current attempt and authorized project in SQLite. Timestamps provided by a worker are only advisory. Raw API keys, GitHub installation tokens and app private keys **must not** appear in an LLM-visible message body.

### Example: synthetic lease response

```json
{
  "protocol_version": "draft-0",
  "message_id": "example-message",
  "project_id": "example-project",
  "plan_id": "example-plan",
  "plan_hash": "sha256:example",
  "task_id": "leaf-A",
  "attempt_id": "attempt-A-2",
  "lease_epoch": 2,
  "worker_id": "worker-7",
  "expires_at": "2026-10-22T10:00:00Z",
  "context_hash": "sha256:example",
  "scope_ref": "artifact://scope/leaf-A",
  "acceptance_ref": "artifact://criteria/leaf-A",
  "execution": {
    "model": "assigned-model",
    "effort": "assigned-effort",
    "access_mode": "validated-mode"
  },
  "budget_reservation_ref": "hold-example",
  "remote_git": {
    "repository_id": "example-repo-id",
    "base_commit": "example-commit",
    "allowed_change_manifest_ref": "artifact://policy/leaf-A"
  }
}
```

This is **illustrative**, not a validated OpenAPI payload. Exact enum/currency/time types and artifact URI scheme require approval. Credential delivery must be separate, short-lived and task-scoped.

## Candidate operations (transport-neutral semantics)

| Operation | Actor | Durable preconditions | Response and idempotency |
| --- | --- | --- | --- |
| `RegisterWorker` | Enrolled remote worker | Approved pool, remote identity and capability; not personal computer | Worker record scoped to pool; repeated enrollment identified |
| `ClaimAttempt` | Authenticated worker | Attempt was previously admitted + reserved; only executable leaf; no expired/fenced epoch | Current lease / `NO_WORK`; conditional SQLite claim |
| `Heartbeat` | Lease owner | Matching worker, attempt and current epoch | Bounded extension or `FENCED`; duplicate sequence safe |
| `ReportProgress` | Lease owner | Valid epoch, bounded log sizes and redaction | Durable event receipt; never changes plan topology |
| `SubmitResult` | Lease owner | Immutable plan/context, expected attempt and artifact hashes | Receipt ID; moves to validation, **not** to task success |
| `GetStopInstruction` | Lease owner | Controller policy says cancel/fence or stop-on-risk | Signed/scoped stop directive, then reconcile external charges |
| `ReportCancellation` | Lease owner | Stop instruction and epoch | Receipt; not proof provider billing ended |
| `ReconcileExternalOperation` | Trusted Rust controller only | Durable operation key and provider/Git evidence | Durable resolved/uncertain state |
| `ValidateResult` | Trusted validator only | Artifacts and acceptance evidence verified | Task terminal transition and group-rollup notification |

HTTP method/path mapping is deliberately not yet binding. If using HTTPS pull, `NO_WORK` should support adaptive backoff/jitter or bounded long polling, not hot-spin.

## Claim, dispatch and crash semantics

1. Rust scheduler validates predecessor outcomes and checks current owner-approved ceiling and enforceable maximum exposure.
2. **One SQLite write transaction** creates attempt, lease epoch, reservation, audit and outbox launch intent.
3. After commit, outbox dispatcher attempts provider launch with stable operation key. Provider calls are **never made inside the SQLite write transaction**.
4. If launch response is lost, reconcile by provider identity/status before any new paid launch. Never claim exactly-once provider billing.
5. Worker authenticates and claims the reserved attempt. A late worker can be rejected/fenced without opening a new task node.
6. Worker submits progress/results with `attempt_id + lease_epoch + sequence_no`; duplicate messages are idempotent.
7. Trusted validator checks actual evidence, then atomically updates task status and triggers **ADR 0009** group roll-up if required descendants and criteria succeed.
8. Retrying failure is a **new attempt for the same existing task**, with its own fresh budget reservation.
9. A prior worker can no longer authorize writes after fencing; remote Git and cloud side effects must be reconciled anyway.

## Budget and stop-on-risk messages

**ADR 0010 Approved:** bounded and reserved attempts can continue even if another new task cannot be admitted. If future full exposure cannot be guaranteed within the existing hold, persist stop-on-risk, fence privileged work, request provider cancellation and preserve verified/checkpointed artifacts. A delayed provider bill can still occur. A worker must not decide for itself that an error means the user raised the ceiling.

Proposed event set: `EXPOSURE_BOUND_VERIFIED`, `EXPOSURE_UNKNOWN`, `BUDGET_ADMISSION_DENIED`, `STOP_ON_RISK_REQUESTED`, `STOP_CONFIRMED_BY_WORKER`, `PROVIDER_CANCEL_REQUESTED`, `PROVIDER_TERMINATED`, `FINAL_CHARGE_RECONCILED`. Treat worker-confirmed stop and provider final billing as different facts.

## Error taxonomy and safe retry behavior

| Error | Retry policy | Must not happen |
| --- | --- | --- |
| `INVALID_PROTOCOL_VERSION` / `INVALID_SCHEMA` | Fail request; negotiate or upgrade | Parse arbitrary model text as privileged protocol |
| `UNAUTHENTICATED` / `CROSS_PROJECT` | Reject and audit | Fallback to another project's lease |
| `FENCED_EPOCH` / `LEASE_EXPIRED` | Worker stops, controller reconciles | Accept late success to overwrite current task |
| `BUDGET_BLOCKED` / `EXPOSURE_UNBOUNDED` | New paid work denied | Launch anyway due to free capacity |
| `PROVIDER_OUTCOME_UNKNOWN` | Inspect provider by operation key with backoff | Blindly run a second billable container |
| `DUPLICATE_EVENT` | Return original acknowledgment | Reapply billing/event effects |
| `INVALID_RESULT` | Queue independent validation/review | Mark task succeeded on process exit |
| `TRANSIENT_UNAVAILABLE` | Bounded, jittered retry with idempotency | Unbounded polling or unreviewed provider billing |

## Security and operational acceptance

- Verify channel authentication **and** project/attempt authorization on every operation.
- Rate-limit enrollment/claims/heartbeats, bound event/log size, redact secrets and omit untrusted instructions from privileged fields.
- Use server-authored lease time/epoch; SQLite current attempt is authoritative.
- Reject group claim, non-existent task, forged budget approval, cross-project artifact or wrong base commit.
- Test reconnection, duplicate/out-of-order callbacks, pre/post-commit crash, 401/403, rate limits, provider job already running, stale Git results.
- Measure controller CPU/RSS, requests/sec, SQLite write contention, p95 claim latency and outbox lag on proposed small VPS hardware. **No numeric SLO has yet been approved.**

## Open implementation decisions

**HTTPS transport and initial worker timing are settled:** 15-minute access token, 30-second heartbeat, 90-second lease ([approved worker authentication policy](worker-authentication-policy.md)). Still open: concrete Rust network/auth/bootstrap implementation, exact versioned wire schema and payload limits, adaptive claim-poll/backoff, capacity fairness and long-poll semantics.

Related: [worker transport design](worker-transport-design.md), [logical worker protocol](worker-protocol.md), [Rust + SQLite backend](rust-sqlite-backend.md), [identity](identity-authorization.md), [budget policy](inflight-cost-policy.md), [test matrix](../development/rust-sqlite-test-matrix.md).
