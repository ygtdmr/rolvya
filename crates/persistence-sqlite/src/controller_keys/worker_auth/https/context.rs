//! In-process request correlation minted only by the accepted TLS handler. None
//! of these capabilities is deserializable or supplied by headers/worker JSON.
use super::{super::*, tls::SecretBytes, wire};
use std::{
    collections::BTreeMap,
    net::IpAddr,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Clone)]
pub(super) struct TlsChannel {
    pub(super) exporter: [u8; 32],
    pub(super) peer: IpAddr,
}
struct Active {
    binding: WorkerBinding,
    operation: WorkerOperation,
    request: RequestId,
    channel: [u8; 32],
    created: i64,
    expires: i64,
    attestation: Option<wire::Attestation>,
    response: Option<SecretBytes>,
    delivered: bool,
}
#[derive(Default)]
struct State {
    active: BTreeMap<String, Active>,
    windows: BTreeMap<IpAddr, (i64, u16)>,
}
/// Limits refer to network resource use, not the scheduler's worker capacity.
pub struct ResourceLimits {
    pub(super) connections: usize,
    pub(super) per_worker: usize,
    pub(super) requests_per_minute: u16,
    pub(super) handshake_seconds: u64,
    pub(super) connection_seconds: u64,
    pub(super) request_seconds: u64,
}
impl ResourceLimits {
    pub fn new(
        connections: usize,
        per_worker: usize,
        requests_per_minute: u16,
    ) -> Result<Self, wire::ErrorCode> {
        if !(1..=1024).contains(&connections)
            || per_worker == 0
            || per_worker > connections
            || !(1..=10000).contains(&requests_per_minute)
        {
            return Err(wire::ErrorCode::InvalidProtocol);
        }
        Ok(Self {
            connections,
            per_worker,
            requests_per_minute,
            handshake_seconds: 5,
            connection_seconds: 300,
            request_seconds: 10,
        })
    }
}
impl Default for ResourceLimits {
    fn default() -> Self {
        Self::new(64, 2, 240).expect("constant resource bounds")
    }
}
#[derive(Default)]
pub(super) struct Counters {
    pub(super) accepted: AtomicU64,
    pub(super) denied: AtomicU64,
    pub(super) active: AtomicU64,
    pub(super) peak: AtomicU64,
}
#[derive(Debug, Clone, Copy)]
pub struct Diagnostics {
    pub accepted: u64,
    pub denied: u64,
    pub active: u64,
    pub peak: u64,
}
pub(in crate::controller_keys::worker_auth) struct HttpTransport {
    state: Mutex<State>,
    pub(super) limits: ResourceLimits,
    pub(super) counters: Counters,
    clock: Arc<dyn Clock>,
    #[cfg(test)]
    pub(super) bootstrap_destinations:
        Mutex<BTreeMap<String, (PrepareBootstrap, Option<SecretBytes>)>>,
}
impl HttpTransport {
    pub(super) fn new(limits: ResourceLimits, clock: Arc<dyn Clock>) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State::default()),
            limits,
            counters: Counters::default(),
            clock,
            #[cfg(test)]
            bootstrap_destinations: Mutex::new(BTreeMap::new()),
        })
    }
    pub(super) fn reserve(
        self: &Arc<Self>,
        channel: &TlsChannel,
        binding: &WorkerBinding,
        op: WorkerOperation,
        id: &RequestId,
    ) -> Result<RequestGuard, WorkerAuthError> {
        let t = self.clock.now().map_err(|_| WorkerAuthError::Unavailable)?;
        let mut s = self
            .state
            .lock()
            .map_err(|_| WorkerAuthError::Unavailable)?;
        if s.active
            .values()
            .any(|a| a.binding == *binding && a.request == *id)
            || s.active.len() >= self.limits.connections
            || s.active
                .values()
                .filter(|v| {
                    v.binding.attempt_id == binding.attempt_id
                        && v.binding.task.plan.project_id == binding.task.plan.project_id
                })
                .count()
                >= self.limits.per_worker
        {
            return Err(WorkerAuthError::ResourceLimit);
        }
        let reference = EvidenceRef::new(format!("tls-{}", new_id()?))
            .map_err(|_| WorkerAuthError::Randomness)?;
        let context = WorkerRequestContext {
            request_id: id.clone(),
            transport_evidence: reference,
        };
        s.active.insert(
            context.transport_evidence.as_str().into(),
            Active {
                binding: binding.clone(),
                operation: op,
                request: id.clone(),
                channel: channel.exporter,
                created: t,
                attestation: None,
                expires: t.saturating_add(15),
                response: None,
                delivered: false,
            },
        );
        let n = self.counters.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.counters.peak.fetch_max(n, Ordering::SeqCst);
        self.counters.accepted.fetch_add(1, Ordering::Relaxed);
        Ok(RequestGuard {
            transport: self.clone(),
            context,
            channel: channel.exporter,
        })
    }
    pub(super) fn network_rate_limit(&self, channel: &TlsChannel) -> Result<(), WorkerAuthError> {
        let t = self.clock.now().map_err(|_| WorkerAuthError::Unavailable)?;
        if t < 0 {
            return Err(WorkerAuthError::Unavailable);
        }
        let mut s = self
            .state
            .lock()
            .map_err(|_| WorkerAuthError::Unavailable)?;
        if !s.windows.contains_key(&channel.peer) && s.windows.len() >= 1024 {
            s.windows
                .retain(|_, (start, _)| t >= start.saturating_add(60));
            if s.windows.len() >= 1024 {
                return Err(WorkerAuthError::ResourceLimit);
            }
        }
        let window = s.windows.entry(channel.peer).or_insert((t, 0));
        if t < window.0 {
            return Err(WorkerAuthError::Unavailable);
        }
        if t >= window.0.saturating_add(60) {
            *window = (t, 0);
        }
        if window.1 >= self.limits.requests_per_minute {
            return Err(WorkerAuthError::ResourceLimit);
        }
        window.1 += 1;
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn attestation(
        &self,
        c: &IdentityCheck<'_>,
    ) -> Result<wire::Attestation, WorkerAuthError> {
        let s = self
            .state
            .lock()
            .map_err(|_| WorkerAuthError::Unavailable)?;
        s.active
            .values()
            .find(|a| a.request == *c.request && a.binding == *c.binding)
            .and_then(|a| a.attestation.as_ref())
            .filter(|a| a.reference == *c.evidence)
            .cloned()
            .ok_or(WorkerAuthError::Denied)
    }
    /// Private snapshot of a live accepted request. A JSON channel identifier
    /// cannot construct this evidence. It remains correlation, never AWS identity.
    pub(in crate::controller_keys::worker_auth) fn accepted_channel(
        &self,
        c: &IdentityCheck<'_>,
        now: i64,
    ) -> Result<[u8; 32], WorkerAuthError> {
        self.accepted_proof(c, now).map(|v| v.0)
    }
    pub(in crate::controller_keys::worker_auth) fn accepted_proof(
        &self,
        c: &IdentityCheck<'_>,
        now: i64,
    ) -> Result<([u8; 32], wire::Attestation), WorkerAuthError> {
        let operation = match c.stage {
            VerificationStage::Bootstrap => WorkerOperation::Register,
            VerificationStage::Renew => WorkerOperation::RenewCredential,
            _ => return Err(WorkerAuthError::Denied),
        };
        let s = self
            .state
            .lock()
            .map_err(|_| WorkerAuthError::Unavailable)?;
        let mut matching = s.active.values().filter(|a| {
            a.request == *c.request
                && a.binding == *c.binding
                && a.operation == operation
                && a.attestation
                    .as_ref()
                    .is_some_and(|a| a.reference == *c.evidence)
                && now >= a.created
                && now < a.expires
        });
        let a = matching.next().ok_or(WorkerAuthError::Denied)?;
        if matching.next().is_some() {
            return Err(WorkerAuthError::Denied);
        }
        Ok((
            a.channel,
            a.attestation.clone().ok_or(WorkerAuthError::Denied)?,
        ))
    }
    pub(super) fn diagnostics(&self) -> Diagnostics {
        Diagnostics {
            accepted: self.counters.accepted.load(Ordering::Relaxed),
            denied: self.counters.denied.load(Ordering::Relaxed),
            active: self.counters.active.load(Ordering::Relaxed),
            peak: self.counters.peak.load(Ordering::Relaxed),
        }
    }
}
pub(super) struct RequestGuard {
    transport: Arc<HttpTransport>,
    pub(super) context: WorkerRequestContext,
    channel: [u8; 32],
}
impl RequestGuard {
    pub(super) fn attach_attestation(
        &self,
        attestation: wire::Attestation,
    ) -> Result<(), WorkerAuthError> {
        let mut s = self
            .transport
            .state
            .lock()
            .map_err(|_| WorkerAuthError::Unavailable)?;
        let a = s
            .active
            .get_mut(self.context.transport_evidence.as_str())
            .ok_or(WorkerAuthError::Denied)?;
        if a.attestation.is_some() {
            return Err(WorkerAuthError::Denied);
        }
        a.attestation = Some(attestation);
        Ok(())
    }

    pub(super) fn take_response(&self) -> Result<SecretBytes, WorkerAuthError> {
        let mut s = self
            .transport
            .state
            .lock()
            .map_err(|_| WorkerAuthError::Unavailable)?;
        let active = s
            .active
            .get_mut(self.context.transport_evidence.as_str())
            .ok_or(WorkerAuthError::Denied)?;
        if active.channel != self.channel {
            return Err(WorkerAuthError::Denied);
        }
        active.response.take().ok_or(WorkerAuthError::Denied)
    }
}
impl Drop for RequestGuard {
    fn drop(&mut self) {
        if let Ok(mut s) = self.transport.state.lock() {
            s.active.remove(self.context.transport_evidence.as_str());
        }
        self.transport
            .counters
            .active
            .fetch_sub(1, Ordering::SeqCst);
    }
}
impl WorkerTransport for HttpTransport {
    fn verify(
        &self,
        c: &WorkerRequestContext,
        b: &WorkerBinding,
        op: WorkerOperation,
        t: i64,
    ) -> Result<(), WorkerAuthError> {
        let s = self
            .state
            .lock()
            .map_err(|_| WorkerAuthError::Unavailable)?;
        let a = s
            .active
            .get(c.transport_evidence.as_str())
            .ok_or(WorkerAuthError::Denied)?;
        if a.binding != *b
            || a.operation != op
            || a.request != c.request_id
            || t < a.created
            || t >= a.expires
        {
            return Err(WorkerAuthError::Denied);
        }
        Ok(())
    }
    fn bootstrap(&self, r: &PrepareBootstrap, t: &BootstrapTicket) -> Result<(), WorkerAuthError> {
        #[cfg(test)]
        {
            let mut s = self
                .bootstrap_destinations
                .lock()
                .map_err(|_| WorkerAuthError::Unavailable)?;
            let (expected, body) = s
                .get_mut(r.request_id.as_str())
                .ok_or(WorkerAuthError::Unavailable)?;
            if expected.lease != r.lease || expected.evidence != r.evidence || body.is_some() {
                return Err(WorkerAuthError::Denied);
            }
            let raw = t.secret.as_ref().ok_or(WorkerAuthError::Unavailable)?;
            *body = Some(SecretBytes(Zeroizing::new(raw.0.0.as_bytes().to_vec())));
            Ok(())
        }
        #[cfg(not(test))]
        {
            let _ = (r, t);
            Err(WorkerAuthError::Unavailable)
        }
    }
    fn credential(
        &self,
        c: &WorkerRequestContext,
        ticket: &WorkerCredentialTicket,
    ) -> Result<(), WorkerAuthError> {
        let now = self.clock.now().map_err(|_| WorkerAuthError::Unavailable)?;
        self.verify(c, &ticket.metadata.binding, ticket.response_operation, now)?;
        if ticket.response_request != c.request_id {
            return Err(WorkerAuthError::Denied);
        }
        let mut s = self
            .state
            .lock()
            .map_err(|_| WorkerAuthError::Unavailable)?;
        let active = s
            .active
            .get_mut(c.transport_evidence.as_str())
            .ok_or(WorkerAuthError::Denied)?;
        if active.delivered {
            return Err(WorkerAuthError::Denied);
        }
        let mut body = Zeroizing::new(Vec::with_capacity(2048));
        body.extend_from_slice(b"{\"version\":1,\"request_id\":");
        serde_json::to_writer(&mut *body, &c.request_id).map_err(|_| WorkerAuthError::Integrity)?;
        body.extend_from_slice(b",\"metadata\":");
        serde_json::to_writer(&mut *body, &ticket.metadata)
            .map_err(|_| WorkerAuthError::Integrity)?;
        body.extend_from_slice(b",\"credential\":\"");
        body.extend_from_slice(ticket.secret.0.as_bytes());
        body.extend_from_slice(b"\"}");
        if body.len() > wire::MAX_RESPONSE_BODY {
            return Err(WorkerAuthError::ResourceLimit);
        }
        active.response = Some(SecretBytes(body));
        active.delivered = true;
        Ok(())
    }
}
