use super::*;
use crate::controller_keys::{provider_accounts as registry, secret_broker as broker};
async fn draft(f: &Fixture) -> registry::ConnectionChange {
    let k = registry::CredentialReference {
        connection: "protected-model-credential".into(),
        account: "synthetic-api-account".into(),
        provider: "synthetic-model-provider".into(),
        record: "model-key".into(),
        purpose: "model-execution".into(),
        version: 1,
    };
    let scope = broker::RecordScope {
        connection: broker::ConnectionScope {
            project: f.project.clone(),
            connection: broker::ConnectionId::new(k.connection.clone()).unwrap(),
            account: broker::AccountId::new(k.account.clone()).unwrap(),
            provider: broker::ProviderId::new(k.provider.clone()).unwrap(),
            mode: broker::ProviderMode::ApiKey,
        },
        record: broker::RecordId::new(k.record.clone()).unwrap(),
        purpose: broker::SecretPurpose::new(k.purpose.clone()).unwrap(),
    };
    let mut tx =
        f.s.a
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .unwrap();
    broker::fixture_record(
        &mut tx,
        &f.s.a.auth.key,
        &scope,
        &broker::SecretValue::new(random::<32>().unwrap().to_vec()).unwrap(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    registry::ConnectionChange::Enroll {
        draft: registry::ConnectionDraft {
            id: registry::ConnectionId::generate().unwrap(),
            provider: k.provider.clone(),
            account: k.account.clone(),
            mode: rolvya_domain::plan::AccessMode::ApiKey,
            integration: "synthetic-controller".into(),
            deployment: "synthetic-hosted".into(),
            credential: k,
            models: vec!["synthetic/model".into()],
        },
    }
}
async fn prepare(f: &Fixture, change: &registry::ConnectionChange) -> OwnerAction {
    f.s.a
        .auth
        .prepare_provider_connection(
            &f.owner.handle,
            &f.project,
            change,
            &f.ctx(&f.owner, Operation::Write, 1),
        )
        .await
        .unwrap()
}
#[tokio::test]
async fn provider_enrollment_requires_fresh_owner_proof_and_leaves_entitlement_unverified() {
    let f = Fixture::new().await;
    let change = draft(&f).await;
    let action = prepare(&f, &change).await;
    let id = f.prepare(&action).await;
    assert!(f.execute(&id, &action).await.is_err());
    assert_eq!(f.s.a.count("provider_connections").await, 0);
    f.prove(&id).await;
    f.execute(&id, &action).await.unwrap();
    assert!(f.execute(&id, &action).await.is_err());
    assert_eq!(f.s.a.count("provider_connections").await, 1);
    assert_eq!(f.s.a.count("provider_connection_versions").await, 1);
    assert_eq!(f.s.a.count("provider_project_grants").await, 1);
    assert_eq!(f.s.a.count("provider_capacity_reservations").await, 0);
    assert_eq!(f.s.a.count("dispatch_outbox").await, 0);
    let record: String = sqlx::query_scalar("SELECT record FROM provider_project_grants")
        .fetch_one(&f.s.a.store.pool)
        .await
        .unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&record).unwrap()["entitlement"].is_null());
}
#[tokio::test]
async fn provider_proposal_expiring_during_owner_commit_rolls_back_consent_and_proof() {
    let mut f = Fixture::new().await;
    let change = draft(&f).await;
    let action = prepare(&f, &change).await;
    let proposal_expires = f.s.a.now() + 1800;
    f.s.a.advance(1700);
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    let clock = f.s.a.clock_for_session_test();
    f.s.a.auth.hook = Box::new(move |p| {
        if p == Checkpoint::BeforeOwnerCommit {
            clock.store(proposal_expires, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(())
    });
    assert!(
        f.execute(&id, &action).await.is_err(),
        "proposal expiry is an independent final commit deadline"
    );
    assert_eq!(f.s.a.count("provider_connections").await, 0);
    assert_eq!(f.s.a.count("human_owner_effects").await, 0);
    let consumed: i64 = sqlx::query_scalar("SELECT consumed FROM provider_change_proposals")
        .fetch_one(&f.s.a.store.pool)
        .await
        .unwrap();
    assert_eq!(consumed, 0);
}
#[tokio::test]
async fn ignored_provider_grant_write_rolls_back_owner_effect_and_enrollment() {
    let f = Fixture::new().await;
    let action = prepare(&f, &draft(&f).await).await;
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    sqlx::query("CREATE TRIGGER test_ignore_grant BEFORE INSERT ON provider_project_grants BEGIN SELECT RAISE(IGNORE); END").execute(&f.s.a.store.pool).await.unwrap();
    assert!(f.execute(&id, &action).await.is_err());
    assert_eq!(f.s.a.count("provider_connections").await, 0);
    assert_eq!(f.s.a.count("human_owner_effects").await, 0);
}
#[tokio::test]
async fn exact_owner_consent_and_independent_provider_proof_are_separate_current_boundaries() {
    for case in 0..4 {
        let mut f = Fixture::new().await;
        let change = draft(&f).await;
        let registry::ConnectionChange::Enroll { draft } = &change else {
            unreachable!()
        };
        let connection = draft.id.clone();
        let action = prepare(&f, &change).await;
        let id = f.prepare(&action).await;
        f.prove(&id).await;
        f.execute(&id, &action).await.unwrap();
        let bootstrap = crate::controller_keys::ControllerBootstrap::open(
            f.s.a.store.clone(),
            &f.s.a.keys,
            crate::controller_keys::ServiceIdentity::current(),
            &[],
        )
        .await
        .unwrap();
        let until = f.s.a.now() + 60;
        let (provider, current) = registry::fixture_owner_verifier(
            &bootstrap,
            &connection,
            &f.project,
            f.s.a.now(),
            until,
        )
        .await;
        let clock = f.s.a.clock_for_session_test();
        if case == 1 {
            f.s.a.auth.hook = Box::new(move |p| {
                if p == Checkpoint::BeforeOwnerCommit {
                    clock.store(until, std::sync::atomic::Ordering::SeqCst);
                }
                Ok(())
            });
        }
        if case == 2 {
            f.s.a.auth.hook = Box::new(move |p| {
                if p == Checkpoint::BeforeOwnerCommit {
                    current.store(false, std::sync::atomic::Ordering::SeqCst);
                }
                Ok(())
            });
        }
        let unavailable = registry::ProviderRegistry::open(&bootstrap).await.unwrap();
        let p = if case == 3 {
            &unavailable
        } else {
            provider.as_ref()
        };
        let result = f
            .s
            .a
            .auth
            .verify_provider_account(
                &f.owner.handle,
                &f.project,
                &registry::ProviderVerificationRequest {
                    connection: connection.clone(),
                    expected_version: 1,
                    evidence: crate::finance::EvidenceRef::new("synthetic-provider-owner-proof")
                        .unwrap(),
                },
                p,
                &f.ctx(&f.owner, Operation::Write, 1),
            )
            .await;
        assert_eq!(result.is_ok(), case == 0);
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM provider_security_events WHERE kind='verified'",
        )
        .fetch_one(&f.s.a.store.pool)
        .await
        .unwrap();
        assert_eq!(count, i64::from(case == 0));
        assert_eq!(
            f.s.a.count("provider_connection_versions").await,
            if case == 0 { 2 } else { 1 }
        );
        assert_eq!(f.s.a.count("dispatch_outbox").await, 0);
        if case == 0 {
            let inventory =
                f.s.a
                    .auth
                    .provider_connections(
                        &f.owner.handle,
                        &f.project,
                        None,
                        10,
                        &f.ctx(&f.owner, Operation::Read, 1),
                    )
                    .await
                    .unwrap();
            assert_eq!(inventory.len(), 1);
            assert_eq!(inventory[0].recorded_verification_deadline, Some(until));
        }
    }
}
pub(super) async fn infrastructure_draft(
    f: &Fixture,
) -> crate::controller_keys::worker_auth::aws_fargate::AccountEnrollment {
    use crate::controller_keys::worker_auth::aws_fargate as aws;
    let now = f.s.a.now();
    let credential = registry::CredentialReference {
        connection: "aws-controller".into(),
        account: "123456789012".into(),
        provider: "aws".into(),
        record: "aws-controller".into(),
        purpose: "aws-controller-inspection".into(),
        version: 1,
    };
    let scope = broker::RecordScope {
        connection: broker::ConnectionScope {
            project: f.project.clone(),
            connection: broker::ConnectionId::new(&credential.connection).unwrap(),
            account: broker::AccountId::new(&credential.account).unwrap(),
            provider: broker::ProviderId::new(&credential.provider).unwrap(),
            mode: broker::ProviderMode::ApiKey,
        },
        record: broker::RecordId::new(&credential.record).unwrap(),
        purpose: broker::SecretPurpose::new(&credential.purpose).unwrap(),
    };
    let value = serde_json::json!({"format":1,"account_id":"123456789012","access_key_id":format!("synthetic-{}",super::super::super::super::hex(&random::<16>().unwrap()[..])),"secret_access_key":super::super::super::super::hex(&random::<32>().unwrap()[..]),"session_token":null,"expires_at":now+3600});
    let mut tx =
        f.s.a
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .unwrap();
    broker::fixture_record(
        &mut tx,
        &f.s.a.auth.key,
        &scope,
        &broker::SecretValue::new(serde_json::to_vec(&value).unwrap()).unwrap(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    aws::AccountEnrollment {
        project: f.project.clone(),
        account_ref: crate::finance::EvidenceRef::new("aws-owner-account").unwrap(),
        request_id: request(),
        expected_version: 0,
        workload: aws::fixture_workload(),
        credential,
        consent_reference: crate::finance::EvidenceRef::new("scoped-owner-consent").unwrap(),
        valid_until: now + 3600,
        revoked: false,
    }
}
#[tokio::test]
async fn infrastructure_consent_requires_fresh_owner_proof_and_commits_no_account_by_itself() {
    let f = Fixture::new().await;
    let draft = infrastructure_draft(&f).await;
    let action =
        f.s.a
            .auth
            .prepare_infrastructure_account(
                &f.owner.handle,
                &f.project,
                &draft,
                &f.ctx(&f.owner, Operation::Write, 1),
            )
            .await
            .unwrap();
    let id = f.prepare(&action).await;
    assert!(f.execute(&id, &action).await.is_err());
    assert_eq!(f.s.a.count("aws_account_consents").await, 0);
    f.prove(&id).await;
    f.execute(&id, &action).await.unwrap();
    assert_eq!(f.s.a.count("aws_account_consents").await, 1);
    assert_eq!(f.s.a.count("aws_infrastructure_accounts").await, 0);
    assert_eq!(f.s.a.count("aws_credential_uses").await, 0);
    assert!(f.execute(&id, &action).await.is_err());
}
#[tokio::test]
async fn infrastructure_approval_cannot_overwrite_a_newer_pending_consent() {
    let f = Fixture::new().await;
    let a = infrastructure_draft(&f).await;
    let mut b = a.clone();
    b.request_id = request();
    b.workload.runtime_seconds -= 1;
    let action_a =
        f.s.a
            .auth
            .prepare_infrastructure_account(
                &f.owner.handle,
                &f.project,
                &a,
                &f.ctx(&f.owner, Operation::Write, 1),
            )
            .await
            .unwrap();
    let id_a = f.prepare(&action_a).await;
    f.prove(&id_a).await;
    let action_b =
        f.s.a
            .auth
            .prepare_infrastructure_account(
                &f.owner.handle,
                &f.project,
                &b,
                &f.ctx(&f.owner, Operation::Write, 1),
            )
            .await
            .unwrap();
    let id_b = f.prepare(&action_b).await;
    f.prove(&id_b).await;
    f.execute(&id_b, &action_b).await.unwrap();
    assert!(
        f.execute(&id_a, &action_a).await.is_err(),
        "stale approval cannot supersede a changed consent target"
    );
    assert_eq!(f.s.a.count("aws_account_consents").await, 1);
}
#[tokio::test]
async fn infrastructure_consent_expiry_is_rechecked_at_the_actual_owner_effect() {
    let f = Fixture::new().await;
    let mut draft = infrastructure_draft(&f).await;
    draft.valid_until = f.s.a.now() + 1;
    let action =
        f.s.a
            .auth
            .prepare_infrastructure_account(
                &f.owner.handle,
                &f.project,
                &draft,
                &f.ctx(&f.owner, Operation::Write, 1),
            )
            .await
            .unwrap();
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    f.s.a.advance(1);
    assert!(
        f.execute(&id, &action).await.is_err(),
        "expired consent cannot commit approval or notifications"
    );
    assert_eq!(f.s.a.count("aws_account_consents").await, 0);
}

#[tokio::test]
async fn infrastructure_disconnect_remains_possible_after_credential_loss_and_expiry() {
    let f = Fixture::new().await;
    let mut draft = infrastructure_draft(&f).await;
    let action =
        f.s.a
            .auth
            .prepare_infrastructure_account(
                &f.owner.handle,
                &f.project,
                &draft,
                &f.ctx(&f.owner, Operation::Write, 1),
            )
            .await
            .unwrap();
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    f.execute(&id, &action).await.unwrap();
    // A disconnected/expired credential must not trap the Owner in an active
    // consent state. Disconnect is a metadata-only, exact Owner effect.
    sqlx::query(
        "UPDATE secret_connections SET state='revoked',revision=revision+1 WHERE project_id=?",
    )
    .bind(f.project.as_str())
    .execute(&f.s.a.store.pool)
    .await
    .unwrap();
    draft.revoked = true;
    draft.valid_until = f.s.a.now();
    draft.request_id = request();
    let action =
        f.s.a
            .auth
            .prepare_infrastructure_account(
                &f.owner.handle,
                &f.project,
                &draft,
                &f.ctx(&f.owner, Operation::Write, 1),
            )
            .await
            .unwrap();
    let id = f.prepare(&action).await;
    f.prove(&id).await;
    f.execute(&id, &action).await.unwrap();
    let revoked:bool=sqlx::query_scalar("SELECT json_extract(c.record,'$.enrollment.revoked') FROM aws_account_consent_heads h JOIN aws_account_consents c USING(project_id,account_ref,generation)").fetch_one(&f.s.a.store.pool).await.unwrap();
    assert!(revoked);
    assert_eq!(f.s.a.count("aws_credential_uses").await, 0);
}
