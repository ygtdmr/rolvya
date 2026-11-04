//! Account-specific declarations are checked against registry-selected identity.
//! They become external evidence only through a separately trusted collector.
use super::*;
use crate::controller_keys::provider_accounts::CodingModelBinding;
use rolvya_domain::plan::AccessMode;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountObservation {
    pub version: u16,
    pub binding: CodingModelBinding,
    pub model: String,
    pub account_identity_sha256: String,
    pub permission_sha256: String,
    pub policy_source: String,
    pub policy_revision_sha256: String,
    pub observed_at: i64,
    pub expires_at: i64,
    pub quota_remaining: u64,
    pub concurrent_limit: u16,
    pub cli: Option<CliQualification>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CliQualification {
    pub official_version: String,
    pub executable_sha256: String,
    pub authentication: CliAuthentication,
    pub hosted_permission_sha256: String,
    pub account_home_sha256: String,
    pub refresh_serialized: bool,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CliAuthentication {
    OfficialRemoteLogin,
}
impl AccountObservation {
    pub fn inspect(
        &self,
        expected: &CodingModelBinding,
        model: &str,
        now: i64,
    ) -> Result<String, E> {
        if self.version != 1
            || &self.binding != expected
            || self.model != model
            || !atom(model)
            || self.observed_at < 0
            || now < self.observed_at
            || now >= self.expires_at
            || self
                .expires_at
                .checked_sub(self.observed_at)
                .is_none_or(|v| v > 86400)
            || self.quota_remaining == 0
            || self.concurrent_limit == 0
            || self.concurrent_limit > 64
            || [
                &self.account_identity_sha256,
                &self.permission_sha256,
                &self.policy_revision_sha256,
            ]
            .iter()
            .any(|s| !hash(s))
            || [
                expected.connection_version,
                expected.grant_version,
                expected.credential_version,
            ]
            .iter()
            .any(|v| *v < 1)
        {
            return Err(E::Scope);
        }
        let source = url::Url::parse(&self.policy_source).map_err(|_| E::InvalidInput)?;
        let allowed = match expected.provider.as_str() {
            "openai" => [
                "developers.openai.com",
                "platform.openai.com",
                "openai.com",
                "learn.chatgpt.com",
            ]
            .as_slice(),
            "anthropic" => ["platform.claude.com", "code.claude.com", "anthropic.com"].as_slice(),
            _ => return Err(E::Scope),
        };
        if source.scheme() != "https"
            || !source.username().is_empty()
            || source.password().is_some()
            || source.port().is_some()
            || source.query().is_some()
            || source.fragment().is_some()
            || !source.host_str().is_some_and(|h| allowed.contains(&h))
        {
            return Err(E::Scope);
        }
        match (expected.mode, &self.cli) {
            (AccessMode::ApiKey, None) => {
                if !matches!(
                    (expected.provider.as_str(), expected.integration.as_str()),
                    ("openai", "openai-responses-v1") | ("anthropic", "anthropic-messages-v1")
                ) {
                    return Err(E::Scope);
                }
            }
            (AccessMode::CliSession, Some(cli)) => {
                if !matches!(
                    (expected.provider.as_str(), expected.integration.as_str()),
                    ("openai", "codex-exec-json-v1") | ("anthropic", "claude-print-json-v1")
                ) || !atom(&cli.official_version)
                    || !hash(&cli.executable_sha256)
                    || !hash(&cli.hosted_permission_sha256)
                    || !hash(&cli.account_home_sha256)
                    || !cli.refresh_serialized
                {
                    return Err(E::Scope);
                }
            }
            _ => return Err(E::Scope),
        }
        digest("rolvya/account-qualification/v1", self)
    }
}
