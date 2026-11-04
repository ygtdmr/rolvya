//! Bounded, descriptor-relative Linux distribution inspection. Never follows links.
use rustix::fs::{Dir, Mode, OFlags, openat};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Component, Path},
};

pub(crate) type Result<T> = std::result::Result<T, String>;
pub(crate) const LIMIT: u64 = 256 * 1024 * 1024;
const FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::CLOEXEC)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::NONBLOCK);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entry {
    pub path: String,
    pub bytes: u64,
    pub mode: u32,
    pub sha256: String,
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn canonical(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 240
        && !path.starts_with('/')
        && path.split('/').all(|p| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
}

pub(crate) struct Tree {
    root: File,
}
impl Tree {
    pub fn built_controller_hash(&self) -> Result<String> {
        // Cargo publishes one executable through two hardlinks in target/release.
        // This exception applies only to hashing that fixed fresh build output;
        // packaged payload inspection still requires exactly one link.
        Ok(digest(&self.read_inner("rolvya-controller", LIMIT, 2)?))
    }
    pub fn open(path: &Path) -> Result<Self> {
        if !path.is_absolute() {
            return Err("absolute directory required".into());
        }
        let mut root = File::from(
            rustix::fs::open("/", FLAGS | OFlags::DIRECTORY, Mode::empty())
                .map_err(|_| "directory unavailable")?,
        );
        for part in path.components() {
            match part {
                Component::RootDir => {}
                Component::Normal(name) => {
                    root = File::from(
                        openat(&root, name, FLAGS | OFlags::DIRECTORY, Mode::empty())
                            .map_err(|_| "unsafe directory")?,
                    );
                }
                _ => return Err("normalized directory required".into()),
            }
        }
        Ok(Self { root })
    }

    fn file(&self, path: &str) -> Result<File> {
        if !canonical(path) {
            return Err("invalid relative path".into());
        }
        let mut dir = self.root.try_clone().map_err(|_| "directory unavailable")?;
        let mut parts = path.split('/').peekable();
        while let Some(part) = parts.next() {
            let flags = if parts.peek().is_some() {
                FLAGS | OFlags::DIRECTORY
            } else {
                FLAGS
            };
            let file =
                File::from(openat(&dir, part, flags, Mode::empty()).map_err(|_| "unsafe file")?);
            if parts.peek().is_none() {
                return Ok(file);
            }
            dir = file;
        }
        Err("invalid path".into())
    }

    pub fn read(&self, path: &str, limit: u64) -> Result<Vec<u8>> {
        self.read_inner(path, limit, 1)
    }
    fn read_inner(&self, path: &str, limit: u64, allowed_links: u64) -> Result<Vec<u8>> {
        let file = self.file(path)?;
        let before = file.metadata().map_err(|_| "file metadata unavailable")?;
        if !before.is_file()
            || !(1..=allowed_links).contains(&before.nlink())
            || before.len() > limit
        {
            return Err("unsafe or oversized file".into());
        }
        let mut bytes = Vec::new();
        (&file)
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "file read failed")?;
        let after = file.metadata().map_err(|_| "file metadata unavailable")?;
        if bytes.len() as u64 != before.len()
            || before.len() != after.len()
            || before.mtime() != after.mtime()
            || before.mtime_nsec() != after.mtime_nsec()
            || before.ctime() != after.ctime()
            || before.ctime_nsec() != after.ctime_nsec()
            || after.nlink() != before.nlink()
        {
            return Err("file changed during inspection".into());
        }
        Ok(bytes)
    }

    pub fn inventory(&self) -> Result<Vec<Entry>> {
        let mut paths = BTreeMap::new();
        enumerate(&self.root, "", 0, &mut paths)?;
        let mut total = 0_u64;
        paths
            .into_iter()
            .map(|(path, ())| {
                let file = self.file(&path)?;
                let meta = file.metadata().map_err(|_| "file metadata unavailable")?;
                if !meta.is_file() || meta.nlink() != 1 || meta.mode() & 0o7022 != 0 {
                    return Err("unsafe file type, links or permissions".into());
                }
                total = total.checked_add(meta.len()).ok_or("size overflow")?;
                if total > LIMIT {
                    return Err("distribution too large".into());
                }
                // Hash incrementally. Content inspection uses separately bounded read().
                let mut hash = Sha256::new();
                let mut reader = (&file).take(LIMIT + 1);
                let mut buf = [0_u8; 16384];
                let mut count = 0_u64;
                loop {
                    let n = reader.read(&mut buf).map_err(|_| "file read failed")?;
                    if n == 0 {
                        break;
                    }
                    count += n as u64;
                    hash.update(&buf[..n]);
                }
                let after = file.metadata().map_err(|_| "file metadata unavailable")?;
                if count != meta.len()
                    || after.len() != meta.len()
                    || after.nlink() != 1
                    || after.ctime() != meta.ctime()
                    || after.ctime_nsec() != meta.ctime_nsec()
                    || after.mtime() != meta.mtime()
                    || after.mtime_nsec() != meta.mtime_nsec()
                {
                    return Err("file changed during inspection".into());
                }
                Ok(Entry {
                    path,
                    bytes: count,
                    mode: meta.mode() & 0o7777,
                    sha256: format!("{:x}", hash.finalize()),
                })
            })
            .collect()
    }
}

fn enumerate(
    dir: &File,
    prefix: &str,
    depth: usize,
    paths: &mut BTreeMap<String, ()>,
) -> Result<()> {
    if depth > 8 {
        return Err("directory depth exceeded".into());
    }
    for entry in Dir::read_from(dir).map_err(|_| "directory unavailable")? {
        let entry = entry.map_err(|_| "directory read failed")?;
        let name = entry.file_name().to_str().map_err(|_| "non-UTF8 path")?;
        if name == "." || name == ".." {
            continue;
        }
        let path = format!("{prefix}{name}");
        if !canonical(&path) {
            return Err("invalid distribution path".into());
        }
        let fd = File::from(
            openat(dir, name, FLAGS, Mode::empty()).map_err(|_| "unsafe distribution entry")?,
        );
        let meta = fd.metadata().map_err(|_| "file metadata unavailable")?;
        if meta.is_dir() {
            if meta.mode() & 0o7777 != 0o700 {
                return Err("unsafe distribution directory permissions".into());
            }
            if !matches!(path.as_str(), "bin" | "web" | "config" | "systemd") {
                return Err("unexpected directory".into());
            }
            enumerate(&fd, &format!("{path}/"), depth + 1, paths)?;
        } else {
            if !meta.is_file() || meta.nlink() != 1 {
                return Err("special files and links forbidden".into());
            }
            if paths.len() >= 64 || paths.insert(path, ()).is_some() {
                return Err("file count exceeded".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
    };

    #[test]
    fn paths_reject_traversal_encoding_and_ambiguity() {
        for path in [
            "", "/a", "../a", "a/../b", "a//b", "a/./b", "a%2fb", "a\\b", "a\nb", "é",
        ] {
            assert!(!canonical(path));
        }
        assert!(canonical("web/app.a_B-1.js"));
    }
    #[test]
    fn cargo_hardlinked_build_output_is_distinct_from_distribution_payload() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("rolvya-controller");
        fs::write(&f, b"fresh build").unwrap();
        fs::hard_link(&f, d.path().join("compiler-artifact")).unwrap();
        let tree = Tree::open(d.path()).unwrap();
        assert_eq!(
            tree.built_controller_hash().unwrap(),
            digest(b"fresh build")
        );
        assert!(tree.read("rolvya-controller", LIMIT).is_err());
        assert!(tree.inventory().is_err());
    }

    #[test]
    fn inventory_is_sorted_bounded_and_content_committed() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a"), "abc").unwrap();
        fs::set_permissions(dir.path().join("a"), fs::Permissions::from_mode(0o400)).unwrap();
        let tree = Tree::open(dir.path()).unwrap();
        let entries = tree.inventory().unwrap();
        assert_eq!(entries[0].sha256, digest(b"abc"));
        assert!(tree.read("a", 2).is_err());
        assert_eq!(tree.read("a", 3).unwrap(), b"abc");
    }

    #[test]
    fn symlink_hardlink_fifo_and_parent_substitution_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret"), "not-a-package").unwrap();
        symlink(outside.path().join("secret"), dir.path().join("a")).unwrap();
        assert!(Tree::open(dir.path()).unwrap().inventory().is_err());
        assert!(Tree::open(dir.path()).unwrap().read("a", 100).is_err());
        fs::remove_file(dir.path().join("a")).unwrap();
        fs::hard_link(outside.path().join("secret"), dir.path().join("a")).unwrap();
        assert!(Tree::open(dir.path()).unwrap().inventory().is_err());
        fs::remove_file(dir.path().join("a")).unwrap();
        rustix::fs::mknodat(
            rustix::fs::CWD,
            dir.path().join("a"),
            rustix::fs::FileType::Fifo,
            Mode::RUSR,
            0,
        )
        .unwrap();
        assert!(Tree::open(dir.path()).unwrap().inventory().is_err());
        symlink(outside.path(), dir.path().join("alias")).unwrap();
        assert!(Tree::open(&dir.path().join("alias")).is_err());
    }
}
