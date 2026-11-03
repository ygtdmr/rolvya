//! Descriptor-relative private operational files. Same-UID/root compromise is outside this boundary.
use super::*;
use rustix::{
    fd::OwnedFd,
    fs::{self, AtFlags, Mode, OFlags, RenameFlags},
};
use std::{
    fs::File,
    io::{Read, Write},
};
use zeroize::Zeroizing;

pub(super) struct PrivateDirectory {
    pub(super) path: PathBuf,
    custody: super::super::custody::Directory,
    fd: OwnedFd,
}
impl PrivateDirectory {
    pub(super) fn persistent(&self) -> Result<bool, OperationError> {
        self.validate()?;
        let kind = fs::fstatfs(&self.fd)
            .map_err(|_| OperationError::UnsafeFilesystem)?
            .f_type as u64;
        Ok(matches!(kind, 0xef53 | 0x58465342 | 0x9123683e))
    }
    pub(super) fn open(path: &Path) -> Result<Self, OperationError> {
        let custody = super::super::custody::Directory::open(
            path,
            super::super::ServiceIdentity::current(),
            &[],
        )
        .map_err(|_| OperationError::UnsafeFilesystem)?;
        let fd = fs::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| OperationError::UnsafeFilesystem)?;
        let named = fs::statat(fs::CWD, path, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| OperationError::UnsafeFilesystem)?;
        let opened = fs::fstat(&fd).map_err(|_| OperationError::UnsafeFilesystem)?;
        if (named.st_dev, named.st_ino) != (opened.st_dev, opened.st_ino) {
            return Err(OperationError::UnsafeFilesystem);
        }
        Ok(Self {
            path: path.to_owned(),
            custody,
            fd,
        })
    }
    pub(super) fn validate(&self) -> Result<(), OperationError> {
        self.custody
            .validate()
            .map_err(|_| OperationError::UnsafeFilesystem)?;
        let fresh = fs::statat(fs::CWD, &self.path, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| OperationError::UnsafeFilesystem)?;
        let held = fs::fstat(&self.fd).map_err(|_| OperationError::UnsafeFilesystem)?;
        if (fresh.st_dev, fresh.st_ino) != (held.st_dev, held.st_ino) {
            return Err(OperationError::UnsafeFilesystem);
        }
        Ok(())
    }
    pub(super) fn identity(&self, name: &str) -> Result<[u8; 16], OperationError> {
        Self::name(name)?;
        self.custody
            .file_identity(std::ffi::OsStr::new(name))
            .map_err(|_| OperationError::UnsafeFilesystem)
    }
    pub(super) fn lock(&self) -> Result<super::super::custody::Lock<'_>, OperationError> {
        self.custody.lock().map_err(|_| OperationError::Conflict)
    }
    pub(super) fn present(&self, name: &str) -> Result<bool, OperationError> {
        Self::name(name)?;
        self.validate()?;
        match fs::statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => {
                super::super::custody::check_key(&stat, super::super::ServiceIdentity::current())
                    .map_err(|_| OperationError::UnsafeFilesystem)?;
                Ok(true)
            }
            Err(rustix::io::Errno::NOENT) => {
                self.custody.sync().map_err(|_| OperationError::Storage)?;
                Ok(false)
            }
            Err(_) => Err(OperationError::UnsafeFilesystem),
        }
    }
    fn name(name: &str) -> Result<(), OperationError> {
        if name.is_empty()
            || name.len() > 128
            || name == "."
            || name == ".."
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
        {
            return Err(OperationError::UnsafeFilesystem);
        }
        Ok(())
    }
    pub(super) fn read(
        &self,
        name: &str,
        limit: usize,
    ) -> Result<Zeroizing<Vec<u8>>, OperationError> {
        Self::name(name)?;
        self.validate()?;
        let fd = fs::openat(
            &self.fd,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|_| OperationError::UnsafeFilesystem)?;
        let before = fs::fstat(&fd).map_err(|_| OperationError::UnsafeFilesystem)?;
        super::super::custody::check_key(&before, super::super::ServiceIdentity::current())
            .map_err(|_| OperationError::UnsafeFilesystem)?;
        if before.st_size < 0 || before.st_size as u64 > limit as u64 {
            return Err(OperationError::ResourceLimit);
        }
        let mut file = File::from(fd);
        let mut bytes = Zeroizing::new(Vec::with_capacity(before.st_size as usize));
        Read::by_ref(&mut file)
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| OperationError::Storage)?;
        let after = fs::fstat(&file).map_err(|_| OperationError::UnsafeFilesystem)?;
        let named = fs::statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| OperationError::UnsafeFilesystem)?;
        if bytes.len() > limit
            || before.st_size != bytes.len() as i64
            || (before.st_dev, before.st_ino) != (named.st_dev, named.st_ino)
            || (
                before.st_mtime,
                before.st_mtime_nsec,
                before.st_ctime,
                before.st_ctime_nsec,
            ) != (
                after.st_mtime,
                after.st_mtime_nsec,
                after.st_ctime,
                after.st_ctime_nsec,
            )
        {
            return Err(OperationError::UnsafeFilesystem);
        }
        self.validate()?;
        Ok(bytes)
    }
    pub(super) fn publish(&self, name: &str, bytes: &[u8]) -> Result<(), OperationError> {
        Self::name(name)?;
        self.validate()?;
        let temporary = format!(".{name}.pending");
        let fd = fs::openat(
            &self.fd,
            temporary.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|_| OperationError::Conflict)?;
        let mut file = File::from(fd);
        file.write_all(bytes).map_err(|_| OperationError::Storage)?;
        file.sync_all().map_err(|_| OperationError::Storage)?;
        self.validate()?;
        let held = fs::fstat(&file).map_err(|_| OperationError::Storage)?;
        let named = fs::statat(&self.fd, temporary.as_str(), AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| OperationError::Storage)?;
        if (held.st_dev, held.st_ino) != (named.st_dev, named.st_ino) {
            return Err(OperationError::UnsafeFilesystem);
        }
        fs::renameat_with(
            &self.fd,
            temporary.as_str(),
            &self.fd,
            name,
            RenameFlags::NOREPLACE,
        )
        .map_err(|_| OperationError::Conflict)?;
        fs::fsync(&self.fd).map_err(|_| OperationError::Storage)?;
        self.validate()
    }
    pub(super) fn remove(&self, name: &str) -> Result<(), OperationError> {
        Self::name(name)?;
        self.read(name, MAX_BACKUP_BYTES + 65536)?;
        fs::unlinkat(&self.fd, name, AtFlags::empty()).map_err(|_| OperationError::Storage)?;
        fs::fsync(&self.fd).map_err(|_| OperationError::Storage)?;
        self.validate()
    }
}
pub(super) const MAX_BACKUP_BYTES: usize = 256 * 1024 * 1024;
pub(crate) fn private_file(
    path: &Path,
    limit: usize,
) -> Result<Zeroizing<Vec<u8>>, OperationError> {
    let parent = PrivateDirectory::open(path.parent().ok_or(OperationError::UnsafeFilesystem)?)?;
    parent.read(
        path.file_name()
            .and_then(|x| x.to_str())
            .ok_or(OperationError::UnsafeFilesystem)?,
        limit,
    )
}

/// Validate database/WAL/SHM metadata without loading their contents.
pub(super) fn database_identity(path: &Path) -> Result<[u8; 16], OperationError> {
    let directory = PrivateDirectory::open(path.parent().ok_or(OperationError::UnsafeFilesystem)?)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(OperationError::UnsafeFilesystem)?;
    let identity = directory.identity(name)?;
    for suffix in ["-wal", "-shm"] {
        let sidecar = format!("{name}{suffix}");
        // A validated sidecar may disappear when the last SQLite connection closes.
        // present already checks the exact no-follow metadata; a second stat adds a race.
        directory.present(&sidecar)?;
    }
    Ok(identity)
}

/// Inspect the protected inode and bound it before any SQLite row allocation.
pub(super) fn bounded_file(path: &Path, limit: usize) -> Result<u64, OperationError> {
    let directory = PrivateDirectory::open(path.parent().ok_or(OperationError::UnsafeFilesystem)?)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or(OperationError::UnsafeFilesystem)?;
    PrivateDirectory::name(name)?;
    let fd = fs::openat(
        &directory.fd,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| OperationError::UnsafeFilesystem)?;
    let metadata = fs::fstat(&fd).map_err(|_| OperationError::UnsafeFilesystem)?;
    super::super::custody::check_key(&metadata, super::super::ServiceIdentity::current())
        .map_err(|_| OperationError::UnsafeFilesystem)?;
    let file = File::from(fd);
    if !directory
        .custody
        .matches_file(std::ffi::OsStr::new(name), &file)
        .map_err(|_| OperationError::UnsafeFilesystem)?
    {
        return Err(OperationError::UnsafeFilesystem);
    }
    if metadata.st_size < 0 || metadata.st_size as u64 > limit as u64 {
        return Err(OperationError::ResourceLimit);
    }
    Ok(metadata.st_size as u64)
}

/// Public static assets may be root-owned/readable, but their entire namespace
/// must resist replacement by an unrelated UID before any browser code is loaded.
pub(super) fn public_directory(path: &Path) -> Result<(), OperationError> {
    use std::path::Component;
    let components = path
        .components()
        .filter_map(|c| {
            if let Component::Normal(n) = c {
                Some(n)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if !path.is_absolute() || components.is_empty() || components.len() > 128 {
        return Err(OperationError::UnsafeFilesystem);
    }
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut fd =
        fs::open("/", flags, Mode::empty()).map_err(|_| OperationError::UnsafeFilesystem)?;
    let root = fs::fstat(&fd).map_err(|_| OperationError::UnsafeFilesystem)?;
    if root.st_uid != 0 || root.st_mode & 0o022 != 0 {
        return Err(OperationError::UnsafeFilesystem);
    }
    for (i, name) in components.iter().enumerate() {
        fd = fs::openat(&fd, *name, flags, Mode::empty())
            .map_err(|_| OperationError::UnsafeFilesystem)?;
        let m = fs::fstat(&fd).map_err(|_| OperationError::UnsafeFilesystem)?;
        let intermediate = i + 1 < components.len();
        if (m.st_uid != 0 && m.st_uid != rustix::process::geteuid().as_raw())
            || (m.st_mode & 0o022 != 0
                && !(intermediate && m.st_uid == 0 && m.st_mode & 0o1000 != 0))
        {
            return Err(OperationError::UnsafeFilesystem);
        }
    }
    let held = fs::fstat(&fd).map_err(|_| OperationError::UnsafeFilesystem)?;
    let named = fs::statat(fs::CWD, path, AtFlags::SYMLINK_NOFOLLOW)
        .map_err(|_| OperationError::UnsafeFilesystem)?;
    if (held.st_dev, held.st_ino) != (named.st_dev, named.st_ino) {
        return Err(OperationError::UnsafeFilesystem);
    }
    super::super::custody::local_filesystem(
        fs::fstatfs(&fd)
            .map_err(|_| OperationError::UnsafeFilesystem)?
            .f_type as u64,
    )
    .map_err(|_| OperationError::UnsafeFilesystem)
}
