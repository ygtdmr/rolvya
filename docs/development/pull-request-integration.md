---
title: Verified Routine Pull Request Integration
status: Pre-activation configuration; independent identity and protected smoke verification pending
category: development
last-reviewed: 2026-10-25
tags: [github, ci, integration, security, agents]
---

# Verified routine pull request integration

This is repository development automation, separate from Rolvya's future trusted Rust Git publisher. It preserves [ADR 0008](../decisions/0008-github-authentication.md), [hard budget authority](../decisions/0002-hard-budget-approval.md), [Rust-only backend](../decisions/0004-control-plane-language.md) and [frontend packaging](../decisions/0043-single-bundle-browser-platform-and-ux-boundary.md). No application source or architecture decision is implemented or changed here.

## Activation state

**Routine merging is disabled and is not operationally verified.** The committed automation-author list is empty. The [ruleset template](../../.github/integration-ruleset.json) starts **disabled**, with no bypass actors. Do not activate CODEOWNERS protection until independent authorship is demonstrated, and do not enable routine integration until a real protected smoke merge passes the receipt gate below.

[PR #6](https://github.com/ygtdmr/rolvya/pull/6) remains the bootstrap review. [PR #7](https://github.com/ygtdmr/rolvya/pull/7) is preserved, open, as a harmless CI and shared-identity negative control. Its existing author is `ygtdmr`, so it must fail the independent-author gate. A label, actor variable, App installation or new connection cannot change an existing PR's author; GitHub's [PR update API](https://docs.github.com/en/rest/pulls/pulls#update-a-pull-request) has no author-change operation. A positive independent-author test consequently needs one genuine PR from the new principal. Do not close/recreate #7, impersonate its author, add an Owner exception, or manually merge it to manufacture activation evidence. No additional PR is created merely to replace #6.

## Verified identity support and the approval deadlock

Observed in this repository: Codex creates #6 and #7 as GitHub user `ygtdmr`, ID `72071644`. This is also the human CODEOWNER. GitHub does not allow a PR author to approve their own PR. Trusting that same ID as automation creates an approval deadlock for sensitive changes and cannot distinguish an agent from its human owner.

Checked against official documentation on 2026-10-25: OpenAI describes [per-user GitHub connections for Codex Cloud](https://help.openai.com/en/articles/20001545-using-codex-cloud) and [user authorization of the connector](https://help.openai.com/en/articles/11390924). These sources do **not** document selecting an arbitrary custom GitHub App as Codex Cloud's PR-author identity. No such selector or independent bot principal is exposed in this session. Native custom-App authorship is therefore **unverified**, not assumed available. Existing tasks may retain their old connection/state; verify a fresh task after any change.

GitHub separately supports [App-installation authentication attributed to the App](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/about-authentication-with-a-github-app). That is not proof that Codex Cloud can use a chosen App natively. An external App publisher would be a separate trusted integration, requiring its own implementation, repository-selective installation and verification. Do not place its private key in a model-visible Codex environment or describe an unimplemented publisher as available. PRs created with generic GITHUB_TOKEN can require manual workflow approval; GitHub documents [installation-token publication for unattended CI triggering](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow). The shared `github-actions[bot]` principal is not accepted as a dedicated PR author here.

The smaller supported-connection route to attempt is an independently controlled automation GitHub **user** account, with only repository write access, connected through the normal GitHub-user authorization flow. Whether the actual Codex/workspace connection accepts the selected account must be proved, not inferred from this guide. If that route is unavailable, stop and arrange a separately reviewed App publisher/provider-supported identity solution; there is no fallback to Owner-authored automatic PRs.

The [policy](../../.github/integration-policy.json) separates `humanOwners` from `automationAuthors`. IDs and logins cannot overlap; the actual repository Owner must remain human. Both User and dedicated Bot author types are supported only after verified profile ID/login/type and an actual independent-author `proofPr` match GitHub. Human CODEOWNERS must retain write access. Missing identities, permissions, reassigned logins, Owner-authored proofs and legacy trust overrides fail closed. No review is generated or interpreted as human authorization by the controller. Original CI actors must be automation identities; a human may rerun their CI without becoming the PR author or supplying an approval.

## Source classification and CODEOWNERS

[CODEOWNERS](../../.github/CODEOWNERS) defaults all paths to the human Owner. Initial exceptions are the four exact routine documentation paths in policy. **Future routine Rust and frontend coding PRs are supported through a reviewed source registry**, not through a blanket source exemption.

`sourceRules` is empty until actual implementation boundaries exist. A human-reviewed registration supplies:

- A narrow existing Rust component under `crates/<crate>/src/<component>`, or a presentation component under `apps/web/src/ui/`, `styles/` or `i18n/` with another component segment.
- Its tracked crate/frontend manifest, tracked existing test files, language kind and a concrete low-risk rationale. Rust registrations name the component crate manifest and its integration-test targets; cargo metadata must confirm both the checked workspace member and those executed test targets.
- Matching generated ownerless CODEOWNERS entries for that root and its explicit tests. `codeowners(configuration)` in [the policy module](../../.github/scripts/integration-policy.cjs) is the single renderer; CI checks equality.

The registry rejects speculative/empty/overlapping roots, symlinks, hidden/config files inside ownerless components and security-sensitive path names, including case variations. It admits only `.rs` or frontend `.ts`/`.tsx`/`.css`; manifests, lockfiles, workflows and deployment/configuration do not become routine by proximity. Test evidence cannot be an untracked generated file. Authentication, secrets, sessions, permissions, financial authorization/budgets, storage/migrations, infrastructure, worker/publisher control, audit/fencing and other critical paths remain outside automatic merging. Misplaced critical files inside an ownerless root fail repository CI rather than silently losing review protection. Both sides of renames and deletions are classified.

A source PR also needs its actual **Rust checks** or **Frontend checks** job to succeed. A successful aggregate cannot hide a missing/skipped/failed language job. Future registrations require real source/test inspection and Owner review once per boundary; subsequent verified low-risk code changes inside it need no manual merge. Path classification is a scope control, not semantic proof that arbitrary code is safe. Keep real source inspection and meaningful tests alongside Graphify results.

## CI and merge verification

[CI](../../.github/workflows/ci.yml) executes PR code with only Contents read, no repository secrets, write token, persistent checkout credentials or shared cache. Pinned Actions and checksum-verified Actionlint/Gitleaks perform workflow validation, complete changed-commit secret scanning, documentation/MIT checks and deterministic policy tests. Future manifests activate Rust fmt/clippy/locked tests or frontend lint/typecheck/tests/build/artifact scripts; missing lockfiles/check contracts fail. SQL migrations need a Rust workspace and reviewed replay gate. These jobs do not claim product regressions or browser acceptance are implemented before real code exists.

[Routine integration](../../.github/workflows/integrate.yml) loads trusted main-branch code only, never PR code, artifacts or caches. Its short-lived token has Contents/PR write for squash and Actions/Checks/Statuses read; it has no Administration, workflow-write or approval grant. It requires same-repository `codex/` PRs, independent verified authors, complete file lists, current-base clean mergeability, exact-head current trusted CI, every required check with its expected App, and no failing/pending checks. Native strict rules, CODEOWNERS, deletion/force-push protection, squash-only settings and zero bypass actors are independently checked. Missing API visibility is a blocker, not permission to assume protection exists.

The controller sends the expected head SHA and never enrolls persistent native auto-merge, approves reviews, force-pushes, rebases or bypasses protection. Sensitive PRs are never automatically merged, even after human approval; they follow the human review/merge route. GITHUB_TOKEN merges do not trigger ordinary downstream push workflows; future deployment/release triggers remain separate work.

## Browser-only one-time Owner setup

No local development environment, terminal, repository clone or personal-computer synchronization is needed. The Cloud shell cannot administer GitHub's API here; these external changes require the Owner's browser/admin session. Existing stronger protections must remain intact.

### 1. Establish and prove independent authorship before protection

1. In GitHub, create/manage a dedicated automation account distinct from `ygtdmr`. In **rolvya → Settings → Collaborators**, give that account only repository write access, not admin, bypass rights or CODEOWNER status. Keep the human Owner account separate.
2. In Codex/ChatGPT's GitHub connection management, disconnect the Owner-backed connection for automated work. Use a separate browser profile signed into the automation GitHub account, then complete the normal **Connect to GitHub** authorization and select only Rolvya where repository selection is supported. Do not copy the Owner's cookies, tokens or App keys into Codex. If the product/workspace does not permit this account, stop; do not claim a bot connection exists.
3. Start a **new** Codex Cloud task. Verify its connected GitHub principal and a real created PR both identify the new account. On GitHub, inspect the PR author and that account's public API profile in the browser (`https://api.github.com/users/<login>`). Record its numeric ID, exact login and User/Bot type. A Git commit author string or branch name is insufficient.
4. Through GitHub's file editor on the existing #6 branch, fill `automationAuthors` with that verified identity and the real independent-author `proofPr` number. Keep `humanOwners` including `ygtdmr`; never put ID `72071644` in automation. Do not invent example IDs. The role registry and CODEOWNERS are human-reviewed policy changes. Empty authors intentionally keep the controller unusable for merging.
5. Confirm the human Owner can review the independent author's PR through **Files changed → Review changes** and has repository write/admin access. Agents must not submit that review. If the only available author is still the Owner, do not activate CODEOWNERS protection.

For a dedicated GitHub App instead, an Owner must first provide a supported, separately reviewed trusted publisher: repository-selective App installation, minimum Contents-read/PR-write publication permissions and an actual App-authored proof PR. Remove Owner-backed credentials from coding agents as well: changing only the PR author does not isolate an Owner review credential. If Codex cannot run with an independent or read-only connection, this route is not activation-ready. App credentials stay in that trusted publisher, never the coding environment; no bypass or Owner-review permission is granted. Creating/installing an App alone does not change Codex's author. This PR does not ship or claim an external publisher.

### 2. Bootstrap #6 without creating a self-review trap

The human Owner reviews #6's security/workflow configuration before merging it through the **currently permitted** human bootstrap route. Do not activate the new CODEOWNERS rules first, and do not pretend an Owner-authored PR has an independent approval. If existing protection already requires an unavailable independent CODEOWNER approval, stop: immutable PR authorship cannot be fixed in place by a label or bypass. An already authorized independent human review, or an explicitly authorized independent-author replacement, is then a genuine external prerequisite. Never weaken existing controls to bootstrap.

### 3. Configure GitHub settings while routine merging stays disabled

In **Settings → General → Pull Requests**, enable squash merging only; disable merge commits, rebase merging and native Allow auto-merge. The controller performs guarded squash directly. In **Settings → Actions → General → Workflow permissions**, select read-only defaults and leave Actions-generated PR approvals disabled. Allow the pinned GitHub Actions and checksum-verified binary downloads in the workflow. Use GitHub-hosted runners, no privileged environments/secrets for PR code.

In **Settings → Secrets and variables → Actions → Variables**, set `ROLVYA_MERGE_MODE` to `disabled`. Remove any legacy `ROLVYA_CODEX_AUTHOR_IDS` variable; it is no longer consumed. Do not set activation receipt variables yet.

Open [the ruleset JSON](../../.github/integration-ruleset.json) in GitHub, choose Raw/Download raw file, then use **Settings → Rules → Rulesets → New ruleset → Import a ruleset** to import that JSON. A browser file download/upload is sufficient; there is no repository sync. Inspect/update an existing matching ruleset rather than creating duplicates. [GitHub documents this import path](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/managing-rulesets-for-a-repository#importing-a-ruleset).

The imported template is **disabled**. Only after step 1 proves the separate author/reviewer, change its enforcement to Active in the web UI. Verify main is targeted, no bypass actors exist, force pushes/deletions are blocked, history is linear, PRs and resolved threads are required, and the strict up-to-date `Required checks` context is bound to GitHub Actions App `15368`. General approval count is **zero** with mandatory CODEOWNERS review and stale approvals dismissed: ownerless routine components need no human approval; sensitive changes still do. Do not add a bypass actor, globally require last-push approval, remove CODEOWNERS review or silently substitute a weaker rule if the UI/plan cannot represent this policy. Classic protection alone is not this controller's observable ruleset gate.

### 4. Perform one isolated real protected smoke merge

1. Keep #7 open. Retarget it to main when appropriate and use it to confirm that the shared-Owner author is rejected; do not merge it as positive evidence. Its historical passing/failing CI remains useful, but cannot demonstrate independent authorship.
2. From a fresh independent Codex connection/publisher, open **one necessary positive smoke PR** changing only `docs/development/guides/routine-integration-smoke.md`, on a current `codex/` branch to main. This can be the identity proof PR after updating its branch to current main. Its creation must genuinely use the new principal; do not change display names or manually reattribute #7. Confirm current PR CI passes with an automation original actor. The human may rerun its CI without supplying an approval to the controller.
3. In **Actions → Routine integration → Run workflow**, choose branch `main`, operation `audit`, and inspect the summary. Identity errors, missing rules, API denials and stale branches must be fixed without bypassing gates.
4. Copy the positive smoke PR number and full 40-character latest head SHA from its Commits page. In the same Run workflow form choose `smoke-test`, enter that exact PR/head and run it as the human Owner account. **Leave routine mode disabled.** This isolated operation accepts only that independent-author, one-file harmless revision and executes all normal protection/CI gates. It cannot merge sensitive files, an Owner-authored PR, arbitrary PRs or a changed head. If no exact merge is confirmed, the job fails and provides no activation evidence.
5. Verify the **Protected smoke test** job is successful and the selected PR is actually squash-merged into main by the Actions integration identity. Record the run ID from its browser URL, PR number and merge commit. A dry-run, skipped job, manual merge, label or agent approval is not a receipt.

### 5. Enable routine merging only after that success

In the repository Actions variables UI, set `ROLVYA_VERIFIED_SMOKE_RUN_ID` and `ROLVYA_VERIFIED_SMOKE_PR` to the real successful smoke run and PR. Then set `ROLVYA_MERGE_MODE=enabled`.

The controller does not trust those variable names as assertions: it reads GitHub's run/workflow/job/PR/commit metadata, verifies the exact independently authored harmless merge occurred inside that successful run, confirms the merge is on main, and compares the complete `.github/` plus AGENTS security tree against the verified revision. Missing/forged/stale receipts or changed security configuration block routine integration. Ordinary source/docs changes outside the security tree do not invalidate it. Repeat the isolated smoke after a security/workflow/role/classification change. Do not reset/relabel an old run as new evidence.

After activation, CI completion and a 15-minute poll evaluate routine PRs without a manual merge click. Failures, conflicts, stale branches and sensitive changes need author/Owner attention; the controller does not automatically repair source or update branches. Set `ROLVYA_MERGE_MODE=disabled` or remove it to stop. There are no enrolled native auto-merges to cancel.

## Verification and remaining external blockers

Deterministic tests cover role separation/deadlocks, observed profile and PR provenance, both future source kinds with real temporary source/test boundaries, critical paths/symlinks/configs, language-job omissions/failures, strict checks/conflicts/races, isolated smoke scope and receipt forgery/staleness. Temporary source fixtures are not Rolvya application implementations. The earlier #6/#7 CI pass → intentional missing-link failure → repaired pass verified CI, not protected auto-merge or an independent identity.

Still external: an accepted independent Codex connection or implemented App publisher, observed real independent authorship and reviewability, human bootstrap review, actual repository rules/permissions, and a successful protected smoke. No new identity, protection setting or protected merge is claimed available merely because configuration/tests exist. No activation is performed in this revision.

Related: [review governance](review-process.md), [testing](testing.md), [engineering standards](standards.md), [project status](../project/status.md).
