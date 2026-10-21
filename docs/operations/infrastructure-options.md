---
title: Historical Infrastructure Options and Sizing
status: Proposed
category: operations
last-reviewed: 2026-10-21
tags: [cloud, workers, vps, cost]
---

# Historical infrastructure options and sizing

## Constraints and status

Rolvya's control plane should fit a relatively small **remote** VPS; 2GB and 4GB RAM were discussed. Coding workers can execute commands, tests and builds, so host CPU and RAM needs exceed a simple model API call. No personal-computer worker or local repository synchronization is permitted. Cloud workers are favored for scaling, but no cloud vendor has been selected.

## Explored hosting modes

| Option | Role | Limitations / decisions pending |
| --- | --- | --- |
| Same VPS as control plane | Simple first experiment | Shared 2–4GB capacity limits concurrency; not the preferred scaling design |
| Third-party remote cloud workers | Scalable remote execution | Provider, pricing, isolation, warm starts and quotas undecided |
| User-provided remote host | Explicit remote host access via SSH/API candidate | Credential policy, sandboxing and lifecycle remain open |
| AWS ECS Fargate | Previously diagrammed cloud candidate | Not selected; task sizes, image start latency and pricing need validation |
| Google Cloud Run | Previously diagrammed candidate | Not selected; service/job fit, execution constraints and pricing need validation |

An earlier thought experiment considered **10 Codex CLI plus 10 Claude CLI processes on a 4GB host**; approximate memory consumption was considered too high. These were hypothetical workloads and memory guesses, **not a benchmark**. Never present 20 active workers on 4GB as proven feasible.

## Historical diagram illustrations — not current rates

One earlier diagram showed illustrative daily provider costs: VPS/remote **$0.32/day**, Cloud Run **$0.87/day**, Fargate **$2.14/day**, illustrative total **$3.33/day**. These are examples from a diagram, **not verified provider quotes, current 2026 prices, or endorsed deployment estimates**. The architecture also illustrated spot-to-on-demand fallback; only use such fallback under approved budget and provider policy.

## Shared executor interface (proposed)

Use a common remote worker capability model: provider type, CPU/RAM capacity, concurrency and quotas, secure identity, task startup/shutdown, heartbeat, lease, logging, result artifact reference, retry rules and actual resource/billing telemetry. Maintain separate control-plane and compute costs. Budget admission remains a hard gate across provider types.

## Observability expectations

Report active/ready/running/failed tasks; model/effort, worker/provider, duration, CPU and RAM, estimated versus actual time, queue pressure, completed artifacts, category-specific cost and billing uncertainty. Do not treat example dashboard figures as live metrics.

## Open decisions

Vendor rollout and regions, adapter packaging, worker pooling, **HTTPS** implementation details, use of containers, resource limits, Rust framework/crate choices, SQLite WAL/migrations, network security, startup economics, secrets/SSH policy, recovery and benchmarks. **Backend Rust, database SQLite, HTTPS worker control, GitHub App and multi-provider remote infrastructure are Approved choices**; individual cloud vendors are not.

Related: [approved Rust ADR](../decisions/0004-control-plane-language.md), [approved SQLite ADR](../decisions/0005-authoritative-database.md), [workers](../architecture/workers.md), [budget](../architecture/budget-costs.md), [security](../architecture/security.md), [historical audit](../project/historical-decision-audit.md).
