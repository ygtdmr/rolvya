use super::super::*;
use super::{
    context::*,
    tls::{SecretBytes, ServerIdentity},
    wire,
};
use axum::{
    Extension, Router,
    body::{Body, Bytes, to_bytes},
    extract::State,
    http::{Request, Response, StatusCode, header},
    routing::post,
};
use base64::Engine as _;
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    service::TowerToHyperService,
};
use std::{future::Future, sync::atomic::Ordering, time::Duration};
use tokio::{
    net::TcpListener,
    sync::{Semaphore, watch},
    task::JoinSet,
};

pub struct HttpsWorkerServer {
    pub(super) security: WorkerSecurity,
    pub(super) transport: Arc<HttpTransport>,
    pub(super) identity: ServerIdentity,
    pub(super) secure_fargate: Option<Arc<super::super::aws_fargate::SecureFargateBootstrap>>,
}
impl HttpsWorkerServer {
    /// Production identity verification remains unimplemented/default-deny. This
    /// constructs TLS transport, not provider enrollment or launch authority.
    pub async fn open(
        bootstrap: &ControllerBootstrap,
        policy: Arc<dyn SchedulerPolicy>,
        identity: ServerIdentity,
        limits: ResourceLimits,
    ) -> Result<Arc<Self>, WorkerAuthError> {
        let mut security = WorkerSecurity::open(bootstrap).await?;
        security.policy = policy;
        let transport = HttpTransport::new(limits, security.clock.clone());
        security.transport = transport.clone();
        Ok(Arc::new(Self {
            security,
            transport,
            identity,
            secure_fargate: None,
        }))
    }
    /// Selects the concrete read-only AWS correlation boundary. Production
    /// consent/credential resolution, task possession and delivery remain blocked.
    pub async fn open_fargate(
        bootstrap: &ControllerBootstrap,
        policy: Arc<dyn SchedulerPolicy>,
        identity: ServerIdentity,
        limits: ResourceLimits,
    ) -> Result<Arc<Self>, WorkerAuthError> {
        let aws = super::super::aws_fargate::FargateIdentity::open(bootstrap, policy.clone())
            .await
            .map_err(|_| WorkerAuthError::Unavailable)?;
        let mut s = Self::open(bootstrap, policy, identity, limits).await?;
        let state = Arc::get_mut(&mut s).ok_or(WorkerAuthError::Unavailable)?;
        state.security.identity = Arc::new(super::super::aws_fargate::HttpsIdentity {
            identity: aws,
            transport: state.transport.clone(),
        });
        Ok(s)
    }
    /// The concrete possession verifier remains gated on independent effective
    /// delivery/IAM/launch evidence. No caller can install a success provider.
    pub async fn open_secure_fargate(
        bootstrap: &ControllerBootstrap,
        policy: Arc<dyn SchedulerPolicy>,
        identity: ServerIdentity,
        limits: ResourceLimits,
    ) -> Result<Arc<Self>, WorkerAuthError> {
        let service =
            super::super::aws_fargate::SecureFargateBootstrap::open(bootstrap, policy.clone())
                .await
                .map_err(|_| WorkerAuthError::Unavailable)?;
        let mut server = Self::open(bootstrap, policy, identity, limits).await?;
        let s = Arc::get_mut(&mut server).ok_or(WorkerAuthError::Unavailable)?;
        s.security.identity = Arc::new(super::super::aws_fargate::PossessionIdentity {
            service: service.clone(),
            transport: s.transport.clone(),
        });
        s.secure_fargate = Some(service);
        Ok(server)
    }
    /// Trusted local controller orchestration only, not an HTTP setup route.
    /// The response contains metadata, never the protected bootstrap material.
    pub async fn prepare_fargate_bootstrap(
        &self,
        r: &PrepareBootstrap,
        plan: &super::super::aws_fargate::ProtectedDelivery,
    ) -> Result<super::super::aws_fargate::PossessionChallenge, WorkerAuthError> {
        let service = self
            .secure_fargate
            .as_ref()
            .ok_or(WorkerAuthError::Unavailable)?;
        service
            .register_plan(r, plan)
            .await
            .map_err(|_| WorkerAuthError::Denied)?;
        let ticket = self.security.prepare_bootstrap(r).await?;
        service
            .publish(&ticket.id, &r.request_id)
            .await
            .map_err(|_| WorkerAuthError::Unavailable)
    }
    pub fn diagnostics(&self) -> Diagnostics {
        self.transport.diagnostics()
    }
    /// Only this TLS accept loop can construct request-channel evidence; no router
    /// or cleartext privileged service is publicly exposed.
    pub async fn serve(
        self: Arc<Self>,
        listener: TcpListener,
        shutdown: impl Future<Output = ()> + Send,
    ) -> Result<(), wire::ErrorCode> {
        let connections = Arc::new(Semaphore::new(self.transport.limits.connections));
        let acceptor = tokio_rustls::TlsAcceptor::from(self.identity.config.clone());
        let (stop, stopped) = watch::channel(false);
        let mut tasks = JoinSet::new();
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                _ = &mut shutdown => break,
                Some(_) = tasks.join_next(), if !tasks.is_empty() => {},
                incoming = listener.accept(), if tasks.len() < self.transport.limits.connections => {
                    let (socket, peer) = incoming.map_err(|_| wire::ErrorCode::Unavailable)?;
                    let Ok(permit) = connections.clone().try_acquire_owned() else {
                        drop(socket);
                        continue;
                    };
                    let state = self.clone();
                    let acceptor = acceptor.clone();
                    let mut stopping = stopped.clone();
                    tasks.spawn(async move {
                        let _permit = permit;
                        let handshake = tokio::time::timeout(
                            Duration::from_secs(state.transport.limits.handshake_seconds),
                            acceptor.accept(socket),
                        ).await;
                        let Ok(Ok(tls)) = handshake else { return; };
                        let mut exporter = Zeroizing::new([0u8; 32]);
                        if tls.get_ref().1.export_keying_material(
                            &mut *exporter, b"EXPORTER-Rolvya-worker-v1", Some(b"wire-v1"),
                        ).is_err() { return; }
                        let channel = TlsChannel {
                            exporter: Sha256::digest(exporter.as_slice()).into(),
                            peer: peer.ip(),
                        };
                        let router = Router::new()
                            .route("/v1/worker", post(handle))
                            .fallback(not_found)
                            .layer(Extension(channel))
                            .with_state(state.clone());
                        let service = TowerToHyperService::new(router);
                        let mut builder = hyper::server::conn::http1::Builder::new();
                        builder.timer(TokioTimer::new())
                            .header_read_timeout(Duration::from_secs(5))
                            .max_headers(32).max_buf_size(16384).half_close(false);
                        let connection = builder.serve_connection(TokioIo::new(tls), service);
                        tokio::pin!(connection);
                        tokio::select! {
                            _ = &mut connection => {},
                            // Includes backpressured writes after handler completion.
                            // Closing transport is never evidence of DB rollback.
                            _ = tokio::time::sleep(Duration::from_secs(state.transport.limits.connection_seconds)) => {},
                            _ = stopping.changed() => {
                                connection.as_mut().graceful_shutdown();
                                let _ = tokio::time::timeout(Duration::from_secs(10), &mut connection).await;
                            }
                        }
                    });
                }
            }
        }
        let _ = stop.send(true);
        let drain = async { while tasks.join_next().await.is_some() {} };
        if tokio::time::timeout(Duration::from_secs(15), drain)
            .await
            .is_err()
        {
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
        }
        Ok(())
    }
}
async fn not_found() -> Response<Body> {
    error_response(wire::ErrorCode::InvalidProtocol, StatusCode::NOT_FOUND)
}
fn response(body: SecretBytes, status: StatusCode) -> Response<Body> {
    let mut r = Response::new(Body::from(Bytes::from_owner(body)));
    *r.status_mut() = status;
    r.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );
    r.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    r.headers_mut()
        .insert(header::PRAGMA, header::HeaderValue::from_static("no-cache"));
    r.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    r
}
fn error_response(code: wire::ErrorCode, status: StatusCode) -> Response<Body> {
    let body = serde_json::to_vec(&wire::ErrorResponse {
        version: wire::WIRE_VERSION,
        code,
    })
    .unwrap_or_else(|_| b"{\"version\":1,\"code\":\"unavailable\"}".to_vec());
    response(SecretBytes(Zeroizing::new(body)), status)
}
fn status(code: wire::ErrorCode) -> StatusCode {
    use wire::ErrorCode::*;
    match code {
        Unauthenticated | ExpiredCredential | RevokedCredential => StatusCode::UNAUTHORIZED,
        UnauthorizedScope
        | FencedEpoch
        | LeaseExpired
        | FinanciallyBlocked
        | ProviderEntitlementUnavailable => StatusCode::FORBIDDEN,
        InvalidProtocol | InvalidArtifact => StatusCode::BAD_REQUEST,
        StaleVersion | ConflictingReplay => StatusCode::CONFLICT,
        RateLimited => StatusCode::TOO_MANY_REQUESTS,
        TransientStorage | Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}
pub(super) fn auth_error(error: WorkerAuthError) -> wire::ErrorCode {
    use wire::ErrorCode as E;
    match error {
        WorkerAuthError::Protocol | WorkerAuthError::InvalidInput => E::InvalidProtocol,
        WorkerAuthError::Expired => E::ExpiredCredential,
        WorkerAuthError::Conflict => E::ConflictingReplay,
        WorkerAuthError::FinancialRisk => E::FinanciallyBlocked,
        WorkerAuthError::Busy | WorkerAuthError::Storage => E::TransientStorage,
        WorkerAuthError::ResourceLimit => E::RateLimited,
        WorkerAuthError::Unavailable
        | WorkerAuthError::KeyUnavailable
        | WorkerAuthError::Randomness => E::Unavailable,
        _ => E::Unauthenticated,
    }
}
async fn handle(
    State(state): State<Arc<HttpsWorkerServer>>,
    Extension(channel): Extension<TlsChannel>,
    r: Request<Body>,
) -> Response<Body> {
    let result = tokio::time::timeout(
        Duration::from_secs(state.transport.limits.request_seconds),
        state.process(channel, r),
    )
    .await;
    match result {
        Ok(Ok(r)) => r,
        Ok(Err(code)) => {
            state
                .transport
                .counters
                .denied
                .fetch_add(1, Ordering::Relaxed);
            error_response(code, status(code))
        }
        Err(_) => error_response(wire::ErrorCode::Unavailable, StatusCode::REQUEST_TIMEOUT),
    }
}
impl HttpsWorkerServer {
    async fn process(
        &self,
        channel: TlsChannel,
        r: Request<Body>,
    ) -> Result<Response<Body>, wire::ErrorCode> {
        use wire::ErrorCode as E;
        self.transport
            .network_rate_limit(&channel)
            .map_err(auth_error)?;
        if r.uri().query().is_some() || r.headers().contains_key(header::COOKIE) {
            return Err(E::InvalidProtocol);
        }
        if r.headers()
            .get(header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .map(|v| v.split(';').next().unwrap_or_default())
            != Some("application/json")
        {
            return Err(E::InvalidProtocol);
        }
        let authorization = r
            .headers()
            .get(header::AUTHORIZATION)
            .ok_or(E::Unauthenticated)?;
        if authorization.len() != 71 {
            return Err(E::Unauthenticated);
        }
        let raw = Zeroizing::new(
            authorization
                .to_str()
                .map_err(|_| E::Unauthenticated)?
                .strip_prefix("Bearer ")
                .ok_or(E::Unauthenticated)?
                .to_owned(),
        );
        let credential = WorkerCredential::parse(raw.to_string()).map_err(auth_error)?;
        let bytes = to_bytes(r.into_body(), wire::MAX_HTTP_BODY)
            .await
            .map_err(|_| E::InvalidProtocol)?;
        let frame: wire::Request =
            serde_json::from_slice(&bytes).map_err(|_| E::InvalidProtocol)?;
        frame.validate()?;
        no_bearer_reference(
            &credential,
            &[&serde_json::to_vec(&frame).map_err(|_| E::InvalidProtocol)?],
        )
        .map_err(auth_error)?;
        let mut read = self
            .security
            .store
            .pool
            .begin()
            .await
            .map_err(|_| E::TransientStorage)?;
        let now = self.security.now().map_err(auth_error)?;
        let binding = match &frame.operation {
            wire::Operation::Bootstrap { challenge, .. } => {
                let enrollment: String = sqlx::query_scalar(
                    "SELECT enrollment_id FROM worker_bootstrap_challenges WHERE challenge_id=?",
                )
                .bind(challenge.as_str())
                .fetch_optional(&mut *read)
                .await
                .map_err(|_| E::TransientStorage)?
                .ok_or(E::Unauthenticated)?;
                self.security
                    .enrollment(
                        &mut read,
                        &WorkerEnrollmentId::parse(enrollment).map_err(auth_error)?,
                    )
                    .await
                    .map_err(auth_error)?
                    .scope
                    .binding
            }
            _ => match self
                .security
                .current_credential(&mut read, &credential, now)
                .await
            {
                Ok(c) => c.metadata.binding,
                Err(e) => {
                    drop(read);
                    return Err(self
                        .denial(&credential, e, worker_control::CredentialScopeUse::Control)
                        .await);
                }
            },
        };
        read.commit().await.map_err(|_| E::TransientStorage)?;
        let operation = match &frame.operation {
            wire::Operation::Bootstrap { .. } => WorkerOperation::Register,
            wire::Operation::Renew { .. } | wire::Operation::PreparePossession {} => {
                WorkerOperation::RenewCredential
            }
            wire::Operation::Message { message, .. } => {
                crate::worker_simulation::controller::operation(&message.body)
            }
            wire::Operation::Assignment {} => WorkerOperation::Claim,
        };
        let guard = self
            .transport
            .reserve(&channel, &binding, operation, &frame.request_id)
            .map_err(auth_error)?;
        match frame.operation {
            wire::Operation::Bootstrap {
                challenge,
                attestation,
            } => {
                guard
                    .attach_attestation(attestation.clone())
                    .map_err(auth_error)?;
                let bootstrap = BootstrapSecret(credential);
                let ticket = self
                    .security
                    .claim_bootstrap(BootstrapClaim {
                        id: &challenge,
                        secret: &bootstrap,
                        evidence: &attestation.reference,
                        context: &guard.context,
                    })
                    .await
                    .map_err(auth_error)?;
                self.security
                    .deliver_credential(&ticket, &guard.context)
                    .await
                    .map_err(auth_error)?;
                Ok(response(
                    guard.take_response().map_err(auth_error)?,
                    StatusCode::OK,
                ))
            }
            wire::Operation::PreparePossession {} => {
                let service = self.secure_fargate.as_ref().ok_or(E::Unavailable)?;
                let challenge = service
                    .prepare_renewal(&self.security, &credential, &guard.context)
                    .await
                    .map_err(auth_error)?;
                let body = serde_json::to_vec(&wire::Response::Possession {
                    version: wire::WIRE_VERSION,
                    challenge: Box::new(challenge),
                })
                .map_err(|_| E::Unavailable)?;
                Ok(response(SecretBytes(Zeroizing::new(body)), StatusCode::OK))
            }
            wire::Operation::Renew { attestation } => {
                guard
                    .attach_attestation(attestation.clone())
                    .map_err(auth_error)?;
                let ticket = match self
                    .security
                    .renew_credential(&credential, &attestation.reference, &guard.context)
                    .await
                {
                    Ok(t) => t,
                    Err(e) => {
                        return Err(self
                            .denial(&credential, e, worker_control::CredentialScopeUse::Renew)
                            .await);
                    }
                };
                self.security
                    .deliver_credential(&ticket, &guard.context)
                    .await
                    .map_err(auth_error)?;
                Ok(response(
                    guard.take_response().map_err(auth_error)?,
                    StatusCode::OK,
                ))
            }
            wire::Operation::Message { message, artifact } => {
                if message.binding != binding {
                    return Err(E::UnauthorizedScope);
                }
                if let Some(artifact) = artifact {
                    self.artifact(&binding, &artifact).await?;
                }
                let bytes = crate::worker_simulation::encode_worker_message(&message)
                    .map_err(|_| E::InvalidProtocol)?;
                let (answer, replayed) = match self
                    .security
                    .receive_receipt(&credential, &bytes, &guard.context)
                    .await
                {
                    Ok(a) => a,
                    Err(e) => {
                        return Err(self
                            .denial(
                                &credential,
                                e,
                                if matches!(
                                    operation,
                                    WorkerOperation::StopQuery | WorkerOperation::StopAck
                                ) {
                                    worker_control::CredentialScopeUse::Stop
                                } else {
                                    worker_control::CredentialScopeUse::Control
                                },
                            )
                            .await);
                    }
                };
                let body = serde_json::to_vec(&wire::Response::Message {
                    version: wire::WIRE_VERSION,
                    response: answer,
                    disposition: if replayed {
                        wire::ReceiptDisposition::IdempotentReplay
                    } else {
                        wire::ReceiptDisposition::Applied
                    },
                })
                .map_err(|_| E::Unavailable)?;
                Ok(response(SecretBytes(Zeroizing::new(body)), StatusCode::OK))
            }
            wire::Operation::Assignment {} => {
                let assignment = match self.assignment(&credential, &guard.context).await {
                    Ok(a) => a,
                    Err(E::Unauthenticated) => {
                        return Err(self
                            .denial(
                                &credential,
                                WorkerAuthError::Denied,
                                worker_control::CredentialScopeUse::Control,
                            )
                            .await);
                    }
                    Err(e) => return Err(e),
                };
                let body = serde_json::to_vec(&wire::Response::Assignment {
                    version: wire::WIRE_VERSION,
                    assignment,
                })
                .map_err(|_| E::Unavailable)?;
                if body.len() > wire::MAX_RESPONSE_BODY {
                    return Err(E::Unavailable);
                }
                Ok(response(SecretBytes(Zeroizing::new(body)), StatusCode::OK))
            }
        }
    }
    async fn artifact(
        &self,
        b: &WorkerBinding,
        a: &wire::ArtifactUpload,
    ) -> Result<(), wire::ErrorCode> {
        use wire::ErrorCode as E;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&a.data)
            .map_err(|_| E::InvalidArtifact)?;
        if bytes.len() > 8192 || format!("{:x}", Sha256::digest(&bytes)) != a.sha256.as_str() {
            return Err(E::InvalidArtifact);
        }
        let row = sqlx::query(
            "SELECT content,sha256 FROM synthetic_artifacts WHERE project_id=? AND attempt_id=?",
        )
        .bind(b.task.plan.project_id.as_str())
        .bind(b.attempt_id.as_str())
        .fetch_optional(&self.security.store.pool)
        .await
        .map_err(|_| E::TransientStorage)?
        .ok_or(E::InvalidArtifact)?;
        if row
            .try_get::<&[u8], _>("content")
            .map_err(|_| E::InvalidArtifact)?
            != bytes
            || row
                .try_get::<&str, _>("sha256")
                .map_err(|_| E::InvalidArtifact)?
                != a.sha256.as_str()
        {
            return Err(E::InvalidArtifact);
        }
        Ok(())
    }
    async fn assignment(
        &self,
        credential: &WorkerCredential,
        c: &WorkerRequestContext,
    ) -> Result<wire::Assignment, wire::ErrorCode> {
        use wire::ErrorCode as E;
        let mut tx = self
            .security
            .store
            .pool
            .begin()
            .await
            .map_err(|_| E::TransientStorage)?;
        let now = self.security.now().map_err(auth_error)?;
        let current = self
            .security
            .current_credential(&mut tx, credential, now)
            .await
            .map_err(auth_error)?;
        let e = current.enrollment;
        if !e.identity.permissions.contains(&WorkerPermission::Claim) {
            return Err(E::UnauthorizedScope);
        }
        self.security
            .transport
            .verify(c, &e.scope.binding, WorkerOperation::Claim, now)
            .map_err(auth_error)?;
        if let Err(error) = self
            .security
            .scope(
                &mut tx,
                &e.lease(),
                now,
                worker_control::CredentialScopeUse::Control,
            )
            .await
        {
            drop(tx);
            return Err(self
                .denial(
                    credential,
                    error,
                    worker_control::CredentialScopeUse::Control,
                )
                .await);
        }
        self.security
            .identity
            .control_in(
                &mut tx,
                &e,
                &c.request_id,
                now,
                worker_control::CredentialScopeUse::Control,
            )
            .await
            .map_err(auth_error)?;
        let (binding, a, inputs) =
            crate::orchestration::synthetic_worker_assignment(&mut tx, &e.lease())
                .await
                .map_err(|_| E::Unavailable)?;
        let until = admission::stored_admission(&mut tx, &e.lease())
            .await
            .map_err(|_| E::LeaseExpired)?
            .1;
        if binding != e.scope.binding
            || !e.identity.capabilities.supports(&e.scope.profile)
            || (self.secure_fargate.is_none() && a.capabilities != e.identity.capabilities)
        {
            return Err(E::UnauthorizedScope);
        }
        tx.commit().await.map_err(|_| E::TransientStorage)?;
        Ok(wire::Assignment {
            binding,
            capabilities: e.identity.capabilities,
            seed: a.seed,
            inputs,
            initial_lease_until: until,
            heartbeat_seconds: HEARTBEAT_INTERVAL_SECONDS,
        })
    }
}
