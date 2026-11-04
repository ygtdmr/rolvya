//! Bounded, redacted panel projections under the existing session/membership transaction.
//! The accepted human HTTPS adapter delivers these reads; they confer no command authority.
use super::*;
use rolvya_domain::plan::{AccessMode, PlannedTask};
mod budget;
mod connections;
mod financial;
mod operational;
mod results;
use financial::PanelFinance;
use operational::*;

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
#[serde(rename_all = "camelCase")]
pub struct PanelCapabilities {
    pub execution: bool,
    pub activity: bool,
    pub budget_increase: bool,
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
    pub pages: std::collections::BTreeMap<String, Option<String>>,
    pub provenance: &'static str,
    pub recovery_required: bool,
    pub revision: i64,
    pub cursor: i64,
    pub event_cursor: String,
    pub as_of: String,
    pub role: ProjectRole,
    pub project: PanelProjectIdentity,
    pub plan: PanelPlan,
    pub tasks: Vec<PanelTask>,
    // Explicit unavailable projections, not fabricated zero balances or fixture results.
    pub finance: Option<PanelFinance>,
    pub attempts: Vec<PanelAttempt>,
    pub connections: Vec<PanelConnection>,
    pub qualification: Option<crate::controller_keys::qualification::QualificationSummary>,
    pub results: Vec<PanelResult>,
    pub activity: Vec<PanelActivity>,
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
    async fn panel_revision(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        p: &ProjectId,
    ) -> Result<i64, AuthError> {
        let row=sqlx::query("SELECT (SELECT coalesce(max(sequence),0) FROM scheduler_events WHERE project_id=?1),(SELECT coalesce(max(sequence),0) FROM human_governance_events WHERE project_id=?1),(SELECT coalesce(max(sequence),0) FROM activation_events WHERE project_id=?1),(SELECT coalesce(max(sequence),0) FROM worker_events WHERE project_id=?1),(SELECT coalesce(max(sequence),0) FROM financial_events WHERE project_id=?1),(SELECT coalesce(max(sequence),0) FROM provider_security_events WHERE project_id=?1),(SELECT coalesce(max(sequence),0) FROM secret_events WHERE project_id=?1),(SELECT coalesce(max(rowid),0) FROM git_publication_events WHERE project_id=?1),(SELECT coalesce(max(rowid),0) FROM coding_verifications WHERE project_id=?1),(SELECT coalesce(max(rowid),0) FROM coding_rejections WHERE project_id=?1),(SELECT coalesce(max(rowid),0) FROM aws_execution_events WHERE project_id=?1),(SELECT coalesce(max(rowid),0) FROM provider_qualification_evidence WHERE project_id=?1),(SELECT coalesce(max(rowid),0) FROM provider_qualification_revocations WHERE project_id=?1)").bind(p.as_str()).fetch_one(&mut **tx).await?;
        let mut revision = 0i64;
        for column in 0..13 {
            revision = revision
                .checked_add(row.try_get::<i64, _>(column)?)
                .ok_or(AuthError::Unavailable)?;
        }
        if !(0..=9_007_199_254_740_991).contains(&revision) {
            return Err(AuthError::Unavailable);
        }
        Ok(revision)
    }
    fn scoped_event_cursor(&self, s: &Scope, cursor: i64) -> Result<String, AuthError> {
        Ok(format!(
            "{cursor}.{}",
            hex(&self.mac(
                b"panel-event-cursor",
                &[
                    s.project.as_str().as_bytes(),
                    s.user()?.as_bytes(),
                    s.membership.role.name().as_bytes(),
                    &s.membership.version.to_be_bytes(),
                    &cursor.to_be_bytes()
                ]
            ))
        ))
    }
    /// Accepted HTTPS uses this project/human/membership-bound cursor. The numeric
    /// legacy library helper below cannot authenticate a transport cursor.
    pub(in crate::controller_keys::human_auth) async fn scoped_panel_events(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        cursor: Option<&str>,
        c: &BrowserContext,
    ) -> Result<(String, Vec<(i64, String)>), AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Read).await?;
        let scope = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Read, now)
            .await?;
        let after = if let Some(token) = cursor {
            if token.len() > 96 {
                return Err(AuthError::InvalidInput);
            }
            let (number, _) = token.split_once('.').ok_or(AuthError::InvalidInput)?;
            let number = number.parse::<i64>().map_err(|_| AuthError::InvalidInput)?;
            if number < 0
                || !constant_eq(
                    token.as_bytes(),
                    self.scoped_event_cursor(&scope, number)?.as_bytes(),
                )
            {
                return Err(AuthError::Denied);
            }
            number
        } else {
            0
        };
        let current = self.panel_revision(&mut tx, p).await?;
        if after > current || current.saturating_sub(after) > 1000 {
            return Err(AuthError::Conflict);
        }
        let events = if current > after {
            vec![(current, self.scoped_event_cursor(&scope, current)?)]
        } else {
            vec![]
        };
        let bound = self.scoped_event_cursor(&scope, after)?;
        self.session_checkpoint(
            c,
            Operation::Read,
            scope.deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok((bound, events))
    }
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
        let rows=sqlx::query("SELECT substr(t.definition,1,65537) AS definition,r.state,EXISTS(SELECT 1 FROM task_acceptances a WHERE a.project_id=t.project_id AND a.plan_id=t.plan_id AND a.version=t.version AND a.task_id=t.task_id AND a.fingerprint=?1) AS accepted,EXISTS(SELECT 1 FROM task_dependencies d LEFT JOIN task_acceptances a ON a.project_id=d.project_id AND a.plan_id=d.plan_id AND a.version=d.version AND a.task_id=d.predecessor_id AND a.fingerprint=?1 WHERE d.project_id=t.project_id AND d.plan_id=t.plan_id AND d.version=t.version AND d.successor_id=t.task_id AND a.task_id IS NULL) AS dependency_blocked,EXISTS(SELECT 1 FROM task_fences f JOIN task_attempts a ON a.project_id=f.project_id AND a.attempt_id=f.current_attempt AND a.epoch=f.epoch JOIN attempt_states st ON st.project_id=a.project_id AND st.attempt_id=a.attempt_id AND st.state='admitted' JOIN worker_leases l ON l.project_id=a.project_id AND l.attempt_id=a.attempt_id AND l.lease_until>?5 JOIN dispatch_operations d ON d.project_id=a.project_id AND d.attempt_id=a.attempt_id AND d.state='active' JOIN worker_sessions w ON w.project_id=a.project_id AND w.attempt_id=a.attempt_id AND json_extract(w.state,'$.phase')='Executing' WHERE f.project_id=t.project_id AND f.plan_id=t.plan_id AND f.version=t.version AND f.task_id=t.task_id) AS current_running FROM tasks t JOIN task_runtime r USING(project_id,plan_id,version,task_id) WHERE t.project_id=?2 AND t.plan_id=?3 AND t.version=?4 ORDER BY t.task_id LIMIT 5001")
            .bind(&fingerprint).bind(p.as_str()).bind(&plan_id).bind(version).bind(now).fetch_all(&mut *tx).await?;
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
                ("planned" | "waiting" | "ready", _)
                    if r.try_get::<bool, _>("dependency_blocked")? =>
                {
                    "blocked"
                }
                ("waiting", _) => "blocked",
                ("ready", _) => "ready",
                ("running", _)
                    if t.kind == rolvya_domain::plan::TaskKind::ExecutableLeaf
                        && r.try_get::<bool, _>("current_running")? =>
                {
                    "running"
                }
                ("running", _) => "unknown",
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
        let revision = self.panel_revision(&mut tx, p).await?;
        let as_of: String =
            sqlx::query_scalar("SELECT strftime('%Y-%m-%dT%H:%M:%SZ',?,'unixepoch')")
                .bind(now)
                .fetch_one(&mut *tx)
                .await?;
        // Existing financial/governance visibility is Owner-only. Do not even
        // materialize those records for a less-privileged project reader.
        let mut finance = if scope.membership.role == ProjectRole::Owner {
            self.panel_finance_in(&mut tx, p).await?
        } else {
            None
        };
        let finance_available = finance.is_some();
        let attempts = self
            .panel_attempts_in(&mut tx, &scope, i64::MAX, 20, now)
            .await?;
        let mut pages = std::collections::BTreeMap::new();
        pages.insert("attempts".to_owned(), attempts.next);
        if let Some(finance) = finance.as_mut() {
            let approvals = self
                .panel_approvals_in(&mut tx, &scope, i64::MAX, 20)
                .await?;
            pages.insert("approvals".to_owned(), approvals.next);
            finance.approvals = approvals.items;
        }
        let results = self.panel_results_in(&mut tx, &scope, i64::MAX, 20).await?;
        pages.insert("results".to_owned(), results.next);
        let mut connections = Vec::new();
        if scope.membership.role == ProjectRole::Owner {
            for source in ["model", "github", "aws"] {
                let page = self
                    .panel_connections_in(&mut tx, &scope, source, i64::MAX, 20, now)
                    .await?;
                pages.insert(format!("connections/{source}"), page.next);
                connections.extend(page.items);
            }
        }
        let mut activity = Vec::new();
        for source in [
            "scheduler",
            "worker",
            "activation",
            "finance",
            "governance",
            "provider",
            "publication",
        ] {
            if scope.membership.role != ProjectRole::Owner
                && matches!(source, "finance" | "governance" | "provider")
            {
                continue;
            }
            let page = self
                .panel_activity_in(&mut tx, &scope, source, i64::MAX, 5)
                .await?;
            pages.insert(format!("activity/{source}"), page.next);
            activity.extend(page.items);
        }
        let recovery_required =
            !crate::controller_keys::operations::external_authority_available(&mut tx).await?;
        let snapshot = PanelSnapshot {
            schema: 2,
            pages,
            provenance: "controller",
            recovery_required,
            revision,
            cursor: revision,
            event_cursor: self.scoped_event_cursor(&scope, revision)?,
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
            finance,
            attempts: attempts.items,
            connections,
            qualification: if scope.membership.role == ProjectRole::Owner {
                Some(
                    crate::controller_keys::qualification::panel_summary(
                        &mut tx,
                        self.store.clone(),
                        &self.key,
                        p,
                        now,
                    )
                    .await
                    .map_err(|_| AuthError::Unavailable)?,
                )
            } else {
                None
            },
            results: results.items,
            activity,
            capabilities: PanelCapabilities {
                execution: true,
                activity: true,
                budget_increase: finance_available && scope.membership.role == ProjectRole::Owner,
                events: false,
                finance: finance_available,
                connections: scope.membership.role == ProjectRole::Owner,
                results: true,
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
/// BrowserFacts or a successful verifier. The accepted TLS server has its own
/// private router; mounting this bare router cannot inherit that authority.
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
