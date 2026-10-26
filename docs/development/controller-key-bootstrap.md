# S2-01 controller key bootstrap

Implemented as a Linux Rust library in [controller_keys](../../crates/persistence-sqlite/src/controller_keys.rs), with [descriptor-based custody](../../crates/persistence-sqlite/src/controller_keys/custody.rs) and forward [migration 0006](../../crates/persistence-sqlite/migrations/0006_controller_key_bootstrap.sql). Base: merged PR #21, main `c1227c15e6eb7f631a0bf08a3b53d63b1a01c2d9`. Published migrations 0001–0005 are unchanged. This implements key bootstrap, not encrypted secret records or production authentication.

Authority: [ADRs 0037](../decisions/0037-central-secret-broker-task-scoped-access.md), [0044](../decisions/0044-encrypted-sqlite-secret-storage.md), [0045](../decisions/0045-automatic-local-master-key-unlock.md), [0046](../decisions/0046-off-host-encrypted-master-key-recovery.md) and [0047](../decisions/0047-secret-broker-xchacha20-key-hierarchy.md). The [core handoff](core-verification-handoff.md) remains the historical R11 verification record.

## Interfaces and deployment boundary

`ControllerBootstrap::open` binds an existing `PlanStore`, absolute normalized key directory, expected `ServiceIdentity` and explicit deployment exclusion roots. `state` reports freshness, initialization in progress, initialized, interrupted or recovery-required. `initialize` requires an `InitializationRequest` and a `LocalSetupAuthorization` implementation. `load_existing` is the separate ordinary restart operation and has no entropy-generation path. `ControllerKey` exposes only nonsecret `KeyMetadata`; it has no serialization, clone or public key-byte accessor. Temporary secret buffers and the held KEK use `Zeroizing`.

The authorization trait is a trusted, in-process controller-host/offline administration boundary, with no permissive implementation. It must not be implemented using worker-supplied identity or wired to an unauthenticated request. Tests explicitly use synthetic policies. This milestone does not implement an executable setup CLI, human authentication, Owner step-up or independent human authorization. Directory access alone is not Owner authentication. S2-03 must establish the real human authority boundary.

The future controller service must provision **separate, persistent host directories**: for example `/var/lib/rolvya` for SQLite and `/var/lib/rolvya-keys` for custody. Both must be owned by the same dedicated non-root service UID and mode 0700. The expected UID must equal the effective UID. Initial setup requires a database exclusively created through `PlanStore::open` at an absolute normalized path in that protected directory. Relative/aliased paths, precreated files and upgrades do not establish freshness. Opening a database never generates a key.

Supply every ordinary backup/export, web, worker, image/bundle and source root as a deployment exclusion; these roots must exist and resolve safely. The actual database directory and Git repository ancestry are independently excluded. This is an explicit trusted host-layout contract: the library cannot discover every unrelated backup agent or export configuration. Do not include custody in an image, repository, ordinary backup, environment variable, argument, configuration export or model context. Mount custody separately from container images. Never use a worker workspace. Linux local ext-family, XFS, Btrfs, overlay and tmpfs types are recognized; network/FUSE/unknown types fail closed. Tmpfs/overlay support enables disposable tests and does not certify durable deployment storage. Production needs persistent host storage, service hardening, disabled secret-bearing core dumps and separately tested recovery.

## Format, state and durability

The fixed 84-byte version-1 file contains an 8-byte magic, 4-byte format, 16-byte installation ID, 16-byte key ID, 8-byte key version and exactly 32 bytes of KEK. KEK and identifiers come from OS `getrandom`; no fallback exists. SQLite holds identifiers, versions and a domain-separated HMAC-SHA256 installation verifier. Verification uses the vetted HMAC implementation's constant-time tag check. No KEK or reversible KEK is stored in SQLite/WAL/snapshots. Four newly direct dependencies reuse already locked versions: getrandom 0.3.4, rustix 1.1.5, hmac 0.12.1 and zeroize 1.9.1; no package version changes.

1. Only the retained exclusive database creator, with a verified protected parent and matching file descriptor/inode, may record fresh eligibility. The nonsecret device/inode binding prevents a copied pre-setup snapshot from becoming a fresh installation. Freshness also requires no existing projects. A crash before this eligibility is recorded leaves recovery required.
2. Explicit setup authorizes before mutation, acquires a nonblocking cross-process `flock` on a separately opened directory descriptor and commits a SQLite `pending` record with `BEGIN IMMEDIATE` **before entropy generation**. Independent calls on one object also use separate file descriptions.
3. Create the pending file exclusively with mode 0600, write bounded contents, synchronize the file, publish with `renameat2(RENAME_NOREPLACE)` and synchronize the directory. Never overwrite or delete an existing key or pending file. Descriptor-relative `O_NOFOLLOW` traversal, ownership/mode/type/link checks and inode revalidation reject unsafe replacements. Ancestors must be trusted non-writable directories, except root-owned sticky parents protecting a service-owned child.
4. Recheck local setup authorization, commit the matching immutable ready metadata and verifier in a short SQLite transaction, then load and verify the actual published file before acknowledgment. Filesystem publication and SQLite commit are deliberately separate; no universal atomicity is claimed.

| Observed state | Required behavior |
| --- | --- |
| Verified fresh database, no key/pending file | Ordinary load returns `NotInitialized`; only explicitly authorized setup may generate. |
| Another setup owns the lock | Bounded `InitializationInProgress`; state inspection reports `Initializing`. |
| Pending record, temporary file or published key without matching ready record | Interrupted/recovery required; never adopt, delete or regenerate automatically. |
| Ready record and matching protected file | Automatically load the original identity on every restart; no manual unlock. |
| Ready record with absent, corrupt, wrong-version, copied or inaccessible key | Return a structured error and no key handle. Repeated initialization cannot replace it. |
| Preexisting/populated/unverified database or copied fresh snapshot | Recovery required, with application data preserved. |

A ready SQLite snapshot can load with its matching protected key despite a different database inode. An incorrect installation key fails HMAC/identity verification. This is not rollback detection, external-effect reconciliation or authorization to activate a stale snapshot. In-place hostile rollback by the service UID/root and full host compromise are outside this local-custody boundary.

There is no reset, cleanup/adoption, rotation, emergency restore or alternate-key fallback API. Preserve failed state and use a future deliberately authorized reconciliation/recovery procedure. Losing a key is an incident. Off-host encrypted recovery remains unimplemented; do not introduce live credentials until recovery and the remaining security gates are implemented and verified.

## Executable evidence

[Focused tests](../../crates/persistence-sqlite/src/controller_keys/tests.rs) and the [public integration test](../../crates/persistence-sqlite/tests/controller_keys.rs) exercise real disposable files and SQLite. Compile-fail doctests deny serialization and private key access. No key material is checked in or printed. Test-only hooks and entropy failure injection compile only under `cfg(test)`.

The subprocess suite kills actual setup processes at twelve checkpoints: before setup, after exclusive ownership, before randomness, after generation, after temporary creation, after writing, after file sync, before publication, after publication/directory sync, before SQLite commit, after commit and before acknowledgment. Reopening checks integrity and either preserves fresh eligibility without any generated material, refuses interrupted state, or loads the committed original key. A separate two-process contention test proves exclusive publication. These are process-crash tests, not physical power-loss/storage-hardware guarantees.

Other cases cover denied/revoked local policy; entropy failure; duplicate setup; same-object contention; missing/corrupt/truncated/oversized files; wrong identities/versions; symlink/hardlink/FIFO/directory paths; file/directory replacement and disappearance; ownership and permissions; path aliases/exclusions; immutable ready metadata; preexisting/populated/copied databases; actual v5 upgrade and failed v6 transaction rollback; read/write interruption; pending recovery after failed state commit; and KEK absence from SQLite/WAL/ordinary snapshot. Wrong file ownership is safely tested by modifying disposable `fstat` metadata, without changing host ownership; character-device metadata is checked against read-only `/dev/null`, without creating a device. Network filesystem rejection checks the type policy synthetically; no remote mount was provisioned.

Fresh local results: **318 workspace Rust tests passed** (including 43 bootstrap unit tests, one public integration test and two compile-fail doctests); eight migration checks, 203 controller tests and 23 Python checks passed. The 43-unit count includes the subprocess driver; twelve crash checkpoints are cases within one behavioral test, not twelve extra test functions. Full CI repeats tests and migration replay without inflating unique counts. Verification commands and final-head CI results are recorded in the PR:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo check --workspace --all-targets --all-features --locked
cargo test -p rolvya-persistence-sqlite --lib controller_keys --locked
cargo test -p rolvya-persistence-sqlite --test controller_keys --locked
python3 .github/scripts/ci.py rust
python3 .github/scripts/ci.py inventory
python3 .github/scripts/ci.py repository
node --test --test-isolation=none .github/scripts/integrate.test.cjs
python3 -m unittest discover -s .github/scripts -p '*_test.py'
bash .github/scripts/check-migrations.sh
```

Pinned actionlint and Gitleaks are also run. Cargo vulnerability auditing is not claimed when cargo-audit is absent. Existing R01–R11 synthetic execution does not require secrets and remains intact; this library grants no billable/provider authority. Future broker operations must require successful key loading. Production authenticated service wiring, memory/host hardening, physical durability, real secret encryption, rotation and recovery remain unverified. No startup SLO or production memory guarantee is inferred from tests.

**Next: S2-02**, scoped authenticated XChaCha20-Poly1305 SQLite Secret Broker records, independently wrapped per-record DEKs and a narrow crypto-use interface inside the opaque key boundary. Keep real Owner authentication, worker identity and separately controlled recovery gates explicit. Zero live credentials, providers or external spending were used here. Manual Owner review is required; this work supplies no human approval.
