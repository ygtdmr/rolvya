---
title: Security Boundaries and Trust Model
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [security, git, secrets]
---
# Security boundaries and trust model

## Scope and known constraints
Rolvya orchestrates coding agents that may execute commands on **remote** workers and write to remote Git repositories. Running arbitrary code is a high-trust-risk action. The user explicitly excluded a Local Agent/Local Sync or access to personal-computer repositories. This document is a proposed threat-model foundation, not a claim of existing protection.

## Trust boundaries
- Human owner and explicit approval UI.
- Control plane, single authoritative database and budget authorization.
- Planner/model outputs and worker prompts (untrusted instructions/data).
- Remote worker execution sandboxes.
- Remote Git providers and artifact storage.
- External model/API providers and billing services.

## Threats requiring controls
Prompt injection from repositories/issue text; credential exfiltration; malicious code execution; cross-project workspace leakage; unauthorized repository writes; compromised worker images; uncontrolled cloud spending; replayed task messages; duplicate Git operations; forged approval events; leaked logs/artifacts and stale leases.

## Proposed safeguards
Least-privilege scopes; per-project secret isolation; server-side permission checks; short-lived secrets and worker identities; signed/fenced attempt identity; outbound network restrictions; sandbox resource controls; audited approvals; immutable event logs; idempotent state changes; remote branch review policy; scanning/redaction of logs and artifacts; encryption in transit and at rest as appropriate. Specific mechanisms require threat review and ADR approval.

## Explicit non-goals
No endpoint that reaches into the user's personal computer; no personal desktop Git synchronization; no default worker host on the user's laptop.

## Open decisions
Authentication provider, role model, tenant isolation, key vault, sandbox threat level, supply-chain controls, release signing, secrets rotation, incident response, user audit retention and Git permission escalation.

Related: [overview](overview.md), [workers](workers.md), [execution](execution.md), [decision register](../decisions/README.md).
