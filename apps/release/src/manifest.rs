use crate::files::{Entry, Result, Tree, digest};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Manifest {
    pub schema_version: u32,
    pub candidate: String,
    pub commit: String,
    pub target: String,
    pub paid_execution_enabled: bool,
    pub files: Vec<Entry>,
}

pub(crate) fn payload_allowed(path: &str) -> bool {
    matches!(
        path,
        "bin/rolvya-controller"
            | "web/index.html"
            | "web/sw.js"
            | "web/manifest.webmanifest"
            | "config/deployment.example.json"
            | "systemd/rolvya.service"
            | "SOURCE.json"
            | "SBOM.spdx.json"
            | "LICENSE"
            | "THIRD-PARTY-NOTICES.txt"
            | "EVIDENCE.json"
            | "COVERAGE.json"
    ) || (path.starts_with("web/app.")
        && path.ends_with(".js")
        && path.matches('/').count() == 1
        && path.len() < 100
        && crate::files::canonical(path))
}

fn valid(m: &Manifest) -> bool {
    let paths: BTreeSet<_> = m.files.iter().map(|f| f.path.as_str()).collect();
    m.schema_version == 1
        && m.candidate == "v0.1.0-rc.1"
        && !m.paid_execution_enabled
        && m.target == "x86_64-unknown-linux-gnu"
        && m.commit.len() == 40
        && m.commit
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        && paths.len() == m.files.len()
        && paths.len() == 13
        && paths.iter().all(|p| payload_allowed(p))
        && [
            "bin/rolvya-controller",
            "SOURCE.json",
            "SBOM.spdx.json",
            "EVIDENCE.json",
            "COVERAGE.json",
            "LICENSE",
            "THIRD-PARTY-NOTICES.txt",
            "config/deployment.example.json",
            "systemd/rolvya.service",
            "web/index.html",
            "web/sw.js",
            "web/manifest.webmanifest",
        ]
        .iter()
        .all(|p| paths.contains(p))
        && paths.iter().filter(|p| p.starts_with("web/")).count() == 4
        && m.files.iter().all(|f| {
            f.mode
                == if f.path == "bin/rolvya-controller" {
                    0o500
                } else {
                    0o400
                }
        })
}

pub(crate) fn verify(root: &Path, expected_manifest: &str) -> Result<Manifest> {
    let tree = Tree::open(root)?;
    let raw = tree.read("MANIFEST.json", 65536)?;
    if digest(&raw) != expected_manifest {
        return Err("manifest commitment mismatch".into());
    }
    let manifest: Manifest = serde_json::from_slice(&raw).map_err(|_| "invalid manifest")?;
    if !valid(&manifest) {
        return Err("unsupported or incomplete manifest".into());
    }
    if tree.read("MANIFEST.sha256", 65)? != format!("{expected_manifest}\n").as_bytes() {
        return Err("detached manifest digest mismatch".into());
    }
    let inventory = tree.inventory()?;
    let payload: Vec<_> = inventory
        .iter()
        .filter(|e| !matches!(e.path.as_str(), "MANIFEST.json" | "MANIFEST.sha256"))
        .cloned()
        .collect();
    if inventory.len() != payload.len() + 2
        || payload != manifest.files
        || inventory.iter().any(|f| {
            matches!(f.path.as_str(), "MANIFEST.json" | "MANIFEST.sha256") && f.mode != 0o400
        })
    {
        return Err("distribution inventory mismatch".into());
    }
    Ok(manifest)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};
    pub(crate) fn fixture(root: &Path) -> String {
        fs::set_permissions(root, fs::Permissions::from_mode(0o700)).unwrap();
        for p in ["bin", "web", "config", "systemd"] {
            fs::create_dir(root.join(p)).unwrap();
            fs::set_permissions(root.join(p), fs::Permissions::from_mode(0o700)).unwrap();
        }
        for p in [
            "bin/rolvya-controller",
            "web/index.html",
            "web/sw.js",
            "web/manifest.webmanifest",
            "web/app.abc.js",
            "config/deployment.example.json",
            "systemd/rolvya.service",
            "SOURCE.json",
            "SBOM.spdx.json",
            "LICENSE",
            "THIRD-PARTY-NOTICES.txt",
            "EVIDENCE.json",
            "COVERAGE.json",
        ] {
            fs::write(root.join(p), b"fixture").unwrap();
            fs::set_permissions(
                root.join(p),
                fs::Permissions::from_mode(if p.starts_with("bin/") { 0o500 } else { 0o400 }),
            )
            .unwrap();
        }
        let files = Tree::open(root).unwrap().inventory().unwrap();
        let m = Manifest {
            schema_version: 1,
            candidate: "v0.1.0-rc.1".into(),
            commit: "a".repeat(40),
            target: "x86_64-unknown-linux-gnu".into(),
            paid_execution_enabled: false,
            files,
        };
        let bytes = serde_json::to_vec(&m).unwrap();
        let sha = digest(&bytes);
        pause("payload");
        fs::write(root.join("MANIFEST.json"), bytes).unwrap();
        pause("manifest");
        fs::write(root.join("MANIFEST.sha256"), format!("{sha}\n")).unwrap();
        for p in ["MANIFEST.json", "MANIFEST.sha256"] {
            fs::set_permissions(root.join(p), fs::Permissions::from_mode(0o400)).unwrap();
        }
        sha
    }

    fn pause(stage: &str) {
        if std::env::var("ROLVYA_RELEASE_CRASH_STAGE").as_deref() == Ok(stage) {
            let marker = std::env::var("ROLVYA_RELEASE_CRASH_MARKER").unwrap();
            fs::write(marker, b"at boundary").unwrap();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }

    #[test]
    fn publication_crash_child() {
        if let Ok(root) = std::env::var("ROLVYA_RELEASE_CRASH_ROOT") {
            fixture(Path::new(&root));
        }
    }

    #[test]
    fn sigkill_partial_publication_never_becomes_a_verified_candidate() {
        use std::{
            process::{Command, Stdio},
            time::{Duration, Instant},
        };
        let baseline = tempfile::tempdir().unwrap();
        let sha = fixture(baseline.path());
        for stage in ["payload", "manifest"] {
            let root = tempfile::tempdir().unwrap();
            let marker = root.path().join("boundary");
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "manifest::tests::publication_crash_child"])
                .env("ROLVYA_RELEASE_CRASH_ROOT", root.path())
                .env("ROLVYA_RELEASE_CRASH_MARKER", &marker)
                .env("ROLVYA_RELEASE_CRASH_STAGE", stage)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let start = Instant::now();
            while !marker.exists() {
                if start.elapsed() > Duration::from_secs(5) {
                    let _ = child.kill();
                    panic!("child did not reach publication boundary");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            fs::remove_file(marker).unwrap();
            assert!(verify(root.path(), &sha).is_err());
            // Fresh inspection must not repair or invent the missing digest/manifest.
            assert!(verify(root.path(), &sha).is_err());
        }
    }

    #[test]
    fn complete_manifest_and_external_digest_are_required() {
        let d = tempfile::tempdir().unwrap();
        let sha = fixture(d.path());
        assert!(verify(d.path(), &sha).is_ok());
        assert!(verify(d.path(), &"0".repeat(64)).is_err());
    }

    #[test]
    fn extra_missing_changed_or_executable_payload_is_rejected() {
        for mutation in 0..5 {
            let d = tempfile::tempdir().unwrap();
            let sha = fixture(d.path());
            match mutation {
                0 => fs::write(d.path().join("unexpected"), b"x").unwrap(),
                1 => fs::remove_file(d.path().join("web/index.html")).unwrap(),
                2 => {
                    let p = d.path().join("web/index.html");
                    fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
                    fs::write(p, b"changed").unwrap();
                }
                3 => fs::set_permissions(
                    d.path().join("web/app.abc.js"),
                    fs::Permissions::from_mode(0o500),
                )
                .unwrap(),
                _ => fs::create_dir(d.path().join("extra")).unwrap(),
            }
            assert!(verify(d.path(), &sha).is_err(), "mutation {mutation}");
        }
    }
}
