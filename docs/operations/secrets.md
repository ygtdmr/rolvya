---
title: Operational Secrets and Access Boundaries
status: Proposed
category: operations
last-reviewed: 2026-10-21
tags: [secrets, security, access]
---

# Operational secrets and access boundaries

## Approved and proposed scope

Rolvya may execute arbitrary project code on **remote** workers and access remote repositories. The user explicitly excluded Local Agent, Local Sync and personal-computer repository access. Secrets handling described here is a **proposed security contract**, not currently implemented controls.

## Classification

| Material | Protection policy |
| --- | --- |
| Remote Git provider credentials | Project-scoped least privilege; short validity where available |
| Model API keys / CLI login tokens | Provider-specific authorization and protected secret reference; never in task prompt |
| Worker registration credentials | Dedicated worker identity, rotation and revocation |
| Database encryption/authentication | Restrict to control services; rotate on compromise |
| Artifact credentials | Per-project or per-attempt scoped access |
| Audit and approval records | Integrity protection; permissioned reads; redacted exports |

## Proposed handling workflow

1. Human configures connection through an authenticated interface. The control plane stores only a secret-manager reference in task-related records.
2. Before lease issuance, verify worker/task/project authorization and provider mode.
3. Issue ephemeral minimum-scope credentials or securely deliver references to an isolated executor.
4. Redact secrets in stdout, stderr, traces, telemetry, cost reports and artifacts; do not assume redaction alone prevents exfiltration.
5. Revoke on lease expiry, project disconnect, suspicious behavior or worker loss.
6. Rotate credentials and reconcile external permissions after incidents; audit grants and actual use.

## Defensive boundaries

Treat repository text and build scripts as hostile until validated. Enforce project isolation and command/network restrictions appropriate to risk. Prevent privilege escalation via injected instructions. Do not expose credentials in GitHub Actions logs, commits, model-visible messages or examples. Avoid treating unverified provider subscription sessions as transferable remote-worker identities.

## Operational incident outline

Freeze new affected privileged admissions; revoke/rotate exposed identities; isolate involved remote worker; preserve sanitized audit evidence; assess repository permissions, Git writes and cost exposure; notify authorized owner; resume only after revalidation.

## Open decisions

Secret manager and storage encryption, role-based access policy, tenant isolation, key lifecycle, worker sandbox/eBPF/container restrictions, audit retention, approved incident response contacts and provider-specific scopes.

Related: [security](../architecture/security.md), [Git integration](../architecture/integration-github.md), [deployment](deployment.md), [worker lifecycle](worker-lifecycle.md).
