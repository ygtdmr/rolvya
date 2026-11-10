---
status: Implemented build tooling; complete offline system acceptance remains blocked
---

# Release candidate validation

G1-12 adds a build-host Rust tool, `apps/release`, around the existing controller packager. It is not deployed as a controller or credential authority. No migration or runtime authorization changes are included.

Final verification also reproduced a SQLx 0.8.6 pool-close race: a connection returning after the initial idle scan could survive an awaited close. `PlanStore::close` now drains those finite late returners after closing acquisition. A deterministic real-SQLite barrier test fails with the former implementation and verifies zero remaining pooled connections, committed bytes and integrity with the correction. This is graceful pool shutdown, not a guarantee that SQLite WAL files are empty or removable. Metadata tests use a separate unopened inode for deliberately invalid sidecars and preserve real database close/reopen checks.

Email browser tests retrieve only an exact, new, independently delivered synthetic login message through private test-process IPC. They retain actual Rust/TLS/UI authentication and assert the first confirmation/session/projects/snapshot requests independently, with redacted status diagnostics. The fixture does not introduce a production mail authority, bypass explicit Magic Link confirmation or weaken cookies, CSP or Service Worker tests.

## Commands and provenance

From clean reviewed source, with Rust 1.99.1, Node 24 and npm 11.9.0:

```bash
bash scripts/package-controller.sh /absolute/new/candidate
target/release/rolvya-release verify /absolute/new/candidate EXPECTED_MANIFEST_SHA256
target/release/rolvya-release archive "$PWD" /absolute/new/candidate EXPECTED_MANIFEST_SHA256 /absolute/new/candidate.tar.gz
target/release/rolvya-release verify-archive /absolute/new/candidate EXPECTED_MANIFEST_SHA256 /absolute/new/candidate.tar.gz
target/release/rolvya-release installed-prefix /absolute/new/candidate EXPECTED_MANIFEST_SHA256
target/release/rolvya-release require-offline /absolute/new/candidate EXPECTED_MANIFEST_SHA256
```

The last command currently fails: mandatory installed-system acceptance is incomplete. The prefix fixture requires a dedicated non-root build-host identity and genuinely protected filesystem metadata. It never accepts a test success flag or changes host ownership guards.

`SOURCE.json` records source/lock/toolchain/target/build inputs. `MANIFEST.json` commits every payload file, size and mode. `MANIFEST.sha256` is detached integrity metadata and is explicitly outside the manifest's payload to avoid circular hashing. Obtain the expected manifest digest through a trusted channel; hashes alone do not authenticate a publisher or protect a fully compromised build host. The archive is deterministic USTAR/gzip with commit time, sorted names, fixed ownership and verified payload bytes. Verification never extracts untrusted archive paths.

The package contains only the optimized controller, four production web files, configuration example, systemd unit, MIT license, third-party notices, SPDX 2.3 SBOM, provenance, coverage and evidence reports. It contains no Node runtime, build tool, credentials, databases, backups or recovery kit. Distribution inspection rejects extra files, traversal, links, special files and unexpected executable modes. Builds reject unreviewed compiler/native SQLite overrides. A fresh target directory is supported for independent rebuilds.

## Supply chain

The SBOM traverses the locked Linux controller normal/build closure, separately describing build-only dependencies. A normal closure can include compile-time procedural macros; membership does not claim every crate is linked into the executable. Rendered browser modules retain their exact npm installation directories and versions; type-only and test tooling are not labeled deployed code. Cached registry archive SHA-256 values are checked against the exact Cargo-generated v4 lock checksum before inclusion; extracted-source reuse cannot waive that check. Unsupported lock formatting fails closed. Deployed-file SHA-256 values and SPDX relationships are included. The pinned upstream SPDX schema is validated with the already locked AJV build dependency. Unknown license expressions and missing notices fail packaging. Nested vendored notices are retained.

RC-01 ([#45](https://github.com/ygtdmr/rolvya/issues/45)) identifies Dependabot alert #1 as GHSA-8ffr-xgwf-xj56 in `aws-smithy-json` and locks the patched 0.62.7. The full-lock `rsa 0.9.10` (RUSTSEC-2023-0071) entry is a reviewed lock-only exception, absent from every normal/build/dev graph on every target. The newly published bundled-SQLite advisory RUSTSEC-2026-0459 has no reachable patched SQLx release and is a **Proposed** scoped exception pending Owner security review. Evidence, scope and removal conditions: [dependency advisory hygiene](dependency-advisories.md). External font/icon resources are not bundled; their separate provenance remains in [frontend notices](../../apps/web/THIRD_PARTY_NOTICES.md).

## Installed acceptance and release decisions

The installed-prefix fixture executes the manifest-verified optimized controller directly. It creates a disposable private layout, guarded key setup, a separately signed **synthetic** enrollment proof, real password-verifier enrollment, existing-key restarts, encrypted SQLite backup, independent encrypted key kit, isolated database/key restoration and readiness checks. Wrong-key/replay attempts fail; restored financial/provider holds remain closed. Reports contain only stage outcomes, durations and sampled subprocess RSS. Temporary identities, passwords, signing material, database and recovery bytes are deleted with the fixture and never uploaded as evidence. Zeroization/substrings cannot prove hardware or whole-host secrecy.

This prefix does **not** prove real password login over installed HTTPS, authenticated project binding, complete worker scheduling, independent verification or Git publication through the installed service. Existing genuine SQLite/TLS/HTTP component suites remain valuable separate evidence. `COVERAGE.json` enumerates every PR-001–037 and AC-001–184, retaining missing implementation separately from unavailable external authorization. Test-source links locate fixtures; they are not execution receipts.

The release job runs after existing repository, secret, Rust and frontend gates, builds/inspects the actual archive, executes the prefix, rebuilds into a fresh Cargo target and compares every distributed byte. It uploads only a build candidate and redacted reports to CI artifacts. A green **assessment job** means assessment completed; it is not offline release qualification. No tag, GitHub Release, production deployment or merge is created.

`buildCandidateQualified` can describe a successfully sealed build. `offlineReleaseCandidateQualified` remains false while mandatory evidence is absent. Live AWS/charging/email/PKI/off-host evidence has separate production requirements and explicit offline-protocol counterpart checks. Missing real provider proof alone cannot make a fully demonstrated offline simulator journey fail; the current missing installed journey still does. `productionReleaseQualified` and `paidExecutionAuthorized` remain false; no imported report or configuration override can grant execution authority.

## Remaining release domains

| Domain | Current blocker; component evidence is not an installed-system substitute |
| --- | --- |
| Rust/controller | Final-head repository/Rust/CI outcomes must accompany the candidate; source inspection alone is insufficient. |
| Immutable DAG | Frozen validation, atomic admission and R09 fixtures exist; installed main-planner-to-dispatch journey is incomplete. |
| Worker lifecycle | RC-02: the production service also runs the R06/R08/R09 control loop in **closed** mode (admits nothing) with durable liveness, graceful stop and SIGKILL-tested restart; see [installed runtime](installed-runtime.md). Offline execution authority (RC-04) and installed end-to-end acceptance (RC-05) remain. |
| Independent verification | Separate verifier/report/R09 fixtures exist; installed remote execution composition and live isolation remain unqualified. |
| Git publication | Private GitHub protocol fixtures exist; installed workflow and real App/protected-repository evidence remain absent. |
| Human authentication | Genuine library TLS/browser fixtures exist; installed positive TLS login/binding journey remains unverified. |
| Owner approvals | Existing action-bound Owner authority remains mandatory; prefix enrollment cannot grant project or budget authority. |
| Financial hard budget | Existing integer ledger and holds remain authoritative; no new billable gate opens. |
| Multi-account provider authority | Separate API/CLI connections remain; genuine account-specific permissions and hosted CLI eligibility remain absent. |
| AWS isolation | Static/SDK fixtures do not establish effective IAM/network/runtime containment. |
| External charging bounds | Provider stopping/billing evidence remains absent; estimates are not enforceable bounds. |
| Restore/reconciliation | Actual encrypted prefix restore retains a permanent hold; external state is never assumed rolled back. |
| Email delivery | Synthetic signed delivery fixtures are not genuine operational delivery or an identity issuer. |
| TLS/PKI | Production server requires public PKI and qualified persistent storage. Private CA constructors remain test-only; overlay/tmpfs is not certified controller storage. |
| Off-host recovery | Local fixture destinations do not prove independent off-host custody or retained-key rotation. |
| Dependency advisories | Dependabot #1 (`aws-smithy-json`) is patched; `rsa` is a lock-only reviewed exception; bundled SQLite RUSTSEC-2026-0459 is a Proposed exception pending Owner security review and an upstream SQLx release. CI does not yet run `cargo audit`. See [dependency advisories](dependency-advisories.md). |
| CI/CodeQL | Only exact-head completed analyses count; skipped/missing jobs do not pass. |
| Artifact reproducibility | Independent clean-target rebuild and complete byte comparison are required, not two cache hits. |
| Browser/accessibility | Four-file CSP/PWA/browser gates remain; final icons, physical devices and manual screen-reader certification are outstanding. |
| Production operations | Activity archive/checkpoint/migration and retained-version KEK rotation are missing implementation; routine protected smoke requires Owner re-verification. |

Next dependency: complete installed controller runtime composition and mandatory scope gaps before a full offline RC. Real identity, mail, PKI, off-host storage, provider/IAM and financial qualification require separate authentic authorization/evidence. No live operation is authorized by this tooling.
