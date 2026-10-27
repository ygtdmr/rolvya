//! Linux descriptor-relative custody. No path-based key open or overwriting rename.
use super::{BootstrapError as Error, Boundary, FILE_BYTES, ServiceIdentity};
use rustix::{
    fd::OwnedFd,
    fs::{self, AtFlags, FlockOperation, Mode, OFlags, RenameFlags, Stat},
};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use zeroize::Zeroizing;
pub(super) const KEY: &str = "controller.kek";
const TEMPORARY: &str = ".controller.kek.pending";
const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
pub(super) struct Directory {
    fd: OwnedFd,
    path: PathBuf,
    owner: ServiceIdentity,
    identity: (u64, u64),
}
pub(super) struct Lock<'a>(&'a OwnedFd);
impl Drop for Lock<'_> {
    fn drop(&mut self) {
        let _ = fs::flock(self.0, FlockOperation::Unlock);
    }
}
fn io_error(error: rustix::io::Errno) -> Error {
    match error {
        rustix::io::Errno::LOOP | rustix::io::Errno::NOTDIR => Error::UnsafePath,
        rustix::io::Errno::ACCESS | rustix::io::Errno::PERM => Error::InsecurePermissions,
        _ => Error::StorageFailure,
    }
}
fn normalized(path: &Path) -> bool {
    if !path.is_absolute() || path == Path::new("/") {
        return false;
    }
    // components() normalizes internal dots and repeated slashes; disallow those aliases too.
    let rebuilt: PathBuf = path.components().collect();
    rebuilt.as_os_str() == path.as_os_str()
        && path
            .components()
            .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
}
fn identity(stat: &Stat) -> (u64, u64) {
    (stat.st_dev, stat.st_ino)
}
fn walk(path: &Path, owner: ServiceIdentity) -> Result<OwnedFd, Error> {
    if !normalized(path) {
        return Err(Error::UnsafePath);
    }
    let mut fd = fs::open("/", DIRECTORY_FLAGS, Mode::empty()).map_err(io_error)?;
    let components: Vec<_> = path
        .components()
        .filter_map(|c| {
            if let Component::Normal(c) = c {
                Some(c)
            } else {
                None
            }
        })
        .collect();
    if components.len() > 128 {
        return Err(Error::UnsafePath);
    }
    for (i, component) in components.iter().enumerate() {
        check_repository(&fd)?;
        fd = fs::openat(&fd, *component, DIRECTORY_FLAGS, Mode::empty()).map_err(io_error)?;
        let stat = fs::fstat(&fd).map_err(io_error)?;
        if i + 1 == components.len() {
            if stat.st_uid != owner.0 {
                return Err(Error::OwnershipMismatch);
            }
            if stat.st_mode & 0o7777 != 0o700 {
                return Err(Error::InsecurePermissions);
            }
        } else {
            if stat.st_uid != 0 && stat.st_uid != owner.0 {
                return Err(Error::OwnershipMismatch);
            }
            // Root-owned sticky directories (e.g. /tmp) protect a service-owned
            // child from other users' rename/unlink. All other ancestors must
            // prohibit group/other writes. Same-UID or root compromise is out of scope.
            if stat.st_mode & 0o022 != 0 && !(stat.st_uid == 0 && stat.st_mode & 0o1000 != 0) {
                return Err(Error::InsecurePermissions);
            }
        }
    }
    check_repository(&fd)?;
    local_filesystem(fs::fstatfs(&fd).map_err(io_error)?.f_type as u64)?;
    Ok(fd)
}
// Return the actual OS metadata rather than a synthetic presence value. These
// probes enforce repository exclusion; their result is never key material.
fn stat_optional(fd: &OwnedFd, name: &str) -> Result<Option<Stat>, Error> {
    match fs::statat(fd, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => Ok(Some(stat)),
        Err(rustix::io::Errno::NOENT) => Ok(None),
        Err(e) => Err(io_error(e)),
    }
}
fn check_repository(fd: &OwnedFd) -> Result<(), Error> {
    match fs::statat(fd, ".git", AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => {
            if fs::FileType::from_raw_mode(stat.st_mode) != fs::FileType::Directory {
                return Err(Error::UnsafePath);
            }
            let git = fs::openat(fd, ".git", DIRECTORY_FLAGS, Mode::empty()).map_err(io_error)?;
            // Empty read-only sandbox placeholders are not Git repositories.
            if stat_optional(&git, "HEAD")?.is_some()
                || stat_optional(&git, "config")?.is_some()
                || stat_optional(&git, "objects")?.is_some()
            {
                return Err(Error::UnsafePath);
            }
        }
        Err(rustix::io::Errno::NOENT) => {}
        Err(e) => return Err(io_error(e)),
    }
    if stat_optional(fd, "HEAD")?.is_some() && stat_optional(fd, "objects")?.is_some() {
        return Err(Error::UnsafePath);
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn repository_probe_distinguishes_presence_absence_and_errors() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("present"), b"nonsecret probe").unwrap();
    let fd = fs::open(directory.path(), DIRECTORY_FLAGS, Mode::empty()).unwrap();
    assert!(stat_optional(&fd, "present").unwrap().is_some());
    assert!(stat_optional(&fd, "missing").unwrap().is_none());
    assert_eq!(
        stat_optional(&fd, "present/child").unwrap_err(),
        Error::UnsafePath
    );
}
// Linux statfs magic values (statfs(2)): only explicitly supported local
// filesystems. Remote/FUSE/unknown filesystems cannot establish local custody.
fn local_filesystem(kind: u64) -> Result<(), Error> {
    match kind {
        0xef53 | 0x58465342 | 0x9123683e | 0x794c7630 | 0x01021994 => Ok(()),
        _ => Err(Error::UnsupportedFilesystem),
    }
}
pub(super) fn check_key(stat: &Stat, owner: ServiceIdentity) -> Result<(), Error> {
    if fs::FileType::from_raw_mode(stat.st_mode) != fs::FileType::RegularFile || stat.st_nlink != 1
    {
        return Err(Error::UnsafePath);
    }
    if stat.st_uid != owner.0 {
        return Err(Error::OwnershipMismatch);
    }
    if stat.st_mode & 0o7777 != 0o600 {
        return Err(Error::InsecurePermissions);
    }
    Ok(())
}
impl Directory {
    pub(super) fn open(
        path: &Path,
        owner: ServiceIdentity,
        excluded: &[PathBuf],
    ) -> Result<Self, Error> {
        if owner.0 == 0 || owner.0 != rustix::process::geteuid().as_raw() {
            return Err(Error::OwnershipMismatch);
        }
        if !normalized(path) {
            return Err(Error::UnsafePath);
        }
        // Canonicalize exclusion roots only, never the key path: aliases of web,
        // backups or workers cannot bypass the explicit deployment exclusions.
        for root in excluded {
            let root = std::fs::canonicalize(root).map_err(|_| Error::UnsafePath)?;
            if path.starts_with(&root) || root.starts_with(path) {
                return Err(Error::UnsafePath);
            }
        }
        let fd = walk(path, owner)?;
        let stat = fs::fstat(&fd).map_err(io_error)?;
        Ok(Self {
            fd,
            path: path.to_path_buf(),
            owner,
            identity: identity(&stat),
        })
    }
    pub(super) fn file_identity(&self, name: &std::ffi::OsStr) -> Result<[u8; 16], Error> {
        self.validate()?;
        let stat = fs::statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW).map_err(io_error)?;
        check_key(&stat, self.owner)?;
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&stat.st_dev.to_be_bytes());
        bytes[8..].copy_from_slice(&stat.st_ino.to_be_bytes());
        Ok(bytes)
    }
    pub(super) fn matches_file(&self, name: &std::ffi::OsStr, file: &File) -> Result<bool, Error> {
        self.validate()?;
        let original = fs::fstat(file).map_err(io_error)?;
        let named = fs::statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW).map_err(io_error)?;
        check_key(&original, self.owner)?;
        check_key(&named, self.owner)?;
        Ok(identity(&original) == identity(&named))
    }
    pub(super) fn sync(&self) -> Result<(), Error> {
        self.validate()?;
        fs::fsync(&self.fd).map_err(io_error)
    }
    pub(super) fn reopen(&self) -> Result<Self, Error> {
        self.validate()?;
        let fd = walk(&self.path, self.owner)?;
        if identity(&fs::fstat(&fd).map_err(io_error)?) != self.identity {
            return Err(Error::UnsafePath);
        }
        Ok(Self {
            fd,
            path: self.path.clone(),
            owner: self.owner,
            identity: self.identity,
        })
    }
    pub(super) fn validate(&self) -> Result<(), Error> {
        let fd = walk(&self.path, self.owner)?;
        if identity(&fs::fstat(&fd).map_err(io_error)?) != self.identity {
            return Err(Error::UnsafePath);
        }
        Ok(())
    }
    pub(super) fn lock(&self) -> Result<Lock<'_>, Error> {
        fs::flock(&self.fd, FlockOperation::NonBlockingLockExclusive).map_err(|e| {
            if e == rustix::io::Errno::WOULDBLOCK {
                Error::InitializationInProgress
            } else {
                io_error(e)
            }
        })?;
        self.validate()?;
        Ok(Lock(&self.fd))
    }
    pub(super) fn has_material(&self) -> Result<bool, Error> {
        self.validate()?;
        for name in [KEY, TEMPORARY] {
            match fs::statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW) {
                Ok(_) => return Ok(true),
                Err(rustix::io::Errno::NOENT) => {}
                Err(e) => return Err(io_error(e)),
            }
        }
        Ok(false)
    }
    pub(super) fn read(
        &self,
        boundary: impl Fn(Boundary) -> Result<(), Error>,
    ) -> Result<Zeroizing<[u8; FILE_BYTES]>, Error> {
        self.validate()?;
        let fd = fs::openat(
            &self.fd,
            KEY,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| {
            if e == rustix::io::Errno::NOENT {
                Error::MissingKey
            } else {
                io_error(e)
            }
        })?;
        boundary(Boundary::ReadOpened)?;
        let before = fs::fstat(&fd).map_err(io_error)?;
        local_filesystem(fs::fstatfs(&fd).map_err(io_error)?.f_type as u64)?;
        check_key(&before, self.owner)?;
        if before.st_size != FILE_BYTES as i64 {
            return Err(Error::InvalidKeyMaterial);
        }
        let mut file = File::from(fd);
        let mut bytes = Zeroizing::new([0u8; FILE_BYTES]);
        file.read_exact(bytes.as_mut())
            .map_err(|_| Error::InvalidKeyMaterial)?;
        let mut excess = Zeroizing::new([0u8; 1]);
        if file
            .read(excess.as_mut())
            .map_err(|_| Error::StorageFailure)?
            != 0
        {
            return Err(Error::InvalidKeyMaterial);
        }
        boundary(Boundary::ReadComplete)?;
        let after = fs::fstat(&file).map_err(io_error)?;
        let named = fs::statat(&self.fd, KEY, AtFlags::SYMLINK_NOFOLLOW).map_err(io_error)?;
        check_key(&after, self.owner)?;
        if identity(&before) != identity(&named)
            || identity(&before) != identity(&after)
            || before.st_size != after.st_size
            || before.st_mtime != after.st_mtime
            || before.st_mtime_nsec != after.st_mtime_nsec
            || before.st_ctime != after.st_ctime
            || before.st_ctime_nsec != after.st_ctime_nsec
        {
            return Err(Error::UnsafePath);
        }
        self.validate()?;
        Ok(bytes)
    }
    pub(super) fn publish(
        &self,
        bytes: &[u8; FILE_BYTES],
        boundary: impl Fn(Boundary) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.validate()?;
        let fd = fs::openat(
            &self.fd,
            TEMPORARY,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(io_error)?;
        check_key(&fs::fstat(&fd).map_err(io_error)?, self.owner)?;
        let mut file = File::from(fd);
        boundary(Boundary::TemporaryCreated)?;
        file.write_all(bytes).map_err(|_| Error::StorageFailure)?;
        boundary(Boundary::ContentsWritten)?;
        file.sync_all().map_err(|_| Error::StorageFailure)?;
        boundary(Boundary::FileSynced)?;
        self.validate()?;
        let stat = fs::fstat(&file).map_err(io_error)?;
        check_key(&stat, self.owner)?;
        let named = fs::statat(&self.fd, TEMPORARY, AtFlags::SYMLINK_NOFOLLOW).map_err(io_error)?;
        if identity(&stat) != identity(&named) || stat.st_size != FILE_BYTES as i64 {
            return Err(Error::UnsafePath);
        }
        boundary(Boundary::BeforePublication)?;
        fs::renameat_with(&self.fd, TEMPORARY, &self.fd, KEY, RenameFlags::NOREPLACE)
            .map_err(io_error)?;
        fs::fsync(&self.fd).map_err(io_error)?;
        let named = fs::statat(&self.fd, KEY, AtFlags::SYMLINK_NOFOLLOW).map_err(io_error)?;
        if identity(&stat) != identity(&named) {
            return Err(Error::UnsafePath);
        }
        check_key(&named, self.owner)?;
        self.validate()?;
        Ok(())
    }
}

#[cfg(test)]
#[test]
fn remote_and_unknown_filesystems_are_not_local_key_custody() {
    for kind in [0x6969, 0xff534d42, 0x65735546, 0] {
        assert_eq!(
            local_filesystem(kind).unwrap_err(),
            Error::UnsupportedFilesystem
        );
    }
}
