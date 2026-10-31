//! Bounded task data. A deserialized package is never server-side authority.
use super::store::{decode, same};
use super::*;
use crate::controller_keys::github_app::GitSha;

pub const WORK_VERSION: u16 = 1;
pub const SOURCE_CHUNK_BYTES: usize = 16 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEntry {
    pub path: String,
    pub mode: String,
    pub git_blob: GitSha,
    pub sha256: Digest,
    pub bytes: u32,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodingWork {
    pub version: u16,
    pub identity: Digest,
    pub binding: WorkerBinding,
    pub context: CodingContext,
    pub profile: ExecutionProfile,
    pub connection: String,
    pub provider: EvidenceRef,
    pub account: EvidenceRef,
    pub financial_event: i64,
    pub operation: Digest,
    pub inputs: Vec<SyntheticInput>,
    pub source: Vec<SourceEntry>,
    pub created_at: i64,
    pub expires_at: i64,
    pub initial_lease_until: i64,
    pub criteria: Vec<String>,
    pub goal: String,
    pub instructions: String,
    pub exclusions: Vec<String>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceChunk {
    pub work: Digest,
    pub file: u16,
    pub offset: u32,
    pub bytes: Vec<u8>,
    pub sha256: Digest,
}
impl std::fmt::Debug for SourceChunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceChunk")
            .field("file", &self.file)
            .field("offset", &self.offset)
            .finish_non_exhaustive()
    }
}
impl CodingWork {
    /// Worker-side integrity only. The server independently reloads authority.
    pub fn validate(&self, expected: &WorkerBinding, now: i64) -> Result<(), OrchestrationError> {
        if self.version != WORK_VERSION
            || &self.binding != expected
            || self.binding.context_hash != commitment(&self.context)?
            || self.created_at < 0
            || now < self.created_at
            || now >= self.expires_at
            || self.initial_lease_until <= self.created_at
            || self.criteria != [CODING_CRITERION]
            || self.source.len() > 256
            || self.inputs.len() > 64
            || self.connection.is_empty()
            || self.connection.len() > 128
        {
            return Err(OrchestrationError::ScopeMismatch);
        }
        self.binding.validate()?;
        let mut copy = self.clone();
        copy.identity = Digest::new("0".repeat(64))?;
        same(&commitment(&copy)?, &self.identity)?;
        let mut names = BTreeSet::new();
        let mut total = 0u32;
        for f in &self.source {
            total = total
                .checked_add(f.bytes)
                .ok_or(OrchestrationError::LimitExceeded)?;
            if f.bytes > 65536
                || !names.insert(&f.path)
                || !matches!(f.mode.as_str(), "100644" | "100755")
            {
                return Err(OrchestrationError::InvalidArtifact);
            }
        }
        if total as usize > MAX_SOURCE_BYTES {
            return Err(OrchestrationError::LimitExceeded);
        }
        Ok(())
    }
    pub fn reconstruct(&self, chunks: &[SourceChunk]) -> Result<PinnedSource, OrchestrationError> {
        let mut files = Vec::new();
        let mut n = 0;
        for (i, f) in self.source.iter().enumerate() {
            let mut bytes = Vec::with_capacity(f.bytes as usize);
            loop {
                let c = chunks.get(n).ok_or(OrchestrationError::MissingArtifact)?;
                if c.work != self.identity
                    || usize::from(c.file) != i
                    || c.offset as usize != bytes.len()
                    || c.bytes.len() > SOURCE_CHUNK_BYTES
                    || hash(&c.bytes) != c.sha256.as_str()
                    || bytes.len() + c.bytes.len() > f.bytes as usize
                    || (c.bytes.is_empty() && f.bytes != 0)
                {
                    return Err(OrchestrationError::InvalidArtifact);
                }
                bytes.extend_from_slice(&c.bytes);
                n += 1;
                if bytes.len() == f.bytes as usize {
                    break;
                }
            }
            if hash(&bytes) != f.sha256.as_str() {
                return Err(OrchestrationError::InvalidArtifact);
            }
            files.push(SourceFile {
                path: f.path.clone(),
                mode: f.mode.clone(),
                sha: f.git_blob.clone(),
                bytes,
            });
        }
        if n != chunks.len() {
            return Err(OrchestrationError::InvalidArtifact);
        }
        let source = PinnedSource {
            repository: self.context.repository,
            commit: self.context.base_commit.clone(),
            tree: self.context.base_tree.clone(),
            files,
        };
        crate::controller_keys::github_app::validate_coding_source(&source)
            .map_err(|_| OrchestrationError::InvalidArtifact)?;
        model::inventory(&source.files)?;
        same(
            &tree_commitment(&source.files)?,
            &self.context.source_sha256,
        )?;
        Ok(source)
    }
    pub fn result(
        &self,
        source: &PinnedSource,
        changes: Vec<FileChange>,
        worker_execution: String,
        verifier_execution: String,
    ) -> Result<CodingResult, OrchestrationError> {
        same(
            &tree_commitment(&source.files)?,
            &self.context.source_sha256,
        )?;
        let files = apply_changes(source, &changes, &self.context.paths)?;
        let result = CodingResult {
            format: CodingFormat::V1,
            installation: self.context.installation.clone(),
            binding: self.binding.clone(),
            repository: self.context.repository,
            base_commit: self.context.base_commit.clone(),
            base_tree: self.context.base_tree.clone(),
            source_sha256: self.context.source_sha256.clone(),
            inputs: self.inputs.clone(),
            financial_event: self.financial_event,
            operation: self.operation.clone(),
            changes,
            resulting_tree_sha256: tree_commitment(&files)?,
            worker_execution,
            verification_execution: verifier_execution,
        };
        encode(&result, MAX_CODING_BYTES)?;
        Ok(result)
    }
}
impl SyntheticCoordinator {
    pub(crate) async fn coding_source_current_in(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
        now: i64,
    ) -> Result<(CodingContext, PinnedSource, i64), OrchestrationError> {
        let paused: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM coding_recovery_holds WHERE project_id=?)",
        )
        .bind(l.task.plan.project_id.as_str())
        .fetch_one(&mut *c)
        .await?;
        if paused {
            return Err(OrchestrationError::RecoveryRequired);
        }
        let (ctx, source, v) = self.coding_source(c, &l.task).await?;
        self.coding_authority()?
            .key
            .coding_repository_current(c, &l.task.plan.project_id, v, &ctx, now)
            .await?;
        Ok((ctx, source, v))
    }
    pub(crate) async fn coding_work_in(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
        now: i64,
    ) -> Result<CodingWork, OrchestrationError> {
        let (context, source, _) = self.coding_source_current_in(c, l, now).await?;
        let (cmd, _, _) = crate::worker_control::command(c, l).await?;
        let task = self.context(&l.task, &context)?;
        let admitted = admission::live(c, l, now).await?;
        if cmd.binding().plan_hash.as_str() != self.fingerprint() {
            return Err(OrchestrationError::ScopeMismatch);
        }
        if let Some(row) =
            sqlx::query("SELECT body,seal FROM coding_work WHERE project_id=? AND attempt_id=?")
                .bind(l.task.plan.project_id.as_str())
                .bind(l.attempt_id.as_str())
                .fetch_optional(&mut *c)
                .await?
        {
            let work: CodingWork = decode(&row.get::<Vec<u8>, _>(0), 65536)?;
            self.coding_authority()?
                .verify(b"work", &work, &row.get::<Vec<u8>, _>(1))?;
            work.validate(cmd.binding(), now)?;
            same(&work.context, &context)?;
            return Ok(work);
        }
        let created: i64 = sqlx::query_scalar(
            "SELECT admitted_at FROM task_attempts WHERE project_id=? AND attempt_id=?",
        )
        .bind(l.task.plan.project_id.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_one(&mut *c)
        .await?;
        let mut inputs = Vec::new();
        for t in &task.dependencies {
            let digest = super::super::artifacts::accepted_digest(c, t).await?;
            self.verify_tree(c, t, &digest).await?;
            inputs.push(SyntheticInput {
                task: t.clone(),
                manifest_sha256: digest,
            });
        }
        let mut work = CodingWork {
            version: WORK_VERSION,
            identity: Digest::new("0".repeat(64))?,
            binding: cmd.binding().clone(),
            context,
            profile: cmd.profile().clone(),
            connection: task
                .execution
                .as_ref()
                .ok_or(OrchestrationError::ScopeMismatch)?
                .connection_ref
                .clone(),
            provider: admitted.permit.provider,
            account: admitted.permit.account,
            financial_event: cmd.financial_event(),
            operation: Digest::new(cmd.digest()?)?,
            inputs,
            source: source
                .files
                .iter()
                .map(|f| {
                    Ok(SourceEntry {
                        path: f.path.clone(),
                        mode: f.mode.clone(),
                        git_blob: f.sha.clone(),
                        sha256: Digest::new(hash(&f.bytes))?,
                        bytes: f.bytes.len() as u32,
                    })
                })
                .collect::<Result<Vec<_>, OrchestrationError>>()?,
            created_at: created,
            expires_at: admitted.permit.valid_until.min(
                created
                    .checked_add(i64::from(cmd.profile().resources.timeout_seconds))
                    .ok_or(OrchestrationError::LimitExceeded)?,
            ),
            initial_lease_until: admission::stored_admission(c, l).await?.1,
            criteria: task.acceptance_criteria.clone(),
            goal: task.goal.clone(),
            instructions: task.instructions.clone(),
            exclusions: task.exclusions.clone(),
        };
        work.identity = commitment(&work)?;
        let bytes = encode(&work, 65536)?;
        work.validate(cmd.binding(), now)?;
        let seal = self.coding_authority()?.seal(b"work", &work)?;
        one(sqlx::query("INSERT INTO coding_work VALUES(?,?,?,?,?)")
            .bind(l.task.plan.project_id.as_str())
            .bind(l.attempt_id.as_str())
            .bind(work.identity.as_str())
            .bind(bytes)
            .bind(seal)
            .execute(&mut *c)
            .await?
            .rows_affected())?;
        Ok(work)
    }
    pub(crate) async fn coding_chunk_in(
        &self,
        c: &mut SqliteConnection,
        l: &LeaseRef,
        work: &Digest,
        file: u16,
        offset: u32,
        now: i64,
    ) -> Result<SourceChunk, OrchestrationError> {
        let w = self.coding_work_in(c, l, now).await?;
        same(&w.identity, work)?;
        let (_, source, _) = self.coding_source_current_in(c, l, now).await?;
        let f = source
            .files
            .get(usize::from(file))
            .ok_or(OrchestrationError::ScopeMismatch)?;
        let o = offset as usize;
        if !o.is_multiple_of(SOURCE_CHUNK_BYTES)
            || o > f.bytes.len()
            || (o == f.bytes.len() && o != 0)
        {
            return Err(OrchestrationError::ScopeMismatch);
        }
        let bytes = f.bytes[o..(o + SOURCE_CHUNK_BYTES).min(f.bytes.len())].to_vec();
        Ok(SourceChunk {
            work: work.clone(),
            file,
            offset,
            sha256: Digest::new(hash(&bytes))?,
            bytes,
        })
    }
}

impl SyntheticCoordinator {
    /// Restriction-only local restore/incident boundary. No resume or external
    /// verification launch is authorized. Accepted historical evidence survives.
    pub async fn hold_coding_after_restore(
        &self,
        clock: &impl Clock,
    ) -> Result<(), OrchestrationError> {
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let p = &self.plan_key().project_id;
        let now = admission::at(clock)?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM coding_recovery_holds WHERE project_id=?)",
        )
        .bind(p.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if !exists {
            let seal = self.coding_authority()?.seal(b"recovery-hold", &(p, now))?;
            one(
                sqlx::query("INSERT INTO coding_recovery_holds VALUES(?,?,?)")
                    .bind(p.as_str())
                    .bind(now)
                    .bind(seal)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected(),
            )?;
        }
        tx.commit().await?;
        Ok(())
    }
}

impl std::fmt::Debug for CodingWork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodingWork")
            .field("identity", &self.identity)
            .field("binding", &self.binding)
            .finish_non_exhaustive()
    }
}
