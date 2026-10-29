#![cfg(target_os = "linux")]
use super::*;
use crate::{
    controller_keys::{InitializationRequest, LocalSetupAuthorization, ServiceIdentity},
    finance::*,
    resilience::{common, synthetic},
    worker_control::*,
    worker_simulation::*,
};
use rolvya_domain::{ids::*, plan::*};
use rolvya_plan_validator::Limits;
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Mutex,
};
fn temporary() -> tempfile::TempDir {
    let tmp = std::env::temp_dir();
    let uid = rustix::process::geteuid().as_raw();
    let mut base = tmp.as_path();
    if ![0, uid].contains(&fs::metadata(base).unwrap().uid()) {
        base = Path::new(env!("CARGO_MANIFEST_DIR"));
        while !base.join(".git").exists() {
            base = base.parent().unwrap();
        }
        base = base.parent().unwrap();
    }
    tempfile::tempdir_in(base).unwrap()
}
fn transport_evidence(id: &str) -> EvidenceRef {
    synthetic::e(&format!(
        "transport-{}",
        &format!("{:x}", Sha256::digest(id.as_bytes()))[..40]
    ))
}
struct SyntheticSetup;
impl LocalSetupAuthorization for SyntheticSetup {
    fn permits(&self, r: &InitializationRequest) -> bool {
        r.request_id().as_str() == "worker-test-setup"
    }
}
struct Allowed {
    controller: ControllerIdentity,
    stage: VerificationStage,
    request: RequestId,
    evidence: EvidenceRef,
    binding: WorkerBinding,
    digest: String,
    challenge: Option<BootstrapChallengeId>,
    credential: Option<WorkerCredentialMetadata>,
    account: EvidenceRef,
}
impl Allowed {
    fn matches(&self, c: &IdentityCheck<'_>) -> bool {
        self.controller == *c.controller
            && self.stage == c.stage
            && self.request == *c.request
            && self.evidence == *c.evidence
            && self.binding == *c.binding
            && self.digest == c.command_digest
            && self.challenge.as_ref() == c.challenge
            && self.credential.as_ref() == c.credential
            && c.provider.as_str() == "synthetic"
            && *c.account == self.account
            && c.pool.as_str() == "synthetic-pool"
    }
}
#[derive(Default)]
struct SyntheticIdentity(
    Mutex<BTreeMap<String, Allowed>>,
    Mutex<Option<VerifiedWorkerIdentity>>,
);
impl SyntheticIdentity {
    fn claims(&self, c: &IdentityCheck<'_>) -> VerifiedWorkerIdentity {
        self.1
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| VerifiedWorkerIdentity {
                subject: synthetic::e("synthetic-enrolled-remote-worker"),
                execution: synthetic::e(&format!(
                    "synthetic-execution-{}",
                    c.binding.attempt_id.as_str()
                )),
                capabilities: synthetic::caps(),
                permissions: vec![
                    WorkerPermission::Register,
                    WorkerPermission::Claim,
                    WorkerPermission::AcceptLease,
                    WorkerPermission::Heartbeat,
                    WorkerPermission::Progress,
                    WorkerPermission::SubmitResult,
                    WorkerPermission::StopQuery,
                    WorkerPermission::StopAck,
                    WorkerPermission::Renew,
                ],
                valid_until: synthetic::T + 3600,
            })
    }
}
impl IdentityAuthority for SyntheticIdentity {
    fn verify<'a>(&'a self, c: &'a IdentityCheck<'a>, now: i64) -> VerificationFuture<'a> {
        Box::pin(async move {
            let map = self.0.lock().unwrap();
            let a = map.get(c.request.as_str()).ok_or(WorkerAuthError::Denied)?;
            if !a.matches(c) {
                return Err(WorkerAuthError::Denied);
            }
            Ok(IdentityGrant {
                identity: self.claims(c),
                scope: c.scope(),
                not_before: now,
                expires_at: now + 60,
            })
        })
    }
    fn current(
        &self,
        c: &IdentityCheck<'_>,
        g: &IdentityGrant,
        now: i64,
    ) -> Result<(), WorkerAuthError> {
        if now < g.not_before
            || now >= g.expires_at
            || g.identity != self.claims(c)
            || !self
                .0
                .lock()
                .unwrap()
                .get(c.request.as_str())
                .is_some_and(|a| a.matches(c))
        {
            return Err(WorkerAuthError::Denied);
        }
        Ok(())
    }
}
#[derive(Default)]
struct SyntheticTransport(Mutex<BTreeMap<String, (WorkerBinding, WorkerOperation)>>);
#[derive(Default)]
struct SyntheticRevocation(
    Mutex<BTreeMap<String, (WorkerEnrollmentId, i64, EvidenceRef, WorkerBinding)>>,
);
impl RevocationAuthority for SyntheticRevocation {
    fn verify(
        &self,
        r: &WorkerRevocation,
        record: &EnrollmentRecord,
        _: i64,
    ) -> Result<(), WorkerAuthError> {
        if self
            .0
            .lock()
            .unwrap()
            .get(r.request_id.as_str())
            .is_some_and(|(id, epoch, evidence, b)| {
                *id == r.enrollment
                    && *epoch == r.expected_security_epoch
                    && *evidence == r.evidence
                    && *b == record.scope.binding
            })
        {
            Ok(())
        } else {
            Err(WorkerAuthError::Denied)
        }
    }
}
impl WorkerTransport for SyntheticTransport {
    fn verify(
        &self,
        c: &WorkerRequestContext,
        b: &WorkerBinding,
        op: WorkerOperation,
        _: i64,
    ) -> Result<(), WorkerAuthError> {
        if c.transport_evidence == transport_evidence(c.request_id.as_str())
            && self
                .0
                .lock()
                .unwrap()
                .get(c.request_id.as_str())
                .is_some_and(|(expected, operation)| expected == b && *operation == op)
        {
            Ok(())
        } else {
            Err(WorkerAuthError::Denied)
        }
    }
    fn bootstrap(&self, _: &PrepareBootstrap, _: &BootstrapTicket) -> Result<(), WorkerAuthError> {
        Err(WorkerAuthError::Unavailable)
    }
    fn credential(
        &self,
        _: &WorkerRequestContext,
        _: &WorkerCredentialTicket,
    ) -> Result<(), WorkerAuthError> {
        Err(WorkerAuthError::Unavailable)
    }
}
struct Fixture {
    registry: Arc<crate::controller_keys::provider_accounts::ProviderRegistry>,
    root: tempfile::TempDir,
    path: PathBuf,
    keys: PathBuf,
    bootstrap: ControllerBootstrap,
    security: WorkerSecurity,
    store: PlanStore,
    clock: Arc<DeterministicClock>,
    binding: WorkerBinding,
    lease: LeaseRef,
    digest: String,
    identity: Arc<SyntheticIdentity>,
    transport: Arc<SyntheticTransport>,
    revocation: Arc<SyntheticRevocation>,
}
impl Fixture {
    async fn new() -> Self {
        Self::with_provider_limit(None).await
    }
    async fn with_provider_limit(until: Option<i64>) -> Self {
        let root = temporary();
        let data = root.path().join("data");
        let keys = root.path().join("keys");
        for p in [&data, &keys] {
            fs::create_dir(p).unwrap();
            fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let path = data.join("state.sqlite");
        let store = PlanStore::open(&path).await.unwrap();
        let bootstrap =
            ControllerBootstrap::open(store.clone(), &keys, ServiceIdentity::current(), &[])
                .await
                .unwrap();
        bootstrap
            .initialize(
                &InitializationRequest::new(RequestId::new("worker-test-setup").unwrap()),
                &SyntheticSetup,
            )
            .await
            .unwrap();
        let content = common::plan();
        let policy = synthetic::Authority::default();
        let validated =
            rolvya_plan_validator::validate(&content, &policy, Limits::default()).unwrap();
        store
            .create_project(&content.key.project_id, &common::owner())
            .await
            .unwrap();
        let revision = store
            .save_draft(&validated, None, &common::owner(), &common::OwnerPolicy)
            .await
            .unwrap();
        store
            .activate(
                &revision,
                &common::request(),
                &common::owner(),
                &common::OwnerPolicy,
                &policy,
                Limits::default(),
            )
            .await
            .unwrap();
        store
            .apply_financial(
                &FinancialRequest {
                    project: content.key.project_id.clone(),
                    request_id: RequestId::new("worker-test-budget").unwrap(),
                    actor: common::owner(),
                    reason: "Scoped synthetic budget; no provider spending".into(),
                    command: FinancialCommand::Approve {
                        expected_version: 0,
                        new_ceiling: synthetic::money(100),
                        proof: OwnerProof {
                            reference: synthetic::e("synthetic-initial-budget"),
                            verified_at: SystemClock.now().unwrap(),
                        },
                    },
                },
                &synthetic::Finance,
            )
            .await
            .unwrap();
        let registry =
            crate::controller_keys::provider_accounts::fixture_registry(&bootstrap, synthetic::T)
                .await;
        if let Some(until) = until {
            crate::controller_keys::provider_accounts::fixture_limit(
                &registry,
                &content.key.project_id,
                until,
            )
            .await;
        }
        let policy = registry.policy(policy);
        let clock = Arc::new(DeterministicClock::new(synthetic::T).unwrap());
        let lease = store
            .admit(
                &admission::AdmissionRequest {
                    task: TaskRef {
                        plan: content.key,
                        task_id: TaskId::new("task-a").unwrap(),
                    },
                    fingerprint: revision.fingerprint().into(),
                    expected_epoch: 0,
                    request_id: RequestId::new("worker-attempt-a").unwrap(),
                    actor: common::owner(),
                    permit: admission::ExecutionPermit {
                        id: synthetic::e("synthetic-admission-evidence"),
                        provider: synthetic::e("synthetic"),
                        account: synthetic::e("synthetic-account"),
                        pool: synthetic::e("synthetic-pool"),
                        executor: crate::PrincipalId::new("synthetic-worker").unwrap(),
                        maximum: synthetic::costs(),
                        valid_until: synthetic::T + 3600,
                    },
                },
                &policy,
                clock.as_ref(),
                Limits::default(),
            )
            .await
            .unwrap()
            .lease;
        let dispatcher = OutboxDispatcher::new(store.clone());
        let ticket = dispatcher
            .claim(
                &lease,
                &common::owner(),
                &policy,
                clock.as_ref(),
                Limits::default(),
            )
            .await
            .unwrap();
        assert_eq!(
            dispatcher
                .dispatch(
                    &ticket,
                    &SyntheticProvider::new(store.clone()),
                    &policy,
                    clock.as_ref(),
                    Limits::default()
                )
                .await
                .unwrap(),
            DispatchState::Active
        );
        let binding = DurableWorkerController::new(store.clone())
            .binding(&lease)
            .await
            .unwrap();
        let digest = sqlx::query_scalar(
            "SELECT digest FROM dispatch_operations WHERE project_id=? AND attempt_id=?",
        )
        .bind(lease.task.plan.project_id.as_str())
        .bind(lease.attempt_id.as_str())
        .fetch_one(&store.pool)
        .await
        .unwrap();
        let mut security = WorkerSecurity::open(&bootstrap).await.unwrap();
        let identity = Arc::new(SyntheticIdentity::default());
        let transport = Arc::new(SyntheticTransport::default());
        let revocation = Arc::new(SyntheticRevocation::default());
        security.identity = identity.clone();
        security.transport = transport.clone();
        security.policy = Arc::new(policy);
        security.clock = clock.clone();
        security.revocation = revocation.clone();
        Self {
            registry,
            root,
            path,
            keys,
            bootstrap,
            security,
            store,
            clock,
            binding,
            lease,
            digest,
            identity,
            transport,
            revocation,
        }
    }
    fn policy(
        &self,
    ) -> crate::controller_keys::provider_accounts::RegistryPolicy<synthetic::Authority> {
        self.registry.policy(synthetic::Authority::default())
    }
    fn request(&self, id: &str) -> PrepareBootstrap {
        let r = PrepareBootstrap {
            lease: self.lease.clone(),
            request_id: RequestId::new(id).unwrap(),
            evidence: synthetic::e(&format!("attestation-{id}")),
        };
        self.identity.0.lock().unwrap().insert(
            id.into(),
            Allowed {
                account: synthetic::e("synthetic-account"),
                controller: self.security.controller(),
                stage: VerificationStage::Prepare,
                request: r.request_id.clone(),
                evidence: r.evidence.clone(),
                binding: self.binding.clone(),
                digest: self.digest.clone(),
                challenge: None,
                credential: None,
            },
        );
        r
    }
    fn context(&self, id: &str, op: WorkerOperation) -> WorkerRequestContext {
        let c = WorkerRequestContext {
            request_id: RequestId::new(id).unwrap(),
            transport_evidence: transport_evidence(id),
        };
        self.transport
            .0
            .lock()
            .unwrap()
            .insert(id.into(), (self.binding.clone(), op));
        c
    }
    fn grant(
        &self,
        stage: VerificationStage,
        c: &WorkerRequestContext,
        evidence: &EvidenceRef,
        challenge: Option<&BootstrapChallengeId>,
        credential: Option<&WorkerCredentialMetadata>,
    ) {
        self.identity.0.lock().unwrap().insert(
            c.request_id.as_str().into(),
            Allowed {
                account: synthetic::e("synthetic-account"),
                controller: self.security.controller(),
                stage,
                request: c.request_id.clone(),
                evidence: evidence.clone(),
                binding: self.binding.clone(),
                digest: self.digest.clone(),
                challenge: challenge.cloned(),
                credential: credential.cloned(),
            },
        );
    }
    async fn issued(&self) -> WorkerCredentialTicket {
        let ticket = self
            .security
            .prepare_bootstrap(&self.request("prepare"))
            .await
            .unwrap();
        let c = self.context("claim", WorkerOperation::Register);
        let evidence = synthetic::e("independent-bootstrap-proof");
        self.grant(
            VerificationStage::Bootstrap,
            &c,
            &evidence,
            Some(&ticket.id),
            None,
        );
        self.security
            .claim_bootstrap(BootstrapClaim {
                id: &ticket.id,
                secret: ticket.secret.as_ref().unwrap(),
                evidence: &evidence,
                context: &c,
            })
            .await
            .unwrap()
    }
    fn message(&self, sequence: u64, body: WorkerBody) -> WorkerMessage {
        WorkerMessage {
            protocol_version: PROTOCOL_VERSION,
            message_id: MessageId::new(format!("message-{sequence}")).unwrap(),
            binding: self.binding.clone(),
            sequence,
            body,
        }
    }
    async fn send(
        &self,
        credential: &WorkerCredentialTicket,
        message: &WorkerMessage,
    ) -> Result<ControllerMessage, WorkerAuthError> {
        let op = crate::worker_simulation::controller::operation(&message.body);
        let c = self.context(&format!("send-{}", message.message_id.as_str()), op);
        self.security
            .receive(
                &credential.secret,
                &encode_worker_message(message).unwrap(),
                &c,
            )
            .await
    }
    async fn executing(&self, t: &WorkerCredentialTicket) {
        for (i, b) in [
            (
                1,
                WorkerBody::Register {
                    capabilities: synthetic::caps(),
                },
            ),
            (2, WorkerBody::Claim {}),
            (
                3,
                WorkerBody::AcceptLease {
                    offer_id: MessageId::new("message-2").unwrap(),
                },
            ),
        ] {
            self.send(t, &self.message(i, b)).await.unwrap();
        }
    }
    fn revoke_request(&self, t: &WorkerCredentialTicket, id: &str) -> WorkerRevocation {
        let r = WorkerRevocation {
            enrollment: t.metadata.enrollment.clone(),
            expected_security_epoch: 1,
            request_id: RequestId::new(id).unwrap(),
            evidence: synthetic::e(&format!("incident-{id}")),
        };
        self.revocation.0.lock().unwrap().insert(
            id.into(),
            (
                r.enrollment.clone(),
                1,
                r.evidence.clone(),
                self.binding.clone(),
            ),
        );
        r
    }
    async fn reopened(&self) -> WorkerSecurity {
        let store = PlanStore::open(&self.path).await.unwrap();
        let bootstrap =
            ControllerBootstrap::open(store, &self.keys, ServiceIdentity::current(), &[])
                .await
                .unwrap();
        let mut s = WorkerSecurity::open(&bootstrap).await.unwrap();
        s.identity = self.identity.clone();
        s.transport = self.transport.clone();
        s.policy = self.security.policy.clone();
        s.clock = self.clock.clone();
        s.revocation = self.revocation.clone();
        s
    }
    async fn rows(&self, table: &str) -> i64 {
        sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&self.store.pool)
            .await
            .unwrap()
    }
    fn renewal(&self, t: &WorkerCredentialTicket, id: &str) -> (WorkerRequestContext, EvidenceRef) {
        let c = self.context(id, WorkerOperation::RenewCredential);
        let e = synthetic::e(&format!("fresh-identity-{id}"));
        self.grant(VerificationStage::Renew, &c, &e, None, Some(&t.metadata));
        (c, e)
    }
    async fn unknown_exposure(&self) {
        self.store
            .apply_financial(
                &FinancialRequest {
                    project: self.binding.task.plan.project_id.clone(),
                    request_id: RequestId::new("unknown-cost").unwrap(),
                    actor: common::owner(),
                    reason: "Synthetic delayed billing exposure".into(),
                    command: FinancialCommand::Report {
                        liability: LiabilityId::new("uncertain-provider-effect").unwrap(),
                        expected_revision: 0,
                        unreserved: synthetic::zero(),
                        uncertain: true,
                        evidence: synthetic::e("synthetic-unknown-exposure"),
                    },
                },
                &synthetic::Finance,
            )
            .await
            .unwrap();
    }
}
#[tokio::test]
async fn production_constructor_cannot_enroll_a_self_asserted_worker() {
    let f = Fixture::new().await;
    let denied = WorkerSecurity::open(&f.bootstrap).await.unwrap();
    assert!(
        denied
            .prepare_bootstrap(&f.request("untrusted"))
            .await
            .is_err()
    );
}
#[tokio::test]
async fn independently_verified_bootstrap_preparation_persists_one_scoped_challenge() {
    let f = Fixture::new().await;
    let ticket = f
        .security
        .prepare_bootstrap(&f.request("verified"))
        .await
        .expect("trusted synthetic enrollment must persist a challenge");
    assert_eq!(ticket.expires_at, synthetic::T + 300);
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM worker_bootstrap_challenges WHERE challenge_id=?")
            .bind(ticket.id.as_str())
            .fetch_one(&f.store.pool)
            .await
            .unwrap();
    assert_eq!(n, 1);
}
#[tokio::test]
async fn bootstrap_claim_consumes_one_challenge_and_persists_only_one_credential_commitment() {
    let f = Fixture::new().await;
    let ticket = f
        .security
        .prepare_bootstrap(&f.request("prepare"))
        .await
        .unwrap();
    let c = f.context("claim", WorkerOperation::Register);
    let evidence = synthetic::e("independent-bootstrap-proof");
    f.grant(
        VerificationStage::Bootstrap,
        &c,
        &evidence,
        Some(&ticket.id),
        None,
    );
    let claim = || BootstrapClaim {
        id: &ticket.id,
        secret: ticket.secret.as_ref().unwrap(),
        evidence: &evidence,
        context: &c,
    };
    let credential = f
        .security
        .claim_bootstrap(claim())
        .await
        .expect("verified one-use bootstrap must issue a credential");
    assert_eq!(credential.metadata.generation, 1);
    assert_eq!(credential.metadata.binding, f.binding);
    assert_eq!(credential.metadata.expires_at, synthetic::T + 900);
    assert!(f.security.claim_bootstrap(claim()).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_credentials")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        1
    );
    let raw = credential.secret.0.as_bytes();
    let snapshot = f.root.path().join("ordinary-snapshot.sqlite");
    f.store.snapshot(&snapshot).await.unwrap();
    for path in [&f.path, &snapshot] {
        assert!(!fs::read(path).unwrap().windows(raw.len()).any(|w| w == raw));
    }
}
#[tokio::test]
async fn credential_drives_real_durable_worker_lifecycle_without_authorizing_task_success() {
    let f = Fixture::new().await;
    let ticket = f.issued().await;
    for (sequence, body) in [
        (
            1,
            WorkerBody::Register {
                capabilities: synthetic::caps(),
            },
        ),
        (2, WorkerBody::Claim {}),
        (
            3,
            WorkerBody::AcceptLease {
                offer_id: MessageId::new("message-2").unwrap(),
            },
        ),
        (4, WorkerBody::Heartbeat {}),
        (
            5,
            WorkerBody::SubmitResult {
                outcome: ExecutionOutcome::Succeeded {
                    manifest_sha256: rolvya_domain::worker::Digest::new("2".repeat(64)).unwrap(),
                },
            },
        ),
    ] {
        let message = f.message(sequence, body);
        let first = f
            .send(&ticket, &message)
            .await
            .expect("credential must authorize only its real durable lifecycle");
        assert_eq!(f.send(&ticket, &message).await.unwrap(), first);
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_messages")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        5
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_acceptances")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM task_runtime WHERE task_id='task-a'")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        "running"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_attempts")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        1
    );
}
#[tokio::test]
async fn fresh_identity_renewal_rotates_generation_without_renewing_lease_or_budget() {
    let f = Fixture::new().await;
    let first = f.issued().await;
    f.executing(&first).await;
    let before = f
        .store
        .financial_snapshot(&f.binding.task.plan.project_id)
        .await
        .unwrap();
    let until = DurableWorkerController::new(f.store.clone())
        .lease_until(&f.lease)
        .await
        .unwrap();
    f.clock.advance_to(synthetic::T + 60).unwrap();
    let c = f.context("renew-a", WorkerOperation::RenewCredential);
    let e = synthetic::e("fresh-worker-identity-for-renewal");
    f.grant(
        VerificationStage::Renew,
        &c,
        &e,
        None,
        Some(&first.metadata),
    );
    let next = f
        .security
        .renew_credential(&first.secret, &e, &c)
        .await
        .expect("fresh scoped identity must rotate current authority");
    assert_eq!(next.metadata.generation, 2);
    assert_eq!(next.metadata.expires_at, synthetic::T + 960);
    assert_eq!(
        DurableWorkerController::new(f.store.clone())
            .lease_until(&f.lease)
            .await
            .unwrap(),
        until
    );
    assert!(
        f.send(&first, &f.message(4, WorkerBody::Heartbeat {}))
            .await
            .is_err()
    );
    f.send(&next, &f.message(4, WorkerBody::Heartbeat {}))
        .await
        .unwrap();
    let after = f
        .store
        .financial_snapshot(&f.binding.task.plan.project_id)
        .await
        .unwrap();
    assert_eq!(before.ceiling, after.ceiling);
    assert_eq!(before.held, after.held);
}
#[tokio::test]
async fn independently_authorized_revocation_fences_and_preserves_financial_liability() {
    let f = Fixture::new().await;
    let ticket = f.issued().await;
    f.executing(&ticket).await;
    let before = f
        .store
        .financial_snapshot(&f.binding.task.plan.project_id)
        .await
        .unwrap();
    let request = f.revoke_request(&ticket, "revoke");
    let revoked = f
        .security
        .revoke_worker(&request)
        .await
        .expect("verified controller incident must retire scoped worker authority");
    assert_eq!(revoked.fencing_epoch, Some(f.lease.epoch + 1));
    assert_eq!(revoked.security_epoch, 2);
    assert_eq!(f.security.revoke_worker(&request).await.unwrap(), revoked);
    assert!(
        f.send(&ticket, &f.message(4, WorkerBody::Heartbeat {}))
            .await
            .is_err()
    );
    let after = f
        .store
        .financial_snapshot(&f.binding.task.plan.project_id)
        .await
        .unwrap();
    assert_eq!(before.held, after.held);
    assert_eq!(before.ceiling, after.ceiling);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM dispatch_operations")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        "blocked"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM synthetic_operations")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        "active"
    );
}
#[tokio::test]
async fn legacy_worker_callback_cannot_bypass_an_enrolled_credential_boundary() {
    let f = Fixture::new().await;
    let ticket = f.issued().await;
    f.executing(&ticket).await;
    let m = f.message(4, WorkerBody::Heartbeat {});
    let result = DurableWorkerController::new(f.store.clone())
        .receive(
            &PeerEvidence {
                reference: synthetic::peer_ref(&f.binding),
            },
            &encode_worker_message(&m).unwrap(),
            &synthetic::Authority::default(),
            &synthetic::Authority::default(),
            f.clock.as_ref(),
            Limits::default(),
        )
        .await;
    assert!(
        result.is_err(),
        "legacy verifier must not replace durable credential authority"
    );
}
#[tokio::test]
async fn legacy_broker_callback_cannot_treat_control_enrollment_as_secret_permission() {
    let f = Fixture::new().await;
    let ticket = f.issued().await;
    f.executing(&ticket).await;
    let mut tx = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let result = worker_control::broker_guard(
        &mut tx,
        &f.binding,
        &PeerEvidence {
            reference: synthetic::peer_ref(&f.binding),
        },
        &synthetic::Authority::default(),
        &synthetic::Authority::default(),
        synthetic::T,
        "synthetic-connection",
        "synthetic-account",
        "synthetic",
        "api_key",
    )
    .await;
    assert!(
        result.is_err(),
        "provider secret authority must remain independently unavailable"
    );
}
#[tokio::test]
async fn reflected_control_bearer_cannot_be_persisted_as_an_untrusted_message_identity() {
    let f = Fixture::new().await;
    let ticket = f.issued().await;
    f.executing(&ticket).await;
    let mut m = f.message(4, WorkerBody::Heartbeat {});
    m.message_id = MessageId::new(ticket.secret.0.as_str()).unwrap();
    let c = f.context("reflected-message", WorkerOperation::Heartbeat);
    assert!(
        f.security
            .receive(&ticket.secret, &encode_worker_message(&m).unwrap(), &c)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_messages")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        3
    );
}
#[tokio::test]
async fn reflected_control_bearer_cannot_be_persisted_as_a_renewal_request_reference() {
    let f = Fixture::new().await;
    let ticket = f.issued().await;
    f.executing(&ticket).await;
    f.clock.advance_to(synthetic::T + 60).unwrap();
    let c = f.context(&ticket.secret.0, WorkerOperation::RenewCredential);
    let e = synthetic::e("independent-renewal-proof");
    f.grant(
        VerificationStage::Renew,
        &c,
        &e,
        None,
        Some(&ticket.metadata),
    );
    assert!(
        f.security
            .renew_credential(&ticket.secret, &e, &c)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_credentials")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        1
    );
}
#[path = "tests/adversarial.rs"]
mod adversarial;
#[path = "tests/crashes.rs"]
mod crashes;
#[path = "tests/faults.rs"]
mod faults;
#[path = "tests/integration.rs"]
mod integration;
#[path = "tests/migrations.rs"]
mod migrations;
#[tokio::test]
async fn provider_entitlement_retirement_still_delivers_scoped_stop_and_ack_without_financial_release()
 {
    let f = Fixture::new().await;
    let token = f.issued().await;
    f.executing(&token).await;
    sqlx::query("UPDATE provider_policy_revisions SET revision='withdrawn-policy',version=2")
        .execute(&f.store.pool)
        .await
        .unwrap();
    let stop = f
        .send(&token, &f.message(4, WorkerBody::GetStopInstruction {}))
        .await;
    assert!(
        stop.is_ok(),
        "a provider denial must produce a scoped stop instruction: {stop:?}"
    );
    assert!(matches!(stop.unwrap().body, ControllerBody::Stop { .. }));
    f.send(
        &token,
        &f.message(
            5,
            WorkerBody::StopAcknowledged {
                instruction_id: MessageId::new("message-4").unwrap(),
            },
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        f.store
            .financial_snapshot(&f.lease.task.plan.project_id)
            .await
            .unwrap()
            .held,
        synthetic::costs()
    );
    let held: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM provider_capacity_reservations WHERE state='held'",
    )
    .fetch_one(&f.store.pool)
    .await
    .unwrap();
    assert_eq!(held, 1);
}
#[tokio::test]
async fn provider_deadline_caps_initial_lease_and_authenticated_heartbeat_without_renewing_token() {
    let f = Fixture::with_provider_limit(Some(synthetic::T + 35)).await;
    let token = f.issued().await;
    f.executing(&token).await;
    let controller = DurableWorkerController::new(f.store.clone());
    assert_eq!(
        controller.lease_until(&f.lease).await.unwrap(),
        synthetic::T + 35
    );
    let original_expiry = token.metadata.expires_at;
    f.clock.advance_to(synthetic::T + 30).unwrap();
    let c = f.context("provider-bounded-heartbeat", WorkerOperation::Heartbeat);
    f.security
        .receive(
            &token.secret,
            &encode_worker_message(&f.message(4, WorkerBody::Heartbeat {})).unwrap(),
            &c,
        )
        .await
        .unwrap();
    assert_eq!(
        controller.lease_until(&f.lease).await.unwrap(),
        synthetic::T + 35
    );
    assert_eq!(token.metadata.expires_at, original_expiry);
    f.clock.advance_to(synthetic::T + 35).unwrap();
    let c = f.context("provider-expired-heartbeat", WorkerOperation::Heartbeat);
    assert!(
        f.security
            .receive(
                &token.secret,
                &encode_worker_message(&f.message(5, WorkerBody::Heartbeat {})).unwrap(),
                &c
            )
            .await
            .is_err()
    );
    assert_eq!(
        f.store
            .financial_snapshot(&f.binding.task.plan.project_id)
            .await
            .unwrap()
            .held,
        synthetic::costs()
    );
    assert_eq!(f.rows("provider_capacity_reservations").await, 1);
}

mod provider_fixture;
pub(in crate::controller_keys) use provider_fixture::FixtureWorker;
