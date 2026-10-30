use super::super::super::aws_fargate::{self as native, secure_fixture, *};
use super::*;
#[tokio::test]
async fn protected_native_possession_uses_real_tls_and_the_existing_one_use_issuer() {
    let (f, mut server, _, expected, _, http) = super::aws::setup().await;
    let enrollment =
        crate::controller_keys::human_auth::sessions::owner::infrastructure_consent_on_existing(
            f.store.clone(),
            f.root.path().join("data/state.sqlite"),
            f.root.path().join("keys"),
            &expected,
        )
        .await;
    let identity = native::reviewed_fixture_open(
        &f.bootstrap,
        f.policy.clone(),
        f.clock.clone(),
        &expected,
        http.http(),
    )
    .await;
    identity
        .verify_account(
            &enrollment,
            &RequestId::new("real-owner-controller-check").unwrap(),
        )
        .await
        .unwrap();
    identity.enroll_account(&enrollment).await.unwrap();
    identity
        .record_execution(&native::fixture::registration(&expected))
        .await
        .unwrap();
    let plan=ProtectedDelivery{format:1,execution:expected.clone(),secret:SecretArn::parse("arn:aws:secretsmanager:eu-central-1:123456789012:secret:rolvya/bootstrap/attempt-a-AbCdEf").unwrap(),version_id:super::super::super::hex(&super::super::super::random::<16>().unwrap()[..]),expires_at:expected.authorization_until};
    let world = secure_fixture::World::new(&plan);
    let service = secure_fixture::service(identity, world.clone());
    let transport = server.transport.clone();
    let s = Arc::get_mut(&mut server).unwrap();
    s.security.identity = Arc::new(native::PossessionIdentity {
        service: service.clone(),
        transport,
    });
    s.secure_fargate = Some(service);
    let request = PrepareBootstrap {
        lease: expected.lease(),
        request_id: RequestId::new("native-prepare").unwrap(),
        evidence: synthetic::e("native-protected-prepare"),
    };
    let challenge = server
        .prepare_fargate_bootstrap(&request, &plan)
        .await
        .unwrap();
    assert_eq!(world.value_count(), 1);
    let delivered = ProtectedDelivery {
        format: 1,
        execution: expected.clone(),
        secret: challenge.secret.clone(),
        version_id: challenge.secret_version.clone(),
        expires_at: challenge.expires_at,
    };
    let material = world
        .read_as_role(&expected.workload.task_role, &delivered, synthetic::T)
        .await
        .unwrap();
    let (url, stop, join) = listen(server).await;
    let mut client = f.cert.client(&url);
    material
        .authenticate(&mut client, RequestId::new("native-claim").unwrap())
        .await
        .unwrap();
    assert_eq!(client.metadata().unwrap().binding, expected.binding);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_credentials")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM aws_protected_challenges")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        "consumed"
    );
    let again = world
        .read_as_role(&expected.workload.task_role, &delivered, synthetic::T)
        .await
        .unwrap();
    let mut second = f.cert.client(&url);
    assert!(
        again
            .authenticate(&mut second, RequestId::new("native-replay").unwrap())
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
    stop.send(()).unwrap();
    join.await.unwrap();
}

struct NativeFixture {
    f: Fixture<super::aws::AwsAuthority>,
    server: Arc<HttpsWorkerServer>,
    expected: ExpectedExecution,
    world: Arc<secure_fixture::World>,
    plan: ProtectedDelivery,
    http: Arc<native::fixture::AwsFixture>,
}
impl NativeFixture {
    async fn new() -> Self {
        let (f, mut server, _, expected, identity, http) = super::aws::setup().await;
        super::aws::registered(&identity, &expected).await;
        let plan=ProtectedDelivery{format:1,execution:expected.clone(),secret:SecretArn::parse("arn:aws:secretsmanager:eu-central-1:123456789012:secret:rolvya/bootstrap/attempt-a-AbCdEf").unwrap(),version_id:new_id().unwrap(),expires_at:expected.authorization_until};
        let world = secure_fixture::World::new(&plan);
        let service = secure_fixture::service(identity, world.clone());
        let state = Arc::get_mut(&mut server).unwrap();
        state.security.identity = Arc::new(native::PossessionIdentity {
            service: service.clone(),
            transport: state.transport.clone(),
        });
        state.secure_fargate = Some(service);
        Self {
            f,
            server,
            expected,
            world,
            plan,
            http,
        }
    }
    async fn prepare(&self) -> PossessionChallenge {
        self.server
            .prepare_fargate_bootstrap(
                &PrepareBootstrap {
                    lease: self.expected.lease(),
                    request_id: RequestId::new("secure-prepare").unwrap(),
                    evidence: synthetic::e("secure-prepare-evidence"),
                },
                &self.plan,
            )
            .await
            .unwrap()
    }
    fn resource(c: &PossessionChallenge) -> ProtectedDelivery {
        ProtectedDelivery {
            format: 1,
            execution: c.execution.clone(),
            secret: c.secret.clone(),
            version_id: c.secret_version.clone(),
            expires_at: c.expires_at,
        }
    }
    async fn material(&self, c: &PossessionChallenge) -> DeliveredPossession {
        self.world
            .read_as_role(
                &self.expected.workload.task_role,
                &Self::resource(c),
                self.f.clock.now().unwrap(),
            )
            .await
            .unwrap()
    }
    async fn credentials(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM worker_credentials")
            .fetch_one(&self.f.store.pool)
            .await
            .unwrap()
    }
}
#[tokio::test]
async fn native_proof_is_bound_to_live_tls_and_stored_challenge() {
    let n = NativeFixture::new().await;
    let c = n.prepare().await;
    let material = n.material(&c).await;
    let (url, stop, join) = listen(n.server.clone()).await;
    let client = n.f.cert.client(&url);
    assert!(
        material
            .probe(
                &client,
                RequestId::new("other-channel").unwrap(),
                None,
                true
            )
            .await
            .is_err()
    );
    for field in 0..5 {
        let mut changed = c.clone();
        match field {
            0 => changed.execution.task = native::fixture::expected("task-b").task,
            1 => {
                changed.execution.workload.task_role =
                    RoleArn::parse("arn:aws:iam::123456789012:role/other/task").unwrap()
            }
            2 => changed.execution.installation = "f".repeat(32),
            3 => changed.secret_version = "f".repeat(32),
            _ => changed.expires_at -= 1,
        };
        assert!(
            material
                .probe(
                    &client,
                    RequestId::new(format!("changed-{field}")).unwrap(),
                    Some(&changed),
                    false
                )
                .await
                .is_err()
        );
    }
    assert_eq!(n.credentials().await, 0);
    let mut correct = n.f.cert.client(&url);
    material
        .authenticate(&mut correct, RequestId::new("correct-channel").unwrap())
        .await
        .unwrap();
    assert_eq!(n.credentials().await, 1);
    stop.send(()).unwrap();
    join.await.unwrap();
}
#[tokio::test]
async fn native_protected_delivery_rejects_other_roles_and_shared_role_activation() {
    let n = NativeFixture::new().await;
    let c = n.prepare().await;
    assert!(
        n.world
            .read_as_role(
                &RoleArn::parse("arn:aws:iam::123456789012:role/unrelated/task").unwrap(),
                &NativeFixture::resource(&c),
                synthetic::T
            )
            .await
            .is_err()
    );
    // AWS authorizes role sessions, not task ARN assertions. Shared-role access
    // really succeeds in the fixture; exclusivity must independently block use.
    let material = n.material(&c).await;
    n.world
        .exclusive
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let (url, stop, join) = listen(n.server.clone()).await;
    let mut client = n.f.cert.client(&url);
    assert!(
        material
            .authenticate(&mut client, RequestId::new("shared-role-claim").unwrap())
            .await
            .is_err()
    );
    assert_eq!(n.credentials().await, 0);
    stop.send(()).unwrap();
    join.await.unwrap();
    let n = NativeFixture::new().await;
    n.world
        .exclusive
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let r = PrepareBootstrap {
        lease: n.expected.lease(),
        request_id: RequestId::new("shared-role-prepare").unwrap(),
        evidence: synthetic::e("shared-role"),
    };
    assert!(
        n.server
            .prepare_fargate_bootstrap(&r, &n.plan)
            .await
            .is_err()
    );
    assert_eq!(n.world.value_count(), 0);
}
#[tokio::test]
async fn native_revocation_at_commit_rolls_back_both_one_use_authorities_and_preserves_holds() {
    let mut n = NativeFixture::new().await;
    let c = n.prepare().await;
    let material = n.material(&c).await;
    let before =
        n.f.store
            .financial_snapshot(&n.expected.binding.task.plan.project_id)
            .await
            .unwrap();
    let world = n.world.clone();
    Arc::get_mut(&mut n.server).unwrap().security.hook = Box::new(move |p| {
        if p == CommitCheckpoint::BeforeBootstrap {
            world
                .revoked
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    });
    let (url, stop, join) = listen(n.server.clone()).await;
    let mut client = n.f.cert.client(&url);
    assert!(
        material
            .authenticate(&mut client, RequestId::new("revocation-race").unwrap())
            .await
            .is_err()
    );
    assert_eq!(n.credentials().await, 0);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM worker_bootstrap_challenges")
            .fetch_one(&n.f.store.pool)
            .await
            .unwrap(),
        "pending"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM aws_protected_challenges")
            .fetch_one(&n.f.store.pool)
            .await
            .unwrap(),
        "delivered"
    );
    assert_eq!(
        n.f.store
            .financial_snapshot(&n.expected.binding.task.plan.project_id)
            .await
            .unwrap(),
        before
    );
    stop.send(()).unwrap();
    join.await.unwrap();
}
#[tokio::test]
async fn native_concurrent_claims_and_lost_response_never_reissue() {
    for lost in [false, true] {
        let n = NativeFixture::new().await;
        let c = n.prepare().await;
        let a = n.material(&c).await;
        let b = n.material(&c).await;
        let (url, stop, join) = listen(n.server.clone()).await;
        let mut ca = n.f.cert.client(&url);
        let mut cb = n.f.cert.client(&url);
        if lost {
            a.probe(&ca, RequestId::new("lost-response").unwrap(), None, false)
                .await
                .unwrap();
            assert!(ca.metadata().is_none());
            assert!(
                b.authenticate(&mut cb, RequestId::new("lost-replay").unwrap())
                    .await
                    .is_err()
            );
        } else {
            let (x, y) = tokio::join!(
                a.authenticate(&mut ca, RequestId::new("parallel-a").unwrap()),
                b.authenticate(&mut cb, RequestId::new("parallel-b").unwrap())
            );
            assert_eq!(usize::from(x.is_ok()) + usize::from(y.is_ok()), 1);
        }
        assert_eq!(n.credentials().await, 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM aws_delivery_events WHERE kind='cleanup_requested'"
            )
            .fetch_one(&n.f.store.pool)
            .await
            .unwrap(),
            1
        );
        stop.send(()).unwrap();
        join.await.unwrap();
    }
}
#[tokio::test]
async fn native_ignored_consumption_audit_rolls_back_credential_and_proof() {
    let n = NativeFixture::new().await;
    let c = n.prepare().await;
    let material = n.material(&c).await;
    sqlx::query("CREATE TRIGGER ignored_native_consumption BEFORE INSERT ON aws_delivery_events WHEN NEW.kind='consumed' BEGIN SELECT RAISE(IGNORE); END").execute(&n.f.store.pool).await.unwrap();
    let (url, stop, join) = listen(n.server.clone()).await;
    let mut client = n.f.cert.client(&url);
    assert!(
        material
            .authenticate(&mut client, RequestId::new("ignored-audit").unwrap())
            .await
            .is_err()
    );
    assert_eq!(n.credentials().await, 0);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM aws_protected_challenges")
            .fetch_one(&n.f.store.pool)
            .await
            .unwrap(),
        "delivered"
    );
    stop.send(()).unwrap();
    join.await.unwrap();
}
#[tokio::test]
async fn native_expired_registration_has_no_durable_side_effect() {
    let mut n = NativeFixture::new().await;
    n.plan.expires_at = synthetic::T + 30;
    n.f.clock.advance_to(synthetic::T + 31).unwrap();
    let r = PrepareBootstrap {
        lease: n.expected.lease(),
        request_id: RequestId::new("expired-registration").unwrap(),
        evidence: synthetic::e("expired"),
    };
    assert!(
        n.server
            .prepare_fargate_bootstrap(&r, &n.plan)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM aws_attempt_roles")
            .fetch_one(&n.f.store.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(n.world.value_count(), 0);
}
#[tokio::test]
async fn native_renewal_requires_fresh_protected_material_and_retires_old_generation() {
    let n = NativeFixture::new().await;
    let c = n.prepare().await;
    let old_material = n.material(&c).await;
    let material = n.material(&c).await;
    let (url, stop, join) = listen(n.server.clone()).await;
    let mut client = n.f.cert.client(&url);
    material
        .authenticate(&mut client, RequestId::new("renewal-initial").unwrap())
        .await
        .unwrap();
    let assignment = client
        .assignment(RequestId::new("native-assignment").unwrap())
        .await
        .unwrap();
    let script = FakeWorker::new(assignment.binding, assignment.capabilities, assignment.seed)
        .unwrap()
        .with_inputs(assignment.inputs)
        .unwrap()
        .script(FakeBehavior::Success, synthetic::T)
        .unwrap();
    for e in script.events.iter().take(4) {
        n.f.clock.advance_to(e.at).unwrap();
        let SimulatedInput::Message(bytes) = &e.input else {
            panic!("expected worker message")
        };
        client
            .message(serde_json::from_slice(bytes).unwrap(), None)
            .await
            .unwrap();
    }
    n.f.clock.advance_to(synthetic::T + 60).unwrap();
    let old = client.fixture_stolen_credential();
    let before = client.metadata().unwrap().clone();
    let renewal = client
        .prepare_possession(RequestId::new("renewal-preparation").unwrap())
        .await
        .unwrap();
    assert_ne!(renewal.id, c.id);
    assert_ne!(renewal.secret_version, c.secret_version);
    assert_eq!(renewal.credential.as_ref(), Some(&before.id));
    assert_eq!(n.world.value_count(), 2);
    // The consumed old secret cannot become renewal proof even with a valid
    // current bearer. This probe has no access to the fresh protected material.
    assert!(
        old_material
            .probe(
                &old,
                RequestId::new("old-material-renewal").unwrap(),
                Some(&renewal),
                false
            )
            .await
            .is_err()
    );
    let fresh = n.material(&renewal).await;
    fresh
        .authenticate(&mut client, RequestId::new("native-renewal").unwrap())
        .await
        .unwrap();
    assert_eq!(client.metadata().unwrap().generation, 2);
    assert_eq!(n.credentials().await, 2);
    assert!(
        old.assignment(RequestId::new("retired-generation").unwrap())
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM aws_protected_challenges WHERE state='consumed'"
        )
        .fetch_one(&n.f.store.pool)
        .await
        .unwrap(),
        2
    );
    stop.send(()).unwrap();
    join.await.unwrap();
}
#[tokio::test]
async fn native_restart_ciphertext_only_snapshot_and_wrong_factory_fail_closed() {
    let mut n = NativeFixture::new().await;
    let c = n.prepare().await;
    let material = n.material(&c).await;
    for suffix in ["data/state.sqlite", "data/state.sqlite-wal"] {
        if let Ok(bytes) = fs::read(n.f.root.path().join(suffix)) {
            assert!(!material.raw_occurs_in(&bytes));
        }
    }
    let snapshot = n.f.root.path().join("ordinary.snapshot");
    n.f.store.snapshot(&snapshot).await.unwrap();
    assert!(!material.raw_occurs_in(&fs::read(&snapshot).unwrap()));
    let (url, stop, join) = listen(n.server.clone()).await;
    let mut client = n.f.cert.client(&url);
    material
        .authenticate(&mut client, RequestId::new("before-restart").unwrap())
        .await
        .unwrap();
    stop.send(()).unwrap();
    join.await.unwrap();
    // A generic/fail-closed production factory must not accept a credential
    // minted under native scoped authority, even though its HMAC remains valid.
    Arc::get_mut(&mut n.server).unwrap().security.identity = Arc::new(Unimplemented);
    let (url, stop, join) = listen(n.server.clone()).await;
    let mut restored = n.f.cert.client(&url);
    restored.fixture_restore_from(&client);
    assert!(
        restored
            .assignment(RequestId::new("wrong-server-factory").unwrap())
            .await
            .is_err()
    );
    assert_eq!(n.credentials().await, 1);
    stop.send(()).unwrap();
    join.await.unwrap();
}
#[tokio::test]
async fn native_fresh_inspection_rejects_stopped_unknown_and_scope_substitution() {
    for case in 0..7 {
        let n = NativeFixture::new().await;
        let c = n.prepare().await;
        let material = n.material(&c).await;
        match case {
            0 => n.http.task.lock().unwrap()["lastStatus"] = serde_json::json!("STOPPED"),
            1 => n.http.task.lock().unwrap()["lastStatus"] = serde_json::json!("UNKNOWN"),
            2 => {
                n.http.task.lock().unwrap()["taskArn"] =
                    serde_json::json!(native::fixture::expected("task-b").task)
            }
            3 => {
                n.http.definition.lock().unwrap()["taskRoleArn"] =
                    serde_json::json!("arn:aws:iam::123456789012:role/other/task")
            }
            4 => {
                n.http.definition.lock().unwrap()["taskDefinitionArn"] = serde_json::json!(
                    "arn:aws:ecs:eu-central-1:123456789012:task-definition/other:1"
                )
            }
            5 => n.http.caller.lock().unwrap().0 = "999999999999".into(),
            _ => {
                n.f.clock.advance_to(c.expires_at).unwrap();
            }
        }
        let before =
            n.f.store
                .financial_snapshot(&n.expected.binding.task.plan.project_id)
                .await
                .unwrap();
        let (url, stop, join) = listen(n.server.clone()).await;
        let mut client = n.f.cert.client(&url);
        assert!(
            material
                .authenticate(&mut client, RequestId::new("bad-external-state").unwrap())
                .await
                .is_err(),
            "case {case}"
        );
        assert_eq!(n.credentials().await, 0);
        assert_eq!(
            n.f.store
                .financial_snapshot(&n.expected.binding.task.plan.project_id)
                .await
                .unwrap(),
            before
        );
        stop.send(()).unwrap();
        join.await.unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_broker_revocation_during_external_verification_is_rechecked_before_mint() {
    let n = NativeFixture::new().await;
    let c = n.prepare().await;
    let material = n.material(&c).await;
    let pause = Arc::new((tokio::sync::Notify::new(), tokio::sync::Notify::new()));
    *n.http.pause.lock().unwrap() = Some(pause.clone());
    let (url, stop, join) = listen(n.server.clone()).await;
    let mut client = n.f.cert.client(&url);
    let claim = material.authenticate(&mut client, RequestId::new("broker-race").unwrap());
    let revoke = async {
        pause.0.notified().await;
        sqlx::query("UPDATE secret_connections SET state='revoked',revision=revision+1 WHERE connection_id='aws-inspection'").execute(&n.f.store.pool).await.unwrap();
        pause.1.notify_one();
    };
    let (result, ()) = tokio::join!(claim, revoke);
    assert!(result.is_err());
    assert_eq!(n.credentials().await, 0);
    stop.send(()).unwrap();
    join.await.unwrap();
}
#[tokio::test]
async fn native_completed_artifacts_and_frozen_plan_survive_later_delivery_revocation() {
    let n = NativeFixture::new().await;
    let c = n.prepare().await;
    let material = n.material(&c).await;
    let fingerprint: Vec<(String, String)> =
        sqlx::query_as("SELECT plan_id,fingerprint FROM plan_versions WHERE state='active'")
            .fetch_all(&n.f.store.pool)
            .await
            .unwrap();
    let (url, stop, join) = listen(n.server.clone()).await;
    let mut client = n.f.cert.client(&url);
    material
        .authenticate(&mut client, RequestId::new("artifact-initial").unwrap())
        .await
        .unwrap();
    let a = client
        .assignment(RequestId::new("artifact-assignment").unwrap())
        .await
        .unwrap();
    let script = FakeWorker::new(a.binding, a.capabilities, a.seed)
        .unwrap()
        .with_inputs(a.inputs)
        .unwrap()
        .script(FakeBehavior::Success, synthetic::T)
        .unwrap();
    for e in &script.events {
        n.f.clock.advance_to(e.at).unwrap();
        if let SimulatedInput::Message(bytes) = &e.input {
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
    n.f.co
        .validator()
        .validate_leaf(&n.expected.lease(), n.f.policy.as_ref(), n.f.clock.as_ref())
        .await
        .unwrap();
    let accepted =
        n.f.co
            .validator()
            .verify_accepted(&n.expected.binding.task)
            .await
            .unwrap();
    n.world
        .revoked
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(
        client
            .assignment(RequestId::new("revoked-after-acceptance").unwrap())
            .await
            .is_err()
    );
    assert_eq!(
        n.f.co
            .validator()
            .verify_accepted(&n.expected.binding.task)
            .await
            .unwrap(),
        accepted
    );
    assert_eq!(
        sqlx::query_as::<_, (String, String)>(
            "SELECT plan_id,fingerprint FROM plan_versions WHERE state='active'"
        )
        .fetch_all(&n.f.store.pool)
        .await
        .unwrap(),
        fingerprint
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_attempts WHERE task_id='group-g'")
            .fetch_one(&n.f.store.pool)
            .await
            .unwrap(),
        0
    );
    stop.send(()).unwrap();
    join.await.unwrap();
}

#[tokio::test]
async fn native_registration_rechecks_expiry_after_external_verification() {
    let mut n = NativeFixture::new().await;
    n.plan.expires_at = synthetic::T + 30;
    *n.world.advance_on_verify.lock().unwrap() = Some((n.f.clock.clone(), synthetic::T + 31));
    let r = PrepareBootstrap {
        lease: n.expected.lease(),
        request_id: RequestId::new("crossed-registration-expiry").unwrap(),
        evidence: synthetic::e("crossed-expiry"),
    };
    assert!(
        n.server
            .prepare_fargate_bootstrap(&r, &n.plan)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM aws_attempt_roles")
            .fetch_one(&n.f.store.pool)
            .await
            .unwrap(),
        0
    );
}
#[tokio::test]
async fn native_uncertain_publication_retries_are_durably_bounded_with_same_material() {
    let n = NativeFixture::new().await;
    n.world
        .lost_ack
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let r = PrepareBootstrap {
        lease: n.expected.lease(),
        request_id: RequestId::new("uncertain-publication").unwrap(),
        evidence: synthetic::e("publication-evidence"),
    };
    for _ in 0..20 {
        assert!(
            n.server
                .prepare_fargate_bootstrap(&r, &n.plan)
                .await
                .is_err()
        );
    }
    let calls = n
        .world
        .publication_calls
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(calls <= 15, "durable bound, observed {calls}");
    assert_eq!(n.world.value_count(), 1);
    assert!(
        sqlx::query_scalar::<_, i64>("SELECT revision FROM aws_protected_challenges")
            .fetch_one(&n.f.store.pool)
            .await
            .unwrap()
            >= 16
    );
    assert_eq!(n.credentials().await, 0);
}

#[tokio::test]
async fn controller_identity_check_expiry_at_account_commit_rolls_back_enrollment() {
    let (f, _, _, expected, _, http) = super::aws::setup().await;
    let r =
        crate::controller_keys::human_auth::sessions::owner::infrastructure_consent_on_existing(
            f.store.clone(),
            f.root.path().join("data/state.sqlite"),
            f.root.path().join("keys"),
            &expected,
        )
        .await;
    let mut i = native::reviewed_fixture_open(
        &f.bootstrap,
        f.policy.clone(),
        f.clock.clone(),
        &expected,
        http.http(),
    )
    .await;
    let receipt = i
        .verify_account(&r, &RequestId::new("controller-expiring-check").unwrap())
        .await
        .unwrap();
    native::fixture_account_expiry(&mut i, f.clock.clone(), receipt.expires_at);
    assert!(i.enroll_account(&r).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM aws_infrastructure_accounts")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        0
    );
}

#[path = "tests_secure_process.rs"]
mod process;
#[tokio::test]
async fn native_disconnect_denies_continuation_but_preserves_reduced_stop_receipts() {
    let n = NativeFixture::new().await;
    let c = n.prepare().await;
    let material = n.material(&c).await;
    let (url, stop, join) = listen(n.server.clone()).await;
    let mut client = n.f.cert.client(&url);
    material
        .authenticate(&mut client, RequestId::new("stop-initial").unwrap())
        .await
        .unwrap();
    let a = client
        .assignment(RequestId::new("stop-assignment").unwrap())
        .await
        .unwrap();
    let script = FakeWorker::new(a.binding, a.capabilities, a.seed)
        .unwrap()
        .with_inputs(a.inputs)
        .unwrap()
        .script(FakeBehavior::Success, synthetic::T)
        .unwrap();
    for e in script.events.iter().take(4) {
        n.f.clock.advance_to(e.at).unwrap();
        let SimulatedInput::Message(bytes) = &e.input else {
            panic!("message")
        };
        client
            .message(serde_json::from_slice(bytes).unwrap(), None)
            .await
            .unwrap();
    }
    let before =
        n.f.store
            .financial_snapshot(&n.expected.binding.task.plan.project_id)
            .await
            .unwrap();
    n.world
        .revoked
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut message = WorkerMessage {
        protocol_version: 1,
        message_id: MessageId::new("revoked-heartbeat").unwrap(),
        binding: n.expected.binding.clone(),
        sequence: 5,
        body: WorkerBody::Heartbeat {},
    };
    assert!(client.message(message.clone(), None).await.is_err());
    message.message_id = MessageId::new("reduced-stop-query").unwrap();
    message.body = WorkerBody::GetStopInstruction {};
    client.message(message, None).await.unwrap();
    assert_eq!(
        n.f.store
            .financial_snapshot(&n.expected.binding.task.plan.project_id)
            .await
            .unwrap(),
        before
    );
    stop.send(()).unwrap();
    join.await.unwrap();
}
#[tokio::test]
async fn restored_snapshot_cannot_activate_production_delivery_or_generate_new_authority() {
    let n = NativeFixture::new().await;
    let c = n.prepare().await;
    let snapshot = n.f.root.path().join("data/stale.snapshot");
    n.f.store.snapshot(&snapshot).await.unwrap();
    let restored = PlanStore::open(&snapshot).await.unwrap();
    restored.check_integrity().await.unwrap();
    let b = ControllerBootstrap::open(
        restored.clone(),
        &n.f.root.path().join("keys"),
        ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let service = SecureFargateBootstrap::open(&b, n.f.policy.clone())
        .await
        .unwrap();
    let r = PrepareBootstrap {
        lease: n.expected.lease(),
        request_id: RequestId::new("restore-activation").unwrap(),
        evidence: synthetic::e("restored-state-is-not-live-aws-evidence"),
    };
    secure_fixture::assert_production_gate_closed(&service, &n.plan, synthetic::T).await;
    assert!(service.register_plan(&r, &n.plan).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_credentials")
            .fetch_one(&restored.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT challenge_id FROM aws_protected_challenges")
            .fetch_one(&restored.pool)
            .await
            .unwrap(),
        c.id.as_str()
    );
    // This verifies the production activation gate, not automatic detection of
    // arbitrary rollback. External restore fencing remains an explicit blocker.
}
#[tokio::test]
async fn native_post_publication_sql_failure_cannot_erase_the_external_retry_bound() {
    let n = NativeFixture::new().await;
    sqlx::query("CREATE TRIGGER fail_after_external_publication BEFORE UPDATE ON aws_protected_challenges WHEN NEW.state='delivered' BEGIN SELECT RAISE(ABORT,'synthetic post-publication write failure'); END").execute(&n.f.store.pool).await.unwrap();
    let r = PrepareBootstrap {
        lease: n.expected.lease(),
        request_id: RequestId::new("publication-write-failure").unwrap(),
        evidence: synthetic::e("publication-write-failure-evidence"),
    };
    for _ in 0..20 {
        assert!(
            n.server
                .prepare_fargate_bootstrap(&r, &n.plan)
                .await
                .is_err()
        );
    }
    let calls = n
        .world
        .publication_calls
        .load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        calls <= 15,
        "post-effect SQL rollback must preserve prior attempt count, observed {calls}"
    );
    assert_eq!(n.world.value_count(), 1);
    assert_eq!(n.credentials().await, 0);
}
#[tokio::test]
async fn native_envelope_is_bound_to_actual_key_scope_and_independent_randomness() {
    let n = NativeFixture::new().await;
    let other = NativeFixture::new().await;
    let c = n.prepare().await;
    let material = n.material(&c).await;
    material
        .assert_encryption_boundary(&n.f.bootstrap, &other.f.bootstrap)
        .await;
    assert!(
        sqlx::query(
            "UPDATE aws_protected_challenges SET ciphertext=zeroblob(80),revision=revision+1"
        )
        .execute(&n.f.store.pool)
        .await
        .is_err()
    );
    let raw = sqlx::query("PRAGMA foreign_key_list(aws_protected_challenges)")
        .fetch_all(&n.f.store.pool)
        .await
        .unwrap();
    for (table, expected) in [
        (
            "worker_enrollments",
            vec!["enrollment_id", "project_id", "attempt_id"],
        ),
        (
            "worker_credentials",
            vec!["previous_credential", "enrollment_id"],
        ),
        (
            "worker_credentials",
            vec!["consumed_credential", "enrollment_id"],
        ),
    ] {
        let ids: std::collections::BTreeSet<i64> = raw
            .iter()
            .filter(|r| r.get::<&str, _>("table") == table)
            .map(|r| r.get("id"))
            .collect();
        assert!(
            ids.into_iter().any(|id| {
                let columns: Vec<String> = raw
                    .iter()
                    .filter(|r| r.get::<i64, _>("id") == id)
                    .map(|r| r.get("from"))
                    .collect();
                columns == expected
            }),
            "compound scope FK required for {table}"
        );
    }
    n.f.store.check_integrity().await.unwrap();
}
