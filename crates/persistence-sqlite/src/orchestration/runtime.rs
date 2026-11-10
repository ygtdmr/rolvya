//! RC-02 installed control-loop supervisor.
//!
//! The supervisor only *drives* the existing R06/R08/R09 kernels: every pass is a
//! [`SyntheticCoordinator::pass`], which re-reads all authority from SQLite, fences
//! expired leases, records terminal-failure reconciliation before any retry and
//! rechecks admission. The supervisor holds no scheduling queue, grants nothing,
//! and its heartbeat row is liveness evidence, never authority.
//!
//! Production composes it with [`ClosedRuntime`], which supplies no execution
//! authority, so the loop runs and reports liveness but admits nothing. Paid
//! constructors stay closed. Synthetic/offline authorities are supplied only by
//! tests and by a separately reviewed offline profile.
use super::*;
use crate::admission::{Clock, SystemClock};
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;

/// What the installed loop may execute. Recorded in the heartbeat and readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    /// No execution authority: the loop runs, admits nothing.
    Closed,
    /// Deterministic synthetic/offline authority; never a paid provider.
    Synthetic,
}
impl RuntimeMode {
    fn name(self) -> &'static str {
        match self {
            Self::Closed => "closed",
            Self::Synthetic => "synthetic",
        }
    }
}

/// Per-project execution authority source for the supervisor.
pub trait RuntimeAuthority: Send + Sync + 'static {
    type Authority: SyntheticExecutionAuthority + 'static;
    fn mode(&self) -> RuntimeMode;
    /// `None` keeps the project closed for this tick. Implementations must not
    /// perform I/O or derive authority from client-controlled data.
    fn for_project(&self, project: &ProjectId) -> Option<&Self::Authority>;
}

/// Uninhabited authority: proves at the type level that [`ClosedRuntime`] can
/// never hand a coordinator any admission, dispatch or worker permission.
pub enum NoAuthority {}
impl rolvya_plan_validator::AssignmentPolicy for NoAuthority {
    fn check(
        &self,
        _: &ExecutionAssignment,
    ) -> Result<(), rolvya_plan_validator::AssignmentDenial> {
        match *self {}
    }
}
impl SchedulerPolicy for NoAuthority {
    fn permits(&self, _: &PrincipalId, _: SchedulerAction<'_>, _: i64) -> bool {
        match *self {}
    }
}
impl WorkerAuthority for NoAuthority {
    fn permits(
        &self,
        _: &PeerEvidence,
        _: &WorkerBinding,
        _: WorkerOperation,
        _: Option<&DeclaredCapabilities>,
        _: i64,
    ) -> bool {
        match *self {}
    }
}
impl SyntheticExecutionAuthority for NoAuthority {
    fn actor(&self) -> PrincipalId {
        match *self {}
    }
    fn assignment(
        &self,
        _: &PlannedTask,
        _: u8,
        _: i64,
    ) -> Result<SyntheticAssignment, OrchestrationError> {
        match *self {}
    }
    fn peer(&self, _: &WorkerBinding) -> PeerEvidence {
        match *self {}
    }
}

/// The production default: no project ever receives execution authority.
pub struct ClosedRuntime;
impl RuntimeAuthority for ClosedRuntime {
    type Authority = NoAuthority;
    fn mode(&self) -> RuntimeMode {
        RuntimeMode::Closed
    }
    fn for_project(&self, _: &ProjectId) -> Option<&NoAuthority> {
        None
    }
}

/// How the loop waits between ticks. Production sleeps in real time; the
/// deterministic simulation clock advances instead, exactly like R09 `run`.
pub trait RuntimeClock: Clock + 'static {
    /// Time to sleep before the next tick, given the earliest durable wake time.
    fn pause(&self, next_wake: Option<i64>, idle: Duration) -> Duration;
}
impl RuntimeClock for SystemClock {
    fn pause(&self, next_wake: Option<i64>, idle: Duration) -> Duration {
        match (next_wake, self.now()) {
            (Some(wake), Ok(now)) if wake > now => {
                idle.min(Duration::from_secs((wake - now) as u64))
            }
            (Some(_), Ok(_)) => Duration::ZERO,
            _ => idle,
        }
    }
}
/// Simulation only: jumps to the next durable wake instead of sleeping.
impl RuntimeClock for DeterministicClock {
    fn pause(&self, next_wake: Option<i64>, idle: Duration) -> Duration {
        match next_wake {
            Some(wake) if self.advance_to(wake).is_ok() => Duration::ZERO,
            _ => idle,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeConfig {
    /// Idle wait when no progress was made and no earlier wake is known.
    pub idle: Duration,
    /// Minimum interval between heartbeat writes.
    pub heartbeat: Duration,
    /// Upper bound for the exponential failure backoff.
    pub max_backoff: Duration,
    /// Active projects examined per tick.
    pub projects: u16,
    pub pass: PassLimits,
}
impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            idle: Duration::from_secs(5),
            heartbeat: Duration::from_secs(15),
            max_backoff: Duration::from_secs(60),
            projects: 100,
            pass: PassLimits::default(),
        }
    }
}
impl RuntimeConfig {
    fn check(&self) -> Result<(), OrchestrationError> {
        if self.projects == 0
            || self.projects > 1000
            || self.idle.is_zero()
            || self.heartbeat.is_zero()
            || self.max_backoff < self.idle
        {
            return Err(OrchestrationError::LimitExceeded);
        }
        self.pass.check()
    }
}

/// Bounded summary of one tick; contains identifiers only, never secrets.
#[derive(Debug, Clone, Default)]
pub struct TickReport {
    pub active_projects: u16,
    pub executed: u16,
    pub closed: u16,
    pub progressed: bool,
    pub next_wake: Option<i64>,
    pub failures: Vec<(ProjectId, OrchestrationError)>,
    pub snapshots: Vec<(ProjectId, RunSnapshot)>,
}

/// Published after every tick for in-process observers (service, tests).
#[derive(Debug, Clone, Default)]
pub struct RuntimeStatus {
    pub ticks: u64,
    pub failed_ticks: u64,
    pub last: Option<TickReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeExit {
    /// Stop was requested and the heartbeat recorded `stopped`.
    Stopped,
    /// Heartbeat could not be recorded; the caller must treat liveness as unknown.
    HeartbeatUnavailable,
}

pub struct RuntimeSupervisor<R: RuntimeAuthority, C: RuntimeClock> {
    store: PlanStore,
    runtime: Arc<R>,
    clock: Arc<C>,
    config: RuntimeConfig,
    status: watch::Sender<RuntimeStatus>,
}
impl<R: RuntimeAuthority, C: RuntimeClock> RuntimeSupervisor<R, C> {
    pub fn new(
        store: PlanStore,
        runtime: R,
        clock: C,
        config: RuntimeConfig,
    ) -> Result<Self, OrchestrationError> {
        config.check()?;
        Ok(Self {
            store,
            runtime: Arc::new(runtime),
            clock: Arc::new(clock),
            config,
            status: watch::channel(RuntimeStatus::default()).0,
        })
    }
    pub fn mode(&self) -> RuntimeMode {
        self.runtime.mode()
    }
    pub fn subscribe(&self) -> watch::Receiver<RuntimeStatus> {
        self.status.subscribe()
    }

    /// One bounded pass over every active project. A failing project never
    /// stops the others; its error is reported and retried next tick.
    pub async fn tick(&self) -> Result<TickReport, OrchestrationError> {
        tick(
            &self.store,
            self.runtime.as_ref(),
            self.clock.as_ref(),
            &self.config,
        )
        .await
    }

    /// Runs until `stop` becomes true. Each tick runs in its own task so a
    /// panic is contained, counted and retried with bounded backoff; durable
    /// state is unaffected because every write is an R06/R08/R09 transaction.
    pub async fn run(self, mut stop: watch::Receiver<bool>) -> RuntimeExit {
        let mut instance = [0; 16];
        if getrandom::fill(&mut instance).is_err() {
            return RuntimeExit::HeartbeatUnavailable;
        }
        let instance: String = instance.iter().map(|b| format!("{b:02x}")).collect();
        let mode = self.runtime.mode();
        let Ok(started) = self.clock.now() else {
            return RuntimeExit::HeartbeatUnavailable;
        };
        if start_heartbeat(&self.store, &instance, mode, started)
            .await
            .is_err()
        {
            return RuntimeExit::HeartbeatUnavailable;
        }
        let (store, runtime, clock, config) = (
            self.store.clone(),
            self.runtime.clone(),
            self.clock.clone(),
            self.config,
        );
        let mut status = RuntimeStatus::default();
        let mut failures: u32 = 0;
        let mut beat_at = started;
        let mut dirty = false;
        loop {
            if *stop.borrow() {
                break;
            }
            let job = {
                let (store, runtime, clock) = (store.clone(), runtime.clone(), clock.clone());
                tokio::spawn(async move {
                    tick(&store, runtime.as_ref(), clock.as_ref(), &config).await
                })
            };
            let (report, failure) = match job.await {
                Ok(Ok(report)) => {
                    let failure = (!report.failures.is_empty()).then_some("pass");
                    (Some(report), failure)
                }
                Ok(Err(_)) => (None, Some("storage")),
                // Panic or cancellation of the contained tick task.
                Err(_) => (None, Some("panic")),
            };
            status.ticks += 1;
            if failure.is_some() {
                status.failed_ticks += 1;
            }
            let progressed = report.as_ref().is_some_and(|r| r.progressed);
            let pause = if failure.is_some() && !progressed {
                failures = failures.saturating_add(1);
                backoff(config.idle, config.max_backoff, failures)
            } else {
                failures = 0;
                match &report {
                    Some(r) if r.progressed => Duration::ZERO,
                    Some(r) => clock.pause(r.next_wake, config.idle),
                    None => config.idle,
                }
            };
            status.last = report;
            self.status.send_replace(status.clone());
            dirty = true;
            let now = clock.now().unwrap_or(beat_at);
            let due = now.saturating_sub(beat_at) >= config.heartbeat.as_secs() as i64;
            if (due || failure.is_some())
                && beat(&store, &instance, now, &status, failure).await.is_ok()
            {
                beat_at = now;
                dirty = false;
            }
            if pause.is_zero() {
                tokio::task::yield_now().await;
            } else {
                tokio::select! {
                    _ = stop.changed() => {}
                    _ = tokio::time::sleep(pause) => {}
                }
            }
        }
        let now = clock.now().unwrap_or(beat_at).max(beat_at);
        if dirty {
            let _ = beat(&store, &instance, now, &status, None).await;
        }
        match stop_heartbeat(&store, &instance, now).await {
            Ok(()) => RuntimeExit::Stopped,
            Err(_) => RuntimeExit::HeartbeatUnavailable,
        }
    }
}

fn backoff(idle: Duration, max: Duration, failures: u32) -> Duration {
    let factor = 1u32.checked_shl(failures.min(16)).unwrap_or(u32::MAX);
    idle.saturating_mul(factor).min(max)
}

async fn tick<R: RuntimeAuthority, C: RuntimeClock>(
    store: &PlanStore,
    runtime: &R,
    clock: &C,
    config: &RuntimeConfig,
) -> Result<TickReport, OrchestrationError> {
    let projects: Vec<String> = sqlx::query_scalar(
        "SELECT project_id FROM plan_versions WHERE state='active' ORDER BY project_id LIMIT ?",
    )
    .bind(config.projects)
    .fetch_all(&store.pool)
    .await?;
    let mut report = TickReport {
        active_projects: projects.len() as u16,
        ..TickReport::default()
    };
    for project in projects {
        let project = ProjectId::new(project).map_err(|_| StoreError::Integrity)?;
        let Some(authority) = runtime.for_project(&project) else {
            report.closed += 1;
            continue;
        };
        report.executed += 1;
        let pass = async {
            let coordinator =
                SyntheticCoordinator::open(store.clone(), &project, authority, Limits::default())
                    .await?;
            coordinator.pass(authority, clock, config.pass).await
        };
        match pass.await {
            Ok(pass) => {
                report.progressed |= pass.progress();
                report.next_wake = match (report.next_wake, pass.next_wake) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                report.snapshots.push((project, pass.snapshot));
            }
            Err(e) => report.failures.push((project, e)),
        }
    }
    Ok(report)
}

async fn start_heartbeat(
    store: &PlanStore,
    instance: &str,
    mode: RuntimeMode,
    now: i64,
) -> Result<(), StoreError> {
    sqlx::query("INSERT INTO controller_runtime_heartbeat(singleton,instance_id,mode,state,started_at,heartbeat_at,ticks,failed_ticks,last_failure) VALUES(1,?,?,'running',?,?,0,0,NULL) ON CONFLICT(singleton) DO UPDATE SET instance_id=excluded.instance_id,mode=excluded.mode,state='running',started_at=excluded.started_at,heartbeat_at=excluded.heartbeat_at,ticks=0,failed_ticks=0,last_failure=NULL")
        .bind(instance)
        .bind(mode.name())
        .bind(now)
        .bind(now)
        .execute(&store.pool)
        .await?;
    Ok(())
}
async fn beat(
    store: &PlanStore,
    instance: &str,
    now: i64,
    status: &RuntimeStatus,
    failure: Option<&str>,
) -> Result<(), StoreError> {
    let n = sqlx::query("UPDATE controller_runtime_heartbeat SET heartbeat_at=max(heartbeat_at,?),ticks=?,failed_ticks=?,last_failure=coalesce(?,last_failure) WHERE singleton=1 AND instance_id=? AND state='running'")
        .bind(now)
        .bind(i64::try_from(status.ticks).unwrap_or(i64::MAX))
        .bind(i64::try_from(status.failed_ticks).unwrap_or(i64::MAX))
        .bind(failure)
        .bind(instance)
        .execute(&store.pool)
        .await?
        .rows_affected();
    // Another instance took over; this one must not claim liveness.
    if n == 1 {
        Ok(())
    } else {
        Err(StoreError::Conflict)
    }
}
async fn stop_heartbeat(store: &PlanStore, instance: &str, now: i64) -> Result<(), StoreError> {
    let n = sqlx::query("UPDATE controller_runtime_heartbeat SET state='stopped',heartbeat_at=max(heartbeat_at,?) WHERE singleton=1 AND instance_id=? AND state='running'")
        .bind(now)
        .bind(instance)
        .execute(&store.pool)
        .await?
        .rows_affected();
    if n == 1 {
        Ok(())
    } else {
        Err(StoreError::Conflict)
    }
}

/// Liveness as read by a separate process (CLI `readiness`). A crashed process
/// leaves `running` with a stale heartbeat, which reads as not running.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeLiveness {
    pub control_loop_running: bool,
    pub mode: Option<String>,
    pub heartbeat_age_seconds: Option<i64>,
    pub failed_ticks: i64,
}
pub const LIVENESS_WINDOW_SECONDS: i64 = 60;
impl PlanStore {
    pub async fn runtime_liveness(&self, now: i64) -> Result<RuntimeLiveness, StoreError> {
        let row = sqlx::query(
            "SELECT mode,state,heartbeat_at,failed_ticks FROM controller_runtime_heartbeat WHERE singleton=1",
        )
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(RuntimeLiveness {
                control_loop_running: false,
                mode: None,
                heartbeat_age_seconds: None,
                failed_ticks: 0,
            });
        };
        let age = now.saturating_sub(row.try_get::<i64, _>("heartbeat_at")?);
        Ok(RuntimeLiveness {
            control_loop_running: row.try_get::<String, _>("state")? == "running"
                && (0..=LIVENESS_WINDOW_SECONDS).contains(&age),
            mode: Some(row.try_get("mode")?),
            heartbeat_age_seconds: Some(age),
            failed_ticks: row.try_get("failed_ticks")?,
        })
    }
}
