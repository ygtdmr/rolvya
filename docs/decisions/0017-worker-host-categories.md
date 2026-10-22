# ADR 0017 — Remote worker host categories

Status: **Approved**
Date: 2026-10-22

## Decision
Rolvya's multi-provider worker architecture supports three host categories under one Rust provider-adapter contract:
1. **Serverless container jobs** (e.g. AWS Fargate, Google Cloud Run Jobs, Azure Container Apps Jobs).
2. **Cloud virtual machines** (e.g. Hetzner Cloud, DigitalOcean, EC2, Vultr).
3. **User-enrolled remote servers** (remote VPS/hosts, **never the user's personal computer**, no local Git synchronization).

All types use the approved worker-initiated HTTPS claim/heartbeat/results protocol; a host type does not change the immutable DAG. The main agent specifies needs and permitted recovery options; the Rust scheduler chooses among **pre-authorized eligible** providers and resource classes, with atomic SQLite cost reservations and bounded risk.

## Shared adapter obligations
- Discover capabilities, limits, region, identity and compatible execution mode.
- Calculate and verify enforceable maximum cost exposure before billable dispatch, including setup/startup, running and cancellation uncertainties.
- Launch with durable idempotency/operation identity; inspect and reconcile ambiguous launches to avoid duplicate charges.
- Observe worker lifecycle, enforce stop/fence/cancel, cleanup and reconcile final costs.
- On failure, reattempt the **same executable leaf** only within project-approved model/provider/retry and budget limits. Independent leaves may run across multiple approved categories/providers in parallel.
- Enforce isolation, project-scoped credentials and artifact/Git security.

## Important distinctions
- Serverless services can have provider-level automatic retries and billing behavior; adapters must disable or account for uncontrolled retries.
- Cloud VMs may incur idle and provisioning charges, even without tasks; reservation policies must reflect those costs.
- User-enrolled remote hosts must verify isolation, credential authority, security, network policy and availability. They are never presumed free and cannot bypass project cost/security rules.

## Scope and validation
This ADR **does not commit to shipping every example provider in v1**. Concrete adapter order, vendor-specific capabilities, service quotas, approved billing bounds and benchmarks remain separate decisions. Test cross-category scheduling, provider outage, budget block, uncertain cancellation, stale leases and cost reconciliation.

Related: [multi-provider ADR](0011-multi-provider-worker-infrastructure.md), [multi-provider adapter contract](../architecture/multi-provider-adapter-contract.md), [inflight budget ADR](0010-inflight-budget.md).
