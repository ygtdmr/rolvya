//! Exact current operation composition, before the existing submitting marker.
use super::*;
use crate::controller_keys::qualification::{
    self as q, OperationClass, QualificationScope, QualificationStore,
};
impl FargateExecution {
    pub(in crate::controller_keys::worker_auth) async fn qualification_in(
        &self,
        c: &mut SqliteConnection,
        r: &LaunchRequest,
        cmd: &LaunchCommand,
        credential_version: i64,
        now: i64,
    ) -> Result<(), E> {
        #[cfg(test)]
        if self.fixture.is_some() && self.qualification.is_none() {
            return Ok(());
        }
        let scope = self
            .qualification_scope_in(c, r, cmd, credential_version)
            .await?;
        #[cfg(test)]
        if let Some(q) = &self.qualification {
            return q
                .require_operation_in(c, &scope, now)
                .await
                .map_err(|_| E::QualificationRequired);
        }
        QualificationStore::borrowed(self.identity.guard.store.clone(), &self.identity.guard.key)
            .require_operation_in(c, &scope, now)
            .await
            .map_err(|_| E::QualificationRequired)
    }
    async fn qualification_scope_in(
        &self,
        c: &mut SqliteConnection,
        r: &LaunchRequest,
        cmd: &LaunchCommand,
        credential_version: i64,
    ) -> Result<QualificationScope, E> {
        let model = crate::controller_keys::provider_accounts::coding_model_binding(
            c,
            &self.identity.guard.key,
            &r.binding.task.plan.project_id,
            &cmd.assignment().connection_ref,
        )
        .await
        .map_err(|_| E::Revoked)?;
        let budget = crate::finance::snapshot(c, &r.binding.task.plan.project_id)
            .await
            .map_err(|_| E::FinancialRisk)?;
        let scope = QualificationScope {
            installation: r.installation.clone(),
            project: r.binding.task.plan.project_id.clone(),
            infrastructure_account: r.account_ref.as_str().into(),
            partition: r.workload.cluster.partition(),
            region: r.workload.cluster.region().into(),
            aws_account: r.workload.cluster.account(),
            task_definition: r.workload.task_definition.clone(),
            image_digest: r.workload.image_digest.clone(),
            infrastructure_version: i64::try_from(r.account_version).map_err(|_| E::Integrity)?,
            infrastructure_credential_version: credential_version,
            configuration_sha256: "0".repeat(64),
            workload_sha256: q::workload_commitment(&r.workload).map_err(|_| E::Integrity)?,
            execution_sha256: q::execution_commitment(&r.binding, &r.command_digest, cmd.profile())
                .map_err(|_| E::Integrity)?,
            exposure_sha256: q::launch_exposure_commitment(
                &r.exposure,
                &r.maximum,
                r.financial_event,
                &r.bootstrap,
            )
            .map_err(|_| E::Integrity)?,
            connection: model.connection,
            connection_version: model.connection_version,
            grant_version: model.grant_version,
            credential_version: model.credential_version,
            financial_revision: budget.version,
            operation: if r.verifier.is_some() {
                OperationClass::VerifierLaunch
            } else {
                OperationClass::CodingLaunch
            },
            operation_id: r.client_token.clone(),
            attempt: r.binding.attempt_id.as_str().into(),
            epoch: r.binding.epoch,
        };
        Ok(scope)
    }
}

pub(in crate::controller_keys) async fn require_model_operation_in(
    c: &mut SqliteConnection,
    store: crate::PlanStore,
    key: &crate::controller_keys::ControllerKey,
    w: &crate::orchestration::coding::work::CodingWork,
    operation: &rolvya_domain::worker::Digest,
    now: i64,
    #[cfg(test)] simulation: Option<&FixtureModelAuthority>,
) -> Result<(), E> {
    let row=sqlx::query("SELECT record,record_mac,request_digest FROM aws_launch_requests WHERE project_id=? AND attempt_id=? AND purpose='coding'").bind(w.binding.task.plan.project_id.as_str()).bind(w.binding.attempt_id.as_str()).fetch_optional(&mut *c).await?.ok_or(E::QualificationRequired)?;
    let text: &str = row.try_get("record")?;
    if text.len() > 32768
        || !super::super::super::equal(
            row.try_get::<&[u8], _>("record_mac")?,
            &super::super::consent::mac(key, b"launch-request", &[text.as_bytes()]),
        )
    {
        return Err(E::Integrity);
    }
    let r: LaunchRequest = serde_json::from_str(text).map_err(|_| E::Integrity)?;
    if r.binding != w.binding
        || r.installation != key.coding_installation()
        || r.digest()? != row.try_get::<&str, _>("request_digest")?
        || now >= r.authorization_until
    {
        return Err(E::Revoked);
    }
    let credential_version = {
        #[cfg(test)]
        if let Some(f) = simulation {
            if f.expected_work != w.identity || !f.enabled.load(std::sync::atomic::Ordering::SeqCst)
            {
                return Err(E::Revoked);
            }
            let account = f
                .identity
                .account(c, &w.binding.task.plan.project_id, &r.account_ref)
                .await?;
            if account.version != r.account_version
                || account.workload != r.workload
                || account.revoked
                || now >= account.valid_until
            {
                return Err(E::Revoked);
            }
            account.credential.version
        } else {
            current_model_infrastructure(c, key, w, &r, now).await?
        }
        #[cfg(not(test))]
        {
            current_model_infrastructure(c, key, w, &r, now).await?
        }
    };
    let m = crate::controller_keys::provider_accounts::coding_model_binding(
        c,
        key,
        &w.binding.task.plan.project_id,
        &w.connection,
    )
    .await
    .map_err(|_| E::Revoked)?;
    if w.model_account.as_ref() != Some(&m) {
        return Err(E::Revoked);
    }
    let budget = crate::finance::snapshot(c, &w.binding.task.plan.project_id)
        .await
        .map_err(|_| E::FinancialRisk)?;
    let scope = QualificationScope {
        installation: r.installation.clone(),
        project: w.binding.task.plan.project_id.clone(),
        infrastructure_account: r.account_ref.as_str().into(),
        partition: r.workload.cluster.partition(),
        region: r.workload.cluster.region().into(),
        aws_account: r.workload.cluster.account(),
        task_definition: r.workload.task_definition.clone(),
        image_digest: r.workload.image_digest.clone(),
        infrastructure_version: i64::try_from(r.account_version).map_err(|_| E::Integrity)?,
        infrastructure_credential_version: credential_version,
        configuration_sha256: "0".repeat(64),
        workload_sha256: q::workload_commitment(&r.workload).map_err(|_| E::Integrity)?,
        execution_sha256: q::model_execution_commitment(w, operation).map_err(|_| E::Integrity)?,
        exposure_sha256: q::launch_exposure_commitment(
            &r.exposure,
            &r.maximum,
            r.financial_event,
            &r.bootstrap,
        )
        .map_err(|_| E::Integrity)?,
        connection: m.connection,
        connection_version: m.connection_version,
        grant_version: m.grant_version,
        credential_version: m.credential_version,
        financial_revision: budget.version,
        operation: OperationClass::ModelCall,
        operation_id: operation.as_str().into(),
        attempt: w.binding.attempt_id.as_str().into(),
        epoch: w.binding.epoch,
    };
    #[cfg(test)]
    if let Some(f) = simulation {
        return f
            .authority
            .qualify_and_require_in(c, &scope, now)
            .await
            .map_err(|_| E::QualificationRequired);
    }
    QualificationStore::borrowed(store, key)
        .require_operation_in(c, &scope, now)
        .await
        .map_err(|_| E::QualificationRequired)
}

#[cfg(test)]
impl FargateExecution {
    pub(in crate::controller_keys) async fn fixture_install_qualification(
        &mut self,
        b: &ControllerBootstrap,
        l: &LeaseRef,
        qualified: bool,
    ) -> Result<QualificationScope, E> {
        let mut c = self.identity.guard.store.pool.begin().await?;
        let r = self.load(&mut c, l).await?;
        let (cmd, _, _) = worker_control::command(&mut c, l)
            .await
            .map_err(worker_error)?;
        let account = self
            .identity
            .account(&mut c, &l.task.plan.project_id, &r.account_ref)
            .await?;
        let mut scope = self
            .qualification_scope_in(&mut c, &r, &cmd, account.credential.version)
            .await?;
        scope.configuration_sha256 =
            q::workload_commitment(&r.workload).map_err(|_| E::Integrity)?;
        c.commit().await?;
        self.qualification = Some(if qualified {
            QualificationStore::fixture_simulation(b, self.identity.guard.clock.clone(), &scope)
                .await
                .map_err(|_| E::Integrity)?
        } else {
            QualificationStore::open(b)
                .await
                .map_err(|_| E::Integrity)?
        });
        Ok(scope)
    }
}

async fn current_model_infrastructure(
    c: &mut SqliteConnection,
    key: &crate::controller_keys::ControllerKey,
    w: &crate::orchestration::coding::work::CodingWork,
    r: &LaunchRequest,
    now: i64,
) -> Result<i64, E> {
    let consent = super::super::consent::current(
        c,
        key,
        &w.binding.task.plan.project_id,
        &r.account_ref,
        now,
    )
    .await?;
    if consent.enrollment.expected_version + 1 != r.account_version
        || consent.enrollment.workload != r.workload
    {
        return Err(E::Revoked);
    }
    super::super::trusted::load_check(c, key, &consent, now).await?;
    Ok(consent.enrollment.credential.version)
}
#[cfg(test)]
pub(in crate::controller_keys) struct FixtureModelAuthority {
    expected_work: rolvya_domain::worker::Digest,
    identity: std::sync::Arc<FargateIdentity>,
    authority: q::SimulationAuthority,
    enabled: std::sync::atomic::AtomicBool,
}
#[cfg(test)]
impl FixtureModelAuthority {
    pub(in crate::controller_keys) async fn new(
        b: &ControllerBootstrap,
        clock: std::sync::Arc<dyn Clock>,
        w: &crate::orchestration::coding::work::CodingWork,
        execution: &FargateExecution,
    ) -> Self {
        Self {
            expected_work: w.identity.clone(),
            identity: execution.identity.clone(),
            authority: q::SimulationAuthority::open(b, clock).await.unwrap(),
            enabled: std::sync::atomic::AtomicBool::new(true),
        }
    }
    pub(in crate::controller_keys) fn revoke(&self) {
        self.enabled
            .store(false, std::sync::atomic::Ordering::SeqCst)
    }
}
