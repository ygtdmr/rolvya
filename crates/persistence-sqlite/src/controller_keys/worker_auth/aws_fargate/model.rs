use super::AwsIdentityError as E;
use crate::{admission::LeaseRef, finance::EvidenceRef};
use rolvya_domain::worker::{Digest, WorkerBinding};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Partition {
    Aws,
    AwsCn,
    AwsUsGov,
}
impl Partition {
    fn parse(s: &str) -> Result<Self, E> {
        match s {
            "aws" => Ok(Self::Aws),
            "aws-cn" => Ok(Self::AwsCn),
            "aws-us-gov" => Ok(Self::AwsUsGov),
            _ => Err(E::UnsupportedPartition),
        }
    }
    fn region(self, s: &str) -> Result<(), E> {
        if s.is_empty()
            || s.len() > 40
            || !s
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || !s.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
            || !s.as_bytes().last().is_some_and(u8::is_ascii_digit)
            || s.contains("--")
        {
            return Err(E::InvalidInput);
        }
        if (s.starts_with("cn-") != (self == Self::AwsCn))
            || (s.starts_with("us-gov-") != (self == Self::AwsUsGov))
        {
            return Err(E::ScopeMismatch);
        }
        Ok(())
    }
    pub(crate) fn dns_suffix(self) -> &'static str {
        if self == Self::AwsCn {
            "amazonaws.com.cn"
        } else {
            "amazonaws.com"
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AwsAccountId(String);
impl AwsAccountId {
    pub fn parse(s: &str) -> Result<Self, E> {
        if s.len() != 12 || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(E::InvalidInput);
        }
        Ok(Self(s.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for AwsAccountId {
    type Error = E;
    fn try_from(s: String) -> Result<Self, E> {
        Self::parse(&s)
    }
}
impl From<AwsAccountId> for String {
    fn from(v: AwsAccountId) -> Self {
        v.0
    }
}
struct Parts<'a> {
    partition: Partition,
    region: &'a str,
    account: &'a str,
    resource: &'a str,
}
fn parts<'a>(value: &'a str, service: &str, regional: bool) -> Result<Parts<'a>, E> {
    if value.len() > 2048
        || !value.is_ascii()
        || value
            .bytes()
            .any(|b| b <= b' ' || b == 127 || b == b'%' || b == b'\\')
    {
        return Err(E::InvalidInput);
    }
    let v: Vec<_> = value.splitn(6, ':').collect();
    if v.len() != 6 || v[0] != "arn" || v[2] != service {
        return Err(E::InvalidInput);
    }
    let partition = Partition::parse(v[1])?;
    AwsAccountId::parse(v[4])?;
    if regional {
        partition.region(v[3])?;
    } else if !v[3].is_empty() {
        return Err(E::InvalidInput);
    }
    Ok(Parts {
        partition,
        region: v[3],
        account: v[4],
        resource: v[5],
    })
}
fn name(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
macro_rules! arn {
    ($type:ident,$service:literal,$regional:literal,$validate:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $type(String);
        impl $type {
            pub fn parse(value: &str) -> Result<Self, E> {
                let p = parts(value, $service, $regional)?;
                if !($validate)(p.resource) {
                    return Err(E::InvalidInput);
                }
                Ok(Self(value.into()))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
            fn parts(&self) -> Parts<'_> {
                parts(&self.0, $service, $regional).expect("validated AWS ARN")
            }
            pub fn account(&self) -> AwsAccountId {
                AwsAccountId(self.parts().account.into())
            }
            pub fn partition(&self) -> Partition {
                self.parts().partition
            }
        }
        impl TryFrom<String> for $type {
            type Error = E;
            fn try_from(s: String) -> Result<Self, E> {
                Self::parse(&s)
            }
        }
        impl From<$type> for String {
            fn from(v: $type) -> Self {
                v.0
            }
        }
    };
}
arn!(ClusterArn, "ecs", true, |r: &str| r
    .strip_prefix("cluster/")
    .is_some_and(|n| name(n, 255)));
arn!(TaskArn, "ecs", true, |r: &str| {
    let v: Vec<_> = r.splitn(4, '/').collect();
    v.len() == 3
        && v[0] == "task"
        && name(v[1], 255)
        && v[2].len() == 32
        && v[2]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
});
arn!(TaskDefinitionArn, "ecs", true, |r: &str| r
    .strip_prefix("task-definition/")
    .and_then(|r| r.split_once(':'))
    .is_some_and(|(family, revision)| name(family, 255)
        && revision
            .parse::<u32>()
            .is_ok_and(|n| n > 0 && n.to_string() == revision)));
arn!(RoleArn, "iam", false, |r: &str| r
    .strip_prefix("role/")
    .is_some_and(|r| !r.is_empty()
        && r.len() <= 512
        && r.split('/').all(|s| !s.is_empty()
            && s.bytes().all(
                |b| b.is_ascii_alphanumeric() || b"_+=,.@-".contains(&b)
            ))));
arn!(AssumedRoleArn, "sts", false, |r: &str| {
    let v: Vec<_> = r.splitn(4, '/').collect();
    v.len() == 3
        && v[0] == "assumed-role"
        && v[1..].iter().all(|s| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_+=,.@-".contains(&b))
        })
});
impl ClusterArn {
    pub fn region(&self) -> &str {
        self.parts().region
    }
    pub fn name(&self) -> &str {
        self.parts()
            .resource
            .strip_prefix("cluster/")
            .expect("validated cluster")
    }
}
impl TaskArn {
    pub fn region(&self) -> &str {
        self.parts().region
    }
    pub fn cluster_name(&self) -> &str {
        self.parts()
            .resource
            .split('/')
            .nth(1)
            .expect("validated task")
    }
    pub fn task_id(&self) -> &str {
        self.parts()
            .resource
            .rsplit('/')
            .next()
            .expect("validated task")
    }
}
impl TaskDefinitionArn {
    pub fn region(&self) -> &str {
        self.parts().region
    }
}
impl RoleArn {
    pub fn role_name(&self) -> &str {
        self.parts()
            .resource
            .rsplit('/')
            .next()
            .expect("validated role")
    }
}
impl AssumedRoleArn {
    pub fn role_name(&self) -> &str {
        self.parts()
            .resource
            .split('/')
            .nth(1)
            .expect("validated role")
    }
    pub fn session_name(&self) -> &str {
        self.parts()
            .resource
            .rsplit('/')
            .next()
            .expect("validated principal")
    }
}
fn identifier(s: &str, prefix: &str) -> bool {
    s.strip_prefix(prefix).is_some_and(|v| {
        (v.len() == 8 || v.len() == 17)
            && v.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn unique(v: &[String]) -> bool {
    let s: std::collections::BTreeSet<_> = v.iter().collect();
    s.len() == v.len()
}
/// Approved configuration reference data. Constructing/validating this DTO does
/// not establish Owner consent, IAM policy, task possession or billing authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FargateWorkload {
    pub format: u16,
    pub cluster: ClusterArn,
    pub task_definition: TaskDefinitionArn,
    pub task_role: RoleArn,
    pub execution_role: RoleArn,
    pub controller_role: RoleArn,
    pub controller_role_id: String,
    pub image: String,
    pub image_digest: String,
    pub container: String,
    pub cpu_units: u32,
    pub memory_mib: u32,
    pub ephemeral_gib: u32,
    pub platform_version: String,
    pub architecture: String,
    pub user: String,
    pub command: Vec<String>,
    pub vpc: String,
    pub subnets: Vec<String>,
    pub security_groups: Vec<String>,
    pub network_review: EvidenceRef,
    pub iam_review: EvidenceRef,
    pub runtime_seconds: u32,
}
impl FargateWorkload {
    pub fn validate(&self) -> Result<(), E> {
        if self.format != 1
            || self.task_definition.partition() != self.cluster.partition()
            || self.task_definition.account() != self.cluster.account()
            || self.task_definition.region() != self.cluster.region()
            || [&self.task_role, &self.execution_role, &self.controller_role]
                .iter()
                .any(|r| {
                    r.partition() != self.cluster.partition()
                        || r.account() != self.cluster.account()
                })
        {
            return Err(E::ScopeMismatch);
        }
        if self.task_role == self.execution_role
            || self.task_role == self.controller_role
            || self.execution_role == self.controller_role
        {
            return Err(E::RoleMismatch);
        }
        let digest = self
            .image_digest
            .strip_prefix("sha256:")
            .ok_or(E::ImageMismatch)?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.image.len() > 1024
            || !self.image.ends_with(&format!("@{}", self.image_digest))
            || self.image.contains(char::is_whitespace)
            || !name(&self.container, 128)
        {
            return Err(E::ImageMismatch);
        }
        let memory = self.memory_mib;
        let valid_resource = match self.cpu_units {
            256 => [512, 1024, 2048].contains(&memory),
            512 => (1024..=4096).contains(&memory) && memory.is_multiple_of(1024),
            1024 => (2048..=8192).contains(&memory) && memory.is_multiple_of(1024),
            2048 => (4096..=16384).contains(&memory) && memory.is_multiple_of(1024),
            4096 => (8192..=30720).contains(&memory) && memory.is_multiple_of(1024),
            8192 => (16384..=61440).contains(&memory) && memory.is_multiple_of(4096),
            16384 => (32768..=122880).contains(&memory) && memory.is_multiple_of(8192),
            _ => false,
        };
        if self.user.len() > 32
            || self.controller_role_id.len() > 128
            || !self.controller_role_id.starts_with("AROA")
            || self.controller_role_id.len() < 16
            || !self
                .controller_role_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric())
        {
            return Err(E::InvalidInput);
        }
        let user: Vec<_> = self.user.splitn(3, ':').collect();
        if !valid_resource
            || !(20..=200).contains(&self.ephemeral_gib)
            || !(1..=86400).contains(&self.runtime_seconds)
            || !matches!(self.architecture.as_str(), "X86_64" | "ARM64")
            || self.platform_version.len() > 32
            || !self
                .platform_version
                .split('.')
                .collect::<Vec<_>>()
                .as_slice()
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            || self.platform_version.split('.').count() != 3
            || user.is_empty()
            || user.len() > 2
            || user.iter().any(|s| {
                s.parse::<u32>()
                    .map_or(true, |n| n == 0 || n.to_string() != *s)
            })
        {
            return Err(E::IsolationMismatch);
        }
        if self.command.is_empty()
            || self.command.len() > 16
            || self
                .command
                .iter()
                .any(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
            || !identifier(&self.vpc, "vpc-")
            || self.subnets.is_empty()
            || self.subnets.len() > 16
            || !unique(&self.subnets)
            || self.subnets.iter().any(|s| !identifier(s, "subnet-"))
            || self.security_groups.is_empty()
            || self.security_groups.len() > 5
            || !unique(&self.security_groups)
            || self.security_groups.iter().any(|s| !identifier(s, "sg-"))
        {
            return Err(E::IsolationMismatch);
        }
        Ok(())
    }
}
/// Immutable expected execution, later stored only by a trusted launch-record
/// authority. Worker-supplied instances of this type confer no permission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedExecution {
    pub format: u16,
    pub installation: String,
    pub account_ref: EvidenceRef,
    pub account_version: u64,
    pub binding: WorkerBinding,
    pub command_digest: Digest,
    pub task: TaskArn,
    pub workload: FargateWorkload,
    pub operation_token: String,
    pub authorized_at: i64,
    pub authorization_until: i64,
}
impl ExpectedExecution {
    pub fn validate(&self) -> Result<(), E> {
        self.workload.validate()?;
        self.binding.validate().map_err(|_| E::InvalidInput)?;
        let c = &self.workload.cluster;
        if self.format != 1
            || self.installation.len() != 32
            || !self
                .installation
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.account_version == 0
            || self.account_version > i64::MAX as u64
            || self.authorized_at < 0
            || self.authorization_until <= self.authorized_at
            || self.operation_token.len() != 64
            || !self
                .operation_token
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(E::InvalidInput);
        }
        if self.task.partition() != c.partition()
            || self.task.account() != c.account()
            || self.task.region() != c.region()
            || self.task.cluster_name() != c.name()
        {
            return Err(E::ScopeMismatch);
        }
        Ok(())
    }
    pub fn expected_operation_token(&self) -> Result<String, E> {
        use sha2::Digest as _;
        let bytes = serde_json::to_vec(&(
            "rolvya/aws-run-task-operation/v1",
            &self.installation,
            &self.account_ref,
            self.account_version,
            &self.binding,
            &self.command_digest,
            &self.workload,
        ))
        .map_err(|_| E::InvalidInput)?;
        Ok(format!("{:x}", sha2::Sha256::digest(bytes)))
    }
    pub fn started_by(&self) -> String {
        format!("rv-{}", self.operation_token.get(..32).unwrap_or(""))
    }
    pub fn lease(&self) -> LeaseRef {
        LeaseRef {
            task: self.binding.task.clone(),
            attempt_id: self.binding.attempt_id.clone(),
            epoch: self.binding.epoch,
        }
    }
}
/// This assessment never authorizes a worker. Matching AWS inspection records
/// cannot establish possession, effective IAM/network isolation or final cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EcsLifecycle {
    Provisioning,
    Pending,
    Running,
    Stopping,
    Stopped,
    Unknown,
}
impl EcsLifecycle {
    pub fn from_status(s: &str) -> Self {
        match s {
            "PROVISIONING" | "ACTIVATING" => Self::Provisioning,
            "PENDING" => Self::Pending,
            "RUNNING" => Self::Running,
            "DEACTIVATING" | "STOPPING" | "DEPROVISIONING" => Self::Stopping,
            "STOPPED" => Self::Stopped,
            _ => Self::Unknown,
        }
    }
    pub fn bootstrap_eligible(self) -> bool {
        self == Self::Running
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ecs_task_arn_requires_explicit_partition_region_account_cluster_and_task_identity() {
        let valid =
            "arn:aws:ecs:eu-central-1:123456789012:task/rolvya/0123456789abcdef0123456789abcdef";
        assert_eq!(TaskArn::parse(valid).unwrap().as_str(), valid);
        for value in [
            "arn:aws:ecs:eu-central-1:123456789012:task/0123456789abcdef0123456789abcdef",
            "arn:aws:ecs:eu-central-1:1234:task/rolvya/0123456789abcdef0123456789abcdef",
            "arn:aws:iam::123456789012:role/worker",
            "arn:aws:ecs:cn-north-1:123456789012:task/rolvya/0123456789abcdef0123456789abcdef",
            "arn:aws:ecs:eu-central-1:123456789012:task/rolvya/../task",
        ] {
            assert!(TaskArn::parse(value).is_err());
        }
    }
}
