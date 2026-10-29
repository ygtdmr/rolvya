use super::*;
pub(super) fn mode(m: AccessMode) -> &'static str {
    match m {
        AccessMode::ApiKey => "api_key",
        AccessMode::CliSession => "cli_session",
    }
}
fn authenticated<T: for<'de> Deserialize<'de>>(
    key: &ControllerKey,
    domain: &[u8],
    text: &str,
    sig: &[u8],
) -> Result<T, RegistryError> {
    if text.len() > 16384
        || !bool::from(subtle::ConstantTimeEq::ct_eq(
            sig,
            mac(key, domain, &[text.as_bytes()]).as_slice(),
        ))
    {
        return Err(RegistryError::Integrity);
    }
    serde_json::from_str(text).map_err(|_| RegistryError::Integrity)
}
pub(super) async fn connection(
    c: &mut SqliteConnection,
    key: &ControllerKey,
    id: &ConnectionId,
) -> Result<ConnectionRecord, RegistryError> {
    let row=sqlx::query("SELECT c.*,v.record,v.mac FROM provider_connections c JOIN provider_connection_versions v USING(connection_id,version) WHERE c.connection_id=?").bind(id.as_str()).fetch_optional(c).await?.ok_or(RegistryError::Eligibility(EligibilityReason::AwaitingProviderVerification))?;
    let r: ConnectionRecord = authenticated(
        key,
        b"connection",
        row.try_get("record")?,
        row.try_get("mac")?,
    )?;
    if r.id != *id
        || r.format != 1
        || r.installation != hex(&key.metadata.installation.0)
        || r.owner != row.try_get::<Option<String>, _>("owner_id")?
        || r.provider != row.try_get::<String, _>("provider")?
        || r.account != row.try_get::<String, _>("account")?
        || mode(r.mode) != row.try_get::<&str, _>("mode")?
        || r.version != row.try_get::<i64, _>("version")?
    {
        return Err(RegistryError::Integrity);
    }
    Ok(r)
}
pub(super) async fn grant(
    c: &mut SqliteConnection,
    key: &ControllerKey,
    id: &ConnectionId,
    p: &ProjectId,
) -> Result<ProjectGrant, RegistryError> {
    let row =
        sqlx::query("SELECT g.*,v.record AS history_record,v.mac AS history_mac FROM provider_project_grants g JOIN provider_project_grant_versions v USING(connection_id,project_id,version) WHERE g.connection_id=? AND g.project_id=?")
            .bind(id.as_str())
            .bind(p.as_str())
            .fetch_optional(c)
            .await?
            .ok_or(RegistryError::Eligibility(
                EligibilityReason::AwaitingUserConsent,
            ))?;
    let g: ProjectGrant =
        authenticated(key, b"grant", row.try_get("record")?, row.try_get("mac")?)?;
    if g.connection != *id
        || g.project != *p
        || g.version != row.try_get::<i64, _>("version")?
        || row.try_get::<&str, _>("record")? != row.try_get::<&str, _>("history_record")?
        || row.try_get::<&[u8], _>("mac")? != row.try_get::<&[u8], _>("history_mac")?
    {
        return Err(RegistryError::Integrity);
    }
    Ok(g)
}
pub(super) async fn save_version(
    c: &mut SqliteConnection,
    key: &ControllerKey,
    id: &ConnectionId,
    r: &ConnectionRecord,
) -> Result<(), RegistryError> {
    let text = encode(r)?;
    one(
        sqlx::query("INSERT INTO provider_connection_versions VALUES(?,?,?,?)")
            .bind(id.as_str())
            .bind(r.version)
            .bind(&text)
            .bind(mac(key, b"connection", &[text.as_bytes()]).as_slice())
            .execute(c)
            .await?
            .rows_affected(),
    )
}
pub(super) async fn save_grant(
    c: &mut SqliteConnection,
    key: &ControllerKey,
    g: &ProjectGrant,
) -> Result<(), RegistryError> {
    let text = encode(g)?;
    one(
        sqlx::query("INSERT INTO provider_project_grant_versions VALUES(?,?,?,?,?)")
            .bind(g.connection.as_str())
            .bind(g.project.as_str())
            .bind(g.version)
            .bind(&text)
            .bind(mac(key, b"grant", &[text.as_bytes()]).as_slice())
            .execute(&mut *c)
            .await?
            .rows_affected(),
    )?;
    one(sqlx::query("INSERT INTO provider_project_grants VALUES(?,?,?,?,?) ON CONFLICT(connection_id,project_id) DO UPDATE SET version=excluded.version,record=excluded.record,mac=excluded.mac").bind(g.connection.as_str()).bind(g.project.as_str()).bind(g.version).bind(&text).bind(mac(key,b"grant",&[text.as_bytes()]).as_slice()).execute(&mut *c).await?.rows_affected())?;
    if encode(&grant(c, key, &g.connection, &g.project).await?)? != text {
        return Err(RegistryError::Integrity);
    }
    Ok(())
}
pub(crate) async fn prepare(
    c: &mut SqliteConnection,
    key: &ControllerKey,
    actor: &str,
    p: &ProjectId,
    change: &ConnectionChange,
    t: i64,
    id: &str,
) -> Result<String, RegistryError> {
    change.validate()?;
    let x = Proposal {
        id: id.into(),
        format: 1,
        actor: actor.into(),
        project: p.clone(),
        change: change.clone(),
        created: t,
        expires: t.checked_add(1800).ok_or(RegistryError::InvalidInput)?,
    };
    target(c, key, &x).await?;
    let text = encode(&x)?;
    one(
        sqlx::query("INSERT INTO provider_change_proposals VALUES(?,?,?,?,?,0)")
            .bind(id)
            .bind(p.as_str())
            .bind(actor)
            .bind(&text)
            .bind(mac(key, b"proposal", &[text.as_bytes()]).as_slice())
            .execute(c)
            .await?
            .rows_affected(),
    )?;
    Ok(hex(&Sha256::digest(text.as_bytes())))
}
async fn proposal(
    c: &mut SqliteConnection,
    key: &ControllerKey,
    id: &str,
    digest: &str,
    actor: &str,
    p: &ProjectId,
    t: i64,
) -> Result<Proposal, RegistryError> {
    let row =
        sqlx::query("SELECT * FROM provider_change_proposals WHERE proposal_id=? AND consumed=0")
            .bind(id)
            .fetch_optional(c)
            .await?
            .ok_or(RegistryError::Denied)?;
    let text: &str = row.try_get("record")?;
    let x: Proposal = authenticated(key, b"proposal", text, row.try_get("mac")?)?;
    if x.id != id
        || x.format != 1
        || x.actor != actor
        || x.project != *p
        || row.try_get::<&str, _>("actor")? != actor
        || row.try_get::<&str, _>("project_id")? != p.as_str()
        || x.created > t
        || t >= x.expires
        || hex(&Sha256::digest(text.as_bytes())) != digest
    {
        return Err(RegistryError::Denied);
    }
    Ok(x)
}
async fn target(
    c: &mut SqliteConnection,
    key: &ControllerKey,
    x: &Proposal,
) -> Result<(), RegistryError> {
    let id = x.change.connection();
    if let ConnectionChange::Enroll { draft } = &x.change {
        if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM provider_connections WHERE connection_id=? OR (provider=? AND account=? AND mode=?))").bind(id.as_str()).bind(&draft.provider).bind(&draft.account).bind(mode(draft.mode)).fetch_one(&mut *c).await? {return Err(RegistryError::Conflict);}
    } else {
        let r = connection(c, key, id).await?;
        if (!matches!(x.change, ConnectionChange::RemoveGrant { .. })
            && r.owner.as_deref() != Some(x.actor.as_str()))
            || r.version != x.change.expected()
            || r.revoked
        {
            return Err(RegistryError::Denied);
        }
    }
    if let Some(expected) = x.change.grant_expected() {
        let v: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM provider_project_grants WHERE connection_id=? AND project_id=?",
        )
        .bind(id.as_str())
        .bind(x.project.as_str())
        .fetch_optional(c)
        .await?;
        if v.unwrap_or(0) != expected {
            return Err(RegistryError::Conflict);
        }
    }
    Ok(())
}
pub(crate) async fn validate_proposal(
    c: &mut SqliteConnection,
    key: &ControllerKey,
    id: &str,
    digest: &str,
    actor: &str,
    p: &ProjectId,
    t: i64,
) -> Result<(), RegistryError> {
    let x = proposal(c, key, id, digest, actor, p, t).await?;
    target(c, key, &x).await
}
pub(crate) async fn apply_proposal(
    c: &mut SqliteConnection,
    key: &ControllerKey,
    id: &str,
    digest: &str,
    actor: &str,
    p: &ProjectId,
    t: i64,
) -> Result<i64, RegistryError> {
    let x = proposal(c, key, id, digest, actor, p, t).await?;
    target(c, key, &x).await?;
    let cid = x.change.connection();
    let mut r = if let ConnectionChange::Enroll { draft } = &x.change {
        one(
            sqlx::query("INSERT INTO provider_connections VALUES(?,?,?,?,?,1)")
                .bind(cid.as_str())
                .bind(actor)
                .bind(&draft.provider)
                .bind(&draft.account)
                .bind(mode(draft.mode))
                .execute(&mut *c)
                .await?
                .rows_affected(),
        )?;
        ConnectionRecord {
            id: cid.clone(),
            format: 1,
            installation: hex(&key.metadata.installation.0),
            owner: Some(actor.into()),
            provider: draft.provider.clone(),
            account: draft.account.clone(),
            mode: draft.mode,
            integration: draft.integration.clone(),
            deployment: draft.deployment.clone(),
            version: 1,
            revoked: false,
            models: draft.models.clone(),
            entitlement: None,
        }
    } else {
        connection(c, key, cid).await?
    };
    if matches!(
        x.change,
        ConnectionChange::Revoke { .. }
            | ConnectionChange::Permissions { .. }
            | ConnectionChange::BeginRefresh { .. }
    ) {
        r.version = r.version.checked_add(1).ok_or(RegistryError::Conflict)?;
        r.entitlement = None;
        if matches!(x.change, ConnectionChange::Revoke { .. }) {
            r.revoked = true;
        }
        if let ConnectionChange::Permissions { models, .. } = &x.change {
            r.models = models.clone();
        }
        save_version(c, key, cid, &r).await?;
        one(sqlx::query(
            "UPDATE provider_connections SET version=? WHERE connection_id=? AND version=?",
        )
        .bind(r.version)
        .bind(cid.as_str())
        .bind(r.version - 1)
        .execute(&mut *c)
        .await?
        .rows_affected())?;
    } else if matches!(x.change, ConnectionChange::Enroll { .. }) {
        save_version(c, key, cid, &r).await?;
    }
    if let ConnectionChange::BeginRefresh { operation_id, .. } = &x.change {
        one(
            sqlx::query("INSERT INTO provider_refresh_locks VALUES(?,?,?,'in_progress',?)")
                .bind(cid.as_str())
                .bind(operation_id)
                .bind(r.version)
                .bind(t.checked_add(300).ok_or(RegistryError::InvalidInput)?)
                .execute(&mut *c)
                .await?
                .rows_affected(),
        )?;
    }
    let credential = match &x.change {
        ConnectionChange::Enroll { draft } => Some(&draft.credential),
        ConnectionChange::Grant { credential, .. }
        | ConnectionChange::Rotate { credential, .. } => Some(credential),
        _ => None,
    };
    if let Some(credential) = credential {
        credential_guard(c, key, &r, p, credential).await?;
        let v: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM provider_project_grants WHERE connection_id=? AND project_id=?",
        )
        .bind(cid.as_str())
        .bind(p.as_str())
        .fetch_optional(&mut *c)
        .await?;
        let next = v
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(RegistryError::Conflict)?;
        save_grant(
            c,
            key,
            &ProjectGrant {
                consent_version: next,
                models: r.models.clone(),
                revoked: false,
                verified_connection_version: None,
                entitlement: None,
                connection: cid.clone(),
                project: p.clone(),
                version: next,
                credential: credential.clone(),
                consent: id.into(),
            },
        )
        .await?;
    }
    if matches!(x.change, ConnectionChange::RemoveGrant { .. }) {
        let mut g = grant(c, key, cid, p).await?;
        g.version = g.version.checked_add(1).ok_or(RegistryError::Conflict)?;
        g.consent_version = g
            .consent_version
            .checked_add(1)
            .ok_or(RegistryError::Conflict)?;
        g.revoked = true;
        g.entitlement = None;
        g.verified_connection_version = None;
        g.consent = id.into();
        save_grant(c, key, &g).await?;
    }
    one(sqlx::query(
        "UPDATE provider_change_proposals SET consumed=1 WHERE proposal_id=? AND consumed=0",
    )
    .bind(id)
    .execute(&mut *c)
    .await?
    .rows_affected())?;
    audit(
        c,
        p,
        cid,
        id,
        if r.revoked { "revoked" } else { "consent" },
        EligibilityReason::AwaitingProviderVerification,
        t,
    )
    .await?;
    if encode(&connection(c, key, cid).await?)? != encode(&r)? {
        return Err(RegistryError::Integrity);
    }
    Ok(x.expires)
}
pub(super) async fn credential_guard(
    c: &mut SqliteConnection,
    key: &ControllerKey,
    r: &ConnectionRecord,
    p: &ProjectId,
    k: &CredentialReference,
) -> Result<(), RegistryError> {
    let mode = if r.mode == AccessMode::ApiKey {
        "api_key"
    } else {
        "cli_subscription"
    };
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM secret_connections c JOIN secret_records s USING(project_id,connection_id) WHERE c.project_id=? AND c.connection_id=? AND c.account_id=? AND c.provider=? AND c.mode=? AND c.state='active' AND s.record_id=? AND s.purpose=? AND s.current_version=?)").bind(p.as_str()).bind(&k.connection).bind(&k.account).bind(&k.provider).bind(mode).bind(&k.record).bind(&k.purpose).bind(k.version).fetch_one(&mut *c).await?;
    if !valid || k.provider != r.provider || k.account != r.account {
        return Err(RegistryError::Eligibility(
            EligibilityReason::ExpiredCredentials,
        ));
    }
    super::super::secret_broker::verify_reference_in(
        c,
        key,
        &broker_scope(p, r, k)?,
        super::super::secret_broker::SecretVersion::new(k.version as u64)
            .map_err(|_| RegistryError::Integrity)?,
    )
    .await
    .map_err(|_| RegistryError::Integrity)
}
pub(super) fn broker_scope(
    p: &ProjectId,
    r: &ConnectionRecord,
    k: &CredentialReference,
) -> Result<super::super::secret_broker::RecordScope, RegistryError> {
    use super::super::secret_broker as b;
    Ok(b::RecordScope {
        connection: b::ConnectionScope {
            project: p.clone(),
            connection: b::ConnectionId::new(k.connection.clone())
                .map_err(|_| RegistryError::InvalidInput)?,
            account: b::AccountId::new(k.account.clone())
                .map_err(|_| RegistryError::InvalidInput)?,
            provider: b::ProviderId::new(k.provider.clone())
                .map_err(|_| RegistryError::InvalidInput)?,
            mode: if r.mode == AccessMode::ApiKey {
                b::ProviderMode::ApiKey
            } else {
                b::ProviderMode::CliSubscription
            },
        },
        record: b::RecordId::new(k.record.clone()).map_err(|_| RegistryError::InvalidInput)?,
        purpose: b::SecretPurpose::new(k.purpose.clone())
            .map_err(|_| RegistryError::InvalidInput)?,
    })
}
pub(super) async fn audit(
    c: &mut SqliteConnection,
    p: &ProjectId,
    id: &ConnectionId,
    request: &str,
    kind: &str,
    reason: EligibilityReason,
    t: i64,
) -> Result<(), RegistryError> {
    one(sqlx::query("INSERT INTO provider_security_events(project_id,connection_id,request_id,kind,reason,recorded_at) VALUES(?,?,?,?,?,?)").bind(p.as_str()).bind(id.as_str()).bind(request).bind(kind).bind(serde_json::to_string(&reason).map_err(|_|RegistryError::Integrity)?).bind(t).execute(c).await?.rows_affected())
}
impl ProviderRegistry {
    pub(in crate::controller_keys) async fn staged_verification(
        &self,
        id: &ConnectionId,
        expected: i64,
        p: &ProjectId,
        evidence: &EvidenceRef,
        t: i64,
    ) -> Result<(ConnectionRecord, Entitlement), RegistryError> {
        let mut read = self.store.pool.begin().await?;
        let r = connection(&mut read, &self.key, id).await?;
        if r.revoked || r.version != expected {
            return Err(RegistryError::Conflict);
        }
        let g = grant(&mut read, &self.key, id, p).await?;
        read.commit().await?;
        let check = VerificationCheck {
            connection: id,
            version: expected,
            record: &r,
            grant: &g,
            evidence,
            now: t,
        };
        check.validate()?;
        let e = self.verifier.verify(&check).await?;
        self.validate_evidence(id, &r, &e, &g, t)?;
        self.verifier.current(&check, &e, t)?;
        Ok((r, e))
    }
    pub(super) fn validate_evidence(
        &self,
        id: &ConnectionId,
        r: &ConnectionRecord,
        e: &Entitlement,
        g: &ProjectGrant,
        t: i64,
    ) -> Result<(), RegistryError> {
        if e.evidence.as_str().is_empty()
            || e.verified_against != r.version
            || e.scope_digest != self.scope_digest(id, r, g)?
            || e.policy_version < 1
            || e.collected_at > t
            || e.expires_at <= t
            || e.expires_at > t.saturating_add(31 * 86400)
            || e.credential_expires_at <= t
            || e.models.len() > 32
        {
            return Err(RegistryError::Denied);
        }
        let url = url::Url::parse(&e.source_url).map_err(|_| RegistryError::Denied)?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || e.source_url.len() > 512
        {
            return Err(RegistryError::Denied);
        }
        if let Some(cli) = &e.cli {
            atom(&cli.home_identity)?;
            atom(&cli.revision)?;
        }
        atom(&e.quota_unit)?;
        for s in [
            &e.provider,
            &e.account,
            &e.integration,
            &e.deployment,
            &e.revision,
            &e.verifier,
            &e.quota_window,
        ] {
            atom(s)?;
        }
        if e.provider != r.provider
            || e.account != r.account
            || e.mode != r.mode
            || e.integration != r.integration
            || e.deployment != r.deployment
        {
            return Err(RegistryError::Eligibility(
                EligibilityReason::AccountIdentityMismatch,
            ));
        }
        for m in &e.models {
            for s in [
                &m.model,
                &m.revision,
                &m.infrastructure_provider,
                &m.infrastructure_account,
                &m.pool,
                &m.region,
                &m.resource_class,
            ] {
                atom(s)?;
            }
            if !r.models.contains(&m.model)
                || m.efforts.is_empty()
                || m.efforts.len() > 4
                || m.efforts
                    .iter()
                    .enumerate()
                    .any(|(i, x)| m.efforts[..i].contains(x))
                || m.quota_units == 0
                || m.cpu_millis == 0
                || m.memory_mib == 0
                || m.timeout_seconds == 0
            {
                return Err(RegistryError::Denied);
            }
        }
        Ok(())
    }
    pub(in crate::controller_keys) async fn recheck_verification(
        &self,
        c: &mut SqliteConnection,
        p: &ProjectId,
        r: &ConnectionRecord,
        e: &Entitlement,
        evidence: &EvidenceRef,
        t: i64,
    ) -> Result<(), RegistryError> {
        let id = &r.id;
        let g = grant(c, &self.key, id, p).await?;
        let check = VerificationCheck {
            connection: id,
            version: r.version,
            record: r,
            grant: &g,
            evidence,
            now: t,
        };
        check.validate()?;
        self.validate_evidence(id, r, e, &g, t)?;
        self.verifier.current(&check, e, t)
    }
    pub(in crate::controller_keys) async fn apply_verification(
        &self,
        c: &mut SqliteConnection,
        p: &ProjectId,
        r: &ConnectionRecord,
        e: &Entitlement,
        evidence: &EvidenceRef,
        t: i64,
    ) -> Result<(), RegistryError> {
        let id = &r.id;
        if encode(&connection(c, &self.key, id).await?)? != encode(r)? {
            return Err(RegistryError::Conflict);
        }
        let mut g = grant(c, &self.key, id, p).await?;
        if e.evidence != *evidence {
            return Err(RegistryError::Denied);
        }
        self.recheck_verification(c, p, r, e, evidence, t).await?;
        let policy = sqlx::query(
            "SELECT version,revision FROM provider_policy_revisions WHERE provider=? AND mode=?",
        )
        .bind(&r.provider)
        .bind(mode(r.mode))
        .fetch_optional(&mut *c)
        .await?;
        match policy {
            Some(x)
                if x.try_get::<i64, _>("version")? > e.policy_version
                    || (x.try_get::<i64, _>("version")? == e.policy_version
                        && x.try_get::<&str, _>("revision")? != e.revision) =>
            {
                return Err(RegistryError::Eligibility(
                    EligibilityReason::ProviderPolicyChanged,
                ));
            }
            Some(x) if x.try_get::<i64, _>("version")? < e.policy_version => {
                one(sqlx::query("UPDATE provider_policy_revisions SET revision=?,version=? WHERE provider=? AND mode=? AND version=?").bind(&e.revision).bind(e.policy_version).bind(&r.provider).bind(mode(r.mode)).bind(x.try_get::<i64,_>("version")?).execute(&mut *c).await?.rows_affected())?;
            }
            None => {
                one(
                    sqlx::query("INSERT INTO provider_policy_revisions VALUES(?,?,?,?)")
                        .bind(&r.provider)
                        .bind(mode(r.mode))
                        .bind(&e.revision)
                        .bind(e.policy_version)
                        .execute(&mut *c)
                        .await?
                        .rows_affected(),
                )?;
            }
            _ => {}
        }
        let lock:Option<String>=sqlx::query_scalar("SELECT operation_id FROM provider_refresh_locks WHERE connection_id=? AND state='in_progress'").bind(id.as_str()).fetch_optional(&mut *c).await?;
        if lock != e.refresh_operation {
            return Err(RegistryError::Denied);
        }
        if let Some(operation) = &lock {
            one(sqlx::query("UPDATE provider_refresh_locks SET state='reconciled' WHERE connection_id=? AND operation_id=? AND expected_version=? AND state='in_progress'").bind(id.as_str()).bind(operation).bind(r.version).execute(&mut *c).await?.rows_affected())?;
        }
        if let Some(cli) = &e.cli {
            let home: Option<String> = sqlx::query_scalar(
                "SELECT home_identity FROM provider_cli_homes WHERE connection_id=?",
            )
            .bind(id.as_str())
            .fetch_optional(&mut *c)
            .await?;
            if home.as_deref().is_some_and(|h| h != cli.home_identity) {
                return Err(RegistryError::Denied);
            }
            if home.is_none() {
                one(sqlx::query("INSERT INTO provider_cli_homes VALUES(?,?)")
                    .bind(id.as_str())
                    .bind(&cli.home_identity)
                    .execute(&mut *c)
                    .await?
                    .rows_affected())?;
            }
        }
        let changed = r
            .entitlement
            .as_ref()
            .map(account_facts)
            .transpose()?
            .as_ref()
            != Some(&account_facts(e)?);
        let mut next = r.clone();
        if changed {
            next.version = next.version.checked_add(1).ok_or(RegistryError::Conflict)?;
            next.entitlement = Some(e.clone());
            save_version(c, &self.key, id, &next).await?;
            one(sqlx::query(
                "UPDATE provider_connections SET version=? WHERE connection_id=? AND version=?",
            )
            .bind(next.version)
            .bind(id.as_str())
            .bind(r.version)
            .execute(&mut *c)
            .await?
            .rows_affected())?;
        }
        g.version = g.version.checked_add(1).ok_or(RegistryError::Conflict)?;
        g.verified_connection_version = Some(next.version);
        g.entitlement = Some(e.clone());
        save_grant(c, &self.key, &g).await?;
        audit(c, p, id, evidence.as_str(), "verified", e.decision, t).await?;
        if encode(&connection(c, &self.key, id).await?)? != encode(&next)? {
            return Err(RegistryError::Integrity);
        }
        Ok(())
    }
}
pub(crate) async fn verify_request_owner(
    c: &mut SqliteConnection,
    key: &ControllerKey,
    id: &ConnectionId,
    expected: i64,
    actor: &str,
    p: &ProjectId,
) -> Result<(), RegistryError> {
    let r = connection(c, key, id).await?;
    let g = grant(c, key, id, p).await?;
    if r.revoked || g.revoked || r.owner.as_deref() != Some(actor) || r.version != expected {
        return Err(RegistryError::Denied);
    }
    Ok(())
}
pub(super) fn account_facts(e: &Entitlement) -> Result<String, RegistryError> {
    encode(&(
        1,
        &e.provider,
        &e.account,
        e.mode,
        &e.integration,
        &e.deployment,
        e.policy,
        e.permission,
        &e.cli,
        &e.quota_unit,
        &e.revision,
        e.policy_version,
        e.decision,
        &e.models,
        e.concurrency,
        (&e.quota_window, e.quota_ceiling, e.refresh_serialized),
    ))
}
pub(in crate::controller_keys) async fn metadata(
    c: &mut SqliteConnection,
    key: &ControllerKey,
    p: &ProjectId,
    after: Option<&ConnectionId>,
    limit: u16,
) -> Result<Vec<ConnectionMetadata>, RegistryError> {
    let ids:Vec<String>=sqlx::query_scalar("SELECT connection_id FROM provider_project_grants WHERE project_id=? AND connection_id>? ORDER BY connection_id LIMIT ?").bind(p.as_str()).bind(after.map(ConnectionId::as_str).unwrap_or("")).bind(limit).fetch_all(&mut *c).await?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let id = ConnectionId::parse(id)?;
        let r = connection(c, key, &id).await?;
        let g = grant(c, key, &id, p).await?;
        out.push(ConnectionMetadata {
            connection: id,
            provider: r.provider,
            account: r.account,
            mode: r.mode,
            connection_version: r.version,
            grant_version: g.version,
            revoked: r.revoked || g.revoked,
            consented_models: g.models,
            recorded_verification_deadline: g.entitlement.as_ref().map(Entitlement::deadline),
        });
    }
    Ok(out)
}
