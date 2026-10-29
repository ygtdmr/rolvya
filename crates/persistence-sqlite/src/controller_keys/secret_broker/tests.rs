#![cfg(target_os = "linux")]
use super::*;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};
struct SyntheticSetup;
impl super::super::LocalSetupAuthorization for SyntheticSetup {
    fn permits(&self, _: &super::super::InitializationRequest) -> bool {
        true
    }
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
fn admin(id: &str) -> AdminRequest {
    AdminRequest {
        actor: PrincipalId::new("synthetic-owner").unwrap(),
        request_id: RequestId::new(id).unwrap(),
    }
}
fn scope(project: &str) -> ConnectionScope {
    ConnectionScope {
        project: ProjectId::new(project).unwrap(),
        connection: ConnectionId::new(if project == "project-b" {
            "synthetic-connection-b"
        } else {
            "synthetic-connection"
        })
        .unwrap(),
        account: AccountId::new(if project == "project-b" {
            "synthetic-account-b"
        } else {
            "synthetic-account"
        })
        .unwrap(),
        provider: ProviderId::new("synthetic").unwrap(),
        mode: ProviderMode::ApiKey,
    }
}
fn record(project: &str) -> RecordScope {
    RecordScope {
        connection: scope(project),
        record: RecordId::new("record").unwrap(),
        purpose: SecretPurpose::new("model-access").unwrap(),
    }
}
struct SyntheticAuthority {
    scope: ConnectionScope,
    actions: Vec<BrokerAction>,
}
impl HumanAuthority for SyntheticAuthority {
    fn check(&self, c: &AdminContext<'_>) -> Result<(), BrokerError> {
        if *c.scope == self.scope
            && c.request.actor == admin("x").actor
            && self.actions.contains(&c.action)
        {
            Ok(())
        } else {
            Err(BrokerError::Forbidden)
        }
    }
}
impl UseAuthority for SyntheticAuthority {
    fn check(&self, r: &SecretUseRequest, _: i64) -> Result<(), BrokerError> {
        if r.scope.connection == self.scope && self.actions.contains(&BrokerAction::Use) {
            Ok(())
        } else {
            Err(BrokerError::Forbidden)
        }
    }
}
impl EntitlementAuthority for SyntheticAuthority {
    fn check(
        &self,
        c: &ConnectionScope,
        _: CredentialOperation,
        _: i64,
    ) -> Result<(), BrokerError> {
        if *c == self.scope && c.provider.as_str() == "synthetic" && c.mode == ProviderMode::ApiKey
        {
            Ok(())
        } else {
            Err(BrokerError::Forbidden)
        }
    }
}
struct SyntheticConsumer;
impl TrustedConsumer for SyntheticConsumer {
    fn consume(&self, request: &SecretUseRequest, value: &SecretValue) -> Result<(), BrokerError> {
        let expected: &[u8] = if request.scope.connection.project.as_str() == "project-b" {
            b"synthetic-project-b-private-value"
        } else {
            b"synthetic-confidential-value"
        };
        if value.0.as_slice() == expected {
            Ok(())
        } else {
            Err(BrokerError::Integrity)
        }
    }
}
struct Fixture {
    root: tempfile::TempDir,
    path: PathBuf,
    keys: PathBuf,
    store: PlanStore,
    bootstrap: ControllerBootstrap,
}
impl Fixture {
    async fn new() -> Self {
        let root = temporary();
        let data = root.path().join("data");
        let keys = root.path().join("keys");
        for p in [&data, &keys] {
            fs::create_dir(p).unwrap();
            fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let path = data.join("state.sqlite");
        let store = PlanStore::open(&path).await.unwrap();
        let bootstrap = ControllerBootstrap::open(
            store.clone(),
            &keys,
            super::super::ServiceIdentity::current(),
            &[],
        )
        .await
        .unwrap();
        bootstrap
            .initialize(
                &super::super::InitializationRequest::new(
                    RequestId::new("synthetic-setup").unwrap(),
                ),
                &SyntheticSetup,
            )
            .await
            .unwrap();
        for p in ["project-a", "project-b"] {
            store
                .create_project(&ProjectId::new(p).unwrap(), &admin("a").actor)
                .await
                .unwrap();
        }
        Self {
            root,
            path,
            keys,
            store,
            bootstrap,
        }
    }
    async fn broker(&self, project: &str) -> SecretBroker {
        let mut b = SecretBroker::open(&self.bootstrap).await.unwrap();
        let auth = Arc::new(SyntheticAuthority {
            scope: scope(project),
            actions: vec![
                BrokerAction::CreateConnection,
                BrokerAction::ReadMetadata,
                BrokerAction::Store,
                BrokerAction::Replace,
                BrokerAction::Revoke,
                BrokerAction::VerifyIntegrity,
                BrokerAction::Use,
            ],
        });
        b.human = auth.clone();
        b.user = auth.clone();
        b.entitlement = auth;
        b.consumer = Arc::new(SyntheticConsumer);
        b
    }
}
fn put(project: &str, id: &str) -> PutSecret {
    PutSecret {
        scope: record(project),
        expected: None,
        administration: admin(id),
    }
}
fn value() -> SecretValue {
    SecretValue::new(b"synthetic-confidential-value".to_vec()).unwrap()
}
fn use_request(project: &str, id: &str) -> SecretUseRequest {
    SecretUseRequest {
        scope: record(project),
        version: SecretVersion(1),
        request_id: RequestId::new(id).unwrap(),
        operation: CredentialOperation::LocalIntegrityInspection,
        worker: None,
    }
}
#[tokio::test]
async fn authorized_synthetic_broker_really_encrypts_and_consumes() {
    let f = Fixture::new().await;
    let b = f.broker("project-a").await;
    b.create_connection(&scope("project-a"), &admin("connection"))
        .await
        .unwrap();
    b.put_secret(&put("project-a", "store"), value())
        .await
        .unwrap();
    b.use_secret(&use_request("project-a", "use"))
        .await
        .unwrap();
    let cipher: Vec<u8> = sqlx::query_scalar("SELECT ciphertext FROM secret_versions")
        .fetch_one(&f.store.pool)
        .await
        .unwrap();
    assert!(
        !cipher
            .windows(28)
            .any(|w| w == b"synthetic-confidential-value")
    );
}
async fn ready(f: &Fixture) -> SecretBroker {
    let b = f.broker("project-a").await;
    b.create_connection(&scope("project-a"), &admin("connection"))
        .await
        .unwrap();
    b.put_secret(&put("project-a", "store"), value())
        .await
        .unwrap();
    b
}
#[tokio::test]
async fn proxy_operation_without_admitted_worker_is_denied() {
    let f = Fixture::new().await;
    let b = ready(&f).await;
    let mut request = use_request("project-a", "missing-worker");
    request.operation = CredentialOperation::ProxyModelRequest;
    assert_eq!(
        b.use_secret(&request).await.unwrap_err(),
        BrokerError::AttemptAuthority
    );
}
#[tokio::test]
async fn unimplemented_authority_has_same_error_for_unknown_project() {
    let f = Fixture::new().await;
    let b = SecretBroker::open(&f.bootstrap).await.unwrap();
    for project in ["project-a", "not-a-project"] {
        assert_eq!(
            b.use_secret(&use_request(project, "denied"))
                .await
                .unwrap_err(),
            BrokerError::UnavailableAuthority
        );
    }
}
#[tokio::test]
async fn integrity_request_identity_cannot_be_reused_for_another_record() {
    let f = Fixture::new().await;
    let b = ready(&f).await;
    let mut second = put("project-a", "second");
    second.scope.record = RecordId::new("second").unwrap();
    b.put_secret(&second, value()).await.unwrap();
    b.verify_integrity(&record("project-a"), SecretVersion(1), &admin("verify"))
        .await
        .unwrap();
    assert_eq!(
        b.verify_integrity(&second.scope, SecretVersion(1), &admin("verify"))
            .await
            .unwrap_err(),
        BrokerError::ReplayConflict
    );
}
async fn encrypted(f: &Fixture, b: &SecretBroker) -> crypto::Envelope {
    let mut c = f.store.pool.acquire().await.unwrap();
    store::load(&mut c, &record("project-a"), SecretVersion(1))
        .await
        .unwrap_or_else(|_| {
            let _ = b;
            panic!("synthetic envelope missing")
        })
}
#[tokio::test]
async fn all_public_operations_default_deny_without_production_authorities() {
    let f = Fixture::new().await;
    let b = SecretBroker::open(&f.bootstrap).await.unwrap();
    assert_eq!(
        b.create_connection(&scope("project-a"), &admin("create"))
            .await
            .unwrap_err(),
        BrokerError::UnavailableAuthority
    );
    assert_eq!(
        b.put_secret(&put("project-a", "put"), value())
            .await
            .unwrap_err(),
        BrokerError::UnavailableAuthority
    );
    assert_eq!(
        b.metadata(&record("project-a"), &admin("meta"))
            .await
            .unwrap_err(),
        BrokerError::UnavailableAuthority
    );
    assert_eq!(
        b.revoke_connection(&scope("project-a"), SecretVersion(1), &admin("revoke"))
            .await
            .unwrap_err(),
        BrokerError::UnavailableAuthority
    );
    assert_eq!(
        b.verify_integrity(&record("project-a"), SecretVersion(1), &admin("verify"))
            .await
            .unwrap_err(),
        BrokerError::UnavailableAuthority
    );
    assert_eq!(
        b.use_secret(&use_request("project-a", "use"))
            .await
            .unwrap_err(),
        BrokerError::UnavailableAuthority
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_versions")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_events WHERE outcome='denied'")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        6
    );
}
#[tokio::test]
async fn every_administrative_permission_is_independent() {
    let f = Fixture::new().await;
    let mut b = ready(&f).await;
    b.human = Arc::new(SyntheticAuthority {
        scope: scope("project-a"),
        actions: vec![BrokerAction::ReadMetadata],
    });
    assert!(
        b.metadata(&record("project-a"), &admin("meta"))
            .await
            .is_ok()
    );
    let mut replace = put("project-a", "replace");
    replace.expected = Some(SecretVersion(1));
    assert_eq!(
        b.put_secret(&replace, value()).await.unwrap_err(),
        BrokerError::Forbidden
    );
    assert_eq!(
        b.revoke_connection(&scope("project-a"), SecretVersion(1), &admin("revoke"))
            .await
            .unwrap_err(),
        BrokerError::Forbidden
    );
    assert_eq!(
        b.verify_integrity(&record("project-a"), SecretVersion(1), &admin("verify"))
            .await
            .unwrap_err(),
        BrokerError::Forbidden
    );
    b.user = Arc::new(Unimplemented);
    assert_eq!(
        b.use_secret(&use_request("project-a", "use"))
            .await
            .unwrap_err(),
        BrokerError::UnavailableAuthority
    );
}
#[tokio::test]
async fn project_and_account_connection_provider_mode_purpose_isolation() {
    let f = Fixture::new().await;
    let b = ready(&f).await;
    let mut variants = vec![record("project-b")];
    let original = record("project-a");
    let mut s = original.clone();
    s.connection.account = AccountId::new("other-account").unwrap();
    variants.push(s);
    let mut s = original.clone();
    s.connection.connection = ConnectionId::new("other-connection").unwrap();
    variants.push(s);
    let mut s = original.clone();
    s.connection.provider = ProviderId::new("other-provider").unwrap();
    variants.push(s);
    let mut s = original.clone();
    s.connection.mode = ProviderMode::CliSubscription;
    variants.push(s);
    let mut s = original.clone();
    s.purpose = SecretPurpose::new("other-purpose").unwrap();
    variants.push(s);
    for (i, s) in variants.into_iter().enumerate() {
        let mut r = use_request("project-a", &format!("wrong-{i}"));
        r.scope = s;
        assert!(b.use_secret(&r).await.is_err());
    }
    assert!(
        b.use_secret(&use_request("project-a", "correct"))
            .await
            .is_ok()
    );
}
#[tokio::test]
async fn distinct_records_get_independent_deks_nonces_and_ciphertext() {
    let f = Fixture::new().await;
    let b = ready(&f).await;
    let a = encrypted(&f, &b).await;
    let mut put = put("project-a", "second");
    put.scope.record = RecordId::new("second").unwrap();
    b.put_secret(&put, value()).await.unwrap();
    let mut c = f.store.pool.acquire().await.unwrap();
    let z = store::load(&mut c, &put.scope, SecretVersion(1))
        .await
        .unwrap();
    assert!(
        a.data_nonce != z.data_nonce
            && a.wrap_nonce != z.wrap_nonce
            && a.data_nonce != a.wrap_nonce
    );
    assert!(a.ciphertext != z.ciphertext && a.wrapped != z.wrapped);
    let da = crypto::unwrap(&b.key, &a).unwrap();
    let dz = crypto::unwrap(&b.key, &z).unwrap();
    assert!(da.as_ref() != dz.as_ref());
}
#[tokio::test]
async fn production_os_randomness_protects_complete_independent_envelopes() {
    let f = Fixture::new().await;
    let b = ready(&f).await;
    let seal = || {
        crypto::seal(
            &b.key,
            &record("project-a"),
            SecretVersion(1),
            &value(),
            os_random()?,
            *os_random()?,
            *os_random()?,
        )
    };
    let a = seal().unwrap();
    let z = seal().unwrap();
    assert!(a.data_nonce != z.data_nonce && a.wrap_nonce != z.wrap_nonce);
    assert!(a.ciphertext != z.ciphertext && a.wrapped != z.wrapped);
    assert!(
        crypto::unwrap(&b.key, &a).unwrap().as_ref()
            != crypto::unwrap(&b.key, &z).unwrap().as_ref()
    );
    assert!(crypto::open(&b.key, &a).unwrap().0 == value().0);
    assert!(crypto::open(&b.key, &z).unwrap().0 == value().0);
}
#[tokio::test]
async fn every_authenticated_identity_and_version_rejects_substitution() {
    let f = Fixture::new().await;
    let b = ready(&f).await;
    let original = encrypted(&f, &b).await;
    for i in 0..15 {
        let mut e = original.clone();
        match i {
            0 => e.scope.connection.project = ProjectId::new("project-b").unwrap(),
            1 => e.scope.connection.account = AccountId::new("account-b").unwrap(),
            2 => e.scope.connection.connection = ConnectionId::new("connection-b").unwrap(),
            3 => e.scope.connection.provider = ProviderId::new("provider-b").unwrap(),
            4 => e.scope.connection.mode = ProviderMode::CliSubscription,
            5 => e.scope.record = RecordId::new("record-b").unwrap(),
            6 => e.scope.purpose = SecretPurpose::new("purpose-b").unwrap(),
            7 => e.version = SecretVersion(2),
            8 => e.origin.installation.0[0] ^= 1,
            9 => e.origin.key.0[0] ^= 1,
            10 => e.origin.key_version = 2,
            11 => e.wrapping.key.0[0] ^= 1,
            12 => e.wrapping.key_version = 2,
            13 => e.envelope_version = 2,
            _ => e.cipher = 2,
        }
        assert!(crypto::open(&b.key, &e).is_err(), "substitution case {i}");
    }
}
#[tokio::test]
async fn modified_ciphertext_tags_and_wrapped_deks_do_not_authenticate() {
    let f = Fixture::new().await;
    let b = ready(&f).await;
    let original = encrypted(&f, &b).await;
    for i in 0..6 {
        let mut e = original.clone();
        match i {
            0 => e.ciphertext[0] ^= 1,
            1 => {
                let n = e.ciphertext.len();
                e.ciphertext[n - 1] ^= 1;
            }
            2 => e.wrapped[0] ^= 1,
            3 => e.wrapped[47] ^= 1,
            4 => e.data_nonce[0] ^= 1,
            _ => e.wrap_nonce[0] ^= 1,
        };
        assert!(crypto::open(&b.key, &e).is_err());
    }
}
#[tokio::test]
async fn wrapped_dek_swap_and_wrong_key_with_matching_metadata_fail_authentication() {
    let f = Fixture::new().await;
    let b = ready(&f).await;
    let mut e = encrypted(&f, &b).await;
    let mut r = put("project-a", "second");
    r.scope.record = RecordId::new("second").unwrap();
    b.put_secret(&r, value()).await.unwrap();
    let mut c = f.store.pool.acquire().await.unwrap();
    let other = store::load(&mut c, &r.scope, SecretVersion(1))
        .await
        .unwrap();
    e.wrapped = other.wrapped;
    assert!(crypto::open(&b.key, &e).is_err());
    let g = Fixture::new().await;
    let mut wrong = g.bootstrap.load_existing().await.unwrap();
    wrong.metadata = b.key.metadata.clone();
    assert!(crypto::open(&wrong, &encrypted(&f, &b).await).is_err());
}
#[tokio::test]
async fn ciphertext_and_wrapped_key_truncation_are_rejected() {
    let f = Fixture::new().await;
    let b = ready(&f).await;
    let original = encrypted(&f, &b).await;
    for size in [0, 1, 15, 16, 47] {
        let mut e = original.clone();
        e.wrapped.truncate(size);
        assert!(crypto::open(&b.key, &e).is_err());
    }
    for size in [0, 1, 15] {
        let mut e = original.clone();
        e.ciphertext.truncate(size);
        assert!(crypto::open(&b.key, &e).is_err());
    }
}
#[tokio::test]
async fn empty_and_maximum_values_encrypt_and_validate_without_overflow() {
    let f = Fixture::new().await;
    let b = f.broker("project-a").await;
    b.create_connection(&scope("project-a"), &admin("conn"))
        .await
        .unwrap();
    for (i, size) in [0, MAX_SECRET_BYTES].into_iter().enumerate() {
        let mut r = put("project-a", &format!("size-{i}"));
        r.scope.record = RecordId::new(format!("size-{i}")).unwrap();
        let bytes = SecretValue::new(vec![b'x'; size]).unwrap();
        b.put_secret(&r, bytes).await.unwrap();
        let mut c = f.store.pool.acquire().await.unwrap();
        let e = store::load(&mut c, &r.scope, SecretVersion(1))
            .await
            .unwrap();
        let decoded = crypto::open(&b.key, &e).unwrap();
        assert_eq!(decoded.0.len(), size);
        assert!(decoded.0.iter().all(|v| *v == b'x'));
    }
    assert_eq!(
        SecretValue::new(vec![b'x'; MAX_SECRET_BYTES + 1]).unwrap_err(),
        BrokerError::TooLarge
    );
}
#[test]
fn invalid_identifiers_and_versions_are_rejected() {
    for s in ["", "../key", "a/b", "A", "a\n", "one two"] {
        assert!(ConnectionId::new(s).is_err());
        assert!(RecordId::new(s).is_err());
        assert!(SecretPurpose::new(s).is_err());
    }
    assert!(AccountId::new("a".repeat(65)).is_err());
    assert!(SecretVersion::new(0).is_err());
    assert!(SecretVersion::new(u64::MAX).is_err());
}
#[tokio::test]
async fn entropy_failure_at_every_draw_persists_no_partial_secret() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    for failed in 0..3 {
        let f = Fixture::new().await;
        let mut b = f.broker("project-a").await;
        b.create_connection(&scope("project-a"), &admin("conn"))
            .await
            .unwrap();
        let n = AtomicUsize::new(0);
        b.entropy = Box::new(move |bytes| {
            if n.fetch_add(1, Ordering::SeqCst) == failed {
                // Even a partially written buffer must never become usable material.
                let partial = bytes.len() / 2;
                bytes[..partial].fill(0xa5);
                Err(BrokerError::Randomness)
            } else {
                getrandom::fill(bytes).map_err(|_| BrokerError::Randomness)
            }
        });
        assert_eq!(
            b.put_secret(&put("project-a", "put"), value())
                .await
                .unwrap_err(),
            BrokerError::Randomness
        );
        for table in ["secret_records", "secret_versions", "secret_requests"] {
            let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
                .fetch_one(&f.store.pool)
                .await
                .unwrap();
            assert_eq!(n, if table == "secret_requests" { 1 } else { 0 });
        }
    }
}
#[tokio::test]
async fn real_insert_failure_after_encryption_rolls_back_head_and_audit() {
    let f = Fixture::new().await;
    let b = f.broker("project-a").await;
    b.create_connection(&scope("project-a"), &admin("conn"))
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER injected_write_failure BEFORE INSERT ON secret_versions BEGIN SELECT RAISE(ABORT,'synthetic storage failure'); END").execute(&f.store.pool).await.unwrap();
    assert_eq!(
        b.put_secret(&put("project-a", "put"), value())
            .await
            .unwrap_err(),
        BrokerError::Storage
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_records")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_events WHERE action=3")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        0
    );
}
#[tokio::test]
async fn duplicate_creates_conflicting_payloads_and_replacement_replays() {
    let f = Fixture::new().await;
    let b = f.broker("project-a").await;
    let conn = b
        .create_connection(&scope("project-a"), &admin("conn"))
        .await
        .unwrap();
    assert_eq!(
        b.create_connection(&scope("project-a"), &admin("conn"))
            .await
            .unwrap(),
        conn
    );
    let r = put("project-a", "put");
    let first = b.put_secret(&r, value()).await.unwrap();
    assert_eq!(b.put_secret(&r, value()).await.unwrap(), first);
    assert_eq!(
        b.put_secret(
            &r,
            SecretValue::new(b"different synthetic content".to_vec()).unwrap()
        )
        .await
        .unwrap_err(),
        BrokerError::ReplayConflict
    );
    let mut next = put("project-a", "replace");
    next.expected = Some(SecretVersion(1));
    let second = b.put_secret(&next, value()).await.unwrap();
    assert_eq!(second.version, SecretVersion(2));
    assert_eq!(b.put_secret(&next, value()).await.unwrap(), second);
    assert_eq!(
        b.put_secret(&r, value()).await.unwrap_err(),
        BrokerError::StaleVersion
    );
    assert_eq!(
        b.use_secret(&use_request("project-a", "old-version"))
            .await
            .unwrap_err(),
        BrokerError::StaleVersion
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_versions")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        2
    );
}
#[tokio::test]
async fn revocation_is_idempotent_and_no_receipt_or_replace_can_resurrect_it() {
    let f = Fixture::new().await;
    let b = ready(&f).await;
    let r = use_request("project-a", "use");
    b.use_secret(&r).await.unwrap();
    let receipt = b
        .revoke_connection(&scope("project-a"), SecretVersion(1), &admin("revoke"))
        .await
        .unwrap();
    assert_eq!(
        b.revoke_connection(&scope("project-a"), SecretVersion(1), &admin("revoke"))
            .await
            .unwrap(),
        receipt
    );
    assert_eq!(b.use_secret(&r).await.unwrap_err(), BrokerError::Revoked);
    let mut p = put("project-a", "replace");
    p.expected = Some(SecretVersion(1));
    assert_eq!(
        b.put_secret(&p, value()).await.unwrap_err(),
        BrokerError::Revoked
    );
    assert!(
        b.metadata(&record("project-a"), &admin("metadata"))
            .await
            .unwrap()
            .revoked
    );
}
#[tokio::test]
async fn concurrent_creation_of_same_scope_has_one_active_version() {
    let f = Fixture::new().await;
    let a = f.broker("project-a").await;
    let b = f.broker("project-a").await;
    a.create_connection(&scope("project-a"), &admin("conn"))
        .await
        .unwrap();
    let r1 = put("project-a", "first");
    let r2 = put("project-a", "second");
    let (x, y) = tokio::join!(a.put_secret(&r1, value()), b.put_secret(&r2, value()));
    assert_eq!(usize::from(x.is_ok()) + usize::from(y.is_ok()), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_versions")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        1
    );
}
#[tokio::test]
async fn concurrent_replacement_and_revocation_never_reactivates_connection() {
    let f = Fixture::new().await;
    let a = ready(&f).await;
    let b = f.broker("project-a").await;
    let mut p = put("project-a", "replace");
    p.expected = Some(SecretVersion(1));
    let connection = scope("project-a");
    let revoke = admin("revoke");
    let (x, y) = tokio::join!(
        a.put_secret(&p, value()),
        b.revoke_connection(&connection, SecretVersion(1), &revoke)
    );
    assert!(y.is_ok());
    assert!(x.is_ok() || x == Err(BrokerError::Revoked));
    let mut r = use_request("project-a", "use");
    r.version = if x.is_ok() {
        SecretVersion(2)
    } else {
        SecretVersion(1)
    };
    assert_eq!(a.use_secret(&r).await.unwrap_err(), BrokerError::Revoked);
}
#[tokio::test]
async fn restart_revoke_isolation_and_matching_snapshot_restore_end_to_end() {
    let f = Fixture::new().await;
    let a = ready(&f).await;
    let b = f.broker("project-b").await;
    b.create_connection(&scope("project-b"), &admin("connection-b"))
        .await
        .unwrap();
    b.put_secret(
        &put("project-b", "put-b"),
        SecretValue::new(b"synthetic-project-b-private-value".to_vec()).unwrap(),
    )
    .await
    .unwrap();
    b.use_secret(&use_request("project-b", "use-b"))
        .await
        .unwrap();
    assert_eq!(
        a.use_secret(&use_request("project-b", "cross-project"))
            .await
            .unwrap_err(),
        BrokerError::Forbidden
    );
    let original_envelope = encrypted(&f, &a).await;
    let dek = crypto::unwrap(&a.key, &original_envelope).unwrap();
    let original = original_envelope.ciphertext;
    // Close every authoritative pool and reconstruct both key and broker authority.
    f.store.close().await;
    let reopened = PlanStore::open(&f.path).await.unwrap();
    let bootstrap = ControllerBootstrap::open(
        reopened.clone(),
        &f.keys,
        super::super::ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    assert_eq!(
        bootstrap.load_existing().await.unwrap().metadata(),
        a.key.metadata()
    );
    let mut restarted = SecretBroker::open(&bootstrap).await.unwrap();
    restarted.human = a.human.clone();
    restarted.user = a.user.clone();
    restarted.entitlement = a.entitlement.clone();
    restarted.consumer = Arc::new(SyntheticConsumer);
    restarted
        .use_secret(&use_request("project-a", "after-restart"))
        .await
        .unwrap();
    restarted
        .revoke_connection(&scope("project-a"), SecretVersion(1), &admin("revoke"))
        .await
        .unwrap();
    assert_eq!(
        restarted
            .use_secret(&use_request("project-a", "revoked"))
            .await
            .unwrap_err(),
        BrokerError::Revoked
    );
    let snapshot = f.path.parent().unwrap().join("snapshot.sqlite");
    reopened.snapshot(&snapshot).await.unwrap();
    for path in [
        &f.path,
        &snapshot,
        &PathBuf::from(format!("{}-wal", f.path.display())),
    ] {
        if path.exists() {
            let bytes = fs::read(path).unwrap();
            for marker in [
                b"synthetic-confidential-value".as_slice(),
                b"synthetic-project-b-private-value".as_slice(),
                restarted.key.material.as_slice(),
                dek.as_slice(),
            ] {
                assert!(!bytes.windows(marker.len()).any(|w| w == marker));
            }
        }
    }
    let isolated = temporary();
    let data = isolated.path().join("data");
    let keys = isolated.path().join("keys");
    for p in [&data, &keys] {
        fs::create_dir(p).unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let restored_path = data.join("state.sqlite");
    fs::copy(&snapshot, &restored_path).unwrap();
    fs::copy(f.keys.join("controller.kek"), keys.join("controller.kek")).unwrap();
    let restored = PlanStore::open(&restored_path).await.unwrap();
    restored.check_integrity().await.unwrap();
    let rb = ControllerBootstrap::open(
        restored.clone(),
        &keys,
        super::super::ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let mut broker = SecretBroker::open(&rb).await.unwrap();
    broker.human = b.human.clone();
    broker.user = b.user.clone();
    broker.entitlement = b.entitlement.clone();
    broker.consumer = Arc::new(SyntheticConsumer);
    broker
        .use_secret(&use_request("project-b", "restored-b"))
        .await
        .unwrap();
    let restored_cipher: Vec<u8> =
        sqlx::query_scalar("SELECT ciphertext FROM secret_versions WHERE project_id='project-a'")
            .fetch_one(&restored.pool)
            .await
            .unwrap();
    assert!(restored_cipher == original);
    let wrong = Fixture::new().await;
    let bad = ControllerBootstrap::open(
        restored.clone(),
        &wrong.keys,
        super::super::ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    assert!(matches!(
        SecretBroker::open(&bad).await,
        Err(BrokerError::KeyUnavailable)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_versions")
            .fetch_one(&restored.pool)
            .await
            .unwrap(),
        2
    );
    reopened.close().await;
    restored.close().await;
}
#[tokio::test]
async fn separate_accounts_in_one_project_require_separate_scoped_authority() {
    let f = Fixture::new().await;
    let a = ready(&f).await;
    let mut other = scope("project-a");
    other.connection = ConnectionId::new("account-b-connection").unwrap();
    other.account = AccountId::new("account-b").unwrap();
    let mut b = f.broker("project-a").await;
    let auth = Arc::new(SyntheticAuthority {
        scope: other.clone(),
        actions: vec![
            BrokerAction::CreateConnection,
            BrokerAction::Store,
            BrokerAction::Use,
        ],
    });
    b.human = auth.clone();
    b.user = auth.clone();
    b.entitlement = auth;
    b.create_connection(&other, &admin("other-conn"))
        .await
        .unwrap();
    let mut p = put("project-a", "other-put");
    p.scope.connection = other;
    b.put_secret(&p, value()).await.unwrap();
    let mut r = use_request("project-a", "cross-account");
    r.scope = p.scope;
    assert_eq!(a.use_secret(&r).await.unwrap_err(), BrokerError::Forbidden);
    assert!(b.use_secret(&r).await.is_ok());
    assert!(
        a.use_secret(&use_request("project-a", "original"))
            .await
            .is_ok()
    );
}
#[tokio::test]
async fn key_loss_and_corruption_prevent_broker_restart_without_resetting_records() {
    for corrupt in [false, true] {
        let f = Fixture::new().await;
        let _ = ready(&f).await;
        if corrupt {
            let mut bytes = Zeroizing::new(fs::read(f.keys.join("controller.kek")).unwrap());
            bytes[super::super::HEADER] ^= 1;
            fs::write(f.keys.join("controller.kek"), bytes.as_slice()).unwrap();
        } else {
            fs::remove_file(f.keys.join("controller.kek")).unwrap();
        }
        assert!(matches!(
            SecretBroker::open(&f.bootstrap).await,
            Err(BrokerError::KeyUnavailable)
        ));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_versions")
                .fetch_one(&f.store.pool)
                .await
                .unwrap(),
            1
        );
    }
}
#[tokio::test]
async fn sql_guards_prevent_metadata_ciphertext_and_history_mutation() {
    let f = Fixture::new().await;
    let _ = ready(&f).await;
    for sql in [
        "UPDATE secret_connections SET account_id='other'",
        "UPDATE secret_versions SET ciphertext=zeroblob(32)",
        "UPDATE secret_records SET purpose='other'",
        "DELETE FROM secret_versions",
        "DELETE FROM secret_events",
        "DELETE FROM secret_requests",
        "UPDATE secret_records SET current_version=99",
    ] {
        assert!(sqlx::query(sql).execute(&f.store.pool).await.is_err());
    }
    f.store.check_integrity().await.unwrap();
}
#[tokio::test]
async fn request_foreign_keys_cannot_link_other_tenants_audit() {
    let f = Fixture::new().await;
    let _ = ready(&f).await;
    assert!(sqlx::query("INSERT INTO secret_requests SELECT 'project-b',request_id,commitment,commitment_key_id,commitment_key_version,event_sequence,result_version FROM secret_requests LIMIT 1").execute(&f.store.pool).await.is_err());
}
#[tokio::test]
async fn corrupted_record_nonce_cipher_and_metadata_fail_without_panics() {
    for alteration in [
        "data_nonce=substr(data_nonce,1,23)",
        "ciphertext=x'01'",
        "envelope_version=99",
        "cipher=99",
        "account_id=CAST(x'ff' AS TEXT)",
        "account_id=printf('%01000000d',1)",
    ] {
        let f = Fixture::new().await;
        let b = ready(&f).await;
        let mut c = f.store.pool.acquire().await.unwrap();
        sqlx::query("DROP TRIGGER secret_version_no_update")
            .execute(&mut *c)
            .await
            .unwrap();
        sqlx::query("PRAGMA ignore_check_constraints=ON")
            .execute(&mut *c)
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys=OFF")
            .execute(&mut *c)
            .await
            .unwrap();
        sqlx::query(&format!("UPDATE secret_versions SET {alteration}"))
            .execute(&mut *c)
            .await
            .unwrap();
        drop(c);
        assert!(
            b.verify_integrity(&record("project-a"), SecretVersion(1), &admin("corrupted"))
                .await
                .is_err()
        );
    }
}
#[tokio::test]
async fn cryptographic_integrity_failure_is_redacted_and_audited() {
    let f = Fixture::new().await;
    let b = ready(&f).await;
    let mut e = encrypted(&f, &b).await;
    e.ciphertext[0] ^= 1;
    sqlx::query("DROP TRIGGER secret_version_no_update")
        .execute(&f.store.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE secret_versions SET ciphertext=?")
        .bind(&e.ciphertext)
        .execute(&f.store.pool)
        .await
        .unwrap();
    assert_eq!(
        b.verify_integrity(&record("project-a"), SecretVersion(1), &admin("corrupt"))
            .await
            .unwrap_err(),
        BrokerError::Integrity
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM secret_events WHERE outcome='integrity_failed'"
        )
        .fetch_one(&f.store.pool)
        .await
        .unwrap(),
        1
    );
    let text = serde_json::to_string(&BrokerError::Integrity.to_string()).unwrap();
    assert!(!text.contains("synthetic-confidential-value"));
    assert!(!text.contains(f.root.path().to_str().unwrap()));
}
#[tokio::test]
async fn replays_recheck_current_entitlement_and_authority_without_second_consumption() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Count(Arc<AtomicUsize>);
    impl TrustedConsumer for Count {
        fn consume(&self, _: &SecretUseRequest, v: &SecretValue) -> Result<(), BrokerError> {
            assert!(v.0.as_slice() == b"synthetic-confidential-value");
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    let f = Fixture::new().await;
    let mut b = ready(&f).await;
    let n = Arc::new(AtomicUsize::new(0));
    b.consumer = Arc::new(Count(n.clone()));
    let r = use_request("project-a", "use");
    let original = b.use_secret(&r).await.unwrap();
    assert_eq!(b.use_secret(&r).await.unwrap(), original);
    assert_eq!(n.load(Ordering::SeqCst), 1);
    b.entitlement = Arc::new(Unimplemented);
    assert_eq!(
        b.use_secret(&r).await.unwrap_err(),
        BrokerError::UnavailableAuthority
    );
    assert_eq!(n.load(Ordering::SeqCst), 1);
    b.user = Arc::new(Unimplemented);
    assert_eq!(
        b.use_secret(&r).await.unwrap_err(),
        BrokerError::UnavailableAuthority
    );
}
#[tokio::test]
async fn concurrent_revocation_and_use_linearize_without_plaintext_escape() {
    let f = Fixture::new().await;
    let a = ready(&f).await;
    let b = f.broker("project-a").await;
    let r = use_request("project-a", "use");
    let connection = scope("project-a");
    let revoke = admin("revoke");
    let (u, v) = tokio::join!(
        a.use_secret(&r),
        b.revoke_connection(&connection, SecretVersion(1), &revoke)
    );
    assert!(v.is_ok());
    assert!(u.is_ok() || u == Err(BrokerError::Revoked));
    assert_eq!(a.use_secret(&r).await.unwrap_err(), BrokerError::Revoked);
}
#[tokio::test]
async fn bounded_sqlite_contention_returns_busy_and_no_secret_authority() {
    let f = Fixture::new().await;
    let b = f.broker("project-a").await;
    b.create_connection(&scope("project-a"), &admin("conn"))
        .await
        .unwrap();
    let blocker = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    // Connection-local short timeout on every remaining pooled writer, never a
    // global PRAGMA or a timing-dependent sleep.
    let mut held = Vec::new();
    for _ in 0..3 {
        let mut c = f.store.pool.acquire().await.unwrap();
        sqlx::query("PRAGMA busy_timeout=1")
            .execute(&mut *c)
            .await
            .unwrap();
        held.push(c);
    }
    drop(held);
    assert_eq!(
        b.put_secret(&put("project-a", "blocked"), value())
            .await
            .unwrap_err(),
        BrokerError::Busy
    );
    blocker.rollback().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_versions")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        0
    );
}
use crate::resilience::{common, synthetic};
async fn admitted_worker(
    f: &Fixture,
) -> (
    SecretBroker,
    SecretUseRequest,
    crate::admission::LeaseRef,
    Arc<crate::worker_simulation::DeterministicClock>,
) {
    use crate::{finance::*, orchestration::*, worker_control::*, worker_simulation::*};
    use rolvya_plan_validator::Limits;
    let mut content = common::plan();
    content.tasks[0].acceptance_criteria = vec![ARTIFACT_CRITERION.into()];
    let authority = synthetic::Authority::default();
    let validated =
        rolvya_plan_validator::validate(&content, &authority, Limits::default()).unwrap();
    f.store
        .create_project(&content.key.project_id, &common::owner())
        .await
        .unwrap();
    let revision = f
        .store
        .save_draft(&validated, None, &common::owner(), &common::OwnerPolicy)
        .await
        .unwrap();
    let fixture = common::Fixture {
        directory: temporary(),
        path: f.path.clone(),
        store: f.store.clone(),
        content,
        revision,
    };
    fixture.activate().await.unwrap();
    let now = SystemClock.now().unwrap();
    f.store
        .apply_financial(
            &FinancialRequest {
                project: fixture.content.key.project_id.clone(),
                request_id: RequestId::new("worker-budget").unwrap(),
                actor: common::owner(),
                reason: "Explicit scoped synthetic budget; no provider".into(),
                command: FinancialCommand::Approve {
                    expected_version: 0,
                    new_ceiling: synthetic::money(100),
                    proof: OwnerProof {
                        reference: synthetic::e("synthetic-only-budget-proof"),
                        verified_at: now,
                    },
                },
            },
            &synthetic::Finance,
        )
        .await
        .unwrap();
    let mut b = f.broker("project").await;
    b.create_connection(&scope("project"), &admin("worker-connection"))
        .await
        .unwrap();
    b.put_secret(&put("project", "worker-secret"), value())
        .await
        .unwrap();
    let registry =
        crate::controller_keys::provider_accounts::fixture_registry(&f.bootstrap, synthetic::T)
            .await;
    let authority = registry.policy(authority);
    let co = synthetic::open(&fixture, &authority).await;
    let clock = Arc::new(DeterministicClock::new(synthetic::T).unwrap());
    for offset in 0..=2 {
        clock.advance_to(synthetic::T + offset).unwrap();
        co.pass(
            &authority,
            clock.as_ref(),
            PassLimits {
                admissions: 1,
                events: 16,
                validations: 0,
                groups: 0,
            },
        )
        .await
        .unwrap();
    }
    let lease = synthetic::lease(&fixture, "task-a").await;
    let binding = DurableWorkerController::new(f.store.clone())
        .binding(&lease)
        .await
        .unwrap();
    b.policy = Arc::new(registry.policy(synthetic::Authority::default()));
    b.worker = Arc::new(synthetic::Authority::default());
    b.clock = clock.clone();

    let mut r = use_request("project", "worker-use");
    r.operation = CredentialOperation::ProxyModelRequest;
    r.worker = Some(AttemptSecretScope {
        peer: PeerEvidence {
            reference: synthetic::peer_ref(&binding),
        },
        binding,
    });
    (b, r, lease, clock)
}
#[tokio::test]
async fn admitted_worker_uses_real_reservation_and_stale_fence_blocks_replayed_receipt() {
    let f = Fixture::new().await;
    let (b, r, lease, clock) = admitted_worker(&f).await;
    let before = f
        .store
        .financial_snapshot(&scope("project").project)
        .await
        .unwrap();
    let receipt = b.use_secret(&r).await.unwrap();
    assert_eq!(b.use_secret(&r).await.unwrap(), receipt);
    f.store
        .fence_attempt(
            &FenceRequest {
                lease,
                request_id: RequestId::new("fence").unwrap(),
                actor: common::owner(),
                evidence: synthetic::e("synthetic-fence"),
            },
            &synthetic::Authority::default(),
            clock.as_ref(),
        )
        .await
        .unwrap();
    assert_eq!(
        b.use_secret(&r).await.unwrap_err(),
        BrokerError::AttemptAuthority
    );
    let after = f
        .store
        .financial_snapshot(&scope("project").project)
        .await
        .unwrap();
    assert_eq!(before.held, after.held);
    assert_eq!(before.ceiling, after.ceiling);
}
#[tokio::test]
async fn unknown_financial_exposure_blocks_new_and_duplicate_credential_use() {
    use crate::finance::*;
    let f = Fixture::new().await;
    let (b, r, _, _) = admitted_worker(&f).await;
    b.use_secret(&r).await.unwrap();
    f.store
        .apply_financial(
            &FinancialRequest {
                project: scope("project").project,
                request_id: RequestId::new("unknown").unwrap(),
                actor: common::owner(),
                reason: "Synthetic unknown exposure".into(),
                command: FinancialCommand::Report {
                    liability: LiabilityId::new("unknown-provider-operation").unwrap(),
                    expected_revision: 0,
                    unreserved: synthetic::zero(),
                    uncertain: true,
                    evidence: synthetic::e("synthetic-uncertain"),
                },
            },
            &synthetic::Finance,
        )
        .await
        .unwrap();
    assert_eq!(
        b.use_secret(&r).await.unwrap_err(),
        BrokerError::AttemptAuthority
    );
    let mut new = r.clone();
    new.request_id = RequestId::new("new-use").unwrap();
    assert_eq!(
        b.use_secret(&new).await.unwrap_err(),
        BrokerError::AttemptAuthority
    );
}
#[tokio::test]
async fn expired_unverified_and_cross_attempt_worker_requests_are_rejected() {
    let f = Fixture::new().await;
    let (b, r, _, clock) = admitted_worker(&f).await;
    for i in 0..5 {
        let mut invalid = r.clone();
        let w = invalid.worker.as_mut().unwrap();
        match i {
            0 => w.binding.worker_id = rolvya_domain::ids::WorkerId::new("impersonator").unwrap(),
            1 => w.binding.epoch += 1,
            2 => w.binding.task.plan.project_id = ProjectId::new("project-b").unwrap(),
            3 => w.peer.reference = synthetic::e("unverified-peer"),
            _ => {
                w.binding.attempt_id =
                    rolvya_domain::ids::AttemptId::new("unrelated-attempt").unwrap()
            }
        }
        assert_eq!(
            b.use_secret(&invalid).await.unwrap_err(),
            BrokerError::AttemptAuthority
        );
    }
    clock.advance_to(synthetic::T + 91).unwrap();
    assert_eq!(
        b.use_secret(&r).await.unwrap_err(),
        BrokerError::AttemptAuthority
    );
}
#[tokio::test]
async fn canonical_aad_has_unambiguous_lengths_and_domain_separation() {
    let f = Fixture::new().await;
    let b = ready(&f).await;
    let mut a = encrypted(&f, &b).await;
    let mut z = a.clone();
    a.scope.connection.project = ProjectId::new("ab").unwrap();
    a.scope.connection.account = AccountId::new("c").unwrap();
    z.scope.connection.project = ProjectId::new("a").unwrap();
    z.scope.connection.account = AccountId::new("bc").unwrap();
    assert!(a.data_aad() != z.data_aad());
    assert!(a.data_aad() != a.wrap_aad());
    assert!(a.wrap_aad().len() < 1024);
}
#[tokio::test]
async fn wrapping_nonce_uniqueness_is_enforced_by_sqlite() {
    let f = Fixture::new().await;
    let b = ready(&f).await;
    let e = encrypted(&f, &b).await;
    assert!(sqlx::query("INSERT INTO secret_versions SELECT project_id,connection_id,record_id,purpose,account_id,provider,mode,2,envelope_version,cipher,installation_id,origin_key_id,origin_key_version,wrap_key_id,wrap_key_version,data_nonce,wrap_nonce,wrapped_dek,ciphertext FROM secret_versions WHERE version=1").execute(&f.store.pool).await.is_err());
    assert!(
        b.verify_integrity(
            &record("project-a"),
            SecretVersion(1),
            &admin("still-valid")
        )
        .await
        .is_ok()
    );
    let after = encrypted(&f, &b).await;
    assert!(e.ciphertext == after.ciphertext);
}
#[tokio::test]
async fn synthetic_internal_encryption_workload_is_bounded_and_measured() {
    let f = Fixture::new().await;
    let b = f.broker("project-a").await;
    b.create_connection(&scope("project-a"), &admin("conn"))
        .await
        .unwrap();
    let start = std::time::Instant::now();
    for i in 0..32 {
        let mut p = put("project-a", &format!("put-{i}"));
        p.scope.record = RecordId::new(format!("record-{i}")).unwrap();
        b.put_secret(&p, value()).await.unwrap();
        let mut r = use_request("project-a", &format!("use-{i}"));
        r.scope = p.scope;
        b.use_secret(&r).await.unwrap();
    }
    println!(
        "S2-02 synthetic 32 encrypt/store/use operations: {:?}; Linux debug profile, disposable SQLite, no network",
        start.elapsed()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_versions")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        32
    );
}
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};
struct Victim(Child);
impl Drop for Victim {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn crash_command(f: &Fixture, at: Checkpoint) -> Command {
    let mut c = Command::new(std::env::current_exe().unwrap());
    c.args([
        "--exact",
        "controller_keys::secret_broker::tests::subprocess_driver",
        "--nocapture",
    ])
    .env("ROLVYA_S202_TEST_ROOT", f.root.path())
    .env("ROLVYA_S202_TEST_CHECKPOINT", format!("{at:?}"))
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    c
}
#[tokio::test]
async fn subprocess_driver() {
    let Ok(root) = std::env::var("ROLVYA_S202_TEST_ROOT") else {
        return;
    };
    let store = PlanStore::open(Path::new(&root).join("data/state.sqlite"))
        .await
        .unwrap();
    let bootstrap = ControllerBootstrap::open(
        store.clone(),
        &Path::new(&root).join("keys"),
        super::super::ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let mut b = SecretBroker::open(&bootstrap).await.unwrap();
    let auth = Arc::new(SyntheticAuthority {
        scope: scope("project-a"),
        actions: vec![BrokerAction::Store],
    });
    b.human = auth;
    b.hook = Box::new(|at| {
        if std::env::var("ROLVYA_S202_TEST_CHECKPOINT").unwrap() == format!("{at:?}") {
            println!("S202_READY {at:?}");
            std::io::stdout().flush().unwrap();
            loop {
                std::thread::park();
            }
        }
        Ok(())
    });
    b.put_secret(&put("project-a", "crash-store"), value())
        .await
        .unwrap();
}
#[tokio::test]
async fn actual_sigkill_before_and_after_encryption_and_commit_preserves_authority() {
    for checkpoint in [
        Checkpoint::BeforeEncryption,
        Checkpoint::AfterEncryption,
        Checkpoint::BeforeCommit,
        Checkpoint::AfterCommit,
    ] {
        let f = Fixture::new().await;
        let b = f.broker("project-a").await;
        b.create_connection(&scope("project-a"), &admin("conn"))
            .await
            .unwrap();
        drop(b);
        f.store.close().await;
        let mut child = crash_command(&f, checkpoint).spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                if line.starts_with("S202_READY") {
                    let _ = tx.send(line);
                }
            }
        });
        let mut victim = Victim(child);
        assert!(rx.recv_timeout(Duration::from_secs(30)).is_ok());
        victim.0.kill().unwrap();
        victim.0.wait().unwrap();
        let reopened = PlanStore::open(&f.path).await.unwrap();
        reopened.check_integrity().await.unwrap();
        let bootstrap = ControllerBootstrap::open(
            reopened.clone(),
            &f.keys,
            super::super::ServiceIdentity::current(),
            &[],
        )
        .await
        .unwrap();
        let mut broker = SecretBroker::open(&bootstrap).await.unwrap();
        let auth = Arc::new(SyntheticAuthority {
            scope: scope("project-a"),
            actions: vec![BrokerAction::Store, BrokerAction::Use],
        });
        broker.human = auth.clone();
        broker.user = auth.clone();
        broker.entitlement = auth;
        broker.consumer = Arc::new(SyntheticConsumer);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM secret_versions")
            .fetch_one(&reopened.pool)
            .await
            .unwrap();
        if checkpoint == Checkpoint::AfterCommit {
            assert_eq!(count, 1);
            broker.entropy = Box::new(|_| Err(BrokerError::Randomness));
            let old = broker
                .put_secret(&put("project-a", "crash-store"), value())
                .await
                .unwrap();
            assert_eq!(
                broker
                    .put_secret(&put("project-a", "crash-store"), value())
                    .await
                    .unwrap(),
                old
            );
            broker
                .use_secret(&use_request("project-a", "use-after-crash"))
                .await
                .unwrap();
        } else {
            assert_eq!(count, 0);
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_records")
                    .fetch_one(&reopened.pool)
                    .await
                    .unwrap(),
                0
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_events WHERE action=3")
                    .fetch_one(&reopened.pool)
                    .await
                    .unwrap(),
                0
            );
        }
        reopened.close().await;
    }
}
#[tokio::test]
async fn version_six_upgrade_and_failed_seven_preserve_existing_core_and_custody() {
    for inject_failure in [false, true] {
        let f = Fixture::new().await;
        let path = f.path.parent().unwrap().join("legacy-v6.sqlite");
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
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
        let scripts = f.root.path().join("migration-scripts");
        fs::create_dir(&scripts).unwrap();
        for entry in fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations")).unwrap()
        {
            let e = entry.unwrap();
            if e.file_name().to_string_lossy().as_ref() < "0007" {
                fs::copy(e.path(), scripts.join(e.file_name())).unwrap();
            }
        }
        sqlx::migrate::Migrator::new(scripts.as_path())
            .await
            .unwrap()
            .run(&raw)
            .await
            .unwrap();
        sqlx::query("INSERT INTO projects(project_id,owner_actor) VALUES('legacy','owner')")
            .execute(&raw)
            .await
            .unwrap();
        if inject_failure {
            fs::write(
                scripts.join("0007_secret_broker.sql"),
                format!(
                    "{}\nSELECT missing FROM absent_table;",
                    include_str!("../../../migrations/0007_secret_broker.sql")
                ),
            )
            .unwrap();
            let error = sqlx::migrate::Migrator::new(scripts.as_path())
                .await
                .unwrap()
                .run(&raw)
                .await
                .unwrap_err();
            assert!(matches!(
                error,
                sqlx::migrate::MigrateError::ExecuteMigration(_, 7)
            ));
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM sqlite_schema WHERE name='secret_connections'"
                )
                .fetch_one(&raw)
                .await
                .unwrap(),
                0
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM _sqlx_migrations WHERE success=1"
                )
                .fetch_one(&raw)
                .await
                .unwrap(),
                6
            );
        }
        raw.close().await;
        let upgraded = PlanStore::open(&path).await.unwrap();
        upgraded.check_integrity().await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT owner_actor FROM projects WHERE project_id='legacy'"
            )
            .fetch_one(&upgraded.pool)
            .await
            .unwrap(),
            "owner"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT state FROM controller_installation")
                .fetch_one(&upgraded.pool)
                .await
                .unwrap(),
            "recovery"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_versions")
                .fetch_one(&upgraded.pool)
                .await
                .unwrap(),
            0
        );
        assert!(f.bootstrap.load_existing().await.is_ok());
        upgraded.close().await;
    }
}
#[tokio::test]
async fn revoked_authority_during_encryption_rolls_back_and_records_denial() {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Revocable(Arc<AtomicBool>);
    impl HumanAuthority for Revocable {
        fn check(&self, _: &AdminContext<'_>) -> Result<(), BrokerError> {
            if self.0.load(Ordering::SeqCst) {
                Ok(())
            } else {
                Err(BrokerError::Forbidden)
            }
        }
    }
    let f = Fixture::new().await;
    let mut b = f.broker("project-a").await;
    b.create_connection(&scope("project-a"), &admin("conn"))
        .await
        .unwrap();
    let live = Arc::new(AtomicBool::new(true));
    b.human = Arc::new(Revocable(live.clone()));
    b.hook = Box::new(move |at| {
        if at == Checkpoint::AfterEncryption {
            live.store(false, Ordering::SeqCst);
        }
        Ok(())
    });
    assert_eq!(
        b.put_secret(&put("project-a", "revoked-store"), value())
            .await
            .unwrap_err(),
        BrokerError::Forbidden
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_versions")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_events WHERE outcome='denied'")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        1
    );
}
#[tokio::test]
async fn identical_concurrent_create_requests_return_one_original_receipt() {
    let f = Fixture::new().await;
    let a = f.broker("project-a").await;
    let b = f.broker("project-a").await;
    a.create_connection(&scope("project-a"), &admin("conn"))
        .await
        .unwrap();
    let p = put("project-a", "identical");
    let (x, y) = tokio::join!(a.put_secret(&p, value()), b.put_secret(&p, value()));
    assert_eq!(x.unwrap(), y.unwrap());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_versions")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM secret_events WHERE action=3")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        1
    );
}
#[tokio::test]
async fn ciphertext_copied_between_sqlite_projects_fails_authentication() {
    let f = Fixture::new().await;
    let a = ready(&f).await;
    let b = f.broker("project-b").await;
    b.create_connection(&scope("project-b"), &admin("b-conn"))
        .await
        .unwrap();
    b.put_secret(
        &put("project-b", "b-put"),
        SecretValue::new(b"synthetic-project-b-private-value".to_vec()).unwrap(),
    )
    .await
    .unwrap();
    let ciphertext = encrypted(&f, &a).await.ciphertext;
    sqlx::query("DROP TRIGGER secret_version_no_update")
        .execute(&f.store.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE secret_versions SET ciphertext=? WHERE project_id='project-b'")
        .bind(ciphertext)
        .execute(&f.store.pool)
        .await
        .unwrap();
    assert_eq!(
        b.verify_integrity(
            &record("project-b"),
            SecretVersion(1),
            &admin("substituted")
        )
        .await
        .unwrap_err(),
        BrokerError::Integrity
    );
    assert!(
        a.use_secret(&use_request("project-a", "preserved"))
            .await
            .is_ok()
    );
}
#[tokio::test]
async fn nul_prefixed_oversized_metadata_is_rejected_before_decoding() {
    let f = Fixture::new().await;
    let _ = ready(&f).await;
    let mut c = f.store.pool.acquire().await.unwrap();
    sqlx::query("DROP TRIGGER secret_version_no_update")
        .execute(&mut *c)
        .await
        .unwrap();
    sqlx::query("PRAGMA ignore_check_constraints=ON")
        .execute(&mut *c)
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&mut *c)
        .await
        .unwrap();
    sqlx::query("UPDATE secret_versions SET account_id='a'||char(0)||printf('%01000000d',1)")
        .execute(&mut *c)
        .await
        .unwrap();
    assert_eq!(
        store::preflight(&mut c, &record("project-a"), SecretVersion(1))
            .await
            .unwrap_err(),
        BrokerError::Integrity
    );
}
#[tokio::test]
async fn lease_expiring_during_replay_cannot_return_old_success() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Expiring {
        reads: AtomicUsize,
        valid_reads: usize,
    }
    impl Clock for Expiring {
        fn now(&self) -> Result<i64, crate::admission::AdmissionError> {
            Ok(synthetic::T
                + if self.reads.fetch_add(1, Ordering::SeqCst) < self.valid_reads {
                    2
                } else {
                    91
                })
        }
    }
    let f = Fixture::new().await;
    let (mut b, r, _, _) = admitted_worker(&f).await;
    b.use_secret(&r).await.unwrap();
    b.clock = Arc::new(Expiring {
        reads: AtomicUsize::new(0),
        valid_reads: 1,
    });
    assert_eq!(
        b.use_secret(&r).await.unwrap_err(),
        BrokerError::AttemptAuthority
    );
    let mut new = r;
    new.request_id = RequestId::new("expires-before-use-commit").unwrap();
    // Initial authority, consumption authority and audit time are valid; the
    // final authority check expires after the receipt writes but before commit.
    b.clock = Arc::new(Expiring {
        reads: AtomicUsize::new(0),
        valid_reads: 3,
    });
    assert_eq!(
        b.use_secret(&new).await.unwrap_err(),
        BrokerError::AttemptAuthority
    );
    for query in [
        "SELECT count(*) FROM secret_requests WHERE request_id='expires-before-use-commit'",
        "SELECT count(*) FROM secret_events WHERE request_id='expires-before-use-commit' AND outcome='accepted'",
    ] {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(query)
                .fetch_one(&f.store.pool)
                .await
                .unwrap(),
            0
        );
    }
}

// Reuses real admission, reservation, dispatch, lease and broker guards in a shared
// disposable controller database. No provider or real entitlement is fabricated.
pub(in crate::controller_keys) async fn admitted_for_session_tests(
    store: PlanStore,
    path: PathBuf,
    keys: PathBuf,
) -> (
    SecretBroker,
    SecretUseRequest,
    crate::admission::LeaseRef,
    Arc<crate::worker_simulation::DeterministicClock>,
) {
    let bootstrap = ControllerBootstrap::open(
        store.clone(),
        &keys,
        super::super::ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let fixture = Fixture {
        root: temporary(),
        path,
        keys,
        store,
        bootstrap,
    };
    admitted_worker(&fixture).await
}
