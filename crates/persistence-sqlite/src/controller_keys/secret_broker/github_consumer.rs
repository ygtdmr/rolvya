//! Private trusted operations only. Never returns App private-key bytes.
use super::*;
use crate::controller_keys::github_app::{self as git, GitError as E, RepositoryBinding};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SigningKey<'a> {
    format: u16,
    app_id: i64,
    pkcs8: &'a str,
}
pub(in crate::controller_keys) async fn references(
    c: &mut sqlx::SqliteConnection,
    k: &ControllerKey,
    b: &RepositoryBinding,
) -> Result<(), E> {
    for (r, p) in [
        (&b.signing_key, "github-app-signing"),
        (&b.webhook_secret, "github-webhook"),
    ] {
        let s = r.scope(&b.project, p)?;
        current_reference_in(
            c,
            k,
            &s,
            SecretVersion::new(r.version).map_err(|_| E::InvalidInput)?,
        )
        .await
        .map_err(|_| E::Revoked)?;
    }
    Ok(())
}
async fn value(
    c: &mut sqlx::SqliteConnection,
    k: &ControllerKey,
    b: &git::consent::Consent,
    purpose: &str,
    now: i64,
) -> Result<SecretValue, E> {
    let current = git::consent::current(c, k, &b.binding.project, now).await?;
    if current.version != b.version {
        return Err(E::Revoked);
    }
    let r = if purpose == "github-app-signing" {
        &b.binding.signing_key
    } else {
        &b.binding.webhook_secret
    };
    let s = r.scope(&b.binding.project, purpose)?;
    current_reference_in(
        c,
        k,
        &s,
        SecretVersion::new(r.version).map_err(|_| E::Integrity)?,
    )
    .await
    .map_err(|_| E::Revoked)?;
    let e = store::load(
        c,
        &s,
        SecretVersion::new(r.version).map_err(|_| E::Integrity)?,
    )
    .await
    .map_err(|_| E::Integrity)?;
    crypto::open(k, &e).map_err(|_| E::Integrity)
}
pub(in crate::controller_keys) async fn jwt(
    c: &mut sqlx::SqliteConnection,
    k: &ControllerKey,
    b: &git::consent::Consent,
    now: i64,
) -> Result<Zeroizing<String>, E> {
    let plain = value(c, k, b, "github-app-signing", now).await?;
    if plain.0.len() > 16384 {
        return Err(E::ResourceLimit);
    }
    let v: SigningKey<'_> = serde_json::from_slice(&plain.0).map_err(|_| E::Integrity)?;
    if v.format != 1 || v.app_id != b.binding.app.get() {
        return Err(E::Integrity);
    }
    let der = Zeroizing::new(STANDARD.decode(v.pkcs8).map_err(|_| E::Integrity)?);
    let pair = ring::signature::RsaKeyPair::from_pkcs8(&der).map_err(|_| E::Integrity)?;
    let claims = serde_json::json!({"iat":now.checked_sub(60).ok_or(E::InvalidInput)?,"exp":now.checked_add(540).ok_or(E::InvalidInput)?,"iss":b.binding.app.get().to_string()});
    let header = URL_SAFE_NO_PAD.encode(b"{\"alg\":\"RS256\",\"typ\":\"JWT\"}");
    let input = Zeroizing::new(format!(
        "{header}.{}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).map_err(|_| E::Integrity)?)
    ));
    let mut signature = vec![0u8; pair.public().modulus_len()];
    pair.sign(
        &ring::signature::RSA_PKCS1_SHA256,
        &ring::rand::SystemRandom::new(),
        input.as_bytes(),
        &mut signature,
    )
    .map_err(|_| E::Integrity)?;
    Ok(Zeroizing::new(format!(
        "{}.{}",
        input.as_str(),
        URL_SAFE_NO_PAD.encode(signature)
    )))
}
pub(in crate::controller_keys) async fn verify_webhook(
    c: &mut sqlx::SqliteConnection,
    k: &ControllerKey,
    b: &git::consent::Consent,
    now: i64,
    body: &[u8],
    tag: &[u8],
) -> Result<(), E> {
    // Webhooks may still arrive after a restrictive event: current key custody,
    // immutable binding and current Broker version are checked, but an already-held
    // binding must remain able to authenticate another revocation event.
    let r = &b.binding.webhook_secret;
    let s = r.scope(&b.binding.project, "github-webhook")?;
    current_reference_in(
        c,
        k,
        &s,
        SecretVersion::new(r.version).map_err(|_| E::Integrity)?,
    )
    .await
    .map_err(|_| E::Revoked)?;
    let e = store::load(
        c,
        &s,
        SecretVersion::new(r.version).map_err(|_| E::Integrity)?,
    )
    .await
    .map_err(|_| E::Integrity)?;
    let plain = crypto::open(k, &e).map_err(|_| E::Integrity)?;
    if !(32..=128).contains(&plain.0.len()) {
        return Err(E::Integrity);
    }
    let mut m = <Hmac<Sha256> as Mac>::new_from_slice(&plain.0).map_err(|_| E::Integrity)?;
    m.update(body);
    let _ = now;
    m.verify_slice(tag).map_err(|_| E::Denied)
}
