//! Called only by the existing atomic exact-action Owner executor.
use super::*;
use rolvya_domain::ids::ProjectId;
use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposal {
    installation: String,
    id: String,
    actor: String,
    binding: RepositoryBinding,
    created_at: i64,
    expires_at: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::controller_keys) struct Consent {
    pub binding: RepositoryBinding,
    pub version: i64,
    pub installation: String,
    pub actor: String,
    pub role_version: i64,
    pub security_epoch: i64,
    pub proposal: String,
}
pub(in crate::controller_keys) async fn stage(
    c: &mut SqliteConnection,
    k: &ControllerKey,
    actor: &str,
    p: &ProjectId,
    b: &RepositoryBinding,
    now: i64,
    id: &str,
) -> Result<String, GitError> {
    b.validate()?;
    if b.project != *p || b.valid_until <= now || b.expected_version != version(c, p).await? {
        return Err(GitError::Conflict);
    }
    if b.enabled {
        super::super::secret_broker::github_consumer::references(c, k, b).await?;
    }
    let proposal = Proposal {
        installation: hex(&k.metadata.installation.0),
        id: id.into(),
        actor: actor.into(),
        binding: b.clone(),
        created_at: now,
        expires_at: now.checked_add(600).ok_or(GitError::InvalidInput)?,
    };
    let text = encode(&proposal)?;
    one(
        sqlx::query("INSERT INTO git_binding_proposals VALUES(?,?,?,?,?,?)")
            .bind(id)
            .bind(p.as_str())
            .bind(actor)
            .bind(&text)
            .bind(mac(k, b"proposal", &[text.as_bytes()]).as_slice())
            .bind(proposal.expires_at)
            .execute(c)
            .await?
            .rows_affected(),
    )?;
    Ok(hash(text.as_bytes()))
}
async fn proposal(
    c: &mut SqliteConnection,
    k: &ControllerKey,
    id: &str,
    digest: &str,
    actor: &str,
    p: &ProjectId,
    now: i64,
) -> Result<Proposal, GitError> {
    let r = sqlx::query(
        "SELECT * FROM git_binding_proposals WHERE proposal_id=? AND project_id=? AND actor=?",
    )
    .bind(id)
    .bind(p.as_str())
    .bind(actor)
    .fetch_optional(&mut *c)
    .await?
    .ok_or(GitError::Denied)?;
    let t: &str = r.try_get("record")?;
    if !equal(
        r.try_get("record_mac")?,
        &mac(k, b"proposal", &[t.as_bytes()]),
    ) || hash(t.as_bytes()) != digest
    {
        return Err(GitError::Integrity);
    }
    let v: Proposal = decode(t)?;
    v.binding.validate()?;
    if v.id != id
        || v.actor != actor
        || v.binding.project != *p
        || v.installation != hex(&k.metadata.installation.0)
        || now < v.created_at
        || now >= v.expires_at
        || v.expires_at != r.try_get::<i64, _>("expires_at")?
        || v.binding.valid_until <= now
    {
        return Err(GitError::Expired);
    }
    if v.binding.expected_version != version(c, p).await? {
        return Err(GitError::Conflict);
    }
    if v.binding.enabled {
        super::super::secret_broker::github_consumer::references(c, k, &v.binding).await?;
    }
    Ok(v)
}
pub(in crate::controller_keys) async fn validate(
    c: &mut SqliteConnection,
    k: &ControllerKey,
    id: &str,
    digest: &str,
    actor: &str,
    p: &ProjectId,
    now: i64,
) -> Result<(), GitError> {
    proposal(c, k, id, digest, actor, p, now).await.map(|_| ())
}
pub(in crate::controller_keys) async fn apply(
    c: &mut SqliteConnection,
    k: &ControllerKey,
    id: &str,
    digest: &str,
    actor: &str,
    p: &ProjectId,
    now: i64,
) -> Result<i64, GitError> {
    let v = proposal(c, k, id, digest, actor, p, now).await?;
    let owner=sqlx::query("SELECT m.version,u.security_epoch FROM human_memberships m JOIN human_users u USING(user_id) WHERE m.project_id=? AND m.user_id=? AND m.state='active' AND m.role='owner' AND u.state='active'").bind(p.as_str()).bind(actor).fetch_optional(&mut *c).await?.ok_or(GitError::Denied)?;
    let n = v
        .binding
        .expected_version
        .checked_add(1)
        .ok_or(GitError::ResourceLimit)?;
    let record = Consent {
        binding: v.binding,
        version: n,
        installation: v.installation,
        actor: actor.into(),
        role_version: owner.try_get("version")?,
        security_epoch: owner.try_get("security_epoch")?,
        proposal: id.into(),
    };
    let text = encode(&record)?;
    one(
        sqlx::query("INSERT INTO git_bindings VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind(p.as_str())
            .bind(n)
            .bind(record.binding.repository.get())
            .bind(record.binding.installation.get())
            .bind(id)
            .bind(actor)
            .bind(record.role_version)
            .bind(record.security_epoch)
            .bind(&text)
            .bind(mac(k, b"consent", &[text.as_bytes()]).as_slice())
            .execute(&mut *c)
            .await?
            .rows_affected(),
    )?;
    if n == 1 {
        one(sqlx::query("INSERT INTO git_binding_heads VALUES(?,?)")
            .bind(p.as_str())
            .bind(n)
            .execute(&mut *c)
            .await?
            .rows_affected())?
    } else {
        one(
            sqlx::query("UPDATE git_binding_heads SET version=? WHERE project_id=? AND version=?")
                .bind(n)
                .bind(p.as_str())
                .bind(n - 1)
                .execute(&mut *c)
                .await?
                .rows_affected(),
        )?
    }
    Ok(v.expires_at.min(record.binding.valid_until))
}
async fn version(c: &mut SqliteConnection, p: &ProjectId) -> Result<i64, GitError> {
    Ok(
        sqlx::query_scalar("SELECT version FROM git_binding_heads WHERE project_id=?")
            .bind(p.as_str())
            .fetch_optional(c)
            .await?
            .unwrap_or(0),
    )
}
pub(super) async fn load(
    c: &mut SqliteConnection,
    k: &ControllerKey,
    p: &ProjectId,
) -> Result<Consent, GitError> {
    let r=sqlx::query("SELECT b.* FROM git_binding_heads h JOIN git_bindings b USING(project_id,version) WHERE h.project_id=?").bind(p.as_str()).fetch_optional(c).await?.ok_or(GitError::Denied)?;
    let text: &str = r.try_get("record")?;
    if !equal(
        r.try_get("record_mac")?,
        &mac(k, b"consent", &[text.as_bytes()]),
    ) {
        return Err(GitError::Integrity);
    }
    let v: Consent = decode(text)?;
    v.binding.validate()?;
    if v.binding.project != *p
        || v.installation != hex(&k.metadata.installation.0)
        || v.version != r.try_get::<i64, _>("version")?
        || v.actor != r.try_get::<&str, _>("actor")?
        || v.proposal != r.try_get::<&str, _>("proposal_id")?
    {
        return Err(GitError::Integrity);
    }
    Ok(v)
}
pub(in crate::controller_keys) async fn current(
    c: &mut SqliteConnection,
    k: &ControllerKey,
    p: &ProjectId,
    now: i64,
) -> Result<Consent, GitError> {
    let v = load(c, k, p).await?;
    if !v.binding.enabled || now >= v.binding.valid_until {
        return Err(GitError::Revoked);
    }
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM human_memberships m JOIN human_users u USING(user_id) WHERE m.project_id=? AND m.user_id=? AND m.role='owner' AND m.state='active' AND m.version=? AND u.state='active' AND u.security_epoch=?)").bind(p.as_str()).bind(&v.actor).bind(v.role_version).bind(v.security_epoch).fetch_one(&mut *c).await?;
    let effect:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM human_owner_actions a JOIN human_owner_effects e USING(action_id) WHERE a.project_id=? AND a.actor=? AND a.state='committed' AND json_extract(a.payload,'$.type')='GitRepository' AND json_extract(a.payload,'$.proposal')=?)").bind(p.as_str()).bind(&v.actor).bind(&v.proposal).fetch_one(&mut *c).await?;
    let restricted = super::webhook::restricted(c, k, p, v.version).await?;
    if !active || !effect || restricted {
        return Err(GitError::Revoked);
    }
    super::super::secret_broker::github_consumer::references(c, k, &v.binding).await?;
    Ok(v)
}
