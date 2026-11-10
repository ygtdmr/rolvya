---
status: Implemented lock remediation; two scoped exceptions (one Proposed, pending Owner security review)
issue: https://github.com/ygtdmr/rolvya/issues/45
---

# Dependency advisory hygiene (RC-01)

This record covers the full Rust `Cargo.lock` and the frontend npm lock as of 2026-11-10. It replaces the earlier "Dependabot #1 inaccessible/unidentified" statement in [release candidate validation](release-candidate-validation.md#supply-chain). Exceptions are configured in [`.cargo/audit.toml`](../../.cargo/audit.toml); every entry there must be explained here, scoped, and removed when its removal condition is met.

## Summary

| Advisory | Crate | Status | Shipped controller affected? |
| --- | --- | --- | --- |
| GHSA-8ffr-xgwf-xj56 / CVE-2026-18140 (Dependabot #1, high) | `aws-smithy-json` 0.62.4 | **Fixed**: locked 0.62.7 (first patched) | Was in lock; now patched |
| RUSTSEC-2023-0071 (Marvin attack) | `rsa` 0.9.10 | **Reviewed exception**: lock-only | No — absent from every graph |
| RUSTSEC-2026-0459 / CVE-2026-119375 | `libsqlite3-sys` 0.30.1 (bundled SQLite 3.46.0) | **Proposed exception, pending Owner security review**; no patched path exists | Yes, linked; exposure scoped below |
| RUSTSEC-2026-0441 (unmaintained, informational) | `pin-utils` 0.1.1 | Informational warning; not a vulnerability | Not acted on |
| npm audit | `apps/web` lock | 0 vulnerabilities | — |

## Dependabot #1 — `aws-smithy-json`

The alert is identifiable (`GET /repos/ygtdmr/rolvya/dependabot/alerts/1`): GHSA-8ffr-xgwf-xj56, uncontrolled recursion in the unknown-key skip path, vulnerable `<= 0.62.6`, first patched `0.62.7`, manifest `Cargo.lock`, runtime scope. The earlier G1-12B patch was never pushed and is redone here.

`aws-smithy-json 0.62.7` requires `aws-smithy-types ^1.4.9` and `aws-smithy-runtime-api ^1.12.3`, so the exact workspace pins move `aws-smithy-types =1.4.4 → =1.4.9` and `aws-smithy-runtime-api =1.11.4 → =1.12.3`. The resulting minimal lock change is:

- `aws-smithy-json` 0.62.4 → 0.62.7
- `aws-smithy-types` 1.4.4 → 1.4.9
- `aws-smithy-runtime-api` 1.11.4 → 1.12.3
- `aws-smithy-async` 1.2.12 → 1.3.1
- added `aws-smithy-runtime-api-macros` 1.2.0 and `aws-smithy-schema` 0.1.0

No other package changes. The Dependabot alert closes automatically once the fixed lock reaches `main`.

## `rsa` 0.9.10 — RUSTSEC-2023-0071 (reviewed exception)

`rsa` is locked only because SQLx 0.8.6 locks its optional `sqlx-mysql` backend. No workspace feature enables MySQL. Evidence, with the pinned 1.99.1 toolchain and the full workspace:

```sh
for e in normal build dev; do
  cargo tree -q -i rsa --target all --workspace --all-features -e "$e"
done                                                    # prints nothing
cargo tree -q -i sqlx-mysql --target all --workspace --all-features   # prints nothing
```

This covers every target and every feature combination of all workspace members (`--all-features` is a superset), including the shipped `rolvya-controller` and the build-host `rolvya-release` tool. Packaging also independently fails if `rsa` or `sqlx-mysql` enters the SBOM controller closure (`apps/release/src/sbom.rs`). No released SQLx line removes it: SQLx 0.9.0 still declares the optional MySQL backend.

Removal condition: drop the ignore when `rsa` disappears from `Cargo.lock` or `rsa >= 0.10` is locked, or immediately if any `cargo tree` command above prints a path.

## `libsqlite3-sys` 0.30.1 — RUSTSEC-2026-0459 (Proposed exception)

SQLite before 3.54.0 can return uninitialized page-cache memory when a database is opened together with a crafted sibling `-wal` file whose page size differs. The controller links `libsqlite3-sys` with the `bundled` feature (SQLite 3.46.0) through `sqlx-sqlite`.

**No patched path exists today.** The fix is `libsqlite3-sys >= 0.39.0`, but SQLx 0.8.6 requires `libsqlite3-sys ^0.30`, and the newest SQLx release (0.9.0) accepts only `>=0.30.1, <0.38.0`. Overriding the bundled SQLite source would be an unreviewed native SQLite override, which packaging rejects by design. Linking the system SQLite would change the shipped artifact and, on Debian bookworm, select an older version.

Exposure analysis (verified against source, not assumed):

- The advisory requires a **less trusted party to write the database/`-wal` pair** the process opens.
- Controller database directories are opened through `PrivateDirectory` (`crates/persistence-sqlite/src/controller_keys/operations/files.rs`): service-identity custody checks, `O_NOFOLLOW`, and inode identity re-validation. Same-UID/root compromise is explicitly outside this boundary and would already grant full database access.
- Restore (`operations/recovery.rs`) accepts only an XChaCha20-Poly1305-authenticated single database image (`backup::decode`, fixed domain, SHA-256 and size checks) and writes it into an **empty** private target directory, so no foreign `-wal` sibling can exist at open. An off-host attacker cannot forge a backup without the backup key.
- Every non-test SQLite open site (`SqliteConnectOptions` / `PlanStore::open*` in `crates/persistence-sqlite/src/lib.rs`, `operations/operator.rs`, `operations/backup.rs` inventory and `operations/recovery.rs`) targets either the configured controller database or the staged authenticated restore image. No path opens a worker-supplied or otherwise external SQLite file (source search, 2026-11-10).

Residual risk: a local principal able to plant files in the controller's private data directory. That principal already defeats the database confidentiality boundary, so the CVE does not widen it in the current design. This is a reasoning-based conclusion; it is **not** a substitute for the patched library, and any future feature that opens externally supplied SQLite files invalidates it.

Status: **Proposed**. It requires explicit human Owner security review on the RC-01 PR; it is not approved by this document. Removal condition: upgrade as soon as a SQLx release accepts `libsqlite3-sys >= 0.39`, tracked as a follow-up issue.

## `pin-utils` — informational

RUSTSEC-2026-0441 marks `pin-utils` unmaintained. It is informational (no vulnerability) and `cargo audit` reports it as a warning without failing. No action in RC-01.

## Reproducing the audit

```sh
cargo install cargo-audit --locked     # build host only; not a project dependency
cargo audit                            # reads .cargo/audit.toml; expected: 0 vulnerabilities, 1 warning
(cd apps/web && npm audit --audit-level=moderate)   # expected: 0 vulnerabilities
```

CI currently runs `npm audit` but not `cargo audit`. Adding a pinned, checksum-verified `cargo-audit` step to `.github/workflows/ci.yml` is a workflow change that needs separate Owner review and is not part of this PR.
