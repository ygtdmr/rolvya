---
title: Remote Workers and Model Access
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [workers, models, infrastructure]
---
# Remote workers and model access

## Design boundary
The control server should remain lightweight (a 2–4 GB RAM VPS was considered), while computationally demanding coding workers run on remote cloud capacity or a user-provided remote server. Running workers on the control VPS was also considered but is not the preferred scalable choice. **No Local Agent, Local Sync or personal-computer repository integration.**

## Agent roles
The main agent plans the full hierarchy once before execution, chooses an AI model and effort per task, monitors outcomes and diagnoses failures. Sub-agents execute the given scoped task and its acceptance criteria. They do not create new nodes or recursively spawn planners.

## Access modes discussed
CLI-based coding agents include Claude Code CLI and Codex CLI. A previously stated product constraint distinguished subscription-only users (CLI execution, where supported) from API-key users (API mode). The exact per-provider entitlement/legal constraints, security mechanism, compatibility, billing and model taxonomy need fresh verification before treating this as a complete adapter contract. Store model, effort and mode as explicit task fields.

## Remote execution contract (proposed)
A worker receives a bounded task description, immutable DAG/task identity, compact reference context, remote repository checkout scope, allowed operations, resource/time limits, secret references, verification criteria and output artifact destination. It returns lifecycle signals, test evidence, result manifests, Git refs, structured errors and measured billable usage.

## Safety and reliability
Remote workers should have project-scoped short-lived credentials, independent workspaces, memory/CPU quotas, network and command restrictions appropriate to code execution, redacted logs, auditable actions and revocable leases. Never put raw secrets in task prompts or commit them to Git. Runtime capacity must account for executable commands and CPU/RAM overhead—not only API latency.

## Hosting candidates, not selections
Third-party cloud workers, AWS and other provider services, and self-managed remote hosts were evaluated. Provider, operating system, sandbox tech, pooling/warm start and pricing models are still open. Any illustrative 20-worker or <=10-minute figures are **examples or planning targets**, not universal guarantees.

## Open decisions
Worker provisioning/cleanup, cloud provider selection, CLI licensing and automation policy, API adapters, sandboxing, ingress/egress, network identity, concurrency quotas, model fallback authorization, state heartbeats, logs retention and artifact credentials.

Related: [overview](overview.md), [execution](execution.md), [security](security.md), [budget](budget-costs.md).
