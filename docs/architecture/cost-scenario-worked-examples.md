---
title: Cost Estimation Worked Examples and Hard Budget Admission
status: Proposed
category: architecture
last-reviewed: 2026-10-22
tags: [budget, costs, examples, cloud, estimates, accounting]
---

# Cost estimation worked examples and hard budget admission

## Historical request and fixed rules

The Rolvya discussion requested **realistic worked calculations for a project with N planned tasks, different worker durations and concurrent agents**, including AI provider/API and cloud worker expense. It also required a hard budget ceiling with explicit user approval before raising it, **no automatic percentage buffer**, three estimates (**optimistic, expected, pessimistic**) and four separately accounted cost classes (**AI API, compute, storage, transfer**).

This document gives **synthetic numerical examples to explain mathematics**, **not current AWS/Google/model prices**, quotes, region selection or authorization for a paid benchmark. Real quotes need provider/region/date/CPU/RAM/startup/storage/egress detail and billing evidence. Actual user-accepted limits, including a task duration planning target of ~10 minutes and a small 2–4 GB remote controller, do not establish cloud runtime caps.

## Five distinct monetary quantities

| Quantity | Meaning | Safe admission role |
| --- | --- | --- |
| Optimistic forecast | Best plausible planned spending | Informational only |
| Expected forecast | Planning midpoint/likely scenario | Informational only |
| Pessimistic forecast | Adverse but specified scenario | Informational only; may **not** bound all actual provider charges |
| Reserved exposure | Conservative, enforceable maximum for an authorized attempt/operation | **Required** for admission, plus all current liabilities |
| Actual/reconciled spend | Confirmed provider charges plus provisional/unbilled exposure | Reduce available authorization without double counting |

An attempt with an unprovable maximum cost must not be launched as a billable operation merely because its expected estimate is low. Similarly, a forecast above the ceiling doesn't itself charge anything; budget admission evaluates each separately authorized attempt and all outstanding exposure.

## Generic task-level estimate

For task `i`, provider pool `p`, scenario `s`:

```text
compute_i,s  = billed_duration(provider, startup + execution + cleanup,
               runtime_limit, billing_minimums, rounding, retries)
               × configured effective CPU/RAM/storage compute rate
model_i,s    = sum(per-model usage units × current per-unit API rates)
               or explicit subscription allocation/unknown (do not invent $0)
storage_i,s  = retained GB × days/months × storage price + request/log fees
transfer_i,s = billable egress/network units × applicable path/region rate

project_forecast_s = sum over all executable leaves and shared overhead
                     of (model_i,s + compute_i,s + storage_i,s + transfer_i,s)
```

For multiple remote providers, convert each provider to a **single approved accounting currency under a documented exchange/rounding policy** before adding totals. Do not double-charge held exposure already moved into settled cost. Group parent nodes never create their own worker compute unless there is a separately **planned executable integration leaf**.

## Synthetic example: 40 planned executable tasks

Assumptions are intentionally invented solely for arithmetic demonstration. All rows are denominated in **hypothetical USD** and do not correspond to AWS, Google, Anthropic, OpenAI or any live service.

- **40 executable leaf tasks** in one preplanned DAG; a separate group parent, if any, incurs **no independent worker attempt**.
- A purely synthetic aggregate compute billing rate of **$0.12 per worker-hour**, with minimums/rounding ignored **in this first illustration**.
- Synthetic API usage cost per task: $0.04 optimistic, $0.08 expected, $0.15 pessimistic.
- Durations: 7, 10 and 15 minutes respectively; pessimistic scenario also expects **20% extra compute and API attempts** from retries, but these hypothetical retries must each pass budget admission in a real system.
- Shared storage costs: $0.05 / $0.10 / $0.20.
- Shared transfer costs: $0.02 / $0.05 / $0.12.

| Category | Optimistic | Expected | Pessimistic |
| --- | ---: | ---: | ---: |
| AI API | 40 × $0.04 = **$1.60** | 40 × $0.08 = **$3.20** | 40 × 1.2 × $0.15 = **$7.20** |
| Compute | 40 × 7/60 h × $0.12 = **$0.56** | 40 × 10/60 h × $0.12 = **$0.80** | 40 × 15/60 h × 1.2 × $0.12 = **$1.44** |
| Storage | **$0.05** | **$0.10** | **$0.20** |
| Transfer | **$0.02** | **$0.05** | **$0.12** |
| **Total (synthetic)** | **$2.23** | **$4.15** | **$8.96** |

**Interpretation:** The expected forecast is $4.15, but if the user approves a **$5.00 ceiling**, Rolvya must **never automatically raise it to $8.96** or introduce a 5/10/20% overspend margin. A currently funded running attempt may continue only inside its enforceable pre-reserved allowance ([ADR 0010](../decisions/0010-inflight-budget.md)); a new proposed attempt is denied whenever remaining authorized exposure cannot cover its conservative maximum.

### Why concurrency is not “tasks × elapsed wall time”

With 40 independent 10-minute tasks:
- One worker, ignoring startup: approximately **400 worker-minutes**, 400 minutes wall time.
- Ten workers if all tasks and budget permit, ignoring startup: **400 worker-minutes**, perhaps ~40 minutes idealized wall time.
- Forty workers: still **400 worker-minutes**, perhaps ~10 minutes idealized wall time.
- Actual provider invoices may **differ** due to startup, minimum bill, parallel quota, preemption, retries, CPU/memory classes, storage and egress. Dependency-constrained tasks may not all run simultaneously. These are arithmetic illustrations, not throughput/SLA claims.

**2–4 GB controller RAM does not mean 10 or 40 remote CLI workers fit on that controller.** Remote capacity remains separate; a past 20-worker diagram was an example, not a confirmed limit.

## Synthetic hard admission / collision example

Use one currency in exact integer cents:

```text
approved_ceiling = 500 cents
settled_costs    = 210 cents
active_holds     = 160 cents
unreserved_provisional_or_uncertain_bound = 40 cents
available_headroom = 500 - 210 - 160 - 40 = 90 cents

new paid attempt X requires provable bound 65 cents → candidate eligible
new paid attempt Y requires provable bound 55 cents → candidate eligible
X + Y = 120 cents > 90 cents → CANNOT admit both
```

Two Rust scheduler threads must **recheck in the same authoritative SQLite write-serialization regime** and admit at most one. If a cloud invoice later arrives, reconcile it against held/provisional entries rather than counting the same charge twice. If an unknown provider liability has **no defensible finite bound**, halt new billable work until the risk is controlled or explicitly authorized.

## Different remote provider comparison

For providers A/B/C, build a comparable quote/evidence sheet:

| Input | Provider A | Provider B | Provider C |
| --- | --- | --- | --- |
| Region/time, currency and exact plan | Verify | Verify | Verify |
| CPU/RAM/ephemeral disk and isolation class | Verify | Verify | Verify |
| Startup, min billing, rounding, timeout | Verify | Verify | Verify |
| Hidden provider task retries | Detect and bound/disable | Detect and bound/disable | Detect and bound/disable |
| API model entitlement/metering | Verify | Verify | Verify |
| Storage/log/registry duration | Verify | Verify | Verify |
| Network egress and remote Git path | Verify | Verify | Verify |
| Cancellation state vs final billing | Test | Test | Test |
| Invoice vs reported usage lag | Measure | Measure | Measure |
| Maximum authorized reserved exposure | Prove before launch | Prove before launch | Prove before launch |

**AWS vs Google** cost ranking cannot be concluded from service names or illustrative older prices. Approved [ADR 0011](../decisions/0011-multi-provider-worker-infrastructure.md) allows multiple implementations, while [ADR 0007](../decisions/0007-remote-worker-hosting.md) remains Proposed on which vendors and regions ship first.

## Provider minimums, subscriptions and shared services

- A one-minute minimum billing increment means a 10-second job may still incur a minute; each provider may have different rules and charge dimensions.
- User-owned remote VPS cost is sometimes a **monthly fixed invoice**; attribute utilization separately from incremental cost. A $0 incremental compute *estimate* does not imply hosting is free.
- A paid CLI subscription has different cost semantics than API token fees; do not report “API cost = zero” as overall model cost unless a reviewed accounting policy explicitly says so and usage is eligible under provider terms.
- Image registry, logs, persistent storage, idle warm pools, inter-region data transfer and failed/disconnected attempts can continue charging after worker exit.
- Provider bill corrections may be negative; use append-only compensating entries and reviewed release/refund semantics, not negative overflow through authorization arithmetic.

## Provider-bounded stop and forecast drift

An already running worker can continue when its remaining worst-case cost is **proved** within its own reserved bound and aggregate liabilities remain within the approved ceiling. If this proof fails, record stop-on-risk, fence privileged operations, request cloud cancellation, preserve verified artifacts and continue billing reconciliation. **Acknowledged cancellation is not a verified final bill**.

Display forecasts next to actuals, pending holds, unknown exposure and remaining headroom; never silently adjust ceiling. Produce alerts when forecast scenarios or provider invoice drift increase risk, without inventing an automatic approval.

## Verification checklist

- Independent tasks on two provider adapters with different region prices: no currency/time-unit mixing.
- Concurrent X/Y admission example above admits no more than available cents.
- 40-task example reproduces exactly $2.23 / $4.15 / $8.96, with no assertion those are live rates.
- A higher pessimistic forecast never automatically raises the approved ceiling.
- Worker timeout or provider retry creates explicit cost liability rather than a new task node.
- Bounded ongoing attempt may continue while another task is denied; unbounded one triggers stop-on-risk.
- Storage/transfer on canceled attempts are not lost; invoice lag/unknowns remain visible.
- Parent/group task does not generate a separately charged worker attempt.

## Open issues

Actual supplier quotes, currency conversion/tax policy, cost allocation for monthly subscriptions and idle remote VPS, quota/cap enforceability, pessimistic scenario distributions, planned retry count and reporting refresh cadence.

Related: [budget architecture](budget-costs.md), [ledger admission](budget-reservation-reconciliation.md), [model access](model-access-entitlement-contract.md), [provider evaluation](../operations/worker-provider-evaluation.md), [scheduler policy](scheduler-selection-policy.md), [initial rollout ADR](../decisions/0007-remote-worker-hosting.md).
