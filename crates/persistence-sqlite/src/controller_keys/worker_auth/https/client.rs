//! Reusable worker-side HTTPS I/O. No SQL/store calls or local repository access.
use super::super::{BootstrapSecret, WorkerCredential, WorkerCredentialMetadata};
use super::{tls, wire::*};
use crate::RequestId;
use base64::Engine as _;
use rolvya_domain::worker::*;
use serde::Deserialize;
use std::{fmt, sync::Arc, time::Duration};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientError {
    Endpoint,
    TlsOrNetwork,
    Rejected(ErrorCode),
    Protocol,
    ResponseTooLarge,
    CredentialDeliveryUnknown,
    NotAuthenticated,
    Randomness,
}
impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "worker HTTPS: {self:?}")
    }
}
impl std::error::Error for ClientError {}
pub struct ControllerEndpoint(url::Url);
impl ControllerEndpoint {
    pub fn parse(value: &str) -> Result<Self, ClientError> {
        if value.len() > 2048 {
            return Err(ClientError::Endpoint);
        }
        let url = url::Url::parse(value).map_err(|_| ClientError::Endpoint)?;
        if value.len() > 2048
            || url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
            || url.port() == Some(0)
        {
            return Err(ClientError::Endpoint);
        }
        Ok(Self(url))
    }
}
pub struct WorkerHttpClient {
    http: reqwest::Client,
    tls_config: Arc<rustls::ClientConfig>,
    endpoint: ControllerEndpoint,
    credential: Option<WorkerCredential>,
    metadata: Option<WorkerCredentialMetadata>,
}
impl fmt::Debug for WorkerHttpClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WorkerHttpClient([redacted credentials])")
    }
}
impl WorkerHttpClient {
    pub fn new(endpoint: ControllerEndpoint) -> Result<Self, ClientError> {
        Self::with_tls(
            endpoint,
            tls::client().map_err(|_| ClientError::TlsOrNetwork)?,
        )
    }
    pub(super) fn with_tls(
        endpoint: ControllerEndpoint,
        config: Arc<rustls::ClientConfig>,
    ) -> Result<Self, ClientError> {
        let http = reqwest::Client::builder()
            .tls_backend_preconfigured(config.as_ref().clone())
            .https_only(true)
            .http1_only()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(2)
            .pool_idle_timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| ClientError::TlsOrNetwork)?;
        Ok(Self {
            http,
            tls_config: config,
            endpoint,
            credential: None,
            metadata: None,
        })
    }
    pub async fn prepare_possession(
        &self,
        id: RequestId,
    ) -> Result<super::super::aws_fargate::PossessionChallenge, ClientError> {
        let request = Request {
            version: WIRE_VERSION,
            request_id: id,
            operation: Operation::PreparePossession {},
        };
        let body = self
            .exchange(
                &request,
                self.credential
                    .as_ref()
                    .ok_or(ClientError::NotAuthenticated)?,
                false,
            )
            .await?;
        match serde_json::from_slice::<Response>(&body).map_err(|_| ClientError::Protocol)? {
            Response::Possession { version, challenge }
                if version == WIRE_VERSION
                    && self.metadata.as_ref().is_some_and(|m| {
                        challenge.execution.binding == m.binding
                            && challenge.credential.as_ref() == Some(&m.id)
                            && m.generation.checked_add(1) == Some(challenge.generation)
                    }) =>
            {
                Ok(*challenge)
            }
            _ => Err(ClientError::Protocol),
        }
    }
    pub fn metadata(&self) -> Option<&WorkerCredentialMetadata> {
        self.metadata.as_ref()
    }
    /// A failed delivery may have committed issuance. Reconcile out of band;
    /// never retrieve a last token or automatically retry bootstrap/renewal.
    pub async fn bootstrap(
        &mut self,
        id: RequestId,
        challenge: super::super::BootstrapChallengeId,
        secret: &BootstrapSecret,
        attestation: Attestation,
        expected: &WorkerBinding,
    ) -> Result<(), ClientError> {
        if self.credential.is_some() {
            return Err(ClientError::Protocol);
        }
        let request = Request {
            version: WIRE_VERSION,
            request_id: id,
            operation: Operation::Bootstrap {
                challenge,
                attestation,
            },
        };
        let body = self.exchange(&request, &secret.0, false).await?;
        self.accept_credential(&request, &body, expected, None)
    }
    pub async fn renew(
        &mut self,
        id: RequestId,
        attestation: Attestation,
    ) -> Result<(), ClientError> {
        let current = self
            .metadata
            .as_ref()
            .ok_or(ClientError::NotAuthenticated)?
            .clone();
        let request = Request {
            version: WIRE_VERSION,
            request_id: id,
            operation: Operation::Renew { attestation },
        };
        let body = self
            .exchange(
                &request,
                self.credential
                    .as_ref()
                    .ok_or(ClientError::NotAuthenticated)?,
                false,
            )
            .await;
        // The old generation is unusable after an ambiguous renewal. Losing its
        // response cannot revive it, even in this client's memory.
        let result = match body {
            Ok(body) => self.accept_credential(&request, &body, &current.binding, Some(&current)),
            Err(e) => Err(e),
        };
        if result.is_err() {
            self.credential = None;
            self.metadata = None;
        }
        result
    }
    fn accept_credential(
        &mut self,
        request: &Request,
        body: &[u8],
        expected: &WorkerBinding,
        old: Option<&WorkerCredentialMetadata>,
    ) -> Result<(), ClientError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Reply<'a> {
            version: u16,
            request_id: RequestId,
            metadata: WorkerCredentialMetadata,
            #[serde(borrow)]
            credential: &'a str,
        }
        let reply: Reply<'_> =
            serde_json::from_slice(body).map_err(|_| ClientError::CredentialDeliveryUnknown)?;
        let m = &reply.metadata;
        if reply.version != WIRE_VERSION
            || reply.request_id != request.request_id
            || m.binding != *expected
            || m.binding.validate().is_err()
            || !(1..=64).contains(&m.generation)
            || m.issued_at < 0
            || m.expires_at <= m.issued_at
            || m.expires_at
                .checked_sub(m.issued_at)
                .is_none_or(|ttl| ttl > i64::from(ACCESS_TOKEN_TTL_SECONDS))
            || super::super::WorkerCredentialId::parse(m.id.as_str().to_owned()).is_err()
            || super::super::WorkerEnrollmentId::parse(m.enrollment.as_str().to_owned()).is_err()
            || old.map_or(m.generation != 1, |o| {
                Some(m.generation) != o.generation.checked_add(1)
                    || m.enrollment != o.enrollment
                    || m.issued_at < o.issued_at
            })
        {
            return Err(ClientError::CredentialDeliveryUnknown);
        }
        let token = WorkerCredential::parse(reply.credential.to_owned())
            .map_err(|_| ClientError::CredentialDeliveryUnknown)?;
        self.credential = Some(token);
        self.metadata = Some(reply.metadata);
        Ok(())
    }
    /// Scheduling aid for an authenticated worker. Delayed ticks are not
    /// accumulated into a burst. Dropping the interval/request cancels local I/O;
    /// it never implies the server-side operation was rolled back.
    pub fn heartbeat_schedule(&self) -> Result<tokio::time::Interval, ClientError> {
        if self.credential.is_none() {
            return Err(ClientError::NotAuthenticated);
        }
        let period = Duration::from_secs(u64::from(HEARTBEAT_INTERVAL_SECONDS));
        let mut timer = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Ok(timer)
    }
    pub async fn assignment(&self, id: RequestId) -> Result<Assignment, ClientError> {
        let request = Request {
            version: WIRE_VERSION,
            request_id: id,
            operation: Operation::Assignment {},
        };
        let body = self
            .exchange(
                &request,
                self.credential
                    .as_ref()
                    .ok_or(ClientError::NotAuthenticated)?,
                true,
            )
            .await?;
        let response: Response =
            serde_json::from_slice(&body).map_err(|_| ClientError::Protocol)?;
        match response {
            Response::Assignment {
                version,
                assignment,
            } if version == WIRE_VERSION
                && assignment.binding
                    == self
                        .metadata
                        .as_ref()
                        .ok_or(ClientError::NotAuthenticated)?
                        .binding
                && assignment.heartbeat_seconds == HEARTBEAT_INTERVAL_SECONDS
                && assignment.inputs.len() <= 64
                && assignment.capabilities.validate().is_ok()
                && assignment.initial_lease_until >= 0 =>
            {
                Ok(assignment)
            }
            _ => Err(ClientError::Protocol),
        }
    }
    /// Stable message identity is retained across bounded network retries. The
    /// returned receipt is untrusted execution output, never task acceptance.
    pub async fn message(
        &self,
        message: WorkerMessage,
        artifact: Option<&[u8]>,
    ) -> Result<ControllerMessage, ClientError> {
        if message.binding
            != self
                .metadata
                .as_ref()
                .ok_or(ClientError::NotAuthenticated)?
                .binding
        {
            return Err(ClientError::Protocol);
        }
        let artifact = match artifact {
            Some(bytes) if bytes.len() <= 8192 => Some(ArtifactUpload {
                sha256: Digest::new(format!(
                    "{:x}",
                    <sha2::Sha256 as sha2::Digest>::digest(bytes)
                ))
                .map_err(|_| ClientError::Protocol)?,
                data: base64::engine::general_purpose::STANDARD.encode(bytes),
            }),
            Some(_) => return Err(ClientError::Protocol),
            None => None,
        };
        let request = Request {
            version: WIRE_VERSION,
            request_id: RequestId::new(message.message_id.as_str())
                .map_err(|_| ClientError::Protocol)?,
            operation: Operation::Message {
                message: Box::new(message.clone()),
                artifact,
            },
        };
        let body = self
            .exchange(
                &request,
                self.credential
                    .as_ref()
                    .ok_or(ClientError::NotAuthenticated)?,
                true,
            )
            .await?;
        match serde_json::from_slice::<Response>(&body).map_err(|_| ClientError::Protocol)? {
            Response::Message {
                version, response, ..
            } if version == WIRE_VERSION
                && response.binding == message.binding
                && response.response_to == message.message_id
                && response.validate().is_ok() =>
            {
                Ok(response)
            }
            _ => Err(ClientError::Protocol),
        }
    }
    #[cfg(test)]
    pub(super) fn fixture_stolen_credential(&self) -> Self {
        Self {
            http: self.http.clone(),
            tls_config: self.tls_config.clone(),
            endpoint: ControllerEndpoint(self.endpoint.0.clone()),
            credential: self
                .credential
                .as_ref()
                .map(|c| WorkerCredential::parse(c.0.to_string()).unwrap()),
            metadata: self.metadata.clone(),
        }
    }
    #[cfg(test)]
    pub(super) fn fixture_restore_from(&mut self, source: &Self) {
        let copy = source.fixture_stolen_credential();
        self.credential = copy.credential;
        self.metadata = copy.metadata;
    }
    #[cfg(test)]
    pub(super) async fn fixture_request(
        &self,
        make: impl FnOnce(&str) -> Vec<u8>,
    ) -> (u16, Zeroizing<Vec<u8>>) {
        let credential = self.credential.as_ref().expect("fixture authenticated");
        let body = Zeroizing::new(make(&credential.0));
        let mut auth = Zeroizing::new(String::from("Bearer "));
        auth.push_str(&credential.0);
        let mut header = reqwest::header::HeaderValue::from_str(&auth).unwrap();
        header.set_sensitive(true);
        let mut response = self
            .http
            .post(self.endpoint.0.join("v1/worker").unwrap())
            .header(reqwest::header::AUTHORIZATION, header)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(axum::body::Bytes::from_owner(tls::SecretBytes(body)))
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let mut body = Zeroizing::new(Vec::new());
        while let Some(c) = response.chunk().await.unwrap() {
            assert!(body.len() + c.len() <= MAX_RESPONSE_BODY);
            body.extend_from_slice(&c);
        }
        (status, body)
    }
    #[cfg(test)]
    pub(super) async fn fixture_unauthenticated_tls_probe(
        &self,
        r: &Request,
        c: &WorkerCredential,
    ) -> Result<Zeroizing<Vec<u8>>, ClientError> {
        self.exchange(r, c, false).await
    }
    async fn exchange(
        &self,
        request: &Request,
        credential: &WorkerCredential,
        retry: bool,
    ) -> Result<Zeroizing<Vec<u8>>, ClientError> {
        request.validate().map_err(ClientError::Rejected)?;
        let body = Zeroizing::new(serde_json::to_vec(request).map_err(|_| ClientError::Protocol)?);
        if body.len() > MAX_HTTP_BODY {
            return Err(ClientError::Protocol);
        }
        let url = self
            .endpoint
            .0
            .join("v1/worker")
            .map_err(|_| ClientError::Endpoint)?;
        let count = if retry { 3 } else { 1 };
        for attempt in 0..count {
            let mut bearer = Zeroizing::new(String::from("Bearer "));
            bearer.push_str(&credential.0);
            let mut authorization = reqwest::header::HeaderValue::from_str(&bearer)
                .map_err(|_| ClientError::Protocol)?;
            authorization.set_sensitive(true);
            let outgoing =
                axum::body::Bytes::from_owner(tls::SecretBytes(Zeroizing::new(body.to_vec())));
            let result = self
                .http
                .post(url.clone())
                .header(reqwest::header::AUTHORIZATION, authorization)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(outgoing)
                .send()
                .await;
            let outcome = match result {
                Ok(mut response) => {
                    let status = response.status();
                    if response
                        .content_length()
                        .is_some_and(|n| n > MAX_RESPONSE_BODY as u64)
                    {
                        return Err(ClientError::ResponseTooLarge);
                    }
                    let mut bytes = Zeroizing::new(Vec::with_capacity(2048));
                    let read = async {
                        while let Some(chunk) = response
                            .chunk()
                            .await
                            .map_err(|_| ClientError::TlsOrNetwork)?
                        {
                            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BODY {
                                return Err(ClientError::ResponseTooLarge);
                            }
                            bytes.extend_from_slice(&chunk);
                        }
                        Ok(())
                    }
                    .await;
                    match read {
                        Err(e) => Err(e),
                        Ok(()) if status.is_success() => return Ok(bytes),
                        Ok(()) => {
                            let error: ErrorResponse = serde_json::from_slice(&bytes)
                                .map_err(|_| ClientError::Protocol)?;
                            if error.version != WIRE_VERSION {
                                return Err(ClientError::Protocol);
                            }
                            if !matches!(
                                error.code,
                                ErrorCode::TransientStorage
                                    | ErrorCode::Unavailable
                                    | ErrorCode::RateLimited
                            ) {
                                return Err(ClientError::Rejected(error.code));
                            }
                            Err(ClientError::Rejected(error.code))
                        }
                    }
                }
                Err(_) => Err(ClientError::TlsOrNetwork),
            };
            if !retry {
                return match outcome {
                    Err(ClientError::TlsOrNetwork) => Err(ClientError::CredentialDeliveryUnknown),
                    other => other,
                };
            }
            if attempt + 1 == count {
                return outcome;
            }
            let mut jitter = [0u8; 1];
            getrandom::fill(&mut jitter).map_err(|_| ClientError::Randomness)?;
            tokio::time::sleep(Duration::from_millis(
                (100u64 << attempt) + u64::from(jitter[0] % 50),
            ))
            .await;
        }
        Err(ClientError::TlsOrNetwork)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controller_endpoint_cannot_leak_tokens_through_http_redirects_or_urls() {
        for value in [
            "http://localhost/",
            "https://user:password@localhost/",
            "https://localhost/?credential=x",
            "https://localhost/#token",
            "https://localhost/arbitrary/path",
        ] {
            assert!(
                ControllerEndpoint::parse(value).is_err(),
                "untrusted endpoint form must be rejected"
            );
        }
    }
    #[test]
    fn credential_reply_extreme_metadata_is_rejected_without_panic() {
        use rolvya_domain::{ids::*, plan::*};
        let binding = WorkerBinding {
            task: TaskRef {
                plan: PlanKey {
                    project_id: ProjectId::new("project").unwrap(),
                    plan_id: PlanId::new("plan").unwrap(),
                    version: PlanVersion::new(1).unwrap(),
                },
                task_id: TaskId::new("task").unwrap(),
            },
            attempt_id: AttemptId::new("attempt").unwrap(),
            worker_id: WorkerId::new("worker").unwrap(),
            epoch: 1,
            plan_hash: Digest::new("1".repeat(64)).unwrap(),
            context_hash: Digest::new("2".repeat(64)).unwrap(),
        };
        let r = Request {
            version: 1,
            request_id: RequestId::new("reply").unwrap(),
            operation: Operation::Assignment {},
        };
        let mut c = WorkerHttpClient::new(ControllerEndpoint::parse("https://localhost/").unwrap())
            .unwrap();
        let metadata = WorkerCredentialMetadata {
            id: super::super::super::WorkerCredentialId::parse("1".repeat(32)).unwrap(),
            enrollment: super::super::super::WorkerEnrollmentId::parse("2".repeat(32)).unwrap(),
            binding: binding.clone(),
            generation: 1,
            issued_at: i64::MIN,
            expires_at: i64::MAX,
        };
        let bytes=serde_json::to_vec(&serde_json::json!({"version":1,"request_id":r.request_id,"metadata":metadata,"credential":"not-a-token"})).unwrap();
        assert_eq!(
            c.accept_credential(&r, &bytes, &binding, None),
            Err(ClientError::CredentialDeliveryUnknown)
        );
    }
}

#[path = "bound.rs"]
mod bound;
