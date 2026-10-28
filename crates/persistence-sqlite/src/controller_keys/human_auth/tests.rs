use super::*;
#[test]
fn email_validation_preserves_local_case_and_normalizes_domain() {
    assert_eq!(
        EmailAddress::parse("First.Last+tag@EXAMPLE.COM")
            .unwrap()
            .as_str(),
        "First.Last+tag@example.com"
    );
    for s in [
        "a..b@example.com",
        ".a@example.com",
        "a@-host.com",
        "a@host",
        "a@host..com",
        "a@b.com\r\n",
        "a@@host.com",
        "",
        "a@høst.com",
    ] {
        assert!(EmailAddress::parse(s).is_err(), "invalid address accepted");
    }
}

use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicI64, Ordering},
    },
};
#[derive(Default)]
struct SyntheticAuthority(Mutex<BTreeMap<String, Grant>>);
struct Grant {
    action: AdminAction,
    target: Option<String>,
    payload: [u8; 32],
    evidence: String,
    until: i64,
    verified: bool,
}
impl EnrollmentAuthority for SyntheticAuthority {
    fn check(&self, c: &AdminContext<'_>) -> Result<EmailEvidence, AuthError> {
        let grants = self.0.lock().unwrap();
        let g = grants
            .get(c.request.request_id.as_str())
            .ok_or(AuthError::Denied)?;
        if g.action != c.action
            || g.target.as_deref() != c.target
            || g.payload != c.payload
            || g.evidence != c.request.evidence.as_str()
            || c.now >= g.until
        {
            return Err(AuthError::Denied);
        }
        Ok(if g.verified {
            EmailEvidence::IndependentlyVerified
        } else {
            EmailEvidence::Pending
        })
    }
}
type TransportGrant = (String, VerifiedTransport, i64, Vec<TransportOperation>);
#[derive(Default)]
struct SyntheticTransport(Mutex<BTreeMap<String, TransportGrant>>);
impl TransportAuthority for SyntheticTransport {
    fn verify(
        &self,
        c: &ClientContext,
        op: TransportOperation,
        now: i64,
    ) -> Result<VerifiedTransport, AuthError> {
        let map = self.0.lock().unwrap();
        let (request, peer, until, operations) = map
            .get(c.transport_evidence.as_str())
            .ok_or(AuthError::Denied)?;
        if request != c.request_id.as_str() || now >= *until || !operations.contains(&op) {
            return Err(AuthError::Denied);
        }
        Ok(peer.clone())
    }
}
#[derive(Default)]
struct SyntheticMail {
    messages: Mutex<BTreeMap<String, (String, EmailAddress)>>,
    verified: Mutex<BTreeMap<String, bool>>,
    mode: AtomicI64,
}
impl MailTransport for SyntheticMail {
    fn ready(&self) -> bool {
        self.mode.load(Ordering::SeqCst) != 4
    }
    fn send<'a>(&'a self, m: &'a MailMessage) -> MailFuture<'a> {
        Box::pin(async move {
            self.messages.lock().unwrap().insert(
                m.challenge.clone(),
                (m.token.0.to_string(), m.recipient.clone()),
            );
            match self.mode.load(Ordering::SeqCst) {
                0 => {
                    self.verified
                        .lock()
                        .unwrap()
                        .insert(m.challenge.clone(), true);
                    DeliveryObservation::Delivered
                }
                1 => DeliveryObservation::Accepted,
                2 => DeliveryObservation::Uncertain,
                3 => DeliveryObservation::Failed,
                _ => DeliveryObservation::Delivered,
            }
        })
    }
}
impl DeliveryAuthority for SyntheticMail {
    fn ready(&self) -> bool {
        self.mode.load(Ordering::SeqCst) != 4
    }
    fn delivered(&self, id: &str, _: i64) -> bool {
        self.verified.lock().unwrap().get(id) == Some(&true)
    }
}
struct ManualClock(Arc<AtomicI64>);
impl Clock for ManualClock {
    fn now(&self) -> Result<i64, crate::admission::AdmissionError> {
        Ok(self.0.load(Ordering::SeqCst))
    }
}
struct SyntheticSetup;
impl super::super::LocalSetupAuthorization for SyntheticSetup {
    fn permits(&self, _: &super::super::InitializationRequest) -> bool {
        true
    }
}
pub(super) struct Fixture {
    pub(super) root: tempfile::TempDir,
    pub(super) path: PathBuf,
    pub(super) keys: PathBuf,
    pub(super) store: PlanStore,
    pub(super) auth: HumanAuth,
    authority: Arc<SyntheticAuthority>,
    transport: Arc<SyntheticTransport>,
    mail: Arc<SyntheticMail>,
    clock: Arc<ManualClock>,
}
fn temporary() -> tempfile::TempDir {
    let tmp = std::env::temp_dir();
    let uid = rustix::process::geteuid().as_raw();
    let base = if [0, uid].contains(&fs::metadata(&tmp).unwrap().uid()) {
        tmp
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|p| p.join(".git").exists())
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    };
    tempfile::tempdir_in(base).unwrap()
}
fn request() -> RequestId {
    RequestId::new(new_id().unwrap()).unwrap()
}
fn password() -> Password {
    Password::new(hex(&random::<32>().unwrap()[..]).into_bytes()).unwrap()
}
impl Fixture {
    pub(super) async fn new() -> Self {
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
                &super::super::InitializationRequest::new(request()),
                &SyntheticSetup,
            )
            .await
            .unwrap();
        let authority = Arc::new(SyntheticAuthority::default());
        let transport = Arc::new(SyntheticTransport::default());
        let mail = Arc::new(SyntheticMail::default());
        let clock = Arc::new(ManualClock(Arc::new(AtomicI64::new(10000))));
        let mut auth = HumanAuth::open(&bootstrap).await.unwrap();
        auth.passwords = PasswordEngine::synthetic();
        auth.authority = authority.clone();
        auth.transport = transport.clone();
        auth.mail = mail.clone();
        auth.delivery = mail.clone();
        auth.clock = clock.clone();
        Self {
            root,
            path,
            keys,
            store,
            auth,
            authority,
            transport,
            mail,
            clock,
        }
    }
    pub(super) fn grant(
        &self,
        action: AdminAction,
        target: Option<&str>,
        payload: [u8; 32],
        verified: bool,
    ) -> Administration {
        let a = Administration {
            request_id: request(),
            evidence: request(),
        };
        self.authority.0.lock().unwrap().insert(
            a.request_id.as_str().to_owned(),
            Grant {
                action,
                target: target.map(str::to_owned),
                payload,
                evidence: a.evidence.as_str().to_owned(),
                until: self.now() + 3600,
                verified,
            },
        );
        a
    }
    pub(super) fn withdraw_client_at(
        &self,
        c: &ClientContext,
        point: Checkpoint,
    ) -> Box<dyn Fn(Checkpoint) -> Result<(), AuthError> + Send + Sync> {
        let authority = self.transport.clone();
        let key = c.transport_evidence.as_str().to_owned();
        Box::new(move |p| {
            if p == point {
                authority.0.lock().unwrap().remove(&key);
            }
            Ok(())
        })
    }
    pub(super) fn synthetic_mail_mode(&self, mode: i64) {
        self.mail.mode.store(mode, Ordering::SeqCst);
    }
    pub(super) fn now(&self) -> i64 {
        self.clock.0.load(Ordering::SeqCst)
    }
    pub(super) fn advance(&self, n: i64) {
        self.clock.0.fetch_add(n, Ordering::SeqCst);
    }
    pub(super) fn client(&self, browser: u8, ip: u8) -> ClientContext {
        let c = ClientContext {
            request_id: request(),
            transport_evidence: request(),
        };
        self.transport.0.lock().unwrap().insert(
            c.transport_evidence.as_str().to_owned(),
            (
                c.request_id.as_str().to_owned(),
                VerifiedTransport {
                    ip: IpAddr::from([192, 0, 2, ip]),
                    browser: [browser; 32],
                    parent: None,
                },
                self.now() + 3600,
                vec![
                    TransportOperation::Password,
                    TransportOperation::IssueEmail,
                    TransportOperation::ConfirmEmail,
                ],
            ),
        );
        c
    }
    pub(super) fn clock_for_session_test(&self) -> Arc<std::sync::atomic::AtomicI64> {
        self.clock.0.clone()
    }
    pub(super) fn bound_client(&self, browser: u8, parent: Option<(String, i64)>) -> ClientContext {
        let c = self.client(browser, 1);
        self.transport
            .0
            .lock()
            .unwrap()
            .get_mut(c.transport_evidence.as_str())
            .unwrap()
            .1
            .parent = parent;
        c
    }
    pub(super) fn enrollment(
        &self,
        email: &str,
        pw: Option<&Password>,
        first: bool,
        verified: bool,
        email_login: bool,
        p: LoginPolicy,
    ) -> Enrollment {
        let mut r = Enrollment {
            administration: Administration {
                request_id: request(),
                evidence: request(),
            },
            email: EmailAddress::parse(email).unwrap(),
            email_login,
            initial_policy: first.then_some(p),
        };
        r.administration = self.grant(
            if first {
                AdminAction::FirstEnrollment
            } else {
                AdminAction::AdditionalEnrollment
            },
            None,
            self.auth.enrollment_commitment(&r, pw),
            verified,
        );
        r
    }
    pub(super) async fn owner(&self) -> (HumanId, Password, EmailAddress) {
        let pw = password();
        let r = self.enrollment(
            "first@example.invalid",
            Some(&pw),
            true,
            true,
            true,
            LoginPolicy::new(true, true).unwrap(),
        );
        let id = self.auth.enroll(&r, Some(&pw)).await.unwrap();
        (id, pw, r.email)
    }
    pub(super) async fn challenge(
        &self,
        email: &EmailAddress,
        kind: ChallengeKind,
        purpose: ChallengePurpose,
    ) -> (ChallengeReceipt, ChallengeSecret, ClientContext) {
        let client = self.client(1, 1);
        let p = self
            .auth
            .prepare_email(&ChallengeRequest {
                client: client.clone(),
                email: email.clone(),
                kind,
                purpose,
                return_target: ReturnTarget::PrimaryAuthentication,
            })
            .await
            .unwrap();
        let receipt = p.receipt().clone();
        assert_eq!(
            self.auth.dispatch_email(p).await.unwrap(),
            MailDispatch::DeliveryVerified
        );
        let token = ChallengeSecret::parse(
            self.mail
                .messages
                .lock()
                .unwrap()
                .get(receipt.reference())
                .unwrap()
                .0
                .clone(),
        )
        .unwrap();
        (receipt, token, client)
    }
    pub(super) async fn redeem(
        &self,
        r: &ChallengeReceipt,
        s: &ChallengeSecret,
        k: ChallengeKind,
        c: &ClientContext,
    ) -> Redemption {
        self.auth
            .redeem_email(
                r,
                s,
                k,
                ChallengePurpose::Login,
                Confirmation::Affirmative,
                c,
            )
            .await
            .unwrap()
    }
    pub(super) async fn count(&self, table: &str) -> i64 {
        sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&self.store.pool)
            .await
            .unwrap()
    }
    pub(super) async fn reopened(&self) -> HumanAuth {
        let store = PlanStore::open(&self.path).await.unwrap();
        let bootstrap = ControllerBootstrap::open(
            store,
            &self.keys,
            super::super::ServiceIdentity::current(),
            &[],
        )
        .await
        .unwrap();
        let mut a = HumanAuth::open(&bootstrap).await.unwrap();
        a.passwords = PasswordEngine::synthetic();
        a.authority = self.authority.clone();
        a.transport = self.transport.clone();
        a.mail = self.mail.clone();
        a.delivery = self.mail.clone();
        a.clock = self.clock.clone();
        a
    }
}
#[test]
fn primary_policy_rejects_unknown_empty_and_totp_only() {
    for m in [
        vec![],
        vec!["totp"],
        vec!["password", "sms"],
        vec!["github"],
    ] {
        assert_eq!(LoginPolicy::from_methods(&m), Err(AuthError::InvalidPolicy));
    }
    for m in [
        vec!["password"],
        vec!["verified_email"],
        vec!["password", "verified_email"],
    ] {
        assert!(LoginPolicy::from_methods(&m).is_ok());
    }
    assert_eq!(ChallengeKind::default(), ChallengeKind::MagicLink);
}
#[test]
fn bounded_redacted_credentials_and_no_redirects() {
    for n in [0, 11, 1025] {
        assert!(Password::new(vec![b'x'; n]).is_err());
    }
    assert!(Password::new(vec![0; 12]).is_err());
    assert!(ChallengeSecret::parse("x".repeat(65536)).is_err());
    let p = password();
    assert!(!format!("{p:?}").contains(std::str::from_utf8(&p.0).unwrap()));
    for s in [
        "https://evil.invalid/",
        "//evil.invalid",
        "javascript:bad",
        "primary-authentication?token=bad",
    ] {
        assert!(ReturnTarget::parse(s).is_err());
    }
    assert!(PasswordParameters::new(1, 1).is_err());
    assert!(PasswordParameters::new(65537, 2).is_err());
    assert!(PasswordParameters::new(19456, 5).is_err());
}
#[tokio::test]
async fn enrollment_is_idempotent_not_merging_email_and_never_grants_roles() {
    let f = Fixture::new().await;
    let pw = password();
    let r = f.enrollment(
        "First@EXAMPLE.INVALID",
        Some(&pw),
        true,
        true,
        true,
        LoginPolicy::new(true, true).unwrap(),
    );
    let id = f.auth.enroll(&r, Some(&pw)).await.unwrap();
    assert_eq!(f.auth.enroll(&r, Some(&pw)).await.unwrap(), id);
    let second = f.enrollment(
        "First@example.invalid",
        Some(&pw),
        false,
        true,
        true,
        LoginPolicy::new(true, true).unwrap(),
    );
    assert_eq!(
        f.auth.enroll(&second, Some(&pw)).await.unwrap_err(),
        AuthError::Conflict
    );
    let third = f.enrollment(
        "first@example.invalid",
        Some(&pw),
        false,
        true,
        true,
        LoginPolicy::new(true, true).unwrap(),
    );
    assert_ne!(f.auth.enroll(&third, Some(&pw)).await.unwrap(), id);
    assert_eq!(f.count("human_users").await, 2);
    assert_eq!(f.count("human_enroll_requests").await, 2);
    assert_eq!(f.count("projects").await, 0);
}
#[tokio::test]
async fn first_enrollment_requires_exact_independent_authority() {
    let f = Fixture::new().await;
    let pw = password();
    let mut r = f.enrollment(
        "first@example.invalid",
        Some(&pw),
        true,
        false,
        false,
        LoginPolicy::new(true, false).unwrap(),
    );
    assert_eq!(
        f.auth.enroll(&r, Some(&pw)).await.unwrap_err(),
        AuthError::Denied
    );
    r.administration.evidence = request();
    assert_eq!(
        f.auth.enroll(&r, Some(&pw)).await.unwrap_err(),
        AuthError::Denied
    );
    let public = f.reopened().await;
    let bootstrap = ControllerBootstrap::open(
        public.store.clone(),
        &f.keys,
        super::super::ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let production = HumanAuth::open(&bootstrap).await.unwrap();
    assert_eq!(
        production.enroll(&r, Some(&pw)).await.unwrap_err(),
        AuthError::Unavailable
    );
    assert_eq!(f.count("human_users").await, 0);
}
#[tokio::test]
async fn enrollment_replay_with_changed_payload_is_denied() {
    let f = Fixture::new().await;
    let p = password();
    let mut r = f.enrollment(
        "first@example.invalid",
        Some(&p),
        true,
        true,
        false,
        LoginPolicy::new(true, false).unwrap(),
    );
    f.auth.enroll(&r, Some(&p)).await.unwrap();
    r.email = EmailAddress::parse("other@example.invalid").unwrap();
    assert_eq!(
        f.auth.enroll(&r, Some(&p)).await.unwrap_err(),
        AuthError::Denied
    );
    assert_eq!(f.count("human_users").await, 1);
}
#[tokio::test]
async fn concurrent_first_claims_create_at_most_one_human() {
    let f = Fixture::new().await;
    let p = password();
    let r1 = f.enrollment(
        "one@example.invalid",
        Some(&p),
        true,
        true,
        false,
        LoginPolicy::new(true, false).unwrap(),
    );
    let r2 = f.enrollment(
        "two@example.invalid",
        Some(&p),
        true,
        true,
        false,
        LoginPolicy::new(true, false).unwrap(),
    );
    let (a, b) = tokio::join!(f.auth.enroll(&r1, Some(&p)), f.auth.enroll(&r2, Some(&p)));
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(f.count("human_users").await, 1);
}
#[tokio::test]
async fn concurrent_duplicate_additional_enrollment_has_one_identity() {
    let f = Fixture::new().await;
    f.owner().await;
    let p = password();
    let r = f.enrollment(
        "other@example.invalid",
        Some(&p),
        false,
        true,
        true,
        LoginPolicy::new(true, true).unwrap(),
    );
    let (a, b) = tokio::join!(f.auth.enroll(&r, Some(&p)), f.auth.enroll(&r, Some(&p)));
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(f.count("human_users").await, 2);
}
#[tokio::test]
async fn passwords_verify_with_unique_argon2id_salts_and_generic_denial() {
    let f = Fixture::new().await;
    let (id, p, e) = f.owner().await;
    let r = f.enrollment(
        "other@example.invalid",
        Some(&p),
        false,
        true,
        true,
        LoginPolicy::new(true, true).unwrap(),
    );
    f.auth.enroll(&r, Some(&p)).await.unwrap();
    let hashes: Vec<String> = sqlx::query_scalar("SELECT phc FROM human_passwords")
        .fetch_all(&f.store.pool)
        .await
        .unwrap();
    assert_ne!(hashes[0], hashes[1]);
    assert!(hashes.iter().all(|h| h.starts_with("$argon2id$v=19$")));
    match f
        .auth
        .authenticate_password(&e, &p, &f.client(1, 1))
        .await
        .unwrap()
    {
        AuthenticationOutcome::Verified(a) => {
            assert_eq!(a.user(), &id);
            assert_eq!(a.stage(), AuthenticationStage::PrimaryVerified);
            assert_eq!(a.method(), PrimaryMethod::Password)
        }
        _ => panic!("valid password denied"),
    }
    let bad = password();
    for address in [&e, &EmailAddress::parse("missing@example.invalid").unwrap()] {
        assert!(matches!(
            f.auth
                .authenticate_password(address, &bad, &f.client(1, 2))
                .await
                .unwrap(),
            AuthenticationOutcome::Denied
        ));
    }
}
#[tokio::test]
async fn production_argon_profile_is_memory_hard_and_bounded() {
    let engine = PasswordEngine::new(PasswordParameters::default()).unwrap();
    let p = password();
    let h = engine.hash(&p).await.unwrap();
    assert!(h.contains("m=19456,t=2,p=1"));
    assert!(engine.verify(&p, Some(&h)).await.unwrap());
    assert!(!engine.verify(&password(), Some(&h)).await.unwrap());
    for h in [
        "bad",
        "$argon2id$v=19$m=999999999,t=999999,p=999$bad$bad",
        &"x".repeat(513),
    ] {
        assert!(!engine.verify(&p, Some(h)).await.unwrap());
    }
}
#[tokio::test]
async fn password_change_version_and_security_epoch_are_atomic() {
    let f = Fixture::new().await;
    let (id, old, e) = f.owner().await;
    let new = password();
    let payload = f.auth.mac(
        b"password-change",
        &[id.0.as_bytes(), &1i64.to_be_bytes(), &new.0],
    );
    let a = f.grant(AdminAction::PasswordChange, Some(&id.0), payload, true);
    f.auth.replace_password(&a, &id, 1, &new).await.unwrap();
    assert_eq!(
        f.auth.replace_password(&a, &id, 1, &new).await.unwrap_err(),
        AuthError::StaleVersion
    );
    assert!(matches!(
        f.auth
            .authenticate_password(&e, &old, &f.client(1, 1))
            .await
            .unwrap(),
        AuthenticationOutcome::Denied
    ));
    match f
        .auth
        .authenticate_password(&e, &new, &f.client(1, 1))
        .await
        .unwrap()
    {
        AuthenticationOutcome::Verified(a) => {
            assert_eq!(a.credential_version(), 2);
            assert_eq!(a.security_epoch(), 2)
        }
        _ => panic!("new password denied"),
    }
}
#[tokio::test]
async fn tampered_or_cross_account_password_verifier_cannot_authenticate() {
    let f = Fixture::new().await;
    let (id, p, e) = f.owner().await;
    sqlx::query("DROP TRIGGER human_password_update")
        .execute(&f.store.pool)
        .await
        .unwrap();
    for bad in [
        "invalid".repeat(10),
        "$argon2id$v=19$m=999999,t=99999,p=4$bad$bad".to_owned(),
    ] {
        sqlx::query("UPDATE human_passwords SET phc=? WHERE user_id=?")
            .bind(bad)
            .bind(&id.0)
            .execute(&f.store.pool)
            .await
            .unwrap();
        assert!(matches!(
            f.auth
                .authenticate_password(&e, &p, &f.client(1, 1))
                .await
                .unwrap(),
            AuthenticationOutcome::Denied
        ));
    }
}
#[tokio::test]
async fn password_account_throttle_expires_without_permanent_lockout() {
    let f = Fixture::new().await;
    let (_, p, e) = f.owner().await;
    for _ in 0..5 {
        assert!(matches!(
            f.auth
                .authenticate_password(&e, &password(), &f.client(1, 1))
                .await
                .unwrap(),
            AuthenticationOutcome::Denied
        ));
    }
    assert!(matches!(
        f.auth
            .authenticate_password(&e, &p, &f.client(1, 2))
            .await
            .unwrap(),
        AuthenticationOutcome::Denied
    ));
    f.advance(900);
    assert!(matches!(
        f.auth
            .authenticate_password(&e, &p, &f.client(1, 1))
            .await
            .unwrap(),
        AuthenticationOutcome::Verified(_)
    ));
}
#[tokio::test]
async fn trusted_client_ip_throttle_uses_distinct_unknown_accounts() {
    let f = Fixture::new().await;
    f.owner().await;
    for i in 0..30 {
        let e = EmailAddress::parse(&format!("missing{i}@example.invalid")).unwrap();
        assert!(matches!(
            f.auth
                .authenticate_password(&e, &password(), &f.client(1, 1))
                .await
                .unwrap(),
            AuthenticationOutcome::Denied
        ));
    }
    assert_eq!(f.count("human_attempts").await, 30);
    let e = EmailAddress::parse("missing-last@example.invalid").unwrap();
    f.auth
        .authenticate_password(&e, &password(), &f.client(1, 1))
        .await
        .unwrap();
    assert_eq!(f.count("human_attempts").await, 30);
}
#[tokio::test]
async fn concurrent_password_attempts_cannot_bypass_reserved_failure_limit() {
    let f = Fixture::new().await;
    let (_, _, e) = f.owner().await;
    let p = password();
    let c = f.client(1, 1);
    for _ in 0..4 {
        let (a, b) = tokio::join!(
            f.auth.authenticate_password(&e, &p, &c),
            f.auth.authenticate_password(&e, &p, &c)
        );
        assert!(
            !matches!(a, Ok(AuthenticationOutcome::Verified(_)))
                && !matches!(b, Ok(AuthenticationOutcome::Verified(_)))
        );
    }
    assert_eq!(f.count("human_attempts").await, 5);
    assert_eq!(f.count("human_authentications").await, 0);
}
#[tokio::test]
async fn password_work_keeps_capacity_after_caller_cancellation() {
    let e = Arc::new(PasswordEngine::new(PasswordParameters::default()).unwrap());
    let p = password();
    let handle = tokio::spawn({
        let e = e.clone();
        async move { e.hash(&p).await }
    });
    handle.abort();
    // A canceled pre-spawn request cannot fabricate a verifier; subsequent bounded
    // work remains usable. In-flight closures own their semaphore permits.
    let p = password();
    assert!(e.hash(&p).await.is_ok());
}

#[tokio::test]
async fn pending_identity_requires_an_usable_verification_route() {
    let f = Fixture::new().await;
    let p = password();
    let first = f.enrollment(
        "first@example.invalid",
        Some(&p),
        true,
        true,
        false,
        LoginPolicy::new(true, false).unwrap(),
    );
    f.auth.enroll(&first, Some(&p)).await.unwrap();
    f.mail.mode.store(4, Ordering::SeqCst);
    let r = f.enrollment(
        "pending@example.invalid",
        Some(&p),
        false,
        false,
        false,
        LoginPolicy::new(true, false).unwrap(),
    );
    assert_eq!(
        f.auth.enroll(&r, Some(&p)).await.unwrap_err(),
        AuthError::InvalidPolicy
    );
}
#[tokio::test]
async fn ordinary_clock_tick_during_issuance_is_not_a_denial() {
    let mut f = Fixture::new().await;
    let (_, _, e) = f.owner().await;
    let clock = f.clock.clone();
    f.auth.hook = Box::new(move |p| {
        if p == Checkpoint::BeforeChallengeCommit {
            clock.0.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    });
    let r = ChallengeRequest {
        client: f.client(1, 1),
        email: e,
        kind: ChallengeKind::MagicLink,
        purpose: ChallengePurpose::Login,
        return_target: ReturnTarget::PrimaryAuthentication,
    };
    assert!(f.auth.prepare_email(&r).await.is_ok());
}
#[tokio::test]
async fn stronger_target_profile_does_not_lock_out_previous_safe_hashes() {
    let e = PasswordEngine::new(PasswordParameters::default()).unwrap();
    let p = password();
    let hash = e.hash(&p).await.unwrap();
    let raised = PasswordEngine::new(PasswordParameters::new(32768, 3).unwrap()).unwrap();
    assert!(raised.verify(&p, Some(&hash)).await.unwrap());
}
struct DowngradedEvidence {
    base: Arc<SyntheticAuthority>,
    calls: AtomicI64,
}
impl EnrollmentAuthority for DowngradedEvidence {
    fn check(&self, c: &AdminContext<'_>) -> Result<EmailEvidence, AuthError> {
        let e = self.base.check(c)?;
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            Ok(e)
        } else {
            Ok(EmailEvidence::Pending)
        }
    }
}
#[tokio::test]
async fn enrollment_cannot_reuse_withdrawn_email_verification() {
    let mut f = Fixture::new().await;
    let p = password();
    let r = f.enrollment(
        "first@example.invalid",
        Some(&p),
        true,
        true,
        false,
        LoginPolicy::new(true, false).unwrap(),
    );
    f.auth.authority = Arc::new(DowngradedEvidence {
        base: f.authority.clone(),
        calls: AtomicI64::new(0),
    });
    assert_eq!(
        f.auth.enroll(&r, Some(&p)).await.unwrap_err(),
        AuthError::Denied
    );
    assert_eq!(f.count("human_users").await, 0);
}
#[tokio::test]
async fn pending_email_verification_is_separate_from_email_login_policy() {
    let f = Fixture::new().await;
    let p = password();
    let r = f.enrollment(
        "first@example.invalid",
        Some(&p),
        true,
        true,
        false,
        LoginPolicy::new(true, false).unwrap(),
    );
    f.auth.enroll(&r, Some(&p)).await.unwrap();
    let r = f.enrollment(
        "pending@example.invalid",
        Some(&p),
        false,
        false,
        false,
        LoginPolicy::new(true, false).unwrap(),
    );
    f.auth.enroll(&r, Some(&p)).await.unwrap();
    assert!(matches!(
        f.auth
            .authenticate_password(&r.email, &p, &f.client(1, 1))
            .await
            .unwrap(),
        AuthenticationOutcome::Denied
    ));
    let (receipt, token, c) = f
        .challenge(
            &r.email,
            ChallengeKind::MagicLink,
            ChallengePurpose::VerifyEmail,
        )
        .await;
    assert!(matches!(
        f.auth
            .redeem_email(
                &receipt,
                &token,
                ChallengeKind::MagicLink,
                ChallengePurpose::VerifyEmail,
                Confirmation::Affirmative,
                &c
            )
            .await
            .unwrap(),
        Redemption::EmailVerified
    ));
    assert!(matches!(
        f.auth
            .authenticate_password(&r.email, &p, &f.client(1, 1))
            .await
            .unwrap(),
        AuthenticationOutcome::Verified(_)
    ));
    assert_eq!(f.count("human_authentications").await, 1);
}
#[tokio::test]
async fn magic_link_requires_confirmation_and_is_one_use() {
    let f = Fixture::new().await;
    let (id, _, e) = f.owner().await;
    let (r, s, c) = f
        .challenge(&e, ChallengeKind::MagicLink, ChallengePurpose::Login)
        .await;
    assert!(matches!(
        f.auth
            .redeem_email(
                &r,
                &s,
                ChallengeKind::MagicLink,
                ChallengePurpose::Login,
                Confirmation::Preview,
                &c
            )
            .await
            .unwrap(),
        Redemption::ConfirmationRequired
    ));
    assert_eq!(f.count("human_authentications").await, 0);
    match f.redeem(&r, &s, ChallengeKind::MagicLink, &c).await {
        Redemption::PrimaryVerified(a) => {
            assert_eq!(a.user(), &id);
            assert_eq!(a.method(), PrimaryMethod::MagicLink)
        }
        _ => panic!("valid challenge denied"),
    }
    assert!(matches!(
        f.redeem(&r, &s, ChallengeKind::MagicLink, &c).await,
        Redemption::Denied
    ));
    assert_eq!(f.count("human_authentications").await, 1);
}
#[tokio::test]
async fn challenges_expire_at_exact_ten_minute_boundary() {
    for kind in [ChallengeKind::MagicLink, ChallengeKind::Otp] {
        let f = Fixture::new().await;
        let (_, _, e) = f.owner().await;
        let (r, s, c) = f.challenge(&e, kind, ChallengePurpose::Login).await;
        f.advance(600);
        assert!(matches!(
            f.redeem(&r, &s, kind, &c).await,
            Redemption::Denied
        ));
        assert_eq!(f.count("human_authentications").await, 0);
        let f = Fixture::new().await;
        let (_, _, e) = f.owner().await;
        let (r, s, c) = f.challenge(&e, kind, ChallengePurpose::Login).await;
        f.advance(599);
        assert!(matches!(
            f.redeem(&r, &s, kind, &c).await,
            Redemption::PrimaryVerified(_)
        ));
    }
}
#[tokio::test]
async fn otp_fifth_failure_exhausts_and_cannot_be_replayed() {
    let f = Fixture::new().await;
    let (_, _, e) = f.owner().await;
    let (r, s, c) = f
        .challenge(&e, ChallengeKind::Otp, ChallengePurpose::Login)
        .await;
    let wrong = ChallengeSecret::parse(
        if *s.0 == "00000000" {
            "00000001"
        } else {
            "00000000"
        }
        .to_owned(),
    )
    .unwrap();
    for _ in 0..5 {
        assert!(matches!(
            f.redeem(&r, &wrong, ChallengeKind::Otp, &c).await,
            Redemption::Denied
        ));
    }
    let row = sqlx::query("SELECT failures,state FROM human_challenges WHERE challenge_id=?")
        .bind(r.reference())
        .fetch_one(&f.store.pool)
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>("failures"), 5);
    assert_eq!(row.get::<&str, _>("state"), "exhausted");
    assert!(matches!(
        f.redeem(&r, &s, ChallengeKind::Otp, &c).await,
        Redemption::Denied
    ));
}
#[tokio::test]
async fn concurrent_otp_failures_have_exact_attempt_ceiling() {
    let f = Fixture::new().await;
    let (_, _, e) = f.owner().await;
    let (r, _, c) = f
        .challenge(&e, ChallengeKind::Otp, ChallengePurpose::Login)
        .await;
    let wrong = ChallengeSecret::parse(hex(&random::<32>().unwrap()[..])).unwrap();
    for _ in 0..4 {
        let (a, b) = tokio::join!(
            f.redeem(&r, &wrong, ChallengeKind::Otp, &c),
            f.redeem(&r, &wrong, ChallengeKind::Otp, &c)
        );
        assert!(matches!(a, Redemption::Denied) && matches!(b, Redemption::Denied));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT failures FROM human_challenges WHERE challenge_id=?")
            .bind(r.reference())
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        5
    );
}
#[tokio::test]
async fn concurrent_magic_and_otp_redemption_commit_one_primary_proof() {
    for k in [ChallengeKind::MagicLink, ChallengeKind::Otp] {
        let f = Fixture::new().await;
        let (_, _, e) = f.owner().await;
        let (r, s, c) = f.challenge(&e, k, ChallengePurpose::Login).await;
        let (a, b) = tokio::join!(f.redeem(&r, &s, k, &c), f.redeem(&r, &s, k, &c));
        assert_eq!(
            usize::from(matches!(a, Redemption::PrimaryVerified(_)))
                + usize::from(matches!(b, Redemption::PrimaryVerified(_))),
            1
        );
        assert_eq!(f.count("human_authentications").await, 1);
    }
}
#[tokio::test]
async fn wrong_browser_purpose_kind_account_or_instance_cannot_consume() {
    let f = Fixture::new().await;
    let (_, _, e) = f.owner().await;
    let (r, s, c) = f
        .challenge(&e, ChallengeKind::MagicLink, ChallengePurpose::Login)
        .await;
    assert!(matches!(
        f.redeem(&r, &s, ChallengeKind::MagicLink, &f.client(2, 1))
            .await,
        Redemption::Denied
    ));
    assert!(matches!(
        f.redeem(&r, &s, ChallengeKind::Otp, &c).await,
        Redemption::Denied
    ));
    assert!(matches!(
        f.auth
            .redeem_email(
                &r,
                &s,
                ChallengeKind::MagicLink,
                ChallengePurpose::VerifyEmail,
                Confirmation::Affirmative,
                &c
            )
            .await
            .unwrap(),
        Redemption::Denied
    ));
    f.advance(60);
    let p = password();
    let other = f.enrollment(
        "other@example.invalid",
        Some(&p),
        false,
        true,
        true,
        LoginPolicy::new(true, true).unwrap(),
    );
    f.auth.enroll(&other, Some(&p)).await.unwrap();
    let (r2, _, _) = f
        .challenge(
            &other.email,
            ChallengeKind::MagicLink,
            ChallengePurpose::Login,
        )
        .await;
    assert!(matches!(
        f.redeem(&r2, &s, ChallengeKind::MagicLink, &c).await,
        Redemption::Denied
    ));
    let second = Fixture::new().await;
    second.owner().await;
    assert!(matches!(
        second
            .auth
            .redeem_email(
                &r,
                &s,
                ChallengeKind::MagicLink,
                ChallengePurpose::Login,
                Confirmation::Affirmative,
                &second.client(1, 1)
            )
            .await
            .unwrap(),
        Redemption::Denied
    ));
    assert!(matches!(
        f.redeem(&r, &s, ChallengeKind::MagicLink, &c).await,
        Redemption::PrimaryVerified(_)
    ));
}
#[tokio::test]
async fn changed_email_security_epoch_or_disabled_user_invalidates_challenges() {
    for mode in 0..3 {
        let f = Fixture::new().await;
        let (id, _, e) = f.owner().await;
        let (r, s, c) = f
            .challenge(&e, ChallengeKind::MagicLink, ChallengePurpose::Login)
            .await;
        match mode {
            0 => {
                let new = EmailAddress::parse("new@example.invalid").unwrap();
                let a = f.grant(
                    AdminAction::EmailChange,
                    Some(&id.0),
                    f.auth.mac(
                        b"email-change",
                        &[id.0.as_bytes(), &1i64.to_be_bytes(), new.0.as_bytes()],
                    ),
                    true,
                );
                f.auth.replace_email(&a, &id, 1, &new).await.unwrap();
            }
            1 => {
                let new = password();
                let a = f.grant(
                    AdminAction::PasswordChange,
                    Some(&id.0),
                    f.auth.mac(
                        b"password-change",
                        &[id.0.as_bytes(), &1i64.to_be_bytes(), &new.0],
                    ),
                    true,
                );
                f.auth.replace_password(&a, &id, 1, &new).await.unwrap();
            }
            _ => {
                let a = f.grant(
                    AdminAction::Restrict,
                    Some(&id.0),
                    f.auth
                        .mac(b"identity-restrict", &[id.0.as_bytes(), b"compromised"]),
                    true,
                );
                f.auth
                    .restrict_identity(&a, &id, AccountRestriction::Compromised)
                    .await
                    .unwrap();
            }
        }
        assert!(matches!(
            f.redeem(&r, &s, ChallengeKind::MagicLink, &c).await,
            Redemption::Denied
        ));
        assert_eq!(f.count("human_authentications").await, 0);
    }
}
#[tokio::test]
async fn mail_acceptance_uncertainty_failure_and_missing_configuration_never_authenticate() {
    for mode in 1..=5 {
        let f = Fixture::new().await;
        let (_, _, e) = f.owner().await;
        f.mail.mode.store(mode, Ordering::SeqCst);
        let c = f.client(1, 1);
        let request = ChallengeRequest {
            client: c.clone(),
            email: e,
            kind: ChallengeKind::MagicLink,
            purpose: ChallengePurpose::Login,
            return_target: ReturnTarget::PrimaryAuthentication,
        };
        if mode == 4 {
            assert_eq!(
                f.auth.prepare_email(&request).await.unwrap_err(),
                AuthError::Unavailable
            );
            assert_eq!(
                f.auth.password_recovery_status().guidance(),
                "Self-service password reset is unavailable. Contact the instance administrator."
            );
            continue;
        }
        let p = f.auth.prepare_email(&request).await.unwrap();
        let r = p.receipt().clone();
        let result = f.auth.dispatch_email(p).await.unwrap();
        assert_ne!(result, MailDispatch::DeliveryVerified);
        let token = ChallengeSecret::parse(
            f.mail
                .messages
                .lock()
                .unwrap()
                .get(r.reference())
                .unwrap()
                .0
                .clone(),
        )
        .unwrap();
        assert!(matches!(
            f.redeem(&r, &token, ChallengeKind::MagicLink, &c).await,
            Redemption::Denied
        ));
    }
}
#[tokio::test]
async fn issuance_is_generic_for_unknown_and_enrolled_accounts_and_never_resends() {
    let f = Fixture::new().await;
    let (_, _, e) = f.owner().await;
    let request = ChallengeRequest {
        client: f.client(1, 1),
        email: e,
        kind: ChallengeKind::MagicLink,
        purpose: ChallengePurpose::Login,
        return_target: ReturnTarget::PrimaryAuthentication,
    };
    let p = f.auth.prepare_email(&request).await.unwrap();
    let r = p.receipt().clone();
    assert!(f.mail.messages.lock().unwrap().is_empty());
    f.auth.dispatch_email(p).await.unwrap();
    let duplicate = f.auth.prepare_email(&request).await.unwrap();
    assert_eq!(duplicate.receipt(), &r);
    assert_eq!(
        f.auth.dispatch_email(duplicate).await.unwrap(),
        MailDispatch::NotConfirmed
    );
    assert_eq!(f.mail.messages.lock().unwrap().len(), 1);
    let unknown = ChallengeRequest {
        email: EmailAddress::parse("unknown@example.invalid").unwrap(),
        client: f.client(1, 1),
        ..request
    };
    let prepared = f.auth.prepare_email(&unknown).await.unwrap();
    assert_eq!(prepared.receipt().reference().len(), r.reference().len());
    assert_eq!(format!("{prepared:?}"), "PreparedChallenge([redacted])");
    f.auth.dispatch_email(prepared).await.unwrap();
    assert_eq!(f.mail.messages.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn resend_has_bounded_issuance_and_cannot_reset_guess_budget() {
    let f = Fixture::new().await;
    let (_, _, e) = f.owner().await;
    let (r, s, c) = f
        .challenge(&e, ChallengeKind::Otp, ChallengePurpose::Login)
        .await;
    let wrong = ChallengeSecret::parse(hex(&random::<32>().unwrap()[..])).unwrap();
    for _ in 0..5 {
        f.redeem(&r, &wrong, ChallengeKind::Otp, &c).await;
    }
    f.advance(60);
    let (r2, s2, c2) = f
        .challenge(&e, ChallengeKind::Otp, ChallengePurpose::Login)
        .await;
    assert!(matches!(
        f.redeem(&r2, &s2, ChallengeKind::Otp, &c2).await,
        Redemption::Denied
    ));
    assert!(matches!(
        f.redeem(&r, &s, ChallengeKind::Otp, &c).await,
        Redemption::Denied
    ));
    for _ in 0..5 {
        let p = f
            .auth
            .prepare_email(&ChallengeRequest {
                client: f.client(1, 1),
                email: e.clone(),
                kind: ChallengeKind::Otp,
                purpose: ChallengePurpose::Login,
                return_target: ReturnTarget::PrimaryAuthentication,
            })
            .await
            .unwrap();
        f.auth.dispatch_email(p).await.unwrap();
    }
    assert_eq!(f.count("human_challenges").await, 2);
}
#[tokio::test]
async fn email_only_policy_needs_independent_ready_delivery_and_actual_enrollment() {
    let f = Fixture::new().await;
    let r = f.enrollment(
        "first@example.invalid",
        None,
        true,
        true,
        true,
        LoginPolicy::new(false, true).unwrap(),
    );
    f.mail.mode.store(4, Ordering::SeqCst);
    assert_eq!(
        f.auth.enroll(&r, None).await.unwrap_err(),
        AuthError::InvalidPolicy
    );
    f.mail.mode.store(0, Ordering::SeqCst);
    f.auth.enroll(&r, None).await.unwrap();
    let (receipt, token, c) = f
        .challenge(&r.email, ChallengeKind::MagicLink, ChallengePurpose::Login)
        .await;
    assert!(matches!(
        f.redeem(&receipt, &token, ChallengeKind::MagicLink, &c)
            .await,
        Redemption::PrimaryVerified(_)
    ));
    let r2 = f.enrollment(
        "other@example.invalid",
        None,
        false,
        true,
        false,
        LoginPolicy::new(false, true).unwrap(),
    );
    assert_eq!(
        f.auth.enroll(&r2, None).await.unwrap_err(),
        AuthError::InvalidPolicy
    );
}
#[tokio::test]
async fn policy_changes_cannot_strand_accounts_or_accept_unverified_authority() {
    let f = Fixture::new().await;
    let (id, _, _) = f.owner().await;
    let p = password();
    let r = f.enrollment(
        "password-only@example.invalid",
        Some(&p),
        false,
        true,
        false,
        LoginPolicy::new(true, true).unwrap(),
    );
    f.auth.enroll(&r, Some(&p)).await.unwrap();
    let new = LoginPolicy::new(false, true).unwrap();
    let payload = f
        .auth
        .mac(b"policy-change", &[&1i64.to_be_bytes(), &[0, 1]]);
    let a = f.grant(AdminAction::PolicyChange, None, payload, true);
    assert_eq!(
        f.auth.configure_policy(&a, 1, new).await.unwrap_err(),
        AuthError::InvalidPolicy
    );
    let a = Administration {
        request_id: request(),
        evidence: request(),
    };
    assert_eq!(
        f.auth.configure_policy(&a, 1, new).await.unwrap_err(),
        AuthError::Denied
    );
    let request = f.grant(
        AdminAction::EmailLoginChange,
        Some(&id.0),
        f.auth.mac(
            b"email-login-change",
            &[id.0.as_bytes(), &1i64.to_be_bytes(), &[0]],
        ),
        true,
    );
    f.auth
        .set_email_login(&request, &id, 1, false)
        .await
        .unwrap();
    assert_eq!(
        f.auth
            .set_email_login(&request, &id, 1, false)
            .await
            .unwrap_err(),
        AuthError::StaleVersion
    );
}
#[tokio::test]
async fn successful_policy_change_increments_version_and_revokes_old_email_proofs() {
    let f = Fixture::new().await;
    let (_, _, e) = f.owner().await;
    let (r, s, c) = f
        .challenge(&e, ChallengeKind::MagicLink, ChallengePurpose::Login)
        .await;
    let a = f.grant(
        AdminAction::PolicyChange,
        None,
        f.auth
            .mac(b"policy-change", &[&1i64.to_be_bytes(), &[1, 0]]),
        true,
    );
    f.auth
        .configure_policy(&a, 1, LoginPolicy::new(true, false).unwrap())
        .await
        .unwrap();
    assert!(matches!(
        f.redeem(&r, &s, ChallengeKind::MagicLink, &c).await,
        Redemption::Denied
    ));
    assert_eq!(
        f.auth
            .configure_policy(&a, 1, LoginPolicy::new(true, false).unwrap())
            .await
            .unwrap_err(),
        AuthError::StaleVersion
    );
}
#[tokio::test]
async fn revoked_transport_and_private_production_defaults_deny_before_proof() {
    let f = Fixture::new().await;
    let (_, p, e) = f.owner().await;
    let c = f.client(1, 1);
    f.transport.0.lock().unwrap().clear();
    assert!(matches!(
        f.auth.authenticate_password(&e, &p, &c).await,
        Err(AuthError::Denied)
    ));
    let bootstrap = ControllerBootstrap::open(
        f.store.clone(),
        &f.keys,
        super::super::ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let prod = HumanAuth::open(&bootstrap).await.unwrap();
    assert!(matches!(
        prod.authenticate_password(&e, &p, &c).await,
        Err(AuthError::Unavailable)
    ));
    assert_eq!(f.count("human_authentications").await, 0);
}
#[tokio::test]
async fn ordinary_restart_preserves_password_and_challenge_one_use_authority() {
    let f = Fixture::new().await;
    let (id, p, e) = f.owner().await;
    let (r, s, c) = f
        .challenge(&e, ChallengeKind::Otp, ChallengePurpose::Login)
        .await;
    let a = f.reopened().await;
    match a
        .authenticate_password(&e, &p, &f.client(1, 1))
        .await
        .unwrap()
    {
        AuthenticationOutcome::Verified(v) => assert_eq!(v.user(), &id),
        _ => panic!("restart lost verifier"),
    }
    assert!(matches!(
        a.redeem_email(
            &r,
            &s,
            ChallengeKind::Otp,
            ChallengePurpose::Login,
            Confirmation::Affirmative,
            &c
        )
        .await
        .unwrap(),
        Redemption::PrimaryVerified(_)
    ));
    assert!(matches!(
        f.redeem(&r, &s, ChallengeKind::Otp, &c).await,
        Redemption::Denied
    ));
    f.store.check_integrity().await.unwrap();
}
#[tokio::test]
async fn sqlite_failure_rolls_back_challenge_consumption_and_primary_evidence() {
    let f = Fixture::new().await;
    let (_, _, e) = f.owner().await;
    let (r, s, c) = f
        .challenge(&e, ChallengeKind::MagicLink, ChallengePurpose::Login)
        .await;
    sqlx::query("CREATE TRIGGER fail_auth_audit BEFORE INSERT ON human_security_events WHEN NEW.event='primary_verified' BEGIN SELECT RAISE(ABORT,'synthetic storage failure'); END").execute(&f.store.pool).await.unwrap();
    assert!(matches!(
        f.auth
            .redeem_email(
                &r,
                &s,
                ChallengeKind::MagicLink,
                ChallengePurpose::Login,
                Confirmation::Affirmative,
                &c
            )
            .await,
        Err(AuthError::Storage)
    ));
    assert_eq!(f.count("human_authentications").await, 0);
    sqlx::query("DROP TRIGGER fail_auth_audit")
        .execute(&f.store.pool)
        .await
        .unwrap();
    assert!(matches!(
        f.redeem(&r, &s, ChallengeKind::MagicLink, &c).await,
        Redemption::PrimaryVerified(_)
    ));
}
#[tokio::test]
async fn final_expiry_or_transport_revocation_rolls_back_successful_consumption() {
    for revoke in [false, true] {
        let mut f = Fixture::new().await;
        let (_, _, e) = f.owner().await;
        let (r, s, c) = f
            .challenge(&e, ChallengeKind::MagicLink, ChallengePurpose::Login)
            .await;
        let clock = f.clock.clone();
        let transport = f.transport.clone();
        f.auth.hook = Box::new(move |p| {
            if p == Checkpoint::BeforeConsumptionCommit {
                if revoke {
                    transport.0.lock().unwrap().clear();
                } else {
                    clock.0.fetch_add(600, Ordering::SeqCst);
                }
            }
            Ok(())
        });
        assert!(matches!(
            f.redeem(&r, &s, ChallengeKind::MagicLink, &c).await,
            Redemption::Denied
        ));
        assert_eq!(f.count("human_authentications").await, 0);
        let state: String =
            sqlx::query_scalar("SELECT state FROM human_challenges WHERE challenge_id=?")
                .bind(r.reference())
                .fetch_one(&f.store.pool)
                .await
                .unwrap();
        assert_eq!(state, "delivered");
    }
}
#[tokio::test]
async fn database_wal_and_snapshot_store_no_plaintext_credential_material() {
    let f = Fixture::new().await;
    let (_, p, e) = f.owner().await;
    let (_, s, _) = f
        .challenge(&e, ChallengeKind::MagicLink, ChallengePurpose::Login)
        .await;
    f.advance(60);
    let (_, otp_secret, _) = f
        .challenge(&e, ChallengeKind::Otp, ChallengePurpose::Login)
        .await;
    let snapshot = f.root.path().join("data/snapshot.sqlite");
    f.store.snapshot(&snapshot).await.unwrap();
    for path in [&f.path, &f.path.with_extension("sqlite-wal"), &snapshot] {
        if path != &f.path.with_extension("sqlite-wal") || path.exists() {
            let bytes = fs::read(path).unwrap();
            for secret in [&p.0[..], s.0.as_bytes(), otp_secret.0.as_bytes()] {
                assert!(
                    !bytes.windows(secret.len()).any(|w| w == secret),
                    "plaintext persisted"
                );
            }
        }
    }
    let restored = PlanStore::open(&snapshot).await.unwrap();
    let bootstrap = ControllerBootstrap::open(
        restored.clone(),
        &f.keys,
        super::super::ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    assert!(HumanAuth::open(&bootstrap).await.is_ok());
    restored.check_integrity().await.unwrap();
}
#[tokio::test]
async fn missing_controller_key_never_creates_an_authentication_fallback() {
    let f = Fixture::new().await;
    f.owner().await;
    for entry in fs::read_dir(&f.keys).unwrap() {
        let p = entry.unwrap().path();
        if p.is_file() && fs::metadata(&p).unwrap().len() > 32 {
            fs::remove_file(p).unwrap();
        }
    }
    let bootstrap = ControllerBootstrap::open(
        f.store.clone(),
        &f.keys,
        super::super::ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    assert!(matches!(
        HumanAuth::open(&bootstrap).await,
        Err(AuthError::KeyUnavailable)
    ));
    assert_eq!(f.count("human_users").await, 1);
}
#[tokio::test]
async fn bounded_cleanup_never_deletes_primary_or_audit_evidence() {
    let f = Fixture::new().await;
    let (_, p, e) = f.owner().await;
    f.auth
        .authenticate_password(&e, &p, &f.client(1, 1))
        .await
        .unwrap();
    let before = f.count("human_security_events").await;
    f.advance(900);
    let a = f.grant(
        AdminAction::Maintenance,
        None,
        f.auth.mac(b"maintenance", &[&1u32.to_be_bytes()]),
        true,
    );
    assert_eq!(f.auth.cleanup_attempts(&a, 1).await.unwrap(), 1);
    assert_eq!(f.count("human_authentications").await, 1);
    assert_eq!(f.count("human_security_events").await, before + 1);
    assert_eq!(
        f.auth.cleanup_attempts(&a, 101).await.unwrap_err(),
        AuthError::InvalidInput
    );
}

use std::{
    io::{BufRead, BufReader, Read, Write},
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
fn child_command(f: &Fixture, checkpoint: Option<Checkpoint>, operation: &str) -> Command {
    let mut c = Command::new(std::env::current_exe().unwrap());
    c.args([
        "--exact",
        "controller_keys::human_auth::tests::subprocess_driver",
        "--nocapture",
    ])
    .env("ROLVYA_S203A_TEST_ROOT", f.root.path())
    .env("ROLVYA_S203A_TEST_OPERATION", operation)
    .env(
        "ROLVYA_S203A_TEST_CHECKPOINT",
        checkpoint.map(|p| format!("{p:?}")).unwrap_or_default(),
    )
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    c
}
#[tokio::test]
async fn subprocess_driver() {
    let Ok(root) = std::env::var("ROLVYA_S203A_TEST_ROOT") else {
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
    let mut auth = HumanAuth::open(&bootstrap).await.unwrap();
    auth.passwords = PasswordEngine::synthetic();
    let authority = Arc::new(SyntheticAuthority::default());
    let transport = Arc::new(SyntheticTransport::default());
    let mail = Arc::new(SyntheticMail::default());
    auth.authority = authority.clone();
    auth.transport = transport.clone();
    auth.mail = mail.clone();
    auth.delivery = mail;
    auth.clock = Arc::new(ManualClock(Arc::new(AtomicI64::new(10000))));
    auth.hook = Box::new(|p| {
        if std::env::var("ROLVYA_S203A_TEST_CHECKPOINT").unwrap() == format!("{p:?}") {
            println!("S203A_READY {p:?}");
            std::io::stdout().flush().unwrap();
            loop {
                std::thread::park();
            }
        }
        Ok(())
    });
    let client = ClientContext {
        request_id: request(),
        transport_evidence: request(),
    };
    transport.0.lock().unwrap().insert(
        client.transport_evidence.as_str().to_owned(),
        (
            client.request_id.as_str().to_owned(),
            VerifiedTransport {
                ip: IpAddr::from([192, 0, 2, 1]),
                browser: [1; 32],
                parent: None,
            },
            13600,
            vec![
                TransportOperation::Password,
                TransportOperation::IssueEmail,
                TransportOperation::ConfirmEmail,
            ],
        ),
    );
    match std::env::var("ROLVYA_S203A_TEST_OPERATION")
        .unwrap()
        .as_str()
    {
        "enroll" => {
            let password = password();
            let a = Administration {
                request_id: request(),
                evidence: request(),
            };
            let r = Enrollment {
                administration: a,
                email: EmailAddress::parse("first@example.invalid").unwrap(),
                email_login: true,
                initial_policy: Some(LoginPolicy::new(true, true).unwrap()),
            };
            authority.0.lock().unwrap().insert(
                r.administration.request_id.as_str().to_owned(),
                Grant {
                    action: AdminAction::FirstEnrollment,
                    target: None,
                    payload: auth.enrollment_commitment(&r, Some(&password)),
                    evidence: r.administration.evidence.as_str().to_owned(),
                    until: 13600,
                    verified: true,
                },
            );
            match auth.enroll(&r, Some(&password)).await {
                Ok(_) | Err(AuthError::Conflict | AuthError::RecoveryRequired) => {}
                Err(_) => panic!("unexpected enrollment outcome"),
            }
        }
        "issue" => {
            let prepared = auth
                .prepare_email(&ChallengeRequest {
                    client,
                    email: EmailAddress::parse("first@example.invalid").unwrap(),
                    kind: ChallengeKind::MagicLink,
                    purpose: ChallengePurpose::Login,
                    return_target: ReturnTarget::PrimaryAuthentication,
                })
                .await
                .unwrap();
            auth.dispatch_email(prepared).await.unwrap();
        }
        "consume" => {
            // Raw synthetic bearer passes through a bounded anonymous stdin pipe,
            // never environment variables, process arguments or tracked fixtures.
            let mut input = Zeroizing::new(String::new());
            std::io::stdin()
                .take(256)
                .read_to_string(&mut input)
                .unwrap();
            let (id, token) = input.split_once('\n').unwrap();
            let token = ChallengeSecret::parse(token.to_owned()).unwrap();
            auth.redeem_email(
                &ChallengeReceipt {
                    reference: id.to_owned(),
                },
                &token,
                ChallengeKind::MagicLink,
                ChallengePurpose::Login,
                Confirmation::Affirmative,
                &client,
            )
            .await
            .unwrap();
        }
        _ => panic!("invalid synthetic driver operation"),
    }
}
#[tokio::test]
async fn sigkill_at_seven_authoritative_boundaries_preserves_one_use_and_incomplete_setup() {
    for checkpoint in [
        Checkpoint::EnrollmentClaimed,
        Checkpoint::BeforeChallengeCommit,
        Checkpoint::AfterChallengeCommit,
        Checkpoint::BeforeMailSend,
        Checkpoint::AfterMailSend,
        Checkpoint::BeforeConsumptionCommit,
        Checkpoint::AfterConsumptionCommit,
    ] {
        let f = Fixture::new().await;
        let operation = match checkpoint {
            Checkpoint::EnrollmentClaimed => "enroll",
            Checkpoint::BeforeChallengeCommit
            | Checkpoint::AfterChallengeCommit
            | Checkpoint::BeforeMailSend
            | Checkpoint::AfterMailSend => "issue",
            _ => "consume",
        };
        let mut challenge = None;
        if operation != "enroll" {
            let (_, _, email) = f.owner().await;
            if operation == "consume" {
                challenge = Some(
                    f.challenge(&email, ChallengeKind::MagicLink, ChallengePurpose::Login)
                        .await,
                );
            }
        }
        let mut child = child_command(&f, Some(checkpoint), operation)
            .spawn()
            .unwrap();
        if let Some((r, s, _)) = &challenge {
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(r.reference().as_bytes()).unwrap();
            stdin.write_all(b"\n").unwrap();
            stdin.write_all(s.0.as_bytes()).unwrap();
        }
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.starts_with("S203A_READY") {
                    let _ = tx.send(());
                }
            }
        });
        let mut victim = Victim(child);
        assert!(
            rx.recv_timeout(Duration::from_secs(30)).is_ok(),
            "child did not reach crash barrier {checkpoint:?}"
        );
        victim.0.kill().unwrap();
        victim.0.wait().unwrap();
        let reopened = f.reopened().await;
        reopened.store.check_integrity().await.unwrap();
        match checkpoint {
            Checkpoint::EnrollmentClaimed => {
                assert_eq!(f.count("human_users").await, 0);
                let p = password();
                let r = f.enrollment(
                    "new@example.invalid",
                    Some(&p),
                    true,
                    true,
                    true,
                    LoginPolicy::new(true, true).unwrap(),
                );
                assert_eq!(
                    reopened.enroll(&r, Some(&p)).await.unwrap_err(),
                    AuthError::RecoveryRequired
                );
            }
            Checkpoint::BeforeChallengeCommit => assert_eq!(f.count("human_challenges").await, 0),
            Checkpoint::AfterChallengeCommit => {
                assert_eq!(f.count("human_challenges").await, 1);
                assert_eq!(f.count("human_authentications").await, 0);
                assert_eq!(
                    sqlx::query_scalar::<_, String>("SELECT state FROM human_challenges")
                        .fetch_one(&f.store.pool)
                        .await
                        .unwrap(),
                    "queued"
                );
            }
            Checkpoint::BeforeMailSend | Checkpoint::AfterMailSend => {
                assert_eq!(f.count("human_authentications").await, 0);
                assert_eq!(
                    sqlx::query_scalar::<_, String>("SELECT state FROM human_challenges")
                        .fetch_one(&f.store.pool)
                        .await
                        .unwrap(),
                    "dispatching"
                );
            }
            Checkpoint::BeforeSessionCommit
            | Checkpoint::AfterSessionCommit
            | Checkpoint::BeforeFactorCommit
            | Checkpoint::AfterFactorCommit
            | Checkpoint::BeforeOwnerCommit
            | Checkpoint::AfterOwnerCommit => {
                unreachable!("S2-03A driver never selects session/factor checkpoints")
            }
            Checkpoint::BeforeConsumptionCommit | Checkpoint::AfterConsumptionCommit => {
                let (r, s, c) = challenge.unwrap();
                let was_committed = checkpoint == Checkpoint::AfterConsumptionCommit;
                assert_eq!(
                    f.count("human_authentications").await,
                    i64::from(was_committed)
                );
                let result = reopened
                    .redeem_email(
                        &r,
                        &s,
                        ChallengeKind::MagicLink,
                        ChallengePurpose::Login,
                        Confirmation::Affirmative,
                        &c,
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    matches!(result, Redemption::PrimaryVerified(_)),
                    !was_committed
                );
                assert_eq!(f.count("human_authentications").await, 1);
            }
        }
    }
}
#[tokio::test]
async fn two_real_processes_cannot_win_first_enrollment() {
    let f = Fixture::new().await;
    let a = child_command(&f, None, "enroll").spawn().unwrap();
    let b = child_command(&f, None, "enroll").spawn().unwrap();
    let (tx, rx) = mpsc::channel();
    for mut child in [Victim(a), Victim(b)] {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let result = child.0.wait().unwrap().success();
            let _ = tx.send(result);
        });
    }
    for _ in 0..2 {
        assert!(rx.recv_timeout(Duration::from_secs(30)).unwrap());
    }
    let a = f.reopened().await;
    a.store.check_integrity().await.unwrap();
    assert_eq!(f.count("human_users").await, 1);
    assert_eq!(f.count("human_enroll_requests").await, 1);
}
#[tokio::test]
async fn sqlite_contention_is_bounded_and_creates_no_primary_authority() {
    let f = Fixture::new().await;
    let (_, p, e) = f.owner().await;
    let lock = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let result = f.auth.authenticate_password(&e, &p, &f.client(1, 1)).await;
    assert!(matches!(result, Err(AuthError::Busy)));
    lock.rollback().await.unwrap();
    assert_eq!(f.count("human_authentications").await, 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn canceled_inflight_argon_jobs_retain_permits_until_actual_completion() {
    let mut engine = PasswordEngine::synthetic();
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let (tx, rx) = mpsc::channel();
    let gate = barrier.clone();
    engine.started = Some(Arc::new(move || {
        tx.send(()).unwrap();
        gate.wait();
    }));
    let engine = Arc::new(engine);
    let a = tokio::spawn({
        let engine = engine.clone();
        async move { engine.hash(&password()).await }
    });
    let b = tokio::spawn({
        let engine = engine.clone();
        async move { engine.hash(&password()).await }
    });
    for _ in 0..2 {
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    a.abort();
    b.abort();
    assert!(a.await.unwrap_err().is_cancelled());
    assert!(b.await.unwrap_err().is_cancelled());
    assert_eq!(engine.slots.available_permits(), 0);
    assert_eq!(engine.hash(&password()).await.unwrap_err(), AuthError::Busy);
    barrier.wait();
    let permits = tokio::time::timeout(
        Duration::from_secs(10),
        engine.slots.clone().acquire_many_owned(2),
    )
    .await
    .unwrap()
    .unwrap();
    drop(permits);
}
#[tokio::test]
async fn keyed_commitments_reject_unkeyed_and_cross_installation_substitution() {
    use sha2::Digest;
    let f = Fixture::new().await;
    let (_, _, e) = f.owner().await;
    let (r, s, c) = f
        .challenge(&e, ChallengeKind::Otp, ChallengePurpose::Login)
        .await;
    let row = sqlx::query("SELECT * FROM human_challenges WHERE challenge_id=?")
        .bind(r.reference())
        .fetch_one(&f.store.pool)
        .await
        .unwrap();
    let expected = f.auth.row_challenge_mac(&row, &s).unwrap();
    let other = Fixture::new().await;
    assert_ne!(expected, other.auth.row_challenge_mac(&row, &s).unwrap());
    assert_ne!(expected, sha2::Sha256::digest(s.0.as_bytes()).as_slice());
    sqlx::query("DROP TRIGGER human_challenge_update")
        .execute(&f.store.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE human_challenges SET token_commitment=? WHERE challenge_id=?")
        .bind(sha2::Sha256::digest(s.0.as_bytes()).as_slice())
        .bind(r.reference())
        .execute(&f.store.pool)
        .await
        .unwrap();
    assert!(matches!(
        f.redeem(&r, &s, ChallengeKind::Otp, &c).await,
        Redemption::Denied
    ));
    assert_eq!(f.count("human_authentications").await, 0);
}
#[tokio::test]
async fn independent_accounts_cannot_swap_password_credentials() {
    let f = Fixture::new().await;
    let (id, _, e) = f.owner().await;
    let p = password();
    let r = f.enrollment(
        "other@example.invalid",
        Some(&p),
        false,
        true,
        true,
        LoginPolicy::new(true, true).unwrap(),
    );
    let other = f.auth.enroll(&r, Some(&p)).await.unwrap();
    let row = sqlx::query("SELECT phc,binding FROM human_passwords WHERE user_id=?")
        .bind(other.0)
        .fetch_one(&f.store.pool)
        .await
        .unwrap();
    sqlx::query("DROP TRIGGER human_password_update")
        .execute(&f.store.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE human_passwords SET phc=?,binding=? WHERE user_id=?")
        .bind(row.get::<String, _>("phc"))
        .bind(row.get::<Vec<u8>, _>("binding"))
        .bind(&id.0)
        .execute(&f.store.pool)
        .await
        .unwrap();
    assert!(matches!(
        f.auth
            .authenticate_password(&e, &p, &f.client(1, 1))
            .await
            .unwrap(),
        AuthenticationOutcome::Denied
    ));
}
#[tokio::test]
async fn email_replacement_requires_fresh_independent_verification() {
    let mut f = Fixture::new().await;
    let (id, _, original) = f.owner().await;
    let new = EmailAddress::parse("new@example.invalid").unwrap();
    let a = f.grant(
        AdminAction::EmailChange,
        Some(&id.0),
        f.auth.mac(
            b"email-change",
            &[id.0.as_bytes(), &1i64.to_be_bytes(), new.0.as_bytes()],
        ),
        true,
    );
    f.auth.authority = Arc::new(DowngradedEvidence {
        base: f.authority.clone(),
        calls: AtomicI64::new(0),
    });
    assert_eq!(
        f.auth.replace_email(&a, &id, 1, &new).await.unwrap_err(),
        AuthError::Denied
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT address FROM human_emails WHERE user_id=?")
            .bind(&id.0)
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        original.0
    );
}
#[tokio::test]
async fn operation_confused_transport_evidence_never_verifies_password_or_email() {
    let f = Fixture::new().await;
    let (_, p, e) = f.owner().await;
    let (r, s, c) = f
        .challenge(&e, ChallengeKind::MagicLink, ChallengePurpose::Login)
        .await;
    f.transport
        .0
        .lock()
        .unwrap()
        .get_mut(c.transport_evidence.as_str())
        .unwrap()
        .3 = vec![TransportOperation::IssueEmail];
    assert!(matches!(
        f.auth.authenticate_password(&e, &p, &c).await,
        Err(AuthError::Denied)
    ));
    assert!(matches!(
        f.auth
            .redeem_email(
                &r,
                &s,
                ChallengeKind::MagicLink,
                ChallengePurpose::Login,
                Confirmation::Affirmative,
                &c
            )
            .await,
        Err(AuthError::Denied)
    ));
    assert_eq!(f.count("human_authentications").await, 0);
}
#[tokio::test]
async fn additional_factor_required_accounts_never_receive_a_complete_authentication() {
    let f = Fixture::new().await;
    let (id, p, e) = f.owner().await;
    // Simulate a later reviewed factor enrollment, not an S2-03A public API.
    sqlx::query("DROP TRIGGER human_user_update")
        .execute(&f.store.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE human_users SET factor_required=1,security_epoch=security_epoch+1 WHERE user_id=?",
    )
    .bind(&id.0)
    .execute(&f.store.pool)
    .await
    .unwrap();
    let result = f
        .auth
        .authenticate_password(&e, &p, &f.client(1, 1))
        .await
        .unwrap();
    assert!(
        matches!(result,AuthenticationOutcome::Verified(a) if a.stage()==AuthenticationStage::AdditionalFactorRequired)
    );
    let (r, s, c) = f
        .challenge(&e, ChallengeKind::MagicLink, ChallengePurpose::Login)
        .await;
    assert!(
        matches!(f.redeem(&r,&s,ChallengeKind::MagicLink,&c).await,Redemption::PrimaryVerified(a) if a.stage()==AuthenticationStage::AdditionalFactorRequired)
    );
    let new = password();
    let a = f.grant(
        AdminAction::PasswordChange,
        Some(&id.0),
        f.auth.mac(
            b"password-change",
            &[id.0.as_bytes(), &1i64.to_be_bytes(), &new.0],
        ),
        true,
    );
    assert_eq!(
        f.auth.replace_password(&a, &id, 1, &new).await.unwrap_err(),
        AuthError::Denied
    );
}
#[tokio::test]
async fn migration_guards_reject_scope_rewrites_and_audit_or_evidence_mutation() {
    let f = Fixture::new().await;
    let (_, p, e) = f.owner().await;
    f.auth
        .authenticate_password(&e, &p, &f.client(1, 1))
        .await
        .unwrap();
    f.challenge(&e, ChallengeKind::Otp, ChallengePurpose::Login)
        .await;
    for sql in [
        "UPDATE human_emails SET address='different@example.invalid'",
        "UPDATE human_challenges SET purpose='verify_email'",
        "UPDATE human_challenges SET expires_at=expires_at+600",
        "UPDATE human_authentications SET security_epoch=security_epoch+1",
        "DELETE FROM human_authentications",
        "DELETE FROM human_security_events",
        "UPDATE human_enrollment SET state='fresh'",
        "UPDATE human_users SET factor_required=1,security_epoch=security_epoch+1",
    ] {
        assert!(
            sqlx::query(sql).execute(&f.store.pool).await.is_err(),
            "authority mutation unexpectedly allowed"
        );
    }
    f.store.check_integrity().await.unwrap();
}
