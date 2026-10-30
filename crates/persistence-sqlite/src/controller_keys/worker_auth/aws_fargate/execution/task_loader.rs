//! Fargate-only credential source: select the official ECS provider alone, with a
//! connector that cannot follow environment URLs, profiles, proxies or redirects.
//! Effective remote role confinement remains a separate production gate.
use super::*;
use aws_smithy_runtime_api::client::{
    http::{
        HttpConnector, HttpConnectorFuture, SharedHttpClient, SharedHttpConnector, http_client_fn,
    },
    orchestrator::{HttpRequest, HttpResponse},
    result::ConnectorError,
};
use aws_smithy_types::body::SdkBody;
use std::time::Duration;
use zeroize::Zeroizing;
struct TaskRoleSource {
    path: String,
}
impl TaskRoleSource {
    fn capture() -> Result<Self, E> {
        for name in [
            "AWS_CONTAINER_CREDENTIALS_FULL_URI",
            "AWS_CONTAINER_AUTHORIZATION_TOKEN",
            "AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE",
            "AWS_ACCESS_KEY_ID",
            "AWS_SECRET_ACCESS_KEY",
            "AWS_SESSION_TOKEN",
            "AWS_PROFILE",
            "AWS_DEFAULT_PROFILE",
            "AWS_SHARED_CREDENTIALS_FILE",
            "AWS_CONFIG_FILE",
            "AWS_WEB_IDENTITY_TOKEN_FILE",
            "AWS_ROLE_ARN",
        ] {
            if std::env::var_os(name).is_some() {
                return Err(E::UnverifiedControllerAuthority);
            }
        }
        let path = std::env::var("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI")
            .map_err(|_| E::UnverifiedControllerAuthority)?;
        validate_path(&path)?;
        Ok(Self { path })
    }
    async fn credentials(
        &self,
        transport: SharedHttpClient,
    ) -> Result<aws_credential_types::Credentials, E> {
        // Fail before the SDK can read token files or log invalid token overrides.
        if Self::capture()?.path != self.path {
            return Err(E::ScopeMismatch);
        }
        let config = aws_config::provider_config::ProviderConfig::empty()
            .with_http_client(transport)
            .with_retry_config(
                aws_sdk_ecs::config::retry::RetryConfig::standard().with_max_attempts(1),
            );
        let provider = aws_config::ecs::EcsCredentialsProvider::builder()
            .configure(&config)
            .connect_timeout(Duration::from_secs(1))
            .read_timeout(Duration::from_secs(1))
            .build();
        tokio::time::timeout(Duration::from_secs(3), provider.credentials())
            .await
            .map_err(|_| E::Transient)?
            .map_err(|_| E::UnverifiedControllerAuthority)
    }
}
fn validate_path(path: &str) -> Result<(), E> {
    let id = path
        .strip_prefix("/v2/credentials/")
        .ok_or(E::ScopeMismatch)?;
    if id.is_empty()
        || id.len() > 128
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(E::ScopeMismatch);
    }
    Ok(())
}
#[derive(Clone)]
struct MetadataOnly {
    path: String,
    http: reqwest::Client,
}
impl std::fmt::Debug for MetadataOnly {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FargateTaskCredentialEndpoint([redacted])")
    }
}
impl MetadataOnly {
    fn new(path: String) -> Result<Self, E> {
        validate_path(&path)?;
        let tls =
            crate::controller_keys::worker_auth::https::tls::client().map_err(|_| E::Transient)?;
        Ok(Self {
            path,
            http: reqwest::Client::builder()
                .tls_backend_preconfigured(tls.as_ref().clone())
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(1))
                .timeout(Duration::from_secs(2))
                .build()
                .map_err(|_| E::Transient)?,
        })
    }
    fn check(&self, r: &http::Request<SdkBody>) -> Result<url::Url, E> {
        let url = url::Url::parse(&r.uri().to_string()).map_err(|_| E::ScopeMismatch)?;
        if r.method() != http::Method::GET
            || url.scheme() != "http"
            || url.host_str() != Some("169.254.170.2")
            || url.port().is_some()
            || url.path() != self.path
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || r.headers().contains_key("authorization")
            || r.body().bytes().is_some_and(|b| !b.is_empty())
        {
            return Err(E::ScopeMismatch);
        }
        Ok(url)
    }
}
impl HttpConnector for MetadataOnly {
    fn call(&self, request: HttpRequest) -> HttpConnectorFuture {
        let c = self.clone();
        HttpConnectorFuture::new(async move {
            let r = request.try_into_http1x().map_err(|_| failure())?;
            let uri = c.check(&r).map_err(|_| failure())?;
            let mut response = c.http.get(uri).send().await.map_err(|_| failure())?;
            if response.status() != 200 || response.content_length().is_some_and(|v| v > 16384) {
                return Err(failure());
            }
            let mut bytes = Zeroizing::new(Vec::new());
            while let Some(part) = response.chunk().await.map_err(|_| failure())? {
                if bytes.len() + part.len() > 16384 {
                    return Err(failure());
                }
                bytes.extend_from_slice(&part)
            }
            HttpResponse::try_from(
                http::Response::builder()
                    .status(200)
                    .body(SdkBody::from(axum::body::Bytes::from_owner(
                        crate::controller_keys::worker_auth::https::tls::SecretBytes(bytes),
                    )))
                    .map_err(|_| failure())?,
            )
            .map_err(|_| failure())
        })
    }
}
fn failure() -> ConnectorError {
    ConnectorError::user(Box::new(E::UnverifiedControllerAuthority))
}
/// Private capability: current effective IAM/task confinement has no production
/// verifier yet. No public request/DTO can construct or install this authority.
pub struct TaskBootstrapLoader {
    plan: ProtectedDelivery,
    source: TaskRoleSource,
}
impl TaskBootstrapLoader {
    pub fn for_current_task(plan: &ProtectedDelivery) -> Result<Self, E> {
        plan.validate()?;
        let _ = TaskRoleSource::capture()?;
        // Offline implementation cannot verify effective real-task IAM confinement.
        Err(E::SecureDeliveryUnverified)
    }
    /// One bounded retrieval of the exact secret VERSION. ResourceNotFound is a
    /// retryable startup observation, never fallback to AWSCURRENT/account/profile.
    pub async fn fetch_once(&self, now: i64) -> Result<DeliveredPossession, E> {
        self.plan.validate()?;
        if now >= self.plan.expires_at {
            return Err(E::ExpiredEvidence);
        }
        let connector = SharedHttpConnector::new(MetadataOnly::new(self.source.path.clone())?);
        let http = http_client_fn(move |_, _| connector.clone());
        let credentials = self.source.credentials(http).await?;
        super::super::secrets_sdk::SecretSdk::new(&self.plan, credentials)?
            .fetch(now)
            .await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn task_metadata_path_and_transport_reject_ambient_url_and_authority() {
        for path in [
            "/v2/credentials/",
            "/v2/credentials/../root",
            "/v2/credentials/id?x=y",
            "https://attacker.invalid/x",
        ] {
            assert!(validate_path(path).is_err())
        }
        let c = MetadataOnly::new("/v2/credentials/fixture".into()).unwrap();
        for uri in [
            "https://attacker.invalid/v2/credentials/fixture",
            "http://127.0.0.1/v2/credentials/fixture",
            "http://169.254.170.2/v2/credentials/other",
            "http://169.254.170.2/v2/credentials/fixture?x=y",
        ] {
            let req = http::Request::builder()
                .uri(uri)
                .body(SdkBody::empty())
                .unwrap();
            assert!(c.check(&req).is_err())
        }
        let req = http::Request::builder()
            .uri("http://169.254.170.2/v2/credentials/fixture")
            .body(SdkBody::empty())
            .unwrap();
        assert!(c.check(&req).is_ok());
    }
    #[tokio::test]
    async fn official_ecs_provider_child() {
        if std::env::var("ROLVYA_PRIVATE_TASK_LOADER_CHILD")
            .ok()
            .as_deref()
            != Some("1")
        {
            return;
        }
        #[derive(Clone, Debug)]
        struct Fake;
        impl HttpConnector for Fake {
            fn call(&self, r: HttpRequest) -> HttpConnectorFuture {
                HttpConnectorFuture::new(async move {
                    let r = r.try_into_http1x().unwrap();
                    MetadataOnly::new("/v2/credentials/fixture".into())
                        .unwrap()
                        .check(&r)
                        .unwrap();
                    let marker = super::super::super::super::new_id().unwrap();
                    let body = serde_json::json!({"AccessKeyId":format!("synthetic-{marker}"),"SecretAccessKey":super::super::super::super::new_id().unwrap(),"Token":super::super::super::super::new_id().unwrap(),"Expiration":"2099-01-01T00:00:00Z"});
                    Ok(HttpResponse::try_from(
                        http::Response::builder()
                            .status(200)
                            .body(SdkBody::from(body.to_string()))
                            .unwrap(),
                    )
                    .unwrap())
                })
            }
        }
        let c = SharedHttpConnector::new(Fake);
        let source = TaskRoleSource::capture().unwrap();
        let credentials = source
            .credentials(http_client_fn(move |_, _| c.clone()))
            .await
            .unwrap();
        assert!(credentials.access_key_id().starts_with("synthetic-"));
    }
    #[test]
    fn official_provider_isolated_from_host_profiles_and_secrets() {
        let status=std::process::Command::new(std::env::current_exe().unwrap()).env_clear().env("AWS_CONTAINER_CREDENTIALS_RELATIVE_URI","/v2/credentials/fixture").env("ROLVYA_PRIVATE_TASK_LOADER_CHILD","1").args(["--exact","controller_keys::worker_auth::aws_fargate::execution::task_loader::tests::official_ecs_provider_child","--nocapture"]).status().unwrap();
        assert!(status.success());
    }
}
