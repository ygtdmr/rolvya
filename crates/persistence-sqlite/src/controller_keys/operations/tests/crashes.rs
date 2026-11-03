use super::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Command, Stdio},
    time::Duration,
};
#[tokio::test]
async fn operations_crash_child() {
    let Some(config) = std::env::var_os("ROLVYA_OPS_CHILD_CONFIG") else {
        return;
    };
    let config = DeploymentConfig::load(Path::new(&config)).unwrap();
    let stage = std::env::var("ROLVYA_OPS_TEST_STOP").unwrap();
    if stage.starts_with("restore-") {
        let source =
            FilesystemDestination::open(Path::new(&std::env::var("ROLVYA_OPS_SOURCE").unwrap()))
                .unwrap();
        let _ = HostOperator::restore_database(
            &config,
            &source,
            &std::env::var("ROLVYA_OPS_BACKUP").unwrap(),
        )
        .await
        .unwrap();
        panic!("requested restore crash boundary was not reached");
    }
    let op = HostOperator::open(config).await.unwrap();
    if stage.starts_with("backup-") {
        let _ = op.backup(BackupClass::Frequent).await.unwrap();
    } else if stage.starts_with("retention-") {
        op.retain_backups().await.unwrap();
    } else {
        let mut secret = Zeroizing::new([0; 32]);
        std::io::stdin().read_exact(&mut *secret).unwrap();
        let kit = op
            .config
            .identity_verifier_key
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("kit/crash.rvkit");
        if stage.starts_with("recovery-") {
            op.recover_controller_key(&kit, &*secret).await.unwrap();
        } else {
            op.export_recovery_kit(&kit, &*secret).await.unwrap();
        }
    }
    panic!("requested crash boundary was not reached");
}
#[tokio::test]
async fn sigkill_restore_and_recovered_key_publication_never_enable_external_effects() {
    for stage in [
        "restore-file",
        "restore-before-commit",
        "restore-after-commit",
        "restore-published",
        "recovery-key-published",
        "recovery-before-commit",
        "recovery-after-commit",
    ] {
        let f = Fixture::new().await;
        f.initialize().await;
        f.enroll().await;
        let backup = f.op.backup(BackupClass::Frequent).await.unwrap();
        let kit = f.root.path().join("kit/crash.rvkit");
        f.op.export_recovery_kit(&kit, &[0x57; 32]).await.unwrap();
        let (_target, c) = f.restored_config().await;
        let configuration = c
            .identity_verifier_key
            .parent()
            .unwrap()
            .join("deployment.json");
        write_private(&configuration, &canonical(&c).unwrap());
        let target_kit = c
            .identity_verifier_key
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("kit/crash.rvkit");
        write_private(&target_kit, &files::private_file(&kit, 4096).unwrap());
        if stage.starts_with("recovery-") {
            HostOperator::restore_database(
                &c,
                &FilesystemDestination::open(&f.config.backup_destination).unwrap(),
                &backup.id,
            )
            .await
            .unwrap();
        }
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "controller_keys::operations::tests::crashes::operations_crash_child",
                "--nocapture",
            ])
            .env("ROLVYA_OPS_CHILD_CONFIG", &configuration)
            .env("ROLVYA_OPS_TEST_STOP", stage)
            .env("ROLVYA_OPS_SOURCE", &f.config.backup_destination)
            .env("ROLVYA_OPS_BACKUP", &backup.id)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&[0x57; 32]).unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            for l in BufReader::new(stdout).lines() {
                let l = l.unwrap();
                if l == format!("ops-barrier:{stage}") {
                    let _ = send.send(());
                    break;
                }
            }
        });
        if receive.recv_timeout(Duration::from_secs(30)).is_err() {
            child.kill().unwrap();
            panic!("child did not reach {stage}");
        }
        child.kill().unwrap();
        child.wait().unwrap();
        reader.join().unwrap();
        if c.database.exists() {
            let restored = HostOperator::open(c.clone()).await.unwrap();
            restored.store.check_integrity().await.unwrap();
            let mut tx = restored.store.pool.begin().await.unwrap();
            assert!(!external_authority_available(&mut tx).await.unwrap());
            tx.commit().await.unwrap();
            if stage.starts_with("recovery-") {
                restored.bootstrap.load_existing().await.unwrap();
                if stage != "recovery-after-commit" {
                    restored
                        .recover_controller_key(&target_kit, &[0x57; 32])
                        .await
                        .unwrap();
                }
                assert!(
                    !restored
                        .readiness()
                        .await
                        .unwrap()
                        .billable_execution_allowed
                );
            }
            restored.close().await;
        } else {
            assert!(HostOperator::open(c.clone()).await.is_err());
            assert!(!c.database.exists());
        }
        f.op.store.check_integrity().await.unwrap();
        f.op.close().await;
    }
}
#[tokio::test]
async fn sigkill_reopen_preserves_ambiguous_backup_and_key_export_without_secret_leaks() {
    for stage in [
        "backup-intent",
        "backup-encrypted",
        "backup-published",
        "backup-before-commit",
        "backup-after-commit",
        "kit-intent",
        "kit-published",
        "retention-intent",
    ] {
        let f = Fixture::new().await;
        f.initialize().await;
        if stage == "retention-intent" {
            archived_point(&f, BackupClass::Frequent, 8).await;
        }
        let path = f.root.path().join("config/deployment.json");
        write_private(&path, &canonical(&f.config).unwrap());
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "controller_keys::operations::tests::crashes::operations_crash_child",
                "--nocapture",
            ])
            .env("ROLVYA_OPS_CHILD_CONFIG", &path)
            .env("ROLVYA_OPS_TEST_STOP", stage)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&[0x43; 32]).unwrap();
        let stdout = child.stdout.take().unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            for l in BufReader::new(stdout).lines() {
                let l = l.unwrap();
                if l == format!("ops-barrier:{stage}") {
                    let _ = send.send(());
                    break;
                }
            }
        });
        if receive.recv_timeout(Duration::from_secs(30)).is_err() {
            child.kill().unwrap();
            panic!("child did not reach {stage}");
        }
        child.kill().unwrap();
        let status = child.wait().unwrap();
        assert!(!status.success());
        reader.join().unwrap();
        f.op.store.check_integrity().await.unwrap();
        f.op.bootstrap.load_existing().await.unwrap();
        let verified: i64 = sqlx::query_scalar("SELECT count(*) FROM controller_backup_points")
            .fetch_one(&f.op.store.pool)
            .await
            .unwrap();
        if stage == "retention-intent" {
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM controller_operation_events WHERE kind='backup_retired'"
                )
                .fetch_one(&f.op.store.pool)
                .await
                .unwrap(),
                0,
                "cleanup intent must not claim filesystem retirement"
            );
            assert_eq!(f.op.retain_backups().await.unwrap(), 1);
        }
        assert_eq!(
            verified,
            if stage == "backup-after-commit" {
                1
            } else if stage == "retention-intent" {
                2
            } else {
                0
            }
        );
        for path in [
            &f.config.database,
            &PathBuf::from(format!("{}-wal", f.config.database.display())),
        ] {
            if let Ok(bytes) = std::fs::read(path) {
                assert!(!bytes.windows(32).any(|b| b == [0x43; 32]));
            }
        }
        f.op.close().await;
    }
}
