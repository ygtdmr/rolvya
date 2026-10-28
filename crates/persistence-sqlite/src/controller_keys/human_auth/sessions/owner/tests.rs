use super::*;
#[tokio::test]
async fn legacy_owner_claim_is_not_a_human_project_membership() {
    let f = super::super::tests::Fixture::new().await;
    let s = f.session(1).await;
    let p = ProjectId::new("legacy-project").unwrap();
    sqlx::query("INSERT INTO projects(project_id,owner_actor) VALUES(?,?)")
        .bind(p.as_str())
        .bind(&f.user.0)
        .execute(&f.a.store.pool)
        .await
        .unwrap();
    assert!(
        f.a.auth
            .project_membership(
                &s.handle,
                &p,
                &f.context(Operation::Read, 1, Some(&s), true)
            )
            .await
            .is_err()
    );
    let request = Administration {
        request_id: RequestId::new(new_id().unwrap()).unwrap(),
        evidence: RequestId::new(new_id().unwrap()).unwrap(),
    };
    assert!(
        f.a.auth
            .bind_existing_project(
                &s.handle,
                &p,
                &request,
                &f.context(Operation::Write, 1, Some(&s), true)
            )
            .await
            .is_err()
    );
    assert_eq!(f.a.count("human_memberships").await, 0);
}

use super::super::tests::{Fixture as SessionFixture, code};
use std::{collections::BTreeMap, sync::Mutex};
type SyntheticBindingGrant = (String, String, String, String, i64);
#[derive(Default)]
struct BindingAuthority(Mutex<BTreeMap<String, SyntheticBindingGrant>>);
impl ProjectBindingAuthority for BindingAuthority {
    fn verify(&self, c: &BindingContext<'_>) -> Result<(), AuthError> {
        let map = self.0.lock().unwrap();
        let (p, u, legacy, e, expires) = map
            .get(c.request.request_id.as_str())
            .ok_or(AuthError::Denied)?;
        if p != c.project.as_str()
            || u != c.user
            || legacy != c.legacy
            || e != c.request.evidence.as_str()
            || c.now >= *expires
        {
            return Err(AuthError::Denied);
        }
        Ok(())
    }
}
struct Account {
    id: HumanId,
    credential: Option<Password>,
    email: EmailAddress,
    session: SessionTicket,
}
struct Fixture {
    s: SessionFixture,
    owner: SessionTicket,
    project: ProjectId,
    binding: Arc<BindingAuthority>,
}
fn request() -> RequestId {
    RequestId::new(new_id().unwrap()).unwrap()
}
fn usd(v: i64) -> Money {
    Money::new(v, Currency::new("USD").unwrap()).unwrap()
}
fn budget(v: u64, old: i64, new: i64) -> OwnerAction {
    OwnerAction::budget(v, usd(old), usd(new)).unwrap()
}
impl Fixture {
    async fn new() -> Self {
        Self::from_session(
            SessionFixture::new().await,
            ProjectId::new("project-a").unwrap(),
        )
        .await
    }
    async fn from_session(mut s: SessionFixture, project: ProjectId) -> Self {
        let owner = s.session(1).await;
        let binding = Arc::new(BindingAuthority::default());
        s.a.auth.project_binding = binding.clone();
        let f = Self {
            s,
            owner,
            project,
            binding,
        };
        f.bind(&f.project).await;
        f
    }
    async fn bind(&self, p: &ProjectId) {
        let existing: Option<String> =
            sqlx::query_scalar("SELECT owner_actor FROM projects WHERE project_id=?")
                .bind(p.as_str())
                .fetch_optional(&self.s.a.store.pool)
                .await
                .unwrap();
        let legacy = existing
            .clone()
            .unwrap_or_else(|| "legacy-only-actor".to_owned());
        if existing.is_none() {
            sqlx::query("INSERT INTO projects(project_id,owner_actor) VALUES(?,?)")
                .bind(p.as_str())
                .bind(&legacy)
                .execute(&self.s.a.store.pool)
                .await
                .unwrap();
        }
        let r = Administration {
            request_id: request(),
            evidence: request(),
        };
        self.binding.0.lock().unwrap().insert(
            r.request_id.as_str().to_owned(),
            (
                p.as_str().to_owned(),
                self.s.user.0.clone(),
                legacy,
                r.evidence.as_str().to_owned(),
                self.s.a.now() + 60,
            ),
        );
        self.s
            .a
            .auth
            .bind_existing_project(
                &self.owner.handle,
                p,
                &r,
                &self.ctx(&self.owner, Operation::Write, 1),
            )
            .await
            .unwrap();
    }
    fn ctx(&self, s: &SessionTicket, op: Operation, b: u8) -> BrowserContext {
        self.s.context(op, b, Some(s), true)
    }
    async fn account(&self, n: u8, local: bool) -> Account {
        let credential =
            local.then(|| Password::new(hex(&random::<32>().unwrap()[..]).into_bytes()).unwrap());
        let r = self.s.a.enrollment(
            &format!("person{n}@example.invalid"),
            credential.as_ref(),
            false,
            true,
            true,
            LoginPolicy::new(true, true).unwrap(),
        );
        let id = self.s.a.auth.enroll(&r, credential.as_ref()).await.unwrap();
        let proof = if let Some(p) = &credential {
            let AuthenticationOutcome::Verified(a) = self
                .s
                .a
                .auth
                .authenticate_password(&r.email, p, &self.s.a.client(n, 1))
                .await
                .unwrap()
            else {
                panic!("password denied")
            };
            a
        } else {
            self.s.a.advance(900);
            let (receipt, secret, client) = self
                .s
                .a
                .challenge(&r.email, ChallengeKind::MagicLink, ChallengePurpose::Login)
                .await;
            let Redemption::PrimaryVerified(a) = self
                .s
                .a
                .redeem(&receipt, &secret, ChallengeKind::MagicLink, &client)
                .await
            else {
                panic!("email denied")
            };
            a
        };
        let browser = if local { n } else { 1 };
        let SessionStart::Authenticated(session) = self
            .s
            .a
            .auth
            .start_session(
                &proof,
                &self.s.context(Operation::Issue, browser, None, true),
                &super::super::tests::device(),
            )
            .await
            .unwrap()
        else {
            panic!("factor unexpected")
        };
        Account {
            id,
            credential,
            email: r.email,
            session: *session,
        }
    }
    async fn prepare(&self, action: &OwnerAction) -> OwnerActionId {
        self.s
            .a
            .auth
            .prepare_owner_action(
                &self.owner.handle,
                &self.project,
                action,
                &self.ctx(&self.owner, Operation::Write, 2),
            )
            .await
            .unwrap()
    }
    async fn prove(&self, id: &OwnerActionId) {
        self.prove_on(&self.owner, &self.project, id, &self.s.password, None)
            .await
            .unwrap()
    }
    async fn prove_on(
        &self,
        s: &SessionTicket,
        p: &ProjectId,
        id: &OwnerActionId,
        password: &Password,
        totp: Option<&TotpCode>,
    ) -> Result<(), AuthError> {
        let c = self.ctx(s, Operation::Write, 3);
        let client = self.s.a.client(3, 1);
        let q = OwnerProofRequest {
            session: &s.handle,
            project: p,
            action: id,
            confirmation: Confirmation::Affirmative,
            context: &c,
        };
        self.s
            .a
            .auth
            .prove_owner_password(&q, password, &client, totp)
            .await
    }
    async fn execute(
        &self,
        id: &OwnerActionId,
        a: &OwnerAction,
    ) -> Result<GovernanceReceipt, AuthError> {
        self.s
            .a
            .auth
            .execute_owner_action(
                &self.owner.handle,
                &self.project,
                id,
                a,
                Confirmation::Affirmative,
                &self.ctx(&self.owner, Operation::Write, 4),
            )
            .await
    }
    async fn invite(&self, a: &Account, role: ProjectRole) -> InvitationTicket {
        self.s
            .a
            .auth
            .issue_project_invitation(
                &self.owner.handle,
                &self.project,
                &a.id,
                role,
                &self.ctx(&self.owner, Operation::Write, 5),
            )
            .await
            .unwrap()
    }
    async fn redeem(&self, a: &Account, t: &InvitationTicket) -> Result<Membership, AuthError> {
        self.s
            .a
            .auth
            .redeem_project_invitation(
                &a.session.handle,
                &self.project,
                &t.id,
                t.secret.as_ref().unwrap(),
                &self.ctx(&a.session, Operation::Write, 6),
            )
            .await
    }
    async fn promote(&self, a: &Account, version: i64, transfer: bool) -> OwnerActionId {
        let action = OwnerAction::elevation(&a.id, version, transfer).unwrap();
        let id = self.prepare(&action).await;
        self.prove(&id).await;
        self.s
            .a
            .auth
            .accept_owner_elevation(
                &a.session.handle,
                &id,
                Confirmation::Affirmative,
                &self.ctx(&a.session, Operation::Write, 6),
            )
            .await
            .unwrap();
        self.execute(&id, &action).await.unwrap();
        id
    }
}
#[tokio::test]
async fn actual_password_owner_without_totp_commits_budget_proof_audit_and_no_dispatch() {
    let f = Fixture::new().await;
    let a = budget(0, 0, 1000);
    let id = f.prepare(&a).await;
    assert!(f.execute(&id, &a).await.is_err());
    f.prove(&id).await;
    assert_eq!(f.s.a.count("budget_approvals").await, 0);
    let receipt = f.execute(&id, &a).await.unwrap();
    assert!(receipt.financial_sequence.is_some());
    assert_eq!(f.s.a.count("human_owner_effects").await, 1);
    assert_eq!(f.s.a.count("budget_approvals").await, 1);
    assert_eq!(
        f.s.a
            .store
            .financial_snapshot(&f.project)
            .await
            .unwrap()
            .ceiling,
        usd(1000)
    );
    for table in ["task_attempts", "dispatch_outbox"] {
        assert_eq!(f.s.a.count(table).await, 0)
    }
    assert!(f.execute(&id, &a).await.is_err());
    assert_eq!(f.s.a.count("budget_approvals").await, 1);
    let actor: String = sqlx::query_scalar("SELECT actor FROM financial_events")
        .fetch_one(&f.s.a.store.pool)
        .await
        .unwrap();
    assert_eq!(actor, f.s.user.0);
    assert_ne!(actor, "legacy-only-actor");
}
#[tokio::test]
async fn legacy_callback_and_constructed_owner_proof_cannot_approve_a_human_installation() {
    struct Claim;
    impl crate::finance::FinancialAuthorization for Claim {
        fn permits(&self, _: &PrincipalId, _: &crate::finance::FinancialRequest, _: i64) -> bool {
            true
        }
    }
    let f = Fixture::new().await;
    for p in [
        f.project.clone(),
        ProjectId::new("unbound-project").unwrap(),
    ] {
        if p != f.project {
            sqlx::query(
                "INSERT INTO projects(project_id,owner_actor) VALUES(?,'legacy-only-actor')",
            )
            .bind(p.as_str())
            .execute(&f.s.a.store.pool)
            .await
            .unwrap();
        }
        let r = crate::finance::FinancialRequest {
            project: p,
            request_id: request(),
            actor: PrincipalId::new("legacy-only-actor").unwrap(),
            reason: "Synthetic unauthorized claim".to_owned(),
            command: crate::finance::FinancialCommand::Approve {
                expected_version: 0,
                new_ceiling: usd(1000),
                proof: crate::finance::OwnerProof {
                    reference: crate::finance::EvidenceRef::new(new_id().unwrap()).unwrap(),
                    verified_at: crate::finance::now().unwrap(),
                },
            },
        };
        assert!(f.s.a.store.apply_financial(&r, &Claim).await.is_err());
    }
    assert_eq!(f.s.a.count("financial_events").await, 0);
}
#[tokio::test]
async fn normal_session_and_login_evidence_never_create_an_owner_approval() {
    let f = Fixture::new().await;
    let a = budget(0, 0, 1000);
    let id = f.prepare(&a).await;
    let _ordinary = f.s.proof(2, Some(&f.owner)).await;
    assert!(f.execute(&id, &a).await.is_err());
    assert_eq!(f.s.a.count("human_owner_proofs").await, 0);
}
#[tokio::test]
async fn scoped_roles_deny_editor_and_viewer_governance_and_cross_project_access() {
    let f = Fixture::new().await;
    for (n, role) in [(2, ProjectRole::Editor), (3, ProjectRole::Viewer)] {
        let a = f.account(n, true).await;
        let t = f.invite(&a, role).await;
        f.redeem(&a, &t).await.unwrap();
        for permission in [
            ProjectPermission::Read,
            ProjectPermission::ManageApprovedWork,
            ProjectPermission::Govern,
        ] {
            let op = if permission == ProjectPermission::Read {
                Operation::Read
            } else {
                Operation::Write
            };
            let result =
                f.s.a
                    .auth
                    .check_project_permission(
                        &a.session.handle,
                        &f.project,
                        permission,
                        &f.ctx(&a.session, op, n),
                    )
                    .await;
            assert_eq!(
                result.is_ok(),
                permission == ProjectPermission::Read
                    || role == ProjectRole::Editor
                        && permission == ProjectPermission::ManageApprovedWork
            );
        }
        assert!(
            f.s.a
                .auth
                .prepare_owner_action(
                    &a.session.handle,
                    &f.project,
                    &budget(0, 0, 1000),
                    &f.ctx(&a.session, Operation::Write, n)
                )
                .await
                .is_err()
        );
        assert!(
            f.s.a
                .auth
                .issue_project_invitation(
                    &a.session.handle,
                    &f.project,
                    &f.s.user,
                    ProjectRole::Viewer,
                    &f.ctx(&a.session, Operation::Write, n)
                )
                .await
                .is_err()
        );
        assert!(
            f.s.a
                .auth
                .project_membership(
                    &a.session.handle,
                    &ProjectId::new("absent").unwrap(),
                    &f.ctx(&a.session, Operation::Read, n)
                )
                .await
                .is_err()
        );
    }
}
#[tokio::test]
async fn valid_session_without_membership_and_target_mismatch_cannot_redeem() {
    let f = Fixture::new().await;
    let alice = f.account(2, true).await;
    let bob = f.account(3, true).await;
    let t = f.invite(&alice, ProjectRole::Viewer).await;
    assert!(
        f.s.a
            .auth
            .project_membership(
                &alice.session.handle,
                &f.project,
                &f.ctx(&alice.session, Operation::Read, 2)
            )
            .await
            .is_err()
    );
    assert!(f.redeem(&bob, &t).await.is_err());
    assert_eq!(f.s.a.count("human_memberships").await, 1);
    f.redeem(&alice, &t).await.unwrap();
    assert!(f.redeem(&alice, &t).await.is_err());
    assert!(
        f.s.a
            .auth
            .issue_project_invitation(
                &f.owner.handle,
                &f.project,
                &bob.id,
                ProjectRole::Owner,
                &f.ctx(&f.owner, Operation::Write, 2)
            )
            .await
            .is_err()
    );
}
#[tokio::test]
async fn invitation_concurrent_consumption_creates_one_membership() {
    let f = Fixture::new().await;
    let a = f.account(2, true).await;
    let t = f.invite(&a, ProjectRole::Editor).await;
    let (x, y) = tokio::join!(f.redeem(&a, &t), f.redeem(&a, &t));
    assert_eq!(usize::from(x.is_ok()) + usize::from(y.is_ok()), 1);
    assert_eq!(f.s.a.count("human_memberships").await, 2);
}
#[tokio::test]
async fn invitation_revocation_and_stale_target_or_issuer_versions_fail_closed() {
    for mode in ["revoked", "target", "issuer", "email", "epoch"] {
        let f = Fixture::new().await;
        let a = f.account(2, true).await;
        let t = f.invite(&a, ProjectRole::Viewer).await;
        match mode {
            "revoked" => {
                f.s.a
                    .auth
                    .revoke_project_invitation(
                        &f.owner.handle,
                        &f.project,
                        &t.id,
                        1,
                        &f.ctx(&f.owner, Operation::Write, 3),
                    )
                    .await
                    .unwrap()
            }
            "target" => {
                let second = f.invite(&a, ProjectRole::Editor).await;
                f.redeem(&a, &second).await.unwrap();
                let action = OwnerAction::membership(&a.id, 1, None).unwrap();
                let id = f.prepare(&action).await;
                f.prove(&id).await;
                f.execute(&id, &action).await.unwrap();
            }
            "issuer" => {
                sqlx::query("UPDATE human_memberships SET version=version+1 WHERE project_id=? AND user_id=?").bind(f.project.as_str()).bind(&f.s.user.0).execute(&f.s.a.store.pool).await.unwrap();
            }
            "email" => {
                sqlx::query("UPDATE human_emails SET version=version+1 WHERE user_id=?")
                    .bind(&a.id.0)
                    .execute(&f.s.a.store.pool)
                    .await
                    .unwrap();
            }
            _ => {
                sqlx::query(
                    "UPDATE human_users SET security_epoch=security_epoch+1 WHERE user_id=?",
                )
                .bind(&a.id.0)
                .execute(&f.s.a.store.pool)
                .await
                .unwrap();
            }
        }
        assert!(f.redeem(&a, &t).await.is_err());
    }
}
#[tokio::test]
async fn invitation_has_exact_24_hour_expiry_even_with_renewed_recipient_cookie() {
    let f = Fixture::new().await;
    let mut a = f.account(2, true).await;
    let t = f.invite(&a, ProjectRole::Viewer).await;
    f.s.a.advance(86400 - 1);
    a.session =
        f.s.a
            .auth
            .renew_session(&a.session.handle, &f.ctx(&a.session, Operation::Renew, 2))
            .await
            .unwrap();
    f.s.a.advance(1);
    assert!(f.redeem(&a, &t).await.is_err());
    assert_eq!(f.s.a.count("human_memberships").await, 1);
}
#[tokio::test]
async fn owner_promotion_needs_independent_target_acceptance_not_an_invitation() {
    let f = Fixture::new().await;
    let a = f.account(2, true).await;
    let t = f.invite(&a, ProjectRole::Editor).await;
    f.redeem(&a, &t).await.unwrap();
    let action = OwnerAction::elevation(&a.id, 1, false).unwrap();
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    assert!(f.execute(&id, &action).await.is_err());
    assert!(
        f.s.a
            .auth
            .accept_owner_elevation(
                &a.session.handle,
                &id,
                Confirmation::Preview,
                &f.ctx(&a.session, Operation::Write, 3)
            )
            .await
            .is_err()
    );
    f.s.a
        .auth
        .accept_owner_elevation(
            &a.session.handle,
            &id,
            Confirmation::Affirmative,
            &f.ctx(&a.session, Operation::Write, 3),
        )
        .await
        .unwrap();
    f.execute(&id, &action).await.unwrap();
    assert_eq!(
        f.s.a
            .auth
            .project_membership(
                &a.session.handle,
                &f.project,
                &f.ctx(&a.session, Operation::Read, 2)
            )
            .await
            .unwrap()
            .role,
        ProjectRole::Owner
    );
    assert_eq!(
        f.s.a
            .auth
            .project_membership(
                &f.owner.handle,
                &f.project,
                &f.ctx(&f.owner, Operation::Read, 1)
            )
            .await
            .unwrap()
            .role,
        ProjectRole::Owner
    );
}
#[tokio::test]
async fn transfer_keeps_initiator_until_atomic_completion_and_never_orphans() {
    let f = Fixture::new().await;
    let a = f.account(2, true).await;
    let action = OwnerAction::elevation(&a.id, 0, true).unwrap();
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    assert!(f.execute(&id, &action).await.is_err());
    assert_eq!(
        f.s.a
            .auth
            .project_membership(
                &f.owner.handle,
                &f.project,
                &f.ctx(&f.owner, Operation::Read, 1)
            )
            .await
            .unwrap()
            .role,
        ProjectRole::Owner
    );
    f.s.a
        .auth
        .accept_owner_elevation(
            &a.session.handle,
            &id,
            Confirmation::Affirmative,
            &f.ctx(&a.session, Operation::Write, 2),
        )
        .await
        .unwrap();
    f.execute(&id, &action).await.unwrap();
    assert_eq!(
        f.s.a
            .auth
            .project_membership(
                &f.owner.handle,
                &f.project,
                &f.ctx(&f.owner, Operation::Read, 1)
            )
            .await
            .unwrap()
            .role,
        ProjectRole::Editor
    );
    assert_eq!(
        f.s.a
            .auth
            .project_membership(
                &a.session.handle,
                &f.project,
                &f.ctx(&a.session, Operation::Read, 2)
            )
            .await
            .unwrap()
            .role,
        ProjectRole::Owner
    );
}
#[tokio::test]
async fn last_owner_demotion_and_removal_roll_back_proof_and_effect() {
    for role in [None, Some(ProjectRole::Editor)] {
        let f = Fixture::new().await;
        let action = OwnerAction::membership(&f.s.user, 1, role).unwrap();
        let id = f.prepare(&action).await;
        f.prove(&id).await;
        assert!(f.execute(&id, &action).await.is_err());
        assert_eq!(f.s.a.count("human_owner_effects").await, 0);
        assert_eq!(f.s.a.count("human_owner_proofs").await, 1);
    }
}
#[tokio::test]
async fn two_owners_cannot_concurrently_remove_both_owner_memberships() {
    let f = Fixture::new().await;
    let a = f.account(2, true).await;
    f.promote(&a, 0, false).await;
    let first = OwnerAction::membership(&f.s.user, 1, None).unwrap();
    let one = f.prepare(&first).await;
    f.prove(&one).await;
    let second = OwnerAction::membership(&a.id, 1, None).unwrap();
    let two =
        f.s.a
            .auth
            .prepare_owner_action(
                &a.session.handle,
                &f.project,
                &second,
                &f.ctx(&a.session, Operation::Write, 2),
            )
            .await
            .unwrap();
    f.prove_on(
        &a.session,
        &f.project,
        &two,
        a.credential.as_ref().unwrap(),
        None,
    )
    .await
    .unwrap();
    let context = f.ctx(&a.session, Operation::Write, 3);
    let (x, y) = tokio::join!(
        f.execute(&one, &first),
        f.s.a.auth.execute_owner_action(
            &a.session.handle,
            &f.project,
            &two,
            &second,
            Confirmation::Affirmative,
            &context
        )
    );
    assert_eq!(usize::from(x.is_ok()) + usize::from(y.is_ok()), 1);
    let owners: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM human_memberships WHERE role='owner' AND state='active'",
    )
    .fetch_one(&f.s.a.store.pool)
    .await
    .unwrap();
    assert_eq!(owners, 1);
}
#[tokio::test]
async fn same_role_version_in_two_projects_cannot_substitute_action_authority() {
    let f = Fixture::new().await;
    let b = ProjectId::new("project-b").unwrap();
    f.bind(&b).await;
    let action = budget(0, 0, 1000);
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    assert!(
        f.s.a
            .auth
            .execute_owner_action(
                &f.owner.handle,
                &b,
                &id,
                &action,
                Confirmation::Affirmative,
                &f.ctx(&f.owner, Operation::Write, 4)
            )
            .await
            .is_err()
    );
    assert_eq!(f.s.a.count("budget_approvals").await, 0);
    f.execute(&id, &action).await.unwrap();
}
#[tokio::test]
async fn payload_currency_version_and_confirmation_substitutions_do_not_consume_proof() {
    let f = Fixture::new().await;
    let action = budget(0, 0, 1000);
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    for wrong in [
        budget(0, 0, 1001),
        budget(1, 0, 1000),
        OwnerAction::budget(
            0,
            Money::new(0, Currency::new("EUR").unwrap()).unwrap(),
            Money::new(1000, Currency::new("EUR").unwrap()).unwrap(),
        )
        .unwrap(),
    ] {
        assert!(f.execute(&id, &wrong).await.is_err());
    }
    assert!(
        f.s.a
            .auth
            .execute_owner_action(
                &f.owner.handle,
                &f.project,
                &id,
                &action,
                Confirmation::Preview,
                &f.ctx(&f.owner, Operation::Write, 4)
            )
            .await
            .is_err()
    );
    assert_eq!(f.s.a.count("human_owner_effects").await, 0);
    f.execute(&id, &action).await.unwrap();
}
#[tokio::test]
async fn approval_expires_exactly_five_minutes_after_fresh_proof_without_refresh() {
    let f = Fixture::new().await;
    let action = budget(0, 0, 1000);
    let id = f.prepare(&action).await;
    f.s.a.advance(600);
    f.prove(&id).await;
    f.s.a.advance(300);
    assert!(f.execute(&id, &action).await.is_err());
    assert!(
        f.prove_on(&f.owner, &f.project, &id, &f.s.password, None)
            .await
            .is_err()
    );
    assert_eq!(f.s.a.count("budget_approvals").await, 0);
}
#[tokio::test]
async fn five_minute_minus_one_and_concurrent_consumption_have_one_effect() {
    let f = Fixture::new().await;
    let action = budget(0, 0, 1000);
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    f.s.a.advance(299);
    let (x, y) = tokio::join!(f.execute(&id, &action), f.execute(&id, &action));
    assert_eq!(usize::from(x.is_ok()) + usize::from(y.is_ok()), 1);
    assert_eq!(f.s.a.count("budget_approvals").await, 1);
    assert_eq!(f.s.a.count("human_owner_effects").await, 1);
}
#[tokio::test]
async fn stale_session_role_security_and_policy_versions_deny_prepared_approvals() {
    for mode in ["logout", "rotate", "epoch", "role", "policy"] {
        let f = Fixture::new().await;
        let action = budget(0, 0, 1000);
        let id = f.prepare(&action).await;
        f.prove(&id).await;
        match mode {
            "logout" => {
                f.s.a
                    .auth
                    .logout(&f.owner.handle, &f.ctx(&f.owner, Operation::Write, 3))
                    .await
                    .unwrap()
            }
            "rotate" => {
                f.s.a
                    .auth
                    .renew_session(&f.owner.handle, &f.ctx(&f.owner, Operation::Renew, 3))
                    .await
                    .unwrap();
            }
            "epoch" => {
                sqlx::query(
                    "UPDATE human_users SET security_epoch=security_epoch+1 WHERE user_id=?",
                )
                .bind(&f.s.user.0)
                .execute(&f.s.a.store.pool)
                .await
                .unwrap();
            }
            "role" => {
                sqlx::query("UPDATE human_memberships SET version=version+1 WHERE user_id=?")
                    .bind(&f.s.user.0)
                    .execute(&f.s.a.store.pool)
                    .await
                    .unwrap();
            }
            _ => {
                sqlx::query("UPDATE human_login_policy SET version=version+1")
                    .execute(&f.s.a.store.pool)
                    .await
                    .unwrap();
            }
        }
        assert!(f.execute(&id, &action).await.is_err());
        assert_eq!(f.s.a.count("budget_approvals").await, 0);
    }
}
#[tokio::test]
async fn notification_insert_failure_rolls_back_budget_effect_and_proof_consumption() {
    let f = Fixture::new().await;
    let a = f.account(2, true).await;
    f.promote(&a, 0, false).await;
    let action = budget(0, 0, 1000);
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    let before = f.s.a.count("human_owner_effects").await;
    sqlx::query("CREATE TRIGGER fail_notice BEFORE INSERT ON human_owner_notifications BEGIN SELECT RAISE(ABORT,'injected notification failure'); END").execute(&f.s.a.store.pool).await.unwrap();
    assert!(f.execute(&id, &action).await.is_err());
    assert_eq!(f.s.a.count("budget_approvals").await, 0);
    assert_eq!(f.s.a.count("human_owner_effects").await, before);
    sqlx::query("DROP TRIGGER fail_notice")
        .execute(&f.s.a.store.pool)
        .await
        .unwrap();
    f.execute(&id, &action).await.unwrap();
    let context = f.ctx(&a.session, Operation::Read, 2);
    let notices =
        f.s.a
            .auth
            .owner_notifications(&a.session.handle, 0, 100, &context)
            .await
            .unwrap();
    assert!(!notices.is_empty());
}
#[tokio::test]
async fn stale_financial_version_does_not_consume_another_exact_action() {
    let f = Fixture::new().await;
    let a = budget(0, 0, 1000);
    let first = f.prepare(&a).await;
    let second = f.prepare(&a).await;
    f.prove(&first).await;
    f.prove(&second).await;
    f.execute(&first, &a).await.unwrap();
    assert!(f.execute(&second, &a).await.is_err());
    assert_eq!(f.s.a.count("budget_approvals").await, 1);
}
#[tokio::test]
async fn wrong_password_and_preview_do_not_create_step_up_proof() {
    let f = Fixture::new().await;
    let id = f.prepare(&budget(0, 0, 1000)).await;
    let wrong = Password::new(hex(&random::<32>().unwrap()[..]).into_bytes()).unwrap();
    assert!(
        f.prove_on(&f.owner, &f.project, &id, &wrong, None)
            .await
            .is_err()
    );
    assert_eq!(f.s.a.count("human_owner_proofs").await, 0);
    let c = f.ctx(&f.owner, Operation::Write, 3);
    let q = OwnerProofRequest {
        session: &f.owner.handle,
        project: &f.project,
        action: &id,
        confirmation: Confirmation::Preview,
        context: &c,
    };
    assert!(
        f.s.a
            .auth
            .prove_owner_password(&q, &f.s.password, &f.s.a.client(3, 1), None)
            .await
            .is_err()
    );
    f.prove(&id).await;
}

#[tokio::test]
async fn ignored_notification_write_cannot_commit_an_unnotified_financial_approval() {
    let f = Fixture::new().await;
    let a = f.account(2, true).await;
    f.promote(&a, 0, false).await;
    let action = budget(0, 0, 1000);
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    sqlx::query("CREATE TRIGGER ignore_notice BEFORE INSERT ON human_owner_notifications BEGIN SELECT RAISE(IGNORE); END").execute(&f.s.a.store.pool).await.unwrap();
    assert!(f.execute(&id, &action).await.is_err());
    assert_eq!(f.s.a.count("budget_approvals").await, 0);
}
#[tokio::test]
async fn ignored_membership_write_cannot_consume_invitation_without_grant() {
    let f = Fixture::new().await;
    let a = f.account(2, true).await;
    let t = f.invite(&a, ProjectRole::Viewer).await;
    sqlx::query("CREATE TRIGGER ignore_membership BEFORE INSERT ON human_memberships BEGIN SELECT RAISE(IGNORE); END").execute(&f.s.a.store.pool).await.unwrap();
    assert!(f.redeem(&a, &t).await.is_err());
    let state: String =
        sqlx::query_scalar("SELECT state FROM human_project_invitations WHERE invitation_id=?")
            .bind(t.id.as_str())
            .fetch_one(&f.s.a.store.pool)
            .await
            .unwrap();
    assert_eq!(state, "pending");
}

impl Fixture {
    async fn prepare_email_for(
        &self,
        a: &Account,
        id: &OwnerActionId,
        kind: ChallengeKind,
    ) -> (PreparedOwnerEmail, ChallengeSecret, ClientContext) {
        let c = self.ctx(&a.session, Operation::Write, 7);
        let client = self.s.a.client(7, 1);
        let q = OwnerProofRequest {
            session: &a.session.handle,
            project: &self.project,
            action: id,
            confirmation: Confirmation::Affirmative,
            context: &c,
        };
        let mail = self
            .s
            .a
            .auth
            .prepare_owner_email(&q, kind, &client)
            .await
            .unwrap();
        let secret = mail.synthetic_recipient_secret();
        (mail, secret, client)
    }
    async fn email_prove(
        &self,
        a: &Account,
        id: &OwnerActionId,
        challenge: &OwnerChallengeId,
        secret: &ChallengeSecret,
        attempt: (ChallengeKind, Confirmation),
        client: &ClientContext,
    ) -> Result<(), AuthError> {
        let (kind, confirm) = attempt;
        let c = self.ctx(&a.session, Operation::Write, 7);
        let q = OwnerProofRequest {
            session: &a.session.handle,
            project: &self.project,
            action: id,
            confirmation: confirm,
            context: &c,
        };
        self.s
            .a
            .auth
            .redeem_owner_email(&q, challenge, secret, kind, client, None)
            .await
    }
}
#[tokio::test]
async fn both_email_only_owner_routes_create_dedicated_fresh_proofs_without_totp() {
    for kind in [ChallengeKind::MagicLink, ChallengeKind::Otp] {
        let f = Fixture::new().await;
        let a = f.account(2, false).await;
        f.promote(&a, 0, false).await;
        f.s.a.advance(900);
        let action = budget(0, 0, 2000);
        let id =
            f.s.a
                .auth
                .prepare_owner_action(
                    &a.session.handle,
                    &f.project,
                    &action,
                    &f.ctx(&a.session, Operation::Write, 7),
                )
                .await
                .unwrap();
        let (mail, secret, client) = f.prepare_email_for(&a, &id, kind).await;
        let challenge = mail.reference().clone();
        assert!(
            f.email_prove(
                &a,
                &id,
                &challenge,
                &secret,
                (kind, Confirmation::Affirmative),
                &client
            )
            .await
            .is_err()
        );
        assert_eq!(
            f.s.a.auth.dispatch_owner_email(mail).await.unwrap(),
            MailDispatch::DeliveryVerified
        );
        assert!(
            f.email_prove(
                &a,
                &id,
                &challenge,
                &secret,
                (kind, Confirmation::Preview),
                &client
            )
            .await
            .is_err()
        );
        assert_eq!(f.s.a.count("human_owner_proofs").await, 1); // Initiator's earlier promotion only.
        f.email_prove(
            &a,
            &id,
            &challenge,
            &secret,
            (kind, Confirmation::Affirmative),
            &client,
        )
        .await
        .unwrap();
        assert!(
            f.email_prove(
                &a,
                &id,
                &challenge,
                &secret,
                (kind, Confirmation::Affirmative),
                &client
            )
            .await
            .is_err()
        );
        f.s.a
            .auth
            .execute_owner_action(
                &a.session.handle,
                &f.project,
                &id,
                &action,
                Confirmation::Affirmative,
                &f.ctx(&a.session, Operation::Write, 7),
            )
            .await
            .unwrap();
        let notices =
            f.s.a
                .auth
                .owner_notifications(
                    &f.owner.handle,
                    0,
                    100,
                    &f.ctx(&f.owner, Operation::Read, 1),
                )
                .await
                .unwrap();
        let notice = notices.iter().find(|n| n.reference == id.0).unwrap();
        assert_eq!(notice.actor, a.id);
        assert_eq!(notice.action, Some(action));
        let snapshot =
            f.s.a
                .auth
                .project_budget(
                    &a.session.handle,
                    &f.project,
                    &f.ctx(&a.session, Operation::Read, 2),
                )
                .await
                .unwrap();
        assert_eq!(snapshot.ceiling, usd(2000));
    }
}
#[tokio::test]
async fn owner_email_exact_ten_minute_expiry_and_fifth_failed_otp_are_durable() {
    for expire in [true, false] {
        let f = Fixture::new().await;
        let a = f.account(2, false).await;
        f.promote(&a, 0, false).await;
        f.s.a.advance(900);
        let action = budget(0, 0, 1000);
        let id =
            f.s.a
                .auth
                .prepare_owner_action(
                    &a.session.handle,
                    &f.project,
                    &action,
                    &f.ctx(&a.session, Operation::Write, 7),
                )
                .await
                .unwrap();
        let (mail, secret, client) = f.prepare_email_for(&a, &id, ChallengeKind::Otp).await;
        let challenge = mail.reference().clone();
        f.s.a.auth.dispatch_owner_email(mail).await.unwrap();
        if expire {
            f.s.a.advance(600);
        } else {
            let wrong = ChallengeSecret::parse(
                if secret.0.as_str() == "11111111" {
                    "22222222"
                } else {
                    "11111111"
                }
                .to_owned(),
            )
            .unwrap();
            for _ in 0..5 {
                assert!(
                    f.email_prove(
                        &a,
                        &id,
                        &challenge,
                        &wrong,
                        (ChallengeKind::Otp, Confirmation::Affirmative),
                        &client
                    )
                    .await
                    .is_err()
                );
            }
            let failures: i64 = sqlx::query_scalar(
                "SELECT failures FROM human_owner_challenges WHERE challenge_id=?",
            )
            .bind(challenge.as_str())
            .fetch_one(&f.s.a.store.pool)
            .await
            .unwrap();
            assert_eq!(failures, 5);
        }
        assert!(
            f.email_prove(
                &a,
                &id,
                &challenge,
                &secret,
                (ChallengeKind::Otp, Confirmation::Affirmative),
                &client
            )
            .await
            .is_err()
        );
        assert_eq!(f.s.a.count("budget_approvals").await, 0);
    }
}
#[tokio::test]
async fn owner_email_concurrent_redemption_and_wrong_purpose_never_duplicate_proof() {
    let f = Fixture::new().await;
    let a = f.account(2, false).await;
    f.promote(&a, 0, false).await;
    f.s.a.advance(900);
    let action = budget(0, 0, 1000);
    let id =
        f.s.a
            .auth
            .prepare_owner_action(
                &a.session.handle,
                &f.project,
                &action,
                &f.ctx(&a.session, Operation::Write, 7),
            )
            .await
            .unwrap();
    let (mail, secret, client) = f.prepare_email_for(&a, &id, ChallengeKind::MagicLink).await;
    let challenge = mail.reference().clone();
    f.s.a.auth.dispatch_owner_email(mail).await.unwrap();
    assert!(
        f.email_prove(
            &a,
            &id,
            &challenge,
            &secret,
            (ChallengeKind::Otp, Confirmation::Affirmative),
            &client
        )
        .await
        .is_err()
    );
    let (x, y) = tokio::join!(
        f.email_prove(
            &a,
            &id,
            &challenge,
            &secret,
            (ChallengeKind::MagicLink, Confirmation::Affirmative),
            &client
        ),
        f.email_prove(
            &a,
            &id,
            &challenge,
            &secret,
            (ChallengeKind::MagicLink, Confirmation::Affirmative),
            &client
        )
    );
    assert_eq!(usize::from(x.is_ok()) + usize::from(y.is_ok()), 1);
}
#[tokio::test]
async fn owner_email_cannot_bind_another_browser_action_or_account() {
    let f = Fixture::new().await;
    let a = f.account(2, false).await;
    f.promote(&a, 0, false).await;
    f.s.a.advance(900);
    let action = budget(0, 0, 1000);
    let id =
        f.s.a
            .auth
            .prepare_owner_action(
                &a.session.handle,
                &f.project,
                &action,
                &f.ctx(&a.session, Operation::Write, 7),
            )
            .await
            .unwrap();
    let (mail, secret, client) = f.prepare_email_for(&a, &id, ChallengeKind::MagicLink).await;
    let challenge = mail.reference().clone();
    f.s.a.auth.dispatch_owner_email(mail).await.unwrap();
    let other =
        f.s.a
            .auth
            .prepare_owner_action(
                &a.session.handle,
                &f.project,
                &action,
                &f.ctx(&a.session, Operation::Write, 7),
            )
            .await
            .unwrap();
    assert!(
        f.email_prove(
            &a,
            &other,
            &challenge,
            &secret,
            (ChallengeKind::MagicLink, Confirmation::Affirmative),
            &client
        )
        .await
        .is_err()
    );
    let c = f.ctx(&a.session, Operation::Write, 8);
    let q = OwnerProofRequest {
        session: &a.session.handle,
        project: &f.project,
        action: &id,
        confirmation: Confirmation::Affirmative,
        context: &c,
    };
    assert!(
        f.s.a
            .auth
            .redeem_owner_email(
                &q,
                &challenge,
                &secret,
                ChallengeKind::MagicLink,
                &f.s.a.client(8, 1),
                None
            )
            .await
            .is_err()
    );
    assert!(
        f.email_prove(
            &a,
            &id,
            &challenge,
            &secret,
            (ChallengeKind::MagicLink, Confirmation::Affirmative),
            &client
        )
        .await
        .is_ok()
    );
}
#[tokio::test]
async fn email_step_up_cannot_fallback_around_an_enrolled_password() {
    let f = Fixture::new().await;
    let id = f.prepare(&budget(0, 0, 1000)).await;
    let c = f.ctx(&f.owner, Operation::Write, 7);
    let q = OwnerProofRequest {
        session: &f.owner.handle,
        project: &f.project,
        action: &id,
        confirmation: Confirmation::Affirmative,
        context: &c,
    };
    assert!(
        f.s.a
            .auth
            .prepare_owner_email(&q, ChallengeKind::Otp, &f.s.a.client(7, 1))
            .await
            .is_err()
    );
    assert_eq!(f.s.a.count("human_owner_challenges").await, 0);
}
#[tokio::test]
async fn enrolled_totp_login_alone_cannot_authorize_a_sensitive_action() {
    let mut f = Fixture::new().await;
    let (e, active) = f.s.factor().await;
    f.owner = active.session;
    let action = budget(0, 0, 1000);
    let id = f.prepare(&action).await;
    assert!(
        f.prove_on(&f.owner, &f.project, &id, &f.s.password, None)
            .await
            .is_err()
    );
    assert!(f.execute(&id, &action).await.is_err());
    f.s.a.advance(30);
    f.prove_on(
        &f.owner,
        &f.project,
        &id,
        &f.s.password,
        Some(&code(&e.seed, f.s.a.now())),
    )
    .await
    .unwrap();
    f.execute(&id, &action).await.unwrap();
    let second = f.prepare(&budget(1, 1000, 2000)).await;
    assert!(
        f.prove_on(
            &f.owner,
            &f.project,
            &second,
            &f.s.password,
            Some(&code(&e.seed, f.s.a.now()))
        )
        .await
        .is_err()
    );
}
#[tokio::test]
async fn concurrent_owner_totp_reuse_has_one_successful_fresh_proof() {
    let mut f = Fixture::new().await;
    let (e, active) = f.s.factor().await;
    f.owner = active.session;
    f.s.a.advance(30);
    let action = budget(0, 0, 1000);
    let first = f.prepare(&action).await;
    let second = f.prepare(&action).await;
    let code = code(&e.seed, f.s.a.now());
    let (x, y) = tokio::join!(
        f.prove_on(&f.owner, &f.project, &first, &f.s.password, Some(&code)),
        f.prove_on(&f.owner, &f.project, &second, &f.s.password, Some(&code))
    );
    assert_eq!(usize::from(x.is_ok()) + usize::from(y.is_ok()), 1);
}
#[tokio::test]
async fn last_owner_account_compromise_blocks_authority_without_reassigning_membership() {
    let f = Fixture::new().await;
    let action = budget(0, 0, 1000);
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    let grant = f.s.a.grant(
        AdminAction::Restrict,
        Some(&f.s.user.0),
        f.s.a.auth.mac(
            b"identity-restrict",
            &[f.s.user.0.as_bytes(), b"compromised"],
        ),
        true,
    );
    f.s.a
        .auth
        .restrict_identity(&grant, &f.s.user, AccountRestriction::Compromised)
        .await
        .unwrap();
    assert!(f.execute(&id, &action).await.is_err());
    assert_eq!(f.s.a.count("budget_approvals").await, 0);
    let role: String = sqlx::query_scalar("SELECT role FROM human_memberships")
        .fetch_one(&f.s.a.store.pool)
        .await
        .unwrap();
    assert_eq!(role, "owner");
    let state: String = sqlx::query_scalar("SELECT state FROM human_users WHERE user_id=?")
        .bind(&f.s.user.0)
        .fetch_one(&f.s.a.store.pool)
        .await
        .unwrap();
    assert_eq!(state, "compromised");
}
#[tokio::test]
async fn scoped_incident_holds_related_proofs_and_invites_but_preserves_other_session() {
    let f = Fixture::new().await;
    let healthy = f.s.session(2).await;
    let a = f.account(3, true).await;
    let t = f.invite(&a, ProjectRole::Viewer).await;
    let action = budget(0, 0, 1000);
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    let evidence = f.s.incident_evidence(&f.owner);
    f.s.a
        .auth
        .contain_compromised_session(&f.owner.handle, &evidence)
        .await
        .unwrap();
    assert!(f.execute(&id, &action).await.is_err());
    assert!(f.redeem(&a, &t).await.is_err());
    assert!(
        f.s.a
            .auth
            .project_membership(
                &healthy.handle,
                &f.project,
                &f.ctx(&healthy, Operation::Read, 2)
            )
            .await
            .is_ok()
    );
    let state: String =
        sqlx::query_scalar("SELECT state FROM human_owner_actions WHERE action_id=?")
            .bind(id.as_str())
            .fetch_one(&f.s.a.store.pool)
            .await
            .unwrap();
    assert_eq!(state, "held");
}
#[test]
fn malformed_identifiers_money_and_deadlines_fail_without_panics() {
    for input in [
        String::new(),
        "q".repeat(32),
        "a".repeat(33),
        "a".repeat(4096),
    ] {
        assert!(HumanId::parse(input.clone()).is_err());
        assert!(OwnerActionId::parse(input.clone()).is_err());
        assert!(InvitationId::parse(input.clone()).is_err());
        assert!(InvitationSecret::parse(input).is_err());
    }
    assert!(OwnerAction::budget(u64::MAX, usd(0), usd(1)).is_err());
    assert!(OwnerAction::budget(0, usd(1), usd(1)).is_err());
    assert!(deadline(i64::MAX - 1, 86400).is_err());
    assert!(OwnerAction::decode(&"a".repeat(2049)).is_err());
}

#[tokio::test]
async fn email_challenge_and_fresh_owner_proof_have_separate_ten_and_five_minute_clocks() {
    for wait in [299, 300] {
        let f = Fixture::new().await;
        let a = f.account(2, false).await;
        f.promote(&a, 0, false).await;
        f.s.a.advance(900);
        let action = budget(0, 0, 1000);
        let id =
            f.s.a
                .auth
                .prepare_owner_action(
                    &a.session.handle,
                    &f.project,
                    &action,
                    &f.ctx(&a.session, Operation::Write, 7),
                )
                .await
                .unwrap();
        let (mail, secret, client) = f.prepare_email_for(&a, &id, ChallengeKind::Otp).await;
        let challenge = mail.reference().clone();
        f.s.a.auth.dispatch_owner_email(mail).await.unwrap();
        f.s.a.advance(599);
        f.email_prove(
            &a,
            &id,
            &challenge,
            &secret,
            (ChallengeKind::Otp, Confirmation::Affirmative),
            &client,
        )
        .await
        .unwrap();
        f.s.a.advance(wait);
        let result =
            f.s.a
                .auth
                .execute_owner_action(
                    &a.session.handle,
                    &f.project,
                    &id,
                    &action,
                    Confirmation::Affirmative,
                    &f.ctx(&a.session, Operation::Write, 7),
                )
                .await;
        assert_eq!(result.is_ok(), wait == 299);
    }
}
#[tokio::test]
async fn ordinary_login_challenge_cannot_be_redeemed_as_an_owner_challenge() {
    let f = Fixture::new().await;
    let a = f.account(2, false).await;
    f.promote(&a, 0, false).await;
    f.s.a.advance(900);
    let action = budget(0, 0, 1000);
    let id =
        f.s.a
            .auth
            .prepare_owner_action(
                &a.session.handle,
                &f.project,
                &action,
                &f.ctx(&a.session, Operation::Write, 1),
            )
            .await
            .unwrap();
    let (receipt, token, client) =
        f.s.a
            .challenge(&a.email, ChallengeKind::Otp, ChallengePurpose::Login)
            .await;
    let impostor = OwnerChallengeId::parse(receipt.reference().to_owned()).unwrap();
    let c = f.ctx(&a.session, Operation::Write, 1);
    let q = OwnerProofRequest {
        session: &a.session.handle,
        project: &f.project,
        action: &id,
        confirmation: Confirmation::Affirmative,
        context: &c,
    };
    assert!(
        f.s.a
            .auth
            .redeem_owner_email(&q, &impostor, &token, ChallengeKind::Otp, &client, None)
            .await
            .is_err()
    );
    assert_eq!(f.s.a.count("budget_approvals").await, 0);
    let unsupported = ChallengeRequest {
        client: f.s.a.client(1, 1),
        email: a.email.clone(),
        kind: ChallengeKind::Otp,
        purpose: ChallengePurpose::OwnerAction,
        return_target: ReturnTarget::PrimaryAuthentication,
    };
    assert!(f.s.a.auth.prepare_email(&unsupported).await.is_err());
}
#[tokio::test]
async fn undelivered_partial_or_unavailable_owner_mail_never_authorizes() {
    for mode in [1, 2, 3, 4] {
        let f = Fixture::new().await;
        let a = f.account(2, false).await;
        f.promote(&a, 0, false).await;
        f.s.a.advance(900);
        let action = budget(0, 0, 1000);
        let id =
            f.s.a
                .auth
                .prepare_owner_action(
                    &a.session.handle,
                    &f.project,
                    &action,
                    &f.ctx(&a.session, Operation::Write, 7),
                )
                .await
                .unwrap();
        if mode == 4 {
            f.s.a.synthetic_mail_mode(mode);
            let c = f.ctx(&a.session, Operation::Write, 7);
            let q = OwnerProofRequest {
                session: &a.session.handle,
                project: &f.project,
                action: &id,
                confirmation: Confirmation::Affirmative,
                context: &c,
            };
            assert!(
                f.s.a
                    .auth
                    .prepare_owner_email(&q, ChallengeKind::Otp, &f.s.a.client(7, 1))
                    .await
                    .is_err()
            );
        } else {
            let (mail, secret, client) = f.prepare_email_for(&a, &id, ChallengeKind::Otp).await;
            let challenge = mail.reference().clone();
            f.s.a.synthetic_mail_mode(mode);
            assert_ne!(
                f.s.a.auth.dispatch_owner_email(mail).await.unwrap(),
                MailDispatch::DeliveryVerified
            );
            assert!(
                f.email_prove(
                    &a,
                    &id,
                    &challenge,
                    &secret,
                    (ChallengeKind::Otp, Confirmation::Affirmative),
                    &client
                )
                .await
                .is_err()
            );
        }
        assert_eq!(f.s.a.count("budget_approvals").await, 0);
    }
}
#[tokio::test]
async fn three_owners_need_one_approval_with_exact_once_notification_fanout() {
    let f = Fixture::new().await;
    let a = f.account(2, true).await;
    let b = f.account(3, true).await;
    f.promote(&a, 0, false).await;
    f.promote(&b, 0, false).await;
    let action = budget(0, 0, 1000);
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    f.execute(&id, &action).await.unwrap();
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM human_owner_notifications n JOIN human_governance_events e ON e.sequence=n.event_sequence WHERE e.reference=?").bind(&id.0).fetch_one(&f.s.a.store.pool).await.unwrap();
    assert_eq!(count, 2);
    for account in [&a, &b] {
        let c = f.ctx(&account.session, Operation::Read, 2);
        let notices =
            f.s.a
                .auth
                .owner_notifications(&account.session.handle, 0, 100, &c)
                .await
                .unwrap();
        let notice = notices.iter().find(|n| n.reference == id.0).unwrap();
        assert_eq!(notice.actor, f.s.user);
        assert_eq!(notice.action, Some(action.clone()));
    }
    assert!(f.execute(&id, &action).await.is_err());
    assert_eq!(f.s.a.count("budget_approvals").await, 1);
}
#[tokio::test]
async fn bound_membership_history_and_audit_survive_restart_without_regranting_legacy_authority() {
    let f = Fixture::new().await;
    let a = f.account(2, true).await;
    let t = f.invite(&a, ProjectRole::Viewer).await;
    f.redeem(&a, &t).await.unwrap();
    let change = OwnerAction::membership(&a.id, 1, Some(ProjectRole::Editor)).unwrap();
    let id = f.prepare(&change).await;
    f.prove(&id).await;
    f.execute(&id, &change).await.unwrap();
    let auth = f.s.reopened().await;
    let rows = auth
        .project_membership_history(
            &f.owner.handle,
            &f.project,
            &a.id,
            0,
            100,
            &f.ctx(&f.owner, Operation::Read, 1),
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].role, ProjectRole::Viewer);
    assert_eq!(rows[1].role, ProjectRole::Editor);
    let audit = auth
        .project_governance_history(
            &f.owner.handle,
            &f.project,
            0,
            100,
            &f.ctx(&f.owner, Operation::Read, 1),
        )
        .await
        .unwrap();
    assert!(
        audit
            .iter()
            .any(|e| e.event == "action_committed" && e.reference == id.0)
    );
    assert!(
        auth.project_membership_history(
            &a.session.handle,
            &f.project,
            &a.id,
            0,
            100,
            &f.ctx(&a.session, Operation::Read, 2)
        )
        .await
        .is_err()
    );
}
#[tokio::test]
async fn notification_and_proof_mutation_guards_prevent_history_rewriting() {
    let f = Fixture::new().await;
    let a = budget(0, 0, 1000);
    let id = f.prepare(&a).await;
    f.prove(&id).await;
    f.execute(&id, &a).await.unwrap();
    for table in [
        "human_project_bindings",
        "human_memberships",
        "human_membership_history",
        "human_owner_actions",
        "human_owner_proofs",
        "human_owner_effects",
        "human_governance_events",
    ] {
        assert!(
            sqlx::query(&format!("DELETE FROM {table}"))
                .execute(&f.s.a.store.pool)
                .await
                .is_err()
        );
    }
    assert!(
        sqlx::query("UPDATE human_owner_proofs SET expires_at=expires_at+300")
            .execute(&f.s.a.store.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE human_owner_actions SET state='pending' WHERE state='committed'")
            .execute(&f.s.a.store.pool)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn recipient_logout_or_expiry_during_final_commit_cannot_complete_a_transfer() {
    for at_commit in [false, true] {
        let mut f = Fixture::new().await;
        let mut target = f.account(2, true).await;
        // Make recipient credential end first, while the initiating Owner remains valid.
        f.s.a.advance(86400 - 100);
        f.owner =
            f.s.a
                .auth
                .renew_session(&f.owner.handle, &f.ctx(&f.owner, Operation::Renew, 1))
                .await
                .unwrap();
        let action = OwnerAction::elevation(&target.id, 0, true).unwrap();
        let id = f.prepare(&action).await;
        f.prove(&id).await;
        f.s.a
            .auth
            .accept_owner_elevation(
                &target.session.handle,
                &id,
                Confirmation::Affirmative,
                &f.ctx(&target.session, Operation::Write, 2),
            )
            .await
            .unwrap();
        if at_commit {
            let clock = f.s.a.clock_for_session_test();
            let until = f.s.a.now() + 100;
            f.s.a.auth.hook = Box::new(move |p| {
                if p == Checkpoint::BeforeOwnerCommit {
                    clock.store(until, std::sync::atomic::Ordering::SeqCst);
                }
                Ok(())
            });
        } else {
            f.s.a
                .auth
                .logout(
                    &target.session.handle,
                    &f.ctx(&target.session, Operation::Write, 2),
                )
                .await
                .unwrap();
        }
        assert!(f.execute(&id, &action).await.is_err());
        assert_eq!(f.s.a.count("human_owner_effects").await, 0);
        // Keep ownership alive; no fallback recipient grant appears.
        let owners: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM human_memberships WHERE role='owner' AND state='active'",
        )
        .fetch_one(&f.s.a.store.pool)
        .await
        .unwrap();
        assert_eq!(owners, 1);
        let _ = &mut target;
    }
}
#[tokio::test]
async fn concurrent_transfers_cannot_remove_initiating_owner_twice() {
    let f = Fixture::new().await;
    let a = f.account(2, true).await;
    let b = f.account(3, true).await;
    let first = OwnerAction::elevation(&a.id, 0, true).unwrap();
    let second = OwnerAction::elevation(&b.id, 0, true).unwrap();
    let x = f.prepare(&first).await;
    let y = f.prepare(&second).await;
    f.prove(&x).await;
    f.prove(&y).await;
    for (account, id) in [(&a, &x), (&b, &y)] {
        f.s.a
            .auth
            .accept_owner_elevation(
                &account.session.handle,
                id,
                Confirmation::Affirmative,
                &f.ctx(&account.session, Operation::Write, 2),
            )
            .await
            .unwrap();
    }
    let (one, two) = tokio::join!(f.execute(&x, &first), f.execute(&y, &second));
    assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
    assert_eq!(f.s.a.count("human_owner_effects").await, 1);
}
#[tokio::test]
async fn repeated_password_failures_share_durable_primary_authentication_throttles() {
    let f = Fixture::new().await;
    let id = f.prepare(&budget(0, 0, 1000)).await;
    let wrong = Password::new(hex(&random::<32>().unwrap()[..]).into_bytes()).unwrap();
    for _ in 0..5 {
        assert!(
            f.prove_on(&f.owner, &f.project, &id, &wrong, None)
                .await
                .is_err()
        );
    }
    assert!(
        f.prove_on(&f.owner, &f.project, &id, &f.s.password, None)
            .await
            .is_err()
    );
    f.s.a.advance(900);
    f.prove(&id).await;
}
#[tokio::test]
async fn hard_headroom_and_unknown_exposure_still_block_machine_reservations() {
    use crate::finance::*;
    use rolvya_domain::money::CostBreakdown;
    struct SyntheticAccounting(ProjectId);
    impl FinancialAuthorization for SyntheticAccounting {
        fn permits(&self, _: &PrincipalId, r: &FinancialRequest, _: i64) -> bool {
            r.project == self.0
                && r.actor.as_str() == "synthetic-accounting"
                && !matches!(r.command, FinancialCommand::Approve { .. })
        }
    }
    let f = Fixture::new().await;
    let action = budget(0, 0, 5);
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    f.execute(&id, &action).await.unwrap();
    let costs = |v| CostBreakdown {
        ai_api: usd(v),
        compute: usd(0),
        storage: usd(0),
        transfer: usd(0),
    };
    let make = |command| FinancialRequest {
        project: f.project.clone(),
        request_id: request(),
        actor: PrincipalId::new("synthetic-accounting").unwrap(),
        reason: "No-cost synthetic accounting observation".to_owned(),
        command,
    };
    let policy = SyntheticAccounting(f.project.clone());
    let reserve = |id: &str, v| FinancialCommand::Reserve {
        liability: LiabilityId::new(id).unwrap(),
        maximum: costs(v),
        bound_evidence: EvidenceRef::new("synthetic-bound-only").unwrap(),
    };
    assert!(
        f.s.a
            .store
            .apply_financial(&make(reserve("over", 6)), &policy)
            .await
            .is_err()
    );
    f.s.a
        .store
        .apply_financial(&make(reserve("held", 3)), &policy)
        .await
        .unwrap();
    f.s.a
        .store
        .apply_financial(
            &make(FinancialCommand::Report {
                liability: LiabilityId::new("unknown").unwrap(),
                expected_revision: 0,
                unreserved: costs(0),
                uncertain: true,
                evidence: EvidenceRef::new("synthetic-unknown-observation").unwrap(),
            }),
            &policy,
        )
        .await
        .unwrap();
    assert!(
        f.s.a
            .store
            .apply_financial(&make(reserve("unsafe", 1)), &policy)
            .await
            .is_err()
    );
    let snapshot = f.s.a.store.financial_snapshot(&f.project).await.unwrap();
    assert_eq!(snapshot.ceiling, usd(5));
    assert_eq!(snapshot.held.total().unwrap(), usd(3));
    assert_eq!(snapshot.uncertain_liabilities, 1);
}
#[tokio::test]
async fn human_incident_preserves_real_admitted_worker_and_financial_hold() {
    let primary = super::super::super::tests::Fixture::new().await;
    let (broker, use_request, lease, _) =
        crate::controller_keys::secret_broker::tests::admitted_for_session_tests(
            primary.store.clone(),
            primary.path.clone(),
            primary.keys.clone(),
        )
        .await;
    let sessions = SessionFixture::from_primary(primary).await;
    let f = Fixture::from_session(sessions, lease.task.plan.project_id.clone()).await;
    let before = f.s.a.store.financial_snapshot(&f.project).await.unwrap();
    let action = OwnerAction::budget(
        before.version,
        before.ceiling,
        Money::new(before.ceiling.minor_units() + 1, before.ceiling.currency()).unwrap(),
    )
    .unwrap();
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    let evidence = f.s.incident_evidence(&f.owner);
    f.s.a
        .auth
        .contain_compromised_session(&f.owner.handle, &evidence)
        .await
        .unwrap();
    assert!(f.execute(&id, &action).await.is_err());
    broker.use_secret(&use_request).await.unwrap();
    let after = f.s.a.store.financial_snapshot(&f.project).await.unwrap();
    assert_eq!(before, after);
}

#[tokio::test]
async fn factor_corruption_after_fresh_proof_blocks_the_final_effect() {
    let mut f = Fixture::new().await;
    let (e, active) = f.s.factor().await;
    f.owner = active.session;
    f.s.a.advance(30);
    let action = budget(0, 0, 1000);
    let id = f.prepare(&action).await;
    f.prove_on(
        &f.owner,
        &f.project,
        &id,
        &f.s.password,
        Some(&code(&e.seed, f.s.a.now())),
    )
    .await
    .unwrap();
    // Disposable corruption fixture bypasses a SQL guard to test cryptographic revalidation.
    sqlx::query("DROP TRIGGER human_factor_update")
        .execute(&f.s.a.store.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE human_factors SET ciphertext=zeroblob(48)")
        .execute(&f.s.a.store.pool)
        .await
        .unwrap();
    assert!(f.execute(&id, &action).await.is_err());
    assert_eq!(f.s.a.count("budget_approvals").await, 0);
}
#[tokio::test]
async fn password_corruption_after_fresh_proof_on_an_email_session_blocks_the_effect() {
    let mut f = Fixture::new().await;
    let (r, secret, client) =
        f.s.a
            .challenge(
                &f.s.email,
                ChallengeKind::MagicLink,
                ChallengePurpose::Login,
            )
            .await;
    let Redemption::PrimaryVerified(proof) =
        f.s.a
            .redeem(&r, &secret, ChallengeKind::MagicLink, &client)
            .await
    else {
        panic!("login denied")
    };
    let SessionStart::Authenticated(session) =
        f.s.a
            .auth
            .start_session(
                &proof,
                &f.s.context(Operation::Issue, 1, None, true),
                &super::super::tests::device(),
            )
            .await
            .unwrap()
    else {
        panic!("factor unexpected")
    };
    f.owner = *session;
    let action = budget(0, 0, 1000);
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    sqlx::query("DROP TRIGGER human_password_update")
        .execute(&f.s.a.store.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE human_passwords SET phc=? WHERE user_id=?")
        .bind("invalid-verifier".repeat(4))
        .bind(&f.s.user.0)
        .execute(&f.s.a.store.pool)
        .await
        .unwrap();
    assert!(f.execute(&id, &action).await.is_err());
    assert_eq!(f.s.a.count("budget_approvals").await, 0);
}

#[tokio::test]
async fn repeated_context_cannot_turn_rolled_back_guesses_into_a_success() {
    let f = Fixture::new().await;
    let a = f.account(2, false).await;
    f.promote(&a, 0, false).await;
    f.s.a.advance(900);
    let action = budget(0, 0, 1000);
    let id =
        f.s.a
            .auth
            .prepare_owner_action(
                &a.session.handle,
                &f.project,
                &action,
                &f.ctx(&a.session, Operation::Write, 7),
            )
            .await
            .unwrap();
    let (mail, secret, client) = f.prepare_email_for(&a, &id, ChallengeKind::Otp).await;
    let challenge = mail.reference().clone();
    f.s.a.auth.dispatch_owner_email(mail).await.unwrap();
    let c = f.ctx(&a.session, Operation::Write, 7);
    let q = OwnerProofRequest {
        session: &a.session.handle,
        project: &f.project,
        action: &id,
        confirmation: Confirmation::Affirmative,
        context: &c,
    };
    for i in 0..7 {
        let candidate = format!("{i:08}");
        let candidate = if candidate == secret.0.as_str() {
            format!("{:08}", i + 10)
        } else {
            candidate
        };
        let wrong = ChallengeSecret::parse(candidate).unwrap();
        assert!(
            f.s.a
                .auth
                .redeem_owner_email(&q, &challenge, &wrong, ChallengeKind::Otp, &client, None)
                .await
                .is_err()
        );
    }
    assert!(
        f.s.a
            .auth
            .redeem_owner_email(&q, &challenge, &secret, ChallengeKind::Otp, &client, None)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn binding_authority_expiry_at_final_checkpoint_rolls_back_initial_owner() {
    let mut f = Fixture::new().await;
    let p = ProjectId::new("project-b").unwrap();
    sqlx::query("INSERT INTO projects(project_id,owner_actor) VALUES(?,'legacy-b')")
        .bind(p.as_str())
        .execute(&f.s.a.store.pool)
        .await
        .unwrap();
    let authority = Administration {
        request_id: request(),
        evidence: request(),
    };
    let expires = f.s.a.now() + 1;
    f.binding.0.lock().unwrap().insert(
        authority.request_id.as_str().to_owned(),
        (
            p.as_str().to_owned(),
            f.s.user.0.clone(),
            "legacy-b".to_owned(),
            authority.evidence.as_str().to_owned(),
            expires,
        ),
    );
    let clock = f.s.a.clock_for_session_test();
    f.s.a.auth.hook = Box::new(move |p| {
        if p == Checkpoint::BeforeSessionCommit {
            clock.store(expires, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    });
    assert!(
        f.s.a
            .auth
            .bind_existing_project(
                &f.owner.handle,
                &p,
                &authority,
                &f.ctx(&f.owner, Operation::Write, 1)
            )
            .await
            .is_err()
    );
    assert_eq!(f.s.a.count("human_project_bindings").await, 1);
}
#[tokio::test]
async fn password_transport_withdrawal_at_final_checkpoint_cannot_mint_proof() {
    let mut f = Fixture::new().await;
    let id = f.prepare(&budget(0, 0, 1000)).await;
    let c = f.ctx(&f.owner, Operation::Write, 3);
    let client = f.s.a.client(3, 1);
    f.s.a.auth.hook =
        f.s.a
            .withdraw_client_at(&client, Checkpoint::BeforeOwnerCommit);
    let q = OwnerProofRequest {
        session: &f.owner.handle,
        project: &f.project,
        action: &id,
        confirmation: Confirmation::Affirmative,
        context: &c,
    };
    assert!(
        f.s.a
            .auth
            .prove_owner_password(&q, &f.s.password, &client, None)
            .await
            .is_err()
    );
    assert_eq!(f.s.a.count("human_owner_proofs").await, 0);
}
#[tokio::test]
async fn email_transport_withdrawal_at_final_checkpoint_cannot_mint_proof() {
    let mut f = Fixture::new().await;
    let a = f.account(2, false).await;
    f.promote(&a, 0, false).await;
    f.s.a.advance(900);
    let action = budget(0, 0, 1000);
    let id =
        f.s.a
            .auth
            .prepare_owner_action(
                &a.session.handle,
                &f.project,
                &action,
                &f.ctx(&a.session, Operation::Write, 7),
            )
            .await
            .unwrap();
    let (mail, secret, client) = f.prepare_email_for(&a, &id, ChallengeKind::Otp).await;
    let challenge = mail.reference().clone();
    f.s.a.auth.dispatch_owner_email(mail).await.unwrap();
    f.s.a.auth.hook =
        f.s.a
            .withdraw_client_at(&client, Checkpoint::BeforeOwnerCommit);
    assert!(
        f.email_prove(
            &a,
            &id,
            &challenge,
            &secret,
            (ChallengeKind::Otp, Confirmation::Affirmative),
            &client
        )
        .await
        .is_err()
    );
    assert_eq!(f.s.a.count("human_owner_proofs").await, 1);
}

struct CrashClock(i64);
impl crate::admission::Clock for CrashClock {
    fn now(&self) -> Result<i64, crate::admission::AdmissionError> {
        Ok(self.0)
    }
}
struct CrashBrowser {
    context: BrowserContext,
    facts: BrowserFacts,
    until: i64,
}
impl BrowserAuthority for CrashBrowser {
    fn verify(
        &self,
        c: &BrowserContext,
        op: Operation,
        now: i64,
    ) -> Result<BrowserFacts, AuthError> {
        if c.request_id != self.context.request_id
            || c.evidence != self.context.evidence
            || op != Operation::Write
            || now >= self.until
        {
            return Err(AuthError::Denied);
        }
        Ok(self.facts.clone())
    }
    fn deliver(&self, _: &BrowserContext, _: &SessionTicket, _: i64) -> Result<(), AuthError> {
        Err(AuthError::Unavailable)
    }
    fn provision(&self, _: &BrowserContext, _: &FactorEnrollment, _: i64) -> Result<(), AuthError> {
        Err(AuthError::Unavailable)
    }
    fn recovery(&self, _: &BrowserContext, _: &RecoveryBundle, _: i64) -> Result<(), AuthError> {
        Err(AuthError::Unavailable)
    }
    fn pending(&self, _: &BrowserContext, _: &PendingLogin, _: i64) -> Result<(), AuthError> {
        Err(AuthError::Unavailable)
    }
}
struct CrashTransport {
    client: ClientContext,
    until: i64,
}
impl TransportAuthority for CrashTransport {
    fn verify(
        &self,
        c: &ClientContext,
        op: TransportOperation,
        now: i64,
    ) -> Result<VerifiedTransport, AuthError> {
        if c.request_id != self.client.request_id
            || c.transport_evidence != self.client.transport_evidence
            || !matches!(
                op,
                TransportOperation::Password | TransportOperation::ConfirmEmail
            )
            || now >= self.until
        {
            return Err(AuthError::Denied);
        }
        Ok(VerifiedTransport {
            ip: std::net::IpAddr::from([192, 0, 2, 1]),
            browser: [7; 32],
            parent: None,
        })
    }
}
struct CrashMailEvidence {
    challenge: Option<String>,
    until: i64,
}
impl MailTransport for CrashMailEvidence {
    fn ready(&self) -> bool {
        self.challenge.is_some()
    }
    fn send<'a>(&'a self, _: &'a MailMessage) -> MailFuture<'a> {
        Box::pin(async { DeliveryObservation::Failed })
    }
}
impl DeliveryAuthority for CrashMailEvidence {
    fn ready(&self) -> bool {
        self.challenge.is_some()
    }
    fn delivered(&self, id: &str, now: i64) -> bool {
        self.challenge.as_deref() == Some(id) && now < self.until
    }
}
#[tokio::test]
async fn subprocess_driver() {
    use std::io::{Read, Write};
    let Ok(op) = std::env::var("ROLVYA_S203C_CRASH_OPERATION") else {
        return;
    };
    let path = std::path::PathBuf::from(std::env::var("ROLVYA_S203C_CRASH_DATABASE").unwrap());
    let root = path.parent().unwrap().parent().unwrap();
    assert_eq!(
        std::fs::read(root.join("owner-crash-test-only")).unwrap(),
        b"synthetic-no-live-data-v1"
    );
    let store = PlanStore::open(&path).await.unwrap();
    let keys = std::path::PathBuf::from(std::env::var("ROLVYA_S203C_CRASH_KEYS").unwrap());
    assert_eq!(keys.parent().unwrap(), root);
    let bootstrap = ControllerBootstrap::open(
        store.clone(),
        &keys,
        crate::controller_keys::ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let mut auth = HumanAuth::open(&bootstrap).await.unwrap();
    auth.passwords = super::super::super::credentials::PasswordEngine::synthetic();
    let mut raw = Zeroizing::new(String::new());
    std::io::stdin()
        .take(16384)
        .read_to_string(&mut raw)
        .unwrap();
    let mut data: BTreeMap<String, String> = serde_json::from_str(&raw).unwrap();
    let now = data.remove("now").unwrap().parse::<i64>().unwrap();
    auth.clock = Arc::new(CrashClock(now));
    let h = SessionHandle::parse(data.remove("cookie").unwrap()).unwrap();
    let csrf = Zeroizing::new(data.remove("csrf").unwrap());
    let c = BrowserContext {
        request_id: request(),
        evidence: request(),
    };
    let f = BrowserFacts {
        secure: true,
        method: Method::Post,
        origin: "https://rolvya.invalid".to_owned(),
        expected_origin: "https://rolvya.invalid".to_owned(),
        same_origin: true,
        browser: [7; 32],
        credential: Some(auth.handle_lookup(&h)),
        csrf: Some(auth.csrf_commitment(&csrf)),
        preauth_csrf: false,
        activity: Activity::Human,
        client_key: auth.mac(b"synthetic-client", &[b"192.0.2.1"]),
    };
    auth.browser_authority = Arc::new(CrashBrowser {
        context: c.clone(),
        facts: f,
        until: now + 60,
    });
    let client = ClientContext {
        request_id: request(),
        transport_evidence: request(),
    };
    auth.transport = Arc::new(CrashTransport {
        client: client.clone(),
        until: now + 60,
    });
    let challenge = data.remove("challenge").filter(|s| !s.is_empty());
    let delivery = Arc::new(CrashMailEvidence {
        challenge: challenge.clone(),
        until: now + 600,
    });
    auth.mail = delivery.clone();
    auth.delivery = delivery;
    let stop = std::env::var("ROLVYA_S203C_CRASH_CHECKPOINT").unwrap();
    auth.hook = Box::new(move |p| {
        if stop == format!("{p:?}") {
            println!("S203C_READY");
            std::io::stdout().flush().unwrap();
            loop {
                std::thread::park()
            }
        }
        Ok(())
    });
    let project = ProjectId::new(data.remove("project").unwrap()).unwrap();
    if op == "invitation" {
        let id = InvitationId::parse(data.remove("reference").unwrap()).unwrap();
        let secret = InvitationSecret::parse(data.remove("secret").unwrap()).unwrap();
        auth.redeem_project_invitation(&h, &project, &id, &secret, &c)
            .await
            .unwrap();
    } else {
        let id = OwnerActionId::parse(data.remove("reference").unwrap()).unwrap();
        match op.as_str() {
            "budget" | "transfer" => {
                let payload: String =
                    sqlx::query_scalar("SELECT payload FROM human_owner_actions WHERE action_id=?")
                        .bind(id.as_str())
                        .fetch_one(&store.pool)
                        .await
                        .unwrap();
                let action = OwnerAction::decode(&payload).unwrap();
                auth.execute_owner_action(
                    &h,
                    &project,
                    &id,
                    &action,
                    Confirmation::Affirmative,
                    &c,
                )
                .await
                .unwrap();
            }
            "password" => {
                let password = Password::new(data.remove("secret").unwrap().into_bytes()).unwrap();
                let q = OwnerProofRequest {
                    session: &h,
                    project: &project,
                    action: &id,
                    confirmation: Confirmation::Affirmative,
                    context: &c,
                };
                auth.prove_owner_password(&q, &password, &client, None)
                    .await
                    .unwrap();
            }
            "email" => {
                let challenge = OwnerChallengeId::parse(challenge.unwrap()).unwrap();
                let secret = ChallengeSecret::parse(data.remove("secret").unwrap()).unwrap();
                let q = OwnerProofRequest {
                    session: &h,
                    project: &project,
                    action: &id,
                    confirmation: Confirmation::Affirmative,
                    context: &c,
                };
                auth.redeem_owner_email(&q, &challenge, &secret, ChallengeKind::Otp, &client, None)
                    .await
                    .unwrap();
            }
            _ => panic!("unsupported synthetic crash operation"),
        }
    }
    panic!("selected checkpoint was not reached");
}
struct KillOnDrop(std::process::Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[tokio::test]
async fn ten_sigkill_boundaries_preserve_proofs_governance_ledger_and_notifications() {
    use std::io::{BufRead, Write};
    use zeroize::Zeroize;
    for op in ["budget", "transfer", "password", "email", "invitation"] {
        for after in [false, true] {
            let f = Fixture::new().await;
            std::fs::write(
                f.s.a.root.path().join("owner-crash-test-only"),
                b"synthetic-no-live-data-v1",
            )
            .unwrap();
            let mut account = None;
            let mut invitation = None;
            let mut challenge = None;
            let mut secret = Zeroizing::new(String::new());
            let action = if op == "transfer" {
                account = Some(f.account(2, true).await);
                OwnerAction::elevation(&account.as_ref().unwrap().id, 0, true).unwrap()
            } else {
                budget(0, 0, 1000)
            };
            if op == "email" {
                account = Some(f.account(2, false).await);
                f.promote(account.as_ref().unwrap(), 0, false).await;
                f.s.a.advance(900);
            }
            let actor = if op == "email" || op == "invitation" {
                if account.is_none() {
                    account = Some(f.account(2, true).await);
                }
                &account.as_ref().unwrap().session
            } else {
                &f.owner
            };
            let id = if op == "email" {
                f.s.a
                    .auth
                    .prepare_owner_action(
                        &actor.handle,
                        &f.project,
                        &action,
                        &f.ctx(actor, Operation::Write, 7),
                    )
                    .await
                    .unwrap()
            } else {
                f.prepare(&action).await
            };
            match op {
                "budget" => f.prove(&id).await,
                "transfer" => {
                    f.prove(&id).await;
                    f.s.a
                        .auth
                        .accept_owner_elevation(
                            &account.as_ref().unwrap().session.handle,
                            &id,
                            Confirmation::Affirmative,
                            &f.ctx(&account.as_ref().unwrap().session, Operation::Write, 7),
                        )
                        .await
                        .unwrap();
                }
                "password" => secret.push_str(std::str::from_utf8(&f.s.password.0).unwrap()),
                "email" => {
                    let (mail, token, _) = f
                        .prepare_email_for(account.as_ref().unwrap(), &id, ChallengeKind::Otp)
                        .await;
                    challenge = Some(mail.reference().clone());
                    secret.push_str(&token.0);
                    f.s.a.auth.dispatch_owner_email(mail).await.unwrap();
                }
                "invitation" => {
                    let invite = f
                        .invite(account.as_ref().unwrap(), ProjectRole::Viewer)
                        .await;
                    secret.push_str(invite.secret.as_ref().unwrap().synthetic_bearer());
                    invitation = Some(invite);
                }
                _ => unreachable!(),
            }
            let effects = f.s.a.count("human_owner_effects").await;
            let proofs = f.s.a.count("human_owner_proofs").await;
            let approvals = f.s.a.count("budget_approvals").await;
            let memberships = f.s.a.count("human_memberships").await;
            let reference = invitation
                .as_ref()
                .map(|t| t.reference().as_str())
                .unwrap_or(id.as_str());
            let mut fields = BTreeMap::from([
                ("cookie".to_owned(), actor.handle.0.to_string()),
                ("csrf".to_owned(), actor.csrf.to_string()),
                ("now".to_owned(), f.s.a.now().to_string()),
                ("project".to_owned(), f.project.as_str().to_owned()),
                ("reference".to_owned(), reference.to_owned()),
                ("secret".to_owned(), secret.to_string()),
                (
                    "challenge".to_owned(),
                    challenge
                        .as_ref()
                        .map(|r| r.as_str().to_owned())
                        .unwrap_or_default(),
                ),
            ]);
            let input = Zeroizing::new(serde_json::to_string(&fields).unwrap());
            for v in fields.values_mut() {
                v.zeroize()
            }
            assert!(input.len() < 16384);
            let point = if after {
                Checkpoint::AfterOwnerCommit
            } else {
                Checkpoint::BeforeOwnerCommit
            };
            let mut child = KillOnDrop(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "controller_keys::human_auth::sessions::owner::tests::subprocess_driver",
                        "--nocapture",
                    ])
                    .env("ROLVYA_S203C_CRASH_OPERATION", op)
                    .env("ROLVYA_S203C_CRASH_CHECKPOINT", format!("{point:?}"))
                    .env("ROLVYA_S203C_CRASH_DATABASE", &f.s.a.path)
                    .env("ROLVYA_S203C_CRASH_KEYS", &f.s.a.keys)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            child
                .0
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
            let stdout = child.0.stdout.take().unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            let reader = std::thread::spawn(move || {
                for line in std::io::BufReader::new(stdout).lines() {
                    if line.is_ok_and(|s| s == "S203C_READY") {
                        let _ = tx.send(());
                        break;
                    }
                }
            });
            if rx.recv_timeout(std::time::Duration::from_secs(20)).is_err() {
                let _ = child.0.kill();
                let _ = child.0.wait();
                let _ = reader.join();
                panic!("synthetic child did not reach {op}/{point:?}")
            }
            child.0.kill().unwrap();
            let status = child.0.wait().unwrap();
            assert!(!status.success());
            #[cfg(unix)]
            assert_eq!(
                std::os::unix::process::ExitStatusExt::signal(&status),
                Some(9)
            );
            reader.join().unwrap();
            let reopened = PlanStore::open(&f.s.a.path).await.unwrap();
            reopened.check_integrity().await.unwrap();
            match op {
                "budget" => {
                    assert_eq!(
                        f.s.a.count("budget_approvals").await,
                        approvals + i64::from(after)
                    );
                    assert_eq!(
                        f.s.a.count("human_owner_effects").await,
                        effects + i64::from(after)
                    );
                    if after {
                        assert!(f.execute(&id, &action).await.is_err());
                    } else {
                        f.execute(&id, &action).await.unwrap();
                    }
                }
                "transfer" => {
                    assert_eq!(
                        f.s.a.count("human_owner_effects").await,
                        effects + i64::from(after)
                    );
                    let role: String = sqlx::query_scalar(
                        "SELECT role FROM human_memberships WHERE project_id=? AND user_id=?",
                    )
                    .bind(f.project.as_str())
                    .bind(&f.s.user.0)
                    .fetch_one(&reopened.pool)
                    .await
                    .unwrap();
                    assert_eq!(role, if after { "editor" } else { "owner" });
                    let count:i64=sqlx::query_scalar("SELECT count(*) FROM human_memberships WHERE project_id=? AND role='owner' AND state='active'").bind(f.project.as_str()).fetch_one(&reopened.pool).await.unwrap();
                    assert_eq!(count, 1);
                }
                "password" | "email" => {
                    assert_eq!(
                        f.s.a.count("human_owner_proofs").await,
                        proofs + i64::from(after)
                    );
                    assert_eq!(f.s.a.count("budget_approvals").await, approvals);
                    if op == "email" {
                        let state: String = sqlx::query_scalar(
                            "SELECT state FROM human_owner_challenges WHERE challenge_id=?",
                        )
                        .bind(challenge.as_ref().unwrap().as_str())
                        .fetch_one(&reopened.pool)
                        .await
                        .unwrap();
                        assert_eq!(state, if after { "consumed" } else { "delivered" });
                    }
                }
                "invitation" => {
                    assert_eq!(
                        f.s.a.count("human_memberships").await,
                        memberships + i64::from(after)
                    );
                    if after {
                        assert!(
                            f.redeem(account.as_ref().unwrap(), invitation.as_ref().unwrap())
                                .await
                                .is_err()
                        )
                    } else {
                        f.redeem(account.as_ref().unwrap(), invitation.as_ref().unwrap())
                            .await
                            .unwrap();
                    }
                }
                _ => unreachable!(),
            }
            assert_eq!(f.s.a.count("task_attempts").await, 0);
            assert_eq!(f.s.a.count("dispatch_outbox").await, 0);
        }
    }
}

#[tokio::test]
async fn final_proof_expiry_rolls_back_ledger_consumption_and_notifications() {
    let mut f = Fixture::new().await;
    let a = f.account(2, true).await;
    f.promote(&a, 0, false).await;
    let action = budget(0, 0, 1000);
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    let effects = f.s.a.count("human_owner_effects").await;
    let notices = f.s.a.count("human_owner_notifications").await;
    let until = f.s.a.now() + 300;
    let clock = f.s.a.clock_for_session_test();
    f.s.a.auth.hook = Box::new(move |p| {
        if p == Checkpoint::BeforeOwnerCommit {
            clock.store(until, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    });
    assert!(f.execute(&id, &action).await.is_err());
    assert_eq!(f.s.a.count("budget_approvals").await, 0);
    assert_eq!(f.s.a.count("human_owner_effects").await, effects);
    assert_eq!(f.s.a.count("human_owner_notifications").await, notices);
}
#[tokio::test]
async fn sqlite_contention_leaves_approval_unused_and_can_retry_after_lock_release() {
    let f = Fixture::new().await;
    let action = budget(0, 0, 1000);
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    let lock =
        f.s.a
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .unwrap();
    assert_eq!(f.execute(&id, &action).await.unwrap_err(), AuthError::Busy);
    lock.rollback().await.unwrap();
    assert_eq!(f.s.a.count("budget_approvals").await, 0);
    f.execute(&id, &action).await.unwrap();
}
#[tokio::test]
async fn snapshot_restores_bound_authority_with_matching_key_and_never_persists_bearers() {
    let f = Fixture::new().await;
    let recipient = f.account(2, true).await;
    let invite = f.invite(&recipient, ProjectRole::Viewer).await;
    let email_owner = f.account(3, false).await;
    f.promote(&email_owner, 0, false).await;
    f.s.a.advance(900);
    let action = budget(0, 0, 1000);
    let id =
        f.s.a
            .auth
            .prepare_owner_action(
                &email_owner.session.handle,
                &f.project,
                &action,
                &f.ctx(&email_owner.session, Operation::Write, 7),
            )
            .await
            .unwrap();
    let (mail, secret, client) = f
        .prepare_email_for(&email_owner, &id, ChallengeKind::MagicLink)
        .await;
    let challenge = mail.reference().clone();
    let diagnostics = format!("{:?} {:?} {:?} {:?}", mail, invite, f.owner, f.s.password);
    f.s.a.auth.dispatch_owner_email(mail).await.unwrap();
    f.email_prove(
        &email_owner,
        &id,
        &challenge,
        &secret,
        (ChallengeKind::MagicLink, Confirmation::Affirmative),
        &client,
    )
    .await
    .unwrap();
    let path = f.s.a.root.path().join("data/owner-snapshot.sqlite");
    f.s.a.store.snapshot(&path).await.unwrap();
    let mut artifacts = vec![
        std::fs::read(&f.s.a.path).unwrap(),
        std::fs::read(&path).unwrap(),
        diagnostics.as_bytes().to_vec(),
    ];
    let wal = f.s.a.path.with_extension("sqlite-wal");
    if wal.exists() {
        artifacts.push(std::fs::read(wal).unwrap());
    }
    for artifact in artifacts {
        for marker in [
            f.s.password.0.as_slice(),
            f.owner.handle.0.as_bytes(),
            f.owner.csrf.as_bytes(),
            invite
                .secret
                .as_ref()
                .unwrap()
                .synthetic_bearer()
                .as_bytes(),
            secret.0.as_bytes(),
        ] {
            assert!(
                !artifact.windows(marker.len()).any(|v| v == marker),
                "secret persisted or formatted"
            );
        }
    }
    let restored = PlanStore::open(&path).await.unwrap();
    restored.check_integrity().await.unwrap();
    let bootstrap = ControllerBootstrap::open(
        restored.clone(),
        &f.s.a.keys,
        crate::controller_keys::ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let mut auth = HumanAuth::open(&bootstrap).await.unwrap();
    auth.passwords = super::super::super::credentials::PasswordEngine::synthetic();
    auth.clock = f.s.a.auth.clock.clone();
    auth.browser_authority = f.s.a.auth.browser_authority.clone();
    auth.mail = f.s.a.auth.mail.clone();
    auth.delivery = f.s.a.auth.delivery.clone();
    let m = auth
        .project_membership(
            &f.owner.handle,
            &f.project,
            &f.ctx(&f.owner, Operation::Read, 1),
        )
        .await
        .unwrap();
    assert_eq!(m.role, ProjectRole::Owner);
    auth.execute_owner_action(
        &email_owner.session.handle,
        &f.project,
        &id,
        &action,
        Confirmation::Affirmative,
        &f.ctx(&email_owner.session, Operation::Write, 7),
    )
    .await
    .unwrap();
    assert_eq!(
        restored
            .financial_snapshot(&f.project)
            .await
            .unwrap()
            .ceiling,
        usd(1000)
    );
    let wrong = SessionFixture::new().await;
    let bad = ControllerBootstrap::open(
        restored.clone(),
        &wrong.a.keys,
        crate::controller_keys::ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    assert!(matches!(
        HumanAuth::open(&bad).await,
        Err(AuthError::KeyUnavailable)
    ));
    assert_eq!(
        restored
            .financial_snapshot(&f.project)
            .await
            .unwrap()
            .ceiling,
        usd(1000)
    );
}
#[tokio::test]
async fn reused_email_context_cannot_bypass_a_voluntarily_enrolled_totp_factor() {
    let f = Fixture::new().await;
    let mut a = f.account(2, false).await;
    f.promote(&a, 0, false).await;
    f.s.a.advance(900);
    let client = f.s.a.bound_client(
        7,
        Some((
            a.session.metadata.id.0.clone(),
            a.session.metadata.generation,
        )),
    );
    let prepared =
        f.s.a
            .auth
            .prepare_email(&ChallengeRequest {
                client: client.clone(),
                email: a.email.clone(),
                kind: ChallengeKind::MagicLink,
                purpose: ChallengePurpose::Login,
                return_target: ReturnTarget::PrimaryAuthentication,
            })
            .await
            .unwrap();
    let receipt = prepared.receipt().clone();
    let token =
        ChallengeSecret::parse(prepared.message.as_ref().unwrap().token.0.to_string()).unwrap();
    f.s.a.auth.dispatch_email(prepared).await.unwrap();
    let Redemption::PrimaryVerified(proof) =
        f.s.a
            .auth
            .redeem_email(
                &receipt,
                &token,
                ChallengeKind::MagicLink,
                ChallengePurpose::Login,
                Confirmation::Affirmative,
                &client,
            )
            .await
            .unwrap()
    else {
        panic!("primary denied")
    };
    let enrollment =
        f.s.a
            .auth
            .begin_totp_enrollment(
                &a.session.handle,
                &proof,
                &f.ctx(&a.session, Operation::BeginFactor, 7),
            )
            .await
            .unwrap();
    let activated =
        f.s.a
            .auth
            .confirm_totp_enrollment(
                &a.session.handle,
                &enrollment.reference(),
                &code(&enrollment.seed, f.s.a.now()),
                &f.ctx(&a.session, Operation::ConfirmFactor, 7),
            )
            .await
            .unwrap();
    a.session = activated.session;
    f.s.a.advance(900);
    let action = budget(0, 0, 1000);
    let id =
        f.s.a
            .auth
            .prepare_owner_action(
                &a.session.handle,
                &f.project,
                &action,
                &f.ctx(&a.session, Operation::Write, 7),
            )
            .await
            .unwrap();
    let (mail, secret, client) = f.prepare_email_for(&a, &id, ChallengeKind::Otp).await;
    let challenge = mail.reference().clone();
    f.s.a.auth.dispatch_owner_email(mail).await.unwrap();
    let c = f.ctx(&a.session, Operation::Write, 7);
    let q = OwnerProofRequest {
        session: &a.session.handle,
        project: &f.project,
        action: &id,
        confirmation: Confirmation::Affirmative,
        context: &c,
    };
    let first =
        f.s.a
            .auth
            .redeem_owner_email(&q, &challenge, &secret, ChallengeKind::Otp, &client, None);
    let second =
        f.s.a
            .auth
            .redeem_owner_email(&q, &challenge, &secret, ChallengeKind::Otp, &client, None);
    let (x, y) = tokio::join!(first, second);
    assert!(x.is_err() && y.is_err());
    let factor = code(&enrollment.seed, f.s.a.now());
    assert!(
        f.s.a
            .auth
            .redeem_owner_email(
                &q,
                &challenge,
                &secret,
                ChallengeKind::Otp,
                &client,
                Some(&factor)
            )
            .await
            .is_err()
    );
    let fresh = f.ctx(&a.session, Operation::Write, 7);
    let q = OwnerProofRequest {
        session: &a.session.handle,
        project: &f.project,
        action: &id,
        confirmation: Confirmation::Affirmative,
        context: &fresh,
    };
    f.s.a
        .auth
        .redeem_owner_email(
            &q,
            &challenge,
            &secret,
            ChallengeKind::Otp,
            &client,
            Some(&factor),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn historical_status_is_read_only_and_requires_current_session_authority() {
    let f = Fixture::new().await;
    let action = budget(0, 0, 1000);
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    f.execute(&id, &action).await.unwrap();
    let status =
        f.s.a
            .auth
            .owner_action_status(
                &f.owner.handle,
                &f.project,
                &id,
                &f.ctx(&f.owner, Operation::Read, 1),
            )
            .await
            .unwrap();
    assert_eq!(status.state, OwnerActionState::Committed);
    assert!(status.financial_sequence.is_some());
    let next =
        f.s.a
            .auth
            .renew_session(&f.owner.handle, &f.ctx(&f.owner, Operation::Renew, 1))
            .await
            .unwrap();
    assert!(
        f.s.a
            .auth
            .owner_action_status(
                &f.owner.handle,
                &f.project,
                &id,
                &f.ctx(&f.owner, Operation::Read, 1)
            )
            .await
            .is_err()
    );
    assert_eq!(
        f.s.a
            .auth
            .owner_action_status(
                &next.handle,
                &f.project,
                &id,
                &f.ctx(&next, Operation::Read, 2)
            )
            .await
            .unwrap()
            .state,
        OwnerActionState::Committed
    );
    assert!(
        f.s.a
            .auth
            .execute_owner_action(
                &next.handle,
                &f.project,
                &id,
                &action,
                Confirmation::Affirmative,
                &f.ctx(&next, Operation::Write, 2)
            )
            .await
            .is_err()
    );
    f.s.a
        .auth
        .logout(&next.handle, &f.ctx(&next, Operation::Write, 2))
        .await
        .unwrap();
    assert!(
        f.s.a
            .auth
            .owner_action_status(
                &next.handle,
                &f.project,
                &id,
                &f.ctx(&next, Operation::Read, 2)
            )
            .await
            .is_err()
    );
    assert_eq!(f.s.a.count("budget_approvals").await, 1);
}
