//! Current project membership and one exact human-authorized effect, never machine authority.
use super::*;
use crate::{PrincipalId, ProjectId};
use rolvya_domain::money::{Currency, Money};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
mod governance;
mod infrastructure;
mod invitations;
mod providers;
pub use email::{OwnerChallengeId, PreparedOwnerEmail};
pub(crate) use governance::VerifiedBudgetApproval;
pub use governance::{
    GovernanceEvent, MembershipRevision, OwnerActionState, OwnerActionStatus, OwnerNotification,
};
pub use invitations::{InvitationId, InvitationSecret, InvitationTicket, InvitationView};
pub use step_up::OwnerProofRequest;
mod email;
mod step_up;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(in crate::controller_keys) use tests::infrastructure_consent_on_existing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRole {
    Owner,
    Editor,
    Viewer,
}
impl ProjectRole {
    fn name(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Editor => "editor",
            Self::Viewer => "viewer",
        }
    }
    fn parse(s: &str) -> Result<Self, AuthError> {
        match s {
            "owner" => Ok(Self::Owner),
            "editor" => Ok(Self::Editor),
            "viewer" => Ok(Self::Viewer),
            _ => Err(AuthError::Denied),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectPermission {
    Read,
    ManageApprovedWork,
    Govern,
}
#[derive(Debug, Clone)]
pub struct Membership {
    pub user: HumanId,
    pub role: ProjectRole,
    pub version: i64,
    pub active: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerAction {
    payload: Payload,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
enum Payload {
    AwsInfrastructure {
        proposal: String,
        digest: String,
    },
    ProviderConnection {
        proposal: String,
        digest: String,
    },
    Budget {
        expected_version: i64,
        currency: String,
        previous: i64,
        proposed: i64,
    },
    Membership {
        target: String,
        expected_version: i64,
        role: Option<ProjectRole>,
    },
    Elevation {
        target: String,
        expected_version: i64,
        transfer: bool,
    },
}
impl OwnerAction {
    pub fn budget(
        expected_version: u64,
        previous: Money,
        proposed: Money,
    ) -> Result<Self, AuthError> {
        if previous.currency() != proposed.currency()
            || proposed.minor_units() <= previous.minor_units()
        {
            return Err(AuthError::InvalidInput);
        }
        Ok(Self {
            payload: Payload::Budget {
                expected_version: i64::try_from(expected_version)
                    .map_err(|_| AuthError::InvalidInput)?,
                currency: proposed.currency().to_string(),
                previous: previous.minor_units(),
                proposed: proposed.minor_units(),
            },
        })
    }
    pub fn membership(
        target: &HumanId,
        expected_version: i64,
        role: Option<ProjectRole>,
    ) -> Result<Self, AuthError> {
        if expected_version < 1 || role == Some(ProjectRole::Owner) {
            return Err(AuthError::InvalidInput);
        }
        Ok(Self {
            payload: Payload::Membership {
                target: target.0.clone(),
                expected_version,
                role,
            },
        })
    }
    pub fn elevation(
        target: &HumanId,
        expected_version: i64,
        transfer: bool,
    ) -> Result<Self, AuthError> {
        if expected_version < 0 {
            return Err(AuthError::InvalidInput);
        }
        Ok(Self {
            payload: Payload::Elevation {
                target: target.0.clone(),
                expected_version,
                transfer,
            },
        })
    }
    fn validate(&self) -> Result<(), AuthError> {
        match &self.payload {
            Payload::ProviderConnection { proposal, digest }
            | Payload::AwsInfrastructure { proposal, digest } => {
                SessionId::parse(proposal.clone())?;
                if digest.len() != 64
                    || !digest
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(AuthError::InvalidInput);
                }
            }
            Payload::Budget {
                expected_version,
                currency,
                previous,
                proposed,
            } => {
                if *expected_version < 0 || *previous < 0 || proposed <= previous {
                    return Err(AuthError::InvalidInput);
                }
                Currency::new(currency).map_err(|_| AuthError::InvalidInput)?;
            }
            Payload::Membership {
                target,
                expected_version,
                role,
            } => {
                SessionId::parse(target.clone())?;
                if *expected_version < 1 || *role == Some(ProjectRole::Owner) {
                    return Err(AuthError::InvalidInput);
                }
            }
            Payload::Elevation {
                target,
                expected_version,
                ..
            } => {
                SessionId::parse(target.clone())?;
                if *expected_version < 0 {
                    return Err(AuthError::InvalidInput);
                }
            }
        }
        Ok(())
    }
    fn canonical(&self) -> Vec<u8> {
        // Fixed version + discriminant + length-framed fields. JSON is storage only.
        let mut out = b"rolvya/owner-action/v1\0".to_vec();
        let parts: Vec<Vec<u8>> = match &self.payload {
            Payload::AwsInfrastructure { proposal, digest } => vec![
                b"aws_infrastructure".to_vec(),
                proposal.as_bytes().to_vec(),
                digest.as_bytes().to_vec(),
            ],
            Payload::ProviderConnection { proposal, digest } => vec![
                b"provider_connection".to_vec(),
                proposal.as_bytes().to_vec(),
                digest.as_bytes().to_vec(),
            ],
            Payload::Budget {
                expected_version,
                currency,
                previous,
                proposed,
            } => vec![
                b"budget".to_vec(),
                expected_version.to_be_bytes().to_vec(),
                currency.as_bytes().to_vec(),
                previous.to_be_bytes().to_vec(),
                proposed.to_be_bytes().to_vec(),
            ],
            Payload::Membership {
                target,
                expected_version,
                role,
            } => vec![
                b"membership".to_vec(),
                target.as_bytes().to_vec(),
                expected_version.to_be_bytes().to_vec(),
                role.map(|r| r.name())
                    .unwrap_or("revoked")
                    .as_bytes()
                    .to_vec(),
            ],
            Payload::Elevation {
                target,
                expected_version,
                transfer,
            } => vec![
                b"elevation".to_vec(),
                target.as_bytes().to_vec(),
                expected_version.to_be_bytes().to_vec(),
                vec![u8::from(*transfer)],
            ],
        };
        for p in parts {
            out.extend_from_slice(&(p.len() as u32).to_be_bytes());
            out.extend_from_slice(&p)
        }
        out
    }
    fn digest(&self) -> [u8; 32] {
        Sha256::digest(self.canonical()).into()
    }
    fn decode(s: &str) -> Result<Self, AuthError> {
        if s.len() > 2048 {
            return Err(AuthError::Denied);
        }
        let a = Self {
            payload: serde_json::from_str(s).map_err(|_| AuthError::Denied)?,
        };
        a.validate()?;
        Ok(a)
    }
}
#[derive(Debug, Clone)]
pub struct OwnerActionId(String);
impl OwnerActionId {
    pub fn parse(s: String) -> Result<Self, AuthError> {
        SessionId::parse(s).map(|x| Self(x.0))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone)]
pub struct GovernanceReceipt {
    pub action: OwnerActionId,
    pub financial_sequence: Option<i64>,
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "local project ownership ceremony is deliberately unavailable"
    )
)]
pub(in crate::controller_keys::human_auth) struct BindingContext<'a> {
    pub request: &'a Administration,
    pub project: &'a ProjectId,
    pub user: &'a str,
    pub legacy: &'a str,
    pub now: i64,
}
pub(in crate::controller_keys::human_auth) trait ProjectBindingAuthority:
    Send + Sync
{
    fn verify(&self, c: &BindingContext<'_>) -> Result<(), AuthError>;
}
impl ProjectBindingAuthority for Unimplemented {
    fn verify(&self, _: &BindingContext<'_>) -> Result<(), AuthError> {
        Err(AuthError::Unavailable)
    }
}
struct GovernanceAudit<'a> {
    kind: &'a str,
    reference: &'a str,
    at: i64,
}
struct MemberMutation<'a> {
    role: ProjectRole,
    active: bool,
    reference: &'a str,
}
struct Scope {
    project: ProjectId,
    session: SqliteRow,
    membership: Membership,
    deadline: i64,
}
impl Scope {
    fn user(&self) -> Result<&str, AuthError> {
        Ok(self.session.try_get("user_id")?)
    }
}

impl HumanAuth {
    async fn member_in(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        project: &ProjectId,
        user: &str,
    ) -> Result<Membership, AuthError> {
        let binding = sqlx::query("SELECT * FROM human_project_bindings WHERE project_id=?")
            .bind(project.as_str())
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(AuthError::Denied)?;
        let expected = self.mac(
            b"project-human-binding",
            &[
                project.as_str().as_bytes(),
                binding.try_get::<&str, _>("initial_user")?.as_bytes(),
                binding.try_get::<&str, _>("legacy_actor")?.as_bytes(),
                binding.try_get::<&str, _>("request_id")?.as_bytes(),
                &binding.try_get::<i64, _>("established_at")?.to_be_bytes(),
            ],
        );
        if !constant_eq(&expected, &binding.try_get::<Vec<u8>, _>("binding")?) {
            return Err(AuthError::Denied);
        }
        let r=sqlx::query("SELECT m.* FROM human_memberships m JOIN human_users u USING(user_id) WHERE project_id=? AND m.user_id=? AND m.state='active' AND u.state='active'").bind(project.as_str()).bind(user).fetch_optional(&mut **tx).await?.ok_or(AuthError::Denied)?;
        Ok(Membership {
            user: HumanId(user.to_owned()),
            role: ProjectRole::parse(r.try_get("role")?)?,
            version: r.try_get("version")?,
            active: true,
        })
    }
    async fn project_scope(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        h: &SessionHandle,
        f: &BrowserFacts,
        project: &ProjectId,
        permission: ProjectPermission,
        now: i64,
    ) -> Result<Scope, AuthError> {
        let s = self
            .session_row(tx, h, f, now, permission != ProjectPermission::Read)
            .await?;
        let m = self.member_in(tx, project, s.try_get("user_id")?).await?;
        if (permission == ProjectPermission::Govern && m.role != ProjectRole::Owner)
            || (permission == ProjectPermission::ManageApprovedWork
                && m.role == ProjectRole::Viewer)
        {
            return Err(AuthError::Denied);
        }
        let deadline = self.original_deadline(tx, h, &s).await?;
        Ok(Scope {
            project: project.clone(),
            session: s,
            membership: m,
            deadline,
        })
    }
    /// Returns current scoped data, not a reusable authorization capability.
    pub async fn project_membership(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        c: &BrowserContext,
    ) -> Result<Membership, AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Read).await?;
        let s = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Read, now)
            .await?;
        self.session_checkpoint(
            c,
            Operation::Read,
            s.deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(s.membership)
    }
    /// Authorization snapshot only. Future work mutations must repeat the check in their transaction.
    pub async fn check_project_permission(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        permission: ProjectPermission,
        c: &BrowserContext,
    ) -> Result<(), AuthError> {
        let op = if permission == ProjectPermission::Read {
            Operation::Read
        } else {
            Operation::Write
        };
        let (mut tx, f, now) = self.browser_transaction(c, op).await?;
        let s = self
            .project_scope(&mut tx, h, &f, p, permission, now)
            .await?;
        self.session_checkpoint(
            c,
            op,
            s.deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn project_members(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        after: Option<&HumanId>,
        limit: u16,
        c: &BrowserContext,
    ) -> Result<Vec<Membership>, AuthError> {
        if !(1..=100).contains(&limit) {
            return Err(AuthError::InvalidInput);
        }
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Read).await?;
        let s = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Read, now)
            .await?;
        if s.membership.role != ProjectRole::Owner {
            return Err(AuthError::Denied);
        }
        let rows=sqlx::query("SELECT * FROM human_memberships WHERE project_id=? AND user_id>? ORDER BY user_id LIMIT ?").bind(p.as_str()).bind(after.map(|u|u.0.as_str()).unwrap_or("")).bind(limit).fetch_all(&mut *tx).await?;
        let result = rows
            .iter()
            .map(|r| {
                Ok(Membership {
                    user: HumanId(r.try_get("user_id")?),
                    role: ProjectRole::parse(r.try_get("role")?)?,
                    version: r.try_get("version")?,
                    active: r.try_get::<&str, _>("state")? == "active",
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
    pub async fn bind_existing_project(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        authority: &Administration,
        c: &BrowserContext,
    ) -> Result<(), AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Write).await?;
        let s = self.session_row(&mut tx, h, &f, now, true).await?;
        let user: &str = s.try_get("user_id")?;
        let legacy: String =
            sqlx::query_scalar("SELECT owner_actor FROM projects WHERE project_id=?")
                .bind(p.as_str())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(AuthError::Denied)?;
        let context = BindingContext {
            request: authority,
            project: p,
            user,
            legacy: &legacy,
            now,
        };
        self.project_binding.verify(&context)?;
        let tag = self.mac(
            b"project-human-binding",
            &[
                p.as_str().as_bytes(),
                user.as_bytes(),
                legacy.as_bytes(),
                authority.request_id.as_str().as_bytes(),
                &now.to_be_bytes(),
            ],
        );
        one(
            sqlx::query("INSERT INTO human_project_bindings VALUES(?,?,?,?,?,?)")
                .bind(p.as_str())
                .bind(user)
                .bind(&legacy)
                .bind(authority.request_id.as_str())
                .bind(now)
                .bind(tag.as_slice())
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        one(
            sqlx::query("INSERT INTO human_memberships VALUES(?,?,'owner','active',1)")
                .bind(p.as_str())
                .bind(user)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        self.membership_history(&mut tx, p, user, authority.request_id.as_str(), now)
            .await?;
        self.governance_event(
            &mut tx,
            p,
            user,
            &authority.request_id,
            GovernanceAudit {
                kind: "ownership_bound",
                reference: authority.request_id.as_str(),
                at: now,
            },
        )
        .await?;
        self.session_checkpoint(
            c,
            Operation::Write,
            self.original_deadline(&mut tx, h, &s).await?,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        self.project_binding.verify(&BindingContext {
            now: self.now()?,
            ..context
        })?;
        tx.commit().await?;
        Ok(())
    }
    async fn governance_event(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        p: &ProjectId,
        user: &str,
        r: &RequestId,
        record: GovernanceAudit<'_>,
    ) -> Result<i64, AuthError> {
        let GovernanceAudit {
            kind: event,
            reference,
            at: now,
        } = record;
        Ok(sqlx::query_scalar("INSERT INTO human_governance_events(project_id,actor,request_id,event,reference,occurred_at) VALUES(?,?,?,?,?,?) RETURNING sequence").bind(p.as_str()).bind(user).bind(r.as_str()).bind(event).bind(reference).bind(now).fetch_one(&mut **tx).await?)
    }
    fn action_binding(&self, r: &SqliteRow) -> Result<[u8; 32], AuthError> {
        let a = OwnerAction::decode(r.try_get("payload")?)?;
        let digest: Vec<u8> = r.try_get("digest")?;
        if !constant_eq(&a.digest(), &digest) {
            return Err(AuthError::Denied);
        }
        let mut parts: Vec<Vec<u8>> = Vec::new();
        for key in [
            "action_id",
            "project_id",
            "actor",
            "session_id",
            "request_id",
        ] {
            parts.push(r.try_get::<String, _>(key)?.into_bytes());
        }
        for key in [
            "generation",
            "role_version",
            "security_epoch",
            "email_version",
            "credential_version",
            "policy_version",
            "created_at",
            "expires_at",
        ] {
            parts.push(r.try_get::<i64, _>(key)?.to_be_bytes().to_vec());
        }
        parts.push(digest);
        Ok(self.mac(
            b"owner-pending-action",
            &parts.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        ))
    }
    async fn action_in(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        id: &OwnerActionId,
        s: &Scope,
        now: i64,
    ) -> Result<SqliteRow, AuthError> {
        let r =
            sqlx::query("SELECT * FROM human_owner_actions WHERE action_id=? AND state='pending'")
                .bind(&id.0)
                .fetch_optional(&mut **tx)
                .await?
                .ok_or(AuthError::Denied)?;
        if r.try_get::<&str, _>("project_id")? != s.project.as_str()
            || r.try_get::<&str, _>("actor")? != s.user()?
            || r.try_get::<&str, _>("session_id")? != s.session.try_get::<&str, _>("session_id")?
            || r.try_get::<i64, _>("role_version")? != s.membership.version
            || now < r.try_get::<i64, _>("created_at")?
            || now >= r.try_get::<i64, _>("expires_at")?
            || !constant_eq(
                &self.action_binding(&r)?,
                &r.try_get::<Vec<u8>, _>("binding")?,
            )
        {
            return Err(AuthError::Denied);
        }
        for key in [
            "generation",
            "security_epoch",
            "email_version",
            "credential_version",
            "policy_version",
        ] {
            if r.try_get::<i64, _>(key)? != s.session.try_get::<i64, _>(key)? {
                return Err(AuthError::Denied);
            }
        }
        Ok(r)
    }
    pub async fn prepare_owner_action(
        &self,
        h: &SessionHandle,
        p: &ProjectId,
        action: &OwnerAction,
        c: &BrowserContext,
    ) -> Result<OwnerActionId, AuthError> {
        action.validate()?;
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Write).await?;
        let s = self
            .project_scope(&mut tx, h, &f, p, ProjectPermission::Govern, now)
            .await?;
        self.validate_target(&mut tx, p, &action.payload, s.user()?)
            .await?;
        let payload =
            serde_json::to_string(&action.payload).map_err(|_| AuthError::InvalidInput)?;
        if let Some(old) = sqlx::query("SELECT * FROM human_owner_actions WHERE request_id=?")
            .bind(c.request_id.as_str())
            .fetch_optional(&mut *tx)
            .await?
        {
            let id = OwnerActionId(old.try_get("action_id")?);
            let r = self.action_in(&mut tx, &id, &s, now).await?;
            if r.try_get::<&str, _>("project_id")? != p.as_str()
                || r.try_get::<&str, _>("payload")? != payload
            {
                return Err(AuthError::Conflict);
            }
            return Ok(id);
        }
        let pending:i64=sqlx::query_scalar("SELECT count(*) FROM human_owner_actions WHERE project_id=? AND actor=? AND state='pending' AND expires_at>?").bind(p.as_str()).bind(s.user()?).bind(now).fetch_one(&mut *tx).await?;
        if pending >= 32 {
            return Err(AuthError::Busy);
        }
        let id = OwnerActionId(new_id()?);
        let projected=sqlx::query("SELECT ? AS action_id,? AS project_id,? AS actor,? AS session_id,? AS generation,? AS role_version,? AS security_epoch,? AS email_version,? AS credential_version,? AS policy_version,? AS payload,? AS digest,? AS created_at,? AS expires_at,? AS request_id")
            .bind(&id.0).bind(p.as_str()).bind(s.user()?).bind(s.session.try_get::<&str,_>("session_id")?).bind(s.session.try_get::<i64,_>("generation")?).bind(s.membership.version).bind(s.session.try_get::<i64,_>("security_epoch")?).bind(s.session.try_get::<i64,_>("email_version")?).bind(s.session.try_get::<i64,_>("credential_version")?).bind(s.session.try_get::<i64,_>("policy_version")?).bind(&payload).bind(action.digest().as_slice()).bind(now).bind(deadline(now,1800)?).bind(c.request_id.as_str()).fetch_one(&mut *tx).await?;
        let tag = self.action_binding(&projected)?;
        one(sqlx::query(
            "INSERT INTO human_owner_actions VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'pending',?)",
        )
        .bind(&id.0)
        .bind(p.as_str())
        .bind(s.user()?)
        .bind(s.session.try_get::<&str, _>("session_id")?)
        .bind(s.session.try_get::<i64, _>("generation")?)
        .bind(s.membership.version)
        .bind(s.session.try_get::<i64, _>("security_epoch")?)
        .bind(s.session.try_get::<i64, _>("email_version")?)
        .bind(s.session.try_get::<i64, _>("credential_version")?)
        .bind(s.session.try_get::<i64, _>("policy_version")?)
        .bind(&payload)
        .bind(action.digest().as_slice())
        .bind(now)
        .bind(deadline(now, 1800)?)
        .bind(c.request_id.as_str())
        .bind(tag.as_slice())
        .execute(&mut *tx)
        .await?
        .rows_affected())?;
        self.governance_event(
            &mut tx,
            p,
            s.user()?,
            &c.request_id,
            GovernanceAudit {
                kind: "action_requested",
                reference: &id.0,
                at: now,
            },
        )
        .await?;
        self.session_checkpoint(
            c,
            Operation::Write,
            s.deadline,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        Ok(id)
    }
}

fn deadline(now: i64, seconds: i64) -> Result<i64, AuthError> {
    now.checked_add(seconds).ok_or(AuthError::Unavailable)
}

fn one(rows: u64) -> Result<(), AuthError> {
    if rows == 1 {
        Ok(())
    } else {
        Err(AuthError::Storage)
    }
}
