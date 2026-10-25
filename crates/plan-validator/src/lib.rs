//! Pure, side-effect-free validation and versioned SHA-256 fingerprints.
//! A validated borrow prevents changing draft content while this snapshot exists.
//! It is not persisted activation or permission to launch a billable attempt.
#![forbid(unsafe_code)]
mod canonical;
mod validation;

use rolvya_domain::{ids::TaskId, plan::PlanContent};
use sha2::{Digest, Sha256};
use std::fmt;
pub use validation::{AssignmentDenial, AssignmentPolicy, Diagnostic, DiagnosticCode, Limits};

pub const CANONICAL_FORMAT: &str = "rolvya-plan/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanFingerprint([u8; 32]);
impl PlanFingerprint {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
impl fmt::Display for PlanFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct ValidatedPlan<'a> {
    plan: &'a PlanContent,
    order: Vec<TaskId>,
    canonical: Vec<u8>,
    fingerprint: PlanFingerprint,
}
impl<'a> ValidatedPlan<'a> {
    pub fn plan(&self) -> &'a PlanContent {
        self.plan
    }
    /// Explicit dependency order, including groups; this is not a dispatch queue.
    pub fn topological_order(&self) -> &[TaskId] {
        &self.order
    }
    pub fn fingerprint(&self) -> PlanFingerprint {
        self.fingerprint
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }
}

/// Capability policy must come from a trusted caller. A successful pure check
/// cannot establish live provider entitlement, funds, storage activation or leases.
pub fn validate<'a>(
    plan: &'a PlanContent,
    policy: &impl AssignmentPolicy,
    limits: Limits,
) -> Result<ValidatedPlan<'a>, Vec<Diagnostic>> {
    let order = validation::check(plan, policy, limits)?;
    let canonical = canonical::encode(plan).map_err(|_| {
        vec![Diagnostic {
            code: DiagnosticCode::CanonicalEncoding,
            task: None,
            related: None,
            field: "plan",
        }]
    })?;
    let fingerprint = PlanFingerprint(Sha256::digest(&canonical).into());
    Ok(ValidatedPlan {
        plan,
        order,
        canonical,
        fingerprint,
    })
}
