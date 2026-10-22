---
title: Observability and Audit Architecture
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [observability, metrics, alerts, audit]
---

# Observability and audit architecture

## Scope and status

This is a proposed instrumentation contract for Rolvya's control plane and **remote** workers, not a claim that a telemetry stack, alert service or dashboard is deployed. Observability must distinguish intended plans, dispatched attempts, validated results, provider charges and actual implementation. Reference the [scheduler](scheduler.md), [worker protocol](worker-protocol.md), [budget](budget-costs.md) and [security](security.md).

## Correlation and event provenance

Every significant event should contain a stable project ID, plan version/hash, task ID where applicable, attempt ID where applicable, worker ID, event ID, trace/correlation ID, event type, timestamp, sequence/version and actor classification. Never include raw secrets, full credentials or unrestricted model prompts in metrics labels.

Proposed event groups:

| Group | Examples | Why it matters |
| --- | --- | --- |
| Plan | proposed, graph validation failed, activated | Complete upfront plan is immutable during execution |
| Scheduling | dependency blocked, ready, budget rejected, lease issued | Diagnose idle or wrongly blocked work |
| Worker | registered, heartbeat late, fenced, terminated | Detect lost capacity and stale attempts |
| Verification | artifact received, tests failed, task validated | Separate execution from proven completion |
| Git integration | ref published, conflict, merge rejected, commit verified | Preserve idempotent remote changes |
| Budget | estimated, reserved, actual charged, approval requested/granted/denied | Explain spending without automatic overrun |
| Security | denied scope, suspicious output, secret rotated | Incident response and accountability |
| Infrastructure | database unavailable, artifact missing, backup verified | Recovery preparedness |

## Metrics (names illustrative)

- **DAG throughput:** ready/waiting/running/succeeded/failed tasks, wait-to-start and verify-to-complete latency by project.
- **Capacity:** workers available/leased/lost, worker CPU/RAM, cold-start latency, dispatch retries, queue age, project fairness.
- **Reliability:** attempt failure rate by typed cause, lease expirations, stale-result rejections, replay deduplication, Git conflicts.
- **Cost:** approved ceiling, reserved exposure, settled actual and forecast separately; tag AI API, compute, storage and transfer; report optimistic/expected/pessimistic scenarios as estimates rather than factual spend.
- **Security:** rejected authorization events and suspicious operations as aggregates (no raw token or private payload in logs).

Metrics cardinality must be bounded: high-cardinality IDs belong in logs/traces, not arbitrary timeseries labels. Define aggregation and retention budgets before deploying a monitoring backend.

## Logs, traces and audit ledger

Logs provide redacted diagnostics; traces show request-to-dispatch-to-validation causal chains. The immutable or tamper-evident **audit trail** must persist separately from best-effort logs for budget approvals, credential changes, plan activation, remote Git write authorization, attempts and terminal state changes.

A worker's untrusted stdout must never be executed as a privileged directive. Access to traces and logs is permissioned per project; provide retention, export and deletion controls according to approved policy.

## Alerting and operator response

| Alert class | Suggested trigger | Operator response |
| --- | --- | --- |
| Budget admission paused | insufficient approved available budget | Show remaining, reservations, affected tasks; request **explicit** ceiling increase |
| Reservation/accounting drift | provider invoice vs ledger discrepancy | Freeze risky new admissions, reconcile exposure |
| Worker pool unhealthy | no available eligible workers / repeated lease expiry | Check remote worker connectivity, fence attempts |
| DB persistence failing | unable to durably write task/approval events | Stop dispatch and protect state |
| Git write ambiguity | network failure after possible external write | Reconcile provider operation before retry |
| Secret/security incident | suspicious privileged request or credential exposure | Revoke/isolate and follow incident runbook |
| Backup verification failed | latest restore test or checksum missing | Escalate before major migrations |

Threshold values, channels, on-call roster and alert deduplication are **not yet approved**. Alerts never imply human spending approval.

## Proposed dashboards

An owner panel should show active plan version, DAG state, task/worker timeline, blockers and dependency links, validation evidence, remaining budget and pending approvals. An operator view should highlight lease expiration, worker availability, remote Git conflicts, storage/DB health, error types and backup verification. Present missing cost meter data as unknown, never as zero.

## Failure and privacy considerations

Instrumentation failure must not silently bypass authoritative scheduling or budget checks. Prefer durable event-outbox and bounded asynchronous exporters. Protect private project text, redacted logs and any personal information with project-level authorization. Sample high-volume traces, but never silently sample out financially or security critical audit records.

## Proposed verification checks

1. Trace a task from activated plan through reservation, remote lease, output verification and accepted Git ref.
2. Reconstruct the cause of a paused project solely from persisted audit/ledger evidence.
3. Duplicate callback does not inflate spend or task success counts.
4. Expired worker lease generates an actionable event, not a false task success.
5. Secret-like strings in worker stdout are not revealed in dashboard, trace or webhook payload.
6. Unavailable external exporter does not lose authoritative approved ceiling or task state.

## Approved project Activity Log baseline

[ADR 0030](../decisions/0030-project-activity-audit-log.md) approves a project-scoped, user-visible **Activity Log** for important human, agent, worker, approval, cost, security, Git and task events. Entries must be **append-only at the application authorization layer**: no ordinary project role can edit or erase history, and corrections must be new traceable events. Secrets, tokens and confidential worker output must not appear in event details. Rust/SQLite is authoritative; crash-safe durable events and tenant/role-aware reads are mandatory. This is **not** proof that SQLite prevents privileged host/database administrators from tampering; immutable cryptographic anchoring and exact retention/role-filter policy remain Proposed.

## Open decisions

Telemetry backend, event schema, audit integrity strategy, dashboards, alert destinations/thresholds, log redaction implementation, retention/tenant isolation and observability cost ceiling.

Related: [scheduler](scheduler.md), [failure recovery](failure-recovery.md), [worker protocol](worker-protocol.md), [data model](data-model.md), [runbooks](../operations/runbooks.md), [testing](../development/testing.md).
