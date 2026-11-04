//! Release labels describe evidence. They never grant execution authority.
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Outcome {
    Pass,
    Fail,
    Blocked,
    NotApplicable,
    Unverified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvidenceKind {
    SourceInspection,
    Build,
    Component,
    ProtocolFixture,
    InstalledBinary,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Check {
    pub id: String,
    pub outcome: Outcome,
    pub evidence: EvidenceKind,
    pub required_evidence: EvidenceKind,
    pub required_for_build: bool,
    pub required_for_offline: bool,
    pub source_commit: String,
    pub reference: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Decision {
    pub build_candidate_qualified: bool,
    pub offline_release_candidate_qualified: bool,
    pub production_release_qualified: bool,
    pub paid_execution_authorized: bool,
    pub checks: Vec<Check>,
}

pub(crate) fn assess(commit: &str, checks: Vec<Check>) -> Decision {
    let mut ids = std::collections::BTreeSet::new();
    let current = commit.len() == 40
        && commit
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        && checks.iter().all(|c| {
            c.source_commit == commit
                && !c.id.is_empty()
                && ids.insert(&c.id)
                && !c.reference.is_empty()
                && c.reference.len() <= 1024
                && !c.reference.chars().any(char::is_control)
        });
    let demonstrated = |c: &Check| c.outcome == Outcome::Pass && c.evidence == c.required_evidence;
    let build = current
        && checks.iter().any(|c| c.required_for_build)
        && checks
            .iter()
            .filter(|c| c.required_for_build)
            .all(demonstrated);
    let offline = build
        && checks.iter().any(|c| c.required_for_offline)
        && checks
            .iter()
            .filter(|c| c.required_for_offline)
            .all(demonstrated);
    Decision {
        build_candidate_qualified: build,
        offline_release_candidate_qualified: offline,
        production_release_qualified: false,
        paid_execution_authorized: false,
        checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const COMMIT: &str = "1234567890123456789012345678901234567890";
    fn check(id: &str, kind: EvidenceKind) -> Check {
        Check {
            id: id.into(),
            outcome: Outcome::Pass,
            evidence: kind,
            required_evidence: kind,
            required_for_build: true,
            required_for_offline: true,
            source_commit: COMMIT.into(),
            reference: "executed-test-receipt".into(),
        }
    }

    #[test]
    fn complete_build_evidence_can_qualify_only_a_build_candidate() {
        let mut c = check("locked-build", EvidenceKind::Build);
        c.required_for_offline = false;
        let mut pending = check("installed-system", EvidenceKind::InstalledBinary);
        pending.required_for_build = false;
        pending.outcome = Outcome::Unverified;
        let report = assess(COMMIT, vec![c, pending]);
        assert!(report.build_candidate_qualified);
        assert!(!report.offline_release_candidate_qualified);
        assert!(!report.production_release_qualified);
        assert!(!report.paid_execution_authorized);
    }

    #[test]
    fn component_or_protocol_success_is_not_installed_binary_acceptance() {
        for weaker in [EvidenceKind::Component, EvidenceKind::ProtocolFixture] {
            let mut c = check("installed-https", EvidenceKind::InstalledBinary);
            c.evidence = weaker;
            let report = assess(COMMIT, vec![c]);
            assert!(!report.build_candidate_qualified);
            assert!(!report.offline_release_candidate_qualified);
        }
    }

    #[test]
    fn missing_failed_blocked_and_unverified_evidence_never_passes() {
        assert!(!assess(COMMIT, vec![]).build_candidate_qualified);
        for outcome in [
            Outcome::Fail,
            Outcome::Blocked,
            Outcome::Unverified,
            Outcome::NotApplicable,
        ] {
            let mut c = check("required-test", EvidenceKind::Component);
            c.outcome = outcome;
            assert!(!assess(COMMIT, vec![c]).build_candidate_qualified);
        }
    }

    #[test]
    fn stale_source_duplicate_identity_and_missing_reference_fail_closed() {
        let c = check("source", EvidenceKind::SourceInspection);
        assert!(!assess(&"a".repeat(40), vec![c.clone()]).build_candidate_qualified);
        assert!(!assess(COMMIT, vec![c.clone(), c.clone()]).build_candidate_qualified);
        let mut no_reference = c;
        no_reference.reference.clear();
        assert!(!assess(COMMIT, vec![no_reference]).build_candidate_qualified);
    }

    #[test]
    fn complete_offline_evidence_never_authorizes_paid_or_production_execution() {
        let report = assess(
            COMMIT,
            vec![check(
                "complete-installed-journey",
                EvidenceKind::InstalledBinary,
            )],
        );
        assert!(report.build_candidate_qualified);
        assert!(report.offline_release_candidate_qualified);
        assert!(!report.production_release_qualified);
        assert!(!report.paid_execution_authorized);
    }
}
