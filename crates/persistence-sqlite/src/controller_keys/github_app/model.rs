use super::*;
use crate::controller_keys::secret_broker as broker;
use rolvya_domain::ids::ProjectId;
use serde::{Deserialize, Serialize};
macro_rules! number {
    ($n:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $n(i64);
        impl $n {
            pub fn new(n: i64) -> Result<Self, GitError> {
                if n > 0 {
                    Ok(Self(n))
                } else {
                    Err(GitError::InvalidInput)
                }
            }
            pub fn get(self) -> i64 {
                self.0
            }
        }
    };
}
number!(RepositoryId);
number!(InstallationId);
number!(AppId);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GitSha(String);
impl GitSha {
    pub fn parse(s: impl Into<String>) -> Result<Self, GitError> {
        let s = s.into();
        if s.len() != 40
            || !s
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(GitError::InvalidInput);
        }
        Ok(Self(s))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CredentialReference {
    pub connection: String,
    pub account: String,
    pub record: String,
    pub version: u64,
}
impl CredentialReference {
    pub(in crate::controller_keys) fn scope(
        &self,
        p: &ProjectId,
        purpose: &str,
    ) -> Result<broker::RecordScope, GitError> {
        Ok(broker::RecordScope {
            connection: broker::ConnectionScope {
                project: p.clone(),
                connection: broker::ConnectionId::new(self.connection.clone())
                    .map_err(|_| GitError::InvalidInput)?,
                account: broker::AccountId::new(self.account.clone())
                    .map_err(|_| GitError::InvalidInput)?,
                provider: broker::ProviderId::new("github").map_err(|_| GitError::InvalidInput)?,
                mode: broker::ProviderMode::ApiKey,
            },
            record: broker::RecordId::new(self.record.clone())
                .map_err(|_| GitError::InvalidInput)?,
            purpose: broker::SecretPurpose::new(purpose).map_err(|_| GitError::InvalidInput)?,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    Read,
    Write,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallationPermissions {
    pub contents: Permission,
    pub metadata: Permission,
    pub pull_requests: Permission,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IsolatedNamespace {
    RolvyaAttemptsV1,
}
/// Request data only. Applying it requires exact fresh OwnerAction proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryBinding {
    pub project: ProjectId,
    pub app: AppId,
    pub installation: InstallationId,
    pub repository: RepositoryId,
    pub location: String,
    pub source_branch: String,
    pub permissions: InstallationPermissions,
    pub namespace: IsolatedNamespace,
    pub allowed_prefixes: Vec<String>,
    pub signing_key: CredentialReference,
    pub webhook_secret: CredentialReference,
    pub expected_version: i64,
    pub valid_until: i64,
    pub enabled: bool,
}
impl RepositoryBinding {
    pub(super) fn validate(&self) -> Result<(), GitError> {
        AppId::new(self.app.0)?;
        InstallationId::new(self.installation.0)?;
        RepositoryId::new(self.repository.0)?;
        location(&self.location)?;
        if self.permissions.metadata != Permission::Read {
            return Err(GitError::InvalidInput);
        }
        branch(&self.source_branch)?;
        if self.expected_version < 0
            || self.valid_until < 0
            || self.allowed_prefixes.is_empty()
            || self.allowed_prefixes.len() > 16
        {
            return Err(GitError::InvalidInput);
        }
        for p in &self.allowed_prefixes {
            path(p.trim_end_matches('/'))?;
            if !p.ends_with('/') {
                return Err(GitError::InvalidInput);
            }
        }
        for (r, p) in [
            (&self.signing_key, "github-app-signing"),
            (&self.webhook_secret, "github-webhook"),
        ] {
            r.scope(&self.project, p)?;
            broker::SecretVersion::new(r.version).map_err(|_| GitError::InvalidInput)?;
        }
        Ok(())
    }
}
pub(super) fn location(s: &str) -> Result<(), GitError> {
    let v: Vec<_> = s.split('/').collect();
    if v.len() != 2
        || v.iter().any(|s| {
            s.is_empty()
                || s.len() > 100
                || *s == "."
                || *s == ".."
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
    {
        return Err(GitError::InvalidInput);
    }
    Ok(())
}
pub(super) fn branch(s: &str) -> Result<(), GitError> {
    if s.is_empty()
        || s.len() > 120
        || s.starts_with('/')
        || s.ends_with('/')
        || s.ends_with('.')
        || s.contains("..")
        || s.contains("@{")
        || s.split('/')
            .any(|c| c.is_empty() || c.starts_with('.') || c.ends_with(".lock"))
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_/.".contains(&b))
    {
        return Err(GitError::InvalidInput);
    }
    Ok(())
}
pub(super) fn path(s: &str) -> Result<(), GitError> {
    if s.is_empty()
        || s.len() > 240
        || s.split('/').any(|c| {
            c.is_empty()
                || c == "."
                || c == ".."
                || c.starts_with('.')
                || c.to_ascii_lowercase().ends_with(".lock")
        })
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b))
    {
        return Err(GitError::InvalidInput);
    }
    let lower = s.to_ascii_lowercase();
    if lower.split('/').any(|c| {
        matches!(
            c,
            "security.md"
                | "agents.md"
                | "codeowners"
                | "cargo.toml"
                | "cargo.lock"
                | "package.json"
                | "dockerfile"
        ) || c.contains("credential")
            || c.contains("secret")
    }) {
        return Err(GitError::Denied);
    }
    Ok(())
}
pub(super) fn allowed(b: &RepositoryBinding, p: &str) -> Result<(), GitError> {
    path(p)?;
    if !b.allowed_prefixes.iter().any(|a| p.starts_with(a)) {
        return Err(GitError::Denied);
    }
    Ok(())
}
pub(super) fn target(installation: &str, project: &str, task: &str, attempt: &str) -> String {
    format!(
        "refs/heads/rolvya/{}/{}/{}/{}",
        hash(installation.as_bytes()),
        hash(project.as_bytes()),
        hash(task.as_bytes()),
        hash(attempt.as_bytes())
    )
}
/// Git's SHA-1 object identity only; authorization and artifact commitments use SHA-256/HMAC.
pub(super) fn object(kind: &str, b: &[u8]) -> GitSha {
    let mut c = ring::digest::Context::new(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY);
    c.update(format!("{kind} {}\0", b.len()).as_bytes());
    c.update(b);
    GitSha(hex(c.finish().as_ref()))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFile {
    pub path: String,
    pub mode: String,
    pub sha: GitSha,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedSource {
    pub repository: RepositoryId,
    pub commit: GitSha,
    pub tree: GitSha,
    pub files: Vec<SourceFile>,
}
