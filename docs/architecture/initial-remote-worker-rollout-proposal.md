---
title: "Initial Remote Worker Provider Rollout — Staged Proposal"
status: Proposed
category: architecture
last-reviewed: 2026-10-24
tags: [remote-worker, providers, rollout, budget, validation]
---

# Initial remote worker rollout — staged proposal

> **Status: Proposed.** This is a short rollout recommendation, **not a newly Owner-approved release order, provider deployment, price guarantee or authorization to spend**. [ADR 0018](../decisions/0018-initial-worker-adapters.md) has **already approved** these first-three adapter development priorities: Google Cloud Run Jobs, Hetzner Cloud, user-enrolled remote VPS. [ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md) already requires an extensible multi-provider Rust architecture.

## Proposed progression

1. **First fake-provider, no-billing integration tests:** prove Rust/SQLite immutable DAG, claim/heartbeat/fencing, budget reservation, outbox, cancellation/reconciliation and isolation in a synthetic environment. This does not require a live cloud account or provider model key.
2. **First real remote adapter: Google Cloud Run Jobs.** Validate a single bounded, isolated batch worker and then independent eligible leaves. Before any real execution, an authenticated Owner approves a specific test spending envelope; configure external retries to prevent unreserved provider-level executions. Do not assume the default Cloud Run task timeout or retries match Rolvya's scheduler rules.
3. **Second adapter: Hetzner Cloud.** Validate provision, remote isolated worker boot, persistent-host safety, explicit teardown and full billing life cycle. This category may suit longer or more sustained workloads, but a VM can accrue charges while idle or powered off. Do not label it cheaper without benchmarks.
4. **Third adapter: user-enrolled remote VPS.** Verify remotely supplied host identity, worker isolation and accountable capacity/billing; **never** add a Local Agent, personal-computer worker or local repository synchronization.
5. **Only after validated evidence**, decide whether public v0.1 needs one fully verified provider with the other two incrementally supported, or multiple simultaneously production-ready. **This exact public-release provider minimum is not yet approved.**

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
| **Hetzner Cloud** | Longer-running or repeatedly utilized remote VM pool | Billing may continue while a VM exists even when turned off; startup, idle occupancy and isolation matter |
| **User-enrolled remote VPS** | Use an independently enrolled existing remote host | Real host isolation, quotas, trust and non-provider-visible operating costs vary; not automatically free |

## Official references checked 2026-10-24

- [Google Cloud Run Jobs — Create jobs](https://cloud.google.com/run/docs/create-jobs): job default task timeout and retries and their configuration.
- [Google Cloud Run Jobs — Execute jobs](https://docs.cloud.google.com/run/docs/execute/jobs): execution and task lifecycle.
- [Cloud Run pricing](https://cloud.google.com/run/pricing): region/service pricing, billing and potential transfer costs; no fixed Rolvya quote.
- [Hetzner Cloud server billing FAQ](https://docs.hetzner.com/cloud/billing/faq/): hourly charges, monthly caps, billing while a server exists.
- [Hetzner 2026 price adjustment](https://docs.hetzner.com/general/infrastructure-and-availability/price-adjustment/): price/region/category changes make older screenshots unreliable.
- [Existing worker-provider evaluation](../operations/worker-provider-evaluation.md): required test matrix and reserved expenditure safeguards.

## Next Owner decision, limited in scope

**Proposed:** Implement/validate the three approved adapter categories **in ADR 0018 priority order**, but allow the first working prototype to use **one verified real adapter (Cloud Run Jobs)** while Hetzner and user VPS are added incrementally. The public stable release's mandatory provider count remains a separate explicit decision. No cloud account creation or paid tests now.
