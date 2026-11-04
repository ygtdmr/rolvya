use crate::{
    coverage,
    files::{Result, Tree, digest},
    gate::{self, Check, EvidenceKind, Outcome},
    manifest::{self, Manifest},
    process, sbom,
};
use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

pub(crate) const DOMAINS: [&str; 20] = [
    "rust-controller",
    "immutable-dag",
    "worker-lifecycle",
    "independent-verification",
    "git-publication",
    "human-authentication",
    "owner-approvals",
    "financial-hard-budget",
    "multi-account-provider-authority",
    "aws-isolation",
    "external-charging-bounds",
    "restore-reconciliation",
    "email-delivery",
    "tls-pki",
    "off-host-recovery",
    "dependency-advisories",
    "ci-codeql",
    "artifact-reproducibility",
    "browser-accessibility",
    "production-operations",
];

pub(crate) fn write(root: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    if !crate::files::canonical(name) {
        return Err("invalid publication path".into());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o400)
        .open(root.join(name))
        .map_err(|_| "candidate publication conflict")?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| "candidate publication failed")?;
    Ok(())
}
pub(crate) fn write_json(root: &Path, name: &str, value: &impl serde::Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| "serialization failed")?;
    bytes.push(b'\n');
    write(root, name, &bytes)
}

pub(crate) fn decision(commit: &str) -> gate::Decision {
    let mut checks = Vec::new();
    for id in [
        "locked-release-build",
        "four-file-browser",
        "sbom-license-inventory",
        "complete-payload",
    ] {
        checks.push(Check {
            id: id.into(),
            outcome: Outcome::Pass,
            evidence: EvidenceKind::Build,
            required_evidence: EvidenceKind::Build,
            required_for_build: true,
            required_for_offline: false,
            source_commit: commit.into(),
            reference: "SOURCE.json; MANIFEST.json; SBOM.spdx.json".into(),
        });
    }
    for id in DOMAINS {
        let external = matches!(
            id,
            "aws-isolation"
                | "external-charging-bounds"
                | "email-delivery"
                | "tls-pki"
                | "off-host-recovery"
        );
        checks.push(Check {
            id: id.into(),
            outcome: if external {
                Outcome::Blocked
            } else {
                Outcome::Unverified
            },
            evidence: EvidenceKind::SourceInspection,
            required_evidence: EvidenceKind::InstalledBinary,
            required_for_build: false,
            required_for_offline: true,
            source_commit: commit.into(),
            reference: format!(
                "COVERAGE.json; docs/development/release-candidate-validation.md#{id}"
            ),
        });
    }
    gate::assess(commit, checks)
}

pub(crate) fn seal(root: &Path, dist: &Path) -> Result<String> {
    let commit = process::clean(root)?;
    let source_tree = Tree::open(root)?;
    let before = Tree::open(dist)?;
    let initial = before.inventory()?;
    if initial.len() != 8 || initial.iter().any(|e| !manifest::payload_allowed(&e.path)) {
        return Err("unexpected unsealed payload".into());
    }
    let source: serde_json::Value = serde_json::from_slice(&before.read("SOURCE.json", 16384)?)
        .map_err(|_| "invalid initial provenance")?;
    let binary = before.read("bin/rolvya-controller", crate::files::LIMIT)?;
    let binary_sha = digest(&binary);
    if source["commit"] != commit
        || source["binary_sha256"] != binary_sha
        || source["paid_execution_enabled"] != false
    {
        return Err("stale initial build provenance".into());
    }
    let target = std::env::var("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| root.join("target"));
    let expected = Tree::open(&target.join("release"))?.built_controller_hash()?;
    if expected != binary_sha {
        return Err("installed binary differs from built binary".into());
    }
    let built_web = Tree::open(&root.join("apps/web/dist"))?;
    for entry in initial.iter().filter(|e| e.path.starts_with("web/")) {
        if digest(&built_web.read(
            entry.path.strip_prefix("web/").ok_or("invalid web path")?,
            4 * 1024 * 1024,
        )?) != entry.sha256
        {
            return Err("packaged web differs from built web".into());
        }
    }
    let dynamic = process::output(
        root,
        "readelf",
        &[
            "--dynamic",
            dist.join("bin/rolvya-controller")
                .to_str()
                .ok_or("invalid binary path")?,
        ],
    )?;
    let mut native = Vec::new();
    for line in dynamic.lines().filter(|l| l.contains("(NEEDED)")) {
        let library = line
            .split('[')
            .nth(1)
            .and_then(|s| s.strip_suffix(']'))
            .ok_or("invalid native dependency metadata")?;
        if ![
            "libgcc_s.so.1",
            "libm.so.6",
            "libc.so.6",
            "ld-linux-x86-64.so.2",
            "libpthread.so.0",
            "librt.so.1",
            "libdl.so.2",
        ]
        .contains(&library)
        {
            return Err("unreviewed dynamic native dependency".into());
        }
        native.push(library.to_owned());
    }
    if native.is_empty() {
        return Err("native dependency inventory unavailable".into());
    }
    process::output(
        &root.join("apps/web"),
        "node",
        &[
            "scripts/verify-dist.mjs",
            dist.join("web").to_str().ok_or("invalid asset directory")?,
        ],
    )?;
    let created = process::output(root, "git", &["show", "-s", "--format=%cI", "HEAD"])?;
    // SPDX creation time is UTC; git's ISO commit date may have a non-UTC offset.
    let created = process::output(
        root,
        "date",
        &["-u", "-d", created.trim(), "+%Y-%m-%dT%H:%M:%SZ"],
    )?;
    let (sbom, notices) = sbom::generate(root, dist, &commit, created.trim())?;
    write_json(dist, "SBOM.spdx.json", &sbom)?;
    process::output(
        &root.join("apps/web"),
        "node",
        &[
            "scripts/verify-sbom.mjs",
            dist.join("SBOM.spdx.json")
                .to_str()
                .ok_or("invalid SBOM directory")?,
        ],
    )?;
    write(dist, "THIRD-PARTY-NOTICES.txt", notices.as_bytes())?;
    write(dist, "LICENSE", &source_tree.read("LICENSE", 32768)?)?;
    write_json(dist, "COVERAGE.json", &coverage::generate(root)?)?;
    let binary_path = dist.join("bin/rolvya-controller");
    let mut denials = Vec::new();
    for command in [
        "serve",
        "signup",
        "reset-password",
        "enable-billing",
        "launch-worker",
        "clear-restore-hold",
    ] {
        process::denied(
            &binary_path,
            &[command, "/nonexistent/rolvya-release-deny.json"],
        )?;
        denials.push(json!({"command":command,"outcome":"expected_denial","evidenceKind":"installed_binary_negative_only"}));
    }
    let report = decision(&commit);
    write_json(
        dist,
        "EVIDENCE.json",
        &json!({"version":1,"sourceCommit":commit,"binarySha256":binary_sha,"decision":report,"installedSmoke":denials,
        "outcomeVocabulary":[Outcome::Pass,Outcome::Fail,Outcome::Blocked,Outcome::NotApplicable,Outcome::Unverified],
        "evidenceVocabulary":[EvidenceKind::SourceInspection,EvidenceKind::Build,EvidenceKind::Component,EvidenceKind::ProtocolFixture,EvidenceKind::InstalledBinary],
        "note":"Negative CLI smoke proves unsafe input denial, not a successful installed HTTPS/authentication/orchestration journey. Full-suite/CI outcomes are reported separately and must match this source head."}),
    )?;
    let rustc = process::output(root, "rustc", &["--version", "--verbose"])?;
    if !rustc.starts_with("rustc 1.99.1 ") || !rustc.contains("host: x86_64-unknown-linux-gnu") {
        return Err("unsupported release toolchain or target".into());
    }
    let node = process::output(root, "node", &["--version"])?;
    let npm = process::output(root, "npm", &["--version"])?;
    if !node.trim().starts_with("v24.") || npm.trim() != "11.9.0" {
        return Err("unsupported Node/npm toolchain".into());
    }
    if process::clean(root)? != commit {
        return Err("source changed during packaging".into());
    }
    fs::remove_file(dist.join("SOURCE.json")).map_err(|_| "initial provenance removal failed")?;
    write_json(
        dist,
        "SOURCE.json",
        &json!({"version":2,"candidate":"v0.1.0-rc.1","commit":commit,"target":"x86_64-unknown-linux-gnu","rustc":rustc.trim(),"node":node.trim(),"npm":npm.trim(),"cargoLockSha256":digest(&source_tree.read("Cargo.lock",1024*1024)?),"npmLockSha256":digest(&source_tree.read("apps/web/package-lock.json",2*1024*1024)?),"systemProvidedElfLibraries":native,"buildConfiguration":{"profile":"release","locked":true,"cargoFeatures":"controller defaults; SQLite only","nodeInstall":"ci --ignore-scripts --no-audit --no-fund","webMode":"production"},"binarySha256":binary_sha,"webAssets":initial.iter().filter(|f| f.path.starts_with("web/")).collect::<Vec<_>>(),"sbom":{"path":"SBOM.spdx.json","sha256":digest(&Tree::open(dist)?.read("SBOM.spdx.json",8*1024*1024)?)},"notices":"THIRD-PARTY-NOTICES.txt","evidence":"EVIDENCE.json","coverage":"COVERAGE.json","qualification":"build_candidate_only_offline_acceptance_incomplete","paid_execution_enabled":false,"productionReleaseQualified":false}),
    )?;
    for entry in fs::read_dir(dist).map_err(|_| "distribution unavailable")? {
        let path = entry.map_err(|_| "distribution unavailable")?.path();
        if path.is_file() {
            fs::set_permissions(path, fs::Permissions::from_mode(0o400))
                .map_err(|_| "candidate permissions failed")?;
        }
    }
    let files = Tree::open(dist)?.inventory()?;
    let manifest = Manifest {
        schema_version: 1,
        candidate: "v0.1.0-rc.1".into(),
        commit,
        target: "x86_64-unknown-linux-gnu".into(),
        paid_execution_enabled: false,
        files,
    };
    write_json(dist, "MANIFEST.json", &manifest)?;
    let sha = digest(&Tree::open(dist)?.read("MANIFEST.json", 65536)?);
    write(dist, "MANIFEST.sha256", format!("{sha}\n").as_bytes())?;
    manifest::verify(dist, &sha)?;
    fs::File::open(dist)
        .and_then(|f| f.sync_all())
        .map_err(|_| "directory sync failed")?;
    Ok(sha)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assessment_contains_every_domain_and_never_equates_missing_evidence_with_success() {
        let r = decision(&"a".repeat(40));
        assert_eq!(r.checks.len(), 24);
        assert!(r.build_candidate_qualified);
        assert!(
            !r.offline_release_candidate_qualified
                && !r.production_release_qualified
                && !r.paid_execution_authorized
        );
        for id in DOMAINS {
            let c = r.checks.iter().find(|c| c.id == id).unwrap();
            assert!(c.required_for_offline);
            assert_ne!(c.outcome, Outcome::Pass);
        }
    }
}
