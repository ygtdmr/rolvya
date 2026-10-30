//! Composes existing HumanAuth Owner proofs; references alone never grant consent.
use super::{AccountEnrollment, AwsIdentityError as E};
use crate::{
    controller_keys::{ControllerKey, secret_broker as broker},
    finance::EvidenceRef,
};
use hmac::{Hmac, Mac};
use rolvya_domain::ids::ProjectId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(in crate::controller_keys) struct Consent {
    pub format: u16,
    pub installation: String,
    pub proposal: String,
    pub generation: i64,
    pub actor: String,
    pub role_version: i64,
    pub security_epoch: i64,
    pub enrollment: AccountEnrollment,
    pub created_at: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposal {
    format: u16,
    installation: String,
    id: String,
    actor: String,
    enrollment: AccountEnrollment,
    expected_consent_generation: i64,
    created_at: i64,
    expires_at: i64,
}
pub(super) fn mac(k: &ControllerKey, domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut m =
        Hmac::<Sha256>::new_from_slice(&k.material[..]).expect("HMAC accepts every key length");
    for p in [
        b"rolvya/aws-infrastructure-consent/v1".as_slice(),
        &k.metadata.installation.0[..],
        &k.metadata.key.0[..],
        &k.metadata.key_version.to_be_bytes()[..],
        domain,
    ]
    .into_iter()
    .chain(parts.iter().copied())
    {
        m.update(&(p.len() as u32).to_be_bytes());
        m.update(p);
    }
    m.finalize().into_bytes().into()
}
fn installation(k: &ControllerKey) -> String {
    super::super::hex(&k.metadata.installation.0)
}
fn encode(v: &impl Serialize) -> Result<String, E> {
    let s = serde_json::to_string(v).map_err(|_| E::InvalidInput)?;
    if s.len() > 32768 {
        return Err(E::ResourceLimit);
    }
    Ok(s)
}
async fn consent_generation(c: &mut SqliteConnection, r: &AccountEnrollment) -> Result<i64, E> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT generation FROM aws_account_consent_heads WHERE project_id=? AND account_ref=?",
    )
    .bind(r.project.as_str())
    .bind(r.account_ref.as_str())
    .fetch_optional(c)
    .await?
    .unwrap_or(0))
}
async fn current_version(c: &mut SqliteConnection, r: &AccountEnrollment) -> Result<(), E> {
    let v:Option<i64>=sqlx::query_scalar("SELECT current_version FROM aws_infrastructure_accounts WHERE project_id=? AND account_ref=?").bind(r.project.as_str()).bind(r.account_ref.as_str()).fetch_optional(c).await?;
    if v.unwrap_or(0) as u64 != r.expected_version {
        return Err(E::Conflict);
    }
    Ok(())
}
async fn credential(
    c: &mut SqliteConnection,
    k: &ControllerKey,
    r: &AccountEnrollment,
) -> Result<(), E> {
    super::store::validate_credential(&r.workload, &r.credential)?;
    broker::current_reference_in(
        c,
        k,
        &super::store::credential_scope(&r.project, &r.credential)?,
        broker::SecretVersion::new(r.credential.version as u64).map_err(|_| E::InvalidInput)?,
    )
    .await
    .map_err(|_| E::Integrity)
}
// Disconnect consumes exact Owner proof but no provider credentials. Loading
// this signed target is NOT an authorization to use it or resurrect its scope.
async fn target(
    c: &mut SqliteConnection,
    k: &ControllerKey,
    r: &AccountEnrollment,
) -> Result<(), E> {
    if !r.revoked {
        return credential(c, k, r).await;
    }
    let row=sqlx::query("SELECT a.record,a.record_mac FROM aws_account_consent_heads h JOIN aws_account_consents a USING(project_id,account_ref,generation) WHERE h.project_id=? AND h.account_ref=?").bind(r.project.as_str()).bind(r.account_ref.as_str()).fetch_optional(c).await?.ok_or(E::UnverifiedControllerAuthority)?;
    let text: &str = row.try_get("record")?;
    if text.len() > 32768
        || !super::super::equal(
            row.try_get::<&[u8], _>("record_mac")?,
            &mac(k, b"consent", &[text.as_bytes()]),
        )
    {
        return Err(E::Integrity);
    }
    let previous: Consent = serde_json::from_str(text).map_err(|_| E::Integrity)?;
    if previous.installation != installation(k)
        || previous.enrollment.project != r.project
        || previous.enrollment.account_ref != r.account_ref
        || previous.enrollment.workload != r.workload
        || previous.enrollment.credential != r.credential
    {
        return Err(E::ScopeMismatch);
    }
    Ok(())
}
pub(in crate::controller_keys) async fn stage(
    c: &mut SqliteConnection,
    k: &ControllerKey,
    actor: &str,
    p: &ProjectId,
    r: &AccountEnrollment,
    now: i64,
    id: &str,
) -> Result<String, E> {
    r.workload.validate()?;
    if r.project != *p
        || (!r.revoked && r.valid_until <= now)
        || r.expected_version >= i64::MAX as u64
    {
        return Err(E::InvalidInput);
    }
    current_version(c, r).await?;
    target(c, k, r).await?;
    let proposal = Proposal {
        format: 1,
        installation: installation(k),
        id: id.into(),
        actor: actor.into(),
        enrollment: r.clone(),
        expected_consent_generation: consent_generation(c, r).await?,
        created_at: now,
        expires_at: now.checked_add(600).ok_or(E::InvalidInput)?,
    };
    let text = encode(&proposal)?;
    let signature = mac(k, b"proposal", &[text.as_bytes()]);
    one(
        sqlx::query("INSERT INTO aws_account_proposals VALUES(?,?,?,?,?,?,?)")
            .bind(id)
            .bind(p.as_str())
            .bind(actor)
            .bind(&text)
            .bind(signature.as_slice())
            .bind(now)
            .bind(proposal.expires_at)
            .execute(c)
            .await?
            .rows_affected(),
    )?;
    Ok(super::super::hex(&Sha256::digest(text.as_bytes())))
}
async fn proposal(
    c: &mut SqliteConnection,
    k: &ControllerKey,
    id: &str,
    digest: &str,
    actor: &str,
    p: &ProjectId,
    now: i64,
) -> Result<Proposal, E> {
    let row=sqlx::query("SELECT project_id,actor,record,record_mac,created_at,expires_at FROM aws_account_proposals WHERE proposal_id=?").bind(id).fetch_optional(&mut *c).await?.ok_or(E::UnverifiedControllerAuthority)?;
    let text: &str = row.try_get("record")?;
    if text.len() > 32768
        || row.try_get::<&str, _>("project_id")? != p.as_str()
        || row.try_get::<&str, _>("actor")? != actor
        || !super::super::equal(
            row.try_get::<&[u8], _>("record_mac")?,
            &mac(k, b"proposal", &[text.as_bytes()]),
        )
        || super::super::hex(&Sha256::digest(text.as_bytes())) != digest
    {
        return Err(E::Integrity);
    }
    let v: Proposal = serde_json::from_str(text).map_err(|_| E::Integrity)?;
    if v.format != 1
        || v.installation != installation(k)
        || v.id != id
        || v.actor != actor
        || v.enrollment.project != *p
        || now < v.created_at
        || now >= v.expires_at
        || row.try_get::<i64, _>("created_at")? != v.created_at
        || row.try_get::<i64, _>("expires_at")? != v.expires_at
    {
        return Err(E::ExpiredEvidence);
    }
    if v.expected_consent_generation != consent_generation(c, &v.enrollment).await? {
        return Err(E::Conflict);
    }
    if !v.enrollment.revoked && now >= v.enrollment.valid_until {
        return Err(E::ExpiredEvidence);
    }
    current_version(c, &v.enrollment).await?;
    target(c, k, &v.enrollment).await?;
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
) -> Result<(), E> {
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
) -> Result<i64, E> {
    let v = proposal(c, k, id, digest, actor, p, now).await?;
    let owner=sqlx::query("SELECT m.version,u.security_epoch FROM human_memberships m JOIN human_users u USING(user_id) WHERE m.project_id=? AND m.user_id=? AND m.role='owner' AND m.state='active' AND u.state='active'").bind(p.as_str()).bind(actor).fetch_optional(&mut *c).await?.ok_or(E::UnverifiedControllerAuthority)?;
    let old: Option<i64> = sqlx::query_scalar(
        "SELECT generation FROM aws_account_consent_heads WHERE project_id=? AND account_ref=?",
    )
    .bind(p.as_str())
    .bind(v.enrollment.account_ref.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let generation = old.unwrap_or(0).checked_add(1).ok_or(E::ResourceLimit)?;
    let record = Consent {
        format: 1,
        installation: installation(k),
        proposal: id.into(),
        generation,
        actor: actor.into(),
        role_version: owner.try_get("version")?,
        security_epoch: owner.try_get("security_epoch")?,
        enrollment: v.enrollment,
        created_at: now,
    };
    let text = encode(&record)?;
    let tag = mac(k, b"consent", &[text.as_bytes()]);
    one(
        sqlx::query("INSERT INTO aws_account_consents VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind(p.as_str())
            .bind(record.enrollment.account_ref.as_str())
            .bind(generation)
            .bind(id)
            .bind(actor)
            .bind(record.role_version)
            .bind(record.security_epoch)
            .bind(text)
            .bind(tag.as_slice())
            .bind(now)
            .execute(&mut *c)
            .await?
            .rows_affected(),
    )?;
    if let Some(old) = old {
        one(sqlx::query("UPDATE aws_account_consent_heads SET generation=? WHERE project_id=? AND account_ref=? AND generation=?").bind(generation).bind(p.as_str()).bind(record.enrollment.account_ref.as_str()).bind(old).execute(&mut *c).await?.rows_affected())?;
    } else {
        one(
            sqlx::query("INSERT INTO aws_account_consent_heads VALUES(?,?,?)")
                .bind(p.as_str())
                .bind(record.enrollment.account_ref.as_str())
                .bind(generation)
                .execute(&mut *c)
                .await?
                .rows_affected(),
        )?;
    }
    Ok(if record.enrollment.revoked {
        v.expires_at
    } else {
        v.expires_at.min(record.enrollment.valid_until)
    })
}
pub(super) async fn current(
    c: &mut SqliteConnection,
    k: &ControllerKey,
    p: &ProjectId,
    id: &EvidenceRef,
    now: i64,
) -> Result<Consent, E> {
    let row=sqlx::query("SELECT a.generation,a.actor,a.role_version,a.security_epoch,a.record,a.record_mac FROM aws_account_consent_heads h JOIN aws_account_consents a USING(project_id,account_ref,generation) WHERE h.project_id=? AND h.account_ref=?").bind(p.as_str()).bind(id.as_str()).fetch_optional(&mut *c).await?.ok_or(E::UnverifiedControllerAuthority)?;
    let text: &str = row.try_get("record")?;
    if text.len() > 32768
        || !super::super::equal(
            row.try_get::<&[u8], _>("record_mac")?,
            &mac(k, b"consent", &[text.as_bytes()]),
        )
    {
        return Err(E::Integrity);
    }
    let v: Consent = serde_json::from_str(text).map_err(|_| E::Integrity)?;
    if v.format != 1
        || v.installation != installation(k)
        || v.enrollment.project != *p
        || v.enrollment.account_ref != *id
        || v.generation != row.try_get::<i64, _>("generation")?
        || v.actor != row.try_get::<&str, _>("actor")?
        || v.role_version != row.try_get::<i64, _>("role_version")?
        || v.security_epoch != row.try_get::<i64, _>("security_epoch")?
        || now < v.created_at
        || now >= v.enrollment.valid_until
        || v.enrollment.revoked
    {
        return Err(E::Revoked);
    }
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM human_memberships m JOIN human_users u USING(user_id) WHERE m.project_id=? AND m.user_id=? AND m.role='owner' AND m.state='active' AND m.version=? AND u.state='active' AND u.security_epoch=?)").bind(p.as_str()).bind(&v.actor).bind(v.role_version).bind(v.security_epoch).fetch_one(&mut *c).await?;
    let committed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM human_owner_actions a JOIN human_owner_effects e USING(action_id) WHERE a.project_id=? AND a.actor=? AND a.state='committed' AND json_extract(a.payload,'$.type')='AwsInfrastructure' AND json_extract(a.payload,'$.proposal')=?)").bind(p.as_str()).bind(&v.actor).bind(&v.proposal).fetch_one(&mut *c).await?;
    if !active || !committed {
        return Err(E::Revoked);
    }
    credential(c, k, &v.enrollment).await?;
    Ok(v)
}
fn one(n: u64) -> Result<(), E> {
    if n == 1 { Ok(()) } else { Err(E::Storage) }
}
