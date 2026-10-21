---
title: "ADR 0008: GitHub Provider Authentication"
status: Proposed
category: decision
last-reviewed: 2026-10-21
tags: [adr, github, git, security, oauth]
---

# ADR 0008: GitHub provider authentication

## Status and boundary

**Proposed — NOT approved.** Rolvya only integrates with **remote Git provider services**; it must never introduce a personal-computer Local Agent, Local Sync or direct desktop repository access. GitHub is the first documented candidate; support for self-hosted remote Git services remains a separate adapter design question.

## Requirement

Rolvya must access authorized remote repositories with least privilege, permit independently executing remote worker tasks to publish isolated refs, support revocation and enforce human-reviewed integration. Secrets must not enter model prompts, repository commits or untrusted logs.

## Alternatives

| Option | Useful properties | Important tradeoffs |
| --- | --- | --- |
| **GitHub App installation** | Repository-scoped installation permissions, fine-grained access, short-lived installation tokens, webhooks | App registration, key security, installation administration and permission design |
| OAuth App with user token | User-delegated access model for interactive activities | Broad scopes and access lifecycle concerns; automation may depend on user identity |
| Fine-grained PAT (manual) | Simple prototype troubleshooting in limited circumstances | Manual renewal/rotation, human-owned secret and difficult multi-tenant governance |
| Deploy key | Narrow Git repo access for constrained use | Key lifecycle; does not replace GitHub API/PR permissions |
| Self-hosted Git adapter | May use service-specific token, SSH or OAuth | Must validate provider-specific scopes and operation semantics |

GitHub's documentation generally recommends GitHub Apps over OAuth apps for integrations because Apps can constrain permissions to repositories, use finer scopes and short-lived tokens. This is evidence **for a proposal**, not proof a GitHub App has been registered for Rolvya.

## Working recommendation

**Prefer a GitHub App installation model** for GitHub repository access in the initial supported integration, if owner agrees. Design distinct read-only planning access, isolated attempt branch write capability, and tightly controlled merge/release authority; do not give every worker repository administration or default-branch write privileges.

### Proposed authorization flow

1. Owner selects target remote GitHub repository and reviews the app's requested permissions.
2. Installation ID and allowed repository IDs are bound to the Rolvya project; store only secret references and provider metadata.
3. Control plane requests narrowly scoped, short-lived installation credentials after policy and budget admission.
4. Remote workers use project/attempt-scoped execution identity; ideally only isolated remote ref writes are permitted.
5. An authorized integration controller or reviewer validates test evidence and expected base commit before merging.
6. Installation removal/revocation invalidates subsequent privileged operations; pending attempts are fenced and audited.
7. Every write records task ID, attempt ID, idempotency key and resulting immutable commit/ref.

Exact GitHub App permissions, authentication grant flow and rotation process require engineering security review and a follow-up implementation contract.

## Threat model and acceptance

- Wrong-repository installation or lost owner access → reject.
- Untrusted README/issue asks to print tokens → ignore and protect secret boundary.
- Leaked installation token → short validity, revocation and limited repository exposure.
- Stale task tries to update main → controller denies protected ref write.
- Network timeout after successful Git push → reconcile remote commit/ref, never blindly re-push.
- App installation revoked mid-run → block new privileged actions and preserve validated sibling work.
- No operation may reach a local repository or personal-computer agent.

## Decision gate

Owner must approve GitHub App model and requested permission matrix, confirmation UI, merge policy, and self-hosted compatibility boundary. Tests should use a dedicated throwaway remote repository and separate fake-provider adapters before any production repository is touched.

## Official reference

- [GitHub documentation: deciding when to build a GitHub App](https://docs.github.com/en/apps/creating-github-apps/about-creating-github-apps/deciding-when-to-build-a-github-app)
- [GitHub Docs: GitHub App versus OAuth](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/differences-between-github-apps-and-oauth-apps)

Related: [OQ-007](../project/open-questions.md), [remote Git integration](../architecture/integration-github.md), [security](../architecture/security.md), [review](../development/review-process.md).
