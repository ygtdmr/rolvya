//! Reusable remote coding session. It performs no model invocation or OS commands.
use super::client::{ClientError, WorkerHttpClient};
use crate::controller_keys::github_app::PinnedSource;
use crate::orchestration::coding::{work::CodingWork, *};
use rolvya_domain::{ids::MessageId, worker::*};

pub struct CodingWorker {
    client: WorkerHttpClient,
    work: CodingWork,
    source: PinnedSource,
    sequence: u64,
    stopped: bool,
}
impl CodingWorker {
    pub async fn begin(
        client: WorkerHttpClient,
        capabilities: DeclaredCapabilities,
        now: i64,
    ) -> Result<Self, ClientError> {
        let work = client
            .coding_work(
                crate::RequestId::new("coding-work-v1").map_err(|_| ClientError::Protocol)?,
                now,
            )
            .await?;
        let source = client.coding_source(&work).await?;
        let mut s = Self {
            client,
            work,
            source,
            sequence: 1,
            stopped: false,
        };
        s.send(WorkerBody::Register { capabilities }, None).await?;
        let claim = s.send(WorkerBody::Claim {}, None).await?;
        let offer = match claim.body {
            ControllerBody::LeaseAssignment { offer } => offer,
            _ => return Err(ClientError::Protocol),
        };
        if offer.profile != s.work.profile
            || offer.financial_event != s.work.financial_event
            || offer.lease_until < now
        {
            return Err(ClientError::Protocol);
        }
        s.send(
            WorkerBody::AcceptLease {
                offer_id: offer.offer_id,
            },
            None,
        )
        .await?;
        Ok(s)
    }
    pub fn work(&self) -> &CodingWork {
        &self.work
    }
    pub fn source(&self) -> &PinnedSource {
        &self.source
    }
    pub fn heartbeat_schedule(&self) -> Result<tokio::time::Interval, ClientError> {
        self.client.heartbeat_schedule()
    }
    async fn send(
        &mut self,
        body: WorkerBody,
        artifact: Option<&[u8]>,
    ) -> Result<ControllerMessage, ClientError> {
        if self.stopped {
            return Err(ClientError::NotAuthenticated);
        }
        let message = WorkerMessage {
            protocol_version: PROTOCOL_VERSION,
            message_id: MessageId::new(format!(
                "coding-{}-{}",
                &self.work.identity.as_str()[..24],
                self.sequence
            ))
            .map_err(|_| ClientError::Protocol)?,
            binding: self.work.binding.clone(),
            sequence: self.sequence,
            body,
        };
        match self.client.message(message, artifact).await {
            Ok(r) => {
                self.sequence = self.sequence.checked_add(1).ok_or(ClientError::Protocol)?;
                Ok(r)
            }
            Err(e) => {
                self.stopped = true;
                Err(e)
            }
        }
    }
    pub async fn heartbeat(&mut self) -> Result<(), ClientError> {
        let response = self.send(WorkerBody::GetStopInstruction {}, None).await?;
        if let ControllerBody::Stop { instruction } = response.body {
            let _ = self
                .send(
                    WorkerBody::StopAcknowledged {
                        instruction_id: instruction.instruction_id,
                    },
                    None,
                )
                .await;
            self.stopped = true;
            return Err(ClientError::Rejected(
                super::wire::ErrorCode::RevokedCredential,
            ));
        }
        self.send(WorkerBody::Heartbeat {}, None).await?;
        Ok(())
    }
    pub async fn submit(
        &mut self,
        changes: Vec<FileChange>,
        verification_execution: String,
    ) -> Result<CodingResult, ClientError> {
        self.heartbeat().await?;
        let r = self
            .work
            .result(
                &self.source,
                changes,
                format!("remote-worker-{}", self.work.binding.worker_id.as_str()),
                verification_execution,
            )
            .map_err(|_| ClientError::Protocol)?;
        let raw = serde_json::to_vec(&r).map_err(|_| ClientError::Protocol)?;
        let hash = Digest::new(format!(
            "{:x}",
            <sha2::Sha256 as sha2::Digest>::digest(&raw)
        ))
        .map_err(|_| ClientError::Protocol)?;
        self.send(
            WorkerBody::SubmitResult {
                outcome: ExecutionOutcome::Succeeded {
                    manifest_sha256: hash,
                },
            },
            Some(&raw),
        )
        .await?;
        self.stopped = true;
        Ok(r)
    }
}
