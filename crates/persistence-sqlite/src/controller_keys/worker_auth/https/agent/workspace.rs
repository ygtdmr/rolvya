use super::*;
use crate::controller_keys::github_app::PinnedSource;
use std::{
    collections::BTreeMap,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};
/// Only exact source bytes and mediated edits live here. No executable tool.
pub(super) struct Workspace {
    pub source: PinnedSource,
    files: BTreeMap<String, Vec<u8>>,
    paths: Vec<String>,
    roots: BTreeMap<std::path::PathBuf, (u64, u64)>,
}
impl Workspace {
    pub fn new(source: PinnedSource, paths: Vec<String>) -> Self {
        let files = source
            .files
            .iter()
            .map(|f| (f.path.clone(), f.bytes.clone()))
            .collect();
        Self {
            source,
            files,
            paths,
            roots: Default::default(),
        }
    }
    pub fn read(&self, path: &str) -> Result<String, AgentError> {
        if !self
            .paths
            .iter()
            .any(|a| path == a || a.ends_with('/') && path.starts_with(a))
        {
            return Err(AgentError::ToolDenied);
        }
        let b = self.files.get(path).ok_or(AgentError::ToolDenied)?;
        if b.len() > MAX_FILE_BYTES {
            return Err(AgentError::ResourceLimit);
        }
        String::from_utf8(b.clone()).map_err(|_| AgentError::ToolDenied)
    }
    pub fn write(&mut self, path: &str, content: &str) -> Result<(), AgentError> {
        let mut next = self.files.clone();
        next.insert(path.into(), content.as_bytes().to_vec());
        self.replace(next)
    }
    pub fn delete(&mut self, path: &str) -> Result<(), AgentError> {
        if !self.files.contains_key(path) {
            return Err(AgentError::ToolDenied);
        }
        let mut next = self.files.clone();
        next.remove(path);
        self.replace(next)
    }
    fn replace(&mut self, next: BTreeMap<String, Vec<u8>>) -> Result<(), AgentError> {
        let changes = self.diff(&next)?;
        apply_changes(&self.source, &changes, &self.paths).map_err(|_| AgentError::ToolDenied)?;
        self.files = next;
        Ok(())
    }
    fn diff(&self, next: &BTreeMap<String, Vec<u8>>) -> Result<Vec<FileChange>, AgentError> {
        let old: BTreeMap<_, _> = self
            .source
            .files
            .iter()
            .map(|f| (f.path.as_str(), f))
            .collect();
        let mut changes = Vec::new();
        for (p, b) in next {
            if old.get(p.as_str()).is_some_and(|f| f.bytes == *b) {
                continue;
            }
            let content = String::from_utf8(b.clone()).map_err(|_| AgentError::ToolDenied)?;
            changes.push(if let Some(f) = old.get(p.as_str()) {
                FileChange::Modify {
                    path: p.clone(),
                    original_sha256: digest(&f.bytes),
                    content,
                }
            } else {
                FileChange::Add {
                    path: p.clone(),
                    content,
                }
            });
        }
        for f in &self.source.files {
            if !next.contains_key(&f.path) {
                changes.push(FileChange::Delete {
                    path: f.path.clone(),
                    original_sha256: digest(&f.bytes),
                });
            }
        }
        changes.sort_by(|a, b| a.path().cmp(b.path()));
        if changes.len() > MAX_CHANGED_FILES {
            return Err(AgentError::ResourceLimit);
        }
        Ok(changes)
    }
    pub fn changes(&self) -> Result<Vec<FileChange>, AgentError> {
        self.diff(&self.files)
    }
    pub fn materialize(&mut self, root: &Path) -> Result<(), AgentError> {
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(root)
            .map_err(|_| AgentError::Scope)?;
        let metadata = std::fs::symlink_metadata(root).map_err(|_| AgentError::Scope)?;
        self.roots
            .insert(root.to_path_buf(), (metadata.dev(), metadata.ino()));
        for (p, b) in &self.files {
            let dest = root.join(p);
            let mut parent = root.to_path_buf();
            for c in p.split('/').take(p.split('/').count() - 1) {
                parent.push(c);
                match std::fs::DirBuilder::new().mode(0o700).create(&parent) {
                    Ok(()) => {}
                    Err(e)
                        if e.kind() == std::io::ErrorKind::AlreadyExists
                            && std::fs::symlink_metadata(&parent).is_ok_and(|m| m.is_dir()) => {}
                    Err(_) => return Err(AgentError::Scope),
                }
            }
            let executable = self
                .source
                .files
                .iter()
                .any(|f| f.path == *p && f.mode == "100755");
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(if executable { 0o700 } else { 0o600 })
                .open(dest)
                .map_err(|_| AgentError::Scope)?;
            std::io::Write::write_all(&mut f, b).map_err(|_| AgentError::Scope)?;
        }
        Ok(())
    }
    pub fn collect(&mut self, root: &Path) -> Result<(), AgentError> {
        let m = std::fs::symlink_metadata(root).map_err(|_| AgentError::Scope)?;
        if !m.is_dir() || self.roots.get(root).copied() != Some((m.dev(), m.ino())) {
            return Err(AgentError::Scope);
        }
        fn walk(
            root: &Path,
            dir: &Path,
            files: &mut BTreeMap<String, Vec<u8>>,
            total: &mut usize,
            entries: &mut usize,
            depth: usize,
        ) -> Result<(), AgentError> {
            if depth > 16 {
                return Err(AgentError::ResourceLimit);
            }
            for entry in std::fs::read_dir(dir).map_err(|_| AgentError::Scope)? {
                *entries += 1;
                if *entries > 1024 {
                    return Err(AgentError::ResourceLimit);
                }
                let p = entry.map_err(|_| AgentError::Scope)?.path();
                let m = std::fs::symlink_metadata(&p).map_err(|_| AgentError::Scope)?;
                if m.is_dir() {
                    walk(root, &p, files, total, entries, depth + 1)?;
                    continue;
                }
                if !m.is_file() || m.nlink() != 1 || m.len() > 65536 || files.len() >= 256 {
                    return Err(AgentError::ToolDenied);
                }
                *total = total
                    .checked_add(m.len() as usize)
                    .ok_or(AgentError::ResourceLimit)?;
                if *total > MAX_SOURCE_BYTES {
                    return Err(AgentError::ResourceLimit);
                }
                let path = p
                    .strip_prefix(root)
                    .map_err(|_| AgentError::Scope)?
                    .to_str()
                    .ok_or(AgentError::Scope)?
                    .to_owned();
                let bytes = std::fs::read(&p).map_err(|_| AgentError::Scope)?;
                if bytes.len() != m.len() as usize {
                    return Err(AgentError::Scope);
                }
                files.insert(path, bytes);
            }
            Ok(())
        }
        let mut next = BTreeMap::new();
        walk(root, root, &mut next, &mut 0, &mut 0, 0)?;
        for p in next.keys() {
            let m = std::fs::symlink_metadata(root.join(p)).map_err(|_| AgentError::Scope)?;
            let expected = self
                .source
                .files
                .iter()
                .find(|f| f.path == *p)
                .is_some_and(|f| f.mode == "100755");
            if (m.permissions().mode() & 0o111 != 0) != expected {
                return Err(AgentError::ToolDenied);
            }
        }
        self.replace(next)
    }
}
#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::controller_keys::github_app::SourceFile;
    pub(in super::super) fn fixture() -> Workspace {
        use crate::controller_keys::github_app::{GitSha, RepositoryId};
        let source = PinnedSource {
            repository: RepositoryId::new(99).unwrap(),
            commit: GitSha::parse("a".repeat(40)).unwrap(),
            tree: GitSha::parse("b".repeat(40)).unwrap(),
            files: vec![SourceFile {
                path: "src/a.rs".into(),
                mode: "100644".into(),
                sha: GitSha::parse("c".repeat(40)).unwrap(),
                bytes: b"old\r\n".to_vec(),
            }],
        };
        Workspace::new(source, vec!["src/a.rs".into()])
    }
    #[test]
    fn agent_workspace_preserves_exact_original_and_denies_scope() {
        let mut w = fixture();
        assert_eq!(w.write("../outside", "bad"), Err(AgentError::ToolDenied));
        w.write("src/a.rs", "new\r\n").unwrap();
        assert_eq!(w.source.files[0].bytes, b"old\r\n");
        assert_eq!(w.files["src/a.rs"], b"new\r\n");
    }
}

#[cfg(test)]
mod boundary_regressions {
    use super::*;
    #[test]
    fn agent_workspace_root_symlink_substitution_is_denied() {
        let mut w = tests::fixture();
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("workspace");
        w.materialize(&root).unwrap();
        let other = d.path().join("substituted");
        std::fs::rename(&root, &other).unwrap();
        std::fs::write(other.join("src/a.rs"), b"substituted\n").unwrap();
        std::os::unix::fs::symlink(&other, &root).unwrap();
        assert_eq!(w.collect(&root), Err(AgentError::Scope));
    }
}

#[cfg(test)]
mod filesystem_cases {
    use super::*;
    #[test]
    fn agent_workspace_rejects_special_files_modes_and_unapproved_paths() {
        for mode in ["symlink", "hardlink", "executable", "outside", "oversized"] {
            let mut w = tests::fixture();
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().join("work");
            w.materialize(&root).unwrap();
            match mode {
                "symlink" => {
                    std::fs::remove_file(root.join("src/a.rs")).unwrap();
                    std::os::unix::fs::symlink("/etc/passwd", root.join("src/a.rs")).unwrap();
                }
                "hardlink" => {
                    std::fs::hard_link(root.join("src/a.rs"), dir.path().join("copy")).unwrap();
                }
                "executable" => std::fs::set_permissions(
                    root.join("src/a.rs"),
                    std::fs::Permissions::from_mode(0o700),
                )
                .unwrap(),
                "outside" => std::fs::write(root.join("src/unapproved.rs"), b"unexpected").unwrap(),
                _ => std::fs::write(root.join("src/a.rs"), vec![b'x'; 65537]).unwrap(),
            }
            assert!(w.collect(&root).is_err(), "{mode}");
        }
        for path in [
            "../x",
            "src/../x",
            "src/a.rs/child",
            ".github/workflows/build.yml",
            "src/credentials.txt",
            "SRC/a.rs",
            "src/.env",
        ] {
            assert!(tests::fixture().write(path, "denied").is_err());
        }
    }
}

/// Removes only the newly created attempt directory with the same inode. This
/// is ordinary bounded cleanup, not secure erasure or hostile-host isolation.
pub(super) struct AttemptDirectory {
    path: std::path::PathBuf,
    device: u64,
    inode: u64,
}
impl AttemptDirectory {
    pub fn existing_created(path: &Path) -> Result<Self, AgentError> {
        let m = std::fs::symlink_metadata(path).map_err(|_| AgentError::Scope)?;
        if !m.is_dir() {
            return Err(AgentError::Scope);
        }
        Ok(Self {
            path: path.into(),
            device: m.dev(),
            inode: m.ino(),
        })
    }
    pub fn cleanup(&self) -> Result<(), AgentError> {
        let m = match std::fs::symlink_metadata(&self.path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(AgentError::Scope),
            Ok(m) => m,
        };
        if !m.is_dir() || m.dev() != self.device || m.ino() != self.inode {
            return Err(AgentError::Scope);
        }
        std::fs::remove_dir_all(&self.path).map_err(|_| AgentError::ProcessFailure)
    }
}
impl Drop for AttemptDirectory {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
