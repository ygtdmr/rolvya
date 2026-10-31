use super::*;
impl FargateExecution {
    pub(super) async fn load_optional(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
    ) -> Result<Option<LaunchRequest>, E> {
        let row=sqlx::query("SELECT record,record_mac,request_digest FROM aws_launch_requests WHERE project_id=? AND attempt_id=? AND purpose=?").bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).bind(self.purpose).fetch_optional(c).await?;
        let Some(row) = row else { return Ok(None) };
        let text: &str = row.try_get("record")?;
        if text.len() > 32768
            || !super::super::super::equal(
                row.try_get::<&[u8], _>("record_mac")?,
                &super::super::consent::mac(
                    &self.identity.guard.key,
                    b"launch-request",
                    &[text.as_bytes()],
                ),
            )
        {
            return Err(E::Integrity);
        }
        let r: LaunchRequest = serde_json::from_str(text).map_err(|_| E::Integrity)?;
        if r.lease() != *l
            || r.installation != self.identity.guard.controller().installation
            || r.digest()? != row.try_get::<&str, _>("request_digest")?
        {
            return Err(E::Integrity);
        }
        Ok(Some(r))
    }
    pub(super) async fn load(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
    ) -> Result<LaunchRequest, E> {
        self.load_optional(c, l).await?.ok_or(E::UnknownExecution)
    }
    pub(super) async fn latest(
        &self,
        c: &mut SqliteConnection,
        r: &LaunchRequest,
    ) -> Result<ExecutionStatus, E> {
        let row=sqlx::query("SELECT revision,kind,recorded_at,record,record_mac FROM aws_execution_events WHERE project_id=? AND attempt_id=? AND purpose=? ORDER BY revision DESC LIMIT 1").bind(r.binding.task.plan.project_id.as_str()).bind(r.binding.attempt_id.as_str()).bind(self.purpose).fetch_one(c).await?;
        parse_event(&self.identity.guard.key, r, &row)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "explicit evidence fields keep lifecycle transitions auditable"
    )]
    pub(super) async fn event(
        &self,
        c: &mut SqliteConnection,
        r: &LaunchRequest,
        kind: &str,
        task: Option<TaskArn>,
        lifecycle: EcsLifecycle,
        unknown: bool,
        stop: bool,
        cleanup: bool,
        now: i64,
    ) -> Result<ExecutionStatus, E> {
        let revision:i64=sqlx::query_scalar("SELECT coalesce(max(revision),0)+1 FROM aws_execution_events WHERE project_id=? AND attempt_id=? AND purpose=?").bind(r.binding.task.plan.project_id.as_str()).bind(r.binding.attempt_id.as_str()).bind(self.purpose).fetch_one(&mut *c).await?;
        let s = ExecutionStatus {
            revision: revision as u64,
            kind: kind.into(),
            task,
            lifecycle,
            unknown_exposure: unknown,
            stop_requested: stop,
            cleanup_requested: cleanup,
            observed_at: now,
        };
        let text = encode(&s)?;
        let h = r.digest()?;
        let tag = super::super::consent::mac(
            &self.identity.guard.key,
            b"execution-event",
            &[h.as_bytes(), text.as_bytes()],
        );
        one(
            sqlx::query("INSERT INTO aws_execution_events VALUES(?,?,?,?,?,?,?,?)")
                .bind(r.binding.task.plan.project_id.as_str())
                .bind(r.binding.attempt_id.as_str())
                .bind(self.purpose)
                .bind(revision)
                .bind(kind)
                .bind(now)
                .bind(text)
                .bind(tag.as_slice())
                .execute(c)
                .await?
                .rows_affected(),
        )?;
        Ok(s)
    }
    pub(super) async fn mark_unknown(
        &self,
        c: &mut SqliteConnection,
        r: &LaunchRequest,
        s: &ExecutionStatus,
    ) -> Result<(), E> {
        let (cmd, _, _) = worker_control::command(c, &r.lease())
            .await
            .map_err(worker_error)?;
        let o = self.observation(&cmd, s)?;
        worker_control::mark_unknown(c, &cmd, &o, s.observed_at)
            .await
            .map_err(worker_error)
    }
    pub(super) fn observation(
        &self,
        cmd: &LaunchCommand,
        s: &ExecutionStatus,
    ) -> Result<ProviderObservation, E> {
        Ok(ProviderObservation {
            key: cmd.operation_key(),
            command_digest: rolvya_domain::worker::Digest::new(cmd.digest().map_err(worker_error)?)
                .map_err(|_| E::Integrity)?,
            state: if s.unknown_exposure {
                ProviderState::Unknown
            } else if s.kind == "prepared" {
                ProviderState::Absent
            } else if s.lifecycle == EcsLifecycle::Stopped {
                ProviderState::Finished
            } else if s.task.is_some() {
                ProviderState::Active
            } else {
                ProviderState::Unknown
            },
            unknown_exposure: s.unknown_exposure,
            evidence: EvidenceRef::new(if self.purpose == "coding" {
                format!("aws-{}-{}", s.kind, s.revision)
            } else {
                format!("aws-verifier-{}-{}", s.kind, s.revision)
            })
            .map_err(|_| E::Integrity)?,
            revision: s.revision,
        })
    }
    /// Bind a correlated durable SDK receipt through S2-06B, after R08 accepted the
    /// provider effect. This does not grant worker authority: C still independently
    /// inspects Running/configuration and verifies secret possession on real TLS.
    pub async fn bind_receipt(&self, l: &LeaseRef) -> Result<ExpectedExecution, E> {
        if self.purpose != "coding" {
            return Err(E::ScopeMismatch);
        }
        let mut tx = self.identity.guard.store.pool.begin().await?;
        let r = self.load(&mut tx, l).await?;
        let s = self.latest(&mut tx, &r).await?;
        if s.unknown_exposure {
            return Err(E::UnknownExecution);
        }
        let task = s.task.ok_or(E::UnknownExecution)?;
        let e = r.expected(task)?;
        tx.commit().await?;
        self.identity
            .record_execution(&ExecutionRegistration {
                expected: e.clone(),
                request_id: RequestId::new(format!("receipt-{}", &r.digest()?[..48]))
                    .map_err(|_| E::Integrity)?,
                receipt_reference: EvidenceRef::new(r.digest()?).map_err(|_| E::Integrity)?,
            })
            .await?;
        Ok(e)
    }
}
pub(super) fn parse_event(
    k: &crate::controller_keys::ControllerKey,
    r: &LaunchRequest,
    row: &sqlx::sqlite::SqliteRow,
) -> Result<ExecutionStatus, E> {
    let text: &str = row.try_get("record")?;
    let h = r.digest()?;
    if text.len() > 32768
        || !super::super::super::equal(
            &super::super::consent::mac(k, b"execution-event", &[h.as_bytes(), text.as_bytes()]),
            row.try_get::<&[u8], _>("record_mac")?,
        )
    {
        return Err(E::Integrity);
    }
    let s: ExecutionStatus = serde_json::from_str(text).map_err(|_| E::Integrity)?;
    if s.revision != row.try_get::<i64, _>("revision")? as u64
        || s.kind != row.try_get::<&str, _>("kind")?
        || s.observed_at != row.try_get::<i64, _>("recorded_at")?
    {
        return Err(E::Integrity);
    }
    Ok(s)
}
/// No caller-provided receipt can satisfy this sealed verification. Only the
/// durable SDK path writes the authenticated request and receipt records.
pub(in crate::controller_keys::worker_auth::aws_fargate) async fn verify_receipt(
    c: &mut SqliteConnection,
    k: &crate::controller_keys::ControllerKey,
    registration: &ExecutionRegistration,
) -> Result<(), E> {
    let e = &registration.expected;
    let p = e.binding.task.plan.project_id.as_str();
    let a = e.binding.attempt_id.as_str();
    let row = sqlx::query(
        "SELECT record,record_mac FROM aws_launch_requests WHERE project_id=? AND attempt_id=? AND purpose=?",
    )
    .bind(p)
    .bind(a)
    .bind("coding")
    .fetch_optional(&mut *c)
    .await?
    .ok_or(E::UnverifiedControllerAuthority)?;
    let text: &str = row.try_get("record")?;
    if text.len() > 32768
        || !super::super::super::equal(
            row.try_get::<&[u8], _>("record_mac")?,
            &super::super::consent::mac(k, b"launch-request", &[text.as_bytes()]),
        )
    {
        return Err(E::Integrity);
    }
    let r: LaunchRequest = serde_json::from_str(text).map_err(|_| E::Integrity)?;
    if r.expected(e.task.clone())? != *e || registration.receipt_reference.as_str() != r.digest()? {
        return Err(E::ScopeMismatch);
    }
    let row=sqlx::query("SELECT revision,kind,recorded_at,record,record_mac FROM aws_execution_events WHERE project_id=? AND attempt_id=? AND purpose=? AND kind='receipt' ORDER BY revision DESC LIMIT 1").bind(p).bind(a).bind("coding").fetch_optional(c).await?.ok_or(E::UnverifiedControllerAuthority)?;
    let s = parse_event(k, &r, &row)?;
    if s.task.as_ref() != Some(&e.task) || s.unknown_exposure {
        return Err(E::UnknownExecution);
    }
    Ok(())
}
impl LaunchAdapter for FargateExecution {
    async fn inspect(
        &self,
        cmd: &LaunchCommand,
    ) -> Result<ProviderObservation, crate::worker_simulation::WorkerError> {
        if self.purpose != "coding" {
            return Err(crate::worker_simulation::WorkerError::Unauthorized);
        }
        let mut tx = self.identity.guard.store.pool.begin().await?;
        let r = self
            .load_optional(&mut tx, &cmd.lease())
            .await
            .map_err(denied)?;
        let s = if let Some(r) = r {
            self.latest(&mut tx, &r).await.map_err(denied)?
        } else {
            ExecutionStatus {
                revision: 0,
                kind: "prepared".into(),
                task: None,
                lifecycle: EcsLifecycle::Unknown,
                unknown_exposure: false,
                stop_requested: false,
                cleanup_requested: false,
                observed_at: self.identity.now().map_err(denied)?,
            }
        };
        tx.commit().await?;
        self.observation(cmd, &s).map_err(denied)
    }
    async fn launch(
        &self,
        auth: &LaunchAuthorization,
        clock: &impl Clock,
    ) -> Result<LaunchResult, crate::worker_simulation::WorkerError> {
        self.launch_inner(auth, clock).await.map_err(denied)
    }
}
impl FargateExecution {
    async fn launch_inner(
        &self,
        auth: &LaunchAuthorization,
        clock: &impl Clock,
    ) -> Result<LaunchResult, E> {
        if self.purpose != "coding" {
            return Err(E::ScopeMismatch);
        }
        self.submit(auth.command(), clock).await
    }
    pub(super) async fn submit(
        &self,
        cmd: &LaunchCommand,
        clock: &impl Clock,
    ) -> Result<LaunchResult, E> {
        let l = cmd.lease();
        let mut tx = self
            .identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let r = self.load(&mut tx, &l).await?;
        let old = self.latest(&mut tx, &r).await?;
        if old.kind != "prepared" {
            tx.commit().await?;
            return Ok(LaunchResult::Observed(self.observation(cmd, &old)?));
        }
        let now = clock.now().map_err(|_| E::ExpiredEvidence)?;
        self.authority(&mut tx, &r, cmd, now).await?;
        let (_, state, _) = worker_control::command(&mut tx, &l)
            .await
            .map_err(worker_error)?;
        if (self.purpose == "coding" && state != worker_control::DispatchState::Launching)
            || (self.purpose == "verifier" && state != worker_control::DispatchState::Active)
        {
            return Err(E::Conflict);
        }
        self.event(
            &mut tx,
            &r,
            "submitting",
            None,
            EcsLifecycle::Unknown,
            true,
            false,
            false,
            now,
        )
        .await?;
        self.checkpoint(Checkpoint::BeforeSubmit)?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterSubmit)?;
        // Receipt uncertainty now exists durably. No automatic retry, even with the
        // same token, and never a new token after the provider window expires.
        let sdk = self
            .client(
                &r,
                super::super::credential_use::InfrastructureOperation::LaunchExecution,
            )
            .await?;
        sdk.validate_workload(&r).await?;
        // The task-definition lookup and credential consumption occur outside the writer.
        // Recheck mutable authority again immediately before the mutating SDK call.
        let mut check = self
            .identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        self.authority(
            &mut check,
            &r,
            cmd,
            clock.now().map_err(|_| E::ExpiredEvidence)?,
        )
        .await?;
        check.commit().await?;
        let result = sdk.launch(&r).await;
        self.checkpoint(Checkpoint::AfterEffect)?;
        let mut tx = self
            .identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let end = clock.now().map_err(|_| E::ExpiredEvidence)?;
        let current = self.latest(&mut tx, &r).await?;
        if current.kind != "submitting" {
            return Err(E::Conflict);
        }
        let valid = self.authority(&mut tx, &r, cmd, end).await.is_ok();
        let receipt = result.as_ref().ok();
        let task = receipt.and_then(|v| v.task.clone());
        let s = match task {
            Some(task) => {
                self.event(
                    &mut tx,
                    &r,
                    "receipt",
                    Some(task),
                    EcsLifecycle::Provisioning,
                    !valid,
                    false,
                    false,
                    end,
                )
                .await?
            }
            None => {
                self.event(
                    &mut tx,
                    &r,
                    "unknown",
                    None,
                    EcsLifecycle::Unknown,
                    true,
                    false,
                    false,
                    end,
                )
                .await?
            }
        };
        if let Some(receipt) = receipt {
            let text = encode(receipt)?;
            let mac = super::super::consent::mac(
                &self.identity.guard.key,
                b"sdk-receipt",
                &[r.digest()?.as_bytes(), text.as_bytes()],
            );
            one(
                sqlx::query("INSERT INTO aws_sdk_receipts VALUES(?,?,?,?,?,?)")
                    .bind(r.binding.task.plan.project_id.as_str())
                    .bind(r.binding.attempt_id.as_str())
                    .bind(self.purpose)
                    .bind(s.revision as i64)
                    .bind(text)
                    .bind(mac.as_slice())
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
        }
        if s.unknown_exposure {
            self.mark_unknown(&mut tx, &r, &s).await?
        }
        self.checkpoint(Checkpoint::BeforeReceipt)?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterReceipt)?;
        Ok(LaunchResult::Observed(self.observation(cmd, &s)?))
    }
}
fn denied(_: E) -> crate::worker_simulation::WorkerError {
    crate::worker_simulation::WorkerError::Unauthorized
}
impl ExecutionAdapter for FargateExecution {
    async fn stop(&self, l: &LeaseRef, reason: StopReason) -> Result<ExecutionStatus, E> {
        // An intent may have committed without reaching AWS. Inspect first, then
        // retry only the original task, at most three explicitly requested calls.
        let prior = self.status(l).await?;
        let mut expected_revision = prior.revision;
        if prior.stop_requested {
            let observed = self.reconcile(l).await?;
            expected_revision = observed.revision;
            if observed.lifecycle == EcsLifecycle::Stopped {
                return Ok(observed);
            }
            if observed.lifecycle == EcsLifecycle::Unknown {
                return Err(E::UnknownExecution);
            }
        }
        let mut tx = self
            .identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let r = self.load(&mut tx, l).await?;
        let old = self.latest(&mut tx, &r).await?;
        if old.revision != expected_revision {
            return Err(E::Conflict);
        }
        let stops:i64=sqlx::query_scalar("SELECT count(*) FROM aws_execution_events WHERE project_id=? AND attempt_id=? AND purpose=? AND kind='stop_requested'").bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).bind(self.purpose).fetch_one(&mut *tx).await?;
        if stops >= 3 {
            return Err(E::ResourceLimit);
        }
        let now = self.identity.now()?;
        self.activation(&r, now)?;
        let (_, state, _) = worker_control::command(&mut tx, l)
            .await
            .map_err(worker_error)?;
        let live = crate::admission::live(&mut tx, l, now).await.is_ok();
        // A supplied reason is not authority. Need actual controller stop/fence,
        // expiration, terminal or runtime-bound state; no human/worker bypass.
        if live
            && !matches!(
                state,
                worker_control::DispatchState::Blocked | worker_control::DispatchState::Finished
            )
            && now < r.authorized_at + i64::from(r.workload.runtime_seconds)
            && !worker_control::has_stop(&mut tx, l)
                .await
                .map_err(worker_error)?
        {
            return Err(E::UnverifiedControllerAuthority);
        }
        let task = old.task.clone().ok_or(E::UnknownExecution)?;
        self.event(
            &mut tx,
            &r,
            "stop_requested",
            Some(task.clone()),
            old.lifecycle,
            old.unknown_exposure,
            true,
            old.cleanup_requested,
            now,
        )
        .await?;
        self.checkpoint(Checkpoint::BeforeStop)?;
        tx.commit().await?;
        self.checkpoint(Checkpoint::AfterStop)?;
        let sdk = self
            .client(
                &r,
                super::super::credential_use::InfrastructureOperation::StopExecution,
            )
            .await?;
        let result = sdk.stop(&r, &task, reason).await;
        self.checkpoint(Checkpoint::AfterStopEffect)?;
        let mut tx = self
            .identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let latest = self.latest(&mut tx, &r).await?;
        let s = self
            .event(
                &mut tx,
                &r,
                if result.is_ok() {
                    "stop_ack"
                } else {
                    "stop_unknown"
                },
                Some(task),
                latest.lifecycle,
                latest.unknown_exposure || result.is_err(),
                true,
                latest.cleanup_requested,
                self.identity.now()?,
            )
            .await?;
        if s.unknown_exposure {
            self.mark_unknown(&mut tx, &r, &s).await?
        }
        tx.commit().await?;
        Ok(s)
    }
    async fn request_cleanup(&self, l: &LeaseRef) -> Result<ExecutionStatus, E> {
        let mut tx = self
            .identity
            .guard
            .store
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let r = self.load(&mut tx, l).await?;
        let old = self.latest(&mut tx, &r).await?;
        if old.cleanup_requested {
            tx.commit().await?;
            return Ok(old);
        }
        if old.lifecycle != EcsLifecycle::Stopped {
            return Err(E::TaskNotRunning);
        }
        // A cleanup intent does not claim secret deletion, AWS termination, billing
        // settlement or artifact deletion. Exact resource identity lives in the request.
        let s = self
            .event(
                &mut tx,
                &r,
                "cleanup_requested",
                old.task,
                old.lifecycle,
                old.unknown_exposure,
                old.stop_requested,
                true,
                self.identity.now()?,
            )
            .await?;
        tx.commit().await?;
        Ok(s)
    }
}
