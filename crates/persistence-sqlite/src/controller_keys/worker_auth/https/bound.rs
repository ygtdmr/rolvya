//! Single-origin, one-operation rustls channel. No raw credential/exporter export.
use super::*;
use crate::controller_keys::worker_auth::aws_fargate::{PossessionChallenge, PossessionOperation};
use hyper::{body::Incoming, client::conn::http1::SendRequest};
use hyper_util::rt::TokioIo;
use sha2::{Digest as _, Sha256};
use tokio::{net::TcpStream, task::JoinHandle};
struct Channel {
    sender: SendRequest<axum::body::Body>,
    driver: JoinHandle<()>,
    exporter: [u8; 32],
    authority: String,
}
impl Drop for Channel {
    fn drop(&mut self) {
        self.driver.abort();
    }
}
impl Channel {
    async fn connect(
        endpoint: &ControllerEndpoint,
        tls: Arc<rustls::ClientConfig>,
    ) -> Result<Self, ClientError> {
        let url = &endpoint.0;
        let host = url.host_str().ok_or(ClientError::Endpoint)?;
        let port = url.port_or_known_default().ok_or(ClientError::Endpoint)?;
        let name = rustls::pki_types::ServerName::try_from(host.to_owned())
            .map_err(|_| ClientError::Endpoint)?;
        tokio::time::timeout(Duration::from_secs(5), async {
            let socket = TcpStream::connect((host, port))
                .await
                .map_err(|_| ClientError::TlsOrNetwork)?;
            let stream = tokio_rustls::TlsConnector::from(tls)
                .connect(name, socket)
                .await
                .map_err(|_| ClientError::TlsOrNetwork)?;
            let mut raw = Zeroizing::new([0u8; 32]);
            stream
                .get_ref()
                .1
                .export_keying_material(&mut *raw, b"EXPORTER-Rolvya-worker-v1", Some(b"wire-v1"))
                .map_err(|_| ClientError::TlsOrNetwork)?;
            let exporter = Sha256::digest(raw.as_slice()).into();
            let (sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
                .await
                .map_err(|_| ClientError::TlsOrNetwork)?;
            let driver = tokio::spawn(async move {
                let _ = connection.await;
            });
            let authority = if host.contains(':') {
                format!("[{host}]:{port}")
            } else {
                format!("{host}:{port}")
            };
            Ok(Self {
                sender,
                driver,
                exporter,
                authority,
            })
        })
        .await
        .map_err(|_| ClientError::TlsOrNetwork)?
    }
    async fn exchange(
        &mut self,
        request: &Request,
        credential: &WorkerCredential,
    ) -> Result<Zeroizing<Vec<u8>>, ClientError> {
        request.validate().map_err(ClientError::Rejected)?;
        let bytes = Zeroizing::new(serde_json::to_vec(request).map_err(|_| ClientError::Protocol)?);
        if bytes.len() > MAX_HTTP_BODY {
            return Err(ClientError::Protocol);
        }
        let mut bearer = Zeroizing::new(String::from("Bearer "));
        bearer.push_str(&credential.0);
        let mut header = http::HeaderValue::from_str(&bearer).map_err(|_| ClientError::Protocol)?;
        header.set_sensitive(true);
        let body = axum::body::Body::from(axum::body::Bytes::from_owner(tls::SecretBytes(bytes)));
        let req = http::Request::builder()
            .method("POST")
            .uri("/v1/worker")
            .header(http::header::HOST, &self.authority)
            .header(http::header::CONTENT_TYPE, "application/json")
            .header(http::header::AUTHORIZATION, header)
            .body(body)
            .map_err(|_| ClientError::Protocol)?;
        tokio::time::timeout(Duration::from_secs(10), async {
            let reply: http::Response<Incoming> = self
                .sender
                .send_request(req)
                .await
                .map_err(|_| ClientError::CredentialDeliveryUnknown)?;
            let status = reply.status();
            let bytes =
                axum::body::to_bytes(axum::body::Body::new(reply.into_body()), MAX_RESPONSE_BODY)
                    .await
                    .map_err(|_| ClientError::CredentialDeliveryUnknown)?;
            let body = Zeroizing::new(bytes.to_vec());
            if !status.is_success() {
                let e: ErrorResponse =
                    serde_json::from_slice(&body).map_err(|_| ClientError::Protocol)?;
                if e.version != WIRE_VERSION {
                    return Err(ClientError::Protocol);
                }
                return Err(ClientError::Rejected(e.code));
            }
            Ok(body)
        })
        .await
        .map_err(|_| ClientError::CredentialDeliveryUnknown)?
    }
}
impl WorkerHttpClient {
    /// The supplied secret must originate in the protected delivery consumer.
    /// Server time and stored challenge authority are authoritative. No retries.
    pub async fn possession_bootstrap(
        &mut self,
        id: RequestId,
        challenge: &PossessionChallenge,
        secret: &BootstrapSecret,
    ) -> Result<(), ClientError> {
        if self.credential.is_some() || challenge.operation != PossessionOperation::Bootstrap {
            return Err(ClientError::Protocol);
        }
        let mut channel = Channel::connect(&self.endpoint, self.tls_config.clone()).await?;
        let frame = proof_frame(id, challenge, secret, &channel.exporter)?;
        let body = channel.exchange(&frame, &secret.0).await?;
        self.accept_credential(&frame, &body, &challenge.execution.binding, None)
    }
    /// A separately delivered fresh renewal challenge is mandatory. Retired
    /// bootstrap material cannot substitute for this operation/generation.
    pub async fn possession_renew(
        &mut self,
        id: RequestId,
        challenge: &PossessionChallenge,
        secret: &BootstrapSecret,
    ) -> Result<(), ClientError> {
        let old = self.metadata.clone().ok_or(ClientError::NotAuthenticated)?;
        if challenge.operation != PossessionOperation::Renew
            || challenge.execution.binding != old.binding
            || challenge.credential.as_ref() != Some(&old.id)
            || old.generation.checked_add(1) != Some(challenge.generation)
        {
            return Err(ClientError::Protocol);
        }
        let result = async {
            let mut channel = Channel::connect(&self.endpoint, self.tls_config.clone()).await?;
            let frame = proof_frame(id, challenge, secret, &channel.exporter)?;
            let body = channel
                .exchange(
                    &frame,
                    self.credential
                        .as_ref()
                        .ok_or(ClientError::NotAuthenticated)?,
                )
                .await?;
            self.accept_credential(&frame, &body, &old.binding, Some(&old))
        }
        .await;
        if result.is_err() {
            self.credential = None;
            self.metadata = None;
        }
        result
    }
}
fn proof_frame(
    id: RequestId,
    c: &PossessionChallenge,
    secret: &BootstrapSecret,
    channel: &[u8; 32],
) -> Result<Request, ClientError> {
    let signature = c
        .sign(secret, &id, channel, c.issued_at)
        .map_err(|_| ClientError::Protocol)?;
    let attestation = Attestation {
        reference: crate::finance::EvidenceRef::new(c.id.as_str())
            .map_err(|_| ClientError::Protocol)?,
        statement: serde_json::to_string(c).map_err(|_| ClientError::Protocol)?,
        signature: crate::controller_keys::worker_auth::hex(&signature),
    };
    let operation = match c.operation {
        PossessionOperation::Bootstrap => Operation::Bootstrap {
            challenge: c.id.clone(),
            attestation,
        },
        PossessionOperation::Renew => Operation::Renew { attestation },
    };
    Ok(Request {
        version: WIRE_VERSION,
        request_id: id,
        operation,
    })
}

#[cfg(test)]
impl WorkerHttpClient {
    pub(in crate::controller_keys::worker_auth) async fn fixture_native_probe(
        &self,
        id: RequestId,
        challenge: &PossessionChallenge,
        secret: &BootstrapSecret,
        wrong_channel: bool,
    ) -> Result<(), ClientError> {
        let mut a = Channel::connect(&self.endpoint, self.tls_config.clone()).await?;
        let frame = proof_frame(id, challenge, secret, &a.exporter)?;
        if wrong_channel {
            let mut b = Channel::connect(&self.endpoint, self.tls_config.clone()).await?;
            assert_ne!(
                a.exporter, b.exporter,
                "independent live TLS connections have distinct exporters"
            );
            let _ = b.exchange(&frame, &secret.0).await?;
        } else {
            // Deliberately discard a committed response. No token enters client
            // state and no recovery endpoint reconstructs the lost credential.
            let _ = a.exchange(&frame, &secret.0).await?;
        }
        Ok(())
    }
}
