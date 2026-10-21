---
title: In-Flight Worker Budget Policy Options
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [budget, billing, cancellation, workers, approval]
---

# In-flight worker budget policy options

## Already approved versus still open

**Approved:** when remaining approved budget is inadequate, do not start new billable attempts. Alert the owner, preserve completed work, await explicit human approval to raise the ceiling, and allow no automatic percentage overrun ([ADR 0002](../decisions/0002-hard-budget-approval.md)). **Open:** what happens to a *currently executing, previously authorized* worker when its metered spending approaches the ceiling, billing is delayed or new exposure cannot be bounded. Do not silently infer a policy.

## Financial concepts

An admission reserves conservative exposure across AI API, cloud compute, storage and transfer, then provisions paid work if authorized. Costs can arrive late or diverge from estimates. Distinguish settled invoices, held reservations, provisional accrual and unknown provider liabilities. Provider retry defaults may cause extra physical executions outside Rolvya's attempt model; such retries must be disabled or fully authorized and reconciled.

## Three candidate policies for owner review

| Policy | Behavior | Benefit | Tradeoff |
| --- | --- | --- | --- |
| A — Stop on risk (recommended to evaluate) | Cancel/fence paid operations when future exposure cannot safely fit remaining authorization, using provider stop APIs/timeouts | Stronger budget risk mitigation | Minimum billing and late fees may still apply; work may be interrupted |
| B — Finish within previously reserved bound | Allow an already admitted attempt to continue only while remaining reserved/provider-enforced maximum exposure covers it | Better completion, fewer lost builds | Requires verifiable enforceable upper bound and prompt metering/cancellation controls |
| C — Let any running task finish unbounded | Continue regardless of exposure and reconcile afterward | Simplest UX | Incompatible with reliable hard-budget intent if exposure is not capped; **not recommended** |

**Neither A nor B is approved.** A usable policy may combine conservative reservations, provider-side hard time/compute bounds, stop-on-risk and validated checkpointing, but the mechanism must be reviewed against real provider limitations. Even strict internal stop logic cannot guarantee an external provider's final invoice never exceeds the ceiling.

## Recommended safe default before approval (engineering hold)

Do not admit paid work with unbounded or unknown exposure. During design-only and synthetic tests, simulate A and B, evaluate resume/recovery and preserve artifacts. Do not use the existence of this document to authorize real billable workers or to declare an in-flight policy selected.

## State/decision table (proposal)

| Signal | New billable admissions | Running worker | Accounting/UI |
| --- | --- | --- | --- |
| Headroom sufficient and all holds bounded | May admit via SQLite atomic guard | Continue within approved bound | Show settled, held, provisional, unknown separately |
| Headroom insufficient for next attempt | **Block** | Apply separately approved A/B policy | Notify owner, retain accepted artifacts |
| Provider meter missing/stale | **Block risky new work** | Stop-or-finish policy must be explicitly configured/approved | Mark exposure **unknown**, never zero |
| Worker provider secretly retries a failed container | Deny unreserved restart where controllable | Detect/fence/reconcile extra instance | Attribute actual compute and alert drift |
| Owner explicitly increases total ceiling | Recheck all admission guards | Apply approved policy without assuming a new lease | Audit actor, prior and new ceiling |
| Worker stopped after possible remote Git push | Do not blindly retry | Fence/inspect provider | Reconcile remote ref and late bill first |

## Required provider controls for either viable policy

- Proven maximum execution duration/cancellation mechanism and a conservative estimate of cloud startup/minimum billing.
- Token/request cost limits or constrained API execution where supported; provider modes lacking such caps require conservative risk classification.
- Bounded data transfer and artifact retention; cleanup of billed idle/persistent resources.
- Idempotent launch and worker lease fencing to avoid duplicate spend.
- Accurate accrual/reconciliation audit and safe behavior on missing invoices.
- No cloud provider automatic retries that bypass a new authorized Rolvya attempt.

## Verification scenarios

1. A paid attempt is authorized with reserve R, then an unrelated late invoice arrives: no new admission, running attempt follows policy A/B, previous success remains.
2. Worker crashes after code push: release only confirmed unused exposure; reconcile Git and compute before retry.
3. Cancellation response says success but provider still bills minimum time: post actual invoice and flag variance.
4. API mode spends faster than estimation: stop/cap behavior is tested; no claim of mathematically guaranteed invoice ceiling without provider support.
5. Alert dismissed without a new authenticated approval: approved ceiling unchanged.
6. Cloud provider auto-retry creates second container: detect and attribute charge; halt unsafe new admissions.

## Explicit owner question to resolve

Should Rolvya **attempt to stop already-running billable workers when cost exposure nears the approved ceiling**, or **allow them to finish only within a demonstrably bounded, pre-reserved allowance**? The answer should be recorded in an ADR with exceptions for unknown provider metering, cancellation, refunds and checkpoint loss. Until then this topic remains **Open**.

Related: [budget reservation and reconciliation](budget-reservation-reconciliation.md), [human approvals](approval-workflow.md), [worker protocol](worker-protocol.md), [failure recovery](failure-recovery.md), [decision readiness](../project/decision-readiness.md).
