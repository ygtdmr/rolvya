//! Host-only non-billable inspection. No AWS/model transport is invoked here.
use super::*;
use crate::{RequestId, controller_keys::qualification::*};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationInspection {
    pub scope: QualificationScope,
    pub request: RequestId,
    pub configuration: IsolationConfiguration,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationRevocation {
    pub project: rolvya_domain::ids::ProjectId,
    pub evidence: RequestId,
    pub request: RequestId,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExposureInspection {
    pub coding: crate::controller_keys::worker_auth::aws_fargate::FargateWorkload,
    pub verifier: crate::controller_keys::worker_auth::aws_fargate::FargateWorkload,
    pub calculation: ExposureCalculation,
}
impl HostOperator {
    pub async fn inspect_provider_operations(
        &self,
        query: &crate::controller_keys::worker_auth::aws_fargate::execution::ReconciliationQuery,
    ) -> Result<
        crate::controller_keys::worker_auth::aws_fargate::execution::ReconciliationPage,
        OperationError,
    > {
        self.config.validate_host()?;
        let key = self
            .bootstrap
            .load_existing()
            .await
            .map_err(|_| OperationError::Unauthorized)?;
        let mut c = self.store.pool.begin().await?;
        let report = crate::controller_keys::worker_auth::aws_fargate::execution::inspect_journal(
            &mut c,
            &key,
            &query.project,
            query.after.as_ref(),
            query.limit,
        )
        .await
        .map_err(|_| OperationError::Integrity)?;
        c.commit().await?;
        Ok(report)
    }

    pub async fn inspect_qualification(
        &self,
        r: &QualificationInspection,
    ) -> Result<QualificationReport, OperationError> {
        self.config.validate_host()?;
        if r.configuration.network.controller_origin != self.config.canonical_origin {
            return Err(OperationError::Unauthorized);
        }
        QualificationStore::open(&self.bootstrap)
            .await
            .map_err(qerror)?
            .inspect_configuration(&r.scope, &r.request, &r.configuration)
            .await
            .map_err(qerror)
    }
    pub async fn qualification_report(
        &self,
        scope: &QualificationScope,
    ) -> Result<QualificationReport, OperationError> {
        self.config.validate_host()?;
        QualificationStore::open(&self.bootstrap)
            .await
            .map_err(qerror)?
            .report(scope)
            .await
            .map_err(qerror)
    }
    pub async fn revoke_qualification(
        &self,
        r: &QualificationRevocation,
    ) -> Result<(), OperationError> {
        self.config.validate_host()?;
        QualificationStore::open(&self.bootstrap)
            .await
            .map_err(qerror)?
            .revoke(&r.project, &r.evidence, &r.request)
            .await
            .map_err(qerror)
    }
    pub async fn verify_qualification(
        &self,
        evidence: &SignedEvidence,
    ) -> Result<(), OperationError> {
        self.config.validate_host()?;
        QualificationStore::open(&self.bootstrap)
            .await
            .map_err(qerror)?
            .submit_evidence(evidence)
            .await
            .map_err(qerror)
    }
    pub fn estimate_exposure(
        &self,
        r: &ExposureInspection,
    ) -> Result<EstimatedExposure, OperationError> {
        self.config.validate_host()?;
        r.calculation
            .estimate(&r.coding, &r.verifier, now()?)
            .map_err(qerror)
    }
}
fn qerror(e: QualificationError) -> OperationError {
    match e {
        QualificationError::Storage => OperationError::Storage,
        QualificationError::Integrity => OperationError::Integrity,
        QualificationError::Conflict => OperationError::Conflict,
        QualificationError::UntrustedIssuer | QualificationError::Revoked => {
            OperationError::Unauthorized
        }
        _ => OperationError::InvalidConfiguration,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountInspection {
    pub project: rolvya_domain::ids::ProjectId,
    pub connection: crate::controller_keys::provider_accounts::ConnectionId,
    pub model: String,
    pub observation: AccountObservation,
}
impl HostOperator {
    pub async fn inspect_account_qualification(
        &self,
        r: &AccountInspection,
    ) -> Result<String, OperationError> {
        self.config.validate_host()?;
        crate::controller_keys::provider_accounts::ProviderRegistry::open(&self.bootstrap)
            .await
            .map_err(|_| OperationError::Unauthorized)?
            .inspect_qualification_declaration(&r.project, &r.connection, &r.model, &r.observation)
            .await
            .map_err(|_| OperationError::Unauthorized)
    }
}
