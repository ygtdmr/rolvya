use super::*;
use argon2::{
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
    password_hash::SaltString,
};
use tokio::sync::Semaphore;
/// Engineering defaults: OWASP minimum Argon2id profile (19 MiB, two passes,
/// one lane). At most two simultaneous jobs, including canceled callers' jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordParameters {
    memory_kib: u32,
    passes: u32,
}
impl Default for PasswordParameters {
    fn default() -> Self {
        Self {
            memory_kib: 19456,
            passes: 2,
        }
    }
}
impl PasswordParameters {
    pub fn new(memory_kib: u32, passes: u32) -> Result<Self, AuthError> {
        if !(19456..=65536).contains(&memory_kib) || !(2..=4).contains(&passes) {
            return Err(AuthError::InvalidInput);
        }
        Ok(Self { memory_kib, passes })
    }
    fn params(self) -> Result<Params, AuthError> {
        Params::new(self.memory_kib, self.passes, 1, Some(32)).map_err(|_| AuthError::InvalidInput)
    }
    fn hasher(self) -> Result<Argon2<'static>, AuthError> {
        Ok(Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            self.params()?,
        ))
    }
}
pub(super) struct PasswordEngine {
    profile: PasswordParameters,
    pub(super) slots: Arc<Semaphore>,
    dummy: String,
    #[cfg(test)]
    pub(super) started: Option<Arc<dyn Fn() + Send + Sync>>,
}
impl PasswordEngine {
    #[cfg(test)]
    pub(super) fn synthetic() -> Self {
        Self::new(PasswordParameters {
            memory_kib: 32,
            passes: 1,
        })
        .unwrap()
    }
    pub(super) fn new(profile: PasswordParameters) -> Result<Self, AuthError> {
        let random = random::<32>()?;
        let dummy = Self::hash_sync(&random[..], profile)?;
        Ok(Self {
            profile,
            slots: Arc::new(Semaphore::new(2)),
            dummy,
            #[cfg(test)]
            started: None,
        })
    }
    fn hash_sync(value: &[u8], profile: PasswordParameters) -> Result<String, AuthError> {
        let salt =
            SaltString::encode_b64(&random::<16>()?[..]).map_err(|_| AuthError::Randomness)?;
        profile
            .hasher()?
            .hash_password(value, &salt)
            .map(|h| h.to_string())
            .map_err(|_| AuthError::Unavailable)
    }
    pub(super) async fn hash(&self, password: &Password) -> Result<String, AuthError> {
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| AuthError::Busy)?;
        let input = Zeroizing::new(password.0.to_vec());
        let profile = self.profile;
        #[cfg(test)]
        let started = self.started.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            #[cfg(test)]
            if let Some(started) = started {
                started();
            }
            Self::hash_sync(&input, profile)
        })
        .await
        .map_err(|_| AuthError::Unavailable)?
    }
    pub(super) fn valid_phc(&self, value: &str) -> bool {
        if value.len() > 512 {
            return false;
        }
        let Ok(h) = PasswordHash::new(value) else {
            return false;
        };
        let p = &h.params;
        h.algorithm.as_str() == "argon2id"
            && h.version == Some(19)
            && p.iter().count() == 3
            && p.get_decimal("m")
                .is_some_and(|m| (self.profile.memory_kib.min(19456)..=65536).contains(&m))
            && p.get_decimal("t")
                .is_some_and(|t| (self.profile.passes.min(2)..=4).contains(&t))
            && p.get_decimal("p") == Some(1)
            && h.hash.is_some_and(|h| h.len() == 32)
            && h.salt.is_some_and(|s| (22..=43).contains(&s.len()))
    }
    pub(super) async fn verify(
        &self,
        password: &Password,
        phc: Option<&str>,
    ) -> Result<bool, AuthError> {
        let valid = phc.is_some_and(|p| self.valid_phc(p));
        let hash = if valid {
            phc.unwrap_or(&self.dummy)
        } else {
            &self.dummy
        }
        .to_owned();
        let input = Zeroizing::new(password.0.to_vec());
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| AuthError::Busy)?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let result = PasswordHash::new(&hash)
                .is_ok_and(|h| Argon2::default().verify_password(&input, &h).is_ok());
            Ok(result && valid)
        })
        .await
        .map_err(|_| AuthError::Unavailable)?
    }
}
impl HumanAuth {
    pub async fn authenticate_password(
        &self,
        email: &EmailAddress,
        password: &Password,
        client: &ClientContext,
    ) -> Result<AuthenticationOutcome, AuthError> {
        let now = self.now()?;
        let peer = self
            .transport
            .verify(client, TransportOperation::Password, now)?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let login_policy = policy(&mut tx).await?;
        if !login_policy.1.local_method {
            return Ok(AuthenticationOutcome::Denied);
        }
        let account = self.mac(b"account", &[email.0.as_bytes()]);
        let address = self.mac(b"client", &[peer.ip.to_string().as_bytes()]);
        let Some(attempt) = self
            .reserve_attempt(&mut tx, "password", &account, &address, now)
            .await?
        else {
            return Ok(AuthenticationOutcome::Denied);
        };
        let row=sqlx::query("SELECT u.user_id,u.state,u.security_epoch,u.factor_required,e.version AS email_version,e.verified,p.version AS credential_version,p.phc,p.binding FROM human_emails e JOIN human_users u USING(user_id) LEFT JOIN human_passwords p USING(user_id) WHERE e.address=?").bind(&email.0).fetch_optional(&mut *tx).await?;
        tx.commit().await?;
        let record = row.as_ref().and_then(|r| {
            let id: String = r.try_get("user_id").ok()?;
            let version: i64 = r.try_get("credential_version").ok()?;
            let phc: String = r.try_get("phc").ok()?;
            let tag: Vec<u8> = r.try_get("binding").ok()?;
            let expected = self.password_binding(&id, version, &phc);
            (constant_eq(&tag, &expected)
                && r.try_get::<&str, _>("state").ok()? == "active"
                && r.try_get::<i64, _>("verified").ok()? == 1)
                .then_some((id, version, phc))
        });
        let success = self
            .passwords
            .verify(password, record.as_ref().map(|r| r.2.as_str()))
            .await?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let end = self.now()?;
        let current=sqlx::query("SELECT u.security_epoch,u.factor_required,e.version AS email_version,p.version,p.phc,p.binding FROM human_users u JOIN human_emails e USING(user_id) JOIN human_passwords p USING(user_id) WHERE u.user_id=? AND u.state='active' AND e.verified=1 AND e.address=?")
            .bind(record.as_ref().map(|r|r.0.as_str()).unwrap_or("")).bind(&email.0).fetch_optional(&mut *tx).await?;
        let unchanged = if let (Some(old), Some(new)) = (row.as_ref(), current.as_ref()) {
            old.try_get::<i64, _>("security_epoch")? == new.try_get::<i64, _>("security_epoch")?
                && old.try_get::<i64, _>("email_version")?
                    == new.try_get::<i64, _>("email_version")?
                && record.as_ref().is_some_and(|r| {
                    new.get::<i64, _>("version") == r.1
                        && new.get::<String, _>("phc") == r.2
                        && constant_eq(
                            &new.get::<Vec<u8>, _>("binding"),
                            &self.password_binding(&r.0, r.1, &r.2),
                        )
                })
        } else {
            false
        };
        let allowed = success
            && unchanged
            && end >= now
            && end < now.checked_add(60).ok_or(AuthError::Unavailable)?
            && policy(&mut tx).await? == login_policy
            && self
                .transport
                .verify(client, TransportOperation::Password, end)
                .is_ok();
        sqlx::query("UPDATE human_attempts SET state=? WHERE attempt_id=? AND state='pending'")
            .bind(if allowed { "succeeded" } else { "failed" })
            .bind(&attempt)
            .execute(&mut *tx)
            .await?;
        let result = if allowed {
            let row = current.as_ref().ok_or(AuthError::Storage)?;
            let (id, version, _) = record.as_ref().ok_or(AuthError::Storage)?;
            AuthenticationOutcome::Verified(
                self.record_primary(
                    &mut tx,
                    PrimaryRecord {
                        user: id,
                        method: PrimaryMethod::Password,
                        version: *version,
                        epoch: row.try_get("security_epoch")?,
                        policy: login_policy.0,
                        factor: row.try_get("factor_required")?,
                        source: &attempt,
                        email_version: row.try_get("email_version")?,
                        context: &peer,
                        now: end,
                    },
                    &client.request_id,
                )
                .await?,
            )
        } else {
            audit(&mut tx, None, &client.request_id, "password_denied", end).await?;
            AuthenticationOutcome::Denied
        };
        // Time and trusted request authority may expire while SQLite awaited writes.
        if allowed
            && (self.now()? >= now + 60
                || self
                    .transport
                    .verify(client, TransportOperation::Password, self.now()?)
                    .is_err())
        {
            return Ok(AuthenticationOutcome::Denied);
        }
        tx.commit().await?;
        Ok(result)
    }
    pub(super) fn password_binding(&self, id: &str, version: i64, phc: &str) -> [u8; 32] {
        self.mac(
            b"password-binding",
            &[id.as_bytes(), &version.to_be_bytes(), phc.as_bytes()],
        )
    }
    pub(super) async fn reserve_attempt(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        channel: &str,
        account: &[u8],
        client: &[u8],
        now: i64,
    ) -> Result<Option<String>, AuthError> {
        let counts=sqlx::query("SELECT count(*) FILTER(WHERE account_key=?) AS a,count(*) FILTER(WHERE client_key=?) AS c FROM human_attempts WHERE channel=? AND started_at>? AND state IN ('pending','failed') AND (account_key=? OR client_key=?)")
            .bind(account).bind(client).bind(channel).bind(now.saturating_sub(900)).bind(account).bind(client).fetch_one(&mut **tx).await?;
        if counts.get::<i64, _>("a") >= 5 || counts.get::<i64, _>("c") >= 30 {
            return Ok(None);
        }
        let id = new_id()?;
        sqlx::query("INSERT INTO human_attempts VALUES(?,?,?,?,?,'pending')")
            .bind(&id)
            .bind(channel)
            .bind(account)
            .bind(client)
            .bind(now)
            .execute(&mut **tx)
            .await?;
        Ok(Some(id))
    }
}
