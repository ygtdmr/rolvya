use super::*;
mod adversarial;
mod crashes;
mod driver;
mod tls;
pub(in crate::controller_keys) use driver::operational_browser_driver;
async fn archived_point(f: &Fixture, class: BackupClass, days: i64) -> (String, String) {
    let current = f.op.backup(class).await.unwrap();
    let destination = FilesystemDestination::open(&f.config.backup_destination).unwrap();
    let key = backup::BackupKey::load(&f.config.backup_key).unwrap();
    let (mut manifest, db) =
        backup::decode(&key, &destination.get(&current.id).await.unwrap()).unwrap();
    manifest.id = random_id().unwrap();
    manifest.created_at = now().unwrap() - days * 86400;
    let metadata = canonical(&manifest).unwrap();
    let mut plain = Zeroizing::new(Vec::new());
    plain.extend((metadata.len() as u32).to_be_bytes());
    plain.extend(&metadata);
    plain.extend(&*db);
    let envelope = backup::seal(&key, backup::BACKUP_DOMAIN, &plain).unwrap();
    destination.put(&manifest.id, &envelope).await.unwrap();
    sqlx::query("INSERT INTO controller_backup_points VALUES(?,?,?,?,?,?)")
        .bind(&manifest.id)
        .bind(class.name())
        .bind(std::str::from_utf8(&metadata).unwrap())
        .bind(hex(&Sha256::digest(&envelope)))
        .bind(destination.identity())
        .bind(manifest.created_at)
        .execute(&f.op.store.pool)
        .await
        .unwrap();
    f.op.event(
        &manifest.id,
        "backup_verified",
        &Sha256::digest(&envelope),
        None,
    )
    .await
    .unwrap();
    (manifest.id, current.id)
}
fn config() -> DeploymentConfig {
    serde_json::from_value(serde_json::json!({
        "version":1,"service_uid":1000,"database":"/var/lib/rolvya/controller.sqlite",
        "custody":"/var/lib/rolvya-keys","scratch":"/var/lib/rolvya-scratch",
        "assets":"/opt/rolvya/web","asset_sha256":"00".repeat(32),"backup_destination":"/srv/rolvya-backups",
        "backup_key":"/var/lib/rolvya-backup-keys/backup.key",
        "identity_verifier_key":"/etc/rolvya/enrollment.pub","identity_verifier_sha256":"00".repeat(32),"canonical_origin":"https://control.example.org",
        "listen":"127.0.0.1:8443","certificate_chain":["/etc/rolvya/tls/leaf.der"],
        "tls_key":"/etc/rolvya/tls/private.der","support_url":null
    })).unwrap()
}
#[test]
fn unsafe_deployment_configuration_is_rejected_without_secret_diagnostics() {
    let mut c = config();
    c.custody = c.assets.join("keys");
    assert_eq!(c.validate(), Err(OperationError::InvalidConfiguration));
}

pub(super) fn checkpoint(stage: &str) {
    if std::env::var("ROLVYA_OPS_TEST_STOP").ok().as_deref() == Some(stage) {
        use std::io::Write;
        println!("ops-barrier:{stage}");
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::park();
        }
    }
}
use ring::signature::KeyPair;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
struct Fixture {
    root: tempfile::TempDir,
    config: DeploymentConfig,
    op: HostOperator,
    signer: ring::signature::Ed25519KeyPair,
}
fn write_private(path: &Path, bytes: &[u8]) {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    f.write_all(bytes).unwrap();
    f.sync_all().unwrap();
}
impl Fixture {
    async fn new() -> Self {
        use std::os::unix::fs::MetadataExt;
        let tmp = std::env::temp_dir();
        let base = if [0, rustix::process::geteuid().as_raw()]
            .contains(&std::fs::metadata(&tmp).unwrap().uid())
        {
            tmp
        } else {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .find(|p| p.join(".git").exists())
                .unwrap()
                .parent()
                .unwrap()
                .to_owned()
        };
        let root = tempfile::tempdir_in(base).unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
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
            let p = root.path().join(name);
            std::fs::create_dir(&p).unwrap();
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let mut c = config();
        c.service_uid = rustix::process::geteuid().as_raw();
        c.database = root.path().join("data/controller.sqlite");
        c.custody = root.path().join("keys");
        c.scratch = root.path().join("scratch");
        c.assets = root.path().join("web");
        c.backup_destination = root.path().join("backups");
        c.backup_key = root.path().join("backup-keys/backup.key");
        c.identity_verifier_key = root.path().join("config/enrollment.pub");
        c.certificate_chain = vec![root.path().join("config/leaf.der")];
        c.tls_key = root.path().join("config/tls.der");
        let rng = ring::rand::SystemRandom::new();
        let secret = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let signer = ring::signature::Ed25519KeyPair::from_pkcs8(secret.as_ref()).unwrap();
        write_private(&c.identity_verifier_key, signer.public_key().as_ref());
        c.identity_verifier_sha256 = hex(&Sha256::digest(signer.public_key().as_ref()));
        HostOperator::generate_protection_secret(&c.backup_key).unwrap();
        let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/web/dist");
        if dist.join("index.html").exists() {
            for e in std::fs::read_dir(dist).unwrap() {
                let e = e.unwrap();
                write_private(
                    &c.assets.join(e.file_name()),
                    &std::fs::read(e.path()).unwrap(),
                );
            }
        }
        if !c.assets.join("index.html").exists() {
            // Rust-only CI has no web build; browser CI supplies the audited production build.
            use base64::Engine;
            let app = b"void 0;";
            let sri = base64::engine::general_purpose::STANDARD.encode(sha2::Sha384::digest(app));
            let html = format!(
                "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src &#39;none&#39;; script-src &#39;self&#39;; style-src-attr &#39;none&#39;; connect-src &#39;self&#39;; object-src &#39;none&#39;\"><script src=\"/app.fixture.js\" integrity=\"sha384-{sri}\"></script>"
            );
            for (name, bytes) in [
                ("index.html", html.as_bytes()),
                ("app.fixture.js", app.as_slice()),
                ("sw.js", b"void 0;"),
                ("manifest.webmanifest", b"{}"),
            ] {
                write_private(&c.assets.join(name), bytes);
            }
        }
        c.asset_sha256 = service::asset_digest(&c.assets).unwrap();
        let op = HostOperator::open(c.clone()).await.unwrap();
        Self {
            root,
            config: c,
            op,
            signer,
        }
    }
    async fn initialize(&self) {
        let r = self.op.prepare_setup().await.unwrap();
        self.op.initialize(&r.id, &r.sha256).await.unwrap();
    }
    async fn enroll(&self) -> super::super::human_auth::HumanId {
        let password =
            super::super::human_auth::Password::new(b"offline-unique-human-password".to_vec())
                .unwrap();
        let spec = EnrollmentSpec {
            email: "owner@example.test".to_owned(),
            methods: vec!["password".to_owned()],
            email_login: false,
        };
        let r = self
            .op
            .prepare_enrollment(&spec, Some(&password))
            .await
            .unwrap();
        let statement = operator::EnrollmentStatement {
            version: 1,
            request_id: r.id,
            installation: r.payload["installation"].as_str().unwrap().to_owned(),
            recipient: spec.email,
            request_sha256: r.sha256,
            identity_evidence_sha256: hex(&Sha256::digest(b"private independent mailbox proof")),
            verified_at: now().unwrap(),
            expires_at: r.expires_at,
        };
        let mut message = b"rolvya/independent-first-human/v1\0".to_vec();
        message.extend(canonical(&statement).unwrap());
        use base64::Engine;
        let signature =
            base64::engine::general_purpose::STANDARD.encode(self.signer.sign(&message).as_ref());
        self.op
            .enroll(
                &SignedEnrollment {
                    statement,
                    signature,
                },
                Some(&password),
            )
            .await
            .unwrap()
    }
    async fn restored_config(&self) -> (tempfile::TempDir, DeploymentConfig) {
        let other = Fixture::new().await;
        let Fixture {
            root,
            mut config,
            op,
            signer: _,
        } = other;
        op.close().await;
        std::fs::remove_file(&config.database).unwrap();
        for extension in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{extension}", config.database.display()));
        }
        std::fs::remove_file(&config.backup_key).unwrap();
        write_private(
            &config.backup_key,
            &files::private_file(&self.config.backup_key, 32).unwrap(),
        );
        config.identity_verifier_sha256 = self.config.identity_verifier_sha256.clone();
        std::fs::remove_file(&config.identity_verifier_key).unwrap();
        write_private(
            &config.identity_verifier_key,
            self.signer.public_key().as_ref(),
        );
        (root, config)
    }
}
#[test]
fn configuration_rejects_unknown_fields_aliases_root_and_overlapping_scopes() {
    let c = config();
    assert!(c.validate().is_ok());
    for path in [
        "relative/path",
        "/",
        "/var//lib/keys",
        "/var/lib/../keys",
        "/var/lib/./keys",
    ] {
        let mut b = c.clone();
        b.custody = path.into();
        assert!(b.validate().is_err());
    }
    let mut b = c.clone();
    b.service_uid = 0;
    assert!(b.validate().is_err());
    let mut v = serde_json::to_value(c).unwrap();
    v["production_enabled"] = serde_json::Value::Bool(true);
    assert!(serde_json::from_value::<DeploymentConfig>(v).is_err());
}
#[tokio::test]
async fn protected_host_setup_is_exact_one_use_and_normal_startup_never_generates() {
    let f = Fixture::new().await;
    assert!(f.op.bootstrap.load_existing().await.is_err());
    let r = f.op.prepare_setup().await.unwrap();
    assert_eq!(
        f.op.initialize(&r.id, &"00".repeat(32)).await,
        Err(OperationError::Unauthorized)
    );
    f.op.initialize(&r.id, &r.sha256).await.unwrap();
    let before = std::fs::read(f.config.custody.join("controller.kek")).unwrap();
    assert!(f.op.initialize(&r.id, &r.sha256).await.is_err());
    assert!(f.op.prepare_setup().await.is_err());
    f.op.close().await;
    let reopened = HostOperator::open(f.config.clone()).await.unwrap();
    reopened.bootstrap.load_existing().await.unwrap();
    assert_eq!(
        before,
        std::fs::read(f.config.custody.join("controller.kek")).unwrap()
    );
    std::fs::remove_file(f.config.custody.join("controller.kek")).unwrap();
    assert!(reopened.bootstrap.load_existing().await.is_err());
    assert!(reopened.prepare_setup().await.is_err());
    reopened.close().await;
}
#[tokio::test]
async fn independent_signed_first_human_enrollment_does_not_create_project_roles() {
    let f = Fixture::new().await;
    f.initialize().await;
    let user = f.enroll().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM human_memberships WHERE user_id=?")
            .bind(user.as_str())
            .fetch_one(&f.op.store.pool)
            .await
            .unwrap(),
        0
    );
    f.op.close().await;
    let reopened = HostOperator::open(f.config.clone()).await.unwrap();
    let proof: String=sqlx::query_scalar("SELECT e.proof FROM controller_operator_evidence e JOIN human_enroll_requests h ON h.request_id=e.request_id WHERE h.user_id=?").bind(user.as_str()).fetch_one(&reopened.store.pool).await.unwrap();
    assert!(proof.contains("identity_evidence_sha256"));
    assert!(proof.contains("signature"));
    assert!(reopened.readiness().await.unwrap().authentication_available);
    assert!(
        !reopened
            .readiness()
            .await
            .unwrap()
            .billable_execution_allowed
    );
    reopened.close().await;
}
#[tokio::test]
async fn encrypted_snapshot_and_separate_key_kit_restore_matching_installation() {
    let f = Fixture::new().await;
    f.initialize().await;
    let user = f.enroll().await;
    let snapshot_start = std::time::Instant::now();
    let b = f.op.backup(BackupClass::Frequent).await.unwrap();
    let snapshot_ms = snapshot_start.elapsed().as_millis();
    assert!(b.local_filesystem_only);
    let recovery = Zeroizing::new([0x91; 32]);
    let kit = f.root.path().join("kit/emergency.rvkit");
    f.op.export_recovery_kit(&kit, &*recovery).await.unwrap();
    let (_target, c) = f.restored_config().await;
    let source = FilesystemDestination::open(&f.config.backup_destination).unwrap();
    let restore_start = std::time::Instant::now();
    let restored = HostOperator::restore_database(&c, &source, &b.id)
        .await
        .unwrap();
    assert!(restored.external_reconciliation_required);
    let op = HostOperator::open(c.clone()).await.unwrap();
    assert!(op.bootstrap.load_existing().await.is_err());
    let destkit = c
        .identity_verifier_key
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("kit/emergency.rvkit");
    write_private(&destkit, &files::private_file(&kit, 4096).unwrap());
    assert!(
        op.recover_controller_key(&destkit, &[0x92; 32])
            .await
            .is_err()
    );
    assert!(!c.custody.join("controller.kek").exists());
    op.recover_controller_key(&destkit, &*recovery)
        .await
        .unwrap();
    assert!(op.bootstrap.load_existing().await.is_ok());
    assert!(
        op.recover_controller_key(&destkit, &*recovery)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT user_id FROM human_users")
            .fetch_one(&op.store.pool)
            .await
            .unwrap(),
        user.as_str()
    );
    let mut tx = op.store.pool.begin().await.unwrap();
    assert!(!external_authority_available(&mut tx).await.unwrap());
    tx.commit().await.unwrap();
    assert!(!op.readiness().await.unwrap().billable_execution_allowed);
    println!(
        "ops recovery: snapshot={}ms restore_and_key={}ms",
        snapshot_ms,
        restore_start.elapsed().as_millis()
    );
    op.close().await;
    f.op.close().await;
}
