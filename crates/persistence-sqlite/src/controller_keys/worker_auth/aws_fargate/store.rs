use super::{
    super::{WorkerAuthError, WorkerSecurity},
    AwsIdentityError as E, *,
};
use crate::{
    RequestId,
    admission::SchedulerPolicy,
    controller_keys::{
        ControllerBootstrap, provider_accounts::CredentialReference, secret_broker as broker,
    },
    finance::EvidenceRef,
    worker_control,
};
use rolvya_domain::ids::{AttemptId, ProjectId};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqliteConnection};
use std::{future::Future, pin::Pin, sync::Arc};

pub const AWS_PROVIDER: &str = "aws-ecs-fargate";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AccountEnrollment {
    pub project: ProjectId,
    pub account_ref: EvidenceRef,
    pub request_id: RequestId,
    pub expected_version: u64,
    pub workload: FargateWorkload,
    pub credential: CredentialReference,
    pub consent_reference: EvidenceRef,
    pub valid_until: i64,
    pub revoked: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExecutionRegistration {
    pub expected: ExpectedExecution,
    pub request_id: RequestId,
    pub receipt_reference: EvidenceRef,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum Origin {
    ReviewedControllerRegistration,
    #[cfg(test)]
    SyntheticAwsProtocolFixture,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct AccountRecord {
    pub format: u16,
    pub installation: String,
    pub project: ProjectId,
    pub account_ref: EvidenceRef,
    pub version: u64,
    pub request_id: RequestId,
    pub workload: FargateWorkload,
    pub credential: CredentialReference,
    pub consent_reference: EvidenceRef,
    pub actor: EvidenceRef,
    pub valid_until: i64,
    pub revoked: bool,
    origin: Origin,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ExecutionRecord {
    format: u16,
    pub(super) expected: ExpectedExecution,
    request_id: RequestId,
    receipt_reference: EvidenceRef,
    actor: EvidenceRef,
    origin: Origin,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InspectionRecord {
    pub format: u16,
    pub project: ProjectId,
    pub attempt: AttemptId,
    pub revision: u64,
    pub request_id: RequestId,
    pub observed_at: i64,
    pub account_ref: EvidenceRef,
    pub account_version: u64,
    pub assessment: Result<InspectionAssessment, E>,
    pub control_denial: Option<E>,
}
impl InspectionRecord {
    pub fn worker_issuance(&self) -> Result<(), E> {
        if let Some(e) = self.control_denial {
            return Err(e);
        }
        match &self.assessment {
            Ok(a) => a.worker_issuance(),
            Err(e) => Err(*e),
        }
    }
}
pub(super) enum AdministrativeOperation<'a> {
    Enroll(&'a AccountEnrollment),
    Bind(&'a ExecutionRegistration),
    Inspect {
        account: &'a AccountRecord,
        execution: &'a ExpectedExecution,
        request: &'a RequestId,
    },
}
pub(super) type Authorization<'a> =
    Pin<Box<dyn Future<Output = Result<EvidenceRef, E>> + Send + 'a>>;
pub(super) struct ResolvedCredentials {
    pub(super) account_ref: EvidenceRef,
    pub(super) account_version: u64,
    pub(super) reference: CredentialReference,
    pub(super) expires_at: i64,
    pub(super) credentials: aws_credential_types::Credentials,
    #[cfg(test)]
    pub(super) http: Option<aws_smithy_runtime_api::client::http::SharedHttpClient>,
}
pub(super) type CredentialLoad<'a> =
    Pin<Box<dyn Future<Output = Result<ResolvedCredentials, E>> + Send + 'a>>;
/// Sealed controller boundary. authorize_in is bounded SQLite work only, never
/// AWS/email/network I/O. No public caller can install its own success provider.
pub(super) trait InfrastructureAuthority: Send + Sync {
    fn authorize_in<'a>(
        &'a self,
        c: &'a mut SqliteConnection,
        op: AdministrativeOperation<'a>,
        now: i64,
    ) -> Authorization<'a>;
    #[cfg(test)]
    fn controller_http(&self) -> Option<aws_smithy_runtime_api::client::http::SharedHttpClient> {
        None
    }
    fn credentials<'a>(
        &'a self,
        account: &'a AccountRecord,
        execution: &'a ExpectedExecution,
        now: i64,
    ) -> CredentialLoad<'a>;
}
pub struct FargateIdentity {
    pub(super) guard: WorkerSecurity,
    pub(super) authority: Arc<dyn InfrastructureAuthority>,
    #[cfg(test)]
    pub(super) hook: Box<dyn Fn(AwsCheckpoint) -> Result<(), E> + Send + Sync>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AwsCheckpoint {
    BeforeAccount,
    AfterAccount,
    BeforeBinding,
    AfterBinding,
    BeforeInspection,
    AfterInspection,
}
impl FargateIdentity {
    pub async fn open(
        b: &ControllerBootstrap,
        policy: Arc<dyn SchedulerPolicy>,
    ) -> Result<Arc<Self>, E> {
        let mut guard = WorkerSecurity::open(b).await.map_err(auth_error)?;
        guard.policy = policy;
        let authority =
            Arc::new(super::trusted::ReviewedInfrastructure::new(b, guard.clock.clone()).await?);
        Ok(Arc::new(Self {
            guard,
            authority,
            #[cfg(test)]
            hook: Box::new(|_| Ok(())),
        }))
    }
    pub(super) fn now(&self) -> Result<i64, E> {
        self.guard.clock.now().map_err(|_| E::ExpiredEvidence)
    }
    fn checkpoint(&self, p: AwsCheckpoint) -> Result<(), E> {
        #[cfg(test)]
        {
            (self.hook)(p)?;
        }
        let _ = p;
        Ok(())
    }
    pub async fn enroll_account(&self, r: &AccountEnrollment) -> Result<u64, E> {
        r.workload.validate()?;
        if r.expected_version >= i64::MAX as u64 {
            return Err(E::InvalidInput);
        }
        validate_credential(&r.workload, &r.credential)?;
        let mut tx = self.guard.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let now = self.now()?;
        if r.valid_until <= now {
            return Err(E::ExpiredEvidence);
        }
        let actor = self
            .authority
            .authorize_in(&mut tx, AdministrativeOperation::Enroll(r), now)
            .await?;
        let old = self
            .account_optional(&mut tx, &r.project, &r.account_ref)
            .await?;
        if let Some(old) = &old
            && old.request_id == r.request_id
        {
            let mut expected = old.clone();
            expected.version = r.expected_version + 1;
            expected.workload = r.workload.clone();
            expected.credential = r.credential.clone();
            expected.valid_until = r.valid_until;
            expected.revoked = r.revoked;
            expected.consent_reference = r.consent_reference.clone();
            if expected != *old {
                return Err(E::Conflict);
            }
            tx.commit().await?;
            return Ok(old.version);
        }
        if old.as_ref().map(|r| r.version).unwrap_or(0) != r.expected_version {
            return Err(E::Conflict);
        }
        self.credential_reference(&mut tx, &r.project, &r.workload, &r.credential)
            .await?;
        let version = r.expected_version + 1;
        let record = AccountRecord {
            format: 1,
            installation: self.guard.controller().installation,
            project: r.project.clone(),
            account_ref: r.account_ref.clone(),
            version,
            request_id: r.request_id.clone(),
            workload: r.workload.clone(),
            credential: r.credential.clone(),
            consent_reference: r.consent_reference.clone(),
            actor,
            valid_until: r.valid_until,
            revoked: r.revoked,
            origin: origin(),
        };
        let text = encode(&record, 32768)?;
        let mac = self.guard.mac(
            b"aws-account-record",
            &[
                r.project.as_str().as_bytes(),
                r.account_ref.as_str().as_bytes(),
                &version.to_be_bytes(),
                text.as_bytes(),
            ],
        );
        one(
            sqlx::query("INSERT INTO aws_infrastructure_account_versions VALUES(?,?,?,?,?,?)")
                .bind(r.project.as_str())
                .bind(r.account_ref.as_str())
                .bind(version as i64)
                .bind(text)
                .bind(mac.as_slice())
                .bind(now)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        if r.expected_version == 0 {
            one(
                sqlx::query("INSERT INTO aws_infrastructure_accounts VALUES(?,?,?)")
                    .bind(r.project.as_str())
                    .bind(r.account_ref.as_str())
                    .bind(version as i64)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
        } else {
            one(sqlx::query("UPDATE aws_infrastructure_accounts SET current_version=? WHERE project_id=? AND account_ref=? AND current_version=?").bind(version as i64).bind(r.project.as_str()).bind(r.account_ref.as_str()).bind(r.expected_version as i64).execute(&mut *tx).await?.rows_affected())?;
        }
        self.checkpoint(AwsCheckpoint::BeforeAccount)?;
        let end = self.now()?;
        if end < now || end >= r.valid_until {
            return Err(E::ExpiredEvidence);
        }
        let current_actor = self
            .authority
            .authorize_in(&mut tx, AdministrativeOperation::Enroll(r), end)
            .await?;
        if current_actor != record.actor {
            return Err(E::Revoked);
        }
        tx.commit().await?;
        self.checkpoint(AwsCheckpoint::AfterAccount)?;
        Ok(version)
    }
    pub async fn record_execution(&self, r: &ExecutionRegistration) -> Result<(), E> {
        r.expected.validate()?;
        if r.expected.operation_token != r.expected.expected_operation_token()?
            || r.expected.installation != self.guard.controller().installation
        {
            return Err(E::ScopeMismatch);
        }
        let mut tx = self.guard.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let now = self.now()?;
        let actor = self
            .authority
            .authorize_in(&mut tx, AdministrativeOperation::Bind(r), now)
            .await?;
        let account = self
            .account(
                &mut tx,
                &r.expected.binding.task.plan.project_id,
                &r.expected.account_ref,
            )
            .await?;
        self.account_current(&account, &r.expected, now)?;
        let scope = self
            .guard
            .scope(
                &mut tx,
                &r.expected.lease(),
                now,
                worker_control::CredentialScopeUse::Bootstrap,
            )
            .await
            .map_err(auth_error)?;
        if scope.provider.as_str() != AWS_PROVIDER
            || scope.account != r.expected.account_ref
            || scope.binding != r.expected.binding
            || scope.command_digest != r.expected.command_digest.as_str()
            || r.expected.authorization_until > scope.valid_until
            || u64::from(scope.profile.resources.cpu_millis)
                > u64::from(r.expected.workload.cpu_units) * 1000 / 1024
            || scope.profile.resources.memory_mib > r.expected.workload.memory_mib
            || r.expected.workload.runtime_seconds > scope.profile.resources.timeout_seconds
        {
            return Err(E::ScopeMismatch);
        }
        self.credential_reference(
            &mut tx,
            &account.project,
            &account.workload,
            &account.credential,
        )
        .await?;
        let record = ExecutionRecord {
            format: 1,
            expected: r.expected.clone(),
            request_id: r.request_id.clone(),
            receipt_reference: r.receipt_reference.clone(),
            actor,
            origin: origin(),
        };
        let text = encode(&record, 32768)?;
        let p = r.expected.binding.task.plan.project_id.as_str();
        let attempt = r.expected.binding.attempt_id.as_str();
        if let Some(old) = self
            .execution_optional(&mut tx, &r.expected.lease())
            .await?
        {
            if old != record {
                return Err(E::Conflict);
            }
            tx.commit().await?;
            return Ok(());
        }
        let mac = self.guard.mac(
            b"aws-execution-record",
            &[p.as_bytes(), attempt.as_bytes(), text.as_bytes()],
        );
        one(
            sqlx::query("INSERT INTO aws_execution_bindings VALUES(?,?,?,?,?,?,?,?,?)")
                .bind(p)
                .bind(attempt)
                .bind(account.account_ref.as_str())
                .bind(account.version as i64)
                .bind(r.expected.task.as_str())
                .bind(r.request_id.as_str())
                .bind(text)
                .bind(mac.as_slice())
                .bind(now)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        self.checkpoint(AwsCheckpoint::BeforeBinding)?;
        let end = self.now()?;
        self.guard
            .scope(
                &mut tx,
                &r.expected.lease(),
                end,
                worker_control::CredentialScopeUse::Bootstrap,
            )
            .await
            .map_err(auth_error)?;
        if end < now || end >= r.expected.authorization_until {
            return Err(E::ExpiredEvidence);
        }
        tx.commit().await?;
        self.checkpoint(AwsCheckpoint::AfterBinding)?;
        Ok(())
    }
    pub async fn inspect(
        &self,
        lease: &crate::admission::LeaseRef,
        request: &RequestId,
    ) -> Result<InspectionRecord, E> {
        let mut read = self.guard.store.pool.begin().await?;
        let now = self.now()?;
        let execution = self
            .execution_optional(&mut read, lease)
            .await?
            .ok_or(E::UnknownExecution)?;
        let expected = &execution.expected;
        let account = self
            .account(
                &mut read,
                &lease.task.plan.project_id,
                &expected.account_ref,
            )
            .await?;
        self.account_current(&account, expected, now)?;
        self.authority
            .authorize_in(
                &mut read,
                AdministrativeOperation::Inspect {
                    account: &account,
                    execution: expected,
                    request,
                },
                now,
            )
            .await?;
        self.credential_reference(
            &mut read,
            &account.project,
            &account.workload,
            &account.credential,
        )
        .await?;
        let previous = self.latest(&mut read, lease).await?;
        if let Some(old) = &previous
            && old.request_id == *request
        {
            let control = self
                .guard
                .scope(
                    &mut read,
                    lease,
                    now,
                    worker_control::CredentialScopeUse::Control,
                )
                .await
                .err()
                .map(auth_error);
            if control != old.control_denial {
                return Err(control.unwrap_or(E::Revoked));
            }
            if now < old.observed_at || now >= old.observed_at.saturating_add(15) {
                return Err(E::ExpiredEvidence);
            }
            read.commit().await?;
            return Ok(old.clone());
        }
        read.commit().await?;
        // No SQLite transaction is held while credentials are resolved or AWS is queried.
        let loaded = self.authority.credentials(&account, expected, now).await?;
        if loaded.account_ref != account.account_ref
            || loaded.account_version != account.version
            || loaded.reference != account.credential
            || loaded.expires_at <= self.now()?
        {
            return Err(E::UnverifiedControllerAuthority);
        }
        #[cfg(test)]
        let clients = if let Some(http) = loaded.http {
            sdk::SdkClients::fixture(&expected.workload, loaded.credentials, http)
        } else {
            sdk::SdkClients::new(&expected.workload, loaded.credentials)?
        };
        #[cfg(not(test))]
        let clients = sdk::SdkClients::new(&expected.workload, loaded.credentials)?;
        let assessment = clients.inspect(expected, self.now()?).await;
        let mut tx = self.guard.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let end = self.now()?;
        let current = self
            .account(&mut tx, &account.project, &account.account_ref)
            .await?;
        if current != account
            || self.execution_optional(&mut tx, lease).await?.as_ref() != Some(&execution)
            || self.latest(&mut tx, lease).await? != previous
        {
            return Err(E::Conflict);
        }
        self.account_current(&current, expected, end)?;
        self.authority
            .authorize_in(
                &mut tx,
                AdministrativeOperation::Inspect {
                    account: &current,
                    execution: expected,
                    request,
                },
                end,
            )
            .await?;
        self.credential_reference(
            &mut tx,
            &current.project,
            &current.workload,
            &current.credential,
        )
        .await?;
        if end < now {
            return Err(E::ExpiredEvidence);
        }
        let control_denial = self
            .guard
            .scope(
                &mut tx,
                lease,
                end,
                worker_control::CredentialScopeUse::Control,
            )
            .await
            .err()
            .map(auth_error);
        let revision = previous
            .as_ref()
            .map(|v| v.revision)
            .unwrap_or(0)
            .checked_add(1)
            .filter(|r| *r <= 2048)
            .ok_or(E::ResourceLimit)?;
        if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM aws_task_inspections WHERE project_id=? AND attempt_id=? AND request_id=?)").bind(lease.task.plan.project_id.as_str()).bind(lease.attempt_id.as_str()).bind(request.as_str()).fetch_one(&mut *tx).await?{return Err(E::Conflict);}
        let record = InspectionRecord {
            format: 1,
            project: account.project.clone(),
            attempt: lease.attempt_id.clone(),
            revision,
            request_id: request.clone(),
            observed_at: end,
            account_ref: account.account_ref.clone(),
            account_version: account.version,
            assessment,
            control_denial,
        };
        let text = encode(&record, 8192)?;
        let mac = self.guard.mac(
            b"aws-inspection-record",
            &[
                record.project.as_str().as_bytes(),
                record.attempt.as_str().as_bytes(),
                &revision.to_be_bytes(),
                text.as_bytes(),
            ],
        );
        one(
            sqlx::query("INSERT INTO aws_task_inspections VALUES(?,?,?,?,?,?,?,?,?)")
                .bind(record.project.as_str())
                .bind(record.attempt.as_str())
                .bind(revision as i64)
                .bind(account.account_ref.as_str())
                .bind(account.version as i64)
                .bind(request.as_str())
                .bind(end)
                .bind(text)
                .bind(mac.as_slice())
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        self.checkpoint(AwsCheckpoint::BeforeInspection)?;
        tx.commit().await?;
        self.checkpoint(AwsCheckpoint::AfterInspection)?;
        Ok(record)
    }
    pub(super) async fn check_https(
        &self,
        c: &super::super::IdentityCheck<'_>,
        now: i64,
    ) -> Result<(), E> {
        if c.provider.as_str() != AWS_PROVIDER || c.controller != &self.guard.controller() {
            return Err(E::ScopeMismatch);
        }
        let mut tx = self.guard.store.pool.begin().await?;
        let lease = crate::admission::LeaseRef {
            task: c.binding.task.clone(),
            attempt_id: c.binding.attempt_id.clone(),
            epoch: c.binding.epoch,
        };
        let execution = self
            .execution_optional(&mut tx, &lease)
            .await?
            .ok_or(E::UnknownExecution)?;
        let e = &execution.expected;
        if e.binding != *c.binding
            || e.account_ref != *c.account
            || e.command_digest.as_str() != c.command_digest
            || now < e.authorized_at
            || now >= e.authorization_until
        {
            return Err(E::ScopeMismatch);
        }
        tx.commit().await?;
        let observed = self.inspect(&lease, c.request).await?;
        // All observations, even perfectly matching RUNNING, remain denial-only.
        observed.worker_issuance()
    }
    pub(super) fn account_current(
        &self,
        a: &AccountRecord,
        e: &ExpectedExecution,
        now: i64,
    ) -> Result<(), E> {
        if a.revoked {
            return Err(E::Revoked);
        }
        if now >= a.valid_until {
            return Err(E::ExpiredEvidence);
        }
        if a.installation != e.installation
            || a.project != e.binding.task.plan.project_id
            || a.account_ref != e.account_ref
            || a.version != e.account_version
            || a.workload != e.workload
        {
            return Err(E::ScopeMismatch);
        }
        Ok(())
    }
    pub(super) async fn account(
        &self,
        c: &mut SqliteConnection,
        p: &ProjectId,
        id: &EvidenceRef,
    ) -> Result<AccountRecord, E> {
        self.account_optional(c, p, id)
            .await?
            .ok_or(E::UnconfiguredAccount)
    }
    pub(super) async fn account_optional(
        &self,
        c: &mut SqliteConnection,
        p: &ProjectId,
        id: &EvidenceRef,
    ) -> Result<Option<AccountRecord>, E> {
        let row=sqlx::query("SELECT v.version,v.record,v.record_mac FROM aws_infrastructure_accounts h JOIN aws_infrastructure_account_versions v USING(project_id,account_ref) WHERE h.project_id=? AND h.account_ref=? AND h.current_version=v.version").bind(p.as_str()).bind(id.as_str()).fetch_optional(c).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let version = row.try_get::<i64, _>("version")?;
        let text = row.try_get::<&str, _>("record")?;
        if version <= 0
            || text.len() > 32768
            || !super::super::equal(
                row.try_get::<&[u8], _>("record_mac")?,
                &self.guard.mac(
                    b"aws-account-record",
                    &[
                        p.as_str().as_bytes(),
                        id.as_str().as_bytes(),
                        &(version as u64).to_be_bytes(),
                        text.as_bytes(),
                    ],
                ),
            )
        {
            return Err(E::Integrity);
        }
        let r: AccountRecord = serde_json::from_str(text).map_err(|_| E::Integrity)?;
        r.workload.validate()?;
        if r.format != 1
            || r.installation != self.guard.controller().installation
            || r.project != *p
            || r.account_ref != *id
            || r.version != version as u64
        {
            return Err(E::Integrity);
        }
        Ok(Some(r))
    }
    pub(super) async fn execution_optional(
        &self,
        c: &mut SqliteConnection,
        l: &crate::admission::LeaseRef,
    ) -> Result<Option<ExecutionRecord>, E> {
        let row=sqlx::query("SELECT record,record_mac FROM aws_execution_bindings WHERE project_id=? AND attempt_id=?").bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).fetch_optional(c).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let text = row.try_get::<&str, _>("record")?;
        if text.len() > 32768
            || !super::super::equal(
                row.try_get::<&[u8], _>("record_mac")?,
                &self.guard.mac(
                    b"aws-execution-record",
                    &[
                        l.task.plan.project_id.as_str().as_bytes(),
                        l.attempt_id.as_str().as_bytes(),
                        text.as_bytes(),
                    ],
                ),
            )
        {
            return Err(E::Integrity);
        }
        let r: ExecutionRecord = serde_json::from_str(text).map_err(|_| E::Integrity)?;
        r.expected.validate()?;
        if r.format != 1
            || r.expected.lease() != *l
            || r.expected.installation != self.guard.controller().installation
        {
            return Err(E::Integrity);
        }
        Ok(Some(r))
    }
    pub(super) async fn latest(
        &self,
        c: &mut SqliteConnection,
        l: &crate::admission::LeaseRef,
    ) -> Result<Option<InspectionRecord>, E> {
        let row=sqlx::query("SELECT revision,record,record_mac FROM aws_task_inspections WHERE project_id=? AND attempt_id=? ORDER BY revision DESC LIMIT 1").bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).fetch_optional(c).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let revision = row.try_get::<i64, _>("revision")?;
        let text = row.try_get::<&str, _>("record")?;
        if revision <= 0
            || text.len() > 8192
            || !super::super::equal(
                row.try_get::<&[u8], _>("record_mac")?,
                &self.guard.mac(
                    b"aws-inspection-record",
                    &[
                        l.task.plan.project_id.as_str().as_bytes(),
                        l.attempt_id.as_str().as_bytes(),
                        &(revision as u64).to_be_bytes(),
                        text.as_bytes(),
                    ],
                ),
            )
        {
            return Err(E::Integrity);
        }
        let r: InspectionRecord = serde_json::from_str(text).map_err(|_| E::Integrity)?;
        if r.format != 1
            || r.project != l.task.plan.project_id
            || r.attempt != l.attempt_id
            || r.revision != revision as u64
        {
            return Err(E::Integrity);
        }
        Ok(Some(r))
    }
    pub(super) async fn credential_reference(
        &self,
        c: &mut SqliteConnection,
        p: &ProjectId,
        w: &FargateWorkload,
        k: &CredentialReference,
    ) -> Result<(), E> {
        validate_credential(w, k)?;
        let scope = credential_scope(p, k)?;
        broker::current_reference_in(
            c,
            &self.guard.key,
            &scope,
            broker::SecretVersion::new(k.version as u64).map_err(|_| E::InvalidInput)?,
        )
        .await
        .map_err(|e| {
            if e == broker::BrokerError::Revoked {
                E::Revoked
            } else {
                E::Integrity
            }
        })
    }
}
pub(super) fn origin() -> Origin {
    #[cfg(test)]
    {
        Origin::SyntheticAwsProtocolFixture
    }
    #[cfg(not(test))]
    {
        Origin::ReviewedControllerRegistration
    }
}
pub(super) fn validate_credential(w: &FargateWorkload, k: &CredentialReference) -> Result<(), E> {
    if k.provider != "aws"
        || k.account != w.cluster.account().as_str()
        || k.purpose != "aws-controller-inspection"
        || k.version < 1
    {
        return Err(E::ScopeMismatch);
    }
    Ok(())
}
pub(super) fn credential_scope(
    p: &ProjectId,
    k: &CredentialReference,
) -> Result<broker::RecordScope, E> {
    Ok(broker::RecordScope {
        connection: broker::ConnectionScope {
            project: p.clone(),
            connection: broker::ConnectionId::new(k.connection.clone())
                .map_err(|_| E::InvalidInput)?,
            account: broker::AccountId::new(k.account.clone()).map_err(|_| E::InvalidInput)?,
            provider: broker::ProviderId::new(k.provider.clone()).map_err(|_| E::InvalidInput)?,
            mode: broker::ProviderMode::ApiKey,
        },
        record: broker::RecordId::new(k.record.clone()).map_err(|_| E::InvalidInput)?,
        purpose: broker::SecretPurpose::new(k.purpose.clone()).map_err(|_| E::InvalidInput)?,
    })
}
pub(super) fn one(n: u64) -> Result<(), E> {
    if n == 1 { Ok(()) } else { Err(E::Conflict) }
}
fn encode(v: &impl Serialize, max: usize) -> Result<String, E> {
    let s = serde_json::to_string(v).map_err(|_| E::InvalidInput)?;
    if s.len() > max {
        return Err(E::InvalidInput);
    }
    Ok(s)
}
pub(super) fn auth_error(e: WorkerAuthError) -> E {
    match e {
        WorkerAuthError::FinancialRisk => E::FinancialRisk,
        WorkerAuthError::Expired => E::ExpiredEvidence,
        WorkerAuthError::Busy => E::Busy,
        WorkerAuthError::Storage => E::Storage,
        WorkerAuthError::KeyUnavailable => E::KeyUnavailable,
        _ => E::Revoked,
    }
}

#[cfg(test)]
#[path = "store_fixture.rs"]
pub(in crate::controller_keys::worker_auth) mod fixture;

#[cfg(test)]
impl AccountRecord {
    pub(super) fn origin_is_reviewed(&self) -> bool {
        self.origin == Origin::ReviewedControllerRegistration
    }
}
