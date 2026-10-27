use super::*;
use crypto::Envelope;
use sqlx::{Row, Sqlite, SqliteConnection, Transaction};
use subtle::ConstantTimeEq;
impl SecretBroker {
    async fn audit(
        &self,
        c: &mut SqliteConnection,
        admin: &AdminRequest,
        scope: &ConnectionScope,
        record: Option<&RecordScope>,
        action: BrokerAction,
        outcome: &str,
    ) -> Result<i64, BrokerError> {
        let now = self.now()?;
        sqlx::query("INSERT INTO secret_events(project_id,request_id,connection_id,record_id,action,outcome,recorded_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(project_id,request_id,action,outcome) DO NOTHING")
            .bind(scope.project.as_str()).bind(admin.request_id.as_str()).bind(scope.connection.as_str()).bind(record.map_or("",|r|r.record.as_str())).bind(action as u8).bind(outcome).bind(now).execute(&mut *c).await?;
        Ok(sqlx::query_scalar("SELECT sequence FROM secret_events WHERE project_id=? AND request_id=? AND action=? AND outcome=?")
            .bind(scope.project.as_str()).bind(admin.request_id.as_str()).bind(action as u8).bind(outcome).fetch_one(c).await?)
    }
    async fn authorize(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        ctx: &AdminContext<'_>,
    ) -> Result<(), BrokerError> {
        if let Err(error) = self.human.check(ctx) {
            // A denied public request cannot create a foreign project. Audit only
            // existing scopes; failures remain denied and reveal no existence data.
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE project_id=?)")
                    .bind(ctx.scope.project.as_str())
                    .fetch_one(&mut **tx)
                    .await?;
            if exists {
                self.audit(tx, ctx.request, ctx.scope, ctx.record, ctx.action, "denied")
                    .await?;
            }
            return Err(error);
        }
        Ok(())
    }
    async fn denied(
        &self,
        mut tx: Transaction<'_, Sqlite>,
        ctx: &AdminContext<'_>,
        error: BrokerError,
    ) -> Result<BrokerReceipt, BrokerError> {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE project_id=?)")
                .bind(ctx.scope.project.as_str())
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            tx.commit().await?;
            return Err(error);
        }
        self.audit(
            &mut tx,
            ctx.request,
            ctx.scope,
            ctx.record,
            ctx.action,
            if error == BrokerError::Integrity {
                "integrity_failed"
            } else {
                "denied"
            },
        )
        .await?;
        tx.commit().await?;
        Err(error)
    }
    pub async fn create_connection(
        &self,
        scope: &ConnectionScope,
        request: &AdminRequest,
    ) -> Result<BrokerReceipt, BrokerError> {
        let version = SecretVersion(1);
        let commitment = crypto::commitment(
            &self.key,
            scope,
            None,
            BrokerAction::CreateConnection,
            version,
            request,
            None,
        );
        let ctx = AdminContext {
            request,
            scope,
            record: None,
            action: BrokerAction::CreateConnection,
            version,
            commitment: &commitment,
        };
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Err(e) = self.authorize(&mut tx, &ctx).await {
            tx.commit().await?;
            return Err(e);
        }
        if let Some(row) = connection(&mut tx, scope).await? {
            if row.try_get::<String, _>("state")? != "active" {
                return Err(BrokerError::Revoked);
            }
            if let Some(receipt) =
                replay(&self.key, &mut tx, scope, &request.request_id, &commitment).await?
            {
                self.human.check(&ctx)?;
                tx.commit().await?;
                return Ok(receipt);
            }
            return Err(BrokerError::StaleVersion);
        }
        ensure_unused(&mut tx, scope, &request.request_id).await?;
        sqlx::query("INSERT INTO secret_connections VALUES(?,?,?,?,?,1,'active')")
            .bind(scope.project.as_str())
            .bind(scope.connection.as_str())
            .bind(scope.account.as_str())
            .bind(scope.provider.as_str())
            .bind(scope.mode.name())
            .execute(&mut *tx)
            .await?;
        let event = self
            .audit(
                &mut tx,
                request,
                scope,
                None,
                BrokerAction::CreateConnection,
                "accepted",
            )
            .await?;
        remember(
            &self.key,
            &mut tx,
            scope,
            &request.request_id,
            &commitment,
            event,
            version,
        )
        .await?;
        self.human.check(&ctx)?;
        self.checkpoint(Checkpoint::BeforeCommit)?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterCommit)?;
        Ok(BrokerReceipt {
            event_sequence: event,
            version,
        })
    }
    pub async fn put_secret(
        &self,
        r: &PutSecret,
        value: SecretValue,
    ) -> Result<BrokerReceipt, BrokerError> {
        let scope = &r.scope.connection;
        let action = if r.expected.is_some() {
            BrokerAction::Replace
        } else {
            BrokerAction::Store
        };
        let version = r
            .expected
            .map_or(Ok(SecretVersion(1)), SecretVersion::next)?;
        let commitment = crypto::commitment(
            &self.key,
            scope,
            Some(&r.scope),
            action,
            version,
            &r.administration,
            Some(&value),
        );
        let ctx = AdminContext {
            request: &r.administration,
            scope,
            record: Some(&r.scope),
            action,
            version,
            commitment: &commitment,
        };
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Err(e) = self.authorize(&mut tx, &ctx).await {
            tx.commit().await?;
            return Err(e);
        }
        active(&mut tx, scope).await?;
        let head = head(&mut tx, &r.scope).await?;
        if let Some(receipt) = replay(
            &self.key,
            &mut tx,
            scope,
            &r.administration.request_id,
            &commitment,
        )
        .await?
        {
            if head != Some(receipt.version) {
                return Err(BrokerError::StaleVersion);
            }
            self.human.check(&ctx)?;
            tx.commit().await?;
            return Ok(receipt);
        }
        if head != r.expected {
            return Err(BrokerError::StaleVersion);
        }
        self.checkpoint(Checkpoint::BeforeEncryption)?;
        let envelope = crypto::seal(
            &self.key,
            &r.scope,
            version,
            &value,
            self.random()?,
            *self.random()?,
            *self.random()?,
        )?;
        self.checkpoint(Checkpoint::AfterEncryption)?;
        if r.expected.is_none() {
            sqlx::query("INSERT INTO secret_records VALUES(?,?,?,?,?)")
                .bind(scope.project.as_str())
                .bind(scope.connection.as_str())
                .bind(r.scope.record.as_str())
                .bind(r.scope.purpose.as_str())
                .bind(version.0)
                .execute(&mut *tx)
                .await?;
        }
        insert(&mut tx, &envelope).await?;
        if let Some(expected) = r.expected {
            let n=sqlx::query("UPDATE secret_records SET current_version=? WHERE project_id=? AND connection_id=? AND record_id=? AND current_version=?")
                .bind(version.0).bind(scope.project.as_str()).bind(scope.connection.as_str()).bind(r.scope.record.as_str()).bind(expected.0).execute(&mut *tx).await?.rows_affected();
            if n != 1 {
                return Err(BrokerError::StaleVersion);
            }
        }
        if let Err(e) = self.human.check(&ctx) {
            tx.rollback().await?;
            let tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
            return self.denied(tx, &ctx, e).await;
        }
        let event = self
            .audit(
                &mut tx,
                &r.administration,
                scope,
                Some(&r.scope),
                action,
                "accepted",
            )
            .await?;
        remember(
            &self.key,
            &mut tx,
            scope,
            &r.administration.request_id,
            &commitment,
            event,
            version,
        )
        .await?;
        self.checkpoint(Checkpoint::BeforeCommit)?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterCommit)?;
        Ok(BrokerReceipt {
            event_sequence: event,
            version,
        })
    }
    pub async fn revoke_connection(
        &self,
        scope: &ConnectionScope,
        expected_revision: SecretVersion,
        request: &AdminRequest,
    ) -> Result<BrokerReceipt, BrokerError> {
        let commitment = crypto::commitment(
            &self.key,
            scope,
            None,
            BrokerAction::Revoke,
            expected_revision,
            request,
            None,
        );
        let ctx = AdminContext {
            request,
            scope,
            record: None,
            action: BrokerAction::Revoke,
            version: expected_revision,
            commitment: &commitment,
        };
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Err(e) = self.authorize(&mut tx, &ctx).await {
            tx.commit().await?;
            return Err(e);
        }
        let row = connection(&mut tx, scope)
            .await?
            .ok_or(BrokerError::NotFound)?;
        if let Some(receipt) =
            replay(&self.key, &mut tx, scope, &request.request_id, &commitment).await?
        {
            if row.try_get::<String, _>("state")? != "revoked" {
                return Err(BrokerError::Integrity);
            }
            self.human.check(&ctx)?;
            tx.commit().await?;
            return Ok(receipt);
        }
        if row.try_get::<String, _>("state")? != "active" {
            return Err(BrokerError::Revoked);
        }
        if row.try_get::<i64, _>("revision")? != expected_revision.0 {
            return Err(BrokerError::StaleVersion);
        }
        sqlx::query("UPDATE secret_connections SET state='revoked',revision=revision+1 WHERE project_id=? AND connection_id=? AND revision=? AND state='active'")
            .bind(scope.project.as_str()).bind(scope.connection.as_str()).bind(expected_revision.0).execute(&mut *tx).await?;
        let result = expected_revision.next()?;
        let event = self
            .audit(
                &mut tx,
                request,
                scope,
                None,
                BrokerAction::Revoke,
                "accepted",
            )
            .await?;
        remember(
            &self.key,
            &mut tx,
            scope,
            &request.request_id,
            &commitment,
            event,
            result,
        )
        .await?;
        self.human.check(&ctx)?;
        tx.commit().await?;
        Ok(BrokerReceipt {
            event_sequence: event,
            version: result,
        })
    }
    pub async fn metadata(
        &self,
        scope: &RecordScope,
        request: &AdminRequest,
    ) -> Result<SecretMetadata, BrokerError> {
        let commitment = crypto::commitment(
            &self.key,
            &scope.connection,
            Some(scope),
            BrokerAction::ReadMetadata,
            SecretVersion(1),
            request,
            None,
        );
        let ctx = AdminContext {
            request,
            scope: &scope.connection,
            record: Some(scope),
            action: BrokerAction::ReadMetadata,
            version: SecretVersion(1),
            commitment: &commitment,
        };
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Err(e) = self.authorize(&mut tx, &ctx).await {
            tx.commit().await?;
            return Err(e);
        }
        let row = connection(&mut tx, &scope.connection)
            .await?
            .ok_or(BrokerError::NotFound)?;
        let version = head(&mut tx, scope).await?.ok_or(BrokerError::NotFound)?;
        let revoked = row.try_get::<String, _>("state")? == "revoked";
        self.human.check(&ctx)?;
        tx.commit().await?;
        Ok(SecretMetadata {
            scope: scope.clone(),
            version,
            revoked,
        })
    }
    pub async fn verify_integrity(
        &self,
        scope: &RecordScope,
        version: SecretVersion,
        request: &AdminRequest,
    ) -> Result<BrokerReceipt, BrokerError> {
        let commitment = crypto::commitment(
            &self.key,
            &scope.connection,
            Some(scope),
            BrokerAction::VerifyIntegrity,
            version,
            request,
            None,
        );
        let ctx = AdminContext {
            request,
            scope: &scope.connection,
            record: Some(scope),
            action: BrokerAction::VerifyIntegrity,
            version,
            commitment: &commitment,
        };
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Err(e) = self.authorize(&mut tx, &ctx).await {
            tx.commit().await?;
            return Err(e);
        }
        active(&mut tx, &scope.connection).await?;
        if head(&mut tx, scope).await? != Some(version) {
            return Err(BrokerError::StaleVersion);
        }
        let envelope = match load(&mut tx, scope, version).await {
            Ok(e) => e,
            Err(e) => return self.denied(tx, &ctx, e).await,
        };
        if let Err(error) = crypto::open(&self.key, &envelope) {
            return self.denied(tx, &ctx, error).await;
        }
        self.human.check(&ctx)?;
        if let Some(receipt) = replay(
            &self.key,
            &mut tx,
            &scope.connection,
            &request.request_id,
            &commitment,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }
        let event = self
            .audit(
                &mut tx,
                request,
                &scope.connection,
                Some(scope),
                BrokerAction::VerifyIntegrity,
                "accepted",
            )
            .await?;
        remember(
            &self.key,
            &mut tx,
            &scope.connection,
            &request.request_id,
            &commitment,
            event,
            version,
        )
        .await?;
        tx.commit().await?;
        Ok(BrokerReceipt {
            event_sequence: event,
            version,
        })
    }
    pub async fn use_secret(&self, r: &SecretUseRequest) -> Result<BrokerReceipt, BrokerError> {
        let admin = AdminRequest {
            actor: PrincipalId::new("trusted-broker-operation")
                .map_err(|_| BrokerError::InvalidInput)?,
            request_id: r.request_id.clone(),
        };
        let mut commitment = crypto::commitment(
            &self.key,
            &r.scope.connection,
            Some(&r.scope),
            BrokerAction::Use,
            r.version,
            &admin,
            None,
        );
        // Bind every operation and attempt, rather than allowing an old receipt
        // to acknowledge a different worker action under the same request ID.
        commitment = crypto::use_commitment(&self.key, &commitment, r)?;
        let ctx = AdminContext {
            request: &admin,
            scope: &r.scope.connection,
            record: Some(&r.scope),
            action: BrokerAction::Use,
            version: r.version,
            commitment: &commitment,
        };
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let now = self.now()?;
        if let Err(e) = self.use_authority(&mut tx, r, now).await {
            return self.denied(tx, &ctx, e).await;
        }
        if let Some(receipt) = replay(
            &self.key,
            &mut tx,
            &r.scope.connection,
            &r.request_id,
            &commitment,
        )
        .await?
        {
            // Receipt lookup is asynchronous: elapsed time or external authority
            // revocation must not turn a cached success into renewed privilege.
            if let Err(e) = self.use_authority(&mut tx, r, self.now()?).await {
                return self.denied(tx, &ctx, e).await;
            }
            tx.commit().await?;
            return Ok(receipt);
        }
        let envelope = match load(&mut tx, &r.scope, r.version).await {
            Ok(e) => e,
            Err(e) => return self.denied(tx, &ctx, e).await,
        };
        let value = match crypto::open(&self.key, &envelope) {
            Ok(v) => v,
            Err(e) => return self.denied(tx, &ctx, e).await,
        };
        if let Err(e) = self.use_authority(&mut tx, r, self.now()?).await {
            return self.denied(tx, &ctx, e).await;
        }
        // No network/provider adapter exists. Only a sealed, bounded in-process
        // consumer may run here. Plaintext never leaves this private call boundary.
        self.consumer.consume(r, &value)?;
        drop(value);
        let event = self
            .audit(
                &mut tx,
                &admin,
                &r.scope.connection,
                Some(&r.scope),
                BrokerAction::Use,
                "accepted",
            )
            .await?;
        remember(
            &self.key,
            &mut tx,
            &r.scope.connection,
            &r.request_id,
            &commitment,
            event,
            r.version,
        )
        .await?;
        self.checkpoint(Checkpoint::BeforeCommit)?;
        if let Err(e) = self.use_authority(&mut tx, r, self.now()?).await {
            // Roll back the provisional accepted event and replay receipt before
            // durably recording denial in a separate, short transaction.
            tx.rollback().await?;
            let tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
            return self.denied(tx, &ctx, e).await;
        }
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterCommit)?;
        Ok(BrokerReceipt {
            event_sequence: event,
            version: r.version,
        })
    }
    async fn use_authority(
        &self,
        c: &mut SqliteConnection,
        r: &SecretUseRequest,
        now: i64,
    ) -> Result<(), BrokerError> {
        self.user.check(r, now)?;
        if r.operation != CredentialOperation::LocalIntegrityInspection && r.worker.is_none() {
            return Err(BrokerError::AttemptAuthority);
        }
        self.entitlement
            .check(&r.scope.connection, r.operation, now)?;
        active(c, &r.scope.connection).await?;
        if head(c, &r.scope).await? != Some(r.version) {
            return Err(BrokerError::StaleVersion);
        }
        if let Some(w) = &r.worker {
            if w.binding.task.plan.project_id != r.scope.connection.project {
                return Err(BrokerError::AttemptAuthority);
            }
            crate::worker_control::broker_guard(
                c,
                &w.binding,
                &w.peer,
                self.worker.as_ref(),
                self.policy.as_ref(),
                now,
                r.scope.connection.connection.as_str(),
                r.scope.connection.account.as_str(),
                r.scope.connection.provider.as_str(),
                r.scope.connection.mode.name(),
            )
            .await
            .map_err(|_| BrokerError::AttemptAuthority)?;
        }
        Ok(())
    }
}
async fn connection(
    c: &mut SqliteConnection,
    s: &ConnectionScope,
) -> Result<Option<sqlx::sqlite::SqliteRow>, BrokerError> {
    // Compare identities in SQLite instead of allocating unchecked metadata from
    // a corrupted file. Only fixed state labels and integers enter Rust here.
    let row=sqlx::query("SELECT account_id=? AND provider=? AND mode=? AS scoped,CASE state WHEN 'active' THEN 'active' WHEN 'revoked' THEN 'revoked' ELSE 'invalid' END AS state,revision FROM secret_connections WHERE project_id=? AND connection_id=?")
        .bind(s.account.as_str()).bind(s.provider.as_str()).bind(s.mode.name()).bind(s.project.as_str()).bind(s.connection.as_str()).fetch_optional(c).await?;
    if let Some(r) = &row {
        if !r.try_get::<bool, _>("scoped")? {
            return Err(BrokerError::Forbidden);
        }
        if r.try_get::<String, _>("state")? == "invalid" {
            return Err(BrokerError::Integrity);
        }
    }
    Ok(row)
}
async fn active(c: &mut SqliteConnection, s: &ConnectionScope) -> Result<(), BrokerError> {
    let row = connection(c, s).await?.ok_or(BrokerError::NotFound)?;
    if row.try_get::<String, _>("state")? != "active" {
        return Err(BrokerError::Revoked);
    }
    Ok(())
}
async fn head(
    c: &mut SqliteConnection,
    s: &RecordScope,
) -> Result<Option<SecretVersion>, BrokerError> {
    let r=sqlx::query("SELECT purpose=? AS scoped,current_version FROM secret_records WHERE project_id=? AND connection_id=? AND record_id=?")
        .bind(s.purpose.as_str()).bind(s.connection.project.as_str()).bind(s.connection.connection.as_str()).bind(s.record.as_str()).fetch_optional(c).await?;
    if let Some(r) = r {
        if !r.try_get::<bool, _>(0)? {
            return Err(BrokerError::Forbidden);
        }
        return Ok(Some(SecretVersion::new(
            u64::try_from(r.try_get::<i64, _>(1)?).map_err(|_| BrokerError::Integrity)?,
        )?));
    }
    Ok(None)
}
async fn replay(
    key: &ControllerKey,
    c: &mut SqliteConnection,
    s: &ConnectionScope,
    id: &RequestId,
    commitment: &[u8; 32],
) -> Result<Option<BrokerReceipt>, BrokerError> {
    let row=sqlx::query("SELECT substr(CAST(commitment AS BLOB),1,33),event_sequence,result_version,substr(CAST(commitment_key_id AS BLOB),1,17),commitment_key_version FROM secret_requests WHERE project_id=? AND request_id=?")
        .bind(s.project.as_str()).bind(id.as_str()).fetch_optional(c).await?;
    if let Some(row) = row {
        if row.try_get::<Vec<u8>, _>(3)?.as_slice() != key.metadata.key.0
            || row.try_get::<i64, _>(4)? != key.metadata.key_version as i64
        {
            return Err(BrokerError::UnsupportedVersion);
        }
        if !bool::from(
            row.try_get::<Vec<u8>, _>(0)?
                .as_slice()
                .ct_eq(commitment.as_slice()),
        ) {
            return Err(BrokerError::ReplayConflict);
        }
        return Ok(Some(BrokerReceipt {
            event_sequence: row.try_get(1)?,
            version: SecretVersion::new(
                u64::try_from(row.try_get::<i64, _>(2)?).map_err(|_| BrokerError::Integrity)?,
            )?,
        }));
    }
    Ok(None)
}
async fn ensure_unused(
    c: &mut SqliteConnection,
    s: &ConnectionScope,
    id: &RequestId,
) -> Result<(), BrokerError> {
    let n: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM secret_requests WHERE project_id=? AND request_id=?)",
    )
    .bind(s.project.as_str())
    .bind(id.as_str())
    .fetch_one(c)
    .await?;
    if n {
        return Err(BrokerError::ReplayConflict);
    }
    Ok(())
}
async fn remember(
    key: &ControllerKey,
    c: &mut SqliteConnection,
    s: &ConnectionScope,
    id: &RequestId,
    commitment: &[u8; 32],
    event: i64,
    version: SecretVersion,
) -> Result<(), BrokerError> {
    sqlx::query("INSERT INTO secret_requests VALUES(?,?,?,?,?,?,?)")
        .bind(s.project.as_str())
        .bind(id.as_str())
        .bind(commitment.as_slice())
        .bind(key.metadata.key.0.as_slice())
        .bind(key.metadata.key_version as i64)
        .bind(event)
        .bind(version.0)
        .execute(c)
        .await?;
    Ok(())
}
async fn insert(c: &mut SqliteConnection, e: &Envelope) -> Result<(), BrokerError> {
    let s = &e.scope;
    sqlx::query("INSERT INTO secret_versions VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(s.connection.project.as_str())
        .bind(s.connection.connection.as_str())
        .bind(s.record.as_str())
        .bind(s.purpose.as_str())
        .bind(s.connection.account.as_str())
        .bind(s.connection.provider.as_str())
        .bind(s.connection.mode.name())
        .bind(e.version.0)
        .bind(i64::from(e.envelope_version))
        .bind(i64::from(e.cipher))
        .bind(e.origin.installation.0.as_slice())
        .bind(e.origin.key.0.as_slice())
        .bind(e.origin.key_version as i64)
        .bind(e.wrapping.key.0.as_slice())
        .bind(e.wrapping.key_version as i64)
        .bind(e.data_nonce.as_slice())
        .bind(e.wrap_nonce.as_slice())
        .bind(&e.wrapped)
        .bind(&e.ciphertext)
        .execute(c)
        .await?;
    Ok(())
}
pub(super) async fn load(
    c: &mut SqliteConnection,
    s: &RecordScope,
    version: SecretVersion,
) -> Result<Envelope, BrokerError> {
    preflight(c, s, version).await?;
    let r=sqlx::query("SELECT * FROM secret_versions WHERE project_id=? AND connection_id=? AND record_id=? AND version=?")
        .bind(s.connection.project.as_str()).bind(s.connection.connection.as_str()).bind(s.record.as_str()).bind(version.0).fetch_one(c).await?;
    if r.try_get::<String, _>("account_id")? != s.connection.account.as_str()
        || r.try_get::<String, _>("provider")? != s.connection.provider.as_str()
        || r.try_get::<String, _>("mode")? != s.connection.mode.name()
        || r.try_get::<String, _>("purpose")? != s.purpose.as_str()
    {
        return Err(BrokerError::Integrity);
    }
    let origin = crypto::metadata(
        r.try_get("installation_id")?,
        r.try_get("origin_key_id")?,
        r.try_get("origin_key_version")?,
    )?;
    let wrapping = crypto::metadata(
        r.try_get("installation_id")?,
        r.try_get("wrap_key_id")?,
        r.try_get("wrap_key_version")?,
    )?;
    Ok(Envelope {
        scope: s.clone(),
        version,
        envelope_version: u16::try_from(r.try_get::<i64, _>("envelope_version")?)
            .map_err(|_| BrokerError::UnsupportedVersion)?,
        cipher: u8::try_from(r.try_get::<i64, _>("cipher")?)
            .map_err(|_| BrokerError::UnsupportedVersion)?,
        origin,
        wrapping,
        data_nonce: r
            .try_get::<Vec<u8>, _>("data_nonce")?
            .try_into()
            .map_err(|_| BrokerError::Integrity)?,
        wrap_nonce: r
            .try_get::<Vec<u8>, _>("wrap_nonce")?
            .try_into()
            .map_err(|_| BrokerError::Integrity)?,
        wrapped: r.try_get("wrapped_dek")?,
        ciphertext: r.try_get("ciphertext")?,
    })
}

pub(super) async fn preflight(
    c: &mut SqliteConnection,
    s: &RecordScope,
    version: SecretVersion,
) -> Result<(), BrokerError> {
    // Bound SQL blob allocations before reading encrypted bytes, even if a damaged
    // database was written with CHECK constraints deliberately disabled.
    let lengths=sqlx::query("SELECT length(installation_id),length(origin_key_id),length(wrap_key_id),length(data_nonce),length(wrap_nonce),length(wrapped_dek),length(ciphertext),length(CAST(account_id AS BLOB)),length(CAST(provider AS BLOB)),length(CAST(mode AS BLOB)),length(CAST(purpose AS BLOB)) FROM secret_versions WHERE project_id=? AND connection_id=? AND record_id=? AND version=?")
        .bind(s.connection.project.as_str()).bind(s.connection.connection.as_str()).bind(s.record.as_str()).bind(version.0).fetch_optional(&mut *c).await?.ok_or(BrokerError::NotFound)?;
    let expected = [16i64, 16, 16, 24, 24, 48];
    for (i, n) in expected.iter().enumerate() {
        if lengths.try_get::<i64, _>(i)? != *n {
            return Err(BrokerError::Integrity);
        }
    }
    if !(16..=MAX_SECRET_BYTES as i64 + 16).contains(&lengths.try_get::<i64, _>(6)?) {
        return Err(BrokerError::Integrity);
    }
    for i in 7..11 {
        if !(1..=64).contains(&lengths.try_get::<i64, _>(i)?) {
            return Err(BrokerError::Integrity);
        }
    }
    Ok(())
}
