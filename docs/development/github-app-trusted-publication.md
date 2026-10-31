---
title: G1-01 trusted GitHub App remote publication
status: Implemented
last-reviewed: 2026-10-30
---

# Trusted GitHub App remote publication

G1-01 is an **offline Rust/SQLite foundation**, with real RS256 signing and bounded GitHub REST request/response handling exercised through private HTTP fixtures. No App was created or installed. The production connector has no client and returns `LiveIntegrationDisabled`; there is no public transport, verifier, credential-export or activation override.

Base: PR [#32](https://github.com/ygtdmr/rolvya/pull/32), merged as `1cb3b2638f71faf24bc2ae8ce188d5ccb59e9a71`, with its complete tree verified against `9d725f6aecd0791efedaa14ab8c78e7d2f803183`. Approved authority remains [ADR 0008](../decisions/0008-github-authentication.md), [remote-only Git](../decisions/0003-remote-git-only.md), the [Git contract](../architecture/git-remote-integration-contract.md) and existing human, financial and worker controls.

## Implemented boundary

- `controller_keys::github_app::{GitHubRemote, RemoteGit, RepositoryBinding, RepositoryId, InstallationId, AppId, CredentialReference, GitSha, PinnedSource}`. Numeric immutable IDs authorize scope; owner/name is validated mutable location metadata. One project has a versioned binding; changing it requires another exact Owner action.
- `HumanAuth::prepare_git_repository` stages a bounded, authenticated proposal. Existing current session/membership, fresh password or dedicated email proof, voluntary enrolled factor, explicit confirmation, five-minute proof consumption, governance audit and notifications apply through the same Owner transaction. A session or legacy Owner string alone grants no binding.
- Private `secret_broker::github_consumer` validates exact project/account/connection/purpose/version and current committed consent, then signs internally with existing **ring 0.17.14** RS256. Temporary decoded material is zeroized where supported. Ring-owned key state and HTTP header copies are not claimed universally zeroizable. There is no raw key getter.
- App JWT uses `iat = now - 60`, `exp = now + 540`, App ID issuer and fixed RS256 header. Separate token requests specify one exact repository and minimal read/publication/PR permissions. Returned scope, expiry, repository metadata and current installation are checked; no 40-character token assumption, redirects, environment credentials or PAT fallback.
- Pinned reads resolve the approved source branch, check exact commit identity through authenticated API correlation, independently recompute complete bounded tree/blob Git identities and preserve provenance. Source text is data. Symlinks/submodules, truncated trees, malformed identities and excessive data fail closed. Limits: 256 entries/directories, 64 KiB/file, 1 MiB aggregate and 2 MiB HTTP body; unsupported names/trees are rejected. This is not arbitrary-size repository support or independent reconstruction of signed source-commit raw bytes.

## Accepted bytes and publication

G1-01’s original R09 path accepts **synthetic result JSON**, not general coding patches. G1-01 publishes exactly those accepted bytes to an Owner-allowed result path. `AcceptedChange` is internal and has no public constructor or deserialization path. A red/green external compile-fail regression guards that boundary; no public publication method accepts caller-manufactured acceptance metadata. It cannot attach unrelated code to a successful task. [G1-02](verified-coding-artifacts.md) now adds a separate typed coding-result path with private synthetic verification; it does not let coding bytes borrow synthetic acceptance. Production independent execution remains blocked.

`GitHubRemote::prepare` calls a narrow R09 bridge in the authoritative transaction. It rechecks actual artifact bytes, accepted manifest/provenance, frozen fingerprint, executable attempt, current fence and succeeded state. Expired execution leases do not erase accepted work. Only trusted generated `refs/heads/rolvya/<installation>/<project>/<task>/<attempt>` identities are used; each component is SHA-256 encoded. There is no caller-chosen destination, force update, deletion or merge endpoint.

Allowed-prefix policy is bound to Owner consent. Traversal, hidden/Git paths, workflow/security/credential configuration, manifest files and unsupported file types are denied. The trusted controller computes the exact content-addressed blob, complete tree and deterministic commit before any remote Git write. Git SHA-1 is used only for Git object protocol identity; accepted evidence, operation commitments and MACs use SHA-256/HMAC.

Forward **0016** adds scoped immutable bindings/proposals, intents, append-only events/deliveries and redacted credential-use evidence. Published 0001–0015 are unchanged. No second scheduler, task ledger or financial journal is introduced.

## Recovery and revocation

Intent and submitting markers commit before remote calls. Each external call happens outside SQLite write transactions. Current consent, credential versions, accepted bytes and fencing are rechecked before writes, including immediately after the last ref lookup; receipt-side authority is rechecked in its transaction. GitHub cannot participate in a universal SQLite transaction: authority can change after the final check, so uncertain effects remain durable and require reconciliation.

An ambiguous write never triggers automatic repeated object/ref creation. Recovery reads exact remote ref/commit state. Missing refs remain unknown; divergent refs are blocked and never overwritten. This intentionally sacrifices automatic progress after an uncertain pre-ref interruption. Reconciliation does not imply exactly-once provider effects.

Signing-record replacement is synthetically tested through a new exact Owner consent: old intents are invalidated, and newly authorized work signs with a distinct new key. This does not add/remove a live App key at GitHub.

Draft PR preparation/creation has a separate durable submitting marker, exact head/repository/base checks, bounded trusted description and read-only lookup after lost acknowledgment. Opening a draft PR grants no merge approval.

Webhook HMAC-SHA256 is checked over the original bounded raw body before parsing. Delivery IDs are unsigned deduplication hints. Previously authenticated body digests are also deduplicated across renewed bindings retaining the same App/installation/webhook-key custody; a changed header cannot create another restrictive effect. This does not prove freshness of previously unseen signed payloads. Authenticated lifecycle events can only hold/revoke authority; none can enable or expand it. Stored decisions are MAC-checked on subsequent use, with a 1,024-delivery inspection bound per binding (exceeding it fails closed; production retention/reconciliation remains unfinished). Re-enabling access requires renewed Owner consent and trusted provider checks. Accepted sibling artifacts and financial holds are preserved.

An unresolved restored snapshot is tested against a separate durable simulated remote database, and reconciles without another write. **Arbitrary stale-snapshot rollback is not detected.** Production activation still requires operational restore fencing and current external inventory; local backups cannot roll GitHub reality back.

## Executable evidence

Fresh focused suite: **41 tests** (including the private subprocess driver), `cargo test -p rolvya-persistence-sqlite --lib github_app --locked -- --nocapture`. It includes real R06/R08/R09 independent A/B execution/acceptance, two distinct remote refs, unchanged frozen graph/accounting, group acceptance still required before C, HTTP JWT verification, scope/expiry/revocation negatives, tampering, duplicate/concurrent publication, lost Git/PR acknowledgments and snapshot consistency.

A private subprocess driver exercises **eight actual SIGKILL points**: before/after intent commit, submission commit, remote ref effect and receipt commit. Driver entries and repeated child runs are not counted as separate guarantees. The provider-effect SQLite is separate from controller state. No physical power-loss, live GitHub permission, real browser or remote protection claim follows.

Independent read-only reviewers inspected credential/token/webhook boundaries and transactional acceptance/fencing/recovery. Both identified the final-ref lookup race; a failing regression observed a ref despite concurrent security-epoch revocation, and the corrected boundary leaves zero refs. A second reproduced corruption regression required authenticating persisted webhook decisions on use. Reviews are technical evidence, not human Owner approval. Fresh full verification: **770 workspace Rust tests**, **15 migration checks over 16 versions**, **203 Node tests**, **23 Python tests**, formatting, warning-denied Clippy, locked compilation, Rust/repository/inventory CI wrappers, Actionlint and tracked-source secret scanning pass. Three additional bounded same-attempt concurrency repetitions pass and are not new test identities. Exact-head CI belongs to the PR and [status](../project/status.md).

## Dependency/security gates

Dependabot [alert #1](https://github.com/ygtdmr/rolvya/security/dependabot/1) remains **inaccessible and unidentified**: the connector rejects the security endpoint and configured `gh api` returns `Forbidden`. It is not dismissed or resolved.

Separately, pinned `cargo-audit 0.22.2` with RustSec database `2295466c4e9df26647fd67d8fa1c705ac2fc7f81` reports unpatched **rsa 0.9.10 / RUSTSEC-2023-0071 / CVE-2023-49092** in Cargo.lock (exit 1). Exact feature-aware Linux normal/build `cargo tree --workspace --all-features --locked --target x86_64-unknown-linux-gnu --edges normal,build -i rsa` has no compiled RSA path. Removing unused SQLx macro support experimentally preserved migration replay but did **not** remove optional MySQL/RSA resolution; that experiment was reverted. A separate no-lockfile resolution probe without macro features also retains MySQL/RSA. A facade/driver dependency migration is deferred to focused remediation. No advisory suppression, patched-version fiction or clean-audit claim. New signing uses ring, not rsa; no new dependency version is added.

## Production gates and next work

Real App registration/installation, protected key onboarding, permission/ruleset/bypass verification, effective webhook trust/transport, live App signing-key rollover and operational restore policy require separate authorized integration evidence. Real-source/commit interoperability and real protected-branch behavior have not been tested. G1-01’s positive scope is accepted synthetic result bytes; [G1-02](verified-coding-artifacts.md) adds independently fixture-verified coding artifacts, with production runner attestation still unavailable. No live credentials, external user-repository publication, email, cloud/model operations or spending occurred. No protected merge or next milestone is started.

## Official contracts checked 2026-10-30

- [App JWT / RS256](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-json-web-token-jwt-for-a-github-app)
- [Installation authentication and token scope](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/authenticating-as-a-github-app-installation)
- [Installation tokens](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app)
- [Endpoint permissions](https://docs.github.com/en/rest/authentication/permissions-required-for-github-apps)
- [Repositories](https://docs.github.com/en/rest/repos/repos), [installations](https://docs.github.com/en/rest/apps/installations)
- [Git blobs](https://docs.github.com/en/rest/git/blobs), [trees](https://docs.github.com/en/rest/git/trees), [commits](https://docs.github.com/en/rest/git/commits), [refs](https://docs.github.com/en/rest/git/refs)
- [Pull requests](https://docs.github.com/en/rest/pulls/pulls)
- [Webhook signature](https://docs.github.com/en/webhooks/using-webhooks/validating-webhook-deliveries), [lifecycle events](https://docs.github.com/en/webhooks/webhook-events-and-payloads)
- [Rulesets](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/about-rulesets), [rate limits](https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api)
