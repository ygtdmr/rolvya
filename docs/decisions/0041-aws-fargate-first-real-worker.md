# ADR 0041 — AWS Fargate is Rolvya's first real worker integration

Status: **Approved**  
Date: 2026-10-24  
Supersedes: **the order of real-provider integration in [ADR 0040](0040-single-provider-first-working-version.md)** and the initial adapter order listed in [ADR 0018](0018-initial-worker-adapters.md).  
Preserves: the one-provider-first strategy and all other core technical, financial, and security safeguards.

## Decision

The Owner explicitly changed the initial real remote integration from **Google Cloud Run Jobs** to **Amazon ECS on AWS Fargate**. The newly approved implementation order is:

1. **No-cost synthetic fake worker integration tests** to verify Rust/SQLite, complete immutable DAGs, dependency-aware concurrency, attempt fencing, hard budget admission, and recovery.
2. **Amazon ECS on AWS Fargate** as the **first real cloud adapter** and the only real-provider integration required for the **first working version**.
3. **Google Cloud Run Jobs** added incrementally as the second real cloud adapter.
4. **User-enrolled remote VPS** added incrementally as the third adapter; personal-computer agents and local Git sync remain excluded.

The initial adapter priority list under [ADR 0018](0018-initial-worker-adapters.md) follows the same order: **AWS Fargate, Google Cloud Run Jobs, user-enrolled remote VPS**. [ADR 0039](0039-aws-ecs-fargate-replaces-hetzner.md) still records the earlier decision to replace Hetzner with Fargate, but its prior numerical ordering is now superseded by this decision.

## Implementation guidance, not new approvals

Use **ECS RunTask with the Fargate launch type** under Rolvya's own Rust/SQLite scheduler. Keep authorization, idempotency, task/attempt identity, provider inspection, retries, hard cost reservation, worker-initiated HTTPS, Secret Broker, and separate CLI/API provider entitlements under their already approved policies. Provider-managed retries, lingering costs, secrets and network restrictions must be explicitly validated.

An isolated Fargate task cannot assume privileged Docker-in-Docker or durable credentials on its ephemeral filesystem. The container image, task CPU/RAM, AWS region, IAM/network architecture and verified maximum cost exposure will be selected during implementation/security testing. No billable test or deployment is authorized by this ADR.

## Scope boundaries

- The **first working version** need not have Google Cloud Run Jobs or user-VPS adapters installed, provided the first Fargate adapter and the claimed foundational capabilities have been verified.
- The **public v0.1 release** retains its previously approved feature/safety boundaries. This ADR does not silently waive secure GitHub integration, real task output validation, human authorization, hard budgets or required panel behavior.
- Provider-neutral multi-cloud design remains required from day one; adding Cloud Run and VPS later must not change the immutable DAG or give workers additional authority.
- Real subscription CLI automation remains subject to provider-specific permission checks; an unsupported mode never silently falls back to an API key or another account.

## Future acceptance scenarios — not executed

- Fake provider integration validates dependencies, parallelism, atomic reserved spend and crashes with no live cloud resource.
- One explicitly admitted AWS ECS Fargate worker executes an existing leaf, returns scoped/fenced results, is cleaned up, and reconciles costs without any Google/VPS adapter installed.
- Cloud Run Jobs and enrolled remote VPS can subsequently be integrated without rewriting the Rust scheduling domain or budget authority.
- Failed/ambiguous ECS RunTask launches are inspected and reconciled before a new billable attempt.
- A missing Owner-approved cloud test budget prevents AWS launch; no automatic overrun or unapproved subscription/API substitution.

Related: [current adapter order](0018-initial-worker-adapters.md), [earlier one-provider rollout decision](0040-single-provider-first-working-version.md), [AWS replacement rationale](0039-aws-ecs-fargate-replaces-hetzner.md), [rollout plan](../architecture/initial-remote-worker-rollout-proposal.md), [product scope](../product/scope.md), [open questions](../project/open-questions.md).

**This ADR is an Owner-approved development-order change, not working code, an AWS deployment, or authorization to spend.**
