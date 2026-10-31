use super::super::artifacts::{Manifest, accepted_digest};
use super::*;
use crate::controller_keys::github_app::{GitSha, RepositoryId};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AcceptedCoding {
    pub lease: LeaseRef,
    pub acceptance: Digest,
    pub artifact: Digest,
    pub repository: RepositoryId,
    pub base: GitSha,
    pub base_tree: GitSha,
    pub source_sha256: Digest,
    pub changes: Vec<FileChange>,
    pub resulting_tree_sha256: Digest,
    pub fingerprint: String,
}
fn decode<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    max: usize,
) -> Result<T, OrchestrationError> {
    if bytes.is_empty() || bytes.len() > max {
        return Err(OrchestrationError::LimitExceeded);
    }
    serde_json::from_slice(bytes).map_err(|_| OrchestrationError::InvalidArtifact)
}
fn same<T: PartialEq>(a: &T, b: &T) -> Result<(), OrchestrationError> {
    if a == b {
        Ok(())
    } else {
        Err(OrchestrationError::ScopeMismatch)
    }
}
impl SyntheticCoordinator {
    fn context(
        &self,
        t: &TaskRef,
        context: &CodingContext,
    ) -> Result<&PlannedTask, OrchestrationError> {
        if t.plan != *self.plan_key() {
            return Err(OrchestrationError::ScopeMismatch);
        }
        let task = self.task(&t.task_id)?;
        let authority = self.coding_authority()?;
        if task.kind != TaskKind::ExecutableLeaf
            || task.acceptance_criteria != [CODING_CRITERION]
            || context.installation != authority.installation()
            || commitment(context)?.as_str() != task.context.sha256
            || context.paths != task.scope
            || context.paths.is_empty()
            || context.paths.len() > 32
        {
            return Err(OrchestrationError::UnsupportedCriteria);
        }
        for p in &context.paths {
            model::path(p.trim_end_matches('/'))?;
        }
        match &task
            .execution
            .as_ref()
            .ok_or(OrchestrationError::ScopeMismatch)?
            .write_scope
        {
            WriteScope::Remote {
                repository,
                git_ref,
                paths,
            } if repository == &context.repository_location
                && git_ref == &format!("refs/heads/rolvya/planned/{}", t.task_id.as_str())
                && paths == &context.paths => {}
            _ => return Err(OrchestrationError::ScopeMismatch),
        }
        GitSha::parse(context.base_commit.as_str())
            .map_err(|_| OrchestrationError::InvalidArtifact)?;
        GitSha::parse(context.base_tree.as_str())
            .map_err(|_| OrchestrationError::InvalidArtifact)?;
        if context.repository.get() <= 0 {
            return Err(OrchestrationError::InvalidArtifact);
        }
        let p = &context.verification;
        let resources = &task
            .execution
            .as_ref()
            .ok_or(OrchestrationError::ScopeMismatch)?
            .resources;
        if p.identity.is_empty()
            || p.identity.len() > 120
            || p.commands.is_empty()
            || p.commands.len() > 8
            || p.commands.iter().any(|c| {
                c.is_empty()
                    || c.len() > 16
                    || c.iter()
                        .any(|a| a.is_empty() || a.len() > 256 || a.chars().any(char::is_control))
            })
            || p.required_tests.is_empty()
            || p.required_tests.len() > 64
            || p.required_tests
                .iter()
                .any(|s| s.is_empty() || s.len() > 120)
            || p.required_tests.iter().collect::<BTreeSet<_>>().len() != p.required_tests.len()
            || p.timeout_seconds == 0
            || p.timeout_seconds > resources.timeout_seconds
            || p.cpu_millis == 0
            || p.cpu_millis > resources.cpu_millis
            || p.memory_mib == 0
            || p.memory_mib > resources.memory_mib
            || p.max_output_bytes == 0
            || p.max_output_bytes > 8192
        {
            return Err(OrchestrationError::LimitExceeded);
        }
        Ok(task)
    }
    /// Called only with a source fetched by the existing trusted Git adapter.
    pub(crate) async fn pin_coding_source_in(
        &self,
        c: &mut SqliteConnection,
        t: &TaskRef,
        context: &CodingContext,
        source: &PinnedSource,
        binding_version: i64,
    ) -> Result<(), OrchestrationError> {
        self.context(t, context)?;
        same(&source.repository, &context.repository)?;
        same(&source.commit, &context.base_commit)?;
        same(&source.tree, &context.base_tree)?;
        same(&tree_commitment(&source.files)?, &context.source_sha256)?;
        crate::controller_keys::github_app::validate_coding_source(source)
            .map_err(|_| OrchestrationError::InvalidArtifact)?;
        model::inventory(&source.files)?;
        let body = (t, context, source, binding_version);
        let seal = self.coding_authority()?.seal(b"source", &body)?;
        if let Some(r)=sqlx::query("SELECT context,source,binding_version,seal FROM coding_sources WHERE project_id=? AND plan_id=? AND version=? AND task_id=?").bind(t.plan.project_id.as_str()).bind(t.plan.plan_id.as_str()).bind(version(&t.plan)?).bind(t.task_id.as_str()).fetch_optional(&mut *c).await? {
            if r.get::<Vec<u8>,_>(0)!=encode(context,32768)?||r.get::<Vec<u8>,_>(1)!=encode(source,2*1024*1024)?||r.get::<i64,_>(2)!=binding_version||r.get::<Vec<u8>,_>(3)!=seal{return Err(OrchestrationError::Conflict);}return Ok(());
        }
        one(
            sqlx::query("INSERT INTO coding_sources VALUES(?,?,?,?,?,?,?,?)")
                .bind(t.plan.project_id.as_str())
                .bind(t.plan.plan_id.as_str())
                .bind(version(&t.plan)?)
                .bind(t.task_id.as_str())
                .bind(encode(context, 32768)?)
                .bind(encode(source, 2 * 1024 * 1024)?)
                .bind(binding_version)
                .bind(seal)
                .execute(&mut *c)
                .await?
                .rows_affected(),
        )?;
        Ok(())
    }
    async fn coding_source(
        &self,
        c: &mut SqliteConnection,
        t: &TaskRef,
    ) -> Result<(CodingContext, PinnedSource, i64), OrchestrationError> {
        let r=sqlx::query("SELECT context,source,binding_version,seal FROM coding_sources WHERE project_id=? AND plan_id=? AND version=? AND task_id=?").bind(t.plan.project_id.as_str()).bind(t.plan.plan_id.as_str()).bind(version(&t.plan)?).bind(t.task_id.as_str()).fetch_optional(&mut *c).await?.ok_or(OrchestrationError::MissingArtifact)?;
        let context: CodingContext = decode(&r.get::<Vec<u8>, _>(0), 32768)?;
        let source: PinnedSource = decode(&r.get::<Vec<u8>, _>(1), 2 * 1024 * 1024)?;
        let v = r.get::<i64, _>(2);
        self.coding_authority()?.verify(
            b"source",
            &(t, &context, &source, v),
            &r.get::<Vec<u8>, _>(3),
        )?;
        self.context(t, &context)?;
        crate::controller_keys::github_app::validate_coding_source(&source)
            .map_err(|_| OrchestrationError::InvalidArtifact)?;
        same(&tree_commitment(&source.files)?, &context.source_sha256)?;
        Ok((context, source, v))
    }
    async fn structural(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
        raw: &[u8],
    ) -> Result<(CodingResult, Vec<SourceFile>, CodingContext), OrchestrationError> {
        let result: CodingResult = decode(raw, MAX_CODING_BYTES)?;
        let (context, source, _) = self.coding_source(c, &l.task).await?;
        let task = self.context(&l.task, &context)?;
        let cmd: String = sqlx::query_scalar(
            "SELECT command FROM dispatch_operations WHERE project_id=? AND attempt_id=?",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_optional(&mut *c)
        .await?
        .ok_or(OrchestrationError::MissingArtifact)?;
        let command: LaunchCommand = decode(cmd.as_bytes(), 16384)?;
        if result.binding != *command.binding()
            || result.binding.task != l.task
            || result.binding.attempt_id != l.attempt_id
            || result.binding.epoch != l.epoch
            || result.binding.plan_hash.as_str() != self.fingerprint()
            || result.binding.context_hash.as_str() != task.context.sha256
            || result.installation != context.installation
            || result.repository != context.repository
            || result.base_commit != context.base_commit
            || result.base_tree != context.base_tree
            || result.source_sha256 != context.source_sha256
            || result.operation.as_str() != command.digest()?
            || result.financial_event != command.financial_event()
            || result.worker_execution.is_empty()
            || result.worker_execution.len() > 120
            || result.verification_execution.is_empty()
            || result.verification_execution.len() > 120
        {
            return Err(OrchestrationError::ScopeMismatch);
        }
        let dependencies: BTreeSet<_> = task.dependencies.iter().collect();
        if result.inputs.len() != dependencies.len()
            || result
                .inputs
                .iter()
                .map(|i| &i.task)
                .collect::<BTreeSet<_>>()
                != dependencies
        {
            return Err(OrchestrationError::InvalidInputs);
        }
        for i in &result.inputs {
            if accepted_digest(c, &i.task).await? != i.manifest_sha256 {
                return Err(OrchestrationError::InvalidInputs);
            }
        }
        // Reverify complete predecessor trees including typed coding acceptances.
        // verify_many calls structural on prior nodes, so avoid recursion here;
        // the shared bounded walker below verifies them before final acceptance.
        for expected in &task.upstream_artifacts {
            let actual = result
                .inputs
                .iter()
                .find(|i| i.task == expected.predecessor)
                .ok_or(OrchestrationError::InvalidInputs)?;
            let bytes: Vec<u8> = sqlx::query_scalar(
                "SELECT manifest FROM synthetic_validations WHERE project_id=? AND sha256=?",
            )
            .bind(l.task.plan.project_id.as_str())
            .bind(actual.manifest_sha256.as_str())
            .fetch_one(&mut *c)
            .await?;
            let m: Manifest = decode(&bytes, 65536)?;
            let digest = match expected.name.as_str() {
                "code_change_v1" => m.artifact.as_ref(),
                "aggregate.json" if m.lease.is_none() => Some(&actual.manifest_sha256),
                _ => None,
            };
            if digest.map(Digest::as_str) != Some(expected.sha256.as_str()) {
                return Err(OrchestrationError::InvalidInputs);
            }
        }
        let files = apply_changes(&source, &result.changes, &context.paths)?;
        if tree_commitment(&files)? != result.resulting_tree_sha256 {
            return Err(OrchestrationError::InvalidArtifact);
        }
        Ok((result, files, context))
    }
    /// Bounded out-of-band artifact ingestion. This confers no acceptance authority;
    /// the existing R08 successful result receipt must independently commit its hash.
    async fn submit_coding_artifact_inner(
        &self,
        l: &LeaseRef,
        raw: &[u8],
        clock: &impl Clock,
    ) -> Result<CodingState, OrchestrationError> {
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        admission::live(&mut tx, l, admission::at(clock)?).await?;
        let (result, _, _) = self.structural(&mut tx, l, raw).await?;
        let digest = Digest::new(hash(raw))?;
        if let Some(old) = sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT content FROM coding_artifacts WHERE project_id=? AND attempt_id=?",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_optional(&mut *tx)
        .await?
        {
            same(&old, &raw.to_vec())?;
        } else {
            one(sqlx::query("INSERT INTO coding_artifacts VALUES(?,?,?,?)")
                .bind(l.task.plan.project_id.as_str())
                .bind(l.attempt_id.as_str())
                .bind(raw)
                .bind(digest.as_str())
                .execute(&mut *tx)
                .await?
                .rows_affected())?;
        }
        tx.commit().await?;
        Ok(CodingState::StructurallyValid {
            artifact_sha256: digest,
            change_set_sha256: commitment(&result.changes)?,
        })
    }
    fn report(
        &self,
        result: &CodingResult,
        raw: &[u8],
        context: &CodingContext,
        r: &VerificationReport,
    ) -> Result<(), OrchestrationError> {
        let p = &context.verification;
        if r.binding != result.binding
            || r.artifact_sha256.as_str() != hash(raw)
            || r.source_sha256 != context.source_sha256
            || r.change_set_sha256 != commitment(&result.changes)?
            || r.policy_sha256 != commitment(p)?
            || r.limits != *p
            || r.execution != result.verification_execution
            || r.verifier.is_empty()
            || r.verifier.len() > 120
            || r.verifier_version != 1
            || r.environment.is_empty()
            || r.environment.len() > 120
            || r.started_at < 0
            || r.completed_at < r.started_at
            || r.completed_at - r.started_at > i64::from(p.timeout_seconds)
            || r.terminal != VerificationTerminal::Completed
            || r.exit_code != 0
            || r.tests_completed.iter().collect::<BTreeSet<_>>()
                != p.required_tests.iter().collect::<BTreeSet<_>>()
            || r.tests_completed.len() != p.required_tests.len()
            || r.outputs.len() > 8
        {
            return Err(OrchestrationError::InvalidArtifact);
        }
        let mut total = 0usize;
        let mut names = BTreeSet::new();
        for o in &r.outputs {
            total = total
                .checked_add(o.content.len())
                .ok_or(OrchestrationError::LimitExceeded)?;
            if o.identity.is_empty()
                || o.identity.len() > 120
                || !names.insert(&o.identity)
                || hash(o.content.as_bytes()) != o.sha256.as_str()
            {
                return Err(OrchestrationError::InvalidArtifact);
            }
        }
        if total > p.max_output_bytes as usize {
            return Err(OrchestrationError::LimitExceeded);
        }
        Ok(())
    }
    async fn chronology(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
        r: &VerificationReport,
    ) -> Result<(), OrchestrationError> {
        let admitted: i64 = sqlx::query_scalar(
            "SELECT admitted_at FROM task_attempts WHERE project_id=? AND attempt_id=?",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_one(c)
        .await?;
        if r.started_at < admitted {
            return Err(OrchestrationError::InvalidArtifact);
        }
        Ok(())
    }
    /// Ordinary request data cannot create independent execution authority. The
    /// production runner adapter is unavailable. Test roots exist only under cfg(test).
    async fn verify_coding_execution_inner(
        &self,
        l: &LeaseRef,
        report: &VerificationReport,
        clock: &impl Clock,
    ) -> Result<CodingState, OrchestrationError> {
        self.coding_authority()?.verify_runner(report)?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        admission::live(&mut tx, l, admission::at(clock)?).await?;
        let raw = self.coding_raw(&mut tx, l).await?;
        let (result, _, context) = self.structural(&mut tx, l, &raw).await?;
        self.report(&result, &raw, &context, report)?;
        self.chronology(&mut tx, l, report).await?;
        let now = admission::at(clock)?;
        if report.completed_at > now {
            return Err(OrchestrationError::InvalidArtifact);
        }
        let bytes = encode(report, 32768)?;
        let seal = self
            .coding_authority()?
            .seal(b"evidence", &(l, hash(&raw), report))?;
        if let Some(old) = sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT evidence FROM coding_verifications WHERE project_id=? AND attempt_id=?",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_optional(&mut *tx)
        .await?
        {
            same(&old, &bytes)?;
        } else {
            one(
                sqlx::query("INSERT INTO coding_verifications VALUES(?,?,?,?,?)")
                    .bind(l.task.plan.project_id.as_str())
                    .bind(l.attempt_id.as_str())
                    .bind(hash(&raw))
                    .bind(&bytes)
                    .bind(seal)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
        }
        tx.commit().await?;
        Ok(CodingState::IndependentlyVerified {
            evidence_sha256: Digest::new(hash(&bytes))?,
        })
    }
    async fn coding_raw(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
    ) -> Result<Vec<u8>, OrchestrationError> {
        let row = sqlx::query(
            "SELECT content,sha256 FROM coding_artifacts WHERE project_id=? AND attempt_id=?",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_optional(c)
        .await?
        .ok_or(OrchestrationError::MissingArtifact)?;
        let raw: Vec<u8> = row.get(0);
        if raw.len() > MAX_CODING_BYTES || hash(&raw) != row.get::<String, _>(1) {
            return Err(OrchestrationError::InvalidArtifact);
        }
        Ok(raw)
    }
    async fn checked(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
    ) -> Result<(CodingResult, Vec<SourceFile>, Digest), OrchestrationError> {
        let raw = self.coding_raw(c, l).await?;
        let (result, files, context) = self.structural(c, l, &raw).await?;
        let (binding, digest, receipt) = crate::worker_control::coding_receipt_in(c, l).await?;
        if binding != result.binding || digest.as_str() != hash(&raw) || receipt.observed_at < 0 {
            return Err(OrchestrationError::MissingResult);
        }
        let row=sqlx::query("SELECT evidence,artifact_sha256,seal FROM coding_verifications WHERE project_id=? AND attempt_id=?").bind(l.task.plan.project_id.as_str()).bind(l.attempt_id.as_str()).fetch_optional(&mut *c).await?.ok_or(OrchestrationError::VerificationUnavailable)?;
        let report: VerificationReport = decode(&row.get::<Vec<u8>, _>(0), 32768)?;
        if row.get::<String, _>(1) != hash(&raw) {
            return Err(OrchestrationError::InvalidArtifact);
        }
        self.coding_authority()?.verify(
            b"evidence",
            &(l, hash(&raw), &report),
            &row.get::<Vec<u8>, _>(2),
        )?;
        self.report(&result, &raw, &context, &report)?;
        self.chronology(c, l, &report).await?;
        Ok((result, files, commitment(&(report, receipt))?))
    }
    pub async fn accept_coding_result(
        &self,
        l: &LeaseRef,
        policy: &impl SchedulerPolicy,
        clock: &impl Clock,
    ) -> Result<CodingState, OrchestrationError> {
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let (result, _, evidence) = self.checked(&mut tx, l).await?;
        self.verify_many(
            &mut tx,
            result
                .inputs
                .iter()
                .map(|i| (i.task.clone(), i.manifest_sha256.clone()))
                .collect(),
        )
        .await?;
        let task = self.task(&l.task.task_id)?;
        let m = Manifest {
            format: 2,
            task: l.task.clone(),
            plan_hash: Digest::new(self.fingerprint())?,
            context_hash: Digest::new(&task.context.sha256)?,
            lease: Some(l.clone()),
            artifact: Some(Digest::new(hash(&self.coding_raw(&mut tx, l).await?))?),
            inputs: result.inputs,
            children: vec![],
            criteria: task.acceptance_criteria.clone(),
            scope: task.scope.clone(),
            operation: Some(result.operation),
            coding: Some(evidence),
        };
        let bytes = encode(&m, 65536)?;
        let sha = hash(&bytes);
        let identity = id("verify", &(m.task.clone(), &m.lease))?;
        let request = ValidationRequest {
            task: l.task.clone(),
            fingerprint: self.fingerprint().into(),
            lease: Some(l.clone()),
            request_id: identity.clone(),
            actor: PrincipalId::new("coding-validator")?,
            evidence: EvidenceRef::new(identity.as_str())?,
            manifest_sha256: sha.clone(),
        };
        if let Some(old) = sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT manifest FROM synthetic_validations WHERE project_id=? AND validation_id=?",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(identity.as_str())
        .fetch_optional(&mut *tx)
        .await?
        {
            same(&old, &bytes)?;
        } else {
            crate::worker_control::credential_scope(
                &mut tx,
                l,
                policy,
                admission::at(clock)?,
                crate::worker_control::CredentialScopeUse::Control,
            )
            .await?;
            one(
                sqlx::query("INSERT INTO synthetic_validations VALUES(?,?,?,?,?,?,?,?,?)")
                    .bind(l.task.plan.project_id.as_str())
                    .bind(l.task.plan.plan_id.as_str())
                    .bind(version(&l.task.plan)?)
                    .bind(l.task.task_id.as_str())
                    .bind(identity.as_str())
                    .bind(l.attempt_id.as_str())
                    .bind(&bytes)
                    .bind(&sha)
                    .bind(admission::at(clock)?)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
            let seal = self.coding_authority()?.seal(b"decision", &(l, &sha))?;
            one(
                sqlx::query("INSERT INTO coding_decisions VALUES(?,?,?,?,?)")
                    .bind(l.task.plan.project_id.as_str())
                    .bind(l.attempt_id.as_str())
                    .bind(identity.as_str())
                    .bind(&sha)
                    .bind(seal)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
        }
        self.verify_coding_in(&mut tx, &m).await?;
        self.coding_authority()?.checkpoint("accept-before");
        let sequence = self
            .store
            .accept_task_in(&mut tx, &self.plan, &request, policy, clock)
            .await?;
        self.coding_authority()?.checkpoint("accept-uncommitted");
        tx.commit().await?;
        self.coding_authority()?.checkpoint("accept-after");
        Ok(CodingState::Accepted(ValidationReceipt {
            task: l.task.clone(),
            manifest_sha256: Digest::new(sha)?,
            acceptance_event: sequence,
        }))
    }
    pub(in crate::orchestration) async fn verify_coding_in(
        &self,
        c: &mut SqliteConnection,
        m: &Manifest,
    ) -> Result<(), OrchestrationError> {
        let l = m
            .lease
            .as_ref()
            .ok_or(OrchestrationError::InvalidArtifact)?;
        let sha = hash(&encode(m, 65536)?);
        let row = sqlx::query(
            "SELECT manifest_sha256,seal FROM coding_decisions WHERE project_id=? AND attempt_id=?",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_optional(&mut *c)
        .await?
        .ok_or(OrchestrationError::VerificationUnavailable)?;
        if row.get::<String, _>(0) != sha
            || m.format != 2
            || m.task != l.task
            || m.plan_hash.as_str() != self.fingerprint()
        {
            return Err(OrchestrationError::InvalidArtifact);
        }
        self.coding_authority()?
            .verify(b"decision", &(l, &sha), &row.get::<Vec<u8>, _>(1))?;
        let (result, _, evidence) = self.checked(c, l).await?;
        if m.coding.as_ref() != Some(&evidence)
            || m.artifact.as_ref().map(Digest::as_str)
                != Some(hash(&self.coding_raw(c, l).await?).as_str())
            || m.inputs != result.inputs
            || m.operation.as_ref() != Some(&result.operation)
            || !m.children.is_empty()
        {
            return Err(OrchestrationError::InvalidArtifact);
        }
        Ok(())
    }
    pub(crate) async fn coding_publication_in(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
    ) -> Result<(AcceptedCoding, PinnedSource, Vec<SourceFile>), OrchestrationError> {
        let digest = accepted_digest(c, &l.task).await?;
        self.verify_tree(c, &l.task, &digest).await?;
        let exact:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM task_acceptances a JOIN task_fences f USING(project_id,plan_id,version,task_id) JOIN attempt_states s ON s.project_id=a.project_id AND s.attempt_id=a.attempt_id WHERE a.project_id=? AND a.plan_id=? AND a.version=? AND a.task_id=? AND a.attempt_id=? AND a.epoch=? AND f.current_attempt=a.attempt_id AND f.epoch=a.epoch AND s.state='succeeded')").bind(l.task.plan.project_id.as_str()).bind(l.task.plan.plan_id.as_str()).bind(version(&l.task.plan)?).bind(l.task.task_id.as_str()).bind(l.attempt_id.as_str()).bind(l.epoch as i64).fetch_one(&mut *c).await?;
        if !exact {
            return Err(OrchestrationError::ScopeMismatch);
        }
        let (result, files, _) = self.checked(c, l).await?;
        let (_, source, _) = self.coding_source(c, &l.task).await?;
        let accepted = AcceptedCoding {
            lease: l.clone(),
            acceptance: digest,
            artifact: Digest::new(hash(&self.coding_raw(c, l).await?))?,
            repository: result.repository,
            base: result.base_commit,
            base_tree: result.base_tree,
            source_sha256: result.source_sha256,
            changes: result.changes,
            resulting_tree_sha256: result.resulting_tree_sha256,
            fingerprint: self.fingerprint().into(),
        };
        Ok((accepted, source, files))
    }
}

impl SyntheticCoordinator {
    /// Reports overlap only; does not merge, revoke acceptance or create work.
    pub async fn coding_conflicts(
        &self,
        left: &LeaseRef,
        right: &LeaseRef,
    ) -> Result<Vec<CodingConflict>, OrchestrationError> {
        let mut tx = self.store.pool.begin().await?;
        let (a, _, _) = self.coding_publication_in(&mut tx, left).await?;
        let (b, _, _) = self.coding_publication_in(&mut tx, right).await?;
        if a.repository != b.repository || a.base != b.base || a.source_sha256 != b.source_sha256 {
            return Err(OrchestrationError::ScopeMismatch);
        }
        let mut out = Vec::new();
        if left != right {
            for x in &a.changes {
                for y in &b.changes {
                    let p = x.path().to_ascii_lowercase();
                    let q = y.path().to_ascii_lowercase();
                    if p == q
                        || p.starts_with(&(q.clone() + "/"))
                        || q.starts_with(&(p.clone() + "/"))
                    {
                        out.push(CodingConflict {
                            left_path: x.path().into(),
                            right_path: y.path().into(),
                            left_artifact: a.artifact.clone(),
                            right_artifact: b.artifact.clone(),
                        });
                    }
                }
            }
        }
        tx.commit().await?;
        Ok(out)
    }
}

/// Authenticated transport stages untrusted bytes only. Full source/evidence
/// validation and acceptance are separate and never inferred from upload success.
pub(crate) async fn stage_untrusted(
    c: &mut SqliteConnection,
    b: &WorkerBinding,
    raw: &[u8],
    now: i64,
) -> Result<(), OrchestrationError> {
    let result: CodingResult = decode(raw, MAX_CODING_BYTES)?;
    if result.binding != *b {
        return Err(OrchestrationError::ScopeMismatch);
    }
    let l = LeaseRef {
        task: b.task.clone(),
        attempt_id: b.attempt_id.clone(),
        epoch: b.epoch,
    };
    admission::live(c, &l, now).await?;
    let text: String = sqlx::query_scalar(
        "SELECT definition FROM tasks WHERE project_id=? AND plan_id=? AND version=? AND task_id=?",
    )
    .bind(b.task.plan.project_id.as_str())
    .bind(b.task.plan.plan_id.as_str())
    .bind(version(&b.task.plan)?)
    .bind(b.task.task_id.as_str())
    .fetch_one(&mut *c)
    .await?;
    let task: PlannedTask = decode(text.as_bytes(), 65536)?;
    if task.acceptance_criteria != [CODING_CRITERION]
        || task.context.sha256 != b.context_hash.as_str()
    {
        return Err(OrchestrationError::UnsupportedCriteria);
    }
    if let Some(old) = sqlx::query_scalar::<_, Vec<u8>>(
        "SELECT content FROM coding_artifacts WHERE project_id=? AND attempt_id=?",
    )
    .bind(b.task.plan.project_id.as_str())
    .bind(b.attempt_id.as_str())
    .fetch_optional(&mut *c)
    .await?
    {
        same(&old, &raw.to_vec())?;
    } else {
        one(sqlx::query("INSERT INTO coding_artifacts VALUES(?,?,?,?)")
            .bind(b.task.plan.project_id.as_str())
            .bind(b.attempt_id.as_str())
            .bind(raw)
            .bind(hash(raw))
            .execute(&mut *c)
            .await?
            .rows_affected())?;
    }
    Ok(())
}

impl SyntheticCoordinator {
    pub async fn submit_coding_artifact(
        &self,
        l: &LeaseRef,
        raw: &[u8],
        clock: &impl Clock,
    ) -> Result<CodingState, OrchestrationError> {
        let result = self.submit_coding_artifact_inner(l, raw, clock).await;
        if let Err(e) = &result {
            self.coding_rejection(l, "artifact", e, clock).await?;
        }
        result
    }
    pub async fn verify_coding_execution(
        &self,
        l: &LeaseRef,
        report: &VerificationReport,
        clock: &impl Clock,
    ) -> Result<CodingState, OrchestrationError> {
        let result = self.verify_coding_execution_inner(l, report, clock).await;
        if let Err(e) = &result {
            self.coding_rejection(l, "verification", e, clock).await?;
        }
        result
    }
    async fn coding_rejection(
        &self,
        l: &LeaseRef,
        phase: &str,
        error: &OrchestrationError,
        clock: &impl Clock,
    ) -> Result<(), OrchestrationError> {
        if l.task.plan != *self.plan_key() || self.coding.is_none() {
            return Ok(());
        }
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let now = admission::at(clock)?;
        if admission::live(&mut tx, l, now).await.is_err() {
            return Ok(());
        }
        let ordinal: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM coding_rejections WHERE project_id=? AND attempt_id=?",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if ordinal < 16 {
            let reason = match error {
                OrchestrationError::VerificationUnavailable => "unavailable",
                OrchestrationError::ScopeMismatch => "scope",
                OrchestrationError::LimitExceeded => "resource",
                OrchestrationError::InvalidInputs => "dependency",
                OrchestrationError::Storage(_) => "storage",
                _ => "invalid",
            };
            one(
                sqlx::query("INSERT INTO coding_rejections VALUES(?,?,?,?,?,?)")
                    .bind(l.task.plan.project_id.as_str())
                    .bind(l.attempt_id.as_str())
                    .bind(ordinal)
                    .bind(phase)
                    .bind(reason)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
        }
        tx.commit().await?;
        Ok(())
    }
}
