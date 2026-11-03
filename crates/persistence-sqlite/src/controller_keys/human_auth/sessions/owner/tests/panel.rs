use super::*;
use crate::{PlanAuthorization, PlanOperation, Revision};
use rolvya_domain::{
    ids::{PlanId, PlanVersion, TaskId},
    money::{CostBreakdown, CostScenarios},
    plan::*,
};
use sqlx::Connection;
pub(in crate::controller_keys) type MailProbe = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;
pub(in crate::controller_keys) struct BrowserSeed {
    pub auth: HumanAuth,
    pub root: tempfile::TempDir,
    pub store: PlanStore,
    pub clock: Arc<std::sync::atomic::AtomicI64>,
    pub mail: MailProbe,
    pub user: HumanId,
    pub email: EmailAddress,
    pub credential: Password,
    pub viewer_email: EmailAddress,
    pub viewer_credential: Password,
    pub project: ProjectId,
    pub fingerprint: String,
    pub totp: Option<Zeroizing<Vec<u8>>>,
    pub recovery: Vec<Zeroizing<String>>,
}
/// Only unit-test code can seed independently verified enrollment/binding. Network
/// authority is replaced by the genuine TLS adapter after setup; no public signup.
pub(in crate::controller_keys::human_auth) async fn browser_seed(factor: bool) -> BrowserSeed {
    let mut primary = super::super::super::super::tests::Fixture::new().await;
    primary.auth.passwords =
        super::super::super::super::credentials::PasswordEngine::new(PasswordParameters::default())
            .unwrap();
    let mut f = Fixture::from_session(
        SessionFixture::from_primary(primary).await,
        ProjectId::new("project-a").unwrap(),
    )
    .await;
    let fingerprint = activate(&f).await;
    let initial_budget = budget(0, 0, 20000);
    let action = f.prepare(&initial_budget).await;
    f.prove(&action).await;
    f.execute(&action, &initial_budget).await.unwrap();
    let viewer = f.account(2, true).await;
    let invite = f.invite(&viewer, ProjectRole::Viewer).await;
    f.redeem(&viewer, &invite).await.unwrap();
    f.bind(&ProjectId::new("project-b").unwrap()).await;
    let (totp, recovery) = if factor {
        let (enrollment, active) = f.s.factor().await;
        f.s.a.advance(30);
        (
            Some(enrollment.seed),
            active.recovery.codes.into_iter().map(|c| c.0).collect(),
        )
    } else {
        (None, vec![])
    };
    let clock = f.s.a.browser_clock();
    let mail = f.s.a.browser_mail_probe();
    f.s.a.auth.authority = Arc::new(Unimplemented);
    f.s.a.auth.project_binding = Arc::new(Unimplemented);
    BrowserSeed {
        clock,
        mail,
        store: f.s.a.store,
        auth: f.s.a.auth,
        root: f.s.a.root,
        user: f.s.user,
        email: f.s.email,
        credential: f.s.password,
        viewer_email: viewer.email,
        viewer_credential: viewer.credential.unwrap(),
        project: f.project,
        fingerprint,
        totp,
        recovery,
    }
}
struct PanelFixturePolicy;
impl rolvya_plan_validator::AssignmentPolicy for PanelFixturePolicy {
    fn check(
        &self,
        a: &ExecutionAssignment,
    ) -> Result<(), rolvya_plan_validator::AssignmentDenial> {
        if a.model == "synthetic/model" {
            Ok(())
        } else {
            Err(rolvya_plan_validator::AssignmentDenial::UnknownModel)
        }
    }
}
impl PlanAuthorization for PanelFixturePolicy {
    fn permits(&self, o: &PrincipalId, a: &PrincipalId, _: &Revision, _: PlanOperation) -> bool {
        o == a && a.as_str() == "legacy-only-actor"
    }
}
async fn activate(f: &Fixture) -> String {
    let key = PlanKey {
        project_id: f.project.clone(),
        plan_id: PlanId::new("panel-plan").unwrap(),
        version: PlanVersion::new(1).unwrap(),
    };
    let z = usd(0);
    let costs = CostBreakdown {
        ai_api: z,
        compute: z,
        storage: z,
        transfer: z,
    };
    let task = PlannedTask {
        id: TaskId::new("task-a").unwrap(),
        plan: key.clone(),
        parent: None,
        dependencies: vec![],
        kind: TaskKind::ExecutableLeaf,
        required: true,
        goal: "Bounded panel task".to_owned(),
        instructions: "PRIVATE-INSTRUCTIONS-NOT-FOR-PANEL".to_owned(),
        scope: vec!["src/alpha.rs".to_owned()],
        exclusions: vec![],
        acceptance_criteria: vec!["Independent tests pass".to_owned()],
        context: ContextReference {
            reference: "PRIVATE-CONTEXT".to_owned(),
            sha256: "a".repeat(64),
        },
        upstream_artifacts: vec![],
        execution: Some(ExecutionAssignment {
            model: "synthetic/model".to_owned(),
            effort: Effort::Medium,
            access_mode: AccessMode::ApiKey,
            connection_ref: "PRIVATE-CREDENTIAL-REFERENCE".to_owned(),
            resources: ResourceRequirements {
                cpu_millis: 1000,
                memory_mib: 512,
                timeout_seconds: 60,
            },
            costs: CostScenarios {
                optimistic: costs,
                expected: costs,
                pessimistic: costs,
            },
            write_scope: WriteScope::ReadOnly {},
            policy_refs: vec!["fixture/no-spend".to_owned()],
        }),
    };
    let content = PlanContent {
        schema_version: 1,
        key,
        root_policy: RootPolicy::Forest,
        tasks: vec![task],
    };
    let valid = rolvya_plan_validator::validate(
        &content,
        &PanelFixturePolicy,
        rolvya_plan_validator::Limits::default(),
    )
    .unwrap();
    let actor = PrincipalId::new("legacy-only-actor").unwrap();
    let revision =
        f.s.a
            .store
            .save_draft(&valid, None, &actor, &PanelFixturePolicy)
            .await
            .unwrap();
    f.s.a
        .store
        .activate(
            &revision,
            &request(),
            &actor,
            &PanelFixturePolicy,
            &PanelFixturePolicy,
            rolvya_plan_validator::Limits::default(),
        )
        .await
        .unwrap();
    revision.fingerprint().to_owned()
}
#[tokio::test]
async fn panel_projection_requires_current_session_membership_and_bounded_pagination() {
    let f = Fixture::new().await;
    let other = f.account(2, true).await;
    let auth = &f.s.a.auth;
    assert!(
        auth.panel_projects(
            &f.owner.handle,
            None,
            0,
            &f.ctx(&f.owner, Operation::Read, 1)
        )
        .await
        .is_err()
    );
    assert!(
        auth.panel_projects(
            &f.owner.handle,
            None,
            101,
            &f.ctx(&f.owner, Operation::Read, 1)
        )
        .await
        .is_err()
    );
    let items = auth
        .panel_projects(
            &f.owner.handle,
            None,
            1,
            &f.ctx(&f.owner, Operation::Read, 1),
        )
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, f.project.as_str());
    assert!(
        auth.panel_projects(
            &other.session.handle,
            None,
            10,
            &f.ctx(&other.session, Operation::Read, 2)
        )
        .await
        .unwrap()
        .is_empty()
    );
    assert!(
        auth.panel_snapshot(
            &other.session.handle,
            &f.project,
            &f.ctx(&other.session, Operation::Read, 2)
        )
        .await
        .is_err()
    );
    assert!(
        auth.panel_projects(
            &f.owner.handle,
            Some(&f.project),
            1,
            &f.ctx(&f.owner, Operation::Read, 1)
        )
        .await
        .unwrap()
        .is_empty()
    );
    let another = ProjectId::new("another-project").unwrap();
    assert!(
        auth.panel_snapshot(
            &f.owner.handle,
            &another,
            &f.ctx(&f.owner, Operation::Read, 1)
        )
        .await
        .is_err()
    );
    sqlx::query("UPDATE human_users SET security_epoch=security_epoch+1 WHERE user_id=?")
        .bind(&f.s.user.0)
        .execute(&f.s.a.store.pool)
        .await
        .unwrap();
    assert!(
        auth.panel_projects(
            &f.owner.handle,
            None,
            1,
            &f.ctx(&f.owner, Operation::Read, 1)
        )
        .await
        .is_err()
    );
}
#[tokio::test]
async fn panel_projection_preserves_fingerprint_and_redacts_private_context_and_authority() {
    let f = Fixture::new().await;
    let fingerprint = activate(&f).await;
    let before = f.s.inspect(&f.owner).await.unwrap().last_human_at;
    let snapshot =
        f.s.a
            .auth
            .panel_snapshot(
                &f.owner.handle,
                &f.project,
                &f.ctx(&f.owner, Operation::Read, 1),
            )
            .await
            .unwrap();
    assert_eq!(snapshot.plan.fingerprint, fingerprint);
    assert_eq!(snapshot.tasks[0].status, "planned");
    assert_eq!(snapshot.tasks[0].title, "Bounded panel task");
    assert!(snapshot.plan.complete);
    let json = serde_json::to_string(&snapshot).unwrap();
    let contract: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/web/src/testing/controller-snapshot.json"
    )))
    .unwrap();
    let mut actual = serde_json::to_value(&snapshot).unwrap();
    // The cursor MAC is installation-specific; its cross-scope verification is
    // exercised through accepted TLS. Normalize only that opaque contract field.
    assert_eq!(
        snapshot.event_cursor.split_once('.').unwrap().0,
        snapshot.cursor.to_string()
    );
    assert_eq!(snapshot.event_cursor.split_once('.').unwrap().1.len(), 64);
    actual["eventCursor"] = contract["eventCursor"].clone();
    assert_eq!(actual, contract);
    for secret in [
        "PRIVATE-INSTRUCTIONS",
        "PRIVATE-CONTEXT",
        "PRIVATE-CREDENTIAL",
        "legacy-only-actor",
    ] {
        assert!(!json.contains(secret));
    }
    assert!(snapshot.finance.is_none());
    assert!(!snapshot.capabilities.mutations);
    assert!(!snapshot.capabilities.events);
    assert_eq!(before, f.s.inspect(&f.owner).await.unwrap().last_human_at);
    // A process/runtime success without independent R09 acceptance is never verified success.
    sqlx::query("UPDATE task_runtime SET state='succeeded' WHERE project_id=?")
        .bind(f.project.as_str())
        .execute(&f.s.a.store.pool)
        .await
        .unwrap();
    let snapshot =
        f.s.a
            .auth
            .panel_snapshot(
                &f.owner.handle,
                &f.project,
                &f.ctx(&f.owner, Operation::Read, 1),
            )
            .await
            .unwrap();
    assert_eq!(snapshot.tasks[0].status, "unknown");
    assert_eq!(snapshot.plan.fingerprint, fingerprint);
    assert_eq!(f.s.a.count("task_attempts").await, 0);
    assert_eq!(f.s.a.count("financial_events").await, 0);
}
#[tokio::test]
async fn panel_viewer_reads_plan_but_never_finance_or_mutation_capability() {
    let f = Fixture::new().await;
    activate(&f).await;
    let viewer = f.account(2, true).await;
    let invite = f.invite(&viewer, ProjectRole::Viewer).await;
    f.redeem(&viewer, &invite).await.unwrap();
    let snapshot =
        f.s.a
            .auth
            .panel_snapshot(
                &viewer.session.handle,
                &f.project,
                &f.ctx(&viewer.session, Operation::Read, 2),
            )
            .await
            .unwrap();
    assert_eq!(snapshot.role, ProjectRole::Viewer);
    assert!(snapshot.finance.is_none());
    assert!(!snapshot.capabilities.finance);
    assert!(!snapshot.capabilities.mutations);
    sqlx::query("INSERT INTO controller_restore_holds VALUES(?,?,?,?)")
        .bind("a1".repeat(32))
        .bind("b1".repeat(32))
        .bind("c1".repeat(32))
        .bind(f.s.a.now())
        .execute(&f.s.a.store.pool)
        .await
        .unwrap();
    let restored =
        f.s.a
            .auth
            .panel_snapshot(
                &viewer.session.handle,
                &f.project,
                &f.ctx(&viewer.session, Operation::Read, 2),
            )
            .await
            .unwrap();
    assert!(restored.finance.is_none());
    assert_eq!(
        serde_json::to_value(&restored).unwrap()["recoveryRequired"],
        true
    );

    assert!(
        f.s.a
            .auth
            .project_budget(
                &viewer.session.handle,
                &f.project,
                &f.ctx(&viewer.session, Operation::Read, 2)
            )
            .await
            .is_err()
    );
}
#[tokio::test]
async fn panel_production_browser_evidence_cannot_be_forged() {
    let mut f = Fixture::new().await;
    activate(&f).await;
    f.s.a.auth.browser_authority = Arc::new(Unimplemented);
    assert_eq!(
        f.s.a
            .auth
            .panel_snapshot(
                &f.owner.handle,
                &f.project,
                &f.ctx(&f.owner, Operation::Read, 1)
            )
            .await
            .unwrap_err(),
        AuthError::Unavailable
    );
}
#[tokio::test]
async fn panel_axum_transport_is_closed_even_with_cookie_and_worker_bearer() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server =
        tokio::spawn(async move { axum::serve(listener, browser_panel_routes()).await.unwrap() });
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    for path in [
        "projects",
        "projects/project-a/snapshot",
        "projects/project-a/events",
    ] {
        let response = client
            .get(format!("http://{address}/api/control/v1/{path}"))
            .header("Cookie", format!("__Host-rolvya={}", "0".repeat(64)))
            .bearer_auth("0".repeat(64))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 503);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let text = response.text().await.unwrap();
        assert!(text.contains("browser_transport_unavailable"));
        assert!(!text.contains("project-a"));
    }
    server.abort();
}

#[tokio::test]
async fn panel_preflight_uses_byte_limits_before_materializing_definitions() {
    let mut c = sqlx::SqliteConnection::connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("CREATE TABLE definitions(definition TEXT NOT NULL)")
        .execute(&mut c)
        .await
        .unwrap();
    let multibyte = "é".repeat(40000);
    sqlx::query("INSERT INTO definitions VALUES(?)")
        .bind(&multibyte)
        .execute(&mut c)
        .await
        .unwrap();
    let r=sqlx::query("SELECT count(*) AS n,max(length(CAST(definition AS BLOB))) AS maximum,sum(length(CAST(definition AS BLOB))) AS total FROM definitions").fetch_one(&mut c).await.unwrap();
    assert_eq!(r.get::<i64, _>("maximum"), 80000);
    assert_eq!(
        super::super::panel::resource_bounds(r.get("n"), r.get("maximum"), r.get("total")),
        Err(AuthError::Unavailable)
    );
    assert_eq!(
        super::super::panel::resource_bounds(5001, 100, 500100),
        Err(AuthError::Unavailable)
    );
    assert_eq!(
        super::super::panel::resource_bounds(100, 20000, 2000000),
        Err(AuthError::Unavailable)
    );
    assert!(super::super::panel::resource_bounds(10, 2000, 20000).is_ok());
}

#[tokio::test]
async fn panel_finance_projects_real_owner_ledger_without_inventing_forecasts() {
    let f = Fixture::new().await;
    activate(&f).await;
    let action = budget(0, 0, 1200);
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    f.execute(&id, &action).await.unwrap();
    let snapshot =
        f.s.a
            .auth
            .panel_snapshot(
                &f.owner.handle,
                &f.project,
                &f.ctx(&f.owner, Operation::Read, 1),
            )
            .await
            .unwrap();
    let value = serde_json::to_value(snapshot).unwrap();
    assert_eq!(value["finance"]["ceiling"], "1200");
    assert_eq!(value["finance"]["version"], 1);
    assert_eq!(value["finance"]["forecasts"], serde_json::Value::Null);
    assert_eq!(value["finance"]["categories"].as_array().unwrap().len(), 4);
    assert_eq!(value["capabilities"]["finance"], true);
}

impl GitOwnerFixture {
    pub(in crate::controller_keys) async fn operational_snapshot(&self) -> PanelSnapshot {
        self.try_operational_snapshot().await.unwrap()
    }
    pub(in crate::controller_keys) async fn try_operational_snapshot(
        &self,
    ) -> Result<PanelSnapshot, AuthError> {
        let f = &self.0;
        f.s.a
            .auth
            .panel_snapshot(
                &f.owner.handle,
                &f.project,
                &f.ctx(&f.owner, Operation::Read, 1),
            )
            .await
    }
}

#[tokio::test]
async fn panel_never_calls_stale_runtime_without_current_worker_running() {
    let f = Fixture::new().await;
    activate(&f).await;
    sqlx::query("UPDATE task_runtime SET state='running' WHERE project_id=?")
        .bind(f.project.as_str())
        .execute(&f.s.a.store.pool)
        .await
        .unwrap();
    let snapshot =
        f.s.a
            .auth
            .panel_snapshot(
                &f.owner.handle,
                &f.project,
                &f.ctx(&f.owner, Operation::Read, 1),
            )
            .await
            .unwrap();
    assert_eq!(snapshot.tasks[0].status, "unknown");
    assert!(snapshot.attempts.is_empty());
}

impl GitOwnerFixture {
    pub(in crate::controller_keys) async fn operational_browser_seed(
        &self,
        bootstrap: &crate::controller_keys::ControllerBootstrap,
        fingerprint: &str,
        factor: bool,
    ) -> BrowserSeed {
        let f = &self.0;
        let viewer = f.account(2, true).await;
        let invite = f.invite(&viewer, ProjectRole::Viewer).await;
        f.redeem(&viewer, &invite).await.unwrap();
        f.bind(&ProjectId::new("project-b").unwrap()).await;
        let (totp, recovery) = if factor {
            let (enrollment, active) = f.s.factor().await;
            f.s.a.advance(30);
            (
                Some(enrollment.seed),
                active.recovery.codes.into_iter().map(|c| c.0).collect(),
            )
        } else {
            (None, vec![])
        };
        let mut auth = HumanAuth::open(bootstrap).await.unwrap();
        // Existing Git/R09 private fixtures enroll with the test-only low-cost
        // Argon2 profile; the separate ordinary browser fixture uses defaults.
        auth.passwords = super::super::super::super::credentials::PasswordEngine::synthetic();
        auth.clock = f.s.a.auth.clock.clone();
        BrowserSeed {
            auth,
            root: tempfile::tempdir().unwrap(),
            store: f.s.a.store.clone(),
            clock: f.s.a.browser_clock(),
            mail: f.s.a.browser_mail_probe(),
            user: f.s.user.clone(),
            email: f.s.email.clone(),
            credential: Password::new(f.s.password.0.to_vec()).unwrap(),
            viewer_email: viewer.email,
            viewer_credential: viewer.credential.unwrap(),
            project: f.project.clone(),
            fingerprint: fingerprint.to_owned(),
            totp,
            recovery,
        }
    }
}

#[tokio::test]
async fn operational_pages_are_bounded_scoped_and_revocation_is_current() {
    let f = Fixture::new().await;
    activate(&f).await;
    let viewer = f.account(2, true).await;
    let invite = f.invite(&viewer, ProjectRole::Viewer).await;
    f.redeem(&viewer, &invite).await.unwrap();
    for _ in 0..31 {
        sqlx::query("INSERT INTO scheduler_events(project_id,request_id,kind,actor,recorded_at,payload) VALUES(?,?,'validate',?,?,'{\"private\":\"PRIVATE-CREDENTIAL\"}')").bind(f.project.as_str()).bind(new_id().unwrap()).bind(f.s.user.as_str()).bind(f.s.a.now()).execute(&f.s.a.store.pool).await.unwrap();
    }
    let auth = &f.s.a.auth;
    let ctx = f.ctx(&viewer.session, Operation::Read, 2);
    let mut cursor = None;
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..20 {
        let page = auth
            .panel_collection(
                &viewer.session.handle,
                &f.project,
                "activity/scheduler",
                cursor.as_deref(),
                3,
                &ctx,
            )
            .await
            .unwrap();
        assert!(page["items"].as_array().unwrap().len() <= 3);
        assert!(!page.to_string().contains("PRIVATE-CREDENTIAL"));
        for item in page["items"].as_array().unwrap() {
            assert!(seen.insert(item["id"].as_str().unwrap().to_owned()));
        }
        cursor = page["next"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 31);
    let first = auth
        .panel_collection(
            &viewer.session.handle,
            &f.project,
            "activity/scheduler",
            None,
            1,
            &ctx,
        )
        .await
        .unwrap();
    let token = first["next"].as_str().unwrap();
    assert!(
        auth.panel_collection(
            &f.owner.handle,
            &f.project,
            "activity/scheduler",
            Some(token),
            1,
            &f.ctx(&f.owner, Operation::Read, 1)
        )
        .await
        .is_err()
    );
    let snapshot = auth
        .panel_snapshot(&viewer.session.handle, &f.project, &ctx)
        .await
        .unwrap();
    let action = OwnerAction::membership(&viewer.id, 1, Some(ProjectRole::Editor)).unwrap();
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    f.execute(&id, &action).await.unwrap();
    assert!(
        auth.scoped_panel_events(
            &viewer.session.handle,
            &f.project,
            Some(&snapshot.event_cursor),
            &ctx
        )
        .await
        .is_err()
    );
    let action = OwnerAction::membership(&viewer.id, 2, None).unwrap();
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    f.execute(&id, &action).await.unwrap();
    assert!(
        auth.panel_collection(
            &viewer.session.handle,
            &f.project,
            "activity/scheduler",
            Some(token),
            1,
            &ctx
        )
        .await
        .is_err()
    );
    assert!(
        auth.panel_collection(
            &f.owner.handle,
            &f.project,
            "activity/scheduler",
            None,
            51,
            &f.ctx(&f.owner, Operation::Read, 1)
        )
        .await
        .is_err()
    );
}
impl GitOwnerFixture {
    pub(in crate::controller_keys) async fn change_browser_viewer_role(&self) {
        let f = &self.0;
        let (id,version):(String,i64)=sqlx::query_as("SELECT user_id,version FROM human_memberships WHERE project_id=? AND role='viewer' AND state='active'").bind(f.project.as_str()).fetch_one(&f.s.a.store.pool).await.unwrap();
        let action = OwnerAction::membership(
            &HumanId::parse(id).unwrap(),
            version,
            Some(ProjectRole::Editor),
        )
        .unwrap();
        let id = f.prepare(&action).await;
        f.prove(&id).await;
        f.execute(&id, &action).await.unwrap();
    }
}

#[tokio::test]
async fn panel_uses_latest_liability_revision_and_preserves_unknown_headroom() {
    use crate::finance::{
        EvidenceRef, FinancialAuthorization, FinancialCommand, FinancialRequest, LiabilityId,
    };
    struct ReportOnly;
    impl FinancialAuthorization for ReportOnly {
        fn permits(&self, owner: &PrincipalId, r: &FinancialRequest, _: i64) -> bool {
            r.actor == *owner && matches!(r.command, FinancialCommand::Report { .. })
        }
    }
    let f = Fixture::new().await;
    activate(&f).await;
    let action = budget(0, 0, 1200);
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    f.execute(&id, &action).await.unwrap();
    for (revision, value) in [(0, 10), (1, 30)] {
        f.s.a
            .store
            .apply_financial(
                &FinancialRequest {
                    project: f.project.clone(),
                    request_id: request(),
                    actor: PrincipalId::new("legacy-only-actor").unwrap(),
                    reason: "Private independently attributable billing observation".into(),
                    command: FinancialCommand::Report {
                        liability: LiabilityId::new("panel-observation").unwrap(),
                        expected_revision: revision,
                        unreserved: CostBreakdown {
                            ai_api: usd(value),
                            compute: usd(0),
                            storage: usd(0),
                            transfer: usd(0),
                        },
                        uncertain: true,
                        evidence: EvidenceRef::new(format!("panel-observation-{revision}"))
                            .unwrap(),
                    },
                },
                &ReportOnly,
            )
            .await
            .unwrap();
    }
    let v = serde_json::to_value(
        f.s.a
            .auth
            .panel_snapshot(
                &f.owner.handle,
                &f.project,
                &f.ctx(&f.owner, Operation::Read, 1),
            )
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(v["finance"]["provisional"], "30"); // cumulative positions, not 10+30
    assert_eq!(v["finance"]["unknownLiabilities"], 1);
    assert!(v["finance"]["available"].is_null());
    assert!(v["finance"]["forecasts"].is_null());
    assert_eq!(v["finance"]["paused"], true);
    assert_eq!(v["finance"]["ceiling"], "1200");
    assert_eq!(f.s.a.count("task_attempts").await, 0);
}
