use super::*;
use crate::controller_keys::worker_auth::aws_fargate::{FargateWorkload, SecretArn};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IsolationConfiguration {
    pub version: u16,
    pub coding: FargateWorkload,
    pub verifier: FargateWorkload,
    pub coding_secret: SecretArn,
    pub verifier_secret: SecretArn,
    pub verifier_policy_sha256: String,
    pub verifier_executables_sha256: String,
    pub coding_identity_policy: serde_json::Value,
    pub verifier_identity_policy: serde_json::Value,
    pub controller_policy: serde_json::Value,
    pub coding_trust: serde_json::Value,
    pub verifier_trust: serde_json::Value,
    pub network: NetworkConfiguration,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NetworkConfiguration {
    pub controller_origin: String,
    pub artifact_origin: String,
    pub vpc: String,
    pub subnets: Vec<String>,
    pub groups: Vec<String>,
    pub ingress: Vec<NetworkRule>,
    pub egress: Vec<NetworkRule>,
    pub endpoints: Vec<Endpoint>,
    pub dns_support: bool,
    pub dns_hostnames: bool,
    pub public_ip: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NetworkRule {
    pub protocol: String,
    pub from: u16,
    pub to: u16,
    pub destination_group: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub purpose: EndpointPurpose,
    pub group: String,
    pub hostname: String,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EndpointPurpose {
    Controller,
    SecretsManager,
    EcrApi,
    EcrRegistry,
    Logs,
    Artifacts,
    Dns,
}
impl IsolationConfiguration {
    /// Strictly reviews a supported declaration subset, never effective IAM.
    /// Unknown policy constructs and equivalent-but-unreviewed expansions fail.
    pub fn inspect(&self) -> Result<String, E> {
        self.coding.validate().map_err(|_| E::Scope)?;
        self.verifier.validate().map_err(|_| E::Scope)?;
        let a = &self.coding;
        let b = &self.verifier;
        if self.version != 1
            || a.coding_isolation!=Some(crate::controller_keys::worker_auth::aws_fargate::CodingIsolation::CredentialProxyV1)
            || b.coding_isolation.is_some()
            || a.execution_role==b.execution_role
            || !hash(&self.verifier_policy_sha256) || !hash(&self.verifier_executables_sha256)
            || b.runtime_seconds>300
            || b.command!=["/opt/rolvya/verifier","--policy",&self.verifier_policy_sha256,"--executables",&self.verifier_executables_sha256]
            || a.cluster != b.cluster
            || a.task_role == b.task_role
            || a.task_definition == b.task_definition
            || a.image_digest == b.image_digest
            || [
                a.execution_role.as_str(),
                b.execution_role.as_str(),
                a.controller_role.as_str(),
                b.controller_role.as_str(),
            ]
            .contains(&a.task_role.as_str())
            || [
                a.execution_role.as_str(),
                b.execution_role.as_str(),
                a.controller_role.as_str(),
                b.controller_role.as_str(),
            ]
            .contains(&b.task_role.as_str())
            || a.controller_role != b.controller_role
            || a.controller_role_id != b.controller_role_id
            || self.coding_secret == self.verifier_secret
        {
            return Err(E::Scope);
        }
        for s in [&self.coding_secret, &self.verifier_secret] {
            if s.partition() != a.cluster.partition()
                || s.account() != a.cluster.account()
                || s.region() != a.cluster.region()
            {
                return Err(E::Scope);
            }
        }
        let p = self.required_policies();
        if self.coding_identity_policy != p.coding
            || self.verifier_identity_policy != p.verifier
            || self.controller_policy != p.controller
            || self.coding_trust != p.trust
            || self.verifier_trust != p.trust
        {
            return Err(E::Scope);
        }
        self.network.inspect(a, b)?;
        digest("rolvya/isolation-configuration/v1", self)
    }
    /// Expected *identity policy* subset. Resource policies, SCPs, boundaries,
    /// KMS and actual role sessions require independent external checks.
    pub fn required_policies(&self) -> RequiredPolicies {
        use serde_json::json;
        let account = self.coding.cluster.account();
        let partition = match self.coding.cluster.partition() {
            crate::controller_keys::worker_auth::aws_fargate::Partition::Aws => "aws",
            crate::controller_keys::worker_auth::aws_fargate::Partition::AwsCn => "aws-cn",
            crate::controller_keys::worker_auth::aws_fargate::Partition::AwsUsGov => "aws-us-gov",
        };
        let read = |s: &SecretArn| json!({"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"secretsmanager:GetSecretValue","Resource":s}]});
        let trust = json!({"Version":"2012-10-17","Statement":[{"Effect":"Allow","Principal":{"Service":"ecs-tasks.amazonaws.com"},"Action":"sts:AssumeRole","Condition":{"StringEquals":{"aws:SourceAccount":account.as_str()},"ArnLike":{"aws:SourceArn":format!("arn:{partition}:ecs:{}:{}:*",self.coding.cluster.region(),account.as_str())}}}]});
        RequiredPolicies {
            coding: read(&self.coding_secret),
            verifier: read(&self.verifier_secret),
            trust,
            controller: json!({"Version":"2012-10-17","Statement":[
                {"Effect":"Allow","Action":"ecs:RunTask","Resource":[self.coding.task_definition,self.verifier.task_definition],"Condition":{"ArnEquals":{"ecs:cluster":self.coding.cluster}}},
                {"Effect":"Allow","Action":"iam:PassRole","Resource":std::collections::BTreeSet::from([self.coding.task_role.as_str(),self.coding.execution_role.as_str(),self.verifier.task_role.as_str(),self.verifier.execution_role.as_str()]),"Condition":{"StringEquals":{"iam:PassedToService":"ecs-tasks.amazonaws.com"}}}
            ]}),
        }
    }
}
pub struct RequiredPolicies {
    pub coding: serde_json::Value,
    pub verifier: serde_json::Value,
    pub controller: serde_json::Value,
    pub trust: serde_json::Value,
}
impl NetworkConfiguration {
    fn inspect(&self, a: &FargateWorkload, b: &FargateWorkload) -> Result<(), E> {
        use std::collections::BTreeSet;
        let groups = |v: &[String]| v.iter().cloned().collect::<BTreeSet<_>>();
        let controller = origin_host(&self.controller_origin)?;
        let artifacts = origin_host(&self.artifact_origin)?;
        if groups(&self.groups).len() != self.groups.len()
            || groups(&self.subnets).len() != self.subnets.len()
            || self.vpc != a.vpc
            || self.vpc != b.vpc
            || groups(&self.subnets) != groups(&a.subnets)
            || groups(&self.subnets) != groups(&b.subnets)
            || groups(&self.groups) != groups(&a.security_groups)
            || groups(&self.groups) != groups(&b.security_groups)
            || !self.ingress.is_empty()
            || self.public_ip
            || !self.dns_support
            || !self.dns_hostnames
            || self.endpoints.len() != 6
            || self.egress.len() != 6
        {
            return Err(E::Scope);
        }
        let purposes = self
            .endpoints
            .iter()
            .map(|e| e.purpose)
            .collect::<BTreeSet<_>>();
        if purposes.len() != 6 || purposes.contains(&EndpointPurpose::Dns) {
            return Err(E::Scope);
        }
        let mut destinations = BTreeSet::new();
        for e in &self.endpoints {
            if !group(&e.group)
                || self.groups.contains(&e.group)
                || !destinations.insert(&e.group)
                || e.hostname.len() > 253
                || !e.hostname.contains('.')
                || e.hostname.split('.').any(|s| {
                    s.is_empty()
                        || s.starts_with('-')
                        || s.ends_with('-')
                        || !s
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                })
            {
                return Err(E::Scope);
            }
            let suffix = if a.cluster.partition()
                == crate::controller_keys::worker_auth::aws_fargate::Partition::AwsCn
            {
                "amazonaws.com.cn"
            } else {
                "amazonaws.com"
            };
            let expected = match e.purpose {
                EndpointPurpose::Controller => controller.clone(),
                EndpointPurpose::Artifacts => artifacts.clone(),
                EndpointPurpose::SecretsManager => {
                    format!("secretsmanager.{}.{suffix}", a.cluster.region())
                }
                EndpointPurpose::EcrApi => format!("api.ecr.{}.{suffix}", a.cluster.region()),
                EndpointPurpose::EcrRegistry => format!(
                    "{}.dkr.ecr.{}.{suffix}",
                    a.cluster.account().as_str(),
                    a.cluster.region()
                ),
                EndpointPurpose::Logs => format!("logs.{}.{suffix}", a.cluster.region()),
                EndpointPurpose::Dns => return Err(E::Scope),
            };
            if e.hostname != expected {
                return Err(E::Scope);
            }
            if self
                .egress
                .iter()
                .filter(|r| {
                    r.protocol == "tcp"
                        && r.from == 443
                        && r.to == 443
                        && r.destination_group == e.group
                })
                .count()
                != 1
            {
                return Err(E::Scope);
            }
        }
        // DNS resolver policy, routing, endpoint policies, S3 image layers, NACLs
        // and effective egress remain mandatory external qualification domains.
        Ok(())
    }
}
fn group(s: &str) -> bool {
    s.strip_prefix("sg-").is_some_and(|v| {
        (v.len() == 8 || v.len() == 17)
            && v.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

fn origin_host(value: &str) -> Result<String, E> {
    let u = url::Url::parse(value).map_err(|_| E::InvalidInput)?;
    if u.scheme() != "https"
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
        || u.port().is_some()
        || u.origin().ascii_serialization() != value
    {
        return Err(E::Scope);
    }
    u.host_str().map(String::from).ok_or(E::Scope)
}
