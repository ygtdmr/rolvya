use super::*;
#[derive(Debug, Clone)]
pub struct InvitationId(String);
impl InvitationId {
    pub fn parse(s: String) -> Result<Self, AuthError> {
        SessionId::parse(s).map(|s| Self(s.0))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
pub struct InvitationSecret(SessionHandle);
impl InvitationSecret {
    #[cfg(test)]
    pub(super) fn synthetic_bearer(&self) -> &str {
        &self.0.0
    }
    pub fn parse(s: String) -> Result<Self, AuthError> {
        SessionHandle::parse(s).map(Self)
    }
}
impl fmt::Debug for InvitationSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InvitationSecret([redacted])")
    }
}
pub struct InvitationTicket {
    pub(super) id: InvitationId,
    pub(super) secret: Option<InvitationSecret>,
    grant: DeliveryGrant,
}
impl InvitationTicket {
    pub fn reference(&self) -> &InvitationId {
        &self.id
    }
}
impl fmt::Debug for InvitationTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InvitationTicket([redacted])")
    }
}
#[derive(Debug)]
pub struct InvitationView {
    pub id: InvitationId,
    pub recipient: HumanId,
    pub role: ProjectRole,
    pub version: i64,
    pub expires_at: i64,
    pub pending: bool,
}
impl HumanAuth {
    fn invite_binding(&self, r: &SqliteRow) -> Result<[u8; 32], AuthError> {
        let mut fields = Vec::new();
        for key in [
            "invitation_id",
            "project_id",
            "issuer",
            "issuer_session",
            "recipient",
            "role",
            "request_id",
        ] {
            fields.push(r.try_get::<String, _>(key)?.into_bytes())
        }
        for key in [
            "issuer_role_version",
            "issuer_security_epoch",
            "email_version",
            "security_epoch",
            "target_version",
            "issued_at",
            "expires_at",
        ] {
            fields.push(r.try_get::<i64, _>(key)?.to_be_bytes().to_vec())
        }
        fields.push(r.try_get::<Vec<u8>, _>("commitment")?);
        Ok(self.mac(
            b"scoped-invitation/v1",
            &fields.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        ))
    }
    fn invitation_commitment(
        &self,
        id: &str,
        p: &ProjectId,
        user: &str,
        secret: &InvitationSecret,
    ) -> [u8; 32] {
        self.mac(
            b"invitation-bearer/v1",
            &[
                id.as_bytes(),
                p.as_str().as_bytes(),
                user.as_bytes(),
                secret.0.0.as_bytes(),
            ],
        )
    }
    pub async fn issue_project_invitation(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        recipient: &HumanId,
        role: ProjectRole,
        c: &BrowserContext,
    ) -> Result<InvitationTicket, AuthError> {
        if role == ProjectRole::Owner {
            return Err(AuthError::Denied);
        }
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Write).await?;
        if f.activity != Activity::Human {
            return Err(AuthError::Denied);
        }
        let s = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Govern, now)
            .await?;
        self.require_enrolled_assurance(&s.session).await?;
        let until = deadline(now, 86400)?;
        let target=sqlx::query("SELECT u.security_epoch,e.version,e.verified FROM human_users u JOIN human_emails e USING(user_id) WHERE user_id=? AND u.state='active' AND e.verified=1").bind(&recipient.0).fetch_optional(&mut *tx).await?.ok_or(AuthError::Denied)?;
        let previous = sqlx::query(
            "SELECT version,state FROM human_memberships WHERE project_id=? AND user_id=?",
        )
        .bind(p.as_str())
        .bind(&recipient.0)
        .fetch_optional(&mut *tx)
        .await?;
        if previous
            .as_ref()
            .is_some_and(|r| r.get::<&str, _>("state") == "active")
        {
            return Err(AuthError::Conflict);
        }
        let target_version = previous
            .as_ref()
            .map(|r| r.get::<i64, _>("version"))
            .unwrap_or(0);
        let grant = self.delivery_scope(c, Operation::Write, &f, s.deadline)?;
        if let Some(r) = sqlx::query("SELECT * FROM human_project_invitations WHERE request_id=?")
            .bind(c.request_id.as_str())
            .fetch_optional(&mut *tx)
            .await?
        {
            if r.try_get::<&str, _>("project_id")? != p.as_str()
                || r.try_get::<&str, _>("issuer")? != s.user()?
                || r.try_get::<&str, _>("recipient")? != recipient.0
                || r.try_get::<&str, _>("role")? != role.name()
                || r.try_get::<i64, _>("target_version")? != target_version
                || !constant_eq(
                    &self.invite_binding(&r)?,
                    &r.try_get::<Vec<u8>, _>("binding")?,
                )
            {
                return Err(AuthError::Conflict);
            }
            return Ok(InvitationTicket {
                id: InvitationId(r.try_get("invitation_id")?),
                secret: None,
                grant,
            });
        }
        let id = InvitationId(new_id()?);
        let secret = InvitationSecret(SessionHandle(Zeroizing::new(hex(&random::<32>()?[..]))));
        let tag = self.invitation_commitment(&id.0, p, &recipient.0, &secret);
        let projected=sqlx::query("SELECT ? AS invitation_id,? AS project_id,? AS issuer,? AS issuer_role_version,? AS issuer_session,? AS issuer_security_epoch,? AS recipient,? AS email_version,? AS security_epoch,? AS target_version,? AS role,? AS issued_at,? AS expires_at,? AS request_id,? AS commitment")
            .bind(&id.0).bind(p.as_str()).bind(s.user()?).bind(s.membership.version).bind(s.session.try_get::<&str,_>("session_id")?).bind(s.session.try_get::<i64,_>("security_epoch")?).bind(&recipient.0).bind(target.try_get::<i64,_>("version")?).bind(target.try_get::<i64,_>("security_epoch")?).bind(target_version).bind(role.name()).bind(now).bind(until).bind(c.request_id.as_str()).bind(tag.as_slice()).fetch_one(&mut *tx).await?;
        let binding = self.invite_binding(&projected)?;
        one(sqlx::query("INSERT INTO human_project_invitations VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,1,'pending')").bind(&id.0).bind(p.as_str()).bind(s.user()?).bind(s.membership.version).bind(s.session.try_get::<&str,_>("session_id")?).bind(s.session.try_get::<i64,_>("security_epoch")?).bind(&recipient.0).bind(target.try_get::<i64,_>("version")?).bind(target.try_get::<i64,_>("security_epoch")?).bind(target_version).bind(role.name()).bind(now).bind(until).bind(c.request_id.as_str()).bind(tag.as_slice()).bind(binding.as_slice()).execute(&mut *tx).await?.rows_affected())?;
        self.governance_event(
            &mut tx,
            p,
            s.user()?,
            &c.request_id,
            GovernanceAudit {
                kind: "invitation_issued",
                reference: &id.0,
                at: now,
            },
        )
        .await?;
        self.session_checkpoint(
            c,
            Operation::Write,
            s.deadline,
            Checkpoint::BeforeOwnerCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterOwnerCommit)?;
        Ok(InvitationTicket {
            id,
            secret: Some(secret),
            grant,
        })
    }
    pub async fn deliver_project_invitation(
        &self,
        h: &SessionHandle,
        t: &InvitationTicket,
        c: &BrowserContext,
    ) -> Result<(), AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Write).await?;
        if f.activity != Activity::Human {
            return Err(AuthError::Denied);
        }
        self.check_delivery(&t.grant, c, now)?;
        let r = sqlx::query(
            "SELECT * FROM human_project_invitations WHERE invitation_id=? AND state='pending'",
        )
        .bind(&t.id.0)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AuthError::Denied)?;
        let p =
            ProjectId::new(r.try_get::<String, _>("project_id")?).map_err(|_| AuthError::Denied)?;
        let s = self
            .project_scope(&mut tx, h, &f, &p, ProjectPermission::Govern, now)
            .await?;
        if s.user()? != r.try_get::<&str, _>("issuer")?
            || s.membership.version != r.try_get::<i64, _>("issuer_role_version")?
            || now >= r.try_get::<i64, _>("expires_at")?
            || !constant_eq(
                &self.invite_binding(&r)?,
                &r.try_get::<Vec<u8>, _>("binding")?,
            )
            || !constant_eq(
                &self.invitation_commitment(
                    &t.id.0,
                    &p,
                    r.try_get("recipient")?,
                    t.secret.as_ref().ok_or(AuthError::Unavailable)?,
                ),
                &r.try_get::<Vec<u8>, _>("commitment")?,
            )
        {
            return Err(AuthError::Denied);
        }
        self.session_checkpoint(
            c,
            Operation::Write,
            s.deadline.min(t.grant.expires),
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        self.check_delivery(&t.grant, c, self.now()?)?;
        self.browser_authority.invitation(c, t, self.now()?)
    }
    pub async fn redeem_project_invitation(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        id: &InvitationId,
        secret: &InvitationSecret,
        c: &BrowserContext,
    ) -> Result<Membership, AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Write).await?;
        if f.activity != Activity::Human {
            return Err(AuthError::Denied);
        }
        let s = self.session_row(&mut tx, h, &f, now, true).await?;
        self.require_enrolled_assurance(&s).await?;
        let user: &str = s.try_get("user_id")?;
        let r = sqlx::query(
            "SELECT * FROM human_project_invitations WHERE invitation_id=? AND project_id=?",
        )
        .bind(&id.0)
        .bind(p.as_str())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AuthError::Denied)?;
        if r.try_get::<&str, _>("state")? != "pending"
            || r.try_get::<&str, _>("recipient")? != user
            || now < r.try_get::<i64, _>("issued_at")?
            || now >= r.try_get::<i64, _>("expires_at")?
            || r.try_get::<i64, _>("security_epoch")? != s.try_get::<i64, _>("security_epoch")?
            || r.try_get::<i64, _>("email_version")? != s.try_get::<i64, _>("email_version")?
            || !constant_eq(
                &self.invite_binding(&r)?,
                &r.try_get::<Vec<u8>, _>("binding")?,
            )
            || !constant_eq(
                &self.invitation_commitment(&id.0, p, user, secret),
                &r.try_get::<Vec<u8>, _>("commitment")?,
            )
        {
            let event = if now >= r.try_get::<i64, _>("expires_at")? {
                "invitation_expired"
            } else {
                "invitation_rejected"
            };
            self.governance_event(
                &mut tx,
                p,
                user,
                &c.request_id,
                GovernanceAudit {
                    kind: event,
                    reference: &id.0,
                    at: now,
                },
            )
            .await?;
            tx.commit().await?;
            return Err(AuthError::Denied);
        }
        let issuer = self.member_in(&mut tx, p, r.try_get("issuer")?).await;
        if !issuer.is_ok_and(|m| {
            m.role == ProjectRole::Owner && m.version == r.get::<i64, _>("issuer_role_version")
        }) {
            self.governance_event(
                &mut tx,
                p,
                user,
                &c.request_id,
                GovernanceAudit {
                    kind: "invitation_rejected",
                    reference: &id.0,
                    at: now,
                },
            )
            .await?;
            tx.commit().await?;
            return Err(AuthError::Denied);
        }
        let issuer_epoch: i64 =
            sqlx::query_scalar("SELECT security_epoch FROM human_users WHERE user_id=?")
                .bind(r.try_get::<&str, _>("issuer")?)
                .fetch_one(&mut *tx)
                .await?;
        if issuer_epoch != r.try_get::<i64, _>("issuer_security_epoch")? {
            return Err(AuthError::Denied);
        }
        let version = sqlx::query_scalar::<_, i64>(
            "SELECT version FROM human_memberships WHERE project_id=? AND user_id=?",
        )
        .bind(p.as_str())
        .bind(user)
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or(0);
        if version != r.try_get::<i64, _>("target_version")? {
            return Err(AuthError::StaleVersion);
        }
        let role = ProjectRole::parse(r.try_get("role")?)?;
        if role == ProjectRole::Owner {
            return Err(AuthError::Denied);
        }
        self.set_membership(
            &mut tx,
            p,
            user,
            version,
            MemberMutation {
                role,
                active: true,
                reference: &id.0,
            },
        )
        .await?;
        one(sqlx::query("UPDATE human_project_invitations SET state='consumed',version=version+1 WHERE invitation_id=? AND state='pending'").bind(&id.0).execute(&mut *tx).await?.rows_affected())?;
        let event = self
            .governance_event(
                &mut tx,
                p,
                user,
                &c.request_id,
                GovernanceAudit {
                    kind: "invitation_redeemed",
                    reference: &id.0,
                    at: now,
                },
            )
            .await?;
        sqlx::query("INSERT INTO human_owner_notifications(event_sequence,recipient) VALUES(?,?)")
            .bind(event)
            .bind(user)
            .execute(&mut *tx)
            .await?;
        self.session_checkpoint(
            c,
            Operation::Write,
            self.original_deadline(&mut tx, h, &s)
                .await?
                .min(r.try_get::<i64, _>("expires_at")?),
            Checkpoint::BeforeOwnerCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterOwnerCommit)?;
        Ok(Membership {
            user: HumanId(user.to_owned()),
            role,
            version: version + 1,
            active: true,
        })
    }
    pub async fn revoke_project_invitation(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        id: &InvitationId,
        version: i64,
        c: &BrowserContext,
    ) -> Result<(), AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Write).await?;
        if f.activity != Activity::Human {
            return Err(AuthError::Denied);
        }
        let s = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Govern, now)
            .await?;
        let result=sqlx::query("UPDATE human_project_invitations SET state='revoked',version=version+1 WHERE invitation_id=? AND project_id=? AND version=? AND state='pending'").bind(&id.0).bind(p.as_str()).bind(version).execute(&mut *tx).await?;
        if result.rows_affected() != 1 {
            return Err(AuthError::StaleVersion);
        }
        self.governance_event(
            &mut tx,
            p,
            s.user()?,
            &c.request_id,
            GovernanceAudit {
                kind: "invitation_revoked",
                reference: &id.0,
                at: now,
            },
        )
        .await?;
        self.session_checkpoint(
            c,
            Operation::Write,
            s.deadline,
            Checkpoint::BeforeOwnerCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn project_invitations(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        after: Option<&InvitationId>,
        limit: u16,
        c: &BrowserContext,
    ) -> Result<Vec<InvitationView>, AuthError> {
        if !(1..=100).contains(&limit) {
            return Err(AuthError::InvalidInput);
        }
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Read).await?;
        let s = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Govern, now)
            .await?;
        let rows=sqlx::query("SELECT * FROM human_project_invitations WHERE project_id=? AND invitation_id>? ORDER BY invitation_id LIMIT ?").bind(p.as_str()).bind(after.map(|i|i.0.as_str()).unwrap_or("")).bind(limit).fetch_all(&mut *tx).await?;
        let out = rows
            .iter()
            .map(|r| {
                Ok(InvitationView {
                    id: InvitationId(r.try_get("invitation_id")?),
                    recipient: HumanId(r.try_get("recipient")?),
                    role: ProjectRole::parse(r.try_get("role")?)?,
                    version: r.try_get("version")?,
                    expires_at: r.try_get::<i64, _>("expires_at")?,
                    pending: r.try_get::<&str, _>("state")? == "pending"
                        && now < r.try_get::<i64, _>("expires_at")?,
                })
            })
            .collect::<Result<Vec<_>, AuthError>>()?;
        self.session_checkpoint(
            c,
            Operation::Read,
            s.deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(out)
    }
}
