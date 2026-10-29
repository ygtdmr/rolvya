use super::*;
use crate::orchestration::*;

async fn project(f: &Fixture, id: &str) -> common::Fixture {
    let mut content = common::plan();
    content.key.project_id = ProjectId::new(id).unwrap();
    for t in &mut content.tasks {
        t.plan = content.key.clone();
        t.acceptance_criteria = vec![ARTIFACT_CRITERION.into()];
    }
    let authority = synthetic::Authority::default();
    let valid = rolvya_plan_validator::validate(&content, &authority, Limits::default()).unwrap();
    f.store
        .create_project(&content.key.project_id, &common::owner())
        .await
        .unwrap();
    let revision = f
        .store
        .save_draft(&valid, None, &common::owner(), &common::OwnerPolicy)
        .await
        .unwrap();
    let fixture = common::Fixture {
        directory: temporary(),
        path: f.path.clone(),
        store: f.store.clone(),
        content,
        revision,
    };
    fixture.activate().await.unwrap();
    f.store
        .apply_financial(
            &FinancialRequest {
                project: fixture.content.key.project_id.clone(),
                request_id: RequestId::new("secondary-budget").unwrap(),
                actor: common::owner(),
                reason: "Scoped synthetic second project; no external spending".into(),
                command: FinancialCommand::Approve {
                    expected_version: 0,
                    new_ceiling: synthetic::money(100),
                    proof: OwnerProof {
                        reference: synthetic::e("synthetic-secondary-budget-proof"),
                        verified_at: SystemClock.now().unwrap(),
                    },
                },
            },
            &synthetic::Finance,
        )
        .await
        .unwrap();
    crate::controller_keys::provider_accounts::fixture_registry(&f.bootstrap, synthetic::T).await;
    fixture
}
async fn secondary(f: &Fixture) -> (WorkerBinding, WorkerCredentialTicket) {
    let p = project(f, "project-b").await;
    let authority = f.policy();
    let lease = f
        .store
        .admit(
            &admission::AdmissionRequest {
                task: TaskRef {
                    plan: p.content.key.clone(),
                    task_id: TaskId::new("task-a").unwrap(),
                },
                fingerprint: p.revision.fingerprint().into(),
                expected_epoch: 0,
                request_id: RequestId::new("worker-attempt-b").unwrap(),
                actor: common::owner(),
                permit: admission::ExecutionPermit {
                    id: synthetic::e("synthetic-b-admission"),
                    provider: synthetic::e("synthetic"),
                    account: synthetic::e("synthetic-account-b"),
                    pool: synthetic::e("synthetic-pool"),
                    executor: crate::PrincipalId::new("synthetic-worker-b").unwrap(),
                    maximum: synthetic::costs(),
                    valid_until: synthetic::T + 3600,
                },
            },
            &authority,
            f.clock.as_ref(),
            Limits::default(),
        )
        .await
        .unwrap()
        .lease;
    let dispatcher = OutboxDispatcher::new(f.store.clone());
    let t = dispatcher
        .claim(
            &lease,
            &common::owner(),
            &authority,
            f.clock.as_ref(),
            Limits::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        dispatcher
            .dispatch(
                &t,
                &SyntheticProvider::new(f.store.clone()),
                &authority,
                f.clock.as_ref(),
                Limits::default()
            )
            .await
            .unwrap(),
        DispatchState::Active
    );
    let binding = DurableWorkerController::new(f.store.clone())
        .binding(&lease)
        .await
        .unwrap();
    let digest: String = sqlx::query_scalar(
        "SELECT digest FROM dispatch_operations WHERE project_id=? AND attempt_id=?",
    )
    .bind(binding.task.plan.project_id.as_str())
    .bind(binding.attempt_id.as_str())
    .fetch_one(&f.store.pool)
    .await
    .unwrap();
    let req = PrepareBootstrap {
        lease,
        request_id: RequestId::new("prepare-b").unwrap(),
        evidence: synthetic::e("b-controller-evidence"),
    };
    let grant = |stage,
                 request: RequestId,
                 evidence: EvidenceRef,
                 challenge: Option<BootstrapChallengeId>| Allowed {
        controller: f.security.controller(),
        stage,
        request,
        evidence,
        binding: binding.clone(),
        digest: digest.clone(),
        challenge,
        credential: None,
        account: synthetic::e("synthetic-account-b"),
    };
    f.identity.0.lock().unwrap().insert(
        req.request_id.as_str().into(),
        grant(
            VerificationStage::Prepare,
            req.request_id.clone(),
            req.evidence.clone(),
            None,
        ),
    );
    let boot = f.security.prepare_bootstrap(&req).await.unwrap();
    let c = secondary_context(f, &binding, "claim-b", WorkerOperation::Register);
    let e = synthetic::e("b-independent-bootstrap");
    f.identity.0.lock().unwrap().insert(
        c.request_id.as_str().into(),
        grant(
            VerificationStage::Bootstrap,
            c.request_id.clone(),
            e.clone(),
            Some(boot.id.clone()),
        ),
    );
    let ticket = f
        .security
        .claim_bootstrap(BootstrapClaim {
            id: &boot.id,
            secret: boot.secret.as_ref().unwrap(),
            evidence: &e,
            context: &c,
        })
        .await
        .unwrap();
    (binding, ticket)
}
fn secondary_context(
    f: &Fixture,
    b: &WorkerBinding,
    id: &str,
    op: WorkerOperation,
) -> WorkerRequestContext {
    let c = WorkerRequestContext {
        request_id: RequestId::new(id).unwrap(),
        transport_evidence: transport_evidence(id),
    };
    f.transport
        .0
        .lock()
        .unwrap()
        .insert(id.into(), (b.clone(), op));
    c
}
async fn send(
    f: &Fixture,
    t: &WorkerCredentialTicket,
    b: &WorkerBinding,
    i: u64,
    body: WorkerBody,
) -> Result<ControllerMessage, WorkerAuthError> {
    let mut m = f.message(i, body);
    m.binding = b.clone();
    let op = crate::worker_simulation::controller::operation(&m.body);
    let c = secondary_context(f, b, &format!("secondary-{i}"), op);
    f.security
        .receive(&t.secret, &encode_worker_message(&m).unwrap(), &c)
        .await
}
#[tokio::test]
async fn account_project_and_worker_tokens_are_not_interchangeable() {
    let f = Fixture::new().await;
    let a = f.issued().await;
    let (b, bt) = secondary(&f).await;
    assert!(
        send(
            &f,
            &a,
            &b,
            1,
            WorkerBody::Register {
                capabilities: synthetic::caps()
            }
        )
        .await
        .is_err()
    );
    assert!(
        f.send(
            &bt,
            &f.message(
                1,
                WorkerBody::Register {
                    capabilities: synthetic::caps()
                }
            )
        )
        .await
        .is_err()
    );
    f.send(
        &a,
        &f.message(
            1,
            WorkerBody::Register {
                capabilities: synthetic::caps(),
            },
        ),
    )
    .await
    .unwrap();
    send(
        &f,
        &bt,
        &b,
        1,
        WorkerBody::Register {
            capabilities: synthetic::caps(),
        },
    )
    .await
    .unwrap();
    assert_eq!(f.rows("worker_enrollments").await, 2);
    assert_eq!(f.rows("worker_messages").await, 2);
}
#[tokio::test]
async fn revoking_one_worker_preserves_an_independent_account_and_project() {
    let f = Fixture::new().await;
    let a = f.issued().await;
    f.executing(&a).await;
    let (b, bt) = secondary(&f).await;
    for (i, body) in [
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
        send(&f, &bt, &b, i, body).await.unwrap();
    }
    let before = f
        .store
        .financial_snapshot(&b.task.plan.project_id)
        .await
        .unwrap();
    f.security
        .revoke_worker(&f.revoke_request(&a, "incident-a"))
        .await
        .unwrap();
    f.clock.advance_to(synthetic::T + 30).unwrap();
    send(&f, &bt, &b, 4, WorkerBody::Heartbeat {})
        .await
        .unwrap();
    assert_eq!(
        f.store
            .financial_snapshot(&b.task.plan.project_id)
            .await
            .unwrap()
            .held,
        before.held
    );
    assert!(
        f.send(&a, &f.message(4, WorkerBody::Heartbeat {}))
            .await
            .is_err()
    );
}
#[tokio::test]
async fn worker_containment_preserves_real_human_sessions_without_granting_human_identity() {
    let f = Fixture::new().await;
    let t = f.issued().await;
    f.executing(&t).await;
    let human=crate::controller_keys::human_auth::sessions::SyntheticSessionProbe::on_existing_installation(f.store.clone(),f.path.clone(),f.keys.clone()).await;
    let before = human.inspect().await.unwrap();
    assert!(human.rejects_worker_bearer(t.secret.0.to_string()).await);
    f.security
        .revoke_worker(&f.revoke_request(&t, "worker-only-incident"))
        .await
        .unwrap();
    let after = human.inspect().await.unwrap();
    assert_eq!(before.id, after.id);
    assert_eq!(before.user, after.user);
    assert_eq!(before.generation, after.generation);
    assert_eq!(f.rows("human_memberships").await, 0);
    assert_eq!(f.rows("human_project_bindings").await, 0);
}
#[tokio::test]
async fn independently_accepted_artifact_bytes_survive_unrelated_worker_revocation_and_restart() {
    let f = Fixture::new().await;
    let p = project(&f, "verified-project").await;
    let authority = f.policy();
    let co = synthetic::open(&p, &authority).await;
    let clock = DeterministicClock::new(synthetic::T).unwrap();
    assert_eq!(
        co.run(&authority, &clock, RunLimits::default())
            .await
            .unwrap()
            .status,
        RunStatus::Complete
    );
    f.clock.advance_to(clock.now().unwrap()).unwrap();
    let artifact: Vec<u8> = sqlx::query_scalar("SELECT content FROM synthetic_artifacts LIMIT 1")
        .fetch_one(&f.store.pool)
        .await
        .unwrap();
    assert!(!artifact.is_empty());
    let digest = Sha256::digest(&artifact);
    let t = f.issued().await;
    f.executing(&t).await;
    f.security
        .revoke_worker(&f.revoke_request(&t, "incident"))
        .await
        .unwrap();
    let reopened = f.reopened().await;
    reopened.store.check_integrity().await.unwrap();
    let preserved: Vec<u8> = sqlx::query_scalar("SELECT content FROM synthetic_artifacts LIMIT 1")
        .fetch_one(&reopened.store.pool)
        .await
        .unwrap();
    assert_eq!(preserved, artifact);
    assert_eq!(Sha256::digest(&preserved), digest);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM task_acceptances WHERE project_id='verified-project'"
        )
        .fetch_one(&reopened.store.pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM task_runtime WHERE project_id='verified-project'"
        )
        .fetch_one(&reopened.store.pool)
        .await
        .unwrap(),
        "succeeded"
    );
}
#[tokio::test]
async fn consistent_snapshot_preserves_authenticated_state_but_does_not_activate_restore_authority()
{
    let f = Fixture::new().await;
    let t = f.issued().await;
    let dir = f.root.path().join("restore-data");
    fs::create_dir(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
    let path = dir.join("snapshot.sqlite");
    f.store.snapshot(&path).await.unwrap();
    let restored = PlanStore::open(&path).await.unwrap();
    restored.check_integrity().await.unwrap();
    let bootstrap =
        ControllerBootstrap::open(restored.clone(), &f.keys, ServiceIdentity::current(), &[])
            .await
            .unwrap();
    let mut security = WorkerSecurity::open(&bootstrap).await.unwrap();
    security.clock = f.clock.clone();
    let mut read = restored.pool.begin().await.unwrap();
    let record = security
        .enrollment(&mut read, &t.metadata.enrollment)
        .await
        .unwrap();
    read.commit().await.unwrap();
    assert_eq!(record.scope.binding, f.binding);
    let c = f.context("restore-blocked", WorkerOperation::Register);
    assert!(
        security
            .receive(
                &t.secret,
                &encode_worker_message(&f.message(
                    1,
                    WorkerBody::Register {
                        capabilities: synthetic::caps()
                    }
                ))
                .unwrap(),
                &c
            )
            .await
            .is_err()
    );
    let wrong = Fixture::new().await;
    let mismatched = ControllerBootstrap::open(
        restored.clone(),
        &wrong.keys,
        ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    assert!(WorkerSecurity::open(&mismatched).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_credentials")
            .fetch_one(&restored.pool)
            .await
            .unwrap(),
        1
    );
}
