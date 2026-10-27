//! Guarded Linux controller-host bootstrap. No network, credential onboarding or recovery API.
//! Only an explicit trusted local setup policy may initialize; loading never generates keys.
use crate::{PlanStore, RequestId, StoreError};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::Row;
use std::{
    fmt,
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;
#[cfg(target_os = "linux")]
mod custody;
#[cfg(test)]
mod tests;

pub mod secret_broker;

const FORMAT: u32 = 1;
const KEY_VERSION: u64 = 1;
const HEADER: usize = 52;
const FILE_BYTES: usize = HEADER + 32;
const MAGIC: &[u8; 8] = b"ROLVYAK1";
const DOMAIN: &[u8] = b"rolvya/controller-key/installation-verifier/v1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapError {
    NotAuthorized,
    NotInitialized,
    AlreadyInitialized,
    InitializationInProgress,
    InterruptedInstallation,
    MissingKey,
    InvalidKeyMaterial,
    InstallationMismatch,
    UnsafePath,
    InsecurePermissions,
    OwnershipMismatch,
    UnsupportedVersion,
    UnsupportedPlatform,
    UnsupportedFilesystem,
    RandomnessFailure,
    StorageFailure,
    DatabaseStateMismatch,
    RecoveryRequired,
}
impl fmt::Display for BootstrapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Static categories only: no OS error, paths, SQL values, or secret-bearing buffers.
        write!(f, "controller bootstrap: {self:?}")
    }
}
impl std::error::Error for BootstrapError {}
impl From<sqlx::Error> for BootstrapError {
    fn from(_: sqlx::Error) -> Self {
        Self::DatabaseStateMismatch
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupState {
    Fresh,
    Initializing,
    Initialized,
    Interrupted,
    RecoveryRequired,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationIdentity([u8; 16]);
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyIdentifier([u8; 16]);
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyMetadata {
    pub installation: InstallationIdentity,
    pub key: KeyIdentifier,
    pub format_version: u32,
    pub key_version: u64,
}
/// Opaque, non-cloneable and non-serializable. S2-02 must add narrowly scoped crypto
/// operations inside this boundary, rather than exporting plaintext key bytes.
/// ```compile_fail
/// use rolvya_persistence_sqlite::controller_keys::ControllerKey;
/// fn export(key: &ControllerKey) { let _ = serde_json::to_string(key); }
/// ```
/// ```compile_fail
/// use rolvya_persistence_sqlite::controller_keys::ControllerKey;
/// fn export(key: &ControllerKey) { let _ = &key.material; }
/// ```
pub struct ControllerKey {
    metadata: KeyMetadata,
    material: Zeroizing<[u8; 32]>,
}
impl ControllerKey {
    pub fn metadata(&self) -> &KeyMetadata {
        &self.metadata
    }
}
impl fmt::Debug for ControllerKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ControllerKey")
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}
#[derive(Debug, Clone)]
pub struct InitializationRequest {
    request_id: RequestId,
}
impl InitializationRequest {
    pub fn new(request_id: RequestId) -> Self {
        Self { request_id }
    }
    pub fn request_id(&self) -> &RequestId {
        &self.request_id
    }
}
/// Implemented only by a trusted controller-host/offline setup command. A caller's
/// worker ID, HTTP request, configuration change or directory access is not Owner auth.
/// There is deliberately no permissive default implementation or synthetic grant here.
pub trait LocalSetupAuthorization {
    fn permits(&self, request: &InitializationRequest) -> bool;
}
#[derive(Debug, Clone, Copy)]
pub struct ServiceIdentity(u32);
impl ServiceIdentity {
    #[cfg(target_os = "linux")]
    pub fn current() -> Self {
        Self(rustix::process::geteuid().as_raw())
    }
    pub fn from_uid(uid: u32) -> Self {
        Self(uid)
    }
}
/// Deployment must supply all worker, web, image/export and backup roots as exclusions.
/// The actual database directory and repository ancestors are independently excluded.
/// The provisioned key directory must already exist with mode 0700 and expected owner.
pub struct ControllerBootstrap {
    store: PlanStore,
    database_path: PathBuf,
    #[cfg(target_os = "linux")]
    directory: custody::Directory,
    #[cfg(test)]
    hook: Box<dyn Fn(Boundary) -> Result<(), BootstrapError> + Send + Sync>,
    #[cfg(test)]
    entropy: fn(&mut [u8]) -> Result<(), BootstrapError>,
}
impl ControllerBootstrap {
    pub async fn open(
        store: PlanStore,
        directory: &Path,
        owner: ServiceIdentity,
        excluded_roots: &[PathBuf],
    ) -> Result<Self, BootstrapError> {
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (store, directory, owner, excluded_roots);
            Err(BootstrapError::UnsupportedPlatform)
        }
        #[cfg(target_os = "linux")]
        {
            let rows = sqlx::query("PRAGMA database_list")
                .fetch_all(&store.pool)
                .await?;
            let database: String = rows
                .iter()
                .find(|r| r.get::<String, _>("name") == "main")
                .ok_or(BootstrapError::DatabaseStateMismatch)?
                .try_get("file")?;
            let mut excluded = excluded_roots.to_vec();
            excluded.push(
                Path::new(&database)
                    .parent()
                    .ok_or(BootstrapError::UnsafePath)?
                    .to_path_buf(),
            );
            let directory = custody::Directory::open(directory, owner, &excluded)?;
            Ok(Self {
                store,
                database_path: database.into(),
                directory,
                #[cfg(test)]
                hook: Box::new(|_| Ok(())),
                #[cfg(test)]
                entropy: |bytes| {
                    getrandom::fill(bytes).map_err(|_| BootstrapError::RandomnessFailure)
                },
            })
        }
    }
    pub async fn state(&self) -> Result<SetupState, BootstrapError> {
        #[cfg(not(target_os = "linux"))]
        {
            Err(BootstrapError::UnsupportedPlatform)
        }
        #[cfg(target_os = "linux")]
        {
            self.directory.validate()?;
            let directory = self.directory.reopen()?;
            let _lock = match directory.lock() {
                Ok(lock) => lock,
                Err(BootstrapError::InitializationInProgress) => {
                    return Ok(SetupState::Initializing);
                }
                Err(error) => return Err(error),
            };
            let state: String =
                sqlx::query_scalar("SELECT state FROM controller_installation WHERE singleton=1")
                    .fetch_one(&self.store.pool)
                    .await?;
            Ok(match state.as_str() {
                "fresh"
                    if !self.directory.has_material()?
                        && self.empty().await?
                        && self.fresh_identity_matches().await? =>
                {
                    SetupState::Fresh
                }
                "fresh" if !self.empty().await? || !self.fresh_identity_matches().await? => {
                    SetupState::RecoveryRequired
                }
                "fresh" | "pending" => SetupState::Interrupted,
                "ready" => {
                    self.load_existing().await?;
                    SetupState::Initialized
                }
                "recovery" => SetupState::RecoveryRequired,
                _ => return Err(BootstrapError::DatabaseStateMismatch),
            })
        }
    }
    async fn empty(&self) -> Result<bool, BootstrapError> {
        Ok(
            sqlx::query_scalar::<_, i64>("SELECT NOT EXISTS(SELECT 1 FROM projects)")
                .fetch_one(&self.store.pool)
                .await?
                == 1,
        )
    }
    #[cfg(target_os = "linux")]
    fn database_identity(&self) -> Result<[u8; 16], BootstrapError> {
        let directory = custody::Directory::open(
            self.database_path
                .parent()
                .ok_or(BootstrapError::UnsafePath)?,
            ServiceIdentity::current(),
            &[],
        )?;
        directory.file_identity(
            self.database_path
                .file_name()
                .ok_or(BootstrapError::UnsafePath)?,
        )
    }
    #[cfg(target_os = "linux")]
    async fn fresh_identity_matches(&self) -> Result<bool, BootstrapError> {
        let recorded: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT birth_identity FROM controller_installation WHERE singleton=1",
        )
        .fetch_one(&self.store.pool)
        .await?;
        Ok(recorded.as_deref() == Some(self.database_identity()?.as_slice()))
    }
    pub async fn load_existing(&self) -> Result<ControllerKey, BootstrapError> {
        #[cfg(not(target_os = "linux"))]
        {
            Err(BootstrapError::UnsupportedPlatform)
        }
        #[cfg(target_os = "linux")]
        {
            let row = sqlx::query("SELECT * FROM controller_installation WHERE singleton=1")
                .fetch_one(&self.store.pool)
                .await?;
            match row.try_get::<&str, _>("state")? {
                "ready" => {}
                "pending" => return Err(BootstrapError::InterruptedInstallation),
                "fresh"
                    if !self.directory.has_material()?
                        && self.empty().await?
                        && self.fresh_identity_matches().await? =>
                {
                    return Err(BootstrapError::NotInitialized);
                }
                _ => return Err(BootstrapError::RecoveryRequired),
            }
            let key = decode(&*self.directory.read(|b| self.boundary(b))?)?;
            if key.metadata.installation.0.as_slice()
                != row.try_get::<Vec<u8>, _>("installation_id")?
                || key.metadata.key.0.as_slice() != row.try_get::<Vec<u8>, _>("key_id")?
                || i64::from(key.metadata.format_version)
                    != row.try_get::<i64, _>("format_version")?
                || key.metadata.key_version
                    != u64::try_from(row.try_get::<i64, _>("key_version")?)
                        .map_err(|_| BootstrapError::UnsupportedVersion)?
            {
                return Err(BootstrapError::InstallationMismatch);
            }
            let mut verifier = mac(&key)?;
            verifier.update(&header(&key.metadata));
            verifier
                .verify_slice(&row.try_get::<Vec<u8>, _>("verifier")?)
                .map_err(|_| BootstrapError::InstallationMismatch)?;
            self.directory.validate()?;
            Ok(key)
        }
    }
    pub async fn initialize(
        &self,
        request: &InitializationRequest,
        authority: &impl LocalSetupAuthorization,
    ) -> Result<ControllerKey, BootstrapError> {
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (request, authority);
            Err(BootstrapError::UnsupportedPlatform)
        }
        #[cfg(target_os = "linux")]
        {
            if !authority.permits(request) {
                return Err(BootstrapError::NotAuthorized);
            }
            self.boundary(Boundary::BeforeSetup)?;
            // Separate file description per operation: flock must also exclude concurrent
            // setup calls using this same object, not just independently opened processes.
            let directory = self.directory.reopen()?;
            let _lock = directory.lock()?;
            self.boundary(Boundary::OwnershipAcquired)?;
            let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
            let state: String =
                sqlx::query_scalar("SELECT state FROM controller_installation WHERE singleton=1")
                    .fetch_one(&mut *tx)
                    .await?;
            match state.as_str() {
                "ready" => return Err(BootstrapError::AlreadyInitialized),
                "pending" => return Err(BootstrapError::InterruptedInstallation),
                "fresh" => {}
                _ => return Err(BootstrapError::RecoveryRequired),
            }
            let recorded: Vec<u8> = sqlx::query_scalar(
                "SELECT birth_identity FROM controller_installation WHERE singleton=1",
            )
            .fetch_one(&mut *tx)
            .await?;
            if recorded.as_slice() != self.database_identity()?.as_slice() {
                return Err(BootstrapError::RecoveryRequired);
            }
            if directory.has_material()? {
                return Err(BootstrapError::InterruptedInstallation);
            }
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM projects")
                .fetch_one(&mut *tx)
                .await?;
            if count != 0 {
                return Err(BootstrapError::RecoveryRequired);
            }
            sqlx::query("UPDATE controller_installation SET state='pending',request_id=? WHERE singleton=1 AND state='fresh'")
                .bind(request.request_id.as_str()).execute(&mut *tx).await?;
            tx.commit().await?;
            let mut material = Zeroizing::new([0u8; 32]);
            self.boundary(Boundary::BeforeRandomness)?;
            self.fill_entropy(material.as_mut())?;
            let mut installation = [0u8; 16];
            let mut id = [0u8; 16];
            self.fill_entropy(&mut installation)?;
            self.fill_entropy(&mut id)?;
            let key = ControllerKey {
                material,
                metadata: KeyMetadata {
                    installation: InstallationIdentity(installation),
                    key: KeyIdentifier(id),
                    format_version: FORMAT,
                    key_version: KEY_VERSION,
                },
            };
            self.boundary(Boundary::KeyGenerated)?;
            if !authority.permits(request) {
                return Err(BootstrapError::NotAuthorized);
            }
            directory.publish(&encode(&key), |b| self.boundary(b))?;
            self.boundary(Boundary::Published)?;
            directory.validate()?;
            if !authority.permits(request) {
                return Err(BootstrapError::NotAuthorized);
            }
            let mut verifier = mac(&key)?;
            verifier.update(&header(&key.metadata));
            let verifier = verifier.finalize().into_bytes();
            let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
            let changed = sqlx::query("UPDATE controller_installation SET state='ready',installation_id=?,key_id=?,format_version=?,key_version=?,verifier=? WHERE singleton=1 AND state='pending' AND request_id=?")
                .bind(key.metadata.installation.0.as_slice()).bind(key.metadata.key.0.as_slice())
                .bind(i64::from(FORMAT)).bind(1i64).bind(verifier.as_slice())
                .bind(request.request_id.as_str()).execute(&mut *tx).await?.rows_affected();
            if changed != 1 {
                return Err(BootstrapError::DatabaseStateMismatch);
            }
            self.boundary(Boundary::BeforeStateCommit)?;
            tx.commit().await?;
            self.boundary(Boundary::StateCommitted)?;
            directory.validate()?;
            // Read the published descriptor again; verify persistence instead of returning
            // a generated key while the configured pathname refers to something else.
            let loaded = self.load_existing().await?;
            self.boundary(Boundary::BeforeAcknowledgement)?;
            Ok(loaded)
        }
    }
    fn fill_entropy(&self, bytes: &mut [u8]) -> Result<(), BootstrapError> {
        #[cfg(test)]
        {
            (self.entropy)(bytes)
        }
        #[cfg(not(test))]
        {
            getrandom::fill(bytes).map_err(|_| BootstrapError::RandomnessFailure)
        }
    }
    fn boundary(&self, boundary: Boundary) -> Result<(), BootstrapError> {
        #[cfg(test)]
        {
            (self.hook)(boundary)
        }
        #[cfg(not(test))]
        {
            let _ = boundary;
            Ok(())
        }
    }
}
// Internal checkpoints have no caller-selectable fault behavior in production.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Boundary {
    ReadOpened,
    ReadComplete,
    BeforeSetup,
    OwnershipAcquired,
    BeforeRandomness,
    KeyGenerated,
    TemporaryCreated,
    ContentsWritten,
    FileSynced,
    BeforePublication,
    Published,
    BeforeStateCommit,
    StateCommitted,
    BeforeAcknowledgement,
}
// Retained exclusively-created file and protected parent, verified before SQLx
// opens the pathname and again before eligibility is recorded.
pub(super) struct DatabaseBirth {
    #[cfg(target_os = "linux")]
    directory: custody::Directory,
    file: std::fs::File,
    path: PathBuf,
}
pub(super) fn new_database_guard(path: &Path, file: std::fs::File) -> Option<DatabaseBirth> {
    #[cfg(target_os = "linux")]
    {
        let directory =
            custody::Directory::open(path.parent()?, ServiceIdentity::current(), &[]).ok()?;
        if !directory.matches_file(path.file_name()?, &file).ok()? {
            return None;
        }
        file.sync_all().ok()?;
        directory.sync().ok()?;
        Some(DatabaseBirth {
            directory,
            file,
            path: path.to_path_buf(),
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (path, file);
        None
    }
}
pub(super) async fn mark_new_database(
    store: &PlanStore,
    birth: &DatabaseBirth,
) -> Result<(), StoreError> {
    #[cfg(target_os = "linux")]
    {
        if !birth
            .directory
            .matches_file(
                birth.path.file_name().ok_or(StoreError::InvalidPath)?,
                &birth.file,
            )
            .map_err(|_| StoreError::InvalidPath)?
        {
            return Err(StoreError::InvalidPath);
        }
        sqlx::query("UPDATE controller_installation SET state='fresh',birth_identity=? WHERE singleton=1 AND state='recovery' AND NOT EXISTS(SELECT 1 FROM projects)")
            .bind(birth.directory.file_identity(birth.path.file_name().ok_or(StoreError::InvalidPath)?).map_err(|_|StoreError::InvalidPath)?.as_slice())
            .execute(&store.pool).await?;
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (store, birth);
        Ok(())
    }
}
fn header(metadata: &KeyMetadata) -> [u8; HEADER] {
    let mut b = [0u8; HEADER];
    b[..8].copy_from_slice(MAGIC);
    b[8..12].copy_from_slice(&metadata.format_version.to_be_bytes());
    b[12..28].copy_from_slice(&metadata.installation.0);
    b[28..44].copy_from_slice(&metadata.key.0);
    b[44..52].copy_from_slice(&metadata.key_version.to_be_bytes());
    b
}
fn encode(key: &ControllerKey) -> Zeroizing<[u8; FILE_BYTES]> {
    let mut b = Zeroizing::new([0u8; FILE_BYTES]);
    b[..HEADER].copy_from_slice(&header(&key.metadata));
    b[HEADER..].copy_from_slice(key.material.as_ref());
    b
}
fn decode(bytes: &[u8; FILE_BYTES]) -> Result<ControllerKey, BootstrapError> {
    if &bytes[..8] != MAGIC {
        return Err(BootstrapError::InvalidKeyMaterial);
    }
    let format = u32::from_be_bytes(
        bytes[8..12]
            .try_into()
            .map_err(|_| BootstrapError::InvalidKeyMaterial)?,
    );
    let version = u64::from_be_bytes(
        bytes[44..52]
            .try_into()
            .map_err(|_| BootstrapError::InvalidKeyMaterial)?,
    );
    if format != FORMAT || version != KEY_VERSION {
        return Err(BootstrapError::UnsupportedVersion);
    }
    let mut material = Zeroizing::new([0u8; 32]);
    material.copy_from_slice(&bytes[HEADER..]);
    Ok(ControllerKey {
        material,
        metadata: KeyMetadata {
            installation: InstallationIdentity(
                bytes[12..28]
                    .try_into()
                    .map_err(|_| BootstrapError::InvalidKeyMaterial)?,
            ),
            key: KeyIdentifier(
                bytes[28..44]
                    .try_into()
                    .map_err(|_| BootstrapError::InvalidKeyMaterial)?,
            ),
            format_version: format,
            key_version: version,
        },
    })
}
fn mac(key: &ControllerKey) -> Result<Hmac<Sha256>, BootstrapError> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key.material.as_ref())
        .map_err(|_| BootstrapError::InvalidKeyMaterial)?;
    mac.update(DOMAIN);
    Ok(mac)
}
