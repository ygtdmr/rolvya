---
title: "Initial Remote Worker Provider Rollout — Staged Proposal"
status: Proposed
category: architecture
last-reviewed: 2026-10-24
tags: [remote-worker, providers, rollout, budget, validation]
---

# Initial remote worker rollout — staged proposal

> **Status: MIXED.** The **approved order** is synthetic fake workers, then **Amazon ECS on AWS Fargate as the first real integration** for the first working version, followed by Google Cloud Run Jobs and user-enrolled remote VPS ([ADR 0041](../decisions/0041-aws-fargate-first-real-worker.md), refining [ADR 0040](../decisions/0040-single-provider-first-working-version.md)). Exact provider configuration, benchmark spend, production certification and remaining implementation details are **Proposed / unverified**. The provider-neutral architecture remains [ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md).

## Proposed progression

1. **First fake-provider, no-billing integration tests:** prove Rust/SQLite immutable DAG, claim/heartbeat/fencing, budget reservation, outbox, cancellation/reconciliation and isolation in a synthetic environment. This does not require a live cloud account or provider model key.
2. **First real remote adapter: Amazon ECS on AWS Fargate.** Validate isolated on-demand `RunTask` worker boot, task-level IAM/identity, startup/cancellation billing, idempotency `clientToken` and `DescribeTasks` reconciliation. Explicit Owner-approved test exposure is needed before any paid launch; test real CLI/code-build compatibility, including the absence of privileged Docker-in-Docker.
3. **Second real remote adapter: Google Cloud Run Jobs.** Validate bounded isolated batch workers and independent executable leaves, with provider-side automatic retries disabled or financially accounted for; verify task timeout, startup and cleanup semantics.
4. **Third adapter: user-enrolled remote VPS.** Verify remotely supplied host identity, worker isolation and accountable capacity/billing; **never** add a Local Agent, personal-computer worker or local repository synchronization.
5. **Approved for first working version (ADR 0040):** A successful initial real **AWS ECS Fargate** integration is enough to demonstrate the first working remote-provider version; **the second and third approved adapters can follow incrementally**. A fake-only test is not a publicly releasable v0.1. Full public release scope and safety certification remain subject to all MVP gates, provider testing and a separately justified release decision.

## Shared rules — already approved, unchanged

- Controller stays small, workers remain remote, and only executable ready DAG leaves may get attempts.
- The provider adapter is not the model authentication mode. [ADR 0038](../decisions/0038-multi-account-isolated-cli-and-api-access.md) supports distinct multiple real CLI accounts and API-key connections, but **OQ-008 legal/provider entitlement may block unattended remote CLI subscription modes**. Start infrastructure validation with fake models; use actual models/CLI sessions only after provider permission, user consent and separate financial approval.
- Never pay for provider launch, default cloud retry, idle VM or API calls before enforceable exposure is reserved and an authorized dispatch is committed in SQLite. No automatic budget overrun.
- Test provider-controlled retries, cancellation, unknown launch results, worker identity, credential isolation and receipt of delayed bills; preserve work when paused.
- Keep optimistic/expected/pessimistic estimates and separate AI API, compute, storage and transfer.
- No universal price ranking, live cloud benchmarks, chosen region/image/worker size, mandatory count of simultaneous agents or final release order is established by this proposal.

## Minimal comparison (hypotheses, not benchmarks)

| Approved initial provider | Potential fit | Billing or safety risk to test |
| --- | --- | --- |
| **Google Cloud Run Jobs** | Ephemeral, on-demand batch containers | Default task retries can create unreserved executions; image/start/cancel latency and job billing need validation |
| **Amazon ECS on AWS Fargate** | Ephemeral isolated task containers controlled by Rolvya's scheduler | 1-minute Linux minimum, potentially billed image pulls, launch idempotency expiry/eventual consistency, IAM/egress cost and no privileged Docker-in-Docker |
| **User-enrolled remote VPS** | Use an independently enrolled existing remote host | Real host isolation, quotas, trust and non-provider-visible operating costs vary; not automatically free |

## Official references checked 2026-10-24

- [Google Cloud Run Jobs — Create jobs](https://cloud.google.com/run/docs/create-jobs): job default task timeout and retries and their configuration.
- [Google Cloud Run Jobs — Execute jobs](https://docs.cloud.google.com/run/docs/execute/jobs): execution and task lifecycle.
- [Cloud Run pricing](https://cloud.google.com/run/pricing): region/service pricing, billing and potential transfer costs; no fixed Rolvya quote.
- [AWS Fargate pricing](https://aws.amazon.com/fargate/pricing/): Linux one-minute minimum, per-vCPU/memory duration and regional rates.
- [Amazon ECS task launch idempotency](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/ECS_Idempotency.html): `RunTask` client token, bounded deduplication window.
- [Fargate security constraints](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/fargate-security-considerations.html): no privileged Docker-in-Docker.
- [Approved AWS substitution](../decisions/0039-aws-ecs-fargate-replaces-hetzner.md): why ECS/Fargate, rather than an AWS-owned duplicate scheduler.
- [Existing worker-provider evaluation](../operations/worker-provider-evaluation.md): required test matrix and reserved expenditure safeguards.

## Owner resolution (2026-10-24)

**Approved [ADR 0041](../decisions/0041-aws-fargate-first-real-worker.md)** (refining [ADR 0040](../decisions/0040-single-provider-first-working-version.md)): Implement the first working Rolvya with **one real AWS ECS Fargate adapter** after fake-worker correctness checks; Google Cloud Run Jobs and user-enrolled remote VPS are added in later increments. This decision is not an AWS/GCP spending authorization or proof of readiness for public production release. Exact provider settings, costs, and formal release gates remain open.
