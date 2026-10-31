//! Private harmless protocol/executable fixtures. Not real providers or isolation.
use super::*;
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::State,
    http::HeaderMap,
    http::Request,
    routing::post,
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
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
    count: Arc<AtomicUsize>,
}
async fn handler(State(s): State<ApiFixture>, headers: HeaderMap, r: Request<Body>) -> String {
    let raw = to_bytes(r.into_body(), 131072).await.unwrap();
    let request: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(request["model"], s.model);
    assert_eq!(request["stream"], false);
    if s.adapter == Adapter::OpenAiApi {
        assert_eq!(
            headers["authorization"],
            "Bearer synthetic-model-protocol-only"
        );
        assert_eq!(request["store"], false);
    } else {
        assert_eq!(headers["x-api-key"], "synthetic-model-protocol-only");
        assert_eq!(headers["anthropic-version"], "2023-06-01");
    }
    let step = s.count.fetch_add(1, Ordering::SeqCst);
    let modify = (
        "write_file",
        json!({"path":"src/alpha.rs","content":format!("pub fn alpha()->u8{{{}}}\r\n",s.alpha)}),
    );
    let add = (
        "write_file",
        json!({"path":"src/added.rs","content":"pub const ADDED:u8=4;\n"}),
    );
    let delete = ("delete_file", json!({"path":"src/obsolete.rs"}));
    let operations = if s.adapter == Adapter::OpenAiApi {
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
                .contains(if step == 1 { "pub fn alpha" } else { "updated" })
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
            model: worker.work().profile.model.as_str().into(),
            count: count.clone(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!(
            "http://127.0.0.1:{}/v1/{}",
            listener.local_addr().unwrap().port(),
            if adapter == Adapter::OpenAiApi {
                "responses"
            } else {
                "messages"
            }
        );
        let routes = Router::new()
            .route(
                if adapter == Adapter::OpenAiApi {
                    "/v1/responses"
                } else {
                    "/v1/messages"
                },
                post(handler),
            )
            .with_state(s);
        let server = tokio::spawn(async move { axum::serve(listener, routes).await.unwrap() });
        (
            Some(api::ApiTransport::fixture(adapter, endpoint, worker.work())),
            Some(server),
        )
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
    let endpoint = "http://127.0.0.1:9/v1/responses".to_owned();
    let mut h = host(
        a,
        PathBuf::from("/unused-test-path"),
        Adapter::OpenAiApi,
        Some(api::ApiTransport::fixture(
            Adapter::OpenAiApi,
            endpoint.clone(),
            a,
        )),
    );
    let other = host(
        b,
        PathBuf::from("/unused-test-path-b"),
        Adapter::OpenAiApi,
        Some(api::ApiTransport::fixture(Adapter::OpenAiApi, endpoint, b)),
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
    endpoint: String,
    now: i64,
) -> Result<CodingResult, AgentError> {
    let root = tempfile::tempdir().unwrap();
    let h = host(
        worker.work(),
        root.path().join("runtime"),
        Adapter::OpenAiApi,
        Some(api::ApiTransport::fixture(
            Adapter::OpenAiApi,
            endpoint,
            worker.work(),
        )),
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
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!(
        "http://127.0.0.1:{}/v1/responses",
        listener.local_addr().unwrap().port()
    );
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
    let server = tokio::spawn(async move { axum::serve(listener, routes).await.unwrap() });
    let result = execute_endpoint(worker, endpoint, now).await;
    server.abort();
    (result, count.load(Ordering::SeqCst))
}
pub(in crate::controller_keys) fn host_substitutions_denied(w: &CodingWork, now: i64) -> bool {
    let create = || {
        host(
            w,
            PathBuf::from("/unused-private-test"),
            Adapter::OpenAiApi,
            Some(api::ApiTransport::fixture(
                Adapter::OpenAiApi,
                "http://127.0.0.1:9/v1/responses".into(),
                w,
            )),
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
