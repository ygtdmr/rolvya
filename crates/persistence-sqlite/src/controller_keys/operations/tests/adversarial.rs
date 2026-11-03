use super::super::backup::{BackupKey, decode, inventory};

#[tokio::test]
async fn retention_keeps_sole_usable_points_and_distinguishes_frequent_from_daily() {
    let f = Fixture::new().await;
    f.initialize().await;
    let (old, new) = archived_point(&f, BackupClass::Frequent, 8).await;
    let (daily, _) = archived_point(&f, BackupClass::Daily, 8).await;
    let current = f.config.backup_destination.join(format!("{new}.rvbackup"));
    let saved = std::fs::read(&current).unwrap();
    std::fs::write(&current, b"damaged newest point").unwrap();
    assert!(f.op.retain_backups().await.is_err());
    assert!(
        f.config
            .backup_destination
            .join(format!("{old}.rvbackup"))
            .exists()
    );
    std::fs::write(&current, &saved).unwrap();
    assert_eq!(f.op.retain_backups().await.unwrap(), 1);
    assert!(
        !f.config
            .backup_destination
            .join(format!("{old}.rvbackup"))
            .exists()
    );
    assert!(current.exists());
    assert!(
        f.config
            .backup_destination
            .join(format!("{daily}.rvbackup"))
            .exists()
    );
    assert_eq!(f.op.retain_backups().await.unwrap(), 0);
    f.op.close().await;
}
use super::*;
#[tokio::test]
async fn restore_invalidates_pre_restore_host_binding_approval_even_within_its_expiry() {
    let f = Fixture::new().await;
    f.initialize().await;
    let human = f.enroll().await;
    let project = crate::ProjectId::new("legacy-project").unwrap();
    f.op.store
        .create_project(&project, &crate::PrincipalId::new("legacy-owner").unwrap())
        .await
        .unwrap();
    let request =
        f.op.prepare_binding(&project, &human, "legacy-owner")
            .await
            .unwrap();
    let key = f.op.bootstrap.load_existing().await.unwrap();
    sqlx::query("INSERT INTO controller_restore_holds VALUES(?,?,?,?)")
        .bind(random_id().unwrap())
        .bind(random_id().unwrap())
        .bind(random_id().unwrap())
        .bind(now().unwrap())
        .execute(&f.op.store.pool)
        .await
        .unwrap();
    assert!(
        operator::binding(&f.op.store, &key, &request.id)
            .await
            .is_err(),
        "restoring a database must not revive a host approval"
    );
    let fresh =
        f.op.prepare_binding(&project, &human, "legacy-owner")
            .await
            .unwrap();
    assert!(
        operator::binding(&f.op.store, &key, &fresh.id)
            .await
            .is_ok()
    );
    f.op.close().await;
}

#[tokio::test]
async fn interrupted_restore_staging_blocks_creation_of_another_fresh_installation() {
    let f = Fixture::new().await;
    f.op.close().await;
    std::fs::remove_file(&f.config.database).unwrap();
    write_private(
        &f.config.database.with_file_name("restore.pending.sqlite"),
        b"retained restore evidence",
    );
    assert!(
        HostOperator::open(f.config.clone()).await.is_err(),
        "partial restore must not be replaced by a new installation"
    );
    assert!(!f.config.database.exists());
}

#[tokio::test]
async fn host_paths_reject_symlinks_insecure_modes_and_substituted_directories() {
    let f = Fixture::new().await;
    let secret = f.config.backup_key.clone();
    let renamed = secret.with_file_name("moved.key");
    std::fs::rename(&secret, &renamed).unwrap();
    std::os::unix::fs::symlink(&renamed, &secret).unwrap();
    assert!(BackupKey::load(&secret).is_err());
    std::fs::remove_file(&secret).unwrap();
    std::fs::rename(&renamed, &secret).unwrap();
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(BackupKey::load(&secret).is_err());
    std::fs::set_permissions(&f.config.custody, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(f.op.prepare_setup().await.is_err());
    f.op.close().await;
}
#[tokio::test]
async fn first_enrollment_rejects_unsigned_wrong_scope_expired_and_password_substitution() {
    use crate::controller_keys::human_auth::Password;
    use base64::Engine;
    let f = Fixture::new().await;
    f.initialize().await;
    let p = Password::new(b"offline-unique-human-password".to_vec()).unwrap();
    let wrong = Password::new(b"different-human-password".to_vec()).unwrap();
    let spec = EnrollmentSpec {
        email: "owner@example.test".into(),
        methods: vec!["password".into()],
        email_login: false,
    };
    let r = f.op.prepare_enrollment(&spec, Some(&p)).await.unwrap();
    let original = operator::EnrollmentStatement {
        version: 1,
        request_id: r.id,
        installation: r.payload["installation"].as_str().unwrap().to_owned(),
        recipient: spec.email,
        request_sha256: r.sha256,
        identity_evidence_sha256: "a1".repeat(32),
        verified_at: now().unwrap(),
        expires_at: r.expires_at,
    };
    let signature = |s: &operator::EnrollmentStatement| {
        let mut b = b"rolvya/independent-first-human/v1\0".to_vec();
        b.extend(canonical(s).unwrap());
        base64::engine::general_purpose::STANDARD.encode(f.signer.sign(&b).as_ref())
    };
    let signed = SignedEnrollment {
        statement: original.clone(),
        signature: signature(&original),
    };
    assert!(f.op.enroll(&signed, Some(&wrong)).await.is_err());
    for kind in 0..6 {
        let mut s = original.clone();
        match kind {
            0 => s.installation = "00".repeat(16),
            1 => s.recipient = "attacker@example.test".into(),
            2 => s.expires_at = 0,
            3 => s.request_sha256 = "00".repeat(32),
            4 => s.version = 2,
            _ => {}
        };
        let proof = SignedEnrollment {
            signature: if kind == 5 {
                base64::engine::general_purpose::STANDARD.encode([0; 64])
            } else {
                signature(&s)
            },
            statement: s,
        };
        assert!(f.op.enroll(&proof, Some(&p)).await.is_err());
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM human_users")
            .fetch_one(&f.op.store.pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("CREATE TRIGGER fault_evidence BEFORE INSERT ON controller_operator_evidence BEGIN SELECT RAISE(IGNORE); END").execute(&f.op.store.pool).await.unwrap();
    assert!(f.op.enroll(&signed, Some(&p)).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM human_users")
            .fetch_one(&f.op.store.pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query("DROP TRIGGER fault_evidence")
        .execute(&f.op.store.pool)
        .await
        .unwrap();
    f.op.enroll(&signed, Some(&p)).await.unwrap();
    assert!(f.op.enroll(&signed, Some(&p)).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM human_users")
            .fetch_one(&f.op.store.pool)
            .await
            .unwrap(),
        1
    );
    f.op.close().await;
}
#[tokio::test]
async fn ignored_host_intent_and_backup_receipt_writes_never_report_success() {
    let f = Fixture::new().await;
    sqlx::query("CREATE TRIGGER fault_host BEFORE INSERT ON controller_operator_requests BEGIN SELECT RAISE(IGNORE); END").execute(&f.op.store.pool).await.unwrap();
    assert!(f.op.prepare_setup().await.is_err());
    sqlx::query("DROP TRIGGER fault_host")
        .execute(&f.op.store.pool)
        .await
        .unwrap();
    f.initialize().await;
    let before = f.op.backup(BackupClass::Frequent).await.unwrap();
    sqlx::query("CREATE TRIGGER fault_backup BEFORE INSERT ON controller_backup_points BEGIN SELECT RAISE(IGNORE); END").execute(&f.op.store.pool).await.unwrap();
    assert!(f.op.backup(BackupClass::Frequent).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM controller_backup_points")
            .fetch_one(&f.op.store.pool)
            .await
            .unwrap(),
        1
    );
    assert!(
        f.config
            .backup_destination
            .join(format!("{}.rvbackup", before.id))
            .exists()
    );
    f.op.store.check_integrity().await.unwrap();
    f.op.close().await;
}
#[tokio::test]
async fn corrupt_backups_wrong_keys_and_incomplete_destinations_cannot_restore() {
    let f = Fixture::new().await;
    f.initialize().await;
    let b = f.op.backup(BackupClass::Frequent).await.unwrap();
    let destination = FilesystemDestination::open(&f.config.backup_destination).unwrap();
    let bytes = destination.get(&b.id).await.unwrap();
    assert!(decode(&BackupKey::from_bytes(&[0x31; 32]).unwrap(), &bytes).is_err());
    let key = BackupKey::load(&f.config.backup_key).unwrap();
    for n in [0, 32, bytes.len() - 1] {
        assert!(decode(&key, &bytes[..n]).is_err());
    }
    let mut changed = bytes.to_vec();
    let last = changed.len() - 1;
    changed[last] ^= 1;
    assert!(decode(&key, &changed).is_err());
    let (_root, c) = f.restored_config().await;
    write_private(&c.database, b"already active");
    assert!(
        HostOperator::restore_database(&c, &destination, &b.id)
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(&c.database).unwrap(), b"already active");
    f.op.close().await;
}
#[tokio::test]
async fn recovery_kit_cannot_share_backup_secret_or_be_stored_with_database() {
    let f = Fixture::new().await;
    f.initialize().await;
    let backup = files::private_file(&f.config.backup_key, 32).unwrap();
    assert!(
        f.op.export_recovery_kit(&f.root.path().join("kit/emergency.rvkit"), &backup)
            .await
            .is_err()
    );
    assert!(
        f.op.export_recovery_kit(
            &f.config.database.with_file_name("emergency.rvkit"),
            &[0x18; 32]
        )
        .await
        .is_err()
    );
    assert!(
        f.op.export_recovery_kit(
            &f.root.path().join("kit/emergency.rvkit"),
            b"weak recovery password"
        )
        .await
        .is_err()
    );
    f.op.close().await;
}
#[tokio::test]
async fn interrupted_filesystem_publication_is_not_overwritten_or_automatically_adopted() {
    let f = Fixture::new().await;
    let d = files::PrivateDirectory::open(&f.config.scratch).unwrap();
    write_private(&f.config.scratch.join(".point.pending"), b"incomplete");
    assert!(d.publish("point", b"replacement").is_err());
    assert_eq!(
        std::fs::read(f.config.scratch.join(".point.pending")).unwrap(),
        b"incomplete"
    );
    assert!(!f.config.scratch.join("point").exists());
    f.op.close().await;
}
#[tokio::test]
async fn consistent_backup_survives_actual_writer_contention_and_concurrent_attempts() {
    let f = Fixture::new().await;
    f.initialize().await;
    let mut writer = f.op.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    // A real competing writer blocks the durable intent. Nothing is reported verified early.
    let other = HostOperator::open(f.config.clone()).await.unwrap();
    let task = tokio::spawn(async move {
        let r = other.backup(BackupClass::Frequent).await;
        other.close().await;
        r
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(!task.is_finished());
    sqlx::query("INSERT INTO controller_operation_events(request_id,kind,commitment,recorded_at) VALUES('contention','backup_prepared',zeroblob(32),1)").execute(&mut *writer).await.unwrap();
    writer.commit().await.unwrap();
    let receipt = task.await.unwrap().unwrap();
    let d = FilesystemDestination::open(&f.config.backup_destination).unwrap();
    let bytes = d.get(&receipt.id).await.unwrap();
    let (m, _) = decode(&BackupKey::load(&f.config.backup_key).unwrap(), &bytes).unwrap();
    assert!(
        m.tables
            .iter()
            .any(|t| t.name == "controller_operation_events" && t.rows >= 4)
    );
    let (a, b) = tokio::join!(
        f.op.backup(BackupClass::Frequent),
        f.op.backup(BackupClass::Frequent)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    f.op.close().await;
}
#[tokio::test]
async fn recovery_preserves_real_r09_acceptance_git_identity_frozen_plan_and_financial_holds() {
    let (_seed, g) = crate::controller_keys::github_app::operational_browser_seed(false).await;
    let f = Fixture::new().await;
    f.op.close().await;
    let mut config = f.config.clone();
    config.database = sqlx::query("PRAGMA database_list")
        .fetch_all(&g.store.pool)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.get::<String, _>("name") == "main")
        .unwrap()
        .get::<String, _>("file")
        .into();
    config.custody = g.root.path().join("keys");
    let op = HostOperator::open(config.clone()).await.unwrap();
    let finance = g.store.financial_snapshot(&g.project).await.unwrap();
    let fingerprint = g.co.fingerprint().to_owned();
    let before = inventory(&config.database).await.unwrap().1;
    let backup = op.backup(BackupClass::Daily).await.unwrap();
    let kit = f.root.path().join("kit/full.rvkit");
    op.export_recovery_kit(&kit, &[0x84; 32]).await.unwrap();
    let (_target, target) = f.restored_config().await;
    let destination = FilesystemDestination::open(&config.backup_destination).unwrap();
    HostOperator::restore_database(&target, &destination, &backup.id)
        .await
        .unwrap();
    let restored = HostOperator::open(target.clone()).await.unwrap();
    let kit_target = target
        .identity_verifier_key
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("kit/full.rvkit");
    write_private(&kit_target, &files::private_file(&kit, 4096).unwrap());
    restored
        .recover_controller_key(&kit_target, &[0x84; 32])
        .await
        .unwrap();
    assert_eq!(
        restored.store.financial_snapshot(&g.project).await.unwrap(),
        finance
    );
    let after = inventory(&target.database).await.unwrap().1;
    for name in [
        "plan_versions",
        "tasks",
        "task_attempts",
        "task_acceptances",
        "coding_artifacts",
        "coding_decisions",
        "financial_positions",
        "financial_events",
        "human_memberships",
        "git_publications",
        "git_publication_events",
    ] {
        let a = before.iter().find(|x| x.name == name);
        let b = after.iter().find(|x| x.name == name);
        assert!(a == b, "retained table changed: {name}");
    }
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT fingerprint FROM plan_versions WHERE state='active'"
        )
        .fetch_one(&restored.store.pool)
        .await
        .unwrap(),
        fingerprint
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_acceptances")
            .fetch_one(&restored.store.pool)
            .await
            .unwrap(),
        1
    );
    assert!(
        !restored
            .readiness()
            .await
            .unwrap()
            .billable_execution_allowed
    );
    use crate::orchestration::SyntheticExecutionAuthority;
    let active = restored
        .store
        .load_active(
            &g.project,
            &g.policy,
            rolvya_plan_validator::Limits::default(),
        )
        .await
        .unwrap();
    let task = active
        .content
        .tasks
        .iter()
        .find(|t| t.execution.is_some())
        .unwrap();
    let assignment = g
        .policy
        .assignment(
            task,
            1,
            crate::admission::Clock::now(g.clock.as_ref()).unwrap(),
        )
        .unwrap();
    let request = crate::admission::AdmissionRequest {
        task: rolvya_domain::plan::TaskRef {
            plan: task.plan.clone(),
            task_id: task.id.clone(),
        },
        fingerprint: fingerprint.clone(),
        expected_epoch: 0,
        request_id: crate::RequestId::new("restore-must-not-admit").unwrap(),
        actor: crate::resilience::common::owner(),
        permit: assignment.permit,
    };
    assert_eq!(
        restored
            .store
            .admit(
                &request,
                &g.policy,
                g.clock.as_ref(),
                rolvya_plan_validator::Limits::default()
            )
            .await
            .unwrap_err(),
        crate::admission::AdmissionError::RetryBlocked
    );
    let lease = g.lease(g.contexts.keys().next().unwrap().as_str()).await;
    assert!(
        crate::worker_control::OutboxDispatcher::new(restored.store.clone())
            .claim(
                &lease,
                &crate::resilience::common::owner(),
                &g.policy,
                g.clock.as_ref(),
                rolvya_plan_validator::Limits::default()
            )
            .await
            .is_err()
    );
    assert_eq!(
        restored.store.financial_snapshot(&g.project).await.unwrap(),
        finance
    );
    restored.close().await;
    op.close().await;
    g.store.close().await;
}

#[tokio::test]
async fn existing_database_hardlink_is_rejected_before_sqlite_opens_it() {
    let f = Fixture::new().await;
    f.initialize().await;
    f.op.close().await;
    let alias = f.root.path().join("aliased.sqlite");
    std::fs::hard_link(&f.config.database, &alias).unwrap();
    assert!(
        HostOperator::open(f.config.clone()).await.is_err(),
        "an extra DB inode alias violates private deployment storage"
    );
    std::fs::remove_file(&alias).unwrap();
    for suffix in ["-wal", "-shm"] {
        let sidecar = std::path::PathBuf::from(format!("{}{suffix}", f.config.database.display()));
        let _ = std::fs::remove_file(&sidecar);
        write_private(&sidecar, b"metadata-only rejected before SQLite");
        std::fs::hard_link(&sidecar, &alias).unwrap();
        assert!(files::database_identity(&f.config.database).is_err());
        std::fs::remove_file(&alias).unwrap();
        std::fs::set_permissions(&sidecar, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(files::database_identity(&f.config.database).is_err());
        std::fs::remove_file(&sidecar).unwrap();
    }
    for _ in 0..5 {
        let reopened = HostOperator::open(f.config.clone()).await.unwrap();
        reopened.close().await;
    }
}

#[tokio::test]
async fn recovery_selects_new_restore_after_older_recovery_clock_rollback() {
    let f = Fixture::new().await;
    f.initialize().await;
    // Model a previously completed restore before the host clock moved backward.
    let prior = "ff".repeat(32);
    sqlx::query("INSERT INTO controller_restore_holds VALUES(?,?,?,?)")
        .bind(&prior)
        .bind(random_id().unwrap())
        .bind(random_id().unwrap())
        .bind(now().unwrap() + 3600)
        .execute(&f.op.store.pool)
        .await
        .unwrap();
    f.op.event(&prior, "key_recovered", &[7; 32], None)
        .await
        .unwrap();
    let b = f.op.backup(BackupClass::Frequent).await.unwrap();
    let kit = f.root.path().join("kit/emergency.rvkit");
    f.op.export_recovery_kit(&kit, &[0x96; 32]).await.unwrap();
    let (_target, c) = f.restored_config().await;
    let source = FilesystemDestination::open(&f.config.backup_destination).unwrap();
    let r = HostOperator::restore_database(&c, &source, &b.id)
        .await
        .unwrap();
    assert_ne!(r.restore_id, prior);
    let copied = c
        .identity_verifier_key
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("kit/emergency.rvkit");
    write_private(&copied, &files::private_file(&kit, 4096).unwrap());
    let op = HostOperator::open(c).await.unwrap();
    let recovered = op
        .recover_controller_key(&copied, &[0x96; 32])
        .await
        .unwrap();
    assert_eq!(recovered.restore_id, r.restore_id);
    op.close().await;
    f.op.close().await;
}

#[tokio::test]
async fn oversized_snapshot_is_rejected_before_sqlite_inventory_allocation() {
    let f = Fixture::new().await;
    let path = f.config.scratch.join("oversized.sqlite");
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    file.set_len(super::super::files::MAX_BACKUP_BYTES as u64 + 4096)
        .unwrap();
    assert!(matches!(
        inventory(&path).await,
        Err(OperationError::ResourceLimit)
    ));
    f.op.close().await;
}

#[tokio::test]
async fn public_browser_assets_reject_an_unsafe_writable_ancestor() {
    let f = Fixture::new().await;
    let parent = f.root.path().join("unsafe-public-parent");
    std::fs::create_dir(&parent).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o777)).unwrap();
    let assets = parent.join("web");
    std::fs::create_dir(&assets).unwrap();
    std::fs::set_permissions(&assets, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut config = f.config.clone();
    config.assets = assets;
    assert!(
        config.validate_host().is_err(),
        "a trusted leaf below a replaceable parent cannot authorize browser code"
    );
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(config.validate_host().is_ok());
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o770)).unwrap();
    assert!(config.validate_host().is_err());
    f.op.close().await;
}
