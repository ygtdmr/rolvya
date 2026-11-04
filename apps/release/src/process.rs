use crate::files::Result;
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Only fixed reviewed commands call this private helper. Captured output never reaches diagnostics.
pub(crate) fn output(root: &Path, program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| "required build tool unavailable")?;
    if !out.status.success() {
        return Err(format!("{program} failed (output withheld)"));
    }
    if out.stdout.len() > 16 * 1024 * 1024 {
        return Err("tool output too large".into());
    }
    String::from_utf8(out.stdout).map_err(|_| "invalid build tool output".into())
}

pub(crate) fn clean(root: &Path) -> Result<String> {
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        if unsafe_build_variable(&name)
            || [
                "RUSTFLAGS",
                "CARGO_ENCODED_RUSTFLAGS",
                "RUSTC",
                "RUSTC_WRAPPER",
                "RUSTC_WORKSPACE_WRAPPER",
                "NODE_OPTIONS",
                "NODE_PATH",
                "LD_PRELOAD",
                "LD_LIBRARY_PATH",
                "BASH_ENV",
                "ENV",
            ]
            .contains(&name.as_ref())
            || name.starts_with("CARGO_PROFILE_")
            || name.starts_with("CARGO_TARGET_") && name.ends_with("_RUSTFLAGS")
        {
            return Err("unreviewed build environment override".into());
        }
    }
    if !output(
        root,
        "git",
        &["status", "--porcelain", "--untracked-files=normal"],
    )?
    .is_empty()
    {
        return Err("reviewed clean source required".into());
    }
    let commit = output(root, "git", &["rev-parse", "HEAD"])?;
    let commit = commit.trim();
    if commit.len() != 40
        || !commit
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err("invalid source revision".into());
    }
    Ok(commit.to_owned())
}

pub(crate) fn unsafe_build_variable(name: &str) -> bool {
    if name == "CARGO_TARGET_DIR" {
        return false;
    }
    name.starts_with("LIBSQLITE3_")
        || name.starts_with("SQLITE3_")
        || [
            "CC",
            "CXX",
            "CFLAGS",
            "CXXFLAGS",
            "CPPFLAGS",
            "AR",
            "LDFLAGS",
            "RANLIB",
            "PKG_CONFIG",
            "PKG_CONFIG_PATH",
            "PKG_CONFIG_LIBDIR",
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_BUILD_RUSTC",
            "CARGO_BUILD_RUSTC_WRAPPER",
            "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
            "CARGO_BUILD_TARGET",
            "CARGO_BUILD_TARGET_DIR",
        ]
        .contains(&name)
        || [
            "CC_",
            "CXX_",
            "CFLAGS_",
            "CXXFLAGS_",
            "AR_",
            "LDFLAGS_",
            "CARGO_TARGET_",
        ]
        .iter()
        .any(|p| name.starts_with(p))
}

pub(crate) fn expected_exit(code: Option<i32>, success: bool) -> bool {
    code == Some(if success { 0 } else { 1 })
}

/// Run an expected-denial installed command with bounded time and no output retention.
pub(crate) fn denied(binary: &Path, args: &[&str]) -> Result<u128> {
    let start = Instant::now();
    let mut child = Command::new(binary)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "installed command failed to start")?;
    loop {
        match child
            .try_wait()
            .map_err(|_| "installed command inspection failed")?
        {
            Some(status) if expected_exit(status.code(), false) => {
                return Ok(start.elapsed().as_millis());
            }
            Some(_) => return Err("installed command unexpectedly succeeded".into()),
            None if start.elapsed() > Duration::from_secs(10) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("installed command timed out".into());
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crashed_process_is_not_a_demonstrated_security_denial() {
        assert!(denied(Path::new("/bin/sh"), &["-c", "kill -KILL $$"]).is_err());
        assert!(denied(Path::new("/bin/sh"), &["-c", "exit 0"]).is_err());
        assert!(denied(Path::new("/bin/sh"), &["-c", "exit 1"]).is_ok());
        assert!(denied(Path::new("/bin/sh"), &["-c", "exit 127"]).is_err());
    }
    #[test]
    fn native_and_cargo_configuration_aliases_cannot_change_the_claimed_build() {
        for name in [
            "LIBSQLITE3_SYS_USE_PKG_CONFIG",
            "LIBSQLITE3_FLAGS",
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_BUILD_RUSTC_WRAPPER",
            "CC",
            "CFLAGS_x86_64_unknown_linux_gnu",
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER",
        ] {
            assert!(unsafe_build_variable(name), "{name}");
        }
        assert!(!unsafe_build_variable("CARGO_TARGET_DIR"));
    }
}
