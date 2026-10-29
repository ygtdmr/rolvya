//! Private bridge for S2-05 tests: actual S2-04 bootstrap and durable credentials.
use super::*;
pub(in crate::controller_keys) struct FixtureWorker {
    security: WorkerSecurity,
    credential: WorkerCredentialTicket,
    transport: Arc<SyntheticTransport>,
}
impl FixtureWorker {
    pub(in crate::controller_keys) async fn bootstrap(
        b: &ControllerBootstrap,
        policy: Arc<dyn SchedulerPolicy>,
        clock: Arc<DeterministicClock>,
        lease: &LeaseRef,
        capabilities: DeclaredCapabilities,
    ) -> Self {
        let mut security = WorkerSecurity::open(b).await.unwrap();
        security.policy = policy;
        security.clock = clock;
        let binding = DurableWorkerController::new(b.store.clone())
            .binding(lease)
            .await
            .unwrap();
        let digest: String = sqlx::query_scalar(
            "SELECT digest FROM dispatch_operations WHERE project_id=? AND attempt_id=?",
        )
        .bind(lease.task.plan.project_id.as_str())
        .bind(lease.attempt_id.as_str())
        .fetch_one(&b.store.pool)
        .await
        .unwrap();
        let identity = Arc::new(SyntheticIdentity::default());
        let transport = Arc::new(SyntheticTransport::default());
        *identity.1.lock().unwrap() = Some(VerifiedWorkerIdentity {
            subject: synthetic::e(&format!("test-subject-{}", lease.attempt_id.as_str())),
            execution: synthetic::e(&format!("test-execution-{}", lease.attempt_id.as_str())),
            capabilities,
            permissions: vec![
                WorkerPermission::Register,
                WorkerPermission::Claim,
                WorkerPermission::AcceptLease,
                WorkerPermission::Heartbeat,
                WorkerPermission::Progress,
                WorkerPermission::SubmitResult,
                WorkerPermission::StopQuery,
                WorkerPermission::StopAck,
                WorkerPermission::Renew,
            ],
            valid_until: synthetic::T + 3600,
        });
        security.identity = identity.clone();
        security.transport = transport.clone();
        let request = PrepareBootstrap {
            lease: lease.clone(),
            request_id: RequestId::new(format!("p-{}", lease.attempt_id.as_str())).unwrap(),
            evidence: synthetic::e("test-prepare-evidence"),
        };
        identity.0.lock().unwrap().insert(
            request.request_id.as_str().into(),
            Allowed {
                controller: security.controller(),
                stage: VerificationStage::Prepare,
                request: request.request_id.clone(),
                evidence: request.evidence.clone(),
                binding: binding.clone(),
                digest: digest.clone(),
                challenge: None,
                credential: None,
                account: synthetic::e("synthetic-account"),
            },
        );
        let ticket = security.prepare_bootstrap(&request).await.unwrap();
        let c = WorkerRequestContext {
            request_id: RequestId::new(format!("b-{}", lease.attempt_id.as_str())).unwrap(),
            transport_evidence: transport_evidence(&format!("b-{}", lease.attempt_id.as_str())),
        };
        transport.0.lock().unwrap().insert(
            c.request_id.as_str().into(),
            (binding.clone(), WorkerOperation::Register),
        );
        let evidence = synthetic::e("test-independent-claim-evidence");
        identity.0.lock().unwrap().insert(
            c.request_id.as_str().into(),
            Allowed {
                controller: security.controller(),
                stage: VerificationStage::Bootstrap,
                request: c.request_id.clone(),
                evidence: evidence.clone(),
                binding,
                digest,
                challenge: Some(ticket.id.clone()),
                credential: None,
                account: synthetic::e("synthetic-account"),
            },
        );
        let credential = security
            .claim_bootstrap(BootstrapClaim {
                id: &ticket.id,
                secret: ticket.secret.as_ref().unwrap(),
                evidence: &evidence,
                context: &c,
            })
            .await
            .unwrap();
        Self {
            security,
            credential,
            transport,
        }
    }
    pub(in crate::controller_keys) async fn receive(
        &self,
        bytes: &[u8],
    ) -> Result<ControllerMessage, WorkerAuthError> {
        let message: WorkerMessage = serde_json::from_slice(bytes).unwrap();
        let op = match message.body {
            WorkerBody::Register { .. } => WorkerOperation::Register,
            WorkerBody::Claim {} => WorkerOperation::Claim,
            WorkerBody::AcceptLease { .. } => WorkerOperation::AcceptLease,
            WorkerBody::Heartbeat {} => WorkerOperation::Heartbeat,
            WorkerBody::Progress { .. } => WorkerOperation::Progress,
            WorkerBody::SubmitResult { .. } => WorkerOperation::SubmitResult,
            WorkerBody::GetStopInstruction {} => WorkerOperation::StopQuery,
            WorkerBody::StopAcknowledged { .. } => WorkerOperation::StopAck,
        };
        let id = message.message_id.as_str();
        let c = WorkerRequestContext {
            request_id: RequestId::new(id).unwrap(),
            transport_evidence: transport_evidence(id),
        };
        self.transport
            .0
            .lock()
            .unwrap()
            .insert(id.into(), (self.credential.metadata.binding.clone(), op));
        self.security
            .receive(&self.credential.secret, bytes, &c)
            .await
    }
}
