//! Private protocol-test authority. Absent from every production build.
pub(in crate::controller_keys::worker_auth) use super::super::sdk::tests::{
    AwsFixture, expected, reference,
};
use super::*;
use crate::admission::Clock;
use std::sync::atomic::{AtomicBool, Ordering};
pub(in crate::controller_keys::worker_auth) struct FixtureAuthority {
    expected: ExpectedExecution,
    http: aws_smithy_runtime_api::client::http::SharedHttpClient,
    pub revoked: AtomicBool,
}
impl InfrastructureAuthority for FixtureAuthority {
    fn authorize_in<'a>(
        &'a self,
        _: &'a mut SqliteConnection,
        op: AdministrativeOperation<'a>,
        now: i64,
    ) -> Authorization<'a> {
        Box::pin(async move {
            if self.revoked.load(Ordering::SeqCst)
                || now < self.expected.authorized_at
                || now >= self.expected.authorization_until
            {
                return Err(E::Revoked);
            }
            let allowed = match op {
                AdministrativeOperation::Enroll(r) => {
                    r.project == self.expected.binding.task.plan.project_id
                        && r.account_ref == self.expected.account_ref
                        && r.credential == reference()
                }
                AdministrativeOperation::Bind(r) => r.expected == self.expected,
                AdministrativeOperation::Inspect {
                    account,
                    execution,
                    request: _,
                } => {
                    account.project == self.expected.binding.task.plan.project_id
                        && *execution == self.expected
                }
            };
            if !allowed {
                return Err(E::ScopeMismatch);
            }
            Ok(EvidenceRef::new("private-scoped-aws-protocol-fixture").unwrap())
        })
    }
    fn credentials<'a>(
        &'a self,
        a: &'a AccountRecord,
        e: &'a ExpectedExecution,
        now: i64,
    ) -> CredentialLoad<'a> {
        Box::pin(async move {
            if self.revoked.load(Ordering::SeqCst) || *e != self.expected {
                return Err(E::Revoked);
            }
            Ok(ResolvedCredentials {
                account_ref: a.account_ref.clone(),
                account_version: a.version,
                reference: a.credential.clone(),
                expires_at: now + 60,
                credentials: super::super::sdk::tests::credentials(),
                http: Some(self.http.clone()),
            })
        })
    }
}
pub(in crate::controller_keys::worker_auth) async fn open(
    b: &ControllerBootstrap,
    p: Arc<dyn SchedulerPolicy>,
    clock: Arc<dyn Clock>,
    e: &ExpectedExecution,
    http: aws_smithy_runtime_api::client::http::SharedHttpClient,
) -> (Arc<FargateIdentity>, Arc<FixtureAuthority>) {
    let mut guard = WorkerSecurity::open(b).await.unwrap();
    guard.policy = p;
    guard.clock = clock;
    let scope = credential_scope(&e.binding.task.plan.project_id, &reference()).unwrap();
    let mut tx = guard
        .store
        .pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .unwrap();
    if !sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM secret_connections WHERE project_id=? AND connection_id=?)",
    )
    .bind(scope.connection.project.as_str())
    .bind(scope.connection.connection.as_str())
    .fetch_one(&mut *tx)
    .await
    .unwrap()
    {
        broker::fixture_record(
            &mut tx,
            &guard.key,
            &scope,
            &broker::SecretValue::new(
                b"recognizable-synthetic-AWS-inspection-marker-no-live-credential".to_vec(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    let authority = Arc::new(FixtureAuthority {
        expected: e.clone(),
        http,
        revoked: AtomicBool::new(false),
    });
    (
        Arc::new(FargateIdentity {
            guard,
            authority: authority.clone(),
            hook: Box::new(|_| Ok(())),
        }),
        authority,
    )
}
pub(in crate::controller_keys::worker_auth) fn enrollment(
    e: &ExpectedExecution,
) -> AccountEnrollment {
    AccountEnrollment {
        project: e.binding.task.plan.project_id.clone(),
        account_ref: e.account_ref.clone(),
        request_id: RequestId::new("aws-account-registration").unwrap(),
        expected_version: 0,
        workload: e.workload.clone(),
        credential: reference(),
        consent_reference: EvidenceRef::new("private-synthetic-infrastructure-consent").unwrap(),
        valid_until: e.authorization_until,
        revoked: false,
    }
}
pub(in crate::controller_keys::worker_auth) fn registration(
    e: &ExpectedExecution,
) -> ExecutionRegistration {
    ExecutionRegistration {
        expected: e.clone(),
        request_id: RequestId::new(format!("bind-{}", e.binding.attempt_id.as_str())).unwrap(),
        receipt_reference: EvidenceRef::new("synthetic-provider-receipt-not-attestation").unwrap(),
    }
}
pub(in crate::controller_keys::worker_auth) fn checkpoint(
    i: &mut Arc<FargateIdentity>,
    point: &str,
    after: bool,
    kill: bool,
) {
    let target = match (point, after) {
        ("account", false) => AwsCheckpoint::BeforeAccount,
        ("account", true) => AwsCheckpoint::AfterAccount,
        ("binding", false) => AwsCheckpoint::BeforeBinding,
        ("binding", true) => AwsCheckpoint::AfterBinding,
        ("inspection", false) => AwsCheckpoint::BeforeInspection,
        ("inspection", true) => AwsCheckpoint::AfterInspection,
        _ => panic!("unknown private fault checkpoint"),
    };
    Arc::get_mut(i).unwrap().hook = Box::new(move |p| {
        if p == target {
            if kill {
                rustix::process::kill_process(
                    rustix::process::getpid(),
                    rustix::process::Signal::KILL,
                )
                .unwrap();
                unreachable!();
            }
            Err(E::Storage)
        } else {
            Ok(())
        }
    });
}
