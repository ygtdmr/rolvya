use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskPhase {
    Ready,
    Running,
    WaitingDependencies,
    WaitingChildren,
    AwaitingGroupValidation,
    ReceivedUnvalidated,
    Validated,
    ExecutionFailed,
    ReconciliationRequired,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskObservation {
    pub task: TaskRef,
    pub kind: TaskKind,
    pub phase: TaskPhase,
    pub lease: Option<LeaseRef>,
    pub manifest_sha256: Option<Digest>,
    pub rejection: Option<String>,
}
impl SyntheticCoordinator {
    /// Redacted keyset view derived from existing authority. No prompts, artifact
    /// bytes, credentials or worker logs are included; at most 100 rows per call.
    pub async fn tasks(
        &self,
        after: Option<&TaskId>,
        limit: u16,
    ) -> Result<Vec<TaskObservation>, OrchestrationError> {
        if !(1..=100).contains(&limit) {
            return Err(OrchestrationError::LimitExceeded);
        }
        let k = self.plan_key();
        let rows=sqlx::query("SELECT t.task_id,t.kind,a.attempt_id,a.epoch,s.state,json_extract(w.state,'$.phase'),v.manifest_sha256,EXISTS(SELECT 1 FROM task_dependencies d LEFT JOIN task_acceptances va ON va.project_id=d.project_id AND va.plan_id=d.plan_id AND va.version=d.version AND va.task_id=d.predecessor_id WHERE d.project_id=t.project_id AND d.plan_id=t.plan_id AND d.version=t.version AND d.successor_id=t.task_id AND va.task_id IS NULL),EXISTS(SELECT 1 FROM tasks ch LEFT JOIN task_acceptances ca ON ca.project_id=ch.project_id AND ca.plan_id=ch.plan_id AND ca.version=ch.version AND ca.task_id=ch.task_id WHERE ch.project_id=t.project_id AND ch.plan_id=t.plan_id AND ch.version=t.version AND ch.parent_task_id=t.task_id AND ca.task_id IS NULL),(SELECT e.rejection FROM synthetic_worker_events e WHERE e.project_id=a.project_id AND e.attempt_id=a.attempt_id AND e.delivery='rejected' ORDER BY e.ordinal LIMIT 1) FROM tasks t LEFT JOIN task_attempts a ON a.project_id=t.project_id AND a.plan_id=t.plan_id AND a.version=t.version AND a.task_id=t.task_id AND NOT EXISTS(SELECT 1 FROM task_attempts newer WHERE newer.project_id=a.project_id AND newer.plan_id=a.plan_id AND newer.version=a.version AND newer.task_id=a.task_id AND newer.epoch>a.epoch) LEFT JOIN attempt_states s ON s.project_id=a.project_id AND s.attempt_id=a.attempt_id LEFT JOIN worker_sessions w ON w.project_id=a.project_id AND w.attempt_id=a.attempt_id LEFT JOIN task_acceptances v ON v.project_id=t.project_id AND v.plan_id=t.plan_id AND v.version=t.version AND v.task_id=t.task_id WHERE t.project_id=? AND t.plan_id=? AND t.version=? AND t.task_id>? ORDER BY t.task_id LIMIT ?")
   .bind(k.project_id.as_str()).bind(k.plan_id.as_str()).bind(version(k)?).bind(after.map(TaskId::as_str).unwrap_or("")).bind(limit).fetch_all(&self.store.pool).await?;
        rows.into_iter()
            .map(|row| {
                let task = self.reference(
                    &TaskId::new(row.get::<String, _>(0)).map_err(|_| StoreError::Integrity)?,
                );
                let group = row.get::<String, _>(1) == "group";
                let kind = if group {
                    TaskKind::Group
                } else {
                    TaskKind::ExecutableLeaf
                };
                let lease = row
                    .get::<Option<String>, _>(2)
                    .map(|id| {
                        Ok::<_, OrchestrationError>(LeaseRef {
                            task: task.clone(),
                            attempt_id: AttemptId::new(id).map_err(|_| StoreError::Integrity)?,
                            epoch: row.get::<i64, _>(3) as u64,
                        })
                    })
                    .transpose()?;
                let status: Option<String> = row.get(4);
                let session: Option<String> = row.get(5);
                let manifest = row
                    .get::<Option<String>, _>(6)
                    .map(Digest::new)
                    .transpose()?;
                let phase = if manifest.is_some() {
                    TaskPhase::Validated
                } else if status.as_deref() == Some("fenced") {
                    TaskPhase::ReconciliationRequired
                } else if session.as_deref() == Some("Failed") {
                    TaskPhase::ExecutionFailed
                } else if session.as_deref() == Some("ResultReceived") {
                    TaskPhase::ReceivedUnvalidated
                } else if lease.is_some() {
                    TaskPhase::Running
                } else if row.get::<bool, _>(7) {
                    TaskPhase::WaitingDependencies
                } else if group && row.get::<bool, _>(8) {
                    TaskPhase::WaitingChildren
                } else if group {
                    TaskPhase::AwaitingGroupValidation
                } else {
                    TaskPhase::Ready
                };
                Ok(TaskObservation {
                    task,
                    kind,
                    phase,
                    lease,
                    manifest_sha256: manifest,
                    rejection: row.get(9),
                })
            })
            .collect()
    }
}
