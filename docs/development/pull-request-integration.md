---
title: Protected Routine Pull Request Integration
status: Implemented tooling; activation unverified
category: development
last-reviewed: 2026-10-25
tags: [github, ci, integration, security]
---

# Protected routine pull request integration

## Single-account decision and activation state

The Owner explicitly chose to keep **`ygtdmr` (GitHub user ID `72071644`)** for both Codex Cloud development and PR authorship. No separate account, reviewer, external identity proof or new authentication service is required. This repository tooling decision does not change [ADR 0008](../decisions/0008-github-authentication.md), which governs the future Rolvya product's GitHub App integration and trusted Rust publisher.

**Routine merging remains disabled until live repository settings and an actual protected automatic squash merge pass verification.** [PR #6](https://github.com/ygtdmr/rolvya/pull/6) contains this bootstrap. [PR #7](https://github.com/ygtdmr/rolvya/pull/7) remains available for the positive smoke test. Passing unit tests, earlier PR CI or a dry-run does not make integration operational. The [ruleset template](../../.github/integration-ruleset.json) ships disabled; no live protection, variable or merge is activated by committing it.

## Shared identity limitations and manual sensitive changes

GitHub cannot distinguish the human Owner from Codex acting through the same `ygtdmr` connection. An account ID, PR author, workflow dispatch, label, comment or review under that identity **cannot prove independent human authorization**. An agent must never create an approval or claim its own action is a human review. GitHub does not permit authors to approve their own PRs.

Accordingly, main requires **zero approving reviews, no mandatory CODEOWNER review and no last-push approval**, while requiring PRs, resolved review threads, strict CI, linear history, deletion/force-push blocks and zero bypass actors. [CODEOWNERS](../../.github/CODEOWNERS) is advisory ownership for all files; it does not grant merge eligibility or claim an independent approval. Requiring an unavailable self-approval would prevent both routine and sensitive integration.

The controller automatically integrates only explicitly classified low-risk changes. Sensitive or unfamiliar PRs remain open for the **Owner's manual review and squash merge** in GitHub after CI succeeds and conflicts/threads are resolved. The Owner inspects the diff, tests and consequences, requests changes if needed, then clicks **Squash and merge**; no formal approving review is required. This manual process is a procedural responsibility, **not a GitHub-enforced human/agent identity boundary**. Agents must not manually merge sensitive PRs without explicit Owner instructions. The shared account's credentials can exercise the same GitHub permissions as the Owner, including changing settings or manually merging sensitive changes. The controller's path policy cannot prevent that account from using other authorized routes. Tests and paths also cannot prove arbitrary code harmless. These are accepted limitations of the chosen model, not independent human authorization.

## Explicit low-risk classification and future source boundaries

[The committed policy](../../.github/integration-policy.json) pins the account's numeric ID, exact login and User type, repository ID, target main and `codex/` branch prefix. Runtime verifies the live account/repository and write permission, without a separate identity or proof PR. Environment variables cannot replace the reviewed account. Labels, branch names and self-reviews cannot expand path permissions.

Current routine paths are exactly `README.md`, `docs/README.md`, `docs/project/status.md` and `docs/development/guides/routine-integration-smoke.md`. All other files fail closed. The source registry is **empty because application source does not exist**; no blanket `crates/**` or `apps/web/**` exemption is installed.

After actual source boundaries exist, the Owner may manually review a policy PR registering one narrowly scoped, low-risk component with its manifest, existing tracked tests and rationale:

- Rust: a component below `crates/<crate>/src/<component>`, `.rs` only; its crate manifest and integration test files must belong to the checked Cargo workspace and actual test targets.
- Frontend: a component below `apps/web/src/ui/<component>`, `styles/<component>` or `i18n/<component>`, `.ts`/`.tsx`/`.css` only; an actual frontend manifest and explicit `.test.ts(x)`/`.spec.ts(x)` tests are required.

Validation rejects missing/speculative/broad/overlapping roots, symlinks, hidden files, configuration inside source boundaries and critical names, including camel-case variants. Source PRs require successful **Rust checks** or **Frontend checks**, as applicable, in addition to the aggregate; a skipped language job cannot authorize source changes. Policy/manifest/test-runner changes are themselves manual work. Registered ordinary tests can change automatically alongside their component, so passing tests are evidence rather than proof of safety.

Security and authentication, identity and secrets, crypto and credentials, budget and financial authorization, infrastructure and deployment, CI workflows and configuration, Git publication, migrations and persistence, worker and orchestration logic, admission and state authority and other sensitive changes stay excluded, even inside a registered component. Architecture/ADRs, operations, agent instructions and tooling are excluded. Both sides of a rename must be routine. No source boundary can silently authorize new critical behavior under an innocent filename; source inspection and meaningful tests remain necessary. Preserve Graphify-first investigation, actual source verification and `graphify update .` after code edits.

## CI and protected merge verification

[CI](../../.github/workflows/ci.yml) executes PR code with Contents read only, no repository secrets, write token, persistent checkout credentials or shared cache. Pinned Actions and checksum-verified Actionlint/Gitleaks perform workflow validation, complete changed-commit secret scanning, documentation/MIT checks and deterministic policy tests. Actual manifests activate Rust fmt/clippy/locked tests or frontend lint/typecheck/tests/build/artifact scripts; missing lockfiles/check contracts fail. SQL migrations require a Rust workspace and reviewed replay gate. These checks do not claim product or browser acceptance before real implementation exists.

[Routine integration](../../.github/workflows/integrate.yml) loads trusted main-branch code only and never executes PR code, artifacts or caches. Its short-lived token has Contents/PR write for squash and Actions/Checks/Statuses read, without Administration, workflow-write or approval grants. The controller requires same-repository pinned-account PRs, complete file lists, clean current-base mergeability, exact-head current trusted CI and expected check Apps. All required checks must succeed, with no failing or unfinished optional checks/statuses. Both original CI actor and rerun actor must match the pinned account.

The controller independently checks squash-only repository settings, native auto-merge disabled, strict rules, resolved threads, zero review requirements, linear history, deletion/force-push protection and **no bypass actors in any effective ruleset**. Overlapping rules requiring self-approval fail closed. Restricted bypass-list visibility is handled only through the exact Owner-verified revision procedure below; missing verification evidence blocks merging. It sends the expected head SHA and never enrolls persistent native auto-merge, approves reviews, force-pushes, deletes branches, rebases or bypasses protection. GitHub enforces its own checks when accepting the merge. GITHUB_TOKEN merges do not trigger ordinary downstream push workflows; future deployments/releases require separate work.

## Browser-only activation runbook

No local development environment, terminal, clone or personal-computer repository synchronization is required. The Owner performs the following setup through GitHub's web interface. Existing live settings have not been changed or verified by this revision.

### 1. Review and land the bootstrap with routine merging disabled

1. Keep the existing Codex GitHub connection to `ygtdmr`. No identity setup or external proof PR is needed. Review #6's diff and successful current-head CI in GitHub.
2. In **Settings → Secrets and variables → Actions → Variables**, set `ROLVYA_MERGE_MODE=disabled` and remove old `ROLVYA_CODEX_AUTHOR_IDS`, `ROLVYA_VERIFIED_SMOKE_RUN_ID` and `ROLVYA_VERIFIED_SMOKE_PR` values. Missing mode also disables routine merging. Do not reuse a receipt from the earlier policy.
3. If existing main protections already require self-approval, edit those review settings to this explicitly authorized single-account policy: **0 approvals**, **Require review from Code Owners off**, **Require approval of the most recent reviewable push off**. Inspect both **Settings → Rules → Rulesets** and **Settings → Branches** for overlapping rules. Retain all strict CI, PR, thread-resolution, force-push/deletion and no-bypass controls; do not disable a ruleset, add an exception or bypass a failing check to land #6. No second reviewer is required.
4. The Owner manually reviews and squash-merges #6 through the permitted GitHub PR interface after applicable CI passes. Do not record a fictitious approving review. The proposed integration policy does not authorize an agent to perform this sensitive bootstrap merge.

### 2. Configure repository and Actions settings

In **Settings → General → Pull Requests**, enable **Allow squash merging** only. Disable **Allow merge commits**, **Allow rebase merging**, **Allow auto-merge**, and **Automatically delete head branches**. The controller will perform guarded squash directly and leave branches intact.

In **Settings → Actions → General → Workflow permissions**, select read-only defaults and leave Actions-generated PR approvals disabled. Allow the pinned Actions and checksum-verified binary downloads used by CI. Use GitHub-hosted runners and no secrets/privileged environments for PR code. Job-scoped merge permissions are declared explicitly; if policy prevents them or the API cannot observe rules, stop and correct the actual permission rather than adding a broad token or bypass.

Open [the ruleset JSON](../../.github/integration-ruleset.json) in GitHub, select **Raw/Download raw file**, then **Settings → Rules → Rulesets → New ruleset → Import a ruleset**. A browser file download/upload is sufficient; no repository sync is involved. Inspect/update a matching existing ruleset instead of creating duplicates. [GitHub documents ruleset import](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/managing-rulesets-for-a-repository#importing-a-ruleset).

The template starts **Disabled**. Before setting its enforcement to **Active**, confirm:

- Target only `refs/heads/main`, with no exclusions and **empty bypass list**, including no Owner/admin/Actions exceptions.
- Block branch deletion and force pushes; require linear history and a pull request with resolved conversations.
- Require **Required checks** from **GitHub Actions (App ID 15368)** and **Require branches to be up to date before merging**. Check the source App, not just a matching name. The CI aggregate must have appeared in repository CI before it can be selected.
- **0 required approvals**, **Code Owner review off**, **last-push approval off**; dismiss stale approvals if any exist. CODEOWNERS remains advisory.

Review all overlapping rulesets/classic protections for conflicts and zero bypass actors. Do not weaken CI or add a bypass to satisfy this runbook. If the UI/plan cannot represent or expose the policy, keep merging disabled. Classic protection alone does not satisfy the controller's observable ruleset gate.

### 2a. Verify the ruleset revision with a restricted Actions token

[GitHub intentionally omits `bypass_actors` without ruleset write access](https://docs.github.com/en/rest/repos/rules#get-a-repository-ruleset). The merge workflow must not receive Administration permissions or an extra credential to reveal it. An omitted list is **unknown**, never proof of zero bypass actors. Ruleset history/version endpoints also require privileged access and are not used by the controller.

The Owner supplied and explicitly verified ruleset **25836182**, its complete configuration and empty bypass list, at revision **`2026-10-25T23:31:28.338+03:00`** (the same instant as **`2026-10-25T20:31:28.338Z`**). [The committed policy](../../.github/integration-policy.json) binds that ruleset ID, revision, configuration SHA-256, verifying Owner ID and explicit empty bypass list. [The regression fixture](../../.github/scripts/fixtures/ruleset-25836182.json) preserves the exact supplied configuration. The broader `allowed_merge_methods` field in that ruleset does not relax repository-level squash-only settings or the controller's squash-only request.

Every audit/merge reads effective main branch rules and each ruleset with the **same current workflow token**. Each must be active, belong to this repository, target branches, report `current_user_can_bypass=never`, match the attested ID/revision/configuration and contribute the same complete effective rules. Every effective ruleset must be attested, and no attested ruleset may disappear. A visible bypass list must still be an actual empty array. Full visibility alone does not override a changed or missing attestation. Protection is fetched again after CI verification before permitting a merge.

The digest includes all configuration fields and parameters, including rules, conditions and normalized `updated_at`. JSON object-key order and equivalent RFC3339 timezone representations do not affect it; timestamp precision is retained. Caller-specific bypass capability/list and read-only navigation/node/creation metadata are checked separately or excluded from the digest. Unknown configuration fields cause a mismatch. The hidden-list guarantee relies on the Owner's truthful verification and GitHub's revision timestamp changing when configuration (including bypass actors) changes; the workflow cannot independently inspect a hidden list. The attestation is an Owner policy assertion under the already documented shared-identity limitations, not a cryptographic proof of independent human approval.

If an audit reports a missing/changed attestation, **leave routine mode disabled**. In its job log, find **Ruleset observation** for the ID, exact `updatedAt`, `configurationSha256`, enforcement, caller bypass capability and list visibility. No token or hidden list is logged. Observations are diagnostic data, not authorization. Through **Settings → Rules → Rulesets → the ruleset**, the Owner checks the actual current rules/conditions, Active enforcement and **empty bypass list** using their existing authorized GitHub session. Verify the API configuration/revision against the observation; do not copy a new digest blindly or change the pin merely to clear a failure. The observation must still match after that review. The Owner can then edit `rulesetAttestations` in GitHub's file editor through a sensitive, manually reviewed policy PR; update the regression fixture to preserve the verified export as well. No local clone, new account, token or bypass permission is needed. API denials, absent revision/capability or changed configuration remain blockers.

For this fix, the Owner's supplied evidence is already pinned; after manually reviewing/merging the fix with required CI, refresh #7 against current main, wait for exact-head CI and repeat **audit** on main with mode disabled. Do not merge #7 to test the visibility fix. Only after the audit succeeds may the separately authorized protected smoke procedure be considered. This fix neither runs that smoke nor enables routine merging. Any later attestation/security change invalidates the old smoke receipt and requires new protected verification before activation.

### 3. Reuse #7 for one actual protected automatic squash merge

1. Keep `ROLVYA_MERGE_MODE=disabled`. After #6 is merged and main protection is active, change #7's base to **main** using its **Edit** control. Its existing `ygtdmr` authorship is eligible; no reattribution or fresh account is needed.
2. Bring its branch up to date using GitHub's **Update branch** if available. If the old stacked bootstrap history causes conflicts or extra differences after #6's squash, use a Codex Cloud task on the existing `codex/integration-smoke` branch to merge current main without force pushes, resolve bootstrap files to match main exactly and keep only the harmless smoke document change. This uses the cloud repository, never a local clone. Verify **Files changed** contains exactly `docs/development/guides/routine-integration-smoke.md`, added or modified, with no rename. If #7 is already closed/merged or cannot be restored to this exact scope safely, one fresh harmless PR on a current `codex/` branch is necessary; it is not required merely because identities are shared.
3. Wait for successful CI on the **updated exact head**. Confirm it includes current main. Historical #7 pass/fail/repaired-pass runs tested CI, not this protected merge.
4. In **Actions → Routine integration → Run workflow**, choose **main**, operation **audit**, and inspect the summary. Sensitive paths must be ineligible, while #7 should be eligible. Fix settings, API denials, stale branches or CI failures without bypassing gates.
5. Copy #7's full latest 40-character commit SHA from its **Commits** page. Run the workflow on **main** again, operation **smoke-test**, `smoke_pr=7`, `smoke_head=<exact SHA>`. Keep routine mode disabled. This intentionally permits **one isolated automatic merge** while general automation is off; it applies all normal CI/protection/classification gates and accepts only that exact one-file harmless revision. A changed head, sensitive file, failed check or rejected merge fails the job and provides no activation evidence.
6. Verify the **Protected smoke test** job succeeded and #7 is actually squash-merged to protected main by `github-actions[bot]` (ID `41898282`). Record the run ID from its URL, PR number and single-parent squash commit. A manual Owner merge, dry-run, skipped job, label or agent review cannot supply this evidence. A dispatch from the shared account identifies the account only; it does not prove a human initiated it.

### 4. Enable routine mode only after the verified merge

In Actions repository variables, set `ROLVYA_VERIFIED_SMOKE_RUN_ID` and `ROLVYA_VERIFIED_SMOKE_PR` to the actual successful smoke run and PR, then `ROLVYA_MERGE_MODE=enabled`.

The controller verifies the actual GitHub run/workflow/job/PR/commit receipt, exact pinned-account smoke head and harmless file, Actions merge actor, merge timing and squash ancestry on main. It compares the complete `.github/` plus AGENTS security tree against the verified revision. Missing/forged/stale receipts or changed security configuration block routine integration. Ordinary source/docs changes outside that tree do not invalidate the receipt. Repeat the protected smoke after policy, classification, tests or workflow changes; a later one-file modification of the smoke document can supply the necessary new test. Receipt variables are selectors, not authorization assertions.

CI completion and a 15-minute poll then evaluate routine PRs without a manual merge click. Failures, conflicts, stale branches and sensitive changes require author/Owner attention; the controller does not repair source or update branches. Set mode to `disabled` or remove it to stop. No native auto-merge enrollments need cancellation.

## Verification and outstanding Owner steps

Regression tests cover shared-account provenance, absent/incorrect accounts, advisory CODEOWNERS, overlapping self-approval deadlocks, strict checks/conflicts/races, both future source kinds with real temporary boundaries/tests, critical paths/configs/symlinks, omitted language jobs, isolated smoke scope and forged/stale receipts. Temporary source fixtures are not Rolvya implementation.

The Owner still needs to review/land #6, apply and inspect the live GitHub settings above, refresh/retarget #7 and run the actual protected smoke. Only then may receipt variables and routine mode be enabled. **No independent human authorization, live protected merge or operational activation is claimed by this change.** No separate account, reviewer, authentication service or product development is introduced.

Related: [review governance](review-process.md), [testing](testing.md), [engineering standards](standards.md), [project status](../project/status.md).
