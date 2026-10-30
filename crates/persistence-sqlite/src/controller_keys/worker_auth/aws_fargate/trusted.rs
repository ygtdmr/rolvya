//! Concrete Owner-consent/Broker/controller-identity composition. External AWS
//! launch receipt verification remains sealed and unavailable in production.
use super::{
    AwsIdentityError as E,
    credential_use::{InfrastructureOperation, VerifiedInfrastructureUse},
    store::*,
    *,
};
use crate::admission::Clock;
use crate::controller_keys::{ControllerBootstrap, ControllerKey, secret_broker::aws_consumer};
use crate::{RequestId, finance::EvidenceRef};
use rolvya_domain::ids::ProjectId;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use sqlx::{Row, SqliteConnection};
use std::sync::Arc;
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ControllerCheck {
    format: u16,
    account_ref: EvidenceRef,
    project: ProjectId,
    consent_generation: i64,
    scope_digest: String,
    request: RequestId,
    observed_at: i64,
    expires_at: i64,
    origin: Origin,
}
pub(super) struct ReviewedInfrastructure {
    store: crate::PlanStore,
    key: ControllerKey,
    clock: Arc<dyn Clock>,
    #[cfg(test)]
    pub(super) http: Option<aws_smithy_runtime_api::client::http::SharedHttpClient>,
    #[cfg(test)]
    pub(super) launch: Option<ExpectedExecution>,
}
impl ReviewedInfrastructure {
    pub(super) async fn new(b: &ControllerBootstrap, clock: Arc<dyn Clock>) -> Result<Self, E> {
        Ok(Self {
            store: b.store.clone(),
            key: b.load_existing().await.map_err(|_| E::KeyUnavailable)?,
            clock,
            #[cfg(test)]
            http: None,
            #[cfg(test)]
            launch: None,
        })
    }
}
fn enrollment_matches(a: &AccountRecord, c: &consent::Consent) -> bool {
    let r = &c.enrollment;
    a.project == r.project
        && a.account_ref == r.account_ref
        && a.version == r.expected_version + 1
        && a.request_id == r.request_id
        && a.workload == r.workload
        && a.credential == r.credential
        && a.consent_reference == r.consent_reference
        && a.valid_until == r.valid_until
        && a.revoked == r.revoked
        && a.actor.as_str() == format!("human-{}", c.actor)
}
fn scope_digest(r: &AccountEnrollment) -> Result<String, E> {
    Ok(super::super::hex(&Sha256::digest(
        serde_json::to_vec(r).map_err(|_| E::Integrity)?,
    )))
}
impl InfrastructureAuthority for ReviewedInfrastructure {
    fn authorize_in<'a>(
        &'a self,
        c: &'a mut SqliteConnection,
        op: AdministrativeOperation<'a>,
        now: i64,
    ) -> Authorization<'a> {
        Box::pin(async move {
            let (p, id) = match &op {
                AdministrativeOperation::Enroll(r) => (&r.project, &r.account_ref),
                AdministrativeOperation::Bind(r) => (
                    &r.expected.binding.task.plan.project_id,
                    &r.expected.account_ref,
                ),
                AdministrativeOperation::Inspect { account, .. } => {
                    (&account.project, &account.account_ref)
                }
            };
            let approved = consent::current(c, &self.key, p, id, now).await?;
            match op {
                AdministrativeOperation::Enroll(r) => {
                    if approved.enrollment != *r {
                        return Err(E::ScopeMismatch);
                    }
                    load_check(c, &self.key, &approved, now).await?;
                }
                AdministrativeOperation::Bind(r) => {
                    if approved.enrollment.expected_version + 1 != r.expected.account_version
                        || approved.enrollment.workload != r.expected.workload
                    {
                        return Err(E::ScopeMismatch);
                    }
                    // Owner consent and copied ECS metadata cannot fabricate a launch
                    // receipt. Only the private protocol fixture supplies this port.
                    #[cfg(test)]
                    if self.launch.as_ref() != Some(&r.expected) {
                        return Err(E::UnverifiedControllerAuthority);
                    }
                    #[cfg(not(test))]
                    {
                        let _ = r;
                        return Err(E::UnverifiedControllerAuthority);
                    }
                }
                AdministrativeOperation::Inspect {
                    account,
                    execution,
                    request,
                } => {
                    if RequestId::new(request.as_str()).is_err()
                        || !enrollment_matches(account, &approved)
                        || execution.account_version != account.version
                    {
                        return Err(E::Revoked);
                    }
                }
            }
            EvidenceRef::new(format!("human-{}", approved.actor)).map_err(|_| E::Integrity)
        })
    }
    fn credentials<'a>(
        &'a self,
        a: &'a AccountRecord,
        e: &'a ExpectedExecution,
        _: i64,
    ) -> CredentialLoad<'a> {
        Box::pin(async move {
            if a.project != e.binding.task.plan.project_id
                || a.account_ref != e.account_ref
                || a.version != e.account_version
            {
                return Err(E::ScopeMismatch);
            }
            let now = self.clock.now().map_err(|_| E::ExpiredEvidence)?;
            let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
            let use_scope = VerifiedInfrastructureUse::authorize(
                &mut tx,
                &self.key,
                &a.project,
                &a.account_ref,
                &RequestId::new(format!(
                    "aws-read-{}",
                    super::super::new_id().map_err(auth_error)?
                ))
                .map_err(|_| E::InvalidInput)?,
                InfrastructureOperation::InspectExecution,
                now,
            )
            .await?;
            if !enrollment_matches(a, use_scope.consent()) {
                return Err(E::Revoked);
            }
            let loaded = aws_consumer::consume(&mut tx, &self.key, &use_scope, now).await?;
            tx.commit().await?;
            Ok(ResolvedCredentials {
                account_ref: a.account_ref.clone(),
                account_version: a.version,
                reference: a.credential.clone(),
                expires_at: loaded.expires_at,
                credentials: loaded.credentials,
                #[cfg(test)]
                http: self.http.clone(),
            })
        })
    }
    #[cfg(test)]
    fn controller_http(&self) -> Option<aws_smithy_runtime_api::client::http::SharedHttpClient> {
        self.http.clone()
    }
}
async fn load_check(
    c: &mut SqliteConnection,
    k: &ControllerKey,
    v: &consent::Consent,
    now: i64,
) -> Result<ControllerCheck, E> {
    let row=sqlx::query("SELECT record,record_mac FROM aws_controller_checks WHERE project_id=? AND account_ref=? AND consent_generation=? ORDER BY observed_at DESC,request_id DESC LIMIT 1").bind(v.enrollment.project.as_str()).bind(v.enrollment.account_ref.as_str()).bind(v.generation).fetch_optional(c).await?.ok_or(E::UnverifiedControllerAuthority)?;
    let text: &str = row.try_get("record")?;
    if text.len() > 8192
        || !super::super::equal(
            row.try_get::<&[u8], _>("record_mac")?,
            &consent::mac(k, b"controller-check", &[text.as_bytes()]),
        )
    {
        return Err(E::Integrity);
    }
    let fact: ControllerCheck = serde_json::from_str(text).map_err(|_| E::Integrity)?;
    if fact.format != 1
        || fact.project != v.enrollment.project
        || fact.account_ref != v.enrollment.account_ref
        || fact.consent_generation != v.generation
        || fact.scope_digest != scope_digest(&v.enrollment)?
        || now < fact.observed_at
        || now >= fact.expires_at
    {
        return Err(E::ExpiredEvidence);
    }
    Ok(fact)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControllerVerificationReceipt {
    pub account_ref: EvidenceRef,
    pub consent_generation: i64,
    pub observed_at: i64,
    pub expires_at: i64,
}
impl FargateIdentity {
    /// Read-only controller identity check after actual atomic Owner consent. A
    /// request object/credential reference alone cannot trigger consumption.
    pub async fn verify_account(
        &self,
        r: &AccountEnrollment,
        request: &RequestId,
    ) -> Result<ControllerVerificationReceipt, E> {
        let now = self.now()?;
        let mut tx = self.guard.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let approved =
            consent::current(&mut tx, &self.guard.key, &r.project, &r.account_ref, now).await?;
        if approved.enrollment != *r {
            return Err(E::ScopeMismatch);
        }
        let use_scope = VerifiedInfrastructureUse::authorize(
            &mut tx,
            &self.guard.key,
            &r.project,
            &r.account_ref,
            request,
            InfrastructureOperation::VerifyController,
            now,
        )
        .await?;
        let loaded = aws_consumer::consume(&mut tx, &self.guard.key, &use_scope, now).await?;
        tx.commit().await?;
        #[cfg(test)]
        let clients = if let Some(http) = self.authority.controller_http() {
            sdk::SdkClients::fixture(&r.workload, loaded.credentials, http)
        } else {
            sdk::SdkClients::new(&r.workload, loaded.credentials)?
        };
        #[cfg(not(test))]
        let clients = sdk::SdkClients::new(&r.workload, loaded.credentials)?;
        clients.verify_controller(&r.workload).await?;
        let mut tx = self.guard.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let end = self.now()?;
        if end < now
            || end >= loaded.expires_at
            || consent::current(&mut tx, &self.guard.key, &r.project, &r.account_ref, end).await?
                != approved
        {
            return Err(E::Revoked);
        }
        let expires_at = end
            .checked_add(60)
            .ok_or(E::ExpiredEvidence)?
            .min(loaded.expires_at)
            .min(r.valid_until);
        if expires_at <= end {
            return Err(E::ExpiredEvidence);
        }
        let fact = ControllerCheck {
            format: 1,
            account_ref: r.account_ref.clone(),
            project: r.project.clone(),
            consent_generation: approved.generation,
            scope_digest: scope_digest(r)?,
            request: request.clone(),
            observed_at: end,
            expires_at,
            origin: origin(),
        };
        let text = serde_json::to_string(&fact).map_err(|_| E::Integrity)?;
        let tag = consent::mac(&self.guard.key, b"controller-check", &[text.as_bytes()]);
        one(
            sqlx::query("INSERT INTO aws_controller_checks VALUES(?,?,?,?,?,?,?,?)")
                .bind(r.project.as_str())
                .bind(r.account_ref.as_str())
                .bind(approved.generation)
                .bind(request.as_str())
                .bind(text)
                .bind(tag.as_slice())
                .bind(end)
                .bind(expires_at)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        tx.commit().await?;
        Ok(ControllerVerificationReceipt {
            account_ref: r.account_ref.clone(),
            consent_generation: approved.generation,
            observed_at: end,
            expires_at,
        })
    }
}

#[cfg(test)]
pub(in crate::controller_keys::worker_auth) async fn fixture_open(
    b: &ControllerBootstrap,
    p: Arc<dyn crate::admission::SchedulerPolicy>,
    clock: Arc<dyn Clock>,
    expected: &ExpectedExecution,
    http: aws_smithy_runtime_api::client::http::SharedHttpClient,
) -> Arc<FargateIdentity> {
    let mut guard = super::super::WorkerSecurity::open(b).await.unwrap();
    guard.policy = p;
    guard.clock = clock.clone();
    let mut authority = ReviewedInfrastructure::new(b, clock).await.unwrap();
    authority.http = Some(http);
    authority.launch = Some(expected.clone());
    Arc::new(FargateIdentity {
        guard,
        authority: Arc::new(authority),
        hook: Box::new(|_| Ok(())),
    })
}
#[cfg(test)]
pub(in crate::controller_keys::worker_auth) fn fixture_account_expiry(
    i: &mut Arc<FargateIdentity>,
    clock: Arc<crate::worker_simulation::DeterministicClock>,
    deadline: i64,
) {
    Arc::get_mut(i).unwrap().hook = Box::new(move |p| {
        if p == AwsCheckpoint::BeforeAccount {
            clock.advance_to(deadline).unwrap();
        }
        Ok(())
    });
}
