---
name: rolvya-pr-risk-review
description: Review Rolvya pull requests for correctness, invariant regressions, security and budget risks, test gaps, and merge-policy classification. Use for code review or implementation handoff before merge; not to approve or merge a PR.
---

# Rolvya change-risk review

1. Read `AGENTS.md`, the applicable approved ADRs, and `docs/development/pull-request-integration.md`. Do not reopen settled decisions or claim a review constitutes independent human approval.
2. Query Graphify for the changed components and consumers, then inspect exact base-to-head diffs, new source and executed tests. Run `graphify update .` only if source has changed during this task.
3. Classify every changed path, including renamed/deleted paths, as reviewed routine or sensitive/unfamiliar according to current repository policy. Treat workflows, permissions, auth, secret broker, budgeting/admission, migrations, orchestration authority, deployment, and policy/ADR changes as sensitive.
4. Examine behavior, not just path names. Check incorrect authorization, cross-project state, mutable activated DAG, unauthorized budget exposure, stale leases, unsafe Git publication, absent idempotency, and insecure CLI/API entitlement assumptions where relevant.
5. Confirm current exact-head checks, required language jobs, runtime smoke evidence and protections; identify skipped/unrun tests explicitly. Graphify findings and passing CI cannot prove all security properties.
6. Report prioritized actionable findings with precise file/line references, reproduction, impact and suggested verification. Separate blocking findings from non-blocking improvements; say explicitly when no findings were discovered, not that code is guaranteed safe.
7. Never create approving reviews, add a bypass, modify rulesets to enable merging, or merge sensitive PRs. The shared `ygtdmr` account cannot prove independent Owner review. Return a concise handoff for the Owner and implementation agent.

Output: findings (severity, location, evidence), checks observed and missing, sensitive/routine classification, and recommended next step.
