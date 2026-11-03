use super::files::{MAX_BACKUP_BYTES, PrivateDirectory};
use super::*;
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use sqlx::{Column, ConnectOptions, Connection, TypeInfo, ValueRef};
use std::{future::Future, pin::Pin};

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackupClass {
    Frequent,
    Daily,
}
impl BackupClass {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Frequent => "frequent",
            Self::Daily => "daily",
        }
    }
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TableInventory {
    pub name: String,
    pub rows: u64,
    pub sha256: String,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BackupManifest {
    pub version: u32,
    pub id: String,
    pub installation: String,
    pub key_id: String,
    pub key_version: u64,
    pub schema: i64,
    pub created_at: i64,
    pub class: BackupClass,
    pub database_bytes: u64,
    pub database_sha256: String,
    pub tables: Vec<TableInventory>,
    pub external_artifact_bytes_included: bool,
}
#[derive(Serialize, Deserialize)]
pub struct BackupReceipt {
    pub id: String,
    pub envelope_sha256: String,
    pub destination: String,
    pub local_filesystem_only: bool,
}
type DestinationFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, OperationError>> + Send + 'a>>;
/// Trusted storage adapter, not a claim that a local destination is off-host.
/// Success is independently read back and authenticated before SQLite records verification.
pub trait BackupDestination: Send + Sync {
    fn identity(&self) -> String;
    fn put<'a>(&'a self, id: &'a str, bytes: &'a [u8]) -> DestinationFuture<'a, ()>;
    fn get<'a>(&'a self, id: &'a str) -> DestinationFuture<'a, Zeroizing<Vec<u8>>>;
    fn delete<'a>(&'a self, id: &'a str) -> DestinationFuture<'a, ()>;
    fn local_filesystem_only(&self) -> bool;
}
pub struct FilesystemDestination {
    directory: PrivateDirectory,
}
impl FilesystemDestination {
    pub fn open(path: &Path) -> Result<Self, OperationError> {
        Ok(Self {
            directory: PrivateDirectory::open(path)?,
        })
    }
}
fn filename(id: &str) -> Result<String, OperationError> {
    if !config::digest(id) {
        return Err(OperationError::Integrity);
    }
    Ok(format!("{id}.rvbackup"))
}
impl BackupDestination for FilesystemDestination {
    fn identity(&self) -> String {
        hex(&Sha256::digest(
            self.directory.path.as_os_str().as_encoded_bytes(),
        ))
    }
    fn put<'a>(&'a self, id: &'a str, bytes: &'a [u8]) -> DestinationFuture<'a, ()> {
        Box::pin(async move { self.directory.publish(&filename(id)?, bytes) })
    }
    fn get<'a>(&'a self, id: &'a str) -> DestinationFuture<'a, Zeroizing<Vec<u8>>> {
        Box::pin(async move {
            self.directory
                .read(&filename(id)?, MAX_BACKUP_BYTES + 131072)
        })
    }
    fn delete<'a>(&'a self, id: &'a str) -> DestinationFuture<'a, ()> {
        Box::pin(async move { self.directory.remove(&filename(id)?) })
    }
    fn local_filesystem_only(&self) -> bool {
        true
    }
}
pub(super) struct BackupKey(Zeroizing<[u8; 32]>);
impl BackupKey {
    pub(super) fn load(path: &Path) -> Result<Self, OperationError> {
        Self::from_bytes(&files::private_file(path, 32)?)
    }
    pub(super) fn from_bytes(bytes: &[u8]) -> Result<Self, OperationError> {
        if bytes.len() != 32 || bytes.iter().all(|b| *b == 0) {
            return Err(OperationError::Unauthorized);
        }
        let mut key = Zeroizing::new([0; 32]);
        key.copy_from_slice(bytes);
        Ok(Self(key))
    }
}
// Fixed domain, random nonce, authenticated lengths/version. No configurable KDF/cipher.
pub(super) fn seal(
    key: &BackupKey,
    domain: &[u8],
    plain: &[u8],
) -> Result<Zeroizing<Vec<u8>>, OperationError> {
    if plain.len() > MAX_BACKUP_BYTES + 131072 {
        return Err(OperationError::ResourceLimit);
    }
    let mut nonce = [0; 24];
    getrandom::fill(&mut nonce).map_err(|_| OperationError::Unavailable)?;
    let cipher =
        XChaCha20Poly1305::new_from_slice(&key.0[..]).map_err(|_| OperationError::Integrity)?;
    let encrypted = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plain,
                aad: domain,
            },
        )
        .map_err(|_| OperationError::Integrity)?;
    let mut bytes = Zeroizing::new(b"ROLVYAOP1".to_vec());
    bytes.extend(nonce);
    bytes.extend(encrypted);
    Ok(bytes)
}
pub(super) fn unseal(
    key: &BackupKey,
    domain: &[u8],
    bytes: &[u8],
) -> Result<Zeroizing<Vec<u8>>, OperationError> {
    if bytes.len() < 49 || bytes.len() > MAX_BACKUP_BYTES + 131072 || &bytes[..9] != b"ROLVYAOP1" {
        return Err(OperationError::Integrity);
    }
    let cipher =
        XChaCha20Poly1305::new_from_slice(&key.0[..]).map_err(|_| OperationError::Integrity)?;
    cipher
        .decrypt(
            XNonce::from_slice(&bytes[9..33]),
            Payload {
                msg: &bytes[33..],
                aad: domain,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| OperationError::Integrity)
}
pub(super) const BACKUP_DOMAIN: &[u8] = b"rolvya/sqlite-backup/xchacha20poly1305/v1\0";
pub(super) fn decode(
    key: &BackupKey,
    bytes: &[u8],
) -> Result<(BackupManifest, Zeroizing<Vec<u8>>), OperationError> {
    let plain = unseal(key, BACKUP_DOMAIN, bytes)?;
    let n = u32::from_be_bytes(
        plain
            .get(..4)
            .ok_or(OperationError::Integrity)?
            .try_into()
            .map_err(|_| OperationError::Integrity)?,
    ) as usize;
    if n > 131072 || n + 4 > plain.len() {
        return Err(OperationError::Integrity);
    }
    let m: BackupManifest =
        serde_json::from_slice(&plain[4..4 + n]).map_err(|_| OperationError::Integrity)?;
    let db = Zeroizing::new(plain[4 + n..].to_vec());
    if m.version != 1
        || m.database_bytes != db.len() as u64
        || db.len() > MAX_BACKUP_BYTES
        || hex(&Sha256::digest(&db)) != m.database_sha256
        || !config::digest(&m.id)
        || m.external_artifact_bytes_included
    {
        return Err(OperationError::Integrity);
    }
    Ok((m, db))
}
impl HostOperator {
    pub async fn backup(&self, class: BackupClass) -> Result<BackupReceipt, OperationError> {
        let destination = FilesystemDestination::open(&self.config.backup_destination)?;
        self.backup_to(class, &destination).await
    }
    pub async fn backup_to(
        &self,
        class: BackupClass,
        destination: &impl BackupDestination,
    ) -> Result<BackupReceipt, OperationError> {
        self.config.validate_host()?;
        let scratch = PrivateDirectory::open(&self.config.scratch)?;
        let _lock = scratch.lock()?;
        let key = self
            .bootstrap
            .load_existing()
            .await
            .map_err(|_| OperationError::RecoveryRequired)?;
        let encryption = BackupKey::load(&self.config.backup_key)?;
        let id = random_id()?;
        let t = now()?;
        self.event(
            &id,
            "backup_prepared",
            &key.coding_record_seal(b"ops/backup-intent", id.as_bytes()),
            None,
        )
        .await?;
        checkpoint("backup-intent");
        let name = format!("{id}.sqlite");
        let path = self.config.scratch.join(&name);
        self.store.snapshot(&path).await?;
        let (schema, tables) = inventory(&path).await?;
        let db = scratch.read(&name, MAX_BACKUP_BYTES)?;
        let manifest = BackupManifest {
            version: 1,
            id: id.clone(),
            installation: key.coding_installation(),
            key_id: hex(&key.metadata.key.0),
            key_version: key.metadata.key_version,
            schema,
            created_at: t,
            class,
            database_bytes: db.len() as u64,
            database_sha256: hex(&Sha256::digest(&db)),
            tables,
            external_artifact_bytes_included: false,
        };
        let metadata = canonical(&manifest)?;
        if metadata.len() > 131072 {
            return Err(OperationError::ResourceLimit);
        }
        let mut plain = Zeroizing::new(Vec::with_capacity(4 + metadata.len() + db.len()));
        plain.extend((metadata.len() as u32).to_be_bytes());
        plain.extend(&metadata);
        plain.extend(&*db);
        let envelope = seal(&encryption, BACKUP_DOMAIN, &plain)?;
        drop(plain);
        drop(db);
        checkpoint("backup-encrypted");
        destination.put(&id, &envelope).await?;
        checkpoint("backup-published");
        let readback = destination.get(&id).await?;
        let (actual, _) = decode(&encryption, &readback)?;
        if actual != manifest || *readback != *envelope {
            return Err(OperationError::Integrity);
        }
        let digest = hex(&Sha256::digest(&readback));
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        one(
            sqlx::query("INSERT INTO controller_backup_points VALUES(?,?,?,?,?,?)")
                .bind(&id)
                .bind(class.name())
                .bind(std::str::from_utf8(&metadata).map_err(|_| OperationError::Integrity)?)
                .bind(&digest)
                .bind(destination.identity())
                .bind(t)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        operator::event(&mut tx, &id, "backup_verified", &Sha256::digest(&readback)).await?;
        checkpoint("backup-before-commit");
        tx.commit().await?;
        checkpoint("backup-after-commit");
        scratch.remove(&name)?;
        Ok(BackupReceipt {
            id,
            envelope_sha256: digest,
            destination: destination.identity(),
            local_filesystem_only: destination.local_filesystem_only(),
        })
    }
    /// Deletes only verified old points after verifying newer coverage of the same class.
    /// Metadata remains append-only. Unknown/partial objects are never selected for cleanup.
    pub async fn retain_backups(&self) -> Result<u64, OperationError> {
        self.config.validate_host()?;
        let d = FilesystemDestination::open(&self.config.backup_destination)?;
        let _lock = d.directory.lock()?;
        let key = BackupKey::load(&self.config.backup_key)?;
        let mut deleted = 0;
        for class in [BackupClass::Frequent, BackupClass::Daily] {
            let rows=sqlx::query("SELECT * FROM controller_backup_points b WHERE class=? AND destination=? AND NOT EXISTS(SELECT 1 FROM controller_operation_events e WHERE e.request_id=b.request_id AND e.kind='backup_retired') ORDER BY recorded_at DESC,request_id DESC LIMIT 4096").bind(class.name()).bind(d.identity()).fetch_all(&self.store.pool).await?;
            let Some(newest) = rows.first() else { continue };
            let current_id: String = newest.try_get("request_id")?;
            let current = d.get(&current_id).await?;
            let (m, _) = decode(&key, &current)?;
            if m.id != current_id
                || m.class != class
                || m.created_at != newest.try_get::<i64, _>("recorded_at")?
                || hex(&Sha256::digest(&current))
                    != newest.try_get::<String, _>("envelope_sha256")?
            {
                return Err(OperationError::Integrity);
            }
            let age = match class {
                BackupClass::Frequent => 7 * 86400,
                BackupClass::Daily => 30 * 86400,
            };
            let cutoff = now()?.saturating_sub(age);
            for row in rows
                .iter()
                .skip(1)
                .filter(|r| r.get::<i64, _>("recorded_at") < cutoff)
            {
                let id: String = row.try_get("request_id")?;
                let started:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM controller_operation_events WHERE request_id=? AND kind='backup_cleanup_started')").bind(&id).fetch_one(&self.store.pool).await?;
                if started && !d.directory.present(&filename(&id)?)? {
                    self.event(
                        &id,
                        "backup_retired",
                        &operator::hex_decode(row.try_get("envelope_sha256")?)?,
                        None,
                    )
                    .await?;
                    deleted += 1;
                    continue;
                }
                let bytes = d.get(&id).await?;
                let (old, _) = decode(&key, &bytes)?;
                if old.id != id
                    || old.class != class
                    || old.created_at != row.try_get::<i64, _>("recorded_at")?
                    || old.installation != m.installation
                    || hex(&Sha256::digest(&bytes))
                        != row.try_get::<String, _>("envelope_sha256")?
                {
                    return Err(OperationError::Integrity);
                }
                // Record cleanup intent first. A failed unlink retains the file and never removes the newer point.
                if !started {
                    self.event(&id, "backup_cleanup_started", &Sha256::digest(&bytes), None)
                        .await?;
                }
                checkpoint("retention-intent");
                d.delete(&id).await?;
                self.event(&id, "backup_retired", &Sha256::digest(&bytes), None)
                    .await?;
                deleted += 1;
            }
        }
        Ok(deleted)
    }
}
/// Read-only schema, FK and complete per-table inventory. Data stays in encrypted manifests.
pub(super) async fn inventory(path: &Path) -> Result<(i64, Vec<TableInventory>), OperationError> {
    super::files::bounded_file(path, MAX_BACKUP_BYTES)?;
    let mut c = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .disable_statement_logging()
        .connect()
        .await?;
    let rows: Vec<String> = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_all(&mut c)
        .await?;
    if rows != ["ok"]
        || !sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&mut c)
            .await?
            .is_empty()
    {
        return Err(OperationError::Integrity);
    }
    let migrations =
        sqlx::query("SELECT version,checksum,success FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&mut c)
            .await?;
    if migrations.len() != crate::MIGRATOR.iter().count()
        || migrations.iter().zip(crate::MIGRATOR.iter()).any(|(r, m)| {
            r.get::<i64, _>("version") != m.version
                || r.get::<Vec<u8>, _>("checksum") != m.checksum.as_ref()
                || !r.get::<bool, _>("success")
        })
    {
        return Err(OperationError::Integrity);
    }
    let schema = migrations
        .last()
        .ok_or(OperationError::Integrity)?
        .get("version");
    let names:Vec<String>=sqlx::query_scalar("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").fetch_all(&mut c).await?;
    if names.len() > 512 {
        return Err(OperationError::ResourceLimit);
    }
    let mut out = Vec::new();
    let mut total = 0usize;
    for name in names {
        let identifier = format!("\"{}\"", name.replace('"', "\"\""));
        let mut h = Sha256::new();
        let mut count = 0u64;
        let mut after = i64::MIN;
        loop {
            let rows=sqlx::query(&format!("SELECT rowid AS __inventory_rowid__,* FROM {identifier} WHERE rowid>? ORDER BY rowid LIMIT 1")).bind(after).fetch_all(&mut c).await?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                after = row.try_get(0)?;
                count = count.checked_add(1).ok_or(OperationError::ResourceLimit)?;
                h.update(b"row\0");
                for (i, col) in row.columns().iter().enumerate().skip(1) {
                    let v = row.try_get_raw(i)?;
                    let kind = v.type_info().name().to_owned();
                    h.update((col.name().len() as u64).to_be_bytes());
                    h.update(col.name().as_bytes());
                    h.update(kind.as_bytes());
                    let bytes = Zeroizing::new(if v.is_null() {
                        Vec::new()
                    } else {
                        match kind.as_str() {
                            "INTEGER" => row.try_get::<i64, _>(i)?.to_be_bytes().to_vec(),
                            "BLOB" => row.try_get::<Vec<u8>, _>(i)?,
                            "TEXT" => row.try_get::<String, _>(i)?.into_bytes(),
                            _ => return Err(OperationError::Integrity),
                        }
                    });
                    total = total
                        .checked_add(bytes.len())
                        .ok_or(OperationError::ResourceLimit)?;
                    if total > MAX_BACKUP_BYTES {
                        return Err(OperationError::ResourceLimit);
                    }
                    h.update((bytes.len() as u64).to_be_bytes());
                    h.update(&*bytes);
                }
            }
        }
        out.push(TableInventory {
            name,
            rows: count,
            sha256: hex(&h.finalize()),
        });
    }
    c.close().await?;
    Ok((schema, out))
}
#[cfg(test)]
fn checkpoint(stage: &str) {
    super::tests::checkpoint(stage)
}
#[cfg(not(test))]
fn checkpoint(_: &str) {}
