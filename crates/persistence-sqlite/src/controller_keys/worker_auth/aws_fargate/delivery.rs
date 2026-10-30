//! AWS-supported secret distribution contract. Structural policy is necessary,
//! not proof of effective IAM/role exclusivity or deployment isolation.
use super::{AwsIdentityError as E, ExpectedExecution, RoleArn, SecretArn};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProtectedDelivery {
    pub format: u16,
    pub execution: ExpectedExecution,
    pub secret: SecretArn,
    pub version_id: String,
    pub expires_at: i64,
}
impl ProtectedDelivery {
    pub fn validate(&self) -> Result<(), E> {
        let e = &self.execution;
        e.validate()?;
        if self.format != 1
            || self.secret.partition() != e.task.partition()
            || self.secret.account() != e.task.account()
            || self.secret.region() != e.task.region()
            || self.version_id.len() != 32
            || !self
                .version_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.expires_at <= e.authorized_at
            || self.expires_at > e.authorization_until
        {
            return Err(E::ScopeMismatch);
        }
        Ok(())
    }
    pub fn resource_policy(&self) -> Result<Value, E> {
        self.validate()?;
        let until = aws_smithy_types::DateTime::from_secs(self.expires_at)
            .fmt(aws_smithy_types::date_time::Format::DateTime)
            .map_err(|_| E::InvalidInput)?;
        Ok(json!({"Version":"2012-10-17","Statement":[
            {"Sid":"DenyOtherRoleReaders","Effect":"Deny","Principal":"*","Action":"secretsmanager:GetSecretValue","Resource":self.secret,"Condition":{"ArnNotEquals":{"aws:PrincipalArn":self.execution.workload.task_role}}},
            {"Sid":"DenyExpiredReads","Effect":"Deny","Principal":"*","Action":"secretsmanager:GetSecretValue","Resource":self.secret,"Condition":{"DateGreaterThanEquals":{"aws:CurrentTime":until}}},
            {"Sid":"AllowExclusiveAttemptRole","Effect":"Allow","Principal":{"AWS":self.execution.workload.task_role},"Action":"secretsmanager:GetSecretValue","Resource":self.secret}
        ]}))
    }
    pub fn task_identity_policy(&self) -> Result<Value, E> {
        self.validate()?;
        Ok(
            json!({"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"secretsmanager:GetSecretValue","Resource":self.secret}]}),
        )
    }
    /// Exact conservative structural review. External effective policy and role
    /// exclusivity must additionally be independently verified before activation.
    pub fn check_policies(&self, resource: &Value, identity: &Value) -> Result<(), E> {
        if *resource != self.resource_policy()? || *identity != self.task_identity_policy()? {
            return Err(E::SecureDeliveryUnverified);
        }
        Ok(())
    }
    pub fn task_role(&self) -> &RoleArn {
        &self.execution.workload.task_role
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller_keys::worker_auth::aws_fargate::fixture;
    fn plan() -> ProtectedDelivery {
        let e = fixture::expected("task-a");
        ProtectedDelivery{format:1,secret:SecretArn::parse("arn:aws:secretsmanager:eu-central-1:123456789012:secret:rolvya/bootstrap/attempt-a-AbCdEf").unwrap(),version_id:"a".repeat(32),expires_at:e.authorized_at+90,execution:e}
    }
    #[test]
    fn exact_policy_denies_other_roles_expired_reads_and_any_policy_expansion() {
        let p = plan();
        let resource = p.resource_policy().unwrap();
        let identity = p.task_identity_policy().unwrap();
        assert_eq!(p.check_policies(&resource, &identity), Ok(()));
        let mut broad = resource.clone();
        broad["Statement"][0]["Condition"]["ArnNotEquals"]["aws:PrincipalArn"] = json!("*");
        assert!(p.check_policies(&broad, &identity).is_err());
        let mut broad = identity.clone();
        broad["Statement"][0]["Resource"] = json!("*");
        assert!(p.check_policies(&resource, &broad).is_err());
    }
}
