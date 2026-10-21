---
title: GitHub App Authorization and Remote Change Isolation
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [github, security, git, permissions, isolation]
---

# GitHub App authorization and remote change isolation

## Authority and decision status

**Proposed implementation detail, NOT an approved GitHub App selection.** [ADR 0008](../decisions/0008-github-authentication.md) recommends GitHub Apps over OAuth/PAT for evaluation. **No GitHub App has been installed or configured for Rolvya by these documents.** Integrate only with remote Git services. Personal-computer Local Agent, Local Sync and local desktop repository access are forbidden.

GitHub documentation permits an installation access token to be scoped to selected repositories and a subset of installation permissions; installation tokens expire after about an hour. This limits repository/permission scope but **does not in itself enforce an arbitrary branch-prefix write restriction**. A token with Contents write can potentially write to other permitted refs depending on repository rulesets/branch protection. Treat task-branch policy as an independent technical control and security test.

## Responsibilities and privilege separation

| Component | Purpose | Suggested GitHub permission | Additional guard |
| --- | --- | --- | --- |
| Repository connector (read) | Inspect tree, commit, refs and metadata | Metadata read and Contents read as necessary | Confirm selected installation/repository ID |
| Attempt branch publisher | Write isolated task change to remote ref | Contents write on selected repo if necessary | Rust-mediated write with allowed-ref policy and approved base SHA; provider branch rules |
| Review/PR publisher | Create/read PR and validate status | Pull requests read/write as needed | Actor permission, target and source ref review |
| Merge authority | Apply verified changes to protected integration branch | Minimal permission required by selected merge API | Separate controller authorization and GitHub protection; workers never merge |
| Webhook handler | Observe installation/PR/repository events | Webhook configuration and signature verification | Signature, delivery ID dedupe, audit log and replay policy |

**Exact GitHub API endpoint-to-permission mapping must be verified against current official docs before installation**; GitHub's permission matrix is endpoint-specific. Avoid requesting Administration or Workflows write unless justified and owner-approved.

## Proposed installation and identity flow

1. Owner selects a repository and initiates GitHub App installation through an authenticated Rolvya owner session.
2. Callback binds installation ID, GitHub repository ID (not merely a mutable owner/name string), project ID, owner authorization and approved permission inventory in SQLite.
3. Store the App signing key/token material through a secure provider/secret manager; SQLite holds only references and operational metadata.
4. Before a write, an authorized Rust adapter requests a **short-lived installation token narrowed to the required repository and permissions**, where supported.
5. Token never goes into the LLM prompt, shared worker logs, downloadable artifacts or repository files.
6. Task output is published to an isolated attempt-specific remote ref. Validate pinned source SHA, destination ref policy, allowed file changes and GitHub rules.
7. Separate review/merge control verifies task result and current target SHA before a merge/PR update; record resulting immutable commit SHA and idempotency/reconciliation evidence.
8. On installation revocation or permission loss, stop privileged Git writes, fence affected attempts, preserve completed artifacts and notify owner.

## Important security caveat: token vs branch policy

Installation tokens are repository/permission-scoped, not automatically restricted to `refs/heads/rolvya/*`. For strong branch isolation:
- Prefer keeping broad GitHub write credentials **only inside a trusted Rust integration service**, with workers delivering signed/hashed artifacts instead of receiving raw write tokens.
- Perform ref allowlisting and expected-target SHA compare-and-swap in trusted code.
- Configure GitHub repository rulesets/branch protections against protected-branch writes. Evaluate bypass permissions and provider limitations.
- A worker must never hold a credential that can bypass the protected merge policy merely because its task was ready.

If the product later directly gives attempt workers write credentials, explicitly document the weakened trust boundary and require an approved security review.

## Proposed remote Git change manifest

```json
{
  "project_id": "example",
  "plan_hash": "sha256:example",
  "task_id": "task_example",
  "attempt_id": "attempt_example",
  "repository_id": "github-numeric-id",
  "base_commit": "example-sha",
  "proposed_ref": "refs/heads/rolvya/example-task/attempt-1",
  "result_commit": "example-result-sha",
  "diff_hash": "sha256:example",
  "tests_ref": "artifact://test-report/example",
  "merge_status": "pending-review"
}
```

This is illustrative JSON, not an approved public API or an actual GitHub commit. Git SSH pushes and REST Git refs can behave differently; test the selected mechanism with exactly the grants requested.

## Write idempotency and conflict resolution

Before any publication, create a durable operation ID and expected base SHA in SQLite, then issue remote Git write. If the network fails after provider success, query the branch/ref/commit before retry; duplicate operations must not create second effective changes. On divergence, stop automatic overwrite and present conflict evidence for integration review. A valid textual merge alone does not guarantee tests, semantic compatibility or security.

## Threat matrix and negative tests

| Scenario | Expected failure-safe behavior |
| --- | --- |
| App installed to wrong repo/installation | Reject project connection |
| Worker tries protected `main` using scoped context | No worker privilege/credential permits this direct write |
| GitHub installation revoked mid-attempt | No new privileged operation; task marked blocked and audit maintained |
| Malicious README requests token printing | Reject as untrusted source instructions |
| Stale base SHA during merge | Detect divergence and halt integration |
| Duplicate webhook delivery | One event effect, not duplicate task/result/invoice |
| Lost response after branch publication | Look up remote ref and reconcile, never blindly re-push |
| Pull request attempts changing unapproved paths | Flag and reject from trusted validation gate |

## Owner decisions and future ADRs

GitHub App vs OAuth final choice, exact permission scopes and installation UI, worker-to-integrator artifact transfer, branch/PR strategy, approval for merges, private repositories, enterprise/self-hosted support and key storage require separate authorization.

## Official references

- [GitHub installation access tokens](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app)
- [GitHub App REST endpoint permission matrix](https://docs.github.com/en/rest/authentication/permissions-required-for-github-apps)
- [GitHub repository rulesets](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/about-rulesets)

Related: [Git provider architecture](integration-github.md), [ADR 0008](../decisions/0008-github-authentication.md), [identity/authorization](identity-authorization.md), [worker transport](worker-transport-design.md), [security](security.md).
