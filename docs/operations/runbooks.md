---
title: Operations and Incident Runbooks
status: Proposed
category: operations
last-reviewed: 2026-10-21
tags: [operations, incident, recovery, budget]
---

# Operations and incident runbooks

## Use and authority

These are **pre-deployment operational playbooks**. They contain logic and safety gates, not installed command-line services, a real on-call roster or verified automated incident actions. Only authorized operators may execute privileged recovery or approve changes. Consult [deployment](deployment.md), [worker lifecycle](worker-lifecycle.md), [secrets](secrets.md), [failure recovery](../architecture/failure-recovery.md), [observability](../architecture/observability.md) and [backup recovery](backup-recovery.md).

## First-response protocol

1. Identify project, current plan version and the exact incident time/event ID; preserve redacted audit evidence.
2. Determine if safety requires suspending **new billable task admission**, fencing current attempts or revoking secrets.
3. Check authoritative task states, budget ledger and pending reservations before changing workers.
4. Reconcile remote Git refs and artifact manifests before retrying potentially irreversible operations.
5. Escalate material architecture/security/budget decisions to the human owner; do not infer approval from chat acknowledgments or alert dismissal.
6. Record actions, consequences, verification evidence, missing signals and recovery authorization.

## Runbook A: Budget exhausted or forecast exceeds ceiling

**Trigger:** scheduler cannot safely reserve projected billable exposure.

**Expected behavior:** pause admission of new billable work; retain succeeded artifacts, pending task DAG and remaining cost ledger; display spent/reserved/remaining and the four cost categories.

**Steps:** identify whether unknown invoice lag or unreconciled reservations caused the stop; verify actual approved ceiling; notify owner with optimistic/expected/pessimistic forecast and the specific increase requested; await explicit authenticated approval or retain paused state. Never add a percentage buffer or silently increase the ceiling. Any in-flight execution stop policy remains open and must not be fabricated.

**Exit:** only a new approved ceiling, reconciled exposure, and all other eligibility checks can resume billable admission.

## Runbook B: Worker missing, stale lease or duplicated result

**Trigger:** missed heartbeat or duplicate/late callback.

**Steps:** locate attempt ID, lease epoch and event sequence; fence old attempt; inspect provider instance state and artifact/Git side effects; reconcile reservations; issue a **new attempt of the same task** only if authorized and budgeted. Reject stale successful callbacks and do not append tasks to active DAG.

**Exit:** one current attempt or an explicitly blocked/failed task, with preserved earlier artifacts.

## Runbook C: Remote Git push or merge unknown outcome

**Trigger:** timeout after a possible provider-side write or stale target SHA.

**Steps:** look up idempotency/operation ID and provider branch/commit, compare expected base ref, verify actual test manifest and conflict status. If remote write already succeeded, record its SHA and do not repeat it. If divergent, hold for review. No personal-computer repository synchronization workaround.

**Exit:** confirmed remote ref and documented integration decision.

## Runbook D: Database outage, corrupted snapshot or restart

**Trigger:** durable task/approval/event writes unavailable.

**Steps:** fail closed on new billable admissions; preserve current provider/worker status; prevent stale lease authority; verify database integrity and last backup; perform isolated [recovery](backup-recovery.md); reconcile remote Git and billing before re-enabling dispatch.

**Exit:** verified DAG integrity, ledger consistency, lease fencing and independent health checks.

## Runbook E: Credential compromise or unauthorized behavior

**Trigger:** leaked secret, suspicious worker command, cross-project access, injected instruction or unauthorized repository write.

**Steps:** revoke credentials and leases, isolate remote worker and affected repository permissions, preserve sanitized audit traces, evaluate unauthorized commits/cloud/API cost, rotate credentials, rebuild trusted environment, notify owner, then require explicit security approval before resuming.

**Exit:** attack path remediated, access proven limited and impact documented.

## Runbook F: Provider outage and model failure

**Trigger:** cloud, CLI or API adapter errors.

**Steps:** classify transient vs persistent error, check provider access-mode eligibility, rate limits and authorization, apply bounded backoff if approved. A different worker/model/effort creates a new attempt of the same task, not a new DAG node. Recheck available budget for each billable admission.

**Exit:** validated same-task completion or documented pause/escalation.

## Operational evidence checklist

| Item | Must record |
| --- | --- |
| Incident identity | project, plan, task/attempt (if relevant), event and actor |
| Immediate safety | pause/fence/revoke actions and timestamps |
| State | dependency, worker, Git/artifact and budget before/after |
| Changes | exact commands/API operation IDs, remote refs, approval IDs |
| Verification | tests run, result, unknowns and independent reviewer |
| Closure | root cause, ongoing risks, next actions and follow-up ADR |

## Open decisions

On-call ownership, alert destinations, escalation timing, specific provider commands, RPO/RTO, incident severity definitions, human authorization UX and in-flight cancellation policy.

Related: [observability](../architecture/observability.md), [backup recovery](backup-recovery.md), [worker lifecycle](worker-lifecycle.md), [engineering standards](../development/standards.md).
