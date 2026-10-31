//! Separate verifier client. Its signing key is not a coding-worker credential.
//! Key possession alone never enrolls an execution or establishes remote isolation.
use super::{
    client::{ClientError, ControllerEndpoint, bound::Channel},
    tls,
};
use crate::{
    RequestId,
    orchestration::coding::{execution::*, work::*, *},
};
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::sync::Arc;
use zeroize::Zeroizing;

pub struct VerifierHttpClient {
    endpoint: ControllerEndpoint,
    tls: Arc<rustls::ClientConfig>,
    key: Ed25519KeyPair,
    execution: String,
}
impl std::fmt::Debug for VerifierHttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VerifierHttpClient [redacted]")
    }
}
/// Integrity-checked inputs, not authority to execute arbitrary source on this host.
pub struct VerificationInputs {
    pub grant: VerifierExecution,
    pub work: CodingWork,
    pub source: crate::controller_keys::github_app::PinnedSource,
    pub artifact: Vec<u8>,
    pub candidate: CodingResult,
    pub files: Vec<crate::controller_keys::github_app::SourceFile>,
}
impl VerifierHttpClient {
    pub fn new(
        endpoint: ControllerEndpoint,
        execution: String,
        pkcs8: Zeroizing<Vec<u8>>,
    ) -> Result<Self, ClientError> {
        Self::with_tls(
            endpoint,
            execution,
            pkcs8,
            tls::client().map_err(|_| ClientError::TlsOrNetwork)?,
        )
    }
    pub(in crate::controller_keys) fn with_tls(
        endpoint: ControllerEndpoint,
        execution: String,
        pkcs8: Zeroizing<Vec<u8>>,
        tls: Arc<rustls::ClientConfig>,
    ) -> Result<Self, ClientError> {
        if execution.is_empty() || execution.len() > 120 {
            return Err(ClientError::Protocol);
        }
        let key = Ed25519KeyPair::from_pkcs8(&pkcs8).map_err(|_| ClientError::Protocol)?;
        Ok(Self {
            endpoint,
            tls,
            key,
            execution,
        })
    }
    async fn request(
        &self,
        id: RequestId,
        op: VerifierOperation,
    ) -> Result<VerifierReply, ClientError> {
        let mut channel = Channel::connect(&self.endpoint, self.tls.clone()).await?;
        let mut req = VerifierRequest {
            version: 1,
            execution: self.execution.clone(),
            request_id: id,
            operation: op,
            signature: vec![],
        };
        req.sign(&self.key, &channel.exporter)
            .map_err(|_| ClientError::Protocol)?;
        let raw = channel.verifier_exchange(&req).await?;
        serde_json::from_slice(&raw).map_err(|_| ClientError::Protocol)
    }
    fn id(&self, label: &str) -> Result<RequestId, ClientError> {
        let digest = commitment(&(&self.execution, label)).map_err(|_| ClientError::Protocol)?;
        RequestId::new(format!("verifier-{}", &digest.as_str()[..48]))
            .map_err(|_| ClientError::Protocol)
    }
    pub async fn inputs(&self, now: i64) -> Result<VerificationInputs, ClientError> {
        let (g, w) = match self
            .request(self.id("claim")?, VerifierOperation::Claim {})
            .await?
        {
            VerifierReply::Assignment { grant, work } => (*grant, *work),
            _ => return Err(ClientError::Protocol),
        };
        if g.public_key != self.key.public_key().as_ref()
            || g.execution != self.execution
            || g.work != w.identity
            || now < g.issued_at
            || now >= g.expires_at
            || g.binding != w.binding
            || g.installation != w.context.installation
        {
            return Err(ClientError::Protocol);
        }
        w.validate(&g.binding, now)
            .map_err(|_| ClientError::Protocol)?;
        let mut chunks = Vec::new();
        for (i, f) in w.source.iter().enumerate() {
            for o in (0..(f.bytes as usize).max(1)).step_by(SOURCE_CHUNK_BYTES) {
                match self
                    .request(
                        self.id(&format!("source-{i}-{o}"))?,
                        VerifierOperation::Source {
                            work: w.identity.clone(),
                            file: i as u16,
                            offset: o as u32,
                        },
                    )
                    .await?
                {
                    VerifierReply::Source { chunk } => chunks.push(chunk),
                    _ => return Err(ClientError::Protocol),
                }
            }
        }
        let source = w.reconstruct(&chunks).map_err(|_| ClientError::Protocol)?;
        let mut raw = Vec::new();
        loop {
            match self
                .request(
                    self.id(&format!("artifact-{}", raw.len()))?,
                    VerifierOperation::Artifact {
                        offset: raw.len() as u32,
                    },
                )
                .await?
            {
                VerifierReply::Artifact {
                    offset,
                    total,
                    bytes,
                    sha256,
                } => {
                    if offset as usize != raw.len()
                        || total as usize > MAX_CODING_BYTES
                        || bytes.is_empty()
                        || bytes.len() > SOURCE_CHUNK_BYTES
                        || sha256 != g.artifact
                        || raw.len() + bytes.len() > total as usize
                    {
                        return Err(ClientError::Protocol);
                    }
                    raw.extend_from_slice(&bytes);
                    if raw.len() == total as usize {
                        break;
                    }
                }
                _ => return Err(ClientError::Protocol),
            }
        }
        let sha = rolvya_domain::worker::Digest::new(format!(
            "{:x}",
            <sha2::Sha256 as sha2::Digest>::digest(&raw)
        ))
        .map_err(|_| ClientError::Protocol)?;
        if sha != g.artifact {
            return Err(ClientError::Protocol);
        }
        let r: CodingResult = serde_json::from_slice(&raw).map_err(|_| ClientError::Protocol)?;
        if r.binding != w.binding
            || r.repository != w.context.repository
            || r.base_commit != w.context.base_commit
            || r.base_tree != w.context.base_tree
            || r.source_sha256 != w.context.source_sha256
            || r.installation != w.context.installation
            || r.inputs != w.inputs
            || r.financial_event != w.financial_event
            || r.operation != w.operation
            || r.verification_execution != g.execution
        {
            return Err(ClientError::Protocol);
        }
        let files = apply_changes(&source, &r.changes, &w.context.paths)
            .map_err(|_| ClientError::Protocol)?;
        if tree_commitment(&files).map_err(|_| ClientError::Protocol)? != r.resulting_tree_sha256
            || r.resulting_tree_sha256 != g.resulting_tree
            || commitment(&w.context.verification).map_err(|_| ClientError::Protocol)? != g.policy
        {
            return Err(ClientError::Protocol);
        }
        Ok(VerificationInputs {
            grant: g,
            work: w,
            source,
            artifact: raw,
            candidate: r,
            files,
        })
    }
    pub async fn submit(
        &self,
        envelope: AuthenticatedReport,
    ) -> Result<rolvya_domain::worker::Digest, ClientError> {
        match self
            .request(
                self.id("report")?,
                VerifierOperation::Report {
                    envelope: Box::new(envelope),
                },
            )
            .await?
        {
            VerifierReply::Recorded { evidence_sha256 } => Ok(evidence_sha256),
            _ => Err(ClientError::Protocol),
        }
    }
}

/// A provider-established remote execution capability. No public success
/// constructor exists until effective Fargate isolation/identity is validated.
/// ```compile_fail
/// use rolvya_persistence_sqlite::controller_keys::worker_auth::https::verifier::RemoteExecution;
/// let bypass=RemoteExecution{directory:"/tmp".into()};
/// ```
pub struct RemoteExecution {
    directory: std::path::PathBuf,
    execution: String,
    image: rolvya_domain::worker::Digest,
    policy: rolvya_domain::worker::Digest,
    started_at: i64,
    grant_sha256: rolvya_domain::worker::Digest,
}
impl RemoteExecution {
    pub fn from_verified_fargate() -> Result<Self, ClientError> {
        Err(ClientError::Rejected(super::wire::ErrorCode::Unavailable))
    }
    #[cfg(test)]
    pub(crate) fn fixture(
        directory: std::path::PathBuf,
        inputs: &VerificationInputs,
        now: i64,
    ) -> Self {
        Self {
            directory,
            execution: inputs.grant.execution.clone(),
            image: inputs.grant.image.clone(),
            policy: inputs.grant.policy.clone(),
            started_at: now,
            grant_sha256: commitment(&inputs.grant).unwrap(),
        }
    }
}
impl VerifierHttpClient {
    /// Runs only in an independently established remote context. Controller
    /// request data cannot construct RemoteExecution. Offline fixtures are not
    /// hostile-code sandbox evidence; CPU/RAM isolation is a production gate.
    pub async fn execute(
        &self,
        inputs: &VerificationInputs,
        host: RemoteExecution,
    ) -> Result<AuthenticatedReport, ClientError> {
        use std::{
            os::unix::fs::{DirBuilderExt, OpenOptionsExt},
            process::Stdio,
            time::{Duration, Instant},
        };
        use tokio::process::Command;
        if host.execution != inputs.grant.execution
            || host.image != inputs.grant.image
            || host.policy != inputs.grant.policy
            || host.started_at < inputs.grant.issued_at
            || host.started_at >= inputs.grant.expires_at
            || self.key.public_key().as_ref() != inputs.grant.public_key
        {
            return Err(ClientError::Protocol);
        }
        if commitment(&inputs.grant).map_err(|_| ClientError::Protocol)? != host.grant_sha256 {
            return Err(ClientError::Protocol);
        }
        inputs.validate(host.started_at)?;
        let policy = &inputs.work.context.verification;
        if policy.commands.is_empty()
            || policy.commands.len() > 8
            || policy.timeout_seconds == 0
            || policy.timeout_seconds > 300
            || policy.max_output_bytes == 0
            || policy.max_output_bytes > 8192
        {
            return Err(ClientError::Protocol);
        }
        // Reject substitution of executable bytes, not merely command names.
        let mut executables = Vec::new();
        for argv in &policy.commands {
            let p = argv.first().ok_or(ClientError::Protocol)?;
            let path = std::path::Path::new(p);
            if !path.is_absolute()
                || path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(ClientError::Protocol);
            }
            let metadata = std::fs::symlink_metadata(path).map_err(|_| ClientError::Protocol)?;
            if !metadata.is_file() || metadata.len() > 128 * 1024 * 1024 {
                return Err(ClientError::Protocol);
            }
            let sha = hash_file(path)?;
            executables.push((p.clone(), sha));
        }
        if commitment(&executables).map_err(|_| ClientError::Protocol)? != host.image {
            return Err(ClientError::Protocol);
        }
        let root = &host.directory;
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(root)
            .map_err(|_| ClientError::Protocol)?;
        // All source paths were reconstructed/validated independently. No code
        // executes until the complete fresh tree is materialized and rehashed.
        for f in &inputs.files {
            let destination = root.join(&f.path);
            let mut parent = root.clone();
            for part in f.path.split('/').take(f.path.split('/').count() - 1) {
                parent.push(part);
                match std::fs::DirBuilder::new().mode(0o700).create(&parent) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                        if !std::fs::symlink_metadata(&parent)
                            .map_err(|_| ClientError::Protocol)?
                            .is_dir()
                        {
                            return Err(ClientError::Protocol);
                        }
                    }
                    Err(_) => return Err(ClientError::Protocol),
                }
            }
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(if f.mode == "100755" { 0o700 } else { 0o600 })
                .open(destination)
                .map_err(|_| ClientError::Protocol)?;
            std::io::Write::write_all(&mut file, &f.bytes).map_err(|_| ClientError::Protocol)?;
        }
        let started = Instant::now();
        let mut logs = String::new();
        let mut tests = std::collections::BTreeSet::new();
        let mut exit = 0;
        for argv in &policy.commands {
            if argv.is_empty() || argv.len() > 16 {
                return Err(ClientError::Protocol);
            }
            let mut child = Command::new(&argv[0])
                .args(&argv[1..])
                .current_dir(root)
                .env_clear()
                .process_group(0)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true)
                .spawn()
                .map_err(|_| ClientError::Protocol)?;
            let _group = CommandGroup(
                rustix::process::Pid::from_raw(child.id().ok_or(ClientError::Protocol)? as i32)
                    .ok_or(ClientError::Protocol)?,
            );
            let mut stdout = child.stdout.take().ok_or(ClientError::Protocol)?;
            let mut stderr = child.stderr.take().ok_or(ClientError::Protocol)?;
            let remaining = (policy.max_output_bytes as usize)
                .checked_sub(logs.len())
                .ok_or(ClientError::ResponseTooLarge)?;
            let budget = std::sync::atomic::AtomicUsize::new(0);
            let io = async {
                let (a, b) = tokio::try_join!(
                    bounded_output(&mut stdout, &budget, remaining),
                    bounded_output(&mut stderr, &budget, remaining)
                )?;
                let status = child.wait().await.map_err(|_| ClientError::Protocol)?;
                let a = String::from_utf8(a).map_err(|_| ClientError::Protocol)?;
                let b = String::from_utf8(b).map_err(|_| ClientError::Protocol)?;
                Ok((status, a, b))
            };
            let remaining_time = Duration::from_secs(u64::from(policy.timeout_seconds))
                .checked_sub(started.elapsed())
                .ok_or(ClientError::Protocol)?;
            let outcome = tokio::time::timeout(remaining_time, io).await;
            let (status, a, b) = match outcome {
                Ok(Ok(v)) => v,
                Ok(Err(e)) => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    return Err(e);
                }
                Err(_) => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    return Err(ClientError::Protocol);
                }
            };
            for line in a.lines() {
                if let Some(t) = line.strip_prefix("ROLVYA_COMPLETED_TEST ")
                    && (!policy.required_tests.iter().any(|r| r == t)
                        || !tests.insert(t.to_owned()))
                {
                    return Err(ClientError::Protocol);
                }
            }
            logs.push_str(&a);
            logs.push_str(&b);
            if !status.success() {
                exit = status.code().unwrap_or(-1);
                break;
            }
        }
        let elapsed = started.elapsed();
        let completed = host
            .started_at
            .checked_add(elapsed.as_secs() as i64)
            .ok_or(ClientError::Protocol)?;
        if completed >= inputs.grant.expires_at {
            return Err(ClientError::Protocol);
        }
        let expected = policy
            .required_tests
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        if exit == 0 && tests != expected {
            return Err(ClientError::Protocol);
        }
        verify_materialized_tree(root, &inputs.files)?;
        let output = VerificationOutput {
            identity: "bounded-command-output".into(),
            sha256: rolvya_domain::worker::Digest::new(format!(
                "{:x}",
                <sha2::Sha256 as sha2::Digest>::digest(logs.as_bytes())
            ))
            .map_err(|_| ClientError::Protocol)?,
            content: logs,
        };
        let report = VerificationReport {
            format: CodingFormat::V1,
            binding: inputs.grant.binding.clone(),
            artifact_sha256: inputs.grant.artifact.clone(),
            source_sha256: inputs.work.context.source_sha256.clone(),
            change_set_sha256: commitment(&inputs.candidate.changes)
                .map_err(|_| ClientError::Protocol)?,
            policy_sha256: inputs.grant.policy.clone(),
            verifier: inputs.grant.subject.clone(),
            verifier_version: 1,
            environment: inputs.grant.image.as_str().into(),
            execution: inputs.grant.execution.clone(),
            started_at: host.started_at,
            completed_at: completed,
            terminal: if exit == 0 {
                VerificationTerminal::Completed
            } else {
                VerificationTerminal::Failed
            },
            exit_code: exit,
            tests_completed: tests.into_iter().collect(),
            outputs: vec![output],
            limits: policy.clone(),
        };
        AuthenticatedReport::sign(&inputs.grant, report, &self.key)
            .map_err(|_| ClientError::Protocol)
    }
}
fn hash_file(path: &std::path::Path) -> Result<rolvya_domain::worker::Digest, ClientError> {
    use sha2::Digest;
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|_| ClientError::Protocol)?;
    let mut h = sha2::Sha256::new();
    let mut buf = [0; 16384];
    loop {
        let n = file.read(&mut buf).map_err(|_| ClientError::Protocol)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    rolvya_domain::worker::Digest::new(format!("{:x}", h.finalize()))
        .map_err(|_| ClientError::Protocol)
}

#[cfg(test)]
impl VerifierHttpClient {
    pub(crate) fn fixture_signed_request(
        &self,
        operation: VerifierOperation,
        channel: [u8; 32],
    ) -> VerifierRequest {
        let mut r = VerifierRequest {
            version: 1,
            execution: self.execution.clone(),
            request_id: self.id("unit-report").unwrap(),
            operation,
            signature: vec![],
        };
        r.sign(&self.key, &channel).unwrap();
        r
    }
}

impl VerificationInputs {
    fn validate(&self, now: i64) -> Result<(), ClientError> {
        self.work
            .validate(&self.grant.binding, now)
            .map_err(|_| ClientError::Protocol)?;
        if self.grant.version != 1
            || self.grant.work != self.work.identity
            || self.grant.installation != self.work.context.installation
            || now < self.grant.issued_at
            || now >= self.grant.expires_at
            || self.artifact.len() > MAX_CODING_BYTES
            || commitment(&self.work.context.verification).map_err(|_| ClientError::Protocol)?
                != self.grant.policy
        {
            return Err(ClientError::Protocol);
        }
        let r: CodingResult =
            serde_json::from_slice(&self.artifact).map_err(|_| ClientError::Protocol)?;
        let digest = rolvya_domain::worker::Digest::new(format!(
            "{:x}",
            <sha2::Sha256 as sha2::Digest>::digest(&self.artifact)
        ))
        .map_err(|_| ClientError::Protocol)?;
        if r != self.candidate
            || digest != self.grant.artifact
            || r.binding != self.work.binding
            || r.installation != self.work.context.installation
            || r.repository != self.work.context.repository
            || r.base_commit != self.work.context.base_commit
            || r.base_tree != self.work.context.base_tree
            || r.source_sha256 != self.work.context.source_sha256
            || r.inputs != self.work.inputs
            || r.financial_event != self.work.financial_event
            || r.operation != self.work.operation
            || r.verification_execution != self.grant.execution
            || r.resulting_tree_sha256 != self.grant.resulting_tree
            || self.source.repository != r.repository
            || self.source.commit != r.base_commit
            || self.source.tree != r.base_tree
        {
            return Err(ClientError::Protocol);
        }
        crate::controller_keys::github_app::validate_coding_source(&self.source)
            .map_err(|_| ClientError::Protocol)?;
        if tree_commitment(&self.source.files).map_err(|_| ClientError::Protocol)?
            != r.source_sha256
        {
            return Err(ClientError::Protocol);
        }
        let expected = apply_changes(&self.source, &r.changes, &self.work.context.paths)
            .map_err(|_| ClientError::Protocol)?;
        if expected != self.files
            || tree_commitment(&expected).map_err(|_| ClientError::Protocol)?
                != self.grant.resulting_tree
        {
            return Err(ClientError::Protocol);
        }
        Ok(())
    }
}
fn verify_materialized_tree(
    root: &std::path::Path,
    expected: &[crate::controller_keys::github_app::SourceFile],
) -> Result<(), ClientError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut files = std::collections::BTreeMap::new();
    let mut entries = 0;
    while let Some((dir, depth)) = stack.pop() {
        let metadata = std::fs::symlink_metadata(&dir).map_err(|_| ClientError::Protocol)?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || metadata.permissions().mode() & 0o777 != 0o700
            || depth > 16
        {
            return Err(ClientError::Protocol);
        }
        for e in std::fs::read_dir(&dir).map_err(|_| ClientError::Protocol)? {
            let e = e.map_err(|_| ClientError::Protocol)?;
            entries += 1;
            if entries > 512 {
                return Err(ClientError::Protocol);
            }
            let p = e.path();
            let m = std::fs::symlink_metadata(&p).map_err(|_| ClientError::Protocol)?;
            if m.file_type().is_symlink() {
                return Err(ClientError::Protocol);
            }
            if m.is_dir() {
                stack.push((p, depth + 1));
                continue;
            }
            if !m.is_file() || m.nlink() != 1 || m.len() > 65536 {
                return Err(ClientError::Protocol);
            }
            let name = p
                .strip_prefix(root)
                .map_err(|_| ClientError::Protocol)?
                .to_str()
                .ok_or(ClientError::Protocol)?
                .to_owned();
            let f = expected
                .iter()
                .find(|f| f.path == name)
                .ok_or(ClientError::Protocol)?;
            let mode = if f.mode == "100755" { 0o700 } else { 0o600 };
            if m.permissions().mode() & 0o777 != mode
                || hash_file(&p)?
                    != rolvya_domain::worker::Digest::new(format!(
                        "{:x}",
                        <sha2::Sha256 as sha2::Digest>::digest(&f.bytes)
                    ))
                    .map_err(|_| ClientError::Protocol)?
            {
                return Err(ClientError::Protocol);
            }
            files.insert(name, ());
        }
    }
    if files.len() != expected.len() {
        return Err(ClientError::Protocol);
    }
    Ok(())
}

async fn bounded_output(
    r: &mut (impl tokio::io::AsyncRead + Unpin),
    budget: &std::sync::atomic::AtomicUsize,
    max: usize,
) -> Result<Vec<u8>, ClientError> {
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    let mut block = [0u8; 1024];
    loop {
        let n = r
            .read(&mut block)
            .await
            .map_err(|_| ClientError::Protocol)?;
        if n == 0 {
            break;
        }
        if budget
            .fetch_add(n, std::sync::atomic::Ordering::Relaxed)
            .saturating_add(n)
            > max
        {
            return Err(ClientError::ResponseTooLarge);
        }
        bytes.extend_from_slice(&block[..n]);
    }
    Ok(bytes)
}
#[cfg(test)]
impl VerifierHttpClient {
    pub(crate) fn fixture_sign_report(
        &self,
        g: &VerifierExecution,
        r: VerificationReport,
    ) -> AuthenticatedReport {
        AuthenticatedReport::sign(g, r, &self.key).unwrap()
    }
    pub(crate) async fn fixture_wrong_tls(&self) -> Result<(), ClientError> {
        let a = Channel::connect(&self.endpoint, self.tls.clone()).await?;
        let mut b = Channel::connect(&self.endpoint, self.tls.clone()).await?;
        assert_ne!(a.exporter, b.exporter);
        let request = self.fixture_signed_request(VerifierOperation::Claim {}, a.exporter);
        b.verifier_exchange(&request).await?;
        Ok(())
    }
}
#[cfg(test)]
impl VerifierHttpClient {
    pub(crate) fn fixture_endpoint(mut self, url: &str, ca: Vec<u8>) -> Self {
        self.endpoint = ControllerEndpoint::parse(url).unwrap();
        self.tls = tls::fixture_client(rustls::pki_types::CertificateDer::from(ca));
        self
    }
}

// Best-effort bounded cleanup of ordinary descendants, including future
// cancellation. Not a sandbox: a hostile setsid/host compromise still requires
// provider-enforced task isolation and termination before production activation.
struct CommandGroup(rustix::process::Pid);
impl Drop for CommandGroup {
    fn drop(&mut self) {
        let _ = rustix::process::kill_process_group(self.0, rustix::process::Signal::KILL);
    }
}
