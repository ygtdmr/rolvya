//! AWS-compatible protocol fixtures, not AWS-signed attestation or live-account evidence.
use super::super::model::*;
use super::*;
use crate::{controller_keys::provider_accounts::CredentialReference, finance::EvidenceRef};
use aws_smithy_runtime_api::client::http::{HttpConnector, SharedHttpConnector, http_client_fn};
use rolvya_domain::{ids::*, plan::*, worker::*};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

pub(in crate::controller_keys::worker_auth) const T: i64 = 1_800_000_000;
pub(in crate::controller_keys::worker_auth) fn expected(task_id: &str) -> ExpectedExecution {
    let w = FargateWorkload {
        format: 1,
        cluster: ClusterArn::parse("arn:aws:ecs:eu-central-1:123456789012:cluster/rolvya").unwrap(),
        task_definition: TaskDefinitionArn::parse(
            "arn:aws:ecs:eu-central-1:123456789012:task-definition/rolvya-worker:7",
        )
        .unwrap(),
        task_role: RoleArn::parse("arn:aws:iam::123456789012:role/worker/task").unwrap(),
        execution_role: RoleArn::parse("arn:aws:iam::123456789012:role/worker/execution").unwrap(),
        controller_role: RoleArn::parse("arn:aws:iam::123456789012:role/controller/inspection")
            .unwrap(),
        controller_role_id: "AROAFIXTURECONTROLLER".into(),
        image: format!(
            "123456789012.dkr.ecr.eu-central-1.amazonaws.com/worker@sha256:{}",
            "a".repeat(64)
        ),
        image_digest: format!("sha256:{}", "a".repeat(64)),
        container: "worker".into(),
        cpu_units: 1024,
        memory_mib: 2048,
        ephemeral_gib: 20,
        platform_version: "1.4.0".into(),
        architecture: "X86_64".into(),
        user: "10001:10001".into(),
        command: vec!["rolvya-worker".into()],
        vpc: "vpc-0123456789abcdef0".into(),
        subnets: vec!["subnet-0123456789abcdef0".into()],
        security_groups: vec!["sg-0123456789abcdef0".into()],
        network_review: EvidenceRef::new("synthetic-network-review-pending-live-validation")
            .unwrap(),
        iam_review: EvidenceRef::new("synthetic-iam-review-pending-live-validation").unwrap(),
        runtime_seconds: 60,
    };
    let plan = PlanKey {
        project_id: ProjectId::new("project").unwrap(),
        plan_id: PlanId::new("plan").unwrap(),
        version: PlanVersion::new(1).unwrap(),
    };
    let mut e = ExpectedExecution {
        format: 1,
        installation: "1".repeat(32),
        account_ref: EvidenceRef::new("aws-account-a").unwrap(),
        account_version: 1,
        binding: WorkerBinding {
            task: TaskRef {
                plan,
                task_id: TaskId::new(task_id).unwrap(),
            },
            attempt_id: AttemptId::new(format!("attempt-{task_id}")).unwrap(),
            worker_id: WorkerId::new("worker").unwrap(),
            epoch: 1,
            plan_hash: Digest::new("2".repeat(64)).unwrap(),
            context_hash: Digest::new("3".repeat(64)).unwrap(),
        },
        command_digest: Digest::new("4".repeat(64)).unwrap(),
        task: TaskArn::parse(&format!(
            "arn:aws:ecs:eu-central-1:123456789012:task/rolvya/{}",
            if task_id == "task-a" {
                "a".repeat(32)
            } else {
                "b".repeat(32)
            }
        ))
        .unwrap(),
        workload: w,
        operation_token: "0".repeat(64),
        authorized_at: T,
        authorization_until: T + 3600,
    };
    e.operation_token = e.expected_operation_token().unwrap();
    e
}
pub(in crate::controller_keys::worker_auth) fn reference() -> CredentialReference {
    CredentialReference {
        connection: "aws-inspection".into(),
        account: "123456789012".into(),
        provider: "aws".into(),
        record: "controller-credential".into(),
        purpose: "aws-controller-inspection".into(),
        version: 1,
    }
}
pub(in crate::controller_keys::worker_auth) fn credentials() -> Credentials {
    let access = format!(
        "synthetic-{}",
        super::super::super::hex(&super::super::super::random::<16>().unwrap()[..])
    );
    let secret = super::super::super::hex(&super::super::super::random::<32>().unwrap()[..]);
    Credentials::new(
        access,
        secret,
        None,
        None,
        "scoped-AWS-protocol-fixture-only",
    )
}
pub(in crate::controller_keys::worker_auth) fn task(e: &ExpectedExecution) -> Value {
    json!({"taskArn":e.task,"clusterArn":e.workload.cluster,"taskDefinitionArn":e.workload.task_definition,"startedBy":e.started_by(),"launchType":"FARGATE","platformFamily":"Linux","platformVersion":e.workload.platform_version,"cpu":e.workload.cpu_units.to_string(),"memory":e.workload.memory_mib.to_string(),"desiredStatus":"RUNNING","lastStatus":"RUNNING","enableExecuteCommand":false,"containers":[{"name":e.workload.container,"image":e.workload.image,"imageDigest":e.workload.image_digest,"lastStatus":"RUNNING"}],"attachments":[{"type":"ElasticNetworkInterface","status":"ATTACHED","details":[{"name":"subnetId","value":e.workload.subnets[0]},{"name":"networkInterfaceId","value":"eni-0123456789abcdef0"}]}]})
}
pub(in crate::controller_keys::worker_auth) fn definition(e: &ExpectedExecution) -> Value {
    json!({"taskDefinitionArn":e.workload.task_definition,"status":"ACTIVE","networkMode":"awsvpc","requiresCompatibilities":["FARGATE"],"cpu":e.workload.cpu_units.to_string(),"memory":e.workload.memory_mib.to_string(),"taskRoleArn":e.workload.task_role,"executionRoleArn":e.workload.execution_role,"runtimePlatform":{"operatingSystemFamily":"LINUX","cpuArchitecture":e.workload.architecture},"containerDefinitions":[{"name":e.workload.container,"image":e.workload.image,"user":e.workload.user,"command":e.workload.command,"readonlyRootFilesystem":true,"privileged":false,"essential":true}]})
}
pub(in crate::controller_keys::worker_auth) struct AwsFixture {
    pub calls: AtomicUsize,
    pub task: Mutex<Value>,
    pub definition: Mutex<Value>,
    pub caller: Mutex<(String, String, String)>,
    pub failure: Mutex<Option<(u16, String)>>,
    pub requested: Mutex<Vec<(String, String)>>,
    pub pause: Mutex<Option<Arc<(tokio::sync::Notify, tokio::sync::Notify)>>>,
}
impl fmt::Debug for AwsFixture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AwsProtocolFixture([redacted requests])")
    }
}
impl AwsFixture {
    pub fn new(e: &ExpectedExecution) -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            task: Mutex::new(task(e)),
            definition: Mutex::new(definition(e)),
            caller: Mutex::new((
                "123456789012".into(),
                "arn:aws:sts::123456789012:assumed-role/inspection/controller-session".into(),
                "AROAFIXTURECONTROLLER:controller-session".into(),
            )),
            failure: Mutex::new(None),
            requested: Mutex::new(Vec::new()),
            pause: Mutex::new(None),
        })
    }
    pub fn http(self: &Arc<Self>) -> SharedHttpClient {
        let c = SharedHttpConnector::new(FixtureConnector(self.clone()));
        http_client_fn(move |_, _| c.clone())
    }
}
#[derive(Debug, Clone)]
struct FixtureConnector(Arc<AwsFixture>);
impl HttpConnector for FixtureConnector {
    fn call(&self, r: HttpRequest) -> HttpConnectorFuture {
        let state = self.0.clone();
        HttpConnectorFuture::new(async move {
            let req = r.try_into_http1x().unwrap();
            assert_eq!(req.method(), http::Method::POST);
            assert!(
                req.headers()
                    .get("authorization")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("AWS4-HMAC-SHA256 ")
            );
            assert!(req.headers().contains_key("x-amz-date"));
            let target = req
                .headers()
                .get("x-amz-target")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("GetCallerIdentity")
                .to_owned();
            let body = std::str::from_utf8(req.body().bytes().unwrap())
                .unwrap()
                .to_owned();
            state.calls.fetch_add(1, Ordering::SeqCst);
            {
                let mut calls = state.requested.lock().unwrap();
                assert!(calls.len() < 64);
                calls.push((target.clone(), body));
            }
            let (status, content_type, body) = if let Some((status, code)) =
                state.failure.lock().unwrap().clone()
            {
                if target == "GetCallerIdentity" {
                    (
                        status,
                        "text/xml",
                        format!(
                            "<ErrorResponse xmlns=\"https://sts.amazonaws.com/doc/2011-06-15/\"><Error><Type>Sender</Type><Code>{code}</Code><Message>synthetic service failure</Message></Error><RequestId>fixture-error</RequestId></ErrorResponse>"
                        ),
                    )
                } else {
                    (
                        status,
                        "application/x-amz-json-1.1",
                        json!({"__type":code,"message":"synthetic AWS service failure"})
                            .to_string(),
                    )
                }
            } else if target == "GetCallerIdentity" {
                assert_eq!(req.uri().host(), Some("sts.eu-central-1.amazonaws.com"));
                let (account, arn, user) = state.caller.lock().unwrap().clone();
                (
                    200,
                    "text/xml",
                    format!(
                        "<GetCallerIdentityResponse xmlns=\"https://sts.amazonaws.com/doc/2011-06-15/\"><GetCallerIdentityResult><Account>{account}</Account><Arn>{arn}</Arn><UserId>{user}</UserId></GetCallerIdentityResult><ResponseMetadata><RequestId>synthetic-controller-query</RequestId></ResponseMetadata></GetCallerIdentityResponse>"
                    ),
                )
            } else if target.ends_with(".DescribeTasks") {
                (
                    200,
                    "application/x-amz-json-1.1",
                    json!({"tasks":[state.task.lock().unwrap().clone()],"failures":[]}).to_string(),
                )
            } else if target.ends_with(".DescribeTaskDefinition") {
                (
                    200,
                    "application/x-amz-json-1.1",
                    json!({"taskDefinition":state.definition.lock().unwrap().clone()}).to_string(),
                )
            } else {
                return Err(ConnectorError::user(Box::new(E::LiveLaunchDisabled)));
            };
            let pause = state.pause.lock().unwrap().clone();
            if target.ends_with(".DescribeTaskDefinition")
                && let Some(p) = pause
            {
                p.0.notify_one();
                p.1.notified().await;
            }
            HttpResponse::try_from(
                http::Response::builder()
                    .status(status)
                    .header("content-type", content_type)
                    .body(SdkBody::from(body))
                    .unwrap(),
            )
            .map_err(|_| connector_error())
        })
    }
}
#[tokio::test]
async fn official_sdk_protocol_correlates_records_but_never_proves_task_possession() {
    let e = expected("task-a");
    let f = AwsFixture::new(&e);
    let sdk = SdkClients::fixture(&e.workload, credentials(), f.http());
    let result = sdk.inspect(&e, T).await.unwrap();
    assert!(result.configuration_correlated);
    assert_eq!(result.lifecycle, EcsLifecycle::Running);
    assert!(result.unknown_financial_exposure);
    assert_eq!(result.worker_issuance(), Err(E::TaskPossessionUnverified));
    assert_eq!(f.calls.load(Ordering::SeqCst), 3);
    let calls = f.requested.lock().unwrap();
    let task: Value = serde_json::from_str(&calls[1].1).unwrap();
    assert_eq!(task["tasks"], json!([e.task]));
    assert_eq!(task["cluster"], json!(e.workload.cluster));
}
#[tokio::test]
async fn controller_sts_identity_is_not_worker_identity_or_account_ownership() {
    for change in ["account", "role", "role-id", "session"] {
        let e = expected("task-a");
        let f = AwsFixture::new(&e);
        {
            let mut c = f.caller.lock().unwrap();
            match change {
                "account" => c.0 = "999999999999".into(),
                "role" => {
                    c.1 = "arn:aws:sts::123456789012:assumed-role/task/controller-session".into()
                }
                "role-id" => c.2 = "AROADIFFERENTROLE:controller-session".into(),
                _ => c.2 = "AROAFIXTURECONTROLLER:other-session".into(),
            };
        }
        let sdk = SdkClients::fixture(&e.workload, credentials(), f.http());
        assert!(sdk.inspect(&e, T).await.is_err());
        assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    }
}
#[tokio::test]
async fn task_role_definition_image_and_isolation_substitutions_are_rejected() {
    for field in [
        "task",
        "cluster",
        "definition",
        "role",
        "execution-role",
        "digest",
        "image",
        "privileged",
        "root",
        "readonly",
        "ports",
        "entrypoint",
        "environment",
        "mount",
        "network",
        "cpu",
        "memory",
        "disk",
        "exec",
        "started-by",
    ] {
        let e = expected("task-a");
        let f = AwsFixture::new(&e);
        {
            let mut t = f.task.lock().unwrap();
            let mut d = f.definition.lock().unwrap();
            match field {
                "task" => t["taskArn"] = json!(expected("task-b").task),
                "cluster" => {
                    t["clusterArn"] = json!("arn:aws:ecs:eu-central-1:123456789012:cluster/other")
                }
                "definition" => {
                    t["taskDefinitionArn"] = json!(
                        "arn:aws:ecs:eu-central-1:123456789012:task-definition/rolvya-worker:8"
                    )
                }
                "role" => d["taskRoleArn"] = json!(e.workload.execution_role),
                "execution-role" => d["executionRoleArn"] = json!(e.workload.task_role),
                "digest" => {
                    t["containers"][0]["imageDigest"] = json!(format!("sha256:{}", "f".repeat(64)))
                }
                "image" => d["containerDefinitions"][0]["image"] = json!("worker:latest"),
                "entrypoint" => {
                    d["containerDefinitions"][0]["entryPoint"] = json!(["/bin/sh", "-c"])
                }
                "privileged" => d["containerDefinitions"][0]["privileged"] = json!(true),
                "root" => d["containerDefinitions"][0]["user"] = json!("0"),
                "readonly" => d["containerDefinitions"][0]["readonlyRootFilesystem"] = json!(false),
                "ports" => {
                    d["containerDefinitions"][0]["portMappings"] = json!([{"containerPort":8080}])
                }
                "environment" => {
                    d["containerDefinitions"][0]["environment"] =
                        json!([{"name":"UNAPPROVED_VALUE","value":"synthetic"}])
                }
                "mount" => {
                    d["containerDefinitions"][0]["mountPoints"] =
                        json!([{"sourceVolume":"host","containerPath":"/host"}])
                }
                "network" => d["networkMode"] = json!("host"),
                "cpu" => t["cpu"] = json!("2048"),
                "memory" => t["memory"] = json!("4096"),
                "disk" => t["ephemeralStorage"] = json!({"sizeInGiB":200}),
                "exec" => t["enableExecuteCommand"] = json!(true),
                _ => t["startedBy"] = json!("unrelated-operation"),
            }
        }
        let sdk = SdkClients::fixture(&e.workload, credentials(), f.http());
        assert!(
            sdk.inspect(&e, T).await.is_err(),
            "substituted AWS observation rejected"
        );
    }
}
#[tokio::test]
async fn pending_stopping_stopped_and_unknown_observations_remain_non_authorizing() {
    for (state, expected_state) in [
        ("PROVISIONING", EcsLifecycle::Provisioning),
        ("PENDING", EcsLifecycle::Pending),
        ("STOPPING", EcsLifecycle::Stopping),
        ("STOPPED", EcsLifecycle::Stopped),
        ("FUTURE_PROVIDER_STATE", EcsLifecycle::Unknown),
    ] {
        let e = expected("task-a");
        let f = AwsFixture::new(&e);
        f.task.lock().unwrap()["lastStatus"] = json!(state);
        let sdk = SdkClients::fixture(&e.workload, credentials(), f.http());
        let result = sdk.inspect(&e, T).await.unwrap();
        assert_eq!(result.lifecycle, expected_state);
        assert!(!result.configuration_correlated);
        assert!(result.worker_issuance().is_err());
        assert!(result.unknown_financial_exposure);
    }
}
#[tokio::test]
async fn service_throttling_is_bounded_and_does_not_retry_or_claim_authentication() {
    let e = expected("task-a");
    let f = AwsFixture::new(&e);
    *f.failure.lock().unwrap() = Some((429, "ThrottlingException".into()));
    let sdk = SdkClients::fixture(&e.workload, credentials(), f.http());
    assert_eq!(sdk.inspect(&e, T).await, Err(E::Throttled));
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
}
#[test]
fn outbound_connector_rejects_mutating_actions_and_arbitrary_endpoints_without_network() {
    let e = expected("task-a");
    let client = ReadOnlyHttps::new(&e.workload).unwrap();
    for (uri, target) in [
        (
            "https://ecs.eu-central-1.amazonaws.com/",
            "AmazonEC2ContainerServiceV20141113.RunTask",
        ),
        (
            "https://ecs.eu-central-1.amazonaws.com/",
            "AmazonEC2ContainerServiceV20141113.StopTask",
        ),
        (
            "https://untrusted.invalid/",
            "AmazonEC2ContainerServiceV20141113.DescribeTasks",
        ),
        (
            "http://ecs.eu-central-1.amazonaws.com/",
            "AmazonEC2ContainerServiceV20141113.DescribeTasks",
        ),
    ] {
        let request = http::Request::builder()
            .method("POST")
            .uri(uri)
            .header("x-amz-target", target)
            .body(SdkBody::from("{}".to_owned()))
            .unwrap();
        assert!(client.check(&request).is_err());
    }
}
#[tokio::test]
async fn contradictory_or_missing_task_container_lifecycle_is_not_correlated_running() {
    for field in [
        "missing-desired",
        "unknown-desired",
        "stopped-container",
        "unknown-container",
    ] {
        let e = expected("task-a");
        let f = AwsFixture::new(&e);
        {
            let mut t = f.task.lock().unwrap();
            match field {
                "missing-desired" => {
                    t.as_object_mut().unwrap().remove("desiredStatus");
                }
                "unknown-desired" => t["desiredStatus"] = json!("UNKNOWN"),
                "stopped-container" => t["containers"][0]["lastStatus"] = json!("STOPPED"),
                _ => t["containers"][0]["lastStatus"] = json!("UNKNOWN"),
            }
        }
        let sdk = SdkClients::fixture(&e.workload, credentials(), f.http());
        assert!(
            !sdk.inspect(&e, T)
                .await
                .is_ok_and(|r| r.configuration_correlated && r.lifecycle == EcsLifecycle::Running),
            "incomplete or contradictory lifecycle cannot be correlated running"
        );
    }
}
#[test]
fn partial_describe_tasks_missing_ambiguous_and_cross_account_records_fail_closed() {
    use aws_sdk_ecs::{operation::describe_tasks::DescribeTasksOutput, types::Failure};
    let e = expected("task-a");
    for (arn, reason, want) in [
        (e.task.as_str(), "MISSING", E::TaskNotVisible),
        (e.task.as_str(), "UNKNOWN", E::UnknownExecution),
        (
            "arn:aws:ecs:eu-central-1:999999999999:task/rolvya/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "MISSING",
            E::ScopeMismatch,
        ),
    ] {
        let response = DescribeTasksOutput::builder()
            .failures(Failure::builder().arn(arn).reason(reason).build())
            .build();
        assert_eq!(
            super::super::inspection::task_record(&e, &response).map(|_| ()),
            Err(want)
        );
    }
    assert_eq!(
        super::super::inspection::task_record(&e, &DescribeTasksOutput::builder().build())
            .map(|_| ()),
        Err(E::TaskNotVisible)
    );
}
#[test]
fn workload_and_operation_identity_reject_scope_drift_and_unbounded_configuration() {
    let original = expected("task-a");
    for field in [
        "revision",
        "partition",
        "region",
        "roles",
        "image",
        "cpu",
        "memory",
        "disk",
        "uid",
        "network",
        "runtime",
        "command",
    ] {
        let mut e = original.clone();
        match field{
        "revision"=>e.format=2,"partition"=>e.task=TaskArn::parse("arn:aws-cn:ecs:cn-north-1:123456789012:task/rolvya/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),"region"=>e.task=TaskArn::parse("arn:aws:ecs:us-east-1:123456789012:task/rolvya/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),"roles"=>e.workload.execution_role=e.workload.task_role.clone(),"image"=>e.workload.image="worker:latest".into(),"cpu"=>e.workload.cpu_units=99999,"memory"=>e.workload.memory_mib=1,"disk"=>e.workload.ephemeral_gib=201,"uid"=>e.workload.user="0:0".into(),"network"=>e.workload.subnets.push(e.workload.subnets[0].clone()),"runtime"=>e.workload.runtime_seconds=0,_=>e.workload.command=vec!["x".repeat(257)]};
        assert!(e.validate().is_err());
    }
    let mut changed = original.clone();
    changed.binding.epoch += 1;
    assert_ne!(
        changed.expected_operation_token().unwrap(),
        original.operation_token
    );
    assert_eq!(original.operation_token.len(), 64);
    assert!(original.operation_token.is_ascii());
}
