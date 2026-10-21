---
title: Budget Reservation, Metering and Reconciliation Contract
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [budget, sqlite, reconciliation, approvals, billing]
---

# Budget reservation, metering and reconciliation contract

## Authority and non-negotiable policy

The product owner has **approved** the project budget as a hard admission gate: when funds are insufficient, stop starting **new billable work**; alert the owner; preserve completed work; resume new billable admission only after explicit approval to increase the limit. No automatic percentage overrun is allowed ([ADR 0002](../decisions/0002-hard-budget-approval.md)). The backend is **Rust only** ([ADR 0004](../decisions/0004-control-plane-language.md)); authoritative persistence is **SQLite only** ([ADR 0005](../decisions/0005-authoritative-database.md)).

This document proposes financial state transitions and provider integration. In-flight cancellation semantics, refund/late-billing policy, accounting currency, taxes, exchange rates and service-level cost limits are **not yet approved**. An application-layer reservation cannot physically prevent a provider from submitting late or larger charges: product UI must distinguish *authorized spend*, *reserved exposure*, *provisional accrued cost*, *settled billed cost* and *unknown exposure*.

## Terms and cost dimensions

| Term | Meaning | Inclusion in headroom |
| --- | --- | --- |
| Ceiling C | Total amount last explicitly approved by an authorized human | Upper admission boundary; not increased by automated jobs |
| Settled spend S | Provider-confirmed or reconciled monetary charge, including corrections | Reduce headroom |
| Held reservations H | Conservatively reserved cost for dispatched or scheduled billable attempts | Reduce headroom |
| Provisional unbilled exposure U | Estimated additional charge already incurred but not yet settled or held | Reduce headroom until reconciled; avoid double counting |
| Proposed exposure E | Conservative bound for a candidate new billable operation, including startup/minimum billing and data transfer where relevant | Must fit remaining headroom |
| Scenario forecast | Optimistic / expected / pessimistic whole-project estimate | For planning only; **not** permission to charge |

The ledger **must separate** `ai_api`, `compute`, `storage`, `transfer`. If an operation spans categories, track category-specific reservations/charges and aggregate a safe project-level total. If spending currency differs across providers, no admission may assume a conversion without an approved conversion/rounding policy.

### Budget headroom invariant

```text
Available = C - S - H - U
Admit new billable operation only when:
  C is explicitly human-approved,
  E has a defensible conservative cap or hard provider enforcement,
  provider access and task dependencies are valid,
  and E <= Available, atomically rechecked with attempt/dispatch creation
```

**Avoid double counting:** after an invoice settles, move exposure from a reservation/provisional bucket to settled entries transactionally. Do not independently add a settled charge and keep the same already-covering hold forever. For unknown or unbounded provider exposure, mark `cost_uncertain`, deny new billable admissions, and request policy/owner intervention rather than claim the remaining budget is safe. Zero reported usage from a delayed provider feed is not proof of zero spend.

## Transactional admission (proposed, SQLite)

1. A Rust scheduler evaluates the fixed DAG and confirms the task is eligible, the desired remote worker is authorized and conflicts are isolated.
2. Determine total reserved exposure `E`: known provider minimum duration, estimated maximum task runtime/size, model API budget, storage and transfer where chargeable. **Planning target ~10 minutes is not itself a billing cap.**
3. Open a short SQLite write transaction (e.g. `BEGIN IMMEDIATE` candidate). Read current ceiling/version, booked costs, outstanding holds and uncertain provider exposure.
4. Recheck project policy and `E <= available` *inside* that serialized transaction.
5. Atomically create a new attempt of the **same pre-existing task**, increment fencing epoch, hold funds, append audit event and write a durable dispatch-outbox command with idempotency key.
6. Commit. A dispatcher reads the outbox only after commit, starts the remote worker and records the provider operation ID; attempt retries are new **attempts**, not new graph nodes.
7. If provider launch result is ambiguous, look up the existing provider job/task and its cost exposure **before** issuing a new paid launch.

A denied transaction must not create a billable cloud job, an API call or an idle warm worker. All billable control-plane background work (including benchmarking) must have the same admission discipline.

## Reservation state proposal

`requested → held → (partially_settled / reconciled / released / disputed)`. The final enum is not selected. A hold may be **partially consumed**; evidence is append-only. Release only documented *unused* exposure, not an unconfirmed possible invoice. If billing feed arrives after release, record correction, flag drift and prevent additional speculative admissions until reconciliation.

Approval request states: `pending → approved / denied / expired`. Only a separately authenticated actor with explicit budget-increase authority may transition to approved; recorded amount, currency, previous and new ceilings, actor, reason, timestamp and idempotency key are mandatory. Alert dismissal, worker status and proposal acceptance must not change approved ceiling.

## In-flight operations: decision deliberately open

The previously agreed rule stops **new billable admissions** on insufficient headroom; it does not yet define whether an already running paid worker must be killed or allowed to finish. Two policies need explicit owner evaluation:

| Candidate | Benefit | Risk |
| --- | --- | --- |
| Stop/terminate in-flight attempts when exposure nears ceiling | Stronger attempt-level cap | May lose uncheckpointed effort, still incur minimum or delayed fees |
| Allow already authorized reserved attempts to finish within bounded reserved exposure | Preserves progress | Requires trustworthy provider-side duration/size enforcement and conservative reservations |

Neither is approved. The implementation must not silently choose or promise zero overrun without provider-side enforcement. A cancellation must fence lease authority, persist artifacts and reconcile cloud charges/Git side effects.

## Example (illustrative exact integers, no live rate)

All amounts are synthetic integer minor units in **one currency**:

```text
approved ceiling: 20000
settled costs:     9200
active holds:      5000
provisional delta:  800
remaining:         5000
candidate exposure: 6500 -> DENY new paid launch
candidate exposure: 4200 -> ADMIT only if all other guards and a safe bound hold
```

A late 1700-unit invoice is **not** a reason to retroactively imply authorization or silently add a budget buffer; post and reconcile it with any corresponding held/provisional entries, preserve the audit trail and surface variance.

## Failure scenarios and expected behavior

| Scenario | Required result |
| --- | --- |
| Two scheduler threads reserve against last headroom | At most one wins serialized SQLite admission |
| Provider charges twice due to hidden retry | Detect duplicate execution; reconcile charges; policy blocks unsafe new work |
| Worker dies after remote Git push | Investigate Git ref and provider job before a second paid attempt |
| Stripe-like notification or UI acknowledgment | No project ceiling change |
| Repeated invoice callback | Idempotent ledger posting; same charge not counted twice |
| Worker size or region changes | Recalculate reservation; reject if additional authorized exposure absent |
| SQLite is busy/unavailable | **Fail closed** for billable work, preserve recorded approved state |
| Invoice feed missing or unbounded estimate | Mark unknown and halt unsafe new admissions |
| Project budget paused | Verified work, immutable DAG, artifacts and decision history remain accessible |

## Testing and validation

Build deterministic fake billing providers, randomize completion/invoice ordering, and fault-inject at each SQLite transaction and external-launch step. Assert no unauthorized new billable attempt is admitted, attempts remain on the same graph, approved ceilings change only with explicit human actions, and late provider events are detectable. Live tests require a separately approved spending envelope.

## Open decisions and sources

Owner approval required for **in-flight cancellation**, currency conversions, liability/hold limits, provider-specific caps and refund semantics. Implementation choices: SQLite transaction locking, exact schema, billing driver, stale reservation expiry, provider ledger reconciliation latency and alert thresholds.

Related: [approved budget ADR](../decisions/0002-hard-budget-approval.md), [SQLite blueprint](sqlite-schema-draft.md), [scheduler](scheduler.md), [failure recovery](failure-recovery.md), [worker provider evaluation](../operations/worker-provider-evaluation.md), [user flows](../product/user-flows.md), [open questions](../project/open-questions.md).
