use super::*;
impl SyntheticCoordinator {
    pub async fn open(
        store: PlanStore,
        project: &ProjectId,
        policy: &impl SchedulerPolicy,
        limits: Limits,
    ) -> Result<Self, OrchestrationError> {
        let plan = store.load_active(project, policy, limits).await?;
        let positions = plan
            .content
            .tasks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id.clone(), i))
            .collect();
        let mut children: BTreeMap<TaskId, Vec<TaskId>> = BTreeMap::new();
        for task in &plan.content.tasks {
            if let Some(parent) = &task.parent {
                children
                    .entry(parent.task_id.clone())
                    .or_default()
                    .push(task.id.clone());
            }
        }
        Ok(Self {
            children,
            store,
            plan,
            positions,
            limits,
        })
    }
    pub fn plan_key(&self) -> &PlanKey {
        &self.plan.content.key
    }
    pub fn fingerprint(&self) -> &str {
        self.plan.revision.fingerprint()
    }
    async fn task_batch(&self, kind: &str, limit: u16) -> Result<Vec<TaskId>, OrchestrationError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let k = self.plan_key();
        let rows=sqlx::query_scalar::<_,String>("SELECT t.task_id FROM tasks t JOIN task_runtime r USING(project_id,plan_id,version,task_id) WHERE t.project_id=? AND t.plan_id=? AND t.version=? AND t.kind=? AND r.state IN ('planned','waiting','ready') AND NOT EXISTS(SELECT 1 FROM task_dependencies d LEFT JOIN task_acceptances a ON a.project_id=d.project_id AND a.plan_id=d.plan_id AND a.version=d.version AND a.task_id=d.predecessor_id WHERE d.project_id=t.project_id AND d.plan_id=t.plan_id AND d.version=t.version AND d.successor_id=t.task_id AND a.task_id IS NULL) AND (t.kind='executable_leaf' OR NOT EXISTS(SELECT 1 FROM tasks ch LEFT JOIN task_acceptances a ON a.project_id=ch.project_id AND a.plan_id=ch.plan_id AND a.version=ch.version AND a.task_id=ch.task_id WHERE ch.project_id=t.project_id AND ch.plan_id=t.plan_id AND ch.version=t.version AND ch.parent_task_id=t.task_id AND a.task_id IS NULL)) ORDER BY t.task_id LIMIT ?")
   .bind(k.project_id.as_str()).bind(k.plan_id.as_str()).bind(version(k)?).bind(kind).bind(limit).fetch_all(&self.store.pool).await?;
        rows.into_iter()
            .map(|id| TaskId::new(id).map_err(|_| StoreError::Integrity.into()))
            .collect()
    }
    async fn leases(&self, include_fenced: bool) -> Result<Vec<LeaseRef>, OrchestrationError> {
        let k = self.plan_key();
        let rows=sqlx::query("SELECT a.task_id,a.attempt_id,a.epoch FROM task_attempts a JOIN attempt_states s USING(project_id,attempt_id) WHERE a.project_id=? AND a.plan_id=? AND a.version=? AND (s.state='admitted' OR (? AND s.state='fenced')) AND NOT EXISTS(SELECT 1 FROM task_attempts newer WHERE newer.project_id=a.project_id AND newer.plan_id=a.plan_id AND newer.version=a.version AND newer.task_id=a.task_id AND newer.epoch>a.epoch) ORDER BY a.task_id LIMIT 100")
   .bind(k.project_id.as_str()).bind(k.plan_id.as_str()).bind(version(k)?).bind(include_fenced).fetch_all(&self.store.pool).await?;
        rows.into_iter()
            .map(|r| {
                Ok(LeaseRef {
                    task: self.reference(
                        &TaskId::new(r.get::<String, _>(0)).map_err(|_| StoreError::Integrity)?,
                    ),
                    attempt_id: AttemptId::new(r.get::<String, _>(1))
                        .map_err(|_| StoreError::Integrity)?,
                    epoch: r.get::<i64, _>(2) as u64,
                })
            })
            .collect()
    }
    pub async fn snapshot(&self) -> Result<RunSnapshot, OrchestrationError> {
        let k = self.plan_key();
        let row=sqlx::query("SELECT count(*),coalesce(sum(r.state='running'),0),coalesce(sum(a.task_id IS NOT NULL AND r.state='succeeded'),0),coalesce(sum(t.kind='executable_leaf' AND r.state IN ('planned','waiting','ready') AND EXISTS(SELECT 1 FROM task_dependencies d LEFT JOIN task_acceptances x ON x.project_id=d.project_id AND x.plan_id=d.plan_id AND x.version=d.version AND x.task_id=d.predecessor_id WHERE d.project_id=t.project_id AND d.plan_id=t.plan_id AND d.version=t.version AND d.successor_id=t.task_id AND x.task_id IS NULL)),0) FROM tasks t JOIN task_runtime r USING(project_id,plan_id,version,task_id) LEFT JOIN task_acceptances a USING(project_id,plan_id,version,task_id) WHERE t.project_id=? AND t.plan_id=? AND t.version=?")
   .bind(k.project_id.as_str()).bind(k.plan_id.as_str()).bind(version(k)?).fetch_one(&self.store.pool).await?;
        let received:i64=sqlx::query_scalar("SELECT count(*) FROM worker_sessions s JOIN task_attempts a USING(project_id,attempt_id) JOIN attempt_states st USING(project_id,attempt_id) WHERE a.project_id=? AND a.plan_id=? AND a.version=? AND json_extract(s.state,'$.phase')='ResultReceived' AND st.state='admitted'").bind(k.project_id.as_str()).bind(k.plan_id.as_str()).bind(version(k)?).fetch_one(&self.store.pool).await?;
        let recovery:i64=sqlx::query_scalar("SELECT count(*) FROM dispatch_operations d JOIN task_attempts a USING(project_id,attempt_id) WHERE a.project_id=? AND a.plan_id=? AND a.version=? AND d.state IN ('reconcile','launching','blocked')").bind(k.project_id.as_str()).bind(k.plan_id.as_str()).bind(version(k)?).fetch_one(&self.store.pool).await?;
        Ok(RunSnapshot {
            total: row.get::<i64, _>(0) as u64,
            running: row.get::<i64, _>(1) as u64,
            accepted: row.get::<i64, _>(2) as u64,
            waiting_dependencies: row.get::<i64, _>(3) as u64,
            received_unvalidated: received as u64,
            recovery_required: recovery as u64,
            uncertain_liabilities:self.store.financial_snapshot(&k.project_id).await.map_err(AdmissionError::from)?.uncertain_liabilities,
            ready:sqlx::query_scalar::<_,i64>("SELECT count(*) FROM tasks t JOIN task_runtime r USING(project_id,plan_id,version,task_id) WHERE t.project_id=? AND t.plan_id=? AND t.version=? AND t.kind='executable_leaf' AND r.state IN ('planned','waiting','ready') AND NOT EXISTS(SELECT 1 FROM task_dependencies d LEFT JOIN task_acceptances a ON a.project_id=d.project_id AND a.plan_id=d.plan_id AND a.version=d.version AND a.task_id=d.predecessor_id WHERE d.project_id=t.project_id AND d.plan_id=t.plan_id AND d.version=t.version AND d.successor_id=t.task_id AND a.task_id IS NULL)").bind(k.project_id.as_str()).bind(k.plan_id.as_str()).bind(version(k)?).fetch_one(&self.store.pool).await? as u64,
        })
    }
    /// One bounded work pass. Ready reads are advisory; R06 rechecks all authority.
    pub async fn pass(
        &self,
        authority: &impl SyntheticExecutionAuthority,
        clock: &impl Clock,
        limits: PassLimits,
    ) -> Result<PassReport, OrchestrationError> {
        self.pass_with_adapter(
            authority,
            &SyntheticProvider::new(self.store.clone()),
            clock,
            limits,
        )
        .await
    }
    pub async fn pass_with_adapter(
        &self,
        authority: &impl SyntheticExecutionAuthority,
        adapter: &impl LaunchAdapter,
        clock: &impl Clock,
        limits: PassLimits,
    ) -> Result<PassReport, OrchestrationError> {
        limits.check()?;
        let mut report = PassReport::default();
        let now = admission::at(clock)?;
        let dispatcher = OutboxDispatcher::new(self.store.clone());
        for expired in dispatcher
            .recover_expired(
                &self.plan_key().project_id,
                &authority.actor(),
                authority,
                clock,
                100,
            )
            .await?
        {
            if let Err(e) = expired.result {
                report.blocked.push(TaskBlock {
                    task: expired.lease.task,
                    reason: e.into(),
                });
            }
        }
        self.recover_failures(authority, adapter, clock, &mut report)
            .await?;
        for task_id in self
            .task_batch("executable_leaf", limits.admissions)
            .await?
        {
            let task = self.task(&task_id)?;
            if !matches!(
                task.execution.as_ref().map(|e| &e.write_scope),
                Some(WriteScope::ReadOnly {})
            ) {
                report.blocked.push(TaskBlock {
                    task: self.reference(&task_id),
                    reason: OrchestrationError::UnsupportedCriteria,
                });
                continue;
            }
            let (epoch, count) = self.attempt_summary(&task_id).await?;
            let result = async {
                artifacts::supported_task(task)?;
                self.inputs(task).await?;
                let ordinal =
                    u8::try_from(count + 1).map_err(|_| OrchestrationError::LimitExceeded)?;
                let assignment = authority.assignment(task, ordinal, now)?;
                let request = AdmissionRequest {
                    task: self.reference(&task_id),
                    fingerprint: self.fingerprint().into(),
                    expected_epoch: epoch,
                    request_id: id("attempt", &(self.plan_key(), &task_id, epoch, ordinal))?,
                    actor: authority.actor(),
                    permit: assignment.permit,
                };
                Ok::<_, OrchestrationError>(
                    self.store
                        .admit(&request, authority, clock, self.limits)
                        .await?
                        .lease,
                )
            }
            .await;
            match result {
                Ok(lease) => report.admitted.push(lease),
                Err(reason) => report.blocked.push(TaskBlock {
                    task: self.reference(&task_id),
                    reason,
                }),
            }
        }
        // Admit the ready batch before driving any worker. Independent attempts coexist.
        for lease in self.leases(false).await? {
            let result = async {
                let ticket = dispatcher
                    .claim(&lease, &authority.actor(), authority, clock, self.limits)
                    .await?;
                let state = dispatcher
                    .dispatch(&ticket, adapter, authority, clock, self.limits)
                    .await?;
                if state != DispatchState::Active {
                    return Err(OrchestrationError::RecoveryRequired);
                }
                self.ensure_job(&lease, authority, clock).await
            }
            .await;
            if let Err(reason) = result {
                report.blocked.push(TaskBlock {
                    task: lease.task,
                    reason,
                });
            }
        }
        self.deliver(authority, clock, limits.events, &mut report)
            .await?;
        let controller = DurableWorkerController::new(self.store.clone());
        let mut validations = 0;
        for lease in self.leases(false).await? {
            if validations >= limits.validations {
                break;
            }
            match controller.received_outcome(&lease).await {
                Ok(Some(ExecutionOutcome::Succeeded { .. })) => {
                    validations += 1;
                    match self
                        .validator()
                        .validate_leaf(&lease, authority, clock)
                        .await
                    {
                        Ok(r) => report.validated.push(r.task),
                        Err(reason) => report.blocked.push(TaskBlock {
                            task: lease.task,
                            reason,
                        }),
                    }
                }
                Ok(Some(ExecutionOutcome::Failed { class })) => report.blocked.push(TaskBlock {
                    task: lease.task,
                    reason: OrchestrationError::ExecutionFailed(class),
                }),
                Ok(None) | Err(WorkerError::UnknownAttempt) => {}
                Err(e) => report.blocked.push(TaskBlock {
                    task: lease.task,
                    reason: e.into(),
                }),
            }
        }
        // Iterative bottom-up roll-up: the SQL predicate selects only accepted children.
        // At most one bounded group batch per pass; nested parents wake on later passes.
        for task_id in self.task_batch("group", limits.groups).await? {
            match self
                .validator()
                .aggregate(&self.reference(&task_id), authority, clock)
                .await
            {
                Ok(r) => report.aggregated.push(r.task),
                Err(reason) => report.blocked.push(TaskBlock {
                    task: self.reference(&task_id),
                    reason,
                }),
            }
        }
        report.snapshot = self.snapshot().await?;
        report.next_wake = self.next_wake(now).await?;
        Ok(report)
    }
    async fn attempt_summary(&self, task: &TaskId) -> Result<(u64, u64), OrchestrationError> {
        let k = self.plan_key();
        let row=sqlx::query("SELECT coalesce((SELECT epoch FROM task_fences f WHERE f.project_id=? AND f.plan_id=? AND f.version=? AND f.task_id=?),0),(SELECT count(*) FROM task_attempts a WHERE a.project_id=? AND a.plan_id=? AND a.version=? AND a.task_id=?)").bind(k.project_id.as_str()).bind(k.plan_id.as_str()).bind(version(k)?).bind(task.as_str()).bind(k.project_id.as_str()).bind(k.plan_id.as_str()).bind(version(k)?).bind(task.as_str()).fetch_one(&self.store.pool).await?;
        Ok((row.get::<i64, _>(0) as u64, row.get::<i64, _>(1) as u64))
    }
    async fn ensure_job(
        &self,
        l: &LeaseRef,
        authority: &impl SyntheticExecutionAuthority,
        clock: &impl Clock,
    ) -> Result<(), OrchestrationError> {
        let mut read = self.store.pool.begin().await?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM synthetic_jobs WHERE project_id=? AND attempt_id=?)",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_one(&mut *read)
        .await?;
        if exists {
            artifacts::job(&mut read, l).await?;
            read.commit().await?;
            return Ok(());
        }
        let (admitted, _, _) = admission::stored_admission(&mut read, l).await?;
        let issued: i64 = sqlx::query_scalar(
            "SELECT admitted_at FROM task_attempts WHERE project_id=? AND attempt_id=?",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_one(&mut *read)
        .await?;
        read.commit().await?;
        let (_, count) = self.attempt_summary(&l.task.task_id).await?;
        let expected = id(
            "attempt",
            &(
                self.plan_key(),
                &l.task.task_id,
                admitted.expected_epoch,
                u8::try_from(count).map_err(|_| OrchestrationError::LimitExceeded)?,
            ),
        )?;
        if admitted.request_id != expected || admitted.actor != authority.actor() {
            return Err(OrchestrationError::Conflict);
        }
        let mut assignment = authority.assignment(
            self.task(&l.task.task_id)?,
            u8::try_from(count).map_err(|_| OrchestrationError::LimitExceeded)?,
            issued,
        )?;
        if assignment.permit != admitted.permit {
            return Err(OrchestrationError::Conflict);
        }
        assignment.permit = admitted.permit;
        let binding = DurableWorkerController::new(self.store.clone())
            .binding(l)
            .await?;
        let inputs = self.inputs(self.task(&l.task.task_id)?).await?;
        let start = admission::at(clock)?;
        let script = FakeWorker::new(
            binding.clone(),
            assignment.capabilities.clone(),
            assignment.seed,
        )?
        .with_inputs(inputs.clone())?
        .script(assignment.behavior, start)?;
        let job = Job {
            format: 1,
            lease: l.clone(),
            binding,
            assignment,
            inputs,
        };
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        admission::live(&mut tx, l, admission::at(clock)?).await?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM synthetic_jobs WHERE project_id=? AND attempt_id=?)",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if exists {
            tx.commit().await?;
            return Ok(());
        }
        one(sqlx::query("INSERT INTO synthetic_jobs VALUES(?,?,?,?)")
            .bind(l.task.plan.project_id.as_str())
            .bind(l.attempt_id.as_str())
            .bind(start)
            .bind(String::from_utf8(encode(&job, 32768)?).map_err(|_| StoreError::Integrity)?)
            .execute(&mut *tx)
            .await?
            .rows_affected())?;
        if let Some(artifact) = script.artifact {
            one(
                sqlx::query("INSERT INTO synthetic_artifacts VALUES(?,?,?,?)")
                    .bind(l.task.plan.project_id.as_str())
                    .bind(l.attempt_id.as_str())
                    .bind(artifact.bytes)
                    .bind(artifact.sha256.as_str())
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
        }
        if script.events.len() > 9 {
            return Err(OrchestrationError::LimitExceeded);
        }
        for (ordinal, event) in script.events.into_iter().enumerate() {
            let (kind, payload, message) = match event.input {
                SimulatedInput::Message(bytes) => {
                    let message = decode_worker_message(&bytes).ok();
                    ("message", bytes, message)
                }
                SimulatedInput::Disconnect(b) => {
                    ("disconnect", encode(&b, MAX_MESSAGE_BYTES)?, None)
                }
            };
            let digest = if let Some(m) = &message {
                hash(&encode_worker_message(m)?)
            } else {
                hash(&payload)
            };
            one(sqlx::query(
                "INSERT INTO synthetic_worker_events VALUES(?,?,?,?,?,?,?,?, 'pending',NULL)",
            )
            .bind(l.task.plan.project_id.as_str())
            .bind(l.attempt_id.as_str())
            .bind(ordinal as i64)
            .bind(event.at)
            .bind(kind)
            .bind(payload)
            .bind(message.map(|m| m.message_id.as_str().to_owned()))
            .bind(digest)
            .execute(&mut *tx)
            .await?
            .rows_affected())?;
        }
        tx.commit().await?;
        Ok(())
    }
    async fn deliver(
        &self,
        authority: &impl SyntheticExecutionAuthority,
        clock: &impl Clock,
        limit: u16,
        report: &mut PassReport,
    ) -> Result<(), OrchestrationError> {
        if limit == 0 {
            return Ok(());
        }
        let k = self.plan_key();
        let now = admission::at(clock)?;
        let rows=sqlx::query("SELECT e.attempt_id,e.ordinal,e.kind,e.payload,e.message_id,e.digest,a.task_id,a.epoch FROM synthetic_worker_events e JOIN task_attempts a USING(project_id,attempt_id) WHERE a.project_id=? AND a.plan_id=? AND a.version=? AND e.delivery='pending' AND e.due_at<=? ORDER BY e.due_at,e.attempt_id,e.ordinal LIMIT ?").bind(k.project_id.as_str()).bind(k.plan_id.as_str()).bind(version(k)?).bind(now).bind(limit).fetch_all(&self.store.pool).await?;
        let controller = DurableWorkerController::new(self.store.clone());
        for row in rows {
            let lease = LeaseRef {
                task: self.reference(
                    &TaskId::new(row.get::<String, _>(6)).map_err(|_| StoreError::Integrity)?,
                ),
                attempt_id: AttemptId::new(row.get::<String, _>(0))
                    .map_err(|_| StoreError::Integrity)?,
                epoch: row.get::<i64, _>(7) as u64,
            };
            let payload: Vec<u8> = row.get(3);
            let message_id: Option<String> = row.get(4);
            let old: Option<String> = if let Some(message_id) = message_id {
                sqlx::query_scalar("SELECT digest FROM worker_messages WHERE project_id=? AND attempt_id=? AND message_id=?").bind(k.project_id.as_str()).bind(lease.attempt_id.as_str()).bind(message_id).fetch_optional(&self.store.pool).await?
            } else {
                None
            };
            // Internal delivery projection recovery only: no old receipt is returned to a
            // worker, and no expired execution authority is recreated by this shortcut.
            let result = if old.as_deref() == Some(row.get::<String, _>(5).as_str()) {
                Ok(())
            } else if row.get::<String, _>(2) == "disconnect" {
                Err(OrchestrationError::Disconnected)
            } else {
                match controller.binding(&lease).await {
                    Ok(binding) => controller
                        .receive(
                            &authority.peer(&binding),
                            &payload,
                            authority,
                            authority,
                            clock,
                            self.limits,
                        )
                        .await
                        .map(|_| ())
                        .map_err(Into::into),
                    Err(e) => Err(e.into()),
                }
            };
            let (delivery, rejection) = match &result {
                Ok(()) => ("delivered", None),
                Err(error) => ("rejected", Some(error.to_string())),
            };
            // Message journal is authority; projection update is replayable after a crash.
            sqlx::query("UPDATE synthetic_worker_events SET delivery=?,rejection=? WHERE project_id=? AND attempt_id=? AND ordinal=? AND delivery='pending'").bind(delivery).bind(rejection).bind(k.project_id.as_str()).bind(lease.attempt_id.as_str()).bind(row.get::<i64,_>(1)).execute(&self.store.pool).await?;
            report.delivered += 1;
            if let Err(reason) = result {
                report.blocked.push(TaskBlock {
                    task: lease.task,
                    reason,
                });
            }
        }
        Ok(())
    }
    async fn recover_failures(
        &self,
        authority: &impl SyntheticExecutionAuthority,
        adapter: &impl LaunchAdapter,
        clock: &impl Clock,
        report: &mut PassReport,
    ) -> Result<(), OrchestrationError> {
        let d = OutboxDispatcher::new(self.store.clone());
        let controller = DurableWorkerController::new(self.store.clone());
        for lease in self.leases(true).await? {
            let Ok(Some(ExecutionOutcome::Failed { class })) =
                controller.received_outcome(&lease).await
            else {
                continue;
            };
            let request = FenceRequest {
                lease: lease.clone(),
                request_id: id("failure", &lease)?,
                actor: authority.actor(),
                evidence: EvidenceRef::new("synthetic-durable-failure")?,
            };
            if let Err(reason) = d
                .recover_attempt(&request, RecoveryCause::Fenced, authority, clock)
                .await
            {
                report.blocked.push(TaskBlock {
                    task: lease.task,
                    reason: reason.into(),
                });
                continue;
            }
            if class != FailureClass::Transient {
                report.blocked.push(TaskBlock {
                    task: lease.task,
                    reason: OrchestrationError::ExecutionFailed(class),
                });
                continue;
            }
            let mut read = self.store.pool.begin().await?;
            let independent = match artifacts::job(&mut read, &lease).await {
                Ok(job) => job.assignment.behavior == FakeBehavior::TransientFailure,
                Err(OrchestrationError::MissingArtifact) => false,
                Err(error) => return Err(error),
            };
            read.commit().await?;
            if !independent {
                report.blocked.push(TaskBlock {
                    task: lease.task,
                    reason: OrchestrationError::FailureClassification,
                });
                continue;
            }
            // A worker failure is not provider termination or final billing evidence.
            // Only separately supplied trusted observations/reconciliation can unblock retry.
            let state = d
                .reconcile(&lease, adapter, authority, clock, self.limits)
                .await?;
            let balance = self
                .store
                .financial_liability(
                    &lease.task.plan.project_id,
                    &LiabilityId::new(lease.attempt_id.as_str())?,
                )
                .await
                .map_err(AdmissionError::from)?;
            if !matches!(state, DispatchState::Finished | DispatchState::NoEffect)
                || !balance.finalized
                || balance.uncertain
            {
                report.blocked.push(TaskBlock {
                    task: lease.task,
                    reason: OrchestrationError::RecoveryRequired,
                });
                continue;
            }
            let r = RecoveryRequest {
                lease: lease.clone(),
                request_id: id("retry", &lease)?,
                actor: authority.actor(),
                evidence: EvidenceRef::new("synthetic-transient-reconciled")?,
            };
            match self.store.reconcile_fenced(&r, authority, clock).await {
                Ok(after) => {
                    if after > admission::at(clock)? {
                        report.next_wake =
                            Some(report.next_wake.map_or(after, |old| old.min(after)));
                    }
                }
                Err(e) => report.blocked.push(TaskBlock {
                    task: lease.task,
                    reason: e.into(),
                }),
            }
        }
        Ok(())
    }
    async fn next_wake(&self, now: i64) -> Result<Option<i64>, OrchestrationError> {
        let k = self.plan_key();
        Ok(sqlx::query_scalar("SELECT min(wake) FROM (SELECT e.due_at AS wake FROM synthetic_worker_events e JOIN task_attempts a USING(project_id,attempt_id) JOIN attempt_states s USING(project_id,attempt_id) WHERE a.project_id=? AND a.plan_id=? AND a.version=? AND s.state='admitted' AND e.delivery='pending' AND e.due_at>? UNION ALL SELECT l.lease_until FROM worker_leases l JOIN task_attempts a USING(project_id,attempt_id) JOIN attempt_states s USING(project_id,attempt_id) WHERE a.project_id=? AND a.plan_id=? AND a.version=? AND s.state='admitted' AND l.lease_until>? UNION ALL SELECT r.not_before FROM attempt_recovery r JOIN task_attempts a USING(project_id,attempt_id) WHERE a.project_id=? AND a.plan_id=? AND a.version=? AND r.not_before>?)")
   .bind(k.project_id.as_str()).bind(k.plan_id.as_str()).bind(version(k)?).bind(now).bind(k.project_id.as_str()).bind(k.plan_id.as_str()).bind(version(k)?).bind(now).bind(k.project_id.as_str()).bind(k.plan_id.as_str()).bind(version(k)?).bind(now).fetch_one(&self.store.pool).await?)
    }
    /// Deterministic simulation driver, never a production clock or polling loop.
    pub async fn run(
        &self,
        authority: &impl SyntheticExecutionAuthority,
        clock: &DeterministicClock,
        limits: RunLimits,
    ) -> Result<RunReport, OrchestrationError> {
        if limits.passes == 0 || limits.passes > 1024 {
            return Err(OrchestrationError::LimitExceeded);
        }
        for pass in 1..=limits.passes {
            let report = self.pass(authority, clock, limits.work).await?;
            if report.snapshot.accepted == report.snapshot.total {
                self.validator().verify_all().await?;
                if report.snapshot.uncertain_liabilities > 0 {
                    return Ok(RunReport {
                        status: RunStatus::Blocked,
                        passes: pass,
                        snapshot: report.snapshot,
                        blocked: Vec::new(),
                        project_block: Some(
                            AdmissionError::Finance(FinanceError::UncertainExposure).into(),
                        ),
                    });
                }
                return Ok(RunReport {
                    status: RunStatus::Complete,
                    passes: pass,
                    snapshot: report.snapshot,
                    blocked: report.blocked,
                    project_block: None,
                });
            }
            if !report.progress() {
                if let Some(wake) = report.next_wake {
                    clock.advance_to(wake)?;
                } else {
                    return Ok(RunReport {
                        status: if report.blocked.is_empty() {
                            RunStatus::Paused
                        } else {
                            RunStatus::Blocked
                        },
                        passes: pass,
                        snapshot: report.snapshot,
                        blocked: report.blocked,
                        project_block: None,
                    });
                }
            }
        }
        Ok(RunReport {
            status: RunStatus::Paused,
            passes: limits.passes,
            snapshot: self.snapshot().await?,
            blocked: Vec::new(),
            project_block: None,
        })
    }
}
