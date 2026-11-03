//! Restricted controller-host operations. No HTTP administration or execution activation.
use super::{ControllerBootstrap, ControllerKey};
use crate::PlanStore;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    path::{Path, PathBuf},
};
mod backup;
mod config;
pub(crate) mod files;
pub(crate) mod operator;
mod recovery;
mod service;
pub use backup::{
    BackupClass, BackupDestination, BackupManifest, BackupReceipt, FilesystemDestination,
};
pub use config::DeploymentConfig;
pub use operator::{EnrollmentSpec, HostOperator, OperatorRequest, SignedEnrollment};
pub use recovery::RestoreReceipt;
pub use service::asset_digest;
pub use service::{ControllerService, Readiness};
use sha2::{Digest, Sha256};
use sqlx::Row;
use zeroize::Zeroizing;

pub(super) fn now() -> Result<i64, OperationError> {
    crate::finance::now().map_err(|_| OperationError::Unavailable)
}
pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub(super) fn random_id() -> Result<String, OperationError> {
    let mut b = [0; 32];
    getrandom::fill(&mut b).map_err(|_| OperationError::Unavailable)?;
    Ok(hex(&b))
}
pub(super) fn canonical<T: Serialize>(v: &T) -> Result<Vec<u8>, OperationError> {
    serde_json::to_vec(v).map_err(|_| OperationError::Integrity)
}
pub(super) fn one(n: u64) -> Result<(), OperationError> {
    if n == 1 {
        Ok(())
    } else {
        Err(OperationError::Storage)
    }
}

/// A restriction-only restore guard. There is deliberately no resume/configuration switch.
pub(crate) async fn external_authority_available(
    c: &mut sqlx::SqliteConnection,
) -> Result<bool, sqlx::Error> {
    Ok(
        !sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM controller_restore_holds)")
            .fetch_one(c)
            .await?,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationError {
    InvalidConfiguration,
    UnsafeFilesystem,
    Unauthorized,
    Conflict,
    Unavailable,
    Storage,
    Integrity,
    ResourceLimit,
    RecoveryRequired,
}
impl fmt::Display for OperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "controller operation: {self:?}")
    }
}
impl std::error::Error for OperationError {}
impl From<sqlx::Error> for OperationError {
    fn from(_: sqlx::Error) -> Self {
        Self::Storage
    }
}
impl From<crate::StoreError> for OperationError {
    fn from(_: crate::StoreError) -> Self {
        Self::Integrity
    }
}

#[cfg(test)]
pub(in crate::controller_keys) mod tests;
