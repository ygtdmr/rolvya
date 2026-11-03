# Controller installation and recovery

G1-10 implements `apps/controller` and `controller_keys::operations`. This is a direct-TLS human-panel service, not a paid execution launcher. Real first-human evidence, public PKI, a qualified mail provider, independent off-host storage and external-effect reconciliation remain operational prerequisites. Private TLS/identity/mail fixtures do not qualify those prerequisites.

## Build and install

Build on Linux from a reviewed clean commit using Rust 1.99.1 and the committed npm lockfile:

```sh
scripts/package-controller.sh /absolute/new/distribution
```

The distribution includes the optimized controller, exactly four web files, a nonsecret configuration example, a systemd unit and `SOURCE.json` with source/binary/web commitments. Node and Vite are build tools only. Install under `/opt/rolvya`; provision a dedicated non-root `rolvya` service identity. Review [the unit](../../deploy/controller/rolvya.service) against the actual host. It allows writes only to database, scratch and backup directories; custody/configuration/application files remain read-only during normal service. Core dumps, new privileges, capabilities, device access and namespace creation are restricted. This unit does not qualify AWS isolation.

Create private service-owned directories with mode 0700 for database, scratch, key custody, backup destination, and backup-key custody. Configuration, DER certificate chain/private key, enrollment public key and protection-secret files must be service-owned regular files with mode 0600. Ancestors must satisfy existing custody ownership/write restrictions; no symlink/hardlink adoption. Public assets may be root-owned and read-only. Use separate normalized absolute paths as illustrated in [the configuration](../../deploy/controller/deployment.example.json); replace every example pin and UID. The service requires recognized persistent ext4/XFS/Btrfs for authoritative database/key storage and the configured filesystem backup destination; overlay/tmpfs fixture acceptance is not production qualification.

Pin the complete audited four-file build with `rolvya-controller asset-digest /opt/rolvya/web`. Configure the canonical HTTPS origin and matching unprivileged listener port. Supply leaf/intermediate DER certificates and a matching DER private key. Production preparation verifies hostname, time validity and the public trust chain. No forwarded-header proxy trust is installed. Do not terminate TLS at an arbitrary proxy.

For a reviewed distribution, a host administrator installs the public executable/assets as root-owned read-only files, then provisions service-owned private state separately (replace the distribution path and complete account/configuration review first):

```sh
install -d -o root -g root -m 0755 /opt/rolvya/bin /opt/rolvya/web
install -o root -g root -m 0555 DISTRIBUTION/bin/rolvya-controller /opt/rolvya/bin/
install -o root -g root -m 0444 DISTRIBUTION/web/* /opt/rolvya/web/
install -d -o rolvya -g rolvya -m 0700 /var/lib/rolvya /var/lib/rolvya-scratch /var/lib/rolvya-keys /var/lib/rolvya-backup-keys /srv/rolvya-backups /etc/rolvya
```

Provision the completed configuration and referenced TLS/enrollment files with service ownership and mode 0600 through the protected host channel. Do not copy example placeholders into an enabled service. Install the reviewed unit under systemd only after setup and validation; use `systemctl start/stop rolvya` for normal lifecycle. Ordinary upgrades stop the unit before explicit migration and replacement. A read-only package directory is not itself a configured service.

## Deliberate setup

Run host commands as the dedicated identity through protected administrative access. No setup HTTP route exists. `inspect CONFIG` opens/inspects the host installation; on genuinely empty database/custody directories the host tool may create the initial database. `serve` never creates or upgrades it. `prepare-setup CONFIG` records a ten-minute installation/UID/request-bound approval. Review its nonsecret payload and commitment; deliver the exact `{ "request": "…", "sha256": "…" }` via a restricted input descriptor to `initialize CONFIG`. No key is printed. Repeated, ambiguous or interrupted setup must be investigated; never delete evidence to force fresh eligibility.

Normal `serve CONFIG` only loads the existing current schema and matching key. Missing keys are errors, not regeneration instructions. Before a reviewed upgrade, stop the service, verify backups and run explicit `migrate CONFIG`; unknown versions/checksums fail. The next schema is 0023; published 0001–0022 remain unchanged.

## First human and initial project binding

Before first-human enrollment, create a verified pre-enrollment backup and separate kit. An interrupted pending enrollment is not reset automatically; use the protected isolated restore procedure with that point and a fresh independently signed request.

Host administration is not human identity or project ownership. Provision an independently controlled Ed25519 enrollment verifier public key and its SHA-256 pin through the protected host boundary. The controller includes no production signing service or self-attestation switch. A real issuer must independently establish the intended identity/mailbox and retain the evidence; an email string, operator UID or fixture signature cannot establish that fact.

`prepare-human CONFIG` accepts bounded strict JSON through a restricted descriptor: `spec` contains email, allowed primary methods and email-login policy; `password` is included only in this private input. The recorded request contains a keyed password/policy commitment, never a password. Password-only operation is supported after legitimate enrollment. Do not put these inputs in shell history, argv or tracked files.

The independent issuer signs the exact serialized `EnrollmentStatement` with Ed25519 over `rolvya/independent-first-human/v1` followed by a NUL byte and the Rust canonical JSON bytes. The statement binds version 1, request ID/commitment, installation, recipient, evidence SHA-256, verification time and expiry (at most ten minutes). `SignedEnrollment` contains this statement and a standard-base64 signature. `enroll-human CONFIG PROOF_FILE` requires the same password on a restricted stdin descriptor. A private proof file and pinned external verification key are mandatory. The canonical signed statement, verifier public key and domain-separated digest are retained as immutable host evidence; both existing enrollment transactions require that exact receipt. No project role is created by enrollment; voluntary TOTP and existing primary-proof rules remain unchanged. Actual issuer qualification is still required for deployment.

For an existing project, `prepare-binding CONFIG` reads strict `{project,human,legacy}` input through a protected descriptor. It verifies the exact existing actor and active human and records a short-lived keyed approval. The intended human must then authenticate normally and affirm the approval through the same-origin CSRF-protected `POST /api/control/v1/auth/project-binding` with `{approval,confirm:true}`. This endpoint accepts no caller-selected project/human/role. It invokes existing `bind_existing_project` authority, preserving conflict and last-Owner protection. There is no public ownership claim, bulk adoption or budget change. Current browser enrollment/binding guidance is operator-assisted; no general administration console is exposed.

## Service, mail and readiness

`serve CONFIG` starts the existing rustls/Axum human panel and bounded backup scheduler, and drains on SIGTERM/SIGINT. Database pool size remains four. Public `/health/live` reports process liveness; `/health/ready` reports a bounded database-serving check only. Host-only `readiness CONFIG` distinguishes schema/key/auth, persistent storage, recent verified backup, restore completion and external reconciliation. Its pre-listener `browser_https_ready` is false. Billable execution is always false in this executable; no Boolean config flag enables workers/providers.

The implicit-TLS SMTP adapter implements fixed-host authentication, bounded protocol handling and one-shot dispatch through existing HumanAuth challenges. SMTP 250 acceptance is not delivery. A separately controlled Ed25519 delivery signer must bind operation, keyed message commitment, provider, installation and chronology; durable replay checks precede redemption. No real provider qualification is installed, so production email login/recovery remains unavailable. Do not claim mail was sent or offer a usable reset link when unavailable. A support URL is displayed only when actually configured. The test provider exercises SMTP TLS and signed delivery without external messages.

## Backups and retention

`generate-secret NEW_PRIVATE_PATH` publishes 32 OS-random bytes once, without printing them. Use distinct backup and emergency recovery secrets with separate custody. Normal startup never asks for the recovery secret.

`backup CONFIG frequent` and `backup CONFIG daily` use the existing consistent SQLite snapshot, integrity/foreign-key/schema verification and complete bounded table inventory. The service checks scheduling every minute for 900-second frequent and 86400-second daily intervals; this is scheduling resolution, not an approved RPO guarantee. Snapshot/envelope size is capped at 256 MiB. A larger installation needs a separately reviewed streaming/storage extension, not truncation.

Backup envelopes use existing XChaCha20-Poly1305 with a distinct 256-bit backup key, random 192-bit nonce and version/domain-separated authenticated content. The encrypted payload contains the manifest and database. Destination readback, authenticated decryption and SHA-256 verification precede the durable successful receipt. The filesystem destination uses exclusive staged publication and fsync; this is not independently protected off-host storage. No external object provider or upload is claimed. Internal artifacts reside in SQLite; external Git/provider resources are represented by identity records, not backed-up remote content.

`retention CONFIG` retains frequent points for seven days and daily points for thirty days, preserving at least one verified usable point in each class. It verifies the replacement before deleting an older point. Cleanup intent precedes unlink, final receipt follows directory fsync; restart may complete this exact recorded cleanup. Corruption blocks deletion. Filesystem publication and SQLite commit are separate boundaries, not a distributed atomic transaction.

## Separate emergency kit and isolated restore

`export-kit CONFIG NEW_KIT_PATH` reads exactly 32 high-entropy recovery-secret bytes from a restricted descriptor. It encrypts the existing controller key and installation/key-version metadata using XChaCha20-Poly1305 with an independent domain and nonce. It rejects reuse of the backup key or controller key. This implementation deliberately uses an OS-generated secret, not a human password; no password-derived KDF is offered. Store the encrypted kit off-host apart from ordinary backups, and the recovery secret separately again. Path separation alone does not prove independent failure domains.

On a clean isolated destination, provision protected directories, compatible configuration and the matching backup key. With the service stopped:

```sh
rolvya-controller restore-db CONFIG SOURCE_DIRECTORY BACKUP_ID
rolvya-controller recover-key CONFIG KIT_PATH < RECOVERY_SECRET_FILE
rolvya-controller readiness CONFIG
```

`restore-db` authenticates/decrypts the backup, verifies schema/digest/table inventory, stages it and commits a permanent restore/reconciliation hold before publishing the database. It refuses an active destination or partial evidence. `recover-key` authenticates the separately encrypted kit against that database, refuses healthy-key overwrite, verifies encrypted Broker records, and durably completes recovery. Interrupted matching publication can resume; unknown/corrupt files are not automatically adopted.

Old sessions, primary proofs, pending host approvals and recovery codes are revoked/restricted; active voluntary TOTP remains enrolled with a replay floor. Legitimate users authenticate afresh. Original memberships, frozen DAG, accepted artifacts, approvals, held/settled/unknown financial records and Git identities remain. Every authorized project role receives a historical-state notice in all five sections. The hold blocks admission, worker/model/provider authority and remote Git writes without releasing money or pretending old external tasks stopped. There is intentionally no resume/unlock command in this milestone. Externally reconciled, independently authorized resumption is the next operational dependency.

Keep multiple compatible recovery points and verify a kit before relying on it. Current controller key version 1 is preserved; this slice adds no general rotation. A host compromise can expose loaded keys/plaintext memory; zeroization and backup encryption do not protect against a fully compromised controller. Backup-key compromise exposes backup contents and permits forged backup envelopes; a matching kit verifies key identity, not externally current ledger history. Recovery-secret compromise with its kit exposes the controller key. Joint custody compromise remains outside these protections.

## Evidence and release gates

Tests use actual SQLite, protected files, Argon2id, rustls HTTPS, subprocess SIGKILL/reopen, signed private identity/mail fixtures and production web assets in browser CI. Process-death tests do not establish physical power-loss guarantees. Runtime measurements and exact-head CI are recorded in the PR, including the distinction between unique tests and nested crash cases.

Public domain/PKI, independently verified first-human issuer, qualified mail receipts, authorized off-host destination, real restore reconciliation, effective AWS isolation, provider entitlements and external spending boundaries remain unqualified. Dependency alert #1 remains inaccessible and separate from the full-lock rsa/RustSec advisory. Routine merge smoke requires Owner-controlled re-verification under the [existing procedure](../development/pull-request-integration.md); this work changes no protections or smoke receipts. Final PWA icons and physical-device/screen-reader certification are still release gates.
