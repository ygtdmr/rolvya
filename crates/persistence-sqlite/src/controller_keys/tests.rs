#![cfg(target_os = "linux")]
use super::*;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt, symlink},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
struct SyntheticLocalSetup(bool);
impl LocalSetupAuthorization for SyntheticLocalSetup {
    fn permits(&self, _: &InitializationRequest) -> bool {
        self.0
    }
}
fn request() -> InitializationRequest {
    InitializationRequest::new(RequestId::new("synthetic-offline-setup").unwrap())
}
fn temporary() -> tempfile::TempDir {
    let tmp = std::env::temp_dir();
    let uid = rustix::process::geteuid().as_raw();
    let mut base = tmp.as_path();
    if ![0, uid].contains(&fs::metadata(base).unwrap().uid()) {
        base = Path::new(env!("CARGO_MANIFEST_DIR"));
        while !base.join(".git").exists() {
            base = base.parent().unwrap();
        }
        base = base.parent().unwrap();
    }
    tempfile::tempdir_in(base).unwrap()
}
struct Fixture {
    root: tempfile::TempDir,
    data: PathBuf,
    keys: PathBuf,
    path: PathBuf,
    store: PlanStore,
}
impl Fixture {
    async fn new() -> Self {
        let root = temporary();
        let data = root.path().join("data");
        let keys = root.path().join("keys");
        for path in [&data, &keys] {
            fs::create_dir(path).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let path = data.join("state.sqlite");
        let store = PlanStore::open(&path).await.unwrap();
        Self {
            root,
            data,
            keys,
            path,
            store,
        }
    }
    async fn bootstrap(&self) -> ControllerBootstrap {
        ControllerBootstrap::open(
            self.store.clone(),
            &self.keys,
            ServiceIdentity::current(),
            &[],
        )
        .await
        .unwrap()
    }
    async fn initialize(&self) -> ControllerKey {
        self.bootstrap()
            .await
            .initialize(&request(), &SyntheticLocalSetup(true))
            .await
            .unwrap()
    }
    fn file(&self) -> PathBuf {
        self.keys.join(custody::KEY)
    }
    async fn database_state(&self) -> String {
        sqlx::query_scalar("SELECT state FROM controller_installation")
            .fetch_one(&self.store.pool)
            .await
            .unwrap()
    }
}
#[tokio::test]
async fn denied_setup_has_no_state_or_filesystem_effect() {
    let f = Fixture::new().await;
    let b = f.bootstrap().await;
    assert_eq!(
        b.initialize(&request(), &SyntheticLocalSetup(false))
            .await
            .unwrap_err(),
        BootstrapError::NotAuthorized
    );
    assert_eq!(f.database_state().await, "fresh");
    assert_eq!(fs::read_dir(&f.keys).unwrap().count(), 0);
}
#[tokio::test]
async fn entropy_failure_leaves_recovery_state_and_no_key() {
    let f = Fixture::new().await;
    let mut b = f.bootstrap().await;
    b.entropy = |_| Err(BootstrapError::RandomnessFailure);
    assert_eq!(
        b.initialize(&request(), &SyntheticLocalSetup(true))
            .await
            .unwrap_err(),
        BootstrapError::RandomnessFailure
    );
    assert_eq!(f.database_state().await, "pending");
    assert!(!f.file().exists());
    assert_eq!(
        b.load_existing().await.unwrap_err(),
        BootstrapError::InterruptedInstallation
    );
    assert_eq!(
        b.initialize(&request(), &SyntheticLocalSetup(true))
            .await
            .unwrap_err(),
        BootstrapError::InterruptedInstallation
    );
}
#[tokio::test]
async fn completed_installation_with_missing_key_never_regenerates() {
    let f = Fixture::new().await;
    f.initialize().await;
    fs::remove_file(f.file()).unwrap();
    let b = f.bootstrap().await;
    assert_eq!(
        b.load_existing().await.unwrap_err(),
        BootstrapError::MissingKey
    );
    assert_eq!(
        b.initialize(&request(), &SyntheticLocalSetup(true))
            .await
            .unwrap_err(),
        BootstrapError::AlreadyInitialized
    );
    assert!(!f.file().exists());
}
#[tokio::test]
async fn changed_key_bytes_fail_verifier_without_leaking_material() {
    let f = Fixture::new().await;
    let key = f.initialize().await;
    let mut bytes = Zeroizing::new(fs::read(f.file()).unwrap());
    bytes[HEADER] ^= 0x80;
    fs::write(f.file(), bytes.as_slice()).unwrap();
    let error = f.bootstrap().await.load_existing().await.unwrap_err();
    assert_eq!(error, BootstrapError::InstallationMismatch);
    let rendered = serde_json::to_string(&error.to_string()).unwrap();
    assert!(!rendered.contains(f.file().to_str().unwrap()));
    assert!(!format!("{key:?}").contains("material"));
}
#[tokio::test]
async fn truncated_oversized_and_empty_files_are_bounded_and_rejected() {
    for length in [0, 1, HEADER, FILE_BYTES - 1, FILE_BYTES + 1, 4096] {
        let f = Fixture::new().await;
        f.initialize().await;
        fs::OpenOptions::new()
            .write(true)
            .open(f.file())
            .unwrap()
            .set_len(length as u64)
            .unwrap();
        assert_eq!(
            f.bootstrap().await.load_existing().await.unwrap_err(),
            BootstrapError::InvalidKeyMaterial
        );
    }
}
#[tokio::test]
async fn malformed_magic_and_unsupported_format_and_key_versions_fail_closed() {
    for (offset, error) in [
        (0, BootstrapError::InvalidKeyMaterial),
        (11, BootstrapError::UnsupportedVersion),
        (51, BootstrapError::UnsupportedVersion),
    ] {
        let f = Fixture::new().await;
        f.initialize().await;
        let mut bytes = Zeroizing::new(fs::read(f.file()).unwrap());
        bytes[offset] ^= 0x40;
        fs::write(f.file(), bytes.as_slice()).unwrap();
        assert_eq!(
            f.bootstrap().await.load_existing().await.unwrap_err(),
            error
        );
    }
}
#[tokio::test]
async fn changed_installation_and_key_identifiers_are_rejected() {
    for offset in [12, 28] {
        let f = Fixture::new().await;
        f.initialize().await;
        let mut bytes = Zeroizing::new(fs::read(f.file()).unwrap());
        bytes[offset] ^= 0x40;
        fs::write(f.file(), bytes.as_slice()).unwrap();
        assert_eq!(
            f.bootstrap().await.load_existing().await.unwrap_err(),
            BootstrapError::InstallationMismatch
        );
    }
}
#[tokio::test]
async fn key_copied_from_another_installation_cannot_unlock() {
    let a = Fixture::new().await;
    let b = Fixture::new().await;
    a.initialize().await;
    b.initialize().await;
    fs::copy(a.file(), b.file()).unwrap();
    assert_eq!(
        b.bootstrap().await.load_existing().await.unwrap_err(),
        BootstrapError::InstallationMismatch
    );
}
#[tokio::test]
async fn symlink_key_is_never_followed() {
    let f = Fixture::new().await;
    f.initialize().await;
    let moved = f.keys.join("original");
    fs::rename(f.file(), &moved).unwrap();
    symlink(&moved, f.file()).unwrap();
    assert_eq!(
        f.bootstrap().await.load_existing().await.unwrap_err(),
        BootstrapError::UnsafePath
    );
}
#[tokio::test]
async fn hardlinked_key_is_rejected() {
    let f = Fixture::new().await;
    f.initialize().await;
    fs::hard_link(f.file(), f.keys.join("alias")).unwrap();
    assert_eq!(
        f.bootstrap().await.load_existing().await.unwrap_err(),
        BootstrapError::UnsafePath
    );
}
#[tokio::test]
async fn directory_and_fifo_at_key_path_are_rejected_without_blocking() {
    for fifo in [false, true] {
        let f = Fixture::new().await;
        f.initialize().await;
        fs::remove_file(f.file()).unwrap();
        if fifo {
            rustix::fs::mknodat(
                rustix::fs::CWD,
                f.file(),
                rustix::fs::FileType::Fifo,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
                0,
            )
            .unwrap();
        } else {
            fs::create_dir(f.file()).unwrap();
        }
        assert_eq!(
            f.bootstrap().await.load_existing().await.unwrap_err(),
            BootstrapError::UnsafePath
        );
    }
}
#[tokio::test]
async fn broad_key_permissions_and_special_bits_are_rejected() {
    for mode in [0o644, 0o640, 0o666, 0o400, 0o1600, 0o2600] {
        let f = Fixture::new().await;
        f.initialize().await;
        fs::set_permissions(f.file(), fs::Permissions::from_mode(mode)).unwrap();
        assert_eq!(
            f.bootstrap().await.load_existing().await.unwrap_err(),
            BootstrapError::InsecurePermissions
        );
    }
}
#[tokio::test]
async fn broad_directory_permissions_are_rejected_at_open_and_restart() {
    let f = Fixture::new().await;
    f.initialize().await;
    let b = f.bootstrap().await;
    for mode in [0o755, 0o770, 0o777, 0o1700] {
        fs::set_permissions(&f.keys, fs::Permissions::from_mode(mode)).unwrap();
        assert_eq!(
            ControllerBootstrap::open(f.store.clone(), &f.keys, ServiceIdentity::current(), &[])
                .await
                .err(),
            Some(BootstrapError::InsecurePermissions)
        );
        assert_eq!(
            b.load_existing().await.unwrap_err(),
            BootstrapError::InsecurePermissions
        );
    }
}
#[tokio::test]
async fn wrong_service_owner_and_root_identity_are_rejected() {
    let f = Fixture::new().await;
    for owner in [
        ServiceIdentity::from_uid(0),
        ServiceIdentity::from_uid(ServiceIdentity::current().0 + 1),
    ] {
        assert_eq!(
            ControllerBootstrap::open(f.store.clone(), &f.keys, owner, &[])
                .await
                .err(),
            Some(BootstrapError::OwnershipMismatch)
        );
    }
}
#[tokio::test]
async fn parent_and_final_directory_symlinks_are_rejected() {
    let f = Fixture::new().await;
    let alias = f.root.path().join("alias");
    symlink(&f.keys, &alias).unwrap();
    assert_eq!(
        ControllerBootstrap::open(f.store.clone(), &alias, ServiceIdentity::current(), &[])
            .await
            .err(),
        Some(BootstrapError::UnsafePath)
    );
    let parent_alias = f.root.path().join("parent-alias");
    symlink(f.root.path(), &parent_alias).unwrap();
    assert_eq!(
        ControllerBootstrap::open(
            f.store.clone(),
            &parent_alias.join("keys"),
            ServiceIdentity::current(),
            &[]
        )
        .await
        .err(),
        Some(BootstrapError::UnsafePath)
    );
}
#[tokio::test]
async fn lexical_path_aliases_are_rejected() {
    let f = Fixture::new().await;
    for path in [
        PathBuf::from("relative"),
        f.keys.join("."),
        f.keys.join(".."),
        PathBuf::from(format!("{}//keys", f.root.path().display())),
    ] {
        assert_eq!(
            ControllerBootstrap::open(f.store.clone(), &path, ServiceIdentity::current(), &[])
                .await
                .err(),
            Some(BootstrapError::UnsafePath)
        );
    }
}
#[tokio::test]
async fn key_directory_replacement_is_detected_by_inode() {
    let f = Fixture::new().await;
    f.initialize().await;
    let b = f.bootstrap().await;
    fs::rename(&f.keys, f.root.path().join("old-keys")).unwrap();
    fs::create_dir(&f.keys).unwrap();
    fs::set_permissions(&f.keys, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        b.load_existing().await.unwrap_err(),
        BootstrapError::UnsafePath
    );
}
#[tokio::test]
async fn insecure_ancestor_directory_is_rejected() {
    let f = Fixture::new().await;
    fs::set_permissions(f.root.path(), fs::Permissions::from_mode(0o777)).unwrap();
    assert_eq!(
        ControllerBootstrap::open(f.store.clone(), &f.keys, ServiceIdentity::current(), &[])
            .await
            .err(),
        Some(BootstrapError::InsecurePermissions)
    );
}
#[tokio::test]
async fn database_backup_and_deployment_roots_are_excluded() {
    let f = Fixture::new().await;
    assert_eq!(
        ControllerBootstrap::open(f.store.clone(), &f.data, ServiceIdentity::current(), &[])
            .await
            .err(),
        Some(BootstrapError::UnsafePath)
    );
    assert_eq!(
        ControllerBootstrap::open(
            f.store.clone(),
            &f.keys,
            ServiceIdentity::current(),
            &[f.root.path().to_path_buf()]
        )
        .await
        .err(),
        Some(BootstrapError::UnsafePath)
    );
    assert_eq!(
        ControllerBootstrap::open(
            f.store.clone(),
            &f.keys,
            ServiceIdentity::current(),
            &[f.keys.join("missing")]
        )
        .await
        .err(),
        Some(BootstrapError::UnsafePath)
    );
}
#[tokio::test]
async fn repositories_and_worktree_pointers_are_excluded() {
    for worktree in [false, true] {
        let f = Fixture::new().await;
        if worktree {
            fs::write(f.keys.join(".git"), "gitdir: /not-used").unwrap();
        } else {
            fs::create_dir(f.keys.join(".git")).unwrap();
            fs::write(f.keys.join(".git/HEAD"), "ref: refs/heads/main").unwrap();
        }
        assert_eq!(
            ControllerBootstrap::open(f.store.clone(), &f.keys, ServiceIdentity::current(), &[])
                .await
                .err(),
            Some(BootstrapError::UnsafePath)
        );
    }
}
#[tokio::test]
async fn populated_database_cannot_initialize_even_if_key_is_absent() {
    let f = Fixture::new().await;
    f.store
        .create_project(
            &rolvya_domain::ids::ProjectId::new("existing").unwrap(),
            &crate::PrincipalId::new("owner").unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        f.bootstrap()
            .await
            .initialize(&request(), &SyntheticLocalSetup(true))
            .await
            .unwrap_err(),
        BootstrapError::RecoveryRequired
    );
    assert!(!f.file().exists());
    assert_eq!(f.database_state().await, "fresh");
}
#[tokio::test]
async fn precreated_empty_database_is_recovery_required() {
    let f = Fixture::new().await;
    let path = f.data.join("preexisting.sqlite");
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    let store = PlanStore::open(&path).await.unwrap();
    let b = ControllerBootstrap::open(store.clone(), &f.keys, ServiceIdentity::current(), &[])
        .await
        .unwrap();
    assert_eq!(b.state().await.unwrap(), SetupState::RecoveryRequired);
    assert_eq!(
        b.initialize(&request(), &SyntheticLocalSetup(true))
            .await
            .unwrap_err(),
        BootstrapError::RecoveryRequired
    );
    store.close().await;
}
#[tokio::test]
async fn file_without_database_marker_is_never_adopted_or_overwritten() {
    let original = Fixture::new().await;
    original.initialize().await;
    let fresh = Fixture::new().await;
    fs::copy(original.file(), fresh.file()).unwrap();
    let bytes = Zeroizing::new(fs::read(fresh.file()).unwrap());
    assert_eq!(
        fresh
            .bootstrap()
            .await
            .initialize(&request(), &SyntheticLocalSetup(true))
            .await
            .unwrap_err(),
        BootstrapError::InterruptedInstallation
    );
    assert!(fs::read(fresh.file()).unwrap().as_slice() == bytes.as_slice());
    assert_eq!(
        fresh.bootstrap().await.load_existing().await.unwrap_err(),
        BootstrapError::RecoveryRequired
    );
}
#[tokio::test]
async fn creation_permissions_are_restrictive_and_key_never_enters_database_or_snapshot() {
    let f = Fixture::new().await;
    let key = f.initialize().await;
    assert_eq!(fs::metadata(&f.keys).unwrap().mode() & 0o7777, 0o700);
    assert_eq!(fs::metadata(f.file()).unwrap().mode() & 0o7777, 0o600);
    let snapshot = f.data.join("backup.sqlite");
    f.store.snapshot(&snapshot).await.unwrap();
    for path in [
        &f.path,
        &snapshot,
        &PathBuf::from(format!("{}-wal", f.path.display())),
    ] {
        if path.exists() {
            let bytes = fs::read(path).unwrap();
            assert!(!bytes.windows(32).any(|w| w == key.material.as_slice()));
        }
    }
    let restored = PlanStore::open(&snapshot).await.unwrap();
    let b = ControllerBootstrap::open(restored.clone(), &f.keys, ServiceIdentity::current(), &[])
        .await
        .unwrap();
    assert_eq!(b.load_existing().await.unwrap().metadata(), key.metadata());
    restored.close().await;
}
#[tokio::test]
async fn sqlite_failure_after_publication_preserves_key_and_pending_state() {
    let f = Fixture::new().await;
    let mut b = f.bootstrap().await;
    b.hook = Box::new(|at| {
        if at == Boundary::BeforeStateCommit {
            Err(BootstrapError::DatabaseStateMismatch)
        } else {
            Ok(())
        }
    });
    assert_eq!(
        b.initialize(&request(), &SyntheticLocalSetup(true))
            .await
            .unwrap_err(),
        BootstrapError::DatabaseStateMismatch
    );
    assert!(f.file().exists());
    f.store.close().await;
    let store = PlanStore::open(&f.path).await.unwrap();
    let b = ControllerBootstrap::open(store.clone(), &f.keys, ServiceIdentity::current(), &[])
        .await
        .unwrap();
    assert_eq!(
        b.load_existing().await.unwrap_err(),
        BootstrapError::InterruptedInstallation
    );
    assert_eq!(
        b.initialize(&request(), &SyntheticLocalSetup(true))
            .await
            .unwrap_err(),
        BootstrapError::InterruptedInstallation
    );
    store.check_integrity().await.unwrap();
    store.close().await;
}
#[tokio::test]
async fn exclusive_publication_cannot_overwrite_a_racing_file() {
    let f = Fixture::new().await;
    let mut b = f.bootstrap().await;
    let path = f.file();
    b.hook = Box::new(move |at| {
        if at == Boundary::BeforePublication {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
                .unwrap()
                .write_all(b"not key material")
                .unwrap();
        }
        Ok(())
    });
    assert_eq!(
        b.initialize(&request(), &SyntheticLocalSetup(true))
            .await
            .unwrap_err(),
        BootstrapError::StorageFailure
    );
    assert!(fs::read(f.file()).unwrap() == b"not key material");
    assert_eq!(f.database_state().await, "pending");
}
#[tokio::test]
async fn interrupted_writes_leave_no_usable_installation() {
    for boundary in [
        Boundary::TemporaryCreated,
        Boundary::ContentsWritten,
        Boundary::FileSynced,
    ] {
        let f = Fixture::new().await;
        let mut b = f.bootstrap().await;
        b.hook = Box::new(move |at| {
            if at == boundary {
                Err(BootstrapError::StorageFailure)
            } else {
                Ok(())
            }
        });
        assert_eq!(
            b.initialize(&request(), &SyntheticLocalSetup(true))
                .await
                .unwrap_err(),
            BootstrapError::StorageFailure
        );
        assert!(!f.file().exists());
        assert_eq!(f.database_state().await, "pending");
        assert_eq!(
            b.load_existing().await.unwrap_err(),
            BootstrapError::InterruptedInstallation
        );
    }
}
#[tokio::test]
async fn read_permission_denial_fails_closed_without_key_replacement() {
    let f = Fixture::new().await;
    f.initialize().await;
    let b = f.bootstrap().await;
    fs::set_permissions(&f.keys, fs::Permissions::from_mode(0o0)).unwrap();
    assert_eq!(
        b.load_existing().await.unwrap_err(),
        BootstrapError::InsecurePermissions
    );
    fs::set_permissions(&f.keys, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(b.load_existing().await.is_ok());
}
#[tokio::test]
async fn database_metadata_is_immutable_after_commit() {
    let f = Fixture::new().await;
    f.initialize().await;
    for sql in [
        "DELETE FROM controller_installation",
        "UPDATE controller_installation SET state='fresh'",
        "UPDATE controller_installation SET verifier=zeroblob(32)",
        "UPDATE controller_installation SET key_version=2",
    ] {
        assert!(sqlx::query(sql).execute(&f.store.pool).await.is_err());
    }
    assert!(f.bootstrap().await.load_existing().await.is_ok());
}
#[tokio::test]
async fn revoked_local_setup_authority_after_generation_does_not_publish() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Once(AtomicUsize);
    impl LocalSetupAuthorization for Once {
        fn permits(&self, _: &InitializationRequest) -> bool {
            self.0.fetch_add(1, Ordering::SeqCst) == 0
        }
    }
    let f = Fixture::new().await;
    assert_eq!(
        f.bootstrap()
            .await
            .initialize(&request(), &Once(AtomicUsize::new(0)))
            .await
            .unwrap_err(),
        BootstrapError::NotAuthorized
    );
    assert!(!f.file().exists());
    assert_eq!(f.database_state().await, "pending");
}
// Child driver is a test binary only. Production contains no env-controlled fault hook.
fn child_command(f: &Fixture, boundary: &str, mode: &str) -> Command {
    let mut c = Command::new(std::env::current_exe().unwrap());
    c.args([
        "--exact",
        "controller_keys::tests::subprocess_driver",
        "--nocapture",
    ])
    .env("ROLVYA_S201_TEST_ROOT", f.root.path())
    .env("ROLVYA_S201_TEST_BOUNDARY", boundary)
    .env("ROLVYA_S201_TEST_MODE", mode)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    c
}
struct Victim(Child);
impl Drop for Victim {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn paused(f: &Fixture, boundary: Boundary) -> Victim {
    let mut child = child_command(f, &format!("{boundary:?}"), "pause")
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                break;
            };
            if line.starts_with("BOUNDARY ") {
                let _ = tx.send(line);
            }
        }
    });
    let victim = Victim(child);
    assert!(
        rx.recv_timeout(Duration::from_secs(30))
            .unwrap()
            .starts_with("BOUNDARY ")
    );
    victim
}
#[tokio::test]
async fn subprocess_driver() {
    let Ok(root) = std::env::var("ROLVYA_S201_TEST_ROOT") else {
        return;
    };
    let store = PlanStore::open(Path::new(&root).join("data/state.sqlite"))
        .await
        .unwrap();
    let mut b = ControllerBootstrap::open(
        store.clone(),
        &Path::new(&root).join("keys"),
        ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    if std::env::var("ROLVYA_S201_TEST_MODE").unwrap() == "contend" {
        assert_eq!(
            b.initialize(&request(), &SyntheticLocalSetup(true))
                .await
                .unwrap_err(),
            BootstrapError::InitializationInProgress
        );
        return;
    }
    b.hook = Box::new(|at| {
        if std::env::var("ROLVYA_S201_TEST_BOUNDARY").unwrap() == format!("{at:?}") {
            println!("BOUNDARY {at:?}");
            std::io::stdout().flush().unwrap();
            std::io::stdin().read_exact(&mut [0u8; 1]).unwrap();
        }
        Ok(())
    });
    b.initialize(&request(), &SyntheticLocalSetup(true))
        .await
        .unwrap();
    store.close().await;
}
#[tokio::test]
async fn sigkill_at_all_setup_boundaries_recovers_without_replacing_published_keys() {
    for boundary in [
        Boundary::BeforeSetup,
        Boundary::OwnershipAcquired,
        Boundary::BeforeRandomness,
        Boundary::KeyGenerated,
        Boundary::TemporaryCreated,
        Boundary::ContentsWritten,
        Boundary::FileSynced,
        Boundary::BeforePublication,
        Boundary::Published,
        Boundary::BeforeStateCommit,
        Boundary::StateCommitted,
        Boundary::BeforeAcknowledgement,
    ] {
        let f = Fixture::new().await;
        f.store.close().await;
        let mut victim = paused(&f, boundary);
        victim.0.kill().unwrap();
        victim.0.wait().unwrap();
        let bytes = f
            .file()
            .exists()
            .then(|| Zeroizing::new(fs::read(f.file()).unwrap()));
        let store = PlanStore::open(&f.path).await.unwrap();
        store.check_integrity().await.unwrap();
        let b = ControllerBootstrap::open(store.clone(), &f.keys, ServiceIdentity::current(), &[])
            .await
            .unwrap();
        match boundary {
            Boundary::BeforeSetup | Boundary::OwnershipAcquired => {
                assert_eq!(b.state().await.unwrap(), SetupState::Fresh);
            }
            Boundary::StateCommitted | Boundary::BeforeAcknowledgement => {
                assert_eq!(b.state().await.unwrap(), SetupState::Initialized);
                assert!(b.load_existing().await.is_ok());
                assert_eq!(
                    b.initialize(&request(), &SyntheticLocalSetup(true))
                        .await
                        .unwrap_err(),
                    BootstrapError::AlreadyInitialized
                );
            }
            _ => {
                assert_eq!(b.state().await.unwrap(), SetupState::Interrupted);
                assert_eq!(
                    b.initialize(&request(), &SyntheticLocalSetup(true))
                        .await
                        .unwrap_err(),
                    BootstrapError::InterruptedInstallation
                );
                assert_eq!(
                    b.load_existing().await.unwrap_err(),
                    BootstrapError::InterruptedInstallation
                );
            }
        }
        if let Some(original) = bytes {
            assert!(fs::read(f.file()).unwrap().as_slice() == original.as_slice());
        }
        store.close().await;
    }
}
#[tokio::test]
async fn two_actual_setup_processes_cannot_establish_conflicting_keys() {
    let f = Fixture::new().await;
    f.store.close().await;
    let mut first = paused(&f, Boundary::OwnershipAcquired);
    let second = child_command(&f, "unused", "contend").output().unwrap();
    assert!(second.status.success());
    first.0.stdin.take().unwrap().write_all(b"x").unwrap();
    assert!(first.0.wait().unwrap().success());
    let store = PlanStore::open(&f.path).await.unwrap();
    let b = ControllerBootstrap::open(store.clone(), &f.keys, ServiceIdentity::current(), &[])
        .await
        .unwrap();
    assert!(b.load_existing().await.is_ok());
    assert_eq!(fs::read_dir(&f.keys).unwrap().count(), 1);
    store.close().await;
}
#[tokio::test]
async fn copied_fresh_database_is_not_a_new_installation() {
    let f = Fixture::new().await;
    let snapshot = f.data.join("pre-setup.sqlite");
    f.store.snapshot(&snapshot).await.unwrap();
    let store = PlanStore::open(&snapshot).await.unwrap();
    let b = ControllerBootstrap::open(store.clone(), &f.keys, ServiceIdentity::current(), &[])
        .await
        .unwrap();
    assert_eq!(
        b.initialize(&request(), &SyntheticLocalSetup(true))
            .await
            .unwrap_err(),
        BootstrapError::RecoveryRequired
    );
    assert!(!f.file().exists());
    store.close().await;
}
#[tokio::test]
async fn replacement_or_disappearance_during_read_returns_no_key_handle() {
    for remove in [false, true] {
        let f = Fixture::new().await;
        f.initialize().await;
        let mut b = f.bootstrap().await;
        let path = f.file();
        let moved = f.keys.join("original");
        b.hook = Box::new(move |at| {
            if at == Boundary::ReadComplete {
                fs::rename(&path, &moved).unwrap();
                if !remove {
                    fs::copy(&moved, &path).unwrap();
                }
            }
            Ok(())
        });
        assert!(matches!(
            b.load_existing().await,
            Err(BootstrapError::UnsafePath | BootstrapError::StorageFailure)
        ));
        assert_eq!(f.database_state().await, "ready");
    }
}
#[tokio::test]
async fn read_interruption_returns_no_authority_and_does_not_regenerate() {
    let f = Fixture::new().await;
    let original = f.initialize().await.metadata().clone();
    let mut b = f.bootstrap().await;
    b.hook = Box::new(|at| {
        if at == Boundary::ReadOpened {
            Err(BootstrapError::StorageFailure)
        } else {
            Ok(())
        }
    });
    assert_eq!(
        b.load_existing().await.unwrap_err(),
        BootstrapError::StorageFailure
    );
    assert_eq!(
        f.bootstrap()
            .await
            .load_existing()
            .await
            .unwrap()
            .metadata(),
        &original
    );
}
#[tokio::test]
async fn replaced_exclusive_database_file_cannot_gain_fresh_authority() {
    let f = Fixture::new().await;
    let path = f.data.join("new.sqlite");
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    let birth = new_database_guard(&path, file).unwrap();
    fs::rename(&path, f.data.join("original.sqlite")).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    assert_eq!(
        mark_new_database(&f.store, &birth).await.unwrap_err(),
        StoreError::InvalidPath
    );
}
#[tokio::test]
async fn exclusive_database_guard_rejects_symlink_parents_and_insecure_storage() {
    let f = Fixture::new().await;
    let alias = f.root.path().join("alias");
    symlink(&f.data, &alias).unwrap();
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(alias.join("new.sqlite"))
        .unwrap();
    assert!(new_database_guard(&alias.join("new.sqlite"), file).is_none());
    fs::set_permissions(&f.data, fs::Permissions::from_mode(0o755)).unwrap();
    let file = fs::OpenOptions::new()
        .read(true)
        .open(f.data.join("new.sqlite"))
        .unwrap();
    assert!(new_database_guard(&f.data.join("new.sqlite"), file).is_none());
}
#[test]
fn wrong_key_file_owner_is_rejected_without_changing_host_ownership() {
    let root = temporary();
    let path = root.path().join("metadata-test");
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    let mut stat = rustix::fs::fstat(&file).unwrap();
    stat.st_uid = ServiceIdentity::current().0 + 1;
    assert_eq!(
        custody::check_key(&stat, ServiceIdentity::current()).unwrap_err(),
        BootstrapError::OwnershipMismatch
    );
    // Inspect an existing character device read-only; never create a device or
    // change its ownership in the host or in the disposable installation.
    let device = std::fs::File::open("/dev/null").unwrap();
    assert_eq!(
        custody::check_key(
            &rustix::fs::fstat(&device).unwrap(),
            ServiceIdentity::current()
        )
        .unwrap_err(),
        BootstrapError::UnsafePath
    );
}
async fn legacy_database(f: &Fixture, populated: bool) -> (PathBuf, PathBuf) {
    let path = f.data.join("legacy.sqlite");
    fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .unwrap();
    let raw = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .foreign_keys(true),
    )
    .await
    .unwrap();
    let scripts = f.root.path().join("scripts");
    fs::create_dir(&scripts).unwrap();
    for entry in fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations")).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name().to_string_lossy().as_ref() < "0006" {
            fs::copy(entry.path(), scripts.join(entry.file_name())).unwrap();
        }
    }
    sqlx::migrate::Migrator::new(scripts.as_path())
        .await
        .unwrap()
        .run(&raw)
        .await
        .unwrap();
    if populated {
        sqlx::query("INSERT INTO projects(project_id,owner_actor) VALUES ('preserved','owner')")
            .execute(&raw)
            .await
            .unwrap();
    }
    raw.close().await;
    (path, scripts)
}
#[tokio::test]
async fn version_five_upgrade_never_grants_fresh_setup_or_changes_existing_data() {
    for populated in [false, true] {
        let f = Fixture::new().await;
        let (path, _) = legacy_database(&f, populated).await;
        let store = PlanStore::open(path).await.unwrap();
        store.check_integrity().await.unwrap();
        let b = ControllerBootstrap::open(store.clone(), &f.keys, ServiceIdentity::current(), &[])
            .await
            .unwrap();
        assert_eq!(b.state().await.unwrap(), SetupState::RecoveryRequired);
        assert_eq!(
            b.initialize(&request(), &SyntheticLocalSetup(true))
                .await
                .unwrap_err(),
            BootstrapError::RecoveryRequired
        );
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM projects")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(count, i64::from(populated));
        store.close().await;
    }
}
#[tokio::test]
async fn failed_bootstrap_migration_rolls_back_and_replays_without_key_creation() {
    let f = Fixture::new().await;
    let (path, scripts) = legacy_database(&f, true).await;
    fs::write(
        scripts.join("0006_controller_key_bootstrap.sql"),
        format!(
            "{}\nSELECT missing_column FROM missing_table;",
            include_str!("../../migrations/0006_controller_key_bootstrap.sql")
        ),
    )
    .unwrap();
    let raw = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .foreign_keys(true),
    )
    .await
    .unwrap();
    assert!(
        sqlx::migrate::Migrator::new(scripts.as_path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM sqlite_schema WHERE name='controller_installation'"
        )
        .fetch_one(&raw)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations WHERE success=1")
            .fetch_one(&raw)
            .await
            .unwrap(),
        5
    );
    raw.close().await;
    let store = PlanStore::open(path).await.unwrap();
    store.check_integrity().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT owner_actor FROM projects WHERE project_id='preserved'"
        )
        .fetch_one(&store.pool)
        .await
        .unwrap(),
        "owner"
    );
    assert_eq!(
        ControllerBootstrap::open(store.clone(), &f.keys, ServiceIdentity::current(), &[])
            .await
            .unwrap()
            .state()
            .await
            .unwrap(),
        SetupState::RecoveryRequired
    );
    assert!(!f.file().exists());
    store.close().await;
}
#[tokio::test]
async fn concurrent_calls_on_one_bootstrap_share_no_reentrant_flock() {
    let f = Fixture::new().await;
    let b = f.bootstrap().await;
    let request = request();
    let authority = SyntheticLocalSetup(true);
    let (a, c) = tokio::join!(
        b.initialize(&request, &authority),
        b.initialize(&request, &authority)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(c.is_ok()), 1);
    let error = if a.is_err() {
        a.unwrap_err()
    } else {
        c.unwrap_err()
    };
    assert!(matches!(
        error,
        BootstrapError::InitializationInProgress | BootstrapError::AlreadyInitialized
    ));
    assert_eq!(b.state().await.unwrap(), SetupState::Initialized);
}
