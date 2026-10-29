//! AWS protocol responses are simulated; sockets/TLS/admission/SQLite are real.
use super::super::super::aws_fargate::{self as aws, fixture as af, *};
use super::*;
use crate::{
    PrincipalId,
    admission::{FenceRequest, SchedulerAction, SchedulerPolicy},
};
use rolvya_plan_validator::{AssignmentDenial, AssignmentPolicy};
use std::{io::Read, sync::atomic::Ordering};
#[derive(Default)]
struct AwsAuthority(synthetic::Authority);
impl AssignmentPolicy for AwsAuthority {
    fn check(&self, a: &ExecutionAssignment) -> Result<(), AssignmentDenial> {
        self.0.check(a)
    }
}
impl SchedulerPolicy for AwsAuthority {
    fn permits(&self, o: &PrincipalId, a: SchedulerAction<'_>, t: i64) -> bool {
        match a {
            SchedulerAction::Admit { request, .. } => {
                request.actor == *o
                    && request.permit.provider.as_str() == AWS_PROVIDER
                    && request.permit.account.as_str() == "aws-account-a"
            }
            _ => SchedulerPolicy::permits(&self.0, o, a, t),
        }
    }
}
impl WorkerAuthority for AwsAuthority {
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
impl SyntheticExecutionAuthority for AwsAuthority {
    fn actor(&self) -> PrincipalId {
        SyntheticExecutionAuthority::actor(&self.0)
    }
    fn assignment(
        &self,
        t: &PlannedTask,
        n: u8,
        now: i64,
    ) -> Result<SyntheticAssignment, OrchestrationError> {
        let mut a = self.0.assignment(t, n, now)?;
        a.permit.provider = synthetic::e(AWS_PROVIDER);
        a.permit.account = synthetic::e("aws-account-a");
        Ok(a)
    }
    fn peer(&self, b: &WorkerBinding) -> PeerEvidence {
        self.0.peer(b)
    }
}
async fn expected_for(
    f: &Fixture<AwsAuthority>,
    s: &HttpsWorkerServer,
    task: &str,
) -> ExpectedExecution {
    let lease = f.lease(task).await;
    let mut tx = f.store.pool.begin().await.unwrap();
    let scope = s
        .security
        .scope(
            &mut tx,
            &lease,
            synthetic::T,
            worker_control::CredentialScopeUse::Bootstrap,
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let mut e = af::expected(task);
    e.installation = s.security.controller().installation;
    e.binding = scope.binding;
    e.command_digest = Digest::new(scope.command_digest).unwrap();
    e.authorization_until = scope.valid_until;
    e.operation_token = e.expected_operation_token().unwrap();
    e
}
async fn setup() -> (
    Fixture<AwsAuthority>,
    Arc<HttpsWorkerServer>,
    Arc<SignedProvider>,
    ExpectedExecution,
    Arc<FargateIdentity>,
    Arc<af::AwsFixture>,
) {
    let f = Fixture::with_authority(AwsAuthority::default()).await;
    let (s, v) = f.server().await;
    let e = expected_for(&f, &s, "task-a").await;
    let http = af::AwsFixture::new(&e);
    let (identity, _) = af::open(
        &f.bootstrap,
        f.policy.clone(),
        f.clock.clone(),
        &e,
        http.http(),
    )
    .await;
    (f, s, v, e, identity, http)
}
async fn registered(i: &FargateIdentity, e: &ExpectedExecution) {
    assert_eq!(i.enroll_account(&af::enrollment(e)).await.unwrap(), 1);
    i.record_execution(&af::registration(e)).await.unwrap();
}
async fn count(f: &Fixture<AwsAuthority>, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&f.store.pool)
        .await
        .unwrap()
}
#[tokio::test]
async fn aws_registry_and_inspection_survive_reopen_without_worker_or_financial_authority() {
    let (f, _, _, e, i, http) = setup().await;
    registered(&i, &e).await;
    let before = f
        .store
        .financial_snapshot(&e.binding.task.plan.project_id)
        .await
        .unwrap();
    let r = i
        .inspect(&e.lease(), &RequestId::new("inspection-a").unwrap())
        .await
        .unwrap();
    assert_eq!(
        r.worker_issuance(),
        Err(AwsIdentityError::TaskPossessionUnverified)
    );
    assert_eq!(r.revision, 1);
    assert_eq!(i.inspect(&e.lease(), &r.request_id).await.unwrap(), r);
    assert_eq!(http.calls.load(Ordering::SeqCst), 3);
    let (reopened, _) = af::open(
        &f.bootstrap,
        f.policy.clone(),
        f.clock.clone(),
        &e,
        http.http(),
    )
    .await;
    assert_eq!(
        reopened.inspect(&e.lease(), &r.request_id).await.unwrap(),
        r
    );
    assert_eq!(count(&f, "worker_credentials").await, 0);
    assert_eq!(count(&f, "task_attempts").await, 2);
    assert_eq!(
        f.store
            .financial_snapshot(&e.binding.task.plan.project_id)
            .await
            .unwrap(),
        before
    );
}
#[tokio::test]
async fn aws_production_enrollment_is_denied_and_claimed_account_is_not_authority() {
    let (f, _, _, e, _, http) = setup().await;
    let i = FargateIdentity::open(&f.bootstrap, f.policy.clone())
        .await
        .unwrap();
    assert_eq!(
        i.enroll_account(&af::enrollment(&e)).await,
        Err(AwsIdentityError::UnverifiedControllerAuthority)
    );
    assert_eq!(count(&f, "aws_infrastructure_accounts").await, 0);
    assert_eq!(http.calls.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn account_and_binding_idempotency_conflicts_and_scope_are_enforced() {
    let (f, _, _, e, i, _) = setup().await;
    registered(&i, &e).await;
    assert_eq!(i.enroll_account(&af::enrollment(&e)).await.unwrap(), 1);
    i.record_execution(&af::registration(&e)).await.unwrap();
    let mut r = af::registration(&e);
    r.receipt_reference = synthetic::e("changed-receipt");
    assert_eq!(
        i.record_execution(&r).await,
        Err(AwsIdentityError::Conflict)
    );
    let mut bad = e.lease();
    bad.epoch += 1;
    assert_eq!(
        i.inspect(&bad, &RequestId::new("wrong-epoch").unwrap())
            .await,
        Err(AwsIdentityError::Integrity)
    );
    bad = e.lease();
    bad.task.plan.project_id = ProjectId::new("different-project").unwrap();
    assert_eq!(
        i.inspect(&bad, &RequestId::new("wrong-project").unwrap())
            .await,
        Err(AwsIdentityError::UnknownExecution)
    );
    assert_eq!(count(&f, "aws_execution_bindings").await, 1);
}
#[tokio::test]
async fn real_https_rejects_copied_aws_metadata_and_old_synthetic_route_without_issuance() {
    let (f, mut s, v, e, i, http) = setup().await;
    registered(&i, &e).await;
    // A private synthetic provider provisions only the challenge prerequisite.
    // The actual HTTPS claim below uses the concrete AWS adapter, never that signer.
    let (ticket, secret, proof, binding, id) = f.prepare(&s, &v, "task-a").await;
    let transport = s.transport.clone();
    Arc::get_mut(&mut s).unwrap().security.identity = Arc::new(aws::HttpsIdentity {
        identity: i,
        transport,
    });
    let (url, stop, join) = listen(s.clone()).await;
    let mut c = f.cert.client(&url);
    assert!(
        c.bootstrap(
            id.clone(),
            ticket.id.clone(),
            &secret,
            proof.clone(),
            &binding
        )
        .await
        .is_err()
    );
    assert_eq!(http.calls.load(Ordering::SeqCst), 3);
    assert_eq!(count(&f, "worker_credentials").await, 0);
    let copied = Attestation {
        reference: synthetic::e("copied-task-metadata"),
        statement: serde_json::to_string(&af::AwsFixture::new(&e).task.lock().unwrap().clone())
            .unwrap(),
        signature: "not-AWS-attestation".into(),
    };
    assert!(
        c.bootstrap(
            RequestId::new("copied-claim").unwrap(),
            ticket.id.clone(),
            &secret,
            copied,
            &binding
        )
        .await
        .is_err()
    );
    assert_eq!(count(&f, "worker_credentials").await, 0);
    assert_eq!(count(&f, "task_attempts").await, 2);
    stop.send(()).unwrap();
    join.await.unwrap();
}
#[tokio::test]
async fn concurrent_registration_and_inspection_apply_at_most_one_record() {
    let (f, _, _, e, i, _) = setup().await;
    let a = af::enrollment(&e);
    let (x, y) = tokio::join!(i.enroll_account(&a), i.enroll_account(&a));
    assert_eq!(x.unwrap(), 1);
    assert_eq!(y.unwrap(), 1);
    let b = af::registration(&e);
    let (x, y) = tokio::join!(i.record_execution(&b), i.record_execution(&b));
    x.unwrap();
    y.unwrap();
    let r = RequestId::new("concurrent-inspection").unwrap();
    let lease = e.lease();
    let (x, y) = tokio::join!(i.inspect(&lease, &r), i.inspect(&lease, &r));
    assert!(x.is_ok() || y.is_ok());
    for result in [x, y] {
        if let Err(err) = result {
            assert_eq!(err, AwsIdentityError::Conflict);
        }
    }
    assert_eq!(count(&f, "aws_infrastructure_account_versions").await, 1);
    assert_eq!(count(&f, "aws_execution_bindings").await, 1);
    assert_eq!(count(&f, "aws_task_inspections").await, 1);
}
#[tokio::test]
async fn revocation_during_sdk_io_rejects_old_observation_without_holding_sqlite_writer() {
    let (f, _, _, e, i, http) = setup().await;
    registered(&i, &e).await;
    let pause = Arc::new((tokio::sync::Notify::new(), tokio::sync::Notify::new()));
    *http.pause.lock().unwrap() = Some(pause.clone());
    let pending = {
        let i = i.clone();
        let l = e.lease();
        tokio::spawn(async move { i.inspect(&l, &RequestId::new("in-flight").unwrap()).await })
    };
    pause.0.notified().await;
    let mut revoke = af::enrollment(&e);
    revoke.expected_version = 1;
    revoke.request_id = RequestId::new("revoke-infrastructure").unwrap();
    revoke.revoked = true;
    assert_eq!(i.enroll_account(&revoke).await.unwrap(), 2);
    pause.1.notify_one();
    assert_eq!(pending.await.unwrap(), Err(AwsIdentityError::Conflict));
    assert_eq!(count(&f, "aws_task_inspections").await, 0);
    assert_eq!(count(&f, "task_attempts").await, 2);
    assert_eq!(count(&f, "worker_credentials").await, 0);
}
#[tokio::test]
async fn delayed_running_cannot_replace_newer_stopped_observation() {
    let (f, _, _, e, i, http) = setup().await;
    registered(&i, &e).await;
    let pause = Arc::new((tokio::sync::Notify::new(), tokio::sync::Notify::new()));
    *http.pause.lock().unwrap() = Some(pause.clone());
    let pending = {
        let i = i.clone();
        let l = e.lease();
        tokio::spawn(async move { i.inspect(&l, &RequestId::new("old-running").unwrap()).await })
    };
    pause.0.notified().await;
    *http.pause.lock().unwrap() = None;
    http.task.lock().unwrap()["lastStatus"] = serde_json::json!("STOPPED");
    let stopped = i
        .inspect(&e.lease(), &RequestId::new("new-stopped").unwrap())
        .await
        .unwrap();
    assert_eq!(
        stopped.worker_issuance(),
        Err(AwsIdentityError::TaskStopped)
    );
    pause.1.notify_one();
    assert_eq!(pending.await.unwrap(), Err(AwsIdentityError::Conflict));
    assert_eq!(count(&f, "aws_task_inspections").await, 1);
}
#[tokio::test]
async fn fenced_and_expired_replays_never_restore_authority_or_release_holds() {
    let (f, _, _, e, i, _) = setup().await;
    registered(&i, &e).await;
    let r = i
        .inspect(&e.lease(), &RequestId::new("prior-inspection").unwrap())
        .await
        .unwrap();
    let before = f
        .store
        .financial_snapshot(&e.binding.task.plan.project_id)
        .await
        .unwrap();
    f.store
        .fence_attempt(
            &FenceRequest {
                lease: e.lease(),
                request_id: RequestId::new("fence-aws").unwrap(),
                actor: common::owner(),
                evidence: synthetic::e("synthetic-stop-on-risk"),
            },
            f.policy.as_ref(),
            f.clock.as_ref(),
        )
        .await
        .unwrap();
    assert_eq!(
        i.inspect(&e.lease(), &r.request_id).await,
        Err(AwsIdentityError::Revoked)
    );
    let later = i
        .inspect(
            &e.lease(),
            &RequestId::new("fenced-reconciliation").unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(later.worker_issuance(), Err(AwsIdentityError::Revoked));
    assert_eq!(
        f.store
            .financial_snapshot(&e.binding.task.plan.project_id)
            .await
            .unwrap(),
        before
    );
}
#[tokio::test]
async fn inspection_tables_are_immutable_scoped_and_mac_verified() {
    let (f, _, _, e, i, _) = setup().await;
    registered(&i, &e).await;
    i.inspect(&e.lease(), &RequestId::new("immutable-inspection").unwrap())
        .await
        .unwrap();
    for table in [
        "aws_infrastructure_account_versions",
        "aws_execution_bindings",
        "aws_task_inspections",
    ] {
        assert!(
            sqlx::query(&format!("DELETE FROM {table}"))
                .execute(&f.store.pool)
                .await
                .is_err()
        );
        assert!(
            sqlx::query(&format!("UPDATE {table} SET record_mac=zeroblob(32)"))
                .execute(&f.store.pool)
                .await
                .is_err()
        );
    }
    // Disposable damaged-copy simulation: remove one guard, corrupt only the MAC.
    sqlx::query("DROP TRIGGER aws_inspection_no_update")
        .execute(&f.store.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE aws_task_inspections SET record_mac=zeroblob(32)")
        .execute(&f.store.pool)
        .await
        .unwrap();
    assert_eq!(
        i.inspect(&e.lease(), &RequestId::new("after-corruption").unwrap())
            .await,
        Err(AwsIdentityError::Integrity)
    );
}
#[tokio::test]
async fn sqlite_abort_rolls_back_account_binding_and_observation_publication() {
    for stage in ["account", "binding", "inspection"] {
        let (f, _, _, e, mut i, _) = setup().await;
        if stage != "account" {
            i.enroll_account(&af::enrollment(&e)).await.unwrap();
        }
        if stage == "inspection" {
            i.record_execution(&af::registration(&e)).await.unwrap();
        }
        af::checkpoint(&mut i, stage, false, false);
        let result = match stage {
            "account" => i.enroll_account(&af::enrollment(&e)).await.map(|_| ()),
            "binding" => i.record_execution(&af::registration(&e)).await,
            _ => i
                .inspect(&e.lease(), &RequestId::new("aborted-inspection").unwrap())
                .await
                .map(|_| ()),
        };
        assert_eq!(result, Err(AwsIdentityError::Storage));
        let table = match stage {
            "account" => "aws_infrastructure_accounts",
            "binding" => "aws_execution_bindings",
            _ => "aws_task_inspections",
        };
        assert_eq!(count(&f, table).await, 0);
        f.store.check_integrity().await.unwrap();
        assert_eq!(count(&f, "worker_credentials").await, 0);
    }
}
#[tokio::test]
async fn old_receipts_expire_and_unknown_provider_observations_preserve_financial_holds() {
    let (f, _, _, e, i, http) = setup().await;
    registered(&i, &e).await;
    let before = f
        .store
        .financial_snapshot(&e.binding.task.plan.project_id)
        .await
        .unwrap();
    let id = RequestId::new("short-observation").unwrap();
    i.inspect(&e.lease(), &id).await.unwrap();
    f.clock.advance_to(synthetic::T + 15).unwrap();
    assert_eq!(
        i.inspect(&e.lease(), &id).await,
        Err(AwsIdentityError::ExpiredEvidence)
    );
    http.task.lock().unwrap()["lastStatus"] = serde_json::json!("PROVIDER_UNKNOWN");
    let r = i
        .inspect(&e.lease(), &RequestId::new("unknown-state").unwrap())
        .await
        .unwrap();
    assert_eq!(r.worker_issuance(), Err(AwsIdentityError::UnknownExecution));
    assert!(r.assessment.unwrap().unknown_financial_exposure);
    assert_eq!(
        f.store
            .financial_snapshot(&e.binding.task.plan.project_id)
            .await
            .unwrap(),
        before
    );
}
#[tokio::test]
async fn separate_concurrent_attempts_cannot_substitute_task_or_inspection_scope() {
    let (f, s, _, a, ia, ha) = setup().await;
    registered(&ia, &a).await;
    let b = expected_for(&f, &s, "task-b").await;
    let hb = af::AwsFixture::new(&b);
    let (ib, _) = af::open(
        &f.bootstrap,
        f.policy.clone(),
        f.clock.clone(),
        &b,
        hb.http(),
    )
    .await;
    ib.record_execution(&af::registration(&b)).await.unwrap();
    let la = a.lease();
    let lb = b.lease();
    let ra = RequestId::new("observe-a").unwrap();
    let rb = RequestId::new("observe-b").unwrap();
    let (ra, rb) = tokio::join!(ia.inspect(&la, &ra), ib.inspect(&lb, &rb));
    assert_eq!(ra.unwrap().attempt, a.binding.attempt_id);
    assert_eq!(rb.unwrap().attempt, b.binding.attempt_id);
    assert_eq!(count(&f, "aws_task_inspections").await, 2);
    *ha.task.lock().unwrap() = hb.task.lock().unwrap().clone();
    let swapped = ia
        .inspect(&la, &RequestId::new("swapped-task").unwrap())
        .await
        .unwrap();
    assert_eq!(swapped.assessment, Err(AwsIdentityError::ScopeMismatch));
    assert_eq!(count(&f, "worker_credentials").await, 0);
}
#[tokio::test]
async fn consistent_snapshot_retains_inspection_identity_and_encrypted_credential_reference() {
    let (f, _, _, e, i, http) = setup().await;
    registered(&i, &e).await;
    let id = RequestId::new("snapshot-observation").unwrap();
    let r = i.inspect(&e.lease(), &id).await.unwrap();
    let snapshot = f.root.path().join("data/snapshot.sqlite");
    f.store.snapshot(&snapshot).await.unwrap();
    let restored = PlanStore::open(&snapshot).await.unwrap();
    restored.check_integrity().await.unwrap();
    let bootstrap = ControllerBootstrap::open(
        restored,
        f.root.path().join("keys").as_path(),
        ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let (other, _) = af::open(
        &bootstrap,
        f.policy.clone(),
        f.clock.clone(),
        &e,
        http.http(),
    )
    .await;
    assert_eq!(other.inspect(&e.lease(), &id).await.unwrap(), r);
    let marker = b"recognizable-synthetic-AWS-inspection-marker-no-live-credential";
    for path in [
        snapshot,
        f.root.path().join("data/state.sqlite"),
        f.root.path().join("data/state.sqlite-wal"),
    ] {
        if path.exists() {
            let bytes = fs::read(path).unwrap();
            assert!(!bytes.windows(marker.len()).any(|v| v == marker));
        }
    }
}
#[tokio::test]
async fn aws_denial_preserves_independently_accepted_r09_artifact_and_frozen_plan() {
    let mut authority = AwsAuthority::default();
    authority.0.behaviors.insert(
        TaskId::new("task-b").unwrap(),
        FakeBehavior::PermanentFailure,
    );
    let f = Fixture::with_authority(authority).await;
    let (s, _) = f.server().await;
    let e = expected_for(&f, &s, "task-a").await;
    let http = af::AwsFixture::new(&e);
    let (i, _) = af::open(
        &f.bootstrap,
        f.policy.clone(),
        f.clock.clone(),
        &e,
        http.http(),
    )
    .await;
    registered(&i, &e).await;
    let run =
        f.co.run(f.policy.as_ref(), f.clock.as_ref(), RunLimits::default())
            .await
            .unwrap();
    assert_eq!(run.snapshot.accepted, 1);
    assert_eq!(run.status, RunStatus::Blocked);
    let before=sqlx::query_as::<_,(Vec<u8>,String)>("SELECT a.content,a.sha256 FROM synthetic_artifacts a JOIN task_attempts t USING(project_id,attempt_id) WHERE t.task_id='task-a'").fetch_one(&f.store.pool).await.unwrap();
    let plan = f
        .store
        .load(
            f.co.plan_key(),
            &synthetic::Authority::default(),
            Limits::default(),
        )
        .await
        .unwrap()
        .revision;
    let outcome = i
        .inspect(&e.lease(), &RequestId::new("after-accepted").unwrap())
        .await
        .unwrap();
    assert!(outcome.worker_issuance().is_err());
    assert_eq!(sqlx::query_as::<_,(Vec<u8>,String)>("SELECT a.content,a.sha256 FROM synthetic_artifacts a JOIN task_attempts t USING(project_id,attempt_id) WHERE t.task_id='task-a'").fetch_one(&f.store.pool).await.unwrap(),before);
    assert_eq!(
        f.store
            .load(
                f.co.plan_key(),
                &synthetic::Authority::default(),
                Limits::default()
            )
            .await
            .unwrap()
            .revision,
        plan
    );
    assert_eq!(count(&f, "task_attempts").await, 2);
    assert_eq!(count(&f, "tasks").await, 4);
    assert_eq!(count(&f, "task_acceptances").await, 1);
}
#[tokio::test]
async fn inspection_foreign_key_requires_the_exact_execution_account_version() {
    let (f, _, _, e, i, _) = setup().await;
    registered(&i, &e).await;
    i.inspect(&e.lease(), &RequestId::new("scoped-inspection").unwrap())
        .await
        .unwrap();
    let mut r = af::enrollment(&e);
    r.expected_version = 1;
    r.request_id = RequestId::new("new-account-version").unwrap();
    i.enroll_account(&r).await.unwrap();
    let result=sqlx::query("INSERT INTO aws_task_inspections SELECT project_id,attempt_id,2,account_ref,2,'wrong-account-version',observed_at,json_set(record,'$.revision',2,'$.account_version',2,'$.request_id','wrong-account-version'),zeroblob(32) FROM aws_task_inspections WHERE revision=1").execute(&f.store.pool).await;
    assert!(
        result.is_err(),
        "inspection must reference the account version bound to its execution"
    );
}
#[derive(Serialize, Deserialize)]
struct CrashConfig {
    database: String,
    keys: String,
    expected: ExpectedExecution,
    stage: String,
    after: bool,
}
const AWS_DRIVER: &str = "controller_keys::worker_auth::https::tests::aws::aws_process_fixture";
#[tokio::test]
async fn aws_process_fixture() {
    if std::env::var("ROLVYA_AWS_TEST_PROCESS").as_deref() != Ok("disposable-crash-fixture") {
        return;
    }
    let mut input = String::new();
    std::io::Read::take(std::io::stdin(), 65537)
        .read_to_string(&mut input)
        .unwrap();
    assert!(input.len() <= 65536);
    let c: CrashConfig = serde_json::from_str(&input).unwrap();
    let store = PlanStore::open(Path::new(&c.database)).await.unwrap();
    let bootstrap =
        ControllerBootstrap::open(store, Path::new(&c.keys), ServiceIdentity::current(), &[])
            .await
            .unwrap();
    let registry = provider_accounts::fixture_registry_for(
        &bootstrap,
        synthetic::T,
        AWS_PROVIDER,
        "aws-account-a",
    )
    .await;
    let policy = Arc::new(registry.policy(AwsAuthority::default()));
    let clock = Arc::new(DeterministicClock::new(synthetic::T).unwrap());
    let http = af::AwsFixture::new(&c.expected);
    let (mut i, _) = af::open(&bootstrap, policy, clock, &c.expected, http.http()).await;
    af::checkpoint(&mut i, &c.stage, c.after, true);
    i.enroll_account(&af::enrollment(&c.expected))
        .await
        .unwrap();
    i.record_execution(&af::registration(&c.expected))
        .await
        .unwrap();
    i.inspect(
        &c.expected.lease(),
        &RequestId::new("crash-inspection").unwrap(),
    )
    .await
    .unwrap();
    panic!("expected SIGKILL checkpoint not reached");
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sigkill_before_and_after_durable_aws_commits_reopens_without_grants_or_lost_holds() {
    use std::{os::unix::process::ExitStatusExt, process::Stdio};
    use tokio::{io::AsyncWriteExt, process::Command};
    for stage in ["account", "binding", "inspection"] {
        for after in [false, true] {
            let (f, _, _, e, _, _) = setup().await;
            let before = f
                .store
                .financial_snapshot(&e.binding.task.plan.project_id)
                .await
                .unwrap();
            let config = CrashConfig {
                database: f
                    .root
                    .path()
                    .join("data/state.sqlite")
                    .to_str()
                    .unwrap()
                    .into(),
                keys: f.root.path().join("keys").to_str().unwrap().into(),
                expected: e.clone(),
                stage: stage.into(),
                after,
            };
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", AWS_DRIVER, "--nocapture"])
                .env("ROLVYA_AWS_TEST_PROCESS", "disposable-crash-fixture")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(&serde_json::to_vec(&config).unwrap())
                .await
                .unwrap();
            let status = tokio::time::timeout(std::time::Duration::from_secs(20), child.wait())
                .await
                .expect("bounded disposable crash child")
                .unwrap();
            assert_eq!(
                status.signal(),
                Some(9),
                "test must reach actual SIGKILL, not panic"
            );
            let reopened = PlanStore::open(Path::new(&config.database)).await.unwrap();
            reopened.check_integrity().await.unwrap();
            let counts=sqlx::query_as::<_,(i64,i64,i64)>("SELECT (SELECT count(*) FROM aws_infrastructure_accounts),(SELECT count(*) FROM aws_execution_bindings),(SELECT count(*) FROM aws_task_inspections)").fetch_one(&reopened.pool).await.unwrap();
            assert_eq!(
                counts,
                match (stage, after) {
                    ("account", false) => (0, 0, 0),
                    ("account", true) | ("binding", false) => (1, 0, 0),
                    ("binding", true) | ("inspection", false) => (1, 1, 0),
                    _ => (1, 1, 1),
                }
            );
            assert_eq!(
                reopened
                    .financial_snapshot(&e.binding.task.plan.project_id)
                    .await
                    .unwrap(),
                before
            );
            assert_eq!(count(&f, "worker_credentials").await, 0);
            assert_eq!(count(&f, "task_attempts").await, 2);
        }
    }
}
