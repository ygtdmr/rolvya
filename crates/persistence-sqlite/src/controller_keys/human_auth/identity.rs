use super::*;
impl HumanAuth {
    pub(super) fn enrollment_commitment(
        &self,
        r: &Enrollment,
        password: Option<&Password>,
    ) -> [u8; 32] {
        let methods = r
            .initial_policy
            .map(|p| [u8::from(p.password), u8::from(p.verified_email)])
            .unwrap_or([0, 0]);
        self.mac(
            b"enrollment-request",
            &[
                r.email.0.as_bytes(),
                &[u8::from(r.email_login)],
                &methods,
                password.map(|p| p.0.as_slice()).unwrap_or(&[]),
            ],
        )
    }
    pub async fn enroll(
        &self,
        r: &Enrollment,
        password: Option<&Password>,
    ) -> Result<HumanId, AuthError> {
        let first = r.initial_policy.is_some();
        let commitment = self.enrollment_commitment(r, password);
        let context = AdminContext {
            request: &r.administration,
            action: if first {
                AdminAction::FirstEnrollment
            } else {
                AdminAction::AdditionalEnrollment
            },
            target: None,
            payload: commitment,
            now: self.now()?,
        };
        let evidence = self.authority.check(&context)?;
        if first && evidence != EmailEvidence::IndependentlyVerified {
            return Err(AuthError::Denied);
        }
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(old) = sqlx::query("SELECT * FROM human_enroll_requests WHERE request_id=?")
            .bind(r.administration.request_id.as_str())
            .fetch_optional(&mut *tx)
            .await?
        {
            return if constant_eq(&old.try_get::<Vec<u8>, _>("commitment")?, &commitment) {
                Ok(HumanId(old.try_get("user_id")?))
            } else {
                Err(AuthError::Conflict)
            };
        }
        let state: String =
            sqlx::query_scalar("SELECT state FROM human_enrollment WHERE singleton=1")
                .fetch_one(&mut *tx)
                .await?;
        if first {
            if state == "pending" {
                return Err(AuthError::RecoveryRequired);
            }
            if state != "fresh" {
                return Err(AuthError::Conflict);
            }
            let policy = r.initial_policy.ok_or(AuthError::InvalidPolicy)?;
            self.validate_route(policy, password.is_some(), r.email_login, evidence)?;
            sqlx::query("UPDATE human_enrollment SET state='pending',request_id=?,commitment=? WHERE singleton=1 AND state='fresh'").bind(r.administration.request_id.as_str()).bind(commitment.as_slice()).execute(&mut *tx).await?;
            audit(
                &mut tx,
                None,
                &r.administration.request_id,
                "enrollment_pending",
                context.now,
            )
            .await?;
            tx.commit().await?;
            self.checkpoint(Checkpoint::EnrollmentClaimed)?;
        } else {
            if state != "ready" {
                return Err(AuthError::RecoveryRequired);
            }
            self.validate_route(
                policy(&mut tx).await?.1,
                password.is_some(),
                r.email_login,
                evidence,
            )?;
            tx.commit().await?;
        }
        let verifier = match password {
            Some(p) => Some(self.passwords.hash(p).await?),
            None => None,
        };
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        if self.authority.check(&AdminContext {
            now: self.now()?,
            ..context
        })? != evidence
        {
            return Err(AuthError::Denied);
        }
        if let Some(old) = sqlx::query("SELECT * FROM human_enroll_requests WHERE request_id=?")
            .bind(r.administration.request_id.as_str())
            .fetch_optional(&mut *tx)
            .await?
        {
            return if constant_eq(&old.try_get::<Vec<u8>, _>("commitment")?, &commitment) {
                Ok(HumanId(old.try_get("user_id")?))
            } else {
                Err(AuthError::Conflict)
            };
        }
        if first {
            let pending = sqlx::query("SELECT * FROM human_enrollment WHERE singleton=1")
                .fetch_one(&mut *tx)
                .await?;
            if pending.try_get::<&str, _>("state")? != "pending"
                || pending.try_get::<&str, _>("request_id")? != r.administration.request_id.as_str()
                || !constant_eq(&pending.try_get::<Vec<u8>, _>("commitment")?, &commitment)
            {
                return Err(AuthError::RecoveryRequired);
            }
            let p = r.initial_policy.ok_or(AuthError::InvalidPolicy)?;
            self.validate_route(p, password.is_some(), r.email_login, evidence)?;
            sqlx::query("INSERT INTO human_login_policy VALUES(1,1,?,?)")
                .bind(p.password)
                .bind(p.verified_email)
                .execute(&mut *tx)
                .await?;
        } else {
            self.validate_route(
                policy(&mut tx).await?.1,
                password.is_some(),
                r.email_login,
                evidence,
            )?;
        }
        if sqlx::query_scalar::<_, i64>("SELECT count(*) FROM human_emails WHERE address=?")
            .bind(&r.email.0)
            .fetch_one(&mut *tx)
            .await?
            != 0
        {
            return Err(AuthError::Conflict);
        }
        let user = new_id()?;
        sqlx::query("INSERT INTO human_users VALUES(?,'active',1,0,?)")
            .bind(&user)
            .bind(self.now()?)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO human_emails VALUES(?,?,1,?,?)")
            .bind(&user)
            .bind(&r.email.0)
            .bind(evidence == EmailEvidence::IndependentlyVerified)
            .bind(r.email_login)
            .execute(&mut *tx)
            .await?;
        if let Some(phc) = verifier {
            sqlx::query("INSERT INTO human_passwords VALUES(?,1,1,?,?)")
                .bind(&user)
                .bind(&phc)
                .bind(self.password_binding(&user, 1, &phc).as_slice())
                .execute(&mut *tx)
                .await?;
        }
        if first {
            sqlx::query("UPDATE human_enrollment SET state='ready',user_id=? WHERE singleton=1 AND state='pending'").bind(&user).execute(&mut *tx).await?;
        }
        sqlx::query("INSERT INTO human_enroll_requests VALUES(?,?,?)")
            .bind(r.administration.request_id.as_str())
            .bind(commitment.as_slice())
            .bind(&user)
            .execute(&mut *tx)
            .await?;
        audit(
            &mut tx,
            Some(&user),
            &r.administration.request_id,
            "enrolled",
            self.now()?,
        )
        .await?;
        if self.authority.check(&AdminContext {
            now: self.now()?,
            ..context
        })? != evidence
        {
            return Err(AuthError::Denied);
        }
        tx.commit().await?;
        Ok(HumanId(user))
    }
    fn validate_route(
        &self,
        p: LoginPolicy,
        password: bool,
        email_login: bool,
        evidence: EmailEvidence,
    ) -> Result<(), AuthError> {
        if (!p.password && !p.verified_email)
            || (!p.password && !self.mail_ready())
            || !(p.password && password || p.verified_email && email_login && self.mail_ready())
        {
            return Err(AuthError::InvalidPolicy);
        }
        if evidence == EmailEvidence::Pending && !self.mail_ready() {
            return Err(AuthError::InvalidPolicy);
        }
        if email_login && evidence != EmailEvidence::IndependentlyVerified {
            return Err(AuthError::Denied);
        }
        Ok(())
    }
    /// Explicit authority and expected credential version; not a password-reset API.
    /// Future recovery and voluntarily enrolled factors must supply their own proofs.
    pub async fn replace_password(
        &self,
        a: &Administration,
        user: &HumanId,
        expected: i64,
        new_password: &Password,
    ) -> Result<(), AuthError> {
        if expected < 0 || expected == i64::MAX {
            return Err(AuthError::InvalidInput);
        }
        let payload = self.mac(
            b"password-change",
            &[user.0.as_bytes(), &expected.to_be_bytes(), &new_password.0],
        );
        let c = AdminContext {
            request: a,
            action: AdminAction::PasswordChange,
            target: Some(&user.0),
            payload,
            now: self.now()?,
        };
        self.authority.check(&c)?;
        let phc = self.passwords.hash(new_password).await?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row=sqlx::query("SELECT u.state,u.factor_required,coalesce(p.version,0) AS version FROM human_users u LEFT JOIN human_passwords p USING(user_id) WHERE user_id=?").bind(&user.0).fetch_optional(&mut *tx).await?.ok_or(AuthError::Denied)?;
        if row.get::<&str, _>("state") != "active" || row.get::<i64, _>("factor_required") != 0 {
            return Err(AuthError::Denied);
        }
        if row.get::<i64, _>("version") != expected {
            return Err(AuthError::StaleVersion);
        }
        sqlx::query("INSERT INTO human_passwords VALUES(?,?,1,?,?) ON CONFLICT(user_id) DO UPDATE SET version=excluded.version,phc=excluded.phc,binding=excluded.binding").bind(&user.0).bind(expected+1).bind(&phc).bind(self.password_binding(&user.0,expected+1,&phc).as_slice()).execute(&mut *tx).await?;
        sqlx::query("UPDATE human_users SET security_epoch=security_epoch+1 WHERE user_id=?")
            .bind(&user.0)
            .execute(&mut *tx)
            .await?;
        audit(
            &mut tx,
            Some(&user.0),
            &a.request_id,
            "password_changed",
            self.now()?,
        )
        .await?;
        self.authority.check(&AdminContext {
            now: self.now()?,
            ..c
        })?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn replace_email(
        &self,
        a: &Administration,
        user: &HumanId,
        expected: i64,
        email: &EmailAddress,
    ) -> Result<(), AuthError> {
        let payload = self.mac(
            b"email-change",
            &[
                user.0.as_bytes(),
                &expected.to_be_bytes(),
                email.0.as_bytes(),
            ],
        );
        let c = AdminContext {
            request: a,
            action: AdminAction::EmailChange,
            target: Some(&user.0),
            payload,
            now: self.now()?,
        };
        if self.authority.check(&c)? != EmailEvidence::IndependentlyVerified {
            return Err(AuthError::Denied);
        }
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let changed=sqlx::query("UPDATE human_emails SET address=?,version=version+1,verified=1 WHERE user_id=? AND version=? AND EXISTS(SELECT 1 FROM human_users WHERE user_id=? AND state='active' AND factor_required=0)").bind(&email.0).bind(&user.0).bind(expected).bind(&user.0).execute(&mut *tx).await?;
        if changed.rows_affected() != 1 {
            return Err(AuthError::StaleVersion);
        }
        sqlx::query("UPDATE human_users SET security_epoch=security_epoch+1 WHERE user_id=?")
            .bind(&user.0)
            .execute(&mut *tx)
            .await?;
        audit(
            &mut tx,
            Some(&user.0),
            &a.request_id,
            "email_changed",
            self.now()?,
        )
        .await?;
        if self.authority.check(&AdminContext {
            now: self.now()?,
            ..c
        })? != EmailEvidence::IndependentlyVerified
        {
            return Err(AuthError::Denied);
        }
        tx.commit().await?;
        Ok(())
    }
    pub async fn set_email_login(
        &self,
        a: &Administration,
        user: &HumanId,
        expected: i64,
        enabled: bool,
    ) -> Result<(), AuthError> {
        let payload = self.mac(
            b"email-login-change",
            &[
                user.0.as_bytes(),
                &expected.to_be_bytes(),
                &[u8::from(enabled)],
            ],
        );
        let c = AdminContext {
            request: a,
            action: AdminAction::EmailLoginChange,
            target: Some(&user.0),
            payload,
            now: self.now()?,
        };
        self.authority.check(&c)?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let r=sqlx::query("SELECT e.*,u.state,u.factor_required FROM human_emails e JOIN human_users u USING(user_id) WHERE user_id=?").bind(&user.0).fetch_optional(&mut *tx).await?.ok_or(AuthError::Denied)?;
        let p = policy(&mut tx).await?.1;
        if r.get::<i64, _>("version") != expected {
            return Err(AuthError::StaleVersion);
        }
        if r.get::<&str, _>("state") != "active"
            || r.get::<i64, _>("factor_required") != 0
            || (enabled
                && (r.get::<i64, _>("verified") != 1 || !p.verified_email || !self.mail_ready()))
        {
            return Err(AuthError::Denied);
        }
        if !enabled && !(p.password && self.has_password(&mut tx, &user.0).await?) {
            return Err(AuthError::InvalidPolicy);
        }
        sqlx::query("UPDATE human_emails SET email_login=?,version=version+1 WHERE user_id=?")
            .bind(enabled)
            .bind(&user.0)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE human_users SET security_epoch=security_epoch+1 WHERE user_id=?")
            .bind(&user.0)
            .execute(&mut *tx)
            .await?;
        audit(
            &mut tx,
            Some(&user.0),
            &a.request_id,
            "email_login_changed",
            self.now()?,
        )
        .await?;
        self.authority.check(&AdminContext {
            now: self.now()?,
            ..c
        })?;
        tx.commit().await?;
        Ok(())
    }
    async fn has_password(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        user: &str,
    ) -> Result<bool, AuthError> {
        let row = sqlx::query("SELECT * FROM human_passwords WHERE user_id=?")
            .bind(user)
            .fetch_optional(&mut **tx)
            .await?;
        Ok(row.is_some_and(|r| {
            let phc = r.get::<String, _>("phc");
            self.passwords.valid_phc(&phc)
                && constant_eq(
                    &r.get::<Vec<u8>, _>("binding"),
                    &self.password_binding(user, r.get("version"), &phc),
                )
        }))
    }
    pub async fn configure_policy(
        &self,
        a: &Administration,
        expected: i64,
        p: LoginPolicy,
    ) -> Result<(), AuthError> {
        let payload = self.mac(
            b"policy-change",
            &[
                &expected.to_be_bytes(),
                &[u8::from(p.password), u8::from(p.verified_email)],
            ],
        );
        let c = AdminContext {
            request: a,
            action: AdminAction::PolicyChange,
            target: None,
            payload,
            now: self.now()?,
        };
        self.authority.check(&c)?;
        if !p.password && !self.mail_ready() {
            return Err(AuthError::InvalidPolicy);
        }
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        if policy(&mut tx).await?.0 != expected {
            return Err(AuthError::StaleVersion);
        }
        // Rare administrative change: bounded pages rather than an unbounded allocation.
        let mut after = String::new();
        loop {
            let users=sqlx::query("SELECT u.user_id,e.verified,e.email_login FROM human_users u JOIN human_emails e USING(user_id) WHERE u.state='active' AND u.user_id>? ORDER BY u.user_id LIMIT 128").bind(&after).fetch_all(&mut *tx).await?;
            if users.is_empty() {
                break;
            }
            for u in users {
                after = u.get("user_id");
                if u.get::<i64, _>("verified") != 1
                    || !(p.password && self.has_password(&mut tx, &after).await?
                        || p.verified_email
                            && self.mail_ready()
                            && u.get::<i64, _>("email_login") == 1)
                {
                    return Err(AuthError::InvalidPolicy);
                }
            }
        }
        sqlx::query("UPDATE human_login_policy SET version=version+1,password=?,verified_email=? WHERE singleton=1").bind(p.password).bind(p.verified_email).execute(&mut *tx).await?;
        audit(&mut tx, None, &a.request_id, "policy_changed", self.now()?).await?;
        self.authority.check(&AdminContext {
            now: self.now()?,
            ..c
        })?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn restrict_identity(
        &self,
        a: &Administration,
        user: &HumanId,
        restriction: AccountRestriction,
    ) -> Result<(), AuthError> {
        let name = match restriction {
            AccountRestriction::Disabled => "disabled",
            AccountRestriction::Compromised => "compromised",
        };
        let c = AdminContext {
            request: a,
            action: AdminAction::Restrict,
            target: Some(&user.0),
            payload: self.mac(b"identity-restrict", &[user.0.as_bytes(), name.as_bytes()]),
            now: self.now()?,
        };
        self.authority.check(&c)?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        if sqlx::query(
            "UPDATE human_users SET state=?,security_epoch=security_epoch+1 WHERE user_id=?",
        )
        .bind(name)
        .bind(&user.0)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            != 1
        {
            return Err(AuthError::Denied);
        }
        audit(
            &mut tx,
            Some(&user.0),
            &a.request_id,
            "identity_restricted",
            self.now()?,
        )
        .await?;
        self.authority.check(&AdminContext {
            now: self.now()?,
            ..c
        })?;
        tx.commit().await?;
        Ok(())
    }
    /// Bounded cleanup of old throttle reservations only. Challenges, primary proofs
    /// and redacted audit remain retained for security/reconciliation; retention policy
    /// beyond this foundation requires a later reviewed migration.
    pub async fn cleanup_attempts(&self, a: &Administration, limit: u32) -> Result<u64, AuthError> {
        if limit == 0 || limit > 100 {
            return Err(AuthError::InvalidInput);
        }
        let c = AdminContext {
            request: a,
            action: AdminAction::Maintenance,
            target: None,
            payload: self.mac(b"maintenance", &[&limit.to_be_bytes()]),
            now: self.now()?,
        };
        self.authority.check(&c)?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let n=sqlx::query("DELETE FROM human_attempts WHERE attempt_id IN (SELECT attempt_id FROM human_attempts WHERE started_at<=? ORDER BY started_at LIMIT ?)").bind(c.now.saturating_sub(900)).bind(limit).execute(&mut *tx).await?.rows_affected();
        audit(&mut tx, None, &a.request_id, "maintenance", self.now()?).await?;
        self.authority.check(&AdminContext {
            now: self.now()?,
            ..c
        })?;
        tx.commit().await?;
        Ok(n)
    }
}
