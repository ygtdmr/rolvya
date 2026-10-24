# ADR 0040 — First working version with one real remote provider

Status: **Approved**  
Date: 2026-10-24

## Decision

The Owner explicitly approved a **staged first working version** of Rolvya:

1. **Start with Rust/SQLite and synthetic/fake remote-worker integration tests**, verifying immutable preplanned DAGs, parallel independent leaves, dependency waits, atomic hard-budget admission, fenced leases, durable events/outbox and safe recovery without paid model/cloud use.
2. **Bring up the first real remote worker integration with Google Cloud Run Jobs**, initially in a small, explicitly authorized and cost-bounded test. The **first working Rolvya version need not wait for all three approved adapters**. It must still meet applicable core project, approved budget, secure identity, isolation and correctness requirements for its claimed working scope.
3. **Add Amazon ECS on AWS Fargate and user-enrolled remote VPS incrementally**, retaining their previously approved adapter development order under [ADRs 0018](0018-initial-worker-adapters.md) and [0039](0039-aws-ecs-fargate-replaces-hetzner.md). Do not require local/personal-computer agents or repository synchronization.
4. **Use a provider-neutral Rust adapter interface and SQLite financial authority from day one.** Selecting one concrete adapter first does not remove multi-provider support from the product roadmap or authorize a single-provider-specific domain schema.
5. **Keep first-working demonstration distinct from public production certification.** A fake-only demo is not a secure publicly releasable MVP. The previously approved [v0.1 product boundary](../product/scope.md) still includes secure real remote execution, remote Git, hard budget, verification and basic web panel; release gates, multi-provider public rollout timing and production safety must be validated before any release claim.
6. **No implicit spend or provider licensing approval.** Cloud account setup, live benchmark expenditures, chosen region/image/CPU/RAM, real CLI subscription rights and paid API mode all require their existing separate provider/authentication/budget gates. Subscription CLI use blocked by third-party rules is not silently replaced by billable API calls.
7. **Future adapter addition is a normal extension:** Do not alter active user task graphs or replace existing project budgets when registering a new provider. Launch/retry only within the original task, permitted account/model and a fresh authorized financial reservation.

## Why

This avoids delaying a working end-to-end product on three cloud integrations and enables quicker validated feedback, while preserving the architecture's multi-provider, isolated-account and hard-budget invariants.

## Explicit non-decisions

This ADR does **not** choose a cloud region, task image or size, frontend framework, release date, provider-specific maximum spend, number of production workers, model provider terms, or exact production certification conditions. The staged rollout's provider benchmarking and release-specific settings remain tracked under OQ-010.

## Future acceptance tests (not run)

- Fake-provider integration demonstrates concurrency/dependencies, budget hard stop, restart/replay fencing and no provider expense.
- A Cloud Run Jobs worker can execute one approved real remote attempt safely and record a validated result, with authorized cost exposure and cancellation/reconciliation.
- A working one-provider demonstration is not rejected solely because AWS Fargate and remote VPS adapters are not yet integrated.
- Additional adapters can be registered without changing the immutable DAG, SQLite budget model or existing worker protocol; provider identities and model account grants remain isolated.
- No cloud/API resource is started before explicit expenditure authorization; no unsafe subscription entitlement bypass.

Related: [staged rollout document](../architecture/initial-remote-worker-rollout-proposal.md), [approved first adapters](0018-initial-worker-adapters.md), [AWS Fargate selection](0039-aws-ecs-fargate-replaces-hetzner.md), [multi-provider Rust architecture](0011-multi-provider-worker-infrastructure.md), [product MVP scope](../product/scope.md), [open decisions](../project/open-questions.md).

**Documentation approval only. No code, cloud deployment or paid test is implied.**
