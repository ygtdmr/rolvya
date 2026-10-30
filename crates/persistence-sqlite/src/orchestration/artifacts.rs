use super::*;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    format: String,
    binding: WorkerBinding,
    seed: u64,
    #[serde(default)]
    inputs: Vec<SyntheticInput>,
    #[serde(default)]
    input_digest: Option<Digest>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    pub(super) format: u8,
    pub(super) task: TaskRef,
    pub(super) plan_hash: Digest,
    pub(super) context_hash: Digest,
    pub(super) lease: Option<LeaseRef>,
    pub(super) artifact: Option<Digest>,
    pub(super) inputs: Vec<SyntheticInput>,
    pub(super) children: Vec<SyntheticInput>,
    pub(super) criteria: Vec<String>,
    pub(super) scope: Vec<String>,
    pub(super) operation: Option<Digest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) coding: Option<Digest>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReceipt {
    pub task: TaskRef,
    pub manifest_sha256: Digest,
    pub acceptance_event: i64,
}
pub struct SyntheticResultValidator<'a> {
    pub(super) coordinator: &'a SyntheticCoordinator,
}
struct Checked<'a, P> {
    base: &'a P,
    request: ValidationRequest,
}
impl<P: SchedulerPolicy> rolvya_plan_validator::AssignmentPolicy for Checked<'_, P> {
    fn check(
        &self,
        a: &ExecutionAssignment,
    ) -> Result<(), rolvya_plan_validator::AssignmentDenial> {
        self.base.check(a)
    }
}
impl<P: SchedulerPolicy> SchedulerPolicy for Checked<'_, P> {
    fn permits(&self, owner: &PrincipalId, action: SchedulerAction<'_>, now: i64) -> bool {
        match action {
            SchedulerAction::Validate { request, task } => {
                *request == self.request
                    && self
                        .base
                        .permits(owner, SchedulerAction::Validate { request, task }, now)
            }
            _ => false,
        }
    }
}
impl SyntheticCoordinator {
    pub fn validator(&self) -> SyntheticResultValidator<'_> {
        SyntheticResultValidator { coordinator: self }
    }
    pub(super) fn task(&self, id: &TaskId) -> Result<&PlannedTask, OrchestrationError> {
        self.positions
            .get(id)
            .and_then(|i| self.plan.content.tasks.get(*i))
            .ok_or(OrchestrationError::ScopeMismatch)
    }
    pub(super) fn reference(&self, id: &TaskId) -> TaskRef {
        TaskRef {
            plan: self.plan.content.key.clone(),
            task_id: id.clone(),
        }
    }
    pub(super) async fn inputs(
        &self,
        task: &PlannedTask,
    ) -> Result<Vec<SyntheticInput>, OrchestrationError> {
        let mut tx = self.store.pool.begin().await?;
        let mut inputs = Vec::with_capacity(task.dependencies.len());
        if task.dependencies.len() > 64 {
            return Err(OrchestrationError::LimitExceeded);
        }
        for dependency in &task.dependencies {
            let digest = accepted_digest(&mut tx, dependency).await?;
            inputs.push(SyntheticInput {
                task: dependency.clone(),
                manifest_sha256: digest,
            });
        }
        self.verify_many(
            &mut tx,
            inputs
                .iter()
                .map(|i| (i.task.clone(), i.manifest_sha256.clone()))
                .collect(),
        )
        .await?;
        if task.upstream_artifacts.len() > 64 {
            return Err(OrchestrationError::LimitExceeded);
        }
        for expected in &task.upstream_artifacts {
            let input = inputs
                .iter()
                .find(|i| i.task == expected.predecessor)
                .ok_or(OrchestrationError::InvalidInputs)?;
            let manifest:Vec<u8>=sqlx::query_scalar("SELECT manifest FROM synthetic_validations WHERE project_id=? AND plan_id=? AND version=? AND task_id=? AND sha256=?").bind(input.task.plan.project_id.as_str()).bind(input.task.plan.plan_id.as_str()).bind(version(&input.task.plan)?).bind(input.task.task_id.as_str()).bind(input.manifest_sha256.as_str()).fetch_one(&mut *tx).await?;
            let checked: Manifest =
                serde_json::from_slice(&manifest).map_err(|_| StoreError::Integrity)?;
            let actual = match expected.name.as_str() {
                "result.json" => checked.artifact.as_ref().map(Digest::as_str),
                "aggregate.json" if checked.lease.is_none() => Some(input.manifest_sha256.as_str()),
                _ => None,
            };
            if actual != Some(expected.sha256.as_str()) {
                return Err(OrchestrationError::InvalidInputs);
            }
        }
        tx.commit().await?;
        inputs.sort_by(|a, b| a.task.cmp(&b.task));
        Ok(inputs)
    }
    pub(super) async fn verify_tree(
        &self,
        c: &mut SqliteConnection,
        root: &TaskRef,
        digest: &Digest,
    ) -> Result<(), OrchestrationError> {
        self.verify_many(c, vec![(root.clone(), digest.clone())])
            .await
    }
    pub(super) async fn verify_many(
        &self,
        c: &mut SqliteConnection,
        mut pending: Vec<(TaskRef, Digest)>,
    ) -> Result<(), OrchestrationError> {
        let mut seen = BTreeMap::new();
        let mut work = 0;
        while let Some((task, expected)) = pending.pop() {
            work += 1;
            if work > 4096 {
                return Err(OrchestrationError::LimitExceeded);
            }
            if task.plan != self.plan.content.key {
                return Err(OrchestrationError::ScopeMismatch);
            }
            if let Some(previous) = seen.insert(task.clone(), expected.clone()) {
                if previous != expected {
                    return Err(OrchestrationError::Conflict);
                }
                continue;
            }
            let row=sqlx::query("SELECT v.manifest FROM task_acceptances a JOIN synthetic_validations v ON v.project_id=a.project_id AND v.plan_id=a.plan_id AND v.version=a.version AND v.task_id=a.task_id AND v.sha256=a.manifest_sha256 WHERE a.project_id=? AND a.plan_id=? AND a.version=? AND a.task_id=? AND a.manifest_sha256=? AND a.fingerprint=?")
    .bind(task.plan.project_id.as_str()).bind(task.plan.plan_id.as_str()).bind(version(&task.plan)?).bind(task.task_id.as_str()).bind(expected.as_str()).bind(self.plan.revision.fingerprint()).fetch_optional(&mut *c).await?.ok_or(OrchestrationError::MissingArtifact)?;
            let bytes: Vec<u8> = row.get(0);
            if hash(&bytes) != expected.as_str() {
                return Err(OrchestrationError::InvalidArtifact);
            }
            let manifest: Manifest =
                serde_json::from_slice(&bytes).map_err(|_| OrchestrationError::InvalidArtifact)?;
            let planned = self.task(&task.task_id)?;
            if !matches!(manifest.format, 1 | 2)
                || manifest.task != task
                || manifest.plan_hash.as_str() != self.plan.revision.fingerprint()
                || manifest.context_hash.as_str() != planned.context.sha256
                || manifest.criteria != planned.acceptance_criteria
                || manifest.scope != planned.scope
            {
                return Err(OrchestrationError::ScopeMismatch);
            }
            if planned.kind == TaskKind::ExecutableLeaf {
                let lease = manifest
                    .lease
                    .as_ref()
                    .ok_or(OrchestrationError::InvalidArtifact)?;
                if manifest.coding.is_some() {
                    self.verify_coding_in(c, &manifest).await?;
                } else {
                    if manifest.format != 1 {
                        return Err(OrchestrationError::InvalidArtifact);
                    }
                    let artifact = artifact_bytes(c, lease).await?;
                    let digest = manifest
                        .artifact
                        .as_ref()
                        .ok_or(OrchestrationError::InvalidArtifact)?;
                    if hash(&artifact) != digest.as_str() {
                        return Err(OrchestrationError::InvalidArtifact);
                    }
                    let job = job(c, lease).await?;
                    check_artifact(planned, &job, &artifact, digest)?;
                    if manifest.inputs != job.inputs || !manifest.children.is_empty() {
                        return Err(OrchestrationError::InvalidInputs);
                    }
                }
            } else {
                if manifest.coding.is_some() || manifest.format != 1 {
                    return Err(OrchestrationError::InvalidArtifact);
                }

                if manifest.lease.is_some()
                    || manifest.artifact.is_some()
                    || manifest.operation.is_some()
                {
                    return Err(OrchestrationError::InvalidArtifact);
                }
                let children: BTreeSet<_> = self
                    .children
                    .get(&task.task_id)
                    .into_iter()
                    .flatten()
                    .map(|id| self.reference(id))
                    .collect();
                let recorded: BTreeSet<_> =
                    manifest.children.iter().map(|i| i.task.clone()).collect();
                if children != recorded || children.len() != manifest.children.len() {
                    return Err(OrchestrationError::InvalidArtifact);
                }
            }
            for input in manifest.inputs.into_iter().chain(manifest.children) {
                if accepted_digest(c, &input.task).await? != input.manifest_sha256 {
                    return Err(OrchestrationError::InvalidInputs);
                }
                pending.push((input.task, input.manifest_sha256));
            }
        }
        Ok(())
    }
}
fn check_artifact(
    task: &PlannedTask,
    job: &Job,
    bytes: &[u8],
    digest: &Digest,
) -> Result<(), OrchestrationError> {
    if bytes.is_empty() || bytes.len() > MAX_MESSAGE_BYTES || hash(bytes) != digest.as_str() {
        return Err(OrchestrationError::InvalidArtifact);
    }
    let artifact: Artifact =
        serde_json::from_slice(bytes).map_err(|_| OrchestrationError::InvalidArtifact)?;
    if artifact.format != "rolvya-synthetic-artifact-v1"
        || artifact.binding != job.binding
        || artifact.seed != job.assignment.seed
        || artifact.binding.task != job.lease.task
        || artifact.binding.epoch != job.lease.epoch
        || artifact.binding.context_hash.as_str() != task.context.sha256
    {
        return Err(OrchestrationError::InvalidArtifact);
    }
    let expected_input_digest = if job.inputs.is_empty() {
        None
    } else {
        Some(Digest::new(hash(&encode(&job.inputs, 32768)?))?)
    };
    if artifact.inputs != job.inputs || artifact.input_digest != expected_input_digest {
        return Err(OrchestrationError::InvalidInputs);
    }
    supported_task(task)?;
    Ok(())
}
async fn artifact_bytes(
    c: &mut SqliteConnection,
    l: &LeaseRef,
) -> Result<Vec<u8>, OrchestrationError> {
    let row = sqlx::query(
        "SELECT content,sha256 FROM synthetic_artifacts WHERE project_id=? AND attempt_id=?",
    )
    .bind(l.task.plan.project_id.as_str())
    .bind(l.attempt_id.as_str())
    .fetch_optional(c)
    .await?
    .ok_or(OrchestrationError::MissingArtifact)?;
    let bytes: Vec<u8> = row.get(0);
    if hash(&bytes) != row.get::<String, _>(1) {
        return Err(OrchestrationError::InvalidArtifact);
    }
    Ok(bytes)
}
pub(super) async fn job(c: &mut SqliteConnection, l: &LeaseRef) -> Result<Job, OrchestrationError> {
    let text: String = sqlx::query_scalar(
        "SELECT specification FROM synthetic_jobs WHERE project_id=? AND attempt_id=?",
    )
    .bind(l.task.plan.project_id.as_str())
    .bind(l.attempt_id.as_str())
    .fetch_optional(c)
    .await?
    .ok_or(OrchestrationError::MissingArtifact)?;
    let job: Job = serde_json::from_str(&text).map_err(|_| StoreError::Integrity)?;
    if job.format != 1 || job.lease != *l {
        return Err(OrchestrationError::ScopeMismatch);
    }
    Ok(job)
}
pub(super) async fn accepted_digest(
    c: &mut SqliteConnection,
    t: &TaskRef,
) -> Result<Digest, OrchestrationError> {
    let sha:String=sqlx::query_scalar("SELECT a.manifest_sha256 FROM task_acceptances a JOIN task_runtime r USING(project_id,plan_id,version,task_id) WHERE a.project_id=? AND a.plan_id=? AND a.version=? AND a.task_id=? AND r.state='succeeded'").bind(t.plan.project_id.as_str()).bind(t.plan.plan_id.as_str()).bind(version(&t.plan)?).bind(t.task_id.as_str()).fetch_optional(c).await?.ok_or(AdmissionError::DependencyIncomplete)?;
    Ok(Digest::new(sha)?)
}
impl SyntheticResultValidator<'_> {
    /// Recheck durable accepted evidence, without issuing execution privileges.
    pub async fn verify_accepted(&self, task: &TaskRef) -> Result<Digest, OrchestrationError> {
        let co = self.coordinator;
        let mut tx = co.store.pool.begin().await?;
        let digest = accepted_digest(&mut tx, task).await?;
        co.verify_tree(&mut tx, task, &digest).await?;
        tx.commit().await?;
        Ok(digest)
    }
    pub async fn verify_all(&self) -> Result<(), OrchestrationError> {
        let co = self.coordinator;
        let mut tx = co.store.pool.begin().await?;
        let mut roots = Vec::with_capacity(co.positions.len());
        for task in &co.plan.content.tasks {
            let reference = co.reference(&task.id);
            let digest = accepted_digest(&mut tx, &reference).await?;
            roots.push((reference, digest));
        }
        co.verify_many(&mut tx, roots).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn validate_leaf<P: SchedulerPolicy>(
        &self,
        l: &LeaseRef,
        policy: &P,
        clock: &impl Clock,
    ) -> Result<ValidationReceipt, OrchestrationError> {
        let co = self.coordinator;
        if l.task.plan != co.plan.content.key {
            return Err(OrchestrationError::ScopeMismatch);
        }
        let task = co.task(&l.task.task_id)?;
        if task.kind != TaskKind::ExecutableLeaf {
            return Err(AdmissionError::NotEligible.into());
        }
        let outcome = DurableWorkerController::new(co.store.clone())
            .received_outcome(l)
            .await?;
        let Some(ExecutionOutcome::Succeeded { manifest_sha256 }) = outcome else {
            return Err(OrchestrationError::MissingResult);
        };
        let binding = DurableWorkerController::new(co.store.clone())
            .binding(l)
            .await?;
        let mut tx = co.store.pool.begin().await?;
        let job = job(&mut tx, l).await?;
        if job.binding != binding || job.binding.plan_hash.as_str() != co.fingerprint() {
            return Err(OrchestrationError::ScopeMismatch);
        }
        let bytes = artifact_bytes(&mut tx, l).await?;
        check_artifact(task, &job, &bytes, &manifest_sha256)?;
        let operation: String = sqlx::query_scalar(
            "SELECT digest FROM dispatch_operations WHERE project_id=? AND attempt_id=?",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(OrchestrationError::MissingArtifact)?;
        tx.commit().await?;
        if co.inputs(task).await? != job.inputs {
            return Err(OrchestrationError::InvalidInputs);
        }
        let manifest = Manifest {
            format: 1,
            task: l.task.clone(),
            plan_hash: Digest::new(co.plan.revision.fingerprint())?,
            context_hash: Digest::new(&task.context.sha256)?,
            lease: Some(l.clone()),
            artifact: Some(manifest_sha256),
            inputs: job.inputs,
            children: Vec::new(),
            criteria: task.acceptance_criteria.clone(),
            scope: task.scope.clone(),
            operation: Some(Digest::new(operation)?),
            coding: None,
        };
        self.commit(&manifest, policy, clock).await
    }
    pub async fn aggregate<P: SchedulerPolicy>(
        &self,
        t: &TaskRef,
        policy: &P,
        clock: &impl Clock,
    ) -> Result<ValidationReceipt, OrchestrationError> {
        let co = self.coordinator;
        if t.plan != co.plan.content.key {
            return Err(OrchestrationError::ScopeMismatch);
        }
        let task = co.task(&t.task_id)?;
        if task.kind != TaskKind::Group || task.acceptance_criteria != [GROUP_CRITERION] {
            return Err(OrchestrationError::UnsupportedCriteria);
        }
        let mut children = Vec::new();
        let mut tx = co.store.pool.begin().await?;
        for child_id in co.children.get(&t.task_id).into_iter().flatten() {
            let child = co.task(child_id)?;
            if !child.required || children.len() >= 128 {
                return Err(OrchestrationError::LimitExceeded);
            }
            let reference = co.reference(&child.id);
            let digest = accepted_digest(&mut tx, &reference).await?;
            children.push(SyntheticInput {
                task: reference,
                manifest_sha256: digest,
            });
        }
        co.verify_many(
            &mut tx,
            children
                .iter()
                .map(|i| (i.task.clone(), i.manifest_sha256.clone()))
                .collect(),
        )
        .await?;
        tx.commit().await?;
        if children.is_empty() {
            return Err(OrchestrationError::UnsupportedCriteria);
        }
        children.sort_by(|a, b| a.task.cmp(&b.task));
        let manifest = Manifest {
            format: 1,
            task: t.clone(),
            plan_hash: Digest::new(co.plan.revision.fingerprint())?,
            context_hash: Digest::new(&task.context.sha256)?,
            lease: None,
            artifact: None,
            inputs: co.inputs(task).await?,
            children,
            criteria: task.acceptance_criteria.clone(),
            scope: task.scope.clone(),
            operation: None,
            coding: None,
        };
        self.commit(&manifest, policy, clock).await
    }
    async fn commit<P: SchedulerPolicy>(
        &self,
        m: &Manifest,
        policy: &P,
        clock: &impl Clock,
    ) -> Result<ValidationReceipt, OrchestrationError> {
        let co = self.coordinator;
        let bytes = encode(m, 65536)?;
        let sha = hash(&bytes);
        let identity = id("verify", &(m.task.clone(), &m.lease))?;
        let request = ValidationRequest {
            task: m.task.clone(),
            fingerprint: co.plan.revision.fingerprint().into(),
            lease: m.lease.clone(),
            request_id: identity.clone(),
            actor: PrincipalId::new("synthetic-validator")?,
            evidence: EvidenceRef::new(identity.as_str())?,
            manifest_sha256: sha.clone(),
        };
        let mut tx = co.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let now = admission::at(clock)?;
        let old: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT manifest FROM synthetic_validations WHERE project_id=? AND validation_id=?",
        )
        .bind(m.task.plan.project_id.as_str())
        .bind(identity.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        if old.as_ref().is_some_and(|old| old != &bytes) {
            return Err(OrchestrationError::Conflict);
        }
        if old.is_none() {
            if let Some(l) = &m.lease {
                admission::live(&mut tx, l, now).await?;
            }
            let owner = admission::owner(&mut tx, &m.task.plan.project_id).await?;
            if !policy.permits(
                &owner,
                SchedulerAction::Validate {
                    request: &request,
                    task: co.task(&m.task.task_id)?,
                },
                now,
            ) {
                return Err(AdmissionError::Forbidden.into());
            }
            one(
                sqlx::query("INSERT INTO synthetic_validations VALUES(?,?,?,?,?,?,?,?,?)")
                    .bind(m.task.plan.project_id.as_str())
                    .bind(m.task.plan.plan_id.as_str())
                    .bind(version(&m.task.plan)?)
                    .bind(m.task.task_id.as_str())
                    .bind(identity.as_str())
                    .bind(m.lease.as_ref().map(|l| l.attempt_id.as_str()))
                    .bind(bytes)
                    .bind(&sha)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
        }
        tx.commit().await?;
        let checked = Checked {
            base: policy,
            request: request.clone(),
        };
        let sequence = co
            .store
            .accept_task(&request, &checked, clock, co.limits)
            .await?;
        Ok(ValidationReceipt {
            task: m.task.clone(),
            manifest_sha256: Digest::new(sha)?,
            acceptance_event: sequence,
        })
    }
}

pub(super) fn supported_task(task: &PlannedTask) -> Result<(), OrchestrationError> {
    let criteria: BTreeSet<_> = task
        .acceptance_criteria
        .iter()
        .map(String::as_str)
        .collect();
    if !criteria.contains(ARTIFACT_CRITERION)
        || criteria
            .iter()
            .any(|c| !matches!(*c, ARTIFACT_CRITERION | INPUTS_CRITERION))
        || (!task.dependencies.is_empty() && !criteria.contains(INPUTS_CRITERION))
        || !matches!(
            task.execution.as_ref().map(|e| &e.write_scope),
            Some(WriteScope::ReadOnly {})
        )
    {
        return Err(OrchestrationError::UnsupportedCriteria);
    }
    Ok(())
}

// Trusted publisher bridge: verifies actual accepted bytes in the caller's
// authoritative transaction. No new acceptance or execution authority is issued.
impl SyntheticCoordinator {
    pub(crate) async fn publication_artifact_in(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
    ) -> Result<(Vec<u8>, Digest, String), OrchestrationError> {
        if l.task.plan != self.plan.content.key {
            return Err(OrchestrationError::ScopeMismatch);
        }
        let digest = accepted_digest(c, &l.task).await?;
        let coding: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM coding_decisions WHERE project_id=? AND attempt_id=?)",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_one(&mut *c)
        .await?;
        if coding {
            return Err(OrchestrationError::ScopeMismatch);
        }
        self.verify_tree(c, &l.task, &digest).await?;
        let exact:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM task_acceptances a JOIN task_fences f USING(project_id,plan_id,version,task_id) JOIN attempt_states s ON s.project_id=a.project_id AND s.attempt_id=a.attempt_id WHERE a.project_id=? AND a.plan_id=? AND a.version=? AND a.task_id=? AND a.attempt_id=? AND a.epoch=? AND f.current_attempt=a.attempt_id AND f.epoch=a.epoch AND s.state='succeeded')")
            .bind(l.task.plan.project_id.as_str()).bind(l.task.plan.plan_id.as_str()).bind(version(&l.task.plan)?).bind(l.task.task_id.as_str()).bind(l.attempt_id.as_str()).bind(l.epoch as i64).fetch_one(&mut *c).await?;
        if !exact {
            return Err(OrchestrationError::ScopeMismatch);
        }
        Ok((
            artifact_bytes(c, l).await?,
            digest,
            self.plan.revision.fingerprint().into(),
        ))
    }
}
