# ADR 0018 — Initial Remote Worker Adapter Priorities

Status: **Approved**
Date: 2026-10-22

## Decision

**Current Owner decision (2026-10-24):** [ADR 0039](0039-aws-ecs-fargate-replaces-hetzner.md) replaced Hetzner Cloud with ECS on Fargate; subsequent [ADR 0041](0041-aws-fargate-first-real-worker.md) **moves AWS Fargate to priority 1**, Google Cloud Run Jobs to priority 2 and retains user remote VPS as priority 3. Former priority orders are historical.

The first three remote worker integrations to implement and validate are:
1. **Amazon ECS on AWS Fargate** — isolated, on-demand ECS task execution category; direct RunTask, not an additional independent task scheduler.
2. **Google Cloud Run Jobs** — serverless container job category.
3. **User-enrolled remote VPS** — user-controlled remote server category.

The project owner approved this initial development priority on 2026-10-22. This is an implementation priority, **not evidence that any adapter is already deployed or that all three providers are certified production-ready**. The user retains the ability to use additional providers once supported; the common Rust adapter architecture must remain extensible.

## Shared constraints
- Immutable upfront DAG; executable leaf attempts only, no new runtime tasks.
- Worker-initiated HTTPS control protocol, task-scoped identity and fenced leases.
- Atomic SQLite authorization, budget reservation, attempt and outbox intent before any billable provider launch. No budget overrun without explicit owner approval.
- Idempotent launch/reconciliation, provider-job inspection before reattempt on ambiguous outcome, cancellation and final cost reconciliation.
- Model/effort fallback only within pre-authorized user choices and configured retry limits.
- No Local Agent or direct access/synchronization with the user's personal computer.

## Exit criteria for each initial adapter
- Verify credential enrollment, region/capabilities/quotas, remote sandbox and worker bootstrap.
- Validate launch, status inspection, stop, cleanup, idempotency and lost-response behavior.
- Demonstrate credible maximum exposure bounds including startup, retry, idle and delayed-billing scenarios; pause if bounds cannot be enforced.
- Test representative worker concurrency, latency, CPU/RAM and full cost breakdown (AI API, compute, storage, transfer) on supported deployment environments.
- Demonstrate safe cross-provider reassignment only for the same task as a new attempt.

## Open details
Versions, ECS Fargate task CPU/memory and image compatibility, unsupported privileged Docker-in-Docker workloads, SDK/CLI implementation strategy, pricing and region choices, first release maturity, default routing order and formal performance thresholds remain subject to evaluation. AWS, Azure, DigitalOcean and other providers remain eligible for later adapters; no permanent provider cap is imposed.

Related: [Current order ADR 0041](0041-aws-fargate-first-real-worker.md), [Historical replacement ADR 0039](0039-aws-ecs-fargate-replaces-hetzner.md), [Worker host categories](0017-worker-host-categories.md), [Multi-provider infrastructure](0011-multi-provider-worker-infrastructure.md), [Adapter contract](../architecture/multi-provider-adapter-contract.md).
