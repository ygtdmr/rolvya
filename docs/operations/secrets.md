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

**Encryption storage (SQLite) and automatic controller-host key-file loading are approved (ADRs 0044–0045)**. Still open: exact OS file provisioning/permissions hardening, independent key recovery and rotation, crypto library, tenant isolation, worker sandbox/container restrictions, incident response contacts and provider-specific scopes.

Related: [security](../architecture/security.md), [Git integration](../architecture/integration-github.md), [deployment](deployment.md), [worker lifecycle](worker-lifecycle.md).

## Approved central credential architecture (ADR 0037, 2026-10-24)

[ADR 0037](../decisions/0037-central-secret-broker-task-scoped-access.md) approves centralized protected credential storage, a trusted Secret Broker validating active project/attempt/lease/budget authority, broker-side provider calls when feasible, and minimum-scope credential access for isolated remote executors only when technically necessary. Directly injected upstream keys can be extracted from compromised workers; audit/rotation and fail-closed isolation remain necessary. Provider subscription rights are **not** guaranteed by this architectural choice and remain OQ-008. No broker is implemented or tested.

## Owner-approved multi-account CLI grants (ADR 0038, 2026-10-24)

[ADR 0038](../decisions/0038-multi-account-isolated-cli-and-api-access.md) approves real provider CLI login for **multiple different accounts** in isolated remote sign-in homes, account-scoped protected connections and per-task private execution homes. Separate accounts can run eligible tasks concurrently, with no inherited host authentication, automatic account/API fallback or unapproved token-copy shortcuts. The [Secret Broker](../decisions/0037-central-secret-broker-task-scoped-access.md) validates per-account permission and protected credential use. CLI subscription use remains subject to provider-specific rights under OQ-008; architecture is not deployed.

## Encrypted SQLite selected for first Secret Broker (ADR 0044, 2026-10-25)

[ADR 0044](../decisions/0044-encrypted-sqlite-secret-storage.md) explicitly approves **application-layer authenticated encryption of credential records before persisting them to SQLite** under the trusted Rust Secret Broker. SQLite remains the one authoritative DB; an AWS Secrets Manager/Vault backend is **not required** for the first version. The master/key-encryption key must **not** reside in SQLite, its ordinary backups, container images, source code or logs. **ADR 0045 approves noninteractive loading of an existing separately stored, OS-protected key file from the controller host at every ordinary restart**. Exact path and secure first-time provisioning, cryptographic format, rotation and independently protected recovery remain **OQ-005/P0**; ciphertext at rest does not protect against a host compromised together with its live key. Worker-specific, account-specific access restrictions and upstream credential revocation remain required. No encryption code, live credential or deployment was created.

## Automatic key loading and startup failure policy — Approved ADR 0045

[ADR 0045](../decisions/0045-automatic-local-master-key-unlock.md): a dedicated non-root Rust service loads a protected **existing** root/key-encryption file from the remote controller host at startup. Owner-only permissions (`0600` file and `0700` containing directory or stricter/equivalent), no path under SQLite/normal backup/source/images/web roots, no read grant to remote workers or browser. Safe first-run creation must be an explicitly controlled setup distinct from ordinary restart. Missing, unreadable, incorrectly owned or invalid key means **fail closed, without generating a replacement, wiping ciphertext or starting credential-dependent work**. Ordinary restart needs no manual unlock. An independently protected key recovery/restore procedure is still necessary before live-production use, but is not a manual startup step. Exact implementation and failure drills are **unverified**.

## Encrypted off-host emergency recovery — Approved ADR 0046

[ADR 0046](../decisions/0046-off-host-encrypted-master-key-recovery.md) requires a **separately stored, encrypted recovery copy of SQLite secret-encryption key material** outside the controller and ordinary DB backups, unlockable using an **independently protected recovery secret only during authorized emergency restore**. Ordinary automatic restarts still load the existing controller key file under ADR 0045 with no human intervention. No mandatory Secrets Manager, Vault or new provider spend is authorized. Exact encrypted-kit format, custody location, version rotation, safe setup and tested recovery stay OQ-005/P0. No key material or kit is created by this decision.
