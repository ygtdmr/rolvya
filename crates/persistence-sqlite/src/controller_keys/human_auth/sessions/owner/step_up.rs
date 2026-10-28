use super::*;

/// Request data, not an authorization capability. All fields are rechecked.
pub struct OwnerProofRequest<'a> {
    pub session: &'a SessionHandle,
    pub project: &'a ProjectId,
    pub action: &'a OwnerActionId,
    pub confirmation: Confirmation,
    pub context: &'a BrowserContext,
}
impl HumanAuth {
    pub(super) async fn proof_scope<'a>(
        &'a self,
        q: &OwnerProofRequest<'_>,
    ) -> Result<(Transaction<'a, Sqlite>, BrowserFacts, i64, Scope, SqliteRow), AuthError> {
        if q.confirmation != Confirmation::Affirmative {
            return Err(AuthError::Denied);
        }
        let (mut tx, f, now) = self
            .browser_transaction(q.context, Operation::Write)
            .await?;
        if f.activity != Activity::Human {
            return Err(AuthError::Denied);
        }
        let s = self
            .project_scope(
                &mut tx,
                q.session,
                &f,
                q.project,
                ProjectPermission::Govern,
                now,
            )
            .await?;
        let a = self.action_in(&mut tx, q.action, &s, now).await?;
        // A verified request identity has one verification outcome. Replays are
        // denied before comparing another secret, so an idempotent audit key
        // cannot roll back guessing counters and create a fresh oracle.
        let used:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM human_governance_events WHERE project_id=? AND request_id=? AND event IN ('proof_denied','fresh_proof'))").bind(q.project.as_str()).bind(q.context.request_id.as_str()).fetch_one(&mut *tx).await?;
        if used {
            return Err(AuthError::Denied);
        }
        if sqlx::query_scalar::<_, i64>("SELECT count(*) FROM human_owner_proofs WHERE action_id=?")
            .bind(&q.action.0)
            .fetch_one(&mut *tx)
            .await?
            != 0
        {
            return Err(AuthError::Conflict);
        }
        Ok((tx, f, now, s, a))
    }
    pub(super) fn proof_binding(
        &self,
        a: &SqliteRow,
        p: &SqliteRow,
    ) -> Result<[u8; 32], AuthError> {
        let factor: Option<String> = p.try_get("factor_id")?;
        let step: Option<i64> = p.try_get("factor_step")?;
        Ok(self.mac(
            b"owner-exact-proof",
            &[
                &self.action_binding(a)?,
                p.try_get::<&str, _>("method")?.as_bytes(),
                &p.try_get::<i64, _>("credential_version")?.to_be_bytes(),
                factor.as_deref().unwrap_or("").as_bytes(),
                &step.unwrap_or(-1).to_be_bytes(),
                &p.try_get::<i64, _>("verified_at")?.to_be_bytes(),
                &p.try_get::<i64, _>("expires_at")?.to_be_bytes(),
            ],
        ))
    }
    pub(super) async fn fresh_factor(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        s: &Scope,
        f: &BrowserFacts,
        code: Option<&TotpCode>,
        now: i64,
    ) -> Result<Option<(String, i64)>, AuthError> {
        let required: i64 =
            sqlx::query_scalar("SELECT factor_required FROM human_users WHERE user_id=?")
                .bind(s.user()?)
                .fetch_one(&mut **tx)
                .await?;
        if required == 0 {
            return if code.is_none() {
                Ok(Some((String::new(), -1)))
            } else {
                Ok(None)
            };
        }
        if !self.factor_rate(tx, s.user()?, f, now).await? {
            return Ok(None);
        }
        let r = sqlx::query("SELECT * FROM human_factors WHERE user_id=? AND state='active'")
            .bind(s.user()?)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(AuthError::Denied)?;
        let step = if let Some(code) = code {
            super::super::factors::matching_step(
                &self.factor_seed(&r, s.user()?)?,
                code,
                now,
                r.try_get("last_step")?,
            )?
        } else {
            None
        };
        self.record_factor_attempt(tx, s.user()?, f, now, step.is_some())
            .await?;
        if let Some(step) = step {
            let id: String = r.try_get("factor_id")?;
            let updated=sqlx::query("UPDATE human_factors SET last_step=? WHERE factor_id=? AND last_step<? AND state='active'").bind(step).bind(&id).bind(step).execute(&mut **tx).await?;
            if updated.rows_affected() != 1 {
                return Err(AuthError::Denied);
            }
            return Ok(Some((id, step)));
        }
        Ok(None)
    }
    pub(super) async fn record_owner_proof(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        a: &SqliteRow,
        method: &str,
        version: i64,
        factor: Option<&(String, i64)>,
        now: i64,
    ) -> Result<(), AuthError> {
        let factor = factor.filter(|(id, _)| !id.is_empty());
        let projected=sqlx::query("SELECT ? AS method,? AS credential_version,? AS factor_id,? AS factor_step,? AS verified_at,? AS expires_at")
            .bind(method).bind(version).bind(factor.map(|(id,_)|id.as_str())).bind(factor.map(|(_,step)|*step)).bind(now).bind(now+300).fetch_one(&mut **tx).await?;
        let tag = self.proof_binding(a, &projected)?;
        one(
            sqlx::query("INSERT INTO human_owner_proofs VALUES(?,?,?,?,?,?,?,?)")
                .bind(a.try_get::<&str, _>("action_id")?)
                .bind(method)
                .bind(version)
                .bind(factor.map(|(id, _)| id.as_str()))
                .bind(factor.map(|(_, step)| *step))
                .bind(now)
                .bind(now + 300)
                .bind(tag.as_slice())
                .execute(&mut **tx)
                .await?
                .rows_affected(),
        )?;
        Ok(())
    }
    /// Verifies the actual current password anew. Ordinary PrimaryAuthentication
    /// objects are deliberately not accepted by this API.
    pub async fn prove_owner_password(
        &self,
        q: &OwnerProofRequest<'_>,
        input: &Password,
        client: &ClientContext,
        code: Option<&TotpCode>,
    ) -> Result<(), AuthError> {
        let (mut tx, f, now, s, _) = self.proof_scope(q).await?;
        let peer = self
            .transport
            .verify(client, TransportOperation::Password, now)?;
        if peer.browser != f.browser {
            return Err(AuthError::Denied);
        }
        let p = policy(&mut tx).await?;
        if !p.1.local_method {
            return Err(AuthError::Denied);
        }
        let row=sqlx::query("SELECT p.*,e.address FROM human_passwords p JOIN human_emails e USING(user_id) WHERE p.user_id=?").bind(s.user()?).fetch_optional(&mut *tx).await?.ok_or(AuthError::Denied)?;
        let version: i64 = row.try_get("version")?;
        let phc: String = row.try_get("phc")?;
        if !constant_eq(
            &self.password_binding(s.user()?, version, &phc),
            &row.try_get::<Vec<u8>, _>("binding")?,
        ) {
            return Err(AuthError::Denied);
        }
        let account = self.mac(b"account", &[row.try_get::<&str, _>("address")?.as_bytes()]);
        let ip = self.mac(b"client", &[peer.ip.to_string().as_bytes()]);
        let attempt = self
            .reserve_attempt(&mut tx, "password", &account, &ip, now)
            .await?
            .ok_or(AuthError::Denied)?;
        tx.commit().await?;
        let valid = self.passwords.verify(input, Some(&phc)).await?;
        let (mut tx, f, end, s, a) = self.proof_scope(q).await?;
        let current =
            sqlx::query("SELECT * FROM human_passwords WHERE user_id=? AND version=? AND phc=?")
                .bind(s.user()?)
                .bind(version)
                .bind(&phc)
                .fetch_optional(&mut *tx)
                .await?;
        let factor = if valid && current.is_some() && end >= now && end < deadline(now, 60)? {
            self.fresh_factor(&mut tx, &s, &f, code, end).await?
        } else {
            None
        };
        let peer_end = self
            .transport
            .verify(client, TransportOperation::Password, end)?;
        if peer_end.ip != peer.ip
            || peer_end.browser != peer.browser
            || peer_end.parent != peer.parent
        {
            return Err(AuthError::Denied);
        }
        sqlx::query("UPDATE human_attempts SET state=? WHERE attempt_id=? AND state='pending'")
            .bind(if factor.is_some() {
                "succeeded"
            } else {
                "failed"
            })
            .bind(attempt)
            .execute(&mut *tx)
            .await?;
        let good = factor.is_some();
        if good {
            self.record_owner_proof(&mut tx, &a, "password", version, factor.as_ref(), end)
                .await?;
        }
        self.governance_event(
            &mut tx,
            q.project,
            s.user()?,
            &q.context.request_id,
            GovernanceAudit {
                kind: if good { "fresh_proof" } else { "proof_denied" },
                reference: &q.action.0,
                at: end,
            },
        )
        .await?;
        self.session_checkpoint(
            q.context,
            Operation::Write,
            s.deadline
                .min(deadline(now, 60)?)
                .min(a.try_get::<i64, _>("expires_at")?),
            Checkpoint::BeforeOwnerCommit,
            (&f, end),
        )?;
        self.owner_transport_current(client, TransportOperation::Password, &peer)?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterOwnerCommit)?;
        if good { Ok(()) } else { Err(AuthError::Denied) }
    }
}

impl HumanAuth {
    pub(super) async fn owner_credentials_current(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        s: &Scope,
        p: &SqliteRow,
    ) -> Result<(), AuthError> {
        let version: i64 = p.try_get("credential_version")?;
        match p.try_get::<&str, _>("method")? {
            "password" => {
                let r = sqlx::query("SELECT * FROM human_passwords WHERE user_id=? AND version=?")
                    .bind(s.user()?)
                    .bind(version)
                    .fetch_optional(&mut **tx)
                    .await?
                    .ok_or(AuthError::Denied)?;
                let phc: &str = r.try_get("phc")?;
                if !self.passwords.valid_phc(phc)
                    || !constant_eq(
                        &self.password_binding(s.user()?, version, phc),
                        &r.try_get::<Vec<u8>, _>("binding")?,
                    )
                {
                    return Err(AuthError::Denied);
                }
            }
            "magic_link" | "otp" => {
                self.email_only_route(tx, s).await?;
                if s.session.try_get::<i64, _>("email_version")? != version {
                    return Err(AuthError::Denied);
                }
            }
            _ => return Err(AuthError::Denied),
        }
        let required: i64 =
            sqlx::query_scalar("SELECT factor_required FROM human_users WHERE user_id=?")
                .bind(s.user()?)
                .fetch_one(&mut **tx)
                .await?;
        let factor: Option<&str> = p.try_get("factor_id")?;
        if required == 1 {
            let factor = factor.ok_or(AuthError::Denied)?;
            let step: Option<i64> = p.try_get("factor_step")?;
            let step = step.ok_or(AuthError::Denied)?;
            let r=sqlx::query("SELECT * FROM human_factors WHERE factor_id=? AND user_id=? AND state='active' AND last_step>=?").bind(factor).bind(s.user()?).bind(step).fetch_optional(&mut **tx).await?.ok_or(AuthError::Denied)?;
            // Immutable encrypted factor bytes must still authenticate; a signed
            // five-minute proof is not a cache that overrides corrupt credentials.
            let _seed = self.factor_seed(&r, s.user()?)?;
        } else if factor.is_some() {
            return Err(AuthError::Denied);
        }
        Ok(())
    }
}

impl HumanAuth {
    pub(super) fn owner_transport_current(
        &self,
        client: &ClientContext,
        op: TransportOperation,
        expected: &VerifiedTransport,
    ) -> Result<(), AuthError> {
        let current = self.transport.verify(client, op, self.now()?)?;
        if current.ip != expected.ip
            || current.browser != expected.browser
            || current.parent != expected.parent
        {
            return Err(AuthError::Denied);
        }
        Ok(())
    }
}
