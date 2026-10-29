//! Official AWS SDKs with explicit credentials and a read-only, bounded HTTPS
//! connector. No default credential chain, endpoint override, RunTask or StopTask.
use super::super::https::tls;
use super::{AwsIdentityError as E, *};
use aws_credential_types::Credentials;
use aws_sdk_ecs::config::{BehaviorVersion, Region, retry::RetryConfig, timeout::TimeoutConfig};
use aws_smithy_runtime_api::client::orchestrator::{HttpRequest, HttpResponse};
use aws_smithy_runtime_api::client::{
    http::{
        HttpConnector, HttpConnectorFuture, SharedHttpClient, SharedHttpConnector, http_client_fn,
    },
    result::ConnectorError,
};
use aws_smithy_types::body::SdkBody;
use std::{fmt, time::Duration};
use zeroize::Zeroizing;

const MAX_AWS_BODY: usize = 512 * 1024;
#[derive(Clone)]
struct ReadOnlyHttps {
    http: reqwest::Client,
    ecs: String,
    sts: String,
}
impl fmt::Debug for ReadOnlyHttps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ReadOnlyAwsHttps([redacted request/credential state])")
    }
}
fn connector_error() -> ConnectorError {
    ConnectorError::user(Box::new(E::InvalidInput))
}
impl ReadOnlyHttps {
    fn new(w: &FargateWorkload) -> Result<Self, E> {
        let tls = tls::client().map_err(|_| E::Transient)?;
        let http = reqwest::Client::builder()
            .tls_backend_preconfigured(tls.as_ref().clone())
            .https_only(true)
            .http1_only()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(4))
            .pool_max_idle_per_host(2)
            .pool_idle_timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| E::Transient)?;
        Ok(Self {
            http,
            ecs: format!(
                "ecs.{}.{}",
                w.cluster.region(),
                w.cluster.partition().dns_suffix()
            ),
            sts: format!(
                "sts.{}.{}",
                w.cluster.region(),
                w.cluster.partition().dns_suffix()
            ),
        })
    }
    fn check(&self, request: &http::Request<SdkBody>) -> Result<url::Url, E> {
        let uri = url::Url::parse(&request.uri().to_string()).map_err(|_| E::InvalidInput)?;
        if uri.scheme() != "https"
            || uri.path() != "/"
            || uri.query().is_some()
            || uri.fragment().is_some()
            || uri.port().is_some_and(|p| p != 443)
            || !uri.username().is_empty()
            || uri.password().is_some()
            || request.method() != http::Method::POST
            || request.headers().len() > 32
            || request
                .headers()
                .iter()
                .any(|(k, v)| k.as_str().len() + v.len() > 8192)
        {
            return Err(E::InvalidInput);
        }
        let body = request.body().bytes().ok_or(E::InvalidInput)?;
        if body.len() > 16384 {
            return Err(E::InvalidInput);
        }
        match uri.host_str() {
            Some(h) if h == self.ecs => {
                match request
                    .headers()
                    .get("x-amz-target")
                    .and_then(|v| v.to_str().ok())
                {
                    Some(
                        "AmazonEC2ContainerServiceV20141113.DescribeTasks"
                        | "AmazonEC2ContainerServiceV20141113.DescribeTaskDefinition",
                    ) => {}
                    _ => return Err(E::LiveLaunchDisabled),
                }
            }
            Some(h) if h == self.sts => {
                let entries: Vec<_> = url::form_urlencoded::parse(body).collect();
                if entries.len() != 2 {
                    return Err(E::LiveLaunchDisabled);
                }
                let pairs: std::collections::BTreeMap<_, _> = entries.into_iter().collect();
                if pairs.len() != 2
                    || pairs.get("Action").map(|v| v.as_ref()) != Some("GetCallerIdentity")
                    || pairs.get("Version").map(|v| v.as_ref()) != Some("2011-06-15")
                {
                    return Err(E::LiveLaunchDisabled);
                }
            }
            _ => return Err(E::ScopeMismatch),
        }
        Ok(uri)
    }
}
impl HttpConnector for ReadOnlyHttps {
    fn call(&self, request: HttpRequest) -> HttpConnectorFuture {
        let client = self.clone();
        HttpConnectorFuture::new(async move {
            let mut request = request.try_into_http1x().map_err(|_| connector_error())?;
            let url = client.check(&request).map_err(|_| connector_error())?;
            for name in [
                http::header::AUTHORIZATION,
                http::header::HeaderName::from_static("x-amz-security-token"),
            ] {
                if let Some(h) = request.headers_mut().get_mut(name) {
                    h.set_sensitive(true);
                }
            }
            let body = Zeroizing::new(request.body().bytes().ok_or_else(connector_error)?.to_vec());
            let outgoing = axum::body::Bytes::from_owner(tls::SecretBytes(body));
            let mut response = client
                .http
                .post(url)
                .headers(request.headers().clone())
                .body(outgoing)
                .send()
                .await
                .map_err(|_| ConnectorError::io(Box::new(E::Transient)))?;
            if response
                .content_length()
                .is_some_and(|n| n > MAX_AWS_BODY as u64)
            {
                return Err(connector_error());
            }
            let status = response.status();
            let headers = response.headers().clone();
            let mut bytes = Zeroizing::new(Vec::with_capacity(2048));
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| ConnectorError::io(Box::new(E::Transient)))?
            {
                if bytes.len().saturating_add(chunk.len()) > MAX_AWS_BODY {
                    return Err(connector_error());
                }
                bytes.extend_from_slice(&chunk);
            }
            let mut result = http::Response::builder()
                .status(status)
                .body(SdkBody::from(axum::body::Bytes::from_owner(
                    tls::SecretBytes(bytes),
                )))
                .map_err(|_| connector_error())?;
            *result.headers_mut() = headers;
            HttpResponse::try_from(result).map_err(|_| connector_error())
        })
    }
}
pub(super) struct SdkClients {
    ecs: aws_sdk_ecs::Client,
    sts: aws_sdk_sts::Client,
}
impl fmt::Debug for SdkClients {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ScopedAwsSdkClients([redacted credentials])")
    }
}
impl SdkClients {
    pub(super) fn new(w: &FargateWorkload, credentials: Credentials) -> Result<Self, E> {
        w.validate()?;
        let connector = SharedHttpConnector::new(ReadOnlyHttps::new(w)?);
        Self::with_http(
            w,
            credentials,
            http_client_fn(move |_, _| connector.clone()),
        )
    }
    fn with_http(
        w: &FargateWorkload,
        credentials: Credentials,
        http: SharedHttpClient,
    ) -> Result<Self, E> {
        w.validate()?;
        let timeout = TimeoutConfig::builder()
            .connect_timeout(Duration::from_secs(2))
            .read_timeout(Duration::from_secs(4))
            .operation_timeout(Duration::from_secs(6))
            .operation_attempt_timeout(Duration::from_secs(6))
            .build();
        let ecs = aws_sdk_ecs::config::Builder::new()
            .region(Region::new(w.cluster.region().to_owned()))
            .credentials_provider(credentials.clone())
            .http_client(http.clone())
            .behavior_version(BehaviorVersion::latest())
            .retry_config(RetryConfig::disabled())
            .timeout_config(timeout.clone())
            .build();
        let sts = aws_sdk_sts::config::Builder::new()
            .region(Region::new(w.cluster.region().to_owned()))
            .credentials_provider(credentials)
            .http_client(http)
            .behavior_version(BehaviorVersion::latest())
            .retry_config(RetryConfig::disabled())
            .timeout_config(timeout)
            .build();
        Ok(Self {
            ecs: aws_sdk_ecs::Client::from_conf(ecs),
            sts: aws_sdk_sts::Client::from_conf(sts),
        })
    }
    pub(super) async fn inspect(
        &self,
        expected: &ExpectedExecution,
        now: i64,
    ) -> Result<InspectionAssessment, E> {
        expected.validate()?;
        let caller = self
            .sts
            .get_caller_identity()
            .send()
            .await
            .map_err(|e| service_error(e.as_service_error().and_then(|e| e.meta().code())))?;
        super::inspection::controller_identity(&expected.workload, &caller)?;
        let tasks = self
            .ecs
            .describe_tasks()
            .cluster(expected.workload.cluster.as_str())
            .tasks(expected.task.as_str())
            .send()
            .await
            .map_err(|e| service_error(e.as_service_error().and_then(|e| e.meta().code())))?;
        let task = super::inspection::task_record(expected, &tasks)?;
        let definition = self
            .ecs
            .describe_task_definition()
            .task_definition(expected.workload.task_definition.as_str())
            .send()
            .await
            .map_err(|e| service_error(e.as_service_error().and_then(|e| e.meta().code())))?;
        super::inspection::correlate(
            expected,
            task,
            definition
                .task_definition()
                .ok_or(E::TaskDefinitionMismatch)?,
            now,
        )
    }
    #[cfg(test)]
    pub(super) fn fixture(w: &FargateWorkload, c: Credentials, http: SharedHttpClient) -> Self {
        Self::with_http(w, c, http).unwrap()
    }
}
fn service_error(code: Option<&str>) -> E {
    match code {
        Some(
            "Throttling"
            | "ThrottlingException"
            | "TooManyRequestsException"
            | "RequestLimitExceeded",
        ) => E::Throttled,
        Some(
            "AccessDenied"
            | "AccessDeniedException"
            | "UnrecognizedClientException"
            | "InvalidClientTokenId"
            | "ExpiredToken"
            | "ExpiredTokenException",
        ) => E::UnverifiedControllerAuthority,
        _ => E::Transient,
    }
}

#[cfg(test)]
#[path = "sdk_tests.rs"]
pub(super) mod tests;
