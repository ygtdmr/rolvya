---
title: "ADR 0006: HTTPS Worker-Initiated Communication"
status: Approved
category: decision
last-reviewed: 2026-10-21
tags: [adr, protocol, workers, https, security]
---

# ADR 0006: HTTPS worker-initiated communication

## Status and approval

**Approved by the project owner on 2026-10-21.** The worker-to-controller transport for Rolvya is **HTTPS with worker-initiated pull/claim, heartbeat, progress and result submission**. The earlier proposed alternatives (gRPC-first, external broker as the primary worker control channel and SSH-first execution) are not selected. This is a **design approval, not a deployed protocol or tested endpoint**.

## Context and rationale

Rolvya has one Rust backend with SQLite as the only authoritative DB, expected to coordinate many independently ready **remote** workers from a modest control VPS. Workers may run on different cloud providers or user-supplied remote servers, including hosts behind NAT. No personal-computer Local Agent or local repository synchronization is allowed.

Worker-initiated HTTPS can operate using outbound connections without requiring a control-plane connection into each worker. It reduces the initial number of operational dependencies and provides ordinary request/response tracing. Adaptive idle backoff or bounded long polling is necessary to avoid busy-loop polling and excessive SQLite/API work. Network/HTTP stack, serialization, polling intervals and SLOs remain engineering choices.

## Decision

1. **Worker control communication uses TLS-protected HTTPS initiated by enrolled remote workers.** The Rust controller publishes authenticated claim, heartbeat, progress, result and stop/lease-check operations. No worker opens SQLite directly.
2. The controller first commits an **atomic SQLite** admission: validated executable-leaf task, budget reservation, fenced attempt identity, audit and durable dispatch/outbox entry **before a paid provider launch**. The outbox/dispatch provider API is separate from the worker HTTPS control channel.
3. Workers claim only **pre-existing, eligible executable leaves**. Group parents are not leased ([ADR 0009](0009-parent-aggregation.md)). A claim never authorizes creating additional tasks, changing the immutable DAG or increasing a project budget.
4. Every write/callback requires authenticated worker/project scope, current attempt identity, monotonic lease/fencing epoch, message idempotency and server-authoritative state checks.
5. Worker-visible instructions/context contain immutable hashes and permitted scope, **not** long-lived GitHub/cloud keys or human approval capabilities.
6. Duplicate messages, lost responses, expired leases, restart and external side-effect uncertainty are handled by **durable SQLite state and reconciliation**; HTTPS does not imply exactly-once remote execution.
7. **ADR 0010** applies: previously authorized attempts may continue only within enforceable, pre-reserved exposure; otherwise the controller issues stop-on-risk/fencing, requests provider termination and reconciles late charges.
8. HTTPS supports workers from **multiple cloud/provider adapters** without rewriting the task DAG or budget authority (see [ADR 0011](0011-multi-provider-worker-infrastructure.md)).

## Transport boundaries

- **Approved:** worker-initiated HTTPS control channel and Rust/SQLite state authority.
- **Not selected:** exact Rust HTTP/TLS crates, enrollment/attestation implementation, OpenAPI/JSON or other codec, URL paths, claim/long-poll settings and provider bootstrap. **Subsequent worker authentication policy approved a 15-minute access token, 30-second heartbeat and 90-second lease**; the concrete protocol implementation remains open.
- A separate provider launch/inspect/cancel adapter may call a provider API via its native HTTPS/SDK/remote-host transport; that does **not** replace the worker's approved HTTPS controller channel.
- A future gRPC or broker **replacement for the worker control channel** would require a superseding owner-approved ADR. A supplementary internal queue may be evaluated without bypassing SQLite authority or this HTTPS channel.

## Security and failure acceptance

| Test | Required result |
| --- | --- |
| Worker has wrong project ID or invalid identity | Deny and audit without data disclosure |
| Worker claims a group parent | Reject; no attempt or cloud launch |
| Old attempt reports success after lease fenced | Reject authority; preserve validated later attempt |
| Request/result duplicated or out of order | Idempotent logical state; no duplicate charge or accepted outcome |
| SQLite admission cannot commit | No newly billable provider launch |
| Provider created job but response was lost | Reconcile external operation before any second paid launch |
| Many workers idle | Backoff/long poll prevents unbounded controller and DB pressure |
| Budget no longer covers future worker exposure | Apply stop-on-risk policy, preserve verified artifacts and reconcile |

## Alternatives and consequences

gRPC streaming can reduce certain latency overheads but requires persistent connections/reconnect behavior; external brokers can help fan-out but add infrastructure and replay complexity; SSH-first is not a universal NAT-safe worker control channel. These were considered and are not approved as the initial worker-to-controller standard.

## Approval record

- **Choice:** remote-worker-initiated HTTPS pull/claim + heartbeat/progress/result and durable SQLite outbox.
- **Status:** Approved.
- **Approved by:** project owner in conversation, 2026-10-21.
- **Pending implementation:** authenticated wire schema, Rust server/client code, bounds, telemetry, reliability tests and deployment.
- **No implementation claim:** no live HTTPS worker interface has been verified.

Related: [logical protocol](../architecture/worker-protocol.md), [HTTPS implementation draft](../architecture/worker-wire-contract-draft.md), [worker transport design](../architecture/worker-transport-design.md), [multi-provider ADR](0011-multi-provider-worker-infrastructure.md), [integration tests](../development/remote-integration-test-matrix.md).
