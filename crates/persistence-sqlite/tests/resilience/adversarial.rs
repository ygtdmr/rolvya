use super::common::owner;
use super::*;
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
enum Action {
    Pass,
    Advance(u8),
    Restart,
    Duplicate,
    Cancel,
    Validate,
    Charge(u8),
    Approve,
    WorkerApprove,
    Observe(u8),
    Revoke,
}
fn sequence(seed: u64) -> Vec<Action> {
    let mut state = seed;
    let mut out = Vec::with_capacity(48);
    for _ in 0..48 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let n = (state >> 32) as u8;
        out.push(match state % 11 {
            0 => Action::Pass,
            1 => Action::Advance(n % 37),
            2 => Action::Restart,
            3 => Action::Duplicate,
            4 => Action::Cancel,
            5 => Action::Validate,
            6 => Action::Charge(n % 5),
            7 => Action::Approve,
            8 => Action::WorkerApprove,
            9 => Action::Observe(n % 4),
            _ => Action::Revoke,
        });
    }
    out
}
async fn latest(f: &Fixture) -> Option<LeaseRef> {
    let row = sqlx::query(
        "SELECT task_id,attempt_id,epoch FROM task_attempts ORDER BY admission_event DESC LIMIT 1",
    )
    .fetch_optional(&f.store.pool)
    .await
    .unwrap()?;
    Some(LeaseRef {
        task: scope(f, &row.get::<String, _>(0)),
        attempt_id: AttemptId::new(row.get::<String, _>(1)).unwrap(),
        epoch: row.get::<i64, _>(2) as u64,
    })
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
#[tokio::test]
async fn seeded_operation_prefixes_preserve_authority_and_accepted_work_after_every_step() {
    for seed in [1u64, 7, 0x5eed, 0x9e3779b97f4a7c15] {
        let mut f = fixture(100).await;
        let mut a = Authority::default();
        let mut co = open(&f, &a).await;
        let clock = DeterministicClock::new(T).unwrap();
        phase(&co, &a, &clock, 1, 16, 0).await;
        co.pass(&a, &clock, PassLimits::default()).await.unwrap();
        let mut image = AuthorityImage::read(&f.store, &f.content.key).await;
        let actions = sequence(seed);
        let mut trace = Vec::new();
        for (index, action) in actions.iter().enumerate() {
            println!(
                "R10 replay seed={seed:#x}, prefix={}, action={action:?}",
                index + 1
            );
            trace.push(*action);
            let before = image.clone();
            let mut approved = false;
            match *action {
                Action::Pass => {
                    let _ = co
                        .pass(
                            &a,
                            &clock,
                            PassLimits {
                                admissions: 2,
                                events: 4,
                                validations: 1,
                                groups: 1,
                            },
                        )
                        .await;
                }
                Action::Advance(seconds) => clock
                    .advance_to(clock.now().unwrap() + i64::from(seconds))
                    .unwrap(),
                Action::Restart => {
                    f.store.close().await;
                    f.store = reopen(&f.path).await;
                    co = open(&f, &a).await;
                }
                Action::Duplicate => {
                    let payload:Option<Vec<u8>>=sqlx::query_scalar("SELECT e.payload FROM synthetic_worker_events e JOIN worker_messages m ON m.project_id=e.project_id AND m.attempt_id=e.attempt_id AND m.message_id=e.message_id ORDER BY m.event_sequence DESC LIMIT 1").fetch_optional(&f.store.pool).await.unwrap();
                    if let Some(bytes) = payload {
                        let deadlines: Vec<(String, i64)> = sqlx::query_as(
                            "SELECT attempt_id,lease_until FROM worker_leases ORDER BY attempt_id",
                        )
                        .fetch_all(&f.store.pool)
                        .await
                        .unwrap();
                        let message = decode_worker_message(&bytes).unwrap();
                        let _ = DurableWorkerController::new(f.store.clone())
                            .receive(
                                &a.peer(&message.binding),
                                &bytes,
                                &a,
                                &a,
                                &clock,
                                Limits::default(),
                            )
                            .await;
                        assert_eq!(deadlines,sqlx::query_as::<_,(String,i64)>("SELECT attempt_id,lease_until FROM worker_leases ORDER BY attempt_id").fetch_all(&f.store.pool).await.unwrap(),"duplicate renewed privilege: seed={seed}, prefix={}",index+1);
                    }
                }
                Action::Cancel => {
                    if let Some(l) = latest(&f).await {
                        let _ = OutboxDispatcher::new(f.store.clone())
                            .recover_attempt(
                                &FenceRequest {
                                    lease: l,
                                    request_id: RequestId::new(format!("cancel-{seed:x}-{index}"))
                                        .unwrap(),
                                    actor: owner(),
                                    evidence: e("synthetic-sequence-stop"),
                                },
                                RecoveryCause::Canceled,
                                &a,
                                &clock,
                            )
                            .await;
                    }
                }
                Action::Validate => {
                    if let Some(l) = latest(&f).await {
                        let _ = co.validator().validate_leaf(&l, &a, &clock).await;
                    }
                    let _ = co
                        .validator()
                        .aggregate(&scope(&f, "group-g"), &a, &clock)
                        .await;
                }
                Action::Charge(amount) => {
                    if let Some(l) = latest(&f).await {
                        let liability = LiabilityId::new(l.attempt_id.as_str()).unwrap();
                        let b = f
                            .store
                            .financial_liability(&f.content.key.project_id, &liability)
                            .await
                            .unwrap();
                        let mut provisional = b.provisional;
                        provisional.ai_api = provisional
                            .ai_api
                            .checked_add(money(i64::from(amount)))
                            .unwrap();
                        let _ = f
                            .store
                            .apply_financial(
                                &FinancialRequest {
                                    project: f.content.key.project_id.clone(),
                                    request_id: RequestId::new(format!("charge-{seed:x}-{index}"))
                                        .unwrap(),
                                    actor: owner(),
                                    reason: "Synthetic sequence exposure only".into(),
                                    command: FinancialCommand::Report {
                                        liability,
                                        expected_revision: b.revision,
                                        unreserved: provisional,
                                        uncertain: amount == 0,
                                        evidence: e(&format!("charge-e-{seed:x}-{index}")),
                                    },
                                },
                                &Finance,
                            )
                            .await;
                    }
                }
                Action::Approve | Action::WorkerApprove => {
                    let current = f
                        .store
                        .financial_snapshot(&f.content.key.project_id)
                        .await
                        .unwrap();
                    let result=f.store.apply_financial(&FinancialRequest {project:f.content.key.project_id.clone(),request_id:RequestId::new(format!("approve-{seed:x}-{index}")).unwrap(),actor:if matches!(action,Action::Approve) {owner()}else {PrincipalId::new("synthetic-worker").unwrap()},reason:"Explicitly scoped synthetic approval fixture, not human authentication".into(),command:FinancialCommand::Approve {expected_version:current.version,new_ceiling:money(current.ceiling.minor_units()+1),proof:OwnerProof {reference:e(&format!("proof-{seed:x}-{index}")),verified_at:now()}}},&Finance).await;
                    approved = matches!(action, Action::Approve) && result.is_ok();
                    if matches!(action, Action::WorkerApprove) {
                        assert!(result.is_err());
                    }
                }
                Action::Observe(n) => {
                    if let Some(l) = latest(&f).await {
                        let revision:Option<i64>=sqlx::query_scalar("SELECT revision FROM synthetic_operations WHERE project_id=? AND attempt_id=?").bind(f.content.key.project_id.as_str()).bind(l.attempt_id.as_str()).fetch_optional(&f.store.pool).await.unwrap();
                        if let Some(rev) = revision {
                            let provider = SyntheticProvider::new(f.store.clone());
                            provider
                                .observe(
                                    &OperationKey {
                                        project: f.content.key.project_id.clone(),
                                        attempt: l.attempt_id.clone(),
                                    },
                                    rev as u64,
                                    match n {
                                        0 => ProviderState::Active,
                                        1 => ProviderState::CancelAck,
                                        2 => ProviderState::Finished,
                                        _ => ProviderState::Unknown,
                                    },
                                    n == 3,
                                    &e(&format!("observation-{seed:x}-{index}")),
                                )
                                .await
                                .unwrap();
                            let _ = OutboxDispatcher::new(f.store.clone())
                                .reconcile(&l, &provider, &a, &clock, Limits::default())
                                .await;
                        }
                    }
                }
                Action::Revoke => a.revoked = !a.revoked,
            }
            image = AuthorityImage::read(&f.store, &f.content.key).await;
            image.preserves(&before);
            assert_eq!(
                image.ceiling,
                before.ceiling + i64::from(approved),
                "unapproved ceiling change: seed={seed}, shortest failing prefix={}, trace={trace:?}",
                index + 1
            );
            for (task, _) in &image.accepted {
                co.validator()
                    .verify_accepted(&scope(&f, task))
                    .await
                    .unwrap_or_else(|error| {
                        panic!(
                            "seed={seed}, shortest failing prefix={}, trace={trace:?}: {error}",
                            index + 1
                        )
                    });
            }
        }
        // Trace serialization + prefix are a deterministic replay/shrink interface.
        let saved = serde_json::to_vec(&actions).unwrap();
        let replay: Vec<Action> = serde_json::from_slice(&saved).unwrap();
        assert_eq!(format!("{actions:?}"), format!("{replay:?}"));
        println!("R10 seed {seed:#x}: {} checked prefixes", actions.len());
    }
}
#[tokio::test]
async fn repeated_admission_charge_race_is_linearized_before_or_after_ledger_event() {
    for round in 0..8 {
        let f = fixture(14).await;
        let a = Authority::default();
        let clock = DeterministicClock::new(T).unwrap();
        let first = f
            .store
            .admit(&standalone(&f), &a, &clock, Limits::default())
            .await
            .unwrap();
        let bill = FinancialRequest {
            project: f.content.key.project_id.clone(),
            request_id: RequestId::new("racing-bill").unwrap(),
            actor: owner(),
            reason: "Synthetic delayed cost race".into(),
            command: FinancialCommand::Report {
                liability: LiabilityId::new(first.lease.attempt_id.as_str()).unwrap(),
                expected_revision: 1,
                unreserved: CostBreakdown {
                    ai_api: money(8),
                    ..zero()
                },
                uncertain: false,
                evidence: e("racing-bill-e"),
            },
        };
        let request = admission_request(
            &f.content.key,
            f.revision.fingerprint(),
            "racing-b",
            "task-b",
        );
        let (b, charge) = tokio::join!(
            f.store.admit(&request, &a, &clock, Limits::default()),
            f.store.apply_financial(&bill, &Finance)
        );
        let charge = charge.unwrap();
        let image = AuthorityImage::read(&f.store, &f.content.key).await;
        assert_eq!(image.ceiling, 14);
        match b {
            Ok(r) => {
                assert!(
                    r.financial_event < charge.sequence,
                    "round {round}: stale headroom admitted after charge"
                );
                assert_eq!(image.attempts, 2);
            }
            Err(_) => assert_eq!(image.attempts, 1),
        }
        assert!(
            f.store
                .admit(
                    &admission_request(
                        &f.content.key,
                        f.revision.fingerprint(),
                        "after-cost",
                        "task-b"
                    ),
                    &a,
                    &clock,
                    Limits::default()
                )
                .await
                .is_err()
        );
    }
}
#[tokio::test]
async fn repeated_heartbeat_fence_and_revocation_races_never_resurrect_cached_authority() {
    for _ in 0..8 {
        let f = fixture(7).await;
        let mut a = Authority::default();
        let co = open(&f, &a).await;
        let clock = DeterministicClock::new(T).unwrap();
        for t in [0, 1, 2] {
            clock.advance_to(T + t).unwrap();
            co.pass(&a, &clock, PassLimits::default()).await.unwrap();
        }
        let l = lease(&f, "task-a").await;
        let controller = DurableWorkerController::new(f.store.clone());
        let binding = controller.binding(&l).await.unwrap();
        clock.advance_to(T + 30).unwrap();
        let hb = WorkerMessage {
            protocol_version: PROTOCOL_VERSION,
            message_id: MessageId::new("r10-heartbeat").unwrap(),
            binding: binding.clone(),
            sequence: 4,
            body: WorkerBody::Heartbeat {},
        };
        let bytes = encode_worker_message(&hb).unwrap();
        let peer = a.peer(&binding);
        let fence = FenceRequest {
            lease: l.clone(),
            request_id: RequestId::new("r10-fence").unwrap(),
            actor: owner(),
            evidence: e("r10-stop-e"),
        };
        let d = OutboxDispatcher::new(f.store.clone());
        let (heartbeat, stop) = tokio::join!(
            controller.receive(&peer, &bytes, &a, &a, &clock, Limits::default()),
            d.recover_attempt(&fence, RecoveryCause::Canceled, &a, &clock)
        );
        stop.unwrap();
        assert!(f.store.check_lease(&l, &clock).await.is_err());
        assert!(
            controller
                .receive(&peer, &bytes, &a, &a, &clock, Limits::default())
                .await
                .is_err()
        );
        a.revoked = true;
        assert_eq!(
            controller
                .receive(&peer, &bytes, &a, &a, &clock, Limits::default())
                .await
                .unwrap_err(),
            WorkerError::Unauthorized
        );
        let image = AuthorityImage::read(&f.store, &f.content.key).await;
        assert_eq!(image.positions[4..8], [1, 2, 3, 1]);
        assert_eq!(image.ceiling, 7);
        assert_eq!(image.attempts, 1);
        if heartbeat.is_ok() {
            assert_eq!(controller.lease_until(&l).await.unwrap(), T + 120);
        }
    }
}
/// Deliberately faulty remote stand-in. Extra provider execution IDs are durable
/// test records, never new controller attempts or authorization records.
struct NativeRetry {
    store: PlanStore,
    provider: SyntheticProvider,
}
impl LaunchAdapter for NativeRetry {
    async fn inspect(&self, c: &LaunchCommand) -> Result<ProviderObservation, WorkerError> {
        self.provider.inspect(c).await
    }
    async fn launch(
        &self,
        a: &LaunchAuthorization,
        clock: &impl Clock,
    ) -> Result<LaunchResult, WorkerError> {
        let result = self.provider.launch(a, clock).await?;
        let LaunchResult::Observed(mut observation) = result else {
            return Ok(result);
        };
        for execution in ["first", "hidden-native-retry"] {
            sqlx::query("INSERT INTO r10_remote_effects VALUES(?,?,?)")
                .bind(observation.key.project.as_str())
                .bind(observation.key.attempt.as_str())
                .bind(execution)
                .execute(&self.store.pool)
                .await?;
        }
        observation.unknown_exposure = true;
        observation.revision = 2;
        observation.evidence = e("synthetic-hidden-native-retry");
        Ok(LaunchResult::Observed(observation))
    }
}
#[tokio::test]
async fn hidden_native_retry_effects_are_detected_as_unknown_exposure_not_free_work() {
    let f = fixture(10).await;
    let a = Authority::default();
    let clock = DeterministicClock::new(T).unwrap();
    sqlx::query("CREATE TABLE r10_remote_effects(project_id TEXT,operation_key TEXT,execution_id TEXT,PRIMARY KEY(project_id,operation_key,execution_id)) STRICT").execute(&f.store.pool).await.unwrap();
    let r = f
        .store
        .admit(&standalone(&f), &a, &clock, Limits::default())
        .await
        .unwrap();
    let d = OutboxDispatcher::new(f.store.clone());
    let ticket = d
        .claim(&r.lease, &owner(), &a, &clock, Limits::default())
        .await
        .unwrap();
    assert_eq!(
        d.dispatch(
            &ticket,
            &NativeRetry {
                store: f.store.clone(),
                provider: SyntheticProvider::new(f.store.clone())
            },
            &a,
            &clock,
            Limits::default()
        )
        .await
        .unwrap(),
        DispatchState::Blocked
    );
    assert_eq!(count(&f, "r10_remote_effects").await, 2);
    assert_eq!(count(&f, "task_attempts").await, 1);
    let version: i64 = sqlx::query_scalar(
        "SELECT max(version) FROM liability_revisions WHERE project_id=? AND liability_id=?",
    )
    .bind(f.content.key.project_id.as_str())
    .bind(r.lease.attempt_id.as_str())
    .fetch_one(&f.store.pool)
    .await
    .unwrap();
    let late_cost = FinancialRequest {
        project: f.content.key.project_id.clone(),
        request_id: RequestId::new("hidden-retry-late-cost").unwrap(),
        actor: owner(),
        reason: "Synthetic delayed observation of hidden provider execution".into(),
        command: FinancialCommand::Report {
            liability: LiabilityId::new(r.lease.attempt_id.as_str()).unwrap(),
            expected_revision: version as u64,
            unreserved: CostBreakdown {
                ai_api: money(2),
                compute: money(2),
                storage: money(2),
                transfer: money(1),
            },
            uncertain: true,
            evidence: e("hidden-retry-late-cost-e"),
        },
    };
    let receipt = f.store.apply_financial(&late_cost, &Finance).await.unwrap();
    assert_eq!(
        f.store.apply_financial(&late_cost, &Finance).await.unwrap(),
        receipt
    );
    let image = AuthorityImage::read(&f.store, &f.content.key).await;
    assert_eq!(image.uncertain, 1);
    assert_eq!(image.ceiling, 10);
    assert_eq!(image.positions[4..8], [1, 2, 3, 1]);
    assert_eq!(image.positions[8..12], [2, 2, 2, 1]);
    assert!(
        f.store
            .financial_snapshot(&f.content.key.project_id)
            .await
            .unwrap()
            .over_ceiling
    );
    assert!(
        f.store
            .admit(
                &admission_request(&f.content.key, f.revision.fingerprint(), "new-b", "task-b"),
                &a,
                &clock,
                Limits::default()
            )
            .await
            .is_err()
    );
    f.store.close().await;
    let s = reopen(&f.path).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM r10_remote_effects")
            .fetch_one(&s.pool)
            .await
            .unwrap(),
        2
    );
    assert_eq!(AuthorityImage::read(&s, &f.content.key).await, image);
}
struct LookupGate {
    provider: SyntheticProvider,
    entered: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}
impl LaunchAdapter for LookupGate {
    async fn inspect(&self, c: &LaunchCommand) -> Result<ProviderObservation, WorkerError> {
        self.entered.notify_one();
        self.resume.notified().await;
        self.provider.inspect(c).await
    }
    async fn launch(
        &self,
        a: &LaunchAuthorization,
        clock: &impl Clock,
    ) -> Result<LaunchResult, WorkerError> {
        self.provider.launch(a, clock).await
    }
}
#[tokio::test]
async fn delayed_status_lookup_rechecks_expiry_or_fencing_without_holding_writer() {
    for existing in [false, true] {
        let f = fixture(100).await;
        let a = Authority::default();
        let clock = DeterministicClock::new(T).unwrap();
        let admitted = f
            .store
            .admit(&standalone(&f), &a, &clock, Limits::default())
            .await
            .unwrap();
        let d = OutboxDispatcher::new(f.store.clone());
        let ticket = d
            .claim(&admitted.lease, &owner(), &a, &clock, Limits::default())
            .await
            .unwrap();
        let p = SyntheticProvider::new(f.store.clone());
        if existing {
            d.dispatch(&ticket, &p, &a, &clock, Limits::default())
                .await
                .unwrap();
        }
        let gate = LookupGate {
            provider: p,
            entered: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
        };
        let lookup = d.dispatch(&ticket, &gate, &a, &clock, Limits::default());
        let revoke = async {
            gate.entered.notified().await;
            let tx = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
            tx.rollback().await.unwrap();
            if existing {
                d.recover_attempt(
                    &FenceRequest {
                        lease: admitted.lease.clone(),
                        request_id: RequestId::new("lookup-stop").unwrap(),
                        actor: owner(),
                        evidence: e("lookup-stop-e"),
                    },
                    RecoveryCause::Canceled,
                    &a,
                    &clock,
                )
                .await
                .unwrap();
            } else {
                clock.advance_to(T + 90).unwrap();
            }
            gate.resume.notify_one();
        };
        let (result, ()) = tokio::join!(lookup, revoke);
        if existing {
            assert_eq!(result.unwrap(), DispatchState::Blocked);
        } else {
            assert!(result.is_err());
        }
        let image = AuthorityImage::read(&f.store, &f.content.key).await;
        assert_eq!(image.effects, i64::from(existing));
        assert_eq!(image.positions[4..8], [1, 2, 3, 1]);
        assert_eq!(image.ceiling, 100);
    }
}
