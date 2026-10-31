#![allow(dead_code)]
// Shared deterministic fixture, not production authentication or provider limits.
use super::common::owner;
use super::common::*;
use rolvya_domain::{ids::*, money::*, plan::*, worker::*};
use rolvya_persistence_sqlite::{
    admission::*, finance::*, orchestration::*, worker_simulation::*, *,
};
use rolvya_plan_validator::{AssignmentDenial, AssignmentPolicy, Limits};
use std::collections::BTreeMap;
pub const T: i64 = 1_800_000_000;
pub fn e(s: &str) -> EvidenceRef {
    EvidenceRef::new(s).unwrap()
}
pub fn money(n: i64) -> Money {
    Money::new(n, Currency::new("USD").unwrap()).unwrap()
}
pub fn costs() -> CostBreakdown {
    CostBreakdown {
        ai_api: money(1),
        compute: money(2),
        storage: money(3),
        transfer: money(1),
    }
}
pub fn caps() -> DeclaredCapabilities {
    DeclaredCapabilities {
        models: vec![ModelName::new("synthetic/model").unwrap()],
        efforts: vec![Effort::Medium],
        access_modes: vec![AccessMode::ApiKey],
        cpu_millis: 1000,
        memory_mib: 512,
        evidence: Some(CapabilityRef::new("synthetic-capability").unwrap()),
    }
}
#[derive(Default)]
pub struct Authority {
    pub behaviors: BTreeMap<TaskId, FakeBehavior>,
    pub revoked: bool,
    pub fargate: bool,
}
impl AssignmentPolicy for Authority {
    fn check(&self, a: &ExecutionAssignment) -> Result<(), AssignmentDenial> {
        SyntheticPolicy.check(a)
    }
}
impl SchedulerPolicy for Authority {
    fn permits(&self, owner: &PrincipalId, a: SchedulerAction<'_>, _: i64) -> bool {
        if self.revoked {
            return false;
        }
        match a {
            SchedulerAction::Admit { request, .. } => {
                request.actor == *owner
                    && request.permit.provider.as_str()
                        == if self.fargate {
                            "aws-ecs-fargate"
                        } else {
                            "synthetic"
                        }
            }
            SchedulerAction::Dispatch { request, .. } => request.actor == *owner,
            SchedulerAction::Validate { request, .. } => {
                matches!(
                    request.actor.as_str(),
                    "synthetic-validator" | "coding-validator"
                )
            }
            SchedulerAction::Fence(r) => r.actor == *owner,
            SchedulerAction::Recover(r) => r.actor == *owner,
        }
    }
}
impl WorkerAuthority for Authority {
    fn permits(
        &self,
        p: &PeerEvidence,
        b: &WorkerBinding,
        _: WorkerOperation,
        c: Option<&DeclaredCapabilities>,
        _: i64,
    ) -> bool {
        !self.revoked
            && p.reference == peer_ref(b)
            && b.worker_id.as_str() == "synthetic-worker"
            && c.is_some_and(|c| *c == caps())
    }
}
pub fn peer_ref(b: &WorkerBinding) -> EvidenceRef {
    e(&format!("peer-{}", b.attempt_id.as_str()))
}
impl SyntheticExecutionAuthority for Authority {
    fn actor(&self) -> PrincipalId {
        owner()
    }
    fn assignment(
        &self,
        t: &PlannedTask,
        ordinal: u8,
        now: i64,
    ) -> Result<SyntheticAssignment, OrchestrationError> {
        Ok(SyntheticAssignment {
            permit: ExecutionPermit {
                id: e(&format!("cap-{}-{ordinal}", t.id.as_str())),
                provider: e(if self.fargate {
                    "aws-ecs-fargate"
                } else {
                    "synthetic"
                }),
                account: e(if self.fargate && t.id.as_str() == "task-b" {
                    "synthetic-account-b"
                } else {
                    "synthetic-account"
                }),
                pool: e("synthetic-pool"),
                executor: PrincipalId::new("synthetic-worker").unwrap(),
                maximum: costs(),
                valid_until: now + 3600,
            },
            capabilities: caps(),
            seed: if t.id.as_str() == "task-a" { 1 } else { 2 },
            behavior: self
                .behaviors
                .get(&t.id)
                .copied()
                .unwrap_or(FakeBehavior::Success),
        })
    }
    fn peer(&self, b: &WorkerBinding) -> PeerEvidence {
        PeerEvidence {
            reference: peer_ref(b),
        }
    }
}
pub struct Finance;
impl FinancialAuthorization for Finance {
    fn permits(&self, o: &PrincipalId, r: &FinancialRequest, _: i64) -> bool {
        r.actor == *o
    }
}
pub async fn fixture(ceiling: i64) -> Fixture {
    fixture_with(ceiling, |_| {}).await
}
pub async fn fixture_with(ceiling: i64, edit: impl FnOnce(&mut PlanContent)) -> Fixture {
    let mut f = Fixture::new().await;
    f.content.key.version = PlanVersion::new(2).unwrap();
    let mut g = group("group-g", &f.content.key);
    g.acceptance_criteria = vec![GROUP_CRITERION.into()];
    let mut a = task("task-a", &f.content.key);
    a.parent = Some(task_ref(&g));
    a.acceptance_criteria = vec![ARTIFACT_CRITERION.into()];
    let mut b = a.clone();
    b.id = TaskId::new("task-b").unwrap();
    let mut c = task("task-c", &f.content.key);
    c.dependencies = vec![task_ref(&g)];
    c.acceptance_criteria = vec![ARTIFACT_CRITERION.into(), INPUTS_CRITERION.into()];
    f.content.tasks = vec![g, a, b, c];
    edit(&mut f.content);
    let valid =
        rolvya_plan_validator::validate(&f.content, &Authority::default(), Limits::default())
            .unwrap();
    f.revision = f
        .store
        .save_draft(&valid, Some(&f.revision), &owner(), &OwnerPolicy)
        .await
        .unwrap();
    f.activate().await.unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    f.store
        .apply_financial(
            &FinancialRequest {
                project: f.content.key.project_id.clone(),
                request_id: RequestId::new("budget").unwrap(),
                actor: owner(),
                reason: "Explicit synthetic fixture; no real charge".into(),
                command: FinancialCommand::Approve {
                    expected_version: 0,
                    new_ceiling: money(ceiling),
                    proof: OwnerProof {
                        reference: e("synthetic-budget-proof"),
                        verified_at: now,
                    },
                },
            },
            &Finance,
        )
        .await
        .unwrap();
    f
}
pub async fn open<P: SyntheticExecutionAuthority>(f: &Fixture, a: &P) -> SyntheticCoordinator {
    SyntheticCoordinator::open(
        f.store.clone(),
        &f.content.key.project_id,
        a,
        Limits::default(),
    )
    .await
    .unwrap()
}
pub fn scope(f: &Fixture, id: &str) -> TaskRef {
    TaskRef {
        plan: f.content.key.clone(),
        task_id: TaskId::new(id).unwrap(),
    }
}
pub async fn lease(f: &Fixture, id: &str) -> LeaseRef {
    let row = sqlx::query(
        "SELECT attempt_id,epoch FROM task_attempts WHERE task_id=? ORDER BY epoch DESC LIMIT 1",
    )
    .bind(id)
    .fetch_one(&f.raw().await)
    .await
    .unwrap();
    use sqlx::Row;
    LeaseRef {
        task: scope(f, id),
        attempt_id: AttemptId::new(row.get::<String, _>(0)).unwrap(),
        epoch: row.get::<i64, _>(1) as u64,
    }
}
pub async fn count(f: &Fixture, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&f.raw().await)
        .await
        .unwrap()
}
pub async fn phase(
    co: &SyntheticCoordinator,
    a: &Authority,
    clock: &DeterministicClock,
    admissions: u16,
    validations: u16,
    groups: u16,
) {
    for (index, offset) in [0, 1, 2, 30, 42, 50].into_iter().enumerate() {
        clock.advance_to(T + offset).unwrap();
        co.pass(
            a,
            clock,
            PassLimits {
                admissions: if index == 0 { admissions } else { 0 },
                events: 64,
                validations,
                groups,
            },
        )
        .await
        .unwrap();
    }
}
pub fn zero() -> CostBreakdown {
    CostBreakdown {
        ai_api: money(0),
        compute: money(0),
        storage: money(0),
        transfer: money(0),
    }
}
pub async fn synthetic_final(f: &Fixture, l: &LeaseRef) {
    let balance = f
        .store
        .financial_liability(
            &l.task.plan.project_id,
            &LiabilityId::new(l.attempt_id.as_str()).unwrap(),
        )
        .await
        .unwrap();
    f.store
        .apply_financial(
            &FinancialRequest {
                project: l.task.plan.project_id.clone(),
                request_id: RequestId::new(format!("bill-{}", l.attempt_id.as_str())).unwrap(),
                actor: owner(),
                reason: "Trusted deterministic synthetic-provider final evidence; zero real charge"
                    .into(),
                command: FinancialCommand::Reconcile {
                    liability: LiabilityId::new(l.attempt_id.as_str()).unwrap(),
                    expected_revision: balance.revision,
                    settled: zero(),
                    remaining_hold: zero(),
                    remaining_provisional: zero(),
                    state: ReconciliationState::Final,
                    evidence: e(&format!("bill-{}", l.attempt_id.as_str())),
                },
            },
            &Finance,
        )
        .await
        .unwrap();
}
