use super::super::verifier::{CommandGroup, bounded_output, hash_file};
use super::*;
use serde_json::Value;
use std::{path::Path, process::Stdio, sync::atomic::AtomicUsize};
use tokio::{io::AsyncWriteExt, process::Command};
/// Nonsecret per-attempt home identity. The independently authenticated registry
/// still controls consent, encrypted session reference, capacity and refresh.
/// A directory is not a sandbox and cannot establish hosted-login permission.
pub(super) fn runtime_identity(w: &CodingWork, adapter: Adapter) -> Result<Digest, AgentError> {
    let account = w.model_account.as_ref().ok_or(AgentError::Scope)?;
    if !adapter.matches_binding(account) {
        return Err(AgentError::Scope);
    }
    commitment(&(
        "rolvya/isolated-account-home/v1",
        &w.context.installation,
        &w.binding,
        &w.connection,
        &w.identity,
        account,
        adapter,
    ))
    .map_err(|_| AgentError::Scope)
}
/// A trusted image inspector must pin these bytes, exact version and capabilities.
/// A version string reported by the candidate binary is not independent trust.
pub(super) struct CliExecutable {
    pub path: PathBuf,
    pub sha256: Digest,
    pub version: String,
}
impl CliExecutable {
    async fn process(
        &self,
        args: &[String],
        input: &str,
        root: &Path,
        home: &Path,
        max: usize,
    ) -> Result<(Vec<u8>, Vec<u8>), AgentError> {
        let metadata = std::fs::symlink_metadata(&self.path).map_err(|_| AgentError::Scope)?;
        if !metadata.is_file() || metadata.len() > 128 * 1024 * 1024 {
            return Err(AgentError::Scope);
        }
        if !self.path.is_absolute()
            || hash_file(&self.path).map_err(|_| AgentError::Scope)? != self.sha256
        {
            return Err(AgentError::Scope);
        }
        let mut child = Command::new(&self.path)
            .args(args)
            .current_dir(root)
            .env_clear()
            .env("HOME", home)
            .env("CODEX_HOME", home.join("codex"))
            .env("CLAUDE_CONFIG_DIR", home.join("claude"))
            .env("PATH", "/usr/bin:/bin")
            .env("NO_COLOR", "1")
            .process_group(0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| AgentError::ProcessFailure)?;
        let _group = CommandGroup(
            rustix::process::Pid::from_raw(child.id().ok_or(AgentError::ProcessFailure)? as i32)
                .ok_or(AgentError::ProcessFailure)?,
        );
        let mut stdin = child.stdin.take().ok_or(AgentError::ProcessFailure)?;
        let mut stdout = child.stdout.take().ok_or(AgentError::ProcessFailure)?;
        let mut stderr = child.stderr.take().ok_or(AgentError::ProcessFailure)?;
        let budget = AtomicUsize::new(0);
        let io = async {
            let write = async {
                stdin
                    .write_all(input.as_bytes())
                    .await
                    .map_err(|_| super::super::client::ClientError::Protocol)?;
                drop(stdin);
                Ok(())
            };
            let ((), a, b) = tokio::try_join!(
                write,
                bounded_output(&mut stdout, &budget, max),
                bounded_output(&mut stderr, &budget, max)
            )
            .map_err(|_| AgentError::ResourceLimit)?;
            let status = child.wait().await.map_err(|_| AgentError::ProcessFailure)?;
            if !status.success() {
                return Err(AgentError::ProcessFailure);
            }
            Ok((a, b))
        };
        // Outer executor also enforces attempt deadline and authenticated heartbeat.
        match tokio::time::timeout(Duration::from_secs(60), io).await {
            Ok(v) => v,
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                Err(AgentError::Timeout)
            }
        }
    }
    pub async fn validate(
        &self,
        adapter: Adapter,
        root: &Path,
        home: &Path,
    ) -> Result<(), AgentError> {
        let (version, _) = self
            .process(&["--version".into()], "", root, home, 4096)
            .await?;
        if std::str::from_utf8(&version)
            .map_err(|_| AgentError::InvalidResponse)?
            .trim()
            != self.version
        {
            return Err(AgentError::Scope);
        }
        let help_args = if adapter == Adapter::CodexCli {
            vec!["exec".into(), "--help".into()]
        } else {
            vec!["--help".into()]
        };
        let (help, _) = self.process(&help_args, "", root, home, 65536).await?;
        let help = std::str::from_utf8(&help).map_err(|_| AgentError::InvalidResponse)?;
        let required = if adapter == Adapter::CodexCli {
            vec![
                "--json",
                "--ephemeral",
                "--sandbox",
                "--model",
                "--skip-git-repo-check",
                "--ignore-user-config",
                "--ignore-rules",
            ]
        } else {
            vec![
                "--output-format",
                "--restricted",
                "--tools",
                "--model",
                "--effort",
                "--no-session-persistence",
            ]
        };
        if required.iter().any(|f| !help.contains(f)) {
            return Err(AgentError::Unavailable);
        }
        Ok(())
    }
    pub fn arguments(
        adapter: Adapter,
        model: &str,
        effort: Effort,
    ) -> Result<Vec<String>, AgentError> {
        let level = match effort {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::ExtraHigh => return Err(AgentError::UnsupportedEffort),
        };
        let args = match adapter {
            Adapter::CodexCli => vec![
                "exec".into(),
                "--json".into(),
                "--ephemeral".into(),
                "--ignore-user-config".into(),
                "--ignore-rules".into(),
                "--skip-git-repo-check".into(),
                "--sandbox".into(),
                "workspace-write".into(),
                "--model".into(),
                model.into(),
                "-c".into(),
                format!("model_reasoning_effort=\"{level}\""),
                "-".into(),
            ],
            Adapter::ClaudeCodeCli => vec![
                "--restricted".into(),
                "--bare".into(),
                "-p".into(),
                "--output-format".into(),
                "json".into(),
                "--tools".into(),
                "Read,Edit,Write".into(),
                "--allowedTools".into(),
                "Read,Edit,Write".into(),
                "--no-session-persistence".into(),
                "--model".into(),
                model.into(),
                "--effort".into(),
                level.into(),
            ],
            _ => return Err(AgentError::Scope),
        };
        Ok(args)
    }
    pub async fn run(
        &self,
        adapter: Adapter,
        args: &[String],
        prompt: &str,
        root: &Path,
        home: &Path,
        max: usize,
    ) -> Result<(Digest, Usage), AgentError> {
        let (stdout, _stderr) = self.process(args, prompt, root, home, max).await?;
        let usage = parse(adapter, &stdout)?;
        Ok((digest(&stdout), usage))
    }
}
pub(super) fn parse(adapter: Adapter, raw: &[u8]) -> Result<Usage, AgentError> {
    if adapter == Adapter::ClaudeCodeCli {
        let v: Value = serde_json::from_slice(raw).map_err(|_| AgentError::InvalidResponse)?;
        if v["type"] != "result" || v["subtype"] != "success" || v["is_error"] != false {
            return Err(AgentError::InvalidResponse);
        }
        return api::usage(&v["usage"]);
    }
    if adapter != Adapter::CodexCli {
        return Err(AgentError::Scope);
    }
    let text = std::str::from_utf8(raw).map_err(|_| AgentError::InvalidResponse)?;
    let mut begun = false;
    let mut terminal = None;
    for line in text.lines() {
        if terminal.is_some() {
            return Err(AgentError::InvalidResponse);
        }
        let v: Value = serde_json::from_str(line).map_err(|_| AgentError::InvalidResponse)?;
        match v["type"].as_str() {
            Some("thread.started") if !begun => begun = true,
            Some("turn.started")
            | Some("item.started")
            | Some("item.updated")
            | Some("item.completed")
                if begun => {}
            Some("turn.completed") if begun => {
                terminal = Some(api::usage(&v["usage"])?);
            }
            _ => return Err(AgentError::InvalidResponse),
        }
    }
    terminal.ok_or(AgentError::InvalidResponse)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agent_cli_terminal_and_usage_are_required_without_fallback() {
        let codex=b"{\"type\":\"thread.started\",\"thread_id\":\"fixture\"}\n{\"type\":\"turn.started\"}\n{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":40,\"output_tokens\":5}}\n";
        assert_eq!(
            parse(Adapter::CodexCli, codex).unwrap().input_tokens,
            Some(40)
        );
        for bad in [
            b"".as_slice(),
            b"{\"type\":\"turn.completed\"}",
            b"{\"type\":\"thread.started\"}\n{\"type\":\"turn.failed\"}",
            b"{\"type\":\"thread.started\"}",
        ] {
            assert!(parse(Adapter::CodexCli, bad).is_err())
        }
        let claude=b"{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"usage\":{\"input_tokens\":40,\"output_tokens\":5}}";
        assert_eq!(
            parse(Adapter::ClaudeCodeCli, claude).unwrap().output_tokens,
            Some(5)
        );
        assert!(
            parse(
                Adapter::ClaudeCodeCli,
                b"{\"type\":\"result\",\"subtype\":\"error_max_turns\",\"is_error\":true}"
            )
            .is_err()
        );
        assert!(parse(Adapter::CodexCli, claude).is_err());
        assert!(parse(Adapter::ClaudeCodeCli, codex).is_err());
        for a in [Adapter::CodexCli, Adapter::ClaudeCodeCli] {
            let args = CliExecutable::arguments(a, "assigned-model", Effort::Medium).unwrap();
            assert!(args.contains(&"assigned-model".into()));
            assert!(
                !args
                    .iter()
                    .any(|a| a.contains("bypass") || a.contains("dangerously"))
            );
            assert!(CliExecutable::arguments(a, "assigned-model", Effort::ExtraHigh).is_err());
        }
    }
}

#[cfg(test)]
mod process_regressions {
    use super::*;
    #[tokio::test]
    async fn agent_cli_timeout_drop_terminates_ordinary_descendant() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("workspace");
        let home = d.path().join("home");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&home).unwrap();
        let path = super::super::fixture::executable();
        let exe = CliExecutable {
            sha256: hash_file(&path).unwrap(),
            path,
            version: "rolvya-cli-protocol-fixture 1".into(),
        };
        let answer = tokio::time::timeout(
            Duration::from_millis(500),
            exe.process(&["--fixture-timeout".into()], "", &root, &home, 8192),
        )
        .await;
        assert!(answer.is_err());
        let pid: u32 = std::fs::read_to_string(root.join("child.pid"))
            .unwrap()
            .parse()
            .unwrap();
        // A zombie is dead, even if this test host's PID 1 has not reaped it yet.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"));
        assert!(
            stat.is_err()
                || stat
                    .unwrap()
                    .split(')')
                    .nth(1)
                    .unwrap()
                    .trim_start()
                    .starts_with('Z')
        );
    }
}

#[cfg(test)]
mod executable_regressions {
    use super::*;
    #[tokio::test]
    async fn agent_cli_executable_symlink_is_not_the_pinned_image_file() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        std::fs::create_dir(&home).unwrap();
        let original = super::super::fixture::executable();
        let linked = root.path().join("substituted-cli");
        std::os::unix::fs::symlink(&original, &linked).unwrap();
        let cli = CliExecutable {
            path: linked,
            sha256: hash_file(&original).unwrap(),
            version: "rolvya-cli-protocol-fixture 1".into(),
        };
        assert_eq!(
            cli.process(&["--version".into()], "", root.path(), &home, 4096)
                .await
                .err(),
            Some(AgentError::Scope)
        );
    }
}
