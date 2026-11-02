//! Bounded display records. No returned value is an execution or financial grant.
use super::*;
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelAttempt {
    pub id: String,
    pub task: String,
    pub epoch: i64,
    pub worker: Option<String>,
    pub provider: Option<String>,
    pub connection: Option<String>,
    pub state: &'static str,
    pub admitted: String,
    pub started: Option<String>,
    pub ended: Option<String>,
    pub lease_until: i64,
    pub observed_at: i64,
    pub phase: String,
    pub steps: Vec<PanelStep>,
    pub logs: Vec<String>,
    pub logs_available: bool,
}
#[derive(Debug, Serialize)]
pub struct PanelStep {
    pub label: &'static str,
    pub at: String,
    pub complete: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelConnection {
    pub id: String,
    pub provider: &'static str,
    pub label: &'static str,
    pub account: String,
    pub mode: &'static str,
    pub status: &'static str,
    pub scope: String,
    pub version: i64,
    pub reason: &'static str,
    pub operational: bool,
    pub models: Vec<String>,
    pub verified_until: Option<i64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelResult {
    pub id: String,
    pub task: String,
    pub attempt: String,
    pub accepted: bool,
    pub verification: &'static str,
    pub tests: Vec<String>,
    pub sha256: String,
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub published: bool,
    pub at: String,
    pub fingerprint: String,
    pub epoch: i64,
    pub publication_state: &'static str,
    pub acceptance: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct PanelActivity {
    pub id: String,
    pub task: Option<String>,
    pub at: Option<String>,
    pub kind: &'static str,
    pub title: String,
    pub detail: &'static str,
    pub source: String,
    pub sequence: i64,
}
#[derive(Debug, Serialize)]
pub struct PanelPage<T> {
    pub schema: u8,
    pub project: String,
    pub resource: String,
    pub provenance: &'static str,
    pub items: Vec<T>,
    pub next: Option<String>,
}
#[derive(serde::Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PageCursor {
    key: i64,
}
impl HumanAuth {
    pub(super) fn panel_connection_id(&self, p: &ProjectId, kind: &str, id: &str) -> String {
        format!(
            "connection-{}",
            hex(&self.mac(
                b"panel-connection",
                &[p.as_str().as_bytes(), kind.as_bytes(), id.as_bytes()]
            )[..16])
        )
    }
    pub(super) fn panel_cursor(
        &self,
        p: &ProjectId,
        user: &str,
        resource: &str,
        key: i64,
    ) -> Result<String, AuthError> {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        let bytes = serde_json::to_vec(&PageCursor { key }).map_err(|_| AuthError::Storage)?;
        let tag = self.mac(
            b"panel-page",
            &[
                p.as_str().as_bytes(),
                user.as_bytes(),
                resource.as_bytes(),
                &bytes,
            ],
        );
        Ok(format!("{}.{}", URL_SAFE_NO_PAD.encode(&bytes), hex(&tag)))
    }
    pub(super) fn panel_after(
        &self,
        p: &ProjectId,
        user: &str,
        resource: &str,
        cursor: Option<&str>,
    ) -> Result<i64, AuthError> {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        let Some(cursor) = cursor else {
            return Ok(i64::MAX);
        };
        if cursor.len() > 256 {
            return Err(AuthError::InvalidInput);
        }
        let (data, tag) = cursor.split_once('.').ok_or(AuthError::InvalidInput)?;
        let bytes = URL_SAFE_NO_PAD
            .decode(data)
            .map_err(|_| AuthError::InvalidInput)?;
        let expected = hex(&self.mac(
            b"panel-page",
            &[
                p.as_str().as_bytes(),
                user.as_bytes(),
                resource.as_bytes(),
                &bytes,
            ],
        ));
        if !constant_eq(expected.as_bytes(), tag.as_bytes()) {
            return Err(AuthError::InvalidInput);
        }
        let value: PageCursor =
            serde_json::from_slice(&bytes).map_err(|_| AuthError::InvalidInput)?;
        if value.key <= 0 {
            return Err(AuthError::InvalidInput);
        }
        Ok(value.key)
    }
    pub(super) fn panel_page<T>(
        &self,
        p: &ProjectId,
        user: &str,
        resource: &str,
        mut items: Vec<(i64, T)>,
        limit: u16,
    ) -> Result<PanelPage<T>, AuthError> {
        let more = items.len() > usize::from(limit);
        items.truncate(usize::from(limit));
        let next = if more {
            items
                .last()
                .map(|(n, _)| self.panel_cursor(p, user, resource, *n))
                .transpose()?
        } else {
            None
        };
        Ok(PanelPage {
            schema: 2,
            project: p.as_str().to_owned(),
            resource: resource.to_owned(),
            provenance: "controller",
            items: items.into_iter().map(|(_, v)| v).collect(),
            next,
        })
    }
    pub(super) async fn panel_attempts_in(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        s: &Scope,
        after: i64,
        limit: u16,
        now: i64,
    ) -> Result<PanelPage<PanelAttempt>, AuthError> {
        let p = &s.project;
        let rows=sqlx::query("SELECT a.attempt_id,a.task_id,a.epoch,a.admission_event,strftime('%Y-%m-%dT%H:%M:%SZ',a.admitted_at,'unixepoch') AS admitted,st.state AS authority,f.epoch AS fence,f.current_attempt,l.lease_until,l.observed_at,d.state AS external,json_extract(d.command,'$.binding.worker_id') AS worker,json_extract(d.command,'$.admission.permit.provider') AS provider,json_extract(ws.state,'$.phase') AS phase,pc.connection_id FROM task_attempts a JOIN plan_versions pv USING(project_id,plan_id,version) JOIN attempt_states st USING(project_id,attempt_id) JOIN task_fences f USING(project_id,plan_id,version,task_id) JOIN worker_leases l USING(project_id,attempt_id) LEFT JOIN dispatch_operations d USING(project_id,attempt_id) LEFT JOIN worker_sessions ws USING(project_id,attempt_id) LEFT JOIN provider_capacity_reservations pc USING(project_id,attempt_id) WHERE a.project_id=? AND pv.state='active' AND a.admission_event<? ORDER BY a.admission_event DESC LIMIT ?")
            .bind(p.as_str()).bind(after).bind(i64::from(limit)+1).fetch_all(&mut **tx).await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let id: String = r.try_get("attempt_id")?;
            let phase: Option<String> = r.try_get("phase")?;
            let external: Option<String> = r.try_get("external")?;
            let authority: &str = r.try_get("authority")?;
            let current = r.try_get::<i64, _>("epoch")? == r.try_get::<i64, _>("fence")?
                && r.try_get::<Option<String>, _>("current_attempt")?
                    .as_deref()
                    == Some(&id);
            let state = if authority == "fenced" || !current {
                "fenced"
            } else if authority == "succeeded" {
                "accepted"
            } else if matches!(external.as_deref(), Some("launching" | "reconcile")) {
                "unknown"
            } else if matches!(external.as_deref(), Some("blocked")) {
                "blocked"
            } else if matches!(external.as_deref(), Some("finished" | "no_effect")) {
                "terminated"
            } else if phase.as_deref() == Some("ResultReceived") {
                "awaiting_verification"
            } else if r.try_get::<i64, _>("lease_until")? <= now {
                "unknown"
            } else if external.as_deref() == Some("active") && phase.as_deref() == Some("Executing")
            {
                "running"
            } else {
                "admitted"
            };
            let admitted: String = r.try_get("admitted")?;
            let connection: Option<String> = r.try_get("connection_id")?;
            out.push((
                r.try_get("admission_event")?,
                PanelAttempt {
                    id,
                    task: r.try_get("task_id")?,
                    epoch: r.try_get("epoch")?,
                    worker: r.try_get("worker")?,
                    provider: r.try_get("provider")?,
                    connection: connection.map(|id| self.panel_connection_id(p, "model", &id)),
                    state,
                    admitted: admitted.clone(),
                    started: None,
                    ended: None,
                    lease_until: r.try_get("lease_until")?,
                    observed_at: r.try_get("observed_at")?,
                    phase: format!(
                        "{} · external {}",
                        phase.as_deref().unwrap_or("No worker receipt"),
                        external.as_deref().unwrap_or("not dispatched")
                    ),
                    steps: vec![PanelStep {
                        label: "Admitted with financial hold",
                        at: admitted,
                        complete: true,
                    }],
                    logs: vec![],
                    logs_available: false,
                },
            ));
        }
        self.panel_page(p, s.user()?, "attempts", out, limit)
    }
    pub(super) async fn panel_activity_in(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        s: &Scope,
        source: &str,
        after: i64,
        limit: u16,
    ) -> Result<PanelPage<PanelActivity>, AuthError> {
        if source == "publication" {
            let rows = crate::controller_keys::github_app::panel_activity(
                tx, &self.key, &s.project, after, limit,
            )
            .await
            .map_err(|_| AuthError::Unavailable)?;
            let mut items = Vec::with_capacity(rows.len());
            for (n, kind, at) in rows {
                let at: String =
                    sqlx::query_scalar("SELECT strftime('%Y-%m-%dT%H:%M:%SZ',?,'unixepoch')")
                        .bind(at)
                        .fetch_one(&mut **tx)
                        .await?;
                items.push((n, PanelActivity { id: format!("publication-{n}"), task: None, at: Some(at), kind: "publication", title: format!("publication: {}", kind.replace('_', " ")), detail: "Authenticated publisher journal; external effects and reconciliation remain distinct.", source: source.to_owned(), sequence: n }));
            }
            return self.panel_page(&s.project, s.user()?, "activity/publication", items, limit);
        }
        let (table, event, at, kind, owner) = match source {
            "scheduler" => (
                "scheduler_events",
                "kind",
                "recorded_at",
                "execution",
                false,
            ),
            "worker" => ("worker_events", "kind", "recorded_at", "execution", false),
            "activation" => (
                "activation_events",
                "'activated'",
                "NULL",
                "execution",
                false,
            ),
            "finance" => ("financial_events", "kind", "recorded_at", "financial", true),
            "governance" => (
                "human_governance_events",
                "event",
                "occurred_at",
                "security",
                true,
            ),
            "provider" => (
                "provider_security_events",
                "kind",
                "recorded_at",
                "security",
                true,
            ),
            _ => return Err(AuthError::InvalidInput),
        };
        if owner && s.membership.role != ProjectRole::Owner {
            return Err(AuthError::Denied);
        }
        // All identifiers above are fixed Rust literals, never browser SQL input.
        let query = format!(
            "SELECT sequence,{event} AS event,strftime('%Y-%m-%dT%H:%M:%SZ',{at},'unixepoch') AS at FROM {table} WHERE project_id=? AND sequence<? ORDER BY sequence DESC LIMIT ?"
        );
        let rows = sqlx::query(&query)
            .bind(s.project.as_str())
            .bind(after)
            .bind(i64::from(limit) + 1)
            .fetch_all(&mut **tx)
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let sequence: i64 = r.try_get("sequence")?;
            let event: String = r.try_get("event")?;
            if event.len() > 64 || !event.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
                return Err(AuthError::Unavailable);
            }
            out.push((
                sequence,
                PanelActivity {
                    id: format!("{source}-{sequence}"),
                    task: None,
                    at: r.try_get("at")?,
                    kind: if source == "scheduler" && event == "validate" {
                        "verification"
                    } else {
                        kind
                    },
                    title: format!("{source}: {}", event.replace('_', " ")),
                    detail: "Redacted journal metadata; payload and credentials withheld.",
                    source: source.to_owned(),
                    sequence,
                },
            ));
        }
        self.panel_page(
            &s.project,
            s.user()?,
            &format!("activity/{source}"),
            out,
            limit,
        )
    }
}

impl HumanAuth {
    /// Each page is newly authorized; a scoped cursor is never a permission.
    pub async fn panel_collection(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        resource: &str,
        cursor: Option<&str>,
        limit: u16,
        c: &BrowserContext,
    ) -> Result<serde_json::Value, AuthError> {
        if !(1..=50).contains(&limit) {
            return Err(AuthError::InvalidInput);
        }
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Read).await?;
        let s = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Read, now)
            .await?;
        let after = self.panel_after(p, s.user()?, resource, cursor)?;
        let mut result = match resource {
            "attempts" => serde_json::to_value(
                self.panel_attempts_in(&mut tx, &s, after, limit, now)
                    .await?,
            ),
            "approvals" => {
                serde_json::to_value(self.panel_approvals_in(&mut tx, &s, after, limit).await?)
            }
            "results" => {
                serde_json::to_value(self.panel_results_in(&mut tx, &s, after, limit).await?)
            }
            resource if resource.starts_with("connections/") => serde_json::to_value(
                self.panel_connections_in(&mut tx, &s, &resource[12..], after, limit, now)
                    .await?,
            ),
            resource if resource.starts_with("activity/") => serde_json::to_value(
                self.panel_activity_in(&mut tx, &s, &resource[9..], after, limit)
                    .await?,
            ),
            _ => return Err(AuthError::InvalidInput),
        }
        .map_err(|_| AuthError::Storage)?;
        result["role"] = serde_json::to_value(s.membership.role).map_err(|_| AuthError::Storage)?;
        if serde_json::to_vec(&result)
            .map_err(|_| AuthError::Storage)?
            .len()
            > 500_000
        {
            return Err(AuthError::Unavailable);
        }
        self.session_checkpoint(
            c,
            Operation::Read,
            s.deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(result)
    }
}
