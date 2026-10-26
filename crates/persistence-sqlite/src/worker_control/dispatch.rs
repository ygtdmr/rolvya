use super::*;
#[derive(Clone)]
pub struct OutboxDispatcher {
    pub(super) store: PlanStore,
}
impl OutboxDispatcher {
    pub fn new(store: PlanStore) -> Self {
        Self { store }
    }
    /// Atomic R06 dispatch claim plus durable R08 command. Existing legacy R06
    /// claims start in reconciliation: their absence of an R08 row proves nothing.
    pub async fn claim<P: SchedulerPolicy>(
        &self,
        l: &LeaseRef,
        actor: &PrincipalId,
        policy: &P,
        clock: &impl Clock,
        limits: Limits,
    ) -> Result<DispatchTicket, WorkerError> {
        let mut read = self.store.pool.begin().await?;
        let (r, _, _) = admission::stored_admission(&mut read, l).await?;
        read.commit().await?;
        let plan = self.store.load(&l.task.plan, policy, limits).await?;
        let task = admission::task_in(&plan, &l.task, &r.fingerprint)?;
        let assignment = task.execution.as_ref().ok_or(AdmissionError::NotEligible)?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let t = admission::at(clock)?;
        let (_, until, _) = admission::stored_admission(&mut tx, l).await?;
        let financial_event: i64 = sqlx::query_scalar(
            "SELECT financial_event FROM task_attempts WHERE project_id=? AND attempt_id=?",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        let cmd = LaunchCommand {
            binding: WorkerBinding {
                task: l.task.clone(),
                attempt_id: l.attempt_id.clone(),
                worker_id: WorkerId::new(r.permit.executor.as_str())
                    .map_err(|_| WorkerError::ScopeMismatch)?,
                epoch: l.epoch,
                plan_hash: Digest::new(&r.fingerprint)?,
                context_hash: Digest::new(&task.context.sha256)?,
            },
            assignment: assignment.clone(),
            profile: ExecutionProfile {
                model: ModelName::new(&assignment.model)?,
                effort: assignment.effort,
                access_mode: assignment.access_mode,
                resources: assignment.resources.clone(),
            },
            admission: r.clone(),
            financial_event,
            initial_lease_until: until,
        };
        let request = DispatchRequest {
            lease: l.clone(),
            request_id: identity("dispatch", l)?,
            actor: actor.clone(),
        };
        let owner = admission::owner(&mut tx, &l.task.plan.project_id).await?;
        if !policy.permits(
            &owner,
            SchedulerAction::Dispatch {
                request: &request,
                admission: &r,
            },
            t,
        ) || actor != &r.actor
        {
            return Err(WorkerError::Unauthorized);
        }
        current(&mut tx, &cmd, policy, t).await?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM dispatch_operations WHERE project_id=? AND attempt_id=?)",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if exists {
            command(&mut tx, l).await?;
            tx.commit().await?;
            return Ok(DispatchTicket { lease: l.clone() });
        }
        let claimed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM dispatch_claims WHERE project_id=? AND attempt_id=?)",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if !claimed {
            let payload =
                serde_json::json!({"format":1,"kind":"dispatch","request":request}).to_string();
            if admission::existing(
                &mut tx,
                &l.task.plan.project_id,
                &request.request_id,
                &payload,
            )
            .await?
            .is_some()
            {
                return Err(WorkerError::ReplayConflict);
            }
            let e = admission::event(
                &mut tx,
                &l.task.plan.project_id,
                &request.request_id,
                "dispatch",
                actor,
                t,
                &payload,
            )
            .await?;
            affected(
                sqlx::query("INSERT INTO dispatch_claims VALUES(?,?,?)")
                    .bind(l.task.plan.project_id.as_str())
                    .bind(l.attempt_id.as_str())
                    .bind(e)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
        }
        let state = if claimed {
            DispatchState::Reconcile
        } else {
            DispatchState::Claimed
        };
        let e = audit(
            &mut tx,
            l,
            "claim",
            t,
            &serde_json::json!({"digest":cmd.digest()?,"legacy_claim":claimed}),
        )
        .await?;
        affected(
            sqlx::query("INSERT INTO dispatch_operations VALUES(?,?,?,?,?,0,?)")
                .bind(l.task.plan.project_id.as_str())
                .bind(l.attempt_id.as_str())
                .bind(json(&cmd)?)
                .bind(cmd.digest()?)
                .bind(state.name())
                .bind(e)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        let end = admission::at(clock)?;
        if end < t {
            return Err(WorkerError::InvalidTime);
        }
        current(&mut tx, &cmd, policy, end).await?;
        tx.commit().await?;
        Ok(DispatchTicket { lease: l.clone() })
    }
    /// Indexed, bounded scan. No ticket returned by this read is launch authority.
    /// Caller schedules scans/backoff; this component creates no polling loop.
    pub async fn pending(
        &self,
        project: &ProjectId,
        after: i64,
        limit: u16,
    ) -> Result<Vec<DispatchIntent>, WorkerError> {
        if after < 0 || !(1..=100).contains(&limit) {
            return Err(WorkerError::ResourceLimit);
        }
        let rows=sqlx::query("SELECT a.plan_id,a.version,a.task_id,a.attempt_id,a.epoch,o.sequence FROM dispatch_outbox o JOIN task_attempts a USING(project_id,attempt_id) LEFT JOIN dispatch_operations d USING(project_id,attempt_id) WHERE o.project_id=? AND o.sequence>? AND (d.state IS NULL OR d.state IN ('claimed','launching','reconcile')) ORDER BY o.sequence LIMIT ?")
            .bind(project.as_str()).bind(after).bind(limit).fetch_all(&self.store.pool).await?;
        rows.into_iter()
            .map(|r| {
                Ok(DispatchIntent {
                    sequence: r.get(5),
                    lease: LeaseRef {
                        task: rolvya_domain::plan::TaskRef {
                            plan: rolvya_domain::plan::PlanKey {
                                project_id: project.clone(),
                                plan_id: PlanId::new(r.get::<String, _>(0))
                                    .map_err(|_| StoreError::Integrity)?,
                                version: PlanVersion::new(r.get::<i64, _>(1) as u64)
                                    .map_err(|_| StoreError::Integrity)?,
                            },
                            task_id: TaskId::new(r.get::<String, _>(2))
                                .map_err(|_| StoreError::Integrity)?,
                        },
                        attempt_id: AttemptId::new(r.get::<String, _>(3))
                            .map_err(|_| StoreError::Integrity)?,
                        epoch: r.get::<i64, _>(4) as u64,
                    },
                })
            })
            .collect()
    }
    pub async fn state(&self, l: &LeaseRef) -> Result<DispatchState, WorkerError> {
        let mut tx = self.store.pool.begin().await?;
        let (_, s, _) = command(&mut tx, l).await?;
        tx.commit().await?;
        Ok(s)
    }
    /// Inspect first, mark before launch, never launch again from launching/reconcile.
    /// Adapter calls occur with no controller transaction or write lock held.
    pub async fn dispatch<P: SchedulerPolicy>(
        &self,
        ticket: &DispatchTicket,
        adapter: &impl LaunchAdapter,
        policy: &P,
        clock: &impl Clock,
        limits: Limits,
    ) -> Result<DispatchState, WorkerError> {
        let l = &ticket.lease;
        let mut read = self.store.pool.begin().await?;
        let (cmd, _, _) = command(&mut read, l).await?;
        read.commit().await?;
        let observation = adapter.inspect(&cmd).await?;
        if observation.key != cmd.operation_key()
            || observation.command_digest.as_str() != cmd.digest()?
        {
            return Err(WorkerError::ReplayConflict);
        }
        if observation.state != ProviderState::Absent {
            return self
                .record_observation(l, &observation, policy, clock, limits)
                .await;
        }
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let t = admission::at(clock)?;
        let (_, state, revision) = command(&mut tx, l).await?;
        if state != DispatchState::Claimed {
            // Another dispatcher may be between marker commit and its effect.
            // Absence cannot cancel that request or allow a competing launch.
            if state == DispatchState::Launching && current(&mut tx, &cmd, policy, t).await.is_ok()
            {
                tx.commit().await?;
                return Ok(DispatchState::Launching);
            }
            tx.commit().await?;
            return self
                .record_observation(l, &observation, policy, clock, limits)
                .await;
        }
        current(&mut tx, &cmd, policy, t).await?;
        let e = audit(
            &mut tx,
            l,
            "prepare",
            t,
            &serde_json::json!({"digest":cmd.digest()?}),
        )
        .await?;
        transition(&mut tx, l, DispatchState::Launching, revision, e).await?;
        let end = admission::at(clock)?;
        if end < t {
            return Err(WorkerError::InvalidTime);
        }
        current(&mut tx, &cmd, policy, end).await?;
        tx.commit().await?;
        let authorization = LaunchAuthorization { command: cmd };
        let result = adapter.launch(&authorization, clock).await;
        match result {
            Ok(LaunchResult::Observed(observation)) => {
                self.record_observation(l, &observation, policy, clock, limits)
                    .await
            }
            Ok(LaunchResult::Indeterminate) | Err(_) => {
                let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
                let (_, state, revision) = command(&mut tx, l).await?;
                if state == DispatchState::Launching {
                    let e = audit(
                        &mut tx,
                        l,
                        "observation",
                        admission::at(clock)?,
                        &serde_json::json!({"outcome":"acknowledgement_unknown"}),
                    )
                    .await?;
                    transition(&mut tx, l, DispatchState::Reconcile, revision, e).await?;
                    tx.commit().await?;
                    Ok(DispatchState::Reconcile)
                } else {
                    tx.commit().await?;
                    Ok(state)
                }
            }
        }
    }
    pub async fn reconcile<P: SchedulerPolicy>(
        &self,
        l: &LeaseRef,
        adapter: &impl LaunchAdapter,
        policy: &P,
        clock: &impl Clock,
        limits: Limits,
    ) -> Result<DispatchState, WorkerError> {
        let mut read = self.store.pool.begin().await?;
        let (cmd, _, _) = command(&mut read, l).await?;
        read.commit().await?;
        let observation = adapter.inspect(&cmd).await?;
        self.record_observation(l, &observation, policy, clock, limits)
            .await
    }
    async fn record_observation<P: SchedulerPolicy>(
        &self,
        l: &LeaseRef,
        o: &ProviderObservation,
        policy: &P,
        clock: &impl Clock,
        _limits: Limits,
    ) -> Result<DispatchState, WorkerError> {
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let t = admission::at(clock)?;
        let (cmd, state, revision) = command(&mut tx, l).await?;
        if o.key != cmd.operation_key() || o.command_digest.as_str() != cmd.digest()? {
            return Err(WorkerError::ReplayConflict);
        }
        if o.state == ProviderState::Absent
            && state == DispatchState::Launching
            && current(&mut tx, &cmd, policy, t).await.is_ok()
        {
            tx.commit().await?;
            return Ok(DispatchState::Launching);
        }
        let payload = json(o)?;
        let digest = hash(&payload);
        let old:Option<String>=sqlx::query_scalar("SELECT digest FROM dispatch_observations WHERE project_id=? AND attempt_id=? AND evidence_id=?").bind(o.key.project.as_str()).bind(o.key.attempt.as_str()).bind(o.evidence.as_str()).fetch_optional(&mut *tx).await?;
        if old.as_ref().is_some_and(|d| d != &digest) {
            return Err(WorkerError::ReplayConflict);
        }
        // Recheck present authority even for an identical old provider observation.
        let authorized = current(&mut tx, &cmd, policy, t).await;
        let newest:Option<i64>=sqlx::query_scalar("SELECT max(provider_revision) FROM dispatch_observations WHERE project_id=? AND attempt_id=?").bind(o.key.project.as_str()).bind(o.key.attempt.as_str()).fetch_one(&mut *tx).await?;
        let provider_revision =
            i64::try_from(o.revision).map_err(|_| WorkerError::ResourceLimit)?;
        if newest.is_some_and(|n| provider_revision < n) {
            if state == DispatchState::Active && authorized.is_err() {
                let e = audit(
                    &mut tx,
                    l,
                    "observation",
                    t,
                    &serde_json::json!({"outcome":"stale_observation_with_retired_authority"}),
                )
                .await?;
                transition(&mut tx, l, DispatchState::Blocked, revision, e).await?;
                tx.commit().await?;
                return Ok(DispatchState::Blocked);
            }
            tx.commit().await?;
            return Ok(state);
        }
        if newest == Some(provider_revision) && old.is_none() {
            return Err(WorkerError::ReplayConflict);
        }
        let prior_effect:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM dispatch_observations o JOIN worker_events e ON e.sequence=o.event_sequence WHERE o.project_id=? AND o.attempt_id=? AND json_extract(e.payload,'$.observation.state') IN ('active','finished','cancel_ack'))").bind(o.key.project.as_str()).bind(o.key.attempt.as_str()).fetch_one(&mut *tx).await?;
        let contradiction = (o.state == ProviderState::Rejected && prior_effect)
            || (state == DispatchState::NoEffect && o.state != ProviderState::Rejected)
            || (state == DispatchState::Finished && !matches!(o.state, ProviderState::Finished));
        if o.unknown_exposure || contradiction {
            super::recovery::mark_unknown(&mut tx, &cmd, o, t).await?;
        }
        let next = if contradiction || o.unknown_exposure {
            DispatchState::Blocked
        } else if o.state == ProviderState::Finished {
            DispatchState::Finished
        } else if o.state == ProviderState::Rejected {
            DispatchState::NoEffect
        } else if matches!(
            state,
            DispatchState::Blocked | DispatchState::NoEffect | DispatchState::Finished
        ) {
            state
        } else if authorized.is_err() {
            DispatchState::Blocked
        } else {
            match o.state {
                ProviderState::Active => DispatchState::Active,
                _ => DispatchState::Reconcile,
            }
        };
        if old.is_some() && next == state {
            tx.commit().await?;
            return Ok(state);
        }
        let e = audit(
            &mut tx,
            l,
            "observation",
            t,
            &serde_json::json!({"observation":o,"next":next}),
        )
        .await?;
        if old.is_none() {
            affected(
                sqlx::query("INSERT INTO dispatch_observations VALUES(?,?,?,?,?,?)")
                    .bind(o.key.project.as_str())
                    .bind(o.key.attempt.as_str())
                    .bind(o.evidence.as_str())
                    .bind(digest)
                    .bind(provider_revision)
                    .bind(e)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
        }
        transition(&mut tx, l, next, revision, e).await?;
        if next == DispatchState::Active {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM worker_sessions WHERE project_id=? AND attempt_id=?)",
            )
            .bind(o.key.project.as_str())
            .bind(o.key.attempt.as_str())
            .fetch_one(&mut *tx)
            .await?;
            if !exists {
                affected(
                    sqlx::query("INSERT INTO worker_sessions VALUES(?,?,?,0,?)")
                        .bind(o.key.project.as_str())
                        .bind(o.key.attempt.as_str())
                        .bind(json(&cmd.session())?)
                        .bind(e)
                        .execute(&mut *tx)
                        .await?
                        .rows_affected(),
                )?;
            }
            let end = admission::at(clock)?;
            if end < t {
                return Err(WorkerError::InvalidTime);
            }
            current(&mut tx, &cmd, policy, end).await?;
        }
        tx.commit().await?;
        Ok(next)
    }
}
