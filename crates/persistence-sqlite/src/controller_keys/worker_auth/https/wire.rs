//! Versioned HTTP framing over the existing worker protocol, not another authority.
use super::super::BootstrapChallengeId;
use crate::worker_simulation::SyntheticInput;
use crate::{RequestId, finance::EvidenceRef};
use rolvya_domain::worker::*;
use serde::{Deserialize, Serialize};

pub const WIRE_VERSION: u16 = 1;
pub const MAX_HTTP_BODY: usize = 128 * 1024;
pub const MAX_RESPONSE_BODY: usize = 96 * 1024;
/// Provider statement/signature are untrusted until independently verified.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attestation {
    pub reference: EvidenceRef,
    pub statement: String,
    pub signature: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactUpload {
    pub sha256: Digest,
    pub data: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptDisposition {
    Applied,
    IdempotentReplay,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Bootstrap {
        challenge: BootstrapChallengeId,
        attestation: Attestation,
    },
    Renew {
        attestation: Attestation,
    },
    /// Metadata only. Possession material is delivered through the separate,
    /// restricted resource, never this bearer-authenticated response.
    PreparePossession {},
    Message {
        message: Box<WorkerMessage>,
        artifact: Option<ArtifactUpload>,
    },
    Assignment {},
    CodingWork {},
    CodingSource {
        work: Digest,
        file: u16,
        offset: u32,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u16,
    pub request_id: RequestId,
    pub operation: Operation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthenticated,
    UnauthorizedScope,
    InvalidProtocol,
    StaleVersion,
    ExpiredCredential,
    RevokedCredential,
    FencedEpoch,
    LeaseExpired,
    FinanciallyBlocked,
    ProviderEntitlementUnavailable,
    RateLimited,
    ConflictingReplay,
    TransientStorage,
    Unavailable,
    InvalidArtifact,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorResponse {
    pub version: u16,
    pub code: ErrorCode,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub binding: WorkerBinding,
    pub capabilities: DeclaredCapabilities,
    pub seed: u64,
    pub inputs: Vec<SyntheticInput>,
    pub initial_lease_until: i64,
    pub heartbeat_seconds: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Response {
    Message {
        version: u16,
        response: ControllerMessage,
        disposition: ReceiptDisposition,
    },
    Assignment {
        version: u16,
        assignment: Assignment,
    },
    CodingWork {
        version: u16,
        work: Box<crate::orchestration::coding::work::CodingWork>,
    },
    CodingSource {
        version: u16,
        chunk: crate::orchestration::coding::work::SourceChunk,
    },
    Possession {
        version: u16,
        challenge: Box<super::super::aws_fargate::PossessionChallenge>,
    },
}
// Credential-bearing responses have a separate, transport-owned codec. They
// deliberately do not appear in a serializable DTO or this public enum.
impl std::fmt::Debug for Attestation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Attestation([untrusted redacted statement])")
    }
}
impl Request {
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.version != WIRE_VERSION || RequestId::new(self.request_id.as_str()).is_err() {
            return Err(ErrorCode::InvalidProtocol);
        }
        match &self.operation {
            Operation::Bootstrap {
                challenge,
                attestation,
            } => {
                if BootstrapChallengeId::parse(challenge.as_str().to_owned()).is_err() {
                    return Err(ErrorCode::InvalidProtocol);
                }
                attestation.validate()?;
            }
            Operation::Renew { attestation } => attestation.validate()?,
            Operation::Message { message, artifact } => {
                message.validate().map_err(|_| ErrorCode::InvalidProtocol)?;
                if self.request_id.as_str() != message.message_id.as_str() {
                    return Err(ErrorCode::InvalidProtocol);
                }
                match (&message.body, artifact) {
                    (
                        WorkerBody::SubmitResult {
                            outcome: ExecutionOutcome::Succeeded { manifest_sha256 },
                        },
                        Some(a),
                    ) if a.sha256 == *manifest_sha256 && a.data.len() <= 88 * 1024 => {}
                    (
                        WorkerBody::SubmitResult {
                            outcome: ExecutionOutcome::Succeeded { .. },
                        },
                        _,
                    ) => return Err(ErrorCode::InvalidArtifact),
                    (_, None) => {}
                    _ => return Err(ErrorCode::InvalidProtocol),
                }
            }
            Operation::Assignment {}
            | Operation::PreparePossession {}
            | Operation::CodingWork {}
            | Operation::CodingSource { .. } => {}
        }
        Ok(())
    }
}
impl Attestation {
    fn validate(&self) -> Result<(), ErrorCode> {
        if self.statement.is_empty()
            || self.statement.len() > 16384
            || self.signature.len() > 128
            || self.signature.is_empty()
        {
            return Err(ErrorCode::InvalidProtocol);
        }
        Ok(())
    }
}
impl std::fmt::Debug for ArtifactUpload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArtifactUpload")
            .field("sha256", &self.sha256)
            .field("encoded_length", &self.data.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_wire_version_cannot_reach_worker_authority() {
        let r = Request {
            version: 2,
            request_id: RequestId::new("wire-test").unwrap(),
            operation: Operation::Assignment {},
        };
        assert_eq!(r.validate(), Err(ErrorCode::InvalidProtocol));
    }
    #[test]
    fn unknown_privileged_fields_are_not_ignored() {
        assert!(serde_json::from_str::<Request>(r#"{"version":1,"request_id":"wire-test","operation":{"kind":"assignment"},"trusted":true}"#).is_err());
    }
}
