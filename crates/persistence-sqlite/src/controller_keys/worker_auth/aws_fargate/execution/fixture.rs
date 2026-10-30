//! Private synthetic AWS protocol service. Effects survive controller restart in
//! a separate disposable SQLite file; this is not application authority.
use super::*;
use aws_smithy_runtime_api::client::{
    http::{
        HttpConnector, HttpConnectorFuture, SharedHttpClient, SharedHttpConnector, http_client_fn,
    },
    orchestrator::{HttpRequest, HttpResponse},
    result::ConnectorError,
};
use aws_smithy_types::body::SdkBody;
use serde_json::{Value, json};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU8, Ordering},
};
pub(in crate::controller_keys::worker_auth) struct World {
    effects: sqlx::SqlitePool,
    expected: Vec<ExpectedExecution>,
    boots: Mutex<std::collections::BTreeSet<String>>,
    pub pause_definition: Mutex<Option<Arc<(tokio::sync::Notify, tokio::sync::Notify)>>>,
    pub use_broker: AtomicBool,
    pub revoked: AtomicBool,
    pub behavior: AtomicU8,
    pub status: Mutex<String>,
    pub requests: Mutex<Vec<Value>>,
}
impl World {
    pub(in crate::controller_keys::worker_auth) async fn new(
        path: &std::path::Path,
        expected: Vec<ExpectedExecution>,
    ) -> Arc<Self> {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
        let effects = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(
                SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true)
                    .busy_timeout(std::time::Duration::from_secs(2)),
            )
            .await
            .unwrap();
        sqlx::query("CREATE TABLE IF NOT EXISTS effects(token TEXT PRIMARY KEY,digest TEXT NOT NULL,task TEXT NOT NULL,stops INTEGER NOT NULL DEFAULT 0)").execute(&effects).await.unwrap();
        Arc::new(Self {
            effects,
            expected,
            boots: Mutex::new(Default::default()),
            pause_definition: Mutex::new(None),
            use_broker: AtomicBool::new(false),
            revoked: AtomicBool::new(false),
            behavior: AtomicU8::new(0),
            status: Mutex::new("RUNNING".into()),
            requests: Mutex::new(Vec::new()),
        })
    }
    pub(in crate::controller_keys::worker_auth) async fn stop_count(&self) -> i64 {
        sqlx::query_scalar("SELECT coalesce(sum(stops),0) FROM effects")
            .fetch_one(&self.effects)
            .await
            .unwrap()
    }
    pub(in crate::controller_keys::worker_auth) async fn count(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM effects")
            .fetch_one(&self.effects)
            .await
            .unwrap()
    }
    pub(in crate::controller_keys::worker_auth) fn authorize_boot(&self, a: &FargateExecution) {
        self.boots.lock().unwrap().insert(a.boot.clone());
    }
    pub(super) fn current(&self, r: &LaunchRequest, now: i64, boot: &str) -> Result<(), E> {
        if !self.boots.lock().unwrap().contains(boot) {
            return Err(E::UnknownExecution);
        }
        if self.revoked.load(Ordering::SeqCst) {
            return Err(E::Revoked);
        }
        if !self.expected.iter().any(|e| {
            e.binding == r.binding
                && e.installation == r.installation
                && e.workload == r.workload
                && e.account_ref == r.account_ref
                && e.account_version == r.account_version
        }) {
            return Err(E::ScopeMismatch);
        }
        if now < r.authorized_at {
            return Err(E::ExpiredEvidence);
        }
        Ok(())
    }
    pub(super) fn http(self: &Arc<Self>, r: &LaunchRequest) -> SharedHttpClient {
        let connector = SharedHttpConnector::new(Connector {
            world: self.clone(),
            request: r.clone(),
        });
        http_client_fn(move |_, _| connector.clone())
    }
}
#[derive(Clone)]
struct Connector {
    world: Arc<World>,
    request: LaunchRequest,
}
impl std::fmt::Debug for Connector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrivateEcsProtocolFixture([redacted])")
    }
}
impl HttpConnector for Connector {
    fn call(&self, request: HttpRequest) -> HttpConnectorFuture {
        let c = self.clone();
        HttpConnectorFuture::new(async move {
            let req = request.try_into_http1x().unwrap();
            assert!(
                req.headers()
                    .get("authorization")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("AWS4-HMAC-SHA256 ")
            );
            let b: Value = serde_json::from_slice(req.body().bytes().unwrap()).unwrap();
            c.world.requests.lock().unwrap().push(b.clone());
            let target = req.headers().get("x-amz-target").unwrap().to_str().unwrap();
            let e = c
                .world
                .expected
                .iter()
                .find(|e| e.binding == c.request.binding)
                .unwrap();
            if !target.ends_with("DescribeTaskDefinition") {
                assert_eq!(b["cluster"], json!(e.workload.cluster));
            }
            let task = json!({"taskArn":e.task,"clusterArn":e.workload.cluster,"taskDefinitionArn":e.workload.task_definition,"startedBy":e.started_by(),"launchType":"FARGATE","lastStatus":c.world.status.lock().unwrap().clone()});
            let mode = c.world.behavior.load(Ordering::SeqCst);
            let body = match target {
                "AmazonEC2ContainerServiceV20141113.DescribeTaskDefinition" => {
                    let pause = c.world.pause_definition.lock().unwrap().take();
                    if let Some(pause) = pause {
                        pause.0.notify_one();
                        pause.1.notified().await;
                    }
                    let mut d = super::super::sdk::tests::definition(e);
                    if mode == 8 {
                        d["taskRoleArn"] = json!(e.workload.execution_role)
                    }
                    json!({"taskDefinition":d})
                }
                "AmazonEC2ContainerServiceV20141113.RunTask" => {
                    assert_eq!(b["clientToken"], c.request.client_token);
                    assert_eq!(b["count"], 1);
                    assert_eq!(b["launchType"], "FARGATE");
                    assert_eq!(b["taskDefinition"], json!(e.workload.task_definition));
                    assert!(b.get("overrides").is_none());
                    if mode == 4 {
                        return Err(ConnectorError::io(Box::new(E::Transient)));
                    }
                    let d = digest(&b).unwrap();
                    let mut tx = c.world.effects.begin_with("BEGIN IMMEDIATE").await.unwrap();
                    let old: Option<String> =
                        sqlx::query_scalar("SELECT digest FROM effects WHERE token=?")
                            .bind(&c.request.client_token)
                            .fetch_optional(&mut *tx)
                            .await
                            .unwrap();
                    if let Some(old) = old {
                        assert_eq!(old, d, "same token changed parameters")
                    } else {
                        sqlx::query("INSERT INTO effects(token,digest,task) VALUES(?,?,?)")
                            .bind(&c.request.client_token)
                            .bind(d)
                            .bind(e.task.as_str())
                            .execute(&mut *tx)
                            .await
                            .unwrap();
                    }
                    tx.commit().await.unwrap();
                    if mode == 1 {
                        return Err(ConnectorError::io(Box::new(E::UnknownExecution)));
                    }
                    if mode == 2 {
                        json!({"tasks":[task],"failures":[{"arn":e.task,"reason":"RESOURCE:CPU"}]})
                    } else if mode == 3 {
                        json!({"tasks":[]})
                    } else {
                        json!({"tasks":[task]})
                    }
                }
                "AmazonEC2ContainerServiceV20141113.DescribeTasks" => {
                    if mode == 5 {
                        json!({"failures":[{"arn":e.task,"reason":"MISSING"}]})
                    } else {
                        json!({"tasks":[task]})
                    }
                }
                "AmazonEC2ContainerServiceV20141113.ListTasks" => {
                    assert!(b.get("desiredStatus").is_none());
                    assert!(b.get("family").is_none());
                    if mode == 5 {
                        json!({"taskArns":[]})
                    } else {
                        let tasks: Vec<String> =
                            sqlx::query_scalar("SELECT task FROM effects WHERE token=?")
                                .bind(&c.request.client_token)
                                .fetch_all(&c.world.effects)
                                .await
                                .unwrap();
                        json!({"taskArns":tasks})
                    }
                }
                "AmazonEC2ContainerServiceV20141113.StopTask" => {
                    sqlx::query("UPDATE effects SET stops=stops+1 WHERE token=?")
                        .bind(&c.request.client_token)
                        .execute(&c.world.effects)
                        .await
                        .unwrap();
                    if mode == 6 {
                        return Err(ConnectorError::io(Box::new(E::UnknownExecution)));
                    }
                    json!({"task":task})
                }
                _ => panic!("unregistered AWS operation"),
            };
            if mode == 7 && target.ends_with("RunTask") {
                c.world.revoked.store(true, Ordering::SeqCst);
            }
            HttpResponse::try_from(
                http::Response::builder()
                    .status(200)
                    .header("content-type", "application/x-amz-json-1.1")
                    .body(SdkBody::from(body.to_string()))
                    .unwrap(),
            )
            .map_err(|_| ConnectorError::user(Box::new(E::Integrity)))
        })
    }
}
pub(in crate::controller_keys::worker_auth) async fn open(
    identity: Arc<FargateIdentity>,
    world: Arc<World>,
    clear: bool,
) -> FargateExecution {
    let mut a = FargateExecution::from_identity(identity).await.unwrap();
    a.fixture = Some(world.clone());
    if clear {
        world.authorize_boot(&a)
    }
    a
}
pub(in crate::controller_keys::worker_auth) fn kill_at(a: &mut FargateExecution, label: &str) {
    let point = match label {
        "prepare-before" => Checkpoint::BeforePrepare,
        "prepare-after" => Checkpoint::AfterPrepare,
        "submit-before" => Checkpoint::BeforeSubmit,
        "submit-after" => Checkpoint::AfterSubmit,
        "effect-after" => Checkpoint::AfterEffect,
        "receipt-before" => Checkpoint::BeforeReceipt,
        "receipt-after" => Checkpoint::AfterReceipt,
        "stop-before" => Checkpoint::BeforeStop,
        "stop-after" => Checkpoint::AfterStop,
        "stop-effect-after" => Checkpoint::AfterStopEffect,
        _ => panic!("unknown private fault"),
    };
    a.hook = Box::new(move |at| {
        if at == point {
            rustix::process::kill_process(rustix::process::getpid(), rustix::process::Signal::KILL)
                .unwrap();
            unreachable!()
        }
        Ok(())
    });
}
