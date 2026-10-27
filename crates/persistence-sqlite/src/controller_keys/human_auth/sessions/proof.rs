use super::*;
pub(super) struct Proof {
    pub evidence: String,
    pub user: String,
    pub method: String,
    pub credential: i64,
    pub epoch: i64,
    pub policy: i64,
    pub factor: i64,
    pub email: i64,
    pub browser: Vec<u8>,
    pub parent: Option<String>,
    pub parent_generation: Option<i64>,
    pub verified: i64,
    pub expires: i64,
}
struct ContextBinding<'a> {
    browser: &'a [u8],
    parent: Option<&'a str>,
    generation: Option<i64>,
    expires: i64,
}
pub(super) struct IdentityVersions<'a> {
    pub user: &'a str,
    pub method: &'a str,
    pub credential: i64,
    pub epoch: i64,
    pub email: i64,
    pub policy_version: i64,
    pub factor: i64,
}
impl HumanAuth {
    pub(in crate::controller_keys::human_auth) async fn persist_primary_context(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        id: &str,
        user: &str,
        email: i64,
        context: &VerifiedTransport,
        now: i64,
    ) -> Result<(), AuthError> {
        if let Some((session, generation)) = &context.parent {
            let row=sqlx::query("SELECT * FROM human_sessions WHERE session_id=? AND user_id=? AND generation=? AND revoked_at IS NULL").bind(session).bind(user).bind(generation).fetch_optional(&mut **tx).await?.ok_or(AuthError::Denied)?;
            self.family_check(tx, &row, now).await?;
        }
        let parent = context.parent.as_ref().map(|p| p.0.as_str());
        let generation = context.parent.as_ref().map(|p| p.1);
        let tag = self.context_binding(
            id,
            user,
            email,
            ContextBinding {
                browser: &context.browser,
                parent,
                generation,
                expires: now + PROOF_SECONDS,
            },
        );
        sqlx::query("INSERT INTO human_primary_context VALUES(?,?,?,?,?,?,?)")
            .bind(id)
            .bind(context.browser.as_slice())
            .bind(email)
            .bind(parent)
            .bind(generation)
            .bind(now + PROOF_SECONDS)
            .bind(tag.as_slice())
            .execute(&mut **tx)
            .await?;
        Ok(())
    }
    fn context_binding(
        &self,
        id: &str,
        user: &str,
        email: i64,
        context: ContextBinding<'_>,
    ) -> [u8; 32] {
        self.mac(
            b"primary-browser-context",
            &[
                id.as_bytes(),
                user.as_bytes(),
                context.browser,
                &email.to_be_bytes(),
                context.parent.unwrap_or("").as_bytes(),
                &context.generation.unwrap_or(0).to_be_bytes(),
                &context.expires.to_be_bytes(),
            ],
        )
    }
    pub(super) async fn primary(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        id: &str,
        browser: &[u8],
        now: i64,
        unused: bool,
    ) -> Result<Proof, AuthError> {
        let r=sqlx::query("SELECT a.*,c.browser,c.email_version,c.parent_session,c.parent_generation,c.expires_at,c.binding AS context_binding FROM human_authentications a JOIN human_primary_context c USING(evidence_id) WHERE evidence_id=? AND NOT EXISTS(SELECT 1 FROM human_primary_blocks b WHERE b.evidence_id=a.evidence_id)").bind(id).fetch_optional(&mut **tx).await?.ok_or(AuthError::Denied)?;
        let p = Proof {
            evidence: id.to_owned(),
            user: r.try_get("user_id")?,
            method: r.try_get("method")?,
            credential: r.try_get("credential_version")?,
            epoch: r.try_get("security_epoch")?,
            policy: r.try_get("policy_version")?,
            factor: r.try_get("factor_required")?,
            email: r.try_get("email_version")?,
            browser: r.try_get("browser")?,
            parent: r.try_get("parent_session")?,
            parent_generation: r.try_get("parent_generation")?,
            verified: r.try_get("verified_at")?,
            expires: r.try_get("expires_at")?,
        };
        let expected = self.mac(
            b"primary-evidence",
            &[
                id.as_bytes(),
                p.user.as_bytes(),
                p.method.as_bytes(),
                &p.credential.to_be_bytes(),
                &p.epoch.to_be_bytes(),
                &p.policy.to_be_bytes(),
                &p.verified.to_be_bytes(),
                &p.factor.to_be_bytes(),
                r.try_get::<&str, _>("source_ref")?.as_bytes(),
            ],
        );
        if !constant_eq(&expected, &r.try_get::<Vec<u8>, _>("binding")?)
            || !constant_eq(&p.browser, browser)
            || now < p.verified
            || now >= p.expires
            || p.expires != p.verified + PROOF_SECONDS
            || !constant_eq(
                &self.context_binding(
                    id,
                    &p.user,
                    p.email,
                    ContextBinding {
                        browser: &p.browser,
                        parent: p.parent.as_deref(),
                        generation: p.parent_generation,
                        expires: p.expires,
                    },
                ),
                &r.try_get::<Vec<u8>, _>("context_binding")?,
            )
        {
            return Err(AuthError::Denied);
        }
        if unused
            && sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM human_primary_uses WHERE evidence_id=?",
            )
            .bind(id)
            .fetch_one(&mut **tx)
            .await?
                != 0
        {
            return Err(AuthError::Denied);
        }
        self.current_identity(
            tx,
            IdentityVersions {
                user: &p.user,
                method: &p.method,
                credential: p.credential,
                epoch: p.epoch,
                email: p.email,
                policy_version: p.policy,
                factor: p.factor,
            },
        )
        .await?;
        if let Some(parent) = &p.parent {
            let s=sqlx::query("SELECT * FROM human_sessions WHERE session_id=? AND user_id=? AND generation=? AND revoked_at IS NULL").bind(parent).bind(&p.user).bind(p.parent_generation).fetch_optional(&mut **tx).await?.ok_or(AuthError::Denied)?;
            self.family_check(tx, &s, now).await?;
        }
        Ok(p)
    }
    pub(super) async fn current_identity(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        expected: IdentityVersions<'_>,
    ) -> Result<(), AuthError> {
        let IdentityVersions {
            user,
            method,
            credential,
            epoch,
            email,
            policy_version,
            factor,
        } = expected;
        let r=sqlx::query("SELECT u.state,u.security_epoch,u.factor_required,e.version,e.verified,e.email_login FROM human_users u JOIN human_emails e USING(user_id) WHERE user_id=?").bind(user).fetch_optional(&mut **tx).await?.ok_or(AuthError::Denied)?;
        let p = policy(tx).await?;
        if r.try_get::<&str, _>("state")? != "active"
            || r.try_get::<i64, _>("security_epoch")? != epoch
            || r.try_get::<i64, _>("factor_required")? != factor
            || r.try_get::<i64, _>("version")? != email
            || r.try_get::<i64, _>("verified")? != 1
            || p.0 != policy_version
        {
            return Err(AuthError::Denied);
        }
        match method {
            "password" => {
                if !p.1.local_method {
                    return Err(AuthError::Denied);
                }
                let r = sqlx::query("SELECT * FROM human_passwords WHERE user_id=? AND version=?")
                    .bind(user)
                    .bind(credential)
                    .fetch_optional(&mut **tx)
                    .await?
                    .ok_or(AuthError::Denied)?;
                let phc: String = r.try_get("phc")?;
                if !self.passwords.valid_phc(&phc)
                    || !constant_eq(
                        &self.password_binding(user, credential, &phc),
                        &r.try_get::<Vec<u8>, _>("binding")?,
                    )
                {
                    return Err(AuthError::Denied);
                }
            }
            "magic_link" | "otp" => {
                if !p.1.verified_email
                    || !self.mail_ready()
                    || r.try_get::<i64, _>("email_login")? != 1
                    || credential != email
                {
                    return Err(AuthError::Denied);
                }
            }
            _ => return Err(AuthError::Denied),
        }
        if factor == 1
            && sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM human_factors WHERE user_id=? AND state='active'",
            )
            .bind(user)
            .fetch_one(&mut **tx)
            .await?
                != 1
        {
            return Err(AuthError::Denied);
        }
        Ok(())
    }
    pub(super) async fn consume_primary(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        proof: &Proof,
        purpose: &str,
        now: i64,
    ) -> Result<(), AuthError> {
        sqlx::query("INSERT INTO human_primary_uses VALUES(?,?,?)")
            .bind(&proof.evidence)
            .bind(purpose)
            .bind(now)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }
    pub async fn start_session(
        &self,
        proof: &PrimaryAuthentication,
        context: &BrowserContext,
        device: &DeviceLabel,
    ) -> Result<SessionStart, AuthError> {
        let (mut tx, browser, now) = self.browser_transaction(context, Operation::Issue).await?;
        let p = self
            .primary(&mut tx, &proof.evidence, &browser.browser, now, true)
            .await?;
        if p.user != proof.user.0
            || p.credential != proof.credential_version
            || p.epoch != proof.security_epoch
            || p.policy != proof.policy_version
            || p.method != proof.method.name()
            || p.verified != proof.verified_at
            || (p.factor != 0) != (proof.stage == AuthenticationStage::AdditionalFactorRequired)
        {
            return Err(AuthError::Denied);
        }
        let mut result = if p.factor == 1 {
            let factor: String = sqlx::query_scalar(
                "SELECT factor_id FROM human_factors WHERE user_id=? AND state='active'",
            )
            .bind(&p.user)
            .fetch_one(&mut *tx)
            .await?;
            let secret = Zeroizing::new(hex(&random::<32>()?[..]));
            let id = new_id()?;
            self.consume_primary(&mut tx, &p, "factor_pending", now)
                .await?;
            let commitment = self.mac(
                b"pending-factor",
                &[
                    id.as_bytes(),
                    p.user.as_bytes(),
                    p.evidence.as_bytes(),
                    secret.as_bytes(),
                ],
            );
            sqlx::query("INSERT INTO human_pending_logins VALUES(?,?,?,?,'pending',0)")
                .bind(&id)
                .bind(&p.evidence)
                .bind(factor)
                .bind(commitment.as_slice())
                .execute(&mut *tx)
                .await?;
            self.session_event(
                &mut tx,
                &p.user,
                None,
                &context.request_id,
                "factor_pending",
                now,
            )
            .await?;
            SessionStart::FactorRequired(PendingLogin { id, secret })
        } else {
            self.consume_primary(&mut tx, &p, "session", now).await?;
            SessionStart::Authenticated(Box::new(
                self.create_family(
                    &mut tx,
                    &p,
                    Assurance::Primary,
                    &device.0,
                    &context.request_id,
                    now,
                )
                .await?,
            ))
        };
        if let SessionStart::Authenticated(ticket) = &mut result {
            ticket.delivery =
                Some(self.delivery_scope(context, Operation::Issue, &browser, p.expires)?);
        }
        self.session_checkpoint(
            context,
            Operation::Issue,
            p.expires,
            Checkpoint::BeforeSessionCommit,
            (&browser, now),
        )?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterSessionCommit)?;
        Ok(result)
    }
    pub(super) fn session_checkpoint(
        &self,
        c: &BrowserContext,
        op: Operation,
        expires: i64,
        point: Checkpoint,
        initial: (&BrowserFacts, i64),
    ) -> Result<(), AuthError> {
        self.checkpoint(point)?;
        let now = self.now()?;
        if now >= expires || now < initial.1 || self.browser(c, op, now)? != *initial.0 {
            return Err(AuthError::Denied);
        }
        Ok(())
    }
}
