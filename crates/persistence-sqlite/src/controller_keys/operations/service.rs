use super::super::{
    human_auth::sessions::https::{BrowserAssets, HumanBrowserServer},
    worker_auth::https::tls::ServerIdentity,
};
use super::*;
use crate::orchestration::runtime::{
    ClosedRuntime, RuntimeAuthority, RuntimeClock, RuntimeConfig, RuntimeExit, RuntimeMode,
    RuntimeSupervisor,
};
use std::{future::Future, time::Duration};
/// Starts the control loop on the service's stop signal; returns its mode and task.
type RuntimeLauncher = Box<
    dyn FnOnce(
            tokio::sync::watch::Receiver<bool>,
        ) -> (RuntimeMode, tokio::task::JoinHandle<RuntimeExit>)
        + Send,
>;
fn launcher<R: RuntimeAuthority, C: RuntimeClock>(
    supervisor: RuntimeSupervisor<R, C>,
) -> RuntimeLauncher {
    Box::new(move |stop| (supervisor.mode(), tokio::spawn(supervisor.run(stop))))
}
#[derive(Serialize)]
pub struct Readiness {
    pub version: u32,
    pub configuration_valid: bool,
    pub persistent_storage_qualified: bool,
    pub database_ready: bool,
    pub controller_key_loaded: bool,
    pub authentication_available: bool,
    pub browser_https_ready: bool,
    pub email_delivery_available: bool,
    pub backup_verified: bool,
    pub restore_last_verified: bool,
    pub external_reconciliation_required: bool,
    pub worker_infrastructure_qualified: bool,
    pub provider_entitlement_verified: bool,
    pub financial_hard_gate_ready: bool,
    pub billable_execution_allowed: bool,
    /// Durable heartbeat of the installed control loop, read independently.
    pub control_loop_running: bool,
    pub control_loop_mode: Option<String>,
    pub qualification_blockers: Vec<crate::controller_keys::qualification::Domain>,
}
pub struct ControllerService {
    operator: HostOperator,
    server: std::sync::Arc<HumanBrowserServer>,
    runtime: RuntimeLauncher,
}
impl ControllerService {
    /// Ordinary startup loads an existing current schema/key; no setup or migrations.
    pub async fn prepare(config: DeploymentConfig) -> Result<Self, OperationError> {
        config.validate_host()?;
        if !persistent_storage(&config)? {
            return Err(OperationError::UnsafeFilesystem);
        }
        let operator = HostOperator::open_existing(config).await?;
        let key = operator
            .bootstrap
            .load_existing()
            .await
            .map_err(|_| OperationError::RecoveryRequired)?;
        drop(key);
        let assets = validated_assets(&operator.config).await?;
        let identity = tls_identity(&operator.config)?;
        Self::build(operator, identity, assets, None).await
    }
    /// Production composes only [`ClosedRuntime`]: the loop runs, admits nothing.
    async fn build(
        operator: HostOperator,
        identity: ServerIdentity,
        assets: BrowserAssets,
        runtime: Option<RuntimeLauncher>,
    ) -> Result<Self, OperationError> {
        let report = readiness(&operator).await?;
        if !report.authentication_available {
            return Err(OperationError::RecoveryRequired);
        }
        let unfinished:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM controller_restore_holds r WHERE NOT EXISTS(SELECT 1 FROM controller_operation_events e WHERE e.request_id=r.restore_id AND e.kind='key_recovered'))").fetch_one(&operator.store.pool).await?;
        if unfinished {
            return Err(OperationError::RecoveryRequired);
        }
        let server = HumanBrowserServer::open_operational(
            &operator.bootstrap,
            identity,
            &operator.config.canonical_origin,
            assets,
            operator.config.email.clone(),
            operator.config.support_url.clone(),
        )
        .await
        .map_err(|_| OperationError::Unavailable)?;
        let runtime = match runtime {
            Some(runtime) => runtime,
            None => launcher(
                RuntimeSupervisor::new(
                    operator.store.clone(),
                    ClosedRuntime,
                    crate::admission::SystemClock,
                    RuntimeConfig::default(),
                )
                .map_err(|_| OperationError::InvalidConfiguration)?,
            ),
        };
        Ok(Self {
            operator,
            server,
            runtime,
        })
    }
    /// Serves the human panel, backups and the closed control loop. No paid
    /// scheduler or provider constructor is started.
    pub async fn serve(
        self,
        shutdown: impl Future<Output = ()> + Send,
    ) -> Result<(), OperationError> {
        let listener = tokio::net::TcpListener::bind(self.operator.config.listen)
            .await
            .map_err(|_| OperationError::Unavailable)?;
        self.serve_listener(listener, shutdown).await
    }
    pub(super) async fn serve_listener(
        self,
        listener: tokio::net::TcpListener,
        shutdown: impl Future<Output = ()> + Send,
    ) -> Result<(), OperationError> {
        println!(
            "{{\"event\":\"human_https_listener_bound\",\"billable_execution_allowed\":false}}"
        );
        let (stop, mut stopped) = tokio::sync::watch::channel(false);
        let (mode, mut runtime) = (self.runtime)(stop.subscribe());
        println!(
            "{{\"event\":\"control_loop_started\",\"mode\":\"{}\",\"billable_execution_allowed\":false}}",
            match mode {
                RuntimeMode::Closed => "closed",
                RuntimeMode::Synthetic => "synthetic",
            }
        );
        let operator = std::sync::Arc::new(self.operator);
        let backups = operator.clone();
        let background = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                tokio::select! {_=stopped.changed()=>break,_=interval.tick()=>{}}
                for (class, seconds) in [(BackupClass::Frequent, 900), (BackupClass::Daily, 86400)]
                {
                    if *stopped.borrow() {
                        return;
                    }
                    let latest = sqlx::query_scalar::<_, Option<i64>>(
                        "SELECT max(recorded_at) FROM controller_backup_points WHERE class=?",
                    )
                    .bind(class.name())
                    .fetch_one(&backups.store.pool)
                    .await;
                    if let (Ok(t), Ok(last)) = (now(), latest)
                        && last.is_none_or(|last| t.saturating_sub(last) >= seconds)
                        && backups.backup(class).await.is_err()
                    {
                        eprintln!("controller backup verification unavailable");
                    }
                }
                if backups.retain_backups().await.is_err() {
                    eprintln!("controller retention verification unavailable");
                }
            }
        });
        let result = self
            .server
            .serve(listener, shutdown)
            .await
            .map_err(|_| OperationError::Unavailable);
        let _ = stop.send(true);
        let mut background = background;
        if tokio::time::timeout(Duration::from_secs(15), &mut background)
            .await
            .is_err()
        {
            background.abort();
            let _ = background.await;
            runtime.abort();
            let _ = runtime.await;
            return Err(OperationError::RecoveryRequired);
        }
        // Each pass is transactional, so aborting a stuck loop loses no authority;
        // the stale heartbeat then reads as not running.
        match tokio::time::timeout(Duration::from_secs(15), &mut runtime).await {
            Ok(Ok(RuntimeExit::Stopped)) => {}
            Ok(_) => eprintln!("controller control loop liveness unavailable"),
            Err(_) => {
                runtime.abort();
                let _ = runtime.await;
                return Err(OperationError::RecoveryRequired);
            }
        }
        operator.store.check_integrity().await?;
        operator.store.close().await;
        result
    }
    #[cfg(test)]
    pub(super) async fn fixture(
        operator: HostOperator,
        identity: ServerIdentity,
        assets: BrowserAssets,
    ) -> Result<Self, OperationError> {
        Self::build(operator, identity, assets, None).await
    }
}
impl HostOperator {
    pub async fn readiness(&self) -> Result<Readiness, OperationError> {
        self.config.validate_host()?;
        readiness(self).await
    }
}
async fn readiness(op: &HostOperator) -> Result<Readiness, OperationError> {
    op.store.check_integrity().await?;
    op.bootstrap
        .load_existing()
        .await
        .map_err(|_| OperationError::RecoveryRequired)?;
    let auth:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM human_enrollment e JOIN human_login_policy p ON p.singleton=e.singleton WHERE e.state='ready' AND p.password=1)").fetch_one(&op.store.pool).await?;
    let restored: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM controller_restore_holds)")
            .fetch_one(&op.store.pool)
            .await?;
    let latest=sqlx::query("SELECT request_id,envelope_sha256 FROM controller_backup_points WHERE recorded_at>=? AND destination=? ORDER BY recorded_at DESC,request_id DESC LIMIT 1").bind(now()?.saturating_sub(900)).bind(FilesystemDestination::open(&op.config.backup_destination)?.identity()).fetch_optional(&op.store.pool).await?;
    let backup = if let Some(row) = latest {
        let destination = FilesystemDestination::open(&op.config.backup_destination)?;
        match destination.get(row.try_get("request_id")?).await {
            Ok(bytes) => {
                hex(&Sha256::digest(&bytes)) == row.try_get::<String, _>("envelope_sha256")?
                    && backup::decode(&backup::BackupKey::load(&op.config.backup_key)?, &bytes)
                        .is_ok()
            }
            Err(_) => false,
        }
    } else {
        false
    };
    let restore_verified: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM controller_operation_events WHERE kind='key_recovered')",
    )
    .fetch_one(&op.store.pool)
    .await?;
    let liveness = op.store.runtime_liveness(now()?).await?;
    Ok(Readiness {
        version: 1,
        configuration_valid: true,
        persistent_storage_qualified: persistent_storage(&op.config)?,
        database_ready: true,
        controller_key_loaded: true,
        authentication_available: auth,
        browser_https_ready: false,
        email_delivery_available: false,
        backup_verified: backup,
        restore_last_verified: restore_verified,
        external_reconciliation_required: restored,
        worker_infrastructure_qualified: false,
        provider_entitlement_verified: false,
        financial_hard_gate_ready: true,
        billable_execution_allowed: false,
        control_loop_running: liveness.control_loop_running,
        control_loop_mode: liveness.mode,
        qualification_blockers: crate::controller_keys::qualification::REQUIRED.to_vec(),
    })
}
fn persistent_storage(config: &DeploymentConfig) -> Result<bool, OperationError> {
    for p in [
        config
            .database
            .parent()
            .ok_or(OperationError::UnsafeFilesystem)?,
        config.custody.as_path(),
        config.backup_destination.as_path(),
        config
            .backup_key
            .parent()
            .ok_or(OperationError::UnsafeFilesystem)?,
    ] {
        if !files::PrivateDirectory::open(p)?.persistent()? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn tls_identity(config: &DeploymentConfig) -> Result<ServerIdentity, OperationError> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    let mut chain = Vec::new();
    for path in &config.certificate_chain {
        chain.push(CertificateDer::from(
            files::private_file(path, 16384)?.to_vec(),
        ));
    }
    let key_bytes = files::private_file(&config.tls_key, 16384)?;
    let key = PrivateKeyDer::try_from(&key_bytes[..])
        .map_err(|_| OperationError::InvalidConfiguration)?
        .clone_key();
    let origin = url::Url::parse(&config.canonical_origin)
        .map_err(|_| OperationError::InvalidConfiguration)?;
    ServerIdentity::from_der(
        chain,
        key,
        origin
            .host_str()
            .ok_or(OperationError::InvalidConfiguration)?,
    )
    .map_err(|_| OperationError::Unauthorized)
}
async fn validated_assets(config: &DeploymentConfig) -> Result<BrowserAssets, OperationError> {
    if asset_digest(&config.assets)? != config.asset_sha256 {
        return Err(OperationError::Integrity);
    }
    BrowserAssets::load(&config.assets)
        .await
        .map_err(|_| OperationError::Integrity)
}
/// Nonsecret digest to pin the reviewed four-file build in host configuration.
pub fn asset_digest(directory: &Path) -> Result<String, OperationError> {
    use std::os::unix::fs::MetadataExt;
    let mut entries = std::fs::read_dir(directory)
        .map_err(|_| OperationError::UnsafeFilesystem)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| OperationError::UnsafeFilesystem)?;
    if entries.len() != 4 {
        return Err(OperationError::Integrity);
    }
    entries.sort_by_key(|e| e.file_name());
    let mut hash = Sha256::new();
    hash.update(b"rolvya/browser-distribution/v1\0");
    for e in entries {
        let m =
            std::fs::symlink_metadata(e.path()).map_err(|_| OperationError::UnsafeFilesystem)?;
        if !m.is_file()
            || m.nlink() != 1
            || m.mode() & 0o022 != 0
            || (m.uid() != 0 && m.uid() != rustix::process::geteuid().as_raw())
            || m.len() > 4 * 1024 * 1024
        {
            return Err(OperationError::UnsafeFilesystem);
        }
        let name = e
            .file_name()
            .into_string()
            .map_err(|_| OperationError::Integrity)?;
        if !matches!(
            name.as_str(),
            "index.html" | "sw.js" | "manifest.webmanifest"
        ) && !(name.starts_with("app.") && name.ends_with(".js"))
        {
            return Err(OperationError::Integrity);
        }
        let b = std::fs::read(e.path()).map_err(|_| OperationError::Storage)?;
        if b.len() as u64 != m.len() {
            return Err(OperationError::Integrity);
        }
        hash.update((name.len() as u64).to_be_bytes());
        hash.update(name.as_bytes());
        hash.update((b.len() as u64).to_be_bytes());
        hash.update(b);
    }
    Ok(hex(&hash.finalize()))
}
