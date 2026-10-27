use super::*;
#[test]
fn cookie_contract_is_host_only_secure_and_http_only() {
    let c = CookieContract::session();
    assert_eq!(c.name, "__Host-rolvya");
    assert!(c.secure && c.http_only);
    assert_eq!(c.same_site, "Strict");
    assert_eq!(c.path, "/");
    assert_eq!(c.domain, None);
}
use super::super::tests::Fixture as PrimaryFixture;
use std::{collections::BTreeMap, sync::Mutex};
struct BrowserGrant {
    request: String,
    operations: Vec<Operation>,
    facts: BrowserFacts,
    until: i64,
}
#[derive(Default)]
struct SyntheticBrowser {
    grants: Mutex<BTreeMap<String, BrowserGrant>>,
    cookies: Mutex<Vec<Zeroizing<String>>>,
    provisioned: Mutex<usize>,
}
impl BrowserAuthority for SyntheticBrowser {
    fn verify(
        &self,
        c: &BrowserContext,
        op: Operation,
        now: i64,
    ) -> Result<BrowserFacts, AuthError> {
        let grants = self.grants.lock().unwrap();
        let g = grants.get(c.evidence.as_str()).ok_or(AuthError::Denied)?;
        if g.request != c.request_id.as_str() || !g.operations.contains(&op) || now >= g.until {
            return Err(AuthError::Denied);
        }
        Ok(g.facts.clone())
    }
    fn deliver(&self, _: &BrowserContext, t: &SessionTicket, _: i64) -> Result<(), AuthError> {
        self.cookies.lock().unwrap().push(Zeroizing::new(format!(
            "{}={}; Path=/; Secure; HttpOnly; SameSite=Strict",
            CookieContract::session().name,
            t.handle.0.as_str()
        )));
        Ok(())
    }
    fn provision(&self, _: &BrowserContext, _: &FactorEnrollment, _: i64) -> Result<(), AuthError> {
        *self.provisioned.lock().unwrap() += 1;
        Ok(())
    }
    fn recovery(&self, _: &BrowserContext, _: &RecoveryBundle, _: i64) -> Result<(), AuthError> {
        Ok(())
    }
    fn pending(&self, _: &BrowserContext, _: &PendingLogin, _: i64) -> Result<(), AuthError> {
        Ok(())
    }
}
#[derive(Default)]
struct SyntheticIncidents(Mutex<BTreeMap<String, (String, String, i64, String)>>);
impl IncidentAuthority for SyntheticIncidents {
    fn verify(&self, c: &IncidentContext<'_>) -> Result<(), AuthError> {
        let g = self.0.lock().unwrap();
        let t = g
            .get(c.evidence.request_id.as_str())
            .ok_or(AuthError::Denied)?;
        if t.0 != c.user
            || t.1 != c.session
            || t.2 != c.generation
            || t.3 != c.evidence.evidence.as_str()
            || c.now < 0
        {
            return Err(AuthError::Denied);
        }
        Ok(())
    }
}
struct Fixture {
    a: PrimaryFixture,
    browser: Arc<SyntheticBrowser>,
    incidents: Arc<SyntheticIncidents>,
    user: HumanId,
    password: Password,
    email: EmailAddress,
}
fn req() -> RequestId {
    RequestId::new(new_id().unwrap()).unwrap()
}
fn device() -> DeviceLabel {
    DeviceLabel::new("Synthetic browser".to_owned()).unwrap()
}
impl Fixture {
    async fn new() -> Self {
        let mut a = PrimaryFixture::new().await;
        let (user, password, email) = a.owner().await;
        let browser = Arc::new(SyntheticBrowser::default());
        let incidents = Arc::new(SyntheticIncidents::default());
        a.auth.browser_authority = browser.clone();
        a.auth.incident_authority = incidents.clone();
        Self {
            a,
            browser,
            incidents,
            user,
            password,
            email,
        }
    }
    fn context(
        &self,
        op: Operation,
        b: u8,
        ticket: Option<&SessionTicket>,
        human: bool,
    ) -> BrowserContext {
        let c = BrowserContext {
            request_id: req(),
            evidence: req(),
        };
        self.browser.grants.lock().unwrap().insert(
            c.evidence.as_str().to_owned(),
            BrowserGrant {
                request: c.request_id.as_str().to_owned(),
                operations: vec![op],
                until: self.a.now() + 20 * 86400,
                facts: BrowserFacts {
                    secure: true,
                    method: if op == Operation::Read {
                        Method::Get
                    } else {
                        Method::Post
                    },
                    origin: "https://rolvya.invalid".to_owned(),
                    expected_origin: "https://rolvya.invalid".to_owned(),
                    same_origin: true,
                    browser: [b; 32],
                    credential: ticket.map(|t| self.a.auth.handle_lookup(&t.handle)),
                    csrf: ticket.map(|t| self.a.auth.csrf_commitment(&t.csrf)),
                    preauth_csrf: true,
                    activity: if human {
                        Activity::Human
                    } else {
                        Activity::Background
                    },
                    client_key: self.a.auth.mac(b"synthetic-client", &[b"192.0.2.1"]),
                },
            },
        );
        c
    }
    async fn proof(&self, b: u8, parent: Option<&SessionTicket>) -> PrimaryAuthentication {
        let c = self.a.bound_client(
            b,
            parent.map(|t| (t.metadata.id.0.clone(), t.metadata.generation)),
        );
        match self
            .a
            .auth
            .authenticate_password(&self.email, &self.password, &c)
            .await
            .unwrap()
        {
            AuthenticationOutcome::Verified(p) => p,
            _ => panic!("primary denied"),
        }
    }
    async fn session(&self, b: u8) -> SessionTicket {
        let p = self.proof(b, None).await;
        match self
            .a
            .auth
            .start_session(
                &p,
                &self.context(Operation::Issue, b, None, true),
                &device(),
            )
            .await
            .unwrap()
        {
            SessionStart::Authenticated(s) => *s,
            _ => panic!("unexpected factor required"),
        }
    }
    async fn pending(&self, b: u8) -> PendingLogin {
        let p = self.proof(b, None).await;
        match self
            .a
            .auth
            .start_session(
                &p,
                &self.context(Operation::Issue, b, None, true),
                &device(),
            )
            .await
            .unwrap()
        {
            SessionStart::FactorRequired(p) => p,
            _ => panic!("factor bypass"),
        }
    }
    async fn factor(&self) -> (FactorEnrollment, FactorActivation) {
        let s = self.session(1).await;
        let p = self.proof(2, Some(&s)).await;
        let e = self
            .a
            .auth
            .begin_totp_enrollment(
                &s.handle,
                &p,
                &self.context(Operation::BeginFactor, 2, Some(&s), true),
            )
            .await
            .unwrap();
        let code = code(&e.seed, self.a.now());
        let active = self
            .a
            .auth
            .confirm_totp_enrollment(
                &s.handle,
                &e.reference(),
                &code,
                &self.context(Operation::ConfirmFactor, 2, Some(&s), true),
            )
            .await
            .unwrap();
        (e, active)
    }
    async fn inspect(&self, t: &SessionTicket) -> Result<SessionMetadata, AuthError> {
        self.a
            .auth
            .inspect_session(
                &t.handle,
                &self.context(Operation::Read, 19, Some(t), false),
            )
            .await
    }
    async fn reopened(&self) -> HumanAuth {
        let mut a = self.a.reopened().await;
        a.browser_authority = self.browser.clone();
        a.incident_authority = self.incidents.clone();
        a
    }
}
fn code(seed: &[u8], now: i64) -> TotpCode {
    TotpCode::parse(
        totp_rs::TOTP::new(totp_rs::Algorithm::SHA1, 6, 0, 30, seed.to_vec())
            .unwrap()
            .generate(now as u64),
    )
    .unwrap()
}
#[tokio::test]
async fn primary_proof_issues_one_persistent_session_and_no_other_authority() {
    let f = Fixture::new().await;
    let p = f.proof(1, None).await;
    let c = f.context(Operation::Issue, 1, None, true);
    let first = f.a.auth.start_session(&p, &c, &device()).await.unwrap();
    let SessionStart::Authenticated(t) = first else {
        panic!("unexpected factor")
    };
    assert_eq!(t.metadata.user, f.user);
    assert!(f.inspect(&t).await.is_ok());
    assert!(matches!(
        f.a.auth.start_session(&p, &c, &device()).await,
        Err(AuthError::Denied)
    ));
    assert_eq!(f.a.count("human_sessions").await, 1);
    assert_eq!(f.a.count("financial_events").await, 0);
    assert_eq!(f.a.count("task_attempts").await, 0);
}
#[tokio::test]
async fn concurrent_primary_consumption_can_only_issue_one_session() {
    let f = Fixture::new().await;
    let p = f.proof(1, None).await;
    let c = f.context(Operation::Issue, 1, None, true);
    let d = device();
    let (a, b) = tokio::join!(
        f.a.auth.start_session(&p, &c, &d),
        f.a.auth.start_session(&p, &c, &d)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(f.a.count("human_sessions").await, 1);
}
#[tokio::test]
async fn mismatched_browser_or_expired_primary_cannot_issue_session() {
    for expire in [false, true] {
        let f = Fixture::new().await;
        let p = f.proof(1, None).await;
        if expire {
            f.a.advance(300)
        }
        let c = f.context(Operation::Issue, if expire { 1 } else { 2 }, None, true);
        assert!(matches!(
            f.a.auth.start_session(&p, &c, &device()).await,
            Err(AuthError::Denied)
        ));
        assert_eq!(f.a.count("human_sessions").await, 0);
    }
}
#[tokio::test]
async fn rotated_cookie_cannot_restore_authority_and_lost_ack_requires_fresh_auth() {
    let f = Fixture::new().await;
    let t = f.session(1).await;
    let c = f.context(Operation::Renew, 2, Some(&t), false);
    let next = f.a.auth.renew_session(&t.handle, &c).await.unwrap();
    assert_eq!(next.metadata.generation, 2);
    assert!(f.inspect(&t).await.is_err());
    assert!(f.inspect(&next).await.is_ok());
    assert!(f.a.auth.renew_session(&t.handle, &c).await.is_err());
    assert_eq!(f.a.count("human_sessions").await, 1);
}
#[tokio::test]
async fn concurrent_tabs_get_one_generation_without_global_revocation() {
    let f = Fixture::new().await;
    let t = f.session(1).await;
    let c = f.context(Operation::Renew, 2, Some(&t), false);
    let (a, b) = tokio::join!(
        f.a.auth.renew_session(&t.handle, &c),
        f.a.auth.renew_session(&t.handle, &c)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let next = a.or(b).unwrap();
    assert!(f.inspect(&next).await.is_ok());
    assert_eq!(f.a.count("human_session_tokens").await, 2);
}
#[tokio::test]
async fn renewal_cannot_cross_original_token_expiry_before_commit() {
    let mut f = Fixture::new().await;
    let t = f.session(1).await;
    f.a.advance(86400 - 1);
    let clock_context = f.a.client(1, 1);
    let _ = clock_context;
    let clock = f.a.clock_for_session_test();
    f.a.auth.hook = Box::new(move |p| {
        if p == Checkpoint::BeforeSessionCommit {
            clock.fetch_add(2, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    });
    let result =
        f.a.auth
            .renew_session(&t.handle, &f.context(Operation::Renew, 1, Some(&t), true))
            .await;
    assert!(matches!(result, Err(AuthError::Denied)));
    assert_eq!(f.a.count("human_session_tokens").await, 1);
}
#[tokio::test]
async fn stale_invalidated_families_do_not_lock_out_fresh_primary() {
    let f = Fixture::new().await;
    for b in 1..=32 {
        f.session(b).await;
    }
    let new = Password::new(hex(&random::<32>().unwrap()[..]).into_bytes()).unwrap();
    let a = f.a.grant(
        AdminAction::PasswordChange,
        Some(&f.user.0),
        f.a.auth.mac(
            b"password-change",
            &[f.user.0.as_bytes(), &1i64.to_be_bytes(), &new.0],
        ),
        true,
    );
    f.a.auth
        .replace_password(&a, &f.user, 1, &new)
        .await
        .unwrap();
    let client = f.a.client(1, 1);
    let AuthenticationOutcome::Verified(p) =
        f.a.auth
            .authenticate_password(&f.email, &new, &client)
            .await
            .unwrap()
    else {
        panic!("fresh primary denied")
    };
    assert!(
        f.a.auth
            .start_session(&p, &f.context(Operation::Issue, 1, None, true), &device())
            .await
            .is_ok()
    );
}
#[tokio::test]
async fn secure_cookie_delivery_is_restricted_and_does_not_serialize_bearers() {
    let f = Fixture::new().await;
    let t = f.session(1).await;
    f.a.auth
        .deliver_session(&t, &t.delivery.as_ref().unwrap().context)
        .await
        .unwrap();
    {
        let cookies = f.browser.cookies.lock().unwrap();
        assert_eq!(cookies.len(), 1);
        assert!(cookies[0].contains("__Host-rolvya="));
        assert!(cookies[0].contains("Secure; HttpOnly; SameSite=Strict"));
        assert!(!format!("{t:?}").contains(t.handle.0.as_str()));
    }
    let mut a = f.reopened().await;
    a.browser_authority = Arc::new(Unimplemented);
    assert_eq!(
        a.deliver_session(&t, &t.delivery.as_ref().unwrap().context)
            .await
            .unwrap_err(),
        AuthError::Unavailable
    );
    assert_eq!(f.browser.cookies.lock().unwrap().len(), 1);
}
#[tokio::test]
async fn tls_origin_method_csrf_and_cookie_confusion_are_rejected() {
    let f = Fixture::new().await;
    let t = f.session(1).await;
    for mode in 0..9 {
        let c = f.context(Operation::Renew, 1, Some(&t), true);
        {
            let mut grants = f.browser.grants.lock().unwrap();
            let facts = &mut grants.get_mut(c.evidence.as_str()).unwrap().facts;
            match mode {
                0 => facts.secure = false,
                1 => facts.origin = "http://rolvya.invalid".into(),
                2 => facts.origin = "https://evil.invalid".into(),
                3 => facts.origin = "https://rolvya.invalid/path".into(),
                4 => facts.method = Method::Get,
                5 => facts.same_origin = false,
                6 => facts.csrf = None,
                7 => facts.csrf = Some([0; 32]),
                _ => facts.credential = Some([0; 32]),
            }
        }
        assert!(f.a.auth.renew_session(&t.handle, &c).await.is_err());
        assert_eq!(f.a.count("human_session_tokens").await, 1);
    }
    assert!(f.inspect(&t).await.is_ok());
}
#[tokio::test]
async fn seven_day_idle_expires_despite_eight_days_of_automatic_background_activity() {
    let f = Fixture::new().await;
    let mut t = f.session(1).await;
    let original = t.metadata.last_human_at;
    for half_day in 1..=16 {
        f.a.advance(12 * 3600);
        let c = f.context(Operation::Renew, 1, Some(&t), false);
        let result = f.a.auth.renew_session(&t.handle, &c).await;
        if half_day < 14 {
            t = result.unwrap();
            assert_eq!(t.metadata.last_human_at, original)
        } else {
            assert!(result.is_err())
        }
    }
    let last: i64 = sqlx::query_scalar("SELECT max(last_human_at) FROM human_sessions")
        .fetch_one(&f.a.store.pool)
        .await
        .unwrap();
    assert_eq!(last, original);
}
#[tokio::test]
async fn only_trusted_human_activity_extends_idle_without_removing_absolute_bounds() {
    let f = Fixture::new().await;
    let mut t = f.session(1).await;
    let absolute = t.metadata.absolute_expires;
    for _ in 0..16 {
        f.a.advance(12 * 3600);
        t =
            f.a.auth
                .renew_session(&t.handle, &f.context(Operation::Renew, 1, Some(&t), true))
                .await
                .unwrap();
    }
    assert_eq!(t.metadata.last_human_at, f.a.now());
    assert_eq!(t.metadata.absolute_expires, absolute);
    assert!(f.inspect(&t).await.is_ok());
    f.a.advance(FAMILY_SECONDS);
    assert!(f.inspect(&t).await.is_err());
}
#[tokio::test]
async fn logout_and_single_device_revocation_preserve_unrelated_sessions() {
    let f = Fixture::new().await;
    let a = f.session(1).await;
    let b = f.session(2).await;
    f.a.auth
        .logout(&a.handle, &f.context(Operation::Write, 1, Some(&a), true))
        .await
        .unwrap();
    assert!(f.inspect(&a).await.is_err());
    assert!(f.inspect(&b).await.is_ok());
    let a = f.reopened().await;
    assert!(
        a.inspect_session(&b.handle, &f.context(Operation::Read, 2, Some(&b), false))
            .await
            .is_ok()
    );
    assert!(
        a.inspect_session(
            &a_handle_invalid(),
            &f.context(Operation::Read, 1, None, false)
        )
        .await
        .is_err()
    );
}
fn a_handle_invalid() -> SessionHandle {
    SessionHandle::parse(hex(&random::<32>().unwrap()[..])).unwrap()
}
#[tokio::test]
async fn all_device_revocation_survives_restart() {
    let f = Fixture::new().await;
    let a = f.session(1).await;
    let b = f.session(2).await;
    f.a.auth
        .revoke_all_sessions(&a.handle, &f.context(Operation::Write, 1, Some(&a), true))
        .await
        .unwrap();
    assert!(f.inspect(&a).await.is_err());
    assert!(f.inspect(&b).await.is_err());
    let reopened = f.reopened().await;
    assert!(
        reopened
            .inspect_session(&b.handle, &f.context(Operation::Read, 2, Some(&b), false))
            .await
            .is_err()
    );
}
#[tokio::test]
async fn revoked_session_cannot_recover_through_stale_rotation_or_pending_primary() {
    let f = Fixture::new().await;
    let s = f.session(1).await;
    let proof = f.proof(2, Some(&s)).await;
    f.a.auth
        .logout(&s.handle, &f.context(Operation::Write, 1, Some(&s), true))
        .await
        .unwrap();
    assert!(
        f.a.auth
            .start_session(
                &proof,
                &f.context(Operation::Issue, 2, None, true),
                &device()
            )
            .await
            .is_err()
    );
    assert!(
        f.a.auth
            .renew_session(&s.handle, &f.context(Operation::Renew, 1, Some(&s), true))
            .await
            .is_err()
    );
}
#[tokio::test]
async fn session_and_primary_fail_after_password_policy_email_or_account_epoch_changes() {
    for mode in 0..4 {
        let f = Fixture::new().await;
        let s = f.session(1).await;
        let proof = f.proof(2, None).await;
        match mode {
            0 => {
                let p = Password::new(hex(&random::<32>().unwrap()[..]).into_bytes()).unwrap();
                let a = f.a.grant(
                    AdminAction::PasswordChange,
                    Some(&f.user.0),
                    f.a.auth.mac(
                        b"password-change",
                        &[f.user.0.as_bytes(), &1i64.to_be_bytes(), &p.0],
                    ),
                    true,
                );
                f.a.auth.replace_password(&a, &f.user, 1, &p).await.unwrap();
            }
            1 => {
                let a = f.a.grant(
                    AdminAction::PolicyChange,
                    None,
                    f.a.auth
                        .mac(b"policy-change", &[&1i64.to_be_bytes(), &[1, 1]]),
                    true,
                );
                f.a.auth
                    .configure_policy(&a, 1, LoginPolicy::new(true, true).unwrap())
                    .await
                    .unwrap();
            }
            2 => {
                let email = EmailAddress::parse("changed@example.invalid").unwrap();
                let a = f.a.grant(
                    AdminAction::EmailChange,
                    Some(&f.user.0),
                    f.a.auth.mac(
                        b"email-change",
                        &[f.user.0.as_bytes(), &1i64.to_be_bytes(), email.0.as_bytes()],
                    ),
                    true,
                );
                f.a.auth
                    .replace_email(&a, &f.user, 1, &email)
                    .await
                    .unwrap();
            }
            _ => {
                let a = f.a.grant(
                    AdminAction::Restrict,
                    Some(&f.user.0),
                    f.a.auth
                        .mac(b"identity-restrict", &[f.user.0.as_bytes(), b"compromised"]),
                    true,
                );
                f.a.auth
                    .restrict_identity(&a, &f.user, AccountRestriction::Compromised)
                    .await
                    .unwrap();
            }
        }
        assert!(f.inspect(&s).await.is_err());
        assert!(
            f.a.auth
                .start_session(
                    &proof,
                    &f.context(Operation::Issue, 2, None, true),
                    &device()
                )
                .await
                .is_err()
        );
    }
}
#[tokio::test]
async fn scoped_incident_requires_independent_evidence_and_keeps_healthy_family() {
    let f = Fixture::new().await;
    let a = f.session(1).await;
    let b = f.session(2).await;
    let e = IncidentEvidence {
        request_id: req(),
        evidence: req(),
    };
    assert!(
        f.a.auth
            .contain_compromised_session(&a.handle, &e)
            .await
            .is_err()
    );
    assert!(f.inspect(&a).await.is_ok());
    f.incidents.0.lock().unwrap().insert(
        e.request_id.as_str().into(),
        (
            f.user.0.clone(),
            a.metadata.id.0.clone(),
            a.metadata.generation,
            e.evidence.as_str().into(),
        ),
    );
    f.a.auth
        .contain_compromised_session(&a.handle, &e)
        .await
        .unwrap();
    assert!(f.inspect(&a).await.is_err());
    assert!(f.inspect(&b).await.is_ok());
    assert_eq!(f.a.count("human_session_events").await, 3);
}
#[tokio::test]
async fn session_cookie_and_primary_context_cannot_cross_installations() {
    let a = Fixture::new().await;
    let b = Fixture::new().await;
    let t = a.session(1).await;
    assert!(
        b.a.auth
            .inspect_session(&t.handle, &b.context(Operation::Read, 1, Some(&t), false))
            .await
            .is_err()
    );
    let p = a.proof(2, None).await;
    assert!(
        b.a.auth
            .start_session(&p, &b.context(Operation::Issue, 2, None, true), &device())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn primary_and_session_integrity_mac_detect_record_tampering() {
    let f = Fixture::new().await;
    let p = f.proof(1, None).await;
    sqlx::query("DROP TRIGGER human_context_update")
        .execute(&f.a.store.pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE human_primary_context SET email_version=email_version+1 WHERE evidence_id=?",
    )
    .bind(&p.evidence)
    .execute(&f.a.store.pool)
    .await
    .unwrap();
    assert!(
        f.a.auth
            .start_session(&p, &f.context(Operation::Issue, 1, None, true), &device())
            .await
            .is_err()
    );
    let s = f.session(2).await;
    sqlx::query("DROP TRIGGER human_session_update")
        .execute(&f.a.store.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE human_sessions SET assurance='recovery' WHERE session_id=?")
        .bind(&s.metadata.id.0)
        .execute(&f.a.store.pool)
        .await
        .unwrap();
    assert!(f.inspect(&s).await.is_err());
}
#[tokio::test]
async fn totp_is_optional_but_active_factor_requires_a_pending_flow() {
    let f = Fixture::new().await;
    let (enrollment, active) = f.factor().await;
    assert_eq!(active.session.metadata.assurance, Assurance::Totp);
    assert!(f.inspect(&active.session).await.is_ok());
    assert_eq!(active.recovery.codes.len(), 10);
    let pending = f.pending(3).await;
    assert_eq!(f.a.count("human_sessions").await, 2);
    assert!(
        f.a.auth
            .complete_totp_login(
                &pending,
                &code(&enrollment.seed, f.a.now()),
                &f.context(Operation::CompleteFactor, 3, None, true),
                &device()
            )
            .await
            .is_err()
    );
    f.a.advance(30);
    let s =
        f.a.auth
            .complete_totp_login(
                &pending,
                &code(&enrollment.seed, f.a.now()),
                &f.context(Operation::CompleteFactor, 3, None, true),
                &device(),
            )
            .await
            .unwrap();
    assert_eq!(s.metadata.assurance, Assurance::Totp);
    assert!(f.inspect(&s).await.is_ok());
    assert!(
        f.a.auth
            .complete_totp_login(
                &pending,
                &code(&enrollment.seed, f.a.now()),
                &f.context(Operation::CompleteFactor, 3, None, true),
                &device()
            )
            .await
            .is_err()
    );
}
#[tokio::test]
async fn enrollment_needs_fresh_same_account_session_bound_proof_and_initial_code() {
    let f = Fixture::new().await;
    let s = f.session(1).await;
    let unbound = f.proof(2, None).await;
    assert!(
        f.a.auth
            .begin_totp_enrollment(
                &s.handle,
                &unbound,
                &f.context(Operation::BeginFactor, 2, Some(&s), true)
            )
            .await
            .is_err()
    );
    let p = f.proof(2, Some(&s)).await;
    let e =
        f.a.auth
            .begin_totp_enrollment(
                &s.handle,
                &p,
                &f.context(Operation::BeginFactor, 2, Some(&s), true),
            )
            .await
            .unwrap();
    let flag: i64 = sqlx::query_scalar("SELECT factor_required FROM human_users")
        .fetch_one(&f.a.store.pool)
        .await
        .unwrap();
    assert_eq!(flag, 0);
    assert!(f.inspect(&s).await.is_ok());
    f.a.auth
        .deliver_totp_provisioning(&e, &s.handle, &e.delivery.context)
        .await
        .unwrap();
    assert_eq!(*f.browser.provisioned.lock().unwrap(), 1);
    let mut wrong = code(&e.seed, f.a.now()).0.to_string().into_bytes();
    wrong[0] = if wrong[0] == b'0' { b'1' } else { b'0' };
    let wrong = TotpCode::parse(String::from_utf8(wrong).unwrap()).unwrap();
    assert!(
        f.a.auth
            .confirm_totp_enrollment(
                &s.handle,
                &e.reference(),
                &wrong,
                &f.context(Operation::ConfirmFactor, 2, Some(&s), true)
            )
            .await
            .is_err()
    );
    let flag: i64 = sqlx::query_scalar("SELECT factor_required FROM human_users")
        .fetch_one(&f.a.store.pool)
        .await
        .unwrap();
    assert_eq!(flag, 0);
}
#[tokio::test]
async fn expired_enrollment_never_activates_or_enforces_totp() {
    let f = Fixture::new().await;
    let s = f.session(1).await;
    let p = f.proof(2, Some(&s)).await;
    let e =
        f.a.auth
            .begin_totp_enrollment(
                &s.handle,
                &p,
                &f.context(Operation::BeginFactor, 2, Some(&s), true),
            )
            .await
            .unwrap();
    f.a.advance(600);
    assert!(
        f.a.auth
            .confirm_totp_enrollment(
                &s.handle,
                &e.reference(),
                &code(&e.seed, f.a.now()),
                &f.context(Operation::ConfirmFactor, 2, Some(&s), true)
            )
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT factor_required FROM human_users")
            .fetch_one(&f.a.store.pool)
            .await
            .unwrap(),
        0
    );
}
#[tokio::test]
async fn concurrent_totp_timestep_consumption_issues_one_session() {
    let f = Fixture::new().await;
    let (e, _) = f.factor().await;
    f.a.advance(30);
    let a = f.pending(3).await;
    let b = f.pending(4).await;
    let otp = code(&e.seed, f.a.now());
    let ca = f.context(Operation::CompleteFactor, 3, None, true);
    let cb = f.context(Operation::CompleteFactor, 4, None, true);
    let d = device();
    let (a, b) = tokio::join!(
        f.a.auth.complete_totp_login(&a, &otp, &ca, &d),
        f.a.auth.complete_totp_login(&b, &otp, &cb, &d)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
}
#[tokio::test]
async fn invalid_expired_and_replayed_totp_steps_remain_denied_after_restart() {
    let f = Fixture::new().await;
    let (e, _) = f.factor().await;
    let old = code(&e.seed, f.a.now());
    f.a.advance(120);
    let pending = f.pending(3).await;
    assert!(
        f.a.auth
            .complete_totp_login(
                &pending,
                &old,
                &f.context(Operation::CompleteFactor, 3, None, true),
                &device()
            )
            .await
            .is_err()
    );
    let a = f.reopened().await;
    let current = code(&e.seed, f.a.now());
    let t = a
        .complete_totp_login(
            &pending,
            &current,
            &f.context(Operation::CompleteFactor, 3, None, true),
            &device(),
        )
        .await
        .unwrap();
    assert!(f.inspect(&t).await.is_ok());
    let another = f.pending(4).await;
    assert!(
        a.complete_totp_login(
            &another,
            &current,
            &f.context(Operation::CompleteFactor, 4, None, true),
            &device()
        )
        .await
        .is_err()
    );
}
#[tokio::test]
async fn recovery_code_is_one_use_and_revokes_old_sessions_without_removing_factor() {
    let f = Fixture::new().await;
    let (_, active) = f.factor().await;
    let pending = f.pending(3).await;
    let recovery = &active.recovery.codes[0];
    let t =
        f.a.auth
            .complete_recovery_login(
                &pending,
                recovery,
                &f.context(Operation::CompleteFactor, 3, None, true),
                &device(),
            )
            .await
            .unwrap();
    assert_eq!(t.metadata.assurance, Assurance::Recovery);
    assert!(f.inspect(&active.session).await.is_err());
    assert!(f.inspect(&t).await.is_ok());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT factor_required FROM human_users")
            .fetch_one(&f.a.store.pool)
            .await
            .unwrap(),
        1
    );
    let p = f.pending(4).await;
    assert!(
        f.a.auth
            .complete_recovery_login(
                &p,
                recovery,
                &f.context(Operation::CompleteFactor, 4, None, true),
                &device()
            )
            .await
            .is_err()
    );
    assert_eq!(
        f.a.auth
            .remove_totp(&t.handle, &f.context(Operation::Write, 3, Some(&t), true))
            .await
            .unwrap_err(),
        AuthError::Unavailable
    );
}
#[tokio::test]
async fn concurrent_recovery_consumption_has_one_effect() {
    let f = Fixture::new().await;
    let (_, active) = f.factor().await;
    let a = f.pending(3).await;
    let b = f.pending(4).await;
    let ca = f.context(Operation::CompleteFactor, 3, None, true);
    let cb = f.context(Operation::CompleteFactor, 4, None, true);
    let d = device();
    let (a, b) = tokio::join!(
        f.a.auth
            .complete_recovery_login(&a, &active.recovery.codes[0], &ca, &d),
        f.a.auth
            .complete_recovery_login(&b, &active.recovery.codes[0], &cb, &d)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM human_recovery_codes WHERE state='consumed'"
        )
        .fetch_one(&f.a.store.pool)
        .await
        .unwrap(),
        1
    );
}
#[tokio::test]
async fn factor_seed_tampering_and_metadata_scope_substitution_fail_authentication() {
    for column in [
        "ciphertext",
        "wrapped_dek",
        "data_nonce",
        "wrap_nonce",
        "key_id",
    ] {
        let f = Fixture::new().await;
        let (e, _) = f.factor().await;
        f.a.advance(30);
        let p = f.pending(3).await;
        sqlx::query("DROP TRIGGER human_factor_update")
            .execute(&f.a.store.pool)
            .await
            .unwrap();
        let statement = format!(
            "UPDATE human_factors SET {column}=randomblob(length({column})) WHERE user_id=?"
        );
        sqlx::query(&statement)
            .bind(&f.user.0)
            .execute(&f.a.store.pool)
            .await
            .unwrap();
        assert!(
            f.a.auth
                .complete_totp_login(
                    &p,
                    &code(&e.seed, f.a.now()),
                    &f.context(Operation::CompleteFactor, 3, None, true),
                    &device()
                )
                .await
                .is_err()
        );
    }
}
#[tokio::test]
async fn ciphertext_only_storage_in_db_wal_snapshot_includes_session_recovery_and_seed() {
    let f = Fixture::new().await;
    let (e, active) = f.factor().await;
    let path = f.a.root.path().join("data/session-snapshot.sqlite");
    f.a.store.snapshot(&path).await.unwrap();
    let mut artifacts = vec![
        std::fs::read(&f.a.path).unwrap(),
        std::fs::read(path).unwrap(),
    ];
    let wal = f.a.path.with_extension("sqlite-wal");
    if wal.exists() {
        artifacts.push(std::fs::read(wal).unwrap());
    }
    for bytes in artifacts {
        for secret in [
            e.seed.as_slice(),
            active.session.handle.0.as_bytes(),
            active.recovery.codes[0].0.as_bytes(),
            active.session.csrf.as_bytes(),
        ] {
            assert!(
                !bytes.windows(secret.len()).any(|s| s == secret),
                "plaintext persisted"
            );
        }
    }
    f.a.store.check_integrity().await.unwrap();
}
#[tokio::test]
async fn factor_guess_ceiling_and_pending_proof_expiry_are_persistent() {
    let f = Fixture::new().await;
    let (e, _) = f.factor().await;
    f.a.advance(30);
    let p = f.pending(3).await;
    let mut wrong = code(&e.seed, f.a.now()).0.to_string().into_bytes();
    wrong[0] = if wrong[0] == b'0' { b'1' } else { b'0' };
    let wrong = TotpCode::parse(String::from_utf8(wrong).unwrap()).unwrap();
    for _ in 0..5 {
        assert!(
            f.a.auth
                .complete_totp_login(
                    &p,
                    &wrong,
                    &f.context(Operation::CompleteFactor, 3, None, true),
                    &device()
                )
                .await
                .is_err()
        );
    }
    let reopened = f.reopened().await;
    assert!(
        reopened
            .complete_totp_login(
                &p,
                &code(&e.seed, f.a.now()),
                &f.context(Operation::CompleteFactor, 3, None, true),
                &device()
            )
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT failures FROM human_pending_logins WHERE pending_id=?"
        )
        .bind(&p.id)
        .fetch_one(&f.a.store.pool)
        .await
        .unwrap(),
        5
    );
    f.a.advance(300);
    assert!(
        reopened
            .complete_totp_login(
                &p,
                &code(&e.seed, f.a.now()),
                &f.context(Operation::CompleteFactor, 3, None, true),
                &device()
            )
            .await
            .is_err()
    );
}
#[tokio::test]
async fn guarded_recovery_rotation_revokes_old_batch_and_all_prior_sessions() {
    let f = Fixture::new().await;
    let (e, active) = f.factor().await;
    f.a.advance(30);
    let proof = f.proof(4, Some(&active.session)).await;
    let next =
        f.a.auth
            .rotate_recovery_codes(
                &active.session.handle,
                &proof,
                &code(&e.seed, f.a.now()),
                &f.context(Operation::RotateRecovery, 4, Some(&active.session), true),
            )
            .await
            .unwrap();
    assert!(f.inspect(&active.session).await.is_err());
    assert!(f.inspect(&next.session).await.is_ok());
    let pending = f.pending(5).await;
    assert!(
        f.a.auth
            .complete_recovery_login(
                &pending,
                &active.recovery.codes[0],
                &f.context(Operation::CompleteFactor, 5, None, true),
                &device()
            )
            .await
            .is_err()
    );
    f.a.auth
        .deliver_recovery_bundle(
            &next.session,
            next.recovery,
            &next.session.delivery.as_ref().unwrap().context,
        )
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM human_recovery_codes WHERE state='revoked'"
        )
        .fetch_one(&f.a.store.pool)
        .await
        .unwrap(),
        10
    );
}
#[tokio::test]
async fn enrollment_and_pending_login_reconstruct_after_restart_without_plaintext_cache() {
    let f = Fixture::new().await;
    let s = f.session(1).await;
    let p = f.proof(2, Some(&s)).await;
    let e =
        f.a.auth
            .begin_totp_enrollment(
                &s.handle,
                &p,
                &f.context(Operation::BeginFactor, 2, Some(&s), true),
            )
            .await
            .unwrap();
    let reference = FactorId::parse(e.reference().as_str().to_owned()).unwrap();
    let a = f.reopened().await;
    let active = a
        .confirm_totp_enrollment(
            &s.handle,
            &reference,
            &code(&e.seed, f.a.now()),
            &f.context(Operation::ConfirmFactor, 2, Some(&s), true),
        )
        .await
        .unwrap();
    assert!(f.inspect(&active.session).await.is_ok());
    f.a.advance(30);
    let pending = f.pending(3).await;
    f.a.auth
        .deliver_pending_login(&pending, &f.context(Operation::Issue, 3, None, true))
        .await
        .unwrap();
    let restored = PendingLogin::parse(pending.id.clone(), pending.secret.to_string()).unwrap();
    let a = f.reopened().await;
    let session = a
        .complete_totp_login(
            &restored,
            &code(&e.seed, f.a.now()),
            &f.context(Operation::CompleteFactor, 3, None, true),
            &device(),
        )
        .await
        .unwrap();
    assert!(f.inspect(&session).await.is_ok());
}
#[tokio::test]
async fn session_authority_revoked_during_commit_rolls_back_rotation() {
    let mut f = Fixture::new().await;
    let s = f.session(1).await;
    let context = f.context(Operation::Renew, 2, Some(&s), true);
    let grants = f.browser.clone();
    let key = context.evidence.as_str().to_owned();
    f.a.auth.hook = Box::new(move |p| {
        if p == Checkpoint::BeforeSessionCommit {
            grants.grants.lock().unwrap().remove(&key);
        }
        Ok(())
    });
    assert!(f.a.auth.renew_session(&s.handle, &context).await.is_err());
    assert_eq!(f.a.count("human_session_tokens").await, 1);
    assert!(f.inspect(&s).await.is_ok());
}
#[tokio::test]
async fn simultaneous_totp_failures_cannot_exceed_ceiling() {
    let f = Fixture::new().await;
    let (e, _) = f.factor().await;
    f.a.advance(30);
    let p = f.pending(3).await;
    let mut raw = code(&e.seed, f.a.now()).0.to_string().into_bytes();
    raw[0] = if raw[0] == b'0' { b'1' } else { b'0' };
    let wrong = TotpCode::parse(String::from_utf8(raw).unwrap()).unwrap();
    let c = f.context(Operation::CompleteFactor, 3, None, true);
    let d = device();
    for _ in 0..4 {
        let (a, b) = tokio::join!(
            f.a.auth.complete_totp_login(&p, &wrong, &c, &d),
            f.a.auth.complete_totp_login(&p, &wrong, &c, &d)
        );
        assert!(a.is_err() && b.is_err());
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT failures FROM human_pending_logins WHERE pending_id=?"
        )
        .bind(&p.id)
        .fetch_one(&f.a.store.pool)
        .await
        .unwrap(),
        5
    );
}
#[tokio::test]
async fn database_abort_before_consumption_preserves_proof_for_safe_retry() {
    let f = Fixture::new().await;
    let p = f.proof(1, None).await;
    let c = f.context(Operation::Issue, 1, None, true);
    sqlx::query("CREATE TRIGGER synthetic_session_failure BEFORE INSERT ON human_session_events BEGIN SELECT RAISE(ABORT,'synthetic transaction failure'); END").execute(&f.a.store.pool).await.unwrap();
    assert!(f.a.auth.start_session(&p, &c, &device()).await.is_err());
    assert_eq!(f.a.count("human_primary_uses").await, 0);
    assert_eq!(f.a.count("human_sessions").await, 0);
    sqlx::query("DROP TRIGGER synthetic_session_failure")
        .execute(&f.a.store.pool)
        .await
        .unwrap();
    assert!(f.a.auth.start_session(&p, &c, &device()).await.is_ok());
}
#[tokio::test]
async fn cross_account_session_admin_and_factor_access_are_denied() {
    let f = Fixture::new().await;
    let s = f.session(1).await;
    let password = Password::new(hex(&random::<32>().unwrap()[..]).into_bytes()).unwrap();
    let r = f.a.enrollment(
        "other@example.invalid",
        Some(&password),
        false,
        true,
        true,
        LoginPolicy::new(true, true).unwrap(),
    );
    let other = f.a.auth.enroll(&r, Some(&password)).await.unwrap();
    let AuthenticationOutcome::Verified(p) =
        f.a.auth
            .authenticate_password(&r.email, &password, &f.a.client(3, 1))
            .await
            .unwrap()
    else {
        panic!("other primary")
    };
    let SessionStart::Authenticated(t) =
        f.a.auth
            .start_session(&p, &f.context(Operation::Issue, 3, None, true), &device())
            .await
            .unwrap()
    else {
        panic!("other session")
    };
    assert_eq!(t.metadata.user, other);
    assert!(
        f.a.auth
            .revoke_session(
                &s.handle,
                &t.metadata.id,
                &f.context(Operation::Write, 1, Some(&s), true)
            )
            .await
            .is_err()
    );
    let inventory =
        f.a.auth
            .session_inventory(
                &s.handle,
                &f.context(Operation::Read, 1, Some(&s), false),
                None,
                100,
            )
            .await
            .unwrap();
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].user, f.user);
    assert!(f.inspect(&t).await.is_ok());
}
#[tokio::test]
async fn missing_or_wrong_controller_key_blocks_session_and_factor_loading() {
    let f = Fixture::new().await;
    let s = f.session(1).await;
    let other = Fixture::new().await;
    assert!(
        other
            .a
            .auth
            .inspect_session(
                &s.handle,
                &other.context(Operation::Read, 1, Some(&s), false)
            )
            .await
            .is_err()
    );
    for e in std::fs::read_dir(&f.a.keys).unwrap() {
        let p = e.unwrap().path();
        if p.is_file() && std::fs::metadata(&p).unwrap().len() > 32 {
            std::fs::remove_file(p).unwrap();
        }
    }
    let b = ControllerBootstrap::open(
        f.a.store.clone(),
        &f.a.keys,
        super::super::super::ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    assert!(matches!(
        HumanAuth::open(&b).await,
        Err(AuthError::KeyUnavailable)
    ));
}
#[tokio::test]
async fn human_session_revocation_preserves_real_admitted_worker_and_budget() {
    let f = Fixture::new().await;
    let (broker, request, lease, _) =
        crate::controller_keys::secret_broker::tests::admitted_for_session_tests(
            f.a.store.clone(),
            f.a.path.clone(),
            f.a.keys.clone(),
        )
        .await;
    let before =
        f.a.store
            .financial_snapshot(&lease.task.plan.project_id)
            .await
            .unwrap();
    broker.use_secret(&request).await.unwrap();
    let s = f.session(1).await;
    f.a.auth
        .logout(&s.handle, &f.context(Operation::Write, 1, Some(&s), true))
        .await
        .unwrap();
    broker.use_secret(&request).await.unwrap();
    let after =
        f.a.store
            .financial_snapshot(&lease.task.plan.project_id)
            .await
            .unwrap();
    assert_eq!(before.held, after.held);
    assert_eq!(before.ceiling, after.ceiling);
    assert!(f.inspect(&s).await.is_err());
}
#[tokio::test]
async fn seed_ciphertext_cannot_be_decrypted_under_another_user_or_installation() {
    let f = Fixture::new().await;
    f.factor().await;
    let row = sqlx::query("SELECT * FROM human_factors WHERE user_id=?")
        .bind(&f.user.0)
        .fetch_one(&f.a.store.pool)
        .await
        .unwrap();
    let other = Fixture::new().await;
    assert!(f.a.auth.factor_seed(&row, other.user.as_str()).is_err());
    assert!(other.a.auth.factor_seed(&row, &f.user.0).is_err());
    assert!(f.a.auth.factor_seed(&row, &f.user.0).is_ok());
}
#[tokio::test]
async fn consistent_snapshot_preserves_encrypted_factor_and_current_one_use_state() {
    let f = Fixture::new().await;
    let (e, _) = f.factor().await;
    let path = f.a.root.path().join("data/factor-restore.sqlite");
    f.a.store.snapshot(&path).await.unwrap();
    let store = PlanStore::open(&path).await.unwrap();
    let b = ControllerBootstrap::open(
        store.clone(),
        &f.a.keys,
        super::super::super::ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let mut a = HumanAuth::open(&b).await.unwrap();
    a.passwords = PasswordEngine::synthetic();
    a.transport = f.a.auth.transport.clone();
    a.clock = f.a.auth.clock.clone();
    a.browser_authority = f.browser.clone();
    f.a.advance(30);
    let AuthenticationOutcome::Verified(proof) = a
        .authenticate_password(&f.email, &f.password, &f.a.client(5, 1))
        .await
        .unwrap()
    else {
        panic!("restored primary")
    };
    let SessionStart::FactorRequired(pending) = a
        .start_session(
            &proof,
            &f.context(Operation::Issue, 5, None, true),
            &device(),
        )
        .await
        .unwrap()
    else {
        panic!("restored factor bypass")
    };
    assert!(
        a.complete_totp_login(
            &pending,
            &code(&e.seed, f.a.now()),
            &f.context(Operation::CompleteFactor, 5, None, true),
            &device()
        )
        .await
        .is_ok()
    );
    store.check_integrity().await.unwrap();
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
struct ChildClock(i64);
impl Clock for ChildClock {
    fn now(&self) -> Result<i64, crate::admission::AdmissionError> {
        Ok(self.0)
    }
}
fn crash_command(f: &Fixture, operation: &str, at: Checkpoint) -> Command {
    let mut c = Command::new(std::env::current_exe().unwrap());
    c.args([
        "--exact",
        "controller_keys::human_auth::sessions::tests::subprocess_driver",
        "--nocapture",
    ])
    .env("ROLVYA_S203B_TEST_ROOT", f.a.root.path())
    .env("ROLVYA_S203B_TEST_OPERATION", operation)
    .env("ROLVYA_S203B_TEST_TIME", f.a.now().to_string())
    .env("ROLVYA_S203B_TEST_CHECKPOINT", format!("{at:?}"))
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    c
}
#[tokio::test]
async fn subprocess_driver() {
    let Ok(root) = std::env::var("ROLVYA_S203B_TEST_ROOT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    let store = PlanStore::open(root.join("data/state.sqlite"))
        .await
        .unwrap();
    let bootstrap = ControllerBootstrap::open(
        store.clone(),
        &root.join("keys"),
        super::super::super::ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let mut auth = HumanAuth::open(&bootstrap).await.unwrap();
    auth.passwords = PasswordEngine::synthetic();
    let now: i64 = std::env::var("ROLVYA_S203B_TEST_TIME")
        .unwrap()
        .parse()
        .unwrap();
    auth.clock = Arc::new(ChildClock(now));
    let browser = Arc::new(SyntheticBrowser::default());
    auth.browser_authority = browser.clone();
    auth.hook = Box::new(|at| {
        if format!("{at:?}") == std::env::var("ROLVYA_S203B_TEST_CHECKPOINT").unwrap() {
            println!("S203B_READY {at:?}");
            std::io::stdout().flush().unwrap();
            loop {
                std::thread::park();
            }
        }
        Ok(())
    });
    // Synthetic client-held inputs cross only this bounded anonymous pipe. Never
    // environment variables, process arguments, files or ordinary DTO serialization.
    let mut input = Zeroizing::new(String::new());
    std::io::stdin()
        .take(1024)
        .read_to_string(&mut input)
        .unwrap();
    let fields: Vec<&str> = input.lines().collect();
    let operation = std::env::var("ROLVYA_S203B_TEST_OPERATION").unwrap();
    let handle = if operation == "renew" || operation == "activate" {
        Some(SessionHandle::parse(fields[0].to_owned()).unwrap())
    } else {
        None
    };
    let op = match operation.as_str() {
        "issue" => Operation::Issue,
        "renew" => Operation::Renew,
        "activate" => Operation::ConfirmFactor,
        _ => Operation::CompleteFactor,
    };
    let b = if operation == "issue" {
        1
    } else if operation == "activate" {
        2
    } else {
        3
    };
    let c = BrowserContext {
        request_id: req(),
        evidence: req(),
    };
    browser.grants.lock().unwrap().insert(
        c.evidence.as_str().into(),
        BrowserGrant {
            request: c.request_id.as_str().into(),
            operations: vec![op],
            until: now + 3600,
            facts: BrowserFacts {
                secure: true,
                method: Method::Post,
                origin: "https://rolvya.invalid".into(),
                expected_origin: "https://rolvya.invalid".into(),
                same_origin: true,
                browser: [b; 32],
                credential: handle.as_ref().map(|h| auth.handle_lookup(h)),
                csrf: handle.as_ref().map(|_| auth.csrf_commitment(fields[1])),
                preauth_csrf: true,
                activity: Activity::Human,
                client_key: auth.mac(b"synthetic-client", &[b"192.0.2.1"]),
            },
        },
    );
    match operation.as_str() {
        "issue" => {
            let row = sqlx::query("SELECT * FROM human_authentications WHERE evidence_id=?")
                .bind(fields[0])
                .fetch_one(&store.pool)
                .await
                .unwrap();
            let p = PrimaryAuthentication {
                user: HumanId(row.get("user_id")),
                evidence: row.get("evidence_id"),
                method: PrimaryMethod::Password,
                credential_version: row.get("credential_version"),
                security_epoch: row.get("security_epoch"),
                policy_version: row.get("policy_version"),
                verified_at: row.get("verified_at"),
                stage: AuthenticationStage::PrimaryVerified,
            };
            auth.start_session(&p, &c, &device()).await.unwrap();
        }
        "renew" => {
            auth.renew_session(handle.as_ref().unwrap(), &c)
                .await
                .unwrap();
        }
        "activate" => {
            auth.confirm_totp_enrollment(
                handle.as_ref().unwrap(),
                &FactorId::parse(fields[2].to_owned()).unwrap(),
                &TotpCode::parse(fields[3].to_owned()).unwrap(),
                &c,
            )
            .await
            .unwrap();
        }
        "totp" => {
            auth.complete_totp_login(
                &PendingLogin::parse(fields[0].to_owned(), fields[1].to_owned()).unwrap(),
                &TotpCode::parse(fields[2].to_owned()).unwrap(),
                &c,
                &device(),
            )
            .await
            .unwrap();
        }
        "recovery" => {
            auth.complete_recovery_login(
                &PendingLogin::parse(fields[0].to_owned(), fields[1].to_owned()).unwrap(),
                &RecoveryCode::parse(fields[2].to_owned()).unwrap(),
                &c,
                &device(),
            )
            .await
            .unwrap();
        }
        _ => panic!("invalid test operation"),
    }
}
#[tokio::test]
async fn ten_sigkill_commit_boundaries_preserve_session_factor_and_recovery_authority() {
    for operation in ["issue", "renew", "activate", "totp", "recovery"] {
        for committed in [false, true] {
            let f = Fixture::new().await;
            let mut proof = None;
            let mut ticket = None;
            let mut enrollment = None;
            let mut pending = None;
            let mut active = None;
            let mut input = Zeroizing::new(String::new());
            match operation {
                "issue" => {
                    let p = f.proof(1, None).await;
                    input.push_str(&p.evidence);
                    input.push('\n');
                    proof = Some(p);
                }
                "renew" => {
                    let t = f.session(1).await;
                    input.push_str(t.handle.0.as_str());
                    input.push('\n');
                    input.push_str(&t.csrf);
                    input.push('\n');
                    ticket = Some(t);
                }
                "activate" => {
                    let t = f.session(1).await;
                    let p = f.proof(2, Some(&t)).await;
                    let e =
                        f.a.auth
                            .begin_totp_enrollment(
                                &t.handle,
                                &p,
                                &f.context(Operation::BeginFactor, 2, Some(&t), true),
                            )
                            .await
                            .unwrap();
                    input.push_str(t.handle.0.as_str());
                    input.push('\n');
                    input.push_str(&t.csrf);
                    input.push('\n');
                    input.push_str(e.reference().as_str());
                    input.push('\n');
                    input.push_str(&code(&e.seed, f.a.now()).0);
                    input.push('\n');
                    ticket = Some(t);
                    enrollment = Some(e);
                }
                _ => {
                    let (e, a) = f.factor().await;
                    f.a.advance(30);
                    let p = f.pending(3).await;
                    input.push_str(&p.id);
                    input.push('\n');
                    input.push_str(&p.secret);
                    input.push('\n');
                    if operation == "totp" {
                        input.push_str(&code(&e.seed, f.a.now()).0)
                    } else {
                        input.push_str(&a.recovery.codes[0].0)
                    }
                    input.push('\n');
                    enrollment = Some(e);
                    active = Some(a);
                    pending = Some(p);
                }
            }
            let at = if operation == "activate" {
                if committed {
                    Checkpoint::AfterFactorCommit
                } else {
                    Checkpoint::BeforeFactorCommit
                }
            } else if committed {
                Checkpoint::AfterSessionCommit
            } else {
                Checkpoint::BeforeSessionCommit
            };
            let mut child = crash_command(&f, operation, at).spawn().unwrap();
            {
                let mut stdin = child.stdin.take().unwrap();
                stdin.write_all(input.as_bytes()).unwrap();
            }
            let stdout = child.stdout.take().unwrap();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if line.starts_with("S203B_READY") {
                        let _ = tx.send(());
                    }
                }
            });
            let mut victim = Victim(child);
            assert!(
                rx.recv_timeout(Duration::from_secs(30)).is_ok(),
                "crash barrier not reached: {operation} {at:?}"
            );
            victim.0.kill().unwrap();
            victim.0.wait().unwrap();
            let reopened = f.reopened().await;
            reopened.store.check_integrity().await.unwrap();
            match operation {
                "issue" => {
                    assert_eq!(f.a.count("human_sessions").await, i64::from(committed));
                    let result = reopened
                        .start_session(
                            proof.as_ref().unwrap(),
                            &f.context(Operation::Issue, 1, None, true),
                            &device(),
                        )
                        .await;
                    assert_eq!(result.is_ok(), !committed);
                    assert_eq!(f.a.count("human_sessions").await, 1);
                }
                "renew" => {
                    let t = ticket.as_ref().unwrap();
                    assert_eq!(
                        f.a.count("human_session_tokens").await,
                        if committed { 2 } else { 1 }
                    );
                    assert_eq!(
                        reopened
                            .renew_session(
                                &t.handle,
                                &f.context(Operation::Renew, 3, Some(t), true)
                            )
                            .await
                            .is_ok(),
                        !committed
                    );
                    assert_eq!(f.a.count("human_session_tokens").await, 2);
                }
                "activate" => {
                    let t = ticket.as_ref().unwrap();
                    let e = enrollment.as_ref().unwrap();
                    assert_eq!(
                        sqlx::query_scalar::<_, i64>("SELECT factor_required FROM human_users")
                            .fetch_one(&f.a.store.pool)
                            .await
                            .unwrap(),
                        i64::from(committed)
                    );
                    assert_eq!(
                        reopened
                            .confirm_totp_enrollment(
                                &t.handle,
                                &e.reference(),
                                &code(&e.seed, f.a.now()),
                                &f.context(Operation::ConfirmFactor, 2, Some(t), true)
                            )
                            .await
                            .is_ok(),
                        !committed
                    );
                    assert_eq!(
                        sqlx::query_scalar::<_, i64>(
                            "SELECT count(*) FROM human_factors WHERE state='active'"
                        )
                        .fetch_one(&f.a.store.pool)
                        .await
                        .unwrap(),
                        1
                    );
                    assert_eq!(f.a.count("human_recovery_codes").await, 10);
                }
                _ => {
                    let p = pending.as_ref().unwrap();
                    let c = f.context(Operation::CompleteFactor, 3, None, true);
                    let result = if operation == "totp" {
                        reopened
                            .complete_totp_login(
                                p,
                                &code(&enrollment.as_ref().unwrap().seed, f.a.now()),
                                &c,
                                &device(),
                            )
                            .await
                    } else {
                        reopened
                            .complete_recovery_login(
                                p,
                                &active.as_ref().unwrap().recovery.codes[0],
                                &c,
                                &device(),
                            )
                            .await
                    };
                    assert_eq!(result.is_ok(), !committed);
                    assert_eq!(f.a.count("human_sessions").await, 3);
                    if operation == "recovery" {
                        assert_eq!(
                            sqlx::query_scalar::<_, i64>(
                                "SELECT count(*) FROM human_recovery_codes WHERE state='consumed'"
                            )
                            .fetch_one(&f.a.store.pool)
                            .await
                            .unwrap(),
                            1
                        );
                    }
                }
            }
        }
    }
}
#[tokio::test]
async fn renewal_cookie_is_deliverable_to_its_own_accepted_request() {
    let f = Fixture::new().await;
    let s = f.session(1).await;
    let c = f.context(Operation::Renew, 3, Some(&s), true);
    let next = f.a.auth.renew_session(&s.handle, &c).await.unwrap();
    assert!(f.a.auth.deliver_session(&next, &c).await.is_ok());
}
#[tokio::test]
async fn repeated_rotation_is_bounded_without_revoking_the_valid_cookie() {
    let f = Fixture::new().await;
    let s = f.session(1).await;
    let next =
        f.a.auth
            .renew_session(&s.handle, &f.context(Operation::Renew, 1, Some(&s), true))
            .await
            .unwrap();
    assert!(matches!(
        f.a.auth
            .renew_session(
                &next.handle,
                &f.context(Operation::Renew, 1, Some(&next), true)
            )
            .await,
        Err(AuthError::Busy)
    ));
    assert!(f.inspect(&next).await.is_ok());
    f.a.advance(600);
    assert!(
        f.a.auth
            .renew_session(
                &next.handle,
                &f.context(Operation::Renew, 1, Some(&next), true)
            )
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn recovery_bundle_cannot_be_delivered_through_another_same_user_login() {
    let f = Fixture::new().await;
    let (e, active) = f.factor().await;
    f.a.advance(30);
    let pending = f.pending(3).await;
    let context = f.context(Operation::CompleteFactor, 3, None, true);
    let other =
        f.a.auth
            .complete_totp_login(&pending, &code(&e.seed, f.a.now()), &context, &device())
            .await
            .unwrap();
    assert!(
        f.a.auth
            .deliver_recovery_bundle(&other, active.recovery, &context)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn unavailable_email_families_cannot_strand_a_working_password_route() {
    let mut f = Fixture::new().await;
    for _ in 0..32 {
        let (receipt, token, client) =
            f.a.challenge(&f.email, ChallengeKind::MagicLink, ChallengePurpose::Login)
                .await;
        let Redemption::PrimaryVerified(proof) =
            f.a.redeem(&receipt, &token, ChallengeKind::MagicLink, &client)
                .await
        else {
            panic!("email proof denied")
        };
        assert!(matches!(
            f.a.auth
                .start_session(
                    &proof,
                    &f.context(Operation::Issue, 1, None, true),
                    &device()
                )
                .await
                .unwrap(),
            SessionStart::Authenticated(_)
        ));
        // Bounded issuance windows; all families remain within credential/idle bounds.
        f.a.advance(900);
    }
    f.a.auth.mail = Arc::new(Unimplemented);
    f.a.auth.delivery = Arc::new(Unimplemented);
    let proof = f.proof(2, None).await;
    assert!(matches!(
        f.a.auth
            .start_session(
                &proof,
                &f.context(Operation::Issue, 2, None, true),
                &device()
            )
            .await
            .unwrap(),
        SessionStart::Authenticated(_)
    ));
}

#[tokio::test]
async fn recovery_bundle_cannot_follow_a_renewed_generation_to_another_response() {
    let f = Fixture::new().await;
    let (_, active) = f.factor().await;
    let context = f.context(Operation::Renew, 3, Some(&active.session), true);
    let renewed =
        f.a.auth
            .renew_session(&active.session.handle, &context)
            .await
            .unwrap();
    assert!(
        f.a.auth
            .deliver_recovery_bundle(&renewed, active.recovery, &context)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn switching_to_either_email_primary_method_cannot_bypass_enrolled_totp() {
    let f = Fixture::new().await;
    let _active = f.factor().await;
    let sessions = f.a.count("human_sessions").await;
    for kind in [ChallengeKind::MagicLink, ChallengeKind::Otp] {
        f.a.advance(900);
        let (receipt, token, client) = f.a.challenge(&f.email, kind, ChallengePurpose::Login).await;
        let Redemption::PrimaryVerified(proof) = f.a.redeem(&receipt, &token, kind, &client).await
        else {
            panic!("primary verification denied")
        };
        assert_eq!(proof.stage(), AuthenticationStage::AdditionalFactorRequired);
        assert!(matches!(
            f.a.auth
                .start_session(
                    &proof,
                    &f.context(Operation::Issue, 1, None, true),
                    &device()
                )
                .await
                .unwrap(),
            SessionStart::FactorRequired(_)
        ));
    }
    assert_eq!(f.a.count("human_sessions").await, sessions);
    assert_eq!(f.a.count("human_pending_logins").await, 2);
}
