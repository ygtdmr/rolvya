---
title: "ADR 0003: Remote Git Only"
status: Approved
category: decision
last-reviewed: 2026-10-21
tags: [adr, git, privacy]
---

# ADR 0003: Remote Git only

## Status and authority

**Approved scope decision**, previously confirmed by the user.

## Context

Personal-computer integration introduces local access, synchronization and credential risks that the product owner explicitly does not want.

## Decision

Rolvya works with Git repositories through remote services, including prospective self-hosted providers. It does **not** provide a Local Agent, Local Sync, or direct access/synchronization with repositories on the user's personal computer. Remote control-plane VPS and remote cloud/self-managed worker hosts remain in scope and should not be confused with personal-desktop integration.

## Consequences

Git adapters and worker workspaces must use remote repository references; tests must reject local-computer integration; documentation and onboarding must not propose local sync as an option.

## Not decided

Provider priorities, GitHub authorization mechanism, supported self-hosted providers and merge policies.

## Alternatives considered

Local Agent and Local Sync were explicitly rejected.

Related: [GitHub integration](../architecture/integration-github.md), [security](../architecture/security.md), [workers](../architecture/workers.md).
