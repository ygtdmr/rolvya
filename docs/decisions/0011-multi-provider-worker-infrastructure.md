---
title: "ADR 0011: Extensible Multi-Provider Remote Worker Infrastructure"
status: Approved
category: decision
last-reviewed: 2026-10-21
tags: [adr, cloud, providers, workers, adapters, rust]
---

# ADR 0011: Extensible multi-provider remote worker infrastructure

## Status and approval

**Approved by the project owner on 2026-10-21.** Rolvya must support an **extensible, multi-provider remote worker architecture**, rather than bind its execution engine to a single cloud vendor. Multiple cloud services and enrolled user-provided **remote** server providers may be supported through common adapters.

This is a **binding architectural requirement**, **not** approval of a specific vendor, region, pricing plan, public plugin API or any particular initial-release provider lineup. [ADR 0007](0007-remote-worker-hosting.md) remains **Proposed** for the initial provider selection and benchmarking plan. No cloud adapter or provider deployment is verified by this approval.

## Context

Rolvya uses a small, Rust-only control plane with a single authoritative SQLite database, while multiple independent executable leaf tasks can run concurrently on remote workers. Source code, CLI agents, builds and tests execute remotely. The user's **personal computer is not a worker** and has no Local Agent or Local Sync integration.

Users may vary by region, pricing, available remote server credentials, provider quota, CPU/RAM workload and eligibility for coding-agent CLI/API access. Supporting more than one cloud is intended to avoid lock-in and allow capacity and cost-aware scheduling within fixed plan constraints.

## Decision

1. **Provider-independent Rust domain:** task/attempt/lease identifiers, fixed DAG, reservation ledger, artifact provenance and worker protocol remain provider-neutral. A cloud's job identifier is only a reference attached to an existing Rolvya attempt, **never** a task identifier.
2. **Multiple provider adapters:** a Rust backend adapter abstraction supports authorized cloud batch jobs/containers/VMs and user-enrolled **remote** server workers. Every concrete adapter implements a common lifecycle and capability contract, then supplies vendor-specific limits and billing semantics.
3. **Provider-neutral worker control:** all enrolled workers speak the **approved worker-initiated HTTPS protocol** from [ADR 0006](0006-worker-communication.md). Native provider provisioning calls can vary; the task protocol, success verification, budget authority and lease fencing do not.
4. **Per-attempt provider binding:** Rust scheduler may select from *only owner-authorized and task-compatible provider configurations*, considering the fixed plan's declared requirements, allowed fallback policy, capacity, model access eligibility, cost exposure and region/security constraints. Record actual provider/config/region on the new attempt. Provider reassignment may occur **only for a new attempt of the same task** after fencing/reconciliation and authorized admission; no runtime task nodes/edges may be created.
5. **No automatic unapproved paid failover:** if one provider is down or costlier, do not silently switch to an unapproved, more expensive provider. A replacement attempt requires the same verified authorized budget headroom and capability/security checks. Unknown provider spending exposure blocks billable admission.
6. **Hard budget and stop-on-risk across providers:** preserve the single SQLite approval/reservation/ledger with four categories (AI API, compute, storage, transfer), optimistic/expected/pessimistic forecasts, and [ADR 0010](0010-inflight-budget.md) bounded continuation or stop-on-risk. Provider auto-retries must not create unreserved paid executions.
7. **Provider isolation:** each adapter receives narrowly scoped credentials and project identity, creates isolated remote workspaces, uses pinned workload manifests/image digests where relevant, and supports revocation/cancellation/cleanup/usage reconciliation. Never pass unrelated cloud credentials into worker model context.
8. **Support expansion:** adding an adapter must not require rewriting the scheduler's domain state machine, immutable DAG schema or owner approval policy. Provider-specific storage and sandbox details can be added behind versioned capability/config interfaces.
9. **No personal-computer integration:** remote-host support means user-provided **remote infrastructure**, never desktop/local agent access.
10. **Provider-neutral observability:** expose provider, pool, region, attempt, lease, reservation, launch state, provisional/settled/unknown charges, cancellation status and artifacts consistently across adapters.

## Adapter contract and capabilities (conceptual)

| Operation | Mandatory behavior |
| --- | --- |
| `discover_capabilities` | Explicit CPU/memory, supported runtime/image, networking, region, timeout, billing units, quota and identity constraints |
| `quote_exposure` | Conservative **enforceable** maximum across compute, AI API where relevant, storage and transfer; mark unsupported/unbounded as unsafe |
| `launch` | Starts one already-admitted attempt using stable idempotency key; responds with provider job ref or ambiguous state |
| `inspect` | Retrieves job state, operation ref and current cancellation/usage evidence |
| `cancel` | Requests termination after stop-on-risk; return request status separately from confirmed stop/final billing |
| `cleanup` | Confirms teardown of chargeable resources or reports remaining liabilities |
| `usage` / `reconcile` | Produces provisional/settled/unknown charges and external side-effect evidence |
| `health` | Measures availability, quota/capacity and adapter faults without bypassing project policy |

Names are **illustrative trait names**, not approved compiled Rust APIs. Exact plugin loading (in-process Rust crates vs external services), SDK choices, adapter versioning and provider enrollment UI are **Open**. Any Rolvya-owned backend services and adapters must be **Rust**, consistent with ADR 0004.

## Candidates — not commitments

AWS Fargate/ECS, Google Cloud Run Jobs, other cloud container/batch/VM services and user-provided remote VPS hosts are examples. Provider support may be added incrementally; **no specific provider count, ordering, first release availability or global pricing rank is approved** by this ADR.

## Compatibility and safety tests

- Run identical deterministic fake tasks against two fake provider adapters with dissimilar operation IDs; the immutable task IDs and plan hash remain unchanged.
- A single plan can have independent executable leaves running via different **authorized** remote provider configurations while respecting dependencies/conflicts.
- Provider failure followed by new attempt on an approved alternative preserves the existing task ID, increments fencing epoch and creates a new SQLite reservation/outbox; no duplicate external launch.
- A provider lacking enforceable cost bounds cannot obtain paid admission; no hidden percentage budget buffer.
- Cross-provider billing remains attributable by project/attempt/category; delayed costs and cleanup liabilities are visible.
- Provider A's token is not available to Provider B or a model prompt; revoked credentials block future privileged work.
- Worker HTTPS claim and artifact verification are consistent regardless of host.
- An unconfigured provider is never selected automatically and cannot be silently substituted for a subscription-mode limitation.
- Unknown or failed cancellation triggers ADR 0010 stop-on-risk/reconciliation rather than false “free” termination.
- The control VPS is tested at representative concurrency and memory footprint without hosting heavy user code.

## Approval record

- **Decision:** extensible multi-provider remote worker infrastructure with a shared Rust adapter contract and HTTPS worker protocol.
- **Status:** Approved.
- **Approved by:** project owner in conversation, 2026-10-21.
- **Open:** first supported providers, plugin/adapter packaging, deployment and onboarding, tenancy, provider costs/quotas, regions and performance benchmarks.
- **Implementation:** none verified.

Related: [initial provider/hosting options, ADR 0007](0007-remote-worker-hosting.md), [worker provider contract](../architecture/multi-provider-adapter-contract.md), [worker lifecycle](../operations/remote-worker-lifecycle-contract.md), [testing](../development/remote-integration-test-matrix.md), [open decisions](../project/open-questions.md).
