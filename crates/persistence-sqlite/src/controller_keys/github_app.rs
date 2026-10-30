//! Trusted remote Git foundation. No live App enrollment or network activation.
//! Workers propose data; only accepted bytes and current Owner consent can publish.
pub(in crate::controller_keys) mod consent;
mod http;
mod model;
mod publication;
mod webhook;
use super::{ControllerBootstrap, ControllerKey};
use crate::{
    PlanStore,
    admission::{Clock, SystemClock},
};
use hmac::{Hmac, Mac};
pub use model::*;
pub use publication::{PublicationReceipt, PublicationState, PullRequestPreparation, RemoteGit};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::sync::Arc;
pub use webhook::WebhookOutcome;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitError {
    InvalidInput,
    Denied,
    KeyUnavailable,
    Integrity,
    Conflict,
    Revoked,
    Expired,
    Unaccepted,
    StaleAttempt,
    Unsupported,
    ResourceLimit,
    Throttled,
    Unavailable,
    UnknownOutcome,
    Storage,
    Busy,
    LiveIntegrationDisabled,
}
impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "remote Git: {self:?}")
    }
}
impl std::error::Error for GitError {}
impl From<sqlx::Error> for GitError {
    fn from(e: sqlx::Error) -> Self {
        if matches!(crate::StoreError::from(e), crate::StoreError::Busy) {
            Self::Busy
        } else {
            Self::Storage
        }
    }
}
/// No public transport/verifier injection or credential accessor. Production calls
/// fail before networking until a separate reviewed live activation exists.
/// Persisted acceptance metadata cannot be manufactured as a public capability.
/// ```compile_fail
/// let _ = serde_json::from_str::<
///     rolvya_persistence_sqlite::controller_keys::github_app::AcceptedChange>("{}");
/// ```
pub struct GitHubRemote {
    store: PlanStore,
    key: ControllerKey,
    clock: Arc<dyn Clock>,
    http: http::Api,
    #[cfg(test)]
    hook: Box<dyn Fn(&str) + Send + Sync>,
}
impl GitHubRemote {
    pub async fn open(b: &ControllerBootstrap) -> Result<Self, GitError> {
        Ok(Self {
            store: b.store.clone(),
            key: b
                .load_existing()
                .await
                .map_err(|_| GitError::KeyUnavailable)?,
            clock: Arc::new(SystemClock),
            http: http::Api::disabled(),
            #[cfg(test)]
            hook: Box::new(|_| {}),
        })
    }
    fn now(&self) -> Result<i64, GitError> {
        self.clock.now().map_err(|_| GitError::Unavailable)
    }
    fn checkpoint(&self, p: &str) {
        #[cfg(test)]
        (self.hook)(p);
        #[cfg(not(test))]
        let _ = p;
    }
}
fn encode(v: &impl Serialize) -> Result<String, GitError> {
    let s = serde_json::to_string(v).map_err(|_| GitError::InvalidInput)?;
    if s.len() > 2 * 1024 * 1024 {
        return Err(GitError::ResourceLimit);
    }
    Ok(s)
}
fn decode<T: DeserializeOwned>(s: &str) -> Result<T, GitError> {
    if s.len() > 2 * 1024 * 1024 {
        return Err(GitError::ResourceLimit);
    }
    serde_json::from_str(s).map_err(|_| GitError::Integrity)
}
fn hash(v: &[u8]) -> String {
    format!("{:x}", Sha256::digest(v))
}
fn hex(v: &[u8]) -> String {
    v.iter().map(|x| format!("{x:02x}")).collect()
}
fn mac(k: &ControllerKey, domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut m = Hmac::<Sha256>::new_from_slice(&k.material[..]).expect("HMAC key length");
    for p in [
        b"rolvya/github/v1".as_slice(),
        &k.metadata.installation.0,
        &k.metadata.key.0,
        &k.metadata.key_version.to_be_bytes(),
        domain,
    ]
    .into_iter()
    .chain(parts.iter().copied())
    {
        m.update(&(p.len() as u32).to_be_bytes());
        m.update(p)
    }
    m.finalize().into_bytes().into()
}
fn equal(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    a.len() == b.len() && bool::from(a.ct_eq(b))
}
fn one(n: u64) -> Result<(), GitError> {
    if n == 1 {
        Ok(())
    } else {
        Err(GitError::Storage)
    }
}
pub(crate) fn coding_blob(bytes: &[u8]) -> GitSha {
    model::object("blob", bytes)
}
#[cfg(test)]
mod tests;

pub(crate) fn validate_coding_source(source: &PinnedSource) -> Result<(), GitError> {
    publication::validate_source(source)
}
