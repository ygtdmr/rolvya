# ADR 0012 — REST API framework

Status: Approved
Date: 2026-10-22

## Decision

Rolvya uses **Axum** as the Rust REST API framework for the control plane. The user's approval was given on 2026-10-22 after evaluating alternatives including Actix Web and Cloudflare Pingora.

## Rationale

Axum integrates with Tokio, Hyper and Tower and suits worker authentication, claim, heartbeat, lease, results, and user-facing control APIs. Pingora primarily targets proxy/network services rather than typical REST application routing. Axum has not been proven the fastest or lowest-RAM option for Rolvya's workload.

## Validation

Benchmark the real API under 2–4 GB controller memory with 100, 500, and 1000 simulated workers; measure RAM, CPU, p95/p99 latency, errors, SQLite contention and budget-admission correctness. Compare against Actix Web if performance objectives are missed. No benchmarks are claimed to have been run.

## Unresolved

**SQLx was subsequently approved in ADR 0013.** Runtime tuning, TLS deployment and middleware specifics remain implementation decisions. Axum and SQLx selections are closed.
