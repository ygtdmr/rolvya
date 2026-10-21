---
title: "ADR 0006: Worker Communication Transport"
status: Proposed
category: decision
last-reviewed: 2026-10-21
tags: [adr, protocol, workers, queue, security]
---

# ADR 0006: Worker communication transport

## Status and authority

**Proposed — NOT approved.** The [logical worker protocol](../architecture/worker-protocol.md) already specifies leases, fencing, idempotent results and redacted telemetry, without adopting HTTP, gRPC or a message broker. Remote workers can be cloud jobs or user-provided **remote** servers; direct personal-computer integration remains forbidden.

## Communication constraints

- Small controller (2–4 GB RAM discussed); many independent tasks may run simultaneously.
- Worker admission is subject to immutable DAG, resource/credential checks and hard budget reservation.
- No uncontrolled inbound connection into a user-provided remote host or exposed database is a prerequisite.
- Workers must survive disconnection, expired leases, at-least-once delivery, crash and delayed external Git effects.
- Budget approvals occur only in authenticated owner/control plane, never in a worker message.

## Transport comparison

| Alternative | Advantages | Drawbacks for Rolvya |
| --- | --- | --- |
| HTTPS worker-initiated pull + controller-owned DB outbox | Easy egress through NAT, simple deployment, no separate broker initially, stable request tracing | Poll load and latency; needs adaptive backoff, lease fencing and scale testing |
| Bidirectional gRPC streaming | Typed service contracts, low-latency push and heartbeats | Persistent connections, reconnect management, load balancing and remote host network constraints |
| External queue/broker + callback | Durable delivery, routing/backpressure at scale, mature cloud integrations | Additional cost/service and replay behavior, broker credentials, not automatically exactly-once |
| SSH-first remote execution | Directly reaches user-provided remote servers | Credential complexity, firewall/inbound policy, process supervision; not a general protocol replacement |

A queue or gRPC link can still be added later behind a transport adapter. None changes the single authoritative database or active DAG invariants.

## Working recommendation (subject to approval)

Start with a **remote worker-initiated HTTPS pull/claim protocol**, backed by a **transactional outbox / durable DB state**, plus authenticated progress and result endpoints. Lease/heartbeat/dedupe rules are independent of transport, making an eventual managed queue possible without changing task identities.

Do not equate simple HTTP polling with constant rapid polling. Use exponential idle backoff or bounded long polling as supported, strict payload/rate limits, health checks and jitter. Cloud one-off job provisioning may use a provider launch API; the started remote job then authenticates to report progress and results. All billable launches need budget admission first.

## Logical processing contract

1. Controller validates ready task with all predecessor results.
2. Serialize reservation, attempt creation and fence-epoch increment.
3. Write an outbox dispatch record **atomically** with the attempt.
4. Remote worker authenticates and claims work using limited scopes and an attempt-specific lease.
5. Worker heartbeats, runs scoped CLI/API agent and stores result manifest.
6. Controller verifies evidence before task success; duplicate callbacks are deduplicated.
7. Expired lease is fenced; reconcile Git/artifact/billing before retrying **same task**.
8. Resuming budget pause requires explicit user-approved ceiling change.

## Security measures

Prefer outbound TLS, short-lived worker identity, per-project authorization and token revocation. Pin immutable task context hashes; never place raw API keys in a prompt or worker log. Do not trust worker timestamps for authority. Harden server against unbounded heartbeats, polling abuse and malformed envelopes.

## Benchmark and acceptance gate

Use simulated remote workers over unreliable network conditions at representative concurrency. Measure ready-to-start latency, controller RSS/CPU, request/connection rate, pending work backlog, outbox lag, lease retries and recovered duplicates. Test network partitions, delayed responses, bad/missing auth, stale epoch, worker disappearance, controller crash and budget exhaustion. Compare gRPC/queue alternatives if candidate HTTPS exceeds the chosen SLO.

The [candidate worker wire contract v0](../architecture/worker-wire-contract-draft.md) now details message envelope, claim, heartbeat, validation, stale lease handling, duplicate delivery and budget stop events. It remains a **Proposed** HTTPS-candidate contract, not approval of a transport. The [owner technology decision pack](../project/transport-git-hosting-decision-pack.md) summarizes the pending decision.

## Open choices

Transport, broker if any, schema (OpenAPI/Protobuf), poll cadence, lease TTL, payload boundaries, callback security, provider start API, worker deployment bootstrap and exact retry timings.

For a more implementable **proposal**, see [worker transport design](../architecture/worker-transport-design.md). This does not change ADR status.

Related: [OQ-006](../project/open-questions.md), [API contracts](../architecture/api-contracts.md), [scheduler](../architecture/scheduler.md), [worker lifecycle](../operations/worker-lifecycle.md), [failure recovery](../architecture/failure-recovery.md).
