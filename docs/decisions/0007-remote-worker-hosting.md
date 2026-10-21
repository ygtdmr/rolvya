---
title: "ADR 0007: Remote Worker Hosting and Cost Model"
status: Proposed
category: decision
last-reviewed: 2026-10-21
tags: [adr, cloud, worker, aws, google-cloud, budget]
---

# ADR 0007: Remote worker hosting and cost model

## Status and authority

**Proposed for **initial concrete provider(s), regions, sizing and cost benchmarking only**. **Approved [ADR 0011](0011-multi-provider-worker-infrastructure.md)** requires Rolvya to support extensible **multiple remote worker providers** through common Rust adapters; that architecture selection is NOT open. AWS ECS/Fargate, Google Cloud Run Jobs and user-provided remote VPS are candidate adapters, not approved first-release implementations. Personal-computer workers are excluded. No provider, region or concurrency cap is selected; past 20-worker and cost figures were illustrative, not benchmarks.

## Requirements from previously approved decisions

- One small remote control plane (roughly 2–4 GB RAM discussed, not proven).
- Worker hosts are **remote**, isolated and can run CLI agents plus source builds/tests/commands.
- Many independent DAG tasks may execute concurrently when prerequisites and permissions allow.
- Main agent plans once; worker runs its assigned task; attempt retries must not create new DAG nodes.
- Hard budget gate before starting **any new billable work**, including container startup and idle pooled capacity; user approval required to raise ceiling.
- Four cost classes and optimistic/expected/pessimistic reporting: AI API, cloud compute, storage, transfer.
- Strict credential policy and cloud cleanup to prevent runaway bills.

## Candidate hosting models

| Mode | Good fit | Risks / failure modes | Validation needed |
| --- | --- | --- | --- |
| Cloud Run Jobs (Google Cloud) | Batch-style finite container executions, task retries/parallelism | Per-instance minimum billing, limits/quotas, image/start latency, provider retry semantics and credentials | Real task lifecycle, quota, region and total billed seconds |
| ECS Tasks on AWS Fargate | Isolated ephemeral containers with adjustable CPU/memory, AWS integrations | Service complexity, startup time, 1-minute minimum billing, data transfer and egress | ECS provisioning benchmark, IAM security and actual bill reconciliation |
| User-provided remote VPS/host | Known hardware and predictable reserved capacity | User-managed OS, CLI auth, sandbox isolation, idle billing and hard multi-tenant separation | Secure enrollment, cleanup, quota and host failure response |
| Shared main control VPS | Minimal prototype infrastructure | Resource contention breaks small VPS assumption when compiling/testing | Tiny fake-agent demonstration only; not a scale default |
| Other cloud/VM/batch services | Region/cost alternatives, spot capacity | Preemption, differing cost/meter, provider lock-in and security features | Provider-specific benchmark and cost disclosures |

No vendor is universally cheapest. Compare provider rates **by region and date**, billable minimums, start latency, worker size, transfer, snapshots/log storage, rate limits, preemption and CLI support. Do not use prices from earlier diagrams as current quotes.

## Approved infrastructure boundary, proposed initial rollout

**Provider-neutral Rust adapters are mandatory under ADR 0011**, not optional if benchmarking indicates one vendor is cheaper. The question for this ADR is *which concrete adapters to build and configure first*, with tested billing/identity/sandbox behavior. See [multi-provider adapter contract](../architecture/multi-provider-adapter-contract.md). Initial support can be incremental, but never hardcode one cloud identity into the task model or SQLite financial authority.

## Recommended architecture (conditional)

Implement the **Approved provider-independent** WorkerPool / ExecutionAttempt adapter architecture with capabilities `launch`, `inspect`, `cancel`, `cleanup`, `measure_usage` and `health`, plus explicit cost model and idempotency metadata. The control-plane code should not depend on one cloud's job IDs as task IDs.

For the **first isolated remote-worker prototype**, compare one Google Cloud Run Job with one AWS Fargate Task and a user-managed remote VPS. Use **fake model adapters and synthetic repositories first**. Favor the provider with lower verified total cost and simpler secure boot under the measured workload, not an assumed global price winner.

Cloud Run Jobs are finite batch containers that exit when done; Google documents configurable per-task parallelism and retries. Cloud Run Jobs have an instance-based minimum billed duration of one minute. AWS documents Fargate billing based on requested vCPU/memory/storage with a one-minute minimum. These behaviors warrant per-task cost/reservation measurements (see sources).

## Cost model and assumptions

For a provider workload `i`:

```text
compute_cost_i = billed_cpu_units_i * regional_cpu_rate
               + billed_memory_units_i * regional_memory_rate
               + billed_storage_i
worker_overhead_i = image_registry + task startup + monitoring + idle/pool
total_estimate   = sum(compute_i + overhead_i + model_api_i + storage_i + transfer_i)
```

The equation is conceptual. Apply **provider-specific billing time minimums and rounding** instead of a universal rate. Subscription CLI charges may not be modeled as per-token API fees; clearly label coverage unknown and verify license/provider eligibility. Record cost *exposure cap* separately from optimistic/expected/pessimistic forecasts.

### Example workload matrix (benchmark, not a quote)

| Workload | What to measure |
| --- | --- |
| One <10-minute worker with no compilation | Image pull/start time, minimum bill and teardown |
| 10 independent ready tasks | Actual admitted concurrency, startup wave, region quota, total expense |
| One CPU-heavy compile/test | Peak CPU/RAM and wall time; suitability of chosen size |
| Worker crash after remote Git write | Duplicate prevention, time/cost reconciliation and preserved artifact |
| Burst of rejected tasks due to budget | **Zero newly admitted billable attempts**; alert and durable state |
| Spot/preemptible interruption where supported | Retry cost/exposure and artifact preservation |

The 10-task count here is a **test case**, not a product concurrency limit. All live comparisons require explicit test-spend approval and a strict cost ceiling.

## Risks and mitigations

Provisioned capacity can incur charges before the model call starts. Failing to terminate tasks, delayed cost feeds and storage/transfer fees are budget risks. Reserve conservatively and cap execution using provider features where available; never assert that internal estimates alone guarantee total bill is physically unable to exceed a ceiling.

Use short-lived per-attempt identity, network egress policy, isolated remote workspaces, pinned container image digests, project-specific namespaces, structured cleanup and auditable usage.

The [remote worker provisioning and cost lifecycle](../operations/remote-worker-lifecycle-contract.md) defines the provider adapter, launch idempotency, cancellation and billing reconciliation tests; the [owner decision pack](../project/transport-git-hosting-decision-pack.md) separates this proposal from approval. **No provider has been selected or benchmarked by these documents.**

## Decision gate and outcomes

Approve a concrete provider only after measuring representative workloads in at least two comparable providers/hosting modes, documenting region, size, lifecycle, total estimated vs invoiced bill, credential model and preemption/timeout behavior. An initial release may implement only one *concrete* provider adapter if the shared registry/contracts and fake multi-provider integration are in place; additional cloud and user-provided remote server adapters can follow. Do not imply the multi-provider architecture itself is optional.

## Sources checked

- [Google Cloud Run Jobs creation and task semantics](https://cloud.google.com/run/docs/create-jobs)
- [Google Cloud Run pricing](https://cloud.google.com/run/pricing)
- [AWS ECS Fargate pricing](https://aws.amazon.com/ecs/pricing/)
- [AWS Fargate supported resource configurations](https://aws.amazon.com/fargate/pricing/)

For controlled provider comparisons and billing/retry risks, see [remote worker evaluation](../operations/worker-provider-evaluation.md) and [budget reconciliation](../architecture/budget-reservation-reconciliation.md). Provider selection remains Proposed.

Related: [OQ-010](../project/open-questions.md), [workers](../architecture/workers.md), [worker lifecycle](../operations/worker-lifecycle.md), [budget](../architecture/budget-costs.md), [infrastructure history](../operations/infrastructure-options.md).
