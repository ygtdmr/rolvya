use super::*;
async fn change(f: &Fixture, registry: &ProviderRegistry, mutate: impl FnOnce(&mut Entitlement)) {
    let id = ConnectionId::parse("synthetic-connection".into()).unwrap();
    let project = &f.request.task.plan.project_id;
    let mut tx = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let mut r = store::connection(&mut tx, &registry.key, &id)
        .await
        .unwrap();
    let mut g = store::grant(&mut tx, &registry.key, &id, project)
        .await
        .unwrap();
    let mut e = g.entitlement.take().unwrap();
    mutate(&mut e);
    e.verified_against = r.version;
    e.scope_digest = registry.scope_digest(&id, &r, &g).unwrap();
    r.version += 1;
    r.entitlement = Some(e.clone());
    store::save_version(&mut tx, &registry.key, &id, &r)
        .await
        .unwrap();
    sqlx::query("UPDATE provider_connections SET version=? WHERE connection_id=?")
        .bind(r.version)
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .unwrap();
    g.version += 1;
    g.verified_connection_version = Some(r.version);
    g.entitlement = Some(e);
    store::save_grant(&mut tx, &registry.key, &g).await.unwrap();
    tx.commit().await.unwrap();
}
async fn denied(f: &Fixture, r: &Arc<ProviderRegistry>) {
    assert!(
        f.store
            .admit(
                &f.request,
                &r.policy(synthetic::Authority::default()),
                &f.clock(),
                Limits::default()
            )
            .await
            .is_err()
    );
    assert_eq!(f.count("task_attempts").await, 0);
    assert_eq!(f.count("dispatch_outbox").await, 0);
    assert_eq!(f.count("provider_capacity_reservations").await, 0);
    assert_eq!(
        f.store
            .financial_snapshot(&f.request.task.plan.project_id)
            .await
            .unwrap()
            .held
            .total()
            .unwrap()
            .minor_units(),
        0
    );
}
#[tokio::test]
async fn policy_citation_native_login_missing_consent_and_stale_evidence_never_admit() {
    for case in 0..16 {
        let f = Fixture::new().await;
        let r = f.seeded().await;
        change(&f, &r, |e| match case {
            0 => e.permission = AccountPermission::AwaitingProof,
            1 => e.permission = AccountPermission::NativeLoginOnly,
            2 => e.permission = AccountPermission::Revoked,
            3 => e.policy = PolicyClass::Disallowed,
            4 => e.policy = PolicyClass::RegistrationRequired,
            5 => e.policy = PolicyClass::NativeClientOnly,
            6 => e.expires_at = synthetic::T,
            7 => e.credential_expires_at = synthetic::T,
            8 => e.account = "other-account".into(),
            9 => e.provider = "other-provider".into(),
            10 => e.mode = AccessMode::CliSession,
            11 => e.models[0].efforts = vec![Effort::Low],
            12 => e.models[0].pool = "other-region-pool".into(),
            13 => e.quota_ceiling = 0,
            14 => e.decision = EligibilityReason::AwaitingUserConsent,
            _ => e.revision = "withdrawn-policy-v0".into(),
        })
        .await;
        denied(&f, &r).await;
    }
}
fn second(f: &Fixture) -> AdmissionRequest {
    let mut b = f.request.clone();
    b.task.task_id = TaskId::new("task-b").unwrap();
    b.request_id = RequestId::new("provider-attempt-b").unwrap();
    b
}
#[tokio::test]
async fn two_sqlite_pools_contending_for_one_account_slot_commit_one_attempt_and_hold() {
    for _ in 0..3 {
        let f = Fixture::new().await;
        let r = f.seeded().await;
        change(&f, &r, |e| e.concurrency = 1).await;
        let other = PlanStore::open(f._root.path().join("data/state.sqlite"))
            .await
            .unwrap();
        let b = second(&f);
        let p = r.policy(synthetic::Authority::default());
        let q = r.policy(synthetic::Authority::default());
        let clock = f.clock();
        let (a, b) = tokio::join!(
            f.store.admit(&f.request, &p, &clock, Limits::default()),
            other.admit(&b, &q, &clock, Limits::default())
        );
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        assert_eq!(f.count("task_attempts").await, 1);
        assert_eq!(f.count("provider_capacity_reservations").await, 1);
        assert_eq!(
            f.store
                .financial_snapshot(&f.request.task.plan.project_id)
                .await
                .unwrap()
                .held,
            synthetic::costs()
        );
    }
}
#[tokio::test]
async fn quota_usage_survives_reopen_and_cannot_be_reset_by_duplicate_admission() {
    let f = Fixture::new().await;
    let r = f.seeded().await;
    change(&f, &r, |e| e.quota_ceiling = 1).await;
    let p = r.policy(synthetic::Authority::default());
    f.store
        .admit(&f.request, &p, &f.clock(), Limits::default())
        .await
        .unwrap();
    f.store
        .admit(&f.request, &p, &f.clock(), Limits::default())
        .await
        .unwrap();
    let reopened = PlanStore::open(f._root.path().join("data/state.sqlite"))
        .await
        .unwrap();
    assert!(matches!(
        reopened
            .admit(&second(&f), &p, &f.clock(), Limits::default())
            .await,
        Err(AdmissionError::Provider(RegistryError::Eligibility(
            EligibilityReason::QuotaExhausted
        )))
    ));
    assert_eq!(f.count("provider_capacity_reservations").await, 1);
}
#[tokio::test]
async fn ignored_or_aborted_capacity_and_audit_writes_roll_back_real_finance_and_outbox() {
    for (table, kind, action) in [
        ("provider_capacity_reservations", "", "IGNORE"),
        (
            "provider_capacity_reservations",
            "",
            "ABORT,'synthetic storage failure'",
        ),
        (
            "provider_security_events",
            "WHEN NEW.kind='reserved'",
            "IGNORE",
        ),
    ] {
        let f = Fixture::new().await;
        let r = f.seeded().await;
        sqlx::query(&format!("CREATE TRIGGER fail_reservation BEFORE INSERT ON {table} {kind} BEGIN SELECT RAISE({action}); END")).execute(&f.store.pool).await.unwrap();
        denied(&f, &r).await;
        f.store.check_integrity().await.unwrap();
    }
}
#[tokio::test]
async fn revoked_or_changed_credential_and_corrupt_ciphertext_fail_before_admission() {
    for case in 0..3 {
        let f = Fixture::new().await;
        let r = f.seeded().await;
        let sql = match case {
            0 => "UPDATE secret_connections SET state='revoked',revision=revision+1",
            1 => "UPDATE secret_records SET current_version=current_version+1",
            _ => "DROP TRIGGER secret_version_no_update",
        };
        let result = sqlx::query(sql).execute(&f.store.pool).await;
        if case == 2 {
            // Storage corruption is injected only after removing its guard in a disposable fixture.
            let names:Vec<String>=sqlx::query_scalar("SELECT name FROM sqlite_schema WHERE type='trigger' AND tbl_name='secret_versions'").fetch_all(&f.store.pool).await.unwrap();
            for n in names {
                sqlx::query(&format!("DROP TRIGGER {n}"))
                    .execute(&f.store.pool)
                    .await
                    .unwrap();
            }
            sqlx::query("UPDATE secret_versions SET ciphertext=zeroblob(length(ciphertext))")
                .execute(&f.store.pool)
                .await
                .unwrap();
        } else if result.is_err() {
            // An attempted invalid transition being rejected also must not grant a different credential.
            let mut tx = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
            let id = ConnectionId::parse("synthetic-connection".into()).unwrap();
            let mut g = store::grant(&mut tx, &r.key, &id, &f.request.task.plan.project_id)
                .await
                .unwrap();
            g.version += 1;
            g.credential.version += 1;
            store::save_grant(&mut tx, &r.key, &g).await.unwrap();
            tx.commit().await.unwrap();
        }
        denied(&f, &r).await;
    }
}
#[tokio::test]
async fn provider_refresh_cannot_expand_another_projects_original_model_consent() {
    let f = Fixture::new().await;
    let registry = f.seeded().await;
    let id = ConnectionId::parse("synthetic-connection".into()).unwrap();
    let mut tx = f.store.pool.begin().await.unwrap();
    let mut r = store::connection(&mut tx, &registry.key, &id)
        .await
        .unwrap();
    let g = store::grant(&mut tx, &registry.key, &id, &f.request.task.plan.project_id)
        .await
        .unwrap();
    let mut e = g.entitlement.clone().unwrap();
    // A different project's Owner expanded account permissions. This project's
    // prior exact-action consent remains limited to the original planned model.
    r.models.push("synthetic/new-model".into());
    e.models[0].model = "synthetic/new-model".into();
    e.verified_against = r.version;
    e.scope_digest = registry.scope_digest(&id, &r, &g).unwrap();
    registry
        .validate_evidence(&id, &r, &e, &g, synthetic::T)
        .unwrap();
    let mut g = g;
    r.entitlement = Some(e.clone());
    r.version += 1;
    store::save_version(&mut tx, &registry.key, &id, &r)
        .await
        .unwrap();
    sqlx::query("UPDATE provider_connections SET version=? WHERE connection_id=?")
        .bind(r.version)
        .bind(id.as_str())
        .execute(&mut *tx)
        .await
        .unwrap();
    g.version += 1;
    g.verified_connection_version = Some(r.version);
    g.entitlement = Some(e);
    store::save_grant(&mut tx, &registry.key, &g).await.unwrap();
    tx.commit().await.unwrap();
    let mut read = f.store.pool.begin().await.unwrap();
    let mut assignment: ExecutionAssignment = serde_json::from_str(
        &sqlx::query_scalar::<_, String>(
            "SELECT json_extract(definition,'$.execution') FROM tasks WHERE task_id='task-a'",
        )
        .fetch_one(&mut *read)
        .await
        .unwrap(),
    )
    .unwrap();
    assignment.model = "synthetic/new-model".into();
    assert!(
        gate::eligible(
            &mut read,
            &registry,
            &f.request.task.plan.project_id,
            &assignment,
            &f.request.permit,
            synthetic::T
        )
        .await
        .is_err(),
        "provider catalogue refresh cannot grant a new project model"
    );
}
#[tokio::test]
async fn model_credentials_are_distinct_from_worker_infrastructure_and_never_fall_back() {
    let f = Fixture::new().await;
    let registry = f.seeded().await;
    let policy = registry.policy(synthetic::Authority::default());
    f.store
        .admit(&f.request, &policy, &f.clock(), Limits::default())
        .await
        .unwrap();
    let mut tx = f.store.pool.begin().await.unwrap();
    let text: String = sqlx::query_scalar(
        "SELECT json_extract(definition,'$.execution') FROM tasks WHERE task_id='task-a'",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let a: ExecutionAssignment = serde_json::from_str(&text).unwrap();
    gate::broker_connection(
        &mut tx,
        &policy,
        &f.request,
        &a,
        synthetic::T,
        gate::ModelConnection {
            connection: "synthetic-connection",
            account: "synthetic-model-account",
            provider: "synthetic-model-provider",
        },
    )
    .await
    .unwrap();
    assert!(
        gate::broker_connection(
            &mut tx,
            &policy,
            &f.request,
            &a,
            synthetic::T,
            gate::ModelConnection {
                connection: "synthetic-connection",
                account: "synthetic-account",
                provider: "synthetic"
            }
        )
        .await
        .is_err()
    );
    assert!(
        gate::broker_connection(
            &mut tx,
            &policy,
            &f.request,
            &a,
            synthetic::T,
            gate::ModelConnection {
                connection: "other-connection",
                account: "synthetic-model-account",
                provider: "synthetic-model-provider"
            }
        )
        .await
        .is_err()
    );
}
#[tokio::test]
async fn expiry_before_dispatch_prevents_even_provider_inspection_and_preserves_both_holds() {
    use crate::worker_control::*;
    let f = Fixture::new().await;
    let registry = f.seeded().await;
    change(&f, &registry, |e| e.expires_at = synthetic::T + 10).await;
    let policy = registry.policy(synthetic::Authority::default());
    let clock = f.clock();
    let a = f
        .store
        .admit(&f.request, &policy, &clock, Limits::default())
        .await
        .unwrap();
    let dispatcher = OutboxDispatcher::new(f.store.clone());
    let ticket = dispatcher
        .claim(
            &a.lease,
            &common::owner(),
            &policy,
            &clock,
            Limits::default(),
        )
        .await
        .unwrap();
    clock.advance_to(synthetic::T + 10).unwrap();
    assert!(
        dispatcher
            .dispatch(
                &ticket,
                &SyntheticProvider::new(f.store.clone()),
                &policy,
                &clock,
                Limits::default()
            )
            .await
            .is_err()
    );
    assert_eq!(f.count("synthetic_operations").await, 0);
    assert_eq!(f.count("provider_capacity_reservations").await, 1);
    assert_eq!(
        f.store
            .financial_snapshot(&f.request.task.plan.project_id)
            .await
            .unwrap()
            .held,
        synthetic::costs()
    );
    let blocked: i64 =
        sqlx::query_scalar("SELECT count(*) FROM provider_security_events WHERE kind='blocked'")
            .fetch_one(&f.store.pool)
            .await
            .unwrap();
    assert_eq!(blocked, 1);
}
#[tokio::test]
async fn authenticated_metadata_and_attribution_substitution_are_detected() {
    for case in 0..3 {
        let f = Fixture::new().await;
        let r = f.seeded().await;
        let policy = r.policy(synthetic::Authority::default());
        if case == 2 {
            f.store
                .admit(&f.request, &policy, &f.clock(), Limits::default())
                .await
                .unwrap();
            sqlx::query("DROP TRIGGER provider_capacity_guard")
                .execute(&f.store.pool)
                .await
                .unwrap();
            sqlx::query("UPDATE provider_capacity_reservations SET quota_units=quota_units+1")
                .execute(&f.store.pool)
                .await
                .unwrap();
            assert!(
                f.store
                    .admit(&f.request, &policy, &f.clock(), Limits::default())
                    .await
                    .is_err()
            );
        } else {
            sqlx::query("DROP TRIGGER provider_grant_guard")
                .execute(&f.store.pool)
                .await
                .unwrap();
            let sql = if case == 0 {
                "UPDATE provider_project_grants SET record=json_set(record,'$.credential.connection','different-credential')"
            } else {
                "UPDATE provider_project_grants SET mac=zeroblob(32)"
            };
            sqlx::query(sql).execute(&f.store.pool).await.unwrap();
            denied(&f, &r).await;
        }
    }
}
#[tokio::test]
async fn current_api_and_cli_mode_never_fall_back_to_host_credentials_or_another_account() {
    let f = Fixture::new().await;
    let registry = f.seeded().await;
    let mut tx = f.store.pool.begin().await.unwrap();
    let id = ConnectionId::parse("synthetic-connection".into()).unwrap();
    let r = store::connection(&mut tx, &registry.key, &id)
        .await
        .unwrap();
    let mut a: ExecutionAssignment = serde_json::from_str(
        &sqlx::query_scalar::<_, String>(
            "SELECT json_extract(definition,'$.execution') FROM tasks WHERE task_id='task-a'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap(),
    )
    .unwrap();
    a.access_mode = AccessMode::CliSession;
    assert!(
        gate::eligible(
            &mut tx,
            &registry,
            &f.request.task.plan.project_id,
            &a,
            &f.request.permit,
            synthetic::T
        )
        .await
        .is_err()
    );
    a.access_mode = AccessMode::ApiKey;
    a.connection_ref = "unenrolled-fallback".into();
    assert!(
        gate::eligible(
            &mut tx,
            &registry,
            &f.request.task.plan.project_id,
            &a,
            &f.request.permit,
            synthetic::T
        )
        .await
        .is_err()
    );
    assert_eq!(r.mode, AccessMode::ApiKey);
}
struct FinalProvider;
impl reconciliation::ReconciliationAuthority for FinalProvider {
    fn verify<'a>(
        &'a self,
        c: &'a reconciliation::ReconciliationCheck<'a>,
        t: i64,
    ) -> reconciliation::ReconciliationFuture<'a> {
        Box::pin(async move {
            if c.request.evidence.as_str() != "synthetic-model-final" {
                return Err(RegistryError::Denied);
            }
            Ok(reconciliation::ReconciliationGrant {
                digest: hex(&Sha256::digest(c.attribution.as_bytes())),
                provider_operation: "synthetic-model-operation".into(),
                terminated: true,
                billing_final: true,
                expires_at: t + 60,
            })
        })
    }
    fn current(
        &self,
        c: &reconciliation::ReconciliationCheck<'_>,
        g: &reconciliation::ReconciliationGrant,
        t: i64,
    ) -> Result<(), RegistryError> {
        if c.request.evidence.as_str() != "synthetic-model-final"
            || g.digest != hex(&Sha256::digest(c.attribution.as_bytes()))
            || t >= g.expires_at
        {
            return Err(RegistryError::Denied);
        }
        Ok(())
    }
}
#[tokio::test]
async fn slot_release_requires_separate_final_model_provider_and_financial_reconciliation() {
    use crate::worker_control::*;
    let f = Fixture::new().await;
    let mut registry = f.seeded().await;
    Arc::get_mut(&mut registry).unwrap().reconciliation = Arc::new(FinalProvider);
    let policy = registry.policy(synthetic::Authority::default());
    let clock = f.clock();
    let a = f
        .store
        .admit(&f.request, &policy, &clock, Limits::default())
        .await
        .unwrap();
    let d = OutboxDispatcher::new(f.store.clone());
    let ticket = d
        .claim(
            &a.lease,
            &common::owner(),
            &policy,
            &clock,
            Limits::default(),
        )
        .await
        .unwrap();
    let provider = SyntheticProvider::new(f.store.clone());
    d.dispatch(&ticket, &provider, &policy, &clock, Limits::default())
        .await
        .unwrap();
    f.store
        .fence_attempt(
            &FenceRequest {
                lease: a.lease.clone(),
                request_id: RequestId::new("retire-execution").unwrap(),
                actor: common::owner(),
                evidence: synthetic::e("synthetic-cancellation"),
            },
            &policy,
            &clock,
        )
        .await
        .unwrap();
    let r = CapacityRelease {
        lease: a.lease.clone(),
        request_id: RequestId::new("capacity-final").unwrap(),
        evidence: synthetic::e("synthetic-model-final"),
    };
    assert!(
        registry.reconcile_capacity(&r, &clock).await.is_err(),
        "cancellation is not termination or final billing"
    );
    provider
        .observe(
            &OperationKey {
                project: a.lease.task.plan.project_id.clone(),
                attempt: a.lease.attempt_id.clone(),
            },
            1,
            ProviderState::Finished,
            false,
            &synthetic::e("synthetic-worker-final"),
        )
        .await
        .unwrap();
    d.reconcile(&a.lease, &provider, &policy, &clock, Limits::default())
        .await
        .unwrap();
    assert!(
        registry.reconcile_capacity(&r, &clock).await.is_err(),
        "worker termination and model final proof do not release financial holds"
    );
    let balance = f
        .store
        .financial_liability(
            &a.lease.task.plan.project_id,
            &LiabilityId::new(a.lease.attempt_id.as_str()).unwrap(),
        )
        .await
        .unwrap();
    f.store
        .apply_financial(
            &FinancialRequest {
                project: a.lease.task.plan.project_id.clone(),
                request_id: RequestId::new("synthetic-final-billing").unwrap(),
                actor: common::owner(),
                reason: "Synthetic final billing evidence; no real charges".into(),
                command: FinancialCommand::Reconcile {
                    liability: LiabilityId::new(a.lease.attempt_id.as_str()).unwrap(),
                    expected_revision: balance.revision,
                    settled: synthetic::zero(),
                    remaining_hold: synthetic::zero(),
                    remaining_provisional: synthetic::zero(),
                    state: ReconciliationState::Final,
                    evidence: synthetic::e("synthetic-accounting-final"),
                },
            },
            &synthetic::Finance,
        )
        .await
        .unwrap();
    let first = registry.reconcile_capacity(&r, &clock).await.unwrap();
    assert_eq!(
        registry.reconcile_capacity(&r, &clock).await.unwrap(),
        first
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM provider_capacity_reservations")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        "released"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT sum(quota_units) FROM provider_capacity_reservations")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        1
    );
}
#[tokio::test]
async fn provider_policy_revision_cannot_regress_or_change_under_the_same_revision_number() {
    let f = Fixture::new().await;
    let r = f.seeded().await;
    let id = ConnectionId::parse("synthetic-connection".into()).unwrap();
    let p = &f.request.task.plan.project_id;
    let mut tx = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let record = store::connection(&mut tx, &r.key, &id).await.unwrap();
    let g = store::grant(&mut tx, &r.key, &id, p).await.unwrap();
    let mut e = g.entitlement.clone().unwrap();
    e.verified_against = record.version;
    e.scope_digest = r.scope_digest(&id, &record, &g).unwrap();
    e.policy_version = 2;
    e.revision = "synthetic-policy-v2".into();
    e.evidence = synthetic::e("synthetic-revision-two");
    r.apply_verification(&mut tx, p, &record, &e, &e.evidence, synthetic::T)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    for regression in [true, false] {
        let mut tx = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        let record = store::connection(&mut tx, &r.key, &id).await.unwrap();
        let g = store::grant(&mut tx, &r.key, &id, p).await.unwrap();
        let mut e = g.entitlement.clone().unwrap();
        e.verified_against = record.version;
        e.scope_digest = r.scope_digest(&id, &record, &g).unwrap();
        if regression {
            e.policy_version = 1;
        } else {
            e.revision = "contradictory-policy-v2".into();
        }
        assert!(
            r.apply_verification(&mut tx, p, &record, &e, &e.evidence, synthetic::T)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
    }
}
#[tokio::test]
async fn unknown_financial_exposure_blocks_an_otherwise_eligible_independent_attempt() {
    let f = Fixture::new().await;
    let r = f.seeded().await;
    let policy = r.policy(synthetic::Authority::default());
    f.store
        .admit(&f.request, &policy, &f.clock(), Limits::default())
        .await
        .unwrap();
    let balance = f
        .store
        .financial_liability(
            &f.request.task.plan.project_id,
            &LiabilityId::new(f.request.request_id.as_str()).unwrap(),
        )
        .await
        .unwrap();
    f.store
        .apply_financial(
            &FinancialRequest {
                project: f.request.task.plan.project_id.clone(),
                request_id: RequestId::new("unknown-model-charge").unwrap(),
                actor: common::owner(),
                reason: "Synthetic unknown external exposure".into(),
                command: FinancialCommand::Report {
                    liability: LiabilityId::new(f.request.request_id.as_str()).unwrap(),
                    expected_revision: balance.revision,
                    unreserved: synthetic::zero(),
                    uncertain: true,
                    evidence: synthetic::e("synthetic-unknown-model-liability"),
                },
            },
            &synthetic::Finance,
        )
        .await
        .unwrap();
    assert!(
        f.store
            .admit(&second(&f), &policy, &f.clock(), Limits::default())
            .await
            .is_err()
    );
    assert_eq!(f.count("provider_capacity_reservations").await, 1);
    assert_eq!(f.count("dispatch_outbox").await, 1);
    assert_eq!(
        f.store
            .financial_snapshot(&f.request.task.plan.project_id)
            .await
            .unwrap()
            .held,
        synthetic::costs()
    );
}
#[tokio::test]
async fn credential_marker_never_enters_registry_database_wal_or_consistent_snapshot() {
    use crate::controller_keys::secret_broker as b;
    let f = Fixture::new().await;
    let k = CredentialReference {
        connection: "marker-connection".into(),
        account: "marker-account".into(),
        provider: "marker-provider".into(),
        record: "marker-record".into(),
        purpose: "marker-purpose".into(),
        version: 1,
    };
    let scope = b::RecordScope {
        connection: b::ConnectionScope {
            project: f.request.task.plan.project_id.clone(),
            connection: b::ConnectionId::new(k.connection).unwrap(),
            account: b::AccountId::new(k.account).unwrap(),
            provider: b::ProviderId::new(k.provider).unwrap(),
            mode: b::ProviderMode::ApiKey,
        },
        record: b::RecordId::new(k.record).unwrap(),
        purpose: b::SecretPurpose::new(k.purpose).unwrap(),
    };
    let marker = zeroize::Zeroizing::new(format!(
        "synthetic-marker-{}",
        hex(&b::os_random::<32>().unwrap()[..])
    ));
    let mut tx = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    b::fixture_record(
        &mut tx,
        &f.registry.key,
        &scope,
        &b::SecretValue::new(marker.as_bytes().to_vec()).unwrap(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let snapshot = f._root.path().join("ordinary.sqlite");
    f.store.snapshot(&snapshot).await.unwrap();
    for path in [
        snapshot,
        f._root.path().join("data/state.sqlite"),
        f._root.path().join("data/state.sqlite-wal"),
    ] {
        if path.exists() {
            let bytes = fs::read(path).unwrap();
            assert!(!bytes.windows(marker.len()).any(|w| w == marker.as_bytes()));
        }
    }
}
#[tokio::test]
async fn cli_native_login_policy_citation_and_unverified_isolation_never_establish_hosted_permission()
 {
    let f = Fixture::new().await;
    let registry = f.seeded().await;
    let mut tx = f.store.pool.begin().await.unwrap();
    let id = ConnectionId::parse("synthetic-connection".into()).unwrap();
    let mut r = store::connection(&mut tx, &registry.key, &id)
        .await
        .unwrap();
    let g = store::grant(&mut tx, &registry.key, &id, &f.request.task.plan.project_id)
        .await
        .unwrap();
    r.mode = AccessMode::CliSession;
    let mut allowed = g.entitlement.unwrap();
    allowed.mode = AccessMode::CliSession;
    allowed.policy = PolicyClass::ThirdPartySpecificallyPermitted;
    allowed.permission = AccountPermission::VerifiedHostedCli;
    allowed.cli = Some(CliIsolation {
        home_identity: "isolated-synthetic-home".into(),
        revision: "synthetic-v1".into(),
    });
    gate::verify_claims(&r, &allowed, synthetic::T).unwrap();
    for case in 0..7 {
        let mut e = allowed.clone();
        match case {
            0 => e.policy = PolicyClass::NativeClientOnly,
            1 => e.permission = AccountPermission::NativeLoginOnly,
            2 => e.policy = PolicyClass::RegistrationRequired,
            3 => e.cli = None,
            4 => e.refresh_serialized = false,
            5 => e.permission = AccountPermission::AwaitingProof,
            _ => e.policy = PolicyClass::Disallowed,
        };
        assert!(gate::verify_claims(&r, &e, synthetic::T).is_err());
    }
}
#[tokio::test]
async fn refresh_lock_survives_its_deadline_and_blocks_only_its_own_account() {
    let f = Fixture::new().await;
    let r = f.seeded().await;
    sqlx::query("INSERT INTO provider_refresh_locks VALUES('synthetic-connection','uncertain-refresh',2,'in_progress',?)").bind(synthetic::T-1).execute(&f.store.pool).await.unwrap();
    assert!(sqlx::query("INSERT INTO provider_refresh_locks VALUES('synthetic-connection','second-refresh',2,'in_progress',?)").bind(synthetic::T+300).execute(&f.store.pool).await.is_err());
    denied(&f, &r).await;
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM provider_refresh_locks")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        "in_progress"
    );
}
struct RevocableCurrent(Arc<std::sync::atomic::AtomicBool>);
impl EntitlementVerifier for RevocableCurrent {
    fn verify<'a>(&'a self, _: &'a VerificationCheck<'a>) -> VerificationFuture<'a> {
        Box::pin(async { Err(RegistryError::Unavailable) })
    }
    fn current(
        &self,
        c: &VerificationCheck<'_>,
        e: &Entitlement,
        t: i64,
    ) -> Result<(), RegistryError> {
        if !self.0.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(RegistryError::Denied);
        }
        FixtureCurrent(Default::default()).current(c, e, t)
    }
}
struct WithdrawAtFinalTime {
    current: Arc<std::sync::atomic::AtomicBool>,
    calls: std::sync::atomic::AtomicUsize,
}
impl Clock for WithdrawAtFinalTime {
    fn now(&self) -> Result<i64, AdmissionError> {
        if self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) > 0 {
            self.current
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(synthetic::T)
    }
}
#[tokio::test]
async fn provider_revocation_between_admission_check_and_final_commit_rolls_back_all_authority() {
    let f = Fixture::new().await;
    let mut r = f.seeded().await;
    let current = Arc::new(std::sync::atomic::AtomicBool::new(true));
    Arc::get_mut(&mut r).unwrap().verifier = Arc::new(RevocableCurrent(current.clone()));
    let clock = WithdrawAtFinalTime {
        current,
        calls: std::sync::atomic::AtomicUsize::new(0),
    };
    assert!(
        f.store
            .admit(
                &f.request,
                &r.policy(synthetic::Authority::default()),
                &clock,
                Limits::default()
            )
            .await
            .is_err()
    );
    for table in [
        "task_attempts",
        "dispatch_outbox",
        "provider_capacity_reservations",
    ] {
        assert_eq!(f.count(table).await, 0);
    }
    assert_eq!(
        f.store
            .financial_snapshot(&f.request.task.plan.project_id)
            .await
            .unwrap()
            .held
            .total()
            .unwrap()
            .minor_units(),
        0
    );
}
#[tokio::test]
async fn actual_sqlite_writer_contention_is_bounded_and_does_not_reserve_capacity() {
    let f = Fixture::new().await;
    let r = f.seeded().await;
    let policy = r.policy(synthetic::Authority::default());
    let blocker = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let result = f
        .store
        .admit(&f.request, &policy, &f.clock(), Limits::default())
        .await;
    assert!(matches!(
        result,
        Err(AdmissionError::Storage(StoreError::Busy))
    ));
    blocker.rollback().await.unwrap();
    for table in [
        "task_attempts",
        "dispatch_outbox",
        "provider_capacity_reservations",
    ] {
        assert_eq!(f.count(table).await, 0);
    }
    f.store
        .admit(&f.request, &policy, &f.clock(), Limits::default())
        .await
        .unwrap();
    assert_eq!(f.count("provider_capacity_reservations").await, 1);
}

#[tokio::test]
async fn numeric_policy_revision_invalidates_old_entitlement_even_when_text_is_unchanged() {
    let f = Fixture::new().await;
    let registry = f.seeded().await;
    sqlx::query("UPDATE provider_policy_revisions SET version=version+1")
        .execute(&f.store.pool)
        .await
        .unwrap();
    denied(&f, &registry).await;
}
