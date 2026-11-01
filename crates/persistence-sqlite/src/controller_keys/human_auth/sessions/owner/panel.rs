//! Bounded, redacted panel projections under the existing session/membership transaction.
//! This is not an activated browser login/HTTP transport and confers no command authority.
use super::*;
use rolvya_domain::plan::{AccessMode, PlannedTask};

#[derive(Debug, Serialize)]
pub struct PanelProject {
    pub id: String,
    pub name: String,
    pub detail: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelProjectIdentity {
    pub id: String,
    pub name: String,
    pub goal: String,
    pub repository: Option<String>,
    pub repository_id: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct PanelPlan {
    pub id: String,
    pub version: i64,
    pub fingerprint: String,
    pub complete: bool,
}
#[derive(Debug, Serialize)]
pub struct PanelTask {
    pub id: String,
    pub title: String,
    pub kind: &'static str,
    pub parent: Option<String>,
    pub dependencies: Vec<String>,
    pub status: &'static str,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub mode: Option<&'static str>,
    pub scope: Vec<String>,
    pub criteria: Vec<String>,
    pub blockers: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct PanelCapabilities {
    pub events: bool,
    pub finance: bool,
    pub connections: bool,
    pub results: bool,
    pub mutations: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelSnapshot {
    pub schema: u8,
    pub provenance: &'static str,
    pub revision: i64,
    pub cursor: i64,
    pub as_of: String,
    pub role: ProjectRole,
    pub project: PanelProjectIdentity,
    pub plan: PanelPlan,
    pub tasks: Vec<PanelTask>,
    // Explicit unavailable projections, not fabricated zero balances or fixture results.
    pub finance: Option<()>,
    pub attempts: [(); 0],
    pub connections: [(); 0],
    pub results: [(); 0],
    pub activity: [(); 0],
    pub capabilities: PanelCapabilities,
}
pub(super) fn resource_bounds(count: i64, maximum: i64, total: i64) -> Result<(), AuthError> {
    if count > 5000 || maximum > 65536 || total > 1_000_000 || count < 0 || maximum < 0 || total < 0
    {
        Err(AuthError::Unavailable)
    } else {
        Ok(())
    }
}
impl HumanAuth {
    /// Read-only pagination; every returned project also passes the current binding check.
    pub async fn panel_projects(
        &self,
        h: &SessionHandle,
        after: Option<&ProjectId>,
        limit: u16,
        c: &BrowserContext,
    ) -> Result<Vec<PanelProject>, AuthError> {
        if !(1..=100).contains(&limit) {
            return Err(AuthError::InvalidInput);
        }
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Read).await?;
        let session = self.session_row(&mut tx, h, &f, now, false).await?;
        let deadline = self.original_deadline(&mut tx, h, &session).await?;
        let ids: Vec<String> = sqlx::query_scalar("SELECT project_id FROM human_memberships WHERE user_id=? AND state='active' AND project_id>? ORDER BY project_id LIMIT ?")
            .bind(session.try_get::<String,_>("user_id")?).bind(after.map(ProjectId::as_str).unwrap_or("")).bind(limit).fetch_all(&mut *tx).await?;
        let mut projects = Vec::with_capacity(ids.len());
        for id in ids {
            let p = ProjectId::new(&id).map_err(|_| AuthError::Denied)?;
            self.member_in(&mut tx, &p, session.try_get("user_id")?)
                .await?;
            projects.push(PanelProject {
                name: id.clone(),
                id,
                detail: "Current project membership verified".to_owned(),
            });
        }
        self.session_checkpoint(
            c,
            Operation::Read,
            deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(projects)
    }
    /// Complete bounded task projection. Refuse oversized plans instead of truncating a DAG.
    /// Instructions, context bytes, credential references and legacy Owner strings are omitted.
    pub async fn panel_snapshot(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        c: &BrowserContext,
    ) -> Result<PanelSnapshot, AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Read).await?;
        let scope = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Read, now)
            .await?;
        let plan=sqlx::query("SELECT plan_id,version,fingerprint FROM plan_versions WHERE project_id=? AND state='active'").bind(p.as_str()).fetch_optional(&mut *tx).await?.ok_or(AuthError::Unavailable)?;
        let plan_id: String = plan.try_get("plan_id")?;
        let version: i64 = plan.try_get("version")?;
        let fingerprint: String = plan.try_get("fingerprint")?;
        // Enforce the panel budget before SQLx materializes definition strings.
        // length(BLOB) measures bytes; substr(TEXT) would only bound characters.
        let bounds=sqlx::query("SELECT count(*) AS n,coalesce(max(length(CAST(definition AS BLOB))),0) AS maximum,coalesce(sum(length(CAST(definition AS BLOB))),0) AS total FROM tasks WHERE project_id=? AND plan_id=? AND version=?").bind(p.as_str()).bind(&plan_id).bind(version).fetch_one(&mut *tx).await?;
        resource_bounds(
            bounds.try_get("n")?,
            bounds.try_get("maximum")?,
            bounds.try_get("total")?,
        )?;
        let rows=sqlx::query("SELECT substr(t.definition,1,65537) AS definition,r.state,EXISTS(SELECT 1 FROM task_acceptances a WHERE a.project_id=t.project_id AND a.plan_id=t.plan_id AND a.version=t.version AND a.task_id=t.task_id AND a.fingerprint=?) AS accepted FROM tasks t JOIN task_runtime r USING(project_id,plan_id,version,task_id) WHERE t.project_id=? AND t.plan_id=? AND t.version=? ORDER BY t.task_id LIMIT 5001")
            .bind(&fingerprint).bind(p.as_str()).bind(&plan_id).bind(version).fetch_all(&mut *tx).await?;
        if i64::try_from(rows.len()).map_err(|_| AuthError::Unavailable)?
            != bounds.try_get::<i64, _>("n")?
        {
            return Err(AuthError::Unavailable);
        }
        let mut tasks = Vec::with_capacity(rows.len());
        let mut input_bytes = 0usize;
        for r in rows {
            let definition: String = r.try_get("definition")?;
            input_bytes = input_bytes
                .checked_add(definition.len())
                .ok_or(AuthError::Unavailable)?;
            if definition.len() > 65536 || input_bytes > 1_000_000 {
                return Err(AuthError::Unavailable);
            }
            let t: PlannedTask =
                serde_json::from_str(&definition).map_err(|_| AuthError::Storage)?;
            let valid_text = |s: &str, max: usize| {
                s.len() <= max
                    && !s
                        .chars()
                        .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
            };
            if !valid_text(&t.goal, 300)
                || t.scope.len() > 100
                || t.acceptance_criteria.len() > 100
                || t.scope
                    .iter()
                    .chain(&t.acceptance_criteria)
                    .any(|s| !valid_text(s, 512))
            {
                return Err(AuthError::Unavailable);
            }
            let status = match (
                r.try_get::<&str, _>("state")?,
                r.try_get::<bool, _>("accepted")?,
            ) {
                ("succeeded", true) => "verified",
                ("succeeded", false) => "unknown",
                ("waiting", _) => "blocked",
                ("ready", _) => "ready",
                ("running", _) => "running",
                ("validating", _) => "awaiting_verification",
                ("failed", _) => "failed",
                ("canceled", _) => "paused",
                _ => "planned",
            };
            let execution = t.execution.as_ref();
            tasks.push(PanelTask {
                id: t.id.as_str().to_owned(),
                title: t.goal,
                kind: if t.kind == rolvya_domain::plan::TaskKind::Group {
                    "group"
                } else {
                    "leaf"
                },
                parent: t.parent.map(|r| r.task_id.as_str().to_owned()),
                dependencies: t
                    .dependencies
                    .into_iter()
                    .map(|r| r.task_id.as_str().to_owned())
                    .collect(),
                status,
                model: execution.map(|e| e.model.clone()),
                effort: execution.map(|e| {
                    match e.effort {
                        rolvya_domain::plan::Effort::Low => "low",
                        rolvya_domain::plan::Effort::Medium => "medium",
                        rolvya_domain::plan::Effort::High => "high",
                        rolvya_domain::plan::Effort::ExtraHigh => "extra_high",
                    }
                    .to_owned()
                }),
                mode: execution.map(|e| match e.access_mode {
                    AccessMode::ApiKey => "api",
                    AccessMode::CliSession => "cli",
                }),
                scope: t.scope,
                criteria: t.acceptance_criteria,
                blockers: if status == "blocked" {
                    vec!["Required predecessor acceptance is unavailable".to_owned()]
                } else {
                    vec![]
                },
            });
        }
        let revision: i64 = sqlx::query_scalar(
            "SELECT coalesce(max(sequence),0) FROM scheduler_events WHERE project_id=?",
        )
        .bind(p.as_str())
        .fetch_one(&mut *tx)
        .await?;
        let as_of: String =
            sqlx::query_scalar("SELECT strftime('%Y-%m-%dT%H:%M:%SZ',?,'unixepoch')")
                .bind(now)
                .fetch_one(&mut *tx)
                .await?;
        let snapshot = PanelSnapshot {
            schema: 1,
            provenance: "controller",
            revision,
            cursor: revision,
            as_of,
            role: scope.membership.role,
            project: PanelProjectIdentity {
                id: p.as_str().to_owned(),
                name: p.as_str().to_owned(),
                goal: String::new(),
                repository: None,
                repository_id: None,
            },
            plan: PanelPlan {
                id: plan_id,
                version,
                fingerprint,
                complete: true,
            },
            tasks,
            finance: None,
            attempts: [],
            connections: [],
            results: [],
            activity: [],
            capabilities: PanelCapabilities {
                events: false,
                finance: false,
                connections: false,
                results: false,
                mutations: false,
            },
        };
        if serde_json::to_vec(&snapshot)
            .map_err(|_| AuthError::Storage)?
            .len()
            > 1_000_000
        {
            return Err(AuthError::Unavailable);
        }
        self.session_checkpoint(
            c,
            Operation::Read,
            scope.deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(snapshot)
    }
}
/// Deliberately closed Axum boundary. No cookie/header/request field can install
/// BrowserFacts or a successful verifier. Attach a future reviewed transport here;
/// until then the browser must show unavailable rather than receive library authority.
pub fn browser_panel_routes() -> axum::Router {
    use axum::{
        http::{StatusCode, header},
        routing::get,
    };
    async fn unavailable() -> impl axum::response::IntoResponse {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            [
                (header::CACHE_CONTROL, "no-store"),
                (header::CONTENT_TYPE, "application/json"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            "{\"schema\":1,\"error\":\"browser_transport_unavailable\"}",
        )
    }
    axum::Router::new()
        .route("/api/control/v1/projects", get(unavailable))
        .route(
            "/api/control/v1/projects/{project}/snapshot",
            get(unavailable),
        )
        .route(
            "/api/control/v1/projects/{project}/events",
            get(unavailable),
        )
        .layer(axum::extract::DefaultBodyLimit::max(0))
}
