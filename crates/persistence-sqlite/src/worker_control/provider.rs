use super::*;
use std::future::Future;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationKey {
    pub project: ProjectId,
    pub attempt: AttemptId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderState {
    Absent,
    Rejected,
    Active,
    Finished,
    CancelAck,
    Unknown,
}
impl ProviderState {
    fn name(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Rejected => "rejected",
            Self::Active => "active",
            Self::Finished => "finished",
            Self::CancelAck => "cancel_ack",
            Self::Unknown => "unknown",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderObservation {
    pub key: OperationKey,
    pub command_digest: Digest,
    pub state: ProviderState,
    pub unknown_exposure: bool,
    pub evidence: EvidenceRef,
    pub revision: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchResult {
    Observed(ProviderObservation),
    Indeterminate,
}
/// Trusted provider boundary; lookups never confer worker or financial authority.
/// Absence after a launch marker is NOT proof that an in-flight request cannot act.
pub trait LaunchAdapter: Send + Sync {
    fn inspect(
        &self,
        command: &LaunchCommand,
    ) -> impl Future<Output = Result<ProviderObservation, WorkerError>> + Send;
    fn launch(
        &self,
        authorization: &LaunchAuthorization,
        clock: &impl Clock,
    ) -> impl Future<Output = Result<LaunchResult, WorkerError>> + Send;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntheticLaunchBehavior {
    Normal,
    FailBeforeEffect,
    LoseAcknowledgement,
    UnknownBeforeEffect,
}
/// SQLite-persisted simulation of a remote idempotency service. Its operations use
/// separate transactions, never the dispatcher's write transaction. No network I/O.
#[derive(Clone)]
pub struct SyntheticProvider {
    store: PlanStore,
    behavior: SyntheticLaunchBehavior,
}
impl SyntheticProvider {
    pub fn new(store: PlanStore) -> Self {
        Self {
            store,
            behavior: SyntheticLaunchBehavior::Normal,
        }
    }
    pub fn with_behavior(mut self, behavior: SyntheticLaunchBehavior) -> Self {
        self.behavior = behavior;
        self
    }
    /// Trusted test-driver observation, not a worker protocol command. Revision CAS
    /// makes reordered synthetic provider observations explicit instead of last-win.
    pub async fn observe(
        &self,
        key: &OperationKey,
        expected_revision: u64,
        state: ProviderState,
        unknown_exposure: bool,
        evidence: &EvidenceRef,
    ) -> Result<(), WorkerError> {
        if state == ProviderState::Absent {
            return Err(WorkerError::InvalidState);
        }
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let n=sqlx::query("UPDATE synthetic_operations SET state=?,unknown_exposure=?,revision=revision+1,evidence_id=? WHERE project_id=? AND attempt_id=? AND revision=?")
            .bind(state.name()).bind(unknown_exposure).bind(evidence.as_str()).bind(key.project.as_str()).bind(key.attempt.as_str()).bind(i64::try_from(expected_revision).map_err(|_|WorkerError::ResourceLimit)?).execute(&mut *tx).await?.rows_affected();
        if n != 1 {
            return Err(WorkerError::InvalidState);
        }
        tx.commit().await?;
        Ok(())
    }
}
async fn observation(
    c: &mut SqliteConnection,
    cmd: &LaunchCommand,
) -> Result<ProviderObservation, WorkerError> {
    let k = cmd.operation_key();
    let digest = cmd.digest()?;
    let row=sqlx::query("SELECT digest,state,unknown_exposure,evidence_id,revision FROM synthetic_operations WHERE project_id=? AND attempt_id=?").bind(k.project.as_str()).bind(k.attempt.as_str()).fetch_optional(c).await?;
    let (state, unknown_exposure, evidence, revision) = if let Some(row) = row {
        if row.get::<String, _>(0) != digest {
            return Err(WorkerError::ReplayConflict);
        }
        (
            serde_json::from_value(serde_json::Value::String(row.get(1)))
                .map_err(|_| StoreError::Integrity)?,
            row.get(2),
            EvidenceRef::new(row.get::<String, _>(3))?,
            row.get::<i64, _>(4) as u64,
        )
    } else {
        (
            ProviderState::Absent,
            false,
            EvidenceRef::new("synthetic-absent")?,
            0,
        )
    };
    Ok(ProviderObservation {
        key: k,
        command_digest: Digest::new(digest)?,
        state,
        unknown_exposure,
        evidence,
        revision,
    })
}
impl LaunchAdapter for SyntheticProvider {
    async fn inspect(&self, cmd: &LaunchCommand) -> Result<ProviderObservation, WorkerError> {
        let mut tx = self.store.pool.begin().await?;
        let result = observation(&mut tx, cmd).await?;
        tx.commit().await?;
        Ok(result)
    }
    async fn launch(
        &self,
        authorization: &LaunchAuthorization,
        clock: &impl Clock,
    ) -> Result<LaunchResult, WorkerError> {
        let cmd = authorization.command();
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let old = observation(&mut tx, cmd).await?;
        if old.state != ProviderState::Absent {
            tx.commit().await?;
            return Ok(LaunchResult::Observed(old));
        }
        let (_, state, _) = command(&mut tx, &cmd.lease()).await?;
        if state != DispatchState::Launching {
            return Err(WorkerError::InvalidState);
        }
        admission::live(&mut tx, &cmd.lease(), admission::at(clock)?).await?;
        financial_guard(&mut tx, cmd).await?;
        if self.behavior == SyntheticLaunchBehavior::UnknownBeforeEffect {
            tx.commit().await?;
            return Ok(LaunchResult::Indeterminate);
        }
        let rejected = self.behavior == SyntheticLaunchBehavior::FailBeforeEffect;
        let k = cmd.operation_key();
        affected(
            sqlx::query("INSERT INTO synthetic_operations VALUES(?,?,?,?,?,1,?)")
                .bind(k.project.as_str())
                .bind(k.attempt.as_str())
                .bind(cmd.digest()?)
                .bind(if rejected { "rejected" } else { "active" })
                .bind(false)
                .bind(if rejected {
                    "synthetic-rejected"
                } else {
                    "synthetic-launched"
                })
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        let result = observation(&mut tx, cmd).await?;
        admission::live(&mut tx, &cmd.lease(), admission::at(clock)?).await?;
        tx.commit().await?;
        if self.behavior == SyntheticLaunchBehavior::LoseAcknowledgement {
            Ok(LaunchResult::Indeterminate)
        } else {
            Ok(LaunchResult::Observed(result))
        }
    }
}
