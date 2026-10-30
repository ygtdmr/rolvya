//! Exact-resource official SDK operations. No ambient credential chain and no
//! create/launch/delete operations. Publication requires the sealed effective-IAM
//! gate before this capability is constructed. Existing resources start empty.
use super::{
    sdk::{ReadOnlyHttps, service_error},
    secure::DeliveredPossession,
    *,
};
use aws_credential_types::Credentials;
use aws_smithy_runtime_api::client::http::{SharedHttpClient, SharedHttpConnector, http_client_fn};
use aws_smithy_types::body::SdkBody;
use serde_json::{Value, json};
#[cfg(test)]
use zeroize::Zeroizing;

pub(super) struct SecretSdk {
    client: aws_sdk_secretsmanager::Client,
    plan: ProtectedDelivery,
}
impl SecretSdk {
    pub(super) fn new(
        plan: &ProtectedDelivery,
        credentials: Credentials,
    ) -> Result<Self, AwsIdentityError> {
        let connector =
            SharedHttpConnector::new(ReadOnlyHttps::for_secret(&plan.execution.workload, plan)?);
        Self::with_http(
            plan,
            credentials,
            http_client_fn(move |_, _| connector.clone()),
        )
    }
    fn with_http(
        plan: &ProtectedDelivery,
        credentials: Credentials,
        http: SharedHttpClient,
    ) -> Result<Self, AwsIdentityError> {
        plan.validate()?;
        use aws_sdk_secretsmanager::config::{
            BehaviorVersion, Region, retry::RetryConfig, timeout::TimeoutConfig,
        };
        let conf = aws_sdk_secretsmanager::config::Builder::new()
            .region(Region::new(plan.secret.region().to_owned()))
            .credentials_provider(credentials)
            .http_client(http)
            .behavior_version(BehaviorVersion::latest())
            .retry_config(RetryConfig::disabled())
            .timeout_config(
                TimeoutConfig::builder()
                    .connect_timeout(std::time::Duration::from_secs(2))
                    .read_timeout(std::time::Duration::from_secs(4))
                    .operation_timeout(std::time::Duration::from_secs(6))
                    .build(),
            )
            .build();
        Ok(Self {
            client: aws_sdk_secretsmanager::Client::from_conf(conf),
            plan: plan.clone(),
        })
    }
    /// The resource must have been independently provisioned with its returned
    /// full ARN. Restrictive policy is installed and read back before ANY value.
    pub(super) async fn publish(
        &self,
        material: &DeliveredPossession,
        allowed_versions: &[String],
    ) -> Result<(), AwsIdentityError> {
        let p = &self.plan;
        if material.challenge().secret != p.secret
            || material.challenge().secret_version != p.version_id
            || material.challenge().execution != p.execution
            || material.challenge().expires_at != p.expires_at
        {
            return Err(AwsIdentityError::ScopeMismatch);
        }
        let metadata = self
            .client
            .describe_secret()
            .secret_id(p.secret.as_str())
            .send()
            .await
            .map_err(|e| service_error(e.as_service_error().and_then(|e| e.meta().code())))?;
        if metadata.arn() != Some(p.secret.as_str())
            || metadata.deleted_date().is_some()
            || metadata.rotation_enabled() == Some(true)
        {
            return Err(AwsIdentityError::SecureDeliveryUnverified);
        }
        // The exact resource is permanently reserved for this attempt. A retry
        // may find our stable version; an unrelated existing value fails closed.
        if metadata
            .version_ids_to_stages()
            .is_some_and(|v| v.keys().any(|id| !allowed_versions.contains(id)))
        {
            return Err(AwsIdentityError::Conflict);
        }
        let policy = p.resource_policy()?.to_string();
        let reviewed = self
            .client
            .validate_resource_policy()
            .secret_id(p.secret.as_str())
            .resource_policy(&policy)
            .send()
            .await
            .map_err(|e| service_error(e.as_service_error().and_then(|e| e.meta().code())))?;
        if !reviewed.policy_validation_passed() {
            return Err(AwsIdentityError::SecureDeliveryUnverified);
        }
        let applied = self
            .client
            .put_resource_policy()
            .secret_id(p.secret.as_str())
            .resource_policy(&policy)
            .block_public_policy(true)
            .send()
            .await
            .map_err(|e| service_error(e.as_service_error().and_then(|e| e.meta().code())))?;
        if applied.arn() != Some(p.secret.as_str()) {
            return Err(AwsIdentityError::ScopeMismatch);
        }
        let actual = self
            .client
            .get_resource_policy()
            .secret_id(p.secret.as_str())
            .send()
            .await
            .map_err(|e| service_error(e.as_service_error().and_then(|e| e.meta().code())))?;
        if actual.arn() != Some(p.secret.as_str())
            || serde_json::from_str::<Value>(
                actual
                    .resource_policy()
                    .ok_or(AwsIdentityError::SecureDeliveryUnverified)?,
            )
            .map_err(|_| AwsIdentityError::Integrity)?
                != p.resource_policy()?
        {
            return Err(AwsIdentityError::SecureDeliveryUnverified);
        }
        let body = material.payload()?;
        let value = self
            .client
            .put_secret_value()
            .secret_id(p.secret.as_str())
            .client_request_token(&p.version_id)
            .secret_string(body.as_str())
            .send()
            .await
            .map_err(|e| service_error(e.as_service_error().and_then(|e| e.meta().code())))?;
        if value.arn() != Some(p.secret.as_str())
            || value.version_id() != Some(p.version_id.as_str())
        {
            return Err(AwsIdentityError::ScopeMismatch);
        }
        Ok(())
    }
    /// Task-role consumption is intentionally private until trusted remote task
    /// credential loading and effective isolation can be established. Tests use
    /// the real SDK wire codec with an independently scoped private role fixture.
    #[cfg(test)]
    pub(super) async fn fetch(&self, now: i64) -> Result<DeliveredPossession, AwsIdentityError> {
        let p = &self.plan;
        if now >= p.expires_at {
            return Err(AwsIdentityError::ExpiredEvidence);
        }
        let mut reply = self
            .client
            .get_secret_value()
            .secret_id(p.secret.as_str())
            .version_id(&p.version_id)
            .send()
            .await
            .map_err(|e| service_error(e.as_service_error().and_then(|e| e.meta().code())))?;
        if reply.arn() != Some(p.secret.as_str())
            || reply.version_id() != Some(p.version_id.as_str())
            || reply.secret_binary().is_some()
        {
            return Err(AwsIdentityError::ScopeMismatch);
        }
        let bytes = Zeroizing::new(
            reply
                .secret_string
                .take()
                .ok_or(AwsIdentityError::Integrity)?,
        );
        DeliveredPossession::parse(p, bytes.as_bytes(), now)
    }
    #[cfg(test)]
    pub(super) fn fixture(p: &ProtectedDelivery, c: Credentials, http: SharedHttpClient) -> Self {
        Self::with_http(p, c, http).unwrap()
    }
}
pub(super) fn check_request(
    p: &ProtectedDelivery,
    r: &http::Request<SdkBody>,
) -> Result<(), AwsIdentityError> {
    let target = r
        .headers()
        .get("x-amz-target")
        .and_then(|v| v.to_str().ok())
        .ok_or(AwsIdentityError::InvalidInput)?;
    let b: Value = serde_json::from_slice(r.body().bytes().ok_or(AwsIdentityError::InvalidInput)?)
        .map_err(|_| AwsIdentityError::InvalidInput)?;
    if b.get("SecretId").and_then(Value::as_str) != Some(p.secret.as_str()) {
        return Err(AwsIdentityError::ScopeMismatch);
    }
    let expected = match target {
        "secretsmanager.DescribeSecret" | "secretsmanager.GetResourcePolicy" => {
            json!({"SecretId":p.secret})
        }
        "secretsmanager.GetSecretValue" => json!({"SecretId":p.secret,"VersionId":p.version_id}),
        "secretsmanager.ValidateResourcePolicy" => {
            json!({"SecretId":p.secret,"ResourcePolicy":p.resource_policy()?.to_string()})
        }
        "secretsmanager.PutResourcePolicy" => {
            json!({"SecretId":p.secret,"ResourcePolicy":p.resource_policy()?.to_string(),"BlockPublicPolicy":true})
        }
        "secretsmanager.PutSecretValue" => {
            let value = b
                .get("SecretString")
                .and_then(Value::as_str)
                .ok_or(AwsIdentityError::InvalidInput)?;
            if value.len() > 16384 {
                return Err(AwsIdentityError::ResourceLimit);
            }
            json!({"SecretId":p.secret,"ClientRequestToken":p.version_id,"SecretString":value})
        }
        _ => return Err(AwsIdentityError::LiveLaunchDisabled),
    };
    if b != expected {
        return Err(AwsIdentityError::ScopeMismatch);
    }
    Ok(())
}
