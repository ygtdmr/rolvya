//! Real controller authority and SDK protocols; AWS IAM facts are private fixtures.
use super::*;
use crate::controller_keys::worker_auth::aws_fargate::{
    self as aws,
    execution::{
        self as run,
        verifier::{self as fv, VerifierConfiguration},
    },
};
use crate::worker_control;
fn envelope(now: i64, verifier: bool) -> run::ExposureEnvelope {
    let rate = run::UnitRate {
        charge: synthetic::money(0),
        units: 1,
    };
    run::ExposureEnvelope {
        format: 1,
        evidence: synthetic::e("private-bounded-verifier-fixture-not-real-pricing"),
        revision: 1,
        collected_at: now,
        valid_until: now + 600,
        runtime_seconds: 60,
        provisioning_seconds: 30,
        termination_seconds: 30,
        cpu_unit_second: rate,
        memory_mib_second: rate,
        extra_gib_second: rate,
        fixed: rolvya_domain::money::CostBreakdown {
            ai_api: synthetic::money(if verifier { 0 } else { 1 }),
            compute: synthetic::money(1),
            storage: synthetic::money(if verifier { 1 } else { 2 }),
            transfer: synthetic::money(if verifier { 0 } else { 1 }),
        },
    }
}
pub(super) async fn prepare(f: &Fixture, l: &LeaseRef) -> fv::fixture::Execution {
    let now = f.clock.now().unwrap();
    let mut tx = f.store.pool.begin().await.unwrap();
    let (cmd, _, _) = worker_control::command(&mut tx, l).await.unwrap();
    tx.commit().await.unwrap();
    let policy = Arc::new(f.policy.fixture_reuse(synthetic::Authority {
        fargate: true,
        ..Default::default()
    }));
    // The coding execution remains the existing no-cost R08 synthetic worker.
    // Its committed AWS configuration/cost envelope reserves its share; ONLY
    // the independent verifier's RunTask SDK protocol is exercised below.
    let mut coding_workload = aws::fixture_workload();
    let suffix = if l.task.task_id.as_str() == "task-a" {
        "a"
    } else {
        "b"
    };
    coding_workload.task_role =
        aws::RoleArn::parse(&format!("arn:aws:iam::123456789012:role/coding-{suffix}")).unwrap();
    let coding = fv::fixture::open(
        &f.bootstrap,
        policy.clone(),
        f.clock.clone(),
        (
            cmd.binding().clone(),
            synthetic::e(if suffix == "a" {
                "synthetic-account"
            } else {
                "synthetic-account-b"
            }),
        ),
        f.root.path(),
        &format!("coding-{suffix}"),
        Some(coding_workload.clone()),
    )
    .await;
    coding
        .adapter
        .prepare(
            l,
            &coding.expected.account_ref,
            &coding.allocation(now),
            &envelope(now, false),
        )
        .await
        .unwrap();
    let p = &f.contexts[&l.task.task_id].verification;
    let config = VerifierConfiguration {
        executables: p
            .commands
            .iter()
            .map(|a| {
                (
                    a[0].clone(),
                    commitment_bytes(&std::fs::read(&a[0]).unwrap()),
                )
            })
            .collect(),
    };
    let mut w = coding_workload;
    w.task_definition = aws::TaskDefinitionArn::parse(&format!(
        "arn:aws:ecs:eu-central-1:123456789012:task-definition/verifier-{suffix}:1"
    ))
    .unwrap();
    w.task_role =
        aws::RoleArn::parse(&format!("arn:aws:iam::123456789012:role/verifier-{suffix}")).unwrap();
    w.execution_role = aws::RoleArn::parse(&format!(
        "arn:aws:iam::123456789012:role/verifier-execution-{suffix}"
    ))
    .unwrap();
    w.image_digest = format!(
        "sha256:{}",
        if suffix == "a" {
            "a".repeat(64)
        } else {
            "b".repeat(64)
        }
    );
    w.image = format!("example.invalid/verifier@{}", w.image_digest);
    w.command = vec![
        "/opt/rolvya/verifier".into(),
        "--policy".into(),
        commitment(p).unwrap().as_str().into(),
        "--executables".into(),
        commitment(&config.executables).unwrap().as_str().into(),
    ];
    let mut verifier = fv::fixture::open(
        &f.bootstrap,
        policy,
        f.clock.clone(),
        (
            cmd.binding().clone(),
            synthetic::e(&format!("verifier-account-{suffix}")),
        ),
        f.root.path(),
        &format!("verifier-{suffix}"),
        Some(w),
    )
    .await;
    let co = f
        .bootstrap
        .coding_coordinator(&f.project, &f.policy, Limits::default())
        .await
        .unwrap();
    verifier.configure(co, config);
    if let Ok(point) = std::env::var("ROLVYA_G104_CRASH") {
        verifier.kill_at(&point);
    }
    verifier
        .adapter
        .prepare(
            l,
            &verifier.expected.account_ref,
            &verifier.allocation(now),
            &envelope(now, true),
        )
        .await
        .unwrap();
    verifier
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fargate_verifier_scope_budget_and_delivery_fail_closed() {
    let f = Fixture::new_coding_fargate().await;
    let leases = f.coding_admit().await;
    let r = f.coding_result(&leases[0]).await;
    f.coding_receive(&leases[0], &r).await;
    let mut v = prepare(&f, &leases[0]).await;
    let before = f.store.financial_snapshot(&f.project).await.unwrap();
    assert!(v.adapter.authorize_verifier(&leases[0]).await.is_err());
    v.adapter
        .launch_verifier(&leases[0], f.clock.as_ref())
        .await
        .unwrap();
    assert_eq!(v.effects().await, 1);
    v.adapter
        .launch_verifier(&leases[0], f.clock.as_ref())
        .await
        .unwrap();
    assert_eq!(v.effects().await, 1);
    v.distribution(&leases[0]).await;
    let g = v.adapter.authorize_verifier(&leases[0]).await.unwrap();
    assert!(
        v.credential(&leases[0], f.clock.now().unwrap(), true)
            .await
            .is_err()
    );
    assert!(
        v.credential(&leases[0], f.clock.now().unwrap(), false)
            .await
            .is_ok()
    );
    assert_eq!(g.binding, r.binding);
    assert_eq!(f.count("coding_verifier_executions").await, 1);
    assert_eq!(f.count("task_acceptances").await, 0);
    assert!(f.remote.prepare_coding(&f.co, &leases[0]).await.is_err());
    v.revoke_delivery();
    assert!(v.adapter.authorize_verifier(&leases[0]).await.is_err());
    assert_eq!(
        before.held,
        f.store.financial_snapshot(&f.project).await.unwrap().held
    );
}

async fn received() -> (Fixture, LeaseRef, fv::fixture::Execution) {
    let f = Fixture::new_coding_fargate().await;
    let leases = f.coding_admit().await;
    let r = f.coding_result(&leases[0]).await;
    f.coding_receive(&leases[0], &r).await;
    let v = prepare(&f, &leases[0]).await;
    (f, leases[0].clone(), v)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fargate_verifier_lost_launch_response_never_relaunches_or_releases_holds() {
    let (f, l, v) = received().await;
    let before = f.store.financial_snapshot(&f.project).await.unwrap();
    v.behavior(1);
    let status = v
        .adapter
        .launch_verifier(&l, f.clock.as_ref())
        .await
        .unwrap();
    assert!(status.unknown_exposure);
    assert_eq!(v.effects().await, 1);
    for _ in 0..2 {
        let _ = v.adapter.launch_verifier(&l, f.clock.as_ref()).await;
    }
    assert_eq!(v.effects().await, 1);
    assert!(v.adapter.authorize_verifier(&l).await.is_err());
    assert_eq!(
        before.held,
        f.store.financial_snapshot(&f.project).await.unwrap().held
    );
    assert_eq!(f.count("coding_verifier_executions").await, 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fargate_verifier_shared_role_and_sdk_configuration_substitutions_cannot_deliver() {
    for mode in [8, 9, 10] {
        let (f, l, v) = received().await;
        v.behavior(mode);
        assert!(
            v.adapter
                .launch_verifier(&l, f.clock.as_ref())
                .await
                .is_err()
        );
        assert_eq!(v.effects().await, 0);
        assert_eq!(f.count("coding_verifier_executions").await, 0);
    }
    let (f, l, mut v) = received().await;
    v.adapter
        .launch_verifier(&l, f.clock.as_ref())
        .await
        .unwrap();
    v.distribution(&l).await;
    v.shared_role();
    assert!(v.adapter.authorize_verifier(&l).await.is_err());
    assert_eq!(f.count("coding_verifier_executions").await, 0);
}
async fn network(
    f: &Fixture,
) -> (
    crate::controller_keys::worker_auth::https::tests::CodingNetwork,
    String,
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let co = f
        .bootstrap
        .coding_coordinator(&f.project, &f.policy, Limits::default())
        .await
        .unwrap();
    let net = crate::controller_keys::worker_auth::https::tests::CodingNetwork::new(
        &f.bootstrap,
        Arc::new(f.policy.fixture_reuse(synthetic::Authority {
            fargate: true,
            ..Default::default()
        })),
        f.clock.clone(),
        co,
    )
    .await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "https://localhost:{}/",
        listener.local_addr().unwrap().port()
    );
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = net.server.clone();
    let task = tokio::spawn(async move {
        server
            .serve(listener, async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
    });
    (net, url, stop, task)
}
async fn client(
    f: &Fixture,
    l: &LeaseRef,
    v: &fv::fixture::Execution,
    net: &crate::controller_keys::worker_auth::https::tests::CodingNetwork,
    url: &str,
) -> crate::controller_keys::worker_auth::https::verifier::VerifierHttpClient {
    v.credential(l, f.clock.now().unwrap(), false)
        .await
        .unwrap()
        .fixture_tls(
            crate::controller_keys::worker_auth::https::client::ControllerEndpoint::parse(url)
                .unwrap(),
            crate::controller_keys::worker_auth::https::tls::fixture_client(
                rustls::pki_types::CertificateDer::from(net.ca.clone()),
            ),
        )
        .unwrap()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fargate_verifier_historical_acceptance_survives_normal_termination() {
    use crate::controller_keys::worker_auth::https::verifier::RemoteExecution;
    let (f, l, mut v) = received().await;
    v.adapter
        .launch_verifier(&l, f.clock.as_ref())
        .await
        .unwrap();
    v.distribution(&l).await;
    v.adapter.authorize_verifier(&l).await.unwrap();
    let (net, url, stop, task) = network(&f).await;
    let client = client(&f, &l, &v, &net, &url).await;
    let inputs = client.inputs(f.clock.now().unwrap()).await.unwrap();
    let root = tempfile::tempdir().unwrap();
    let report = client
        .execute(
            &inputs,
            RemoteExecution::fixture(
                root.path().join("candidate"),
                &inputs,
                f.clock.now().unwrap(),
            ),
        )
        .await
        .unwrap();
    f.clock.advance_to(report.report.completed_at).unwrap();
    client.submit(report).await.unwrap();
    f.co.accept_coding_result(&l, &f.policy, f.clock.as_ref())
        .await
        .unwrap();
    v.stopped();
    v.adapter.reconcile(&l).await.unwrap();
    assert!(
        f.co.accept_coding_result(&l, &f.policy, f.clock.as_ref())
            .await
            .is_ok(),
        "normal verifier retirement must not invalidate accepted history"
    );
    assert!(
        f.remote.prepare_coding(&f.co, &l).await.is_ok(),
        "accepted historical evidence must remain publishable"
    );
    assert_eq!(f.count("task_acceptances").await, 1);
    stop.send(()).unwrap();
    task.await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fargate_verifier_lifecycle_inspection_cannot_refresh_expired_iam_authority() {
    let (f, l, mut v) = received().await;
    v.adapter
        .launch_verifier(&l, f.clock.as_ref())
        .await
        .unwrap();
    v.distribution(&l).await;
    v.adapter.authorize_verifier(&l).await.unwrap();
    let (net, url, stop, task) = network(&f).await;
    let client = client(&f, &l, &v, &net, &url).await;
    client.inputs(f.clock.now().unwrap()).await.unwrap();
    v.revoke_delivery();
    f.clock.advance_to(f.clock.now().unwrap() + 15).unwrap();
    v.adapter.reconcile(&l).await.unwrap();
    assert!(
        client.inputs(f.clock.now().unwrap()).await.is_err(),
        "lifecycle inspection cannot renew expired or revoked delivery proof"
    );
    assert_eq!(f.count("task_acceptances").await, 0);
    stop.send(()).unwrap();
    task.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fargate_verifier_failed_submission_closes_unknown_financial_authority() {
    let (f, l, v) = received().await;
    v.behavior(8);
    assert!(
        v.adapter
            .launch_verifier(&l, f.clock.as_ref())
            .await
            .is_err()
    );
    assert!(
        f.store
            .financial_snapshot(&f.project)
            .await
            .unwrap()
            .uncertain_liabilities
            > 0,
        "durable uncertain verifier submission must propagate to the existing financial gate"
    );
    assert_eq!(v.effects().await, 0);
    assert_eq!(f.count("task_acceptances").await, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fargate_verifier_current_account_revocation_and_tls_replay_deny_pending_authority() {
    let (f, l, mut v) = received().await;
    v.adapter
        .launch_verifier(&l, f.clock.as_ref())
        .await
        .unwrap();
    v.distribution(&l).await;
    v.adapter.authorize_verifier(&l).await.unwrap();
    let (net, url, stop, task) = network(&f).await;
    let client = client(&f, &l, &v, &net, &url).await;
    client.inputs(f.clock.now().unwrap()).await.unwrap();
    assert!(client.fixture_wrong_tls().await.is_err());
    v.revoke_account().await;
    assert!(client.inputs(f.clock.now().unwrap()).await.is_err());
    assert_eq!(f.count("coding_verifier_reports").await, 0);
    assert_eq!(f.count("task_acceptances").await, 0);
    stop.send(()).unwrap();
    task.await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fargate_verifier_concurrent_launches_and_ignored_grants_preserve_single_effect() {
    let (f, l, v) = received().await;
    let (_a, _b) = tokio::join!(
        v.adapter.launch_verifier(&l, f.clock.as_ref()),
        v.adapter.launch_verifier(&l, f.clock.as_ref())
    );
    assert!(v.effects().await <= 1);
    assert_eq!(f.count("coding_verifier_executions").await, 0);
    // Use a separate eligible fixture so conservative uncertainty from competing
    // submissions cannot be bypassed for the liveness side of this assertion.
    let (f, l, mut v) = received().await;
    v.adapter
        .launch_verifier(&l, f.clock.as_ref())
        .await
        .unwrap();
    v.distribution(&l).await;
    sqlx::query("CREATE TRIGGER reject_verifier_grant BEFORE INSERT ON coding_verifier_executions BEGIN SELECT RAISE(IGNORE); END").execute(&f.store.pool).await.unwrap();
    assert!(v.adapter.authorize_verifier(&l).await.is_err());
    assert_eq!(f.count("coding_verifier_executions").await, 0);
    assert_eq!(f.count("task_acceptances").await, 0);
    sqlx::query("DROP TRIGGER reject_verifier_grant")
        .execute(&f.store.pool)
        .await
        .unwrap();
    let first = v.adapter.authorize_verifier(&l).await.unwrap();
    let replay = v.adapter.authorize_verifier(&l).await.unwrap();
    assert_eq!(first, replay);
    assert_eq!(v.effects().await, 1);
    assert_eq!(f.count("coding_verifier_executions").await, 1);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fargate_verifier_configuration_cost_and_protected_key_tampering_fail_closed() {
    let (f, l, v) = received().await;
    let r = v.request(&l).await;
    let now = f.clock.now().unwrap();
    r.validate(now).unwrap();
    for i in 0..8 {
        let mut bad = r.clone();
        match i {
            0 => bad.bootstrap.task_role = bad.workload.execution_role.clone(),
            1 => bad.workload.image = "example.invalid/unpinned:latest".into(),
            2 => bad.maximum.compute = synthetic::money(0),
            3 => bad
                .verifier
                .as_mut()
                .unwrap()
                .grant
                .public_key
                .pop()
                .map(|_| ())
                .unwrap(),
            4 => bad.verifier.as_mut().unwrap().grant.work = Digest::new("c".repeat(64)).unwrap(),
            5 => bad.binding.epoch += 1,
            6 => {
                bad.workload.cluster =
                    aws::ClusterArn::parse("arn:aws:ecs:eu-west-1:123456789012:cluster/other")
                        .unwrap()
            }
            _ => bad.verifier.as_mut().unwrap().coding_maximum.compute = synthetic::money(i64::MAX),
        };
        assert!(bad.validate(now).is_err(), "substitution {i}");
    }
    let key = f.bootstrap.load_existing().await.unwrap();
    let plain = b"recognizable synthetic verifier credential without live authority";
    let protected = crate::controller_keys::worker_auth::aws_fargate::fixture_seal_verifier(
        &key,
        &r.digest().unwrap(),
        plain,
    );
    assert_eq!(
        &*crate::controller_keys::worker_auth::aws_fargate::fixture_open_verifier(
            &key,
            &r.digest().unwrap(),
            &protected
        )
        .unwrap(),
        plain
    );
    assert!(
        crate::controller_keys::worker_auth::aws_fargate::fixture_open_verifier(
            &key,
            &"c".repeat(64),
            &protected
        )
        .is_err()
    );
    let other = Fixture::new_coding_fargate().await;
    assert!(
        crate::controller_keys::worker_auth::aws_fargate::fixture_open_verifier(
            &other.bootstrap.load_existing().await.unwrap(),
            &r.digest().unwrap(),
            &protected
        )
        .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fargate_verifier_absolute_delivery_proof_expiry_and_explicit_refresh() {
    let (f, l, mut v) = received().await;
    v.adapter
        .launch_verifier(&l, f.clock.as_ref())
        .await
        .unwrap();
    v.distribution(&l).await;
    let pause = v.pause_inspection();
    let now = f.clock.now().unwrap();
    let (grant, ()) = tokio::join!(v.adapter.authorize_verifier(&l), async {
        pause.0.notified().await;
        f.clock.advance_to(now + 14).unwrap();
        pause.1.notify_one();
    });
    let first = grant.unwrap();
    let (net, url, stop, task) = network(&f).await;
    let client = client(&f, &l, &v, &net, &url).await;
    client.inputs(f.clock.now().unwrap()).await.unwrap();
    f.clock.advance_to(now + 15).unwrap();
    assert!(
        client.inputs(f.clock.now().unwrap()).await.is_err(),
        "late commit cannot restart proof lifetime"
    );
    let refreshed = v.adapter.authorize_verifier(&l).await.unwrap();
    assert_eq!(first, refreshed);
    client.inputs(f.clock.now().unwrap()).await.unwrap();
    assert_eq!(v.effects().await, 1);
    assert_eq!(f.count("coding_verifier_executions").await, 1);
    stop.send(()).unwrap();
    task.await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fargate_verifier_process_crash_driver() {
    let Ok(point) = std::env::var("ROLVYA_G104_CRASH") else {
        return;
    };
    let f = Fixture::new_coding_fargate().await;
    let leases = f.coding_admit().await;
    let sibling = f.coding_result(&leases[1]).await;
    f.coding_receive(&leases[1], &sibling).await;
    f.coding_verify(&leases[1], &sibling).await;
    f.coding_accept(&leases[1]).await;
    let result = f.coding_result(&leases[0]).await;
    f.coding_receive(&leases[0], &result).await;
    let report = std::env::var("ROLVYA_G104_CRASH_REPORT").unwrap();
    std::fs::write(report,serde_json::to_vec(&json!({"root":f.root.path(),"lease":leases[0],"project":f.project,"fingerprint":f.co.fingerprint(),"held":f.store.financial_snapshot(&f.project).await.unwrap().held})).unwrap()).unwrap();
    let mut v = prepare(&f, &leases[0]).await;
    f.store
        .snapshot(f.root.path().join("data/before-external.sqlite"))
        .await
        .unwrap();
    v.adapter
        .launch_verifier(&leases[0], f.clock.as_ref())
        .await
        .unwrap();
    v.distribution(&leases[0]).await;
    v.adapter.authorize_verifier(&leases[0]).await.unwrap();
    panic!("unreached kill checkpoint {point}");
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fargate_verifier_sigkill_preparation_effect_receipt_and_grant_reopen() {
    use std::os::unix::process::ExitStatusExt;
    for point in [
        "prepare-before",
        "prepare-after",
        "submit-after",
        "effect-after",
        "receipt-before",
        "receipt-after",
        "grant-before",
        "grant-after",
    ] {
        let report = tempfile::NamedTempFile::new().unwrap();
        let child=tokio::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","controller_keys::github_app::tests::coding::fargate::fargate_verifier_process_crash_driver","--nocapture"]).env("ROLVYA_G104_CRASH",point).env("ROLVYA_G104_CRASH_REPORT",report.path()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::inherit()).kill_on_drop(true).spawn().unwrap();
        let output =
            tokio::time::timeout(std::time::Duration::from_secs(40), child.wait_with_output())
                .await
                .unwrap()
                .unwrap();
        assert_eq!(
            output.status.signal(),
            Some(9),
            "{point}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        let data: serde_json::Value =
            serde_json::from_slice(&std::fs::read(report.path()).unwrap()).unwrap();
        let root = std::path::PathBuf::from(data["root"].as_str().unwrap());
        let store = crate::PlanStore::open(root.join("data/state.sqlite"))
            .await
            .unwrap();
        store.check_integrity().await.unwrap();
        let l: LeaseRef = serde_json::from_value(data["lease"].clone()).unwrap();
        let project = ProjectId::new(data["project"].as_str().unwrap()).unwrap();
        let expected = if point == "prepare-before" { 0 } else { 1 };
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM aws_launch_requests WHERE purpose='verifier'"
            )
            .fetch_one(&store.pool)
            .await
            .unwrap(),
            expected
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM aws_verifier_bootstrap")
                .fetch_one(&store.pool)
                .await
                .unwrap(),
            expected
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM coding_verifier_executions")
                .fetch_one(&store.pool)
                .await
                .unwrap(),
            if point == "grant-after" { 1 } else { 0 }
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM task_acceptances")
                .fetch_one(&store.pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            serde_json::to_value(store.financial_snapshot(&project).await.unwrap().held).unwrap(),
            data["held"]
        );
        let bootstrap = crate::controller_keys::ControllerBootstrap::open(
            store.clone(),
            &root.join("keys"),
            crate::controller_keys::ServiceIdentity::current(),
            &[],
        )
        .await
        .unwrap();
        bootstrap.load_existing().await.unwrap();
        let registry = crate::controller_keys::provider_accounts::fixture_reopen(&bootstrap).await;
        let policy = Arc::new(registry.policy(synthetic::Authority {
            fargate: true,
            ..Default::default()
        }));
        // An ordinary reopen has no fixture activation/clearance. It cannot
        // replay RunTask, mint a replacement grant, or erase external liability.
        let co = bootstrap
            .coding_coordinator(&project, policy.as_ref(), Limits::default())
            .await
            .unwrap();
        let adapter = run::FargateExecution::open_verifier(
            &bootstrap,
            policy,
            co,
            VerifierConfiguration {
                executables: vec![],
            },
        )
        .await
        .unwrap();
        assert!(
            adapter
                .launch_verifier(
                    &l,
                    &crate::worker_simulation::DeterministicClock::new(synthetic::T).unwrap()
                )
                .await
                .is_err()
        );
        if point == "effect-after" {
            let old = crate::PlanStore::open(root.join("data/before-external.sqlite"))
                .await
                .unwrap();
            old.check_integrity().await.unwrap();
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM aws_execution_events WHERE purpose='verifier' AND kind='receipt'").fetch_one(&old.pool).await.unwrap(),0);
            // Snapshot does not roll back the separate simulated AWS effect.
            let external = sqlx::SqlitePool::connect(&format!(
                "sqlite:{}",
                root.join("verifier-a-aws.sqlite").display()
            ))
            .await
            .unwrap();
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT count(*) FROM effects")
                    .fetch_one(&external)
                    .await
                    .unwrap(),
                1
            );
            external.close().await;
            old.close().await;
        }
        store.close().await;
        std::fs::remove_dir_all(root).unwrap();
    }
}
