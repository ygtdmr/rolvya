use super::*;
use crate::admission::{AdmissionError, Clock};
use rolvya_domain::{ids::*, worker::*};
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicI64, Ordering},
};

/// Deterministic server clock for simulation only. No real sleep or background task.
pub struct DeterministicClock(AtomicI64);
impl DeterministicClock {
    pub fn new(at: i64) -> Result<Self, WorkerError> {
        if at < 0 {
            return Err(WorkerError::InvalidTime);
        }
        Ok(Self(AtomicI64::new(at)))
    }
    pub fn advance_to(&self, at: i64) -> Result<(), WorkerError> {
        let old = self.0.load(Ordering::SeqCst);
        if at < old
            || self
                .0
                .compare_exchange(old, at, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
        {
            return Err(WorkerError::InvalidTime);
        }
        Ok(())
    }
}
impl Clock for DeterministicClock {
    fn now(&self) -> Result<i64, AdmissionError> {
        Ok(self.0.load(Ordering::SeqCst))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FakeBehavior {
    Success,
    InvalidArtifact,
    ForgedArtifactDigest,
    MissingInputs,
    ControlledFailure,
    TransientFailure,
    PermanentFailure,
    Disconnect,
    Delayed { seconds: u32 },
    Duplicate,
    OutOfOrder,
    Malformed,
    ExpiredLease,
    StaleEpoch,
    Cancelled,
    MissingCapabilityEvidence,
    InvalidCapabilityEvidence,
    CrossProject,
    CrossAttempt,
    WrongContext,
    Impersonate,
    ResultAfterCancellation,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimulatedInput {
    Message(Vec<u8>),
    Disconnect(WorkerBinding),
}
impl SimulatedInput {
    fn validate(&self) -> Result<(), WorkerError> {
        match self {
            Self::Message(bytes) if bytes.len() > MAX_MESSAGE_BYTES => {
                Err(WorkerError::ResourceLimit)
            }
            Self::Disconnect(binding) => {
                binding.validate()?;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptEvent {
    pub at: i64,
    pub input: SimulatedInput,
}
/// Bounded synthetic bytes, not a real user artifact or acceptance proof.
#[derive(Clone, PartialEq, Eq)]
pub struct SyntheticArtifact {
    pub bytes: Vec<u8>,
    pub sha256: Digest,
}
impl std::fmt::Debug for SyntheticArtifact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntheticArtifact")
            .field("bytes_length", &self.bytes.len())
            .field("sha256", &self.sha256)
            .finish()
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeScript {
    pub events: Vec<ScriptEvent>,
    pub artifact: Option<SyntheticArtifact>,
}
/// Worker side receives only compact assignment binding and declared capabilities.
/// It never receives PlanStore, an Owner policy, SQL, financial or Git authority.
pub struct FakeWorker {
    binding: WorkerBinding,
    capabilities: DeclaredCapabilities,
    seed: u64,
    inputs: Vec<SyntheticInput>,
}
impl FakeWorker {
    pub fn new(
        binding: WorkerBinding,
        capabilities: DeclaredCapabilities,
        seed: u64,
    ) -> Result<Self, WorkerError> {
        binding.validate()?;
        capabilities.validate()?;
        Ok(Self {
            binding,
            capabilities,
            seed,
            inputs: Vec::new(),
        })
    }
    /// Untrusted upstream references for synthetic integration output, not grants.
    pub fn with_inputs(mut self, inputs: Vec<SyntheticInput>) -> Result<Self, WorkerError> {
        if inputs.len() > 64 || inputs.iter().any(|i| i.task.plan != self.binding.task.plan) {
            return Err(WorkerError::ResourceLimit);
        }
        let unique: std::collections::BTreeSet<_> = inputs.iter().map(|i| &i.task).collect();
        if unique.len() != inputs.len() {
            return Err(WorkerError::ScopeMismatch);
        }
        self.inputs = inputs;
        Ok(self)
    }
    fn id(&self, sequence: u64) -> Result<MessageId, WorkerError> {
        MessageId::new(format!("m-{:x}-{sequence}", self.seed))
            .map_err(|_| ProtocolError::InvalidMessage.into())
    }
    fn message(&self, sequence: u64, body: WorkerBody) -> Result<WorkerMessage, WorkerError> {
        Ok(WorkerMessage {
            protocol_version: PROTOCOL_VERSION,
            message_id: self.id(sequence)?,
            binding: self.binding.clone(),
            sequence,
            body,
        })
    }
    /// At most nine script events. The seed controls stable IDs, output and a small
    /// deterministic scheduling offset. All scheduling uses explicit server time.
    pub fn script(&self, behavior: FakeBehavior, start: i64) -> Result<FakeScript, WorkerError> {
        if start < 0 {
            return Err(WorkerError::InvalidTime);
        }
        let mut caps = self.capabilities.clone();
        if behavior == FakeBehavior::MissingCapabilityEvidence {
            caps.evidence = None
        }
        if behavior == FakeBehavior::InvalidCapabilityEvidence {
            caps.evidence = Some(
                CapabilityRef::new("unverified-declaration")
                    .map_err(|_| ProtocolError::InvalidCapabilities)?,
            )
        }
        let mut content = serde_json::json!({"format":"rolvya-synthetic-artifact-v1","binding":self.binding,"seed":self.seed});
        if !self.inputs.is_empty() {
            content["inputs"] =
                serde_json::to_value(&self.inputs).map_err(|_| ProtocolError::Malformed)?;
            let encoded = serde_json::to_vec(&self.inputs).map_err(|_| ProtocolError::Malformed)?;
            content["input_digest"] = serde_json::json!(format!("{:x}", Sha256::digest(encoded)));
        }
        if behavior == FakeBehavior::InvalidArtifact {
            content["seed"] = serde_json::json!(self.seed.wrapping_add(1));
        }
        if behavior == FakeBehavior::MissingInputs
            && let Some(object) = content.as_object_mut()
        {
            object.remove("inputs");
        }
        let bytes = serde_json::to_vec(&content).map_err(|_| ProtocolError::Malformed)?;
        if bytes.len() > MAX_MESSAGE_BYTES {
            return Err(WorkerError::ResourceLimit);
        }
        let mut artifact = SyntheticArtifact {
            sha256: Digest::new(format!("{:x}", Sha256::digest(&bytes)))?,
            bytes,
        };
        if behavior == FakeBehavior::ForgedArtifactDigest {
            artifact.sha256 = Digest::new("f".repeat(64))?;
        }
        let failure = match behavior {
            FakeBehavior::ControlledFailure => Some(FailureClass::Controlled),
            FakeBehavior::TransientFailure => Some(FailureClass::Transient),
            FakeBehavior::PermanentFailure => Some(FailureClass::Permanent),
            FakeBehavior::Cancelled | FakeBehavior::ResultAfterCancellation => {
                Some(FailureClass::Cancelled)
            }
            _ => None,
        };
        let outcome = if let Some(class) = failure {
            ExecutionOutcome::Failed { class }
        } else {
            ExecutionOutcome::Succeeded {
                manifest_sha256: artifact.sha256.clone(),
            }
        };
        let mut frames = vec![
            (
                0,
                self.message(1, WorkerBody::Register { capabilities: caps })?,
            ),
            (1, self.message(2, WorkerBody::Claim {})?),
            (
                2,
                self.message(
                    3,
                    WorkerBody::AcceptLease {
                        offer_id: self.id(2)?,
                    },
                )?,
            ),
            (30, self.message(4, WorkerBody::Heartbeat {})?),
            (
                40 + (self.seed % 3) as i64,
                self.message(
                    5,
                    WorkerBody::Progress {
                        completed: 1,
                        total: 2,
                        code: ProgressCode::Executing,
                    },
                )?,
            ),
            (50, self.message(6, WorkerBody::SubmitResult { outcome })?),
        ];
        match behavior {
            FakeBehavior::Delayed { seconds } => frames[5].0 = 50 + i64::from(seconds),
            FakeBehavior::ExpiredLease => frames[5].0 = i64::from(LEASE_DURATION_SECONDS),
            FakeBehavior::OutOfOrder => frames[3].0 = 45,
            FakeBehavior::StaleEpoch => {
                frames[5].1.binding.epoch = frames[5]
                    .1
                    .binding
                    .epoch
                    .checked_add(1)
                    .ok_or(ProtocolError::InvalidBinding)?
            }
            FakeBehavior::CrossProject => {
                frames[5].1.binding.task.plan.project_id =
                    ProjectId::new("other-project").map_err(|_| ProtocolError::InvalidBinding)?
            }
            FakeBehavior::CrossAttempt => {
                frames[5].1.binding.attempt_id =
                    AttemptId::new("other-attempt").map_err(|_| ProtocolError::InvalidBinding)?
            }
            FakeBehavior::WrongContext => {
                frames[5].1.binding.context_hash = Digest::new("f".repeat(64))?
            }
            FakeBehavior::Impersonate => {
                frames[5].1.binding.worker_id =
                    WorkerId::new("impostor").map_err(|_| ProtocolError::InvalidBinding)?
            }
            FakeBehavior::Duplicate => {
                let mut heartbeat = frames[3].clone();
                heartbeat.0 += 1;
                let mut result = frames[5].clone();
                result.0 += 1;
                frames.extend([heartbeat, result]);
            }
            FakeBehavior::ResultAfterCancellation => frames.push((
                51,
                self.message(
                    7,
                    WorkerBody::SubmitResult {
                        outcome: ExecutionOutcome::Succeeded {
                            manifest_sha256: artifact.sha256.clone(),
                        },
                    },
                )?,
            )),
            _ => {}
        }
        let mut events = Vec::with_capacity(frames.len());
        for (offset, message) in frames {
            if behavior == FakeBehavior::Disconnect && message.sequence == 6 {
                continue;
            }
            let payload = if behavior == FakeBehavior::Malformed && message.sequence == 6 {
                br#"{"kind":"approve_budget","ceiling":999999}"#.to_vec()
            } else {
                encode_worker_message(&message)?
            };
            events.push(ScriptEvent {
                at: start.checked_add(offset).ok_or(WorkerError::InvalidTime)?,
                input: SimulatedInput::Message(payload),
            });
        }
        if behavior == FakeBehavior::Disconnect {
            events.push(ScriptEvent {
                at: start.checked_add(46).ok_or(WorkerError::InvalidTime)?,
                input: SimulatedInput::Disconnect(self.binding.clone()),
            })
        }
        Ok(FakeScript {
            events,
            artifact: if failure.is_none() && behavior != FakeBehavior::Disconnect {
                Some(artifact)
            } else {
                None
            },
        })
    }
}
struct RoutedEvent {
    input: SimulatedInput,
    peer: PeerEvidence,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedEvent {
    pub at: i64,
    pub outcome: Result<Option<ControllerMessage>, WorkerError>,
}
/// Stable FIFO for equal timestamps, with a fixed lifetime event budget. Routing
/// evidence is attached by the trusted test driver, never decoded from worker bytes.
pub struct EventQueue {
    events: BTreeMap<(i64, u64), RoutedEvent>,
    pushed: u16,
    limit: u16,
}
impl EventQueue {
    pub fn new(limit: u16) -> Result<Self, WorkerError> {
        if !(1..=4096).contains(&limit) {
            return Err(WorkerError::ResourceLimit);
        }
        Ok(Self {
            events: BTreeMap::new(),
            pushed: 0,
            limit,
        })
    }
    pub fn len(&self) -> usize {
        self.events.len()
    }
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
    pub fn push(&mut self, event: ScriptEvent, peer: PeerEvidence) -> Result<(), WorkerError> {
        if self.pushed >= self.limit {
            return Err(WorkerError::ResourceLimit);
        }
        if event.at < 0 {
            return Err(WorkerError::InvalidTime);
        }
        event.input.validate()?;
        let order = self.pushed;
        self.events.insert(
            (event.at, u64::from(order)),
            RoutedEvent {
                input: event.input,
                peer,
            },
        );
        self.pushed += 1;
        Ok(())
    }
    pub async fn run_until(
        &mut self,
        until: i64,
        controller: &mut SimulationController,
        authority: &impl WorkerAuthority,
        clock: &DeterministicClock,
    ) -> Result<Vec<ObservedEvent>, WorkerError> {
        if until < clock.now()? {
            return Err(WorkerError::InvalidTime);
        }
        let mut observed = Vec::new();
        while let Some((&(at, _), _)) = self.events.first_key_value() {
            if at > until {
                break;
            }
            if at < clock.now()? {
                return Err(WorkerError::InvalidTime);
            }
            clock.advance_to(at)?;
            let ((at, _), event) = self.events.pop_first().ok_or(WorkerError::InvalidTime)?;
            let outcome = match event.input {
                SimulatedInput::Message(bytes) => controller
                    .receive(&event.peer, &bytes, authority, clock)
                    .await
                    .map(Some),
                SimulatedInput::Disconnect(binding) => controller
                    .disconnect(&event.peer, &binding, authority, clock)
                    .await
                    .map(|()| None),
            };
            observed.push(ObservedEvent { at, outcome });
        }
        Ok(observed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyntheticInput {
    pub task: rolvya_domain::plan::TaskRef,
    pub manifest_sha256: Digest,
}
