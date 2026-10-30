//! Real R06/R08, SQLx, SDK JSON/SigV4 and TLS; no live AWS calls.
use super::super::super::aws_fargate::{
    self as aws,
    execution::{self as run, ExecutionAdapter},
    *,
};
use super::aws::AwsAuthority;
use super::*;
use crate::PrincipalId;
use crate::worker_control::{DispatchState, OutboxDispatcher};
use std::sync::atomic::Ordering;
struct FixtureRun {
    f: Fixture<AwsAuthority>,
    identity: Arc<FargateIdentity>,
    adapter: run::FargateExecution,
    world: Arc<run::fixture::World>,
    expected: ExpectedExecution,
    request: run::LaunchRequest,
}
fn exposure(now: i64) -> run::ExposureEnvelope {
    let rate = run::UnitRate {
        charge: synthetic::money(0),
        units: 1,
    };
    run::ExposureEnvelope {
        format: 1,
        evidence: synthetic::e("synthetic-bounded-aws-fixture-no-live-price"),
        revision: 1,
        collected_at: now,
        valid_until: now + 600,
        runtime_seconds: 60,
        provisioning_seconds: 30,
        termination_seconds: 30,
        cpu_unit_second: rate,
        memory_mib_second: rate,
        extra_gib_second: rate,
        fixed: synthetic::costs(),
    }
}
impl FixtureRun {
    async fn new() -> Self {
        Self::new_scoped(false).await
    }
    async fn new_scoped(reviewed: bool) -> Self {
        Self::new_runtime(reviewed, 60).await
    }
    async fn new_runtime(reviewed: bool, runtime: u32) -> Self {
        let f = Fixture::with_authority_runtime(AwsAuthority::default(), runtime).await;
        let l = f.lease("task-a").await;
        let mut tx = f.store.pool.begin().await.unwrap();
        let (cmd, _, _) = worker_control::command(&mut tx, &l).await.unwrap();
        tx.commit().await.unwrap();
        let mut e = aws::fixture::expected("task-a");
        e.installation = super::super::super::hex(
            &f.bootstrap
                .load_existing()
                .await
                .unwrap()
                .metadata()
                .installation
                .0,
        );
        e.binding = cmd.binding().clone();
        e.command_digest = Digest::new(cmd.digest().unwrap()).unwrap();
        e.workload.runtime_seconds = runtime;
        e.authorization_until = synthetic::T + i64::from(runtime) + 60;
        e.operation_token = e.expected_operation_token().unwrap();
        let http = aws::fixture::AwsFixture::new(&e);
        let identity = if reviewed {
            let enrollment=crate::controller_keys::human_auth::sessions::owner::infrastructure_consent_on_existing(f.store.clone(),f.root.path().join("data/state.sqlite"),f.root.path().join("keys"),&e).await;
            let i = aws::fixture_receipt_open(
                &f.bootstrap,
                f.policy.clone(),
                f.clock.clone(),
                None,
                http.http(),
            )
            .await;
            i.verify_account(
                &enrollment,
                &RequestId::new("lifecycle-controller-check").unwrap(),
            )
            .await
            .unwrap();
            i.enroll_account(&enrollment).await.unwrap();
            i
        } else {
            let (i, _) = aws::fixture::open(
                &f.bootstrap,
                f.policy.clone(),
                f.clock.clone(),
                &e,
                http.http(),
            )
            .await;
            i.enroll_account(&aws::fixture::enrollment(&e))
                .await
                .unwrap();
            i
        };
        let world =
            run::fixture::World::new(&f.root.path().join("external-aws.sqlite"), vec![e.clone()])
                .await;
        world.use_broker.store(reviewed, Ordering::SeqCst);
        let mut adapter = run::fixture::open(identity.clone(), world.clone(), true).await;
        if let Ok(label) = std::env::var("ROLVYA_EXECUTION_CRASH_CHILD") {
            let report = std::env::var("ROLVYA_EXECUTION_CRASH_REPORT").unwrap();
            fs::write(
                report,
                serde_json::to_vec(&serde_json::json!({"root":f.root.path(),"expected":e}))
                    .unwrap(),
            )
            .unwrap();
            run::fixture::kill_at(&mut adapter, &label);
        }
        let allocation=run::BootstrapAllocation{secret:SecretArn::parse("arn:aws:secretsmanager:eu-central-1:123456789012:secret:rolvya/bootstrap/attempt-a-AbCdEf").unwrap(),task_role:e.workload.task_role.clone(),expires_at:synthetic::T+90};
        let mut bounds = exposure(synthetic::T);
        bounds.runtime_seconds = runtime;
        let request = adapter
            .prepare(&l, &e.account_ref, &allocation, &bounds)
            .await
            .unwrap();
        Self {
            f,
            identity,
            adapter,
            world,
            expected: e,
            request,
        }
    }
    async fn dispatch(&self) -> crate::worker_control::DispatchState {
        let l = self.expected.lease();
        let d = OutboxDispatcher::new(self.f.store.clone());
        let ticket = d
            .claim(
                &l,
                &self.f.policy.actor(),
                self.f.policy.as_ref(),
                self.f.clock.as_ref(),
                Limits::default(),
            )
            .await
            .unwrap();
        d.dispatch(
            &ticket,
            &self.adapter,
            self.f.policy.as_ref(),
            self.f.clock.as_ref(),
            Limits::default(),
        )
        .await
        .unwrap()
    }
    async fn events(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM aws_execution_events")
            .fetch_one(&self.f.store.pool)
            .await
            .unwrap()
    }
}
#[tokio::test]
async fn exact_sdk_launch_receipt_is_durable_and_replay_never_relaunches() {
    let f = FixtureRun::new().await;
    assert_eq!(f.dispatch().await, DispatchState::Active);
    assert_eq!(f.world.count().await, 1);
    let s = f.adapter.status(&f.expected.lease()).await.unwrap();
    assert_eq!(s.kind, "receipt");
    assert_eq!(s.task, Some(f.expected.task.clone()));
    assert!(!s.unknown_exposure);
    assert_eq!(f.dispatch().await, DispatchState::Active);
    assert_eq!(f.world.count().await, 1);
    {
        let bodies = f.world.requests.lock().unwrap();
        let b = bodies
            .iter()
            .find(|v| v.get("clientToken").is_some())
            .unwrap();
        assert_eq!(
            b["networkConfiguration"]["awsvpcConfiguration"]["assignPublicIp"],
            "DISABLED"
        );
        assert!(b.get("overrides").is_none());
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_credentials")
            .fetch_one(&f.f.store.pool)
            .await
            .unwrap(),
        0,
        "receipt cannot mint worker credentials"
    );
}
#[tokio::test]
async fn lost_launch_response_keeps_effect_and_all_financial_holds_without_retry() {
    let f = FixtureRun::new().await;
    let before =
        f.f.store
            .financial_snapshot(&f.expected.binding.task.plan.project_id)
            .await
            .unwrap();
    f.world.behavior.store(1, Ordering::SeqCst);
    assert_eq!(f.dispatch().await, DispatchState::Blocked);
    assert_eq!(f.world.count().await, 1);
    let s = f.adapter.status(&f.expected.lease()).await.unwrap();
    assert!(s.unknown_exposure);
    assert!(s.task.is_none());
    f.world.behavior.store(0, Ordering::SeqCst);
    let recovered = f.adapter.reconcile(&f.expected.lease()).await.unwrap();
    assert_eq!(recovered.task, Some(f.expected.task.clone()));
    assert!(
        recovered.unknown_exposure,
        "external identity does not settle unknown charges"
    );
    assert_eq!(f.world.count().await, 1);
    let after =
        f.f.store
            .financial_snapshot(&f.expected.binding.task.plan.project_id)
            .await
            .unwrap();
    assert_eq!(before.held, after.held);
    assert!(after.uncertain_liabilities > 0);
}
#[tokio::test]
async fn stop_ack_is_not_termination_or_final_billing_and_cleanup_is_scoped() {
    let f = FixtureRun::new().await;
    f.dispatch().await;
    assert!(
        f.adapter
            .stop(&f.expected.lease(), run::StopReason::OwnerCancellation)
            .await
            .is_err()
    );
    f.f.clock.advance_to(synthetic::T + 60).unwrap();
    let before =
        f.f.store
            .financial_snapshot(&f.expected.binding.task.plan.project_id)
            .await
            .unwrap();
    let s = f
        .adapter
        .stop(&f.expected.lease(), run::StopReason::RuntimeLimit)
        .await
        .unwrap();
    assert_eq!(s.kind, "stop_ack");
    assert_ne!(s.lifecycle, EcsLifecycle::Stopped);
    assert!(
        f.adapter
            .request_cleanup(&f.expected.lease())
            .await
            .is_err()
    );
    *f.world.status.lock().unwrap() = "STOPPED".into();
    let s = f.adapter.reconcile(&f.expected.lease()).await.unwrap();
    assert_eq!(s.lifecycle, EcsLifecycle::Stopped);
    let s = f
        .adapter
        .request_cleanup(&f.expected.lease())
        .await
        .unwrap();
    assert!(s.cleanup_requested);
    let n = f.events().await;
    f.adapter
        .request_cleanup(&f.expected.lease())
        .await
        .unwrap();
    assert_eq!(f.events().await, n);
    assert_eq!(
        before.held,
        f.f.store
            .financial_snapshot(&f.expected.binding.task.plan.project_id)
            .await
            .unwrap()
            .held
    );
}
#[tokio::test]
async fn mixed_response_delayed_visibility_and_revocation_preserve_uncertainty() {
    for mode in [2, 3, 4, 7] {
        let f = FixtureRun::new().await;
        f.world.behavior.store(mode, Ordering::SeqCst);
        assert_eq!(f.dispatch().await, DispatchState::Blocked, "mode {mode}");
        assert!(
            f.adapter
                .status(&f.expected.lease())
                .await
                .unwrap()
                .unknown_exposure
        );
        assert_eq!(f.world.count().await, if mode == 4 { 0 } else { 1 });
    }
    let f = FixtureRun::new().await;
    f.world.behavior.store(1, Ordering::SeqCst);
    f.dispatch().await;
    f.world.behavior.store(5, Ordering::SeqCst);
    let s = f.adapter.reconcile(&f.expected.lease()).await.unwrap();
    assert!(s.unknown_exposure);
    assert!(s.task.is_none());
    assert_eq!(f.world.count().await, 1);
}
#[tokio::test]
async fn restart_and_stale_snapshot_require_independent_reconciliation() {
    let f = FixtureRun::new().await;
    f.dispatch().await;
    let a = run::fixture::open(f.identity.clone(), f.world.clone(), false).await;
    assert!(
        a.prepare(
            &f.expected.lease(),
            &f.request.account_ref,
            &f.request.bootstrap,
            &f.request.exposure
        )
        .await
        .is_err()
    );
    assert_eq!(f.world.count().await, 1);
    let production = run::FargateExecution::open(&f.f.bootstrap, f.f.policy.clone())
        .await
        .unwrap();
    assert!(
        production
            .prepare(
                &f.expected.lease(),
                &f.request.account_ref,
                &f.request.bootstrap,
                &f.request.exposure
            )
            .await
            .is_err()
    );
}
#[tokio::test]
async fn sdk_receipt_enters_real_owner_broker_bootstrap_tls_and_s204_issuer() {
    let f = FixtureRun::new_scoped(true).await;
    assert!(
        f.identity
            .record_execution(&aws::fixture::registration(&f.expected))
            .await
            .is_err()
    );
    assert_eq!(f.dispatch().await, DispatchState::Active);
    let expected = f.adapter.bind_receipt(&f.expected.lease()).await.unwrap();
    assert_eq!(expected, f.expected);
    let plan = ProtectedDelivery {
        format: 1,
        execution: expected.clone(),
        secret: f.request.bootstrap.secret.clone(),
        version_id: new_id().unwrap(),
        expires_at: f.request.bootstrap.expires_at,
    };
    let world = aws::secure_fixture::World::new(&plan);
    let service = aws::secure_fixture::service(f.identity.clone(), world.clone());
    let (mut server, _) = f.f.server().await;
    let state = Arc::get_mut(&mut server).unwrap();
    state.security.identity = Arc::new(aws::PossessionIdentity {
        service: service.clone(),
        transport: state.transport.clone(),
    });
    state.secure_fargate = Some(service);
    let challenge = server
        .prepare_fargate_bootstrap(
            &PrepareBootstrap {
                lease: expected.lease(),
                request_id: RequestId::new("lifecycle-bootstrap").unwrap(),
                evidence: synthetic::e("lifecycle-protected-resource"),
            },
            &plan,
        )
        .await
        .unwrap();
    let delivered = ProtectedDelivery {
        version_id: challenge.secret_version.clone(),
        expires_at: challenge.expires_at,
        ..plan
    };
    let material = world
        .read_as_role(&expected.workload.task_role, &delivered, synthetic::T)
        .await
        .unwrap();
    let (url, stop, join) = listen(server).await;
    let mut client = f.f.cert.client(&url);
    material
        .authenticate(&mut client, RequestId::new("lifecycle-tls-claim").unwrap())
        .await
        .unwrap();
    assert_eq!(client.metadata().unwrap().binding, expected.binding);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_credentials")
            .fetch_one(&f.f.store.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM aws_lifecycle_credential_uses WHERE operation='launch_execution'"
        )
        .fetch_one(&f.f.store.pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(f.world.count().await, 1);
    f.f.co
        .pass_with_adapter(
            f.f.policy.as_ref(),
            &f.adapter,
            f.f.clock.as_ref(),
            PassLimits {
                admissions: 0,
                events: 0,
                validations: 0,
                groups: 0,
            },
        )
        .await
        .unwrap();
    let fingerprint = f.f.co.fingerprint().to_owned();
    let assignment = client
        .assignment(RequestId::new("launched-artifact-assignment").unwrap())
        .await
        .unwrap();
    let script = FakeWorker::new(assignment.binding, assignment.capabilities, assignment.seed)
        .unwrap()
        .with_inputs(assignment.inputs)
        .unwrap()
        .script(FakeBehavior::Success, synthetic::T)
        .unwrap();
    for event in &script.events {
        f.f.clock.advance_to(event.at).unwrap();
        if let SimulatedInput::Message(bytes) = &event.input {
            let message: WorkerMessage = serde_json::from_slice(bytes).unwrap();
            let artifact = matches!(
                message.body,
                WorkerBody::SubmitResult {
                    outcome: ExecutionOutcome::Succeeded { .. }
                }
            )
            .then(|| script.artifact.as_ref().unwrap().bytes.as_slice());
            client.message(message, artifact).await.unwrap();
        }
    }
    assert_eq!(f.f.co.snapshot().await.unwrap().accepted, 0);
    f.f.co
        .validator()
        .validate_leaf(&expected.lease(), f.f.policy.as_ref(), f.f.clock.as_ref())
        .await
        .unwrap();
    let accepted =
        f.f.co
            .validator()
            .verify_accepted(&expected.binding.task)
            .await
            .unwrap();
    f.world.revoked.store(true, Ordering::SeqCst);
    assert_eq!(
        f.f.co
            .validator()
            .verify_accepted(&expected.binding.task)
            .await
            .unwrap(),
        accepted
    );
    assert_eq!(f.f.co.fingerprint(), fingerprint);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM tasks")
            .fetch_one(&f.f.store.pool)
            .await
            .unwrap(),
        4
    );
    stop.send(()).unwrap();
    join.await.unwrap();
}

#[path = "tests_lifecycle_process.rs"]
mod process;

#[tokio::test]
async fn regression_prepare_replay_preserves_original_timestamp() {
    let f = FixtureRun::new().await;
    f.f.clock.advance_to(synthetic::T + 1).unwrap();
    assert_eq!(
        f.adapter
            .prepare(
                &f.expected.lease(),
                &f.request.account_ref,
                &f.request.bootstrap,
                &f.request.exposure
            )
            .await
            .unwrap(),
        f.request
    );
}
#[tokio::test]
async fn regression_stop_timeout_is_reconciled_and_bounded_retry_reaches_provider() {
    let f = FixtureRun::new().await;
    f.dispatch().await;
    f.f.clock.advance_to(synthetic::T + 60).unwrap();
    f.world.behavior.store(6, Ordering::SeqCst);
    assert_eq!(
        f.adapter
            .stop(&f.expected.lease(), run::StopReason::RuntimeLimit)
            .await
            .unwrap()
            .kind,
        "stop_unknown"
    );
    assert_eq!(f.world.stop_count().await, 1);
    f.world.behavior.store(0, Ordering::SeqCst);
    let s = f
        .adapter
        .stop(&f.expected.lease(), run::StopReason::RuntimeLimit)
        .await
        .unwrap();
    assert_eq!(s.kind, "stop_ack");
    assert_eq!(f.world.stop_count().await, 2);
}
#[tokio::test]
async fn launch_scope_mutations_and_reserved_cost_substitution_are_rejected() {
    let f = FixtureRun::new().await;
    for n in 0..16 {
        let mut r = f.request.clone();
        match n {
            0 => r.installation = "f".repeat(32),
            1 => r.account_ref = synthetic::e("other-account"),
            2 => r.account_version += 1,
            3 => r.binding.epoch += 1,
            4 => r.command_digest = Digest::new("e".repeat(64)).unwrap(),
            5 => {
                r.workload.cluster =
                    ClusterArn::parse("arn:aws:ecs:eu-central-1:999999999999:cluster/rolvya")
                        .unwrap()
            }
            6 => r.workload.image = "worker:latest".into(),
            7 => r.workload.task_role = r.workload.execution_role.clone(),
            8 => r.workload.subnets = vec!["subnet-00000000".into()],
            9 => r.workload.runtime_seconds = 0,
            10 => r.bootstrap.task_role = r.workload.execution_role.clone(),
            11 => r.exposure.fixed.compute = synthetic::money(99),
            12 => r.maximum.ai_api = synthetic::money(0),
            13 => r.format = 99,
            14 => r.authorization_until = r.authorized_at,
            _ => r.client_token = "e".repeat(64),
        };
        assert!(r.validate(synthetic::T).is_err(), "mutation {n}");
    }
    assert!(f.request.validate(f.request.launch_until).is_err());
    assert!(f.request.validate(synthetic::T + 86400).is_err());
    assert_eq!(f.world.count().await, 0);
    assert_eq!(f.events().await, 1);
}
#[tokio::test]
async fn concurrent_dispatchers_commit_at_most_one_effect_or_preserve_unknown_exposure() {
    let f = FixtureRun::new().await;
    let (_a, _b) = tokio::join!(f.dispatch(), f.dispatch());
    let count = f.world.count().await;
    assert!(count <= 1);
    let receipts = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM aws_sdk_receipts")
        .fetch_one(&f.f.store.pool)
        .await
        .unwrap();
    if count == 0 {
        assert!(
            f.f.store
                .financial_snapshot(&f.expected.binding.task.plan.project_id)
                .await
                .unwrap()
                .uncertain_liabilities
                > 0
        )
    } else {
        assert_eq!(receipts, 1)
    }
}

#[tokio::test]
async fn ignored_receipt_write_cannot_erase_provider_effect_or_allow_relaunch() {
    let f = FixtureRun::new().await;
    sqlx::query("CREATE TRIGGER ignore_sdk_receipt BEFORE INSERT ON aws_sdk_receipts BEGIN SELECT RAISE(IGNORE); END").execute(&f.f.store.pool).await.unwrap();
    assert_eq!(f.dispatch().await, DispatchState::Reconcile);
    assert_eq!(f.world.count().await, 1);
    assert_eq!(
        f.adapter.status(&f.expected.lease()).await.unwrap().kind,
        "submitting"
    );
    sqlx::query("DROP TRIGGER ignore_sdk_receipt")
        .execute(&f.f.store.pool)
        .await
        .unwrap();
    assert_eq!(f.dispatch().await, DispatchState::Blocked);
    assert_eq!(f.world.count().await, 1);
    assert!(
        f.adapter
            .status(&f.expected.lease())
            .await
            .unwrap()
            .unknown_exposure
    );
}
#[tokio::test]
async fn actual_snapshot_before_launch_cannot_rollback_external_reality() {
    let f = FixtureRun::new().await;
    let snapshot = f.f.root.path().join("data/before.sqlite");
    sqlx::query("VACUUM INTO ?")
        .bind(snapshot.to_str().unwrap())
        .execute(&f.f.store.pool)
        .await
        .unwrap();
    f.dispatch().await;
    assert_eq!(f.world.count().await, 1);
    let restored = PlanStore::open(&snapshot).await.unwrap();
    let b = ControllerBootstrap::open(
        restored.clone(),
        &f.f.root.path().join("keys"),
        ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let http = aws::fixture::AwsFixture::new(&f.expected);
    let (identity, _) = aws::fixture::open(
        &b,
        f.f.policy.clone(),
        f.f.clock.clone(),
        &f.expected,
        http.http(),
    )
    .await;
    let adapter = run::fixture::open(identity, f.world.clone(), false).await;
    let d = OutboxDispatcher::new(restored.clone());
    let ticket = d
        .claim(
            &f.expected.lease(),
            &f.f.policy.actor(),
            f.f.policy.as_ref(),
            f.f.clock.as_ref(),
            Limits::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        d.dispatch(
            &ticket,
            &adapter,
            f.f.policy.as_ref(),
            f.f.clock.as_ref(),
            Limits::default()
        )
        .await
        .unwrap(),
        DispatchState::Reconcile
    );
    assert_eq!(f.world.count().await, 1);
    assert_eq!(
        adapter.status(&f.expected.lease()).await.unwrap().kind,
        "prepared",
        "snapshot has older controller knowledge"
    );
}
#[tokio::test]
async fn wrong_task_definition_role_prevents_any_mutating_sdk_request() {
    let f = FixtureRun::new().await;
    f.world.behavior.store(8, Ordering::SeqCst);
    assert_eq!(f.dispatch().await, DispatchState::Reconcile);
    assert_eq!(f.world.count().await, 0);
    assert!(
        f.world
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|r| r.get("clientToken").is_none())
    );
}
#[tokio::test]
async fn stale_running_observation_cannot_resurrect_stopped_execution() {
    let f = FixtureRun::new().await;
    f.dispatch().await;
    *f.world.status.lock().unwrap() = "STOPPED".into();
    assert_eq!(
        f.adapter
            .reconcile(&f.expected.lease())
            .await
            .unwrap()
            .lifecycle,
        EcsLifecycle::Stopped
    );
    *f.world.status.lock().unwrap() = "RUNNING".into();
    let s = f.adapter.reconcile(&f.expected.lease()).await.unwrap();
    assert!(s.unknown_exposure);
    assert_eq!(s.lifecycle, EcsLifecycle::Unknown);
}
#[tokio::test]
async fn sqlite_lock_contention_serializes_against_receipt_authority() {
    let f = FixtureRun::new().await;
    let tx = f.f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let lease = f.expected.lease();
    let operation = f.adapter.prepare(
        &lease,
        &f.request.account_ref,
        &f.request.bootstrap,
        &f.request.exposure,
    );
    tokio::pin!(operation);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(25), &mut operation)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert_eq!(operation.await.unwrap(), f.request);
    assert_eq!(f.world.count().await, 0);
}
#[derive(Default)]
struct TwoAccounts(AwsAuthority);
impl rolvya_plan_validator::AssignmentPolicy for TwoAccounts {
    fn check(
        &self,
        a: &ExecutionAssignment,
    ) -> Result<(), rolvya_plan_validator::AssignmentDenial> {
        self.0.check(a)
    }
}
impl crate::admission::SchedulerPolicy for TwoAccounts {
    fn permits(&self, o: &PrincipalId, a: crate::admission::SchedulerAction<'_>, t: i64) -> bool {
        if let crate::admission::SchedulerAction::Admit { request, .. } = a {
            request.actor == *o
                && request.permit.provider.as_str() == AWS_PROVIDER
                && ["aws-account-a", "synthetic-account-b"]
                    .contains(&request.permit.account.as_str())
        } else {
            crate::admission::SchedulerPolicy::permits(&self.0, o, a, t)
        }
    }
}
impl WorkerAuthority for TwoAccounts {
    fn permits(
        &self,
        p: &PeerEvidence,
        b: &WorkerBinding,
        o: WorkerOperation,
        c: Option<&DeclaredCapabilities>,
        t: i64,
    ) -> bool {
        WorkerAuthority::permits(&self.0, p, b, o, c, t)
    }
}
impl SyntheticExecutionAuthority for TwoAccounts {
    fn actor(&self) -> PrincipalId {
        self.0.actor()
    }
    fn assignment(
        &self,
        t: &PlannedTask,
        n: u8,
        at: i64,
    ) -> Result<SyntheticAssignment, OrchestrationError> {
        let mut a = self.0.assignment(t, n, at)?;
        if t.id.as_str() == "task-b" {
            a.permit.account = synthetic::e("synthetic-account-b")
        }
        Ok(a)
    }
    fn peer(&self, b: &WorkerBinding) -> PeerEvidence {
        self.0.peer(b)
    }
}
#[tokio::test]
async fn independent_ab_launches_keep_exact_exclusive_task_roles_and_receipts() {
    let f = Fixture::with_authority_runtime(TwoAccounts::default(), 60).await;
    let mut executions = Vec::new();
    let mut identities = Vec::new();
    for task in ["task-a", "task-b"] {
        let l = f.lease(task).await;
        let mut tx = f.store.pool.begin().await.unwrap();
        let (cmd, _, _) = worker_control::command(&mut tx, &l).await.unwrap();
        tx.commit().await.unwrap();
        let mut e = aws::fixture::expected(task);
        e.installation = super::super::super::hex(
            &f.bootstrap
                .load_existing()
                .await
                .unwrap()
                .metadata()
                .installation
                .0,
        );
        e.binding = cmd.binding().clone();
        e.command_digest = Digest::new(cmd.digest().unwrap()).unwrap();
        e.authorization_until = synthetic::T + 120;
        if task == "task-b" {
            e.account_ref = synthetic::e("synthetic-account-b");
            e.workload.task_role =
                RoleArn::parse("arn:aws:iam::123456789012:role/worker/task-b").unwrap();
            e.workload.task_definition = TaskDefinitionArn::parse(
                "arn:aws:ecs:eu-central-1:123456789012:task-definition/worker:2",
            )
            .unwrap();
        }
        e.operation_token = e.expected_operation_token().unwrap();
        let http = aws::fixture::AwsFixture::new(&e);
        let (i, _) = aws::fixture::open(
            &f.bootstrap,
            f.policy.clone(),
            f.clock.clone(),
            &e,
            http.http(),
        )
        .await;
        i.enroll_account(&aws::fixture::enrollment(&e))
            .await
            .unwrap();
        identities.push(i);
        executions.push(e);
    }
    let world = run::fixture::World::new(
        &f.root.path().join("ab-external.sqlite"),
        executions.clone(),
    )
    .await;
    let mut adapters = Vec::new();
    for (n, (e, i)) in executions.iter().zip(&identities).enumerate() {
        let a = run::fixture::open(i.clone(), world.clone(), true).await;
        let allocation=run::BootstrapAllocation{secret:SecretArn::parse(&format!("arn:aws:secretsmanager:eu-central-1:123456789012:secret:rolvya/bootstrap/attempt-{n}-AbCdEf")).unwrap(),task_role:e.workload.task_role.clone(),expires_at:synthetic::T+90};
        a.prepare(
            &e.lease(),
            &e.account_ref,
            &allocation,
            &exposure(synthetic::T),
        )
        .await
        .unwrap();
        adapters.push(a);
    }
    let d = OutboxDispatcher::new(f.store.clone());
    let a = d
        .claim(
            &executions[0].lease(),
            &f.policy.actor(),
            f.policy.as_ref(),
            f.clock.as_ref(),
            Limits::default(),
        )
        .await
        .unwrap();
    let b = d
        .claim(
            &executions[1].lease(),
            &f.policy.actor(),
            f.policy.as_ref(),
            f.clock.as_ref(),
            Limits::default(),
        )
        .await
        .unwrap();
    let (sa, sb) = tokio::join!(
        d.dispatch(
            &a,
            &adapters[0],
            f.policy.as_ref(),
            f.clock.as_ref(),
            Limits::default()
        ),
        d.dispatch(
            &b,
            &adapters[1],
            f.policy.as_ref(),
            f.clock.as_ref(),
            Limits::default()
        )
    );
    assert_eq!(sa.unwrap(), DispatchState::Active);
    assert_eq!(sb.unwrap(), DispatchState::Active);
    assert_eq!(world.count().await, 2);
    assert_ne!(
        adapters[0]
            .status(&executions[0].lease())
            .await
            .unwrap()
            .task,
        adapters[1]
            .status(&executions[1].lease())
            .await
            .unwrap()
            .task
    );
    assert_ne!(
        executions[0].workload.task_role,
        executions[1].workload.task_role
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_attempts WHERE task_id='group-g'")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        0
    );
}
#[tokio::test]
async fn renewable_execution_horizon_survives_original_ninety_second_launch_window() {
    let f = FixtureRun::new_runtime(false, 180).await;
    assert_eq!(f.request.launch_until, synthetic::T + 90);
    assert_eq!(f.request.authorization_until, synthetic::T + 240);
    f.dispatch().await;
    let expected = f.adapter.bind_receipt(&f.expected.lease()).await.unwrap();
    let plan = ProtectedDelivery {
        format: 1,
        execution: expected.clone(),
        secret: f.request.bootstrap.secret.clone(),
        version_id: new_id().unwrap(),
        expires_at: f.request.bootstrap.expires_at,
    };
    let world = aws::secure_fixture::World::new(&plan);
    let service = aws::secure_fixture::service(f.identity.clone(), world.clone());
    let (mut server, _) = f.f.server().await;
    let state = Arc::get_mut(&mut server).unwrap();
    state.security.identity = Arc::new(aws::PossessionIdentity {
        service: service.clone(),
        transport: state.transport.clone(),
    });
    state.secure_fargate = Some(service);
    let challenge = server
        .prepare_fargate_bootstrap(
            &PrepareBootstrap {
                lease: expected.lease(),
                request_id: RequestId::new("long-bootstrap").unwrap(),
                evidence: synthetic::e("long-protected-resource"),
            },
            &plan,
        )
        .await
        .unwrap();
    let delivered = ProtectedDelivery {
        version_id: challenge.secret_version.clone(),
        expires_at: challenge.expires_at,
        ..plan
    };
    let material = world
        .read_as_role(&expected.workload.task_role, &delivered, synthetic::T)
        .await
        .unwrap();
    f.f.co
        .pass_with_adapter(
            f.f.policy.as_ref(),
            &f.adapter,
            f.f.clock.as_ref(),
            PassLimits {
                admissions: 0,
                events: 0,
                validations: 0,
                groups: 0,
            },
        )
        .await
        .unwrap();
    let (url, stop, join) = listen(server).await;
    let mut client = f.f.cert.client(&url);
    material
        .authenticate(&mut client, RequestId::new("long-tls-claim").unwrap())
        .await
        .unwrap();
    let assignment = client
        .assignment(RequestId::new("long-assignment").unwrap())
        .await
        .unwrap();
    let script = FakeWorker::new(
        assignment.binding.clone(),
        assignment.capabilities,
        assignment.seed,
    )
    .unwrap()
    .script(FakeBehavior::Success, synthetic::T)
    .unwrap();
    for event in script.events.iter().take(4) {
        f.f.clock.advance_to(event.at).unwrap();
        let SimulatedInput::Message(bytes) = &event.input else {
            panic!("message")
        };
        client
            .message(serde_json::from_slice(bytes).unwrap(), None)
            .await
            .unwrap();
    }
    f.f.clock.advance_to(synthetic::T + 60).unwrap();
    let renewal = client
        .prepare_possession(RequestId::new("long-renewal").unwrap())
        .await
        .unwrap();
    let delivery = ProtectedDelivery {
        format: 1,
        execution: renewal.execution.clone(),
        secret: renewal.secret.clone(),
        version_id: renewal.secret_version.clone(),
        expires_at: renewal.expires_at,
    };
    world
        .read_as_role(&expected.workload.task_role, &delivery, synthetic::T + 60)
        .await
        .unwrap()
        .authenticate(&mut client, RequestId::new("long-renewal-claim").unwrap())
        .await
        .unwrap();
    assert_eq!(client.metadata().unwrap().generation, 2);
    f.f.clock.advance_to(synthetic::T + 91).unwrap();
    client
        .message(
            WorkerMessage {
                protocol_version: PROTOCOL_VERSION,
                message_id: MessageId::new("long-heartbeat-past-original-lease").unwrap(),
                binding: assignment.binding,
                sequence: 5,
                body: WorkerBody::Heartbeat {},
            },
            None,
        )
        .await
        .unwrap();
    assert!(client.metadata().unwrap().expires_at > synthetic::T + 91);
    assert!(
        f.request.validate(synthetic::T + 91).is_err(),
        "expired launch authority cannot relaunch"
    );
    assert_eq!(f.world.count().await, 1);
    stop.send(()).unwrap();
    join.await.unwrap();
}

#[tokio::test]
async fn competing_dispatch_during_sdk_inspection_blocks_without_effect_and_keeps_holds() {
    let f = FixtureRun::new().await;
    let barrier = Arc::new((tokio::sync::Notify::new(), tokio::sync::Notify::new()));
    *f.world.pause_definition.lock().unwrap() = Some(barrier.clone());
    let before =
        f.f.store
            .financial_snapshot(&f.expected.binding.task.plan.project_id)
            .await
            .unwrap();
    let (a, b) = tokio::join!(f.dispatch(), async {
        barrier.0.notified().await;
        let b = f.dispatch().await;
        barrier.1.notify_one();
        b
    });
    assert_eq!(a, DispatchState::Blocked);
    assert_eq!(b, DispatchState::Blocked);
    assert_eq!(f.world.count().await, 0);
    let after =
        f.f.store
            .financial_snapshot(&f.expected.binding.task.plan.project_id)
            .await
            .unwrap();
    assert_eq!(before.held, after.held);
    assert!(after.uncertain_liabilities > 0);
    assert!(
        f.adapter
            .status(&f.expected.lease())
            .await
            .unwrap()
            .unknown_exposure
    );
}
