---
title: G1-02 verified coding artifacts
status: Implemented
last-reviewed: 2026-10-30
---

# Verified coding artifacts

G1-02 extends the [G1-01 publisher](github-app-trusted-publication.md) with bounded multi-file coding evidence. Base: PR [#33](https://github.com/ygtdmr/rolvya/pull/33), verified merged as `7b152b163dce5a562fa2a263857a86d4a876fc43`; its complete source tree matches `f25f48d35e9920852380309f6c14c5ab1651327e`. [Remote-only Git](../decisions/0003-remote-git-only.md), [trusted publication](../decisions/0008-github-authentication.md), frozen planning and existing budget/worker authority remain binding.

## Implemented contracts

- `orchestration::coding`: `CodingContext`, `CodingResult`, `FileChange`, `VerificationPolicy`, `VerificationReport`, `CodingState` and read-only `CodingConflict`. `code_change_v1` rejects unknown fields/versions. SHA-256 commitments use explicit typed serialization and exact UTF-8 bytes; no newline normalization. Git SHA-1 remains an object-protocol identity only.
- A context is committed in the frozen task's context hash. It fixes installation, immutable repository ID, original location, base commit/tree, complete source SHA-256, exact planned file scope, verification argv/test inventory and resource bounds. The existing remote write-scope repository/ref/path contract must also match. Runtime requests cannot broaden the task.
- `ControllerBootstrap::coding_coordinator` loads the existing protected key without generating one. `GitHubRemote::pin_coding_source` obtains actual complete source through G1-01's private HTTP reader and current Owner consent, then authenticates it under the existing key. A caller-created `PinnedSource` conveys no trust. Unsupported/incomplete trees fail closed.
- Add, modify and delete regular files; a rename can be deletion plus creation. Modifications/deletions require the exact original SHA-256. Creation cannot overwrite. Complete inventories reject duplicate/case-colliding paths and directory aliases, traversal, encoding tricks, file/directory collisions, symlinks, submodules and sensitive paths. Added files are 100644; existing modes are preserved. Added/modified text must be UTF-8 without binary/control data; untouched bounded binary bytes and deletions retain exact semantics. A heuristic rejects selected credential markers; it does not prove absence of secrets.
- `submit_coding_artifact` produces structural status only. The existing worker `SubmitResult` carries the exact raw artifact SHA-256. The HTTPS v1 opaque upload frame remains unchanged, now permitting bounded coding content; authenticated uploads stage untrusted evidence atomically with the existing receipt transaction, never acceptance; denied messages roll back both. The validator reconstructs the R08 result message and checks its committed digest, receipt, binding and sequence.
- `verify_coding_execution` requires a sealed independent runner adapter. **Production has no such adapter and returns `VerificationUnavailable`.** Private `cfg(test)` fixtures register independently expected synthetic reports; ordinary DTOs cannot install a verifier. Reports bind exact artifact/source/change-set/policy/attempt, execution/environment identities, terminal exit, completed test inventory, output hashes and limits. Execution cannot predate admission or finish in the future. No submitted code or declared command is executed on the controller.
- `accept_coding_result` rechecks bytes, source, report, dependencies, current R06/R08/provider/financial authority and frozen criteria. A narrow internal `accept_task_in` composes the authenticated evidence decision with existing R06 acceptance in **one BEGIN IMMEDIATE transaction**. `task_acceptances` remains the only acceptance ledger. Format-2 coding and format-1 synthetic acceptances stay distinct. The shared R09 walker revalidates both types, including group/dependency evidence. Persisted source/report/decision seals are checked again on every use.
- `GitHubRemote::prepare_coding` obtains accepted changes through the internal R09 bridge, reconstructs all Git blobs/trees/commit and rechecks current Owner paths, installation, permissions and fencing at existing publication boundaries. It accepts no caller-manufactured accepted manifest. Untouched files remain present. Independent attempts get separate generated refs; overlap reporting never combines branches or creates DAG work.
- A prepared snapshot now checks remote ref state **before any repeated object write**. Existing/divergent refs reconcile or block; the final post-lookup authority check remains. Unknown outcomes never authorize blind writes or force pushes. Known prepared/submitting snapshot tests do not establish arbitrary stale-restore detection.

## Persistence and bounds

Forward **0017** adds authenticated source provenance, raw untrusted coding artifacts, sealed verification/decision evidence and bounded redacted rejection records. Existing R09 manifests/acceptances and G1-01 journals are reused. Published 0001–0016 remain unchanged. External GitHub calls stay outside writer transactions. Rejections retain static categories only, up to 16 per active attempt; they confer no authority.

Initial engineering bounds: 256 source/result files, 1 MiB source bytes (also a 2 MiB serialized-source bound), 16 changes, 16 KiB per added/modified file, 64 KiB serialized artifact/changed bytes, 16 path components, 32 scoped paths, 8 verification commands, 64 required tests, 8 KiB combined output and 32 KiB serialized report. Verification limits cannot exceed the task's frozen resources/time. No runner process is launched; SQLite serializes evidence/acceptance writers. These are conservative implementation limits, not production capacity measurements.

## Executable evidence

The offline fixture contains `src/alpha.rs`, `beta.rs`, `shared.rs`, `obsolete.rs` and `tests/alpha_test.rs`, `beta_test.rs`. Real R06 admission, R08 dispatch and interleaved worker messages lead to independently fixture-verified R09 coding acceptances. A modifies/adds/deletes; B modifies a separate file. Real HTTP Git protocol fixtures reconstruct two commits and compare published bytes, unchanged files and refs. C's actual admission is denied before G acceptance; after explicit aggregation C consumes the exact G manifest through normal orchestration. No synthetic accepted row is inserted to make this flow pass.

A genuine loopback rustls/S2-04 transport regression submits coding content above the former 8 KiB bound, rejects sequence 99 without staging bytes, and commits a corrected sequence 4 as untrusted evidence only. This is separate from the A/B acceptance/publication scenario, and does not establish production coding work distribution.

Negative coverage includes fake legacy-policy acceptance, synthetic-to-code escalation, tampered bytes/seals, wrong identity/hash/epoch/scope, untrusted or mismatched reports, impossible chronology, resource bounds, overlapping accepted results, revocation, permission downgrade, concurrent acceptance/publication, SQLite ignored/aborted writes and real busy contention. Nine actual coding SIGKILL points cover acceptance before/uncommitted/after, intent before/after, remote effect before/after and receipt before/after, preserving an accepted sibling and the original holds. Known prepared/submitting snapshots reconcile a separate durable simulated remote database without new writes.

Red/green regressions reproduced impossible verifier chronology, case-folded directory aliases prepared-snapshot object replay and denied-upload artifact pinning before correction. Two independent read-only reviews inspect artifact/evidence authority and SQLite/Git recovery; they are additional scrutiny, not human Owner approval. Exact current counts and CI/CodeQL results belong in the submitted PR.

```sh
cargo test -p rolvya-persistence-sqlite --lib coding --locked -- --nocapture
cargo test -p rolvya-persistence-sqlite --lib github_app --locked -- --nocapture
cargo test -p rolvya-persistence-sqlite --test orchestration --locked
cargo test -p rolvya-persistence-sqlite --test finance --locked
bash .github/scripts/check-migrations.sh
```

## Production gates

Trusted isolated-runner attestation/execution, coding work distribution, real App enrollment/permissions/protections/webhook delivery, operational restore fencing and emergency key recovery remain incomplete. Production coding acceptance and live publication stay disabled. The synthetic verifier reports are deterministic fixture evidence, not actual sandboxed tests or AI-generated work. No arbitrary source was executed on the controller and no live repository was changed.

Dependabot #1 remains inaccessible/unidentified (connector endpoint unsupported; configured CLI Forbidden). Separately, pinned cargo-audit 0.22.2 against RustSec `2295466c4e9df26647fd67d8fa1c705ac2fc7f81` reports unpatched locked `rsa 0.9.10 / RUSTSEC-2023-0071`, exit 1. The Linux all-feature normal/build graph has no RSA path; signing remains ring 0.17.14. No dependency or lockfile version changed, no advisory was suppressed and neither finding is described as resolved.

[G1-03](trusted-coding-work-execution.md) adds authenticated source/work distribution and separately authenticated offline execution. Production isolated-runner establishment and real-App validation remain gated. No next slice, deployment, protected merge, real credential or paid operation is authorized here.
