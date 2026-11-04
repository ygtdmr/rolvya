//! Disposable build-host acceptance of the packaged operator CLI. No service/paid authority.
use crate::{
    files::{Result, Tree, digest},
    manifest,
};
use base64::Engine;
use ring::signature::KeyPair;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

fn private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| "fixture file conflict")?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| "fixture file publication failed".into())
}
struct Fixture {
    root: tempfile::TempDir,
    config: PathBuf,
    value: Value,
}
impl Fixture {
    fn new(dist: &Path, public: &[u8]) -> Result<Self> {
        let root = tempfile::tempdir().map_err(|_| "fixture directory unavailable")?;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700))
            .map_err(|_| "fixture permissions failed")?;
        for name in [
            "data",
            "keys",
            "scratch",
            "web",
            "backups",
            "backup-keys",
            "config",
            "kit",
        ] {
            fs::create_dir(root.path().join(name)).map_err(|_| "fixture layout failed")?;
            fs::set_permissions(root.path().join(name), fs::Permissions::from_mode(0o700))
                .map_err(|_| "fixture permissions failed")?;
        }
        let path = |s: &str| root.path().join(s);
        let mut value: Value = serde_json::from_slice(
            &Tree::open(dist)?.read("config/deployment.example.json", 16384)?,
        )
        .map_err(|_| "invalid configuration example")?;
        for (field, name) in [
            ("database", "data/controller.sqlite"),
            ("custody", "keys"),
            ("scratch", "scratch"),
            ("assets", "web"),
            ("backup_destination", "backups"),
            ("backup_key", "backup-keys/backup.key"),
            ("identity_verifier_key", "config/enrollment.pub"),
            ("tls_key", "config/tls.der"),
        ] {
            value[field] = json!(path(name));
        }
        value["certificate_chain"] = json!([path("config/leaf.der")]);
        value["service_uid"] = json!(rustix::process::geteuid().as_raw());
        value["identity_verifier_sha256"] = json!(digest(public));
        value["canonical_origin"] = json!("https://control.example.invalid:8443");
        value["listen"] = json!("127.0.0.1:8443");
        value["email"] = Value::Null;
        private(&path("config/enrollment.pub"), public)?;
        for entry in Tree::open(dist)?
            .inventory()?
            .iter()
            .filter(|e| e.path.starts_with("web/"))
        {
            private(
                &path(&entry.path),
                &Tree::open(dist)?.read(&entry.path, 4 * 1024 * 1024)?,
            )?;
        }
        Ok(Self {
            config: path("config/deployment.json"),
            value,
            root,
        })
    }
    fn save(&self) -> Result<()> {
        private(
            &self.config,
            &serde_json::to_vec(&self.value).map_err(|_| "fixture config serialization failed")?,
        )
    }
}

#[derive(Serialize)]
struct Statement {
    version: u32,
    request_id: String,
    installation: String,
    recipient: String,
    request_sha256: String,
    identity_evidence_sha256: String,
    verified_at: i64,
    expires_at: i64,
}
struct Runner {
    binary: PathBuf,
    operations: Vec<Value>,
}
impl Runner {
    fn call(
        &mut self,
        label: &str,
        args: &[&str],
        body: &[u8],
        success: bool,
    ) -> Result<Zeroizing<Vec<u8>>> {
        let started = Instant::now();
        let mut child = Command::new(&self.binary)
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "installed fixture process failed")?;
        let mut input = child.stdin.take().ok_or("fixture input unavailable")?;
        if input.write_all(body).is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("installed fixture input failed".into());
        }
        drop(input);
        let mut out = child.stdout.take().ok_or("fixture output unavailable")?;
        let reader = std::thread::spawn(move || {
            let mut bytes = Zeroizing::new(Vec::new());
            out.by_ref()
                .take(65537)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        });
        let mut peak_kib = 0_u64;
        let status = loop {
            if let Ok(status) = fs::read_to_string(format!("/proc/{}/status", child.id())) {
                for line in status.lines().filter(|l| l.starts_with("VmHWM:")) {
                    peak_kib = peak_kib.max(
                        line.split_whitespace()
                            .nth(1)
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0),
                    );
                }
            }
            if let Some(status) = child
                .try_wait()
                .map_err(|_| "fixture process inspection failed")?
            {
                break status;
            }
            if started.elapsed() > Duration::from_secs(60) {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("installed fixture timed out".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let bytes = reader
            .join()
            .map_err(|_| "fixture output failed")?
            .map_err(|_| "fixture output failed")?;
        if bytes.len() > 65536 || !crate::process::expected_exit(status.code(), success) {
            return Err(format!(
                "installed fixture stage {label} failed (diagnostics withheld)"
            ));
        }
        self.operations.push(json!({"stage":label,"expectedSuccess":success,"observedSuccess":status.success(),"elapsedMs":started.elapsed().as_millis(),"sampledPeakRssKiB":peak_kib}));
        Ok(bytes)
    }
    fn json(&mut self, label: &str, args: &[&str], body: &[u8]) -> Result<Value> {
        serde_json::from_slice(&self.call(label, args, body, true)?)
            .map_err(|_| "invalid installed JSON response".into())
    }
}

pub(crate) fn run(dist: &Path, sha: &str) -> Result<Value> {
    let manifest = manifest::verify(dist, sha)?;
    if rustix::process::geteuid().as_raw() == 0 {
        return Err("installed fixture requires a dedicated non-root build-host identity".into());
    }
    let pkcs8 = ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
        .map_err(|_| "fixture issuer unavailable")?;
    let bytes = Zeroizing::new(pkcs8.as_ref().to_vec());
    let issuer = ring::signature::Ed25519KeyPair::from_pkcs8(&bytes)
        .map_err(|_| "fixture issuer unavailable")?;
    let mut original = Fixture::new(dist, issuer.public_key().as_ref())?;
    let mut runner = Runner {
        binary: dist.join("bin/rolvya-controller"),
        operations: Vec::new(),
    };
    let asset = runner.call(
        "packaged-asset-digest",
        &[
            "asset-digest",
            original
                .root
                .path()
                .join("web")
                .to_str()
                .ok_or("invalid fixture path")?,
        ],
        &[],
        true,
    )?;
    original.value["asset_sha256"] = json!(
        std::str::from_utf8(&asset)
            .map_err(|_| "invalid asset response")?
            .trim()
    );
    original.save()?;
    let cfg = original.config.to_str().ok_or("invalid fixture path")?;
    let key = original.root.path().join("backup-keys/backup.key");
    runner.call(
        "create-backup-protection",
        &[
            "generate-secret",
            key.to_str().ok_or("invalid fixture path")?,
        ],
        &[],
        true,
    )?;
    let setup = runner.json("prepare-fresh-installation", &["prepare-setup", cfg], &[])?;
    runner.call(
        "initialize-key-custody",
        &["initialize", cfg],
        &serde_json::to_vec(&json!({"request":setup["id"],"sha256":setup["sha256"]}))
            .map_err(|_| "fixture serialization failed")?,
        true,
    )?;
    runner.call("setup-replay-denied", &["prepare-setup", cfg], &[], false)?;
    let password = Zeroizing::new(String::from("Fixture-only independent enrollment!9382"));
    #[derive(Serialize)]
    struct HumanInput<'a> {
        spec: Value,
        password: &'a str,
    }
    let input=Zeroizing::new(serde_json::to_vec(&HumanInput{spec:json!({"email":"release-fixture@example.invalid","methods":["password"],"email_login":false}),password:&password}).map_err(|_| "fixture serialization failed")?);
    let request = runner.json("prepare-independent-human", &["prepare-human", cfg], &input)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "fixture clock unavailable")?
        .as_secs() as i64;
    let statement = Statement {
        version: 1,
        request_id: request["id"]
            .as_str()
            .ok_or("missing enrollment request")?
            .into(),
        installation: request["payload"]["installation"]
            .as_str()
            .ok_or("missing installation")?
            .into(),
        recipient: "release-fixture@example.invalid".into(),
        request_sha256: request["sha256"]
            .as_str()
            .ok_or("missing request digest")?
            .into(),
        identity_evidence_sha256: digest(
            b"synthetic independent issuer; not a real human identity",
        ),
        verified_at: now,
        expires_at: now + 120,
    };
    let mut message = b"rolvya/independent-first-human/v1\0".to_vec();
    message.extend(
        serde_json::to_vec(&statement).map_err(|_| "fixture signature serialization failed")?,
    );
    let signature =
        base64::engine::general_purpose::STANDARD.encode(issuer.sign(&message).as_ref());
    let proof = original.root.path().join("config/proof.json");
    private(
        &proof,
        &serde_json::to_vec(&json!({"statement":statement,"signature":signature}))
            .map_err(|_| "fixture proof serialization failed")?,
    )?;
    runner.call(
        "independently-signed-synthetic-enrollment",
        &[
            "enroll-human",
            cfg,
            proof.to_str().ok_or("invalid proof path")?,
        ],
        password.as_bytes(),
        true,
    )?;
    runner.call(
        "enrollment-replay-denied",
        &[
            "enroll-human",
            cfg,
            proof.to_str().ok_or("invalid proof path")?,
        ],
        password.as_bytes(),
        false,
    )?;
    let before = runner.json("normal-existing-key-readiness", &["readiness", cfg], &[])?;
    if before["controller_key_loaded"] != true
        || before["authentication_available"] != true
        || before["billable_execution_allowed"] != false
    {
        return Err("installed readiness invariant failed".into());
    }
    let secret = original.root.path().join("kit/recovery.secret");
    runner.call(
        "independent-recovery-secret",
        &[
            "generate-secret",
            secret.to_str().ok_or("invalid secret path")?,
        ],
        &[],
        true,
    )?;
    let recovery =
        Zeroizing::new(fs::read(&secret).map_err(|_| "fixture recovery secret unavailable")?);
    let kit = original.root.path().join("kit/emergency.kit");
    runner.call(
        "encrypted-emergency-kit",
        &["export-kit", cfg, kit.to_str().ok_or("invalid kit path")?],
        &recovery,
        true,
    )?;
    let backup = runner.json(
        "consistent-encrypted-backup",
        &["backup", cfg, "frequent"],
        &[],
    )?;
    let mut restored = Fixture::new(dist, issuer.public_key().as_ref())?;
    restored.value["asset_sha256"] = original.value["asset_sha256"].clone();
    restored.save()?;
    private(
        &restored.root.path().join("backup-keys/backup.key"),
        &Zeroizing::new(fs::read(&key).map_err(|_| "fixture backup secret unavailable")?),
    )?;
    let cfg2 = restored.config.to_str().ok_or("invalid fixture path")?;
    let source = original.root.path().join("backups");
    let id = backup["id"].as_str().ok_or("missing backup receipt")?;
    runner.json(
        "isolated-database-restore",
        &[
            "restore-db",
            cfg2,
            source.to_str().ok_or("invalid backup path")?,
            id,
        ],
        &[],
    )?;
    runner.call(
        "wrong-recovery-secret-denied",
        &["recover-key", cfg2, kit.to_str().ok_or("invalid kit path")?],
        &[0; 32],
        false,
    )?;
    runner.json(
        "matching-key-recovery",
        &["recover-key", cfg2, kit.to_str().ok_or("invalid kit path")?],
        &recovery,
    )?;
    let after = runner.json("restored-existing-key-readiness", &["readiness", cfg2], &[])?;
    if after["external_reconciliation_required"] != true
        || after["controller_key_loaded"] != true
        || after["billable_execution_allowed"] != false
    {
        return Err("restored installation unexpectedly eligible".into());
    }
    manifest::verify(dist, sha)?;
    Ok(
        json!({"version":1,"sourceCommit":manifest.commit,"manifestSha256":sha,"binarySha256":manifest.files.iter().find(|e|e.path=="bin/rolvya-controller").ok_or("missing controller")?.sha256,"evidenceKind":"installed_binary_operator_prefix","operations":runner.operations,"fullInstalledSystemAccepted":false,"offlineReleaseCandidateQualified":false,"productionReleaseQualified":false,"paidExecutionAuthorized":false,"identityEvidence":"private synthetic independently signed issuer; not actual human enrollment certification","limits":"Real CLI/SQLite/custody/Argon verifier enrollment/backup/key recovery. No HTTPS login, Owner binding, worker/provider effects or complete orchestration acceptance is claimed. Peak RSS is sampled per subprocess, not a proven concurrency capacity."}),
    )
}
