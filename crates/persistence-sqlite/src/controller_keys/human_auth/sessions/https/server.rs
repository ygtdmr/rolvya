use super::*;
use crate::controller_keys::worker_auth::https::tls::ServerIdentity;
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    service::TowerToHyperService,
};
use std::{future::Future, time::Duration};
use tokio::{
    net::TcpListener,
    sync::{Semaphore, watch},
    task::JoinSet,
};

pub struct HumanBrowserServer {
    pub(super) auth: Arc<HumanAuth>,
    pub(super) authority: Arc<RequestAuthority>,
    identity: ServerIdentity,
    pub(super) expected: String,
    pub(super) assets: BrowserAssets,
    pub(super) streams: Arc<Semaphore>,
    pub(super) active_producers: Arc<std::sync::atomic::AtomicUsize>,
    pub(super) open_bodies: Arc<std::sync::atomic::AtomicUsize>,
    pub(super) producers: Mutex<JoinSet<()>>,
}
impl HumanBrowserServer {
    /// Trusted local controller-host startup. Requires an existing installation,
    /// enrolled identities/memberships and a public-PKI validated TLS identity.
    /// It does not enable enrollment, mail, ownership binding or paid execution.
    pub async fn open(
        bootstrap: &ControllerBootstrap,
        identity: ServerIdentity,
        canonical_origin: &str,
        assets: BrowserAssets,
    ) -> Result<Arc<Self>, AuthError> {
        let auth = HumanAuth::open(bootstrap).await?;
        Self::build(auth, identity, canonical_origin, assets)
    }
    fn build(
        mut auth: HumanAuth,
        identity: ServerIdentity,
        canonical_origin: &str,
        assets: BrowserAssets,
    ) -> Result<Arc<Self>, AuthError> {
        origin(canonical_origin)?;
        let authority = Arc::new(RequestAuthority::default());
        auth.transport = authority.clone();
        auth.browser_authority = authority.clone();
        Ok(Arc::new(Self {
            auth: Arc::new(auth),
            authority,
            identity,
            expected: canonical_origin.to_owned(),
            assets,
            streams: Arc::new(Semaphore::new(16)),
            active_producers: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            open_bodies: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            producers: Mutex::new(JoinSet::new()),
        }))
    }
    /// Only accepted rustls connections reach the private Axum router. Forwarded
    /// transport/identity headers are unsupported and rejected, not trusted.
    pub async fn serve(
        self: Arc<Self>,
        listener: TcpListener,
        shutdown: impl Future<Output = ()> + Send,
    ) -> Result<(), AuthError> {
        let connections = Arc::new(Semaphore::new(64));
        let acceptor = tokio_rustls::TlsAcceptor::from(self.identity.config.clone());
        let (stop, stopped) = watch::channel(false);
        let mut tasks = JoinSet::new();
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                _=&mut shutdown=>break,
                Some(_)=tasks.join_next(),if !tasks.is_empty()=>{},
                incoming=listener.accept(),if tasks.len()<64=>{
                    let (socket,peer)=incoming.map_err(|_|AuthError::Unavailable)?;
                    let Ok(permit)=connections.clone().try_acquire_owned() else {continue};
                    let state=self.clone();let acceptor=acceptor.clone();let mut stopping=stopped.clone();
                    tasks.spawn(async move {
                        let _permit=permit;
                        let Ok(Ok(tls))=tokio::time::timeout(Duration::from_secs(5),acceptor.accept(socket)).await else {return};
                        let mut exporter=Zeroizing::new([0u8;32]);
                        if tls.get_ref().1.export_keying_material(&mut *exporter,b"EXPORTER-Rolvya-human-browser-v1",Some(b"control-v1")).is_err(){return}
                        let channel=Channel {peer:peer.ip(),exporter:Sha256::digest(exporter.as_slice()).into()};
                        let router=axum::Router::new().fallback(routes::handle).layer(axum::Extension(channel)).with_state(state);
                        let service=TowerToHyperService::new(router);
                        let mut builder=hyper::server::conn::http1::Builder::new();
                        builder.timer(TokioTimer::new()).header_read_timeout(Duration::from_secs(5)).max_headers(40).max_buf_size(16384).half_close(false);
                        let connection=builder.serve_connection(TokioIo::new(tls),service);tokio::pin!(connection);
                        tokio::select! {
                            _=&mut connection=>{},
                            _=tokio::time::sleep(Duration::from_secs(120))=>{},
                            _=stopping.changed()=>{connection.as_mut().graceful_shutdown();let _=tokio::time::timeout(Duration::from_secs(2),&mut connection).await;}
                        }
                    });
                }
            }
        }
        let _ = stop.send(true);
        if tokio::time::timeout(Duration::from_secs(5), async {
            while tasks.join_next().await.is_some() {}
        })
        .await
        .is_err()
        {
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
        }
        let mut producers =
            std::mem::take(&mut *self.producers.lock().map_err(|_| AuthError::Unavailable)?);
        if tokio::time::timeout(Duration::from_secs(10), async {
            while producers.join_next().await.is_some() {}
        })
        .await
        .is_err()
        {
            producers.abort_all();
            while producers.join_next().await.is_some() {}
        }
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::controller_keys::human_auth::sessions) fn fixture(
        auth: HumanAuth,
        identity: ServerIdentity,
        canonical_origin: &str,
        assets: BrowserAssets,
    ) -> Arc<Self> {
        Self::build(auth, identity, canonical_origin, assets).unwrap()
    }
}
