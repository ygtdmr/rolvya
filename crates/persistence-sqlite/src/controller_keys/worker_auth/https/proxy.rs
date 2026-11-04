//! Attempt-scoped model protocol, never a general HTTP forwarding API.
use super::{agent::StepReceipt, wire::ErrorCode};
use rolvya_domain::worker::Digest;
use serde::{Deserialize, Serialize};

pub(super) struct ProviderProxy {
    broker: crate::controller_keys::secret_broker::SecretBroker,
    slots: tokio::sync::Semaphore,
    #[cfg(test)]
    qualifications: std::sync::Mutex<
        std::collections::BTreeMap<
            String,
            std::sync::Arc<
                crate::controller_keys::worker_auth::aws_fargate::execution::FixtureModelAuthority,
            >,
        >,
    >,
    #[cfg(test)]
    routes: std::sync::Mutex<
        std::collections::BTreeMap<
            String,
            (
                crate::orchestration::coding::work::CodingWork,
                super::agent::fixture::ProviderEndpoint,
            ),
        >,
    >,
}
impl ProviderProxy {
    pub(super) async fn open(
        bootstrap: &crate::controller_keys::ControllerBootstrap,
    ) -> Result<Self, ErrorCode> {
        Ok(Self {
            slots: tokio::sync::Semaphore::new(8),
            #[cfg(test)]
            qualifications: Default::default(),
            broker: crate::controller_keys::secret_broker::SecretBroker::open(bootstrap)
                .await
                .map_err(|_| ErrorCode::Unavailable)?,
            #[cfg(test)]
            routes: Default::default(),
        })
    }
}
impl super::server::HttpsWorkerServer {
    /// Constructs the trusted consumer, not effective remote runtime or spending
    /// authority. Public production establishment remains unavailable.
    pub async fn with_provider_proxy(
        mut self: std::sync::Arc<Self>,
        bootstrap: &crate::controller_keys::ControllerBootstrap,
    ) -> Result<std::sync::Arc<Self>, ErrorCode> {
        let proxy = ProviderProxy::open(bootstrap).await?;
        std::sync::Arc::get_mut(&mut self)
            .ok_or(ErrorCode::Unavailable)?
            .provider_proxy = Some(proxy);
        Ok(self)
    }
}
#[cfg(test)]
pub(in crate::controller_keys) fn fixture_route(
    server: &super::server::HttpsWorkerServer,
    work: &crate::orchestration::coding::work::CodingWork,
    endpoint: super::agent::fixture::ProviderEndpoint,
) {
    assert!(
        server
            .provider_proxy
            .as_ref()
            .unwrap()
            .routes
            .lock()
            .unwrap()
            .len()
            < 16
    );
    server
        .provider_proxy
        .as_ref()
        .unwrap()
        .routes
        .lock()
        .unwrap()
        .insert(work.identity.as_str().into(), (work.clone(), endpoint));
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyRequest {
    pub version: u16,
    pub work: Digest,
    pub step: u16,
    /// Exact prior delivered provider bytes, base64 encoded. These are untrusted
    /// until matched to the controller's sealed per-call observations.
    pub previous: Vec<String>,
}
impl ProxyRequest {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.version != 1
            || !(1..=8).contains(&self.step)
            || self.previous.len() != usize::from(self.step - 1)
            || self
                .previous
                .iter()
                .try_fold(0usize, |n, p| n.checked_add(p.len()))
                .is_none_or(|n| n > 88 * 1024)
        {
            return Err(ErrorCode::InvalidProtocol);
        }
        Ok(())
    }
}
impl std::fmt::Debug for ProxyRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProxyRequest")
            .field("work", &self.work)
            .field("step", &self.step)
            .finish_non_exhaustive()
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyReply {
    pub receipt: StepReceipt,
    pub request_sha256: Digest,
    pub response: Result<String, super::agent::AgentError>,
}
impl std::fmt::Debug for ProxyReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProxyReply")
            .field("receipt", &self.receipt)
            .finish_non_exhaustive()
    }
}

use super::super::{
    WorkerCredential, WorkerCredentialMetadata, WorkerPermission, WorkerRequestContext,
    WorkerTransport,
};
use super::agent::{
    Adapter, AgentError, AgentOperation, ExecutionEvidence, Usage, api::ProxyConversation,
};
use crate::admission::LeaseRef;
use crate::controller_keys::{ControllerKey, secret_broker as broker};
use crate::orchestration::coding::work::CodingWork;
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use sqlx::SqliteConnection;
use std::time::Duration;
use zeroize::Zeroizing;

/// Only the accepted controller handler can mint this borrowed consumption
/// capability, after its exact call intent has committed. It is never a DTO.
pub(in crate::controller_keys) struct ProxyPermit<'a> {
    server: &'a super::server::HttpsWorkerServer,
    credential: &'a WorkerCredential,
    context: &'a WorkerRequestContext,
    metadata: WorkerCredentialMetadata,
    work: CodingWork,
    lease: LeaseRef,
    adapter: Adapter,
    step: u16,
    receipt: StepReceipt,
    created: i64,
    expires: i64,
}
impl ProxyPermit<'_> {
    pub(in crate::controller_keys) fn work(&self) -> &CodingWork {
        &self.work
    }
    pub(in crate::controller_keys) fn adapter(&self) -> Adapter {
        self.adapter
    }
    pub(in crate::controller_keys) fn operation(&self) -> &Digest {
        &self.receipt.operation
    }
    pub(in crate::controller_keys) fn secret_reference(
        &self,
    ) -> Result<(broker::RecordScope, broker::SecretVersion), ErrorCode> {
        let m = self
            .work
            .model_account
            .as_ref()
            .ok_or(ErrorCode::UnauthorizedScope)?;
        if m.mode != rolvya_domain::plan::AccessMode::ApiKey
            || !self.adapter.matches_binding(m)
            || m.credential_version <= 0
        {
            return Err(ErrorCode::UnauthorizedScope);
        }
        Ok((
            broker::RecordScope {
                connection: broker::ConnectionScope {
                    project: self.lease.task.plan.project_id.clone(),
                    connection: broker::ConnectionId::new(m.credential_connection.clone())
                        .map_err(|_| ErrorCode::UnauthorizedScope)?,
                    account: broker::AccountId::new(m.account.clone())
                        .map_err(|_| ErrorCode::UnauthorizedScope)?,
                    provider: broker::ProviderId::new(m.provider.clone())
                        .map_err(|_| ErrorCode::UnauthorizedScope)?,
                    mode: broker::ProviderMode::ApiKey,
                },
                record: broker::RecordId::new(m.credential_record.clone())
                    .map_err(|_| ErrorCode::UnauthorizedScope)?,
                purpose: broker::SecretPurpose::new(m.credential_purpose.clone())
                    .map_err(|_| ErrorCode::UnauthorizedScope)?,
            },
            broker::SecretVersion::new(m.credential_version as u64)
                .map_err(|_| ErrorCode::UnauthorizedScope)?,
        ))
    }
    pub(in crate::controller_keys) async fn current_in(
        &self,
        c: &mut SqliteConnection,
        key: &ControllerKey,
        pending: bool,
    ) -> Result<(), ErrorCode> {
        let now = self
            .server
            .security
            .now()
            .map_err(|_| ErrorCode::Unavailable)?;
        if now < self.created || now >= self.expires {
            return Err(ErrorCode::ExpiredCredential);
        }
        let expected = &self.server.security.key.metadata;
        if key.metadata.installation != expected.installation
            || key.metadata.key != expected.key
            || key.metadata.key_version != expected.key_version
        {
            return Err(ErrorCode::UnauthorizedScope);
        }
        let cur = self
            .server
            .proxy_authority(c, self.credential, self.context, now)
            .await?;
        if cur.metadata != self.metadata {
            return Err(ErrorCode::Unauthenticated);
        }
        let co = self.server.coding.as_ref().ok_or(ErrorCode::Unavailable)?;
        let w = co
            .coding_agent_work_in(c, &self.lease, now)
            .await
            .map_err(|_| ErrorCode::UnauthorizedScope)?;
        if w != self.work {
            return Err(ErrorCode::UnauthorizedScope);
        }
        #[cfg(test)]
        let fixture = self.server.provider_proxy.as_ref().and_then(|p| {
            p.qualifications
                .lock()
                .unwrap()
                .get(w.identity.as_str())
                .cloned()
        });
        let legacy_fixture =
            {
                #[cfg(test)]
                {
                    fixture.is_none()
                        && self.server.provider_proxy.as_ref().is_some_and(|p| {
                            p.routes.lock().unwrap().contains_key(w.identity.as_str())
                        })
                }
                #[cfg(not(test))]
                {
                    false
                }
            };
        if !legacy_fixture {
            crate::controller_keys::worker_auth::aws_fargate::execution::require_model_operation_in(c,self.server.security.store.clone(),key,&w,&self.receipt.operation,now,#[cfg(test)] fixture.as_deref()).await.map_err(|_|ErrorCode::Unavailable)?;
        }
        let expiry = co
            .coding_proxy_current_in(
                c,
                &self.lease,
                &w.identity,
                self.step,
                &self.receipt.operation,
                now,
                pending,
            )
            .await
            .map_err(|_| ErrorCode::UnauthorizedScope)?;
        if expiry != self.expires {
            return Err(ErrorCode::UnauthorizedScope);
        }
        let (scope, version) = self.secret_reference()?;
        crate::controller_keys::provider_accounts::gate::broker_for_attempt(
            c,
            self.server.security.policy.as_ref(),
            &self.lease,
            &scope,
            version,
            now,
        )
        .await
        .map_err(|_| ErrorCode::ProviderEntitlementUnavailable)?;
        Ok(())
    }
}
fn adapter(w: &CodingWork) -> Result<Adapter, ErrorCode> {
    let m = w
        .model_account
        .as_ref()
        .ok_or(ErrorCode::UnauthorizedScope)?;
    [Adapter::OpenAiApi, Adapter::AnthropicApi]
        .into_iter()
        .find(|a| a.matches_binding(m))
        .ok_or(ErrorCode::UnauthorizedScope)
}
fn history(r: &ProxyRequest) -> Result<Vec<Zeroizing<Vec<u8>>>, ErrorCode> {
    r.validate()?;
    let mut bytes = 0usize;
    let mut out = Vec::new();
    for p in &r.previous {
        let raw = Zeroizing::new(B64.decode(p).map_err(|_| ErrorCode::InvalidProtocol)?);
        bytes = bytes
            .checked_add(raw.len())
            .ok_or(ErrorCode::InvalidArtifact)?;
        if raw.len() > 65536 || bytes > 65536 || B64.encode(&*raw) != *p {
            return Err(ErrorCode::InvalidArtifact);
        }
        out.push(raw);
    }
    Ok(out)
}
impl ProviderProxy {
    fn runtime(&self, work: &CodingWork) -> Result<(), ErrorCode> {
        #[cfg(not(test))]
        {
            let _ = work;
            Err(ErrorCode::Unavailable)
        }
        #[cfg(test)]
        {
            let map = self.routes.lock().map_err(|_| ErrorCode::Unavailable)?;
            let (expected, _) = map
                .get(work.identity.as_str())
                .ok_or(ErrorCode::Unavailable)?;
            if expected != work {
                return Err(ErrorCode::UnauthorizedScope);
            }
            Ok(())
        }
    }
}
impl super::server::HttpsWorkerServer {
    async fn proxy_authority(
        &self,
        c: &mut SqliteConnection,
        credential: &WorkerCredential,
        context: &WorkerRequestContext,
        now: i64,
    ) -> Result<super::super::credentials::CurrentCredential, ErrorCode> {
        let cur = self
            .security
            .current_credential(c, credential, now)
            .await
            .map_err(|_| ErrorCode::Unauthenticated)?;
        if !cur
            .enrollment
            .identity
            .permissions
            .contains(&WorkerPermission::ModelProxy)
        {
            return Err(ErrorCode::UnauthorizedScope);
        }
        let e = &cur.enrollment;
        self.transport
            .verify(
                context,
                &e.scope.binding,
                crate::worker_simulation::WorkerOperation::UseCredential,
                now,
            )
            .map_err(|_| ErrorCode::Unauthenticated)?;
        self.security
            .scope(
                c,
                &e.lease(),
                now,
                crate::worker_control::CredentialScopeUse::Control,
            )
            .await
            .map_err(|_| ErrorCode::UnauthorizedScope)?;
        self.security
            .identity
            .control_in(
                c,
                e,
                &context.request_id,
                now,
                crate::worker_control::CredentialScopeUse::Control,
            )
            .await
            .map_err(|_| ErrorCode::UnauthorizedScope)?;
        crate::worker_control::coding_provider_ready(c, &e.lease(), now)
            .await
            .map_err(|_| ErrorCode::UnauthorizedScope)?;
        Ok(cur)
    }
    pub(super) async fn model_proxy(
        &self,
        credential: &WorkerCredential,
        context: &WorkerRequestContext,
        r: &ProxyRequest,
    ) -> Result<ProxyReply, ErrorCode> {
        let mut read = self
            .security
            .store
            .pool
            .begin()
            .await
            .map_err(|_| ErrorCode::TransientStorage)?;
        let now = self.security.now().map_err(|_| ErrorCode::Unavailable)?;
        let cur = self
            .proxy_authority(&mut read, credential, context, now)
            .await?;
        let service = self.provider_proxy.as_ref().ok_or(ErrorCode::Unavailable)?;
        let _slot = service
            .slots
            .try_acquire()
            .map_err(|_| ErrorCode::Unavailable)?;
        let co = self.coding.as_ref().ok_or(ErrorCode::Unavailable)?;
        let lease = cur.enrollment.lease();
        let work = co
            .coding_agent_work_in(&mut read, &lease, now)
            .await
            .map_err(|_| ErrorCode::UnauthorizedScope)?;
        if work.identity != r.work {
            return Err(ErrorCode::UnauthorizedScope);
        }
        service.runtime(&work)?;
        let adapter = adapter(&work)?;
        let (_, source, _) = co
            .coding_source_current_in(&mut read, &lease, now)
            .await
            .map_err(|_| ErrorCode::UnauthorizedScope)?;
        let mut conversation = ProxyConversation::new(&work, source, adapter)
            .map_err(|_| ErrorCode::InvalidArtifact)?;
        let prior = history(r)?;
        for (index, raw) in prior.iter().enumerate() {
            co.coding_proxy_previous_in(
                &mut read,
                &lease,
                &work.identity,
                index as u16 + 1,
                &super::agent::digest(raw),
            )
            .await
            .map_err(|_| ErrorCode::ConflictingReplay)?;
            conversation
                .accept(raw)
                .map_err(|_| ErrorCode::InvalidArtifact)?;
        }
        let body = conversation
            .request()
            .map_err(|_| ErrorCode::InvalidArtifact)?;
        let sha = super::agent::digest(
            &serde_json::to_vec(&body).map_err(|_| ErrorCode::InvalidArtifact)?,
        );
        read.commit()
            .await
            .map_err(|_| ErrorCode::TransientStorage)?;
        let mut tx = self
            .security
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|_| ErrorCode::TransientStorage)?;
        let now = self.security.now().map_err(|_| ErrorCode::Unavailable)?;
        let fresh = self
            .proxy_authority(&mut tx, credential, context, now)
            .await?;
        if fresh.metadata != cur.metadata {
            return Err(ErrorCode::Unauthenticated);
        }
        for (index, raw) in prior.iter().enumerate() {
            co.coding_proxy_previous_in(
                &mut tx,
                &lease,
                &work.identity,
                index as u16 + 1,
                &super::agent::digest(raw),
            )
            .await
            .map_err(|_| ErrorCode::ConflictingReplay)?;
        }
        let expires = now.saturating_add(8).min(work.expires_at);
        let io_deadline =
            tokio::time::Instant::now() + Duration::from_secs((expires - now).max(0) as u64);
        let op = AgentOperation::Start {
            work: work.identity.clone(),
            step: r.step,
            adapter,
            request_sha256: sha.clone(),
        };
        let receipt = co
            .coding_proxy_in(&mut tx, &lease, &op, expires, now)
            .await
            .map_err(|_| ErrorCode::Unavailable)?;
        co.coding_checkpoint("proxy-intent-before")
            .map_err(|_| ErrorCode::Unavailable)?;
        self.proxy_authority(
            &mut tx,
            credential,
            context,
            self.security.now().map_err(|_| ErrorCode::Unavailable)?,
        )
        .await?;
        tx.commit().await.map_err(|_| ErrorCode::TransientStorage)?;
        co.coding_checkpoint("proxy-intent-after")
            .map_err(|_| ErrorCode::Unavailable)?;
        if !receipt.execute {
            return Ok(ProxyReply {
                receipt,
                request_sha256: sha,
                response: Err(AgentError::Replay),
            });
        }
        let permit = ProxyPermit {
            server: self,
            credential,
            context,
            metadata: cur.metadata,
            work,
            lease,
            adapter,
            step: r.step,
            receipt: receipt.clone(),
            created: now,
            expires,
        };
        let mut tx = self
            .security
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|_| ErrorCode::TransientStorage)?;
        let transport = service
            .broker
            .consume_model_proxy(&mut tx, &permit)
            .await
            .map_err(|_| ErrorCode::UnauthorizedScope)?;
        permit.current_in(&mut tx, &self.security.key, true).await?;
        co.coding_checkpoint("proxy-secret-before")
            .map_err(|_| ErrorCode::Unavailable)?;
        tx.commit().await.map_err(|_| ErrorCode::TransientStorage)?;
        co.coding_checkpoint("proxy-secret-after")
            .map_err(|_| ErrorCode::Unavailable)?;
        #[cfg(test)]
        let transport = {
            let mut transport = transport;
            let routes = service.routes.lock().map_err(|_| ErrorCode::Unavailable)?;
            routes
                .get(permit.work.identity.as_str())
                .ok_or(ErrorCode::Unavailable)?
                .1
                .route(&mut transport);
            transport
        };
        transport
            .matches(adapter, permit.work())
            .map_err(|_| ErrorCode::UnauthorizedScope)?;
        co.coding_checkpoint("proxy-before-io")
            .map_err(|_| ErrorCode::Unavailable)?;
        // Recheck after preparation, including any pause before I/O. This cannot
        // undo revocation racing an already-started upstream call; such exposure
        // remains held and its response is withheld on the final recheck.
        let mut check = self
            .security
            .store
            .pool
            .begin()
            .await
            .map_err(|_| ErrorCode::TransientStorage)?;
        permit
            .current_in(&mut check, &self.security.key, true)
            .await?;
        check
            .commit()
            .await
            .map_err(|_| ErrorCode::TransientStorage)?;
        if tokio::time::Instant::now() >= io_deadline {
            return Err(ErrorCode::ExpiredCredential);
        }
        // No database transaction is alive during provider I/O. An interruption
        // leaves the already committed intent unresolved; it never grants retry.
        let raw = tokio::time::timeout_at(io_deadline, transport.request(adapter, &body, 65536))
            .await
            .unwrap_or(Err(AgentError::Timeout));
        let response = raw.and_then(|bytes| {
            let bytes = Zeroizing::new(bytes);
            transport.model_response(&bytes)
        });
        let parsed = response
            .as_ref()
            .map_err(|e| *e)
            .and_then(|raw| conversation.accept(raw));
        let evidence = ExecutionEvidence {
            adapter,
            response_sha256: response.as_ref().ok().map(|v| super::agent::digest(v)),
            usage: parsed.as_ref().cloned().unwrap_or(Usage {
                input_tokens: None,
                output_tokens: None,
            }),
            completed: parsed.is_ok(),
        };
        let mut tx = self
            .security
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|_| ErrorCode::TransientStorage)?;
        permit.current_in(&mut tx, &self.security.key, true).await?;
        let op = AgentOperation::Finish {
            work: permit.work.identity.clone(),
            step: r.step,
            evidence,
        };
        co.coding_proxy_in(
            &mut tx,
            &permit.lease,
            &op,
            expires,
            self.security.now().map_err(|_| ErrorCode::Unavailable)?,
        )
        .await
        .map_err(|_| ErrorCode::Unavailable)?;
        co.coding_checkpoint("proxy-observation-before")
            .map_err(|_| ErrorCode::Unavailable)?;
        permit
            .current_in(&mut tx, &self.security.key, false)
            .await?;
        tx.commit().await.map_err(|_| ErrorCode::TransientStorage)?;
        co.coding_checkpoint("proxy-observation-after")
            .map_err(|_| ErrorCode::Unavailable)?;
        let response = match parsed {
            Ok(_) => Ok(B64.encode(&*response.map_err(|_| ErrorCode::InvalidArtifact)?)),
            Err(e) => Err(e),
        };
        Ok(ProxyReply {
            receipt,
            request_sha256: sha,
            response,
        })
    }
}

#[cfg(test)]
pub(in crate::controller_keys) async fn fixture_require_qualification(
    server: &super::server::HttpsWorkerServer,
    b: &crate::controller_keys::ControllerBootstrap,
    clock: std::sync::Arc<dyn crate::admission::Clock>,
    w: &CodingWork,
    execution: &crate::controller_keys::worker_auth::aws_fargate::execution::FargateExecution,
) -> std::sync::Arc<
    crate::controller_keys::worker_auth::aws_fargate::execution::FixtureModelAuthority,
> {
    let authority = std::sync::Arc::new(
        crate::controller_keys::worker_auth::aws_fargate::execution::FixtureModelAuthority::new(
            b, clock, w, execution,
        )
        .await,
    );
    server
        .provider_proxy
        .as_ref()
        .unwrap()
        .qualifications
        .lock()
        .unwrap()
        .insert(w.identity.as_str().into(), authority.clone());
    authority
}
