//! Sealed, exact scoped Broker use. No public constructor or plaintext callback.
use super::{
    AwsIdentityError as E,
    consent::{self, Consent},
};
use crate::{RequestId, controller_keys::ControllerKey, finance::EvidenceRef};
use rolvya_domain::ids::ProjectId;
use sqlx::{Row, SqliteConnection};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::controller_keys) enum InfrastructureOperation {
    VerifyController,
    InspectExecution,
    ProtectedDelivery,
    LaunchExecution,
    StopExecution,
    ReconcileExecution,
}
impl InfrastructureOperation {
    pub(in crate::controller_keys) fn name(self) -> &'static str {
        match self {
            Self::VerifyController => "verify_controller",
            Self::InspectExecution => "inspect_execution",
            Self::ProtectedDelivery => "protected_delivery",
            Self::LaunchExecution => "launch_execution",
            Self::StopExecution => "stop_execution",
            Self::ReconcileExecution => "reconcile_execution",
        }
    }
}
pub(in crate::controller_keys) struct VerifiedInfrastructureUse {
    consent: Consent,
    request: RequestId,
    operation: InfrastructureOperation,
}
impl VerifiedInfrastructureUse {
    pub(super) async fn authorize(
        c: &mut SqliteConnection,
        k: &ControllerKey,
        p: &ProjectId,
        id: &EvidenceRef,
        request: &RequestId,
        operation: InfrastructureOperation,
        now: i64,
    ) -> Result<Self, E> {
        let consent = consent::current(c, k, p, id, now).await?;
        Ok(Self {
            consent,
            request: request.clone(),
            operation,
        })
    }
    pub(in crate::controller_keys) async fn current(
        &self,
        c: &mut SqliteConnection,
        k: &ControllerKey,
        now: i64,
    ) -> Result<(), E> {
        let current = consent::current(
            c,
            k,
            &self.consent.enrollment.project,
            &self.consent.enrollment.account_ref,
            now,
        )
        .await?;
        if current != self.consent {
            return Err(E::Revoked);
        }
        Ok(())
    }
    pub(in crate::controller_keys) fn consent(&self) -> &Consent {
        &self.consent
    }
    pub(in crate::controller_keys) async fn audit(
        &self,
        c: &mut SqliteConnection,
        now: i64,
    ) -> Result<(), E> {
        let v = &self.consent;
        let r = &v.enrollment;
        let k = &r.credential;
        let table = if matches!(
            self.operation,
            InfrastructureOperation::LaunchExecution
                | InfrastructureOperation::StopExecution
                | InfrastructureOperation::ReconcileExecution
        ) {
            "aws_lifecycle_credential_uses"
        } else {
            "aws_credential_uses"
        };
        let old=sqlx::query(&format!("SELECT account_ref,consent_generation,connection_id,record_id,version FROM {table} WHERE project_id=? AND request_id=? AND operation=?")).bind(r.project.as_str()).bind(self.request.as_str()).bind(self.operation.name()).fetch_optional(&mut *c).await?;
        if let Some(old) = old {
            if old.try_get::<&str, _>("account_ref")? != r.account_ref.as_str()
                || old.try_get::<i64, _>("consent_generation")? != v.generation
                || old.try_get::<&str, _>("connection_id")? != k.connection
                || old.try_get::<&str, _>("record_id")? != k.record
                || old.try_get::<i64, _>("version")? != k.version
            {
                return Err(E::Conflict);
            }
            return Ok(());
        }
        let n = sqlx::query(&format!("INSERT INTO {table} VALUES(?,?,?,?,?,?,?,?,?)"))
            .bind(r.project.as_str())
            .bind(r.account_ref.as_str())
            .bind(v.generation)
            .bind(self.request.as_str())
            .bind(&k.connection)
            .bind(&k.record)
            .bind(k.version)
            .bind(self.operation.name())
            .bind(now)
            .execute(c)
            .await?
            .rows_affected();
        if n != 1 {
            return Err(E::Storage);
        }
        Ok(())
    }
}
