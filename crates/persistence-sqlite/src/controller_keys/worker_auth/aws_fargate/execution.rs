//! Durable ECS control composed with R08. Configuration and receipts do not
//! establish effective IAM isolation or an enforceable AWS spending ceiling.
//! Production launch/stop stay hard-disabled until separately verified activation.
mod cost;
mod model;
mod sdk;
mod store;
mod task_loader;
use super::{AwsIdentityError as E, store::AccountRecord, *};
use crate::controller_keys::ControllerBootstrap;
use crate::{
    RequestId,
    admission::{Clock, LeaseRef, SchedulerPolicy},
    finance::EvidenceRef,
    worker_control::{
        self, LaunchAdapter, LaunchAuthorization, LaunchCommand, LaunchResult, ProviderObservation,
        ProviderState,
    },
};
pub use cost::{ExposureEnvelope, UnitRate};
#[cfg(test)]
use model::digest;
pub use model::{BootstrapAllocation, ExecutionStatus, LaunchRequest, StopReason};
use model::{encode, operation_token};
use sqlx::{Row, SqliteConnection};
use std::sync::Arc;
pub use task_loader::TaskBootstrapLoader;
/// Provider-neutral lifecycle complement to the existing R08 LaunchAdapter.
/// Stop is derived from controller state, never worker-supplied authority.
pub trait ExecutionAdapter: LaunchAdapter {
    fn stop(
        &self,
        lease: &LeaseRef,
        reason: StopReason,
    ) -> impl std::future::Future<Output = Result<ExecutionStatus, E>> + Send;
    fn request_cleanup(
        &self,
        lease: &LeaseRef,
    ) -> impl std::future::Future<Output = Result<ExecutionStatus, E>> + Send;
}
/// Opening always records reconciliation-required. No production activation
/// setter or caller-installable success authority is exposed.
pub struct FargateExecution {
    identity: Arc<FargateIdentity>,
    boot: String,
    #[cfg(test)]
    fixture: Option<Arc<fixture::World>>,
    #[cfg(test)]
    hook: Box<dyn Fn(Checkpoint) -> Result<(), E> + Send + Sync>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Checkpoint {
    BeforePrepare,
    AfterPrepare,
    BeforeSubmit,
    AfterSubmit,
    AfterEffect,
    BeforeReceipt,
    AfterReceipt,
    BeforeStop,
    AfterStop,
    AfterStopEffect,
}
impl FargateExecution {
    pub async fn open(b: &ControllerBootstrap, p: Arc<dyn SchedulerPolicy>) -> Result<Self, E> {
        let i = FargateIdentity::open(b, p).await?;
        Self::from_identity(i).await
    }
    async fn from_identity(identity: Arc<FargateIdentity>) -> Result<Self, E> {
        let boot = super::super::new_id().map_err(super::store::auth_error)?;
        let mut tx = identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let now = identity.now()?;
        let installation = identity.guard.controller().installation;
        let mac = identity.guard.mac(
            b"aws-execution-boot",
            &[boot.as_bytes(), installation.as_bytes(), &now.to_be_bytes()],
        );
        one(
            sqlx::query("INSERT INTO aws_execution_recovery VALUES(?,?,?,?)")
                .bind(&boot)
                .bind(installation)
                .bind(now)
                .bind(mac.as_slice())
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        tx.commit().await?;
        Ok(Self {
            identity,
            boot,
            #[cfg(test)]
            fixture: None,
            #[cfg(test)]
            hook: Box::new(|_| Ok(())),
        })
    }
    fn checkpoint(&self, p: Checkpoint) -> Result<(), E> {
        #[cfg(test)]
        (self.hook)(p)?;
        let _ = p;
        Ok(())
    }
    fn activation(&self, r: &LaunchRequest, now: i64) -> Result<(), E> {
        #[cfg(test)]
        if let Some(f) = &self.fixture {
            return f.current(r, now, &self.boot);
        }
        let _ = (r, now, &self.boot);
        Err(E::LiveLaunchDisabled)
    }
    /// Create an immutable request only from an R08 command and currently registered
    /// account. Supplied price data/locators confer no launch authority.
    pub async fn prepare(
        &self,
        lease: &LeaseRef,
        account: &EvidenceRef,
        bootstrap: &BootstrapAllocation,
        exposure: &ExposureEnvelope,
    ) -> Result<LaunchRequest, E> {
        let mut tx = self
            .identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let now = self.identity.now()?;
        let (cmd, _, _) = worker_control::command(&mut tx, lease)
            .await
            .map_err(worker_error)?;
        worker_control::current(&mut tx, &cmd, self.identity.guard.policy.as_ref(), now)
            .await
            .map_err(worker_error)?;
        let a = self
            .identity
            .account(&mut tx, &lease.task.plan.project_id, account)
            .await?;
        let admission = crate::admission::live(&mut tx, lease, now)
            .await
            .map_err(|_| E::Revoked)?;
        if admission.permit.provider.as_str() != AWS_PROVIDER
            || admission.permit.account != *account
            || a.revoked
            || now >= a.valid_until
        {
            return Err(E::Revoked);
        }
        if let Some(old) = self.load_optional(&mut tx, lease).await? {
            if old.account_ref != *account
                || old.bootstrap != *bootstrap
                || old.exposure != *exposure
            {
                return Err(E::Conflict);
            }
            self.authority(&mut tx, &old, &cmd, now).await?;
            tx.commit().await?;
            return Ok(old);
        }
        let r = LaunchRequest {
            format: 1,
            installation: self.identity.guard.controller().installation,
            account_ref: account.clone(),
            account_version: a.version,
            binding: cmd.binding().clone(),
            command_digest: rolvya_domain::worker::Digest::new(cmd.digest().map_err(worker_error)?)
                .map_err(|_| E::Integrity)?,
            workload: a.workload.clone(),
            bootstrap: bootstrap.clone(),
            exposure: exposure.clone(),
            maximum: cmd.reserved(),
            financial_event: cmd.financial_event(),
            client_token: operation_token(
                &a.installation,
                account,
                a.version,
                cmd.binding(),
                &rolvya_domain::worker::Digest::new(cmd.digest().map_err(worker_error)?)
                    .map_err(|_| E::Integrity)?,
                &a.workload,
            )?,
            authorized_at: now,
            launch_until: bootstrap
                .expires_at
                .min(a.valid_until)
                .min(admission.permit.valid_until)
                .min(now.checked_add(90).ok_or(E::InvalidInput)?),
            authorization_until: a
                .valid_until
                .min(admission.permit.valid_until)
                .min(exposure.valid_until)
                .min(
                    now.checked_add(
                        i64::from(a.workload.runtime_seconds)
                            + i64::from(exposure.provisioning_seconds)
                            + i64::from(exposure.termination_seconds),
                    )
                    .ok_or(E::InvalidInput)?,
                ),
        };
        r.validate(now)?;
        self.activation(&r, now)?;
        self.authority(&mut tx, &r, &cmd, now).await?;
        let text = encode(&r)?;
        let mac = super::consent::mac(
            &self.identity.guard.key,
            b"launch-request",
            &[text.as_bytes()],
        );
        one(
            sqlx::query("INSERT INTO aws_launch_requests VALUES(?,?,?,?,?,?,?,?,?,?,?)")
                .bind(lease.task.plan.project_id.as_str())
                .bind(lease.attempt_id.as_str())
                .bind(account.as_str())
                .bind(a.version as i64)
                .bind(&r.client_token)
                .bind(r.digest()?)
                .bind(text)
                .bind(mac.as_slice())
                .bind(r.workload.task_role.as_str())
                .bind(r.bootstrap.secret.as_str())
                .bind(now)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        self.event(
            &mut tx,
            &r,
            "prepared",
            None,
            EcsLifecycle::Unknown,
            false,
            false,
            false,
            now,
        )
        .await?;
        self.checkpoint(Checkpoint::BeforePrepare)?;
        self.authority(&mut tx, &r, &cmd, self.identity.now()?)
            .await?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterPrepare)?;
        Ok(r)
    }
    async fn authority(
        &self,
        c: &mut SqliteConnection,
        r: &LaunchRequest,
        cmd: &LaunchCommand,
        now: i64,
    ) -> Result<AccountRecord, E> {
        r.validate(now)?;
        self.activation(r, now)?;
        if r.binding != *cmd.binding()
            || r.command_digest.as_str() != cmd.digest().map_err(worker_error)?
            || r.maximum != cmd.reserved()
            || r.financial_event != cmd.financial_event()
        {
            return Err(E::ScopeMismatch);
        }
        if u64::from(cmd.profile().resources.cpu_millis)
            > u64::from(r.workload.cpu_units) * 1000 / 1024
            || cmd.profile().resources.memory_mib > r.workload.memory_mib
            || r.workload.runtime_seconds > cmd.profile().resources.timeout_seconds
        {
            return Err(E::IsolationMismatch);
        }
        worker_control::current(c, cmd, self.identity.guard.policy.as_ref(), now)
            .await
            .map_err(worker_error)?;
        let a = self
            .identity
            .account(c, &r.binding.task.plan.project_id, &r.account_ref)
            .await?;
        if a.installation != r.installation
            || a.version != r.account_version
            || a.workload != r.workload
            || a.revoked
            || now >= a.valid_until
        {
            return Err(E::Revoked);
        }
        self.identity
            .credential_reference(c, &a.project, &a.workload, &a.credential)
            .await?;
        #[cfg(test)]
        if self
            .fixture
            .as_ref()
            .is_some_and(|f| !f.use_broker.load(std::sync::atomic::Ordering::SeqCst))
        {
            return Ok(a);
        }
        let approved =
            super::consent::current(c, &self.identity.guard.key, &a.project, &a.account_ref, now)
                .await?;
        if approved.enrollment.workload != r.workload
            || approved.enrollment.expected_version + 1 != r.account_version
        {
            return Err(E::Revoked);
        }
        super::trusted::load_check(c, &self.identity.guard.key, &approved, now).await?;
        Ok(a)
    }
    async fn client(
        &self,
        r: &LaunchRequest,
        op: super::credential_use::InfrastructureOperation,
    ) -> Result<sdk::EcsSdk, E> {
        #[cfg(test)]
        if let Some(f) = &self.fixture
            && !f.use_broker.load(std::sync::atomic::Ordering::SeqCst)
        {
            return Ok(sdk::EcsSdk::new(
                r,
                super::sdk::tests::credentials(),
                Some(f.http(r)),
            ));
        }
        let now = self.identity.now()?;
        let mut tx = self
            .identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let request = RequestId::new(format!(
            "aws-control-{}",
            super::super::new_id().map_err(super::store::auth_error)?
        ))
        .map_err(|_| E::InvalidInput)?;
        let scope = super::credential_use::VerifiedInfrastructureUse::authorize(
            &mut tx,
            &self.identity.guard.key,
            &r.binding.task.plan.project_id,
            &r.account_ref,
            &request,
            op,
            now,
        )
        .await?;
        let loaded = crate::controller_keys::secret_broker::aws_consumer::consume(
            &mut tx,
            &self.identity.guard.key,
            &scope,
            now,
        )
        .await?;
        tx.commit().await?;
        Ok(sdk::EcsSdk::new(
            r,
            loaded.credentials,
            #[cfg(test)]
            self.fixture.as_ref().map(|f| f.http(r)),
        ))
    }
    pub async fn status(&self, lease: &LeaseRef) -> Result<ExecutionStatus, E> {
        let mut tx = self.identity.guard.store.pool.begin().await?;
        let r = self.load(&mut tx, lease).await?;
        let s = self.latest(&mut tx, &r).await?;
        tx.commit().await?;
        Ok(s)
    }
    /// Reconciliation is read-only at AWS, and never releases a financial hold.
    pub async fn reconcile(&self, lease: &LeaseRef) -> Result<ExecutionStatus, E> {
        let mut tx = self.identity.guard.store.pool.begin().await?;
        let r = self.load(&mut tx, lease).await?;
        let old = self.latest(&mut tx, &r).await?;
        tx.commit().await?;
        let sdk = self
            .client(
                &r,
                super::credential_use::InfrastructureOperation::ReconcileExecution,
            )
            .await?;
        let discovered = if let Some(t) = old.task.clone() {
            Some(t)
        } else {
            let tasks = sdk.discover(&r).await?;
            if tasks.len() == 1 {
                tasks.into_iter().next()
            } else {
                None
            }
        };
        let lifecycle = if let Some(t) = &discovered {
            sdk.inspect(&r, t).await.unwrap_or(EcsLifecycle::Unknown)
        } else {
            EcsLifecycle::Unknown
        };
        let mut tx = self
            .identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let current = self.latest(&mut tx, &r).await?;
        if current.revision != old.revision {
            return Err(E::Conflict);
        }
        // A later query must not resurrect a terminal observation, or substitute tasks.
        let conflict = (old.lifecycle == EcsLifecycle::Stopped
            && lifecycle != EcsLifecycle::Stopped)
            || (old.task.is_some() && old.task != discovered);
        let state = if conflict {
            EcsLifecycle::Unknown
        } else {
            lifecycle
        };
        let s = self
            .event(
                &mut tx,
                &r,
                "inspection",
                discovered,
                state,
                old.unknown_exposure || conflict || state == EcsLifecycle::Unknown,
                old.stop_requested,
                old.cleanup_requested,
                self.identity.now()?,
            )
            .await?;
        if s.unknown_exposure {
            self.mark_unknown(&mut tx, &r, &s).await?
        }
        tx.commit().await?;
        Ok(s)
    }
}
fn one(n: u64) -> Result<(), E> {
    if n == 1 { Ok(()) } else { Err(E::Storage) }
}
fn worker_error(e: crate::worker_simulation::WorkerError) -> E {
    use crate::worker_simulation::WorkerError as W;
    match e {
        W::FinancialRisk => E::FinancialRisk,
        W::ScopeMismatch => E::ScopeMismatch,
        _ => E::Revoked,
    }
}
#[cfg(test)]
pub(in crate::controller_keys::worker_auth) mod fixture;

pub(super) use store::verify_receipt;
