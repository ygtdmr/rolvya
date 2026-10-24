---
title: Initial Worker Adapter Implementation Plan
status: Proposed
category: architecture
last-reviewed: 2026-10-25
tags: [workers, aws-fargate, cloud-run, remote-vps, scheduler, budget]
---

# Initial worker adapter implementation plan

## Context

**Current Owner-approved order:** synthetic fake-worker tests first, then **Amazon ECS on AWS Fargate as the first real adapter**, followed by **Google Cloud Run Jobs** and **enrolled user-owned remote VPS** ([ADR 0041](../decisions/0041-aws-fargate-first-real-worker.md), revising ADRs 0018, 0039 and 0040). The original Google-first / Hetzner references in older decisions are historical, not current implementation requirements. This document is a proposed implementation specification; no cloud adapters or paid tests have been deployed.

Shared invariant: provider-specific work must not leak into planner DAG structure, approved budgets or worker HTTPS task protocol.

## Common lifecycle contract

1. **Enroll:** Verify approved account/pool identity and authorization, region and feature set; store secret references outside model-visible context.
2. **Discover:** Advertise resource capacities, available runtime/sandbox, duration, cancellation semantics, startup/idle fees, service-side retries and usage evidence.
3. **Estimate:** Calculate a conservative bound on full exposure (AI API, compute, storage and transfer); if an enforceable bound cannot be demonstrated, refuse billable dispatch.
4. **Admit:** Single SQLite transaction confirms existing executable leaf, authorized provider and model/effort, budget reserve, attempt ID, lease/fencing epoch and durable outbox operation key.
5. **Launch:** Dispatch after commit; send only narrowly scoped bootstrap identity; keep stable operation identity. Timeout or ambiguous result becomes **launch_unknown**, not permission to launch a duplicate.
6. **Claim:** Worker initiates HTTPS connection, verifies its identity, receives attempt-limited token/lease, gets compact task context, and heartbeats.
7. **Finish:** Worker submits hashed artifacts and validation evidence; receiving a result does not itself mark success. Independent validator verifies acceptance.
8. **Cancel/fence:** Controller revokes lease and requests provider cancellation; cancellation request is not termination confirmation.
9. **Reconcile and clean:** Inspect external job, remote Git publication, artifacts, resources and eventually charges; release unused reservation only with evidence. Preserve verified outputs.
10. **Retry:** Main agent diagnoses failures; Rust authorizes a **new attempt of the same DAG leaf** within the preflight-authorized retry limit, model/provider choices and fresh bounded budget. Independents continue, dependents wait.

## Provider-specific requirements

### Amazon ECS on AWS Fargate — first real integration
- Launch one isolated on-demand ECS task per authorized attempt via `RunTask` using the **Fargate launch type**, not a persistent ECS Service or a second job scheduler.
- Use an immutable image digest and explicitly scoped task definition, task role versus execution role, `awsvpc` networking, CPU/RAM, timeout and egress controls. Do not assume privileged Docker-in-Docker is available.
- Persist a stable `clientToken`/operation key and reconcile ambiguous launch outcomes through ECS inspection; ECS idempotency is time-bounded and `DescribeTasks` can be eventually consistent. Never launch a second paid task solely because the first call timed out.
- Track task ARN, task lifecycle, cancellation/`StopTask` evidence, cleanup, Fargate runtime exposure, image pull, log, network/NAT and storage costs. A stop request or SDK success response is not proof that billing exposure ended.
- Require a demonstrably bounded exposure, SQLite admission, task-scoped credentials and explicit Owner authorization for every paid launch or benchmark. Do not enable service-managed extra launches outside Rolvya budget admission.

### Google Cloud Run Jobs — second real integration
- Launch immutable worker image by digest with scoped service identity.
- Configure task count, per-task duration, parallelism and provider-native retry count explicitly; propose **provider-native retry count = 0** so Rolvya owns paid attempt admissions. Confirm actual service behavior via official API and integration tests before enablement.
- Capture execution and task IDs, check completion/stop independently, and reconcile log/storage/egress charges.
- Evaluate actual termination and billing limits instead of assuming a cloud API cancellation instantly ends exposure.

### Enrolled user-owned remote VPS — third adapter
- Use a **pre-enrolled outbound HTTPS worker runtime** on a user-owned **remote** server. It cannot be assumed that a mere VPS address/SSH key satisfies trust, availability or sandbox requirements.
- Define bootstrap/enrollment, isolated per-attempt workspaces, CPU/RAM/timeout quotas, process supervision and log/artifact policy.
- Distinguish provider invoice from internal cost attribution: VPS may have already-paid fixed cost but cannot automatically be labeled zero cost.
- Revoke token/lease, kill supervised task, validate stop and artifact publication permissions before assigning another attempt.
- **No Local Agent, local personal machine execution, or local Git synchronization.**

## Provider-neutral validation matrix

- Idempotent outbox replay and uncertain cloud launch produce **no blind second paid launch**.
- Provider retries set/observed and counted; no invisible extra model/API charges.
- Cross-tenant token, unapproved region, model or resource class rejected.
- Attempt cancelled but provider job continues: stop-on-risk persists and charges stay provisional.
- Server crashes after launch; recovery maps active provider jobs to original attempt IDs.
- One provider fails; other authorized providers continue independently; same task retry gets new epoch and reservation.
- CPU/RAM and p95 claim latency under 100/500/1,000 worker simulations on 2–4 GB controller.
- Verify project budget never implicitly increases; test backup/restore plus provider cost reconciliation.

## Unresolved decisions (do not mark approved)

AWS Fargate region, task definition/image digest, CPU/RAM, task/execution IAM permissions, egress/network configuration and reliable cost/stop bounds; later Cloud Run service identity and region, secure remote VPS runtime packaging, Rust SDK choices, concrete hard timeout settings, provider cost bound proofs, default routing/fairness and live paid benchmark budget.

Related: [Approved AWS-first rollout](../decisions/0041-aws-fargate-first-real-worker.md), [Initial adapters](../decisions/0018-initial-worker-adapters.md), [Multi-provider contract](multi-provider-adapter-contract.md), [Remote lifecycle](../operations/remote-worker-lifecycle-contract.md), [worker auth](worker-authentication-policy.md), [quiet runtime](../product/preflight-quiet-runtime-policy.md).
