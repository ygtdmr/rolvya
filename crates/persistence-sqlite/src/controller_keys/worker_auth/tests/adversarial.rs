use super::*;

#[tokio::test]
async fn bootstrap_denies_unknown_identity_wrong_secret_and_reused_evidence_without_spending() {
    let f = Fixture::new().await;
    let request = f.request("prepare");
    f.identity.0.lock().unwrap().clear();
    assert!(f.security.prepare_bootstrap(&request).await.is_err());
    assert_eq!(f.rows("worker_enrollments").await, 0);
    let ticket = f
        .security
        .prepare_bootstrap(&f.request("prepare"))
        .await
        .unwrap();
    let c = f.context("claim", WorkerOperation::Register);
    let e = synthetic::e("bootstrap-proof");
    assert!(
        f.security
            .claim_bootstrap(BootstrapClaim {
                id: &ticket.id,
                secret: ticket.secret.as_ref().unwrap(),
                evidence: &e,
                context: &c
            })
            .await
            .is_err()
    );
    f.grant(VerificationStage::Bootstrap, &c, &e, Some(&ticket.id), None);
    let wrong = BootstrapSecret(bearer().unwrap());
    assert!(
        f.security
            .claim_bootstrap(BootstrapClaim {
                id: &ticket.id,
                secret: &wrong,
                evidence: &e,
                context: &c
            })
            .await
            .is_err()
    );
    let mut wrong_id = ticket.id.clone();
    wrong_id.0 = new_id().unwrap();
    assert!(
        f.security
            .claim_bootstrap(BootstrapClaim {
                id: &wrong_id,
                secret: ticket.secret.as_ref().unwrap(),
                evidence: &e,
                context: &c
            })
            .await
            .is_err()
    );
    assert_eq!(f.rows("worker_credentials").await, 0);
    assert_eq!(f.rows("task_attempts").await, 1);
}
#[tokio::test]
async fn concurrent_bootstrap_preparations_never_publish_two_challenges() {
    let f = Fixture::new().await;
    let r = f.request("prepare");
    let (a, b) = tokio::join!(
        f.security.prepare_bootstrap(&r),
        f.security.prepare_bootstrap(&r)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.id, b.id);
    assert_eq!(
        usize::from(a.secret.is_some()) + usize::from(b.secret.is_some()),
        1
    );
    assert_eq!(f.rows("worker_enrollments").await, 1);
    assert_eq!(f.rows("worker_bootstrap_challenges").await, 1);
}
#[tokio::test]
async fn competing_preparation_identities_do_not_replace_a_published_challenge() {
    let f = Fixture::new().await;
    let a = f.request("a");
    let b = f.request("b");
    let (a, b) = tokio::join!(
        f.security.prepare_bootstrap(&a),
        f.security.prepare_bootstrap(&b)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(f.rows("worker_bootstrap_challenges").await, 1);
}
#[tokio::test]
async fn concurrent_bootstrap_claims_have_one_effective_credential() {
    let f = Fixture::new().await;
    let ticket = f
        .security
        .prepare_bootstrap(&f.request("prepare"))
        .await
        .unwrap();
    let c = f.context("claim", WorkerOperation::Register);
    let e = synthetic::e("proof");
    f.grant(VerificationStage::Bootstrap, &c, &e, Some(&ticket.id), None);
    let claim = || BootstrapClaim {
        id: &ticket.id,
        secret: ticket.secret.as_ref().unwrap(),
        evidence: &e,
        context: &c,
    };
    let (a, b) = tokio::join!(
        f.security.claim_bootstrap(claim()),
        f.security.claim_bootstrap(claim())
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(f.rows("worker_credentials").await, 1);
}
#[tokio::test]
async fn bootstrap_expiry_does_not_extend_or_replace_the_shorter_lease() {
    let f = Fixture::new().await;
    let ticket = f
        .security
        .prepare_bootstrap(&f.request("prepare"))
        .await
        .unwrap();
    assert_eq!(ticket.expires_at, synthetic::T + 300);
    f.clock.advance_to(synthetic::T + 90).unwrap();
    let c = f.context("claim", WorkerOperation::Register);
    let e = synthetic::e("proof");
    f.grant(VerificationStage::Bootstrap, &c, &e, Some(&ticket.id), None);
    assert!(
        f.security
            .claim_bootstrap(BootstrapClaim {
                id: &ticket.id,
                secret: ticket.secret.as_ref().unwrap(),
                evidence: &e,
                context: &c
            })
            .await
            .is_err()
    );
    assert_eq!(f.rows("worker_credentials").await, 0);
    assert_eq!(
        DurableWorkerController::new(f.store.clone())
            .lease_until(&f.lease)
            .await
            .unwrap(),
        synthetic::T + 90
    );
}
#[tokio::test]
async fn wrong_project_plan_attempt_worker_epoch_and_hashes_never_obtain_message_authority() {
    let f = Fixture::new().await;
    let ticket = f.issued().await;
    let original = f.message(
        1,
        WorkerBody::Register {
            capabilities: synthetic::caps(),
        },
    );
    let mut variants = Vec::new();
    let mut m = original.clone();
    m.binding.worker_id = WorkerId::new("another-worker").unwrap();
    variants.push(m);
    let mut m = original.clone();
    m.binding.task.plan.project_id = ProjectId::new("another-project").unwrap();
    variants.push(m);
    let mut m = original.clone();
    m.binding.task.plan.plan_id = PlanId::new("another-plan").unwrap();
    variants.push(m);
    let mut m = original.clone();
    m.binding.task.task_id = TaskId::new("another-task").unwrap();
    variants.push(m);
    let mut m = original.clone();
    m.binding.attempt_id = AttemptId::new("another-attempt").unwrap();
    variants.push(m);
    let mut m = original.clone();
    m.binding.epoch += 1;
    variants.push(m);
    let mut m = original.clone();
    m.binding.context_hash = rolvya_domain::worker::Digest::new("a".repeat(64)).unwrap();
    variants.push(m);
    let mut m = original.clone();
    m.binding.plan_hash = rolvya_domain::worker::Digest::new("b".repeat(64)).unwrap();
    variants.push(m);
    for m in variants {
        assert!(f.send(&ticket, &m).await.is_err());
    }
    assert_eq!(f.rows("worker_messages").await, 0);
    assert_eq!(f.rows("worker_credential_requests").await, 0);
    f.send(&ticket, &original).await.unwrap();
}
#[tokio::test]
async fn declared_capabilities_do_not_replace_independently_verified_evidence() {
    let f = Fixture::new().await;
    let ticket = f.issued().await;
    for change in 0..4 {
        let mut c = synthetic::caps();
        match change {
            0 => c.evidence = None,
            1 => c.evidence = Some(CapabilityRef::new("self-signed-capability").unwrap()),
            2 => c.models = vec![ModelName::new("unapproved/model").unwrap()],
            _ => c.memory_mib += 1,
        }
        assert!(
            f.send(
                &ticket,
                &f.message(1, WorkerBody::Register { capabilities: c })
            )
            .await
            .is_err()
        );
    }
    assert_eq!(f.rows("worker_messages").await, 0);
}
#[tokio::test]
async fn untrusted_protocol_versions_malformed_oversized_and_privileged_bodies_leave_no_effect() {
    let f = Fixture::new().await;
    let ticket = f.issued().await;
    let c = f.context("bad-wire", WorkerOperation::Register);
    let mut m = f.message(
        1,
        WorkerBody::Register {
            capabilities: synthetic::caps(),
        },
    );
    m.protocol_version += 1;
    let cases = [
        vec![],
        b"{malformed".to_vec(),
        vec![b' '; MAX_MESSAGE_BYTES + 1],
        serde_json::to_vec(&m).unwrap(),
        br#"{"kind":"approve_budget","new_ceiling":999999}"#.to_vec(),
        br#"{"kind":"grant_owner","role":"owner"}"#.to_vec(),
        br#"{"kind":"create_task"}"#.to_vec(),
        br#"{"kind":"get_github_installation_key"}"#.to_vec(),
    ];
    for bytes in cases {
        assert!(
            f.security
                .receive(&ticket.secret, &bytes, &c)
                .await
                .is_err()
        );
    }
    assert_eq!(f.rows("worker_messages").await, 0);
    assert_eq!(f.rows("task_attempts").await, 1);
    assert_eq!(f.rows("tasks").await, 1);
}
#[tokio::test]
async fn reordered_messages_and_changed_content_replay_cannot_change_durable_state() {
    let f = Fixture::new().await;
    let ticket = f.issued().await;
    f.executing(&ticket).await;
    assert!(
        f.send(&ticket, &f.message(5, WorkerBody::Heartbeat {}))
            .await
            .is_err()
    );
    let m = f.message(4, WorkerBody::Heartbeat {});
    let ack = f.send(&ticket, &m).await.unwrap();
    let changed = f.message(4, WorkerBody::GetStopInstruction {});
    assert!(f.send(&ticket, &changed).await.is_err());
    assert_eq!(f.send(&ticket, &m).await.unwrap(), ack);
    assert_eq!(f.rows("worker_messages").await, 4);
}
#[tokio::test]
async fn valid_fifteen_minute_token_cannot_revive_an_expired_ninety_second_lease() {
    let f = Fixture::new().await;
    let ticket = f.issued().await;
    f.executing(&ticket).await;
    f.clock.advance_to(synthetic::T + 90).unwrap();
    assert!(f.clock.now().unwrap() < ticket.metadata.expires_at);
    assert!(
        f.send(&ticket, &f.message(4, WorkerBody::Heartbeat {}))
            .await
            .is_err()
    );
    let (c, e) = f.renewal(&ticket, "expired-renew");
    assert!(
        f.security
            .renew_credential(&ticket.secret, &e, &c)
            .await
            .is_err()
    );
    let stop = f
        .send(&ticket, &f.message(4, WorkerBody::GetStopInstruction {}))
        .await
        .unwrap();
    assert!(matches!(stop.body, ControllerBody::Stop { .. }));
    assert_eq!(
        DurableWorkerController::new(f.store.clone())
            .lease_until(&f.lease)
            .await
            .unwrap(),
        synthetic::T + 90
    );
}
#[tokio::test]
async fn token_expiry_remains_independent_of_a_healthy_renewed_lease() {
    let f = Fixture::new().await;
    let ticket = f.issued().await;
    f.executing(&ticket).await;
    for i in 1..30 {
        f.clock.advance_to(synthetic::T + 30 * i).unwrap();
        f.send(
            &ticket,
            &f.message((i + 3) as u64, WorkerBody::Heartbeat {}),
        )
        .await
        .unwrap();
    }
    let until = DurableWorkerController::new(f.store.clone())
        .lease_until(&f.lease)
        .await
        .unwrap();
    assert!(until > synthetic::T + 900);
    f.clock.advance_to(synthetic::T + 900).unwrap();
    assert!(
        f.send(&ticket, &f.message(33, WorkerBody::Heartbeat {}))
            .await
            .is_err()
    );
    let (c, e) = f.renewal(&ticket, "token-expired");
    assert!(
        f.security
            .renew_credential(&ticket.secret, &e, &c)
            .await
            .is_err()
    );
    assert_eq!(
        DurableWorkerController::new(f.store.clone())
            .lease_until(&f.lease)
            .await
            .unwrap(),
        until
    );
}
#[tokio::test]
async fn external_fencing_retires_credentials_and_preserves_reservations() {
    let f = Fixture::new().await;
    let ticket = f.issued().await;
    f.executing(&ticket).await;
    let before = f
        .store
        .financial_snapshot(&f.binding.task.plan.project_id)
        .await
        .unwrap();
    f.store
        .fence_attempt(
            &admission::FenceRequest {
                lease: f.lease.clone(),
                request_id: RequestId::new("external-fence").unwrap(),
                actor: common::owner(),
                evidence: synthetic::e("trusted-fencing"),
            },
            &synthetic::Authority::default(),
            f.clock.as_ref(),
        )
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM worker_enrollments")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        "revoked"
    );
    assert!(
        f.send(
            &ticket,
            &f.message(
                4,
                WorkerBody::SubmitResult {
                    outcome: ExecutionOutcome::Succeeded {
                        manifest_sha256: rolvya_domain::worker::Digest::new("2".repeat(64))
                            .unwrap()
                    }
                }
            )
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
        before.held
    );
}
#[tokio::test]
async fn r08_expiration_recovery_retires_worker_credentials_without_claiming_provider_termination()
{
    let f = Fixture::new().await;
    let ticket = f.issued().await;
    f.executing(&ticket).await;
    f.clock.advance_to(synthetic::T + 90).unwrap();
    let result = OutboxDispatcher::new(f.store.clone())
        .recover_expired(
            &f.binding.task.plan.project_id,
            &common::owner(),
            &synthetic::Authority::default(),
            f.clock.as_ref(),
            8,
        )
        .await
        .unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].result.is_ok());
    assert!(
        f.send(&ticket, &f.message(4, WorkerBody::Heartbeat {}))
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM synthetic_operations")
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
#[tokio::test]
async fn unknown_exposure_blocks_bootstrap_heartbeats_and_renewal_but_preserves_held_money() {
    let f = Fixture::new().await;
    let ticket = f.issued().await;
    f.executing(&ticket).await;
    let before = f
        .store
        .financial_snapshot(&f.binding.task.plan.project_id)
        .await
        .unwrap();
    f.unknown_exposure().await;
    assert!(
        f.send(&ticket, &f.message(4, WorkerBody::Heartbeat {}))
            .await
            .is_err()
    );
    f.clock.advance_to(synthetic::T + 60).unwrap();
    let (c, e) = f.renewal(&ticket, "risk-renew");
    assert!(
        f.security
            .renew_credential(&ticket.secret, &e, &c)
            .await
            .is_err()
    );
    let stop = f
        .send(&ticket, &f.message(4, WorkerBody::GetStopInstruction {}))
        .await
        .unwrap();
    assert!(matches!(stop.body, ControllerBody::Stop { .. }));
    let after = f
        .store
        .financial_snapshot(&f.binding.task.plan.project_id)
        .await
        .unwrap();
    assert_eq!(after.uncertain_liabilities, 1);
    assert_eq!(after.held, before.held);
    assert_eq!(after.ceiling, before.ceiling);
    assert_eq!(f.rows("worker_credentials").await, 1);
    assert_eq!(f.rows("task_acceptances").await, 0);
}
#[tokio::test]
async fn uncertainty_between_preparation_and_claim_cannot_mint_a_credential() {
    let f = Fixture::new().await;
    let ticket = f
        .security
        .prepare_bootstrap(&f.request("prepare"))
        .await
        .unwrap();
    f.unknown_exposure().await;
    let c = f.context("claim", WorkerOperation::Register);
    let e = synthetic::e("proof");
    f.grant(VerificationStage::Bootstrap, &c, &e, Some(&ticket.id), None);
    assert!(
        f.security
            .claim_bootstrap(BootstrapClaim {
                id: &ticket.id,
                secret: ticket.secret.as_ref().unwrap(),
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
}
#[tokio::test]
async fn renewal_needs_fresh_independent_identity_and_respects_rotation_rate() {
    let f = Fixture::new().await;
    let ticket = f.issued().await;
    f.executing(&ticket).await;
    let (c, e) = f.renewal(&ticket, "too-early");
    assert!(
        f.security
            .renew_credential(&ticket.secret, &e, &c)
            .await
            .is_err()
    );
    f.clock.advance_to(synthetic::T + 60).unwrap();
    f.identity.0.lock().unwrap().remove(c.request_id.as_str());
    assert!(
        f.security
            .renew_credential(&ticket.secret, &e, &c)
            .await
            .is_err()
    );
    assert_eq!(f.rows("worker_credentials").await, 1);
}
#[tokio::test]
async fn concurrent_renewals_have_one_current_generation_and_never_return_the_winners_secret_to_the_loser()
 {
    let f = Fixture::new().await;
    let first = f.issued().await;
    f.executing(&first).await;
    f.clock.advance_to(synthetic::T + 60).unwrap();
    let (a, ea) = f.renewal(&first, "renew-a");
    let (b, eb) = f.renewal(&first, "renew-b");
    let (a, b) = tokio::join!(
        f.security.renew_credential(&first.secret, &ea, &a),
        f.security.renew_credential(&first.secret, &eb, &b)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(f.rows("worker_credentials").await, 2);
    assert!(
        f.send(&first, &f.message(4, WorkerBody::Heartbeat {}))
            .await
            .is_err()
    );
}
#[tokio::test]
async fn revocation_wins_effective_authority_even_if_rotation_commits_first() {
    let f = Fixture::new().await;
    let first = f.issued().await;
    f.executing(&first).await;
    f.clock.advance_to(synthetic::T + 60).unwrap();
    let (c, e) = f.renewal(&first, "renew");
    let revoke = f.revoke_request(&first, "incident");
    let (renewed, revoked) = tokio::join!(
        f.security.renew_credential(&first.secret, &e, &c),
        f.security.revoke_worker(&revoke)
    );
    revoked.unwrap();
    assert!(
        f.send(&first, &f.message(4, WorkerBody::Heartbeat {}))
            .await
            .is_err()
    );
    if let Ok(t) = renewed {
        assert!(
            f.send(&t, &f.message(4, WorkerBody::Heartbeat {}))
                .await
                .is_err()
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM worker_enrollments")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        "revoked"
    );
}
#[tokio::test]
async fn a_worker_cannot_self_authorize_controller_revocation_or_human_owner_authority() {
    let f = Fixture::new().await;
    let t = f.issued().await;
    f.executing(&t).await;
    let r = WorkerRevocation {
        enrollment: t.metadata.enrollment.clone(),
        expected_security_epoch: 1,
        request_id: RequestId::new("self-revoke").unwrap(),
        evidence: synthetic::e("i-am-owner"),
    };
    assert!(f.security.revoke_worker(&r).await.is_err());
    assert_eq!(f.rows("worker_revocations").await, 0);
    f.send(&t, &f.message(4, WorkerBody::Heartbeat {}))
        .await
        .unwrap();
}
#[tokio::test]
async fn restart_reloads_the_same_key_and_preserves_duplicate_result_receipts() {
    let f = Fixture::new().await;
    let t = f.issued().await;
    f.executing(&t).await;
    let m = f.message(
        4,
        WorkerBody::SubmitResult {
            outcome: ExecutionOutcome::Failed {
                class: FailureClass::Transient,
            },
        },
    );
    let ack = f.send(&t, &m).await.unwrap();
    let restarted = f.reopened().await;
    assert_eq!(restarted.key.metadata(), f.security.key.metadata());
    let c = f.context("replay-after-restart", WorkerOperation::SubmitResult);
    assert_eq!(
        restarted
            .receive(&t.secret, &encode_worker_message(&m).unwrap(), &c)
            .await
            .unwrap(),
        ack
    );
    assert_eq!(f.rows("worker_messages").await, 4);
    assert_eq!(f.rows("task_acceptances").await, 0);
}
#[tokio::test]
async fn missing_or_corrupt_key_custody_never_produces_replacement_authority() {
    let f = Fixture::new().await;
    let _t = f.issued().await;
    let file = f.keys.join("controller.kek");
    let entries: Vec<_> = fs::read_dir(&f.keys)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    let key = if file.exists() {
        file
    } else {
        entries
            .into_iter()
            .find(|p| p.is_file() && fs::metadata(p).unwrap().len() > 32)
            .unwrap()
    };
    let original = fs::read(&key).unwrap();
    fs::remove_file(&key).unwrap();
    assert!(WorkerSecurity::open(&f.bootstrap).await.is_err());
    assert_eq!(f.rows("worker_credentials").await, 1);
    fs::write(&key, b"invalid").unwrap();
    fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(WorkerSecurity::open(&f.bootstrap).await.is_err());
    assert_eq!(fs::read(&key).unwrap(), b"invalid");
    fs::write(&key, original).unwrap();
    assert!(WorkerSecurity::open(&f.bootstrap).await.is_ok());
}
#[tokio::test]
async fn unavailable_transport_and_delivery_never_export_bearers() {
    let mut f = Fixture::new().await;
    let request = f.request("prepare");
    let boot = f.security.prepare_bootstrap(&request).await.unwrap();
    assert!(f.security.deliver_bootstrap(&request, &boot).await.is_err());
    let c = f.context("claim", WorkerOperation::Register);
    let e = synthetic::e("proof");
    f.grant(VerificationStage::Bootstrap, &c, &e, Some(&boot.id), None);
    let t = f
        .security
        .claim_bootstrap(BootstrapClaim {
            id: &boot.id,
            secret: boot.secret.as_ref().unwrap(),
            evidence: &e,
            context: &c,
        })
        .await
        .unwrap();
    assert!(f.security.deliver_credential(&t, &c).await.is_err());
    f.security.transport = Arc::new(Unimplemented);
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
async fn debug_formatting_and_invalid_credential_input_do_not_disclose_secret_material() {
    let f = Fixture::new().await;
    let t = f.issued().await;
    assert!(!format!("{t:?}").contains(t.secret.0.as_str()));
    assert!(WorkerCredential::parse(String::new()).is_err());
    assert!(WorkerCredential::parse("f".repeat(65)).is_err());
    assert!(WorkerCredential::parse("Q".repeat(64)).is_err());
    for e in [
        WorkerAuthError::Denied,
        WorkerAuthError::Storage,
        WorkerAuthError::Integrity,
    ] {
        assert!(!format!("{e:?} {e}").contains(t.secret.0.as_str()));
    }
}
#[tokio::test]
async fn authenticated_request_history_is_bounded_while_known_duplicate_receipts_remain_replayable()
{
    let f = Fixture::new().await;
    let t = f.issued().await;
    let m = f.message(
        1,
        WorkerBody::Register {
            capabilities: synthetic::caps(),
        },
    );
    let ack = f.send(&t, &m).await.unwrap();
    let event: i64 =
        sqlx::query_scalar("SELECT event_sequence FROM worker_credential_requests LIMIT 1")
            .fetch_one(&f.store.pool)
            .await
            .unwrap();
    sqlx::query("WITH RECURSIVE n(v) AS (VALUES(1) UNION ALL SELECT v+1 FROM n WHERE v<8191) INSERT INTO worker_credential_requests SELECT ?, 'synthetic-request-'||v, ?, ?, ?, ?, ?, ? FROM n")
        .bind(t.metadata.enrollment.as_str()).bind(t.metadata.id.as_str()).bind(f.binding.task.plan.project_id.as_str()).bind(f.binding.attempt_id.as_str()).bind(m.message_id.as_str()).bind("a".repeat(64)).bind(event).execute(&f.store.pool).await.unwrap();
    assert_eq!(f.rows("worker_credential_requests").await, 8192);
    assert!(
        f.send(&t, &f.message(2, WorkerBody::Claim {}))
            .await
            .is_err()
    );
    assert_eq!(f.send(&t, &m).await.unwrap(), ack);
    assert_eq!(f.rows("worker_credential_requests").await, 8192);
    assert_eq!(f.rows("worker_messages").await, 1);
}
