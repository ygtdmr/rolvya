---
title: Remaining Transport Git and Hosting Decision Pack
status: Proposed
category: project
last-reviewed: 2026-10-21
tags: [adr, choices, transport, github, hosting]
---

# Transport, Git and hosting: owner decision pack

## Purpose

Present three **remaining major technology choices** with a concise proposed default and explicit acceptance gates. They are **not approved** simply because this document exists. Approved stack/policies remain Rust backend, SQLite authoritative database, complete immutable up-front DAG, non-executable parent grouping, remote-only execution/Git, hard budget gate and bounded-or-stop on in-flight charges.

## G1 — Worker-to-controller transport (ADR 0006)

**Proposed first implementation:** **worker-initiated HTTPS** claim/heartbeat/result interactions backed by SQLite transactional outbox. Use adaptive idle waiting/backoff; no direct inbound worker port or worker-side database access.

**Why:** fits remote jobs behind NAT, small controller and simple provider-neutral error handling. gRPC/broker may be attractive later if measured throughput/latency or connection costs require it. Poll load on a 2–4GB remote controller is a real benchmark issue and must not be assumed solved.

**Approval tests:** authenticated worker scope, replay/duplicate/out-of-order events, lost leases, stop-on-risk propagation, exact budget denial, no parent group claim, 2–4GB control-plane RSS/CPU measurements and crash replay. Wire schema/codec, TLS, credentials and lease timings remain separate implementation selections.

**Owner decision requested:** approve HTTPS pull as the initial standard, reject it, or request an additional comparison.

References: [ADR 0006](../decisions/0006-worker-communication.md), [detailed wire draft](../architecture/worker-wire-contract-draft.md).

## G2 — Remote Git authentication (ADR 0008)

**Proposed first implementation:** **GitHub App installation** with repository-scoped, short-lived tokens held in a trusted Rust publisher/integrator service. AI workers submit artifact/patch manifests rather than receiving broad GitHub write tokens.

**Why:** GitHub App tokens can constrain repository and permission scope and are short-lived. **Important caveat:** Contents-write token alone does not mean branch-prefix-only permission; enforce protected ref policy separately with trusted publication/merge logic and GitHub branch rules. Verify the GitHub API endpoint permission matrix for each action.

**Approval tests:** wrong-repo installation rejected, signed/replayed webhook, token revocation, privileged merge separation, protected branch write denied, concurrent branch conflicts, stale base SHA, lost write acknowledgment, permission downgrade.

**Owner decision requested:** approve GitHub App model, reject it or request alternatives. Exact App permissions/installation setup, merge approval rule and support for self-hosted remote Git remain separate reviews.

References: [ADR 0008](../decisions/0008-github-authentication.md), [remote integration contract](../architecture/git-remote-integration-contract.md).

## G3 — Remote worker hosting (ADR 0007)

**Recommended decision posture:** do **not** select an unmeasured “cheapest cloud” yet. Define a provider-neutral Rust adapter, then compare AWS ECS/Fargate, Google Cloud Run Jobs and a user-provided remote VPS using equal synthetic workloads **after owner approves a capped test spend**. Keep heavy code build/test off the tiny control VPS.

**Why:** cold start, billing minimums, automatic provider retries, cleanup, region/CPU/memory and transfer charges can dominate nominal compute price. Cloud Run Jobs default retries and ECS RunTask idempotency/consistency must be tested within Rolvya's approved budget model.

**Approval tests:** provider startup/cleanup and limit enforcement, worker sandbox security, CLI automation entitlements, loss/reconnect and stop-on-risk, reconciled real invoice, total cost by AI API/compute/storage/transfer, pessimistic scenarios and resource profiles.

**Owner decision requested:** approve an initial provider *after measured results*; optionally designate which two cloud candidates and region to benchmark first. No benchmark run or cloud account charges are authorized by this document.

References: [ADR 0007](../decisions/0007-remote-worker-hosting.md), [provisioning contract](../operations/remote-worker-lifecycle-contract.md), [benchmark protocol](../operations/worker-provider-evaluation.md).

## Decision matrix

| Decision | Proposed direction | Status | Blocking approval |
| --- | --- | --- | --- |
| Backend language | Rust only | **Approved — ADR 0004** | None |
| Authoritative DB | SQLite only | **Approved — ADR 0005** | None |
| Non-executable group parents | Group-only + verified roll-up | **Approved — ADR 0009** | None |
| Already-running paid worker | Bounded and pre-reserved, or stop-on-risk | **Approved — ADR 0010** | None |
| Worker transport | Worker-initiated HTTPS | **Proposed — ADR 0006** | Owner transport decision |
| GitHub authentication | GitHub App | **Proposed — ADR 0008** | Owner auth/permission decision |
| Cloud worker | Provider-neutral adapter and benchmark | **Proposed — ADR 0007** | Owner live spend approval + evidence + provider decision |

## Suggested next steps *after* approval

1. If G1 approved, finalize versioned Rust worker endpoints with test-only remote identities and SQLite outbox.
2. If G2 approved, design/install a GitHub App **only with separately authorized repository scopes**; exercise write security on a throwaway remote repo.
3. Implement/execute synthetic provider adapter tests without paid cloud work.
4. Request a separately scoped live benchmark ceiling and decide G3 from actual latency, total costs and cancellation/bill reconciliation.
5. Keep all implementation claims separate from proposal/approval. No Rust controller, cloud benchmark, installed App or production endpoint is verified by these documents.

## Open adjacent questions

Owner identity/delegation, plan activation UX, selected Rust crates/runtime, SQLite migration/WAL policy, provider-specific enforceable spend caps, real network protocol timeouts, signed artifact channel and protected merge approval workflow.

Related: [open questions](open-questions.md), [decision readiness](decision-readiness.md), [implementation slices](implementation-slices.md), [current status](status.md), [ADR index](../decisions/README.md).
