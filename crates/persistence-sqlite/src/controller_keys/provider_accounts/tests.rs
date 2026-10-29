#![cfg(target_os = "linux")]
use super::*;
use crate::{
    controller_keys::{InitializationRequest, LocalSetupAuthorization, ServiceIdentity},
    finance::*,
    resilience::{common, synthetic},
};
use rolvya_domain::ids::*;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
};
struct Setup;
impl LocalSetupAuthorization for Setup {
    fn permits(&self, r: &InitializationRequest) -> bool {
        r.request_id().as_str() == "provider-test-setup"
    }
}
struct Fixture {
    _root: tempfile::TempDir,
    registry: Arc<ProviderRegistry>,
    store: PlanStore,
    request: AdmissionRequest,
}
impl Fixture {
    async fn new() -> Self {
        let mut content = common::plan();
        content.tasks.push(common::task("task-b", &content.key));
        Self::for_plan(content, &synthetic::Authority::default()).await
    }
    async fn for_plan(
        content: PlanContent,
        policy: &impl rolvya_plan_validator::AssignmentPolicy,
    ) -> Self {
        let tmp = std::env::temp_dir();
        let uid = rustix::process::geteuid().as_raw();
        let base = if [0, uid].contains(&fs::metadata(&tmp).unwrap().uid()) {
            tmp
        } else {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .find(|p| p.join(".git").exists())
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf()
        };
        let root = tempfile::tempdir_in(base).unwrap();
        let data = root.path().join("data");
        let keys = root.path().join("keys");
        for p in [&data, &keys] {
            fs::create_dir(p).unwrap();
            fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let store = PlanStore::open(data.join("state.sqlite")).await.unwrap();
        let b = ControllerBootstrap::open(store.clone(), &keys, ServiceIdentity::current(), &[])
            .await
            .unwrap();
        b.initialize(
            &InitializationRequest::new(RequestId::new("provider-test-setup").unwrap()),
            &Setup,
        )
        .await
        .unwrap();
        let registry = Arc::new(ProviderRegistry::open(&b).await.unwrap());
        let validated =
            rolvya_plan_validator::validate(&content, policy, Limits::default()).unwrap();
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
                policy,
                Limits::default(),
            )
            .await
            .unwrap();
        store
            .apply_financial(
                &FinancialRequest {
                    project: content.key.project_id.clone(),
                    request_id: RequestId::new("provider-budget").unwrap(),
                    actor: common::owner(),
                    reason: "Synthetic fixture; zero provider spend".into(),
                    command: FinancialCommand::Approve {
                        expected_version: 0,
                        new_ceiling: synthetic::money(100),
                        proof: OwnerProof {
                            reference: synthetic::e("synthetic-budget"),
                            verified_at: SystemClock.now().unwrap(),
                        },
                    },
                },
                &synthetic::Finance,
            )
            .await
            .unwrap();
        let request = AdmissionRequest {
            task: TaskRef {
                plan: content.key,
                task_id: TaskId::new("task-a").unwrap(),
            },
            fingerprint: revision.fingerprint().into(),
            expected_epoch: 0,
            request_id: RequestId::new("provider-attempt-a").unwrap(),
            actor: common::owner(),
            permit: ExecutionPermit {
                id: synthetic::e("synthetic-permit-a"),
                provider: synthetic::e("synthetic"),
                account: synthetic::e("synthetic-account"),
                pool: synthetic::e("synthetic-pool"),
                executor: PrincipalId::new("synthetic-worker").unwrap(),
                maximum: synthetic::costs(),
                valid_until: synthetic::T + 3600,
            },
        };
        Self {
            _root: root,
            registry,
            store,
            request,
        }
    }
    async fn count(&self, table: &str) -> i64 {
        sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&self.store.pool)
            .await
            .unwrap()
    }
}
#[tokio::test]
async fn configured_controller_cannot_admit_without_durable_entitlement_even_with_legacy_callback()
{
    let f = Fixture::new().await;
    let result = f
        .store
        .admit(
            &f.request,
            &synthetic::Authority::default(),
            &crate::worker_simulation::DeterministicClock::new(synthetic::T).unwrap(),
            Limits::default(),
        )
        .await;
    assert!(
        result.is_err(),
        "missing entitlement must prevent the real admission/outbox transaction"
    );
    assert_eq!(f.count("task_attempts").await, 0);
    assert_eq!(f.count("dispatch_outbox").await, 0);
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
#[test]
fn capacity_is_retryable_but_missing_entitlement_and_identity_are_not() {
    assert!(EligibilityReason::QuotaExhausted.retryable());
    assert!(EligibilityReason::ConcurrencyUnavailable.retryable());
    assert!(!EligibilityReason::AwaitingProviderVerification.retryable());
    assert!(!EligibilityReason::AccountIdentityMismatch.retryable());
}
/// Private migration of historical ready-key synthetic fixtures. Synthetic
/// evidence has NO deserializable production variant and cannot activate a real
/// provider; real Owner/evidence flows have separate tests.
pub(in crate::controller_keys) async fn fixture_registry(
    b: &ControllerBootstrap,
    t: i64,
) -> Arc<ProviderRegistry> {
    let mut registry = Arc::new(ProviderRegistry::open(b).await.unwrap());
    let mut current_grants = std::collections::BTreeSet::new();
    let rows=sqlx::query("SELECT DISTINCT project_id,json_extract(definition,'$.execution') AS assignment FROM tasks WHERE kind='executable_leaf'").fetch_all(&registry.store.pool).await.unwrap();
    let mut tx = registry
        .store
        .pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .unwrap();
    for row in rows {
        let project = ProjectId::new(row.get::<String, _>("project_id")).unwrap();
        let a: ExecutionAssignment =
            serde_json::from_str(row.get::<&str, _>("assignment")).unwrap();
        let id = ConnectionId::parse(a.connection_ref.clone()).unwrap();
        if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM provider_project_grants WHERE connection_id=? AND project_id=?)").bind(id.as_str()).bind(project.as_str()).fetch_one(&mut *tx).await.unwrap(){continue;}
        let r = ConnectionRecord {
            id: id.clone(),
            format: 1,
            installation: hex(&registry.key.metadata.installation.0),
            owner: None,
            provider: match id.as_str() {
                "claude-a" | "claude-b" => "synthetic-anthropic",
                "codex-c" => "synthetic-openai",
                _ => "synthetic-model-provider",
            }
            .into(),
            account: if id.as_str() == "synthetic-connection" {
                "synthetic-model-account".into()
            } else {
                format!("synthetic-account-{}", id.as_str())
            },
            mode: a.access_mode,
            integration: "synthetic-test".into(),
            deployment: "synthetic-remote".into(),
            version: 1,
            revoked: false,
            models: vec![a.model.clone()],
            entitlement: None,
        };
        let existing=sqlx::query("SELECT c.connection_id,c.account_id,c.provider,s.record_id,s.purpose,s.current_version FROM secret_connections c JOIN secret_records s USING(project_id,connection_id) WHERE c.project_id=? AND (c.connection_id=? OR ?='synthetic-connection') AND c.state='active' ORDER BY c.connection_id,s.record_id LIMIT 1").bind(project.as_str()).bind(id.as_str()).bind(id.as_str()).fetch_optional(&mut *tx).await.unwrap();
        let mut r = r;
        let k = if let Some(x) = existing {
            r.provider = x.get("provider");
            r.account = x.get("account_id");
            CredentialReference {
                connection: x.get("connection_id"),
                account: r.account.clone(),
                provider: r.provider.clone(),
                record: x.get("record_id"),
                purpose: x.get("purpose"),
                version: x.get("current_version"),
            }
        } else {
            let k = CredentialReference {
                connection: a.connection_ref.clone(),
                account: r.account.clone(),
                provider: r.provider.clone(),
                record: "synthetic-credential".into(),
                purpose: "synthetic-model".into(),
                version: 1,
            };
            use super::super::secret_broker as broker;
            let scope = store::broker_scope(&project, &r, &k).unwrap();
            broker::fixture_record(
                &mut tx,
                &registry.key,
                &scope,
                &broker::SecretValue::new(broker::os_random::<32>().unwrap().to_vec()).unwrap(),
            )
            .await
            .unwrap();
            k
        };
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM provider_connections WHERE connection_id=?)",
        )
        .bind(id.as_str())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        if !exists {
            sqlx::query("INSERT INTO provider_connections VALUES(?,NULL,?,?,?,1)")
                .bind(id.as_str())
                .bind(&r.provider)
                .bind(&r.account)
                .bind(store::mode(r.mode))
                .execute(&mut *tx)
                .await
                .unwrap();
            store::save_version(&mut tx, &registry.key, &id, &r)
                .await
                .unwrap();
        }
        let mut g = ProjectGrant {
            connection: id.clone(),
            project: project.clone(),
            version: 1,
            consent_version: 1,
            models: vec![a.model.clone()],
            revoked: false,
            verified_connection_version: None,
            entitlement: None,
            credential: k,
            consent: "private-synthetic-fixture".into(),
        };
        store::save_grant(&mut tx, &registry.key, &g).await.unwrap();
        let mut current = store::connection(&mut tx, &registry.key, &id)
            .await
            .unwrap();
        // Existing account facts are retained when adding another project fixture.
        let e = Entitlement {
            policy: if r.mode == AccessMode::ApiKey {
                PolicyClass::ApiGenerallySupported
            } else {
                PolicyClass::ThirdPartySpecificallyPermitted
            },
            permission: if r.mode == AccessMode::ApiKey {
                AccountPermission::VerifiedApi
            } else {
                AccountPermission::VerifiedHostedCli
            },
            cli: if r.mode == AccessMode::CliSession {
                Some(CliIsolation {
                    home_identity: format!("synthetic-home-{}", id.as_str()),
                    revision: "synthetic-isolation-v1".into(),
                })
            } else {
                None
            },
            quota_unit: "synthetic-execution-bound".into(),
            source: EvidenceSource::Synthetic,
            evidence: synthetic::e("private-synthetic-entitlement"),
            provider: r.provider.clone(),
            account: r.account.clone(),
            mode: r.mode,
            integration: r.integration.clone(),
            deployment: r.deployment.clone(),
            source_url: "https://fixture.invalid/policy".into(),
            revision: "synthetic-policy-v1".into(),
            policy_version: 1,
            scope_digest: registry.scope_digest(&id, &current, &g).unwrap(),
            verified_against: current.version,
            collected_at: t,
            expires_at: t + 3600,
            credential_expires_at: t + 3600,
            verifier: "private-synthetic-provider".into(),
            decision: EligibilityReason::Eligible,
            models: vec![ModelCapability {
                model: a.model.clone(),
                revision: "synthetic-model-v1".into(),
                efforts: vec![Effort::Low, Effort::Medium, Effort::High, Effort::ExtraHigh],
                cpu_millis: 10000,
                memory_mib: 8192,
                timeout_seconds: 3600,
                infrastructure_provider: "synthetic".into(),
                infrastructure_account: "synthetic-account".into(),
                pool: "synthetic-pool".into(),
                region: "synthetic-region".into(),
                resource_class: "synthetic-class".into(),
                quota_units: 1,
            }],
            concurrency: 10000,
            quota_window: "synthetic-window".into(),
            quota_ceiling: 100000,
            refresh_serialized: true,
            refresh_operation: None,
        };
        let mut e = e;
        let mut secondary = e.models[0].clone();
        secondary.infrastructure_account = "synthetic-account-b".into();
        e.models.push(secondary);
        if current.entitlement.is_none() {
            current.version += 1;
            current.entitlement = Some(e.clone());
            store::save_version(&mut tx, &registry.key, &id, &current)
                .await
                .unwrap();
            sqlx::query("UPDATE provider_connections SET version=? WHERE connection_id=?")
                .bind(current.version)
                .bind(id.as_str())
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        g.version += 1;
        g.verified_connection_version = Some(current.version);
        current_grants.insert((
            id.as_str().to_owned(),
            project.as_str().to_owned(),
            e.scope_digest.clone(),
            e.evidence.as_str().to_owned(),
        ));
        g.entitlement = Some(e);
        store::save_grant(&mut tx, &registry.key, &g).await.unwrap();
        sqlx::query("INSERT OR IGNORE INTO provider_policy_revisions VALUES(?,?,?,1)")
            .bind(&r.provider)
            .bind(store::mode(r.mode))
            .bind("synthetic-policy-v1")
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
    Arc::get_mut(&mut registry).unwrap().verifier = Arc::new(FixtureCurrent(current_grants));
    registry
}
impl Fixture {
    async fn seeded(&self) -> Arc<ProviderRegistry> {
        let b = ControllerBootstrap::open(
            self.store.clone(),
            &self._root.path().join("keys"),
            ServiceIdentity::current(),
            &[],
        )
        .await
        .unwrap();
        fixture_registry(&b, synthetic::T).await
    }
    fn clock(&self) -> crate::worker_simulation::DeterministicClock {
        crate::worker_simulation::DeterministicClock::new(synthetic::T).unwrap()
    }
}
#[tokio::test]
async fn valid_scoped_evidence_reserves_capacity_and_finance_once_in_actual_admission() {
    let f = Fixture::new().await;
    let registry = f.seeded().await;
    let policy = registry.policy(synthetic::Authority::default());
    let a = f
        .store
        .admit(&f.request, &policy, &f.clock(), Limits::default())
        .await
        .unwrap();
    let b = f
        .store
        .admit(&f.request, &policy, &f.clock(), Limits::default())
        .await
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(f.count("provider_capacity_reservations").await, 1);
    assert_eq!(f.count("task_attempts").await, 1);
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
async fn opaque_registry_with_unverified_connection_or_missing_key_never_supplies_entitlement() {
    let f = Fixture::new().await;
    let p = f.registry.policy(synthetic::Authority::default());
    assert!(
        f.store
            .admit(&f.request, &p, &f.clock(), Limits::default())
            .await
            .is_err()
    );
    assert_eq!(f.count("provider_capacity_reservations").await, 0);
    assert_eq!(f.count("dispatch_outbox").await, 0);
}
struct RejectCurrent;
impl EntitlementVerifier for RejectCurrent {
    fn verify<'a>(&'a self, _: &'a VerificationCheck<'a>) -> VerificationFuture<'a> {
        Box::pin(async { Err(RegistryError::Unavailable) })
    }
    fn current(
        &self,
        _: &VerificationCheck<'_>,
        _: &Entitlement,
        _: i64,
    ) -> Result<(), RegistryError> {
        Err(RegistryError::Denied)
    }
}
#[tokio::test]
async fn withdrawing_current_provider_authority_denies_previously_cached_evidence() {
    let f = Fixture::new().await;
    let mut registry = f.seeded().await;
    Arc::get_mut(&mut registry).unwrap().verifier = Arc::new(RejectCurrent);
    let result = f
        .store
        .admit(
            &f.request,
            &registry.policy(synthetic::Authority::default()),
            &f.clock(),
            Limits::default(),
        )
        .await;
    assert!(
        result.is_err(),
        "cached evidence must not bypass the current verifier"
    );
    assert_eq!(f.count("task_attempts").await, 0);
    assert_eq!(f.count("dispatch_outbox").await, 0);
}
#[tokio::test]
async fn public_reopen_does_not_activate_cached_accounts_with_an_unavailable_verifier() {
    let f = Fixture::new().await;
    f.seeded().await;
    let result = f
        .store
        .admit(
            &f.request,
            &f.registry.policy(synthetic::Authority::default()),
            &f.clock(),
            Limits::default(),
        )
        .await;
    assert!(
        result.is_err(),
        "the public denying verifier must remain a mandatory gate after restart"
    );
    assert_eq!(f.count("provider_capacity_reservations").await, 0);
}
#[tokio::test]
async fn project_grant_removal_retires_authority_instead_of_only_consuming_the_proposal() {
    let f = Fixture::new().await;
    let registry = f.seeded().await;
    let id = ConnectionId::parse("synthetic-connection".into()).unwrap();
    let actor = hex(&super::super::secret_broker::os_random::<16>().unwrap()[..]);
    let project = &f.request.task.plan.project_id;
    sqlx::query("INSERT INTO human_users VALUES(?,'active',1,0,0)")
        .bind(&actor)
        .execute(&f.store.pool)
        .await
        .unwrap();
    let mut tx = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let r = store::connection(&mut tx, &registry.key, &id)
        .await
        .unwrap();
    let g = store::grant(&mut tx, &registry.key, &id, project)
        .await
        .unwrap();
    let change = ConnectionChange::RemoveGrant {
        connection: id.clone(),
        expected_version: r.version,
        expected_grant_version: g.version,
    };
    let digest = store::prepare(
        &mut tx,
        &registry.key,
        &actor,
        project,
        &change,
        synthetic::T,
        "removal-proposal",
    )
    .await
    .unwrap();
    store::apply_proposal(
        &mut tx,
        &registry.key,
        "removal-proposal",
        &digest,
        &actor,
        project,
        synthetic::T,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let mut read = f.store.pool.begin().await.unwrap();
    let retired = store::grant(&mut read, &registry.key, &id, project)
        .await
        .unwrap();
    assert!(
        retired.revoked,
        "a successful RemoveGrant must retire its signed grant"
    );
    assert!(retired.entitlement.is_none());
    assert!(retired.verified_connection_version.is_none());
    assert!(retired.version > g.version);
    let original: String = sqlx::query_scalar("SELECT record FROM provider_project_grant_versions WHERE connection_id=? AND project_id=? AND version=?")
        .bind(id.as_str()).bind(project.as_str()).bind(g.version).fetch_one(&f.store.pool).await.unwrap();
    assert_eq!(
        original,
        encode(&g).unwrap(),
        "retiring a grant must preserve its scoped entitlement evidence for prior attempts"
    );
}

// Only installation-authenticated private synthetic fixture records are accepted.
// This type and the Synthetic evidence variant do not exist in production.
struct FixtureCurrent(std::collections::BTreeSet<(String, String, String, String)>);
impl EntitlementVerifier for FixtureCurrent {
    fn verify<'a>(&'a self, _: &'a VerificationCheck<'a>) -> VerificationFuture<'a> {
        Box::pin(async { Err(RegistryError::Unavailable) })
    }
    fn current(
        &self,
        c: &VerificationCheck<'_>,
        e: &Entitlement,
        t: i64,
    ) -> Result<(), RegistryError> {
        c.validate()?;
        let explicit = self.0.contains(&(
            c.connection.as_str().to_owned(),
            c.grant.project.as_str().to_owned(),
            e.scope_digest.clone(),
            c.evidence.as_str().to_owned(),
        ));
        if e.source != EvidenceSource::Synthetic
            || t >= e.expires_at
            || (!explicit && e.verifier != "private-synthetic-provider")
        {
            return Err(RegistryError::Denied);
        }
        Ok(())
    }
}

mod adversarial;
/// Reopen test-only verification of already MAC-authenticated synthetic evidence;
/// this does not enroll, refresh or synthesize any durable provider authority.
pub(in crate::controller_keys) async fn fixture_reopen(
    b: &ControllerBootstrap,
) -> Arc<ProviderRegistry> {
    let mut r = ProviderRegistry::open(b).await.unwrap();
    r.verifier = Arc::new(FixtureCurrent(Default::default()));
    Arc::new(r)
}
pub(in crate::controller_keys) async fn fixture_limit(
    r: &ProviderRegistry,
    p: &ProjectId,
    until: i64,
) {
    let mut tx = r.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT connection_id FROM provider_project_grants WHERE project_id=?")
            .bind(p.as_str())
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    for id in ids {
        let id = ConnectionId::parse(id).unwrap();
        let mut g = store::grant(&mut tx, &r.key, &id, p).await.unwrap();
        g.version += 1;
        let e = g.entitlement.as_mut().unwrap();
        e.expires_at = until;
        e.credential_expires_at = until;
        store::save_grant(&mut tx, &r.key, &g).await.unwrap();
    }
    tx.commit().await.unwrap();
}

mod multiple_accounts;

mod crashes;
struct OwnerVerifier {
    connection: ConnectionId,
    project: ProjectId,
    owner: String,
    e: Entitlement,
    current: Arc<std::sync::atomic::AtomicBool>,
}
impl EntitlementVerifier for OwnerVerifier {
    fn verify<'a>(&'a self, c: &'a VerificationCheck<'a>) -> VerificationFuture<'a> {
        Box::pin(async move {
            self.current(c, &self.e, c.now)?;
            Ok(self.e.clone())
        })
    }
    fn current(
        &self,
        c: &VerificationCheck<'_>,
        e: &Entitlement,
        t: i64,
    ) -> Result<(), RegistryError> {
        c.validate()?;
        if !self.current.load(std::sync::atomic::Ordering::SeqCst)
            || c.connection != &self.connection
            || c.grant.project != self.project
            || c.record.owner.as_deref() != Some(self.owner.as_str())
            || c.evidence != &self.e.evidence
            || e != &self.e
            || t >= e.deadline()
        {
            return Err(RegistryError::Denied);
        }
        Ok(())
    }
}
pub(in crate::controller_keys) async fn fixture_owner_verifier(
    b: &ControllerBootstrap,
    id: &ConnectionId,
    p: &ProjectId,
    t: i64,
    until: i64,
) -> (Arc<ProviderRegistry>, Arc<std::sync::atomic::AtomicBool>) {
    let mut registry = ProviderRegistry::open(b).await.unwrap();
    let mut read = registry.store.pool.begin().await.unwrap();
    let r = store::connection(&mut read, &registry.key, id)
        .await
        .unwrap();
    let g = store::grant(&mut read, &registry.key, id, p).await.unwrap();
    read.commit().await.unwrap();
    let e = Entitlement {
        source: EvidenceSource::Synthetic,
        evidence: synthetic::e("synthetic-provider-owner-proof"),
        policy: PolicyClass::ApiGenerallySupported,
        permission: AccountPermission::VerifiedApi,
        cli: None,
        quota_unit: "synthetic-request-bound".into(),
        provider: r.provider.clone(),
        account: r.account.clone(),
        mode: r.mode,
        integration: r.integration.clone(),
        deployment: r.deployment.clone(),
        source_url: "https://fixture.invalid/policy".into(),
        revision: "synthetic-v1".into(),
        policy_version: 1,
        scope_digest: registry.scope_digest(id, &r, &g).unwrap(),
        verified_against: r.version,
        collected_at: t,
        expires_at: until,
        credential_expires_at: until,
        verifier: "private-exact-owner-fixture".into(),
        decision: EligibilityReason::Eligible,
        models: g
            .models
            .iter()
            .map(|model| ModelCapability {
                model: model.clone(),
                revision: "synthetic-v1".into(),
                efforts: vec![Effort::Medium],
                cpu_millis: 1000,
                memory_mib: 512,
                timeout_seconds: 3600,
                infrastructure_provider: "synthetic".into(),
                infrastructure_account: "synthetic-account".into(),
                pool: "synthetic-pool".into(),
                region: "synthetic-region".into(),
                resource_class: "synthetic-class".into(),
                quota_units: 1,
            })
            .collect(),
        concurrency: 1,
        quota_window: "synthetic-window".into(),
        quota_ceiling: 10,
        refresh_serialized: true,
        refresh_operation: None,
    };
    let current = Arc::new(std::sync::atomic::AtomicBool::new(true));
    registry.verifier = Arc::new(OwnerVerifier {
        connection: id.clone(),
        project: p.clone(),
        owner: r.owner.unwrap(),
        e,
        current: current.clone(),
    });
    (Arc::new(registry), current)
}
