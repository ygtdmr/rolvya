use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use sqlx::{Row, SqliteConnection};
use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet},
};

// Closed issuer registry, never selected by worker/JSON/environment input.
struct Issuer {
    key: [u8; 32],
    domains: BTreeSet<Domain>,
    simulated: bool,
}
pub struct QualificationStore<K = ControllerKey> {
    store: PlanStore,
    key: K,
    issuers: BTreeMap<String, Issuer>,
    clock: std::sync::Arc<dyn crate::admission::Clock>,
    #[cfg(test)]
    hook: Box<dyn Fn(&str) + Send + Sync>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    statement: EvidenceStatement,
    origin: Origin,
    issuer: String,
    signature: Option<String>,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Origin {
    Configuration,
    Independent,
    #[cfg(test)]
    Simulation,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QualificationReport {
    pub version: u16,
    pub scope_sha256: String,
    pub scope: QualificationScope,
    pub validator_version: &'static str,
    pub inspected: Vec<Domain>,
    pub simulated: Vec<Domain>,
    pub observed: Vec<Domain>,
    pub qualified: Vec<Domain>,
    pub missing: Vec<Domain>,
    pub expired: u32,
    pub revoked: u32,
    pub restore_hold: bool,
    pub financial_revision_current: bool,
    pub financial_uncertainty: bool,
    pub paid_execution_allowed: bool,
}
impl QualificationStore {
    pub async fn open(b: &ControllerBootstrap) -> Result<Self, E> {
        Ok(Self {
            store: b.store.clone(),
            key: b.load_existing().await.map_err(|_| E::Integrity)?,
            issuers: BTreeMap::new(),
            clock: std::sync::Arc::new(crate::admission::SystemClock),
            #[cfg(test)]
            hook: Box::new(|_| {}),
        })
    }
}
impl<K: Borrow<ControllerKey>> QualificationStore<K> {
    fn now(&self) -> Result<i64, E> {
        self.clock.now().map_err(|_| E::Integrity)
    }
    fn checkpoint(&self, label: &str) {
        #[cfg(test)]
        (self.hook)(label);
        let _ = label;
    }
    fn mac(&self, domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
        let mut h = <Hmac<Sha256> as Mac>::new_from_slice(&self.key.borrow().material[..])
            .expect("HMAC key length");
        for p in [b"rolvya/provider-qualification/v1".as_slice(), domain]
            .into_iter()
            .chain(parts.iter().copied())
        {
            h.update(&(p.len() as u64).to_be_bytes());
            h.update(p)
        }
        h.finalize().into_bytes().into()
    }
    fn verify(&self, r: &Record, now: i64) -> Result<(), E> {
        r.statement.validate(now)?;
        match r.origin {
            Origin::Configuration
                if r.statement.level == EvidenceLevel::ConfigurationInspected
                    && r.issuer == "rust-configuration-v1"
                    && r.signature.is_none() =>
            {
                Ok(())
            }
            Origin::Independent => self.verify_signature(
                &r.statement,
                &r.issuer,
                r.signature.as_deref().ok_or(E::Integrity)?,
                false,
            ),
            #[cfg(test)]
            Origin::Simulation => self.verify_signature(
                &r.statement,
                &r.issuer,
                r.signature.as_deref().ok_or(E::Integrity)?,
                true,
            ),
            _ => Err(E::UntrustedIssuer),
        }
    }
    fn verify_signature(
        &self,
        s: &EvidenceStatement,
        issuer: &str,
        signature: &str,
        simulated: bool,
    ) -> Result<(), E> {
        let k = self.issuers.get(issuer).ok_or(E::UntrustedIssuer)?;
        if k.simulated != simulated
            || !k.domains.contains(&s.domain)
            || !matches!(
                s.level,
                EvidenceLevel::ExternallyObserved
                    | EvidenceLevel::IndependentlyQualified
                    | EvidenceLevel::Simulated
            )
            || (simulated && s.level != EvidenceLevel::Simulated)
            || (!simulated && s.level == EvidenceLevel::Simulated)
        {
            return Err(E::UntrustedIssuer);
        }
        if signature.len() > 128 {
            return Err(E::InvalidInput);
        }
        let signature = STANDARD.decode(signature).map_err(|_| E::InvalidInput)?;
        let mut signed = b"rolvya/independent-provider-qualification/v1\0".to_vec();
        signed.extend(serde_json::to_vec(s).map_err(|_| E::Integrity)?);
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, k.key)
            .verify(&signed, &signature)
            .map_err(|_| E::UntrustedIssuer)
    }
    /// Supported declaration checks only, never effective IAM or spending proof.
    pub async fn inspect_configuration(
        &self,
        scope: &QualificationScope,
        request: &RequestId,
        config: &IsolationConfiguration,
    ) -> Result<QualificationReport, E> {
        let commitment = config.inspect()?;
        if commitment != scope.configuration_sha256
            || workload_commitment(if scope.operation == OperationClass::VerifierLaunch {
                &config.verifier
            } else {
                &config.coding
            })? != scope.workload_sha256
        {
            return Err(E::Scope);
        }
        let workload = if scope.operation == OperationClass::VerifierLaunch {
            &config.verifier
        } else {
            &config.coding
        };
        if scope.partition != workload.cluster.partition()
            || scope.region != workload.cluster.region()
            || scope.aws_account != workload.cluster.account()
            || scope.task_definition != workload.task_definition
            || scope.image_digest != workload.image_digest
        {
            return Err(E::Scope);
        }
        let now = self.now()?;
        let mut c = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        self.current_installation(scope)?;
        self.insert(
            &mut c,
            &Record {
                statement: EvidenceStatement {
                    version: 1,
                    request: request.clone(),
                    scope: scope.clone(),
                    domain: Domain::WorkloadIsolation,
                    level: EvidenceLevel::ConfigurationInspected,
                    evidence_sha256: commitment,
                    source_revision: digest("rolvya/qualification-validator/v1", &1)?,
                    observed_at: now,
                    expires_at: now.checked_add(3600).ok_or(E::InvalidInput)?,
                },
                origin: Origin::Configuration,
                issuer: "rust-configuration-v1".into(),
                signature: None,
            },
            now,
        )
        .await?;
        let report = self.report_in(&mut c, scope, now, false).await?;
        self.checkpoint("inspection-before");
        c.commit().await?;
        self.checkpoint("inspection-after");
        Ok(report)
    }
    /// Signatures from unknown issuers confer no authority. No real issuer is
    /// installed by this build: its enrollment requires separate reviewed proof.
    pub async fn submit_evidence(&self, s: &SignedEvidence) -> Result<(), E> {
        let now = self.now()?;
        let origin = if self.issuers.get(&s.issuer).is_some_and(|i| i.simulated) {
            #[cfg(test)]
            {
                Origin::Simulation
            }
            #[cfg(not(test))]
            {
                return Err(E::UntrustedIssuer);
            }
        } else {
            Origin::Independent
        };
        let r = Record {
            statement: s.statement.clone(),
            origin,
            issuer: s.issuer.clone(),
            signature: Some(s.signature.clone()),
        };
        self.verify(&r, now)?;
        let mut c = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        self.current_installation(&s.statement.scope)?;
        if !super::super::operations::external_authority_available(&mut c).await? {
            return Err(E::Revoked);
        }
        self.insert(&mut c, &r, now).await?;
        self.verify(&r, self.now()?)?;
        self.checkpoint("evidence-before");
        c.commit().await?;
        self.checkpoint("evidence-after");
        Ok(())
    }
    fn current_installation(&self, s: &QualificationScope) -> Result<(), E> {
        s.commitment()?;
        let installation = self.key.borrow().coding_installation();
        if s.installation != installation {
            return Err(E::Scope);
        }
        Ok(())
    }
    async fn insert(&self, c: &mut SqliteConnection, r: &Record, now: i64) -> Result<(), E> {
        self.verify(r, now)?;
        let s = &r.statement;
        let text = serde_json::to_string(r).map_err(|_| E::Integrity)?;
        if text.len() > 16384 {
            return Err(E::InvalidInput);
        }
        let old=sqlx::query("SELECT record,record_mac FROM provider_qualification_evidence WHERE project_id=? AND request_id=?").bind(s.scope.project.as_str()).bind(s.request.as_str()).fetch_optional(&mut *c).await?;
        if let Some(old) = old {
            let old_text: String = old.try_get("record")?;
            if !constant_eq(
                &old.try_get::<Vec<u8>, _>("record_mac")?,
                &self.mac(b"record", &[old_text.as_bytes()]),
            ) {
                return Err(E::Integrity);
            }
            let old: Record = serde_json::from_str(&old_text).map_err(|_| E::Integrity)?;
            self.verify(&old, now)?;
            if old.statement.scope != s.scope
                || old.statement.domain != s.domain
                || old.statement.level != s.level
                || old.statement.evidence_sha256 != s.evidence_sha256
                || old.statement.source_revision != s.source_revision
                || old.issuer != r.issuer
                || old.signature != r.signature
            {
                return Err(E::Conflict);
            }
            return Ok(());
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM provider_qualification_evidence WHERE project_id=?",
        )
        .bind(s.scope.project.as_str())
        .fetch_one(&mut *c)
        .await?;
        if count >= 4096 {
            return Err(E::InvalidInput);
        }
        one(
            sqlx::query("INSERT INTO provider_qualification_evidence VALUES(?,?,?,?,?,?,?,?)")
                .bind(s.scope.project.as_str())
                .bind(s.request.as_str())
                .bind(s.scope.commitment()?)
                .bind(serde_json::to_string(&s.domain).map_err(|_| E::Integrity)?)
                .bind(&text)
                .bind(self.mac(b"record", &[text.as_bytes()]).as_slice())
                .bind(s.observed_at)
                .bind(s.expires_at)
                .execute(c)
                .await?
                .rows_affected(),
        )
    }
    pub async fn report(&self, scope: &QualificationScope) -> Result<QualificationReport, E> {
        let mut c = self.store.pool.begin().await?;
        let report = self.report_in(&mut c, scope, self.now()?, false).await?;
        c.commit().await?;
        Ok(report)
    }
    async fn report_in(
        &self,
        c: &mut SqliteConnection,
        scope: &QualificationScope,
        now: i64,
        simulation: bool,
    ) -> Result<QualificationReport, E> {
        self.current_installation(scope)?;
        let scope_sha256 = scope.commitment()?;
        let rows=sqlx::query("SELECT e.*,r.request_id AS revoked_request,r.recorded_at AS revoked_at,r.record_mac AS revocation_mac FROM provider_qualification_evidence e LEFT JOIN provider_qualification_revocations r ON r.project_id=e.project_id AND r.evidence_request=e.request_id WHERE e.project_id=? AND e.scope_sha256=? ORDER BY e.request_id LIMIT 4097").bind(scope.project.as_str()).bind(&scope_sha256).fetch_all(&mut *c).await?;
        if rows.len() > 4096 {
            return Err(E::Integrity);
        }
        let mut inspected = BTreeSet::new();
        let mut simulated = BTreeSet::new();
        let mut observed = BTreeSet::new();
        let mut qualified = BTreeSet::new();
        let mut expired = 0;
        let mut revoked = 0;
        for row in rows {
            let text: String = row.try_get("record")?;
            if text.len() > 16384
                || !constant_eq(
                    &row.try_get::<Vec<u8>, _>("record_mac")?,
                    &self.mac(b"record", &[text.as_bytes()]),
                )
            {
                return Err(E::Integrity);
            }
            let r: Record = serde_json::from_str(&text).map_err(|_| E::Integrity)?;
            if r.statement.scope != *scope
                || r.statement.request.as_str() != row.try_get::<&str, _>("request_id")?
                || r.statement.observed_at != row.try_get::<i64, _>("observed_at")?
                || r.statement.expires_at != row.try_get::<i64, _>("expires_at")?
                || serde_json::to_string(&r.statement.domain).map_err(|_| E::Integrity)?
                    != row.try_get::<&str, _>("domain")?
            {
                return Err(E::Integrity);
            }
            if self.revoked(&row, &r)? {
                revoked += 1;
                continue;
            }
            match self.verify(&r, now) {
                Err(E::Expired) => {
                    expired += 1;
                    continue;
                }
                Err(e) => return Err(e),
                Ok(()) => (),
            }
            let d = r.statement.domain;
            match r.statement.level {
                EvidenceLevel::ConfigurationInspected => {
                    inspected.insert(d);
                }
                EvidenceLevel::Simulated => {
                    simulated.insert(d);
                    if simulation {
                        qualified.insert(d);
                    }
                }
                EvidenceLevel::ExternallyObserved => {
                    observed.insert(d);
                }
                EvidenceLevel::IndependentlyQualified => {
                    qualified.insert(d);
                }
            }
        }
        let restore_hold = !super::super::operations::external_authority_available(c).await?;
        let budget = crate::finance::snapshot(c, &scope.project)
            .await
            .map_err(|_| E::Scope)?;
        let financial_revision_current = budget.version == scope.financial_revision;
        let financial_uncertainty = budget.uncertain_liabilities != 0 || budget.over_ceiling;
        let missing = REQUIRED
            .into_iter()
            .filter(|d| !qualified.contains(d))
            .collect::<Vec<_>>();
        Ok(QualificationReport {
            version: 1,
            scope_sha256,
            scope: scope.clone(),
            validator_version: concat!(env!("CARGO_PKG_VERSION"), "/qualification-v1"),
            inspected: inspected.into_iter().collect(),
            simulated: simulated.into_iter().collect(),
            observed: observed.into_iter().collect(),
            qualified: qualified.into_iter().collect(),
            missing,
            expired,
            revoked,
            restore_hold,
            financial_revision_current,
            financial_uncertainty,
            paid_execution_allowed: false,
        })
    }
    fn revoked(&self, row: &sqlx::sqlite::SqliteRow, r: &Record) -> Result<bool, E> {
        let Some(request) = row.try_get::<Option<String>, _>("revoked_request")? else {
            return Ok(false);
        };
        let at: i64 = row.try_get("revoked_at")?;
        if !constant_eq(
            &row.try_get::<Vec<u8>, _>("revocation_mac")?,
            &self.mac(
                b"revoke",
                &[
                    r.statement.scope.project.as_str().as_bytes(),
                    r.statement.request.as_str().as_bytes(),
                    request.as_bytes(),
                    &at.to_be_bytes(),
                ],
            ),
        ) {
            return Err(E::Integrity);
        }
        Ok(true)
    }
    /// Restriction only: never clears a restore hold or changes financial state.
    pub async fn revoke(
        &self,
        project: &ProjectId,
        evidence: &RequestId,
        request: &RequestId,
    ) -> Result<(), E> {
        let at = self.now()?;
        let mut c = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let old=sqlx::query("SELECT request_id,recorded_at,record_mac FROM provider_qualification_revocations WHERE project_id=? AND evidence_request=?").bind(project.as_str()).bind(evidence.as_str()).fetch_optional(&mut *c).await?;
        if let Some(old) = old {
            let old_at: i64 = old.try_get("recorded_at")?;
            let old_request: &str = old.try_get("request_id")?;
            if old_request != request.as_str() {
                return Err(E::Conflict);
            }
            if !constant_eq(
                &old.try_get::<Vec<u8>, _>("record_mac")?,
                &self.mac(
                    b"revoke",
                    &[
                        project.as_str().as_bytes(),
                        evidence.as_str().as_bytes(),
                        old_request.as_bytes(),
                        &old_at.to_be_bytes(),
                    ],
                ),
            ) {
                return Err(E::Integrity);
            }
        } else {
            one(
                sqlx::query("INSERT INTO provider_qualification_revocations VALUES(?,?,?,?,?)")
                    .bind(project.as_str())
                    .bind(evidence.as_str())
                    .bind(request.as_str())
                    .bind(at)
                    .bind(
                        self.mac(
                            b"revoke",
                            &[
                                project.as_str().as_bytes(),
                                evidence.as_str().as_bytes(),
                                request.as_str().as_bytes(),
                                &at.to_be_bytes(),
                            ],
                        )
                        .as_slice(),
                    )
                    .execute(&mut *c)
                    .await?
                    .rows_affected(),
            )?;
        }
        self.checkpoint("revocation-before");
        c.commit().await?;
        self.checkpoint("revocation-after");
        Ok(())
    }
}
fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    a.ct_eq(b).into()
}
fn one(n: u64) -> Result<(), E> {
    if n == 1 { Ok(()) } else { Err(E::Storage) }
}

impl<'a> QualificationStore<&'a ControllerKey> {
    pub(crate) fn borrowed(store: PlanStore, key: &'a ControllerKey) -> Self {
        Self {
            store,
            key,
            issuers: BTreeMap::new(),
            clock: std::sync::Arc::new(crate::admission::SystemClock),
            #[cfg(test)]
            hook: Box::new(|_| {}),
        }
    }
}

impl<K: Borrow<ControllerKey>> QualificationStore<K> {
    /// Internal composition only. The caller derives all expected fields after
    /// R06/R08/current registry checks, inside the same transaction. This checks
    /// evidence; the existing launch/call journal alone admits an external effect.
    pub(crate) async fn require_operation_in(
        &self,
        c: &mut SqliteConnection,
        expected: &QualificationScope,
        now: i64,
    ) -> Result<(), E> {
        self.current_installation(expected)?;
        let rows=sqlx::query("SELECT record,record_mac FROM provider_qualification_evidence WHERE project_id=? AND json_extract(record,'$.statement.scope.operation_id')=? ORDER BY request_id LIMIT 4097").bind(expected.project.as_str()).bind(&expected.operation_id).fetch_all(&mut *c).await?;
        if rows.len() > 4096 {
            return Err(E::Integrity);
        }
        let mut matched = None;
        for row in rows {
            let text: String = row.try_get("record")?;
            if text.len() > 16384
                || !constant_eq(
                    &row.try_get::<Vec<u8>, _>("record_mac")?,
                    &self.mac(b"record", &[text.as_bytes()]),
                )
            {
                return Err(E::Integrity);
            }
            let r: Record = serde_json::from_str(&text).map_err(|_| E::Integrity)?;
            let mut candidate = expected.clone();
            // Independent statements bind the complete inspected configuration;
            // the actual workload/execution/exposure/version fields must match.
            candidate.configuration_sha256 = r.statement.scope.configuration_sha256.clone();
            if candidate != r.statement.scope {
                continue;
            }
            if matched.as_ref().is_some_and(|s| s != &candidate) {
                return Err(E::Conflict);
            }
            matched = Some(candidate);
        }
        let scope = matched.ok_or(E::UntrustedIssuer)?;
        let simulation =
            cfg!(test) && !self.issuers.is_empty() && self.issuers.values().all(|i| i.simulated);
        let report = self.report_in(c, &scope, now, simulation).await?;
        if !report.missing.is_empty()
            || report.restore_hold
            || !report.financial_revision_current
            || report.financial_uncertainty
        {
            return Err(E::Revoked);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/store.rs"]
mod tests;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualificationSummary {
    pub configuration_inspected: u32,
    pub simulated: u32,
    pub untrusted: u32,
    pub independently_qualified: u32,
    pub paid_execution_allowed: bool,
    pub blockers: Vec<Domain>,
}
pub(crate) async fn panel_summary(
    c: &mut SqliteConnection,
    store: PlanStore,
    key: &ControllerKey,
    project: &ProjectId,
    now: i64,
) -> Result<QualificationSummary, E> {
    let q = QualificationStore::borrowed(store, key);
    let rows=sqlx::query("SELECT e.*,r.request_id AS revoked_request,r.recorded_at AS revoked_at,r.record_mac AS revocation_mac FROM provider_qualification_evidence e LEFT JOIN provider_qualification_revocations r ON r.project_id=e.project_id AND r.evidence_request=e.request_id WHERE e.project_id=? ORDER BY e.request_id LIMIT 4097").bind(project.as_str()).fetch_all(c).await?;
    if rows.len() > 4096 {
        return Err(E::Integrity);
    }
    let mut inspected = 0;
    let mut simulated = 0;
    let mut untrusted = 0;
    for row in rows {
        let text: String = row.try_get("record")?;
        if text.len() > 16384
            || !constant_eq(
                &row.try_get::<Vec<u8>, _>("record_mac")?,
                &q.mac(b"record", &[text.as_bytes()]),
            )
        {
            return Err(E::Integrity);
        }
        let r: Record = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(_) => {
                untrusted += 1;
                continue;
            }
        };
        if r.statement.scope.project != *project
            || q.current_installation(&r.statement.scope).is_err()
        {
            return Err(E::Scope);
        }
        if r.statement.request.as_str() != row.try_get::<&str, _>("request_id")?
            || r.statement.scope.commitment()? != row.try_get::<&str, _>("scope_sha256")?
        {
            return Err(E::Integrity);
        }
        if q.revoked(&row, &r)? {
            continue;
        }
        if r.statement.validate(now).is_err() {
            continue;
        }
        match q.verify(&r, now) {
            Ok(()) if r.statement.level == EvidenceLevel::ConfigurationInspected => inspected += 1,
            _ if r.statement.level == EvidenceLevel::Simulated => simulated += 1,
            _ => untrusted += 1,
        }
    }
    Ok(QualificationSummary {
        configuration_inspected: inspected,
        simulated,
        untrusted,
        independently_qualified: 0,
        paid_execution_allowed: false,
        blockers: REQUIRED.to_vec(),
    })
}

#[cfg(test)]
impl QualificationStore {
    pub(in crate::controller_keys) async fn fixture_simulation(
        b: &ControllerBootstrap,
        clock: std::sync::Arc<dyn crate::admission::Clock>,
        scope: &QualificationScope,
    ) -> Result<Self, E> {
        use ring::signature::{Ed25519KeyPair, KeyPair};
        let mut q = Self::open(b).await?;
        q.clock = clock;
        let raw = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
            .map_err(|_| E::Integrity)?;
        let key = Ed25519KeyPair::from_pkcs8(raw.as_ref()).map_err(|_| E::Integrity)?;
        q.issuers.insert(
            "private-offline-issuer".into(),
            Issuer {
                key: key
                    .public_key()
                    .as_ref()
                    .try_into()
                    .map_err(|_| E::Integrity)?,
                domains: REQUIRED.into_iter().collect(),
                simulated: true,
            },
        );
        let now = q.now()?;
        for domain in REQUIRED {
            let statement = EvidenceStatement {
                version: 1,
                request: RequestId::new(format!("qualification-{}-{domain:?}", scope.operation_id))
                    .or_else(|_| {
                        RequestId::new(format!(
                            "qualification-{}-{}",
                            &scope.operation_id[..16],
                            serde_json::to_string(&domain).unwrap().trim_matches('"')
                        ))
                    })
                    .map_err(|_| E::InvalidInput)?,
                scope: scope.clone(),
                domain,
                level: EvidenceLevel::Simulated,
                evidence_sha256: digest("private-simulation-domain/v1", &domain)?,
                source_revision: digest("private-fixture-revision/v1", &1)?,
                observed_at: now,
                expires_at: now + 60,
            };
            let mut signed = b"rolvya/independent-provider-qualification/v1\0".to_vec();
            signed.extend(serde_json::to_vec(&statement).map_err(|_| E::Integrity)?);
            q.submit_evidence(&SignedEvidence {
                statement,
                issuer: "private-offline-issuer".into(),
                signature: STANDARD.encode(key.sign(&signed).as_ref()),
            })
            .await?;
        }
        Ok(q)
    }
}

/// Private fixture evidence issuer, kept in the trusted fixture process. Its
/// signing capability is not part of worker transport or production builds.
#[cfg(test)]
pub(in crate::controller_keys) struct SimulationAuthority {
    q: QualificationStore,
    signer: ring::signature::Ed25519KeyPair,
}
#[cfg(test)]
impl SimulationAuthority {
    pub(in crate::controller_keys) async fn open(
        b: &ControllerBootstrap,
        clock: std::sync::Arc<dyn crate::admission::Clock>,
    ) -> Result<Self, E> {
        use ring::signature::{Ed25519KeyPair, KeyPair};
        let mut q = QualificationStore::open(b).await?;
        q.clock = clock;
        let raw = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
            .map_err(|_| E::Integrity)?;
        let signer = Ed25519KeyPair::from_pkcs8(raw.as_ref()).map_err(|_| E::Integrity)?;
        q.issuers.insert(
            "private-proxy-qualification".into(),
            Issuer {
                key: signer
                    .public_key()
                    .as_ref()
                    .try_into()
                    .map_err(|_| E::Integrity)?,
                domains: REQUIRED.into_iter().collect(),
                simulated: true,
            },
        );
        Ok(Self { q, signer })
    }
    pub(in crate::controller_keys) async fn qualify_and_require_in(
        &self,
        c: &mut SqliteConnection,
        scope: &QualificationScope,
        now: i64,
    ) -> Result<(), E> {
        let mut scope = scope.clone();
        scope.configuration_sha256 = scope.workload_sha256.clone();
        for domain in REQUIRED {
            let request = RequestId::new(digest(
                "private-proxy-qualification-id/v1",
                &(&scope, domain),
            )?)
            .map_err(|_| E::InvalidInput)?;
            let existing:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM provider_qualification_evidence WHERE project_id=? AND request_id=?)").bind(scope.project.as_str()).bind(request.as_str()).fetch_one(&mut *c).await?;
            if existing {
                continue;
            }
            let statement = EvidenceStatement {
                version: 1,
                request,
                scope: scope.clone(),
                domain,
                level: EvidenceLevel::Simulated,
                evidence_sha256: digest("private-proxy-domain/v1", &domain)?,
                source_revision: digest("private-proxy-version/v1", &1)?,
                observed_at: now,
                expires_at: now.checked_add(60).ok_or(E::Expired)?,
            };
            let mut signed = b"rolvya/independent-provider-qualification/v1\0".to_vec();
            signed.extend(serde_json::to_vec(&statement).map_err(|_| E::Integrity)?);
            self.q
                .insert(
                    c,
                    &Record {
                        statement,
                        origin: Origin::Simulation,
                        issuer: "private-proxy-qualification".into(),
                        signature: Some(STANDARD.encode(self.signer.sign(&signed).as_ref())),
                    },
                    now,
                )
                .await?;
        }
        self.q.require_operation_in(c, &scope, now).await
    }
}
