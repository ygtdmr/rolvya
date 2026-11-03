use super::*;
use crate::controller_keys::{
    human_auth::sessions::https::BrowserAssets, worker_auth::https::tls::ServerIdentity,
};
use crate::resilience::common;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
struct Browser {
    client: reqwest::Client,
    base: String,
    cookies: BTreeMap<String, Zeroizing<String>>,
}
impl Browser {
    async fn request(
        &mut self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
        csrf: Option<&str>,
    ) -> reqwest::Response {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
            .header("sec-fetch-site", "same-origin")
            .header("origin", &self.base);
        if !self.cookies.is_empty() {
            request = request.header(
                "cookie",
                self.cookies
                    .iter()
                    .map(|(k, v)| format!("{k}={}", v.as_str()))
                    .collect::<Vec<_>>()
                    .join("; "),
            );
        }
        if let Some(v) = body {
            request = request
                .header("content-type", "application/json")
                .body(serde_json::to_vec(&v).unwrap());
        }
        if let Some(csrf) = csrf {
            request = request.header("x-rolvya-csrf", csrf);
        }
        let response = request.send().await.unwrap();
        for v in response.headers().get_all("set-cookie") {
            let (k, v) = v
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .split_once('=')
                .unwrap();
            self.cookies
                .insert(k.to_owned(), Zeroizing::new(v.to_owned()));
        }
        response
    }
    async fn json(
        &mut self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
        csrf: Option<&str>,
    ) -> (u16, serde_json::Value) {
        let r = self.request(method, path, body, csrf).await;
        let status = r.status().as_u16();
        let b = r.bytes().await.unwrap();
        (status, serde_json::from_slice(&b).unwrap())
    }
    async fn login(&mut self) {
        let (_, v) = self
            .json(
                reqwest::Method::GET,
                "/api/control/v1/auth/context",
                None,
                None,
            )
            .await;
        assert_eq!(self.json(reqwest::Method::POST,"/api/control/v1/auth/password",Some(serde_json::json!({"email":"owner@example.test","password":"offline-unique-human-password"})),v["csrf"].as_str()).await.0,200);
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_restart_direct_tls_real_password_scoped_binding_and_safe_shutdown() {
    let start = Instant::now();
    let mut f = Fixture::new().await;
    f.initialize().await;
    let user = f.enroll().await;
    let plan = common::plan();
    let project = plan.key.project_id.clone();
    f.op.store
        .create_project(&project, &common::owner())
        .await
        .unwrap();
    let valid = rolvya_plan_validator::validate(
        &plan,
        &common::SyntheticPolicy,
        rolvya_plan_validator::Limits::default(),
    )
    .unwrap();
    let revision =
        f.op.store
            .save_draft(&valid, None, &common::owner(), &common::OwnerPolicy)
            .await
            .unwrap();
    f.op.store
        .activate(
            &revision,
            &common::request(),
            &common::owner(),
            &common::OwnerPolicy,
            &common::SyntheticPolicy,
            rolvya_plan_validator::Limits::default(),
        )
        .await
        .unwrap();
    let approval =
        f.op.prepare_binding(&project, &user, "owner")
            .await
            .unwrap();
    f.op.close().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!(
        "https://localhost:{}",
        listener.local_addr().unwrap().port()
    );
    f.config.canonical_origin = base.clone();
    f.config.listen = listener.local_addr().unwrap();
    let certificate = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let der = certificate.cert.der().clone();
    let key = certificate.signing_key.serialize_der();
    write_private(&f.config.certificate_chain[0], &der);
    write_private(&f.config.tls_key, &key);
    assert!(
        ControllerService::prepare(f.config.clone()).await.is_err(),
        "an untrusted fixture CA must not activate production TLS"
    );
    let operator = HostOperator::open(f.config.clone()).await.unwrap();
    let store = operator.store.clone();
    let identity = ServerIdentity::fixture(
        vec![der.clone()],
        rustls::pki_types::PrivatePkcs8KeyDer::from(key).into(),
    );
    let assets = BrowserAssets::load(&f.config.assets).await.unwrap();
    let service = ControllerService::fixture(operator, identity, assets)
        .await
        .unwrap();
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .tls_backend_preconfigured(
            (*crate::controller_keys::worker_auth::https::tls::fixture_client(der)).clone(),
        )
        .build()
        .unwrap();
    let startup = Instant::now();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(service.serve_listener(listener, async {
        let _ = stopped.await;
    }));
    let mut browser = Browser {
        client: client.clone(),
        base: base.clone(),
        cookies: BTreeMap::new(),
    };
    let public = browser.request(reqwest::Method::GET, "/", None, None).await;
    assert_eq!(public.status(), 200);
    assert!(public.headers().contains_key("content-security-policy"));
    let startup_ms = startup.elapsed().as_millis();
    assert_eq!(
        browser
            .request(
                reqwest::Method::POST,
                "/api/control/v1/setup",
                Some(serde_json::json!({})),
                None
            )
            .await
            .status(),
        400
    );
    let mut other = Browser {
        client,
        base,
        cookies: BTreeMap::new(),
    };
    assert_eq!(
        other
            .request(
                reqwest::Method::GET,
                "/api/control/v1/projects/project/snapshot",
                None,
                None
            )
            .await
            .status(),
        401
    );
    let login = Instant::now();
    browser.login().await;
    let auth_ms = login.elapsed().as_millis();
    let (_, session) = browser
        .json(
            reqwest::Method::GET,
            "/api/control/v1/auth/session",
            None,
            None,
        )
        .await;
    let csrf = session["csrf"].as_str().unwrap();
    assert_eq!(
        browser
            .request(
                reqwest::Method::GET,
                "/api/control/v1/projects/project/snapshot",
                None,
                None
            )
            .await
            .status(),
        403
    );
    assert_eq!(
        browser
            .json(
                reqwest::Method::POST,
                "/api/control/v1/auth/project-binding",
                Some(serde_json::json!({"approval":approval.id,"confirm":true})),
                Some(csrf)
            )
            .await
            .0,
        200
    );
    assert_ne!(
        browser
            .json(
                reqwest::Method::POST,
                "/api/control/v1/auth/project-binding",
                Some(serde_json::json!({"approval":approval.id,"confirm":true})),
                Some(csrf)
            )
            .await
            .0,
        200
    );
    let read = Instant::now();
    let (status, snapshot) = browser
        .json(
            reqwest::Method::GET,
            "/api/control/v1/projects/project/snapshot",
            None,
            None,
        )
        .await;
    assert_eq!(status, 200);
    let read_us = read.elapsed().as_micros();
    assert!(
        serde_json::to_string(&snapshot)
            .unwrap()
            .contains(revision.fingerprint())
    );
    assert_eq!(
        other
            .request(
                reqwest::Method::GET,
                "/api/control/v1/projects/project/snapshot",
                None,
                None
            )
            .await
            .status(),
        401
    );
    // Retain several authenticated streams while reading through the same bounded pool.
    let sse_start = Instant::now();
    let mut streams = Vec::new();
    for _ in 0..4 {
        let stream = browser
            .request(
                reqwest::Method::GET,
                "/api/control/v1/projects/project/events",
                None,
                None,
            )
            .await;
        assert_eq!(stream.status(), 200);
        streams.push(stream);
    }
    let sse_ms = sse_start.elapsed().as_millis();
    let requests = Instant::now();
    for _ in 0..8 {
        assert_eq!(
            browser
                .request(
                    reqwest::Method::GET,
                    "/api/control/v1/projects/project/snapshot",
                    None,
                    None
                )
                .await
                .status(),
            200
        );
    }
    let reads_us = requests.elapsed().as_micros();
    let rss = std::fs::read_to_string("/proc/self/status").unwrap();
    let rss = rss
        .lines()
        .find(|line| line.starts_with("VmRSS:"))
        .unwrap()
        .to_owned();
    let db_bytes = std::fs::metadata(&f.config.database).unwrap().len();
    let wal_bytes = std::fs::metadata(f.config.database.with_extension("sqlite-wal"))
        .map(|m| m.len())
        .unwrap_or(0);
    println!(
        "ops runtime: startup={}ms four_sse={}ms eight_reads={}us {} db={}B wal={}B",
        startup_ms, sse_ms, reads_us, rss, db_bytes, wal_bytes
    );
    drop(streams);
    let health = browser
        .json(reqwest::Method::GET, "/health/live", None, None)
        .await;
    assert_eq!(health.1, serde_json::json!({"status":"alive"}));
    assert_eq!(
        browser
            .json(
                reqwest::Method::POST,
                "/api/control/v1/auth/logout",
                Some(serde_json::json!({})),
                Some(csrf)
            )
            .await
            .0,
        200
    );
    assert_eq!(
        browser
            .request(
                reqwest::Method::GET,
                "/api/control/v1/projects/project/snapshot",
                None,
                None
            )
            .await
            .status(),
        401
    );
    let shutdown = Instant::now();
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    println!(
        "ops measurement: setup-through-read={}ms password={}ms snapshot={}us shutdown={}ms",
        start.elapsed().as_millis(),
        auth_ms,
        read_us,
        shutdown.elapsed().as_millis()
    );
    // The service closes the shared pool only after it has drained requests and backups.
    assert!(store.pool.is_closed());
    // Actual isolated restore, matching emergency key, fresh HTTPS login and preserved reads.
    let original = HostOperator::open(f.config.clone()).await.unwrap();
    let point = original.backup(BackupClass::Frequent).await.unwrap();
    let kit = f.root.path().join("kit/https-restore.rvkit");
    original
        .export_recovery_kit(&kit, &[0x97; 32])
        .await
        .unwrap();
    original.close().await;
    let (_target, mut restored) = f.restored_config().await;
    let source = FilesystemDestination::open(&f.config.backup_destination).unwrap();
    HostOperator::restore_database(&restored, &source, &point.id)
        .await
        .unwrap();
    let recovery_kit = restored
        .identity_verifier_key
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("kit/https-restore.rvkit");
    write_private(&recovery_kit, &files::private_file(&kit, 4096).unwrap());
    let recovered = HostOperator::open(restored.clone()).await.unwrap();
    recovered
        .recover_controller_key(&recovery_kit, &[0x97; 32])
        .await
        .unwrap();
    assert!(
        recovered
            .readiness()
            .await
            .unwrap()
            .external_reconciliation_required
    );
    recovered.close().await;
    let restored_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    restored.listen = restored_listener.local_addr().unwrap();
    restored.canonical_origin = format!("https://localhost:{}", restored.listen.port());
    let resumed = HostOperator::open_existing(restored.clone()).await.unwrap();
    let identity = ServerIdentity::fixture(
        vec![certificate.cert.der().clone()],
        rustls::pki_types::PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der()).into(),
    );
    let assets = BrowserAssets::load(&restored.assets).await.unwrap();
    let service = ControllerService::fixture(resumed, identity, assets)
        .await
        .unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(service.serve_listener(restored_listener, async {
        let _ = stopped.await;
    }));
    let mut restored_browser = Browser {
        client: browser.client.clone(),
        base: restored.canonical_origin,
        cookies: BTreeMap::new(),
    };
    restored_browser.login().await;
    let (status, preserved) = restored_browser
        .json(
            reqwest::Method::GET,
            "/api/control/v1/projects/project/snapshot",
            None,
            None,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(preserved["recoveryRequired"], true);
    assert!(
        serde_json::to_string(&preserved)
            .unwrap()
            .contains(revision.fingerprint())
    );
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(30), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
