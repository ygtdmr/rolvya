---
title: Agent Context and Memory Contracts
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [agents, memory, prompts, tokens]
---

# Agent context and memory

## Approved boundaries

The main planning agent creates the entire fixed plan up front; each worker executes only its assigned bounded task. Task-specific instructions, goals, constraints, model and effort must be available throughout execution. Workers must not generate subtasks. Token efficiency matters in planning and execution.

## Proposed layered context

1. **Project context:** identity, approved decisions, remote repository rules, architecture links and invariants.
2. **Plan context:** immutable graph version and global acceptance criteria.
3. **Task contract:** task ID, bounded goal, permitted scope, explicit exclusions, predecessor artifact references, model/effort/access mode, commands/permissions and measurable exit criteria.
4. **Attempt context:** one worker lease, resource/time budget, relevant failure history, resume checkpoint and secret references; attempts never mutate the active DAG.
5. **Result manifest:** file/ref hashes, tests, validation evidence, usage and structured errors suitable for downstream consumption.

## Context packaging

Use small task prompts, short decision summaries and versioned artifact links instead of duplicating entire transcripts or codebases. Retrieve only dependency outputs relevant to the current task. Hash/version context so retries are reproducible. Distinguish durable decision memory from ephemeral worker conversation state. Keep raw credentials out of model-visible context.

## Retry behavior

When an attempt fails, the orchestrator may diagnose and alter worker/model/effort as allowed while preserving task identity. Record the original scope, attempted fixes, budget impact and handoff information. A failure must not cause an unapproved new task. Changes to approved architectural requirements require human review, not silent summarization.

## Security and quality

Treat retrieved repository text and model outputs as untrusted input, not higher-priority instructions. Redact sensitive fields from logs and persisted prompts. Test for stale references, wrong predecessor revisions, cross-project contamination, oversized prompts and loss of acceptance criteria during compression.

## Open decisions

Context format/token budgets, summarization trigger, storage backend, retrieval model, model-specific adapters, content provenance and retention, prompt injection defenses, and visibility of attempt transcripts.

Related: [task model](dag-planning.md), [workers](workers.md), [persistence](data-model.md), [security](security.md).
