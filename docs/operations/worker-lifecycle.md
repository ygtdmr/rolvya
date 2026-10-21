---
title: Remote Worker Lifecycle and Recovery
status: Proposed
category: operations
last-reviewed: 2026-10-21
tags: [workers, lifecycle, recovery]
---

# Remote worker lifecycle and recovery

## Invariants

Workers are **remote** hosts or isolated cloud workloads. They execute assigned fixed-DAG tasks, never plan additional tasks. Independent tasks can run concurrently only when dependency, capacity, Git isolation and hard-budget gates permit. Completion evidence remains durable even if workers vanish.

## Proposed worker state machine

`provisioning → registering → available → leased → executing → reporting → available`; alternate states: `draining`, `unhealthy`, `lost`, `terminated`. States are a proposal, not an adopted API enum.

## Lease and attempt protocol

1. Authenticate worker and advertise capability (CPU/RAM, execution mode, model adapter versions, supported sandbox, quotas).
2. Scheduler checks DAG readiness, conflicts, budget reservation and worker capacity.
3. Atomically create an attempt and a fenced lease; dispatch immutable task ID, plan version, task context hash and permitted operations.
4. Worker sends authenticated heartbeats and bounded redacted progress; authoritative state is stored server-side.
5. Worker returns artifact hashes, remote Git refs, test outcomes, measured usage and typed errors.
6. Validator checks acceptance criteria and commit provenance; only then mark task successful and unblock dependents.
7. Release reservations and reconcile usage; revoke credentials, scrub workspace and recycle/terminate capacity per approved policy.

## Failure matrix

| Failure | Required reaction |
| --- | --- |
| Startup/registration failure | No task lease; release unused reservation and surface actionable diagnostic |
| Heartbeat timeout or worker loss | Fence stale attempt, reconcile external operations, retry same task under allowed policy |
| Model quota or context exhaustion | Record typed failure, possibly alter model/effort after authorization and budget check |
| Git push succeeds but acknowledgment is lost | Resolve operation by idempotency key and remote ref before retry |
| Test or output validation fails | Do not report success or unlock successors |
| Budget insufficient | Do not admit any new billable attempt; preserve completed outputs |
| Secret revocation/security incident | Isolate worker, revoke scope and stop further privileged operations |

## Concurrency and cost

Actual worker capacity must include command execution, compilation and test CPU/RAM consumption. Scaling limits and remote provider quotas are explicit configuration. Worker autoscaling, startup and idle periods incur potential charges; new billable work must pass the approved budget gate. A subscription login is not proof a provider supports unattended remote CLI use.

## Recovery acceptance checks

- A late worker heartbeat cannot reactivate a fenced attempt.
- Restart reconciles incomplete leases and retained artifacts without duplicate irreversible Git writes.
- Failure does not create a new DAG node.
- Completed independent tasks remain available during budget pause or provider outage.
- Worker cleanup leaves no cross-project credentials or source data.

## Open decisions

Worker boot protocol, lease interval, maximum concurrency, sandbox technology, provider quotas, cancellation semantics, retry/backoff policy, allowed network egress and cost meter accuracy.

Related: [execution](../architecture/execution.md), [workers](../architecture/workers.md), [model access](../architecture/model-access.md), [Git integration](../architecture/integration-github.md), [deployment](deployment.md).
