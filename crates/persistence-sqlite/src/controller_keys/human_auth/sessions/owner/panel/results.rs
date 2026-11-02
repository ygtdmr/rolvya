use super::*;
use rolvya_domain::ids::{AttemptId, PlanId, PlanVersion, TaskId};
impl HumanAuth {
    pub(super) async fn panel_results_in(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        s: &Scope,
        after: i64,
        limit: u16,
    ) -> Result<PanelPage<PanelResult>, AuthError> {
        let p = &s.project;
        let rows=sqlx::query("SELECT a.attempt_id,a.task_id,a.epoch,a.plan_id,a.version,a.admission_event,p.fingerprint,strftime('%Y-%m-%dT%H:%M:%SZ',coalesce((SELECT max(m.recorded_at) FROM worker_messages m WHERE m.project_id=a.project_id AND m.attempt_id=a.attempt_id),a.admitted_at),'unixepoch') AS at FROM task_attempts a JOIN plan_versions p USING(project_id,plan_id,version) WHERE a.project_id=? AND p.state='active' AND a.admission_event<? AND (EXISTS(SELECT 1 FROM coding_artifacts c WHERE c.project_id=a.project_id AND c.attempt_id=a.attempt_id) OR EXISTS(SELECT 1 FROM synthetic_artifacts c WHERE c.project_id=a.project_id AND c.attempt_id=a.attempt_id)) ORDER BY a.admission_event DESC LIMIT ?")
            .bind(p.as_str()).bind(after).bind(i64::from(limit)+1).fetch_all(&mut **tx).await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let id: String = r.try_get("attempt_id")?;
            let task: String = r.try_get("task_id")?;
            let fingerprint: String = r.try_get("fingerprint")?;
            let l = crate::admission::LeaseRef {
                task: rolvya_domain::plan::TaskRef {
                    plan: rolvya_domain::plan::PlanKey {
                        project_id: p.clone(),
                        plan_id: PlanId::new(r.try_get::<String, _>("plan_id")?)
                            .map_err(|_| AuthError::Storage)?,
                        version: PlanVersion::new(r.try_get::<i64, _>("version")? as u64)
                            .map_err(|_| AuthError::Storage)?,
                    },
                    task_id: TaskId::new(&task).map_err(|_| AuthError::Storage)?,
                },
                attempt_id: AttemptId::new(&id).map_err(|_| AuthError::Storage)?,
                epoch: r.try_get::<i64, _>("epoch")? as u64,
            };
            let evidence = crate::orchestration::panel_artifact_in(tx, &self.key, &l, &fingerprint)
                .await
                .map_err(|_| AuthError::Unavailable)?
                .ok_or(AuthError::Unavailable)?;
            let accepted = evidence.acceptance.is_some();
            let publication = if let Some(acceptance) = &evidence.acceptance {
                crate::controller_keys::github_app::panel_receipt(
                    tx,
                    &self.key,
                    &l,
                    &fingerprint,
                    acceptance,
                )
                .await
                .map_err(|_| AuthError::Unavailable)?
            } else {
                None
            };
            let (reference, published, publication_state) = match publication {
                Some((reference, true, state)) => (Some(reference), true, state),
                Some((_, false, state)) => (None, false, state),
                None => (None, false, "not_prepared"),
            };
            out.push((
                r.try_get("admission_event")?,
                PanelResult {
                    id: format!(
                        "result-{}",
                        hex(
                            &self.mac(b"panel-result", &[p.as_str().as_bytes(), id.as_bytes()])
                                [..16]
                        )
                    ),
                    task,
                    attempt: id,
                    accepted,
                    verification: evidence.verification,
                    tests: evidence.tests,
                    sha256: evidence.digest,
                    reference,
                    published,
                    at: r.try_get("at")?,
                    fingerprint,
                    epoch: r.try_get("epoch")?,
                    publication_state,
                    acceptance: evidence.acceptance,
                },
            ));
        }
        self.panel_page(p, s.user()?, "results", out, limit)
    }
}
