//! Independent Fargate verification composes the existing AWS journal and R09
//! authority. No public activation or caller-installed identity verifier exists.
use super::*;
use crate::orchestration::{
    SyntheticCoordinator,
    coding::{
        self,
        execution::{FargateProofReference, VerifierExecution},
    },
};
use ring::signature::{Ed25519KeyPair, KeyPair};
use rolvya_domain::{money::CostBreakdown, worker::Digest};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Configuration commitments only, never evidence of effective confinement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierConfiguration {
    pub executables: Vec<(String, Digest)>,
}
impl VerifierConfiguration {
    fn validate(&self, policy: &coding::VerificationPolicy, w: &FargateWorkload) -> Result<(), E> {
        if self.executables.is_empty()
            || self.executables.len() > 8
            || self.executables.len() != policy.commands.len()
            || w.runtime_seconds < policy.timeout_seconds
            || w.runtime_seconds > 300
            || u64::from(w.cpu_units) * 1000 / 1024 < u64::from(policy.cpu_millis)
            || w.memory_mib < policy.memory_mib
        {
            return Err(E::IsolationMismatch);
        }
        for ((path, _), argv) in self.executables.iter().zip(&policy.commands) {
            if argv.first() != Some(path)
                || !path.starts_with('/')
                || path.split('/').any(|v| v == "..")
            {
                return Err(E::IsolationMismatch);
            }
        }
        let policy_hash = coding::commitment(policy).map_err(|_| E::Integrity)?;
        let executables_hash = coding::commitment(&self.executables).map_err(|_| E::Integrity)?;
        if w.command
            != [
                "/opt/rolvya/verifier",
                "--policy",
                policy_hash.as_str(),
                "--executables",
                executables_hash.as_str(),
            ]
        {
            return Err(E::IsolationMismatch);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierLaunch {
    pub grant: VerifierExecution,
    pub coding_request: Digest,
    pub coding_maximum: CostBreakdown,
    pub configuration: VerifierConfiguration,
}
impl VerifierLaunch {
    pub(super) fn validate(&self, r: &LaunchRequest, now: i64) -> Result<(), E> {
        let g = &self.grant;
        if g.version != 1
            || g.fargate.is_some()
            || g.installation != r.installation
            || g.binding != r.binding
            || g.public_key.len() != 32
            || g.issued_at != r.authorized_at
            || g.expires_at <= now
            || g.expires_at > r.authorization_until
            || g.execution.is_empty()
            || g.execution.len() > 120
            || g.subject != "pending-fargate-possession"
            || g.authority != "aws-fargate-verifier-v1"
            || g.image
                != coding::commitment(&self.configuration.executables).map_err(|_| E::Integrity)?
        {
            return Err(E::ScopeMismatch);
        }
        self.coding_maximum
            .validate()
            .map_err(|_| E::FinancialRisk)?;
        Ok(())
    }
}
pub(super) fn combined(a: CostBreakdown, b: CostBreakdown) -> Result<CostBreakdown, E> {
    let sum = CostBreakdown {
        ai_api: a
            .ai_api
            .checked_add(b.ai_api)
            .map_err(|_| E::FinancialRisk)?,
        compute: a
            .compute
            .checked_add(b.compute)
            .map_err(|_| E::FinancialRisk)?,
        storage: a
            .storage
            .checked_add(b.storage)
            .map_err(|_| E::FinancialRisk)?,
        transfer: a
            .transfer
            .checked_add(b.transfer)
            .map_err(|_| E::FinancialRisk)?,
    };
    sum.total().map_err(|_| E::FinancialRisk)?;
    Ok(sum)
}
impl FargateExecution {
    /// Trusted local composition; registration/configuration is not activation.
    pub async fn open_verifier(
        b: &ControllerBootstrap,
        p: Arc<dyn SchedulerPolicy>,
        co: SyntheticCoordinator,
        configuration: VerifierConfiguration,
    ) -> Result<Self, E> {
        let mut a = Self::open(b, p).await?;
        a.configure_verifier(co, configuration)?;
        Ok(a)
    }
    fn configure_verifier(
        &mut self,
        co: SyntheticCoordinator,
        configuration: VerifierConfiguration,
    ) -> Result<(), E> {
        if co.coding_installation().map_err(|_| E::KeyUnavailable)?
            != self.identity.guard.controller().installation
        {
            return Err(E::ScopeMismatch);
        }
        self.coordinator = Some(Arc::new(co));
        self.verifier_configuration = Some(configuration);
        self.purpose = "verifier";
        Ok(())
    }
    pub(super) async fn verifier_proposal(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
        a: &AccountRecord,
        now: i64,
    ) -> Result<(Option<VerifierLaunch>, Option<Zeroizing<Vec<u8>>>), E> {
        if self.purpose == "coding" {
            return Ok((None, None));
        }
        let co = self
            .coordinator
            .as_ref()
            .ok_or(E::UnverifiedControllerAuthority)?;
        let config = self
            .verifier_configuration
            .clone()
            .ok_or(E::IsolationMismatch)?;
        let (work, mut grant) = co
            .fargate_verifier_proposal_in(c, l, now)
            .await
            .map_err(|_| E::Revoked)?;
        config.validate(&work.context.verification, &a.workload)?;
        // Ring uses its OS-backed SystemRandom; no injectable production randomness.
        let key = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
            .map_err(|_| E::KeyUnavailable)?;
        let pkcs = Zeroizing::new(key.as_ref().to_vec());
        let pair = Ed25519KeyPair::from_pkcs8(&pkcs).map_err(|_| E::KeyUnavailable)?;
        grant.public_key = pair.public_key().as_ref().to_vec();
        grant.image = coding::commitment(&config.executables).map_err(|_| E::Integrity)?;
        grant.expires_at = grant.expires_at.min(a.valid_until);
        let coding = self.coding_request_in(c, l).await?;
        let coding_maximum = coding.exposure.upper_bound(&coding.workload, now)?;
        if a.workload.task_role == coding.workload.task_role
            || a.workload.execution_role == coding.workload.execution_role
            || a.workload.task_definition == coding.workload.task_definition
            || a.workload.image == coding.workload.image
        {
            return Err(E::IsolationMismatch);
        }
        Ok((
            Some(VerifierLaunch {
                grant,
                coding_request: Digest::new(coding.digest()?).map_err(|_| E::Integrity)?,
                coding_maximum,
                configuration: config,
            }),
            Some(pkcs),
        ))
    }
    async fn coding_request_in(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
    ) -> Result<LaunchRequest, E> {
        let row=sqlx::query("SELECT record,record_mac FROM aws_launch_requests WHERE project_id=? AND attempt_id=? AND purpose='coding'")
            .bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).fetch_optional(c).await?.ok_or(E::UnverifiedControllerAuthority)?;
        let text: String = row.try_get(0)?;
        let tag = super::super::consent::mac(
            &self.identity.guard.key,
            b"launch-request",
            &[text.as_bytes()],
        );
        if !super::super::super::equal(&tag, &row.try_get::<Vec<u8>, _>(1)?) {
            return Err(E::Integrity);
        }
        let r: LaunchRequest = serde_json::from_str(&text).map_err(|_| E::Integrity)?;
        if r.verifier.is_some() || r.lease() != *l {
            return Err(E::ScopeMismatch);
        }
        Ok(r)
    }
    pub(super) async fn verifier_authority(
        &self,
        c: &mut SqliteConnection,
        r: &LaunchRequest,
        cmd: &LaunchCommand,
        now: i64,
    ) -> Result<(), E> {
        if r.purpose() != self.purpose {
            return Err(E::ScopeMismatch);
        }
        let Some(v) = &r.verifier else {
            return Ok(());
        };
        let co = self
            .coordinator
            .as_ref()
            .ok_or(E::UnverifiedControllerAuthority)?;
        let (work, expected) = co
            .fargate_verifier_proposal_in(c, &r.lease(), now)
            .await
            .map_err(|_| E::Revoked)?;
        let g = &v.grant;
        if g.work != expected.work
            || g.artifact != expected.artifact
            || g.resulting_tree != expected.resulting_tree
            || g.policy != expected.policy
            || g.execution != expected.execution
            || g.binding != *cmd.binding()
            || g.expires_at > work.expires_at
        {
            return Err(E::ScopeMismatch);
        }
        v.configuration
            .validate(&work.context.verification, &r.workload)?;
        let coding = self.coding_request_in(c, &r.lease()).await?;
        if coding.digest()? != v.coding_request.as_str()
            || coding.exposure.upper_bound(&coding.workload, now)? != v.coding_maximum
        {
            return Err(E::FinancialRisk);
        }
        // The held original reservation covers BOTH executions. No extra ledger,
        // attempt, budget increase or reuse of the same capacity for another verifier.
        let sum = combined(r.exposure.upper_bound(&r.workload, now)?, v.coding_maximum)?;
        for (a, b) in [sum.ai_api, sum.compute, sum.storage, sum.transfer]
            .into_iter()
            .zip([
                cmd.reserved().ai_api,
                cmd.reserved().compute,
                cmd.reserved().storage,
                cmd.reserved().transfer,
            ])
        {
            if a.currency() != b.currency() || a.minor_units() > b.minor_units() {
                return Err(E::FinancialRisk);
            }
        }
        Ok(())
    }
    pub(super) async fn prepare_verifier_secret(
        &self,
        c: &mut SqliteConnection,
        r: &LaunchRequest,
        pkcs: Option<&[u8]>,
    ) -> Result<(), E> {
        if r.verifier.is_none() {
            return Ok(());
        }
        let sealed = super::super::protected::seal_verifier(
            &self.identity.guard.key,
            &r.digest()?,
            pkcs.ok_or(E::KeyUnavailable)?,
        )?;
        one(
            sqlx::query("INSERT INTO aws_verifier_bootstrap VALUES(?,?,'verifier',?,?,?,?)")
                .bind(r.binding.task.plan.project_id.as_str())
                .bind(r.binding.attempt_id.as_str())
                .bind(sealed.dn.as_slice())
                .bind(sealed.wn.as_slice())
                .bind(sealed.wrapped)
                .bind(sealed.ciphertext)
                .execute(c)
                .await?
                .rows_affected(),
        )
    }
    /// Uses the same durable submitting/receipt/unknown path as the coding launch.
    /// There is no worker endpoint for this trusted controller-only operation.
    pub async fn launch_verifier(
        &self,
        l: &LeaseRef,
        clock: &impl Clock,
    ) -> Result<ExecutionStatus, E> {
        if self.purpose != "verifier" {
            return Err(E::ScopeMismatch);
        }
        let mut read = self.identity.guard.store.pool.begin().await?;
        let (cmd, _, _) = worker_control::command(&mut read, l)
            .await
            .map_err(worker_error)?;
        let request = self.load(&mut read, l).await?;
        self.activation(&request, clock.now().map_err(|_| E::ExpiredEvidence)?)?;
        read.commit().await?;
        let submitted = self.submit(&cmd, clock).await;
        let mut tx = self
            .identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let request = self.load(&mut tx, l).await?;
        let current = self.latest(&mut tx, &request).await?;
        if current.unknown_exposure {
            self.mark_unknown(&mut tx, &request, &current).await?;
        }
        tx.commit().await?;
        submitted?;
        self.status(l).await
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderProof {
    kind: String,
    launch: Digest,
    assessment: InspectionAssessment,
    delivery: super::super::secure::DeliveryVerification,
}
async fn provider_proof(
    c: &mut SqliteConnection,
    k: &crate::controller_keys::ControllerKey,
    r: &LaunchRequest,
    g: &VerifierExecution,
    revision: Option<u64>,
) -> Result<(ProviderProof, ExecutionStatus, String), E> {
    let row=if let Some(rev)=revision {
        sqlx::query("SELECT s.record AS proof,s.record_mac AS proof_mac,e.revision,e.kind,e.recorded_at,e.record,e.record_mac FROM aws_sdk_receipts s JOIN aws_execution_events e USING(project_id,attempt_id,purpose,revision) WHERE s.project_id=? AND s.attempt_id=? AND s.purpose='verifier' AND s.revision=?")
            .bind(g.binding.task.plan.project_id.as_str()).bind(g.binding.attempt_id.as_str()).bind(rev as i64).fetch_optional(&mut *c).await?
    }else{
        sqlx::query("SELECT s.record AS proof,s.record_mac AS proof_mac,e.revision,e.kind,e.recorded_at,e.record,e.record_mac FROM aws_sdk_receipts s JOIN aws_execution_events e USING(project_id,attempt_id,purpose,revision) WHERE s.project_id=? AND s.attempt_id=? AND s.purpose='verifier' AND json_extract(s.record,'$.kind')='verifier_authorization' ORDER BY s.revision DESC LIMIT 1")
            .bind(g.binding.task.plan.project_id.as_str()).bind(g.binding.attempt_id.as_str()).fetch_optional(&mut *c).await?
    }.ok_or(E::UnknownExecution)?;
    let text: String = row.try_get("proof")?;
    let tag = super::super::consent::mac(
        k,
        b"verifier-provider-observation",
        &[r.digest()?.as_bytes(), text.as_bytes()],
    );
    if !super::super::super::equal(&tag, &row.try_get::<Vec<u8>, _>("proof_mac")?) {
        return Err(E::Integrity);
    }
    let fact: ProviderProof = serde_json::from_str(&text).map_err(|_| E::Integrity)?;
    let s = super::store::parse_event(k, r, &row)?;
    let p = ProtectedDelivery {
        format: 1,
        execution: r.expected(TaskArn::parse(&g.subject)?)?,
        secret: r.bootstrap.secret.clone(),
        version_id: r.client_token[..32].into(),
        expires_at: r.bootstrap.expires_at,
    };
    fact.delivery.check(&p, s.observed_at)?;
    if fact.kind != "verifier_authorization"
        || fact.launch.as_str() != r.digest()?
        || s.kind != "inspection"
        || s.task.as_ref().map(|t| t.as_str()) != Some(g.subject.as_str())
        || s.lifecycle != EcsLifecycle::Running
        || s.unknown_exposure
        || s.stop_requested
        || fact.assessment.task != p.execution.task
        || !fact.assessment.configuration_correlated
        || fact.assessment.lifecycle != EcsLifecycle::Running
        || fact.assessment.observed_at > s.observed_at
    {
        return Err(E::Integrity);
    }
    Ok((fact, s, text))
}
/// Re-authenticate the same journal for request/acceptance, and authenticate
/// historical grants without reviving retired current execution authority.
pub(crate) async fn grant_in(
    c: &mut SqliteConnection,
    k: &crate::controller_keys::ControllerKey,
    g: &VerifierExecution,
    now: i64,
    historical: bool,
) -> Result<(), E> {
    // Offline SDK/IAM fixture facts cannot activate production, even after restore.
    #[cfg(not(test))]
    {
        let _ = (c, k, g, now, historical);
        Err(E::LiveLaunchDisabled)
    }
    #[cfg(test)]
    {
        if g.authority != "private-fargate-verifier-v1" {
            return Err(E::LiveLaunchDisabled);
        }
        let row=sqlx::query("SELECT record,record_mac FROM aws_launch_requests WHERE project_id=? AND attempt_id=? AND purpose='verifier'").bind(g.binding.task.plan.project_id.as_str()).bind(g.binding.attempt_id.as_str()).fetch_optional(&mut *c).await?.ok_or(E::UnknownExecution)?;
        let text: String = row.try_get(0)?;
        let tag = super::super::consent::mac(k, b"launch-request", &[text.as_bytes()]);
        if !super::super::super::equal(&tag, &row.try_get::<Vec<u8>, _>(1)?) {
            return Err(E::Integrity);
        }
        let r: LaunchRequest = serde_json::from_str(&text).map_err(|_| E::Integrity)?;
        let v = r.verifier.as_ref().ok_or(E::ScopeMismatch)?;
        let proof = g.fargate.as_ref().ok_or(E::Integrity)?;
        let mut expected = v.grant.clone();
        expected.subject = g.subject.clone();
        expected.authority = g.authority.clone();
        expected.fargate = g.fargate.clone();
        if expected != *g || proof.launch.as_str() != r.digest()? {
            return Err(E::ScopeMismatch);
        }
        let (_, origin, text) = provider_proof(c, k, &r, g, Some(proof.revision)).await?;
        if coding::commitment(&text).map_err(|_| E::Integrity)? != proof.commitment
            || origin.observed_at < g.issued_at
            || origin.observed_at >= g.expires_at
        {
            return Err(E::Integrity);
        }
        if !historical {
            let (fact, s, _) = provider_proof(c, k, &r, g, None).await?;
            let p = ProtectedDelivery {
                format: 1,
                execution: r.expected(TaskArn::parse(&g.subject)?)?,
                secret: r.bootstrap.secret.clone(),
                version_id: r.client_token[..32].into(),
                expires_at: r.bootstrap.expires_at,
            };
            fact.delivery.check(&p, now)?;
            let account=sqlx::query("SELECT v.record,v.record_mac,h.current_version FROM aws_infrastructure_accounts h JOIN aws_infrastructure_account_versions v ON v.project_id=h.project_id AND v.account_ref=h.account_ref AND v.version=h.current_version WHERE h.project_id=? AND h.account_ref=?")
                .bind(g.binding.task.plan.project_id.as_str()).bind(r.account_ref.as_str()).fetch_optional(&mut *c).await?.ok_or(E::Revoked)?;
            let text: String = account.try_get(0)?;
            let version = account.try_get::<i64, _>(2)?;
            let tag = k.worker_record_mac(
                b"aws-account-record",
                &[
                    g.binding.task.plan.project_id.as_str().as_bytes(),
                    r.account_ref.as_str().as_bytes(),
                    &(version as u64).to_be_bytes(),
                    text.as_bytes(),
                ],
            );
            if version < 1 || !super::super::super::equal(&tag, &account.try_get::<Vec<u8>, _>(1)?)
            {
                return Err(E::Integrity);
            }
            let a: AccountRecord = serde_json::from_str(&text).map_err(|_| E::Integrity)?;
            if a.installation != g.installation
                || a.project != g.binding.task.plan.project_id
                || a.account_ref != r.account_ref
                || a.version != r.account_version
                || a.workload != r.workload
                || a.revoked
                || now >= a.valid_until
            {
                return Err(E::Revoked);
            }
            let scope = super::super::store::credential_scope(&a.project, &a.credential)?;
            crate::controller_keys::secret_broker::current_reference_in(
                c,
                k,
                &scope,
                crate::controller_keys::secret_broker::SecretVersion::new(
                    a.credential.version as u64,
                )
                .map_err(|_| E::Integrity)?,
            )
            .await
            .map_err(|_| E::Revoked)?;
            // Real registrations additionally retain the exact current Owner consent.
            if a.origin_is_reviewed() {
                let consent =
                    super::super::consent::current(c, k, &a.project, &a.account_ref, now).await?;
                if consent.enrollment.workload != r.workload
                    || consent.enrollment.expected_version + 1 != r.account_version
                {
                    return Err(E::Revoked);
                }
            }
            if now < s.observed_at
                || now - s.observed_at >= 15
                || now < g.issued_at
                || now >= g.expires_at
            {
                return Err(E::ExpiredEvidence);
            }
            let latest=sqlx::query("SELECT revision,kind,recorded_at,record,record_mac FROM aws_execution_events WHERE project_id=? AND attempt_id=? AND purpose='verifier' ORDER BY revision DESC LIMIT 1").bind(g.binding.task.plan.project_id.as_str()).bind(g.binding.attempt_id.as_str()).fetch_one(&mut *c).await?;
            let latest = super::store::parse_event(k, &r, &latest)?;
            if latest.revision < s.revision
                || latest.task != s.task
                || latest.lifecycle != EcsLifecycle::Running
                || latest.stop_requested
                || latest.unknown_exposure
            {
                return Err(E::Revoked);
            }
        }
        Ok(())
    }
}

impl FargateExecution {
    async fn verifier_plan_in(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
    ) -> Result<(LaunchRequest, ProtectedDelivery), E> {
        if self.purpose != "verifier" {
            return Err(E::ScopeMismatch);
        }
        let r = self.load(c, l).await?;
        let s = self.latest(c, &r).await?;
        if s.unknown_exposure || s.stop_requested {
            return Err(E::UnknownExecution);
        }
        let task = s.task.ok_or(E::UnknownExecution)?;
        let p = ProtectedDelivery {
            format: 1,
            execution: r.expected(task)?,
            secret: r.bootstrap.secret.clone(),
            version_id: r.client_token[..32].to_owned(),
            expires_at: r.bootstrap.expires_at,
        };
        p.validate()?;
        Ok((r, p))
    }
    /// Current independent task inspection precedes protected credential delivery.
    /// Missing effective IAM/network/isolation evidence fails before any publish.
    pub async fn authorize_verifier(&self, l: &LeaseRef) -> Result<VerifierExecution, E> {
        let mut read = self.identity.guard.store.pool.begin().await?;
        let (r, p) = self.verifier_plan_in(&mut read, l).await?;
        read.commit().await?;
        let started = self.identity.now()?;
        let fact = self.distribution.verify(&p, started).await?;
        fact.check(&p, started)?;
        let sdk = self
            .client(
                &r,
                super::super::credential_use::InfrastructureOperation::InspectExecution,
            )
            .await?;
        let assessment = sdk.inspect_verifier(&r, &p.execution.task, started).await?;
        let mut tx = self
            .identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let now = self.identity.now()?;
        fact.check(&p, now)?;
        self.distribution.current(&p, now)?;
        let (cmd, _, _) = worker_control::command(&mut tx, l)
            .await
            .map_err(worker_error)?;
        self.authority(&mut tx, &r, &cmd, now).await?;
        let (_, current) = self.verifier_plan_in(&mut tx, l).await?;
        if current != p || now < started {
            return Err(E::Conflict);
        }
        // The persisted inspection event is the existing lifecycle authority,
        // not another result or acceptance journal.
        let observed = self
            .event(
                &mut tx,
                &r,
                "inspection",
                Some(p.execution.task.clone()),
                EcsLifecycle::Running,
                false,
                false,
                false,
                now,
            )
            .await?;
        let provider = ProviderProof {
            kind: "verifier_authorization".into(),
            launch: Digest::new(r.digest()?).map_err(|_| E::Integrity)?,
            assessment,
            delivery: fact.clone(),
        };
        let proof_text = encode(&provider)?;
        let proof_mac = super::super::consent::mac(
            &self.identity.guard.key,
            b"verifier-provider-observation",
            &[r.digest()?.as_bytes(), proof_text.as_bytes()],
        );
        one(
            sqlx::query("INSERT INTO aws_sdk_receipts VALUES(?,?,'verifier',?,?,?)")
                .bind(l.task.plan.project_id.as_str())
                .bind(l.attempt_id.as_str())
                .bind(observed.revision as i64)
                .bind(&proof_text)
                .bind(proof_mac.as_slice())
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        let secret=sqlx::query("SELECT data_nonce,wrap_nonce,wrapped,ciphertext FROM aws_verifier_bootstrap WHERE project_id=? AND attempt_id=?")
            .bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).fetch_one(&mut *tx).await?;
        let envelope = super::super::protected::Envelope {
            dn: secret
                .try_get::<Vec<u8>, _>(0)?
                .try_into()
                .map_err(|_| E::Integrity)?,
            wn: secret
                .try_get::<Vec<u8>, _>(1)?
                .try_into()
                .map_err(|_| E::Integrity)?,
            wrapped: secret.try_get(2)?,
            ciphertext: secret.try_get(3)?,
        };
        let pkcs = super::super::protected::open_verifier(
            &self.identity.guard.key,
            &r.digest()?,
            &envelope,
        )?;
        let key = Ed25519KeyPair::from_pkcs8(&pkcs).map_err(|_| E::Integrity)?;
        let mut g = r.verifier.as_ref().ok_or(E::ScopeMismatch)?.grant.clone();
        if key.public_key().as_ref() != g.public_key {
            return Err(E::Integrity);
        }
        g.subject = p.execution.task.as_str().to_owned();
        // First committed observation fixes the protected payload even if
        // delivery succeeded but the final grant insert/HTTP acknowledgement failed.
        let origin:i64=sqlx::query_scalar("SELECT min(revision) FROM aws_sdk_receipts WHERE project_id=? AND attempt_id=? AND purpose='verifier' AND json_extract(record,'$.kind')='verifier_authorization'")
            .bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).fetch_one(&mut *tx).await?;
        let (_, origin_event, origin_text) = provider_proof(
            &mut tx,
            &self.identity.guard.key,
            &r,
            &g,
            Some(origin as u64),
        )
        .await?;
        g.fargate = Some(FargateProofReference {
            launch: Digest::new(r.digest()?).map_err(|_| E::Integrity)?,
            revision: origin_event.revision,
            commitment: coding::commitment(&origin_text).map_err(|_| E::Integrity)?,
        });
        #[cfg(test)]
        if self.fixture.is_some() {
            g.authority = "private-fargate-verifier-v1".into();
        }
        // Activation stays sealed. Even a restored fixture record is rejected by
        // the production grant validator. No raw signing key is persisted here.
        let payload = VerifierCredential::payload(&g, &r.digest()?, &pkcs)?;
        tx.commit().await?;
        self.distribution.publish_verifier(&p, payload).await?;
        let mut tx = self
            .identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let end = self.identity.now()?;
        fact.check(&p, end)?;
        self.distribution.current(&p, end)?;
        self.authority(&mut tx, &r, &cmd, end).await?;
        self.coordinator
            .as_ref()
            .ok_or(E::UnverifiedControllerAuthority)?
            .install_fargate_verifier_in(&mut tx, &g, end)
            .await
            .map_err(|_| E::Revoked)?;
        self.checkpoint(Checkpoint::BeforeGrant)?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterGrant)?;
        Ok(g)
    }
    pub async fn verifier_delivery(&self, l: &LeaseRef) -> Result<ProtectedDelivery, E> {
        let mut tx = self.identity.guard.store.pool.begin().await?;
        let (_, p) = self.verifier_plan_in(&mut tx, l).await?;
        tx.commit().await?;
        Ok(p)
    }
}
/// Opaque protected delivery; no Clone, serialization, Debug contents or key export.
/// Possession authenticates only the execution already enrolled by the controller.
/// ```compile_fail
/// use rolvya_persistence_sqlite::controller_keys::worker_auth::aws_fargate::execution::verifier::VerifierCredential;
/// fn forge(grant: rolvya_persistence_sqlite::orchestration::coding::execution::VerifierExecution) {
///     let credential = VerifierCredential { grant, pkcs: Default::default() };
/// }
/// ```
pub struct VerifierCredential {
    grant: VerifierExecution,
    pkcs: Zeroizing<Vec<u8>>,
}
impl std::fmt::Debug for VerifierCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VerifierCredential [redacted]")
    }
}
impl VerifierCredential {
    fn payload(g: &VerifierExecution, launch: &str, key: &[u8]) -> Result<Zeroizing<String>, E> {
        use base64::Engine;
        // JSON credential bytes remain only in a zeroizing delivery buffer.
        let mut s = Zeroizing::new(format!("{{\"format\":1,\"launch\":\"{launch}\",\"grant\":"));
        s.push_str(&serde_json::to_string(g).map_err(|_| E::Integrity)?);
        s.push_str(",\"pkcs8\":\"");
        let encoded = Zeroizing::new(base64::engine::general_purpose::STANDARD.encode(key));
        s.push_str(&encoded);
        s.push_str("\"}");
        Ok(s)
    }
    pub(super) fn parse(p: &ProtectedDelivery, body: &[u8], now: i64) -> Result<Self, E> {
        use base64::Engine;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Payload<'a> {
            format: u16,
            launch: &'a str,
            grant: VerifierExecution,
            pkcs8: &'a str,
        }
        if body.len() > 16384 || now >= p.expires_at {
            return Err(E::ExpiredEvidence);
        }
        let data: Payload<'_> = serde_json::from_slice(body).map_err(|_| E::Integrity)?;
        if data.format != 1
            || data.launch.len() != 64
            || data
                .grant
                .fargate
                .as_ref()
                .is_none_or(|v| v.launch.as_str() != data.launch)
            || data.grant.subject != p.execution.task.as_str()
            || data.grant.binding != p.execution.binding
            || data.grant.installation != p.execution.installation
            || now < data.grant.issued_at
            || now >= data.grant.expires_at
        {
            return Err(E::ScopeMismatch);
        }
        let pkcs = Zeroizing::new(
            base64::engine::general_purpose::STANDARD
                .decode(data.pkcs8)
                .map_err(|_| E::Integrity)?,
        );
        if Ed25519KeyPair::from_pkcs8(&pkcs)
            .map_err(|_| E::Integrity)?
            .public_key()
            .as_ref()
            != data.grant.public_key
        {
            return Err(E::Integrity);
        }
        Ok(Self {
            grant: data.grant,
            pkcs,
        })
    }
    pub fn into_client(
        self,
        endpoint: crate::controller_keys::worker_auth::https::client::ControllerEndpoint,
    ) -> Result<
        crate::controller_keys::worker_auth::https::verifier::VerifierHttpClient,
        crate::controller_keys::worker_auth::https::client::ClientError,
    > {
        crate::controller_keys::worker_auth::https::verifier::VerifierHttpClient::new(
            endpoint,
            self.grant.execution,
            self.pkcs,
        )
    }
}

#[cfg(test)]
impl VerifierCredential {
    pub(crate) fn fixture_parse(p: &ProtectedDelivery, body: &[u8], now: i64) -> Result<Self, E> {
        Self::parse(p, body, now)
    }
    pub(crate) fn fixture_tls(
        self,
        endpoint: crate::controller_keys::worker_auth::https::client::ControllerEndpoint,
        tls: Arc<rustls::ClientConfig>,
    ) -> Result<
        crate::controller_keys::worker_auth::https::verifier::VerifierHttpClient,
        crate::controller_keys::worker_auth::https::client::ClientError,
    > {
        crate::controller_keys::worker_auth::https::verifier::VerifierHttpClient::with_tls(
            endpoint,
            self.grant.execution,
            self.pkcs,
            tls,
        )
    }
}
#[cfg(test)]
pub(crate) mod fixture {
    use super::*;
    pub(crate) struct Execution {
        pub adapter: FargateExecution,
        pub expected: ExpectedExecution,
        world: Arc<super::super::fixture::World>,
        delivery: Option<Arc<super::super::super::secure_fixture::World>>,
    }
    pub(crate) async fn open(
        b: &ControllerBootstrap,
        policy: Arc<dyn SchedulerPolicy>,
        clock: Arc<dyn Clock>,
        scope: (rolvya_domain::worker::WorkerBinding, EvidenceRef),
        root: &std::path::Path,
        label: &str,
        workload: Option<FargateWorkload>,
    ) -> Execution {
        let (binding, account) = scope;
        let mut e = super::super::super::store::fixture::expected("task-a");
        e.installation = b.load_existing().await.unwrap().coding_installation();
        e.binding = binding;
        e.account_ref = account;
        let mut tx = b.store.pool.begin().await.unwrap();
        let (cmd, _, _) = worker_control::command(&mut tx, &e.lease()).await.unwrap();
        tx.commit().await.unwrap();
        e.command_digest = Digest::new(cmd.digest().unwrap()).unwrap();
        let now = clock.now().unwrap();
        e.authorized_at = now;
        e.authorization_until = now + 240;
        if let Some(w) = workload {
            e.workload = w;
        }
        // Unique immutable AWS fixture resources for each attempt and purpose.
        let suffix = &coding::commitment(&(label, &e.binding)).unwrap().as_str()[..32].to_owned();
        e.task = TaskArn::parse(&format!(
            "arn:aws:ecs:eu-central-1:123456789012:task/rolvya/{suffix}"
        ))
        .unwrap();
        e.operation_token = e.expected_operation_token().unwrap();
        let http = super::super::super::store::fixture::AwsFixture::new(&e);
        let (identity, _) =
            super::super::super::store::fixture::open(b, policy, clock, &e, http.http()).await;
        identity
            .enroll_account(&super::super::super::store::fixture::enrollment(&e))
            .await
            .unwrap();
        let world = super::super::fixture::World::new(
            &root.join(format!("{label}-aws.sqlite")),
            vec![e.clone()],
        )
        .await;
        let adapter = super::super::fixture::open(identity, world.clone(), true).await;
        Execution {
            adapter,
            expected: e,
            world,
            delivery: None,
        }
    }
    impl Execution {
        pub(crate) fn configure(
            &mut self,
            co: SyntheticCoordinator,
            configuration: VerifierConfiguration,
        ) {
            self.adapter.configure_verifier(co, configuration).unwrap();
        }
        pub(crate) fn allocation(&self, now: i64) -> BootstrapAllocation {
            let suffix = &coding::commitment(&self.expected).unwrap().as_str()[..24].to_owned();
            BootstrapAllocation {
                secret: SecretArn::parse(&format!(
                    "arn:aws:secretsmanager:eu-central-1:123456789012:secret:rolvya/{suffix}-AbCdEf"
                ))
                .unwrap(),
                task_role: self.expected.workload.task_role.clone(),
                expires_at: now + 90,
            }
        }
        pub(crate) async fn distribution(&mut self, l: &LeaseRef) {
            let p = self.adapter.verifier_delivery(l).await.unwrap();
            let d = super::super::super::secure_fixture::World::new(&p);
            self.adapter.distribution = d.clone();
            self.delivery = Some(d);
        }
        pub(crate) async fn credential(
            &self,
            l: &LeaseRef,
            now: i64,
            other_role: bool,
        ) -> Result<VerifierCredential, E> {
            let p = self.adapter.verifier_delivery(l).await?;
            let role = if other_role {
                &p.execution.workload.execution_role
            } else {
                &p.execution.workload.task_role
            };
            let raw = self
                .delivery
                .as_ref()
                .unwrap()
                .verifier_as_role(role, &p, now)
                .await?;
            VerifierCredential::parse(&p, raw.as_bytes(), now)
        }
        pub(crate) fn kill_at(&mut self, label: &str) {
            super::super::fixture::kill_at(&mut self.adapter, label);
        }
        pub(crate) async fn request(&self, l: &LeaseRef) -> LaunchRequest {
            let mut tx = self
                .adapter
                .identity
                .guard
                .store
                .pool
                .begin()
                .await
                .unwrap();
            self.adapter.load(&mut tx, l).await.unwrap()
        }
        pub(crate) async fn revoke_account(&self) {
            let mut r = super::super::super::store::fixture::enrollment(&self.expected);
            r.request_id = RequestId::new("revoke-verifier-account").unwrap();
            r.expected_version = 1;
            r.revoked = true;
            self.adapter.identity.enroll_account(&r).await.unwrap();
        }
        pub(crate) async fn task_payload(
            &self,
            l: &LeaseRef,
            now: i64,
        ) -> (ProtectedDelivery, Zeroizing<String>) {
            let p = self.adapter.verifier_delivery(l).await.unwrap();
            let raw = self
                .delivery
                .as_ref()
                .unwrap()
                .verifier_as_role(&p.execution.workload.task_role, &p, now)
                .await
                .unwrap();
            (p, raw)
        }
        pub(crate) fn revoke_delivery(&self) {
            self.delivery
                .as_ref()
                .unwrap()
                .revoked
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        pub(crate) fn shared_role(&self) {
            self.delivery
                .as_ref()
                .unwrap()
                .exclusive
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
        pub(crate) async fn effects(&self) -> i64 {
            self.world.count().await
        }
        pub(crate) fn pause_inspection(&self) -> Arc<(tokio::sync::Notify, tokio::sync::Notify)> {
            let pause = Arc::new((tokio::sync::Notify::new(), tokio::sync::Notify::new()));
            *self.world.pause_definition.lock().unwrap() = Some(pause.clone());
            pause
        }
        pub(crate) fn stopped(&self) {
            *self.world.status.lock().unwrap() = "STOPPED".into();
        }
        pub(crate) fn behavior(&self, n: u8) {
            self.world
                .behavior
                .store(n, std::sync::atomic::Ordering::SeqCst);
        }
    }
}
