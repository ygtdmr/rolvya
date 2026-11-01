use super::*;
use crate::{PlanAuthorization, PlanOperation, Revision};
use rolvya_domain::{
    ids::{PlanId, PlanVersion, TaskId},
    money::{CostBreakdown, CostScenarios},
    plan::*,
};
use sqlx::Connection;
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
    assert_eq!(serde_json::to_value(&snapshot).unwrap(), contract);
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
