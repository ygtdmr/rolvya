//! SQLx/SQLite plan, budget, admission and durable synthetic worker authority.
#![forbid(unsafe_code)]
use rolvya_domain::{
    ids::ProjectId,
    plan::{PlanContent, PlanKey, TaskKind},
};
use rolvya_plan_validator::{
    AssignmentPolicy, CANONICAL_FORMAT, Diagnostic, Limits, ValidatedPlan, validate,
};
use serde::Deserialize;
use sqlx::{
    ConnectOptions, Connection, Row, Sqlite, SqliteConnection, SqlitePool, Transaction,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{collections::BTreeSet, fmt, fs::OpenOptions, path::Path, time::Duration};

const MAX_CANONICAL_BYTES: usize = 64 * 1024 * 1024;
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    Database,
    Busy,
    Migration,
    UnknownDatabase,
    Integrity,
    Forbidden,
    NotFound,
    StaleRevision,
    AlreadyActive,
    Conflict,
    IdempotencyConflict,
    InvalidIdentifier,
    VersionOutOfRange,
    InvalidPath,
    SnapshotExists,
    InvalidPlan(Vec<Diagnostic>),
}
impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Database => "database operation failed",
                Self::Busy => "database contention exceeded its bounded wait",
                Self::Migration => "schema migration or checksum verification failed",
                Self::UnknownDatabase => "unrecognized database schema",
                Self::Integrity => "stored plan or database integrity verification failed",
                Self::Forbidden => "plan operation is not authorized",
                Self::NotFound => "project or plan not found",
                Self::StaleRevision => "expected draft revision is stale",
                Self::AlreadyActive => "project already has an active plan",
                Self::Conflict => "plan version already exists with different content",
                Self::IdempotencyConflict =>
                    "activation request identity is bound to different content or actor",
                Self::InvalidIdentifier => "invalid principal or request identifier",
                Self::VersionOutOfRange => "version exceeds SQLite positive integer range",
                Self::InvalidPath =>
                    "database path must be a private regular file on local storage",
                Self::SnapshotExists => "snapshot destination already exists",
                Self::InvalidPlan(_) => "plan validation failed",
            }
        )
    }
}
impl std::error::Error for StoreError {}
impl From<sqlx::Error> for StoreError {
    fn from(error: sqlx::Error) -> Self {
        if matches!(error, sqlx::Error::PoolTimedOut) {
            return Self::Busy;
        }
        if let sqlx::Error::Database(ref db) = error
            && db
                .code()
                .is_some_and(|code| matches!(code.as_ref(), "5" | "6" | "261" | "262" | "517"))
        {
            return Self::Busy;
        }
        Self::Database
    }
}
macro_rules! identity {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, StoreError> {
                let value = value.into();
                let b = value.as_bytes();
                if b.is_empty()
                    || b.len() > 64
                    || !b[0].is_ascii_alphanumeric()
                    || !b
                        .iter()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-".contains(c))
                {
                    return Err(StoreError::InvalidIdentifier);
                }
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let value = <String as serde::Deserialize>::deserialize(d)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}
identity!(PrincipalId);
identity!(RequestId);

pub mod admission;
pub mod finance;
pub mod worker_control;
pub mod worker_simulation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revision {
    key: PlanKey,
    fingerprint: String,
}
impl Revision {
    pub fn from_validated(plan: &ValidatedPlan<'_>) -> Self {
        Self {
            key: plan.plan().key.clone(),
            fingerprint: plan.fingerprint().to_string(),
        }
    }
    pub fn key(&self) -> &PlanKey {
        &self.key
    }
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanOperation {
    SaveDraft,
    Activate,
}
/// Trusted control-plane policy over previously authenticated, action-specific
/// evidence. Bounded and side-effect-free; no network awaits in a write transaction.
/// Supplying a matching principal string is not authentication or human consent.
pub trait PlanAuthorization {
    fn permits(
        &self,
        owner: &PrincipalId,
        actor: &PrincipalId,
        revision: &Revision,
        operation: PlanOperation,
    ) -> bool;
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationReceipt {
    pub revision: Revision,
    pub actor: PrincipalId,
    pub request_id: RequestId,
    pub sequence: i64,
}
#[derive(Debug)]
pub struct StoredPlan {
    pub content: PlanContent,
    pub revision: Revision,
    pub active: bool,
}
#[derive(Clone, Debug)]
pub struct PlanStore {
    pool: SqlitePool,
}

fn version(key: &PlanKey) -> Result<i64, StoreError> {
    i64::try_from(key.version.get()).map_err(|_| StoreError::VersionOutOfRange)
}
fn create_private(path: &Path) -> Result<(), StoreError> {
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(StoreError::InvalidPath);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(StoreError::InvalidPath);
            }
        }
        return Ok(());
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map(|_| ())
        .map_err(|_| StoreError::InvalidPath)
}

// Recognize the database read-only before applying journal/durability settings.
// SQLx still performs its own checksum/version validation when migrations run.
async fn preflight_schema(path: &Path) -> Result<(), StoreError> {
    let mut connection = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .disable_statement_logging()
        .connect()
        .await?;
    let known: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name='_sqlx_migrations'",
    )
    .fetch_one(&mut connection)
    .await?;
    let tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_one(&mut connection)
    .await?;
    if known == 0 && tables != 0 {
        return Err(StoreError::UnknownDatabase);
    }
    if known != 0 {
        let rows = sqlx::query("SELECT version,checksum,success FROM _sqlx_migrations")
            .fetch_all(&mut connection)
            .await
            .map_err(|_| StoreError::Migration)?;
        if rows.is_empty() && tables != 1 {
            return Err(StoreError::UnknownDatabase);
        }
        for row in rows {
            let v: i64 = row.try_get("version").map_err(|_| StoreError::Migration)?;
            let checksum: Vec<u8> = row.try_get("checksum").map_err(|_| StoreError::Migration)?;
            let success: bool = row.try_get("success").map_err(|_| StoreError::Migration)?;
            if !success
                || !MIGRATOR
                    .iter()
                    .any(|m| m.version == v && m.checksum.as_ref() == checksum)
            {
                return Err(StoreError::Migration);
            }
        }
    }
    connection.close().await?;
    Ok(())
}

impl PlanStore {
    /// Local controller storage only. Existing unrecognized schemas are not adopted.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        create_private(path.as_ref())?;
        preflight_schema(path.as_ref()).await?;
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .busy_timeout(Duration::from_secs(5))
            .pragma("recursive_triggers", "ON")
            .pragma("wal_autocheckpoint", "1000")
            .disable_statement_logging();
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(6))
            .connect_with(options)
            .await?;
        let store = Self { pool };
        let migration_table: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name='_sqlx_migrations'",
        )
        .fetch_one(&store.pool)
        .await?;
        let tables: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_one(&store.pool)
        .await?;
        if migration_table == 0 && tables != 0 {
            store.pool.close().await;
            return Err(StoreError::UnknownDatabase);
        }
        if MIGRATOR.run(&store.pool).await.is_err() {
            store.pool.close().await;
            return Err(StoreError::Migration);
        }
        store.check_integrity().await?;
        Ok(store)
    }
    pub async fn close(&self) {
        self.pool.close().await;
    }
    pub async fn check_integrity(&self) -> Result<(), StoreError> {
        let rows: Vec<String> = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_all(&self.pool)
            .await?;
        if rows != ["ok"]
            || !sqlx::query("PRAGMA foreign_key_check")
                .fetch_all(&self.pool)
                .await?
                .is_empty()
        {
            return Err(StoreError::Integrity);
        }
        Ok(())
    }
    /// Bootstrap management boundary; caller must authenticate/authorize project creation.
    /// This library does not expose a worker or unauthenticated API endpoint.
    pub async fn create_project(
        &self,
        project: &ProjectId,
        owner: &PrincipalId,
    ) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO projects(project_id,owner_actor) VALUES (?,?)")
            .bind(project.as_str())
            .bind(owner.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
    pub async fn save_draft(
        &self,
        plan: &ValidatedPlan<'_>,
        expected: Option<&Revision>,
        actor: &PrincipalId,
        authorization: &impl PlanAuthorization,
    ) -> Result<Revision, StoreError> {
        let revision = Revision::from_validated(plan);
        let key = revision.key();
        let number = version(key)?;
        if plan.canonical_bytes().len() > MAX_CANONICAL_BYTES {
            return Err(StoreError::Integrity);
        }
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        authorize(
            &mut tx,
            actor,
            &revision,
            PlanOperation::SaveDraft,
            authorization,
        )
        .await?;
        let head = head(&mut tx, &key.project_id).await?;
        let existing=sqlx::query("SELECT fingerprint,canonical FROM plan_versions WHERE project_id=? AND plan_id=? AND version=?")
            .bind(key.project_id.as_str()).bind(key.plan_id.as_str()).bind(number).fetch_optional(&mut *tx).await?;
        if let Some(row) = existing {
            if row.try_get::<String, _>("fingerprint")? != revision.fingerprint
                || row.try_get::<Vec<u8>, _>("canonical")? != plan.canonical_bytes()
            {
                return Err(StoreError::Conflict);
            }
            if head.as_ref() != Some(&revision) {
                return Err(StoreError::StaleRevision);
            }
            tx.commit().await?;
            return Ok(revision);
        }
        if head.as_ref() != expected {
            return Err(StoreError::StaleRevision);
        }
        if active_exists(&mut tx, &key.project_id).await? {
            return Err(StoreError::AlreadyActive);
        }
        if head
            .as_ref()
            .is_some_and(|h| h.key.plan_id == key.plan_id && h.key.version >= key.version)
        {
            return Err(StoreError::StaleRevision);
        }
        sqlx::query("INSERT INTO plan_versions(project_id,plan_id,version,fingerprint,canonical,state) VALUES (?,?,?,?,?,'draft')")
            .bind(key.project_id.as_str()).bind(key.plan_id.as_str()).bind(number).bind(revision.fingerprint()).bind(plan.canonical_bytes()).execute(&mut *tx).await?;
        let decoded: Envelope =
            serde_json::from_slice(plan.canonical_bytes()).map_err(|_| StoreError::Integrity)?;
        for task in &decoded.plan.tasks {
            sqlx::query("INSERT INTO tasks(project_id,plan_id,version,task_id,parent_task_id,kind,definition) VALUES (?,?,?,?,?,?,?)")
                .bind(key.project_id.as_str()).bind(key.plan_id.as_str()).bind(number).bind(task.id.as_str())
                .bind(task.parent.as_ref().map(|p|p.task_id.as_str()))
                .bind(if task.kind==TaskKind::Group {"group"}else{"executable_leaf"})
                .bind(serde_json::to_string(task).map_err(|_|StoreError::Integrity)?).execute(&mut *tx).await?;
        }
        for task in &decoded.plan.tasks {
            for dependency in &task.dependencies {
                sqlx::query("INSERT INTO task_dependencies(project_id,plan_id,version,predecessor_id,successor_id) VALUES (?,?,?,?,?)")
                .bind(key.project_id.as_str()).bind(key.plan_id.as_str()).bind(number).bind(dependency.task_id.as_str()).bind(task.id.as_str()).execute(&mut *tx).await?;
            }
        }
        sqlx::query("UPDATE projects SET draft_plan_id=?,draft_version=? WHERE project_id=?")
            .bind(key.plan_id.as_str())
            .bind(number)
            .bind(key.project_id.as_str())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(revision)
    }
    pub async fn load_active(
        &self,
        project: &ProjectId,
        policy: &impl AssignmentPolicy,
        limits: Limits,
    ) -> Result<StoredPlan, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT plan_id,version FROM plan_versions WHERE project_id=? AND state='active'",
        )
        .bind(project.as_str())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        let key = PlanKey {
            project_id: project.clone(),
            plan_id: rolvya_domain::ids::PlanId::new(row.try_get::<String, _>("plan_id")?)
                .map_err(|_| StoreError::Integrity)?,
            version: rolvya_domain::ids::PlanVersion::new(
                u64::try_from(row.try_get::<i64, _>("version")?)
                    .map_err(|_| StoreError::Integrity)?,
            )
            .map_err(|_| StoreError::Integrity)?,
        };
        let result = read_plan(&mut tx, &key, policy, limits).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn load(
        &self,
        key: &PlanKey,
        policy: &impl AssignmentPolicy,
        limits: Limits,
    ) -> Result<StoredPlan, StoreError> {
        let mut tx = self.pool.begin().await?;
        let result = read_plan(&mut tx, key, policy, limits).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn activate(
        &self,
        revision: &Revision,
        request: &RequestId,
        actor: &PrincipalId,
        authorization: &impl PlanAuthorization,
        policy: &impl AssignmentPolicy,
        limits: Limits,
    ) -> Result<ActivationReceipt, StoreError> {
        let key = revision.key();
        let number = version(key)?;
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        authorize(
            &mut tx,
            actor,
            revision,
            PlanOperation::Activate,
            authorization,
        )
        .await?;
        if let Some(row)=sqlx::query("SELECT sequence,plan_id,version,fingerprint,actor FROM activation_events WHERE project_id=? AND request_id=?")
            .bind(key.project_id.as_str()).bind(request.as_str()).fetch_optional(&mut *tx).await? {
            if row.try_get::<String,_>("plan_id")?!=key.plan_id.as_str()||row.try_get::<i64,_>("version")?!=number||row.try_get::<String,_>("fingerprint")?!=revision.fingerprint()||row.try_get::<String,_>("actor")?!=actor.as_str() {return Err(StoreError::IdempotencyConflict);}
            let complete:i64=sqlx::query_scalar("SELECT count(*) FROM plan_versions v WHERE v.project_id=? AND v.plan_id=? AND v.version=? AND v.state='active' AND v.activated_event=? AND v.fingerprint=? AND (SELECT count(*) FROM task_runtime r WHERE r.project_id=v.project_id AND r.plan_id=v.plan_id AND r.version=v.version)=(SELECT count(*) FROM tasks t WHERE t.project_id=v.project_id AND t.plan_id=v.plan_id AND t.version=v.version)")
                .bind(key.project_id.as_str()).bind(key.plan_id.as_str()).bind(number).bind(row.try_get::<i64,_>("sequence")?).bind(revision.fingerprint()).fetch_one(&mut *tx).await?;
            if complete!=1 {return Err(StoreError::Integrity);}
            let receipt=ActivationReceipt{revision:revision.clone(),actor:actor.clone(),request_id:request.clone(),sequence:row.try_get("sequence")?};
            tx.commit().await?;return Ok(receipt);
        }
        if head(&mut tx, &key.project_id).await?.as_ref() != Some(revision) {
            return Err(StoreError::StaleRevision);
        }
        if active_exists(&mut tx, &key.project_id).await? {
            return Err(StoreError::AlreadyActive);
        }
        let loaded = read_plan(&mut tx, key, policy, limits).await?;
        if loaded.active || loaded.revision != *revision {
            return Err(StoreError::StaleRevision);
        }
        sqlx::query("INSERT INTO task_runtime(project_id,plan_id,version,task_id,state) SELECT project_id,plan_id,version,task_id,'planned' FROM tasks WHERE project_id=? AND plan_id=? AND version=?")
            .bind(key.project_id.as_str()).bind(key.plan_id.as_str()).bind(number).execute(&mut *tx).await?;
        let result=sqlx::query("INSERT INTO activation_events(project_id,plan_id,version,fingerprint,actor,request_id) VALUES (?,?,?,?,?,?)")
            .bind(key.project_id.as_str()).bind(key.plan_id.as_str()).bind(number).bind(revision.fingerprint()).bind(actor.as_str()).bind(request.as_str()).execute(&mut *tx).await?;
        let sequence = result.last_insert_rowid();
        let updated=sqlx::query("UPDATE plan_versions SET state='active',activated_event=? WHERE project_id=? AND plan_id=? AND version=? AND state='draft'")
            .bind(sequence).bind(key.project_id.as_str()).bind(key.plan_id.as_str()).bind(number).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            return Err(StoreError::Integrity);
        }
        tx.commit().await?;
        Ok(ActivationReceipt {
            revision: revision.clone(),
            actor: actor.clone(),
            request_id: request.clone(),
            sequence,
        })
    }
    /// SQLite-supported consistent local snapshot, not an encrypted off-host backup.
    /// Destination must not exist. Failure leaves the file for operator diagnosis.
    pub async fn snapshot(&self, destination: impl AsRef<Path>) -> Result<(), StoreError> {
        let path = destination.as_ref();
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                StoreError::SnapshotExists
            } else {
                StoreError::InvalidPath
            }
        })?;
        sqlx::query("VACUUM INTO ?")
            .bind(path.to_str().ok_or(StoreError::InvalidPath)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
async fn authorize(
    tx: &mut Transaction<'_, Sqlite>,
    actor: &PrincipalId,
    revision: &Revision,
    operation: PlanOperation,
    policy: &impl PlanAuthorization,
) -> Result<(), StoreError> {
    let owner: String = sqlx::query_scalar("SELECT owner_actor FROM projects WHERE project_id=?")
        .bind(revision.key.project_id.as_str())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)?;
    let owner = PrincipalId::new(owner).map_err(|_| StoreError::Integrity)?;
    if actor != &owner || !policy.permits(&owner, actor, revision, operation) {
        return Err(StoreError::Forbidden);
    }
    Ok(())
}
async fn head(
    tx: &mut Transaction<'_, Sqlite>,
    project: &ProjectId,
) -> Result<Option<Revision>, StoreError> {
    let row=sqlx::query("SELECT p.draft_plan_id,p.draft_version,v.fingerprint FROM projects p LEFT JOIN plan_versions v ON v.project_id=p.project_id AND v.plan_id=p.draft_plan_id AND v.version=p.draft_version WHERE p.project_id=?")
        .bind(project.as_str()).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
    let id: Option<String> = row.try_get("draft_plan_id")?;
    match id {
        None => Ok(None),
        Some(id) => {
            let v: i64 = row.try_get("draft_version")?;
            Ok(Some(Revision {
                key: PlanKey {
                    project_id: project.clone(),
                    plan_id: rolvya_domain::ids::PlanId::new(id)
                        .map_err(|_| StoreError::Integrity)?,
                    version: rolvya_domain::ids::PlanVersion::new(
                        u64::try_from(v).map_err(|_| StoreError::Integrity)?,
                    )
                    .map_err(|_| StoreError::Integrity)?,
                },
                fingerprint: row.try_get("fingerprint")?,
            }))
        }
    }
}
async fn active_exists(
    tx: &mut Transaction<'_, Sqlite>,
    project: &ProjectId,
) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM plan_versions WHERE project_id=? AND state='active'",
    )
    .bind(project.as_str())
    .fetch_one(&mut **tx)
    .await?
        != 0)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    plan: PlanContent,
}
async fn read_plan(
    connection: &mut SqliteConnection,
    key: &PlanKey,
    policy: &impl AssignmentPolicy,
    limits: Limits,
) -> Result<StoredPlan, StoreError> {
    let number = version(key)?;
    let row=sqlx::query("SELECT fingerprint,canonical,state FROM plan_versions WHERE project_id=? AND plan_id=? AND version=?")
        .bind(key.project_id.as_str()).bind(key.plan_id.as_str()).bind(number).fetch_optional(&mut *connection).await?.ok_or(StoreError::NotFound)?;
    let canonical: Vec<u8> = row.try_get("canonical")?;
    if canonical.len() > MAX_CANONICAL_BYTES {
        return Err(StoreError::Integrity);
    }
    let envelope: Envelope =
        serde_json::from_slice(&canonical).map_err(|_| StoreError::Integrity)?;
    if envelope.format != CANONICAL_FORMAT || envelope.plan.key != *key {
        return Err(StoreError::Integrity);
    }
    let validated = validate(&envelope.plan, policy, limits).map_err(StoreError::InvalidPlan)?;
    let revision = Revision::from_validated(&validated);
    if revision.fingerprint() != row.try_get::<String, _>("fingerprint")?
        || canonical != validated.canonical_bytes()
    {
        return Err(StoreError::Integrity);
    }
    let rows=sqlx::query("SELECT task_id,parent_task_id,kind,definition FROM tasks WHERE project_id=? AND plan_id=? AND version=? ORDER BY task_id")
        .bind(key.project_id.as_str()).bind(key.plan_id.as_str()).bind(number).fetch_all(&mut *connection).await?;
    if rows.len() != envelope.plan.tasks.len() {
        return Err(StoreError::Integrity);
    }
    let mut expected_edges = BTreeSet::new();
    for (row, task) in rows.iter().zip(&envelope.plan.tasks) {
        let kind = if task.kind == TaskKind::Group {
            "group"
        } else {
            "executable_leaf"
        };
        if row.try_get::<String, _>("task_id")? != task.id.as_str()
            || row
                .try_get::<Option<String>, _>("parent_task_id")?
                .as_deref()
                != task.parent.as_ref().map(|p| p.task_id.as_str())
            || row.try_get::<String, _>("kind")? != kind
            || row.try_get::<String, _>("definition")?
                != serde_json::to_string(task).map_err(|_| StoreError::Integrity)?
        {
            return Err(StoreError::Integrity);
        }
        for dep in &task.dependencies {
            expected_edges.insert((dep.task_id.as_str().to_owned(), task.id.as_str().to_owned()));
        }
    }
    let edges=sqlx::query("SELECT predecessor_id,successor_id FROM task_dependencies WHERE project_id=? AND plan_id=? AND version=?")
        .bind(key.project_id.as_str()).bind(key.plan_id.as_str()).bind(number).fetch_all(connection).await?;
    let actual: BTreeSet<(String, String)> = edges
        .iter()
        .map(|r| Ok((r.try_get("predecessor_id")?, r.try_get("successor_id")?)))
        .collect::<Result<_, sqlx::Error>>()?;
    if actual != expected_edges {
        return Err(StoreError::Integrity);
    }
    Ok(StoredPlan {
        revision,
        content: envelope.plan,
        active: row.try_get::<String, _>("state")? == "active",
    })
}

#[cfg(test)]
mod tests;
