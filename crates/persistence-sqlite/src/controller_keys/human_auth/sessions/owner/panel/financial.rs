//! Display the existing ledger's latest liability positions, never a second sum.
use super::*;
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelFinance {
    pub currency: String,
    pub version: u64,
    pub ceiling: String,
    pub reserved: String,
    pub settled: String,
    pub provisional: String,
    pub available: Option<String>,
    pub unknown_liabilities: u64,
    pub over_ceiling: bool,
    pub paused: bool,
    pub recovery_required: bool,
    pub forecasts: Option<()>,
    pub categories: Vec<PanelCategory>,
    pub approvals: Vec<PanelApproval>,
}
#[derive(Debug, Serialize)]
pub struct PanelCategory {
    pub id: &'static str,
    pub settled: String,
    pub reserved: String,
    pub provisional: String,
    pub forecast: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct PanelApproval {
    pub id: String,
    pub actor: String,
    pub previous: String,
    pub total: String,
    pub version: i64,
    pub at: String,
}
impl HumanAuth {
    pub(super) async fn panel_finance_in(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        p: &ProjectId,
    ) -> Result<Option<PanelFinance>, AuthError> {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM budget_approvals WHERE project_id=?)")
                .bind(p.as_str())
                .fetch_one(&mut **tx)
                .await?;
        if !exists {
            return Ok(None);
        }
        let f = crate::finance::snapshot(tx, p)
            .await
            .map_err(|_| AuthError::Unavailable)?;
        let recovery_required =
            !crate::controller_keys::operations::external_authority_available(tx).await?;
        let settled = f.settled.total().map_err(|_| AuthError::Unavailable)?;
        let reserved = f.held.total().map_err(|_| AuthError::Unavailable)?;
        let provisional = f.provisional.total().map_err(|_| AuthError::Unavailable)?;
        let parts =
            |v: rolvya_domain::money::CostBreakdown| [v.ai_api, v.compute, v.storage, v.transfer];
        let a = parts(f.settled);
        let b = parts(f.held);
        let c = parts(f.provisional);
        let categories = ["ai", "compute", "storage", "transfer"]
            .iter()
            .enumerate()
            .map(|(i, id)| PanelCategory {
                id,
                settled: a[i].minor_units().to_string(),
                reserved: b[i].minor_units().to_string(),
                provisional: c[i].minor_units().to_string(),
                forecast: None,
            })
            .collect();
        Ok(Some(PanelFinance {
            currency: f.ceiling.currency().to_string(),
            version: f.version,
            ceiling: f.ceiling.minor_units().to_string(),
            reserved: reserved.minor_units().to_string(),
            settled: settled.minor_units().to_string(),
            provisional: provisional.minor_units().to_string(),
            available: if f.uncertain_liabilities == 0 && !recovery_required {
                Some(f.headroom.minor_units().to_string())
            } else {
                None
            },
            unknown_liabilities: f.uncertain_liabilities,
            over_ceiling: f.over_ceiling,
            paused: recovery_required
                || f.over_ceiling
                || f.uncertain_liabilities > 0
                || f.headroom.minor_units() == 0,
            recovery_required,
            forecasts: None,
            categories,
            approvals: vec![],
        }))
    }
}

impl HumanAuth {
    pub(super) async fn panel_approvals_in(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        s: &Scope,
        after: i64,
        limit: u16,
    ) -> Result<PanelPage<PanelApproval>, AuthError> {
        if s.membership.role != ProjectRole::Owner {
            return Err(AuthError::Denied);
        }
        let rows = sqlx::query("SELECT b.event_sequence,b.version,b.previous_ceiling,b.new_ceiling,e.actor,strftime('%Y-%m-%dT%H:%M:%SZ',e.recorded_at,'unixepoch') AS at FROM budget_approvals b JOIN financial_events e ON e.sequence=b.event_sequence AND e.project_id=b.project_id WHERE b.project_id=? AND b.version<? ORDER BY b.version DESC LIMIT ?")
            .bind(s.project.as_str()).bind(after).bind(i64::from(limit)+1).fetch_all(&mut **tx).await?;
        let mut items = Vec::with_capacity(rows.len());
        for r in rows {
            items.push((
                r.try_get("version")?,
                PanelApproval {
                    id: format!("approval-{}", r.try_get::<i64, _>("event_sequence")?),
                    actor: r.try_get("actor")?,
                    previous: r.try_get::<i64, _>("previous_ceiling")?.to_string(),
                    total: r.try_get::<i64, _>("new_ceiling")?.to_string(),
                    version: r.try_get("version")?,
                    at: r.try_get("at")?,
                },
            ));
        }
        self.panel_page(&s.project, s.user()?, "approvals", items, limit)
    }
}
