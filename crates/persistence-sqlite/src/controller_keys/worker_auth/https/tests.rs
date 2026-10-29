//! Fixture CA and independently signed provider attestations exist only in this
//! cfg(test) module. Workers never possess the provider signing private key.
use super::{super::*, *};
use crate::{
    controller_keys::{
        InitializationRequest, LocalSetupAuthorization, ServiceIdentity, provider_accounts,
    },
    finance::*,
    orchestration::*,
    resilience::{common, synthetic},
    worker_simulation::*,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use client::*;
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use rolvya_domain::{ids::*, plan::*};
use rolvya_plan_validator::Limits;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
    sync::Mutex,
};
use tokio::{net::TcpListener, sync::oneshot};
use wire::*;

fn temporary() -> tempfile::TempDir {
    let tmp = std::env::temp_dir();
    let uid = rustix::process::geteuid().as_raw();
    let base = if [0, uid].contains(&fs::metadata(&tmp).unwrap().uid()) {
        tmp
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|p| p.join(".git").exists())
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    };
    tempfile::tempdir_in(base).unwrap()
}
struct Setup;
impl LocalSetupAuthorization for Setup {
    fn permits(&self, r: &InitializationRequest) -> bool {
        r.request_id().as_str() == "https-fixture-setup"
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Statement {
    domain: String,
    scope: VerifiedIdentityScope,
    identity: VerifiedWorkerIdentity,
    not_before: i64,
    expires_at: i64,
}
struct SignedProvider {
    public: Vec<u8>,
    transport: Arc<context::HttpTransport>,
    prepare: Mutex<BTreeMap<String, Attestation>>,
    pause: Mutex<Option<Arc<(tokio::sync::Notify, tokio::sync::Notify)>>>,
}
impl SignedProvider {
    fn checked(&self, c: &IdentityCheck<'_>, now: i64) -> Result<IdentityGrant, WorkerAuthError> {
        let a = if c.stage == VerificationStage::Prepare {
            self.prepare
                .lock()
                .map_err(|_| WorkerAuthError::Unavailable)?
                .get(c.request.as_str())
                .cloned()
                .ok_or(WorkerAuthError::Denied)?
        } else {
            self.transport.attestation(c)?
        };
        let signature = B64
            .decode(&a.signature)
            .map_err(|_| WorkerAuthError::Denied)?;
        UnparsedPublicKey::new(&ED25519, &self.public)
            .verify(a.statement.as_bytes(), &signature)
            .map_err(|_| WorkerAuthError::Denied)?;
        let s: Statement =
            serde_json::from_str(&a.statement).map_err(|_| WorkerAuthError::Denied)?;
        if s.domain != "ROLVYA-SYNTHETIC-PROVIDER-TEST-V1"
            || s.scope != c.scope()
            || s.scope.evidence != a.reference
            || now < s.not_before
            || now >= s.expires_at
        {
            return Err(WorkerAuthError::Denied);
        }
        Ok(IdentityGrant {
            identity: s.identity,
            scope: s.scope,
            not_before: s.not_before,
            expires_at: s.expires_at,
        })
    }
}
impl IdentityAuthority for SignedProvider {
    fn verify<'a>(&'a self, c: &'a IdentityCheck<'a>, t: i64) -> VerificationFuture<'a> {
        Box::pin(async move {
            let grant = self.checked(c, t)?;
            let pause = self.pause.lock().unwrap().clone();
            if let Some(pause) = pause {
                pause.0.notify_one();
                pause.1.notified().await;
            }
            Ok(grant)
        })
    }
    fn current(
        &self,
        c: &IdentityCheck<'_>,
        g: &IdentityGrant,
        t: i64,
    ) -> Result<(), WorkerAuthError> {
        let fresh = self.checked(c, t)?;
        if fresh.scope != g.scope
            || fresh.identity != g.identity
            || fresh.expires_at != g.expires_at
        {
            return Err(WorkerAuthError::Denied);
        }
        Ok(())
    }
}
fn sign(key: &Ed25519KeyPair, scope: VerifiedIdentityScope) -> Attestation {
    let identity = VerifiedWorkerIdentity {
        subject: synthetic::e("independently-signed-test-subject"),
        execution: synthetic::e(&format!(
            "test-execution-{}",
            scope.binding.attempt_id.as_str()
        )),
        capabilities: synthetic::caps(),
        permissions: vec![
            WorkerPermission::Register,
            WorkerPermission::Claim,
            WorkerPermission::AcceptLease,
            WorkerPermission::Heartbeat,
            WorkerPermission::Progress,
            WorkerPermission::SubmitResult,
            WorkerPermission::StopQuery,
            WorkerPermission::StopAck,
            WorkerPermission::Renew,
        ],
        valid_until: synthetic::T + 3600,
    };
    let reference = scope.evidence.clone();
    let statement = serde_json::to_string(&Statement {
        domain: "ROLVYA-SYNTHETIC-PROVIDER-TEST-V1".into(),
        scope,
        identity,
        not_before: synthetic::T,
        expires_at: synthetic::T + 3600,
    })
    .unwrap();
    let signature = B64.encode(key.sign(statement.as_bytes()).as_ref());
    Attestation {
        reference,
        statement,
        signature,
    }
}
fn signing_key() -> Ed25519KeyPair {
    let pkcs = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
    Ed25519KeyPair::from_pkcs8(pkcs.as_ref()).unwrap()
}
struct Certificate {
    cert: CertificateDer<'static>,
    key: Vec<u8>,
}
impl Certificate {
    fn new(name: &str, expired: bool) -> Self {
        let mut p = rcgen::CertificateParams::new(vec![name.into()]).unwrap();
        if expired {
            p.not_before = rcgen::date_time_ymd(2020, 1, 1);
            p.not_after = rcgen::date_time_ymd(2021, 1, 1);
        }
        let k = rcgen::KeyPair::generate().unwrap();
        let c = p.self_signed(&k).unwrap();
        Self {
            cert: c.der().clone(),
            key: k.serialize_der(),
        }
    }
    fn server(&self) -> tls::ServerIdentity {
        tls::ServerIdentity::fixture(
            vec![self.cert.clone()],
            PrivatePkcs8KeyDer::from(self.key.clone()).into(),
        )
    }
    fn client(&self, endpoint: &str) -> WorkerHttpClient {
        WorkerHttpClient::with_tls(
            ControllerEndpoint::parse(endpoint).unwrap(),
            tls::fixture_client(self.cert.clone()),
        )
        .unwrap()
    }
}
type Policy = provider_accounts::RegistryPolicy<synthetic::Authority>;
struct Fixture {
    root: tempfile::TempDir,
    bootstrap: ControllerBootstrap,
    store: PlanStore,
    clock: Arc<DeterministicClock>,
    policy: Arc<Policy>,
    co: SyntheticCoordinator,
    key: Ed25519KeyPair,
    cert: Certificate,
}
impl Fixture {
    async fn new() -> Self {
        let root = temporary();
        for p in ["data", "keys"] {
            fs::create_dir(root.path().join(p)).unwrap();
            fs::set_permissions(root.path().join(p), fs::Permissions::from_mode(0o700)).unwrap();
        }
        let store = PlanStore::open(&root.path().join("data/state.sqlite"))
            .await
            .unwrap();
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
                &InitializationRequest::new(RequestId::new("https-fixture-setup").unwrap()),
                &Setup,
            )
            .await
            .unwrap();
        let mut p = common::plan();
        let mut g = common::group("group-g", &p.key);
        g.acceptance_criteria = vec![GROUP_CRITERION.into()];
        let mut a = common::task("task-a", &p.key);
        a.parent = Some(common::task_ref(&g));
        a.acceptance_criteria = vec![ARTIFACT_CRITERION.into()];
        let mut b = a.clone();
        b.id = TaskId::new("task-b").unwrap();
        let mut c = common::task("task-c", &p.key);
        c.dependencies = vec![common::task_ref(&g)];
        c.acceptance_criteria = vec![ARTIFACT_CRITERION.into(), INPUTS_CRITERION.into()];
        p.tasks = vec![g, a, b, c];
        let valid = rolvya_plan_validator::validate(
            &p,
            &synthetic::Authority::default(),
            Limits::default(),
        )
        .unwrap();
        store
            .create_project(&p.key.project_id, &common::owner())
            .await
            .unwrap();
        let rev = store
            .save_draft(&valid, None, &common::owner(), &common::OwnerPolicy)
            .await
            .unwrap();
        store
            .activate(
                &rev,
                &common::request(),
                &common::owner(),
                &common::OwnerPolicy,
                &synthetic::Authority::default(),
                Limits::default(),
            )
            .await
            .unwrap();
        store
            .apply_financial(
                &FinancialRequest {
                    project: p.key.project_id.clone(),
                    request_id: RequestId::new("https-budget").unwrap(),
                    actor: common::owner(),
                    reason: "Synthetic TLS workload only; no provider charge".into(),
                    command: FinancialCommand::Approve {
                        expected_version: 0,
                        new_ceiling: synthetic::money(100),
                        proof: OwnerProof {
                            reference: synthetic::e("synthetic-budget-proof"),
                            verified_at: SystemClock.now().unwrap(),
                        },
                    },
                },
                &synthetic::Finance,
            )
            .await
            .unwrap();
        let registry = provider_accounts::fixture_registry(&bootstrap, synthetic::T).await;
        let policy = Arc::new(registry.policy(synthetic::Authority::default()));
        let clock = Arc::new(DeterministicClock::new(synthetic::T).unwrap());
        let co = SyntheticCoordinator::open(
            store.clone(),
            &p.key.project_id,
            policy.as_ref(),
            Limits::default(),
        )
        .await
        .unwrap();
        let pass = co
            .pass(
                policy.as_ref(),
                clock.as_ref(),
                PassLimits {
                    admissions: 2,
                    events: 0,
                    validations: 0,
                    groups: 0,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            pass.admitted.len(),
            2,
            "both independent leaves admitted before either completes"
        );
        Self {
            root,
            bootstrap,
            store,
            clock,
            policy,
            co,
            key: signing_key(),
            cert: Certificate::new("localhost", false),
        }
    }
    async fn server(&self) -> (Arc<HttpsWorkerServer>, Arc<SignedProvider>) {
        self.server_limits(ResourceLimits::default()).await
    }
    async fn server_limits(
        &self,
        limits: ResourceLimits,
    ) -> (Arc<HttpsWorkerServer>, Arc<SignedProvider>) {
        let mut security = WorkerSecurity::open(&self.bootstrap).await.unwrap();
        security.policy = self.policy.clone();
        security.clock = self.clock.clone();
        let transport = context::HttpTransport::new(limits, self.clock.clone());
        let verifier = Arc::new(SignedProvider {
            public: self.key.public_key().as_ref().to_vec(),
            transport: transport.clone(),
            prepare: Mutex::new(BTreeMap::new()),
            pause: Mutex::new(None),
        });
        security.identity = verifier.clone();
        security.transport = transport.clone();
        (
            Arc::new(HttpsWorkerServer {
                security,
                transport,
                identity: self.cert.server(),
            }),
            verifier,
        )
    }
    async fn lease(&self, task: &str) -> LeaseRef {
        let row=sqlx::query("SELECT attempt_id,epoch FROM task_attempts WHERE task_id=? ORDER BY epoch DESC LIMIT 1").bind(task).fetch_one(&self.store.pool).await.unwrap();
        LeaseRef {
            task: TaskRef {
                plan: self.co.plan_key().clone(),
                task_id: TaskId::new(task).unwrap(),
            },
            attempt_id: AttemptId::new(row.get::<String, _>(0)).unwrap(),
            epoch: row.get::<i64, _>(1) as u64,
        }
    }
    async fn prepare(
        &self,
        s: &HttpsWorkerServer,
        v: &SignedProvider,
        task: &str,
    ) -> (
        BootstrapTicket,
        BootstrapSecret,
        Attestation,
        WorkerBinding,
        RequestId,
    ) {
        let lease = self.lease(task).await;
        let r = PrepareBootstrap {
            lease: lease.clone(),
            request_id: RequestId::new(format!("prepare-{task}")).unwrap(),
            evidence: synthetic::e("signed-prepare"),
        };
        let mut tx = self.store.pool.begin().await.unwrap();
        let scope = s
            .security
            .scope(
                &mut tx,
                &lease,
                self.clock.now().unwrap(),
                worker_control::CredentialScopeUse::Bootstrap,
            )
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let controller = s.security.controller();
        let check = IdentityCheck {
            controller: &controller,
            stage: VerificationStage::Prepare,
            request: &r.request_id,
            evidence: &r.evidence,
            binding: &scope.binding,
            challenge: None,
            credential: None,
            provider: &scope.provider,
            account: &scope.account,
            pool: &scope.pool,
            command_digest: &scope.command_digest,
        };
        v.prepare
            .lock()
            .unwrap()
            .insert(r.request_id.as_str().into(), sign(&self.key, check.scope()));
        let ticket = s.security.prepare_bootstrap(&r).await.unwrap();
        s.transport
            .bootstrap_destinations
            .lock()
            .unwrap()
            .insert(r.request_id.as_str().into(), (r.clone(), None));
        s.security.deliver_bootstrap(&r, &ticket).await.unwrap();
        let raw = s
            .transport
            .bootstrap_destinations
            .lock()
            .unwrap()
            .remove(r.request_id.as_str())
            .unwrap()
            .1
            .unwrap();
        let secret =
            BootstrapSecret::parse(std::str::from_utf8(raw.as_ref()).unwrap().to_owned()).unwrap();
        let id = RequestId::new(format!("claim-{task}")).unwrap();
        let evidence = synthetic::e("signed-bootstrap");
        let mut signed = check.scope();
        signed.stage = VerificationStage::Bootstrap;
        signed.request = id.clone();
        signed.evidence = evidence;
        signed.challenge = Some(ticket.id.clone());
        (ticket, secret, sign(&self.key, signed), scope.binding, id)
    }
}
async fn listen(
    s: Arc<HttpsWorkerServer>,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (stop, stopped) = oneshot::channel();
    let task = tokio::spawn(async move {
        s.serve(listener, async {
            let _ = stopped.await;
        })
        .await
        .unwrap();
    });
    (format!("https://localhost:{port}/"), stop, task)
}
#[tokio::test]
async fn real_tls_bootstrap_assignment_and_durable_worker_receipt() {
    let f = Fixture::new().await;
    let (s, v) = f.server().await;
    let (boot, secret, proof, binding, id) = f.prepare(&s, &v, "task-a").await;
    let (url, stop, task) = listen(s.clone()).await;
    let mut client = f.cert.client(&url);
    client
        .bootstrap(id, boot.id, &secret, proof, &binding)
        .await
        .unwrap();
    let a = client
        .assignment(RequestId::new("assignment-a").unwrap())
        .await
        .unwrap();
    assert_eq!(a.binding, binding);
    let script = FakeWorker::new(a.binding, a.capabilities, a.seed)
        .unwrap()
        .with_inputs(a.inputs)
        .unwrap()
        .script(FakeBehavior::Success, synthetic::T)
        .unwrap();
    for event in script.events {
        f.clock.advance_to(event.at).unwrap();
        if let SimulatedInput::Message(bytes) = event.input {
            let m: WorkerMessage = serde_json::from_slice(&bytes).unwrap();
            let artifact = if matches!(m.body, WorkerBody::SubmitResult { .. }) {
                Some(script.artifact.as_ref().unwrap().bytes.as_slice())
            } else {
                None
            };
            client.message(m, artifact).await.unwrap();
        }
    }
    assert_eq!(
        f.co.snapshot().await.unwrap().accepted,
        0,
        "HTTP result is not acceptance"
    );
    assert_eq!(f.co.snapshot().await.unwrap().received_unvalidated, 1);
    f.co.validator()
        .validate_leaf(
            &f.lease("task-a").await,
            f.policy.as_ref(),
            f.clock.as_ref(),
        )
        .await
        .unwrap();
    assert_eq!(f.co.snapshot().await.unwrap().accepted, 1);
    assert!(s.diagnostics().accepted >= 8);
    stop.send(()).unwrap();
    task.await.unwrap();
}

#[path = "tests_process.rs"]
mod process;

#[path = "tests_adversarial.rs"]
mod adversarial;
