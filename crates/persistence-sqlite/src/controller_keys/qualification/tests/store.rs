use super::*;
use crate::{
    controller_keys::{InitializationRequest, LocalSetupAuthorization, ServiceIdentity},
    finance::*,
    resilience::{common, synthetic},
};
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::{os::unix::fs::PermissionsExt, path::Path};
struct Setup;
impl LocalSetupAuthorization for Setup {
    fn permits(&self, r: &InitializationRequest) -> bool {
        r.request_id().as_str() == "private-qualification-setup"
    }
}
struct Fixture {
    _root: tempfile::TempDir,
    q: QualificationStore,
    bootstrap: ControllerBootstrap,
    scope: QualificationScope,
    config: IsolationConfiguration,
}
impl Fixture {
    async fn new() -> Self {
        let tmp = std::env::temp_dir();
        use std::os::unix::fs::MetadataExt;
        let base = if [0, rustix::process::geteuid().as_raw()]
            .contains(&std::fs::metadata(&tmp).unwrap().uid())
        {
            tmp
        } else {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .find(|p| p.join(".git").exists())
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf()
        };
        let root = tempfile::tempdir_in(base).unwrap();
        for n in ["data", "keys"] {
            let p = root.path().join(n);
            std::fs::create_dir(&p).unwrap();
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let store = PlanStore::open(&root.path().join("data/state.sqlite"))
            .await
            .unwrap();
        let bootstrap = ControllerBootstrap::open(
            store.clone(),
            &root.path().join("keys"),
            ServiceIdentity::current(),
            &[],
        )
        .await
        .unwrap();
        bootstrap
            .initialize(
                &InitializationRequest::new(id("private-qualification-setup")),
                &Setup,
            )
            .await
            .unwrap();
        let project = ProjectId::new("project").unwrap();
        store
            .create_project(&project, &common::owner())
            .await
            .unwrap();
        let now = crate::finance::now().unwrap();
        store
            .apply_financial(
                &FinancialRequest {
                    project: project.clone(),
                    request_id: id("budget"),
                    actor: common::owner(),
                    reason: "Private offline fixture".into(),
                    command: FinancialCommand::Approve {
                        expected_version: 0,
                        new_ceiling: synthetic::money(10000),
                        proof: OwnerProof {
                            reference: synthetic::e("synthetic-budget-proof"),
                            verified_at: now,
                        },
                    },
                },
                &synthetic::Finance,
            )
            .await
            .unwrap();
        let q = QualificationStore::open(&bootstrap).await.unwrap();
        let config = super::super::tests::configuration();
        let scope = QualificationScope {
            installation: q.key.coding_installation(),
            project,
            infrastructure_account: "aws-a".into(),
            partition: config.coding.cluster.partition(),
            region: config.coding.cluster.region().into(),
            aws_account: config.coding.cluster.account(),
            task_definition: config.coding.task_definition.clone(),
            image_digest: config.coding.image_digest.clone(),
            infrastructure_version: 1,
            infrastructure_credential_version: 1,
            configuration_sha256: config.inspect().unwrap(),
            workload_sha256: workload_commitment(&config.coding).unwrap(),
            execution_sha256: "1".repeat(64),
            exposure_sha256: "2".repeat(64),
            connection: "model-a".into(),
            connection_version: 1,
            grant_version: 1,
            credential_version: 1,
            financial_revision: 1,
            operation: OperationClass::CodingLaunch,
            operation_id: "operation-a".into(),
            attempt: "attempt-a".into(),
            epoch: 1,
        };
        Self {
            _root: root,
            q,
            bootstrap,
            scope,
            config,
        }
    }
    async fn inspect(&self) -> QualificationReport {
        self.q
            .inspect_configuration(&self.scope, &id("inspection"), &self.config)
            .await
            .unwrap()
    }
    async fn count(&self, table: &str) -> i64 {
        sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&self.q.store.pool)
            .await
            .unwrap()
    }
    fn issuer(&mut self) -> Ed25519KeyPair {
        let raw = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let key = Ed25519KeyPair::from_pkcs8(raw.as_ref()).unwrap();
        self.q.issuers.insert(
            "private-offline-issuer".into(),
            Issuer {
                key: key.public_key().as_ref().try_into().unwrap(),
                domains: REQUIRED.into_iter().collect(),
                simulated: true,
            },
        );
        key
    }
    fn statement(&self, domain: Domain) -> EvidenceStatement {
        let now = crate::finance::now().unwrap();
        EvidenceStatement {
            version: 1,
            request: id(&format!(
                "evidence-{}",
                serde_json::to_string(&domain).unwrap().trim_matches('"')
            )),
            scope: self.scope.clone(),
            domain,
            level: EvidenceLevel::Simulated,
            evidence_sha256: "a".repeat(64),
            source_revision: "b".repeat(64),
            observed_at: now,
            expires_at: now + 3600,
        }
    }
}
fn id(s: &str) -> RequestId {
    RequestId::new(s).unwrap()
}
fn signed(statement: EvidenceStatement, key: &Ed25519KeyPair) -> SignedEvidence {
    let mut b = b"rolvya/independent-provider-qualification/v1\0".to_vec();
    b.extend(serde_json::to_vec(&statement).unwrap());
    SignedEvidence {
        statement,
        issuer: "private-offline-issuer".into(),
        signature: STANDARD.encode(key.sign(&b).as_ref()),
    }
}
#[tokio::test]
async fn inspection_commits_once_and_never_authorizes_paid_work() {
    let f = Fixture::new().await;
    let a = f.inspect().await;
    let b = f.inspect().await;
    assert_eq!(a, b);
    assert_eq!(a.inspected, vec![Domain::WorkloadIsolation]);
    assert_eq!(a.missing, REQUIRED);
    assert!(!a.paid_execution_allowed);
    assert_eq!(f.count("provider_qualification_evidence").await, 1);
    assert_eq!(f.count("task_attempts").await, 0);
    assert_eq!(f.count("financial_events").await, 1);
}
#[tokio::test]
async fn independent_simulation_requires_every_domain_and_is_not_production_authority() {
    let mut f = Fixture::new().await;
    let key = f.issuer();
    for d in REQUIRED {
        f.q.submit_evidence(&signed(f.statement(d), &key))
            .await
            .unwrap();
    }
    let mut c = f.q.store.pool.begin().await.unwrap();
    f.q.require_operation_in(&mut c, &f.scope, crate::finance::now().unwrap())
        .await
        .unwrap();
    c.commit().await.unwrap();
    let report = f.q.report(&f.scope).await.unwrap();
    assert_eq!(report.simulated.len(), 9);
    assert_eq!(report.qualified.len(), 0);
    assert_eq!(report.missing.len(), 9);
    assert!(!report.paid_execution_allowed);
    let production = QualificationStore::open(&f.bootstrap).await.unwrap();
    assert_eq!(
        production.report(&f.scope).await.unwrap_err(),
        E::UntrustedIssuer
    );
    let mut c = production.store.pool.begin().await.unwrap();
    assert!(
        production
            .require_operation_in(&mut c, &f.scope, crate::finance::now().unwrap())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn valid_signature_with_unknown_issuer_cannot_be_adopted() {
    let mut f = Fixture::new().await;
    let key = f.issuer();
    let s = signed(f.statement(Domain::AwsIdentity), &key);
    let closed = QualificationStore::open(&f.bootstrap).await.unwrap();
    assert_eq!(
        closed.submit_evidence(&s).await.unwrap_err(),
        E::UntrustedIssuer
    );
    assert_eq!(f.count("provider_qualification_evidence").await, 0);
}
#[tokio::test]
async fn simulated_signature_cannot_claim_external_or_independent_trust() {
    let mut f = Fixture::new().await;
    let key = f.issuer();
    for level in [
        EvidenceLevel::ExternallyObserved,
        EvidenceLevel::IndependentlyQualified,
        EvidenceLevel::ConfigurationInspected,
    ] {
        let mut s = f.statement(Domain::AwsIdentity);
        s.level = level;
        assert!(f.q.submit_evidence(&signed(s, &key)).await.is_err());
    }
    assert_eq!(f.count("provider_qualification_evidence").await, 0);
}
#[tokio::test]
async fn signature_scope_and_domain_tampering_are_rejected() {
    let mut f = Fixture::new().await;
    let key = f.issuer();
    let mut s = signed(f.statement(Domain::EffectiveIam), &key);
    s.statement.scope.epoch += 1;
    assert_eq!(
        f.q.submit_evidence(&s).await.unwrap_err(),
        E::UntrustedIssuer
    );
    f.q.issuers
        .get_mut("private-offline-issuer")
        .unwrap()
        .domains
        .remove(&Domain::EffectiveIam);
    assert_eq!(
        f.q.submit_evidence(&signed(f.statement(Domain::EffectiveIam), &key))
            .await
            .unwrap_err(),
        E::UntrustedIssuer
    );
    assert_eq!(f.count("provider_qualification_evidence").await, 0);
}
#[tokio::test]
async fn repeated_report_is_idempotent_but_conflicting_content_is_denied() {
    let mut f = Fixture::new().await;
    let key = f.issuer();
    let s = f.statement(Domain::EffectiveIam);
    let envelope = signed(s.clone(), &key);
    f.q.submit_evidence(&envelope).await.unwrap();
    f.q.submit_evidence(&envelope).await.unwrap();
    let mut changed = s;
    changed.evidence_sha256 = "c".repeat(64);
    assert_eq!(
        f.q.submit_evidence(&signed(changed, &key))
            .await
            .unwrap_err(),
        E::Conflict
    );
    assert_eq!(f.count("provider_qualification_evidence").await, 1);
}
#[tokio::test]
async fn future_expired_and_overlong_evidence_has_no_durable_effect() {
    let mut f = Fixture::new().await;
    let key = f.issuer();
    for n in 0..3 {
        let mut s = f.statement(Domain::EffectiveIam);
        match n {
            0 => s.observed_at += 10,
            1 => s.expires_at = s.observed_at,
            _ => s.expires_at += 86400,
        };
        assert_eq!(
            f.q.submit_evidence(&signed(s, &key)).await.unwrap_err(),
            E::Expired
        );
    }
    assert_eq!(f.count("provider_qualification_evidence").await, 0);
}
#[tokio::test]
async fn revocation_is_append_only_and_replay_cannot_resurrect_evidence() {
    let f = Fixture::new().await;
    f.inspect().await;
    f.q.revoke(&f.scope.project, &id("inspection"), &id("revoke"))
        .await
        .unwrap();
    f.q.revoke(&f.scope.project, &id("inspection"), &id("revoke"))
        .await
        .unwrap();
    let report = f.q.report(&f.scope).await.unwrap();
    assert_eq!(report.revoked, 1);
    assert!(report.inspected.is_empty());
    assert!(!report.paid_execution_allowed);
    assert!(
        sqlx::query("DELETE FROM provider_qualification_revocations")
            .execute(&f.q.store.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE provider_qualification_evidence SET domain='other'")
            .execute(&f.q.store.pool)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn ignored_evidence_write_rolls_back_without_success_receipt() {
    let f = Fixture::new().await;
    sqlx::query("CREATE TRIGGER fault_q BEFORE INSERT ON provider_qualification_evidence BEGIN SELECT RAISE(IGNORE); END").execute(&f.q.store.pool).await.unwrap();
    assert_eq!(
        f.q.inspect_configuration(&f.scope, &id("inspection"), &f.config)
            .await
            .unwrap_err(),
        E::Storage
    );
    assert_eq!(f.count("provider_qualification_evidence").await, 0);
}
#[tokio::test]
async fn ignored_revocation_write_cannot_report_success() {
    let f = Fixture::new().await;
    f.inspect().await;
    sqlx::query("CREATE TRIGGER fault_q BEFORE INSERT ON provider_qualification_revocations BEGIN SELECT RAISE(IGNORE); END").execute(&f.q.store.pool).await.unwrap();
    assert_eq!(
        f.q.revoke(&f.scope.project, &id("inspection"), &id("revoke"))
            .await
            .unwrap_err(),
        E::Storage
    );
    assert_eq!(f.count("provider_qualification_revocations").await, 0);
}
#[tokio::test]
async fn competing_inspectors_preserve_one_committed_operation() {
    let f = Fixture::new().await;
    let other = QualificationStore::open(&f.bootstrap).await.unwrap();
    let request = id("inspection");
    let (a, b) = tokio::join!(
        f.q.inspect_configuration(&f.scope, &request, &f.config),
        other.inspect_configuration(&f.scope, &request, &f.config)
    );
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(f.count("provider_qualification_evidence").await, 1);
}
#[tokio::test]
async fn configuration_and_account_version_changes_invalidate_old_scope() {
    let f = Fixture::new().await;
    f.inspect().await;
    for n in 0..9 {
        let mut s = f.scope.clone();
        match n {
            0 => s.infrastructure_version += 1,
            1 => s.credential_version += 1,
            2 => s.connection_version += 1,
            3 => s.grant_version += 1,
            4 => s.epoch += 1,
            5 => s.workload_sha256 = "9".repeat(64),
            6 => s.exposure_sha256 = "9".repeat(64),
            7 => s.execution_sha256 = "9".repeat(64),
            _ => s.attempt = "other".into(),
        };
        let r = f.q.report(&s).await.unwrap();
        assert!(r.inspected.is_empty());
        assert!(!r.paid_execution_allowed);
    }
}

#[tokio::test]
async fn unknown_liability_and_changed_budget_revision_block_qualified_simulation() {
    let mut f = Fixture::new().await;
    let key = f.issuer();
    for d in REQUIRED {
        f.q.submit_evidence(&signed(f.statement(d), &key))
            .await
            .unwrap();
    }
    f.q.store
        .apply_financial(
            &FinancialRequest {
                project: f.scope.project.clone(),
                request_id: id("late-exposure"),
                actor: common::owner(),
                reason: "Private late provider observation".into(),
                command: FinancialCommand::Report {
                    liability: LiabilityId::new("unresolved-provider").unwrap(),
                    expected_revision: 0,
                    unreserved: synthetic::zero(),
                    uncertain: true,
                    evidence: synthetic::e("private-late-bill"),
                },
            },
            &synthetic::Finance,
        )
        .await
        .unwrap();
    let mut c = f.q.store.pool.begin().await.unwrap();
    assert!(
        f.q.require_operation_in(&mut c, &f.scope, crate::finance::now().unwrap())
            .await
            .is_err()
    );
    c.commit().await.unwrap();
    let report = f.q.report(&f.scope).await.unwrap();
    assert!(report.financial_uncertainty);
    assert!(!report.paid_execution_allowed);
    assert_eq!(f.count("task_attempts").await, 0);
    assert_eq!(f.count("provider_qualification_evidence").await, 9);
}
#[tokio::test]
async fn expired_domain_cannot_be_refreshed_by_replaying_the_same_report() {
    let mut f = Fixture::new().await;
    let key = f.issuer();
    let s = f.statement(Domain::EffectiveIam);
    let envelope = signed(s.clone(), &key);
    f.q.submit_evidence(&envelope).await.unwrap();
    let mut c = f.q.store.pool.begin().await.unwrap();
    let r =
        f.q.report_in(&mut c, &f.scope, s.expires_at, false)
            .await
            .unwrap();
    assert_eq!(r.expired, 1);
    assert!(r.qualified.is_empty());
    assert!(
        f.q.require_operation_in(&mut c, &f.scope, s.expires_at)
            .await
            .is_err()
    );
    c.commit().await.unwrap();
}
#[tokio::test]
async fn qualification_is_project_and_installation_scoped_without_cross_project_rows() {
    let f = Fixture::new().await;
    f.inspect().await;
    let mut other = f.scope.clone();
    other.installation = "f".repeat(32);
    assert_eq!(f.q.report(&other).await.unwrap_err(), E::Scope);
    other = f.scope.clone();
    other.project = ProjectId::new("other-project").unwrap();
    assert_eq!(f.q.report(&other).await.unwrap_err(), E::Scope);
    assert_eq!(f.count("provider_qualification_evidence").await, 1);
}
#[tokio::test]
async fn revocation_racing_operation_check_cannot_restore_authority() {
    let mut f = Fixture::new().await;
    let key = f.issuer();
    for d in REQUIRED {
        f.q.submit_evidence(&signed(f.statement(d), &key))
            .await
            .unwrap();
    }
    let evidence = f.statement(Domain::EffectiveIam).request;
    let other = QualificationStore::open(&f.bootstrap).await.unwrap();
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
    let b = barrier.clone();
    let project = f.scope.project.clone();
    let task = tokio::spawn(async move {
        b.wait().await;
        other
            .revoke(&project, &evidence, &id("race-revoke"))
            .await
            .unwrap()
    });
    barrier.wait().await;
    task.await.unwrap();
    let mut c = f.q.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    assert!(
        f.q.require_operation_in(&mut c, &f.scope, crate::finance::now().unwrap())
            .await
            .is_err()
    );
    c.commit().await.unwrap();
    assert_eq!(f.count("provider_qualification_revocations").await, 1);
    assert_eq!(f.count("task_attempts").await, 0);
}
#[tokio::test]
async fn qualification_sigkill_child() {
    let Ok(stage) = std::env::var("ROLVYA_QUALIFICATION_CRASH") else {
        return;
    };
    let mut f = Fixture::new().await;
    if stage.starts_with("revocation-") {
        f.inspect().await;
    }
    std::fs::write(
        std::env::var("ROLVYA_QUALIFICATION_REPORT").unwrap(),
        serde_json::to_vec(&serde_json::json!({"root":f._root.path(),"scope":f.scope})).unwrap(),
    )
    .unwrap();
    let expected = stage.clone();
    f.q.hook = Box::new(move |label| {
        if label == expected {
            rustix::process::kill_process(rustix::process::getpid(), rustix::process::Signal::KILL)
                .unwrap();
            unreachable!()
        }
    });
    if stage.starts_with("inspection-") {
        f.inspect().await;
    } else {
        f.q.revoke(&f.scope.project, &id("inspection"), &id("crash-revoke"))
            .await
            .unwrap();
    }
    panic!("requested qualification boundary was not reached");
}
#[tokio::test]
async fn sigkill_reopen_preserves_qualification_and_revocation_commit_boundaries() {
    use std::{os::unix::process::ExitStatusExt, time::Duration};
    for stage in [
        "inspection-before",
        "inspection-after",
        "revocation-before",
        "revocation-after",
    ] {
        let report = tempfile::tempdir().unwrap();
        let path = report.path().join("report.json");
        let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "controller_keys::qualification::store::tests::qualification_sigkill_child",
                "--nocapture",
            ])
            .env("ROLVYA_QUALIFICATION_CRASH", stage)
            .env("ROLVYA_QUALIFICATION_REPORT", &path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let status = tokio::time::timeout(Duration::from_secs(30), child.wait())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.signal(), Some(9));
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let root = std::path::PathBuf::from(value["root"].as_str().unwrap());
        let store = PlanStore::open_existing(&root.join("data/state.sqlite"))
            .await
            .unwrap();
        store.check_integrity().await.unwrap();
        let bootstrap = ControllerBootstrap::open(
            store.clone(),
            &root.join("keys"),
            ServiceIdentity::current(),
            &[],
        )
        .await
        .unwrap();
        let q = QualificationStore::open(&bootstrap).await.unwrap();
        let scope: QualificationScope = serde_json::from_value(value["scope"].clone()).unwrap();
        let state = q.report(&scope).await.unwrap();
        assert!(!state.paid_execution_allowed);
        assert!(state.qualified.is_empty());
        assert_eq!(
            state.inspected.len(),
            if stage == "inspection-before" || stage == "revocation-after" {
                0
            } else {
                1
            }
        );
        assert_eq!(
            state.revoked,
            if stage == "revocation-after" { 1 } else { 0 }
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM financial_events")
                .fetch_one(&store.pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_attempts")
                .fetch_one(&store.pool)
                .await
                .unwrap(),
            0
        );
        drop(q);
        drop(bootstrap);
        store.close().await;
        std::fs::remove_dir_all(root).unwrap();
    }
}
#[tokio::test]
async fn review_panel_rejects_unauthenticated_revocation_metadata() {
    let f = Fixture::new().await;
    f.inspect().await;
    sqlx::query("INSERT INTO provider_qualification_revocations VALUES(?,?,?,?,?)")
        .bind(f.scope.project.as_str())
        .bind("inspection")
        .bind("forged-revocation")
        .bind(crate::finance::now().unwrap())
        .bind([0u8; 32].as_slice())
        .execute(&f.q.store.pool)
        .await
        .unwrap();
    let mut c = f.q.store.pool.begin().await.unwrap();
    assert_eq!(
        super::panel_summary(
            &mut c,
            f.q.store.clone(),
            &f.q.key,
            &f.scope.project,
            crate::finance::now().unwrap()
        )
        .await
        .unwrap_err(),
        E::Integrity
    );
}
