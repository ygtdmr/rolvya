//! Challenge/operation/channel-bound proof of a protected per-attempt secret.
//! This is a Rolvya possession proof, never AWS-signed task attestation.
use super::super::{BootstrapChallengeId, BootstrapSecret, WorkerCredentialId};
use super::{AwsIdentityError as E, ExpectedExecution, SecretArn};
use crate::RequestId;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PossessionOperation {
    Bootstrap,
    Renew,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PossessionChallenge {
    pub format: u16,
    pub id: BootstrapChallengeId,
    pub operation: PossessionOperation,
    pub execution: ExpectedExecution,
    pub credential: Option<WorkerCredentialId>,
    pub generation: i64,
    pub issued_at: i64,
    pub expires_at: i64,
    pub secret: SecretArn,
    pub secret_version: String,
}
impl PossessionChallenge {
    pub(super) fn validate(&self, now: i64) -> Result<(), E> {
        self.execution.validate()?;
        if self.execution.operation_token != self.execution.expected_operation_token()?
            || self.secret.account() != self.execution.task.account()
            || self.secret.partition() != self.execution.task.partition()
            || self.secret.region() != self.execution.task.region()
            || self.secret_version.len() != 32
            || !self
                .secret_version
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(E::ScopeMismatch);
        }
        if self.format != 1
            || self.issued_at < self.execution.authorized_at
            || self.expires_at <= self.issued_at
            || self.expires_at - self.issued_at > 300
            || now < self.issued_at
            || now >= self.expires_at
            || self.expires_at > self.execution.authorization_until
            || self.generation < 1
            || self.generation > 64
            || (self.operation == PossessionOperation::Bootstrap
                && (self.credential.is_some() || self.generation != 1))
            || (self.operation == PossessionOperation::Renew
                && (self.credential.is_none() || self.generation < 2))
        {
            return Err(E::ExpiredEvidence);
        }
        Ok(())
    }
    pub(super) fn scope_bytes(&self) -> Result<Vec<u8>, E> {
        // Fixed discriminant plus length-framed fields; JSON is storage only.
        let mut b = b"rolvya/aws-protected-possession/v1\0".to_vec();
        let e = &self.execution;
        let binding = &e.binding;
        for value in [
            e.installation.as_bytes(),
            e.account_ref.as_str().as_bytes(),
            &e.account_version.to_be_bytes(),
            binding.task.plan.project_id.as_str().as_bytes(),
            binding.task.plan.plan_id.as_str().as_bytes(),
            &binding.task.plan.version.get().to_be_bytes(),
            binding.task.task_id.as_str().as_bytes(),
            binding.attempt_id.as_str().as_bytes(),
            binding.worker_id.as_str().as_bytes(),
            &binding.epoch.to_be_bytes(),
            binding.plan_hash.as_str().as_bytes(),
            binding.context_hash.as_str().as_bytes(),
            e.command_digest.as_str().as_bytes(),
            e.task.as_str().as_bytes(),
            e.workload.task_definition.as_str().as_bytes(),
            e.workload.task_role.as_str().as_bytes(),
            e.operation_token.as_bytes(),
            self.id.as_str().as_bytes(),
            match self.operation {
                PossessionOperation::Bootstrap => b"bootstrap",
                PossessionOperation::Renew => b"renew",
            },
            self.credential
                .as_ref()
                .map(|v| v.as_str())
                .unwrap_or("")
                .as_bytes(),
            &self.generation.to_be_bytes(),
            &self.issued_at.to_be_bytes(),
            &self.expires_at.to_be_bytes(),
            self.secret.as_str().as_bytes(),
            self.secret_version.as_bytes(),
        ] {
            b.extend_from_slice(&(value.len() as u32).to_be_bytes());
            b.extend_from_slice(value);
        }
        Ok(b)
    }
    fn canonical(&self, request: &RequestId, channel: &[u8; 32]) -> Result<Vec<u8>, E> {
        let mut b = self.scope_bytes()?;
        for v in [request.as_str().as_bytes(), channel] {
            b.extend_from_slice(&(v.len() as u32).to_be_bytes());
            b.extend_from_slice(v);
        }
        Ok(b)
    }
    pub(in crate::controller_keys::worker_auth) fn sign(
        &self,
        secret: &BootstrapSecret,
        request: &RequestId,
        channel: &[u8; 32],
        now: i64,
    ) -> Result<[u8; 32], E> {
        self.validate(now)?;
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret.0.0.as_bytes()).map_err(|_| E::Integrity)?;
        mac.update(&self.canonical(request, channel)?);
        Ok(mac.finalize().into_bytes().into())
    }
    pub(super) fn verify(
        &self,
        secret: &BootstrapSecret,
        request: &RequestId,
        channel: &[u8; 32],
        signature: &[u8; 32],
        now: i64,
    ) -> Result<(), E> {
        self.validate(now)?;
        let mut mac =
            Hmac::<Sha256>::new_from_slice(secret.0.0.as_bytes()).map_err(|_| E::Integrity)?;
        mac.update(&self.canonical(request, channel)?);
        mac.verify_slice(signature)
            .map_err(|_| E::TaskPossessionUnverified)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller_keys::worker_auth::{aws_fargate::fixture, hex, random};
    fn challenge() -> PossessionChallenge {
        let e = fixture::expected("task-a");
        PossessionChallenge{format:1,id:BootstrapChallengeId::parse("a".repeat(32)).unwrap(),operation:PossessionOperation::Bootstrap,issued_at:e.authorized_at,expires_at:e.authorized_at+90,execution:e,credential:None,generation:1,secret:SecretArn::parse("arn:aws:secretsmanager:eu-central-1:123456789012:secret:rolvya/bootstrap/attempt-a-AbCdEf").unwrap(),secret_version:"a".repeat(32)}
    }
    #[test]
    fn valid_possession_requires_exact_live_channel_challenge_and_operation() {
        let c = challenge();
        let s = BootstrapSecret::parse(hex(&random::<32>().unwrap()[..])).unwrap();
        let r = RequestId::new("bound-request").unwrap();
        let channel = *random::<32>().unwrap();
        let tag = c.sign(&s, &r, &channel, c.issued_at).unwrap();
        assert_eq!(c.verify(&s, &r, &channel, &tag, c.issued_at), Ok(()));
        let other = *random::<32>().unwrap();
        assert!(c.verify(&s, &r, &other, &tag, c.issued_at).is_err());
        let mut changed = c.clone();
        changed.execution.binding.epoch += 1;
        assert!(changed.verify(&s, &r, &channel, &tag, c.issued_at).is_err());
        assert!(c.verify(&s, &r, &channel, &tag, c.expires_at).is_err());
    }
}
