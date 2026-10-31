//! Bounded coding artifacts. Production runner attestation is deliberately unavailable.
//! Private synthetic execution roots cannot be installed by library callers.
//! ```compile_fail
//! use rolvya_persistence_sqlite::orchestration::{SyntheticCoordinator,coding::VerificationReport};
//! fn forge(co:&SyntheticCoordinator,report:VerificationReport){ co.fixture_coding_run(report); }
//! ```
//! ```compile_fail
//! use rolvya_persistence_sqlite::orchestration::coding::AcceptedCoding;
//! ```
pub mod execution;
mod model;
mod store;
pub mod work;
use super::*;
use crate::controller_keys::{
    ControllerKey,
    github_app::{PinnedSource, SourceFile},
};
pub use model::*;
pub(crate) use store::{AcceptedCoding, stage_untrusted};
use subtle::ConstantTimeEq;
pub(crate) struct Authority {
    key: ControllerKey,
    #[cfg(test)]
    runs: std::sync::Mutex<BTreeMap<String, VerificationReport>>,
    #[cfg(test)]
    hook: Box<dyn Fn(&str) + Send + Sync>,
}
impl Authority {
    fn verify_runner(&self, report: &VerificationReport) -> Result<(), OrchestrationError> {
        #[cfg(test)]
        if self
            .runs
            .lock()
            .map_err(|_| OrchestrationError::RecoveryRequired)?
            .get(&report.execution)
            == Some(report)
        {
            return Ok(());
        }
        let _ = report;
        Err(OrchestrationError::VerificationUnavailable)
    }
    fn seal(&self, domain: &[u8], body: &impl Serialize) -> Result<Vec<u8>, OrchestrationError> {
        Ok(self
            .key
            .coding_record_seal(domain, &encode(body, 2 * 1024 * 1024)?)
            .to_vec())
    }
    fn verify(
        &self,
        domain: &[u8],
        body: &impl Serialize,
        seal: &[u8],
    ) -> Result<(), OrchestrationError> {
        let expected = self.seal(domain, body)?;
        if seal.len() != 32 || !bool::from(expected.ct_eq(seal)) {
            return Err(OrchestrationError::InvalidArtifact);
        }
        Ok(())
    }
    fn installation(&self) -> String {
        self.key.coding_installation()
    }
    fn checkpoint(&self, _p: &str) {
        #[cfg(test)]
        (self.hook)(_p);
    }
}
impl SyntheticCoordinator {
    pub(crate) fn attach_coding_key(&mut self, key: ControllerKey) {
        self.coding = Some(Authority {
            key,
            #[cfg(test)]
            runs: Default::default(),
            #[cfg(test)]
            hook: Box::new(|_| {}),
        });
    }
    fn coding_authority(&self) -> Result<&Authority, OrchestrationError> {
        self.coding
            .as_ref()
            .ok_or(OrchestrationError::VerificationUnavailable)
    }
}
#[cfg(test)]
impl SyntheticCoordinator {
    pub(crate) fn fixture_coding_run(&self, report: VerificationReport) {
        assert!(report.verifier.starts_with("private-synthetic-"));
        assert!(report.environment.starts_with("private-synthetic-"));
        self.coding
            .as_ref()
            .unwrap()
            .runs
            .lock()
            .unwrap()
            .insert(report.execution.clone(), report);
    }
    pub(crate) fn fixture_coding_hook(&mut self, hook: Box<dyn Fn(&str) + Send + Sync>) {
        self.coding.as_mut().unwrap().hook = hook;
    }
}

impl SyntheticCoordinator {
    pub(crate) fn coding_installation(
        &self,
    ) -> Result<String, crate::controller_keys::worker_auth::WorkerAuthError> {
        self.coding_authority()
            .map(|a| a.installation())
            .map_err(|_| crate::controller_keys::worker_auth::WorkerAuthError::KeyUnavailable)
    }
    pub(crate) fn coding_checkpoint(
        &self,
        p: &str,
    ) -> Result<(), crate::controller_keys::worker_auth::https::wire::ErrorCode> {
        self.coding_authority()
            .map_err(|_| crate::controller_keys::worker_auth::https::wire::ErrorCode::Unavailable)?
            .checkpoint(p);
        Ok(())
    }
}
