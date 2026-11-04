use super::*;
mod driver;
mod security;
#[test]
fn canonical_origin_and_request_facts_do_not_accept_forwarding_or_cross_origin() {
    for bad in [
        "http://localhost",
        "https://host.test/",
        "https://host.test/path",
        "https://person@host.test",
        "https://host.test?x=1",
    ] {
        assert!(origin(bad).is_err(), "{bad}");
    }
    assert!(origin("https://host.test").is_ok());
    let r = || {
        Request::builder()
            .uri("/api/control/v1/auth/context")
            .header("host", "host.test")
            .header("sec-fetch-site", "same-origin")
            .body(Body::empty())
            .unwrap()
    };
    assert!(check_headers(&r(), "https://host.test").is_ok());
    let mut wrong = r();
    wrong.headers_mut().insert(
        "x-forwarded-proto",
        header::HeaderValue::from_static("https"),
    );
    assert!(check_headers(&wrong, "https://host.test").is_err());
    let wrong = Request::builder()
        .method("POST")
        .uri("/api/control/v1/auth/password")
        .header("host", "host.test")
        .header("sec-fetch-site", "same-origin")
        .header("origin", "https://evil.test")
        .body(Body::empty())
        .unwrap();
    assert!(check_headers(&wrong, "https://host.test").is_err());
}

use crate::controller_keys::human_auth::sessions::owner::tests::panel::{
    BrowserSeed, browser_seed,
};
use crate::controller_keys::worker_auth::https::tls::ServerIdentity;
use base64::Engine;
use rustls::pki_types::PrivatePkcs8KeyDer;
use std::{sync::atomic::Ordering, time::Duration};
struct Running {
    server: Arc<HumanBrowserServer>,
    seed: SeedRemainder,
    http: reqwest::Client,
    base: String,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<Result<(), AuthError>>,
}
struct SeedRemainder {
    _root: tempfile::TempDir,
    store: PlanStore,
    clock: Arc<std::sync::atomic::AtomicI64>,
    mail: super::super::owner::tests::panel::MailProbe,
    user: HumanId,
    email: EmailAddress,
    credential: Password,
    viewer_email: EmailAddress,
    viewer_credential: Password,
    project: crate::ProjectId,
    fingerprint: String,
    totp: Option<Zeroizing<Vec<u8>>>,
    recovery: Vec<Zeroizing<String>>,
}
#[derive(Clone)]
struct Browser {
    http: reqwest::Client,
    base: String,
    cookies: BTreeMap<String, String>,
}
impl Browser {
    fn new(r: &Running) -> Self {
        Self {
            http: r.http.clone(),
            base: r.base.clone(),
            cookies: BTreeMap::new(),
        }
    }
    async fn raw(
        &mut self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
        csrf: Option<&str>,
    ) -> reqwest::Response {
        let mut req = self
            .http
            .request(method, format!("{}{path}", self.base))
            .header("sec-fetch-site", "same-origin")
            .header("origin", &self.base);
        if !self.cookies.is_empty() {
            req = req.header(
                "cookie",
                self.cookies
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        }
        if let Some(csrf) = csrf {
            req = req.header("x-rolvya-csrf", csrf)
        }
        if let Some(body) = body {
            req = req
                .header("content-type", "application/json")
                .body(serde_json::to_vec(&body).unwrap())
        }
        let response = req.send().await.unwrap();
        for c in response.headers().get_all("set-cookie") {
            let (name, value) = c
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .split_once('=')
                .unwrap();
            if value.is_empty() {
                self.cookies.remove(name);
            } else {
                self.cookies.insert(name.to_owned(), value.to_owned());
            }
        }
        response
    }
    async fn call(
        &mut self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
        csrf: Option<&str>,
    ) -> (u16, serde_json::Value) {
        let r = self.raw(method, path, body, csrf).await;
        let status = r.status().as_u16();
        let bytes = r.bytes().await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
    async fn initialize(&mut self) -> String {
        let (status, v) = self
            .call(
                reqwest::Method::GET,
                "/api/control/v1/auth/context",
                None,
                None,
            )
            .await;
        assert_eq!(status, 200);
        v["csrf"].as_str().unwrap().to_owned()
    }
    async fn login(&mut self, r: &Running, viewer: bool) -> (u16, serde_json::Value) {
        let csrf = self.initialize().await;
        let (email, credential) = if viewer {
            (&r.seed.viewer_email, &r.seed.viewer_credential)
        } else {
            (&r.seed.email, &r.seed.credential)
        };
        self.call(reqwest::Method::POST,"/api/control/v1/auth/password",Some(serde_json::json!({"email":email.as_str(),"password":std::str::from_utf8(&credential.0).unwrap()})),Some(&csrf)).await
    }
    async fn session(&mut self) -> (u16, serde_json::Value) {
        self.call(
            reqwest::Method::GET,
            "/api/control/v1/auth/session",
            None,
            None,
        )
        .await
    }
}
async fn fixture_assets(root: &std::path::Path) -> BrowserAssets {
    let path = root.join("assets");
    tokio::fs::create_dir(&path).await.unwrap();
    let app = "void 0;";
    let hash =
        base64::engine::general_purpose::STANDARD.encode(sha2::Sha384::digest(app.as_bytes()));
    let html = format!(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src &#39;none&#39;; script-src &#39;self&#39;; style-src-attr &#39;none&#39;; connect-src &#39;self&#39;; object-src &#39;none&#39;\"><script src=\"/app.abcdefgh.js\" integrity=\"sha384-{hash}\"></script>"
    );
    for (name, content) in [
        ("index.html", html.as_str()),
        ("app.abcdefgh.js", app),
        ("sw.js", "void 0;"),
        ("manifest.webmanifest", "{}"),
    ] {
        tokio::fs::write(path.join(name), content).await.unwrap()
    }
    BrowserAssets::load(&path).await.unwrap()
}
impl Running {
    async fn new(factor: bool) -> Self {
        Self::from_seed(browser_seed(factor).await, None).await
    }
    async fn from_seed(seed: BrowserSeed, assets: Option<BrowserAssets>) -> Self {
        let assets = match assets {
            Some(a) => a,
            None => fixture_assets(seed.root.path()).await,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!(
            "https://localhost:{}",
            listener.local_addr().unwrap().port()
        );
        let certificate = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
        let der = certificate.cert.der().clone();
        let identity = ServerIdentity::fixture(
            vec![der.clone()],
            PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der()).into(),
        );
        let server = HumanBrowserServer::fixture(seed.auth, identity, &base, assets);
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .tls_backend_preconfigured(
                (*crate::controller_keys::worker_auth::https::tls::fixture_client(der)).clone(),
            )
            .build()
            .unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let s = server.clone();
        let task = tokio::spawn(async move {
            s.serve(listener, async {
                let _ = stopped.await;
            })
            .await
        });
        Self {
            server,
            http,
            base,
            stop: Some(stop),
            task,
            seed: SeedRemainder {
                _root: seed.root,
                store: seed.store,
                clock: seed.clock,
                mail: seed.mail,
                user: seed.user,
                email: seed.email,
                credential: seed.credential,
                viewer_email: seed.viewer_email,
                viewer_credential: seed.viewer_credential,
                project: seed.project,
                fingerprint: seed.fingerprint,
                totp: seed.totp,
                recovery: seed.recovery,
            },
        }
    }
    async fn close(mut self) {
        let _ = self.stop.take().unwrap().send(());
        tokio::time::timeout(Duration::from_secs(20), self.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
#[tokio::test]
async fn genuine_tls_password_cookie_scoped_plan_and_server_logout() {
    let r = Running::new(false).await;
    let mut b = Browser::new(&r);
    assert_eq!(
        b.call(reqwest::Method::GET, "/api/control/v1/projects", None, None)
            .await
            .0,
        401
    );
    let (status, login) = b.login(&r, false).await;
    assert_eq!(status, 200);
    assert_eq!(login["state"], "authenticated");
    assert!(b.cookies.contains_key("__Host-rolvya"));
    assert!(b.cookies.contains_key("__Host-rolvya-csrf"));
    let (status, session) = b.session().await;
    assert_eq!(status, 200);
    assert_eq!(session["user"], r.seed.user.as_str());
    let csrf = session["csrf"].as_str().unwrap();
    let (status, projects) = b
        .call(reqwest::Method::GET, "/api/control/v1/projects", None, None)
        .await;
    assert_eq!(status, 200);
    assert_eq!(projects.as_array().unwrap().len(), 2);
    let (status, snapshot) = b
        .call(
            reqwest::Method::GET,
            "/api/control/v1/projects/project-a/snapshot",
            None,
            None,
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(snapshot["plan"]["fingerprint"], r.seed.fingerprint);
    assert_eq!(snapshot["role"], "owner");
    assert_eq!(snapshot["capabilities"]["events"], true);
    assert!(!snapshot.to_string().contains("PRIVATE-"));
    let old = b.clone();
    assert_eq!(
        b.call(
            reqwest::Method::POST,
            "/api/control/v1/auth/logout",
            Some(serde_json::json!({})),
            Some(csrf)
        )
        .await
        .0,
        200
    );
    assert!(!b.cookies.contains_key("__Host-rolvya"));
    let mut old = old;
    assert_eq!(old.session().await.0, 401);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM task_attempts")
        .fetch_one(&r.seed.store.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    r.close().await;
}
#[tokio::test]
async fn enrolled_totp_is_required_and_wrong_codes_never_expose_session() {
    let r = Running::new(true).await;
    let mut b = Browser::new(&r);
    let (status, login) = b.login(&r, false).await;
    assert_eq!(status, 200);
    assert_eq!(login["state"], "factor_required");
    assert!(!b.cookies.contains_key("__Host-rolvya"));
    assert_eq!(
        b.call(reqwest::Method::GET, "/api/control/v1/projects", None, None)
            .await
            .0,
        401
    );
    b.cookies.remove("__Host-rolvya-flow");
    let csrf = b.initialize().await;
    let code = super::super::tests::code(
        r.seed.totp.as_ref().unwrap(),
        r.seed.clock.load(Ordering::SeqCst),
    );
    // Reinitializing creates a different browser ceremony; it cannot attach the
    // earlier pending login, even when the user knows its current TOTP.
    assert_eq!(
        b.call(
            reqwest::Method::POST,
            "/api/control/v1/auth/factor",
            Some(serde_json::json!({"kind":"totp","code":code.0.as_str()})),
            Some(&csrf)
        )
        .await
        .0,
        401
    );
    r.close().await;
}
#[tokio::test]
async fn governance_changes_invalidate_panel_without_a_scheduler_launch() {
    let r = Running::new(false).await;
    let mut b = Browser::new(&r);
    assert_eq!(b.login(&r, false).await.0, 200);
    let (_, before) = b
        .call(
            reqwest::Method::GET,
            "/api/control/v1/projects/project-a/snapshot",
            None,
            None,
        )
        .await;
    sqlx::query("INSERT INTO human_governance_events(project_id,actor,request_id,event,reference,occurred_at) VALUES(?,?,?,?,?,?)")
        .bind(r.seed.project.as_str()).bind(r.seed.user.as_str()).bind(new_id().unwrap()).bind("review_recorded").bind("offline-test").bind(r.seed.clock.load(Ordering::SeqCst)).execute(&r.seed.store.pool).await.unwrap();
    let (_, after) = b
        .call(
            reqwest::Method::GET,
            "/api/control/v1/projects/project-a/snapshot",
            None,
            None,
        )
        .await;
    assert!(after["cursor"].as_i64().unwrap() > before["cursor"].as_i64().unwrap());
    r.close().await;
}
#[tokio::test]
async fn same_browser_email_confirmation_survives_context_reload_and_is_single_use() {
    let r = Running::new(false).await;
    let mut b = Browser::new(&r);
    let csrf = b.initialize().await;
    let (status, pending) = b
        .call(
            reqwest::Method::POST,
            "/api/control/v1/auth/email",
            Some(serde_json::json!({"email":r.seed.email.as_str(),"kind":"otp"})),
            Some(&csrf),
        )
        .await;
    assert_eq!(status, 200);
    let reference = pending["challenge"].as_str().unwrap();
    let token = (r.seed.mail.by_reference)(reference).unwrap();
    let csrf = b.initialize().await;
    let confirmation =
        serde_json::json!({"challenge":reference,"secret":token,"kind":"otp","confirm":true});
    let (status, result) = b
        .call(
            reqwest::Method::POST,
            "/api/control/v1/auth/email/confirm",
            Some(confirmation.clone()),
            Some(&csrf),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(result["state"], "authenticated");
    assert_eq!(
        b.call(
            reqwest::Method::POST,
            "/api/control/v1/auth/email/confirm",
            Some(confirmation),
            Some(&csrf)
        )
        .await
        .0,
        401
    );
    r.close().await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transport_review_revalidates_session_before_csrf_delivery() {
    let mut seed = browser_seed(false).await;
    let armed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = Arc::new(tokio::sync::Notify::new());
    let (release, released) = std::sync::mpsc::sync_channel(1);
    let released = Mutex::new(released);
    let a = armed.clone();
    let o = observed.clone();
    seed.auth.hook = Box::new(move |point| {
        if point == Checkpoint::AfterSessionCommit && a.swap(false, Ordering::SeqCst) {
            o.notify_one();
            released
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .map_err(|_| AuthError::Unavailable)?;
        }
        Ok(())
    });
    let r = Running::from_seed(seed, None).await;
    let mut b = Browser::new(&r);
    assert_eq!(b.login(&r, false).await.0, 200);
    armed.store(true, Ordering::SeqCst);
    let response = tokio::spawn(async move { b.session().await.0 });
    tokio::time::timeout(Duration::from_secs(5), observed.notified())
        .await
        .unwrap();
    sqlx::query("UPDATE human_users SET security_epoch=security_epoch+1 WHERE user_id=?")
        .bind(r.seed.user.as_str())
        .execute(&r.seed.store.pool)
        .await
        .unwrap();
    release.send(()).unwrap();
    assert_eq!(response.await.unwrap(), 401);
    r.close().await;
}
#[test]
fn transport_review_rejects_unrelated_cookie_context() {
    let authority = Arc::new(RequestAuthority::default());
    let guard = authority
        .reserve(Active {
            facts: BrowserFacts {
                secure: true,
                method: Method::Get,
                origin: "https://host.test".to_owned(),
                expected_origin: "https://host.test".to_owned(),
                same_origin: true,
                browser: [1; 32],
                credential: None,
                csrf: None,
                preauth_csrf: false,
                activity: Activity::Background,
                client_key: [2; 32],
            },
            operation: Operation::Read,
            transport: None,
            peer: "127.0.0.1".parse().unwrap(),
            expires: 100,
            evidence: RequestId::new(new_id().unwrap()).unwrap(),
            commitment: [3; 32],
            cookies: Some(vec![header::HeaderValue::from_static(
                "synthetic-response-cookie",
            )]),
            sent: false,
        })
        .unwrap();
    let bad = BrowserContext {
        request_id: guard.context.request_id.clone(),
        evidence: RequestId::new(new_id().unwrap()).unwrap(),
    };
    assert!(authority.cookies(&bad, 10).is_err());
    assert_eq!(authority.cookies(&guard.context, 10).unwrap().len(), 1);
    assert!(authority.cookies(&guard.context, 10).is_err());
}
async fn eventually(check: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !check() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stream_capacity_covers_disconnected_producer_during_real_sqlite_contention() {
    use sqlx::{Connection, Executor};
    let r = Running::new(false).await;
    let mut b = Browser::new(&r);
    assert_eq!(b.login(&r, false).await.0, 200);
    let (_, snapshot) = b
        .call(
            reqwest::Method::GET,
            "/api/control/v1/projects/project-a/snapshot",
            None,
            None,
        )
        .await;
    let cookies = b
        .cookies
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ");
    let mut stream = r
        .http
        .get(format!(
            "{}/api/control/v1/projects/project-a/events",
            r.base
        ))
        .header("sec-fetch-site", "same-origin")
        .header("cookie", cookies)
        .header("last-event-id", snapshot["eventCursor"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(stream.status().as_u16(), 200);
    assert!(stream.chunk().await.unwrap().is_some());
    eventually(|| r.seed.store.pool.num_idle() > 0).await;
    let mut writer = sqlx::SqliteConnection::connect_with(&r.seed.store.pool.connect_options())
        .await
        .unwrap();
    writer.execute("BEGIN IMMEDIATE").await.unwrap();
    let idle = r.seed.store.pool.num_idle();
    eventually(|| r.seed.store.pool.num_idle() < idle).await;
    drop(stream);
    eventually(|| r.server.open_bodies.load(Ordering::SeqCst) == 0).await;
    let active = r.server.active_producers.load(Ordering::SeqCst);
    let capacity_is_held = r.server.streams.available_permits() + active <= 16;
    writer.execute("COMMIT").await.unwrap();
    eventually(|| r.server.active_producers.load(Ordering::SeqCst) == 0).await;
    r.close().await;
    assert!(
        capacity_is_held,
        "disconnected SQL work escaped its stream capacity"
    );
}
#[tokio::test]
async fn foreground_interaction_intents_are_single_use_and_background_reads_never_extend_idle() {
    let r = Running::new(false).await;
    let mut b = Browser::new(&r);
    assert_eq!(b.login(&r, false).await.0, 200);
    let (_, initial) = b.session().await;
    let csrf = initial["csrf"].as_str().unwrap().to_owned();
    r.seed.clock.fetch_add(3600, Ordering::SeqCst);
    let (_, background) = b.session().await;
    assert_eq!(background["lastHumanAt"], initial["lastHumanAt"]);
    let (status, intent) = b
        .call(
            reqwest::Method::POST,
            "/api/control/v1/auth/activity",
            Some(serde_json::json!({})),
            Some(&csrf),
        )
        .await;
    assert_eq!(status, 200);
    let input = serde_json::json!({"intent":intent["intent"],"issued":intent["issued"]});
    let (status, recorded) = b
        .call(
            reqwest::Method::POST,
            "/api/control/v1/auth/interaction",
            Some(input.clone()),
            Some(&csrf),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(recorded["state"], "interaction_recorded");
    r.seed.clock.fetch_add(1, Ordering::SeqCst);
    assert_eq!(
        b.call(
            reqwest::Method::POST,
            "/api/control/v1/auth/interaction",
            Some(input),
            Some(&csrf)
        )
        .await
        .0,
        401
    );
    let (_, current) = b.session().await;
    assert_eq!(
        current["lastHumanAt"].as_i64().unwrap(),
        r.seed.clock.load(Ordering::SeqCst) - 1
    );
    r.close().await;
}
