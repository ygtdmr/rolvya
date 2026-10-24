---
title: Remote Worker Provider Evaluation and Benchmark Protocol
status: Proposed
category: operations
last-reviewed: 2026-10-21
tags: [workers, cloud-run, fargate, cloud, benchmark, budget]
---

# Remote worker provider evaluation and benchmark protocol

## Staged rollout proposal — not a new approval

[The staged rollout proposal](../architecture/initial-remote-worker-rollout-proposal.md) preserves the **already approved** Cloud Run Jobs, **Amazon ECS on AWS Fargate**, and user-enrolled VPS development priorities from ADR 0018 **as revised by ADR 0039**. The initial verified adapter, benchmark payment, release provider minimum, region, exact VM/image and spending envelopes must not be treated as executed or approved just because the proposal exists. Official Cloud Run retry behavior, ECS RunTask idempotency/eventual consistency, Fargate launch/cleanup billing and privilege restrictions need explicit tests.

## Purpose and decision boundary

**ADR 0018 already approves the initial implementation priorities: Google Cloud Run Jobs, **Amazon ECS on AWS Fargate**, and enrolled user-owned remote VPS (ADR 0039 supersedes Hetzner priority 2).** This evaluation selects their concrete regions, images and safe cost/isolation settings; production-ready certification and live spend remain open under [Proposed ADR 0007](../decisions/0007-remote-worker-hosting.md). **Multi-provider support is already Approved** ([ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md)); benchmark priority does not permit a single-vendor scheduler. Rolvya's backend is **Rust** and its authoritative database is **SQLite**, but project compilers and third-party coding agent CLIs run in remote isolated workers. Do not run such work on the user's personal computer. Actual cloud workloads are **billable** and require a separately authorized test ceiling.

## Common worker contract (provider-neutral Rust adapter)

```text
inspect_capabilities(region, cpu, memory, timeout, identity, quotas)
estimate_exposure(project, request, max_duration, storage, transfer, model_api)
launch(attempt_id, immutable_image_digest, operation_key, scoped_identity)
inspect(provider_execution_ref)
request_cancel(provider_execution_ref)
cleanup(provider_execution_ref)
measure_usage(provider_execution_ref, provisional_and_billed)
reconcile(operation_key, provider_execution_ref)
```

All provider-specific objects map to one pre-existing `task_id` and one-or-more `attempt_id` records. Provider job/task IDs never become task IDs. The Rust controller writes an atomic SQLite attempt + budget reservation + outbox **before** calling `launch`.

## Provider capability and billing hazards

| Option | Control API pattern | Key concern | Proposed mitigation |
| --- | --- | --- | --- |
| Google Cloud Run Jobs | Create/configure job, execute it and read execution/task status | Job tasks have configurable retries; **default 3 retry attempts** on failure if not changed, and default task timeout 10 minutes. Automatic retries may result in extra billable task executions without separate Rolvya admission. | Prototype with provider automatic retries **set to zero** unless each restart is explicitly reserved/reconciled and owner-approved; configure task timeout/capacity, observe executions and cost. |
| AWS ECS on Fargate | Register task definition, `RunTask`, `DescribeTasks`, stop | ECS `RunTask` supports caller `clientToken` idempotency with limited TTL; requests/describe calls can exhibit eventual consistency. Multiple launches/unknown outcomes can double bill. | Persist stable token, reconcile remote task ARN using `DescribeTasks` and backoff; never assume client token is indefinitely valid; bound retries and spend. |
| User-provided remote VPS | Authenticate to enrolled remote host, start isolated command/container, monitor and collect | Host may be underprovisioned, untrusted or already billed while idle; no universal API/cost telemetry | Verify host identity, remote access/egress policy, task isolation, CPU/RAM quota, manual ownership and provider/billing declarations. |
| Shared 2–4GB control VPS | Spawn worker commands on controller host | Heavy compilation competes with SQLite/scheduler, violating preferred cloud-worker separation | Do not make this default; if evaluated, use synthetic fake workers without personal-computer access. |

**Cloud Run Jobs note:** provider-level retries are distinct from Rolvya's explicit retry attempts. Without accounting for them, a single reserved task can incur several physical executions. The provider's job execution status must be reconciled with cost events; do not equate “job execution succeeded” with validated Rolvya task completion.

**AWS note:** the ECS API's `RunTask` idempotency token has a documented limited lifetime and parameters must match; an unknown result can require provider state lookup rather than unguarded relaunch.

## Comparable benchmark recipe

1. Create synthetic test repository and fake model adapter; no real proprietary/customer code in benchmark assets.
2. Fix equivalent CPU, memory, container image, region-relative networking, task timeout and egress policy for each provider as closely as possible.
3. Run: (a) cold start + tiny task, (b) moderate compilation/test CPU load, (c) 10 independent fake tasks as **illustrative** concurrency test, (d) worker crash, (e) network partition after successful artifact/Git write, (f) budget denial, (g) retained idle resource/cleanup check.
4. Record: job/task launch latency, first-heartbeat latency, CPU/RAM high water, startup + execution + cleanup billed time, direct compute cost, separate API model cost, storage and network transfer, usage API delay and last provider invoice.
5. Test provider-specific retry defaults, quotas, rate limits, permissions, container exit code semantics, GitHub connectivity and secret leakage.
6. Compare at identical observed work completed and acceptance evidence. A task that failed tests is not “cheaper completed work.”
7. Always stop tests at the pre-approved billable envelope; if exposure becomes unknowable, **stop new paid launches and alert owner**.

The 10-worker case is a measurement scenario, **not** a production cap or a promise that 20 CLI processes fit a 4GB controller.

## Cost methodology (no fabricated prices)

Separate:
- **AI API:** token or request charges where the validated API mode is used; subscription CLI accounting may be categorically different and must respect provider terms.
- **Compute:** CPU/memory provisioned or billed duration, minimum rounding, cold starts, provider retries, always-on host cost, cleanup delay.
- **Storage:** container registry, logs, retained artifacts, snapshots/volumes.
- **Transfer:** Git clone/push, logs, artifact upload, cross-region egress.

For each scenario calculate **optimistic / expected / pessimistic** and a distinct **enforceable reserved exposure**. A pessimistic cost forecast is not necessarily a hard provider ceiling. Unknown taxes, currency FX, region prices or last-bill lag must be labeled unknown rather than zero.

## Proposed evaluation scorecard

| Dimension | Suggested weighting to review | Evidence |
| --- | --- | --- |
| Safety/budget enforcement | Gate (must pass) | No unreserved auto retry; attempt/lease fence; provider-side max duration |
| Isolation and permissions | Gate (must pass) | Credential handling, network egress, no privilege escalation |
| Recoverability/idempotency | Gate (must pass) | Crash/relaunch and duplicated callback tests |
| Total cost per validated task | Compare after gates | Observed invoice and all four categories |
| Startup/cold-start latency | Compare after gates | p50/p95 across runs |
| Capacity and quotas | Compare after gates | CPU/RAM resource requirements, concurrent launches, API limits |
| Developer/operator complexity | Compare after gates | Time to configure, rotate secrets and diagnose failures |

Do not invent numerical weights, scores, region prices or rankings before measured tests. Concrete provider/region rollout and live benchmark spending need owner approval and a dated ADR update; **provider-neutral architecture does not** require another decision.

## Exit artifacts

For each provider, produce: region/date, configuration and container digest; identity/permissions; benchmark commands; provider task IDs and idempotency metadata; CPU/RAM/latency samples; billed compute and noncompute charges; test pass/fail evidence; risk and cleanup logs; recommended upper bounds and SLO draft. Make budget reservations auditable and do not expose raw tokens.

## Sources

- [Google Cloud Run: Create jobs (timeout and retries)](https://cloud.google.com/run/docs/create-jobs)
- [AWS ECS RunTask idempotency](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/ECS_Idempotency.html)
- [AWS ECS RunTask API eventual consistency](https://docs.aws.amazon.com/AmazonECS/latest/APIReference/API_RunTask.html)
- [Google Cloud Run pricing](https://cloud.google.com/run/pricing)
- [Amazon ECS/Fargate pricing](https://aws.amazon.com/ecs/pricing/)

Related: [multi-provider adapter contract](../architecture/multi-provider-adapter-contract.md), [Approved ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md), [ADR 0007](../decisions/0007-remote-worker-hosting.md), [worker lifecycle](worker-lifecycle.md), [budget reconciliation](../architecture/budget-reservation-reconciliation.md), [worker transport](../architecture/worker-transport-design.md), [open questions](../project/open-questions.md).
