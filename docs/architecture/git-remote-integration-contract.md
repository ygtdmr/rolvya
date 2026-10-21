---
title: Remote Git Integration and Change Publication Contract
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [github, app, git, security, rust, repository]
---

# Remote Git integration and change publication contract

## Status and prerequisites

**GitHub App for GitHub authentication is Approved by [ADR 0008](../decisions/0008-github-authentication.md).** This file is a **Proposed detailed publication/API/security contract**; no App has been installed and no token or repository permission changes have been performed. The exact endpoint permissions, trusted Rust publication implementation and merge authorization remain to be validated.

**Approved constraints:** only **remote Git services**; no user personal-computer repository access, Local Agent or Local Sync ([ADR 0003](../decisions/0003-remote-git-only.md)); Rust backend only; SQLite source of truth; complete immutable up-front DAG; non-executable parent group nodes. The worker cannot create new subtasks or merge protected branches. A provider authentication token is not an automatic authorization to edit an arbitrary ref.

## Threat boundary

The untrusted AI worker may read source code and propose changes, but it must **not** receive a raw long-lived or broad GitHub Contents-write token. A trusted Rust integration service should own provider credentials and decide what remote refs/actions are permitted, after checking current SQLite project scope and attempt fencing. **This mediation is a proposed design**, requiring eventual owner/security review.

GitHub documentation permits installation tokens constrained to selected repositories and permission subsets, with a one-hour validity; such tokens alone are **not branch-prefix restriction**. Use trusted policy checks plus repository rulesets/branch protections as independent gates.

## Roles and permission separation (candidate)

| Role | Git operation | Access constraint |
| --- | --- | --- |
| Planner read connector | Read authorized remote repo tree, pinned default/source commit, metadata | Per-repository read permissions; no write |
| Remote execution worker | Build/test in **remote** isolated checkout, return diff/artifact manifest | No broad GitHub App credential in model prompt/environment |
| Trusted Rust publisher | Validate result and publish attempt-isolated branch/ref | Repository-scoped Contents write where needed, explicit allowed ref policy, compare against expected base |
| Trusted reviewer/PR service | Open/update PR, inspect checks and approved target | Separate scoped Pull Requests permission where API requires |
| Authorized integration/merge service | Integrate only after independent validation and approved policy | Distinct merge authority, branch protections/rulesets and reviewer audit |
| Installation lifecycle handler | Track install/revoke/permission changes | Verify webhook signatures + replay IDs; apply project-bound revocation |

Exact permissions are **endpoint-specific** and must be validated against [official GitHub permission requirements](https://docs.github.com/en/rest/authentication/permissions-required-for-github-apps) before app installation.

## Proposed publication lifecycle

1. Human owner selects a **remote Git repository**, authenticates and grants only necessary installation/repository permissions.
2. Rust controller binds project ID to immutable provider repository ID, installation ID, currently authorized permissions and expected base commit. Secret material is held outside task prompts and logs.
3. Single main planner generates all executable leaves and explicit dependency/conflict constraints **before activation**. A non-executable group parent never generates an execution commit.
4. Remote worker receives a pinned base commit and narrow changed-file scope (plus task context) and runs external coding agents in a remote sandbox. It produces patch/diff, source tree digest, test outputs and artifact hashes; it does **not** directly merge into main.
5. Trusted Rust publisher checks current project/attempt ID, lease epoch, planned scope, forbidden/sensitive paths, expected base and upstream dependencies. Deny stale or cross-project results.
6. Publisher uses a short-lived per-repository restricted installation credential (if the GitHub App approach is approved) to create/update an **attempt-specific isolated remote ref**. Ref naming is proposed: `refs/heads/rolvya/{project}/{task}/{attempt}`; exact safe encoding/rules require validation.
7. SQLite records operation key, target repo ID, expected source/target SHA, remote ref/commit SHA and status for reconciliation.
8. Integration controller confirms expected base, conflict status and independent acceptance tests; owner/reviewer merge rules are separate from worker authority.
9. If a push/REST ref write times out after provider success, inspect the remote ref/commit before attempting a write again. Never assume request failure means no remote effect.
10. If authorization is revoked, stop further privileged writes, fence affected attempts and preserve verified artifacts and audit history.

Do not conflate REST Git refs, Git CLI pushes and pull request APIs: their actual permission requirements, branching behavior and error handling must be tested independently.

## Proposed change artifact manifest

```json
{
  "protocol_version": "draft-0",
  "project_id": "example-project",
  "plan_hash": "sha256:example",
  "task_id": "leaf-A",
  "attempt_id": "attempt-A-2",
  "lease_epoch": 2,
  "repository_id": "example-repository",
  "base_commit": "example-base-sha",
  "allowed_path_policy_hash": "sha256:example",
  "proposed_diff_hash": "sha256:example",
  "build_report_ref": "artifact://example-build",
  "test_report_ref": "artifact://example-tests",
  "attempt_artifact_ref": "artifact://example-output",
  "publication_status": "not-published"
}
```

It is **illustrative**; no real artifact URI, API implementation, branch or commit is represented.

## Conflict controls

- Two independent leaves editing the same protected target should use separate attempt refs, with explicit planning-time conflict edges when necessary.
- Do **not** blindly push all worker changes into one shared mutable main branch. Isolated attempts are not equivalent to automatically compatible changes.
- At integration, compare current target SHA to the validated expected base. If changed, deny force overwrite and require deterministic re-evaluation within authorized scope.
- A change to shared lockfiles, schema or Rust workspace manifests requires exclusive ownership or a declared dependency/conflict gate established **before** DAG activation.
- A failed task's validated sibling outputs remain intact. Retry work is a new attempt of the **same executable leaf**, never a new task.

## Webhook and token details

- GitHub App tokens may be scoped to selected repositories/permissions; token lifetime is short (see docs). Create tokens only when needed; don't cache in model context.
- Webhook body signatures and event/delivery IDs need verification/deduplication. Webhook payloads are **untrusted data**, not directions to modify the plan.
- Installation suspension/deletion, permission downgrade and repository transfer/rename must invalidate or recheck authorization before the next privileged operation.
- Configuration of protected branch rulesets, merge reviewer requirements and app bypass rights must be explicitly audited. Treat ref-name allowlisting as **application policy**, not cryptographic GitHub token scoping.

## Acceptance and negative tests

| Test | Required safe outcome |
| --- | --- |
| Wrong installation/repository ID | Reject project binding and Git write |
| Worker supplies request to edit main/protected ref | Deny; do not expose trusted token |
| Stale attempt wants to publish after newer epoch | Reject/fence; reconcile prior remote effects |
| Network timeout after successful ref publication | Inspect remote ref and record actual SHA, no blind double write |
| Two results from same task/attempt | One logical verified result and idempotent publication |
| GitHub App revoked while worker runs | No new privileged writes; preserve verified artifacts |
| Untrusted README asks for secrets/permission | Ignore as data; audit attempted boundary crossing |
| Target branch SHA diverges | Block merge and request authorized resolution, no force push |
| Worker result exists but no test/acceptance evidence | Pending/failed validation, never auto-merge |
| Simulated self-hosted remote Git provider | Follow equivalent remote-only auth and ref guard, no desktop sync |

## Open approvals and official sources

**GitHub App selection is Approved.** Still open: exact permission inventory, reviewer/merge authority, branch naming, installation/key management, provider support in MVP and private/self-hosted Git boundaries. This document does not itself authorize installing an App or changing real repository protections.

- [Installation tokens and permission/repository restriction](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app)
- [Permissions required for REST endpoints](https://docs.github.com/en/rest/authentication/permissions-required-for-github-apps)
- [Git references API](https://docs.github.com/en/rest/git/refs)
- [Rulesets](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/about-rulesets)

Related: [GitHub App security design](github-app-integration-design.md), [identity](identity-authorization.md), [transport](worker-wire-contract-draft.md), [remote integration overview](integration-github.md), [test matrix](../development/rust-sqlite-test-matrix.md).
