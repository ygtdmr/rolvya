---
title: CLI and API Agent Access Modes
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [cli, api, codex, claude, access]
---

# CLI and API agent access modes

## Historical product rule

Earlier discussions specified CLI coding-agent execution (Claude Code CLI and Codex CLI were considered). The user described a mode distinction: **subscription only with no API key → Claude CLI where supported; API key available → API-key mode.** This preference must be represented in the product configuration, not silently treated as proof that a specific subscription authorizes remote automated execution. Actual provider entitlement, applicable terms, supported models and unattended-server availability must be checked before adapter implementation.

## Task assignment

Main planner assigns a model, effort level and access mode for each fixed task. Workers receive instructions and run the assigned task, do not create or delegate subtask trees. Retry may change model/effort/machine under policy **without introducing a new task node**, and must account for new cost exposure before starting a billable attempt.

## Proposed adapter requirements

Each provider adapter declares available models and effort levels, execution mode, entitlement/credential requirements, memory/CPU resource profile, timeout/cancellation behavior, usage metering and error categories. Provider-specific UI labels and legacy illustrative model names are not a stable universal taxonomy.

- **CLI mode:** account/subscription authentication only if the applicable provider permits the targeted remote/unattended workflow; isolate credentials and process sessions on remote workers.
- **API mode:** use approved API credentials by secret reference; meter actual API charges separately from cloud compute.
- **Unavailable mode:** reject scheduling with an actionable configuration error, rather than quietly switching to a paid API path or assuming a subscription is valid.
- **Fallback:** retain the original task ID, append a new attempt, and respect credential authorization and project budget gating.

## Token-efficiency tactics

Use compact task scopes and only relevant dependency artifacts; reference versioned project documentation rather than copying large transcripts; cache/reuse validated results when safe; bound retry histories; record token usage per provider when measurable. Minimize planner repetition and context fragmentation. Do not sacrifice required security or acceptance instructions to save tokens.

## Open decisions

Exact provider licensing/terms, supported CLI versions and model nomenclature, user identity-to-worker credential delegation, cost attribution for subscriptions, effort translation, prompt/context size policy, live provider availability and failover authorization.

Related: [workers](workers.md), [context memory](context-memory.md), [budget](budget-costs.md), [data model](data-model.md).
