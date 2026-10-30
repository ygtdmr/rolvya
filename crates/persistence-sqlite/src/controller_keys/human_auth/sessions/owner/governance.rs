use super::*;
use crate::finance::{
    EvidenceRef, FinancialCommand, FinancialReceipt, FinancialRequest, OwnerProof,
};

/// Constructible only here, after current DB/session/membership/proof verification.
/// Not serializable, not public and not an external policy callback.
pub(crate) struct VerifiedBudgetApproval {
    request: FinancialRequest,
    at: i64,
}
impl VerifiedBudgetApproval {
    pub(crate) fn request(&self) -> &FinancialRequest {
        &self.request
    }
    pub(crate) fn at(&self) -> i64 {
        self.at
    }
}
impl HumanAuth {
    pub(super) async fn validate_target(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        project: &ProjectId,
        payload: &Payload,
        actor: &str,
    ) -> Result<(), AuthError> {
        match payload {
            Payload::AwsInfrastructure { proposal, digest } => {
                crate::controller_keys::worker_auth::aws_fargate::consent::validate(
                    tx,
                    &self.key,
                    proposal,
                    digest,
                    actor,
                    project,
                    self.now()?,
                )
                .await
                .map_err(|_| AuthError::Denied)?;
            }
            Payload::ProviderConnection { proposal, digest } => {
                let t = self.now()?;
                crate::controller_keys::provider_accounts::store::validate_proposal(
                    tx, &self.key, proposal, digest, actor, project, t,
                )
                .await
                .map_err(|_| AuthError::Denied)?;
            }
            Payload::Budget {
                expected_version,
                currency,
                previous,
                ..
            } => {
                let row=sqlx::query("SELECT version,new_ceiling FROM budget_approvals WHERE project_id=? ORDER BY version DESC LIMIT 1").bind(project.as_str()).fetch_optional(&mut **tx).await?;
                let (v, old) = if let Some(r) = row {
                    (
                        r.try_get::<i64, _>("version")?,
                        r.try_get::<i64, _>("new_ceiling")?,
                    )
                } else {
                    (0, 0)
                };
                if v != *expected_version || old != *previous {
                    return Err(AuthError::StaleVersion);
                }
                let existing: Option<String> = sqlx::query_scalar(
                    "SELECT currency FROM financial_accounts WHERE project_id=?",
                )
                .bind(project.as_str())
                .fetch_optional(&mut **tx)
                .await?;
                if existing.as_ref().is_some_and(|c| c != currency) {
                    return Err(AuthError::InvalidInput);
                }
            }
            Payload::Membership {
                target,
                expected_version,
                ..
            }
            | Payload::Elevation {
                target,
                expected_version,
                ..
            } => {
                let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM human_users u JOIN human_emails e USING(user_id) WHERE user_id=? AND u.state='active' AND e.verified=1)").bind(target).fetch_one(&mut **tx).await?;
                if !active {
                    return Err(AuthError::Denied);
                }
                let r = sqlx::query(
                    "SELECT version,state FROM human_memberships WHERE project_id=? AND user_id=?",
                )
                .bind(project.as_str())
                .bind(target)
                .fetch_optional(&mut **tx)
                .await?;
                if r.as_ref().map(|r| r.get::<i64, _>("version")).unwrap_or(0) != *expected_version
                {
                    return Err(AuthError::StaleVersion);
                }
                if matches!(payload, Payload::Elevation { .. }) && target == actor {
                    return Err(AuthError::Denied);
                }
                if matches!(payload, Payload::Membership { .. })
                    && r.as_ref()
                        .is_none_or(|r| r.get::<&str, _>("state") != "active")
                {
                    return Err(AuthError::Denied);
                }
            }
        }
        Ok(())
    }
    pub(super) async fn set_membership(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        p: &ProjectId,
        user: &str,
        expected: i64,
        change: MemberMutation<'_>,
    ) -> Result<(), AuthError> {
        let MemberMutation {
            role,
            active,
            reference,
        } = change;
        if expected == 0 {
            one(
                sqlx::query("INSERT INTO human_memberships VALUES(?,?,?,?,1)")
                    .bind(p.as_str())
                    .bind(user)
                    .bind(role.name())
                    .bind(if active { "active" } else { "revoked" })
                    .execute(&mut **tx)
                    .await?
                    .rows_affected(),
            )?;
        } else {
            let result=sqlx::query("UPDATE human_memberships SET role=?,state=?,version=version+1 WHERE project_id=? AND user_id=? AND version=?").bind(role.name()).bind(if active{"active"}else{"revoked"}).bind(p.as_str()).bind(user).bind(expected).execute(&mut **tx).await?;
            if result.rows_affected() != 1 {
                return Err(AuthError::StaleVersion);
            }
        }
        self.membership_history(tx, p, user, reference, self.now()?)
            .await?;
        sqlx::query("UPDATE human_owner_actions SET state='revoked' WHERE project_id=? AND actor=? AND state='pending' AND action_id!=?").bind(p.as_str()).bind(user).bind(reference).execute(&mut **tx).await?;
        Ok(())
    }
    /// Recipient acceptance is independent of the initiating Owner's proof and
    /// ordinary invitations. It confers no authority until final atomic execution.
    pub async fn accept_owner_elevation(
        &self,
        h: &SessionHandle,
        id: &OwnerActionId,
        confirmation: Confirmation,
        c: &BrowserContext,
    ) -> Result<(), AuthError> {
        if confirmation != Confirmation::Affirmative {
            return Err(AuthError::Denied);
        }
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Write).await?;
        if f.activity != Activity::Human {
            return Err(AuthError::Denied);
        }
        let s = self.session_row(&mut tx, h, &f, now, true).await?;
        let row =
            sqlx::query("SELECT * FROM human_owner_actions WHERE action_id=? AND state='pending'")
                .bind(&id.0)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(AuthError::Denied)?;
        if now >= row.try_get::<i64, _>("expires_at")?
            || !constant_eq(
                &self.action_binding(&row)?,
                &row.try_get::<Vec<u8>, _>("binding")?,
            )
        {
            return Err(AuthError::Denied);
        }
        let action = OwnerAction::decode(row.try_get("payload")?)?;
        let Payload::Elevation { target, .. } = &action.payload else {
            return Err(AuthError::Denied);
        };
        if target != s.try_get::<&str, _>("user_id")?
            || target == row.try_get::<&str, _>("actor")?
        {
            return Err(AuthError::Denied);
        }
        self.require_enrolled_assurance(&s).await?;
        let p = ProjectId::new(row.try_get::<String, _>("project_id")?)
            .map_err(|_| AuthError::Denied)?;
        self.validate_target(&mut tx, &p, &action.payload, row.try_get("actor")?)
            .await?;
        let issuer = self.member_in(&mut tx, &p, row.try_get("actor")?).await?;
        if issuer.role != ProjectRole::Owner
            || issuer.version != row.try_get::<i64, _>("role_version")?
        {
            return Err(AuthError::Denied);
        }
        let lookup = self.handle_lookup(h);
        let tag = self.mac(
            b"owner-recipient-acceptance",
            &[
                &self.action_binding(&row)?,
                target.as_bytes(),
                &lookup,
                &now.to_be_bytes(),
            ],
        );
        one(
            sqlx::query("INSERT INTO human_owner_acceptances VALUES(?,?,?,?,?)")
                .bind(&id.0)
                .bind(target)
                .bind(lookup.as_slice())
                .bind(now)
                .bind(tag.as_slice())
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        self.governance_event(
            &mut tx,
            &p,
            target,
            &c.request_id,
            GovernanceAudit {
                kind: "recipient_accepted",
                reference: &id.0,
                at: now,
            },
        )
        .await?;
        self.session_checkpoint(
            c,
            Operation::Write,
            self.original_deadline(&mut tx, h, &s)
                .await?
                .min(row.try_get::<i64, _>("expires_at")?),
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(())
    }
    pub(super) async fn require_enrolled_assurance(&self, s: &SqliteRow) -> Result<(), AuthError> {
        if s.try_get::<&str, _>("assurance")? == "recovery" {
            return Err(AuthError::Denied);
        }
        Ok(())
    }
    async fn recipient_current(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        r: &SqliteRow,
        target: &str,
        now: i64,
    ) -> Result<i64, AuthError> {
        let a =
            sqlx::query("SELECT * FROM human_owner_acceptances WHERE action_id=? AND recipient=?")
                .bind(r.try_get::<&str, _>("action_id")?)
                .bind(target)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or(AuthError::Denied)?;
        let lookup: Vec<u8> = a.try_get("session_lookup")?;
        let lookup: [u8; 32] = lookup.try_into().map_err(|_| AuthError::Denied)?;
        let s = self.session_lookup(tx, &lookup, now).await?;
        self.require_enrolled_assurance(&s).await?;
        if s.try_get::<&str, _>("user_id")? != target
            || a.try_get::<i64, _>("accepted_at")? > now
            || !constant_eq(
                &self.mac(
                    b"owner-recipient-acceptance",
                    &[
                        &self.action_binding(r)?,
                        target.as_bytes(),
                        &lookup,
                        &a.try_get::<i64, _>("accepted_at")?.to_be_bytes(),
                    ],
                ),
                &a.try_get::<Vec<u8>, _>("binding")?,
            )
        {
            return Err(AuthError::Denied);
        }
        let token_deadline: i64 =
            sqlx::query_scalar("SELECT expires_at FROM human_session_tokens WHERE lookup=?")
                .bind(lookup.as_slice())
                .fetch_one(&mut **tx)
                .await?;
        Ok(token_deadline.min(s.try_get("absolute_expires")?).min(
            s.try_get::<i64, _>("last_human_at")?
                .saturating_add(HUMAN_IDLE_SECONDS),
        ))
    }
    /// One current Owner, one exact proof, one SQLite transaction with the effect.
    pub async fn execute_owner_action(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        id: &OwnerActionId,
        expected: &OwnerAction,
        confirmation: Confirmation,
        c: &BrowserContext,
    ) -> Result<GovernanceReceipt, AuthError> {
        if confirmation != Confirmation::Affirmative {
            return Err(AuthError::Denied);
        }
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Write).await?;
        if f.activity != Activity::Human {
            return Err(AuthError::Denied);
        }
        let s = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Govern, now)
            .await?;
        let r = self.action_in(&mut tx, id, &s, now).await?;
        let action = OwnerAction::decode(r.try_get("payload")?)?;
        if action != *expected {
            return Err(AuthError::Denied);
        }
        self.validate_target(&mut tx, p, &action.payload, s.user()?)
            .await?;
        let proof = sqlx::query("SELECT * FROM human_owner_proofs WHERE action_id=?")
            .bind(&id.0)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AuthError::Denied)?;
        let verified: i64 = proof.try_get("verified_at")?;
        let expires: i64 = proof.try_get::<i64, _>("expires_at")?;
        if now < verified
            || now >= expires
            || expires != verified + 300
            || !constant_eq(
                &self.proof_binding(&r, &proof)?,
                &proof.try_get::<Vec<u8>, _>("binding")?,
            )
        {
            return Err(AuthError::Denied);
        }
        self.owner_credentials_current(&mut tx, &s, &proof).await?;
        let mut final_deadline = s
            .deadline
            .min(expires)
            .min(r.try_get::<i64, _>("expires_at")?);
        let sequence = match &action.payload {
            Payload::AwsInfrastructure { proposal, digest } => {
                let deadline = crate::controller_keys::worker_auth::aws_fargate::consent::apply(
                    &mut tx,
                    &self.key,
                    proposal,
                    digest,
                    s.user()?,
                    p,
                    now,
                )
                .await
                .map_err(|_| AuthError::Denied)?;
                final_deadline = final_deadline.min(deadline);
                None
            }
            Payload::ProviderConnection { proposal, digest } => {
                let proposal_deadline =
                    crate::controller_keys::provider_accounts::store::apply_proposal(
                        &mut tx,
                        &self.key,
                        proposal,
                        digest,
                        s.user()?,
                        p,
                        now,
                    )
                    .await
                    .map_err(|_| AuthError::Denied)?;
                final_deadline = final_deadline.min(proposal_deadline);
                None
            }
            Payload::Budget {
                expected_version,
                currency,
                proposed,
                ..
            } => {
                let request = FinancialRequest {
                    project: p.clone(),
                    request_id: RequestId::new(id.0.clone())
                        .map_err(|_| AuthError::InvalidInput)?,
                    actor: PrincipalId::new(s.user()?.to_owned())
                        .map_err(|_| AuthError::InvalidInput)?,
                    reason: "Explicit human Owner ceiling approval".to_owned(),
                    command: FinancialCommand::Approve {
                        expected_version: *expected_version as u64,
                        new_ceiling: Money::new(
                            *proposed,
                            Currency::new(currency).map_err(|_| AuthError::InvalidInput)?,
                        )
                        .map_err(|_| AuthError::InvalidInput)?,
                        proof: OwnerProof {
                            reference: EvidenceRef::new(id.0.clone())
                                .map_err(|_| AuthError::InvalidInput)?,
                            verified_at: verified,
                        },
                    },
                };
                let guard = VerifiedBudgetApproval { request, at: now };
                let FinancialReceipt { sequence, .. } =
                    crate::finance::apply_human_approval_in(&mut tx, &guard)
                        .await
                        .map_err(|_| AuthError::Denied)?;
                Some(sequence)
            }
            Payload::Membership {
                target,
                expected_version,
                role,
            } => {
                let old = self.member_in(&mut tx, p, target).await?;
                self.set_membership(
                    &mut tx,
                    p,
                    target,
                    *expected_version,
                    MemberMutation {
                        role: role.unwrap_or(old.role),
                        active: role.is_some(),
                        reference: &id.0,
                    },
                )
                .await?;
                None
            }
            Payload::Elevation {
                target,
                expected_version,
                transfer,
            } => {
                final_deadline =
                    final_deadline.min(self.recipient_current(&mut tx, &r, target, now).await?);
                self.set_membership(
                    &mut tx,
                    p,
                    target,
                    *expected_version,
                    MemberMutation {
                        role: ProjectRole::Owner,
                        active: true,
                        reference: &id.0,
                    },
                )
                .await?;
                if *transfer {
                    self.set_membership(
                        &mut tx,
                        p,
                        s.user()?,
                        s.membership.version,
                        MemberMutation {
                            role: ProjectRole::Editor,
                            active: true,
                            reference: &id.0,
                        },
                    )
                    .await?;
                }
                None
            }
        };
        one(sqlx::query("INSERT INTO human_owner_effects VALUES(?,?,?)")
            .bind(&id.0)
            .bind(sequence)
            .bind(now)
            .execute(&mut *tx)
            .await?
            .rows_affected())?;
        one(sqlx::query("UPDATE human_owner_actions SET state='committed' WHERE action_id=? AND state='pending'").bind(&id.0).execute(&mut *tx).await?.rows_affected())?;
        let event = self
            .governance_event(
                &mut tx,
                p,
                s.user()?,
                &c.request_id,
                GovernanceAudit {
                    kind: "action_committed",
                    reference: &id.0,
                    at: now,
                },
            )
            .await?;
        sqlx::query("INSERT INTO human_owner_notifications(event_sequence,recipient) SELECT ?,m.user_id FROM human_memberships m JOIN human_users u USING(user_id) WHERE project_id=? AND role='owner' AND m.state='active' AND u.state='active' AND m.user_id!=?").bind(event).bind(p.as_str()).bind(s.user()?).execute(&mut *tx).await?;
        if let Payload::Membership { target, .. } | Payload::Elevation { target, .. } =
            &action.payload
        {
            sqlx::query("INSERT OR IGNORE INTO human_owner_notifications(event_sequence,recipient) VALUES(?,?)").bind(event).bind(target).execute(&mut *tx).await?;
        }
        let missing:i64=sqlx::query_scalar("SELECT count(*) FROM human_memberships m JOIN human_users u USING(user_id) WHERE project_id=? AND role='owner' AND m.state='active' AND u.state='active' AND m.user_id!=? AND NOT EXISTS(SELECT 1 FROM human_owner_notifications n WHERE n.event_sequence=? AND n.recipient=m.user_id)").bind(p.as_str()).bind(s.user()?).bind(event).fetch_one(&mut *tx).await?;
        if missing != 0 {
            return Err(AuthError::Storage);
        }
        if let Payload::Membership { target, .. } | Payload::Elevation { target, .. } =
            &action.payload
        {
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM human_owner_notifications WHERE event_sequence=? AND recipient=?)").bind(event).bind(target).fetch_one(&mut *tx).await?;
            if !exists {
                return Err(AuthError::Storage);
            }
        }
        self.session_checkpoint(
            c,
            Operation::Write,
            final_deadline,
            Checkpoint::BeforeOwnerCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterOwnerCommit)?;
        Ok(GovernanceReceipt {
            action: id.clone(),
            financial_sequence: sequence,
        })
    }
    pub(in crate::controller_keys::human_auth::sessions) async fn hold_owner_actions(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        session: &str,
        request: &RequestId,
        now: i64,
    ) -> Result<(), AuthError> {
        sqlx::query("UPDATE human_project_invitations SET state='revoked',version=version+1 WHERE issuer_session=? AND state='pending'").bind(session).execute(&mut **tx).await?;
        let affected=sqlx::query("SELECT DISTINCT project_id,actor FROM human_owner_actions WHERE session_id=? AND state='pending'").bind(session).fetch_all(&mut **tx).await?;
        sqlx::query(
            "UPDATE human_owner_actions SET state='held' WHERE session_id=? AND state='pending'",
        )
        .bind(session)
        .execute(&mut **tx)
        .await?;
        for a in affected {
            let p = ProjectId::new(a.try_get::<String, _>("project_id")?)
                .map_err(|_| AuthError::Denied)?;
            self.governance_event(
                tx,
                &p,
                a.try_get("actor")?,
                request,
                GovernanceAudit {
                    kind: "incident_held",
                    reference: session,
                    at: now,
                },
            )
            .await?;
        }
        Ok(())
    }
}
#[derive(Debug)]
pub struct OwnerNotification {
    pub id: i64,
    pub project: ProjectId,
    pub event: String,
    pub actor: HumanId,
    pub action: Option<OwnerAction>,
    pub reference: String,
    pub occurred_at: i64,
}
impl HumanAuth {
    /// Inbox prerequisites only: returning rows is not a delivered/read assertion.
    pub async fn owner_notifications(
        &self,
        h: &SessionHandle,
        after: i64,
        limit: u16,
        c: &BrowserContext,
    ) -> Result<Vec<OwnerNotification>, AuthError> {
        if after < 0 || !(1..=100).contains(&limit) {
            return Err(AuthError::InvalidInput);
        }
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Read).await?;
        let s = self.session_row(&mut tx, h, &f, now, false).await?;
        let rows=sqlx::query("SELECT n.notification_id,e.*,a.payload FROM human_owner_notifications n JOIN human_governance_events e ON e.sequence=n.event_sequence LEFT JOIN human_owner_actions a ON a.action_id=e.reference AND a.project_id=e.project_id WHERE n.recipient=? AND n.notification_id>? AND EXISTS(SELECT 1 FROM human_memberships m WHERE m.project_id=e.project_id AND m.user_id=n.recipient AND m.state='active' AND (a.payload IS NULL OR json_extract(a.payload,'$.type')!='Budget' OR m.role='owner')) ORDER BY n.notification_id LIMIT ?").bind(s.try_get::<&str,_>("user_id")?).bind(after).bind(limit).fetch_all(&mut *tx).await?;
        let result = rows
            .iter()
            .map(|r| {
                Ok(OwnerNotification {
                    id: r.try_get("notification_id")?,
                    project: ProjectId::new(r.try_get::<String, _>("project_id")?)
                        .map_err(|_| AuthError::Denied)?,
                    event: r.try_get("event")?,
                    actor: HumanId(r.try_get("actor")?),
                    action: r
                        .try_get::<Option<&str>, _>("payload")?
                        .map(OwnerAction::decode)
                        .transpose()?,
                    reference: r.try_get("reference")?,
                    occurred_at: r.try_get("occurred_at")?,
                })
            })
            .collect::<Result<Vec<_>, AuthError>>()?;
        self.session_checkpoint(
            c,
            Operation::Read,
            self.original_deadline(&mut tx, h, &s).await?,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(result)
    }
}

impl HumanAuth {
    pub(super) async fn membership_history(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        p: &ProjectId,
        user: &str,
        reference: &str,
        now: i64,
    ) -> Result<(), AuthError> {
        let written=sqlx::query("INSERT INTO human_membership_history SELECT project_id,user_id,version,role,state,?,? FROM human_memberships WHERE project_id=? AND user_id=?").bind(reference).bind(now).bind(p.as_str()).bind(user).execute(&mut **tx).await?;
        one(written.rows_affected())
    }
}

#[derive(Debug)]
pub struct GovernanceEvent {
    pub sequence: i64,
    pub actor: HumanId,
    pub event: String,
    pub reference: String,
    pub occurred_at: i64,
}
#[derive(Debug)]
pub struct MembershipRevision {
    pub user: HumanId,
    pub version: i64,
    pub role: ProjectRole,
    pub active: bool,
    pub reference: String,
    pub occurred_at: i64,
}
impl HumanAuth {
    pub async fn project_budget(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        c: &BrowserContext,
    ) -> Result<crate::finance::BudgetSnapshot, AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Read).await?;
        let s = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Read, now)
            .await?;
        if s.membership.role != ProjectRole::Owner {
            return Err(AuthError::Denied);
        }
        let result = crate::finance::snapshot(&mut tx, p)
            .await
            .map_err(|_| AuthError::Denied)?;
        self.session_checkpoint(
            c,
            Operation::Read,
            s.deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn project_governance_history(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        after: i64,
        limit: u16,
        c: &BrowserContext,
    ) -> Result<Vec<GovernanceEvent>, AuthError> {
        if after < 0 || !(1..=100).contains(&limit) {
            return Err(AuthError::InvalidInput);
        }
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Read).await?;
        let s = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Read, now)
            .await?;
        if s.membership.role != ProjectRole::Owner {
            return Err(AuthError::Denied);
        }
        let rows=sqlx::query("SELECT * FROM human_governance_events WHERE project_id=? AND sequence>? ORDER BY sequence LIMIT ?").bind(p.as_str()).bind(after).bind(limit).fetch_all(&mut *tx).await?;
        let result = rows
            .iter()
            .map(|r| {
                Ok(GovernanceEvent {
                    sequence: r.try_get("sequence")?,
                    actor: HumanId(r.try_get("actor")?),
                    event: r.try_get("event")?,
                    reference: r.try_get("reference")?,
                    occurred_at: r.try_get("occurred_at")?,
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
        Ok(result)
    }
    pub async fn project_membership_history(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        user: &HumanId,
        after: i64,
        limit: u16,
        c: &BrowserContext,
    ) -> Result<Vec<MembershipRevision>, AuthError> {
        if after < 0 || !(1..=100).contains(&limit) {
            return Err(AuthError::InvalidInput);
        }
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Read).await?;
        let s = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Read, now)
            .await?;
        if s.membership.role != ProjectRole::Owner {
            return Err(AuthError::Denied);
        }
        let rows=sqlx::query("SELECT * FROM human_membership_history WHERE project_id=? AND user_id=? AND version>? ORDER BY version LIMIT ?").bind(p.as_str()).bind(&user.0).bind(after).bind(limit).fetch_all(&mut *tx).await?;
        let result = rows
            .iter()
            .map(|r| {
                Ok(MembershipRevision {
                    user: HumanId(r.try_get("user_id")?),
                    version: r.try_get("version")?,
                    role: ProjectRole::parse(r.try_get("role")?)?,
                    active: r.try_get::<&str, _>("state")? == "active",
                    reference: r.try_get("reference")?,
                    occurred_at: r.try_get("occurred_at")?,
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
        Ok(result)
    }
    pub(in crate::controller_keys::human_auth::sessions) async fn revoke_owner_actions(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        session: &str,
    ) -> Result<(), AuthError> {
        sqlx::query(
            "UPDATE human_owner_actions SET state='revoked' WHERE session_id=? AND state='pending'",
        )
        .bind(session)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerActionState {
    Pending,
    Committed,
    Revoked,
    Held,
}
#[derive(Debug)]
pub struct OwnerActionStatus {
    pub state: OwnerActionState,
    pub financial_sequence: Option<i64>,
}
impl HumanAuth {
    /// Historical outcome only, never a reusable approval or permission. Rechecks
    /// current session/membership before reporting an old committed effect.
    pub async fn owner_action_status(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        id: &OwnerActionId,
        c: &BrowserContext,
    ) -> Result<OwnerActionStatus, AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Read).await?;
        let s = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Read, now)
            .await?;
        let r = sqlx::query("SELECT * FROM human_owner_actions WHERE action_id=? AND project_id=?")
            .bind(id.as_str())
            .bind(p.as_str())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AuthError::Denied)?;
        if s.user()? != r.try_get::<&str, _>("actor")? && s.membership.role != ProjectRole::Owner
            || !constant_eq(
                &self.action_binding(&r)?,
                &r.try_get::<Vec<u8>, _>("binding")?,
            )
        {
            return Err(AuthError::Denied);
        }
        let state = match r.try_get::<&str, _>("state")? {
            "pending" => OwnerActionState::Pending,
            "committed" => OwnerActionState::Committed,
            "revoked" => OwnerActionState::Revoked,
            "held" => OwnerActionState::Held,
            _ => return Err(AuthError::Storage),
        };
        let effect =
            sqlx::query("SELECT financial_sequence FROM human_owner_effects WHERE action_id=?")
                .bind(id.as_str())
                .fetch_optional(&mut *tx)
                .await?;
        if (state == OwnerActionState::Committed) != effect.is_some() {
            return Err(AuthError::Storage);
        }
        let sequence = effect
            .map(|r| r.try_get("financial_sequence"))
            .transpose()?
            .flatten();
        self.session_checkpoint(
            c,
            Operation::Read,
            s.deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(OwnerActionStatus {
            state,
            financial_sequence: sequence,
        })
    }
}
