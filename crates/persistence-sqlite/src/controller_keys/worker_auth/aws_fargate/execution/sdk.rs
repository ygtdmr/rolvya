//! Actual AWS SDK serialization, signing and response decoding. There is no
//! production mutating connector: only private cfg(test) protocol fixtures can
//! supply a transport. No ambient credentials or provider-managed retries.
use super::*;
use aws_sdk_ecs::{
    config::{BehaviorVersion, Region, retry::RetryConfig, timeout::TimeoutConfig},
    types::*,
};
use aws_smithy_runtime_api::client::{
    http::{HttpConnector, HttpConnectorFuture, SharedHttpConnector, http_client_fn},
    orchestrator::HttpRequest,
    result::ConnectorError,
};
use std::time::Duration;
#[derive(Debug)]
struct Disabled;
impl HttpConnector for Disabled {
    fn call(&self, _: HttpRequest) -> HttpConnectorFuture {
        HttpConnectorFuture::new(async {
            Err(ConnectorError::user(Box::new(E::LiveLaunchDisabled)))
        })
    }
}
pub(super) struct EcsSdk {
    client: aws_sdk_ecs::Client,
}
impl EcsSdk {
    pub(super) fn new(
        r: &LaunchRequest,
        credentials: aws_credential_types::Credentials,
        #[cfg(test)] http: Option<aws_smithy_runtime_api::client::http::SharedHttpClient>,
    ) -> Self {
        let connector = SharedHttpConnector::new(Disabled);
        let transport = http_client_fn(move |_, _| connector.clone());
        #[cfg(test)]
        let transport = http.unwrap_or(transport);
        let conf = aws_sdk_ecs::Config::builder()
            .behavior_version(BehaviorVersion::v2026_01_12())
            .region(Region::new(r.workload.cluster.region().to_owned()))
            .credentials_provider(credentials)
            .http_client(transport)
            .retry_config(RetryConfig::standard().with_max_attempts(1))
            .timeout_config(
                TimeoutConfig::builder()
                    .connect_timeout(Duration::from_secs(2))
                    .operation_attempt_timeout(Duration::from_secs(4))
                    .operation_timeout(Duration::from_secs(4))
                    .build(),
            )
            .build();
        Self {
            client: aws_sdk_ecs::Client::from_conf(conf),
        }
    }
    pub(super) async fn validate_workload(&self, r: &LaunchRequest) -> Result<(), E> {
        let out = self
            .client
            .describe_task_definition()
            .task_definition(r.workload.task_definition.as_str())
            .send()
            .await
            .map_err(|_| E::UnknownExecution)?;
        let d = out.task_definition().ok_or(E::TaskDefinitionMismatch)?;
        let w = &r.workload;
        if d.task_definition_arn() != Some(w.task_definition.as_str())
            || d.status().map(|s| s.as_str()) != Some("ACTIVE")
            || d.network_mode().map(|v| v.as_str()) != Some("awsvpc")
            || !d
                .requires_compatibilities()
                .iter()
                .any(|v| v.as_str() == "FARGATE")
            || d.cpu() != Some(w.cpu_units.to_string().as_str())
            || d.memory() != Some(w.memory_mib.to_string().as_str())
            || d.ephemeral_storage().map(|s| s.size_in_gib()).unwrap_or(20)
                != w.ephemeral_gib as i32
        {
            return Err(E::TaskDefinitionMismatch);
        }
        if d.task_role_arn() != Some(w.task_role.as_str())
            || d.execution_role_arn() != Some(w.execution_role.as_str())
        {
            return Err(E::RoleMismatch);
        }
        if d.pid_mode().is_some()
            || d.ipc_mode().is_some()
            || !d.volumes().is_empty()
            || d.runtime_platform().is_none_or(|p| {
                p.operating_system_family().map(|s| s.as_str()) != Some("LINUX")
                    || p.cpu_architecture().map(|s| s.as_str()) != Some(w.architecture.as_str())
            })
            || d.container_definitions().len() != 1
        {
            return Err(E::IsolationMismatch);
        }
        let c = &d.container_definitions()[0];
        if c.name() != Some(w.container.as_str())
            || c.image() != Some(w.image.as_str())
            || c.user() != Some(w.user.as_str())
            || c.privileged() == Some(true)
            || c.readonly_root_filesystem() != Some(true)
            || !c.entry_point().is_empty()
            || c.command() != w.command
            || !c.port_mappings().is_empty()
            || !c.mount_points().is_empty()
            || !c.volumes_from().is_empty()
            || !c.secrets().is_empty()
            || !c.environment().is_empty()
            || !c.environment_files().is_empty()
            || c.repository_credentials().is_some()
            || c.firelens_configuration().is_some()
            || c.linux_parameters().is_some_and(|p| {
                !p.devices().is_empty()
                    || !p.tmpfs().is_empty()
                    || p.shared_memory_size().is_some()
                    || p.capabilities().is_some_and(|c| !c.add().is_empty())
            })
        {
            return Err(E::IsolationMismatch);
        }
        Ok(())
    }
    pub(super) async fn launch(&self, r: &LaunchRequest) -> Result<RunReceipt, E> {
        let w = &r.workload;
        let network = NetworkConfiguration::builder()
            .awsvpc_configuration(
                AwsVpcConfiguration::builder()
                    .set_subnets(Some(w.subnets.clone()))
                    .set_security_groups(Some(w.security_groups.clone()))
                    .assign_public_ip(AssignPublicIp::Disabled)
                    .build()
                    .map_err(|_| E::IsolationMismatch)?,
            )
            .build();
        let response = self
            .client
            .run_task()
            .cluster(w.cluster.as_str())
            .task_definition(w.task_definition.as_str())
            .count(1)
            .launch_type(LaunchType::Fargate)
            .platform_version(&w.platform_version)
            .client_token(&r.client_token)
            .started_by(r.started_by())
            .network_configuration(network)
            .enable_execute_command(false)
            .enable_ecs_managed_tags(false)
            .send()
            .await
            .map_err(|_| E::UnknownExecution)?;
        // Mixed success/failure is ambiguous. Never convert a possibly created task
        // into a rejection or authorize another launch.
        let returned: Vec<String> = response
            .tasks()
            .iter()
            .filter_map(|t| t.task_arn().map(str::to_owned))
            .take(100)
            .collect();
        let failures: Vec<(Option<String>, Option<String>)> = response
            .failures()
            .iter()
            .map(|f| (f.arn().map(str::to_owned), f.reason().map(str::to_owned)))
            .take(100)
            .collect();
        if returned.iter().any(|s| s.len() > 2048)
            || failures.iter().any(|(a, b)| {
                a.as_ref().is_some_and(|v| v.len() > 2048)
                    || b.as_ref().is_some_and(|v| v.len() > 512)
            })
        {
            return Err(E::ResourceLimit);
        }
        let task = if response.failures().is_empty() && response.tasks().len() == 1 {
            task_identity(r, &response.tasks()[0]).ok()
        } else {
            None
        };
        Ok(RunReceipt {
            task,
            returned_tasks: returned,
            failures,
        })
    }
    pub(super) async fn inspect(
        &self,
        r: &LaunchRequest,
        task: &TaskArn,
    ) -> Result<EcsLifecycle, E> {
        let out = self
            .client
            .describe_tasks()
            .cluster(r.workload.cluster.as_str())
            .tasks(task.as_str())
            .send()
            .await
            .map_err(|_| E::UnknownExecution)?;
        if out.tasks().len() != 1 || !out.failures().is_empty() {
            return Err(E::TaskNotVisible);
        }
        let t = &out.tasks()[0];
        if task_identity(r, t)? != *task {
            return Err(E::ScopeMismatch);
        }
        Ok(EcsLifecycle::from_status(t.last_status().unwrap_or("")))
    }
    pub(super) async fn discover(&self, r: &LaunchRequest) -> Result<Vec<TaskArn>, E> {
        let mut token = None;
        let mut result = Vec::new();
        let mut pages = std::collections::BTreeSet::new();
        for _ in 0..10 {
            // startedBy must be the sole filter. Cluster is explicit routing scope. Empty
            // listing is NOT proof of absence or safe retry under eventual consistency.
            let out = self
                .client
                .list_tasks()
                .cluster(r.workload.cluster.as_str())
                .started_by(r.started_by())
                .set_next_token(token)
                .max_results(100)
                .send()
                .await
                .map_err(|_| E::UnknownExecution)?;
            for a in out.task_arns() {
                let a = TaskArn::parse(a)?;
                check_task_scope(r, &a)?;
                if !result.contains(&a) {
                    result.push(a)
                }
            }
            token = out.next_token().map(str::to_owned);
            match &token {
                None => return Ok(result),
                Some(v) if pages.insert(v.clone()) => {}
                _ => return Err(E::UnknownExecution),
            }
        }
        Err(E::ResourceLimit)
    }
    pub(super) async fn stop(
        &self,
        r: &LaunchRequest,
        task: &TaskArn,
        reason: StopReason,
    ) -> Result<(), E> {
        check_task_scope(r, task)?;
        let out = self
            .client
            .stop_task()
            .cluster(r.workload.cluster.as_str())
            .task(task.as_str())
            .reason(format!("Rolvya controller: {reason:?}"))
            .send()
            .await
            .map_err(|_| E::UnknownExecution)?;
        let actual = out.task().ok_or(E::UnknownExecution)?;
        if task_identity(r, actual)? != *task {
            return Err(E::ScopeMismatch);
        }
        Ok(())
    }
}
fn check_task_scope(r: &LaunchRequest, t: &TaskArn) -> Result<(), E> {
    let c = &r.workload.cluster;
    if t.account() != c.account()
        || t.partition() != c.partition()
        || t.region() != c.region()
        || t.cluster_name() != c.name()
    {
        Err(E::ScopeMismatch)
    } else {
        Ok(())
    }
}
fn task_identity(r: &LaunchRequest, t: &Task) -> Result<TaskArn, E> {
    let task = TaskArn::parse(t.task_arn().ok_or(E::UnknownExecution)?)?;
    check_task_scope(r, &task)?;
    if t.cluster_arn() != Some(r.workload.cluster.as_str())
        || t.task_definition_arn() != Some(r.workload.task_definition.as_str())
        || t.started_by() != Some(r.started_by().as_str())
        || t.launch_type() != Some(&LaunchType::Fargate)
    {
        return Err(E::ScopeMismatch);
    }
    Ok(task)
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RunReceipt {
    pub task: Option<TaskArn>,
    pub returned_tasks: Vec<String>,
    pub failures: Vec<(Option<String>, Option<String>)>,
}
