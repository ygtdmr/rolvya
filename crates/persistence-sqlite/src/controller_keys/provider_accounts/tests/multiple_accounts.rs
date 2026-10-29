use super::*;
use crate::{orchestration::*, worker_simulation::*};
use rolvya_domain::worker::*;
use rolvya_plan_validator::{AssignmentDenial, AssignmentPolicy};
use std::collections::BTreeMap;
#[derive(Clone)]
struct Accounts(BTreeMap<TaskId, ExecutionAssignment>);
impl AssignmentPolicy for Accounts {
    fn check(&self, a: &ExecutionAssignment) -> Result<(), AssignmentDenial> {
        if self.0.values().any(|allowed| allowed == a) {
            Ok(())
        } else {
            Err(AssignmentDenial::UnauthorizedAccessMode)
        }
    }
}
impl SchedulerPolicy for Accounts {
    fn permits(&self, o: &PrincipalId, a: SchedulerAction<'_>, t: i64) -> bool {
        SchedulerPolicy::permits(&synthetic::Authority::default(), o, a, t)
    }
}
fn caps(a: &ExecutionAssignment) -> DeclaredCapabilities {
    let mut c = synthetic::caps();
    c.models = vec![ModelName::new(&a.model).unwrap()];
    c.access_modes = vec![a.access_mode];
    c
}
impl WorkerAuthority for Accounts {
    fn permits(
        &self,
        p: &PeerEvidence,
        b: &WorkerBinding,
        _: WorkerOperation,
        c: Option<&DeclaredCapabilities>,
        _: i64,
    ) -> bool {
        p.reference == synthetic::peer_ref(b)
            && b.worker_id.as_str() == "synthetic-worker"
            && self
                .0
                .get(&b.task.task_id)
                .is_some_and(|a| c == Some(&caps(a)))
    }
}
impl SyntheticExecutionAuthority for Accounts {
    fn actor(&self) -> PrincipalId {
        common::owner()
    }
    fn assignment(
        &self,
        t: &PlannedTask,
        n: u8,
        now: i64,
    ) -> Result<SyntheticAssignment, OrchestrationError> {
        let mut a =
            SyntheticExecutionAuthority::assignment(&synthetic::Authority::default(), t, n, now)?;
        let scope = self.0.get(&t.id).ok_or(OrchestrationError::Disconnected)?;
        a.capabilities = caps(scope);
        if scope.access_mode == AccessMode::CliSession {
            a.permit.maximum.ai_api = synthetic::money(0);
        }
        Ok(a)
    }
    fn peer(&self, b: &WorkerBinding) -> PeerEvidence {
        PeerEvidence {
            reference: synthetic::peer_ref(b),
        }
    }
}
#[tokio::test]
async fn four_isolated_accounts_execute_through_r06_r09_and_real_s204_credentials() {
    let mut content = common::plan();
    content.tasks.clear();
    let modes = [
        ("task-a", "claude-a", AccessMode::CliSession),
        ("task-b", "claude-b", AccessMode::CliSession),
        ("task-c", "codex-c", AccessMode::CliSession),
        ("task-d", "api-d", AccessMode::ApiKey),
    ];
    for (id, connection, mode) in modes {
        let mut task = common::task(id, &content.key);
        task.acceptance_criteria = vec![ARTIFACT_CRITERION.into()];
        let a = task.execution.as_mut().unwrap();
        a.connection_ref = connection.into();
        a.access_mode = mode;
        a.model = format!("synthetic/{connection}");
        content.tasks.push(task);
    }
    let accounts = Accounts(
        content
            .tasks
            .iter()
            .map(|t| (t.id.clone(), t.execution.clone().unwrap()))
            .collect(),
    );
    let f = Fixture::for_plan(content, &accounts).await;
    let bootstrap = ControllerBootstrap::open(
        f.store.clone(),
        &f._root.path().join("keys"),
        ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let registry = fixture_registry(&bootstrap, synthetic::T).await;
    let policy = registry.policy(accounts.clone());
    let co = SyntheticCoordinator::open(
        f.store.clone(),
        &f.request.task.plan.project_id,
        &policy,
        Limits::default(),
    )
    .await
    .unwrap();
    let fingerprint = co.fingerprint().to_owned();
    let clock = Arc::new(DeterministicClock::new(synthetic::T).unwrap());
    let limits = PassLimits {
        admissions: 4,
        events: 0,
        validations: 0,
        groups: 0,
    };
    let report = co.pass(&policy, clock.as_ref(), limits).await.unwrap();
    assert!(report.blocked.is_empty(), "{:?}", report.blocked);
    assert_eq!(f.count("task_attempts").await, 4);
    assert_eq!(f.count("provider_capacity_reservations").await, 4);
    assert_eq!(f.count("synthetic_operations").await, 4);
    assert_eq!(f.count("task_acceptances").await, 0);
    let distinct: i64 =
        sqlx::query_scalar("SELECT count(DISTINCT account) FROM provider_connections")
            .fetch_one(&f.store.pool)
            .await
            .unwrap();
    assert_eq!(distinct, 4);
    let mut workers = BTreeMap::new();
    for (task, assignment) in &accounts.0 {
        let row = sqlx::query("SELECT attempt_id,epoch FROM task_attempts WHERE task_id=?")
            .bind(task.as_str())
            .fetch_one(&f.store.pool)
            .await
            .unwrap();
        let lease = LeaseRef {
            task: TaskRef {
                plan: f.request.task.plan.clone(),
                task_id: task.clone(),
            },
            attempt_id: AttemptId::new(row.get::<String, _>(0)).unwrap(),
            epoch: row.get::<i64, _>(1) as u64,
        };
        let worker = crate::controller_keys::worker_auth::FixtureWorker::bootstrap(
            &bootstrap,
            Arc::new(registry.policy(accounts.clone())),
            clock.clone(),
            &lease,
            caps(assignment),
        )
        .await;
        workers.insert(lease.attempt_id.as_str().to_owned(), worker);
    }
    assert_eq!(f.count("worker_credentials").await, 4);
    for offset in [0, 1, 2, 30, 42, 50] {
        clock.advance_to(synthetic::T + offset).unwrap();
        let rows=sqlx::query("SELECT attempt_id,payload FROM synthetic_worker_events WHERE delivery='pending' AND due_at<=? ORDER BY due_at,ordinal,attempt_id").bind(clock.now().unwrap()).fetch_all(&f.store.pool).await.unwrap();
        for row in rows {
            let bytes: Vec<u8> = row.get("payload");
            let worker = &workers[&row.get::<String, _>("attempt_id")];
            worker.receive(&bytes).await.unwrap();
            worker.receive(&bytes).await.unwrap();
        }
        // Existing R10 delivery projection recognizes the independently authenticated
        // durable receipts; R09 remains the only artifact-validation/acceptance driver.
        let report = co
            .pass(
                &policy,
                clock.as_ref(),
                PassLimits {
                    admissions: 0,
                    events: 100,
                    validations: 4,
                    groups: 0,
                },
            )
            .await
            .unwrap();
        assert!(report.blocked.is_empty(), "{:?}", report.blocked);
        if offset == 2 {
            assert_eq!(co.snapshot().await.unwrap().running, 4);
        }
    }
    assert_eq!(co.snapshot().await.unwrap().accepted, 4);
    assert_eq!(f.count("task_attempts").await, 4);
    assert_eq!(f.count("synthetic_operations").await, 4);
    assert_eq!(f.count("task_acceptances").await, 4);
    assert_eq!(co.fingerprint(), fingerprint);
    let held = f
        .store
        .financial_snapshot(&f.request.task.plan.project_id)
        .await
        .unwrap()
        .held;
    assert_eq!(held.ai_api.minor_units(), 1);
    assert_eq!(held.compute.minor_units(), 8);
    assert_eq!(held.storage.minor_units(), 12);
    assert_eq!(held.transfer.minor_units(), 4);
    sqlx::query("UPDATE provider_policy_revisions SET revision='withdrawn',version=2")
        .execute(&f.store.pool)
        .await
        .unwrap();
    assert_eq!(co.snapshot().await.unwrap().accepted, 4);
    assert_eq!(f.count("synthetic_validations").await, 4);
    assert_eq!(co.fingerprint(), fingerprint);
    // Reopening preserves accepted work and held liabilities; a public unavailable
    // provider verifier cannot reenable any actual account or extra launch.
    let reopened = PlanStore::open(f._root.path().join("data/state.sqlite"))
        .await
        .unwrap();
    reopened.check_integrity().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM synthetic_operations")
            .fetch_one(&reopened.pool)
            .await
            .unwrap(),
        4
    );
}
