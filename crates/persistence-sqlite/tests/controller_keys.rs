#![cfg(target_os = "linux")]
use rolvya_persistence_sqlite::{PlanStore, RequestId, controller_keys::*};
use std::{fs, os::unix::fs::PermissionsExt};
struct SyntheticLocalSetup;
impl LocalSetupAuthorization for SyntheticLocalSetup {
    fn permits(&self, _: &InitializationRequest) -> bool {
        true
    }
}
#[tokio::test]
async fn explicit_setup_and_restart_preserve_identity_without_reinitializing() {
    // The managed Cloud /tmp is owned by an unmapped UID. Do not weaken
    // production ownership checks to accommodate it; use a service-owned parent.
    let tmp = std::env::temp_dir();
    use std::os::unix::fs::MetadataExt;
    let uid = rustix::process::geteuid().as_raw();
    let mut base = tmp.as_path();
    if ![0, uid].contains(&fs::metadata(base).unwrap().uid()) {
        base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        while !base.join(".git").exists() {
            base = base.parent().unwrap();
        }
        base = base.parent().unwrap();
    }
    let temp = tempfile::tempdir_in(base).unwrap();
    let data = temp.path().join("data");
    let keys = temp.path().join("keys");
    fs::create_dir(&data).unwrap();
    fs::create_dir(&keys).unwrap();
    fs::set_permissions(&data, fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(&keys, fs::Permissions::from_mode(0o700)).unwrap();
    let path = data.join("state.sqlite");
    let store = PlanStore::open(&path).await.unwrap();
    let bootstrap =
        ControllerBootstrap::open(store.clone(), &keys, ServiceIdentity::current(), &[])
            .await
            .unwrap();
    assert_eq!(bootstrap.state().await.unwrap(), SetupState::Fresh);
    assert!(matches!(
        bootstrap.load_existing().await,
        Err(BootstrapError::NotInitialized)
    ));
    let request = InitializationRequest::new(RequestId::new("setup-one").unwrap());
    let key = bootstrap
        .initialize(&request, &SyntheticLocalSetup)
        .await
        .unwrap();
    let identity = key.metadata().clone();
    assert_eq!(
        bootstrap
            .initialize(&request, &SyntheticLocalSetup)
            .await
            .unwrap_err(),
        BootstrapError::AlreadyInitialized
    );
    assert!(!format!("{key:?}").contains("secret"));
    store.close().await;
    for _ in 0..3 {
        let store = PlanStore::open(&path).await.unwrap();
        let bootstrap =
            ControllerBootstrap::open(store.clone(), &keys, ServiceIdentity::current(), &[])
                .await
                .unwrap();
        assert_eq!(
            bootstrap.load_existing().await.unwrap().metadata(),
            &identity
        );
        store.close().await;
    }
}
