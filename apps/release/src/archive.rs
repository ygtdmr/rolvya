//! Narrow USTAR verifier. Never extracts archive paths or executes archive contents.
use crate::{
    files::{Result, Tree, digest},
    manifest,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    process::{Command, Stdio},
};

pub(crate) fn create(source: &Path, dist: &Path, sha: &str, destination: &Path) -> Result<String> {
    let m = manifest::verify(dist, sha)?;
    if crate::process::clean(source)? != m.commit {
        return Err("archive source mismatch".into());
    }
    let epoch = crate::process::output(source, "git", &["show", "-s", "--format=%ct", "HEAD"])?;
    if !epoch.trim().bytes().all(|b| b.is_ascii_digit()) {
        return Err("invalid commit timestamp".into());
    }
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o400)
        .open(destination)
        .map_err(|_| "archive publication conflict")?;
    let mut tar = Command::new("tar")
        .args([
            "--format=ustar",
            "--sort=name",
            "--owner=0",
            "--group=0",
            "--numeric-owner",
        ])
        .arg(format!("--mtime=@{}", epoch.trim()))
        .args(["-cf", "-", "-C"])
        .arg(dist)
        .arg(".")
        .env("TZ", "UTC")
        .env("LC_ALL", "C")
        .env_remove("TAR_OPTIONS")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "archive tool unavailable")?;
    let zip = Command::new("gzip")
        .args(["-n", "-9"])
        .env_remove("GZIP")
        .stdin(tar.stdout.take().ok_or("archive pipe unavailable")?)
        .stdout(file.try_clone().map_err(|_| "archive unavailable")?)
        .stderr(Stdio::null())
        .status();
    let tar_status = tar.wait().map_err(|_| "archive tool failed")?;
    if !zip.map_err(|_| "gzip unavailable")?.success() || !tar_status.success() {
        return Err("archive creation failed; incomplete output retained".into());
    }
    file.sync_all().map_err(|_| "archive sync failed")?;
    verify(dist, sha, destination)?;
    let tree = Tree::open(destination.parent().ok_or("archive parent required")?)?;
    let name = destination
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("invalid archive name")?;
    Ok(digest(&tree.read(name, crate::files::LIMIT)?))
}

fn number(bytes: &[u8]) -> Result<u64> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "invalid archive number")?
        .trim_matches(['\0', ' ']);
    if text.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(text, 8).map_err(|_| "invalid archive number".into())
}
fn text(bytes: &[u8]) -> Result<&str> {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    if bytes[end..].iter().any(|b| *b != 0) {
        return Err("ambiguous archive field".into());
    }
    std::str::from_utf8(&bytes[..end]).map_err(|_| "invalid archive text".into())
}
fn inspect(reader: &mut impl Read, expected: &[crate::files::Entry]) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut total = 0_u64;
    let mut header = [0_u8; 512];
    let mut count = 0;
    loop {
        reader
            .read_exact(&mut header)
            .map_err(|_| "truncated archive")?;
        if header == [0; 512] {
            let mut trailer = Vec::new();
            reader
                .take(10241)
                .read_to_end(&mut trailer)
                .map_err(|_| "invalid archive trailer")?;
            if trailer.len() < 512 || trailer.len() > 10240 || trailer.iter().any(|b| *b != 0) {
                return Err("invalid archive terminator".into());
            }
            break;
        }
        count += 1;
        if count > 32 {
            return Err("archive member limit".into());
        }
        let sum: u64 = header
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    *b as u64
                }
            })
            .sum();
        if number(&header[148..156])? != sum
            || &header[257..263] != b"ustar\0"
            || &header[263..265] != b"00"
            || header[345..500].iter().any(|b| *b != 0)
            || header[157..257].iter().any(|b| *b != 0)
            || number(&header[108..116])? != 0
            || number(&header[116..124])? != 0
        {
            return Err("unsupported archive header".into());
        }
        let name = text(&header[..100])?;
        let bytes = number(&header[124..136])?;
        let mode = number(&header[100..108])?;
        if !seen.insert(name.to_owned()) {
            return Err("duplicate archive member".into());
        }
        if header[156] == b'5' {
            if !["./", "./bin/", "./web/", "./config/", "./systemd/"].contains(&name)
                || bytes != 0
                || mode != 0o700
            {
                return Err("unexpected archive directory".into());
            }
            continue;
        }
        if ![0, b'0'].contains(&header[156]) {
            return Err("archive links and special members forbidden".into());
        }
        let path = name
            .strip_prefix("./")
            .ok_or("non-canonical archive path")?;
        let file = expected
            .iter()
            .find(|e| e.path == path)
            .ok_or("unexpected archive file")?;
        total = total.checked_add(bytes).ok_or("archive size overflow")?;
        if total > crate::files::LIMIT || bytes != file.bytes || mode != u64::from(file.mode) {
            return Err("archive metadata mismatch".into());
        }
        let mut hash = Sha256::new();
        let mut remaining = bytes;
        let mut buffer = [0_u8; 16384];
        while remaining > 0 {
            let n = remaining.min(buffer.len() as u64) as usize;
            reader
                .read_exact(&mut buffer[..n])
                .map_err(|_| "truncated archive file")?;
            hash.update(&buffer[..n]);
            remaining -= n as u64;
        }
        if format!("{:x}", hash.finalize()) != file.sha256 {
            return Err("archive content mismatch".into());
        }
        let pad = ((512 - bytes % 512) % 512) as usize;
        reader
            .read_exact(&mut buffer[..pad])
            .map_err(|_| "truncated archive padding")?;
        if buffer[..pad].iter().any(|b| *b != 0) {
            return Err("invalid archive padding".into());
        }
    }
    if seen.len() != expected.len() + 5
        || expected
            .iter()
            .any(|f| !seen.contains(&format!("./{}", f.path)))
    {
        return Err("incomplete archive".into());
    }
    Ok(())
}

pub(crate) fn verify(dist: &Path, sha: &str, archive: &Path) -> Result<()> {
    manifest::verify(dist, sha)?;
    let meta = fs::symlink_metadata(archive).map_err(|_| "archive unavailable")?;
    if !meta.is_file() || meta.nlink() != 1 || meta.len() > crate::files::LIMIT {
        return Err("unsafe archive".into());
    }
    let directory = Tree::open(archive.parent().ok_or("archive directory required")?)?;
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("invalid archive name")?;
    let compressed = directory.read(name, crate::files::LIMIT)?;
    let mut child = Command::new("gzip")
        .arg("-dc")
        .env_remove("GZIP")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "gzip unavailable")?;
    // Concurrent input avoids filling the gzip pipe while its output is unread.
    let mut input = child.stdin.take().ok_or("archive pipe unavailable")?;
    let writer = std::thread::spawn(move || input.write_all(&compressed));
    let result = inspect(
        &mut child.stdout.take().ok_or("archive pipe unavailable")?,
        &Tree::open(dist)?.inventory()?,
    );
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|_| "gzip failed")?;
    let written = writer.join().map_err(|_| "archive input failed")?;
    result?;
    if !status.success() || written.is_err() {
        return Err("compressed archive integrity failed".into());
    }
    manifest::verify(dist, sha)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_truncated_and_extra_archives_never_verify() {
        for bytes in [vec![], vec![0; 512], vec![0; 1536], vec![b'x'; 1024]] {
            assert!(inspect(&mut bytes.as_slice(), &[]).is_err());
        }
    }

    #[test]
    fn actual_gnu_archive_roundtrip_preserves_exact_inventory_and_detects_corruption() {
        let d = tempfile::tempdir().unwrap();
        let sha = crate::manifest::tests::fixture(d.path());
        let outside = tempfile::tempdir().unwrap();
        let archive = outside.path().join("candidate.tar.gz");
        let status = Command::new("tar")
            .args([
                "--format=ustar",
                "--sort=name",
                "--owner=0",
                "--group=0",
                "--numeric-owner",
                "--mtime=@0",
                "-czf",
            ])
            .arg(&archive)
            .arg("-C")
            .arg(d.path())
            .arg(".")
            .env_remove("TAR_OPTIONS")
            .status()
            .unwrap();
        assert!(status.success());
        verify(d.path(), &sha, &archive).unwrap();
        let mut bytes = fs::read(&archive).unwrap();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0x55;
        fs::write(&archive, bytes).unwrap();
        assert!(verify(d.path(), &sha, &archive).is_err());
    }
}
