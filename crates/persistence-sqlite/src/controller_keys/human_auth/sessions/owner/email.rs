use super::*;
#[derive(Debug, Clone)]
pub struct OwnerChallengeId(String);
impl OwnerChallengeId {
    pub fn parse(s: String) -> Result<Self, AuthError> {
        SessionId::parse(s).map(|s| Self(s.0))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
pub struct PreparedOwnerEmail {
    id: OwnerChallengeId,
    message: Option<MailMessage>,
    grant: DeliveryGrant,
}
impl PreparedOwnerEmail {
    #[cfg(test)]
    pub(super) fn synthetic_recipient_secret(&self) -> ChallengeSecret {
        ChallengeSecret(Zeroizing::new(
            self.message.as_ref().unwrap().token.0.to_string(),
        ))
    }
    pub fn reference(&self) -> &OwnerChallengeId {
        &self.id
    }
}
impl fmt::Debug for PreparedOwnerEmail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PreparedOwnerEmail([redacted])")
    }
}
impl HumanAuth {
    fn owner_email_tag(
        &self,
        a: &SqliteRow,
        r: &SqliteRow,
        secret: &ChallengeSecret,
    ) -> Result<[u8; 32], AuthError> {
        Ok(self.mac(
            b"owner-email-challenge/v1",
            &[
                &self.action_binding(a)?,
                r.try_get::<&str, _>("challenge_id")?.as_bytes(),
                r.try_get::<&str, _>("kind")?.as_bytes(),
                &r.try_get::<Vec<u8>, _>("browser")?,
                &r.try_get::<i64, _>("issued_at")?.to_be_bytes(),
                &r.try_get::<i64, _>("expires_at")?.to_be_bytes(),
                &r.try_get::<Vec<u8>, _>("client_key")?,
                secret.0.as_bytes(),
            ],
        ))
    }
    pub(super) async fn email_only_route(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        s: &Scope,
    ) -> Result<EmailAddress, AuthError> {
        let (_, p) = policy(tx).await?;
        if !p.verified_email || !self.mail_ready() {
            return Err(AuthError::Unavailable);
        }
        let r=sqlx::query("SELECT e.address,e.email_login,EXISTS(SELECT 1 FROM human_passwords p WHERE p.user_id=e.user_id) AS has_local FROM human_emails e WHERE e.user_id=? AND e.verified=1").bind(s.user()?).fetch_one(&mut **tx).await?;
        if r.try_get::<i64, _>("email_login")? != 1
            || p.local_method && r.try_get::<bool, _>("has_local")?
        {
            return Err(AuthError::Denied);
        }
        EmailAddress::parse(r.try_get("address")?)
    }
    pub async fn prepare_owner_email(
        &self,
        q: &OwnerProofRequest<'_>,
        kind: ChallengeKind,
        client: &ClientContext,
    ) -> Result<PreparedOwnerEmail, AuthError> {
        let (mut tx, f, now, s, a) = self.proof_scope(q).await?;
        let address = self.email_only_route(&mut tx, &s).await?;
        let peer = self
            .transport
            .verify(client, TransportOperation::IssueEmail, now)?;
        if peer.browser != f.browser {
            return Err(AuthError::Denied);
        }
        let ip = self.mac(b"client", &[peer.ip.to_string().as_bytes()]);
        let account = self.mac(b"account", &[address.0.as_bytes()]);
        if sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM human_owner_challenges WHERE request_id=?",
        )
        .bind(client.request_id.as_str())
        .fetch_one(&mut *tx)
        .await?
            != 0
        {
            return Err(AuthError::Conflict);
        }
        let counts=sqlx::query("SELECT count(*) FILTER(WHERE a.actor=?) AS user_count,count(*) FILTER(WHERE c.client_key=?) AS ip_count,max(CASE WHEN a.actor=? THEN c.issued_at END) AS last FROM human_owner_challenges c JOIN human_owner_actions a USING(action_id) WHERE c.issued_at>? AND (a.actor=? OR c.client_key=?)").bind(s.user()?).bind(ip.as_slice()).bind(s.user()?).bind(now.saturating_sub(900)).bind(s.user()?).bind(ip.as_slice()).fetch_one(&mut *tx).await?;
        let other=sqlx::query("SELECT count(*) FILTER(WHERE account_key=?) AS user_count,count(*) FILTER(WHERE client_key=?) AS ip_count,max(CASE WHEN account_key=? THEN issued_at END) AS last FROM human_challenges WHERE issued_at>? AND (account_key=? OR client_key=?)").bind(account.as_slice()).bind(ip.as_slice()).bind(account.as_slice()).bind(now.saturating_sub(900)).bind(account.as_slice()).bind(ip.as_slice()).fetch_one(&mut *tx).await?;
        if counts.try_get::<i64, _>("user_count")? + other.try_get::<i64, _>("user_count")? >= 3
            || counts.try_get::<i64, _>("ip_count")? + other.try_get::<i64, _>("ip_count")? >= 30
            || [
                counts.try_get::<Option<i64>, _>("last")?,
                other.try_get::<Option<i64>, _>("last")?,
            ]
            .into_iter()
            .flatten()
            .any(|last| now < last.saturating_add(60))
        {
            return Err(AuthError::Denied);
        }
        let id = OwnerChallengeId(new_id()?);
        let secret = ChallengeSecret(Zeroizing::new(match kind {
            ChallengeKind::MagicLink => hex(&random::<32>()?[..]),
            ChallengeKind::Otp => super::super::super::email::otp()?,
        }));
        let projected=sqlx::query("SELECT ? AS challenge_id,? AS kind,? AS browser,? AS issued_at,? AS expires_at,? AS client_key").bind(&id.0).bind(kind.name()).bind(f.browser.as_slice()).bind(now).bind(now+600).bind(ip.as_slice()).fetch_one(&mut *tx).await?;
        let tag = self.owner_email_tag(&a, &projected, &secret)?;
        one(sqlx::query("INSERT INTO human_owner_challenges(challenge_id,action_id,kind,browser,issued_at,expires_at,client_key,commitment,state,request_id) VALUES(?,?,?,?,?,?,?,?,'queued',?)").bind(&id.0).bind(&q.action.0).bind(kind.name()).bind(f.browser.as_slice()).bind(now).bind(now+600).bind(ip.as_slice()).bind(tag.as_slice()).bind(client.request_id.as_str()).execute(&mut *tx).await?.rows_affected())?;
        let grant = self.delivery_scope(
            q.context,
            Operation::Write,
            &f,
            s.deadline.min(a.try_get::<i64, _>("expires_at")?),
        )?;
        self.session_checkpoint(
            q.context,
            Operation::Write,
            grant.expires,
            Checkpoint::BeforeOwnerCommit,
            (&f, now),
        )?;
        self.owner_transport_current(client, TransportOperation::IssueEmail, &peer)?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterOwnerCommit)?;
        Ok(PreparedOwnerEmail {
            id: id.clone(),
            message: Some(MailMessage {
                challenge: id.0,
                recipient: address,
                kind,
                purpose: ChallengePurpose::OwnerAction,
                token: secret,
            }),
            grant,
        })
    }
    async fn owner_mail_current(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        mail: &PreparedOwnerEmail,
        now: i64,
    ) -> Result<(SqliteRow, SqliteRow), AuthError> {
        let f = self.check_delivery(&mail.grant, &mail.grant.context, now)?;
        let lookup = f.credential.ok_or(AuthError::Denied)?;
        let session = self.session_lookup(tx, &lookup, now).await?;
        let r = sqlx::query("SELECT * FROM human_owner_challenges WHERE challenge_id=?")
            .bind(&mail.id.0)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(AuthError::Denied)?;
        if now >= r.try_get::<i64, _>("expires_at")?
            || now < r.try_get::<i64, _>("issued_at")?
            || !constant_eq(&f.browser, &r.try_get::<Vec<u8>, _>("browser")?)
        {
            return Err(AuthError::Denied);
        }
        let a = sqlx::query("SELECT * FROM human_owner_actions WHERE action_id=?")
            .bind(r.try_get::<&str, _>("action_id")?)
            .fetch_one(&mut **tx)
            .await?;
        let project =
            ProjectId::new(a.try_get::<String, _>("project_id")?).map_err(|_| AuthError::Denied)?;
        let membership = self
            .member_in(tx, &project, session.try_get("user_id")?)
            .await?;
        if membership.role != ProjectRole::Owner {
            return Err(AuthError::Denied);
        }
        let scope = Scope {
            project,
            session,
            membership,
            deadline: mail.grant.expires,
        };
        self.action_in(tx, &OwnerActionId(a.try_get("action_id")?), &scope, now)
            .await?;
        self.email_only_route(tx, &scope).await?;
        Ok((r, a))
    }
    pub async fn dispatch_owner_email(
        &self,
        mail: PreparedOwnerEmail,
    ) -> Result<MailDispatch, AuthError> {
        let message = mail.message.as_ref().ok_or(AuthError::Unavailable)?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let (r, a) = self.owner_mail_current(&mut tx, &mail, self.now()?).await?;
        if r.try_get::<&str, _>("state")? != "queued"
            || !constant_eq(
                &self.owner_email_tag(&a, &r, &message.token)?,
                &r.try_get::<Vec<u8>, _>("commitment")?,
            )
        {
            return Err(AuthError::Denied);
        }
        sqlx::query("UPDATE human_owner_challenges SET state='dispatching' WHERE challenge_id=? AND state='queued'").bind(&mail.id.0).execute(&mut *tx).await?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::BeforeMailSend)?;
        self.check_delivery(&mail.grant, &mail.grant.context, self.now()?)?;
        let observed = self.mail.send(message).await;
        self.checkpoint(Checkpoint::AfterMailSend)?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        self.owner_mail_current(&mut tx, &mail, self.now()?).await?;
        let (state, result) = match observed {
            DeliveryObservation::Delivered if self.delivery.delivered(&mail.id.0, self.now()?) => {
                ("delivered", MailDispatch::DeliveryVerified)
            }
            DeliveryObservation::Delivered | DeliveryObservation::Accepted => {
                ("accepted", MailDispatch::TransportAccepted)
            }
            DeliveryObservation::Failed => ("failed", MailDispatch::NotConfirmed),
            DeliveryObservation::Uncertain => ("uncertain", MailDispatch::NotConfirmed),
        };
        sqlx::query("UPDATE human_owner_challenges SET state=? WHERE challenge_id=? AND state='dispatching'").bind(state).bind(&mail.id.0).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn redeem_owner_email(
        &self,
        q: &OwnerProofRequest<'_>,
        id: &OwnerChallengeId,
        secret: &ChallengeSecret,
        kind: ChallengeKind,
        client: &ClientContext,
        code: Option<&TotpCode>,
    ) -> Result<(), AuthError> {
        let (mut tx, f, now, s, a) = self.proof_scope(q).await?;
        self.email_only_route(&mut tx, &s).await?;
        let peer = self
            .transport
            .verify(client, TransportOperation::ConfirmEmail, now)?;
        let r=sqlx::query("SELECT * FROM human_owner_challenges WHERE challenge_id=? AND action_id=? AND state='delivered' AND failures<5").bind(&id.0).bind(&q.action.0).fetch_optional(&mut *tx).await?.ok_or(AuthError::Denied)?;
        if now < r.try_get::<i64, _>("issued_at")?
            || now >= r.try_get::<i64, _>("expires_at")?
            || kind.name() != r.try_get::<&str, _>("kind")?
            || !constant_eq(&peer.browser, &f.browser)
            || !constant_eq(&f.browser, &r.try_get::<Vec<u8>, _>("browser")?)
            || !self.delivery.delivered(&id.0, now)
        {
            return Err(AuthError::Denied);
        }
        let email = self.email_only_route(&mut tx, &s).await?;
        let account = self.mac(b"account", &[email.0.as_bytes()]);
        let ip = self.mac(b"client", &[peer.ip.to_string().as_bytes()]);
        let attempt = self
            .reserve_attempt(&mut tx, "email", &account, &ip, now)
            .await?
            .ok_or(AuthError::Denied)?;
        let correct = constant_eq(
            &self.owner_email_tag(&a, &r, secret)?,
            &r.try_get::<Vec<u8>, _>("commitment")?,
        );
        let factor = if correct {
            self.fresh_factor(&mut tx, &s, &f, code, now).await?
        } else {
            None
        };
        let good = factor.is_some();
        sqlx::query("UPDATE human_attempts SET state=? WHERE attempt_id=?")
            .bind(if good { "succeeded" } else { "failed" })
            .bind(attempt)
            .execute(&mut *tx)
            .await?;
        if good {
            self.record_owner_proof(
                &mut tx,
                &a,
                kind.name(),
                s.session.try_get::<i64, _>("email_version")?,
                factor.as_ref(),
                now,
            )
            .await?;
            one(sqlx::query(
                "UPDATE human_owner_challenges SET state='consumed' WHERE challenge_id=?",
            )
            .bind(&id.0)
            .execute(&mut *tx)
            .await?
            .rows_affected())?;
        } else {
            sqlx::query("UPDATE human_owner_challenges SET failures=failures+1,state=CASE WHEN failures=4 THEN 'exhausted' ELSE 'delivered' END WHERE challenge_id=?").bind(&id.0).execute(&mut *tx).await?;
        }
        self.governance_event(
            &mut tx,
            q.project,
            s.user()?,
            &q.context.request_id,
            GovernanceAudit {
                kind: if good { "fresh_proof" } else { "proof_denied" },
                reference: &q.action.0,
                at: now,
            },
        )
        .await?;
        self.session_checkpoint(
            q.context,
            Operation::Write,
            s.deadline
                .min(a.try_get::<i64, _>("expires_at")?)
                .min(r.try_get::<i64, _>("expires_at")?),
            Checkpoint::BeforeOwnerCommit,
            (&f, now),
        )?;
        self.owner_transport_current(client, TransportOperation::ConfirmEmail, &peer)?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterOwnerCommit)?;
        if good { Ok(()) } else { Err(AuthError::Denied) }
    }
}
