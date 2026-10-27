use super::proof::{IdentityVersions, Proof};
use super::*;
impl HumanAuth {
    fn family_binding(&self, r: &SqliteRow) -> Result<[u8; 32], AuthError> {
        Ok(self.mac(
            b"session-family",
            &[
                r.try_get::<&str, _>("session_id")?.as_bytes(),
                r.try_get::<&str, _>("user_id")?.as_bytes(),
                r.try_get::<&str, _>("source_evidence")?.as_bytes(),
                &r.try_get::<i64, _>("security_epoch")?.to_be_bytes(),
                &r.try_get::<i64, _>("email_version")?.to_be_bytes(),
                &r.try_get::<i64, _>("credential_version")?.to_be_bytes(),
                &r.try_get::<i64, _>("policy_version")?.to_be_bytes(),
                r.try_get::<&str, _>("method")?.as_bytes(),
                r.try_get::<&str, _>("assurance")?.as_bytes(),
                &r.try_get::<Vec<u8>, _>("browser")?,
                r.try_get::<&str, _>("device")?.as_bytes(),
                &r.try_get::<i64, _>("created_at")?.to_be_bytes(),
                &r.try_get::<i64, _>("absolute_expires")?.to_be_bytes(),
            ],
        ))
    }
    pub(super) async fn family_check(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        r: &SqliteRow,
        now: i64,
    ) -> Result<(), AuthError> {
        if !constant_eq(
            &self.family_binding(r)?,
            &r.try_get::<Vec<u8>, _>("binding")?,
        ) || r.try_get::<Option<i64>, _>("revoked_at")?.is_some()
            || now < r.try_get::<i64, _>("observed_at")?
            || now >= r.try_get::<i64, _>("absolute_expires")?
            || now - r.try_get::<i64, _>("last_human_at")? >= HUMAN_IDLE_SECONDS
            || r.try_get::<i64, _>("absolute_expires")?
                != r.try_get::<i64, _>("created_at")? + FAMILY_SECONDS
        {
            return Err(AuthError::Denied);
        }
        self.current_identity(
            tx,
            IdentityVersions {
                user: r.try_get("user_id")?,
                method: r.try_get("method")?,
                credential: r.try_get("credential_version")?,
                epoch: r.try_get("security_epoch")?,
                email: r.try_get("email_version")?,
                policy_version: r.try_get("policy_version")?,
                factor: i64::from(r.try_get::<&str, _>("assurance")? != "primary"),
            },
        )
        .await
    }
    fn token_binding(
        &self,
        lookup: &[u8],
        family: &SqliteRow,
        generation: i64,
        issued: i64,
        expires: i64,
        csrf: &[u8],
    ) -> Result<[u8; 32], AuthError> {
        Ok(self.mac(
            b"session-generation",
            &[
                lookup,
                &self.family_binding(family)?,
                &generation.to_be_bytes(),
                &issued.to_be_bytes(),
                &expires.to_be_bytes(),
                csrf,
            ],
        ))
    }
    pub(super) async fn token_row(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        handle: &SessionHandle,
        now: i64,
    ) -> Result<SqliteRow, AuthError> {
        let lookup = self.handle_lookup(handle);
        let token = sqlx::query("SELECT * FROM human_session_tokens WHERE lookup=?")
            .bind(lookup.as_slice())
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(AuthError::Denied)?;
        let family = sqlx::query("SELECT * FROM human_sessions WHERE session_id=?")
            .bind(token.try_get::<&str, _>("session_id")?)
            .fetch_one(&mut **tx)
            .await?;
        self.family_check(tx, &family, now).await?;
        let generation = token.try_get("generation")?;
        let issued = token.try_get("issued_at")?;
        let expires = token.try_get("expires_at")?;
        let csrf: Vec<u8> = token.try_get("csrf")?;
        if now < issued
            || now >= expires
            || expires > issued.saturating_add(TOKEN_SECONDS)
            || !constant_eq(
                &self.token_binding(&lookup, &family, generation, issued, expires, &csrf)?,
                &token.try_get::<Vec<u8>, _>("binding")?,
            )
            || generation != family.try_get::<i64, _>("generation")?
        {
            return Err(AuthError::Denied);
        }
        Ok(family)
    }
    pub(super) async fn session_row(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        h: &SessionHandle,
        f: &BrowserFacts,
        _now: i64,
        mutation: bool,
    ) -> Result<SqliteRow, AuthError> {
        let now = self.now()?;
        let lookup = self.handle_lookup(h);
        if f.credential.as_ref() != Some(&lookup) {
            return Err(AuthError::Denied);
        }
        let r = self.token_row(tx, h, now).await?;
        if mutation {
            let csrf: Vec<u8> =
                sqlx::query_scalar("SELECT csrf FROM human_session_tokens WHERE lookup=?")
                    .bind(lookup.as_slice())
                    .fetch_one(&mut **tx)
                    .await?;
            if !f.csrf.as_ref().is_some_and(|tag| constant_eq(tag, &csrf)) {
                return Err(AuthError::Denied);
            }
        }
        Ok(r)
    }
    pub(super) async fn create_family(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        p: &Proof,
        a: Assurance,
        device: &str,
        request: &RequestId,
        now: i64,
    ) -> Result<SessionTicket, AuthError> {
        if now>i64::MAX-FAMILY_SECONDS || sqlx::query_scalar::<_,i64>("SELECT count(*) FROM human_sessions s WHERE user_id=? AND revoked_at IS NULL AND absolute_expires>? AND last_human_at>? AND security_epoch=? AND policy_version=? AND (method='password' OR ?) AND EXISTS(SELECT 1 FROM human_session_tokens t WHERE t.session_id=s.session_id AND t.generation=s.generation AND t.expires_at>?)").bind(&p.user).bind(now).bind(now.saturating_sub(HUMAN_IDLE_SECONDS)).bind(p.epoch).bind(p.policy).bind(self.mail_ready()).bind(now).fetch_one(&mut **tx).await?>=32 {return Err(AuthError::Denied)}
        let id = new_id()?;
        // Bind the immutable family before publication using a SQLite projection rather
        // than maintaining a second serialization of its fields.
        let projected=sqlx::query("SELECT ? AS session_id,? AS user_id,? AS source_evidence,? AS security_epoch,? AS email_version,? AS credential_version,? AS policy_version,? AS method,? AS assurance,? AS browser,? AS device,? AS created_at,? AS absolute_expires")
   .bind(&id).bind(&p.user).bind(&p.evidence).bind(p.epoch).bind(p.email).bind(p.credential).bind(p.policy).bind(&p.method).bind(a.name()).bind(&p.browser).bind(device).bind(now).bind(now+FAMILY_SECONDS).fetch_one(&mut **tx).await?;
        let tag = self.family_binding(&projected)?;
        sqlx::query("INSERT INTO human_sessions VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,1,NULL,?)")
            .bind(&id)
            .bind(&p.user)
            .bind(&p.evidence)
            .bind(p.epoch)
            .bind(p.email)
            .bind(p.credential)
            .bind(p.policy)
            .bind(&p.method)
            .bind(a.name())
            .bind(&p.browser)
            .bind(device)
            .bind(now)
            .bind(now + FAMILY_SECONDS)
            .bind(now)
            .bind(now)
            .bind(tag.as_slice())
            .execute(&mut **tx)
            .await?;
        let family = sqlx::query("SELECT * FROM human_sessions WHERE session_id=?")
            .bind(&id)
            .fetch_one(&mut **tx)
            .await?;
        let t = self.create_token(tx, &family, now).await?;
        self.session_event(tx, &p.user, Some(&id), request, "issued", now)
            .await?;
        Ok(t)
    }
    async fn create_token(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        family: &SqliteRow,
        now: i64,
    ) -> Result<SessionTicket, AuthError> {
        let handle = SessionHandle(Zeroizing::new(hex(&random::<32>()?[..])));
        let csrf = Zeroizing::new(hex(&random::<32>()?[..]));
        let lookup = self.handle_lookup(&handle);
        let csrf_tag = self.csrf_commitment(&csrf);
        let generation = family.try_get("generation")?;
        let expires = (now.saturating_add(TOKEN_SECONDS)).min(family.try_get("absolute_expires")?);
        let tag = self.token_binding(&lookup, family, generation, now, expires, &csrf_tag)?;
        sqlx::query("INSERT INTO human_session_tokens VALUES(?,?,?,?,?,?,?)")
            .bind(lookup.as_slice())
            .bind(family.try_get::<&str, _>("session_id")?)
            .bind(generation)
            .bind(now)
            .bind(expires)
            .bind(csrf_tag.as_slice())
            .bind(tag.as_slice())
            .execute(&mut **tx)
            .await?;
        Ok(SessionTicket {
            metadata: metadata(family)?,
            handle,
            csrf,
            delivery: None,
        })
    }
    pub(super) async fn original_deadline(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        h: &SessionHandle,
        r: &SqliteRow,
    ) -> Result<i64, AuthError> {
        let expires: i64 =
            sqlx::query_scalar("SELECT expires_at FROM human_session_tokens WHERE lookup=?")
                .bind(self.handle_lookup(h).as_slice())
                .fetch_one(&mut **tx)
                .await?;
        Ok(expires.min(r.try_get("absolute_expires")?).min(
            r.try_get::<i64, _>("last_human_at")?
                .saturating_add(HUMAN_IDLE_SECONDS),
        ))
    }
    async fn touch_session(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        r: &SqliteRow,
        f: &BrowserFacts,
        now: i64,
    ) -> Result<(), AuthError> {
        sqlx::query("UPDATE human_sessions SET observed_at=?,last_human_at=CASE WHEN ? THEN ? ELSE last_human_at END WHERE session_id=? AND generation=? AND revoked_at IS NULL").bind(now).bind(f.activity==Activity::Human).bind(now).bind(r.try_get::<&str,_>("session_id")?).bind(r.try_get::<i64,_>("generation")?).execute(&mut **tx).await?;
        Ok(())
    }
    pub async fn inspect_session(
        &self,
        h: &SessionHandle,
        c: &BrowserContext,
    ) -> Result<SessionMetadata, AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Read).await?;
        let r = self.session_row(&mut tx, h, &f, now, false).await?;
        let deadline = self.original_deadline(&mut tx, h, &r).await?;
        self.touch_session(&mut tx, &r, &f, now).await?;
        let mut m = metadata(&r)?;
        if f.activity == Activity::Human {
            m.last_human_at = now
        }
        self.session_checkpoint(
            c,
            Operation::Read,
            deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(m)
    }
    pub async fn renew_session(
        &self,
        h: &SessionHandle,
        c: &BrowserContext,
    ) -> Result<SessionTicket, AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Renew).await?;
        let r = self.session_row(&mut tx, h, &f, now, true).await?;
        let deadline = self.original_deadline(&mut tx, h, &r).await?;
        let generation: i64 = r.try_get("generation")?;
        if generation == i64::MAX {
            return Err(AuthError::Denied);
        }
        if generation > 1 {
            let issued: i64 =
                sqlx::query_scalar("SELECT issued_at FROM human_session_tokens WHERE lookup=?")
                    .bind(self.handle_lookup(h).as_slice())
                    .fetch_one(&mut *tx)
                    .await?;
            if now < issued.saturating_add(600) {
                return Err(AuthError::Busy);
            }
        }
        self.touch_session(&mut tx, &r, &f, now).await?;
        sqlx::query("UPDATE human_sessions SET generation=generation+1 WHERE session_id=? AND generation=? AND revoked_at IS NULL").bind(r.try_get::<&str,_>("session_id")?).bind(generation).execute(&mut *tx).await?;
        let current = sqlx::query("SELECT * FROM human_sessions WHERE session_id=?")
            .bind(r.try_get::<&str, _>("session_id")?)
            .fetch_one(&mut *tx)
            .await?;
        let ticket = self.create_token(&mut tx, &current, now).await?;
        self.session_event(
            &mut tx,
            r.try_get("user_id")?,
            Some(r.try_get("session_id")?),
            &c.request_id,
            "rotated",
            now,
        )
        .await?;
        let ticket = self.scoped_delivery(ticket, c, Operation::Renew, &f, deadline)?;
        self.session_checkpoint(
            c,
            Operation::Renew,
            deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterSessionCommit)?;
        Ok(ticket)
    }
    pub async fn session_inventory(
        &self,
        h: &SessionHandle,
        c: &BrowserContext,
        after: Option<&SessionId>,
        limit: u32,
    ) -> Result<Vec<SessionMetadata>, AuthError> {
        if limit == 0 || limit > 100 {
            return Err(AuthError::InvalidInput);
        }
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Read).await?;
        let r = self.session_row(&mut tx, h, &f, now, false).await?;
        let deadline = self.original_deadline(&mut tx, h, &r).await?;
        let rows=sqlx::query("SELECT * FROM human_sessions WHERE user_id=? AND session_id>? ORDER BY session_id LIMIT ?").bind(r.try_get::<&str,_>("user_id")?).bind(after.map(|id|id.0.as_str()).unwrap_or("")).bind(limit).fetch_all(&mut *tx).await?;
        let result = rows.iter().map(metadata).collect::<Result<Vec<_>, _>>()?;
        self.session_checkpoint(
            c,
            Operation::Read,
            deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn revoke_session(
        &self,
        h: &SessionHandle,
        target: &SessionId,
        c: &BrowserContext,
    ) -> Result<(), AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Write).await?;
        let r = self.session_row(&mut tx, h, &f, now, true).await?;
        let deadline = self.original_deadline(&mut tx, h, &r).await?;
        let user: &str = r.try_get("user_id")?;
        let target_row =
            sqlx::query("SELECT revoked_at FROM human_sessions WHERE session_id=? AND user_id=?")
                .bind(&target.0)
                .bind(user)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(AuthError::Denied)?;
        if target_row
            .try_get::<Option<i64>, _>("revoked_at")?
            .is_none()
        {
            sqlx::query("UPDATE human_sessions SET revoked_at=? WHERE session_id=? AND user_id=? AND revoked_at IS NULL").bind(now).bind(&target.0).bind(user).execute(&mut *tx).await?;
            self.block_family_proofs(&mut tx, &target.0, now).await?;
            self.session_event(
                &mut tx,
                user,
                Some(&target.0),
                &c.request_id,
                "revoked",
                now,
            )
            .await?;
        }
        self.session_checkpoint(
            c,
            Operation::Write,
            deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn logout(&self, h: &SessionHandle, c: &BrowserContext) -> Result<(), AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Write).await?;
        let r = self.session_row(&mut tx, h, &f, now, true).await?;
        let deadline = self.original_deadline(&mut tx, h, &r).await?;
        let id: &str = r.try_get("session_id")?;
        sqlx::query(
            "UPDATE human_sessions SET revoked_at=? WHERE session_id=? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        self.block_family_proofs(&mut tx, id, now).await?;
        self.session_event(
            &mut tx,
            r.try_get("user_id")?,
            Some(id),
            &c.request_id,
            "revoked",
            now,
        )
        .await?;
        self.session_checkpoint(
            c,
            Operation::Write,
            deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn revoke_all_sessions(
        &self,
        h: &SessionHandle,
        c: &BrowserContext,
    ) -> Result<(), AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Write).await?;
        let r = self.session_row(&mut tx, h, &f, now, true).await?;
        let deadline = self.original_deadline(&mut tx, h, &r).await?;
        let user: &str = r.try_get("user_id")?;
        self.revoke_user_sessions(&mut tx, user, now).await?;
        self.session_event(
            &mut tx,
            user,
            Some(r.try_get("session_id")?),
            &c.request_id,
            "revoked_all",
            now,
        )
        .await?;
        self.session_checkpoint(
            c,
            Operation::Write,
            deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(())
    }
    pub(super) async fn revoke_user_sessions(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        user: &str,
        now: i64,
    ) -> Result<(), AuthError> {
        sqlx::query("INSERT OR IGNORE INTO human_primary_blocks SELECT c.evidence_id,? FROM human_primary_context c JOIN human_authentications a USING(evidence_id) WHERE a.user_id=?").bind(now).bind(user).execute(&mut **tx).await?;
        sqlx::query(
            "UPDATE human_sessions SET revoked_at=? WHERE user_id=? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(user)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
    async fn block_family_proofs(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        session: &str,
        now: i64,
    ) -> Result<(), AuthError> {
        sqlx::query("INSERT OR IGNORE INTO human_primary_blocks SELECT c.evidence_id,? FROM human_primary_context c JOIN human_authentications a USING(evidence_id) JOIN human_sessions s ON s.session_id=? WHERE a.user_id=s.user_id AND (c.parent_session=s.session_id OR c.browser=s.browser)").bind(now).bind(session).execute(&mut **tx).await?;
        Ok(())
    }
    pub async fn contain_compromised_session(
        &self,
        h: &SessionHandle,
        e: &IncidentEvidence,
    ) -> Result<(), AuthError> {
        let now = self.now()?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let r=sqlx::query("SELECT s.*,t.generation AS presented_generation FROM human_sessions s JOIN human_session_tokens t USING(session_id) WHERE t.lookup=?").bind(self.handle_lookup(h).as_slice()).fetch_optional(&mut *tx).await?.ok_or(AuthError::Denied)?;
        if !constant_eq(
            &self.family_binding(&r)?,
            &r.try_get::<Vec<u8>, _>("binding")?,
        ) {
            return Err(AuthError::Denied);
        }
        let context = IncidentContext {
            evidence: e,
            user: r.try_get("user_id")?,
            session: r.try_get("session_id")?,
            generation: r.try_get("presented_generation")?,
            now,
        };
        self.incident_authority.verify(&context)?;
        if r.try_get::<Option<i64>, _>("revoked_at")?.is_none() {
            sqlx::query(
                "UPDATE human_sessions SET revoked_at=? WHERE session_id=? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(context.session)
            .execute(&mut *tx)
            .await?;
            self.block_family_proofs(&mut tx, context.session, now)
                .await?;
            self.session_event(
                &mut tx,
                context.user,
                Some(context.session),
                &e.request_id,
                "incident",
                now,
            )
            .await?;
        }
        self.incident_authority.verify(&IncidentContext {
            now: self.now()?,
            ..context
        })?;
        tx.commit().await?;
        Ok(())
    }
}
