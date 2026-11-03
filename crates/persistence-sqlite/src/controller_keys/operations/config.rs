use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeploymentConfig {
    pub version: u32,
    pub service_uid: u32,
    pub database: PathBuf,
    pub custody: PathBuf,
    pub scratch: PathBuf,
    pub assets: PathBuf,
    pub asset_sha256: String,
    pub backup_destination: PathBuf,
    pub backup_key: PathBuf,
    pub identity_verifier_key: PathBuf,
    pub identity_verifier_sha256: String,
    pub canonical_origin: String,
    pub listen: std::net::SocketAddr,
    pub certificate_chain: Vec<PathBuf>,
    pub tls_key: PathBuf,
    pub support_url: Option<String>,
    pub email: Option<super::super::human_auth::operational_mail::SmtpConfiguration>,
}
impl DeploymentConfig {
    /// Configuration is host policy, never proof of human/provider/billing authority.
    pub fn validate(&self) -> Result<(), OperationError> {
        let invalid = OperationError::InvalidConfiguration;
        if self.version != 1
            || self.service_uid == 0
            || self.listen.port() < 1024
            || !(1..=8).contains(&self.certificate_chain.len())
            || !digest(&self.asset_sha256)
            || !digest(&self.identity_verifier_sha256)
        {
            return Err(invalid);
        }
        let origin = url::Url::parse(&self.canonical_origin).map_err(|_| invalid)?;
        if origin.scheme() != "https"
            || origin.host_str().is_none()
            || origin.path() != "/"
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.query().is_some()
            || origin.fragment().is_some()
            || origin.origin().ascii_serialization() != self.canonical_origin
        {
            return Err(invalid);
        }
        if let Some(s) = &self.support_url {
            let u = url::Url::parse(s).map_err(|_| invalid)?;
            if s.len() > 512
                || u.scheme() != "https"
                || u.host_str().is_none()
                || !u.username().is_empty()
                || u.password().is_some()
            {
                return Err(invalid);
            }
        }
        if let Some(email) = &self.email {
            email.validate()?;
        }
        let dirs = [
            self.database.parent().ok_or(invalid)?,
            self.custody.as_path(),
            self.scratch.as_path(),
            self.assets.as_path(),
            self.backup_destination.as_path(),
            self.backup_key.parent().ok_or(invalid)?,
        ];
        let paths = dirs
            .iter()
            .copied()
            .chain(self.certificate_chain.iter().map(PathBuf::as_path))
            .chain([
                self.database.as_path(),
                self.tls_key.as_path(),
                self.identity_verifier_key.as_path(),
                self.backup_key.as_path(),
            ]);
        if paths.into_iter().any(|p| !normalized(p)) {
            return Err(invalid);
        }
        for (i, a) in dirs.iter().enumerate() {
            for b in &dirs[i + 1..] {
                if a.starts_with(b) || b.starts_with(a) {
                    return Err(invalid);
                }
            }
        }
        // Certificates/configuration may share a private config directory, never data or web roots.
        for p in self
            .certificate_chain
            .iter()
            .chain([&self.tls_key, &self.identity_verifier_key])
        {
            if dirs.iter().any(|d| p.starts_with(d)) {
                return Err(invalid);
            }
        }
        if let Some(email) = &self.email {
            for path in [&email.credential_file, &email.delivery_public_key] {
                if !normalized(path) || dirs.iter().any(|d| path.starts_with(d)) {
                    return Err(invalid);
                }
            }
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, OperationError> {
        let bytes = super::files::private_file(path, 16384)?;
        let config: Self =
            serde_json::from_slice(&bytes).map_err(|_| OperationError::InvalidConfiguration)?;
        config.validate()?;
        Ok(config)
    }
    pub(super) fn validate_host(&self) -> Result<(), OperationError> {
        self.validate()?;
        if rustix::process::geteuid().as_raw() != self.service_uid {
            return Err(OperationError::Unauthorized);
        }
        for p in [
            self.database
                .parent()
                .ok_or(OperationError::InvalidConfiguration)?,
            &self.custody,
            &self.scratch,
            &self.backup_destination,
            self.backup_key
                .parent()
                .ok_or(OperationError::InvalidConfiguration)?,
        ] {
            super::files::PrivateDirectory::open(p)?;
        }
        super::files::public_directory(&self.assets)?;
        Ok(())
    }
}
fn normalized(path: &Path) -> bool {
    path.is_absolute()
        && path != Path::new("/")
        && path.components().collect::<PathBuf>().as_os_str() == path.as_os_str()
        && path.components().all(|c| {
            matches!(
                c,
                std::path::Component::RootDir | std::path::Component::Normal(_)
            )
        })
}
pub(super) fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
