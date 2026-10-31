use super::*;
use rolvya_domain::{
    money::CostBreakdown,
    worker::{Digest, WorkerBinding},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
/// Nonsecret allocation before AWS chooses a task ARN. This is not a task
/// attestation, secret value, role-exclusivity proof or permission to launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapAllocation {
    pub secret: SecretArn,
    pub task_role: RoleArn,
    pub expires_at: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchRequest {
    pub format: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifier: Option<super::verifier::VerifierLaunch>,
    pub installation: String,
    pub account_ref: EvidenceRef,
    pub account_version: u64,
    pub binding: WorkerBinding,
    pub command_digest: Digest,
    pub workload: FargateWorkload,
    pub bootstrap: BootstrapAllocation,
    pub exposure: ExposureEnvelope,
    pub maximum: CostBreakdown,
    pub financial_event: i64,
    pub client_token: String,
    pub authorized_at: i64,
    /// Short launch/initial delivery deadline, separate from renewable execution.
    pub launch_until: i64,
    pub authorization_until: i64,
}
impl LaunchRequest {
    pub fn purpose(&self) -> &'static str {
        if self.verifier.is_some() {
            "verifier"
        } else {
            "coding"
        }
    }
    pub(super) fn expected_token(&self) -> Result<String, E> {
        let base = operation_token(
            &self.installation,
            &self.account_ref,
            self.account_version,
            &self.binding,
            &self.command_digest,
            &self.workload,
        )?;
        match &self.verifier {
            Some(v) => digest(&("rolvya/aws-independent-verifier/v1", base, v)),
            None => Ok(base),
        }
    }
    pub fn digest(&self) -> Result<String, E> {
        digest(self)
    }
    pub fn started_by(&self) -> String {
        format!("rv-{}", self.client_token.get(..32).unwrap_or(""))
    }
    pub fn lease(&self) -> crate::admission::LeaseRef {
        crate::admission::LeaseRef {
            task: self.binding.task.clone(),
            attempt_id: self.binding.attempt_id.clone(),
            epoch: self.binding.epoch,
        }
    }
    pub fn validate(&self, now: i64) -> Result<(), E> {
        self.workload.validate()?;
        self.binding.validate().map_err(|_| E::InvalidInput)?;
        if self.format != 1
            || self.installation.len() != 32
            || !self
                .installation
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || self.account_version == 0
            || self.account_version > i64::MAX as u64
            || self.financial_event <= 0
            || self.authorized_at < 0
            || now < self.authorized_at
            || now >= self.authorization_until
            || now >= self.launch_until
            || self.launch_until > self.authorization_until
            || self.launch_until - self.authorized_at > 90
            || self.authorization_until - self.authorized_at
                > i64::from(self.workload.runtime_seconds)
                    + i64::from(self.exposure.provisioning_seconds)
                    + i64::from(self.exposure.termination_seconds)
            || self.bootstrap.expires_at > self.launch_until
            || self.bootstrap.expires_at <= now
        {
            return Err(E::ExpiredEvidence);
        }
        let w = &self.workload;
        let s = &self.bootstrap.secret;
        if self.bootstrap.task_role != w.task_role
            || s.account() != w.cluster.account()
            || s.partition() != w.cluster.partition()
            || s.region() != w.cluster.region()
            || self.client_token != self.expected_token()?
        {
            return Err(E::ScopeMismatch);
        }
        let mut actual = self.exposure.upper_bound(w, now)?;
        if let Some(v) = &self.verifier {
            v.validate(self, now)?;
            actual = super::verifier::combined(actual, v.coding_maximum)?;
        }
        self.maximum.validate().map_err(|_| E::FinancialRisk)?;
        for (a, m) in [
            actual.ai_api,
            actual.compute,
            actual.storage,
            actual.transfer,
        ]
        .into_iter()
        .zip([
            self.maximum.ai_api,
            self.maximum.compute,
            self.maximum.storage,
            self.maximum.transfer,
        ]) {
            if a.currency() != m.currency() || a.minor_units() > m.minor_units() {
                return Err(E::FinancialRisk);
            }
        }
        Ok(())
    }
    pub(super) fn expected(&self, task: TaskArn) -> Result<ExpectedExecution, E> {
        let e = ExpectedExecution {
            format: 1,
            installation: self.installation.clone(),
            account_ref: self.account_ref.clone(),
            account_version: self.account_version,
            binding: self.binding.clone(),
            command_digest: self.command_digest.clone(),
            task,
            workload: self.workload.clone(),
            operation_token: self.client_token.clone(),
            authorized_at: self.authorized_at,
            authorization_until: self.authorization_until,
        };
        e.validate()?;
        Ok(e)
    }
    /// Intended minimum policy only. No claim about effective IAM/SCP/boundaries,
    /// task-role exclusivity, subnet conditions or production permission.
    pub fn intended_controller_policy(&self) -> serde_json::Value {
        serde_json::json!({"Version":"2012-10-17","Statement":[
        {"Effect":"Allow","Action":"ecs:RunTask","Resource":self.workload.task_definition,"Condition":{"ArnEquals":{"ecs:cluster":self.workload.cluster}}},
        {"Effect":"Allow","Action":"iam:PassRole","Resource":[self.workload.task_role,self.workload.execution_role],"Condition":{"StringEquals":{"iam:PassedToService":"ecs-tasks.amazonaws.com"}}},
        {"Effect":"Allow","Action":"ecs:DescribeTaskDefinition","Resource":"*"}
        ]})
    }
}
pub(super) fn operation_token(
    i: &str,
    a: &EvidenceRef,
    v: u64,
    b: &WorkerBinding,
    d: &Digest,
    w: &FargateWorkload,
) -> Result<String, E> {
    digest(&("rolvya/aws-run-task-operation/v1", i, a, v, b, d, w))
}
pub(super) fn digest(v: &impl Serialize) -> Result<String, E> {
    Ok(format!("{:x}", Sha256::digest(encode(v)?.as_bytes())))
}
pub(super) fn encode(v: &impl Serialize) -> Result<String, E> {
    let s = serde_json::to_string(v).map_err(|_| E::Integrity)?;
    if s.len() > 32768 {
        return Err(E::ResourceLimit);
    }
    Ok(s)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    OwnerCancellation,
    Fenced,
    LeaseExpired,
    AuthorizationRevoked,
    SecurityIncident,
    FinancialRisk,
    RuntimeLimit,
    Terminal,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionStatus {
    pub revision: u64,
    pub kind: String,
    pub task: Option<TaskArn>,
    pub lifecycle: EcsLifecycle,
    pub unknown_exposure: bool,
    pub stop_requested: bool,
    pub cleanup_requested: bool,
    pub observed_at: i64,
}
