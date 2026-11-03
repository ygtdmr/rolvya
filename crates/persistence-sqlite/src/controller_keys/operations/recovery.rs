use super::super::human_auth::HumanAuth;
use super::backup::{BackupKey, inventory};
use super::files::PrivateDirectory;
use super::*;

const KIT_DOMAIN: &[u8] = b"rolvya/emergency-kek-recovery/xchacha20poly1305/v1\0";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KitMetadata {
    version: u32,
    installation: String,
    key_id: String,
    key_version: u64,
    created_at: i64,
}
#[derive(Serialize, Deserialize)]
pub struct RestoreReceipt {
    pub restore_id: String,
    pub backup_id: String,
    pub installation: String,
    pub schema: i64,
    pub external_reconciliation_required: bool,
    pub key_recovered: bool,
}

impl HostOperator {
    /// OS-random 256-bit secret published only to a new restricted file. No plaintext output.
    /// Recovery custodians must retain this separately and off-host before relying on the kit.
    pub fn generate_protection_secret(path: &Path) -> Result<(), OperationError> {
        let d = PrivateDirectory::open(path.parent().ok_or(OperationError::UnsafeFilesystem)?)?;
        let mut bytes = Zeroizing::new([0; 32]);
        getrandom::fill(&mut *bytes).map_err(|_| OperationError::Unavailable)?;
        d.publish(
            path.file_name()
                .and_then(|x| x.to_str())
                .ok_or(OperationError::UnsafeFilesystem)?,
            &*bytes,
        )
    }
    pub async fn export_recovery_kit(
        &self,
        destination: &Path,
        recovery_secret: &[u8],
    ) -> Result<(), OperationError> {
        self.config.validate_host()?;
        self.separate_kit_path(destination)?;
        let key = self
            .bootstrap
            .load_existing()
            .await
            .map_err(|_| OperationError::RecoveryRequired)?;
        let recovery = BackupKey::from_bytes(recovery_secret)?;
        if recovery_secret == key.material.as_slice() {
            return Err(OperationError::Unauthorized);
        }
        // Distinct independently held keys are mandatory; do not reuse the ordinary backup key.
        if files::private_file(&self.config.backup_key, 32)?.as_slice() == recovery_secret {
            return Err(OperationError::Unauthorized);
        }
        let metadata = KitMetadata {
            version: 1,
            installation: key.coding_installation(),
            key_id: hex(&key.metadata.key.0),
            key_version: key.metadata.key_version,
            created_at: now()?,
        };
        let bytes = canonical(&metadata)?;
        let mut plain = Zeroizing::new(Vec::new());
        plain.extend((bytes.len() as u32).to_be_bytes());
        plain.extend(&bytes);
        plain.extend(super::super::encode(&key).as_slice());
        let id = random_id()?;
        self.event(
            &id,
            "key_export_prepared",
            &key.coding_record_seal(b"ops/recovery-kit", &bytes),
            None,
        )
        .await?;
        checkpoint("kit-intent");
        let envelope = backup::seal(&recovery, KIT_DOMAIN, &plain)?;
        let directory = PrivateDirectory::open(
            destination
                .parent()
                .ok_or(OperationError::UnsafeFilesystem)?,
        )?;
        let name = destination
            .file_name()
            .and_then(|x| x.to_str())
            .ok_or(OperationError::UnsafeFilesystem)?;
        directory.publish(name, &envelope)?;
        checkpoint("kit-published");
        let actual = directory.read(name, 4096)?;
        let recovered = decode_kit(&recovery, &actual)?;
        if recovered.metadata != key.metadata {
            return Err(OperationError::Integrity);
        }
        self.event(&id, "key_export_verified", &Sha256::digest(&actual), None)
            .await?;
        Ok(())
    }
    fn separate_kit_path(&self, path: &Path) -> Result<(), OperationError> {
        let parent = path.parent().ok_or(OperationError::UnsafeFilesystem)?;
        PrivateDirectory::open(parent)?;
        for root in [
            self.config
                .database
                .parent()
                .ok_or(OperationError::UnsafeFilesystem)?,
            self.config.custody.as_path(),
            self.config.scratch.as_path(),
            self.config.assets.as_path(),
            self.config.backup_destination.as_path(),
            self.config
                .backup_key
                .parent()
                .ok_or(OperationError::UnsafeFilesystem)?,
        ] {
            if parent.starts_with(root) || root.starts_with(parent) {
                return Err(OperationError::UnsafeFilesystem);
            }
        }
        Ok(())
    }
    /// Restores only into an empty target layout, never an active installation.
    /// Publication is staged: the DB is paused before its final pathname exists.
    pub async fn restore_database(
        config: &DeploymentConfig,
        source: &impl BackupDestination,
        id: &str,
    ) -> Result<RestoreReceipt, OperationError> {
        config.validate_host()?;
        let data = PrivateDirectory::open(
            config
                .database
                .parent()
                .ok_or(OperationError::UnsafeFilesystem)?,
        )?;
        let _lock = data.lock()?;
        if std::fs::read_dir(&data.path)
            .map_err(|_| OperationError::UnsafeFilesystem)?
            .next()
            .is_some()
            || std::fs::read_dir(&config.custody)
                .map_err(|_| OperationError::UnsafeFilesystem)?
                .next()
                .is_some()
        {
            return Err(OperationError::Conflict);
        }
        let key = BackupKey::load(&config.backup_key)?;
        let envelope = source.get(id).await?;
        let (manifest, bytes) = backup::decode(&key, &envelope)?;
        if manifest.id != id {
            return Err(OperationError::Integrity);
        }
        let staging = "restore.pending.sqlite";
        data.publish(staging, &bytes)?;
        drop(bytes);
        checkpoint("restore-file");
        let staged = data.path.join(staging);
        let (schema, tables) = inventory(&staged).await?;
        if schema != manifest.schema || tables != manifest.tables {
            return Err(OperationError::Integrity);
        }
        let store = PlanStore::open_existing(&staged).await?;
        let mut tx = store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let identity: Vec<u8> = sqlx::query_scalar(
            "SELECT installation_id FROM controller_installation WHERE state='ready'",
        )
        .fetch_one(&mut *tx)
        .await?;
        if hex(&identity) != manifest.installation {
            return Err(OperationError::Integrity);
        }
        let restore_id = random_id()?;
        let manifest_digest = hex(&Sha256::digest(canonical(&manifest)?));
        one(
            sqlx::query("INSERT INTO controller_restore_holds VALUES(?,?,?,?)")
                .bind(&restore_id)
                .bind(id)
                .bind(&manifest_digest)
                .bind(now()?)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        operator::event(
            &mut tx,
            &restore_id,
            "restore_paused",
            &operator::hex_decode(&manifest_digest)?,
        )
        .await?;
        checkpoint("restore-before-commit");
        tx.commit().await?;
        checkpoint("restore-after-commit");
        store.check_integrity().await?;
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&store.pool)
            .await?;
        store.close().await;
        // Closing SQLite/checkpoint is not a filesystem transaction. Retain staging on interruption.
        let paused = data.read(staging, files::MAX_BACKUP_BYTES)?;
        data.publish(
            config
                .database
                .file_name()
                .and_then(|x| x.to_str())
                .ok_or(OperationError::UnsafeFilesystem)?,
            &paused,
        )?;
        checkpoint("restore-published");
        data.remove(staging)?;
        Ok(RestoreReceipt {
            restore_id,
            backup_id: id.to_owned(),
            installation: manifest.installation,
            schema,
            external_reconciliation_required: true,
            key_recovered: false,
        })
    }
    pub async fn recover_controller_key(
        &self,
        kit: &Path,
        recovery_secret: &[u8],
    ) -> Result<RestoreReceipt, OperationError> {
        self.config.validate_host()?;
        self.separate_kit_path(kit)?;
        let recovery = BackupKey::from_bytes(recovery_secret)?;
        let recovered = decode_kit(&recovery, &files::private_file(kit, 4096)?)?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT * FROM controller_restore_holds ORDER BY rowid DESC LIMIT 1")
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(OperationError::RecoveryRequired)?;
        let id: String = row.try_get("restore_id")?;
        let done:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM controller_operation_events WHERE request_id=? AND kind='key_recovered')").bind(&id).fetch_one(&mut *tx).await?;
        if done {
            return Err(OperationError::Conflict);
        }
        let installation = sqlx::query("SELECT * FROM controller_installation WHERE state='ready'")
            .fetch_one(&mut *tx)
            .await?;
        if installation.try_get::<Vec<u8>, _>("installation_id")?
            != recovered.metadata.installation.0
            || installation.try_get::<Vec<u8>, _>("key_id")? != recovered.metadata.key.0
            || installation.try_get::<i64, _>("key_version")? as u64
                != recovered.metadata.key_version
        {
            return Err(OperationError::Integrity);
        }
        let mut verifier = super::super::mac(&recovered).map_err(|_| OperationError::Integrity)?;
        use hmac::Mac;
        verifier.update(&super::super::header(&recovered.metadata));
        verifier
            .verify_slice(&installation.try_get::<Vec<u8>, _>("verifier")?)
            .map_err(|_| OperationError::Integrity)?;
        tx.commit().await?;
        let directory = self
            .bootstrap
            .directory
            .reopen()
            .map_err(|_| OperationError::UnsafeFilesystem)?;
        let _lock = directory.lock().map_err(|_| OperationError::Conflict)?;
        match self.bootstrap.load_existing().await {
            Ok(existing) if existing.metadata == recovered.metadata => {} // Resume after a published key, never overwrite it.
            Err(super::super::BootstrapError::MissingKey) => directory
                .publish(&super::super::encode(&recovered), |_| Ok(()))
                .map_err(|_| OperationError::RecoveryRequired)?,
            _ => return Err(OperationError::RecoveryRequired),
        }
        checkpoint("recovery-key-published");
        self.bootstrap
            .load_existing()
            .await
            .map_err(|_| OperationError::RecoveryRequired)?;
        super::super::secret_broker::verify_recovered_records(&self.store, &recovered)
            .await
            .map_err(|_| OperationError::Integrity)?;
        let auth = HumanAuth::open(&self.bootstrap)
            .await
            .map_err(|_| OperationError::RecoveryRequired)?;
        auth.restrict_sessions_after_restore(&id)
            .await
            .map_err(|_| OperationError::RecoveryRequired)?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        operator::event(
            &mut tx,
            &id,
            "key_recovered",
            &recovered.coding_record_seal(b"ops/recovered-key", id.as_bytes()),
        )
        .await?;
        checkpoint("recovery-before-commit");
        tx.commit().await?;
        checkpoint("recovery-after-commit");
        self.store.check_integrity().await?;
        Ok(RestoreReceipt {
            restore_id: id,
            backup_id: row.try_get("backup_id")?,
            installation: recovered.coding_installation(),
            schema: 23,
            external_reconciliation_required: true,
            key_recovered: true,
        })
    }
}
fn decode_kit(key: &BackupKey, bytes: &[u8]) -> Result<ControllerKey, OperationError> {
    let plain = backup::unseal(key, KIT_DOMAIN, bytes)?;
    let n = u32::from_be_bytes(
        plain
            .get(..4)
            .ok_or(OperationError::Integrity)?
            .try_into()
            .map_err(|_| OperationError::Integrity)?,
    ) as usize;
    if n > 2048 || plain.len() != 4 + n + super::super::FILE_BYTES {
        return Err(OperationError::Integrity);
    }
    let m: KitMetadata =
        serde_json::from_slice(&plain[4..4 + n]).map_err(|_| OperationError::Integrity)?;
    let key = super::super::decode(
        plain[4 + n..]
            .try_into()
            .map_err(|_| OperationError::Integrity)?,
    )
    .map_err(|_| OperationError::Integrity)?;
    if m.version != 1
        || m.installation != key.coding_installation()
        || m.key_id != hex(&key.metadata.key.0)
        || m.key_version != key.metadata.key_version
        || m.created_at < 0
    {
        return Err(OperationError::Integrity);
    }
    Ok(key)
}
#[cfg(test)]
fn checkpoint(s: &str) {
    super::tests::checkpoint(s)
}
#[cfg(not(test))]
fn checkpoint(_: &str) {}
