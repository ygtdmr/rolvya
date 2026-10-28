//! Purpose-specific human-factor encryption. No provider connection or worker grant.
use super::proof::Proof;
use super::*;
use crate::controller_keys::secret_broker::crypto::{decrypt, encrypt};
use sha2::Digest;
use totp_rs::{Algorithm, TOTP};
pub struct TotpCode(pub(super) Zeroizing<String>);
impl TotpCode {
    pub fn parse(v: String) -> Result<Self, AuthError> {
        let v = Zeroizing::new(v);
        if v.len() != 6 || !v.bytes().all(|b| b.is_ascii_digit()) {
            return Err(AuthError::InvalidInput);
        }
        Ok(Self(v))
    }
}
impl fmt::Debug for TotpCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TotpCode([redacted])")
    }
}
pub struct RecoveryCode(pub(super) Zeroizing<String>);
impl RecoveryCode {
    pub fn parse(v: String) -> Result<Self, AuthError> {
        let v = Zeroizing::new(v);
        if v.len() != 32
            || !v
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(AuthError::InvalidInput);
        }
        Ok(Self(v))
    }
}
impl fmt::Debug for RecoveryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryCode([redacted])")
    }
}
#[derive(Debug, Clone)]
pub struct FactorId(String);
impl FactorId {
    pub fn parse(v: String) -> Result<Self, AuthError> {
        SessionId::parse(v).map(|id| Self(id.0))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
pub struct FactorEnrollment {
    id: String,
    user: String,
    pub(super) seed: Zeroizing<Vec<u8>>,
    session: String,
    generation: i64,
    expires: i64,
    pub(super) delivery: DeliveryGrant,
}
impl FactorEnrollment {
    pub fn reference(&self) -> FactorId {
        FactorId(self.id.clone())
    }
}
impl fmt::Debug for FactorEnrollment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FactorEnrollment([redacted])")
    }
}
pub struct RecoveryBundle {
    pub(super) codes: Vec<RecoveryCode>,
    user: String,
    factor: String,
    batch: i64,
    origin_lookup: [u8; 32],
    expires: i64,
}
impl fmt::Debug for RecoveryBundle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryBundle([redacted])")
    }
}
#[derive(Debug)]
pub struct FactorActivation {
    pub session: SessionTicket,
    pub recovery: RecoveryBundle,
}
struct FactorEnvelope {
    dn: [u8; 24],
    wn: [u8; 24],
    wrapped: Vec<u8>,
    ciphertext: Vec<u8>,
}
impl HumanAuth {
    fn factor_aad(&self, user: &str, id: &str, version: i64) -> Vec<u8> {
        let mut a = Vec::with_capacity(160);
        a.extend_from_slice(b"rolvya/human-factor/content/v1\0");
        for b in [
            &self.key.metadata.installation.0[..],
            &self.key.metadata.key.0[..],
            &self.key.metadata.format_version.to_be_bytes()[..],
            &self.key.metadata.key_version.to_be_bytes()[..],
            user.as_bytes(),
            id.as_bytes(),
            &version.to_be_bytes()[..],
            b"authenticator-totp-seed",
        ] {
            a.extend_from_slice(&(b.len() as u32).to_be_bytes());
            a.extend_from_slice(b);
        }
        a
    }
    fn factor_wrap_aad(&self, aad: &[u8], dn: &[u8], ciphertext: &[u8]) -> Vec<u8> {
        let mut a = aad.to_vec();
        a.extend_from_slice(b"\0rolvya/human-factor/dek-wrap/v1\0");
        a.extend_from_slice(dn);
        a.extend_from_slice(&sha2::Sha256::digest(ciphertext));
        a
    }
    fn seal_factor(
        &self,
        user: &str,
        id: &str,
        version: i64,
        seed: &[u8],
    ) -> Result<FactorEnvelope, AuthError> {
        if seed.len() != 32 {
            return Err(AuthError::InvalidInput);
        }
        let dek = random::<32>()?;
        let dn = *random::<24>()?;
        let wn = *random::<24>()?;
        if dn == wn {
            return Err(AuthError::Randomness);
        }
        let aad = self.factor_aad(user, id, version);
        let ciphertext = encrypt(&dek, &dn, &aad, seed).map_err(|_| AuthError::Denied)?;
        let wrapped = encrypt(
            &self.key.material,
            &wn,
            &self.factor_wrap_aad(&aad, &dn, &ciphertext),
            &dek[..],
        )
        .map_err(|_| AuthError::Denied)?;
        Ok(FactorEnvelope {
            dn,
            wn,
            wrapped,
            ciphertext,
        })
    }
    pub(super) fn factor_seed(
        &self,
        r: &SqliteRow,
        user: &str,
    ) -> Result<Zeroizing<Vec<u8>>, AuthError> {
        let dn: Vec<u8> = r.try_get("data_nonce")?;
        let wn: Vec<u8> = r.try_get("wrap_nonce")?;
        let dn: [u8; 24] = dn.try_into().map_err(|_| AuthError::Denied)?;
        let wn: [u8; 24] = wn.try_into().map_err(|_| AuthError::Denied)?;
        let wrapped: Vec<u8> = r.try_get("wrapped_dek")?;
        let ciphertext: Vec<u8> = r.try_get("ciphertext")?;
        if r.try_get::<&str, _>("user_id")? != user
            || r.try_get::<i64, _>("envelope_version")? != 1
            || r.try_get::<Vec<u8>, _>("key_id")? != self.key.metadata.key.0
            || r.try_get::<i64, _>("key_version")? != self.key.metadata.key_version as i64
            || dn == wn
            || wrapped.len() != 48
            || ciphertext.len() != 48
        {
            return Err(AuthError::Denied);
        }
        let aad = self.factor_aad(user, r.try_get("factor_id")?, r.try_get("version")?);
        let dek = decrypt(
            &self.key.material,
            &wn,
            &self.factor_wrap_aad(&aad, &dn, &ciphertext),
            &wrapped,
        )
        .map_err(|_| AuthError::Denied)?;
        let key: &[u8; 32] = dek.as_slice().try_into().map_err(|_| AuthError::Denied)?;
        let seed = decrypt(key, &dn, &aad, &ciphertext).map_err(|_| AuthError::Denied)?;
        if seed.len() != 32 {
            return Err(AuthError::Denied);
        }
        Ok(seed)
    }
    pub async fn begin_totp_enrollment(
        &self,
        h: &SessionHandle,
        primary: &PrimaryAuthentication,
        c: &BrowserContext,
    ) -> Result<FactorEnrollment, AuthError> {
        // Verify before producing provisioning material, then recheck after crypto.
        let (mut read, f, now) = self.browser_transaction(c, Operation::BeginFactor).await?;
        let s = self.session_row(&mut read, h, &f, now, true).await?;
        let p = self
            .primary(&mut read, &primary.evidence, &f.browser, now, true)
            .await?;
        self.enrollment_scope(&s, &p)?;
        let id = new_id()?;
        let version: i64 = sqlx::query_scalar(
            "SELECT coalesce(max(version),0)+1 FROM human_factors WHERE user_id=?",
        )
        .bind(&p.user)
        .fetch_one(&mut *read)
        .await?;
        read.commit().await?;
        let seed = Zeroizing::new(random::<32>()?.to_vec());
        let envelope = self.seal_factor(&p.user, &id, version, &seed)?;
        let (mut tx, f, now) = self.browser_transaction(c, Operation::BeginFactor).await?;
        let s = self.session_row(&mut tx, h, &f, now, true).await?;
        let p = self
            .primary(&mut tx, &p.evidence, &f.browser, now, true)
            .await?;
        self.enrollment_scope(&s, &p)?;
        sqlx::query("UPDATE human_factors SET state='aborted' WHERE user_id=? AND state='pending' AND expires_at<=?").bind(&p.user).bind(now).execute(&mut *tx).await?;
        if sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM human_factors WHERE user_id=? AND state IN ('pending','active')",
        )
        .bind(&p.user)
        .fetch_one(&mut *tx)
        .await?
            != 0
        {
            return Err(AuthError::Denied);
        }
        self.consume_primary(&mut tx, &p, "factor_enrollment", now)
            .await?;
        sqlx::query(
            "INSERT INTO human_factors VALUES(?,?,?,'pending',?,?,?,?,?,?,-1,0,1,?,?,?,?,?,?)",
        )
        .bind(&id)
        .bind(&p.user)
        .bind(version)
        .bind(s.try_get::<&str, _>("session_id")?)
        .bind(s.try_get::<i64, _>("generation")?)
        .bind(&p.evidence)
        .bind(p.epoch)
        .bind(now)
        .bind(now + 600)
        .bind(self.key.metadata.key.0.as_slice())
        .bind(self.key.metadata.key_version as i64)
        .bind(envelope.dn.as_slice())
        .bind(envelope.wn.as_slice())
        .bind(envelope.wrapped)
        .bind(envelope.ciphertext)
        .execute(&mut *tx)
        .await?;
        self.session_event(
            &mut tx,
            &p.user,
            Some(s.try_get("session_id")?),
            &c.request_id,
            "factor_started",
            now,
        )
        .await?;
        self.session_checkpoint(
            c,
            Operation::BeginFactor,
            p.expires.min(self.original_deadline(&mut tx, h, &s).await?),
            Checkpoint::BeforeFactorCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterFactorCommit)?;
        Ok(FactorEnrollment {
            id,
            user: p.user,
            seed,
            session: s.try_get("session_id")?,
            generation: s.try_get("generation")?,
            expires: now + 600,
            delivery: self.delivery_scope(c, Operation::BeginFactor, &f, p.expires)?,
        })
    }
    fn enrollment_scope(&self, s: &SqliteRow, p: &Proof) -> Result<(), AuthError> {
        if p.user != s.try_get::<String, _>("user_id")?
            || p.factor != 0
            || p.parent.as_deref() != Some(s.try_get("session_id")?)
            || p.parent_generation != Some(s.try_get("generation")?)
        {
            return Err(AuthError::Denied);
        }
        Ok(())
    }
    pub async fn deliver_totp_provisioning(
        &self,
        e: &FactorEnrollment,
        h: &SessionHandle,
        c: &BrowserContext,
    ) -> Result<(), AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::BeginFactor).await?;
        self.check_delivery(&e.delivery, c, now)?;
        let session = self.session_row(&mut tx, h, &f, now, true).await?;
        if now >= e.expires
            || e.session != session.try_get::<String, _>("session_id")?
            || e.generation != session.try_get::<i64, _>("generation")?
            || e.user != session.try_get::<String, _>("user_id")?
        {
            return Err(AuthError::Denied);
        }
        let row = sqlx::query(
            "SELECT * FROM human_factors WHERE factor_id=? AND user_id=? AND state='pending'",
        )
        .bind(&e.id)
        .bind(&e.user)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AuthError::Denied)?;
        if !constant_eq(&self.factor_seed(&row, &e.user)?, &e.seed) {
            return Err(AuthError::Denied);
        }
        self.session_checkpoint(
            c,
            Operation::BeginFactor,
            e.delivery
                .expires
                .min(self.original_deadline(&mut tx, h, &session).await?),
            Checkpoint::BeforeFactorCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        self.check_delivery(&e.delivery, c, self.now()?)?;
        self.browser_authority.provision(c, e, self.now()?)
    }
    pub async fn confirm_totp_enrollment(
        &self,
        h: &SessionHandle,
        id: &FactorId,
        code: &TotpCode,
        c: &BrowserContext,
    ) -> Result<FactorActivation, AuthError> {
        let (mut tx, f, now) = self
            .browser_transaction(c, Operation::ConfirmFactor)
            .await?;
        let s = self.session_row(&mut tx, h, &f, now, true).await?;
        let r = sqlx::query(
            "SELECT * FROM human_factors WHERE factor_id=? AND user_id=? AND state='pending'",
        )
        .bind(&id.0)
        .bind(s.try_get::<&str, _>("user_id")?)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AuthError::Denied)?;
        let mut p = self
            .primary(
                &mut tx,
                r.try_get("source_evidence")?,
                &f.browser,
                now,
                false,
            )
            .await?;
        self.enrollment_scope(&s, &p)?;
        if r.try_get::<i64, _>("session_generation")? != s.try_get::<i64, _>("generation")?
            || now >= r.try_get::<i64, _>("expires_at")?
            || now < r.try_get::<i64, _>("created_at")?
            || r.try_get::<i64, _>("failures")? >= 5
        {
            return Err(AuthError::Denied);
        }
        if !self.factor_rate(&mut tx, &p.user, &f, now).await? {
            return Err(AuthError::Denied);
        }
        let seed = self.factor_seed(&r, &p.user)?;
        let step = matching_step(&seed, code, now, r.try_get("last_step")?)?;
        self.record_factor_attempt(&mut tx, &p.user, &f, now, step.is_some())
            .await?;
        let Some(step) = step else {
            sqlx::query("UPDATE human_factors SET failures=failures+1,state=CASE WHEN failures=4 THEN 'aborted' ELSE 'pending' END WHERE factor_id=?").bind(&id.0).execute(&mut *tx).await?;
            self.session_event(
                &mut tx,
                &p.user,
                Some(s.try_get("session_id")?),
                &c.request_id,
                "factor_denied",
                now,
            )
            .await?;
            tx.commit().await?;
            return Err(AuthError::Denied);
        };
        sqlx::query("UPDATE human_factors SET state='active',last_step=? WHERE factor_id=? AND state='pending'").bind(step).bind(&id.0).execute(&mut *tx).await?;
        sqlx::query("UPDATE human_users SET factor_required=1,security_epoch=security_epoch+1 WHERE user_id=? AND security_epoch=? AND factor_required=0").bind(&p.user).bind(p.epoch).execute(&mut *tx).await?;
        p.epoch += 1;
        p.factor = 1;
        self.revoke_user_sessions(&mut tx, &p.user, now).await?;
        let ticket = self
            .create_family(
                &mut tx,
                &p,
                Assurance::Totp,
                s.try_get("device")?,
                &c.request_id,
                now,
            )
            .await?;
        self.session_event(
            &mut tx,
            &p.user,
            Some(ticket.metadata.id.as_str()),
            &c.request_id,
            "factor_activated",
            now,
        )
        .await?;
        let recovery = self
            .new_recovery_codes(&mut tx, &id.0, &p.user, 1, &ticket)
            .await?;
        let ticket = self.scoped_delivery(ticket, c, Operation::ConfirmFactor, &f, p.expires)?;
        self.session_checkpoint(
            c,
            Operation::ConfirmFactor,
            p.expires
                .min(r.try_get("expires_at")?)
                .min(self.original_deadline(&mut tx, h, &s).await?),
            Checkpoint::BeforeFactorCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterFactorCommit)?;
        Ok(FactorActivation {
            session: ticket,
            recovery,
        })
    }
    pub(super) async fn factor_rate(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        user: &str,
        f: &BrowserFacts,
        now: i64,
    ) -> Result<bool, AuthError> {
        let r=sqlx::query("SELECT count(*) FILTER(WHERE user_id=?) AS u,count(*) FILTER(WHERE client_key=?) AS c FROM human_factor_attempts WHERE occurred_at>? AND succeeded=0 AND (user_id=? OR client_key=?)").bind(user).bind(f.client_key.as_slice()).bind(now.saturating_sub(900)).bind(user).bind(f.client_key.as_slice()).fetch_one(&mut **tx).await?;
        Ok(r.try_get::<i64, _>("u")? < 5 && r.try_get::<i64, _>("c")? < 30)
    }
    pub(super) async fn record_factor_attempt(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        user: &str,
        f: &BrowserFacts,
        now: i64,
        ok: bool,
    ) -> Result<(), AuthError> {
        sqlx::query("INSERT INTO human_factor_attempts VALUES(?,?,?,?,?)")
            .bind(new_id()?)
            .bind(user)
            .bind(f.client_key.as_slice())
            .bind(now)
            .bind(ok)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }
    async fn new_recovery_codes(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        factor: &str,
        user: &str,
        batch: i64,
        origin_ticket: &SessionTicket,
    ) -> Result<RecoveryBundle, AuthError> {
        let mut codes = Vec::with_capacity(10);
        for _ in 0..10 {
            let raw = Zeroizing::new(hex(&random::<16>()?[..]));
            let tag = self.mac(
                b"factor-recovery",
                &[user.as_bytes(), factor.as_bytes(), raw.as_bytes()],
            );
            sqlx::query("INSERT INTO human_recovery_codes VALUES(?,?,?,?,'active')")
                .bind(new_id()?)
                .bind(factor)
                .bind(batch)
                .bind(tag.as_slice())
                .execute(&mut **tx)
                .await?;
            codes.push(RecoveryCode(raw));
        }
        Ok(RecoveryBundle {
            codes,
            user: user.to_owned(),
            factor: factor.to_owned(),
            batch,
            origin_lookup: self.handle_lookup(&origin_ticket.handle),
            expires: self.now()?.saturating_add(PROOF_SECONDS),
        })
    }
    pub async fn complete_totp_login(
        &self,
        pending: &PendingLogin,
        code: &TotpCode,
        c: &BrowserContext,
        device: &DeviceLabel,
    ) -> Result<SessionTicket, AuthError> {
        self.finish_factor(pending, Some(code), None, c, device)
            .await
    }
    pub async fn complete_recovery_login(
        &self,
        pending: &PendingLogin,
        code: &RecoveryCode,
        c: &BrowserContext,
        device: &DeviceLabel,
    ) -> Result<SessionTicket, AuthError> {
        self.finish_factor(pending, None, Some(code), c, device)
            .await
    }
    async fn finish_factor(
        &self,
        pending: &PendingLogin,
        totp: Option<&TotpCode>,
        recovery: Option<&RecoveryCode>,
        c: &BrowserContext,
        device: &DeviceLabel,
    ) -> Result<SessionTicket, AuthError> {
        let (mut tx, f, now) = self
            .browser_transaction(c, Operation::CompleteFactor)
            .await?;
        let r=sqlx::query("SELECT * FROM human_pending_logins WHERE pending_id=? AND state='pending' AND failures<5").bind(&pending.id).fetch_optional(&mut *tx).await?.ok_or(AuthError::Denied)?;
        let mut p = self
            .primary(&mut tx, r.try_get("evidence_id")?, &f.browser, now, false)
            .await?;
        let tag = self.mac(
            b"pending-factor",
            &[
                pending.id.as_bytes(),
                p.user.as_bytes(),
                p.evidence.as_bytes(),
                pending.secret.as_bytes(),
            ],
        );
        if !f.preauth_csrf
            || !constant_eq(&tag, &r.try_get::<Vec<u8>, _>("secret_commitment")?)
            || p.factor != 1
        {
            return Err(AuthError::Denied);
        }
        let factor = sqlx::query(
            "SELECT * FROM human_factors WHERE factor_id=? AND user_id=? AND state='active'",
        )
        .bind(r.try_get::<&str, _>("factor_id")?)
        .bind(&p.user)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AuthError::Denied)?;
        if !self.factor_rate(&mut tx, &p.user, &f, now).await? {
            return Err(AuthError::Denied);
        }
        let factor_id: &str = factor.try_get("factor_id")?;
        let step = match totp {
            Some(code) => matching_step(
                &self.factor_seed(&factor, &p.user)?,
                code,
                now,
                factor.try_get("last_step")?,
            )?,
            None => None,
        };
        let recovery_id = if let Some(code) = recovery {
            sqlx::query_scalar::<_,String>("SELECT code_id FROM human_recovery_codes WHERE factor_id=? AND commitment=? AND state='active'").bind(factor_id).bind(self.mac(b"factor-recovery",&[p.user.as_bytes(),factor_id.as_bytes(),code.0.as_bytes()]).as_slice()).fetch_optional(&mut *tx).await?
        } else {
            None
        };
        let ok = step.is_some() || recovery_id.is_some();
        self.record_factor_attempt(&mut tx, &p.user, &f, now, ok)
            .await?;
        if !ok {
            sqlx::query("UPDATE human_pending_logins SET failures=failures+1,state=CASE WHEN failures=4 THEN 'exhausted' ELSE 'pending' END WHERE pending_id=?").bind(&pending.id).execute(&mut *tx).await?;
            self.session_event(&mut tx, &p.user, None, &c.request_id, "factor_denied", now)
                .await?;
            tx.commit().await?;
            return Err(AuthError::Denied);
        }
        if let Some(step) = step {
            sqlx::query("UPDATE human_factors SET last_step=? WHERE factor_id=? AND last_step<?")
                .bind(step)
                .bind(factor_id)
                .bind(step)
                .execute(&mut *tx)
                .await?;
        }
        let assurance = if let Some(id) = recovery_id {
            sqlx::query("UPDATE human_recovery_codes SET state='consumed' WHERE code_id=? AND state='active'").bind(id).execute(&mut *tx).await?;
            sqlx::query("UPDATE human_users SET security_epoch=security_epoch+1 WHERE user_id=? AND security_epoch=?").bind(&p.user).bind(p.epoch).execute(&mut *tx).await?;
            p.epoch += 1;
            self.revoke_user_sessions(&mut tx, &p.user, now).await?;
            self.session_event(&mut tx, &p.user, None, &c.request_id, "recovery_used", now)
                .await?;
            Assurance::Recovery
        } else {
            Assurance::Totp
        };
        sqlx::query("UPDATE human_pending_logins SET state='consumed' WHERE pending_id=? AND state='pending'").bind(&pending.id).execute(&mut *tx).await?;
        let ticket = self
            .create_family(&mut tx, &p, assurance, &device.0, &c.request_id, now)
            .await?;
        let ticket = self.scoped_delivery(ticket, c, Operation::CompleteFactor, &f, p.expires)?;
        self.session_checkpoint(
            c,
            Operation::CompleteFactor,
            p.expires,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterSessionCommit)?;
        Ok(ticket)
    }
    pub async fn rotate_recovery_codes(
        &self,
        h: &SessionHandle,
        primary: &PrimaryAuthentication,
        code: &TotpCode,
        c: &BrowserContext,
    ) -> Result<FactorActivation, AuthError> {
        let (mut tx, f, now) = self
            .browser_transaction(c, Operation::RotateRecovery)
            .await?;
        let s = self.session_row(&mut tx, h, &f, now, true).await?;
        let mut p = self
            .primary(&mut tx, &primary.evidence, &f.browser, now, true)
            .await?;
        if p.user != s.try_get::<String, _>("user_id")?
            || p.parent.as_deref() != Some(s.try_get("session_id")?)
            || p.parent_generation != Some(s.try_get("generation")?)
            || s.try_get::<&str, _>("assurance")? != "totp"
            || p.factor != 1
        {
            return Err(AuthError::Denied);
        }
        let r = sqlx::query("SELECT * FROM human_factors WHERE user_id=? AND state='active'")
            .bind(&p.user)
            .fetch_one(&mut *tx)
            .await?;
        if !self.factor_rate(&mut tx, &p.user, &f, now).await? {
            return Err(AuthError::Denied);
        }
        let step = matching_step(
            &self.factor_seed(&r, &p.user)?,
            code,
            now,
            r.try_get("last_step")?,
        )?;
        self.record_factor_attempt(&mut tx, &p.user, &f, now, step.is_some())
            .await?;
        let Some(step) = step else {
            self.session_event(
                &mut tx,
                &p.user,
                Some(s.try_get("session_id")?),
                &c.request_id,
                "factor_denied",
                now,
            )
            .await?;
            tx.commit().await?;
            return Err(AuthError::Denied);
        };
        let factor: &str = r.try_get("factor_id")?;
        let batch: i64 = sqlx::query_scalar(
            "SELECT coalesce(max(batch),0)+1 FROM human_recovery_codes WHERE factor_id=?",
        )
        .bind(factor)
        .fetch_one(&mut *tx)
        .await?;
        self.consume_primary(&mut tx, &p, "recovery_rotation", now)
            .await?;
        sqlx::query("UPDATE human_factors SET last_step=? WHERE factor_id=? AND last_step<?")
            .bind(step)
            .bind(factor)
            .bind(step)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE human_recovery_codes SET state='revoked' WHERE factor_id=? AND state='active'",
        )
        .bind(factor)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE human_users SET security_epoch=security_epoch+1 WHERE user_id=? AND security_epoch=?").bind(&p.user).bind(p.epoch).execute(&mut *tx).await?;
        p.epoch += 1;
        self.revoke_user_sessions(&mut tx, &p.user, now).await?;
        let ticket = self
            .create_family(
                &mut tx,
                &p,
                Assurance::Totp,
                s.try_get("device")?,
                &c.request_id,
                now,
            )
            .await?;
        self.session_event(
            &mut tx,
            &p.user,
            Some(ticket.metadata.id.as_str()),
            &c.request_id,
            "recovery_rotated",
            now,
        )
        .await?;
        let recovery = self
            .new_recovery_codes(&mut tx, factor, &p.user, batch, &ticket)
            .await?;
        let ticket = self.scoped_delivery(ticket, c, Operation::RotateRecovery, &f, p.expires)?;
        self.session_checkpoint(
            c,
            Operation::RotateRecovery,
            p.expires.min(self.original_deadline(&mut tx, h, &s).await?),
            Checkpoint::BeforeFactorCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterFactorCommit)?;
        Ok(FactorActivation {
            session: ticket,
            recovery,
        })
    }
    /// Consumes the ephemeral bundle. Lost delivery cannot redisplay plaintext from
    /// SQLite; a new current-primary + TOTP ceremony is required to rotate codes.
    pub async fn deliver_recovery_bundle(
        &self,
        t: &SessionTicket,
        bundle: RecoveryBundle,
        c: &BrowserContext,
    ) -> Result<(), AuthError> {
        let (mut tx, r, f, now) = self.delivery_transaction(t, c).await?;
        if now >= bundle.expires
            || !constant_eq(&bundle.origin_lookup, &self.handle_lookup(&t.handle))
            || bundle.user != r.try_get::<String, _>("user_id")?
            || r.try_get::<&str, _>("assurance")? != "totp"
            || bundle.codes.len() != 10
        {
            return Err(AuthError::Denied);
        }
        for code in &bundle.codes {
            let tag = self.mac(
                b"factor-recovery",
                &[
                    bundle.user.as_bytes(),
                    bundle.factor.as_bytes(),
                    code.0.as_bytes(),
                ],
            );
            if sqlx::query_scalar::<_,i64>("SELECT count(*) FROM human_recovery_codes c JOIN human_factors f USING(factor_id) WHERE c.factor_id=? AND f.user_id=? AND f.state='active' AND c.batch=? AND c.commitment=? AND c.state='active'").bind(&bundle.factor).bind(&bundle.user).bind(bundle.batch).bind(tag.as_slice()).fetch_one(&mut *tx).await?!=1 {return Err(AuthError::Denied)}
        }
        self.session_checkpoint(
            c,
            t.delivery.as_ref().ok_or(AuthError::Denied)?.operation,
            bundle.expires,
            Checkpoint::BeforeFactorCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        self.check_delivery(
            t.delivery.as_ref().ok_or(AuthError::Denied)?,
            c,
            self.now()?,
        )?;
        self.browser_authority.recovery(c, &bundle, self.now()?)
    }
    /// Factor removal and lost-factor recovery approval require the separately reviewed
    /// action-bound authorization/recovery ceremony. An ordinary session cannot bypass it.
    pub async fn remove_totp(
        &self,
        _: &SessionHandle,
        _: &BrowserContext,
    ) -> Result<(), AuthError> {
        Err(AuthError::Unavailable)
    }
}
pub(super) fn matching_step(
    seed: &[u8],
    code: &TotpCode,
    now: i64,
    last: i64,
) -> Result<Option<i64>, AuthError> {
    let engine =
        TOTP::new(Algorithm::SHA1, 6, 0, 30, seed.to_vec()).map_err(|_| AuthError::Denied)?;
    let current = now / 30;
    let mut matched = None;
    for step in [current.saturating_sub(1), current, current + 1] {
        let expected = Zeroizing::new(engine.generate((step * 30) as u64));
        if constant_eq(expected.as_bytes(), code.0.as_bytes()) && step > last {
            matched = Some(step)
        }
    }
    Ok(matched)
}
