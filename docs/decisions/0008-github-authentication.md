---
title: "ADR 0008: GitHub App Authentication and Trusted Remote Git Integration"
status: Approved
category: decision
last-reviewed: 2026-10-21
tags: [adr, github, app, git, security]
---

# ADR 0008: GitHub App authentication and trusted remote Git integration

## Status and approval

**Approved by the project owner on 2026-10-21.** **GitHub App installations** are the selected authentication approach for **GitHub integration** in Rolvya. The App's exact permission inventory, installation process and merge approval policy are implementation/security decisions that still need definition. This approval is **not** a claim that a GitHub App has already been created or installed.

**Other remote Git providers**, including self-hosted remote services, can be added through separate authorized adapters; this ADR selects the mechanism specifically for GitHub. Direct access/sync to the user's personal-computer repositories remains forbidden ([ADR 0003](0003-remote-git-only.md)).

## Decision

1. Connect selected GitHub repositories through **GitHub App installations** with narrow, explicit repository permission grants, rather than using PAT or delegated OAuth as the default product integration.
2. Bind GitHub's immutable repository identity, installation ID and actual granted permissions to the authorized Rolvya project in **SQLite**; require authenticated owner consent for installation/configuration changes.
3. The **trusted Rust Git adapter/publisher** holds GitHub App signing material and obtains short-lived, repository-/permission-scoped installation tokens where supported. **Do not give broad Contents-write installation credentials to untrusted model-visible worker environments.**
4. Remote workers produce scoped patches or artifacts with test and provenance reports. A trusted Rust publisher independently validates permitted paths, pinned source commit, current attempt fence and authorized remote ref before creating or updating an **attempt-isolated branch/ref**.
5. **Repository token scope is not branch scope.** GitHub App Contents-write permission alone cannot guarantee a token only writes `refs/heads/rolvya/*`; enforce ref allowlists and expected SHA in trusted code **and** use GitHub repository rulesets/branch protection for protected targets.
6. Merge/integration has **separate authorization and review**; workers never merge protected refs. Exact owner/reviewer rules are still to be approved.
7. Recheck permissions before privileged writes; on installation removal, suspension or permission loss, block writes/fence affected attempts while preserving artifacts.
8. All provider operations use durable operation IDs and reconcile a remote ref/commit on lost acknowledgment before retrying. Do not treat ambiguous Git write outcomes as failures that are safe to blindly repeat.
9. **Rust backend only**, **SQLite authoritative DB only**, remote Git only, immutable activated DAG, executable leaves only and budget approval policies remain binding.

## Proposed permission design (requires review before installation)

| Workflow | Minimal intended capability | Additional restriction |
| --- | --- | --- |
| Repository discovery/reading | Metadata/read, Contents/read as needed | Only owner-selected repositories |
| Publish isolated task refs | Contents/write where required | Trusted Rust publisher, scoped repo token, ref allowlist, expected base SHA |
| Open/update reviewable pull requests | Pull requests/read-write only when needed | Authorized project and task, trusted integration identity |
| Merge into protected branch | Permission per selected GitHub endpoint/branch protection | Independent validated evidence and explicit approved merge policy |
| Webhook processing | Installation/PR events | Signature validation, delivery dedupe, revocation handling |

The actual endpoint-to-permission matrix must be verified using current [GitHub permissions documentation](https://docs.github.com/en/rest/authentication/permissions-required-for-github-apps). Avoid unnecessary Administration, Workflows or organization-wide permissions.

## Security and acceptance

- Reject wrong-repo installation, forged webhook, cross-project ID, stale task epoch, invalid pinned SHA and ref outside allowlist.
- Reject protected-branch publication by a worker even when App installation has repository Contents-write rights.
- Keep App private keys, installation tokens and renewal material outside LLM-visible prompts, repository files, worker outputs and logs.
- Detect installation revocation/permission changes; no future privileged writes after loss of authority.
- On lost response after provider success, query actual ref/commit before retrying; block conflicting base revisions rather than force overwrite.
- Preserve verified sibling artifacts even if GitHub integration fails.

## Alternatives and future compatibility

PAT, OAuth app and deploy keys may be used for specific independently approved third-party/self-hosted provider adapters if justified, but **GitHub App** is the binding default for GitHub. No personal-computer local Git integration may be introduced. Self-hosted provider compatibility and merge rules require separate design decisions.

## Approval record

- **Choice:** GitHub App for GitHub authentication, with least-privilege installation and trusted Rust Git publication.
- **Status:** Approved.
- **Approved by:** project owner in conversation, 2026-10-21.
- **Implementation status:** not verified; no App installation or production permission change authorized by this documentation update.
- **Open:** exact GitHub App permissions, installation UI, key storage/rotation, webhook configuration and independent merge policy.

## Sources and related specifications

- [GitHub App installation tokens](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app)
- [GitHub App REST endpoint permissions](https://docs.github.com/en/rest/authentication/permissions-required-for-github-apps)
- [GitHub rulesets](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/about-rulesets)
- [Remote Git publication contract](../architecture/git-remote-integration-contract.md)
- [GitHub integration design](../architecture/github-app-integration-design.md)
- [Authentication/authorization model](../architecture/identity-authorization.md)
