use super::*;
use crate::controller_keys::provider_accounts::{self, RegistryPolicy};
use crate::worker_simulation::DeterministicClock;
use crate::{
    RequestId,
    admission::*,
    controller_keys::{
        ControllerBootstrap, InitializationRequest, LocalSetupAuthorization, ServiceIdentity,
    },
    finance::*,
    orchestration::*,
    resilience::{common, synthetic},
};
use axum::{
    body::{Body, to_bytes},
    extract::{Request, State},
    http::StatusCode,
    response::Response,
};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use ring::signature::KeyPair;
use rolvya_domain::ids::*;
use rolvya_plan_validator::Limits;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::task::JoinHandle;
pub const NOW: i64 = synthetic::T + 50;
pub struct Setup;
impl LocalSetupAuthorization for Setup {
    fn permits(&self, r: &InitializationRequest) -> bool {
        r.request_id().as_str() == "private-git-test-setup"
    }
}
pub fn signing_key() -> &'static (Vec<u8>, Vec<u8>) {
    static KEY: OnceLock<(Vec<u8>, Vec<u8>)> = OnceLock::new();
    KEY.get_or_init(generate_signing_key)
}
pub fn generate_signing_key() -> (Vec<u8>, Vec<u8>) {
    let f = tempfile::NamedTempFile::new().unwrap();
    assert!(
        std::process::Command::new("openssl")
            .args([
                "genpkey",
                "-algorithm",
                "RSA",
                "-pkeyopt",
                "rsa_keygen_bits:2048",
                "-out"
            ])
            .arg(f.path())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    let o = std::process::Command::new("openssl")
        .args(["pkcs8", "-topk8", "-nocrypt", "-outform", "DER", "-in"])
        .arg(f.path())
        .output()
        .unwrap();
    assert!(o.status.success());
    let k = ring::signature::RsaKeyPair::from_pkcs8(&o.stdout).unwrap();
    (o.stdout, k.public_key().as_ref().to_vec())
}
pub struct World {
    pub db: sqlx::SqlitePool,
    pub revoked: AtomicBool,
    pub wrong_repo: AtomicBool,
    pub wrong_installation: AtomicBool,
    pub token_extra: AtomicBool,
    pub token_expired: AtomicBool,
    pub permission_read: AtomicBool,
    pub throttle: AtomicBool,
    pub lose_ref: AtomicBool,
    pub lose_pr: AtomicBool,
    pub revoke_after_blob: AtomicBool,
    pub bad_blob: AtomicBool,
    pub symlink: AtomicBool,
    pub attempts: AtomicUsize,
    pub tokens: Mutex<BTreeMap<String, Value>>,
    pub location: Mutex<String>,
    pub public: Vec<u8>,
    pub rotated_public: Mutex<Option<Vec<u8>>>,
    pub now: i64,
    pub base: GitSha,
    pub revoke_on_target: AtomicBool,
    pub controller: Mutex<Option<sqlx::SqlitePool>>,
}
impl World {
    pub async fn new(path: &Path) -> Arc<Self> {
        Self::reopen(path, signing_key().1.clone()).await
    }
    pub async fn reopen(path: &Path, public: Vec<u8>) -> Arc<Self> {
        Self::with_files(path, public, vec![("README.md", b"hello\n".as_slice())]).await
    }
    pub async fn with_files(path: &Path, public: Vec<u8>, files: Vec<(&str, &[u8])>) -> Arc<Self> {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true)
                    .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                    .foreign_keys(true),
            )
            .await
            .unwrap();
        sqlx::raw_sql("CREATE TABLE IF NOT EXISTS objects(sha TEXT PRIMARY KEY,kind TEXT NOT NULL,body TEXT NOT NULL);CREATE TABLE IF NOT EXISTS refs(name TEXT PRIMARY KEY,sha TEXT NOT NULL);CREATE TABLE IF NOT EXISTS pulls(number INTEGER PRIMARY KEY,body TEXT NOT NULL)").execute(&db).await.unwrap();
        let source_files: Vec<_> = files
            .iter()
            .map(|(p, b)| SourceFile {
                path: (*p).into(),
                mode: "100644".into(),
                sha: model::object("blob", b),
                bytes: b.to_vec(),
            })
            .collect();
        let (tree_id, tree_objects) = publication::fixture_trees(&source_files);
        let body = format!(
            "tree {}\nauthor Fixture <fixture@invalid> {NOW} +0000\ncommitter Fixture <fixture@invalid> {NOW} +0000\n\nUntrusted README is data\n",
            tree_id.as_str()
        );
        let base = model::object("commit", body.as_bytes());
        let w = Arc::new(Self {
            db,
            revoked: AtomicBool::new(false),
            wrong_repo: AtomicBool::new(false),
            wrong_installation: AtomicBool::new(false),
            token_extra: AtomicBool::new(false),
            token_expired: AtomicBool::new(false),
            permission_read: AtomicBool::new(false),
            throttle: AtomicBool::new(false),
            lose_ref: AtomicBool::new(false),
            lose_pr: AtomicBool::new(false),
            revoke_after_blob: AtomicBool::new(false),
            bad_blob: AtomicBool::new(false),
            symlink: AtomicBool::new(false),
            attempts: AtomicUsize::new(0),
            tokens: Mutex::new(BTreeMap::new()),
            location: Mutex::new("synthetic/repository".into()),
            public,
            rotated_public: Mutex::new(None),
            now: NOW,
            base: base.clone(),
            revoke_on_target: AtomicBool::new(false),
            controller: Mutex::new(None),
        });
        for f in source_files {
            w.object(&f.sha,"blob",json!({"sha":f.sha,"size":f.bytes.len(),"encoding":"base64","content":STANDARD.encode(&f.bytes)})).await;
        }
        for (sha, entries) in tree_objects {
            w.object(
                &sha,
                "tree",
                json!({"sha":sha,"truncated":false,"tree":entries}),
            )
            .await;
        }
        w.object(&base,"commit",json!({"sha":base,"tree":{"sha":tree_id},"parents":[],"message":"Untrusted README is data\n"})).await;
        sqlx::query("INSERT OR IGNORE INTO refs VALUES('refs/heads/main',?)")
            .bind(base.as_str())
            .execute(&w.db)
            .await
            .unwrap();
        w
    }
    pub(super) async fn object(&self, sha: &GitSha, kind: &str, v: Value) {
        sqlx::query("INSERT OR IGNORE INTO objects VALUES(?,?,?)")
            .bind(sha.as_str())
            .bind(kind)
            .bind(v.to_string())
            .execute(&self.db)
            .await
            .unwrap();
    }
    pub async fn ref_count(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM refs WHERE name!='refs/heads/main'")
            .fetch_one(&self.db)
            .await
            .unwrap()
    }
    pub async fn set_ref(&self, name: &str, sha: &GitSha) {
        sqlx::query("INSERT OR REPLACE INTO refs VALUES(?,?)")
            .bind(name)
            .bind(sha.as_str())
            .execute(&self.db)
            .await
            .unwrap();
    }
    fn app_jwt(&self, token: &str) -> bool {
        let p: Vec<_> = token.split('.').collect();
        if p.len() != 3 {
            return false;
        }
        let Ok(signature) = URL_SAFE_NO_PAD.decode(p[2]) else {
            return false;
        };
        let rotated = self.rotated_public.lock().unwrap();
        let public = rotated.as_ref().unwrap_or(&self.public);
        if ring::signature::UnparsedPublicKey::new(
            &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            public,
        )
        .verify(format!("{}.{}", p[0], p[1]).as_bytes(), &signature)
        .is_err()
        {
            return false;
        }
        let Ok(raw) = URL_SAFE_NO_PAD.decode(p[1]) else {
            return false;
        };
        let Ok(v) = serde_json::from_slice::<Value>(&raw) else {
            return false;
        };
        v["iss"] == "123"
            && v["iat"].as_i64() == Some(self.now - 60)
            && v["exp"].as_i64() == Some(self.now + 540)
    }
    pub async fn server(self: &Arc<Self>) -> (String, JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = axum::Router::new()
            .fallback(handler)
            .with_state(self.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{addr}"), task)
    }
}
fn response(status: StatusCode, v: Value) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(v.to_string()))
        .unwrap()
}
fn ok(v: Value) -> Response {
    response(StatusCode::OK, v)
}
async fn handler(State(w): State<Arc<World>>, r: Request) -> Response {
    let method = r.method().clone();
    let path = r.uri().path().to_owned();
    let query = r.uri().query().unwrap_or("").to_owned();
    let auth = r
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .unwrap_or("")
        .to_owned();
    let raw = to_bytes(r.into_body(), 2 * 1024 * 1024).await.unwrap();
    let body: Value = if raw.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&raw).unwrap()
    };
    if w.throttle.load(Ordering::SeqCst) {
        return response(StatusCode::TOO_MANY_REQUESTS, json!({}));
    }
    let denied = || response(StatusCode::FORBIDDEN, json!({}));
    let app = path.starts_with("/app/") || path.ends_with("/installation");
    if app && !w.app_jwt(&auth) {
        return denied();
    }
    let installation = if w.wrong_installation.load(Ordering::SeqCst) {
        8
    } else {
        7
    };
    let perms = json!({"contents":if w.permission_read.load(Ordering::SeqCst){"read"}else{"write"},"metadata":"read","pull_requests":"write"});
    if path == "/app/installations/7" {
        return ok(
            json!({"id":installation,"app_id":123,"suspended_at":if w.revoked.load(Ordering::SeqCst){Value::String("suspended".into())}else{Value::Null},"permissions":perms}),
        );
    }
    let location = w.location.lock().unwrap().clone();
    if path == format!("/repos/{location}/installation") {
        return ok(json!({"id":installation,"app_id":123}));
    }
    if path == "/app/installations/7/access_tokens" {
        if method != reqwest::Method::POST || body["repository_ids"] != json!([99]) {
            return denied();
        }
        let mut granted = body["permissions"].clone();
        if w.token_extra.load(Ordering::SeqCst) {
            granted["administration"] = json!("write")
        }
        let token = format!("synthetic-installation-{}", hex(&random()));
        w.tokens
            .lock()
            .unwrap()
            .insert(token.clone(), granted.clone());
        return ok(
            json!({"token":token,"expires_at":http::date(if w.token_expired.load(Ordering::SeqCst){w.now}else{w.now+3600}).unwrap(),"permissions":granted,"repositories":[{"id":if w.wrong_repo.load(Ordering::SeqCst){100}else{99},"full_name":location}]}),
        );
    }
    let permission = w.tokens.lock().unwrap().get(&auth).cloned();
    let Some(permission) = permission else {
        return denied();
    };
    if w.revoked.load(Ordering::SeqCst) {
        return denied();
    }
    if path == "/repositories/99" {
        return ok(
            json!({"id":if w.wrong_repo.load(Ordering::SeqCst){100}else{99},"full_name":location}),
        );
    }
    let prefix = format!("/repos/{location}");
    let Some(path) = path.strip_prefix(&prefix) else {
        return response(StatusCode::NOT_FOUND, json!({}));
    };
    if let Some(short) = path.strip_prefix("/git/ref/") {
        let name = format!("refs/{short}");
        if short.starts_with("heads/rolvya/")
            && w.attempts.load(Ordering::SeqCst) > 0
            && w.revoke_on_target.swap(false, Ordering::SeqCst)
        {
            let pool = w.controller.lock().unwrap().as_ref().unwrap().clone();
            sqlx::query("UPDATE human_users SET security_epoch=security_epoch+1")
                .execute(&pool)
                .await
                .unwrap();
        }
        let sha: Option<String> = sqlx::query_scalar("SELECT sha FROM refs WHERE name=?")
            .bind(&name)
            .fetch_optional(&w.db)
            .await
            .unwrap();
        return if let Some(s) = sha {
            ok(json!({"ref":name,"object":{"type":"commit","sha":s}}))
        } else {
            response(StatusCode::NOT_FOUND, json!({}))
        };
    }
    if method == reqwest::Method::GET && path.starts_with("/git/") {
        let id = path.rsplit('/').next().unwrap();
        let text: Option<String> = sqlx::query_scalar("SELECT body FROM objects WHERE sha=?")
            .bind(id)
            .fetch_optional(&w.db)
            .await
            .unwrap();
        return if let Some(text) = text {
            let mut v: Value = serde_json::from_str(&text).unwrap();
            if path.contains("/blobs/") && w.bad_blob.load(Ordering::SeqCst) {
                v["content"] = json!(STANDARD.encode(b"forged"));
            }
            if path.contains("/trees/") && w.symlink.load(Ordering::SeqCst) {
                v["tree"][0]["mode"] = json!("120000");
            }
            ok(v)
        } else {
            response(StatusCode::NOT_FOUND, json!({}))
        };
    }
    if path == "/pulls" {
        if method == reqwest::Method::GET {
            let texts: Vec<String> = sqlx::query_scalar("SELECT body FROM pulls")
                .fetch_all(&w.db)
                .await
                .unwrap();
            let filters: std::collections::BTreeMap<_, _> =
                url::form_urlencoded::parse(query.as_bytes())
                    .into_owned()
                    .collect();
            let values = texts
                .into_iter()
                .map(|t| serde_json::from_str::<Value>(&t).unwrap())
                .filter(|v| {
                    filters.get("head").is_none_or(|h| {
                        h.split_once(':')
                            .is_some_and(|(_, branch)| v["head"]["ref"] == branch)
                    }) && filters.get("base").is_none_or(|b| v["base"]["ref"] == *b)
                })
                .collect();
            return ok(Value::Array(values));
        }
        if permission["pull_requests"] != "write" {
            return denied();
        }
        let head = body["head"].as_str().unwrap();
        let sha: Option<String> = sqlx::query_scalar("SELECT sha FROM refs WHERE name=?")
            .bind(format!("refs/heads/{head}"))
            .fetch_optional(&w.db)
            .await
            .unwrap();
        let Some(sha) = sha else {
            return response(StatusCode::UNPROCESSABLE_ENTITY, json!({}));
        };
        let number: i64 = sqlx::query_scalar("SELECT count(*)+1 FROM pulls")
            .fetch_one(&w.db)
            .await
            .unwrap();
        let pr = json!({"number":number,"head":{"ref":head,"sha":sha,"repo":{"id":99}},"base":{"ref":body["base"],"repo":{"id":99}},"merged_at":null});
        sqlx::query("INSERT INTO pulls VALUES(?,?)")
            .bind(number)
            .bind(pr.to_string())
            .execute(&w.db)
            .await
            .unwrap();
        assert_eq!(body["draft"], true);
        assert_eq!(body["maintainer_can_modify"], false);
        if w.lose_pr.load(Ordering::SeqCst) {
            return response(StatusCode::GATEWAY_TIMEOUT, json!({}));
        }
        return ok(pr);
    }
    if permission["contents"] != "write" || method != reqwest::Method::POST {
        return denied();
    }
    w.attempts.fetch_add(1, Ordering::SeqCst);
    match path {
        "/git/blobs" => {
            let bytes = STANDARD.decode(body["content"].as_str().unwrap()).unwrap();
            let sha = model::object("blob", &bytes);
            w.object(&sha,"blob",json!({"sha":sha,"size":bytes.len(),"encoding":"base64","content":STANDARD.encode(bytes)})).await;
            if w.revoke_after_blob.load(Ordering::SeqCst) {
                w.revoked.store(true, Ordering::SeqCst);
            }
            ok(json!({"sha":sha}))
        }
        "/git/trees" => {
            let entries = body["tree"].as_array().unwrap();
            let mut raw = Vec::new();
            for e in entries {
                raw.extend_from_slice(if e["mode"] == "040000" {
                    b"40000"
                } else {
                    e["mode"].as_str().unwrap().as_bytes()
                });
                raw.push(b' ');
                raw.extend_from_slice(e["path"].as_str().unwrap().as_bytes());
                raw.push(0);
                raw.extend_from_slice(&http::unhex(e["sha"].as_str().unwrap()).unwrap());
            }
            let sha = model::object("tree", &raw);
            w.object(
                &sha,
                "tree",
                json!({"sha":sha,"tree":entries,"truncated":false}),
            )
            .await;
            ok(json!({"sha":sha}))
        }
        "/git/commits" => {
            let tree = body["tree"].as_str().unwrap();
            let base = body["parents"][0].as_str().unwrap();
            let at = http::parse_time(body["author"]["date"].as_str().unwrap()).unwrap();
            let message = body["message"].as_str().unwrap();
            let bytes = format!(
                "tree {tree}\nparent {base}\nauthor Rolvya <rolvya@invalid> {at} +0000\ncommitter Rolvya <rolvya@invalid> {at} +0000\n\n{message}"
            );
            let sha = model::object("commit", bytes.as_bytes());
            w.object(
                &sha,
                "commit",
                json!({"sha":sha,"tree":{"sha":tree},"parents":[{"sha":base}],"message":message}),
            )
            .await;
            ok(json!({"sha":sha}))
        }
        "/git/refs" => {
            let target = body["ref"].as_str().unwrap();
            assert!(
                target.starts_with("refs/heads/rolvya/"),
                "fixture rejects protected branch writes"
            );
            let result = sqlx::query("INSERT INTO refs VALUES(?,?)")
                .bind(target)
                .bind(body["sha"].as_str().unwrap())
                .execute(&w.db)
                .await;
            if result.is_err() {
                return response(StatusCode::UNPROCESSABLE_ENTITY, json!({}));
            }
            if w.lose_ref.load(Ordering::SeqCst) {
                return response(StatusCode::GATEWAY_TIMEOUT, json!({}));
            }
            ok(json!({"ref":target,"object":{"type":"commit","sha":body["sha"]}}))
        }
        _ => response(StatusCode::NOT_FOUND, json!({})),
    }
}
pub struct Fixture {
    pub root: tempfile::TempDir,
    pub store: PlanStore,
    pub bootstrap: ControllerBootstrap,
    pub clock: Arc<DeterministicClock>,
    pub policy: RegistryPolicy<synthetic::Authority>,
    pub co: SyntheticCoordinator,
    pub project: ProjectId,
    pub binding: RepositoryBinding,
    pub owner: crate::controller_keys::human_auth::sessions::owner::GitOwnerFixture,
    pub remote: GitHubRemote,
    pub world: Arc<World>,
    server: JoinHandle<()>,
    pub webhook: Vec<u8>,
    pub contexts: BTreeMap<TaskId, crate::orchestration::coding::CodingContext>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Fixture {
    pub async fn new() -> Self {
        Self::new_kind(false, false, false, "").await
    }
    pub async fn new_coding() -> Self {
        Self::new_kind(true, false, false, "").await
    }
    pub async fn new_coding_execution() -> Self {
        Self::new_kind(true, false, true, "").await
    }
    pub async fn new_coding_overlap() -> Self {
        Self::new_kind(true, true, false, "").await
    }
    pub async fn new_coding_mode(mode: &str) -> Self {
        Self::new_kind(true, false, true, mode).await
    }
    async fn new_kind(coding: bool, overlap: bool, execution: bool, mode: &str) -> Self {
        let root = tempfile::tempdir_in("/workspace/scratch")
            .or_else(|_| tempfile::tempdir())
            .unwrap();
        for p in ["data", "keys"] {
            std::fs::create_dir(root.path().join(p)).unwrap();
            std::fs::set_permissions(root.path().join(p), std::fs::Permissions::from_mode(0o700))
                .unwrap();
        }
        let path = root.path().join("data/state.sqlite");
        let store = PlanStore::open(&path).await.unwrap();
        let bootstrap = ControllerBootstrap::open(
            store.clone(),
            &root.path().join("keys"),
            ServiceIdentity::current(),
            &[],
        )
        .await
        .unwrap();
        bootstrap
            .initialize(
                &InitializationRequest::new(RequestId::new("private-git-test-setup").unwrap()),
                &Setup,
            )
            .await
            .unwrap();
        let world = if coding {
            World::with_files(
                &root.path().join("remote-git.sqlite"),
                signing_key().1.clone(),
                vec![
                    ("src/alpha.rs", b"pub fn alpha()->u8{1}\n"),
                    ("src/beta.rs", b"pub fn beta()->u8{2}\n"),
                    ("src/shared.rs", b"pub const SHARED:u8=3;\n"),
                    ("src/obsolete.rs", b"// delete me\n"),
                    ("tests/alpha_test.rs", if execution { b"#[path=\"../src/alpha.rs\"] mod alpha; #[test] fn alpha_value(){assert_eq!(alpha::alpha(),11); }\n".as_slice() } else { b"// alpha test fixture\n".as_slice() }),
                    ("tests/beta_test.rs", if execution { b"#[path=\"../src/beta.rs\"] mod beta; #[test] fn beta_value(){assert_eq!(beta::beta(),22); }\n".as_slice() } else { b"// beta test fixture\n".as_slice() }),
                ],
            )
            .await
        } else {
            World::new(&root.path().join("remote-git.sqlite")).await
        };
        let mut contexts = BTreeMap::new();
        let mut plan = common::plan();
        let mut group = common::group("group-g", &plan.key);
        group.acceptance_criteria = vec![GROUP_CRITERION.into()];
        let mut a = common::task("task-a", &plan.key);
        a.parent = Some(common::task_ref(&group));
        a.acceptance_criteria = vec![ARTIFACT_CRITERION.into()];
        let mut b = a.clone();
        b.id = TaskId::new("task-b").unwrap();
        let mut c = common::task("task-c", &plan.key);
        c.dependencies = vec![common::task_ref(&group)];
        c.acceptance_criteria = vec![ARTIFACT_CRITERION.into(), INPUTS_CRITERION.into()];
        if coding {
            use crate::orchestration::coding::*;
            let files = vec![
                ("src/alpha.rs", b"pub fn alpha()->u8{1}\n".as_slice()),
                ("src/beta.rs", b"pub fn beta()->u8{2}\n".as_slice()),
                ("src/shared.rs", b"pub const SHARED:u8=3;\n".as_slice()),
                ("src/obsolete.rs", b"// delete me\n".as_slice()),
                ("tests/alpha_test.rs", if execution { b"#[path=\"../src/alpha.rs\"] mod alpha; #[test] fn alpha_value(){assert_eq!(alpha::alpha(),11); }\n".as_slice() } else { b"// alpha test fixture\n".as_slice() }),
                ("tests/beta_test.rs", if execution { b"#[path=\"../src/beta.rs\"] mod beta; #[test] fn beta_value(){assert_eq!(beta::beta(),22); }\n".as_slice() } else { b"// beta test fixture\n".as_slice() }),
            ]
            .into_iter()
            .map(|(p, b)| SourceFile {
                path: p.into(),
                mode: "100644".into(),
                sha: model::object("blob", b),
                bytes: b.to_vec(),
            })
            .collect::<Vec<_>>();
            for t in [&mut a, &mut b] {
                let paths = if t.id.as_str() == "task-a" {
                    vec![
                        "src/added.rs".into(),
                        "src/alpha.rs".into(),
                        "src/obsolete.rs".into(),
                    ]
                } else if overlap {
                    vec!["src/alpha.rs".into()]
                } else {
                    vec!["src/beta.rs".into()]
                };
                let ctx = CodingContext {
                    format: CodingFormat::V1,
                    installation: bootstrap
                        .load_existing()
                        .await
                        .unwrap()
                        .coding_installation(),
                    repository: RepositoryId::new(99).unwrap(),
                    repository_location: "synthetic/repository".into(),
                    base_commit: world.base.clone(),
                    base_tree: publication::fixture_trees(&files).0,
                    source_sha256: tree_commitment(&files).unwrap(),
                    paths: paths.clone(),
                    verification: VerificationPolicy {
                        identity: "private-synthetic-verification-policy".into(),
                        commands: if execution {
                            vec![{
                                let mut c =
                                    super::coding::execution::fixture_command(t.id.as_str());
                                if !mode.is_empty() {
                                    c.push(mode.into());
                                }
                                c
                            }]
                        } else {
                            vec![vec!["cargo".into(), "test".into(), "--locked".into()]]
                        },
                        required_tests: vec![format!("{}-test", t.id.as_str())],
                        timeout_seconds: if mode == "timeout" { 1 } else { 60 },
                        cpu_millis: 1000,
                        memory_mib: 512,
                        max_output_bytes: 1024,
                    },
                };
                t.scope = paths.clone();
                t.acceptance_criteria = vec![CODING_CRITERION.into()];
                t.context.sha256 = commitment(&ctx).unwrap().as_str().into();
                t.execution.as_mut().unwrap().write_scope =
                    rolvya_domain::plan::WriteScope::Remote {
                        repository: "synthetic/repository".into(),
                        git_ref: format!("refs/heads/rolvya/planned/{}", t.id.as_str()),
                        paths,
                    };
                contexts.insert(t.id.clone(), ctx);
            }
        }
        plan.tasks = vec![group, a, b, c];
        let authority = synthetic::Authority::default();
        let valid = rolvya_plan_validator::validate(&plan, &authority, Limits::default()).unwrap();
        store
            .create_project(&plan.key.project_id, &common::owner())
            .await
            .unwrap();
        let revision = store
            .save_draft(&valid, None, &common::owner(), &common::OwnerPolicy)
            .await
            .unwrap();
        store
            .activate(
                &revision,
                &common::request(),
                &common::owner(),
                &common::OwnerPolicy,
                &authority,
                Limits::default(),
            )
            .await
            .unwrap();
        store
            .apply_financial(
                &FinancialRequest {
                    project: plan.key.project_id.clone(),
                    request_id: RequestId::new("git-test-budget").unwrap(),
                    actor: common::owner(),
                    reason: "Synthetic Git fixture; no real provider operations".into(),
                    command: FinancialCommand::Approve {
                        expected_version: 0,
                        new_ceiling: synthetic::money(100),
                        proof: OwnerProof {
                            reference: synthetic::e("synthetic-budget"),
                            verified_at: SystemClock.now().unwrap(),
                        },
                    },
                },
                &synthetic::Finance,
            )
            .await
            .unwrap();
        let registry = provider_accounts::fixture_registry_for(
            &bootstrap,
            synthetic::T,
            "synthetic",
            "synthetic-account",
        )
        .await;
        let policy = registry.policy(authority);
        let clock = Arc::new(DeterministicClock::new(synthetic::T).unwrap());
        let co = if coding {
            bootstrap
                .coding_coordinator(&plan.key.project_id, &policy, Limits::default())
                .await
                .unwrap()
        } else {
            SyntheticCoordinator::open(
                store.clone(),
                &plan.key.project_id,
                &policy,
                Limits::default(),
            )
            .await
            .unwrap()
        };
        if !coding {
            for (index, offset) in [0, 1, 2, 30, 42, 50].into_iter().enumerate() {
                let pass = co
                    .pass(
                        &policy,
                        clock.as_ref(),
                        PassLimits {
                            admissions: if index == 0 { 2 } else { 0 },
                            events: 64,
                            validations: 16,
                            groups: 0,
                        },
                    )
                    .await
                    .unwrap();
                if index == 0 {
                    assert_eq!(pass.admitted.len(), 2);
                }
                clock.advance_to(synthetic::T + offset).unwrap();
            }
            co.pass(
                &policy,
                clock.as_ref(),
                PassLimits {
                    admissions: 0,
                    events: 64,
                    validations: 16,
                    groups: 0,
                },
            )
            .await
            .unwrap();
        }
        let signing = serde_json::to_vec(
            &json!({"format":1,"app_id":123,"pkcs8":STANDARD.encode(&signing_key().0)}),
        )
        .unwrap();
        let webhook = random().to_vec();
        let reference = |record: &str| CredentialReference {
            connection: "github-app".into(),
            account: "app-123".into(),
            record: record.into(),
            version: 1,
        };
        let binding = RepositoryBinding {
            project: plan.key.project_id.clone(),
            app: AppId::new(123).unwrap(),
            installation: InstallationId::new(7).unwrap(),
            repository: RepositoryId::new(99).unwrap(),
            location: "synthetic/repository".into(),
            source_branch: "main".into(),
            permissions: InstallationPermissions {
                contents: Permission::Write,
                metadata: Permission::Read,
                pull_requests: Permission::Write,
            },
            namespace: IsolatedNamespace::RolvyaAttemptsV1,
            allowed_prefixes: if coding {
                vec!["src/".into(), "tests/".into()]
            } else {
                vec!["results/".into()]
            },
            signing_key: reference("signing"),
            webhook_secret: reference("webhook"),
            expected_version: 0,
            valid_until: NOW + 3600,
            enabled: true,
        };
        let owner = crate::controller_keys::human_auth::sessions::owner::git_consent_on_existing(
            store.clone(),
            path,
            root.path().join("keys"),
            &binding,
            &signing,
            &webhook,
            NOW,
        )
        .await;
        *world.controller.lock().unwrap() = Some(store.pool.clone());
        let (origin, server) = world.server().await;
        let mut remote = GitHubRemote::open(&bootstrap).await.unwrap();
        remote.clock = clock.clone();
        remote.http = http::Api::fixture(origin);
        if coding {
            clock.advance_to(NOW).unwrap();
        }
        Self {
            root,
            store,
            bootstrap,
            clock,
            policy,
            co,
            project: plan.key.project_id,
            binding,
            owner,
            remote,
            world,
            server,
            webhook,
            contexts,
        }
    }
    pub async fn lease(&self, task: &str) -> LeaseRef {
        let r = sqlx::query("SELECT * FROM task_attempts WHERE task_id=?")
            .bind(task)
            .fetch_one(&self.store.pool)
            .await
            .unwrap();
        LeaseRef {
            task: rolvya_domain::plan::TaskRef {
                plan: rolvya_domain::plan::PlanKey {
                    project_id: self.project.clone(),
                    plan_id: PlanId::new(r.get::<String, _>("plan_id")).unwrap(),
                    version: PlanVersion::new(r.get::<i64, _>("version") as u64).unwrap(),
                },
                task_id: TaskId::new(task).unwrap(),
            },
            attempt_id: AttemptId::new(r.get::<String, _>("attempt_id")).unwrap(),
            epoch: r.get::<i64, _>("epoch") as u64,
        }
    }
    pub async fn prepare(&self, task: &str) -> Result<PublicationReceipt, GitError> {
        self.remote
            .prepare(
                &self.co,
                &self.lease(task).await,
                &self.world.base,
                &format!("results/{task}.json"),
            )
            .await
    }
    pub async fn count(&self, table: &str) -> i64 {
        sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&self.store.pool)
            .await
            .unwrap()
    }
    pub fn webhook_signature(&self, body: &[u8]) -> String {
        let mut h = Hmac::<Sha256>::new_from_slice(&self.webhook).unwrap();
        h.update(body);
        format!("sha256={}", hex(&h.finalize().into_bytes()))
    }
}

fn random() -> [u8; 32] {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).unwrap();
    b
}
