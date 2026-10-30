use super::*;
use crate::controller_keys::github_app::{GitSha, PinnedSource, RepositoryId, SourceFile};
/// Operations contain exact UTF-8 bytes. No newline normalization or mode override.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum FileChange {
    Add {
        path: String,
        content: String,
    },
    Modify {
        path: String,
        original_sha256: Digest,
        content: String,
    },
    Delete {
        path: String,
        original_sha256: Digest,
    },
}
impl std::fmt::Debug for FileChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FileChange [untrusted content redacted]")
    }
}
impl FileChange {
    pub fn path(&self) -> &str {
        match self {
            Self::Add { path, .. } | Self::Modify { path, .. } | Self::Delete { path, .. } => path,
        }
    }
}
pub const CODING_CRITERION: &str = "coding/independent-tests-v1";
pub const MAX_CODING_BYTES: usize = 64 * 1024;
pub const MAX_CHANGED_FILES: usize = 16;
pub const MAX_FILE_BYTES: usize = 16 * 1024;
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
/// Hashed into the frozen task context before activation. This is data, not authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodingContext {
    pub format: CodingFormat,
    pub installation: String,
    pub repository: RepositoryId,
    pub repository_location: String,
    pub base_commit: GitSha,
    pub base_tree: GitSha,
    pub source_sha256: Digest,
    pub paths: Vec<String>,
    pub verification: VerificationPolicy,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodingFormat {
    #[serde(rename = "code_change_v1")]
    V1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationPolicy {
    pub identity: String,
    /// Fixed command argv, never executed by the controller.
    pub commands: Vec<Vec<String>>,
    pub required_tests: Vec<String>,
    pub timeout_seconds: u32,
    pub cpu_millis: u32,
    pub memory_mib: u32,
    pub max_output_bytes: u32,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodingResult {
    pub format: CodingFormat,
    pub installation: String,
    pub binding: WorkerBinding,
    pub repository: RepositoryId,
    pub base_commit: GitSha,
    pub base_tree: GitSha,
    pub source_sha256: Digest,
    pub inputs: Vec<SyntheticInput>,
    pub financial_event: i64,
    pub operation: Digest,
    pub changes: Vec<FileChange>,
    pub resulting_tree_sha256: Digest,
    /// Untrusted worker provenance reference; never a verifier success claim.
    pub worker_execution: String,
    pub verification_execution: String,
}
impl std::fmt::Debug for CodingResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodingResult")
            .field("binding", &self.binding)
            .finish_non_exhaustive()
    }
}
/// Untrusted report. Only a sealed independently attributable runner can verify it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationReport {
    pub format: CodingFormat,
    pub binding: WorkerBinding,
    pub artifact_sha256: Digest,
    pub source_sha256: Digest,
    pub change_set_sha256: Digest,
    pub policy_sha256: Digest,
    pub verifier: String,
    pub verifier_version: u32,
    pub environment: String,
    pub execution: String,
    pub started_at: i64,
    pub completed_at: i64,
    pub terminal: VerificationTerminal,
    pub exit_code: i32,
    pub tests_completed: Vec<String>,
    pub outputs: Vec<VerificationOutput>,
    pub limits: VerificationPolicy,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationTerminal {
    Completed,
    Failed,
    Interrupted,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationOutput {
    pub identity: String,
    pub content: String,
    pub sha256: Digest,
}
impl std::fmt::Debug for VerificationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VerificationReport [untrusted, redacted]")
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodingState {
    StructurallyValid {
        artifact_sha256: Digest,
        change_set_sha256: Digest,
    },
    IndependentlyVerified {
        evidence_sha256: Digest,
    },
    Accepted(ValidationReceipt),
}
/// SHA-256 of canonical typed bytes. Never hashes ambiguous JSON maps.
pub fn commitment(value: &impl Serialize) -> Result<Digest, OrchestrationError> {
    Ok(Digest::new(hash(&encode(value, 2 * 1024 * 1024)?))?)
}
pub fn tree_commitment(files: &[SourceFile]) -> Result<Digest, OrchestrationError> {
    let mut ordered = files.to_vec();
    ordered.sort_by(|a, b| a.path.cmp(&b.path));
    commitment(&ordered)
}
pub(super) fn path(s: &str) -> Result<(), OrchestrationError> {
    if s.is_empty()
        || s.len() > 240
        || s.split('/').count() > 16
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b))
        || s.split('/')
            .any(|c| c.is_empty() || c.starts_with('.') || c.ends_with('.') || c.ends_with(".lock"))
    {
        return Err(OrchestrationError::ScopeMismatch);
    }
    let l = s.to_ascii_lowercase();
    if l.split('/').any(|c| {
        matches!(
            c,
            "security.md"
                | "agents.md"
                | "codeowners"
                | "cargo.toml"
                | "cargo.lock"
                | "package.json"
                | "dockerfile"
        ) || c.contains("credential")
            || c.contains("secret")
    }) {
        return Err(OrchestrationError::ScopeMismatch);
    }
    Ok(())
}
fn content(s: &str) -> Result<(), OrchestrationError> {
    if s.len() > MAX_FILE_BYTES {
        return Err(OrchestrationError::LimitExceeded);
    }
    if s.as_bytes().contains(&0)
        || s.chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(OrchestrationError::InvalidArtifact);
    }
    // Heuristic only. This is not a proof that generated source contains no secret.
    let l = s.to_ascii_lowercase();
    if [
        "-----begin ",
        "ghp_",
        "github_pat_",
        "akia",
        "aws_secret_access_key",
        "xoxb-",
    ]
    .iter()
    .any(|p| l.contains(p))
    {
        return Err(OrchestrationError::InvalidArtifact);
    }
    Ok(())
}
pub(super) fn inventory(files: &[SourceFile]) -> Result<(), OrchestrationError> {
    if files.len() > 256 {
        return Err(OrchestrationError::LimitExceeded);
    }
    let mut total = 0usize;
    let mut names = BTreeSet::new();
    let mut components = BTreeMap::new();
    for f in files {
        // Untouched paths may be sensitive, but must remain canonical regular files.
        if f.path.is_empty()
            || f.path.len() > 240
            || f.path.split('/').count() > 16
            || f.path
                .split('/')
                .any(|c| c.is_empty() || c == "." || c == "..")
            || !f
                .path
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b))
            || !matches!(f.mode.as_str(), "100644" | "100755")
            || !names.insert(f.path.to_ascii_lowercase())
        {
            return Err(OrchestrationError::InvalidArtifact);
        }
        let mut prefix = String::new();
        for part in f.path.split('/') {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            if let Some(previous) = components.insert(prefix.to_ascii_lowercase(), prefix.clone())
                && previous != prefix
            {
                return Err(OrchestrationError::Conflict);
            }
        }
        total = total
            .checked_add(f.bytes.len())
            .ok_or(OrchestrationError::LimitExceeded)?;
        if f.bytes.len() > 65536 || total > MAX_SOURCE_BYTES {
            return Err(OrchestrationError::LimitExceeded);
        }
    }
    for n in &names {
        let mut parent = n.as_str();
        while let Some((p, _)) = parent.rsplit_once('/') {
            if names.contains(p) {
                return Err(OrchestrationError::Conflict);
            }
            parent = p;
        }
    }
    Ok(())
}
/// Pure structural validation, never an acceptance or publication capability.
pub fn apply_changes(
    source: &PinnedSource,
    changes: &[FileChange],
    allowed: &[String],
) -> Result<Vec<SourceFile>, OrchestrationError> {
    inventory(&source.files)?;
    if changes.is_empty()
        || changes.len() > MAX_CHANGED_FILES
        || allowed.is_empty()
        || allowed.len() > 32
    {
        return Err(OrchestrationError::LimitExceeded);
    }
    let mut files: BTreeMap<String, SourceFile> = source
        .files
        .iter()
        .map(|f| (f.path.clone(), f.clone()))
        .collect();
    let mut seen = BTreeSet::new();
    let mut total = 0usize;
    for c in changes {
        let p = c.path();
        path(p)?;
        if !seen.insert(p.to_ascii_lowercase()) {
            return Err(OrchestrationError::Conflict);
        }
        if !allowed
            .iter()
            .any(|a| p == a || (a.ends_with('/') && p.starts_with(a)))
        {
            return Err(OrchestrationError::ScopeMismatch);
        }
        let old = files.get(p);
        let (text, mode) = match c {
            FileChange::Add { content, .. } => {
                if old.is_some() || files.keys().any(|a| a.eq_ignore_ascii_case(p)) {
                    return Err(OrchestrationError::Conflict);
                }
                (Some(content), "100644".into())
            }
            FileChange::Modify {
                original_sha256,
                content,
                ..
            } => {
                let f = old.ok_or(OrchestrationError::MissingArtifact)?;
                if hash(&f.bytes) != original_sha256.as_str() {
                    return Err(OrchestrationError::Conflict);
                }
                (Some(content), f.mode.clone())
            }
            FileChange::Delete {
                original_sha256, ..
            } => {
                let f = old.ok_or(OrchestrationError::MissingArtifact)?;
                if hash(&f.bytes) != original_sha256.as_str() {
                    return Err(OrchestrationError::Conflict);
                }
                (None, f.mode.clone())
            }
        };
        if let Some(text) = text {
            content(text)?;
            total += text.len();
            if total > MAX_CODING_BYTES {
                return Err(OrchestrationError::LimitExceeded);
            }
            files.insert(
                p.into(),
                SourceFile {
                    path: p.into(),
                    mode,
                    sha: crate::controller_keys::github_app::coding_blob(text.as_bytes()),
                    bytes: text.as_bytes().to_vec(),
                },
            );
        } else {
            files.remove(p);
        }
    }
    let result: Vec<_> = files.into_values().collect();
    inventory(&result)?;
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coding_add_modify_delete_preserves_exact_other_bytes() {
        let file = |path: &str, b: &[u8]| SourceFile {
            path: path.into(),
            mode: "100644".into(),
            sha: GitSha::parse("1".repeat(40)).unwrap(),
            bytes: b.to_vec(),
        };
        let source = PinnedSource {
            repository: RepositoryId::new(99).unwrap(),
            commit: GitSha::parse("2".repeat(40)).unwrap(),
            tree: GitSha::parse("3".repeat(40)).unwrap(),
            files: vec![
                file("src/a.rs", b"old\r\n"),
                file("src/b.rs", b"keep\n"),
                file("src/delete.rs", b"delete\n"),
            ],
        };
        let changes = vec![
            FileChange::Modify {
                path: "src/a.rs".into(),
                original_sha256: Digest::new(hash(b"old\r\n")).unwrap(),
                content: "new\r\n".into(),
            },
            FileChange::Add {
                path: "src/new.rs".into(),
                content: "new file\n".into(),
            },
            FileChange::Delete {
                path: "src/delete.rs".into(),
                original_sha256: Digest::new(hash(b"delete\n")).unwrap(),
            },
        ];
        let result = apply_changes(&source, &changes, &["src/".into()]).unwrap();
        assert_eq!(
            result
                .iter()
                .map(|f| (f.path.as_str(), f.bytes.as_slice()))
                .collect::<Vec<_>>(),
            vec![
                ("src/a.rs", b"new\r\n".as_slice()),
                ("src/b.rs", b"keep\n".as_slice()),
                ("src/new.rs", b"new file\n".as_slice())
            ]
        );
    }
    fn source() -> PinnedSource {
        PinnedSource {
            repository: RepositoryId::new(99).unwrap(),
            commit: GitSha::parse("1".repeat(40)).unwrap(),
            tree: GitSha::parse("2".repeat(40)).unwrap(),
            files: vec![
                SourceFile {
                    path: "src/a.rs".into(),
                    mode: "100644".into(),
                    sha: crate::controller_keys::github_app::coding_blob(b"old\n"),
                    bytes: b"old\n".to_vec(),
                },
                SourceFile {
                    path: "src/keep.rs".into(),
                    mode: "100755".into(),
                    sha: crate::controller_keys::github_app::coding_blob(b"keep\r\n"),
                    bytes: b"keep\r\n".to_vec(),
                },
            ],
        }
    }
    fn modify(p: &str, old: &[u8], new: &str) -> FileChange {
        FileChange::Modify {
            path: p.into(),
            original_sha256: Digest::new(hash(old)).unwrap(),
            content: new.into(),
        }
    }
    #[test]
    fn coding_wrong_original_missing_delete_and_add_overwrite_denied() {
        let s = source();
        for c in [
            modify("src/a.rs", b"wrong", "new"),
            FileChange::Delete {
                path: "src/missing.rs".into(),
                original_sha256: Digest::new(hash(b"old\n")).unwrap(),
            },
            FileChange::Add {
                path: "src/a.rs".into(),
                content: "overwrite".into(),
            },
        ] {
            assert!(apply_changes(&s, &[c], &["src/".into()]).is_err());
        }
    }
    #[test]
    fn coding_duplicate_case_collision_and_file_directory_conflicts_denied() {
        let s = source();
        for changes in [
            vec![
                modify("src/a.rs", b"old\n", "new"),
                modify("src/a.rs", b"new", "later"),
            ],
            vec![FileChange::Add {
                path: "src/A.rs".into(),
                content: "case".into(),
            }],
            vec![FileChange::Add {
                path: "src/a.rs/child.rs".into(),
                content: "child".into(),
            }],
        ] {
            assert!(apply_changes(&s, &changes, &["src/".into()]).is_err());
        }
    }
    #[test]
    fn coding_traversal_encoded_paths_sensitive_paths_and_scope_denied() {
        for p in [
            "../outside",
            "src/../a.rs",
            "src/%2e%2e/a.rs",
            "src/a\\b.rs",
            "/src/a.rs",
            "src//a.rs",
            "src/a.rs/",
            ".git/config",
            ".github/workflows/a.yml",
            "src/credentials.rs",
            "src/Secret.rs",
            "src/Cargo.toml",
            "src/.hidden",
        ] {
            assert!(
                apply_changes(
                    &source(),
                    &[FileChange::Add {
                        path: p.into(),
                        content: "no".into()
                    }],
                    &["src/".into()]
                )
                .is_err(),
                "{p}"
            );
        }
        assert!(
            apply_changes(
                &source(),
                &[modify("src/a.rs", b"old\n", "no")],
                &["tests/".into()]
            )
            .is_err()
        );
    }
    #[test]
    fn coding_no_mode_escalation_symlinks_submodules_or_binary() {
        let mut s = source();
        for mode in ["120000", "160000", "040000", "100664"] {
            s.files[0].mode = mode.into();
            assert!(
                apply_changes(&s, &[modify("src/a.rs", b"old\n", "new")], &["src/".into()])
                    .is_err()
            );
        }
        for text in ["binary\0data", "control\u{1b}data"] {
            assert!(
                apply_changes(
                    &source(),
                    &[modify("src/a.rs", b"old\n", text)],
                    &["src/".into()]
                )
                .is_err()
            );
        }
        let changed = apply_changes(
            &source(),
            &[modify("src/keep.rs", b"keep\r\n", "changed\r\n")],
            &["src/".into()],
        )
        .unwrap();
        assert_eq!(changed[1].mode, "100755");
        let added = apply_changes(
            &source(),
            &[FileChange::Add {
                path: "src/new.rs".into(),
                content: "".into(),
            }],
            &["src/".into()],
        )
        .unwrap();
        assert_eq!(
            added.iter().find(|f| f.path == "src/new.rs").unwrap().mode,
            "100644"
        );
    }
    #[test]
    fn coding_count_bytes_depth_and_secret_heuristic_are_bounded() {
        let s = source();
        let oversized = "x".repeat(MAX_FILE_BYTES + 1);
        assert!(
            apply_changes(
                &s,
                &[modify("src/a.rs", b"old\n", &oversized)],
                &["src/".into()]
            )
            .is_err()
        );
        let many = (0..MAX_CHANGED_FILES + 1)
            .map(|n| FileChange::Add {
                path: format!("src/file{n}.rs"),
                content: "x".into(),
            })
            .collect::<Vec<_>>();
        assert!(apply_changes(&s, &many, &["src/".into()]).is_err());
        let deep = format!("{}a.rs", "src/".repeat(17));
        assert!(
            apply_changes(
                &s,
                &[FileChange::Add {
                    path: deep,
                    content: "x".into()
                }],
                &["src/".into()]
            )
            .is_err()
        );
        // Only public marker prefixes, never a credential fixture.
        for text in [
            "-----BEGIN PRIVATE KEY-----",
            "aws_secret_access_key = value",
            "github_pat_marker",
        ] {
            assert!(
                apply_changes(&s, &[modify("src/a.rs", b"old\n", text)], &["src/".into()]).is_err()
            );
        }
    }
    #[test]
    fn coding_unknown_formats_fields_modes_and_ambiguous_encoding_rejected() {
        let valid = serde_json::to_vec(&FileChange::Add {
            path: "src/new.rs".into(),
            content: "x".into(),
        })
        .unwrap();
        let mut v: serde_json::Value = serde_json::from_slice(&valid).unwrap();
        v["mode"] = serde_json::json!("100755");
        assert!(serde_json::from_value::<FileChange>(v).is_err());
        assert!(serde_json::from_str::<FileChange>("{\"operation\":\"add\",\"path\":\"src/a.rs\",\"path\":\"src/b.rs\",\"content\":\"x\"}").is_err());
        assert!(serde_json::from_str::<CodingFormat>("\"code_change_v2\"").is_err());
    }
    #[test]
    fn coding_overlapping_changes_are_separate_and_report_conflicts() {
        let s = source();
        let a = apply_changes(&s, &[modify("src/a.rs", b"old\n", "A")], &["src/".into()]).unwrap();
        let b = apply_changes(&s, &[modify("src/a.rs", b"old\n", "B")], &["src/".into()]).unwrap();
        assert_ne!(tree_commitment(&a).unwrap(), tree_commitment(&b).unwrap());
        let source_a = PinnedSource { files: a, ..s };
        assert!(
            apply_changes(
                &source_a,
                &[modify("src/a.rs", b"old\n", "B")],
                &["src/".into()]
            )
            .is_err()
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodingConflict {
    pub left_path: String,
    pub right_path: String,
    pub left_artifact: Digest,
    pub right_artifact: Digest,
}

#[cfg(test)]
#[test]
fn coding_case_colliding_directory_components_fail_closed() {
    let files = vec![SourceFile {
        path: "src/a.rs".into(),
        mode: "100644".into(),
        sha: crate::controller_keys::github_app::coding_blob(b"old"),
        bytes: b"old".to_vec(),
    }];
    let s = PinnedSource {
        repository: RepositoryId::new(99).unwrap(),
        commit: GitSha::parse("1".repeat(40)).unwrap(),
        tree: GitSha::parse("2".repeat(40)).unwrap(),
        files,
    };
    assert!(
        apply_changes(
            &s,
            &[FileChange::Add {
                path: "SRC/new.rs".into(),
                content: "new".into()
            }],
            &["SRC/".into()]
        )
        .is_err()
    );
}
