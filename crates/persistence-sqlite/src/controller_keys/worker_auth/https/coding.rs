//! Existing TLS/credential authority composed with the coding evidence store.
use super::{super::*, context::*, server::*, tls::SecretBytes, wire};
use crate::orchestration::coding::execution::*;
use crate::orchestration::{OrchestrationError, SyntheticCoordinator};
use axum::{
    Extension,
    body::{Body, to_bytes},
    extract::State,
    http::{Request, Response, StatusCode, header},
};
use std::time::Duration;
struct SecurityClock<'a>(&'a dyn Clock);
impl Clock for SecurityClock<'_> {
    fn now(&self) -> Result<i64, admission::AdmissionError> {
        self.0.now()
    }
}
fn error(e: OrchestrationError) -> wire::ErrorCode {
    match e {
        OrchestrationError::LimitExceeded => wire::ErrorCode::InvalidArtifact,
        OrchestrationError::Storage(crate::StoreError::Busy) => wire::ErrorCode::TransientStorage,
        OrchestrationError::Storage(_) => wire::ErrorCode::Unavailable,
        OrchestrationError::VerificationUnavailable => wire::ErrorCode::Unavailable,
        OrchestrationError::Conflict => wire::ErrorCode::ConflictingReplay,
        _ => wire::ErrorCode::UnauthorizedScope,
    }
}
impl HttpsWorkerServer {
    /// Trusted local composition, never a network enrollment or trust-root setter.
    /// Existing production identity and verifier activation gates remain in force.
    pub fn with_coding(
        mut self: Arc<Self>,
        co: SyntheticCoordinator,
    ) -> Result<Arc<Self>, WorkerAuthError> {
        if co.coding_installation()? != self.security.controller().installation {
            return Err(WorkerAuthError::Denied);
        }
        Arc::get_mut(&mut self)
            .ok_or(WorkerAuthError::Conflict)?
            .coding = Some(Arc::new(co));
        Ok(self)
    }
    pub(super) async fn coding_fetch(
        &self,
        credential: &WorkerCredential,
        context: &WorkerRequestContext,
        op: &wire::Operation,
    ) -> Result<wire::Response, wire::ErrorCode> {
        let co = self.coding.as_ref().ok_or(wire::ErrorCode::Unavailable)?;
        let mut tx = self
            .security
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|_| wire::ErrorCode::TransientStorage)?;
        let now = self.security.now().map_err(auth_error)?;
        let cur = self
            .security
            .current_credential(&mut tx, credential, now)
            .await
            .map_err(auth_error)?;
        let e = &cur.enrollment;
        if !e.identity.permissions.contains(&WorkerPermission::Claim) {
            return Err(wire::ErrorCode::UnauthorizedScope);
        }
        self.security
            .transport
            .verify(context, &e.scope.binding, WorkerOperation::Claim, now)
            .map_err(auth_error)?;
        self.security
            .scope(
                &mut tx,
                &e.lease(),
                now,
                worker_control::CredentialScopeUse::Control,
            )
            .await
            .map_err(auth_error)?;
        self.security
            .identity
            .control_in(
                &mut tx,
                e,
                &context.request_id,
                now,
                worker_control::CredentialScopeUse::Control,
            )
            .await
            .map_err(auth_error)?;
        let digest = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(op).map_err(|_| wire::ErrorCode::InvalidProtocol)?)
        );
        let answer = match op {
            wire::Operation::CodingWork {} => wire::Response::CodingWork {
                version: 1,
                work: Box::new(
                    co.coding_work_in(&mut tx, &e.lease(), now)
                        .await
                        .map_err(error)?,
                ),
            },
            wire::Operation::CodingSource { work, file, offset } => wire::Response::CodingSource {
                version: 1,
                chunk: co
                    .coding_chunk_in(&mut tx, &e.lease(), work, *file, *offset, now)
                    .await
                    .map_err(error)?,
            },
            _ => return Err(wire::ErrorCode::InvalidProtocol),
        };
        let body = serde_json::to_vec(&answer).map_err(|_| wire::ErrorCode::InvalidProtocol)?;
        if body.len() > wire::MAX_RESPONSE_BODY {
            return Err(wire::ErrorCode::InvalidArtifact);
        }
        let response = format!("{:x}", Sha256::digest(&body));
        let old=sqlx::query("SELECT digest,response_sha256 FROM coding_deliveries WHERE project_id=? AND attempt_id=? AND request_id=?").bind(e.scope.binding.task.plan.project_id.as_str()).bind(e.scope.binding.attempt_id.as_str()).bind(context.request_id.as_str()).fetch_optional(&mut *tx).await.map_err(|_|wire::ErrorCode::Unavailable)?;
        if let Some(old) = old {
            if old.get::<String, _>(0) != digest || old.get::<String, _>(1) != response {
                return Err(wire::ErrorCode::ConflictingReplay);
            }
        } else {
            one(
                sqlx::query("INSERT INTO coding_deliveries VALUES(?,?,?,?,?,?)")
                    .bind(e.scope.binding.task.plan.project_id.as_str())
                    .bind(e.scope.binding.attempt_id.as_str())
                    .bind(context.request_id.as_str())
                    .bind(digest)
                    .bind(response)
                    .bind(now)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| wire::ErrorCode::Unavailable)?
                    .rows_affected(),
            )
            .map_err(auth_error)?;
        }
        co.coding_checkpoint("work-before")?;
        let end = self.security.now().map_err(auth_error)?;
        let fresh = self
            .security
            .current_credential(&mut tx, credential, end)
            .await
            .map_err(auth_error)?;
        if end < now || fresh.metadata != cur.metadata {
            return Err(wire::ErrorCode::Unauthenticated);
        }
        self.security
            .scope(
                &mut tx,
                &e.lease(),
                end,
                worker_control::CredentialScopeUse::Control,
            )
            .await
            .map_err(auth_error)?;
        self.security
            .identity
            .control_in(
                &mut tx,
                e,
                &context.request_id,
                end,
                worker_control::CredentialScopeUse::Control,
            )
            .await
            .map_err(auth_error)?;
        self.security
            .transport
            .verify(context, &e.scope.binding, WorkerOperation::Claim, end)
            .map_err(auth_error)?;
        co.coding_work_in(&mut tx, &e.lease(), end)
            .await
            .map_err(error)?;
        tx.commit()
            .await
            .map_err(|_| wire::ErrorCode::TransientStorage)?;
        co.coding_checkpoint("work-after")?;
        Ok(answer)
    }
    async fn process_verifier(
        &self,
        channel: &TlsChannel,
        r: Request<Body>,
    ) -> Result<Response<Body>, wire::ErrorCode> {
        self.transport
            .network_rate_limit(channel)
            .map_err(auth_error)?;
        if r.uri().query().is_some()
            || r.headers().contains_key(header::COOKIE)
            || r.headers().contains_key(header::AUTHORIZATION)
            || r.headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                != Some("application/json")
        {
            return Err(wire::ErrorCode::InvalidProtocol);
        }
        let raw = to_bytes(r.into_body(), wire::MAX_HTTP_BODY)
            .await
            .map_err(|_| wire::ErrorCode::InvalidProtocol)?;
        let request: VerifierRequest =
            serde_json::from_slice(&raw).map_err(|_| wire::ErrorCode::InvalidProtocol)?;
        let co = self.coding.as_ref().ok_or(wire::ErrorCode::Unavailable)?;
        let mut read = self
            .security
            .store
            .pool
            .begin()
            .await
            .map_err(|_| wire::ErrorCode::TransientStorage)?;
        let binding = co
            .verifier_request_binding_in(&mut read, &request, &channel.exporter)
            .await
            .map_err(error)?;
        read.commit()
            .await
            .map_err(|_| wire::ErrorCode::TransientStorage)?;
        let _guard = self
            .transport
            .reserve(
                channel,
                &binding,
                WorkerOperation::Claim,
                &request.request_id,
            )
            .map_err(auth_error)?;
        let mut tx = self
            .security
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|_| wire::ErrorCode::TransientStorage)?;
        let now = self.security.now().map_err(auth_error)?;
        let applied = co
            .verifier_request_in(
                &mut tx,
                &request,
                &channel.exporter,
                self.security.policy.as_ref(),
                now,
            )
            .await;
        let reply = match applied {
            Ok(r) => r,
            Err(e) => {
                tx.rollback()
                    .await
                    .map_err(|_| wire::ErrorCode::TransientStorage)?;
                if matches!(request.operation, VerifierOperation::Report { .. }) {
                    let l = LeaseRef {
                        task: binding.task.clone(),
                        attempt_id: binding.attempt_id.clone(),
                        epoch: binding.epoch,
                    };
                    co.coding_rejection(
                        &l,
                        "verification",
                        &e,
                        &SecurityClock(self.security.clock.as_ref()),
                    )
                    .await
                    .map_err(error)?;
                }
                return Err(error(e));
            }
        };
        let reporting = matches!(request.operation, VerifierOperation::Report { .. });
        if reporting {
            co.coding_checkpoint("report-before")?;
        }
        let end = self.security.now().map_err(auth_error)?;
        if end < now {
            return Err(wire::ErrorCode::Unauthenticated);
        }
        co.verifier_request_in(
            &mut tx,
            &request,
            &channel.exporter,
            self.security.policy.as_ref(),
            end,
        )
        .await
        .map_err(error)?;
        let body = serde_json::to_vec(&reply).map_err(|_| wire::ErrorCode::Unavailable)?;
        if body.len() > wire::MAX_RESPONSE_BODY {
            return Err(wire::ErrorCode::InvalidArtifact);
        }
        tx.commit()
            .await
            .map_err(|_| wire::ErrorCode::TransientStorage)?;
        if reporting {
            co.coding_checkpoint("report-after")?;
        }
        Ok(response(SecretBytes(Zeroizing::new(body)), StatusCode::OK))
    }
}
pub(super) async fn handle_verifier(
    State(s): State<Arc<HttpsWorkerServer>>,
    Extension(channel): Extension<TlsChannel>,
    r: Request<Body>,
) -> Response<Body> {
    match tokio::time::timeout(
        Duration::from_secs(s.transport.limits.request_seconds),
        s.process_verifier(&channel, r),
    )
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => error_response(e, status(e)),
        Err(_) => error_response(wire::ErrorCode::Unavailable, StatusCode::REQUEST_TIMEOUT),
    }
}
