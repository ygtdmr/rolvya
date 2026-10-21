---
title: Remote Git Repository Integration
status: Proposed
category: architecture
last-reviewed: 2026-10-21
tags: [git, github, integration, security]
---

# Remote Git repository integration

## Approved product boundaries

Rolvya operates on **remote Git repositories and services**, including possible self-hosted Git providers. There is no Local Agent, Local Sync, or access/synchronization to the user's personal-computer repositories. GitHub is the initial documented provider, but provider support and authentication implementation are not approved.

## Integration responsibilities

The control plane coordinates repository access and permissions; remote workers use isolated checkouts/workspaces with strictly scoped task instructions. Parallel task changes must not overwrite each other. A completed worker produces a reviewable remote ref, commit identification, artifact manifest, test results, and cost/attempt evidence; the orchestration layer validates before integration.

## Proposed lifecycle

1. **Connect:** user identifies an existing remote repository and authorizes project-scoped access; verify identity and permissions without storing credentials in prompts.
2. **Snapshot:** record provider repository ID, immutable base commit SHA, branch/ref, access limitations, and plan association.
3. **Prepare workspace:** create task-specific remote checkout and isolated branch or equivalent workspace. Never clone from the personal computer.
4. **Execute:** worker performs permitted changes inside its sandbox; capture commands and test evidence with redacted logs.
5. **Publish:** write task output to an isolated remote ref using least-privilege credentials; record resulting commit SHA and operation idempotency key.
6. **Validate:** verify diff scope, acceptance tests, source base, authorizations, secrets, and compatibility with previously validated changes.
7. **Integrate:** serialize conflicting changes, detect divergent base refs, and follow an approved merge/rebase/review authority policy.
8. **Recover:** after crash, reconcile remote refs and commit IDs before retry to prevent duplicate commits or branch collisions; revoke old credentials and leases.

## Minimum proposed connector contract

| Concept | Required meaning |
| --- | --- |
| repository_identity | Provider-specific immutable repository identifier, project ID, allowed refs |
| connection | Human-authorized installation/token reference, granted scopes, expiration |
| checkout_request | Repository, pinned commit SHA, task ID, attempt ID, isolation policy |
| proposed_change | Base SHA, new ref/commit SHA, paths changed, artifact/test manifest |
| integration_request | Expected target SHA, source ref, validation report, authorized actor |
| operation_result | Idempotency key, provider operation ID, status, actual ref/commit SHA |
| failure | Typed permission/timeout/conflict/rate-limit/provider/validation error |

These are logical fields, **not** deployed endpoints.

## Concurrency and integrity

- Pin input revisions; avoid workers writing directly to a shared main branch.
- Before integration, check changed-file overlap and semantic/API conflicts, not just Git's textual merge result.
- Enforce compare-and-swap on expected remote base ref where supported.
- Retry safe reads; require idempotent or reconciled writes before retrying publish/merge operations.
- Record commit provenance linking each change to plan/task/attempt/test evidence.
- Never permit stale attempts to replace a newer accepted result.
- Branch cleanup must not delete refs needed for audit, rollback or preserved completed work.

## Security and cost

Use scoped short-lived credentials or provider-issued installation credentials, secret references, least privilege, audited writes and isolation per project. Treat repository files, issues and PR content as **untrusted inputs** that can contain prompt injection. Log metadata rather than secrets or unnecessary source content. Account for network transfer, storage and worker/API charges under the approved hard budget gate.

## Required failure cases

Authorization revocation, missing repository, force-pushed target ref, merge conflict, branch collision, API rate limits, network disconnect after a successful remote write, failed tests, malicious repository instructions, leaked credential attempts, and stopped budget admission.

## Acceptance tests to implement

- A worker has no API or filesystem route to personal-computer repositories.
- Two independent workers publish separate task refs and preserve both results.
- Stale-base integration fails safely without silently overwriting changes.
- Retry following an acknowledged or ambiguous network failure does not create duplicate irreversible Git changes.
- Authorization revocation prevents subsequent privileged operations.
- Secrets and untrusted repository text do not gain instruction authority.

The [GitHub App authorization and branch isolation design](github-app-integration-design.md) clarifies the independent controls needed for repository permissions and protected refs. Selecting GitHub App authentication still requires owner approval.

## Open decisions

Initial provider coverage, GitHub App vs OAuth/token flow, required permissions, branch and PR strategy, signing, merge authority, fork support, self-hosted provider adapters, quotas and retention. These are not approved solely by appearing here.

Related: [system overview](overview.md), [execution](execution.md), [security](security.md), [data model](data-model.md), [API contracts](api-contracts.md), [requirements](../product/requirements.md).
