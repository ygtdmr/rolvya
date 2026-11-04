//! Read existing authenticated execution and financial journals together. No
//! remote call, new operation, settlement, hold release or restore clearance.
use super::*;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use sqlx::Connection;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReconciledExecution {
    ConfirmedNotLaunched,
    ConfirmedLaunched,
    Running,
    Stopping,
    ConfirmedStopped,
    Unknown,
    ObservationUnavailable,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReconciledFinance {
    Finalized,
    Unresolved,
}
#[derive(Debug, Clone, Serialize)]
pub struct ReconciliationStatus {
    pub attempt: String,
    pub purpose: String,
    pub operation: String,
    pub task: Option<TaskArn>,
    pub execution: ReconciledExecution,
    pub finance: ReconciledFinance,
    pub observation_revision: u64,
    pub observed_at: i64,
    pub external_event_time: Option<i64>,
    pub restore_hold: bool,
    pub relaunch_allowed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct JournalCursor {
    pub attempt: rolvya_domain::ids::AttemptId,
    pub purpose: JournalPurpose,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JournalPurpose {
    Coding,
    Verifier,
}
impl JournalPurpose {
    fn name(&self) -> &'static str {
        match self {
            Self::Coding => "coding",
            Self::Verifier => "verifier",
        }
    }
}
#[derive(Debug, Serialize)]
pub struct ReconciliationPage {
    pub items: Vec<ReconciliationStatus>,
    pub next: Option<JournalCursor>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconciliationQuery {
    pub project: rolvya_domain::ids::ProjectId,
    pub after: Option<JournalCursor>,
    pub limit: u16,
}
async fn page_keys(
    c: &mut SqliteConnection,
    project: &rolvya_domain::ids::ProjectId,
    after: Option<&JournalCursor>,
    limit: u16,
) -> Result<(Vec<JournalCursor>, Option<JournalCursor>), E> {
    if !(1..=50).contains(&limit) {
        return Err(E::ResourceLimit);
    }
    let attempt = after.map(|a| a.attempt.as_str()).unwrap_or("");
    let purpose = after.map(|a| a.purpose.name()).unwrap_or("");
    let rows=sqlx::query("SELECT attempt_id,purpose FROM aws_launch_requests WHERE project_id=? AND (attempt_id>? OR (attempt_id=? AND purpose>?)) ORDER BY attempt_id,purpose LIMIT ?").bind(project.as_str()).bind(attempt).bind(attempt).bind(purpose).bind(i64::from(limit)+1).fetch_all(c).await?;
    let mut keys = Vec::with_capacity(rows.len());
    for row in rows {
        keys.push(JournalCursor {
            attempt: rolvya_domain::ids::AttemptId::new(row.try_get::<&str, _>("attempt_id")?)
                .map_err(|_| E::Integrity)?,
            purpose: match row.try_get::<&str, _>("purpose")? {
                "coding" => JournalPurpose::Coding,
                "verifier" => JournalPurpose::Verifier,
                _ => return Err(E::Integrity),
            },
        })
    }
    let more = keys.len() > usize::from(limit);
    keys.truncate(usize::from(limit));
    let next = if more { keys.last().cloned() } else { None };
    Ok((keys, next))
}
pub(crate) async fn inspect_journal(
    c: &mut SqliteConnection,
    key: &crate::controller_keys::ControllerKey,
    project: &rolvya_domain::ids::ProjectId,
    after: Option<&JournalCursor>,
    limit: u16,
) -> Result<ReconciliationPage, E> {
    if !(1..=50).contains(&limit) {
        return Err(E::ResourceLimit);
    }
    let (keys, next) = page_keys(c, project, after, limit).await?;
    let restored = !crate::controller_keys::operations::external_authority_available(c).await?;
    let mut out = Vec::with_capacity(keys.len());
    for id in keys {
        let row=sqlx::query("SELECT record,record_mac,request_digest,purpose,attempt_id FROM aws_launch_requests WHERE project_id=? AND attempt_id=? AND purpose=?").bind(project.as_str()).bind(id.attempt.as_str()).bind(id.purpose.name()).fetch_one(&mut *c).await?;
        let text: &str = row.try_get("record")?;
        if text.len() > 32768
            || !super::super::super::equal(
                row.try_get::<&[u8], _>("record_mac")?,
                &super::super::consent::mac(key, b"launch-request", &[text.as_bytes()]),
            )
        {
            return Err(E::Integrity);
        }
        let r: LaunchRequest = serde_json::from_str(text).map_err(|_| E::Integrity)?;
        if r.installation != key.coding_installation()
            || r.binding.task.plan.project_id != *project
            || r.digest()? != row.try_get::<&str, _>("request_digest")?
            || r.purpose() != row.try_get::<&str, _>("purpose")?
            || r.binding.attempt_id.as_str() != row.try_get::<&str, _>("attempt_id")?
        {
            return Err(E::Integrity);
        }
        let event=sqlx::query("SELECT revision,kind,recorded_at,record,record_mac FROM aws_execution_events WHERE project_id=? AND attempt_id=? AND purpose=? ORDER BY revision DESC LIMIT 1").bind(project.as_str()).bind(r.binding.attempt_id.as_str()).bind(r.purpose()).fetch_one(&mut *c).await?;
        let status = super::store::parse_event(key, &r, &event)?;
        let execution = match status.lifecycle {
            EcsLifecycle::Running => ReconciledExecution::Running,
            EcsLifecycle::Stopping => ReconciledExecution::Stopping,
            EcsLifecycle::Stopped => ReconciledExecution::ConfirmedStopped,
            EcsLifecycle::Provisioning | EcsLifecycle::Pending if status.task.is_some() => {
                ReconciledExecution::ConfirmedLaunched
            }
            _ => ReconciledExecution::Unknown,
        };
        let liability = crate::finance::liability(
            c,
            project,
            &crate::finance::LiabilityId::new(r.binding.attempt_id.as_str())
                .map_err(|_| E::Integrity)?,
            r.maximum.ai_api.currency(),
        )
        .await
        .map_err(|_| E::Integrity)?;
        let finance = if liability.is_some_and(|l| {
            l.finalized
                && !l.uncertain
                && l.held.total().is_ok_and(|m| m.minor_units() == 0)
                && l.provisional.total().is_ok_and(|m| m.minor_units() == 0)
        }) {
            ReconciledFinance::Finalized
        } else {
            ReconciledFinance::Unresolved
        };
        out.push(ReconciliationStatus {
            attempt: r.binding.attempt_id.as_str().into(),
            purpose: r.purpose().into(),
            operation: r.client_token,
            task: status.task,
            execution,
            finance,
            observation_revision: status.revision,
            observed_at: status.observed_at,
            external_event_time: None,
            restore_hold: restored,
            relaunch_allowed: false,
        });
    }
    Ok(ReconciliationPage { items: out, next })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn keyset_inventory_exposes_operation_after_the_first_fifty_rows() {
        // Durable SQLite volume fixture exercises the actual key-selection query;
        // authenticated request/event decoding is tested by lifecycle fixtures.
        let dir = tempfile::tempdir().unwrap();
        let mut c = sqlx::SqliteConnection::connect(&format!(
            "sqlite:{}?mode=rwc",
            dir.path().join("inventory.sqlite").display()
        ))
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE aws_launch_requests(project_id TEXT,attempt_id TEXT,purpose TEXT)",
        )
        .execute(&mut c)
        .await
        .unwrap();
        for n in 0..51 {
            sqlx::query("INSERT INTO aws_launch_requests VALUES('project',?,'coding')")
                .bind(format!("attempt-{n:03}"))
                .execute(&mut c)
                .await
                .unwrap();
        }
        sqlx::query(
            "INSERT INTO aws_launch_requests VALUES('other-project','attempt-999','coding')",
        )
        .execute(&mut c)
        .await
        .unwrap();
        let project = rolvya_domain::ids::ProjectId::new("project").unwrap();
        let (first, next) = page_keys(&mut c, &project, None, 50).await.unwrap();
        assert_eq!(first.len(), 50);
        assert!(next.is_some());
        let (second, end) = page_keys(&mut c, &project, next.as_ref(), 50)
            .await
            .unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].attempt.as_str(), "attempt-050");
        assert!(end.is_none());
    }
}
