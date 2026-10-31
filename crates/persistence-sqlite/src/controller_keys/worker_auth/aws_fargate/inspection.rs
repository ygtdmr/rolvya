//! Interpret official SDK response types. A correlated observation remains data,
//! never a worker-credential grant or proof of final billing/real isolation.
use super::{AwsIdentityError as E, *};
use aws_sdk_ecs::{
    operation::describe_tasks::DescribeTasksOutput,
    types::{Task, TaskDefinition},
};
use aws_sdk_sts::operation::get_caller_identity::GetCallerIdentityOutput;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionGate {
    ControllerCredentialConsent,
    TaskPossession,
    ChallengeTlsBinding,
    ProtectedBootstrapDelivery,
    EffectiveIamPolicy,
    EffectiveNetworkIsolation,
    RuntimeCostEnforcement,
    RealCloudValidation,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionAssessment {
    pub task: TaskArn,
    pub lifecycle: EcsLifecycle,
    pub observed_at: i64,
    pub unknown_financial_exposure: bool,
    pub configuration_correlated: bool,
    pub remaining_gates: Vec<ProductionGate>,
}
impl InspectionAssessment {
    /// Inspecting an absent/stopped task is not proof of no launch or final cost.
    pub fn worker_issuance(&self) -> Result<(), E> {
        Err(match self.lifecycle {
            EcsLifecycle::Running => E::TaskPossessionUnverified,
            EcsLifecycle::Stopped | EcsLifecycle::Stopping => E::TaskStopped,
            EcsLifecycle::Unknown => E::UnknownExecution,
            _ => E::TaskNotRunning,
        })
    }
}
pub(super) fn controller_identity(
    expected: &FargateWorkload,
    id: &GetCallerIdentityOutput,
) -> Result<(), E> {
    if id.account() != Some(expected.cluster.account().as_str()) {
        return Err(E::AccountMismatch);
    }
    let principal = AssumedRoleArn::parse(id.arn().ok_or(E::UnverifiedControllerAuthority)?)?;
    let (role, session) = id
        .user_id()
        .and_then(|s| s.split_once(':'))
        .ok_or(E::UnverifiedControllerAuthority)?;
    if principal.partition() != expected.cluster.partition()
        || principal.account() != expected.cluster.account()
        || principal.role_name() != expected.controller_role.role_name()
        || role != expected.controller_role_id
        || session != principal.session_name()
    {
        return Err(E::UnverifiedControllerAuthority);
    }
    Ok(())
}
pub(super) fn task_record<'a>(
    expected: &ExpectedExecution,
    response: &'a DescribeTasksOutput,
) -> Result<&'a Task, E> {
    expected.validate()?;
    if !response.failures().is_empty() {
        if response
            .failures()
            .iter()
            .any(|f| f.arn() != Some(expected.task.as_str()))
        {
            return Err(E::ScopeMismatch);
        }
        return Err(
            if response
                .failures()
                .iter()
                .all(|f| f.reason() == Some("MISSING"))
            {
                E::TaskNotVisible
            } else {
                E::UnknownExecution
            },
        );
    }
    if response.tasks().is_empty() {
        return Err(E::TaskNotVisible);
    }
    if response.tasks().len() != 1 {
        return Err(E::ScopeMismatch);
    }
    let task = &response.tasks()[0];
    if task.task_arn() != Some(expected.task.as_str())
        || task.cluster_arn() != Some(expected.workload.cluster.as_str())
        || task.task_definition_arn() != Some(expected.workload.task_definition.as_str())
        || task.started_by() != Some(expected.started_by().as_str())
    {
        return Err(E::ScopeMismatch);
    }
    Ok(task)
}
pub(super) fn correlate(
    expected: &ExpectedExecution,
    task: &Task,
    definition: &TaskDefinition,
    now: i64,
) -> Result<InspectionAssessment, E> {
    correlate_inner(expected, task, definition, now, false)
}
pub(super) fn correlate_verifier(
    expected: &ExpectedExecution,
    task: &Task,
    definition: &TaskDefinition,
    now: i64,
) -> Result<InspectionAssessment, E> {
    correlate_inner(expected, task, definition, now, true)
}
fn correlate_inner(
    expected: &ExpectedExecution,
    task: &Task,
    definition: &TaskDefinition,
    now: i64,
    verifier: bool,
) -> Result<InspectionAssessment, E> {
    expected.validate()?;
    let w = &expected.workload;
    // Status reconciliation may outlive worker authorization. The separately
    // authorized inspection cannot issue/renew any worker credential.
    if now < expected.authorized_at {
        return Err(E::ExpiredEvidence);
    }
    if task.task_arn() != Some(expected.task.as_str())
        || task.cluster_arn() != Some(w.cluster.as_str())
        || task.task_definition_arn() != Some(w.task_definition.as_str())
    {
        return Err(E::ScopeMismatch);
    }
    let lifecycle =
        if task.desired_status() == Some("STOPPED") && task.last_status() != Some("STOPPED") {
            EcsLifecycle::Stopping
        } else {
            if task.last_status() == Some("RUNNING") && task.desired_status() != Some("RUNNING") {
                EcsLifecycle::Unknown
            } else {
                EcsLifecycle::from_status(task.last_status().unwrap_or(""))
            }
        };
    if lifecycle != EcsLifecycle::Running {
        return Ok(assessment(expected, lifecycle, now, false));
    }
    if task.launch_type().map(|t| t.as_str()) != Some("FARGATE")
        || task.platform_family() != Some("Linux")
        || task.platform_version() != Some(w.platform_version.as_str())
        || task.cpu().and_then(|n| n.parse::<u32>().ok()) != Some(w.cpu_units)
        || task.memory().and_then(|n| n.parse::<u32>().ok()) != Some(w.memory_mib)
        || task.enable_execute_command()
    {
        return Err(E::IsolationMismatch);
    }
    if definition.task_definition_arn() != Some(w.task_definition.as_str())
        || definition.status().map(|s| s.as_str()) != Some("ACTIVE")
        || definition.network_mode().map(|s| s.as_str()) != Some("awsvpc")
        || !definition
            .requires_compatibilities()
            .iter()
            .any(|t| t.as_str() == "FARGATE")
        || definition.cpu().and_then(|n| n.parse::<u32>().ok()) != Some(w.cpu_units)
        || definition.memory().and_then(|n| n.parse::<u32>().ok()) != Some(w.memory_mib)
    {
        return Err(E::TaskDefinitionMismatch);
    }
    if definition.task_role_arn() != Some(w.task_role.as_str())
        || definition.execution_role_arn() != Some(w.execution_role.as_str())
    {
        return Err(E::RoleMismatch);
    }
    if let Some(o) = task.overrides()
        && (o.task_role_arn().is_some_and(|r| r != w.task_role.as_str())
            || o.execution_role_arn()
                .is_some_and(|r| r != w.execution_role.as_str())
            || !o.container_overrides().is_empty())
    {
        return Err(E::RoleMismatch);
    }
    let runtime = definition.runtime_platform().ok_or(E::IsolationMismatch)?;
    if runtime.operating_system_family().map(|s| s.as_str()) != Some("LINUX")
        || runtime.cpu_architecture().map(|s| s.as_str()) != Some(w.architecture.as_str())
        || definition.pid_mode().is_some()
        || definition.ipc_mode().is_some()
        || (!verifier && !definition.volumes().is_empty())
    {
        return Err(E::IsolationMismatch);
    }
    let disk = definition
        .ephemeral_storage()
        .map(|s| s.size_in_gib())
        .unwrap_or(20);
    if disk != w.ephemeral_gib as i32
        || task
            .ephemeral_storage()
            .map(|s| s.size_in_gib())
            .unwrap_or(20)
            != disk
    {
        return Err(E::IsolationMismatch);
    }
    if definition.container_definitions().len() != 1 || task.containers().len() != 1 {
        return Err(E::IsolationMismatch);
    }
    let c = &definition.container_definitions()[0];
    let observed = &task.containers()[0];
    if c.name() != Some(w.container.as_str())
        || c.image() != Some(w.image.as_str())
        || observed.name() != Some(w.container.as_str())
        || observed.image_digest() != Some(w.image_digest.as_str())
        || observed.image() != Some(w.image.as_str())
    {
        return Err(E::ImageMismatch);
    }
    if observed.last_status() != Some("RUNNING") {
        return Err(E::TaskNotRunning);
    }
    if c.privileged() == Some(true)
        || c.readonly_root_filesystem() != Some(true)
        || c.user() != Some(w.user.as_str())
        || !c.entry_point().is_empty()
        || c.command() != w.command
        || !c.port_mappings().is_empty()
        || (!verifier && !c.mount_points().is_empty())
        || !c.volumes_from().is_empty()
        || !c.secrets().is_empty()
        || !c.environment().is_empty()
        || !c.environment_files().is_empty()
        || c.repository_credentials().is_some()
        || c.firelens_configuration().is_some()
    {
        return Err(E::IsolationMismatch);
    }
    if verifier && !verifier_workspace(definition, c) {
        return Err(E::IsolationMismatch);
    }
    if let Some(linux) = c.linux_parameters()
        && (!linux.devices().is_empty()
            || !linux.tmpfs().is_empty()
            || linux.shared_memory_size().is_some()
            || linux
                .capabilities()
                .is_some_and(|caps| !caps.add().is_empty()))
    {
        return Err(E::IsolationMismatch);
    }
    let subnet: Vec<_> = task
        .attachments()
        .iter()
        .filter(|a| a.r#type() == Some("ElasticNetworkInterface"))
        .flat_map(|a| a.details())
        .filter(|d| d.name() == Some("subnetId"))
        .filter_map(|d| d.value())
        .collect();
    let interface: Vec<_> = task
        .attachments()
        .iter()
        .filter(|a| a.r#type() == Some("ElasticNetworkInterface"))
        .flat_map(|a| a.details())
        .filter(|d| d.name() == Some("networkInterfaceId"))
        .filter_map(|d| d.value())
        .collect();
    if subnet.len() != 1
        || !w.subnets.iter().any(|s| s == subnet[0])
        || interface.len() != 1
        || !interface[0].starts_with("eni-")
    {
        return Err(E::IsolationMismatch);
    }
    Ok(assessment(expected, lifecycle, now, true))
}
fn assessment(
    expected: &ExpectedExecution,
    lifecycle: EcsLifecycle,
    now: i64,
    configuration_correlated: bool,
) -> InspectionAssessment {
    InspectionAssessment {
        task: expected.task.clone(),
        lifecycle,
        observed_at: now,
        unknown_financial_exposure: true,
        configuration_correlated,
        remaining_gates: vec![
            ProductionGate::TaskPossession,
            ProductionGate::ChallengeTlsBinding,
            ProductionGate::ProtectedBootstrapDelivery,
            ProductionGate::EffectiveIamPolicy,
            ProductionGate::EffectiveNetworkIsolation,
            ProductionGate::RuntimeCostEnforcement,
            ProductionGate::RealCloudValidation,
        ],
    }
}

/// Fargate-supported ephemeral bind volume only; no host/EFS/FSx, extra mounts
/// or tmpfs assumptions. This validates configuration, not effective isolation.
pub(super) fn verifier_workspace(
    d: &TaskDefinition,
    c: &aws_sdk_ecs::types::ContainerDefinition,
) -> bool {
    let volumes = d.volumes();
    let mounts = c.mount_points();
    volumes.len() == 1
        && volumes[0].name() == Some("verifier-work")
        && volumes[0].host().is_none()
        && volumes[0].efs_volume_configuration().is_none()
        && volumes[0]
            .fsx_windows_file_server_volume_configuration()
            .is_none()
        && volumes[0].docker_volume_configuration().is_none()
        && mounts.len() == 1
        && mounts[0].source_volume() == Some("verifier-work")
        && mounts[0].container_path() == Some("/workspace")
        && mounts[0].read_only() != Some(true)
        && c.linux_parameters().is_some_and(|p| {
            p.init_process_enabled() == Some(true)
                && p.capabilities()
                    .is_some_and(|v| v.add().is_empty() && v.drop() == ["ALL"])
        })
}
