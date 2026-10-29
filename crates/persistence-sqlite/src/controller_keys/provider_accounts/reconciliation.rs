use super::*;
#[derive(Debug, Clone)]
pub struct CapacityRelease {
    pub lease: LeaseRef,
    pub request_id: RequestId,
    pub evidence: EvidenceRef,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapacityReleaseReceipt {
    pub connection: ConnectionId,
    pub audit_event: i64,
}
pub(super) struct ReconciliationCheck<'a> {
    pub(super) request: &'a CapacityRelease,
    pub(super) attribution: &'a str,
}
impl ReconciliationCheck<'_> {
    fn validate(&self) -> Result<(), RegistryError> {
        gate::attribution_scope(self.attribution, &self.request.lease)
    }
}
pub(super) struct ReconciliationGrant {
    pub(super) digest: String,
    pub(super) provider_operation: String,
    pub(super) terminated: bool,
    pub(super) billing_final: bool,
    pub(super) expires_at: i64,
}
pub(super) type ReconciliationFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ReconciliationGrant, RegistryError>> + Send + 'a>>;
/// Independently verified model-provider termination AND final usage, separately
/// from worker-provider observations. No worker acknowledgment is sufficient.
pub(super) trait ReconciliationAuthority: Send + Sync {
    fn verify<'a>(&'a self, c: &'a ReconciliationCheck<'a>, t: i64) -> ReconciliationFuture<'a>;
    fn current(
        &self,
        c: &ReconciliationCheck<'_>,
        g: &ReconciliationGrant,
        t: i64,
    ) -> Result<(), RegistryError>;
}
impl ReconciliationAuthority for Unavailable {
    fn verify<'a>(&'a self, _: &'a ReconciliationCheck<'a>, _: i64) -> ReconciliationFuture<'a> {
        Box::pin(async { Err(RegistryError::Unavailable) })
    }
    fn current(
        &self,
        _: &ReconciliationCheck<'_>,
        _: &ReconciliationGrant,
        _: i64,
    ) -> Result<(), RegistryError> {
        Err(RegistryError::Unavailable)
    }
}
impl ProviderRegistry {
    pub async fn reconcile_capacity(
        &self,
        r: &CapacityRelease,
        clock: &impl Clock,
    ) -> Result<CapacityReleaseReceipt, RegistryError> {
        let mut read = self.store.pool.begin().await?;
        let start = clock.now().map_err(|_| RegistryError::Unavailable)?;
        let (text, _) = gate::attribution_record(
            &mut read,
            self,
            &r.lease.task.plan.project_id,
            &r.lease.attempt_id,
        )
        .await?;
        gate::attribution_scope(&text, &r.lease)?;
        read.commit().await?;
        let check = ReconciliationCheck {
            request: r,
            attribution: &text,
        };
        check.validate()?;
        let grant = self.reconciliation.verify(&check, start).await?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let t = clock.now().map_err(|_| RegistryError::Unavailable)?;
        if t < start
            || t >= grant.expires_at
            || !grant.terminated
            || !grant.billing_final
            || grant.digest != hex(&Sha256::digest(text.as_bytes()))
        {
            return Err(RegistryError::Denied);
        }
        atom(&grant.provider_operation)?;
        self.reconciliation.current(&check, &grant, t)?;
        let (admission, _, status) = crate::admission::stored_admission(&mut tx, &r.lease)
            .await
            .map_err(|_| RegistryError::Denied)?;
        if status == "admitted" {
            return Err(RegistryError::Denied);
        }
        let observation: Option<String> = sqlx::query_scalar(
            "SELECT state FROM dispatch_operations WHERE project_id=? AND attempt_id=?",
        )
        .bind(r.lease.task.plan.project_id.as_str())
        .bind(r.lease.attempt_id.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        if !observation.is_some_and(|x| matches!(x.as_str(), "finished" | "no_effect")) {
            return Err(RegistryError::Denied);
        }
        let l = crate::finance::liability(
            &mut tx,
            &r.lease.task.plan.project_id,
            &crate::finance::LiabilityId::new(r.lease.attempt_id.as_str())
                .map_err(|_| RegistryError::InvalidInput)?,
            admission.permit.maximum.ai_api.currency(),
        )
        .await
        .map_err(|_| RegistryError::Denied)?
        .ok_or(RegistryError::Integrity)?;
        if !l.finalized
            || l.uncertain
            || l.held
                .total()
                .map_err(|_| RegistryError::Integrity)?
                .minor_units()
                != 0
            || l.provisional
                .total()
                .map_err(|_| RegistryError::Integrity)?
                .minor_units()
                != 0
        {
            return Err(RegistryError::Eligibility(
                EligibilityReason::UncertainFinancialExposure,
            ));
        }
        let (verified_text, _) = gate::attribution_record(
            &mut tx,
            self,
            &r.lease.task.plan.project_id,
            &r.lease.attempt_id,
        )
        .await?;
        if verified_text != text {
            return Err(RegistryError::Integrity);
        }
        let current=sqlx::query("SELECT state,record,connection_id FROM provider_capacity_reservations WHERE project_id=? AND attempt_id=?").bind(r.lease.task.plan.project_id.as_str()).bind(r.lease.attempt_id.as_str()).fetch_one(&mut *tx).await?;
        if current.try_get::<&str, _>("record")? != text {
            return Err(RegistryError::Integrity);
        }
        let id = ConnectionId::parse(current.try_get("connection_id")?)?;
        let event = if current.try_get::<&str, _>("state")? == "held" {
            one(sqlx::query("UPDATE provider_capacity_reservations SET state='released' WHERE project_id=? AND attempt_id=? AND state='held'").bind(r.lease.task.plan.project_id.as_str()).bind(r.lease.attempt_id.as_str()).execute(&mut *tx).await?.rows_affected())?;
            store::audit(
                &mut tx,
                &r.lease.task.plan.project_id,
                &id,
                r.request_id.as_str(),
                "released",
                EligibilityReason::Eligible,
                t,
            )
            .await?;
            sqlx::query_scalar("SELECT sequence FROM provider_security_events WHERE project_id=? AND connection_id=? AND request_id=? AND kind='released'").bind(r.lease.task.plan.project_id.as_str()).bind(id.as_str()).bind(r.request_id.as_str()).fetch_one(&mut *tx).await?
        } else {
            sqlx::query_scalar("SELECT sequence FROM provider_security_events WHERE project_id=? AND connection_id=? AND request_id=? AND kind='released'").bind(r.lease.task.plan.project_id.as_str()).bind(id.as_str()).bind(r.request_id.as_str()).fetch_optional(&mut *tx).await?.ok_or(RegistryError::Conflict)?
        };
        let end = clock.now().map_err(|_| RegistryError::Unavailable)?;
        if end < t || end >= grant.expires_at {
            return Err(RegistryError::Denied);
        }
        self.reconciliation.current(&check, &grant, end)?;
        tx.commit().await?;
        Ok(CapacityReleaseReceipt {
            connection: id,
            audit_event: event,
        })
    }
}
