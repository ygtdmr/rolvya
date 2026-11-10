//! RC-02: the installed control-loop supervisor drives the existing R06/R08/R09
//! kernels without adding authority, duplicating effects or skipping recovery.
use super::*;
use crate::orchestration::runtime::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::sync::watch;

/// Test-only synthetic runtime: one project, existing deterministic authority.
struct Synthetic {
    project: ProjectId,
    authority: Authority,
    panic_once: AtomicBool,
}
impl Synthetic {
    fn new(authority: Authority) -> Self {
        Self {
            project: ProjectId::new("project").unwrap(),
            authority,
            panic_once: AtomicBool::new(false),
        }
    }
}
impl RuntimeAuthority for Synthetic {
    type Authority = Authority;
    fn mode(&self) -> RuntimeMode {
        RuntimeMode::Synthetic
    }
    fn for_project(&self, project: &ProjectId) -> Option<&Authority> {
        if self.panic_once.swap(false, Ordering::SeqCst) {
            panic!("injected supervisor tick panic");
        }
        (*project == self.project).then_some(&self.authority)
    }
}
fn config() -> RuntimeConfig {
    RuntimeConfig {
        idle: Duration::from_millis(5),
        heartbeat: Duration::from_secs(1),
        max_backoff: Duration::from_millis(40),
        ..RuntimeConfig::default()
    }
}
fn complete(s: &RuntimeStatus) -> bool {
    s.last.as_ref().is_some_and(|r| {
        r.snapshots
            .iter()
            .any(|(_, n)| n.total > 0 && n.accepted == n.total)
    })
}
/// Runs a supervisor until `done`, then requests a graceful stop.
async fn drive<R: RuntimeAuthority, C: RuntimeClock>(
    supervisor: RuntimeSupervisor<R, C>,
    done: impl Fn(&RuntimeStatus) -> bool,
) -> (RuntimeStatus, RuntimeExit) {
    let mut status = supervisor.subscribe();
    let (stop, stopped) = watch::channel(false);
    let task = tokio::spawn(supervisor.run(stopped));
    let mut watched = status.clone();
    let reached = match tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            watched.changed().await.expect("supervisor exited early");
            let s = watched.borrow().clone();
            if done(&s) {
                break s;
            }
        }
    })
    .await
    {
        Ok(s) => s,
        Err(_) => panic!(
            "supervisor condition not reached; last status: {:?}",
            status.borrow_and_update().clone()
        ),
    };
    stop.send(true).unwrap();
    let exit = tokio::time::timeout(Duration::from_secs(15), task)
        .await
        .expect("supervisor did not stop gracefully")
        .unwrap();
    (reached, exit)
}
async fn scalar(store: &PlanStore, sql: &str) -> i64 {
    sqlx::query_scalar(sql)
        .fetch_one(&store.pool)
        .await
        .unwrap()
}
/// Exactly-once dispatch evidence: one outbox message and one dispatch record per
/// attempt, and at most one non-rejected launch effect per attempt.
async fn assert_exactly_once(store: &PlanStore) {
    let attempts = scalar(store, "SELECT count(*) FROM task_attempts").await;
    assert_eq!(
        scalar(store, "SELECT count(*) FROM dispatch_outbox").await,
        attempts,
        "outbox message lost or duplicated"
    );
    assert_eq!(
        scalar(store, "SELECT count(*) FROM task_attempts a WHERE NOT EXISTS(SELECT 1 FROM dispatch_operations d WHERE d.project_id=a.project_id AND d.attempt_id=a.attempt_id)").await,
        0,
        "admitted attempt never dispatched"
    );
    assert_eq!(
        scalar(store, "SELECT count(*) FROM (SELECT attempt_id FROM synthetic_operations WHERE state<>'rejected' GROUP BY project_id,attempt_id HAVING count(*)>1)").await,
        0,
        "duplicate launch effect"
    );
}

#[tokio::test]
async fn supervisor_drives_activated_plan_to_completion_and_records_liveness() {
    let f = fixture(100).await;
    let supervisor = RuntimeSupervisor::new(
        f.store.clone(),
        Synthetic::new(Authority::default()),
        DeterministicClock::new(T).unwrap(),
        config(),
    )
    .unwrap();
    assert_eq!(supervisor.mode(), RuntimeMode::Synthetic);
    let (status, exit) = drive(supervisor, complete).await;
    assert_eq!(exit, RuntimeExit::Stopped);
    assert_eq!(status.failed_ticks, 0);
    assert_eq!(count(&f, "task_acceptances").await, 4);
    assert_eq!(count(&f, "synthetic_operations").await, 3);
    assert_eq!(count(&f, "task_attempts").await, 3);
    assert_exactly_once(&f.store).await;
    let live = f.store.runtime_liveness(T).await.unwrap();
    assert!(!live.control_loop_running, "stopped loop reported running");
    assert_eq!(live.mode.as_deref(), Some("synthetic"));
    assert!(
        scalar(&f.store, "SELECT ticks FROM controller_runtime_heartbeat").await
            >= status.ticks as i64 - 1
    );
}

#[tokio::test]
async fn closed_runtime_reports_liveness_but_never_admits() {
    let f = fixture(100).await;
    let supervisor = RuntimeSupervisor::new(
        f.store.clone(),
        ClosedRuntime,
        DeterministicClock::new(T).unwrap(),
        config(),
    )
    .unwrap();
    let mut status = supervisor.subscribe();
    let (stop, stopped) = watch::channel(false);
    let task = tokio::spawn(supervisor.run(stopped));
    tokio::time::timeout(Duration::from_secs(10), async {
        while status.borrow().ticks < 5 {
            status.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    let last = status.borrow().last.clone().unwrap();
    assert_eq!(
        (last.active_projects, last.closed, last.executed),
        (1, 1, 0)
    );
    let live = f.store.runtime_liveness(T).await.unwrap();
    assert!(live.control_loop_running);
    assert_eq!(live.mode.as_deref(), Some("closed"));
    // A crashed process leaves `running`; a stale heartbeat must not read as live.
    assert!(
        !f.store
            .runtime_liveness(T + LIVENESS_WINDOW_SECONDS + 1)
            .await
            .unwrap()
            .control_loop_running
    );
    stop.send(true).unwrap();
    assert_eq!(task.await.unwrap(), RuntimeExit::Stopped);
    assert!(
        !f.store
            .runtime_liveness(T)
            .await
            .unwrap()
            .control_loop_running
    );
    for table in [
        "task_attempts",
        "dispatch_outbox",
        "synthetic_operations",
        "scheduler_events",
    ] {
        assert_eq!(count(&f, table).await, 0, "closed runtime wrote {table}");
    }
}

#[tokio::test]
async fn panicking_tick_is_contained_counted_and_retried() {
    let f = fixture(100).await;
    let runtime = Synthetic::new(Authority::default());
    runtime.panic_once.store(true, Ordering::SeqCst);
    let supervisor = RuntimeSupervisor::new(
        f.store.clone(),
        runtime,
        DeterministicClock::new(T).unwrap(),
        config(),
    )
    .unwrap();
    let (status, exit) = drive(supervisor, complete).await;
    assert_eq!(exit, RuntimeExit::Stopped);
    assert_eq!(status.failed_ticks, 1);
    assert_eq!(count(&f, "task_acceptances").await, 4);
    assert_exactly_once(&f.store).await;
    let row = sqlx::query("SELECT failed_ticks,last_failure FROM controller_runtime_heartbeat")
        .fetch_one(&f.store.pool)
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>(0), 1);
    assert_eq!(row.get::<String, _>(1), "panic");
}

#[tokio::test]
async fn invalid_runtime_bounds_are_rejected() {
    let f = fixture(100).await;
    for bad in [
        RuntimeConfig {
            projects: 0,
            ..config()
        },
        RuntimeConfig {
            idle: Duration::ZERO,
            ..config()
        },
        RuntimeConfig {
            max_backoff: Duration::from_millis(1),
            ..config()
        },
        RuntimeConfig {
            pass: PassLimits {
                admissions: 101,
                ..PassLimits::default()
            },
            ..config()
        },
    ] {
        assert!(matches!(
            RuntimeSupervisor::new(
                f.store.clone(),
                ClosedRuntime,
                DeterministicClock::new(T).unwrap(),
                bad
            ),
            Err(OrchestrationError::LimitExceeded)
        ));
    }
}

#[tokio::test]
async fn transient_retry_waits_for_recorded_reconciliation_across_restart() {
    let f = fixture(100).await;
    let mut failing = Authority::default();
    failing.behaviors.insert(
        TaskId::new("task-b").unwrap(),
        FakeBehavior::TransientFailure,
    );
    let first = RuntimeSupervisor::new(
        f.store.clone(),
        Synthetic::new(failing),
        DeterministicClock::new(T).unwrap(),
        config(),
    )
    .unwrap();
    // Many idle ticks after the failure: the loop must not start a retry while
    // termination and billing remain unreconciled.
    let (_, exit) = drive(first, |s| {
        s.ticks >= 200
            && s.last
                .as_ref()
                .is_some_and(|r| r.snapshots.iter().any(|(_, n)| n.accepted == 1))
    })
    .await;
    assert_eq!(exit, RuntimeExit::Stopped);
    let old_b = lease(&f, "task-b").await;
    assert_eq!(count(&f, "task_attempts").await, 2, "retry started early");
    // Terminal-failure reconciliation is durably recorded (fenced, next epoch
    // reserved) before any retry exists.
    let recovery = sqlx::query(
        "SELECT reason,next_epoch FROM worker_recoveries WHERE project_id=? AND attempt_id=?",
    )
    .bind(old_b.task.plan.project_id.as_str())
    .bind(old_b.attempt_id.as_str())
    .fetch_optional(&f.store.pool)
    .await
    .unwrap()
    .unwrap_or_else(|| panic!("terminal failure was not recorded before retry: {old_b:?}"));
    assert_eq!(recovery.get::<String, _>(0), "fenced");
    let next_epoch = recovery.get::<i64, _>(1) as u64;
    // Independent trusted termination and zero-charge reconciliation, as in R09.
    SyntheticProvider::new(f.store.clone())
        .observe(
            &OperationKey {
                project: old_b.task.plan.project_id.clone(),
                attempt: old_b.attempt_id.clone(),
            },
            1,
            ProviderState::Finished,
            false,
            &e("synthetic-provider-terminated-b"),
        )
        .await
        .unwrap();
    synthetic_final(&f, &old_b).await;
    // A restarted controller resumes from SQLite only.
    let resumed_at: i64 = scalar(&f.store, "SELECT max(recorded_at) FROM scheduler_events").await;
    let second = RuntimeSupervisor::new(
        f.store.clone(),
        Synthetic::new(Authority::default()),
        DeterministicClock::new(resumed_at).unwrap(),
        config(),
    )
    .unwrap();
    let (_, exit) = drive(second, complete).await;
    assert_eq!(exit, RuntimeExit::Stopped);
    let new_b = lease(&f, "task-b").await;
    assert_ne!(new_b.attempt_id, old_b.attempt_id);
    assert!(
        new_b.epoch >= next_epoch && next_epoch > old_b.epoch,
        "retry did not respect the fence reserved by the recorded reconciliation"
    );
    assert_eq!(count(&f, "task_attempts").await, 4);
    assert_eq!(count(&f, "task_acceptances").await, 4);
    assert_exactly_once(&f.store).await;
}

/// Kill points for the full supervisor loop: (label, table, nth write).
/// `nth=2` on `task_attempts` is the boundary *after* the first admission commit.
const KILLS: [(FaultPoint, &str, usize); 7] = [
    (FaultPoint::AttemptCreation, "task_attempts", 1),
    (FaultPoint::AfterCommit, "task_attempts", 2),
    (FaultPoint::OutboxInsertion, "dispatch_outbox", 1),
    (FaultPoint::LaunchEffect, "synthetic_operations", 1),
    (FaultPoint::MessageWrite, "worker_messages", 3),
    (FaultPoint::ValidationCommit, "task_acceptances", 1),
    (FaultPoint::GroupCommit, "task_acceptances", 3),
];
async fn arm_nth(store: &PlanStore, point: FaultPoint) {
    let (_, table, nth) = KILLS.iter().find(|(p, _, _)| *p == point).copied().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    for mut c in connections(store).await {
        let hits = hits.clone();
        c.lock_handle()
            .await
            .unwrap()
            .set_update_hook(move |event| {
                if event.database == "main"
                    && event.table == table
                    && hits.fetch_add(1, Ordering::SeqCst) + 1 == nth
                {
                    marker(point);
                }
            });
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sigkill_supervisor_at_commit_boundaries_resumes_without_duplicate_effects() {
    if let Some((path, point)) = child_case() {
        let store = PlanStore::open(path).await.unwrap();
        arm_nth(&store, point).await;
        let supervisor = RuntimeSupervisor::new(
            store,
            Synthetic::new(Authority::default()),
            DeterministicClock::new(T).unwrap(),
            config(),
        )
        .unwrap();
        drive(supervisor, complete).await;
        panic!("supervisor kill boundary not reached: {point:?}");
    }
    for (point, _, _) in KILLS {
        println!("RC-02 supervisor kill boundary: {point:?}");
        let f = fixture(100).await;
        let before = AuthorityImage::read(&f.store, &f.content.key).await;
        let mut victim = Victim::start(
            &f,
            point,
            "resilience::runtime::sigkill_supervisor_at_commit_boundaries_resumes_without_duplicate_effects",
        );
        victim.ready();
        victim.kill();
        f.store.close().await;
        let restored = reopen(&f.path).await;
        AuthorityImage::read(&restored, &f.content.key)
            .await
            .preserves(&before);
        // The killed process left `running`; once stale it must not read as live.
        let beat: i64 = sqlx::query_scalar("SELECT heartbeat_at FROM controller_runtime_heartbeat")
            .fetch_one(&restored.pool)
            .await
            .unwrap();
        assert!(
            restored
                .runtime_liveness(beat)
                .await
                .unwrap()
                .control_loop_running
        );
        let stale = restored
            .runtime_liveness(beat + LIVENESS_WINDOW_SECONDS + 1)
            .await
            .unwrap();
        assert!(!stale.control_loop_running, "{point:?}: crashed loop live");
        // Resume no earlier than any durable time the killed process recorded.
        let resumed_at: i64 = sqlx::query_scalar(
            "SELECT max(?,coalesce((SELECT max(recorded_at) FROM scheduler_events),0))",
        )
        .bind(beat)
        .fetch_one(&restored.pool)
        .await
        .unwrap();
        assert_eq!(stale.mode.as_deref(), Some("synthetic"));
        let supervisor = RuntimeSupervisor::new(
            restored.clone(),
            Synthetic::new(Authority::default()),
            DeterministicClock::new(resumed_at).unwrap(),
            config(),
        )
        .unwrap();
        if point == FaultPoint::LaunchEffect {
            // Crash inside the launch transaction: the claim committed `launching`
            // but the effect did not. R08 cannot prove the external launch never
            // happened, so the loop must hold the dispatch for trusted
            // reconciliation and never re-launch, while recording no failure.
            let (status, exit) = drive(supervisor, |s| s.ticks >= 300).await;
            assert_eq!(exit, RuntimeExit::Stopped);
            assert_eq!(status.failed_ticks, 0);
            let snapshot = &status.last.unwrap().snapshots[0].1;
            assert_eq!(snapshot.recovery_required, 1, "launch gap not held");
            assert!(snapshot.accepted < snapshot.total);
            let image = AuthorityImage::read(&restored, &f.content.key).await;
            image.preserves(&before);
            assert!(
                image.effects < image.attempts,
                "uncertain launch re-executed"
            );
            assert_exactly_once(&restored).await;
            restored.close().await;
            continue;
        }
        let (status, exit) = drive(supervisor, complete).await;
        assert_eq!(exit, RuntimeExit::Stopped, "{point:?}");
        assert_eq!(status.failed_ticks, 0, "{point:?}");
        let image = AuthorityImage::read(&restored, &f.content.key).await;
        image.preserves(&before);
        assert_eq!(image.accepted.len(), 4, "{point:?}");
        assert_eq!(image.effects, 3, "{point:?}: duplicate launch effect");
        assert_eq!(image.attempts, 3, "{point:?}: duplicate admission");
        assert_exactly_once(&restored).await;
        restored.check_integrity().await.unwrap();
        restored.close().await;
    }
}
