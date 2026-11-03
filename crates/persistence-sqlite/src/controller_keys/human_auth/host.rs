//! Sealed adapters for separately approved controller-host operations.
use super::*;
use crate::controller_keys::operations::operator::{VerifiedBinding, VerifiedEnrollment};
impl EnrollmentAuthority for VerifiedEnrollment {
    fn durable_evidence(&self) -> Option<(&str, &str)> {
        Some((self.request.request_id.as_str(), &self.evidence_sha256))
    }
    fn recovery_checkpoint(&self) -> Option<&Option<String>> {
        Some(&self.restore_checkpoint)
    }
    fn check(&self, c: &AdminContext<'_>) -> Result<EmailEvidence, AuthError> {
        if c.action != AdminAction::FirstEnrollment
            || c.target.is_some()
            || c.request.request_id != self.request.request_id
            || c.request.evidence != self.request.evidence
            || c.payload != self.payload
            || c.now < self.not_before
            || c.now >= self.expires
        {
            return Err(AuthError::Denied);
        }
        Ok(EmailEvidence::IndependentlyVerified)
    }
}
impl sessions::owner::ProjectBindingAuthority for VerifiedBinding {
    fn verify(&self, c: &sessions::owner::BindingContext<'_>) -> Result<(), AuthError> {
        if c.request.request_id != self.request.request_id
            || c.request.evidence != self.request.evidence
            || c.project != &self.project
            || c.user != self.user
            || c.legacy != self.legacy
            || c.restore_checkpoint != self.restore_checkpoint
            || c.now < self.not_before
            || c.now >= self.expires
        {
            return Err(AuthError::Denied);
        }
        Ok(())
    }
}
impl HumanAuth {
    pub(in crate::controller_keys) fn host_enrollment_commitment(
        &self,
        r: &Enrollment,
        password: Option<&Password>,
    ) -> [u8; 32] {
        self.enrollment_commitment(r, password)
    }
    pub(in crate::controller_keys) fn host_enrollment_authority(
        &mut self,
        grant: VerifiedEnrollment,
    ) {
        self.authority = Arc::new(grant)
    }
    /// An existing authenticated session confirms an exact, sealed host approval.
    /// An arbitrary project/user pair cannot create a grant through this method.
    pub async fn bind_prepared_project(
        &self,
        h: &sessions::SessionHandle,
        id: &str,
        c: &sessions::BrowserContext,
    ) -> Result<(), AuthError> {
        let grant =
            crate::controller_keys::operations::operator::binding(&self.store, &self.key, id)
                .await
                .map_err(|_| AuthError::Denied)?;
        self.bind_project_with_authority(h, &grant.project, &grant.request, c, &grant)
            .await
    }
}
