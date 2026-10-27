use super::*;
impl HumanAuth {
    /// Generic local receipt only. Dispatch is separate so SMTP timing never makes
    /// this response distinguish enrolled and unknown recipients.
    pub async fn prepare_email(
        &self,
        r: &ChallengeRequest,
    ) -> Result<PreparedChallenge, AuthError> {
        let now = self.now()?;
        let peer = self
            .transport
            .verify(&r.client, TransportOperation::IssueEmail, now)?;
        if !self.mail_ready() {
            return Err(AuthError::Unavailable);
        }
        let request_commitment = self.mac(
            b"challenge-request",
            &[
                r.email.0.as_bytes(),
                r.kind.name().as_bytes(),
                r.purpose.name().as_bytes(),
                &peer.browser,
                b"primary-authentication",
            ],
        );
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let p = policy(&mut tx).await?;
        if r.purpose == ChallengePurpose::Login && !p.1.verified_email {
            return Err(AuthError::Unavailable);
        }
        if let Some(old) = sqlx::query(
            "SELECT challenge_id,request_commitment FROM human_challenges WHERE request_id=?",
        )
        .bind(r.client.request_id.as_str())
        .fetch_optional(&mut *tx)
        .await?
        {
            if !constant_eq(
                &old.try_get::<Vec<u8>, _>("request_commitment")?,
                &request_commitment,
            ) {
                return Err(AuthError::Conflict);
            }
            return Ok(self.prepared(old.try_get("challenge_id")?, None, r.client.clone()));
        }
        let account = self.mac(b"account", &[r.email.0.as_bytes()]);
        let client = self.mac(b"client", &[peer.ip.to_string().as_bytes()]);
        let quota=sqlx::query("SELECT count(*) FILTER(WHERE account_key=?) AS a,count(*) FILTER(WHERE client_key=?) AS c, max(CASE WHEN account_key=? THEN issued_at END) AS last FROM human_challenges WHERE issued_at>? AND (account_key=? OR client_key=?)")
            .bind(account.as_slice()).bind(client.as_slice()).bind(account.as_slice()).bind(now.saturating_sub(900)).bind(account.as_slice()).bind(client.as_slice()).fetch_one(&mut *tx).await?;
        if quota.get::<i64, _>("a") >= 3
            || quota.get::<i64, _>("c") >= 30
            || quota
                .get::<Option<i64>, _>("last")
                .is_some_and(|t| now < t + 60)
        {
            return Ok(self.prepared(new_id()?, None, r.client.clone()));
        }
        let user=sqlx::query("SELECT u.user_id,u.state,u.security_epoch,e.version,e.verified,e.email_login FROM human_users u JOIN human_emails e USING(user_id) WHERE e.address=?").bind(&r.email.0).fetch_optional(&mut *tx).await?;
        let eligible = user.as_ref().is_some_and(|u| {
            u.get::<&str, _>("state") == "active"
                && match r.purpose {
                    ChallengePurpose::Login => {
                        u.get::<i64, _>("verified") == 1 && u.get::<i64, _>("email_login") == 1
                    }
                    ChallengePurpose::VerifyEmail => u.get::<i64, _>("verified") == 0,
                }
        });
        let id = new_id()?;
        let token = ChallengeSecret(Zeroizing::new(match r.kind {
            ChallengeKind::MagicLink => hex(&random::<32>()?[..]),
            ChallengeKind::Otp => otp()?,
        }));
        let uid = user.as_ref().map(|u| u.get::<String, _>("user_id"));
        let version = user
            .as_ref()
            .map(|u| u.get::<i64, _>("version"))
            .unwrap_or(0);
        let epoch = user
            .as_ref()
            .map(|u| u.get::<i64, _>("security_epoch"))
            .unwrap_or(0);
        let binding = ChallengeBinding {
            id: &id,
            user: uid.as_deref().unwrap_or(""),
            email: &r.email.0,
            email_version: version,
            epoch,
            policy: p.0,
            kind: r.kind.name(),
            purpose: r.purpose.name(),
            browser: &peer.browser,
            issued: now,
            expires: now + 600,
        };
        let tag = self.challenge_mac(&binding, &token);
        sqlx::query("INSERT INTO human_challenges(challenge_id,user_id,address,email_version,security_epoch,policy_version,kind,purpose,browser,account_key,client_key,request_id,request_commitment,token_commitment,key_id,key_version,issued_at,expires_at,state) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&id).bind(uid.as_deref()).bind(&r.email.0).bind(version).bind(epoch).bind(p.0).bind(r.kind.name()).bind(r.purpose.name()).bind(peer.browser.as_slice()).bind(account.as_slice()).bind(client.as_slice()).bind(r.client.request_id.as_str()).bind(request_commitment.as_slice()).bind(tag.as_slice()).bind(self.key.metadata.key.0.as_slice()).bind(self.key.metadata.key_version as i64).bind(now).bind(now+600).bind(if eligible {"queued"}else{"failed"}).execute(&mut *tx).await?;
        audit(&mut tx, None, &r.client.request_id, "challenge_queued", now).await?;
        self.checkpoint(Checkpoint::BeforeChallengeCommit)?;
        if self.now()? < now
            || self.now()? >= now + 600
            || self
                .transport
                .verify(&r.client, TransportOperation::IssueEmail, self.now()?)
                .is_err()
        {
            return Err(AuthError::Denied);
        }
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterChallengeCommit)?;
        let message = eligible.then_some(MailMessage {
            challenge: id.clone(),
            recipient: r.email.clone(),
            kind: r.kind,
            purpose: r.purpose,
            token,
        });
        Ok(self.prepared(id, message, r.client.clone()))
    }
    fn prepared(
        &self,
        id: String,
        message: Option<MailMessage>,
        client: ClientContext,
    ) -> PreparedChallenge {
        let seal = self.mac(
            b"prepared-mail",
            &[
                id.as_bytes(),
                client.request_id.as_str().as_bytes(),
                client.transport_evidence.as_str().as_bytes(),
            ],
        );
        PreparedChallenge {
            receipt: ChallengeReceipt { reference: id },
            message,
            seal,
            client,
        }
    }
    /// At most one transport attempt per ephemeral handle/committed challenge.
    /// Lost acknowledgement is uncertain, never proof of non-delivery or permission
    /// to blindly send again. No raw bearer can be reconstructed from SQLite.
    pub async fn dispatch_email(
        &self,
        prepared: PreparedChallenge,
    ) -> Result<MailDispatch, AuthError> {
        let expected = self.mac(
            b"prepared-mail",
            &[
                prepared.receipt.reference.as_bytes(),
                prepared.client.request_id.as_str().as_bytes(),
                prepared.client.transport_evidence.as_str().as_bytes(),
            ],
        );
        if !constant_eq(&expected, &prepared.seal) {
            return Err(AuthError::Denied);
        }
        let Some(message) = prepared.message else {
            return Ok(MailDispatch::NotConfirmed);
        };
        let now = self.now()?;
        let peer = self
            .transport
            .verify(&prepared.client, TransportOperation::IssueEmail, now)?;
        if !self.mail_ready() {
            return Err(AuthError::Unavailable);
        }
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT * FROM human_challenges WHERE challenge_id=?")
            .bind(&message.challenge)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AuthError::Denied)?;
        if row.get::<&str, _>("state") != "queued"
            || message.recipient.0 != row.get::<String, _>("address")
            || message.kind.name() != row.get::<String, _>("kind")
            || message.purpose.name() != row.get::<String, _>("purpose")
            || !self.challenge_current(&mut tx, &row, &peer, now).await?
            || !constant_eq(
                &self.row_challenge_mac(&row, &message.token)?,
                &row.get::<Vec<u8>, _>("token_commitment"),
            )
        {
            return Ok(MailDispatch::NotConfirmed);
        }
        sqlx::query("UPDATE human_challenges SET state='dispatching' WHERE challenge_id=? AND state='queued'").bind(&message.challenge).execute(&mut *tx).await?;
        audit(
            &mut tx,
            None,
            &prepared.client.request_id,
            "mail_dispatching",
            now,
        )
        .await?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::BeforeMailSend)?;
        // No SQLite write lock is held over the transport call.
        let observation = if self
            .transport
            .verify(
                &prepared.client,
                TransportOperation::IssueEmail,
                self.now()?,
            )
            .is_ok()
        {
            self.mail.send(&message).await
        } else {
            DeliveryObservation::Uncertain
        };
        self.checkpoint(Checkpoint::AfterMailSend)?;
        let verified = matches!(observation, DeliveryObservation::Delivered)
            && self.delivery.delivered(&message.challenge, self.now()?);
        let (state, event, outcome) = if verified {
            (
                "delivered",
                "mail_delivered",
                MailDispatch::DeliveryVerified,
            )
        } else if matches!(observation, DeliveryObservation::Accepted) {
            ("accepted", "mail_accepted", MailDispatch::TransportAccepted)
        } else {
            ("uncertain", "mail_uncertain", MailDispatch::NotConfirmed)
        };
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query(
            "UPDATE human_challenges SET state=? WHERE challenge_id=? AND state='dispatching'",
        )
        .bind(state)
        .bind(&message.challenge)
        .execute(&mut *tx)
        .await?;
        audit(
            &mut tx,
            None,
            &prepared.client.request_id,
            event,
            self.now()?,
        )
        .await?;
        tx.commit().await?;
        Ok(outcome)
    }
    pub async fn redeem_email(
        &self,
        receipt: &ChallengeReceipt,
        secret: &ChallengeSecret,
        kind: ChallengeKind,
        purpose: ChallengePurpose,
        confirmation: Confirmation,
        client: &ClientContext,
    ) -> Result<Redemption, AuthError> {
        if confirmation == Confirmation::Preview {
            return Ok(Redemption::ConfirmationRequired);
        }
        let now = self.now()?;
        let peer = self
            .transport
            .verify(client, TransportOperation::ConfirmEmail, now)?;
        if !self.mail_ready() {
            return Err(AuthError::Unavailable);
        }
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let Some(row) = sqlx::query("SELECT * FROM human_challenges WHERE challenge_id=?")
            .bind(&receipt.reference)
            .fetch_optional(&mut *tx)
            .await?
        else {
            return Ok(Redemption::Denied);
        };
        if row.get::<&str, _>("state") != "delivered"
            || row.get::<i64, _>("failures") >= 5
            || row.get::<&str, _>("kind") != kind.name()
            || row.get::<&str, _>("purpose") != purpose.name()
            || !self.challenge_current(&mut tx, &row, &peer, now).await?
        {
            return Ok(Redemption::Denied);
        }
        let Some(attempt) = self
            .reserve_attempt(
                &mut tx,
                "email",
                &row.get::<Vec<u8>, _>("account_key"),
                &self.mac(b"client", &[peer.ip.to_string().as_bytes()]),
                now,
            )
            .await?
        else {
            return Ok(Redemption::Denied);
        };
        let correct = constant_eq(
            &self.row_challenge_mac(&row, secret)?,
            &row.get::<Vec<u8>, _>("token_commitment"),
        );
        if !correct {
            sqlx::query("UPDATE human_challenges SET failures=failures+1,state=CASE WHEN failures=4 THEN 'exhausted' ELSE 'delivered' END WHERE challenge_id=?").bind(&receipt.reference).execute(&mut *tx).await?;
            sqlx::query("UPDATE human_attempts SET state='failed' WHERE attempt_id=?")
                .bind(attempt)
                .execute(&mut *tx)
                .await?;
            audit(&mut tx, None, &client.request_id, "challenge_denied", now).await?;
            tx.commit().await?;
            return Ok(Redemption::Denied);
        }
        sqlx::query("UPDATE human_challenges SET state='consumed' WHERE challenge_id=? AND state='delivered'").bind(&receipt.reference).execute(&mut *tx).await?;
        sqlx::query("UPDATE human_attempts SET state='succeeded' WHERE attempt_id=?")
            .bind(attempt)
            .execute(&mut *tx)
            .await?;
        let user: String = row.try_get("user_id")?;
        let result = match purpose {
            ChallengePurpose::VerifyEmail => {
                sqlx::query("UPDATE human_emails SET verified=1,version=version+1 WHERE user_id=? AND verified=0").bind(&user).execute(&mut *tx).await?;
                sqlx::query(
                    "UPDATE human_users SET security_epoch=security_epoch+1 WHERE user_id=?",
                )
                .bind(&user)
                .execute(&mut *tx)
                .await?;
                audit(
                    &mut tx,
                    Some(&user),
                    &client.request_id,
                    "email_verified",
                    now,
                )
                .await?;
                Redemption::EmailVerified
            }
            ChallengePurpose::Login => {
                let factor =
                    sqlx::query_scalar("SELECT factor_required FROM human_users WHERE user_id=?")
                        .bind(&user)
                        .fetch_one(&mut *tx)
                        .await?;
                Redemption::PrimaryVerified(
                    self.record_primary(
                        &mut tx,
                        PrimaryRecord {
                            user: &user,
                            method: match kind {
                                ChallengeKind::MagicLink => PrimaryMethod::MagicLink,
                                ChallengeKind::Otp => PrimaryMethod::Otp,
                            },
                            version: row.get("email_version"),
                            epoch: row.get("security_epoch"),
                            policy: row.get("policy_version"),
                            factor,
                            source: &receipt.reference,
                            now,
                        },
                        &client.request_id,
                    )
                    .await?,
                )
            }
        };
        self.checkpoint(Checkpoint::BeforeConsumptionCommit)?;
        if self.now()? >= row.get::<i64, _>("expires_at")
            || self.now()? < now
            || self
                .transport
                .verify(client, TransportOperation::ConfirmEmail, self.now()?)
                .is_err()
        {
            return Ok(Redemption::Denied);
        }
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterConsumptionCommit)?;
        Ok(result)
    }
    async fn challenge_current(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        r: &SqliteRow,
        peer: &VerifiedTransport,
        now: i64,
    ) -> Result<bool, AuthError> {
        if now < r.get::<i64, _>("issued_at")
            || now >= r.get::<i64, _>("expires_at")
            || r.get::<i64, _>("expires_at") != r.get::<i64, _>("issued_at") + 600
            || !constant_eq(&r.get::<Vec<u8>, _>("browser"), &peer.browser)
            || r.get::<Vec<u8>, _>("key_id") != self.key.metadata.key.0
            || r.get::<i64, _>("key_version") != self.key.metadata.key_version as i64
        {
            return Ok(false);
        }
        let p = policy(tx).await?;
        if p.0 != r.get::<i64, _>("policy_version")
            || (r.get::<&str, _>("purpose") == "login" && !p.1.verified_email)
        {
            return Ok(false);
        }
        let Some(user)=sqlx::query("SELECT u.*,e.version,e.address,e.verified,e.email_login FROM human_users u JOIN human_emails e USING(user_id) WHERE user_id=?").bind(r.get::<Option<String>,_>("user_id")).fetch_optional(&mut **tx).await? else {return Ok(false)};
        Ok(user.get::<&str, _>("state") == "active"
            && user.get::<i64, _>("security_epoch") == r.get::<i64, _>("security_epoch")
            && user.get::<i64, _>("version") == r.get::<i64, _>("email_version")
            && user.get::<&str, _>("address") == r.get::<&str, _>("address")
            && match r.get::<&str, _>("purpose") {
                "login" => {
                    user.get::<i64, _>("verified") == 1 && user.get::<i64, _>("email_login") == 1
                }
                "verify_email" => user.get::<i64, _>("verified") == 0,
                _ => false,
            })
    }
    pub(super) fn row_challenge_mac(
        &self,
        r: &SqliteRow,
        token: &ChallengeSecret,
    ) -> Result<[u8; 32], AuthError> {
        Ok(self.challenge_mac(
            &ChallengeBinding {
                id: r.try_get("challenge_id")?,
                user: r.try_get::<Option<&str>, _>("user_id")?.unwrap_or(""),
                email: r.try_get("address")?,
                email_version: r.try_get("email_version")?,
                epoch: r.try_get("security_epoch")?,
                policy: r.try_get("policy_version")?,
                kind: r.try_get("kind")?,
                purpose: r.try_get("purpose")?,
                browser: &r.try_get::<Vec<u8>, _>("browser")?,
                issued: r.try_get("issued_at")?,
                expires: r.try_get("expires_at")?,
            },
            token,
        ))
    }
    fn challenge_mac(&self, b: &ChallengeBinding<'_>, token: &ChallengeSecret) -> [u8; 32] {
        self.mac(
            b"email-challenge",
            &[
                b.id.as_bytes(),
                b.user.as_bytes(),
                b.email.as_bytes(),
                &b.email_version.to_be_bytes(),
                &b.epoch.to_be_bytes(),
                &b.policy.to_be_bytes(),
                b.kind.as_bytes(),
                b.purpose.as_bytes(),
                b.browser,
                &b.issued.to_be_bytes(),
                &b.expires.to_be_bytes(),
                token.0.as_bytes(),
            ],
        )
    }
}
struct ChallengeBinding<'a> {
    id: &'a str,
    user: &'a str,
    email: &'a str,
    email_version: i64,
    epoch: i64,
    policy: i64,
    kind: &'a str,
    purpose: &'a str,
    browser: &'a [u8],
    issued: i64,
    expires: i64,
}
fn otp() -> Result<String, AuthError> {
    // Uniform 8-digit decimal codes via bounded rejection sampling, not modulo bias.
    const SPACE: u64 = 100_000_000;
    const LIMIT: u64 = (u32::MAX as u64 + 1) / SPACE * SPACE;
    for _ in 0..16 {
        let n = u32::from_be_bytes(*random::<4>()?) as u64;
        if n < LIMIT {
            return Ok(format!("{:08}", n % SPACE));
        }
    }
    Err(AuthError::Randomness)
}
