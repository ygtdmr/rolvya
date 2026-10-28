use super::*;
#[derive(Clone, Copy)]
enum WriteFault {
    Abort,
    Ignore,
}
impl std::fmt::Display for WriteFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Abort => "ABORT,'synthetic write failure'",
            Self::Ignore => "IGNORE",
        })
    }
}

#[tokio::test]
async fn preparation_write_failures_leave_no_partial_enrollment_or_challenge() {
    for behavior in [WriteFault::Abort, WriteFault::Ignore] {
        for table in [
            "worker_enrollments",
            "worker_bootstrap_challenges",
            "worker_security_events",
        ] {
            let f = Fixture::new().await;
            let before = f
                .store
                .financial_snapshot(&f.binding.task.plan.project_id)
                .await
                .unwrap();
            sqlx::query(&format!(
                "CREATE TRIGGER fault BEFORE INSERT ON {table} BEGIN SELECT RAISE({behavior}); END"
            ))
            .execute(&f.store.pool)
            .await
            .unwrap();
            assert!(
                f.security
                    .prepare_bootstrap(&f.request("prepare"))
                    .await
                    .is_err()
            );
            assert_eq!(f.rows("worker_enrollments").await, 0);
            assert_eq!(f.rows("worker_bootstrap_challenges").await, 0);
            assert_eq!(f.rows("worker_security_events").await, 0);
            assert_eq!(
                f.store
                    .financial_snapshot(&f.binding.task.plan.project_id)
                    .await
                    .unwrap()
                    .held,
                before.held
            );
        }
    }
}
#[tokio::test]
async fn bootstrap_ignored_and_aborted_writes_preserve_the_unconsumed_challenge() {
    for behavior in [WriteFault::Abort, WriteFault::Ignore] {
        for (operation, table) in [
            ("INSERT", "worker_credentials"),
            ("UPDATE", "worker_enrollments"),
            ("UPDATE", "worker_bootstrap_challenges"),
            ("INSERT", "worker_security_events"),
        ] {
            let f = Fixture::new().await;
            let boot = f
                .security
                .prepare_bootstrap(&f.request("prepare"))
                .await
                .unwrap();
            let c = f.context("claim", WorkerOperation::Register);
            let e = synthetic::e("proof");
            f.grant(VerificationStage::Bootstrap, &c, &e, Some(&boot.id), None);
            sqlx::query(&format!("CREATE TRIGGER fault BEFORE {operation} ON {table} BEGIN SELECT RAISE({behavior}); END")).execute(&f.store.pool).await.unwrap();
            assert!(
                f.security
                    .claim_bootstrap(BootstrapClaim {
                        id: &boot.id,
                        secret: boot.secret.as_ref().unwrap(),
                        evidence: &e,
                        context: &c
                    })
                    .await
                    .is_err()
            );
            assert_eq!(f.rows("worker_credentials").await, 0);
            assert_eq!(
                sqlx::query_scalar::<_, String>("SELECT state FROM worker_bootstrap_challenges")
                    .fetch_one(&f.store.pool)
                    .await
                    .unwrap(),
                "pending"
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT current_generation FROM worker_enrollments")
                    .fetch_one(&f.store.pool)
                    .await
                    .unwrap(),
                0
            );
        }
    }
}
#[tokio::test]
async fn message_write_failures_roll_back_receipt_lease_and_authenticated_request_together() {
    for behavior in [WriteFault::Abort, WriteFault::Ignore] {
        for (operation, table) in [
            ("INSERT", "worker_messages"),
            ("INSERT", "worker_receipts"),
            ("UPDATE", "worker_leases"),
            ("INSERT", "worker_credential_requests"),
            ("INSERT", "worker_security_events"),
        ] {
            let f = Fixture::new().await;
            let t = f.issued().await;
            f.executing(&t).await;
            f.clock.advance_to(synthetic::T + 30).unwrap();
            sqlx::query(&format!("CREATE TRIGGER fault BEFORE {operation} ON {table} BEGIN SELECT RAISE({behavior}); END")).execute(&f.store.pool).await.unwrap();
            assert!(
                f.send(&t, &f.message(4, WorkerBody::Heartbeat {}))
                    .await
                    .is_err()
            );
            assert_eq!(f.rows("worker_messages").await, 3);
            assert_eq!(f.rows("worker_receipts").await, 3);
            assert_eq!(f.rows("worker_credential_requests").await, 3);
            assert_eq!(
                DurableWorkerController::new(f.store.clone())
                    .lease_until(&f.lease)
                    .await
                    .unwrap(),
                synthetic::T + 90
            );
            sqlx::query("DROP TRIGGER fault")
                .execute(&f.store.pool)
                .await
                .unwrap();
            f.send(&t, &f.message(4, WorkerBody::Heartbeat {}))
                .await
                .unwrap();
        }
    }
}
#[tokio::test]
async fn revoked_state_fence_and_audit_roll_back_on_ignored_or_aborted_revocation_writes() {
    for behavior in [WriteFault::Abort, WriteFault::Ignore] {
        for (operation, table) in [
            ("INSERT", "worker_revocations"),
            ("INSERT", "worker_security_events"),
            ("UPDATE", "worker_enrollments"),
        ] {
            let f = Fixture::new().await;
            let t = f.issued().await;
            f.executing(&t).await;
            sqlx::query(&format!("CREATE TRIGGER fault BEFORE {operation} ON {table} BEGIN SELECT RAISE({behavior}); END")).execute(&f.store.pool).await.unwrap();
            assert!(
                f.security
                    .revoke_worker(&f.revoke_request(&t, "revoke"))
                    .await
                    .is_err()
            );
            assert_eq!(f.rows("worker_revocations").await, 0);
            assert_eq!(
                sqlx::query_scalar::<_, String>("SELECT state FROM attempt_states")
                    .fetch_one(&f.store.pool)
                    .await
                    .unwrap(),
                "admitted"
            );
            assert_eq!(
                sqlx::query_scalar::<_, String>("SELECT state FROM worker_enrollments")
                    .fetch_one(&f.store.pool)
                    .await
                    .unwrap(),
                "active"
            );
            assert_eq!(
                f.store
                    .financial_snapshot(&f.binding.task.plan.project_id)
                    .await
                    .unwrap()
                    .held,
                synthetic::costs()
            );
        }
    }
}
#[tokio::test]
async fn ignored_pending_challenge_retirement_is_detected_before_fence_commit() {
    let f = Fixture::new().await;
    let boot = f
        .security
        .prepare_bootstrap(&f.request("prepare"))
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER fault BEFORE UPDATE ON worker_bootstrap_challenges WHEN NEW.state='revoked' BEGIN SELECT RAISE(IGNORE); END").execute(&f.store.pool).await.unwrap();
    let r = WorkerRevocation {
        enrollment: boot.enrollment,
        expected_security_epoch: 1,
        request_id: RequestId::new("revoke-pending").unwrap(),
        evidence: synthetic::e("trusted-controller-incident"),
    };
    f.revocation.0.lock().unwrap().insert(
        r.request_id.as_str().into(),
        (
            r.enrollment.clone(),
            1,
            r.evidence.clone(),
            f.binding.clone(),
        ),
    );
    assert!(f.security.revoke_worker(&r).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM attempt_states")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        "admitted"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM worker_bootstrap_challenges")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        "pending"
    );
}
#[tokio::test]
async fn ignored_retirement_audit_rolls_back_trusted_acceptance_instead_of_partial_success() {
    let f = Fixture::new().await;
    let t = f.issued().await;
    f.executing(&t).await;
    let bytes = b"independently checked synthetic artifact";
    let manifest = serde_json::json!({"artifact_sha256":format!("{:x}",Sha256::digest(bytes)),"binding":f.binding,"format":1});
    let digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&manifest).unwrap())
    );
    f.send(
        &t,
        &f.message(
            4,
            WorkerBody::SubmitResult {
                outcome: ExecutionOutcome::Succeeded {
                    manifest_sha256: rolvya_domain::worker::Digest::new(&digest).unwrap(),
                },
            },
        ),
    )
    .await
    .unwrap();
    sqlx::query("CREATE TRIGGER fault BEFORE INSERT ON worker_security_events WHEN NEW.kind='retired' BEGIN SELECT RAISE(IGNORE); END").execute(&f.store.pool).await.unwrap();
    // Independent synthetic validator evidence is separate from the worker result.
    // This test deliberately rolls back; it does not claim a durable R09 artifact.
    let request = admission::ValidationRequest {
        task: f.lease.task.clone(),
        fingerprint: f.binding.plan_hash.as_str().into(),
        lease: Some(f.lease.clone()),
        request_id: RequestId::new("independent-validation").unwrap(),
        actor: crate::PrincipalId::new("synthetic-validator").unwrap(),
        evidence: synthetic::e("independent-synthetic-byte-check"),
        manifest_sha256: digest,
    };
    assert_eq!(
        f.store
            .accept_task(
                &request,
                &synthetic::Authority::default(),
                f.clock.as_ref(),
                Limits::default()
            )
            .await
            .unwrap_err(),
        admission::AdmissionError::Storage(StoreError::Integrity)
    );
    assert_eq!(f.rows("task_acceptances").await, 0);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM attempt_states")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        "admitted"
    );
}
#[tokio::test]
async fn heartbeat_final_commit_cannot_use_its_new_lease_after_original_authority_expires() {
    let mut f = Fixture::new().await;
    let t = f.issued().await;
    f.executing(&t).await;
    f.clock.advance_to(synthetic::T + 60).unwrap();
    let clock = f.clock.clone();
    f.security.hook = Box::new(move |p| {
        if p == CommitCheckpoint::BeforeMessage {
            clock.advance_to(synthetic::T + 90).unwrap();
        }
        Ok(())
    });
    assert!(
        f.send(&t, &f.message(4, WorkerBody::Heartbeat {}))
            .await
            .is_err()
    );
    assert_eq!(f.rows("worker_messages").await, 3);
    assert_eq!(
        DurableWorkerController::new(f.store.clone())
            .lease_until(&f.lease)
            .await
            .unwrap(),
        synthetic::T + 90
    );
}
#[tokio::test]
async fn withdrawn_identity_or_transport_at_final_checkpoint_cannot_mint_bootstrap_or_rotation() {
    for checkpoint in [
        CommitCheckpoint::BeforePrepare,
        CommitCheckpoint::BeforeBootstrap,
        CommitCheckpoint::BeforeRenew,
    ] {
        let mut f = Fixture::new().await;
        if checkpoint == CommitCheckpoint::BeforePrepare {
            let r = f.request("prepare");
            let authority = f.identity.clone();
            f.security.hook = Box::new(move |p| {
                if p == checkpoint {
                    authority.0.lock().unwrap().clear();
                }
                Ok(())
            });
            assert!(f.security.prepare_bootstrap(&r).await.is_err());
            assert_eq!(f.rows("worker_enrollments").await, 0);
        } else if checkpoint == CommitCheckpoint::BeforeBootstrap {
            let boot = f
                .security
                .prepare_bootstrap(&f.request("prepare"))
                .await
                .unwrap();
            let c = f.context("claim", WorkerOperation::Register);
            let e = synthetic::e("proof");
            f.grant(VerificationStage::Bootstrap, &c, &e, Some(&boot.id), None);
            let authority = f.transport.clone();
            f.security.hook = Box::new(move |p| {
                if p == checkpoint {
                    authority.0.lock().unwrap().clear();
                }
                Ok(())
            });
            assert!(
                f.security
                    .claim_bootstrap(BootstrapClaim {
                        id: &boot.id,
                        secret: boot.secret.as_ref().unwrap(),
                        evidence: &e,
                        context: &c
                    })
                    .await
                    .is_err()
            );
            assert_eq!(f.rows("worker_credentials").await, 0);
        } else {
            let t = f.issued().await;
            f.executing(&t).await;
            f.clock.advance_to(synthetic::T + 60).unwrap();
            let (c, e) = f.renewal(&t, "renew");
            let authority = f.identity.clone();
            f.security.hook = Box::new(move |p| {
                if p == checkpoint {
                    authority.0.lock().unwrap().clear();
                }
                Ok(())
            });
            assert!(
                f.security
                    .renew_credential(&t.secret, &e, &c)
                    .await
                    .is_err()
            );
            assert_eq!(f.rows("worker_credentials").await, 1);
            f.send(&t, &f.message(4, WorkerBody::Heartbeat {}))
                .await
                .unwrap();
        }
    }
}
#[tokio::test]
async fn sqlite_contention_fails_without_partial_authority_then_allows_safe_retry() {
    let f = Fixture::new().await;
    let r = f.request("prepare");
    let mut connections = Vec::new();
    for _ in 0..4 {
        let mut c = f.store.pool.acquire().await.unwrap();
        sqlx::query("PRAGMA busy_timeout=40")
            .execute(&mut *c)
            .await
            .unwrap();
        connections.push(c);
    }
    drop(connections);
    let mut blocker = sqlx::SqliteConnection::connect_with(
        &sqlx::sqlite::SqliteConnectOptions::new().filename(&f.path),
    )
    .await
    .unwrap();
    use sqlx::Connection;
    let tx = blocker.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let started = std::time::Instant::now();
    assert_eq!(
        f.security.prepare_bootstrap(&r).await.unwrap_err(),
        WorkerAuthError::Busy
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
    assert_eq!(f.rows("worker_enrollments").await, 0);
    tx.rollback().await.unwrap();
    f.security.prepare_bootstrap(&r).await.unwrap();
}
struct WriterProbingIdentity {
    pool: sqlx::SqlitePool,
    base: Arc<SyntheticIdentity>,
}
impl IdentityAuthority for WriterProbingIdentity {
    fn verify<'a>(&'a self, c: &'a IdentityCheck<'a>, now: i64) -> VerificationFuture<'a> {
        Box::pin(async move {
            let tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
            tx.rollback().await?;
            self.base.verify(c, now).await
        })
    }
    fn current(
        &self,
        c: &IdentityCheck<'_>,
        g: &IdentityGrant,
        now: i64,
    ) -> Result<(), WorkerAuthError> {
        self.base.current(c, g, now)
    }
}
#[tokio::test]
async fn provider_identity_verification_never_runs_under_the_controller_write_lock() {
    let mut f = Fixture::new().await;
    f.security.identity = Arc::new(WriterProbingIdentity {
        pool: f.store.pool.clone(),
        base: f.identity.clone(),
    });
    let t = f.issued().await;
    f.executing(&t).await;
    f.clock.advance_to(synthetic::T + 60).unwrap();
    let (c, e) = f.renewal(&t, "renew");
    f.security
        .renew_credential(&t.secret, &e, &c)
        .await
        .unwrap();
}
#[tokio::test]
async fn heartbeat_and_result_fencing_races_never_restore_retired_authority() {
    for result in [false, true] {
        let f = Fixture::new().await;
        let t = f.issued().await;
        f.executing(&t).await;
        f.clock.advance_to(synthetic::T + 30).unwrap();
        let body = if result {
            WorkerBody::SubmitResult {
                outcome: ExecutionOutcome::Failed {
                    class: FailureClass::Transient,
                },
            }
        } else {
            WorkerBody::Heartbeat {}
        };
        let m = f.message(4, body);
        let r = f.revoke_request(&t, "incident");
        let (_, revoked) = tokio::join!(f.send(&t, &m), f.security.revoke_worker(&r));
        revoked.unwrap();
        assert!(f.send(&t, &m).await.is_err());
        assert_eq!(f.rows("task_acceptances").await, 0);
        assert_eq!(
            f.store
                .financial_snapshot(&f.binding.task.plan.project_id)
                .await
                .unwrap()
                .held,
            synthetic::costs()
        );
    }
}
#[tokio::test]
async fn immutable_records_and_mac_verification_reject_storage_substitution() {
    let f = Fixture::new().await;
    let t = f.issued().await;
    for sql in [
        "UPDATE worker_credentials SET record_mac=zeroblob(32)",
        "DELETE FROM worker_credentials",
        "UPDATE worker_enrollments SET record='{}'",
        "DELETE FROM worker_bootstrap_challenges",
        "UPDATE worker_security_events SET request_id='changed'",
    ] {
        assert!(sqlx::query(sql).execute(&f.store.pool).await.is_err());
    }
    sqlx::query("DROP TRIGGER worker_credential_no_update")
        .execute(&f.store.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE worker_credentials SET record_mac=zeroblob(32)")
        .execute(&f.store.pool)
        .await
        .unwrap();
    assert!(
        f.send(
            &t,
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
    assert_eq!(f.rows("worker_messages").await, 0);
}
#[tokio::test]
async fn an_ignored_nested_credential_retirement_cannot_be_reported_as_a_successful_fence() {
    let f = Fixture::new().await;
    let t = f.issued().await;
    f.executing(&t).await;
    sqlx::query("CREATE TRIGGER fault_retirement BEFORE UPDATE ON worker_enrollments WHEN NEW.state='revoked' BEGIN SELECT RAISE(IGNORE); END").execute(&f.store.pool).await.unwrap();
    let request = admission::FenceRequest {
        lease: f.lease.clone(),
        request_id: RequestId::new("nested-fence").unwrap(),
        actor: common::owner(),
        evidence: synthetic::e("verified-controller-fence"),
    };
    assert!(
        f.store
            .fence_attempt(&request, &synthetic::Authority::default(), f.clock.as_ref())
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM attempt_states")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        "admitted"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM worker_enrollments")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        "active"
    );
    sqlx::query("DROP TRIGGER fault_retirement")
        .execute(&f.store.pool)
        .await
        .unwrap();
    f.store
        .fence_attempt(&request, &synthetic::Authority::default(), f.clock.as_ref())
        .await
        .unwrap();
    assert!(
        f.send(&t, &f.message(4, WorkerBody::Heartbeat {}))
            .await
            .is_err()
    );
}
#[tokio::test]
async fn unicode_escaped_bearer_reflection_cannot_enter_the_durable_journal() {
    let f = Fixture::new().await;
    let t = f.issued().await;
    f.executing(&t).await;
    let mut m = f.message(4, WorkerBody::Heartbeat {});
    m.message_id = MessageId::new(t.secret.0.as_str()).unwrap();
    let encoded = String::from_utf8(encode_worker_message(&m).unwrap()).unwrap();
    let disguised: String = t.secret.0.bytes().map(|b| format!("\\u{b:04x}")).collect();
    let wire = encoded.replace(t.secret.0.as_str(), &disguised);
    assert!(!wire.contains(t.secret.0.as_str()));
    let c = f.context("escaped-message", WorkerOperation::Heartbeat);
    assert!(
        f.security
            .receive(&t.secret, wire.as_bytes(), &c)
            .await
            .is_err()
    );
    assert_eq!(f.rows("worker_messages").await, 3);
    let snapshot = f.root.path().join("snapshot.sqlite");
    f.store.snapshot(&snapshot).await.unwrap();
    for p in [&f.path, &snapshot] {
        assert!(
            !fs::read(p)
                .unwrap()
                .windows(t.secret.0.len())
                .any(|w| w == t.secret.0.as_bytes())
        );
    }
}
#[tokio::test]
async fn known_authenticated_denials_have_redacted_idempotent_security_evidence() {
    let f = Fixture::new().await;
    let t = f.issued().await;
    let mut caps = synthetic::caps();
    caps.evidence = None;
    let m = f.message(1, WorkerBody::Register { capabilities: caps });
    assert!(f.send(&t, &m).await.is_err());
    assert!(f.send(&t, &m).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM worker_security_events WHERE kind='denied'"
        )
        .fetch_one(&f.store.pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(f.rows("worker_messages").await, 0);
    assert_eq!(f.rows("worker_credentials").await, 1);
}
