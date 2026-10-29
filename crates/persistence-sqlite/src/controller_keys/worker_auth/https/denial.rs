//! Denial-only diagnostics. These observations can select a redacted error code;
//! they never create a permit, return success or authorize a cached receipt.
use super::{super::*, server::HttpsWorkerServer, wire::ErrorCode as E};
impl HttpsWorkerServer {
    pub(super) async fn denial(
        &self,
        secret: &WorkerCredential,
        error: WorkerAuthError,
        usage: worker_control::CredentialScopeUse,
    ) -> E {
        if !matches!(error, WorkerAuthError::Denied | WorkerAuthError::Expired) {
            return super::server::auth_error(error);
        }
        self.denial_state(secret, usage)
            .await
            .unwrap_or_else(|_| super::server::auth_error(error))
    }
    async fn denial_state(
        &self,
        secret: &WorkerCredential,
        usage: worker_control::CredentialScopeUse,
    ) -> Result<E, WorkerAuthError> {
        let mut tx = self.security.store.pool.begin().await?;
        let now = self.security.now()?;
        let lookup = self
            .security
            .mac(b"credential-lookup", &[secret.0.as_bytes()]);
        let row=sqlx::query("SELECT c.record,c.record_mac,e.state,e.security_epoch,e.current_generation FROM worker_credentials c JOIN worker_enrollments e USING(enrollment_id) WHERE c.token_lookup=?").bind(lookup.as_slice()).fetch_optional(&mut *tx).await?.ok_or(WorkerAuthError::Denied)?;
        let text: &str = row.try_get("record")?;
        let signature: &[u8] = row.try_get("record_mac")?;
        if text.len() > 8192
            || !equal(
                signature,
                &self
                    .security
                    .mac(b"credential-record", &[text.as_bytes(), &lookup]),
            )
        {
            return Err(WorkerAuthError::Integrity);
        }
        #[derive(Deserialize)]
        struct DiagnosticRecord {
            metadata: WorkerCredentialMetadata,
            security_epoch: i64,
        }
        let record: DiagnosticRecord =
            serde_json::from_str(text).map_err(|_| WorkerAuthError::Integrity)?;
        let revoked: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM worker_revocations WHERE enrollment_id=?)",
        )
        .bind(record.metadata.enrollment.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if revoked {
            return Ok(E::RevokedCredential);
        }
        let b = &record.metadata.binding;
        let fence=sqlx::query("SELECT epoch,current_attempt FROM task_fences WHERE project_id=? AND plan_id=? AND version=? AND task_id=?").bind(b.task.plan.project_id.as_str()).bind(b.task.plan.plan_id.as_str()).bind(b.task.plan.version.get() as i64).bind(b.task.task_id.as_str()).fetch_optional(&mut *tx).await?;
        if let Some(f) = fence
            && (f.try_get::<i64, _>("epoch")? as u64 != b.epoch
                || f.try_get::<Option<&str>, _>("current_attempt")? != Some(b.attempt_id.as_str()))
        {
            return Ok(E::FencedEpoch);
        }
        if row.try_get::<&str, _>("state")? != "active"
            || row.try_get::<i64, _>("security_epoch")? != record.security_epoch
        {
            return Ok(E::RevokedCredential);
        }
        if row.try_get::<i64, _>("current_generation")? != record.metadata.generation {
            return Ok(E::StaleVersion);
        }
        if now >= record.metadata.expires_at || now < record.metadata.issued_at {
            return Ok(E::ExpiredCredential);
        }
        // Reuse the authoritative scope function, retaining its typed error.
        let current = self
            .security
            .current_credential(&mut tx, secret, now)
            .await?;
        let result = worker_control::credential_scope(
            &mut tx,
            &current.enrollment.lease(),
            self.security.policy.as_ref(),
            now,
            usage,
        )
        .await;
        use crate::{admission::AdmissionError as A, worker_simulation::WorkerError as W};
        Ok(match result {
            Err(W::Admission(A::Provider(_))) => E::ProviderEntitlementUnavailable,
            Err(W::Admission(A::Expired)) => E::LeaseExpired,
            Err(W::Admission(A::StaleFence)) | Err(W::StaleEpoch) => E::FencedEpoch,
            Err(W::FinancialRisk) | Err(W::Admission(A::Finance(_))) => E::FinanciallyBlocked,
            Err(W::Unauthorized) | Err(W::Admission(A::Forbidden)) => E::UnauthorizedScope,
            _ => E::Unauthenticated,
        })
    }
}
