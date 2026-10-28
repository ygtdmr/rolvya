---
title: Worker Authentication and Lease Policy
status: Approved
category: architecture
last-reviewed: 2026-10-22
tags: [worker, authentication, security, token, lease, rust, sqlite]
---

# Worker authentication and lease policy

## Decision (approved 2026-10-22)

Rolvya uses **worker-initiated HTTPS**, verified remote-worker enrollment/identity, and **short-lived, task-attempt-scoped access tokens**. The user approved this direction and initial timing parameters in conversation on 2026-10-22.

**Initial defaults (approved, subject to load/security testing):**
- Worker access token lifetime: **15 minutes**.
- Worker heartbeat interval: **30 seconds**.
- Worker lease duration: **90 seconds**.

These are defaults, not a guarantee of immediate provider termination or a substitute for budget-exposure enforcement. Subsequent changes should be recorded as a new decision.

## Trust boundaries

1. Provider adapter launches a remote worker after authorized, budget-reserved dispatch. There is **no local desktop agent**.
2. Worker bootstrap uses a provider-native verifiable identity/attestation if supported, or a pre-enrolled remote-host identity with securely provisioned credentials. **Do not issue a privileged task token solely because a worker supplies a worker_id.**
3. Controller maps verified bootstrap identity to authorized provider account, pool, project, and particular **existing executable leaf** attempt; verifies current `attempt_id`, `lease_epoch`, budget reservation, and enrollment status from SQLite.
4. Controller issues a cryptographically secure, short-lived opaque bearer credential bound in server-side storage to worker identity, project, task, attempt, current lease epoch, narrow actions (claim/heartbeat/progress/result), and expiry. Store only a token hash; send token over TLS; redact from logs and model context.
5. Every endpoint validates credential, exact task/attempt scope, expiry, current lease epoch, and server-authoritative status. A token cannot authorize a new DAG node, a budget increase, GitHub merge, or work on a different project/attempt.
6. Token refresh requires a valid authenticated worker identity and revalidation of the active attempt and budget/lease state; no indefinite refresh of fenced/cancelled attempts.
7. On stop, cancellation, revocation, lease expiry or epoch fencing, deny further privileged operations. **Token expiry alone is insufficient**: enforce immediate server-side revocation/fencing.
8. Raw AI API keys and GitHub App private keys/installation tokens are not placed in model-visible prompts. If a task needs AI access, the execution environment requires separately approved, tightly scoped secret delivery or an authenticated broker, with usage/budget metering; never treat the worker control token as an AI API token.

## Lease timing and safety

A worker normally sends heartbeat every 30 seconds; the controller initially grants/renews a 90-second lease on accepted heartbeats, measured by server time. A missed heartbeat is not instant failure; after lease expiration, the controller fences the epoch, inspects actual provider state, reconciles external Git/cost side effects, and only then considers a newly budgeted retry. Network partitions must never allow two attempts to publish simultaneously. Lease fencing blocks controller-authorized work but does not prove that a cloud process has stopped or billing has ceased; explicit provider cancellation and cost reconciliation remain necessary (ADR 0010).

Keep token expiry (15 min) independent from lease TTL (90 sec); refresh a token before expiry only after verifying identity and active lease. Rate-limit authentication and claims; minimize network egress; audit rotation/revocation without leaking credential material.

## Acceptance criteria

- Unknown or impersonated worker receives no attempt token.
- Cross-project/cross-attempt and expired/revoked tokens are rejected; stale epoch cannot write a result.
- Heartbeats renew only the current attempt's lease and duplicate messages are idempotent.
- Network loss beyond lease expiry fences the old worker; subsequent provider cancellation and financial reconciliation are separately observable.
- Renewal does not bypass explicit budget approval and cannot resurrect a stopped attempt.
- Provider-native and pre-enrolled remote host bootstrap both meet the same controller authorization contract.
- Secret-bearing errors and logs are redacted; replay and stolen-token risks are tested.
- Defaults are validated under controller CPU/RAM constraints and provider latency.

## Current implementation boundary

[S2-04 Rust/SQLite implementation](../development/verified-worker-bootstrap.md) adds keyed attempt-scoped credentials, independently verified identity interfaces, renewal/revocation and same-transaction R08 message authorization. Public construction remains default-deny until reviewed provider attestation, HTTPS and credential-delivery adapters exist. No native AWS identity or live TLS behavior is claimed. Existing trusted synthetic kernels are not network authentication endpoints; enrolled attempts cannot use their legacy policy callbacks.

## Remaining implementation decisions

Concrete native provider attestation, authenticated endpoint routes, TLS termination, gateway rate limits and load-tested timing adjustments remain integration gates. S2-04 implements installation-keyed HMAC-SHA256 token commitments, versioned revocation storage and bounded rotation; these library mechanisms do not complete production authentication. Authentication design alone does not certify an initial cloud provider.

See [worker wire contract](worker-wire-contract-draft.md), [identity and authorization](identity-authorization.md), [remote worker lifecycle](../operations/remote-worker-lifecycle-contract.md), [bounded spend ADR](../decisions/0010-inflight-budget.md), and [multi-provider ADR](../decisions/0011-multi-provider-worker-infrastructure.md).
