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
