---
title: Worker Control Protocol
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [workers, protocol, leases, security]
---

# Worker control protocol

## Scope and constraints

This is a **logical message contract**, not an HTTP/gRPC/queue implementation. Rolvya uses remote cloud or user-supplied remote hosts. No personal-computer Local Agent, Local Sync or desktop repository synchronization. The complete task DAG is fixed before dispatch, and an executor is limited to its assigned task.

## Principal identities

- **Controller:** authoritative task, budget, identity and event writer.
- **Worker instance:** short-lived remotely authenticated execution capacity with declared capabilities.
- **Attempt:** one task execution instance; its `attempt_id`, `lease_epoch` and opaque fencing token must identify current authority.
- **Validator:** accepts/rejects evidence; a worker's exit code alone does not certify completion.

## Message inventory (proposed)

| Message | Direction | Required fields | Safety behavior |
| --- | --- | --- | --- |
| RegisterWorker | worker → controller | worker_id, instance_nonce, capabilities, adapter_versions, auth identity | Reject duplicate/stale identity |
| OfferLease | controller → worker | plan_id, task_id, attempt_id, context_hash, lease_epoch, expiry, budget_authorization_ref, permitted_ops | No secrets in plain prompt |
| AcceptLease | worker → controller | attempt_id, lease_epoch, acknowledgement_id | Reject expired lease, support idempotence |
| Heartbeat | worker → controller | attempt_id, lease_epoch, worker_id, monotonic progress sequence, resource metrics | Reject fenced/stale attempts |
| ProgressEvent | worker → controller | attempt_id, sequence, stage, sanitized message/artifact refs | Bound rate, size and retention |
| SubmitResult | worker → controller | attempt_id, lease_epoch, manifest_hash, remote Git refs, test evidence, usage and error classification | Record durably before validation |
| ValidateResult | validator → controller | attempt_id, evidence, acceptance status, validation_id | Only controller commits task success |
| CancelLease | controller → worker | attempt_id, new fencing epoch, reason | Stop privileged operations; reconcile side effects |
| ReconcileOperation | controller ↔ provider | idempotency key, remote operation ID, observed Git/artifact/billing state | Do not blindly retry irreversible writes |
| CloseAttempt | controller → accounting/storage | outcome, reservation_id, cost reconciliation, cleanup status | Idempotent accounting and cleanup |

No message type authorizes a worker to add nodes to a plan or raise a budget ceiling.

## Example lease envelope (illustrative, NOT a live endpoint)

```json
{
  "schema_version": "draft-1",
  "project_id": "example-project",
  "plan_id": "example-plan",
  "task_id": "example-task",
  "attempt_id": "example-attempt",
  "lease_epoch": 3,
  "expires_at": "example-ISO-8601-time",
  "context_hash": "example-sha256",
  "model": "planner-assigned-model",
  "effort": "planner-assigned-effort",
  "access_mode": "approved-cli-or-api",
  "allowed_operations": ["read-remote-git", "write-isolated-ref"],
  "reservation_ref": "example-reservation"
}
```

The opaque fencing token and credentials are delivered using a separate secure channel/mechanism selected by ADR; their absence in this example is intentional. This is a conceptual shape, not a fixed serialization schema.

## Delivery semantics and ordering

Assume transport may duplicate, delay, reorder or drop messages. Require idempotent commands keyed by request/operation ID; compare lease epochs and monotonically increasing per-attempt sequence numbers. Accept *at-least-once* delivery with safe deduplication instead of claiming exactly-once external execution. Acknowledge results only after durable persistence. Never use clock timestamp alone for worker fencing. Worker heartbeats extend only the matching current lease.

## Work execution and Git isolation

The controller supplies a pinned remote repository commit/ref, permitted paths, task goals, compact context and acceptance criteria. The worker uses a remote isolated workspace; it produces artifacts/commits on an attempt-scoped branch or equivalent remote ref. Final integration authority belongs to an approved controller/review policy, not to arbitrary worker output. Process stdout, source code and issue text are untrusted.

## Provider and subscription access

CLI (e.g. Codex CLI/Claude Code) and API modes are separate adapters. Validate provider licensing, allowed unattended use and credentials before enabling an adapter; do not assume a paid consumer subscription is a transferable server credential. Model and effort are planner assigned; failover must be auditable and budget authorized.

## Security and observability

Mutual authenticated identity (mechanism TBD), per-project authorization, least-privilege short-lived credentials, redacted logs, trace IDs, bounded event volume and isolated workspaces. Block credential exfiltration paths where feasible; raw secrets never enter prompts, Git commits or event logs. Register, lease, progress, validation, cancellation and cleanup must be auditable.

## Test contract

1. Duplicate result submission is recorded once and validated once.
2. Expired/fenced attempt cannot heartbeat into authority or overwrite a newer result.
3. Disconnect after successful Git push reconciles using remote commit/ref, without duplicate writes.
4. Worker cannot alter budget ceiling, active DAG or other projects' task data.
5. Authentication revocation prevents further privileged operations.
6. Worker crash preserves stored successful work and releases/reconciles reservations safely.

## Open decisions

Transport, worker polling vs push, envelope schema, TLS identity, heartbeat cadence, timeout lengths, message queue, payload-size limits, artifact upload/auth mechanism, retry backoff and event retention.

Related: [scheduler](scheduler.md), [workers](workers.md), [API inventory](api-contracts.md), [remote Git](integration-github.md), [security](security.md), [worker lifecycle](../operations/worker-lifecycle.md).
