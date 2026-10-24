# ADR 0037 — Central Secret Broker and task-scoped worker access

Status: **Approved**  
Date: 2026-10-24

## Decision
Rolvya centrally stores and protects user-authorized AI API keys and integration credentials. A trusted Rust-side **Secret Broker** validates the project's approved provider mode, task attempt, worker identity, active lease and budget admission before authorizing the least access necessary. Workers never receive credentials for another project or attempt. Secrets are not included in AI prompts, ordinary task memory or logs.

**Prefer brokered/proxied model requests that keep provider credentials outside the worker.** Where a supported CLI or official integration genuinely needs direct credentials, deliver only to an eligible isolated remote execution environment with the narrowest feasible scope and lifetime; revoke or rotate as appropriate when access is ended or compromised. A directly exposed long-lived upstream API key cannot be made non-extractable merely by a local token or worker cleanup; treat host trust, outbound network and billing exposure as residual risks. Where isolation or bounded spending cannot be verified, deny new dispatch.

Every attempt retains its own verified worker identity and fenced lease. The approved **15-minute worker access token, 30-second heartbeat and 90-second lease** are unchanged; these control-plane tokens are **not AI-provider API keys**. Workers cannot approve budgets, mutate the fixed DAG, receive GitHub App signing keys, access personal computers or impersonate project Owners. The hard Owner budget approval gate and already-approved provider-aware remote isolation remain in force.

Provider subscription/CLI entitlement remains a **separate open P0 decision**: a stored Claude/Codex session does not imply permission to use the subscription in third-party unattended remote automation. Unsupported modes must be blocked rather than silently using paid API or copying user login credentials.

## Open implementation details
Physical secret manager/encryption keys, broker routing, provider-specific scopes and revocation, confidential CLI credential injection, worker-host attestation, precise isolation/runtime restrictions and audit interfaces are selected during implementation/security validation, not silently settled by this ADR.

## Links
- [Worker identity and lease policy](../architecture/worker-authentication-policy.md)
- [Provider-aware worker isolation](0020-provider-aware-worker-isolation.md)
- [Operational secrets](../operations/secrets.md)
- [Model CLI/API entitlement](../architecture/model-access-entitlement-contract.md)

This ADR documents an Owner-approved architecture, **not** an implemented or security-tested credential broker.
