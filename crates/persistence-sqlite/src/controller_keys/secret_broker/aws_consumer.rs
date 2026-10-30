//! Narrow trusted SDK capability, not a provider-credential export API.
use super::*;
use crate::controller_keys::worker_auth::aws_fargate::{
    AwsIdentityError as E, credential_use::VerifiedInfrastructureUse,
};
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Credential<'a> {
    format: u16,
    account_id: &'a str,
    access_key_id: &'a str,
    secret_access_key: &'a str,
    session_token: Option<&'a str>,
    expires_at: i64,
}
pub(in crate::controller_keys) struct ScopedAwsCredentials {
    pub credentials: aws_credential_types::Credentials,
    pub expires_at: i64,
}
pub(in crate::controller_keys) async fn consume(
    c: &mut sqlx::SqliteConnection,
    k: &ControllerKey,
    use_scope: &VerifiedInfrastructureUse,
    now: i64,
) -> Result<ScopedAwsCredentials, E> {
    use_scope.current(c, k, now).await?;
    let r = &use_scope.consent().enrollment;
    let key = &r.credential;
    let scope = RecordScope {
        connection: ConnectionScope {
            project: r.project.clone(),
            connection: ConnectionId::new(key.connection.clone()).map_err(|_| E::Integrity)?,
            account: AccountId::new(key.account.clone()).map_err(|_| E::Integrity)?,
            provider: ProviderId::new(key.provider.clone()).map_err(|_| E::Integrity)?,
            mode: ProviderMode::ApiKey,
        },
        record: RecordId::new(key.record.clone()).map_err(|_| E::Integrity)?,
        purpose: SecretPurpose::new(key.purpose.clone()).map_err(|_| E::Integrity)?,
    };
    if key.provider != "aws"
        || key.purpose != "aws-controller-inspection"
        || key.account != r.workload.cluster.account().as_str()
    {
        return Err(E::ScopeMismatch);
    }
    let version = SecretVersion::new(key.version as u64).map_err(|_| E::Integrity)?;
    current_reference_in(c, k, &scope, version)
        .await
        .map_err(|_| E::Revoked)?;
    let envelope = store::load(c, &scope, version)
        .await
        .map_err(|_| E::Integrity)?;
    let plain = crypto::open(k, &envelope).map_err(|_| E::Integrity)?;
    if plain.0.len() > 8192 {
        return Err(E::ResourceLimit);
    }
    let value: Credential<'_> = serde_json::from_slice(&plain.0).map_err(|_| E::Integrity)?;
    if value.format != 1
        || value.account_id != key.account
        || !(16..=128).contains(&value.access_key_id.len())
        || !(16..=128).contains(&value.secret_access_key.len())
        || value.access_key_id.chars().any(char::is_control)
        || value.secret_access_key.chars().any(char::is_control)
        || value
            .session_token
            .is_some_and(|s| s.is_empty() || s.len() > 4096 || s.chars().any(char::is_control))
        || value.expires_at <= now
    {
        return Err(E::Integrity);
    }
    let expires_at = value.expires_at.min(r.valid_until);
    let expires = std::time::UNIX_EPOCH
        .checked_add(std::time::Duration::from_secs(expires_at as u64))
        .ok_or(E::Integrity)?;
    let credentials = aws_credential_types::Credentials::new(
        value.access_key_id,
        value.secret_access_key,
        value.session_token.map(str::to_owned),
        Some(expires),
        "Rolvya-scoped-encrypted-Broker",
    );
    use_scope.audit(c, now).await?;
    Ok(ScopedAwsCredentials {
        credentials,
        expires_at,
    })
}
