//! Explicit TLS trust. Production accepts public PKI only; fixture roots have no
//! production constructor or runtime configuration switch.
use super::wire::ErrorCode;
use rustls::{
    ClientConfig, RootCertStore, ServerConfig,
    client::{WebPkiServerVerifier, danger::ServerCertVerifier},
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime},
};
use std::sync::Arc;

pub struct ServerIdentity {
    pub(super) config: Arc<ServerConfig>,
}
fn roots() -> RootCertStore {
    RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned())
}
fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}
fn protocols() -> [&'static rustls::SupportedProtocolVersion; 2] {
    [&rustls::version::TLS13, &rustls::version::TLS12]
}
impl ServerIdentity {
    /// Trusted controller-host configuration, not an HTTP configuration endpoint.
    /// Rotation constructs a new validated identity and restarts the listener;
    /// SQLite credentials and receipts are preserved. Private/test CA roots are
    /// unsupported until a separately reviewed deployment trust policy exists.
    pub fn from_der(
        chain: Vec<CertificateDer<'static>>,
        key: PrivateKeyDer<'static>,
        name: &str,
    ) -> Result<Self, ErrorCode> {
        let name = ServerName::try_from(name.to_owned()).map_err(|_| ErrorCode::InvalidProtocol)?;
        let leaf = chain.first().ok_or(ErrorCode::InvalidProtocol)?;
        if chain.len() > 8 || chain.iter().any(|c| c.len() > 16384) {
            return Err(ErrorCode::InvalidProtocol);
        }
        let verifier = WebPkiServerVerifier::builder_with_provider(Arc::new(roots()), provider())
            .build()
            .map_err(|_| ErrorCode::Unavailable)?;
        verifier
            .verify_server_cert(leaf, &chain[1..], &name, &[], UnixTime::now())
            .map_err(|_| ErrorCode::Unauthenticated)?;
        Self::build(chain, key)
    }
    fn build(
        chain: Vec<CertificateDer<'static>>,
        key: PrivateKeyDer<'static>,
    ) -> Result<Self, ErrorCode> {
        let mut config = ServerConfig::builder_with_provider(provider())
            .with_protocol_versions(&protocols())
            .map_err(|_| ErrorCode::Unavailable)?
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .map_err(|_| ErrorCode::InvalidProtocol)?;
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        config.max_early_data_size = 0;
        Ok(Self {
            config: Arc::new(config),
        })
    }
    #[cfg(test)]
    pub(super) fn fixture(
        chain: Vec<CertificateDer<'static>>,
        key: PrivateKeyDer<'static>,
    ) -> Self {
        Self::build(chain, key).unwrap()
    }
}
pub(in crate::controller_keys::worker_auth) fn client() -> Result<Arc<ClientConfig>, ErrorCode> {
    client_with_roots(roots())
}
fn client_with_roots(roots: RootCertStore) -> Result<Arc<ClientConfig>, ErrorCode> {
    let mut config = ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&protocols())
        .map_err(|_| ErrorCode::Unavailable)?
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    config.enable_early_data = false;
    Ok(Arc::new(config))
}
#[cfg(test)]
pub(super) fn fixture_client(root: CertificateDer<'static>) -> Arc<ClientConfig> {
    let mut r = RootCertStore::empty();
    r.add(root).unwrap();
    client_with_roots(r).unwrap()
}

pub(in crate::controller_keys::worker_auth) struct SecretBytes(pub zeroize::Zeroizing<Vec<u8>>);
impl AsRef<[u8]> for SecretBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
