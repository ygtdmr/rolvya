//! Safe, connection-local SQLx hooks, compiled only by the library's cfg(test).
use super::common::owner;
use super::*;
use std::{
    io::{BufRead, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultPoint {
    BeforeAcquire,
    EligibilityRead,
    WriteLock,
    AfterHeadroom,
    AfterReservation,
    AttemptCreation,
    FenceAdvance,
    AuditInsertion,
    OutboxInsertion,
    BeforeCommit,
    AfterCommit,
    DispatchClaim,
    LaunchEffect,
    MessageWrite,
    ResultJournal,
    ValidationCommit,
    GroupCommit,
    GroupAfterCommit,
    Snapshot,
    Checkpoint,
}
impl FaultPoint {
    pub fn name(self) -> String {
        serde_json::to_value(self).unwrap().as_str().unwrap().into()
    }
    pub fn table(self) -> Option<&'static str> {
        match self {
            Self::AfterHeadroom => Some("financial_events"),
            Self::AfterReservation => Some("financial_positions"),
            Self::AttemptCreation => Some("task_attempts"),
            Self::FenceAdvance => Some("task_fences"),
            Self::AuditInsertion => Some("scheduler_events"),
            Self::OutboxInsertion => Some("dispatch_outbox"),
            Self::MessageWrite => Some("worker_messages"),
            _ => None,
        }
    }
}
pub fn marker(point: FaultPoint) -> ! {
    let mut out = std::io::stdout().lock();
    writeln!(out, "R10_READY:{}", point.name()).unwrap();
    out.flush().unwrap();
    drop(out);
    // Deliberate crash victim wait, never compiled into a production control path.
    loop {
        std::thread::park();
    }
}
pub fn rendezvous() {
    let mut out = std::io::stdout().lock();
    writeln!(out, "R10_READY:eligibility_read").unwrap();
    out.flush().unwrap();
    drop(out);
    use std::io::Read;
    let mut b = [0];
    std::io::stdin().read_exact(&mut b).unwrap();
}
pub struct Victim {
    child: Child,
    point: FaultPoint,
}
impl Victim {
    pub fn start(f: &Fixture, point: FaultPoint, test: &str) -> Self {
        std::fs::write(
            f.directory.path().join("r10-disposable"),
            b"rolvya-r10-disposable\n",
        )
        .unwrap();
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture"])
            .env("ROLVYA_R10_POINT", point.name())
            .env("ROLVYA_R10_DB", &f.path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        Self { child, point }
    }
    pub fn ready(&mut self) {
        let out = self.child.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut notified = false;
            for line in std::io::BufReader::new(out).lines() {
                let Ok(line) = line else {
                    break;
                };
                if !notified && line.starts_with("R10_READY:") {
                    tx.send(line).ok();
                    notified = true;
                }
                // Keep draining until EOF so a resumed test harness can report
                // completion without a broken stdout pipe.
            }
        });
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(20))
                .expect("child failed to reach its typed fault boundary"),
            format!("R10_READY:{}", self.point.name())
        );
    }
    pub fn kill(mut self) {
        self.child.kill().unwrap();
        let status = self.child.wait().unwrap();
        assert!(!status.success());
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(status.signal(), Some(9));
        }
    }
    pub fn resume(mut self) {
        self.child.stdin.as_mut().unwrap().write_all(b"g").unwrap();
        assert!(self.child.wait().unwrap().success());
    }
}
impl Drop for Victim {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
pub fn child_case() -> Option<(PathBuf, FaultPoint)> {
    let p = std::env::var("ROLVYA_R10_POINT").ok()?;
    let point = serde_json::from_value(serde_json::Value::String(p)).unwrap();
    let path = PathBuf::from(std::env::var("ROLVYA_R10_DB").unwrap());
    let parent = path.parent().unwrap().canonicalize().unwrap();
    assert!(parent.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    assert_eq!(path.file_name().unwrap(), "plans.sqlite");
    assert_eq!(
        std::fs::read(parent.join("r10-disposable")).unwrap(),
        b"rolvya-r10-disposable\n"
    );
    assert!(
        !std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    Some((path, point))
}
pub async fn connections(store: &PlanStore) -> Vec<sqlx::pool::PoolConnection<Sqlite>> {
    let mut held = Vec::new();
    for _ in 0..4 {
        held.push(store.pool.acquire().await.unwrap());
    }
    held
}
pub async fn arm(store: &PlanStore, point: FaultPoint) {
    let hits = Arc::new(AtomicUsize::new(0));
    let armed = Arc::new(AtomicBool::new(false));
    for mut c in connections(store).await {
        let hits = hits.clone();
        let signal = armed.clone();
        c.lock_handle()
            .await
            .unwrap()
            .set_update_hook(move |event| {
                if event.database != "main" {
                    return;
                }
                if point.table() == Some(event.table) {
                    if point == FaultPoint::FenceAdvance
                        && !matches!(event.operation, sqlx::sqlite::SqliteOperation::Update)
                    {
                        return;
                    }
                    let hit = hits.fetch_add(1, Ordering::SeqCst) + 1;
                    if point != FaultPoint::AfterReservation || hit == 4 {
                        marker(point);
                    }
                }
                if event.table
                    == if matches!(
                        point,
                        FaultPoint::ValidationCommit | FaultPoint::GroupCommit
                    ) {
                        "task_acceptances"
                    } else {
                        "dispatch_outbox"
                    }
                {
                    signal.store(true, Ordering::SeqCst);
                }
            });
        if matches!(
            point,
            FaultPoint::BeforeCommit | FaultPoint::ValidationCommit | FaultPoint::GroupCommit
        ) {
            let signal = armed.clone();
            c.lock_handle().await.unwrap().set_commit_hook(move || {
                if signal.load(Ordering::SeqCst) {
                    marker(point);
                }
                true
            });
        }
    }
}
pub struct Probe {
    pub base: Authority,
    pub point: FaultPoint,
    pub used: AtomicBool,
    pub resume: bool,
}
impl AssignmentPolicy for Probe {
    fn check(&self, a: &ExecutionAssignment) -> Result<(), AssignmentDenial> {
        let result = self.base.check(a);
        if self.point == FaultPoint::EligibilityRead && !self.used.swap(true, Ordering::SeqCst) {
            if self.resume {
                rendezvous();
            } else {
                marker(self.point);
            }
        }
        result
    }
}
impl SchedulerPolicy for Probe {
    fn permits(&self, o: &PrincipalId, action: SchedulerAction<'_>, t: i64) -> bool {
        if self.point == FaultPoint::WriteLock
            && matches!(&action, SchedulerAction::Admit { .. })
            && !self.used.swap(true, Ordering::SeqCst)
        {
            marker(self.point);
        }
        SchedulerPolicy::permits(&self.base, o, action, t)
    }
}
pub fn admission_request(
    key: &PlanKey,
    fingerprint: &str,
    id: &str,
    task: &str,
) -> AdmissionRequest {
    let task = TaskRef {
        plan: key.clone(),
        task_id: TaskId::new(task).unwrap(),
    };
    AdmissionRequest {
        task,
        fingerprint: fingerprint.into(),
        expected_epoch: 0,
        request_id: RequestId::new(id).unwrap(),
        actor: owner(),
        permit: ExecutionPermit {
            id: e(&format!("cap-{id}")),
            provider: e("synthetic"),
            account: e("synthetic-account"),
            pool: e("synthetic-pool"),
            executor: PrincipalId::new("synthetic-worker").unwrap(),
            maximum: costs(),
            valid_until: T + 3600,
        },
    }
}
pub fn standalone(f: &Fixture) -> AdmissionRequest {
    admission_request(
        &f.content.key,
        f.revision.fingerprint(),
        "fault-attempt",
        "task-a",
    )
}
pub struct EffectVictim {
    pub provider: SyntheticProvider,
}
impl LaunchAdapter for EffectVictim {
    async fn inspect(&self, c: &LaunchCommand) -> Result<ProviderObservation, WorkerError> {
        self.provider.inspect(c).await
    }
    async fn launch(
        &self,
        a: &LaunchAuthorization,
        clock: &impl Clock,
    ) -> Result<LaunchResult, WorkerError> {
        self.provider.launch(a, clock).await?;
        marker(FaultPoint::LaunchEffect)
    }
}
pub async fn reopen(path: &Path) -> PlanStore {
    let started = std::time::Instant::now();
    let s = PlanStore::open(path).await.unwrap();
    s.check_integrity().await.unwrap();
    println!("R10 reopen+integrity observation: {:?}", started.elapsed());
    s
}
