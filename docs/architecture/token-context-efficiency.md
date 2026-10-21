---
title: Planner and Worker Token Context Efficiency Contract
status: Proposed
category: architecture
last-reviewed: 2026-10-22
tags: [tokens, context, prompt, model, optimization]
---

# Planner and worker token/context efficiency

## Historical owner intent

The user requested savings **both during the single up-front planning step and throughout worker execution**. Prompts should have a practical **medium** length, and typical worker units were expected to be roughly **10 minutes or less** by design partitioning; this is **not** a guaranteed runtime or enforced timeout. Workers may run independently at scale, but the main agent **does not add tasks at runtime**. Earlier optimization guidelines for worker prompt/token use were accepted generally; the exact original enumerated list could not be recovered in full. Therefore the mechanisms below are **Proposed implementations**, not a retroactively invented quote from the user.

## Non-negotiable correctness priority

A token-saving optimization **cannot** omit approved architecture decisions, security boundaries, assigned goal/scope/exclusions, current attempt fence, allowed remote Git base/ref, necessary validated upstream artifacts or acceptance criteria. Never save tokens by skipping independent tests, weakening budget checks, passing raw secrets into prompts or repeating model calls until cost approval is exhausted.

## Planning phase — one full DAG, bounded reasoning artifacts

- Supply the main planner with one curated source-of-truth bundle: project scope, approved ADR hashes/links, remote repo base, constraints, target acceptance and cost envelope.
- Parse the planner output as a versioned structured full hierarchy and explicit DAG. Validate every node/edge **once before activation**, not through uncontrolled planner-worker replanning cycles.
- Allow hierarchical decomposition at arbitrarily nested conceptual levels during **planning**, with resource limits and deterministic graph validation.
- Preassign model, effort, intended provider mode, scope, dependency artifacts and acceptance for **every executable leaf**.
- Include group nodes only as non-executable aggregators; use a dedicated planned leaf for required integration code/tests.
- Avoid resending entire historic chats and duplicated architecture descriptions to the main planner. Replace them with canonical compact policy summaries and immutable document references.
- Re-evaluate cost scenarios on plan draft revisions **before activation**; activating a plan is not permission to launch unreserved paid workers.

## Worker phase — minimum sufficient context

Proposed compact prompt structure, in order:

1. Task objective and explicit success criteria.
2. Authorized source revision + allowed/forbidden code paths and commands.
3. Concise technical constraints/approved ADR references that materially affect this leaf.
4. *Only* the validated outputs of explicit prerequisite tasks.
5. Current attempt identity, resource/mode/effort restriction, failure handoff summary if retry.
6. Concise expected result manifest + test/reporting requirements.

Do not include: unrelated sibling chat histories, full project-wide logs, raw credentials, unbounded copy of repository code, model-generated earlier reasoning traces, stale attempt instructions, or all other tasks' irrelevant artifacts.

## Runtime techniques — proposed and individually testable

| Technique | Token benefit | Safety/quality guard |
| --- | --- | --- |
| Reference canonical Markdown/ADR paths + digests | Avoid repeatedly copying global policy | Fetch and verify referenced version; never omit required invariant |
| Dependency-aware artifact retrieval | Limit upstream context | Use only independently verified predecessor artifacts tied to plan hash |
| Structured machine-readable handoffs | Shorten retry prompts | Preserve actual commands, changed refs, failures, acceptance/test proof |
| Concise error classification | Avoid dumping massive logs into model | Keep raw authorized logs in secured artifacts, include stable reference/hash |
| Stable prompt templates | Reduce variable repeated instructions | Do not flatten different model/effort/access limitations |
| Deduplicate identical source snippets | Avoid repeated repository text | Include ref/version and prevent cross-project leakage |
| Bounded context summarization | Prevent context overflow | Warn on truncation, preserve approval and security requirements |
| Conditional model escalation | Avoid expensive default model everywhere | Only policy-approved retry on same task with budget admission |
| Review/cache verified work | Avoid recomputing identical accepted artifacts | Respect provenance and scope; never reuse unsafe/stale output blindly |
| Progressive diagnostic detail | Start with compact relevant failure facts | Escalate to deeper logs only when necessary for quality |
| No speculative runtime replanning | Avoid main-agent/worker planning loops | New scope triggers separate human-authorized plan |
| Limit provider status polling | Reduce infrastructure/token waste | HTTPS adaptive backoff; never delay essential cancellation safety |
| Bounded attempts and budgets | Avoid infinite paid retries | Approved hard budget gate; stop-on-risk and explicit owner increase |

Proposed techniques are **not blanket permission** to cache private user code indefinitely, change model provider silently, or skip independent result validation.

## Attempt-to-attempt context contract

```yaml
# Synthetic example, not a deployed wire format.
plan_hash: sha256:example
task_id: existing-executable-leaf
attempt_id: attempt-2
assigned_model: originally-selected-model
effective_model: authorized-fallback-if-any
effort: allowed-effort
source_remote_commit: verified-base
upstream_artifact_hashes: []
previous_attempt:
  typed_failure: context-limit
  last_verified_remote_ref: none
  commands_run_summary: sanitized-reference
  tests_failed: [synthetic-test-name]
  uncommitted_outputs: no-verified-artifacts
  billing_reconciliation: required
security_and_acceptance_constraints: immutable-reference
```

A new attempt cannot silently reinterpret the task goal or expand its scope. Store exact provenance and redacted logs outside the model prompt when verbose.

## Model routing and economic efficiency

Cheapest tokens alone are not equal to cheapest **validated outcome**. Compare total:
`AI API or authorized subscription allocation + remote CPU/RAM + startup/idle + artifacts/log storage + network transfer + bounded retry costs`.

The planner may choose different model/effort per task; stronger/higher-cost models for complex work were considered but no universal mapping or model ladder was approved. Provider CLI/API entitlement must be checked before remote unattended use. No unofficial model token pricing or “free API because subscription” assumptions.

## Measurement plan

For synthetic representative tasks, measure per accepted leaf:
- input/output tokens when provider reports them, context truncation count and repeated prompt bytes;
- CLI vs API eligibility and provider quota denial;
- attempts to verified success, retries and accepted artifact integrity;
- elapsed time, CPU/RAM, cold starts and remote transport overhead;
- four actual/synthetic cost categories vs three forecasts;
- correctness regressions when compression/dedup enabled.

Measure planning token use separately from worker token use, and compare the same fixed DAG/acceptance tests under control and optimized variants. No efficiency percentage target or numeric KPI is approved until observed evidence exists.

## Testable failure modes

- Compressed task prompt loses an approved budget/remote-Git restriction → block as invalid context.
- Parent group accidentally receives a model-execution prompt → reject scheduling.
- Cached artifact comes from unrelated project or old base commit → reject.
- Context compaction drops acceptance evidence → validation fails, not false success.
- Retry context contains secrets or raw credentials → security test fails.
- Optimization changes task/model/effort without approved new attempt → deny.
- A repeated planner call attempts to add runtime DAG nodes → reject.
- Token usage feed absent → mark unmeasured/unknown, never fabricate zero.

## Open decisions

Prompt/context byte and token budgets, summarization methods/thresholds, provider-specific token measurement, caching lifetimes, artifact retention/security, model-effort routing heuristics and owner-approved quantitative efficiency targets.

Related: [context memory](context-memory.md), [model access](model-access-entitlement-contract.md), [task model](task-model.md), [agent handoffs](../development/agent-handoffs.md), [budget accounting](budget-costs.md), [historical audit](../project/historical-decision-audit.md).
