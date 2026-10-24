---
title: Approved Transport and Git Design; Multi-Provider Rollout Choices
status: Proposed
category: project
last-reviewed: 2026-10-21
tags: [adr, implementation, github, https, multi-provider]
---

# Approved transport, Git and multi-provider rollout: remaining decisions

## Authority and scope

On **2026-10-21**, the owner approved:
- [ADR 0006 — worker-initiated HTTPS control](../decisions/0006-worker-communication.md) (claim, heartbeat, progress, result and stop; SQLite-backed admission/outbox).
- [ADR 0008 — GitHub App authentication](../decisions/0008-github-authentication.md) with trusted Rust publication/integration.
- [ADR 0011 — extensible multi-provider remote workers](../decisions/0011-multi-provider-worker-infrastructure.md) with the same Rust scheduler/SQLite ledger and HTTPS worker protocol.

Previously approved Rust-only backend, SQLite authoritative DB, fixed up-front DAG, non-executable parent groups, hard budget gate and bounded-or-stop in-flight worker policy remain in force. No personal-computer Local Agent or Git synchronization is allowed.

**Only implementation details and first provider rollout remain undecided.** No GitHub App, HTTPS worker endpoint, cloud adapter, test environment or live spend was created by these documents.

## G1 — Worker HTTPS implementation (approved architecture)

**Decision settled:** HTTPS with worker-initiated pull/claim plus authenticated heartbeat/progress/result. This is **not** subject to a new transport vote. The [wire draft](../architecture/worker-wire-contract-draft.md) and [transport design](../architecture/worker-transport-design.md) now refine it.

**Still to decide and verify:** Rust HTTP/TLS wiring, worker identity/enrollment mechanics, exact versioned wire schema/routes, payload size, **long-poll interval and retry backoff**, load/rate limits, end-to-end cancellation and 2–4 GB controller performance evidence. **Initial worker token 15 minutes, heartbeat 30 seconds and lease 90 seconds are already approved.** No broker or gRPC-first replacement can silently supersede ADR 0006.

## G2 — GitHub App implementation (approved architecture)

**Decision settled:** GitHub App installations and trusted Rust Git integration for GitHub. The [remote publication contract](../architecture/git-remote-integration-contract.md) and [App security design](../architecture/github-app-integration-design.md) elaborate expected branch/identity protections.

**Still to decide and verify:** minimum GitHub API endpoint permissions, installation UI, token/private key store and rotation, signature checks, protected branch rulesets, isolated ref naming, PR/merge authorization and rollback. A GitHub App token scoped to a repository does **not** on its own enforce permitted branch prefixes. Installing or changing permission grants requires a distinct authorized integration setup action.

## G3 — Multi-provider infrastructure (approved architecture, open first lineup)

**Decision settled:** extensible provider-neutral Rust adapter registry and per-attempt provider binding; AWS, Google Cloud, other cloud platforms and user-provided **remote** hosts can be implemented incrementally. All workers speak ADR 0006 HTTPS and obey the same SQLite budget/fencing rules.

**Still to decide:** provider plugin/adapter packaging, initial vendor(s) and regions, user-provided remote-server onboarding, credentials, cost bounds and approved benchmark spending. The **vendor/initial rollout choice** stays **Proposed under ADR 0007**. The [multi-provider adapter contract](../architecture/multi-provider-adapter-contract.md) provides the proposed technical design.

Benchmark comparable providers with synthetic workloads before deciding priority. An initial implementation may ship one concrete provider while remaining architecturally multi-provider; that is **not** permission to hardcode one vendor into the scheduler or to promise all adapters are complete. Live provider trials must have a separate owner-approved spending ceiling.

## Compact status matrix

| Design question | Status | Remaining evidence |
| --- | --- | --- |
| Backend Rust + SQLite | **Approved, ADR 0004/0005** | Crates, migrations, performance/recovery tests |
| Parent group vs executable leaf | **Approved, ADR 0009** | Aggregation tests and policy parameters |
| In-flight bounded-or-stop | **Approved, ADR 0010** | Provider-specific caps and delayed invoices |
| Worker HTTPS pull | **Approved, ADR 0006** | Actual endpoints, auth, lease/backoff tests |
| GitHub App auth | **Approved, ADR 0008** | Exact permissions, secret lifecycle and merge policy |
| Multi-provider remote worker architecture | **Approved, ADR 0011** | Registry, adapter interface and multi-provider test evidence |
| First cloud provider/region and benchmark | **Open, ADR 0007 Proposed** | Spend authorization and measured cost/latency/security |
| Local/personal-computer worker | **Forbidden by ADR 0003** | No implementation permitted |

## Next implementation gates

1. Approve Rust library/toolchain/SQLite migration choices and worker identity/bootstrap policy.
2. Use fake remote providers and GitHub API fakes to validate HTTPS and Git operations with a single authoritative SQLite ledger.
3. Define provider registry, capability gating and per-attempt credentials; run tests on two synthetic provider adapters before any paid cloud service.
4. Verify a throwaway authorized GitHub repository with the exact approved App permission matrix and independent merge protections.
5. Request a separate explicit spend ceiling before any real cloud worker benchmark; then prioritize initial vendors based on actual recorded evidence.

## Open questions

Specific provider(s) to ship first, exact adaptation/installation mechanism, user-visible cloud-account permissions and tenancy, owner merge workflow, HTTPS wire/TLS libraries and secure worker enrollment.

Related: [open questions](open-questions.md), [decision readiness](decision-readiness.md), [implementation slices](implementation-slices.md), [project status](status.md), [remote integration tests](../development/remote-integration-test-matrix.md).
