use super::*;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Attribution {
    connection: ConnectionId,
    connection_version: i64,
    grant_version: i64,
    project: ProjectId,
    attempt: AttemptId,
    task: TaskRef,
    epoch: u64,
    assignment: ExecutionAssignment,
    permit: ExecutionPermit,
    quota_window: String,
    quota_units: u32,
}
/// True only for the historical trusted synthetic kernel, before any controller
/// installation. It cannot bootstrap S2-04 credentials or a production adapter.
async fn legacy(
    c: &mut SqliteConnection,
    a: &ExecutionAssignment,
    permit: &ExecutionPermit,
) -> Result<bool, RegistryError> {
    let ready: bool =
        sqlx::query_scalar("SELECT state='ready' FROM controller_installation WHERE singleton=1")
            .fetch_one(&mut *c)
            .await?;
    let available: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='provider_connections')",
    )
    .fetch_one(&mut *c)
    .await?;
    let enrolled: bool = if available {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM provider_connections WHERE connection_id=?)",
        )
        .bind(&a.connection_ref)
        .fetch_one(c)
        .await?
    } else {
        false
    };
    Ok(!ready
        && !enrolled
        && a.model == "synthetic/model"
        && a.connection_ref == "synthetic-connection"
        && permit.provider.as_str() == "synthetic"
        && permit.account.as_str() == "synthetic-account"
        && permit.pool.as_str() == "synthetic-pool")
}
fn reason(r: EligibilityReason) -> RegistryError {
    RegistryError::Eligibility(r)
}
pub(super) fn verify_claims(
    r: &ConnectionRecord,
    e: &Entitlement,
    t: i64,
) -> Result<(), RegistryError> {
    if r.revoked {
        return Err(reason(EligibilityReason::RevokedConnection));
    }
    if e.provider != r.provider
        || e.account != r.account
        || e.mode != r.mode
        || e.integration != r.integration
        || e.deployment != r.deployment
    {
        return Err(reason(EligibilityReason::AccountIdentityMismatch));
    }
    if t < e.collected_at || t >= e.expires_at {
        return Err(reason(EligibilityReason::ProviderPolicyChanged));
    }
    if t >= e.credential_expires_at {
        return Err(reason(EligibilityReason::ExpiredCredentials));
    }
    match (r.mode, e.policy, e.permission) {
        (
            AccessMode::ApiKey,
            PolicyClass::ApiGenerallySupported,
            AccountPermission::VerifiedApi,
        ) => {}
        (
            AccessMode::CliSession,
            PolicyClass::ThirdPartySpecificallyPermitted,
            AccountPermission::VerifiedHostedCli,
        ) if e.cli.is_some() && e.refresh_serialized => {}
        (_, PolicyClass::Disallowed, _) => {
            return Err(reason(EligibilityReason::UnsupportedIntegration));
        }
        (_, _, AccountPermission::Revoked) => {
            return Err(reason(EligibilityReason::RevokedConnection));
        }
        _ => return Err(reason(EligibilityReason::AwaitingProviderVerification)),
    }
    if e.decision != EligibilityReason::Eligible {
        return Err(reason(e.decision));
    }
    if e.models.is_empty()
        || e.models.len() > 32
        || e.concurrency == 0
        || e.concurrency > 10000
        || e.quota_ceiling <= 0
    {
        return Err(reason(EligibilityReason::AwaitingProviderVerification));
    }
    for m in &e.models {
        if m.efforts.is_empty() || m.efforts.len() > 4 || m.quota_units == 0 {
            return Err(RegistryError::Integrity);
        }
    }
    Ok(())
}
pub(super) async fn eligible(
    c: &mut SqliteConnection,
    registry: &ProviderRegistry,
    p: &ProjectId,
    a: &ExecutionAssignment,
    permit: &ExecutionPermit,
    t: i64,
) -> Result<(ConnectionRecord, ProjectGrant, ModelCapability), RegistryError> {
    let id = ConnectionId::parse(a.connection_ref.clone())?;
    let r = store::connection(c, &registry.key, &id).await?;
    if r.revoked {
        return Err(reason(EligibilityReason::RevokedConnection));
    }
    if r.mode != a.access_mode {
        return Err(reason(EligibilityReason::UnsupportedIntegration));
    }
    let g = store::grant(c, &registry.key, &id, p).await?;
    store::credential_guard(c, &registry.key, &r, p, &g.credential).await?;
    if let Some(owner) = &r.owner {
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM human_users u JOIN human_emails e USING(user_id) JOIN human_memberships m USING(user_id) WHERE user_id=? AND u.state='active' AND e.verified=1 AND m.project_id=? AND m.state='active' AND m.role='owner')").bind(owner).bind(p.as_str()).fetch_one(&mut *c).await?;
        if !valid {
            return Err(reason(EligibilityReason::AwaitingUserConsent));
        }
    }
    let refreshing:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM provider_refresh_locks WHERE connection_id=? AND state='in_progress')").bind(id.as_str()).fetch_one(&mut *c).await?;
    if refreshing {
        return Err(reason(EligibilityReason::ConcurrencyUnavailable));
    }
    let e = g
        .entitlement
        .as_ref()
        .ok_or(reason(EligibilityReason::AwaitingProviderVerification))?;
    if g.revoked || g.verified_connection_version != Some(r.version) {
        return Err(reason(EligibilityReason::AwaitingUserConsent));
    }
    let mut origin = r.clone();
    origin.version = e.verified_against;
    if registry.scope_digest(&id, &origin, &g)? != e.scope_digest
        || r.entitlement
            .as_ref()
            .map(store::account_facts)
            .transpose()?
            .as_ref()
            != Some(&store::account_facts(e)?)
    {
        return Err(RegistryError::Integrity);
    }
    let check = VerificationCheck {
        connection: &id,
        version: e.verified_against,
        record: &origin,
        grant: &g,
        evidence: &e.evidence,
        now: t,
    };
    check.validate()?;
    registry.verifier.current(&check, e, t)?;
    verify_claims(&r, e, t)?;
    #[cfg(not(test))]
    if r.owner.is_none() {
        return Err(reason(EligibilityReason::AwaitingUserConsent));
    }
    let revision: Option<i64> = sqlx::query_scalar(
        "SELECT version FROM provider_policy_revisions WHERE provider=? AND mode=? AND revision=?",
    )
    .bind(&r.provider)
    .bind(if r.mode == AccessMode::ApiKey {
        "api_key"
    } else {
        "cli_session"
    })
    .bind(&e.revision)
    .fetch_optional(&mut *c)
    .await?;
    if revision != Some(e.policy_version) {
        return Err(reason(EligibilityReason::ProviderPolicyChanged));
    }
    if !r.models.contains(&a.model) || !g.models.contains(&a.model) {
        return Err(reason(EligibilityReason::UnsupportedModelEffort));
    }
    let cap = e
        .models
        .iter()
        .find(|m| {
            m.model == a.model
                && m.efforts.contains(&a.effort)
                && m.cpu_millis >= a.resources.cpu_millis
                && m.memory_mib >= a.resources.memory_mib
                && m.timeout_seconds >= a.resources.timeout_seconds
                && m.infrastructure_provider == permit.provider.as_str()
                && m.infrastructure_account == permit.account.as_str()
                && m.pool == permit.pool.as_str()
        })
        .cloned()
        .ok_or(reason(EligibilityReason::UnsupportedModelEffort))?;
    let refreshing:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM provider_refresh_locks WHERE connection_id=? AND state='in_progress')").bind(id.as_str()).fetch_one(c).await?;
    if refreshing {
        return Err(reason(EligibilityReason::ConcurrencyUnavailable));
    }
    Ok((r, g, cap))
}
pub(crate) async fn before_admission<P: SchedulerPolicy + ?Sized>(
    c: &mut SqliteConnection,
    policy: &P,
    r: &AdmissionRequest,
    a: &ExecutionAssignment,
    t: i64,
) -> Result<(), RegistryError> {
    if let Some(registry) = policy.provider_registry() {
        let (record, _, cap) =
            eligible(c, registry, &r.task.plan.project_id, a, &r.permit, t).await?;
        let used:i64=sqlx::query_scalar("SELECT count(*) FROM provider_capacity_reservations WHERE connection_id=? AND state='held'").bind(&a.connection_ref).fetch_one(&mut *c).await?;
        let e = record
            .entitlement
            .as_ref()
            .ok_or(RegistryError::Integrity)?;
        if used >= i64::from(e.concurrency) {
            return Err(reason(EligibilityReason::ConcurrencyUnavailable));
        }
        let units:i64=sqlx::query_scalar("SELECT coalesce(sum(quota_units),0) FROM provider_capacity_reservations WHERE connection_id=? AND quota_window=?").bind(&a.connection_ref).bind(&e.quota_window).fetch_one(c).await?;
        if units
            .checked_add(i64::from(cap.quota_units))
            .is_none_or(|x| x > e.quota_ceiling)
        {
            return Err(reason(EligibilityReason::QuotaExhausted));
        }
    } else if !legacy(c, a, &r.permit).await? {
        return Err(reason(EligibilityReason::AwaitingProviderVerification));
    }
    Ok(())
}
pub(crate) async fn reserve<P: SchedulerPolicy + ?Sized>(
    c: &mut SqliteConnection,
    policy: &P,
    r: &AdmissionRequest,
    a: &ExecutionAssignment,
    t: i64,
) -> Result<(), RegistryError> {
    let Some(registry) = policy.provider_registry() else {
        if legacy(c, a, &r.permit).await? {
            return Ok(());
        }
        return Err(reason(EligibilityReason::AwaitingProviderVerification));
    };
    let (rec, g, cap) = eligible(c, registry, &r.task.plan.project_id, a, &r.permit, t).await?;
    let e = rec.entitlement.as_ref().ok_or(RegistryError::Integrity)?;
    let attribution = Attribution {
        connection: ConnectionId::parse(a.connection_ref.clone())?,
        connection_version: rec.version,
        grant_version: g.version,
        project: r.task.plan.project_id.clone(),
        attempt: AttemptId::new(r.request_id.as_str()).map_err(|_| RegistryError::InvalidInput)?,
        task: r.task.clone(),
        epoch: r
            .expected_epoch
            .checked_add(1)
            .ok_or(RegistryError::Integrity)?,
        assignment: a.clone(),
        permit: r.permit.clone(),
        quota_window: e.quota_window.clone(),
        quota_units: cap.quota_units,
    };
    let text = encode(&attribution)?;
    one(sqlx::query(
        "INSERT INTO provider_capacity_reservations VALUES(?,?,?,?,?,?,?,'held',?,?)",
    )
    .bind(attribution.project.as_str())
    .bind(attribution.attempt.as_str())
    .bind(&a.connection_ref)
    .bind(rec.version)
    .bind(g.version)
    .bind(&e.quota_window)
    .bind(cap.quota_units)
    .bind(&text)
    .bind(registry.mac(b"attribution", &[text.as_bytes()]).as_slice())
    .execute(&mut *c)
    .await?
    .rows_affected())?;
    store::audit(
        c,
        &r.task.plan.project_id,
        &attribution.connection,
        r.request_id.as_str(),
        "reserved",
        EligibilityReason::Eligible,
        t,
    )
    .await
}
pub(crate) async fn current<P: SchedulerPolicy + ?Sized>(
    c: &mut SqliteConnection,
    policy: &P,
    r: &AdmissionRequest,
    a: &ExecutionAssignment,
    t: i64,
) -> Result<(), RegistryError> {
    let Some(registry) = policy.provider_registry() else {
        if legacy(c, a, &r.permit).await? {
            return Ok(());
        }
        return Err(reason(EligibilityReason::AwaitingProviderVerification));
    };
    let (rec, g, cap) = eligible(c, registry, &r.task.plan.project_id, a, &r.permit, t).await?;
    let (text, state) = attribution_record(
        c,
        registry,
        &r.task.plan.project_id,
        &AttemptId::new(r.request_id.as_str()).map_err(|_| RegistryError::InvalidInput)?,
    )
    .await?;
    if state != "held" {
        return Err(RegistryError::Denied);
    }
    let x: Attribution = serde_json::from_str(&text).map_err(|_| RegistryError::Integrity)?;
    if x.connection.as_str() != a.connection_ref
        || x.project != r.task.plan.project_id
        || x.attempt.as_str() != r.request_id.as_str()
        || x.task != r.task
        || x.epoch
            != r.expected_epoch
                .checked_add(1)
                .ok_or(RegistryError::Integrity)?
        || x.assignment != *a
        || x.permit != r.permit
        || x.connection_version != rec.version
        || x.grant_version != g.version
        || x.quota_units != cap.quota_units
        || x.quota_window
            != rec
                .entitlement
                .as_ref()
                .ok_or(RegistryError::Integrity)?
                .quota_window
    {
        return Err(reason(EligibilityReason::ProviderPolicyChanged));
    }
    Ok(())
}
pub(crate) async fn broker_in<P: SchedulerPolicy + ?Sized>(
    c: &mut SqliteConnection,
    policy: &P,
    request: &super::super::secret_broker::SecretUseRequest,
    t: i64,
) -> Result<(), RegistryError> {
    use super::super::secret_broker::{CredentialOperation, ProviderMode};
    if request.operation == CredentialOperation::LocalIntegrityInspection {
        return Ok(());
    }
    let registry = policy.provider_registry().ok_or(RegistryError::Denied)?;
    let worker = request.worker.as_ref().ok_or(RegistryError::Denied)?;
    let lease = LeaseRef {
        task: worker.binding.task.clone(),
        attempt_id: worker.binding.attempt_id.clone(),
        epoch: worker.binding.epoch,
    };
    let (admission, _, _) = crate::admission::stored_admission(c, &lease)
        .await
        .map_err(|_| RegistryError::Denied)?;
    let text:String=sqlx::query_scalar("SELECT json_extract(definition,'$.execution') FROM tasks WHERE project_id=? AND plan_id=? AND version=? AND task_id=?").bind(lease.task.plan.project_id.as_str()).bind(lease.task.plan.plan_id.as_str()).bind(lease.task.plan.version.get() as i64).bind(lease.task.task_id.as_str()).fetch_one(&mut *c).await?;
    let a: ExecutionAssignment =
        serde_json::from_str(&text).map_err(|_| RegistryError::Integrity)?;
    current(c, policy, &admission, &a, t).await?;
    let (r, g, _) = eligible(
        c,
        registry,
        &lease.task.plan.project_id,
        &a,
        &admission.permit,
        t,
    )
    .await?;
    let s = &request.scope;
    let k = &g.credential;
    let mode = match s.connection.mode {
        ProviderMode::ApiKey => AccessMode::ApiKey,
        ProviderMode::CliSubscription => AccessMode::CliSession,
    };
    if s.connection.project != g.project
        || s.connection.connection.as_str() != k.connection
        || s.connection.account.as_str() != r.account
        || s.connection.provider.as_str() != r.provider
        || mode != r.mode
        || s.record.as_str() != k.record
        || s.purpose.as_str() != k.purpose
        || request.version.get() != k.version as u64
    {
        return Err(RegistryError::Denied);
    }
    Ok(())
}
pub(crate) async fn record_blocked(
    store: &PlanStore,
    r: &AdmissionRequest,
    a: &ExecutionAssignment,
    e: RegistryError,
    t: i64,
) -> Result<(), RegistryError> {
    let id = ConnectionId::parse(a.connection_ref.clone())?;
    let digest = hex(&Sha256::digest(encode(&(
        r.task.clone(),
        r.request_id.as_str(),
        &a.connection_ref,
        format!("{e:?}"),
    ))?));
    let request = format!("blocked-{}", &digest[..48]);
    let why = match e {
        RegistryError::Eligibility(x) => x,
        _ => EligibilityReason::AwaitingProviderVerification,
    };
    let mut tx = store.pool.begin_with("BEGIN IMMEDIATE").await?;
    let old:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM provider_security_events WHERE project_id=? AND connection_id=? AND request_id=? AND kind='blocked')").bind(r.task.plan.project_id.as_str()).bind(id.as_str()).bind(&request).fetch_one(&mut *tx).await?;
    if !old {
        store::audit(
            &mut tx,
            &r.task.plan.project_id,
            &id,
            &request,
            "blocked",
            why,
            t,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Applies the current provider authorization limit separately from token and
/// infrastructure permit lifetimes. It grants neither renewal nor extra spend.
pub(crate) async fn authority_until<P: SchedulerPolicy + ?Sized>(
    c: &mut SqliteConnection,
    policy: &P,
    r: &AdmissionRequest,
    a: &ExecutionAssignment,
    t: i64,
) -> Result<i64, RegistryError> {
    if let Some(registry) = policy.provider_registry() {
        let (_, g, _) = eligible(c, registry, &r.task.plan.project_id, a, &r.permit, t).await?;
        let e = g.entitlement.as_ref().ok_or(RegistryError::Integrity)?;
        Ok(r.permit
            .valid_until
            .min(e.expires_at)
            .min(e.credential_expires_at))
    } else if legacy(c, a, &r.permit).await? {
        Ok(r.permit.valid_until)
    } else {
        Err(reason(EligibilityReason::AwaitingProviderVerification))
    }
}
/// Grouped borrowed model-credential scope; distinct from infrastructure identity.
pub(crate) struct ModelConnection<'a> {
    pub connection: &'a str,
    pub account: &'a str,
    pub provider: &'a str,
}
pub(crate) async fn broker_connection(
    c: &mut SqliteConnection,
    policy: &dyn SchedulerPolicy,
    r: &AdmissionRequest,
    a: &ExecutionAssignment,
    t: i64,
    scope: ModelConnection<'_>,
) -> Result<(), RegistryError> {
    let registry = policy.provider_registry().ok_or(RegistryError::Denied)?;
    let (record, g, _) = eligible(c, registry, &r.task.plan.project_id, a, &r.permit, t).await?;
    if g.credential.connection != scope.connection
        || record.account != scope.account
        || record.provider != scope.provider
    {
        return Err(RegistryError::Denied);
    }
    Ok(())
}

pub(super) async fn attribution_record(
    c: &mut SqliteConnection,
    r: &ProviderRegistry,
    p: &ProjectId,
    attempt: &AttemptId,
) -> Result<(String, String), RegistryError> {
    let row = sqlx::query(
        "SELECT * FROM provider_capacity_reservations WHERE project_id=? AND attempt_id=?",
    )
    .bind(p.as_str())
    .bind(attempt.as_str())
    .fetch_one(c)
    .await?;
    let text: String = row.try_get("record")?;
    if text.len() > 16384
        || !bool::from(subtle::ConstantTimeEq::ct_eq(
            row.try_get::<&[u8], _>("mac")?,
            r.mac(b"attribution", &[text.as_bytes()]).as_slice(),
        ))
    {
        return Err(RegistryError::Integrity);
    }
    let x: Attribution = serde_json::from_str(&text).map_err(|_| RegistryError::Integrity)?;
    if x.project != *p
        || x.attempt != *attempt
        || x.connection.as_str() != row.try_get::<&str, _>("connection_id")?
        || x.connection_version != row.try_get::<i64, _>("connection_version")?
        || x.grant_version != row.try_get::<i64, _>("grant_version")?
        || x.quota_window != row.try_get::<&str, _>("quota_window")?
        || i64::from(x.quota_units) != row.try_get::<i64, _>("quota_units")?
    {
        return Err(RegistryError::Integrity);
    }
    Ok((text, row.try_get("state")?))
}
pub(super) fn attribution_scope(text: &str, lease: &LeaseRef) -> Result<(), RegistryError> {
    let x: Attribution = serde_json::from_str(text).map_err(|_| RegistryError::Integrity)?;
    if x.task != lease.task || x.attempt != lease.attempt_id || x.epoch != lease.epoch {
        return Err(RegistryError::Denied);
    }
    Ok(())
}
