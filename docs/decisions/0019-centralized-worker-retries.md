# ADR 0019 — Centralized Worker Retry Ownership

Status: **Approved**
Date: 2026-10-22

## Decision

Rolvya, rather than an independent cloud provider retry mechanism, owns execution retry decisions. The **main agent diagnoses** failures and selects eligible same-task recovery strategies; the **Rust scheduler authorizes** each new paid attempt under the approved fixed DAG, project-configured attempt cap (default: 3 *total*, including initial), model/effort/provider permissions, and SQLite hard budget reservation.

- Disable provider-native *execution/task retries* whenever supported, and verify the deployed setting (e.g. set Google Cloud Run Jobs task max retries to zero where available).
- Do not confuse provider API-call retries with starting another billable job. Transport-level retries must preserve idempotency/operation identity and reconcile uncertain outcomes.
- If provider-native retries cannot be disabled, the adapter must measure and enforce the **full worst-case retry multiplicity and cost exposure** within an explicit preapproved reservation and attempt policy. If it cannot reliably guarantee the limit, **do not admit billable work** on that configuration.
- Any provider-restarted physical execution counts toward the approved number of actual execution attempts, even if the provider API uses different terminology. Never silently get extra paid attempts beyond the authorized cap.
- The same logical executable leaf retains its task ID; an authorized Rolvya reattempt receives a new attempt ID, lease epoch and budget hold. Group parents do not run; graph topology stays fixed.
- A missing launch acknowledgement, network timeout, unconfirmed provider cancellation or unknown external charge must be inspected and reconciled before another paid launch. Provider stop acknowledgements alone are not final billing evidence.
- On risk of exceeding a bounded reservation: fence privileged work, request stop, retain verified artifacts and reconcile without any automatic budget increase.

## Provider coverage

Applies to Google Cloud Run Jobs, Hetzner Cloud, enrolled user-owned remote VPS and future adapters. For VMs and user-owned hosts, process supervisors and job runners must not silently restart chargeable or AI API-consuming work outside Rolvya authorization.

## Acceptance checks

1. Configured provider-native retry count is zero where supported, validated against effective deployment configuration.
2. Failed attempt is diagnosed by the main agent; only scheduler makes a new admitted attempt and reserves exposure atomically.
3. Lost provider response cannot create duplicate billable execution from an unguarded API retry.
4. A provider that cannot bound hidden automatic retries is rejected for paid tasks.
5. Recovered system reconciles external attempts, invoices and leases before restarting billable work.
6. Project default of three total attempts is enforced; changing it during project preflight is permitted within approved budgets.

Related: [retry policy](../architecture/retry-escalation-policy.md), [first worker adapter plan](../architecture/initial-worker-adapter-implementation-plan.md), [budget limits](0010-inflight-budget.md), [initial adapters](0018-initial-worker-adapters.md).
