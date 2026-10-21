---
title: Configuration and Operational Policy
status: Proposed
category: operations
last-reviewed: 2026-10-21
tags: [configuration, deployment, security, cost]
---

# Configuration and operational policy

## Purpose

Describe configuration domains that a future deployment must support. This is **not** a concrete `.env` template, product API, or approved runtime defaults: storage engine, language, secret manager, provider adapter, authentication and transport remain open.

## Configuration classes

| Class | Examples | Security / ownership rule |
| --- | --- | --- |
| Control server | bind address, TLS/proxy, instance ID, health probe, scheduler loop | Operators only; secure defaults and validated startup |
| Persistence | DB connection reference, schema version, pool and durability policy | Store authentication as secret reference, never in tracked file |
| Worker pools | remote endpoint class, CPU/RAM limit, image/version, quota, region | User-authorized remote servers only; no desktop Local Agent |
| Adapter access | provider, approved model, effort, CLI/API mode, identity ref | Provider licensing/entitlements must be verified |
| Remote Git | provider, installation reference, permitted repositories/branches | Least privilege and isolation of task refs |
| Budget | per-project ceiling/currency, reservation strategy, pricing freshness | Only explicit authenticated approval changes ceiling |
| Artifact storage | bucket/namespace, retention, hashes and upload policy | Project isolation and immutable manifest refs |
| Observability | sampling, redaction, alert destinations, log retention | Auditable security/financial actions not sampled away |
| Reliability | lease duration, retry policy, timeouts, backpressure | Defaults require tests and ADRs before shipping |
| Backup/recovery | snapshot strategy, encryption, restore testing and RPO/RTO | Verified recovery before relying on deployment |

## Precedence and validation proposal

Use one authoritative server policy with strict validation, explicit versioning and an audit trail for changes. Human-approved project budget must not be overridden by environment variable, worker flag, panel reload or feature flag. Worker-supplied settings are never privileged instructions. Secrets are referenced by controlled IDs; avoid accepting arbitrary injection in shell arguments or URLs.

A configuration change that raises billable resource allocation must re-evaluate the budget gate. A retry/concurrency change must not rewrite the active DAG or bypass existing dependencies.

## Startup validation

1. Confirm schema compatibility and authoritative DB health.
2. Verify secret references are resolvable only to authorized services.
3. Check worker type is remote and provider mode allowed.
4. Verify audit/event persistence, budget admission and remote Git permissions.
5. Reject impossible lease/timeout relationships, negative/overflowing cost values and conflicting storage namespaces.
6. Enter readiness state only after required safety controls pass; degraded service must not claim successful dispatch.

## Live updates and rollback

Separate safe display preferences from high-risk authorization and execution policy. High-risk changes (cost ceilings, roles, worker credentials, concurrency, protected refs) require audit events, version checks and appropriate human authorization. On rollback, preserve settled cost entries, approved ceiling history, succeeded artifact manifests and fenced lease epochs.

## Example conceptual configuration shape

```yaml
# Illustrative keys only; not a deployed parser or sample credentials.
control:
  mode: remote
  database_ref: secret-or-connection-reference
workers:
  source: approved-remote-pool
  local_computer_integration: false
repository:
  access: remote-provider-only
budget:
  enforcement: human-approved-hard-gate
  automatic_overrun_percent: 0
observability:
  audit_events: required
```

Do not interpret the example as a supported public config format.

## Unresolved decisions

Schema and reload protocol, environment/secret precedence, encryption, identity, per-project overrides, networking, provider quotas and production defaults.

Related: [deployment](deployment.md), [secrets](secrets.md), [worker lifecycle](worker-lifecycle.md), [budget](../architecture/budget-costs.md), [open decisions](../project/open-questions.md).
