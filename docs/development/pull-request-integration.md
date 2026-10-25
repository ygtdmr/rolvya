---
title: Verified Routine Pull Request Integration
status: Implemented configuration; activation and live merge verification pending
category: development
last-reviewed: 2026-10-25
tags: [github, ci, integration, security, agents]
---

# Verified routine pull request integration

The Owner requested hands-free squash integration of verified routine Codex changes on 2026-10-25. This repository automation is separate from Rolvya's future Rust Git publisher. It preserves [ADR 0008](../decisions/0008-github-authentication.md), [hard budget authority](../decisions/0002-hard-budget-approval.md), the Rust backend and approved frontend architecture. No application or cloud infrastructure is created by this change.

## Current scope and trust boundary

[The reviewed policy](../../.github/integration-policy.json) admits same-repository, non-draft `codex/` PRs to `main` only from explicitly configured numeric GitHub author IDs. The author and CI actor/rerun actor must be trusted. Branch prefixes, labels, display names, review comments and agent self-approval never grant permission. Forks are excluded.

The initial routine list is root README, documentation index, project status and the exact harmless smoke-test document. Every other file requires Owner review: application source, security/authentication/secrets/budget authority, architecture/ADRs, infrastructure/deployments, dependencies, CI/workflows, CODEOWNERS, agent instructions and unfamiliar paths. Both sides of renames are checked; deletions are changes too. When real Rust/frontend components exist, classify their stable low-risk paths once in an Owner-reviewed policy change; routine changes inside those boundaries can then be hands-free. CI language support does not authorize every source path.

[CODEOWNERS](../../.github/CODEOWNERS) defaults all files to `@ygtdmr`, with ownerless exceptions exactly matching the routine list. The controller never merges a non-routine PR, even after human review. Such changes take a separate human review/merge route governed by GitHub protection. Labels and agent approvals cannot release that gate.

**Identity limitation:** the current Codex connector opens PRs as `ygtdmr` (ID `72071644`). GitHub cannot distinguish Codex from the human when they share credentials. Explicitly trusting that ID trusts that account's same-repository `codex/` PRs; it does not attest which application used the account. Prefer a dedicated repository-scoped GitHub App/bot author for stronger provenance. Do not guess a bot ID or select a trust list without Owner authorization. A PR author cannot approve their own CODEOWNERS change: sensitive PRs need a distinct author and human Owner reviewer, or another authorized independent human. Shared-account self-reviews are never human authorization.

## CI foundation

[CI](../../.github/workflows/ci.yml) runs on PRs and main pushes with only `contents:read`. Checkout uses the exact PR head with credential persistence disabled. PR execution receives no repository secrets, App keys, write token, privileged environment or shared cache. Actions are pinned to commit SHAs; Actionlint/Gitleaks binaries have pinned versions and SHA-256 checksums. [Dependabot](../../.github/dependabot.yml) groups monthly Actions updates, which remain sensitive reviewable changes. Review binary-version/checksum updates alongside the regular tooling review.

The stable required job is **`Required checks`**, sourced from the GitHub Actions App (integration ID `15368`). It always runs, including when dependencies fail, and requires all applicable jobs to succeed:

| Job | Current repository | Activation contract |
| --- | --- | --- |
| Repository checks | Relative documentation links/fences, MIT notice, policy/CODEOWNERS consistency, ignored graph artifacts, integration negative tests | Source/tooling deltas require a project status update |
| Secret scan | Gitleaks over all changed commits, no PR API pagination cap; redacted output and no findings artifacts | Findings fail CI; manual CI dispatch scans the latest commit; never use a real secret to test |
| Rust | Explicitly skipped: no Rust manifests | Workspace discovery; committed Cargo.lock and exact stable rust-toolchain.toml; fmt, clippy with warnings denied, locked tests; SQL migrations require a reviewed replay script |
| Frontend | Explicitly skipped: no apps/web/package.json | npm lockfile and lint, typecheck, test:ci, build, verify:dist and check:browser-assets scripts; npm ci uses --ignore-scripts |

Unsupported Node project locations or manifests missing lockfiles/check contracts fail inventory, rather than silently skipping tests. SQLx migration replay, immutable-DAG/budget/fence regressions, browser/security tests and product acceptance still need actual implementations; green tooling CI does not close those product gates. Frontend artifact scripts must implement [ADR 0043](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md) and the [source/build plan](web-frontend-source-and-build-plan.md) before a real frontend can pass.

## Merge controller

[Routine integration](../../.github/workflows/integrate.yml) reacts to CI completion and polls every 15 minutes for other checks/base updates. Manual dispatch is allowed only on `main`. It loads only default-branch code, never PR source, artifacts or caches. Its short-lived token has Contents/PR write solely for squash integration and Actions/Checks/Statuses read for evidence. It cannot approve PRs or administer settings. No PAT or additional App secret is needed.

Before merging, it independently reads GitHub metadata and requires:

1. Trusted identity/repository/branch, complete paginated file inventory, reviewed routine paths and a non-draft open PR.
2. Squash-only settings, native auto-merge enrollment disabled, strict required-status/CODEOWNERS rules, deletion/force-push protection and active rulesets with **no bypass actors**.
3. A clean, conflict-free PR including latest main; unknown mergeability waits.
4. Latest CI for this exact head, from the expected workflow/repository, with `Required checks` successful. Every configured required context must succeed with its expected App identity; all other present checks/statuses must also finish without failure.
5. Unchanged head/base/file count on a fresh read. The squash API receives the expected head SHA; GitHub's strict checks remain authoritative if main changes concurrently.

It does **not** enroll native auto-merge: enrollment could survive a later sensitive push. Every attempt is reclassified. Missing evidence, API denial, failing/cancelled checks, conflicts and rejected merges fail closed with an Actions summary. There is no bypass, automatic approval, force push, automatic rebase or blind retry; later scheduled runs may re-evaluate current evidence. GITHUB_TOKEN merges do not trigger ordinary downstream push workflows; future releases/deployments need a separately reviewed trigger design.

## One-time Owner setup

This bootstrap changes workflow/authorization policy and is itself sensitive. Review and integrate its PR through the existing human-authorized route. If protection already applies, satisfy it using distinct author/reviewer identities; never disable or bypass it. Keep the controller disabled until the code is on main and these settings are verified.

Use an Owner-admin GitHub session against the remote repository. No credentials belong in Git, PR comments or chat. The current cloud shell cannot reach GitHub's API, and the connected tools do not expose rule/Actions/variable administration; these settings have **not** been applied or verified by this change.

```sh
# Squash only; this controller replaces native auto-merge enrollment.
gh api --method PATCH repos/ygtdmr/rolvya \
  -F allow_squash_merge=true -F allow_merge_commit=false \
  -F allow_rebase_merge=false -F allow_auto_merge=false

# Read-only defaults; no Actions-generated approval reviews.
gh api --method PUT repos/ygtdmr/rolvya/actions/permissions/workflow \
  -f default_workflow_permissions=read -F can_approve_pull_request_reviews=false

# Start in dry-run, never enabled by default.
gh variable set ROLVYA_MERGE_MODE --repo ygtdmr/rolvya --body dry-run

# Inspect the author identity before selecting a trust list.
gh api users/ygtdmr --jq '{login,id,type}'
# ONLY if the Owner chooses the shared-account trust model above:
gh variable set ROLVYA_CODEX_AUTHOR_IDS --repo ygtdmr/rolvya --body '[72071644]'

# Inspect existing rules; preserve stronger protections and avoid duplicates.
gh api repos/ygtdmr/rolvya/rulesets
```

Create the reviewed repository ruleset **only if it does not already exist**:

```sh
gh api --method POST repos/ygtdmr/rolvya/rulesets \
  --input .github/integration-ruleset.json
```

For an existing matching ruleset, use its inspected numeric ID with PUT after reviewing differences; retain stronger rules. The [template](../../.github/integration-ruleset.json) requires PRs, GitHub Actions-sourced `Required checks` with up-to-date branches, CODEOWNERS with stale approvals dismissed, resolved review threads, linear history, no force pushes/deletions and **no bypass actors**. General approval count is **zero** so ownerless routine files require no human review; protected changes still require CODEOWNERS approval. Do not require last-push/human approval globally or grant the Actions App a bypass role. Classic branch protection alone does not satisfy the controller's observable ruleset gate.

Allow the pinned checkout/setup-python/setup-node/github-script actions and downloads of the checksum-verified Actionlint/Gitleaks release binaries. GitHub-hosted public runners are the intended environment. No deployment secrets, privileged/self-hosted runners or paid cloud tests belong in PR CI.

## Live acceptance and activation

Local tests do not prove a GitHub merge works. A harmless PR must pass a live dry-run and then be automatically squash-merged before the workflow is called operational.

1. From current main, create `codex/integration-smoke`, change **only** `docs/development/guides/routine-integration-smoke.md`, and open a PR using the selected trusted author. No source, workflow, security or authority change belongs in it.
2. Wait for `Required checks` to succeed. Run `gh workflow run integrate.yml --repo ygtdmr/rolvya --ref main` and inspect its summary. Dry-run must report the exact head eligible and leave the PR open. Fix missing rules/permissions/provenance; never bypass a gate.
3. Enable and dispatch:

   ```sh
   gh variable set ROLVYA_MERGE_MODE --repo ygtdmr/rolvya --body enabled
   gh workflow run integrate.yml --repo ygtdmr/rolvya --ref main
   ```

4. Verify squash merge, the merge SHA on main, successful required checks for the exact tested head and the controller's summary. Record PR/run/commit links in project status before claiming operational integration.
5. Negative cases stay blocked: untrusted/fork PR, failed/pending checks, conflict, changed head, protected rename, missing checks and sensitive/unknown paths. Deterministic tests exercise these without real credentials or hostile privileged execution.

Before bootstrap lands, a stacked harmless PR targeting the configuration branch can exercise live PR CI only. That does **not** verify the main-branch controller or protected auto-merge. Retarget/rebase it onto main after bootstrap, or create a fresh harmless PR for steps 1–4. Keep dry-run mode until live evidence exists.

To stop integration, set `ROLVYA_MERGE_MODE=disabled` or remove it. There are no enrolled native auto-merges to cancel. Resolve failures in scoped PRs, rerun CI and let the controller evaluate again. Sensitive decisions remain human work; verified routine merges need no manual merge click after activation.

Related: [review governance](review-process.md), [testing](testing.md), [engineering standards](standards.md), [project status](../project/status.md).
