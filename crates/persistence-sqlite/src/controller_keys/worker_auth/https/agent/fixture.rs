//! Private harmless protocol/executable fixtures. Not real providers or isolation.
use super::*;
use axum::{
    Extension, Router,
    body::{Body, to_bytes},
    extract::State,
    http::HeaderMap,
    http::Request,
    routing::post,
};
use hyper_util::{rt::TokioIo, service::TowerToHyperService};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// Private DNS/CA capability only; requests retain literal provider HTTPS URLs,
/// real hostname validation, and the production HTTPS-only client policy.
#[derive(Clone)]
pub(in crate::controller_keys) struct ProviderEndpoint {
    pub address: std::net::SocketAddr,
    pub acceptor: tokio_rustls::TlsAcceptor,
    root: rustls::pki_types::CertificateDer<'static>,
}
impl ProviderEndpoint {
    pub(in crate::controller_keys) fn route(&self, transport: &mut api::ApiTransport) {
        transport.fixture_route(self.address, self.root.clone());
    }
    fn new(address: std::net::SocketAddr) -> Self {
        Self::with_names(
            address,
            vec!["api.openai.com".into(), "api.anthropic.com".into()],
        )
    }
    fn with_names(address: std::net::SocketAddr, names: Vec<String>) -> Self {
        assert!(address.ip().is_loopback());
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = rcgen::CertificateParams::new(names)
            .unwrap()
            .self_signed(&key)
            .unwrap();
        let root = cert.der().clone();
        let identity = super::super::tls::ServerIdentity::fixture(
            vec![root.clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
        );
        Self {
            address,
            acceptor: tokio_rustls::TlsAcceptor::from(identity.config),
            root,
        }
    }
    fn transport(&self, adapter: Adapter, work: &CodingWork) -> api::ApiTransport {
        api::ApiTransport::fixture(adapter, self.address, self.root.clone(), work)
    }
}
pub(in crate::controller_keys) async fn provider_listener()
-> (tokio::net::TcpListener, ProviderEndpoint) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = ProviderEndpoint::new(listener.local_addr().unwrap());
    (listener, endpoint)
}
async fn serve_provider(
    listener: tokio::net::TcpListener,
    endpoint: ProviderEndpoint,
    routes: Router,
) {
    let mut connections = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            incoming = listener.accept(), if connections.len() < 16 => {
                let (socket, _) = incoming.unwrap();
                let acceptor = endpoint.acceptor.clone();
                let routes = routes.clone();
                connections.spawn(async move {
                    let Ok(Ok(tls)) = tokio::time::timeout(Duration::from_secs(5), acceptor.accept(socket)).await else { return; };
                    let service = TowerToHyperService::new(routes);
                    let _ = tokio::time::timeout(Duration::from_secs(30), hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(tls), service)).await;
                });
            }
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
}
pub(crate) fn executable() -> PathBuf {
    static BIN: std::sync::OnceLock<(tempfile::TempDir, PathBuf)> = std::sync::OnceLock::new();
    BIN.get_or_init(||{
        let dir=tempfile::tempdir().unwrap();let source=dir.path().join("fixture.rs");let bin=dir.path().join("cli-fixture");
        std::fs::write(&source,r#"
use std::io::Read;
fn main(){
 let a=std::env::args().collect::<Vec<_>>();
 if a.iter().any(|v|v=="--version"){println!("rolvya-cli-protocol-fixture 1");return}
 if a.iter().any(|v|v=="--help"){println!("--json --ephemeral --ignore-user-config --ignore-rules --sandbox --model --skip-git-repo-check --output-format --restricted --tools --effort --no-session-persistence");return}
 if a.iter().any(|v|v=="--fixture-child"){std::fs::write("child.pid",std::process::id().to_string()).unwrap();std::thread::sleep(std::time::Duration::from_secs(30));return}
 if a.iter().any(|v|v=="--fixture-timeout") {let _c=std::process::Command::new(std::env::current_exe().unwrap()).arg("--fixture-child").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();std::thread::sleep(std::time::Duration::from_secs(30));return}
 for k in ["OPENAI_API_KEY","ANTHROPIC_API_KEY","AWS_ACCESS_KEY_ID","AWS_PROFILE","GITHUB_TOKEN"]{assert!(std::env::var(k).is_err(),"ambient credentials forbidden");}
 let home=std::path::PathBuf::from(std::env::var("HOME").unwrap());assert!(home.is_absolute());assert_eq!(std::env::var("CODEX_HOME").unwrap(),home.join("codex").to_str().unwrap());assert_eq!(std::env::var("CLAUDE_CONFIG_DIR").unwrap(),home.join("claude").to_str().unwrap());
 let mut input=String::new();std::io::stdin().read_to_string(&mut input).unwrap();assert!(input.contains("task-b"));assert!(input.contains("src/beta.rs"));
 assert_eq!(std::fs::read("src/beta.rs").unwrap(),b"pub fn beta()->u8{2}\n");
 std::fs::write("src/beta.rs",b"pub fn beta()->u8{22}\n").unwrap();
 if a.iter().any(|v|v=="--json") {
  println!("{{\"type\":\"thread.started\",\"thread_id\":\"fixture-thread\"}}");println!("{{\"type\":\"turn.started\"}}");println!("{{\"type\":\"item.completed\",\"item\":{{\"type\":\"agent_message\",\"text\":\"fixture edit\"}}}}");println!("{{\"type\":\"turn.completed\",\"usage\":{{\"input_tokens\":40,\"output_tokens\":12}}}}");
 }else{println!("{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"usage\":{{\"input_tokens\":40,\"output_tokens\":12}}}}");}
}
"#).unwrap();
        assert!(std::process::Command::new("rustc").args(["--edition=2024","-O"]).arg(&source).arg("-o").arg(&bin).status().unwrap().success());
        (dir,bin)
    }).1.clone()
}
#[derive(Clone)]
struct ApiFixture {
    adapter: Adapter,
    model: String,
    alpha: u8,
    beta: bool,
    count: Arc<AtomicUsize>,
}
#[derive(Clone)]
struct ApiCredential(reqwest::header::HeaderValue);
async fn handler(
    State(s): State<ApiFixture>,
    Extension(auth): Extension<ApiCredential>,
    headers: HeaderMap,
    r: Request<Body>,
) -> String {
    let raw = to_bytes(r.into_body(), 131072).await.unwrap();
    let request: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(request["model"], s.model);
    assert_eq!(request["stream"], false);
    if s.adapter == Adapter::OpenAiApi {
        assert_eq!(headers["host"], "api.openai.com");
        assert!(
            headers["authorization"] == auth.0,
            "incorrect synthetic provider scope"
        );
        assert_eq!(request["store"], false);
    } else {
        assert_eq!(headers["host"], "api.anthropic.com");
        assert!(
            headers["x-api-key"] == auth.0,
            "incorrect synthetic provider scope"
        );
        assert_eq!(headers["anthropic-version"], "2023-06-01");
    }
    let step = s.count.fetch_add(1, Ordering::SeqCst);
    let path = if s.beta {
        "src/beta.rs"
    } else {
        "src/alpha.rs"
    };
    let modify = (
        "write_file",
        if s.beta {
            json!({"path":path,"content":"pub fn beta()->u8{22}\n"})
        } else {
            json!({"path":path,"content":format!("pub fn alpha()->u8{{{}}}\r\n",s.alpha)})
        },
    );
    let add = (
        "write_file",
        json!({"path":"src/added.rs","content":"pub const ADDED:u8=4;\n"}),
    );
    let delete = ("delete_file", json!({"path":"src/obsolete.rs"}));
    let operations = if s.beta {
        match step {
            0 => vec![("read_file", json!({"path":path}))],
            1 => vec![modify],
            2 => vec![],
            _ => panic!("unapproved retry"),
        }
    } else if s.adapter == Adapter::OpenAiApi {
        match step {
            0 => vec![("read_file", json!({"path":"src/alpha.rs"}))],
            1 => vec![modify],
            2 => vec![add],
            3 => vec![delete],
            4 => vec![],
            _ => panic!("unapproved retry"),
        }
    } else {
        match step {
            0 => vec![("read_file", json!({"path":"src/alpha.rs"}))],
            1 => vec![modify, add, delete],
            2 => vec![],
            _ => panic!("unapproved retry"),
        }
    };
    if step > 0 {
        assert!(
            String::from_utf8(raw.to_vec())
                .unwrap()
                .contains(if step == 1 {
                    if s.beta {
                        "pub fn beta"
                    } else {
                        "pub fn alpha"
                    }
                } else {
                    "updated"
                })
        );
    }
    if s.adapter == Adapter::OpenAiApi {
        let output = if operations.is_empty() {
            vec![
                json!({"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"fixture complete"}]}),
            ]
        } else {
            operations.into_iter().enumerate().map(|(i,(n,a))|json!({"type":"function_call","id":format!("fc-{step}-{i}"),"call_id":format!("call-{step}-{i}"),"name":n,"arguments":a.to_string(),"status":"completed"})).collect()
        };
        json!({"id":format!("response-{step}"),"model":s.model,"status":"completed","output":output,"usage":{"input_tokens":40,"output_tokens":12}}).to_string()
    } else {
        let done = operations.is_empty();
        let content = if done {
            vec![json!({"type":"text","text":"fixture complete"})]
        } else {
            operations.into_iter().enumerate().map(|(i,(n,a))|json!({"type":"tool_use","id":format!("tool-{step}-{i}"),"name":n,"input":a})).collect()
        };
        json!({"id":format!("response-{step}"),"model":s.model,"type":"message","role":"assistant","stop_reason":if done{"end_turn"}else{"tool_use"},"content":content,"usage":{"input_tokens":40,"output_tokens":12}}).to_string()
    }
}
fn auth(adapter: Adapter, key: &str) -> ApiCredential {
    let text = if adapter == Adapter::OpenAiApi {
        format!("Bearer {key}")
    } else {
        key.into()
    };
    let mut header = reqwest::header::HeaderValue::from_str(&text).unwrap();
    header.set_sensitive(true);
    ApiCredential(header)
}
pub(in crate::controller_keys) async fn proxy_provider(
    work: &CodingWork,
    adapter: Adapter,
) -> (
    ProviderEndpoint,
    tokio::task::JoinHandle<()>,
    Arc<AtomicUsize>,
) {
    let (listener, endpoint) = provider_listener().await;
    let count = Arc::new(AtomicUsize::new(0));
    let routes = Router::new()
        .route(
            if adapter == Adapter::OpenAiApi {
                "/v1/responses"
            } else {
                "/v1/messages"
            },
            post(handler),
        )
        .with_state(ApiFixture {
            adapter,
            model: work.profile.model.as_str().into(),
            alpha: 11,
            beta: work.binding.task.task_id.as_str() == "task-b",
            count: count.clone(),
        })
        .layer(Extension(auth(
            adapter,
            &format!("synthetic-provider-key-{}", work.connection),
        )));
    let task = tokio::spawn(serve_provider(listener, endpoint.clone(), routes));
    (endpoint, task, count)
}
pub(in crate::controller_keys) fn host(
    w: &CodingWork,
    directory: PathBuf,
    adapter: Adapter,
    api: Option<api::ApiTransport>,
) -> RemoteCoding {
    let model = w.model_account.as_ref().unwrap();
    RemoteCoding {
        work: w.identity.clone(),
        adapter,
        connection: w.connection.clone(),
        account: model.account.clone(),
        provider: model.provider.clone(),
        model: w.profile.model.as_str().into(),
        effort: w.profile.effort,
        mode: w.profile.access_mode,
        directory,
        account_home: cli::runtime_identity(w, adapter).unwrap(),
        max_steps: 8,
        max_context: 65536,
        max_output: 65536,
        max_output_tokens: 8192,
        api,
        cli: if adapter.api() {
            None
        } else {
            let path = executable();
            Some(cli::CliExecutable {
                sha256: super::super::verifier::hash_file(&path).unwrap(),
                path,
                version: "rolvya-cli-protocol-fixture 1".into(),
            })
        },
    }
}
pub(crate) async fn execute(
    worker: &mut super::super::coding_worker::CodingWorker,
    adapter: Adapter,
    now: i64,
) -> CodingResult {
    execute_value(worker, adapter, now, 11).await
}
pub(in crate::controller_keys) async fn execute_proxy(
    worker: &mut super::super::coding_worker::CodingWorker,
    adapter: Adapter,
    now: i64,
) -> CodingResult {
    let root = tempfile::tempdir().unwrap();
    let remote = host(worker.work(), root.path().join("runtime"), adapter, None);
    let verifier = format!("independent-{}", worker.work().binding.attempt_id.as_str());
    worker
        .execute_provider(remote, verifier, now)
        .await
        .unwrap()
}
pub(in crate::controller_keys) async fn execute_value(
    worker: &mut super::super::coding_worker::CodingWorker,
    adapter: Adapter,
    now: i64,
    alpha: u8,
) -> CodingResult {
    let root = tempfile::tempdir().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let (api, server) = if adapter.api() {
        let s = ApiFixture {
            adapter,
            alpha,
            beta: worker.work().binding.task.task_id.as_str() == "task-b",
            model: worker.work().profile.model.as_str().into(),
            count: count.clone(),
        };
        let (listener, endpoint) = provider_listener().await;
        let routes = Router::new()
            .route(
                if adapter == Adapter::OpenAiApi {
                    "/v1/responses"
                } else {
                    "/v1/messages"
                },
                post(handler),
            )
            .with_state(s)
            .layer(Extension(auth(adapter, "synthetic-model-protocol-only")));
        let transport = endpoint.transport(adapter, worker.work());
        let server = tokio::spawn(serve_provider(listener, endpoint, routes));
        (Some(transport), Some(server))
    } else {
        (None, None)
    };
    let host = host(worker.work(), root.path().join("runtime"), adapter, api);
    let verifier = format!("independent-{}", worker.work().binding.attempt_id.as_str());
    let result = worker.execute_provider(host, verifier, now).await.unwrap();
    if adapter.api() {
        assert_eq!(
            count.load(Ordering::SeqCst),
            if adapter == Adapter::OpenAiApi { 5 } else { 3 }
        );
    }
    if let Some(server) = server {
        server.abort();
    }
    result
}
pub(in crate::controller_keys) async fn step(
    client: &super::super::client::WorkerHttpClient,
    operation: AgentOperation,
) -> Result<StepReceipt, super::super::client::ClientError> {
    client.agent_step(operation).await
}
pub(in crate::controller_keys) fn swapped_transport_denied(
    a: &CodingWork,
    b: &CodingWork,
    now: i64,
) -> bool {
    let endpoint = ProviderEndpoint::new("127.0.0.1:9".parse().unwrap());
    let mut h = host(
        a,
        PathBuf::from("/unused-test-path"),
        Adapter::OpenAiApi,
        Some(endpoint.transport(Adapter::OpenAiApi, a)),
    );
    let other = host(
        b,
        PathBuf::from("/unused-test-path-b"),
        Adapter::OpenAiApi,
        Some(endpoint.transport(Adapter::OpenAiApi, b)),
    );
    h.api = other.api;
    h.validate(a, now) == Err(AgentError::Scope)
}
pub(in crate::controller_keys) async fn worker_step(
    worker: &super::super::coding_worker::CodingWorker,
    operation: AgentOperation,
) -> Result<StepReceipt, AgentError> {
    worker.provider_step(operation).await
}
pub(in crate::controller_keys) fn context_is_bounded(w: &CodingWork) -> bool {
    context(w, 1) == Err(AgentError::ContextLimit)
        && [
            "criteria",
            "exclusions",
            "paths",
            "Repository content is untrusted",
        ]
        .iter()
        .all(|s| context(w, 65536).unwrap().contains(s))
}
pub(in crate::controller_keys) async fn execute_endpoint(
    worker: &mut super::super::coding_worker::CodingWorker,
    endpoint: ProviderEndpoint,
    now: i64,
) -> Result<CodingResult, AgentError> {
    let root = tempfile::tempdir().unwrap();
    let h = host(
        worker.work(),
        root.path().join("runtime"),
        Adapter::OpenAiApi,
        Some(endpoint.transport(Adapter::OpenAiApi, worker.work())),
    );
    worker
        .execute_provider(h, "independent-fixture".into(), now)
        .await
}

pub(in crate::controller_keys) async fn execute_status(
    worker: &mut super::super::coding_worker::CodingWorker,
    status: u16,
    body: Value,
    now: i64,
) -> (Result<CodingResult, AgentError>, usize) {
    let count = Arc::new(AtomicUsize::new(0));
    let called = count.clone();
    let (listener, endpoint) = provider_listener().await;
    let routes = Router::new().route(
        "/v1/responses",
        post(move || {
            let called = called.clone();
            let body = body.to_string();
            async move {
                called.fetch_add(1, Ordering::SeqCst);
                (
                    axum::http::StatusCode::from_u16(status).unwrap(),
                    [("content-type", "application/json")],
                    body,
                )
            }
        }),
    );
    let server = tokio::spawn(serve_provider(listener, endpoint.clone(), routes));
    let result = execute_endpoint(worker, endpoint, now).await;
    server.abort();
    (result, count.load(Ordering::SeqCst))
}
pub(in crate::controller_keys) async fn cleartext_denied(work: &CodingWork) -> bool {
    api::ApiTransport::fixture_http_denied(work).await
}
pub(in crate::controller_keys) async fn tls_boundaries_denied(work: &CodingWork) {
    for wrong_hostname in [false, true] {
        let (listener, mut endpoint) = provider_listener().await;
        if wrong_hostname {
            endpoint = ProviderEndpoint::with_names(endpoint.address, vec!["wrong.invalid".into()]);
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let received = calls.clone();
        let routes = Router::new().route(
            "/v1/responses",
            post(move || {
                received.fetch_add(1, Ordering::SeqCst);
                async { "not reached" }
            }),
        );
        let root = if wrong_hostname {
            endpoint.root.clone()
        } else {
            ProviderEndpoint::new(endpoint.address).root
        };
        let transport =
            api::ApiTransport::fixture(Adapter::OpenAiApi, endpoint.address, root, work);
        let server = tokio::spawn(serve_provider(listener, endpoint, routes));
        assert_eq!(
            transport
                .request(Adapter::OpenAiApi, &json!({}), 1024)
                .await
                .err(),
            Some(AgentError::FinancialUncertainty)
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        server.abort();
    }
    let (listener, endpoint) = provider_listener().await;
    let redirected = Arc::new(AtomicUsize::new(0));
    let called = redirected.clone();
    let routes = Router::new()
        .route(
            "/v1/responses",
            post(|| async {
                (
                    axum::http::StatusCode::FOUND,
                    [("location", "https://api.openai.com/redirect-target")],
                )
            }),
        )
        .route(
            "/redirect-target",
            axum::routing::any(move || {
                called.fetch_add(1, Ordering::SeqCst);
                async { "not reached" }
            }),
        );
    let transport = endpoint.transport(Adapter::OpenAiApi, work);
    let server = tokio::spawn(serve_provider(listener, endpoint, routes));
    assert_eq!(
        transport
            .request(Adapter::OpenAiApi, &json!({}), 1024)
            .await
            .err(),
        Some(AgentError::InvalidResponse)
    );
    assert_eq!(redirected.load(Ordering::SeqCst), 0);
    server.abort();
}
pub(in crate::controller_keys) fn host_substitutions_denied(w: &CodingWork, now: i64) -> bool {
    let create = || {
        host(
            w,
            PathBuf::from("/unused-private-test"),
            Adapter::OpenAiApi,
            Some(
                ProviderEndpoint::new("127.0.0.1:9".parse().unwrap())
                    .transport(Adapter::OpenAiApi, w),
            ),
        )
    };
    let mut h = create();
    h.model = "unapproved-fallback".into();
    if h.validate(w, now).is_ok() {
        return false;
    }
    let mut h = create();
    h.account = "another-account".into();
    if h.validate(w, now).is_ok() {
        return false;
    }
    let mut h = create();
    h.connection = "another-connection".into();
    if h.validate(w, now).is_ok() {
        return false;
    }
    let mut h = create();
    h.adapter = Adapter::AnthropicApi;
    if h.validate(w, now).is_ok() {
        return false;
    }
    let mut h = create();
    h.mode = AccessMode::CliSession;
    if h.validate(w, now).is_ok() {
        return false;
    }
    let mut h = create();
    h.effort = Effort::High;
    if h.validate(w, now).is_ok() {
        return false;
    }
    let h = create();
    let mut old = w.clone();
    old.model_account.as_mut().unwrap().credential_version += 1;
    if h.validate(&old, now).is_ok() {
        return false;
    }
    true
}

pub(in crate::controller_keys) fn home_scope_denied(w: &CodingWork, now: i64) {
    let root = tempfile::tempdir().unwrap();
    let mut host = host(w, root.path().join("runtime"), Adapter::OpenAiApi, None);
    let original = host.account_home.clone();
    for variant in 0..6 {
        let mut other = w.clone();
        match variant {
            0 => other.context.installation = "f".repeat(32),
            1 => other.connection = "another-connection".into(),
            2 => other.model_account.as_mut().unwrap().account = "another-account".into(),
            3 => other.model_account.as_mut().unwrap().credential_version += 1,
            4 => other.binding.epoch += 1,
            _ => other.model_account.as_mut().unwrap().credential_record = "another-record".into(),
        }
        let other_identity = cli::runtime_identity(&other, Adapter::OpenAiApi).unwrap();
        assert_ne!(other_identity, original);
        host.account_home = other_identity;
        assert_eq!(host.validate(w, now), Err(AgentError::Scope));
    }
    assert!(!root.path().join("runtime").exists());
}

/// An upstream fixture may pause after receiving the request so tests can prove
/// SQLite is unlocked and revoke current authority before a response arrives.
pub(in crate::controller_keys) async fn proxy_response(
    body: String,
    status: u16,
    pause: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
) -> (
    ProviderEndpoint,
    tokio::task::JoinHandle<()>,
    Arc<AtomicUsize>,
) {
    use axum::response::IntoResponse;
    let (listener, endpoint) = provider_listener().await;
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    let routes = Router::new().route(
        "/v1/responses",
        post(move || {
            let body = body.clone();
            let count = seen.clone();
            let pause = pause.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                if let Some((arrived, release)) = pause {
                    arrived.notify_one();
                    release.notified().await;
                }
                (
                    axum::http::StatusCode::from_u16(status).unwrap(),
                    [("content-type", "application/json")],
                    body,
                )
                    .into_response()
            }
        }),
    );
    let server = tokio::spawn(serve_provider(listener, endpoint.clone(), routes));
    (endpoint, server, count)
}
