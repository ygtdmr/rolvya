use super::*;
use crate::resilience::{common, synthetic};
use crate::{PrincipalId, controller_keys::ServiceIdentity};
use crate::{
    admission::*, orchestration::coding::*, orchestration::*, worker_control::*,
    worker_simulation::*,
};
use base64::Engine;
use rolvya_domain::worker::Digest;
use rolvya_domain::{ids::*, plan::*, worker::*};
use rolvya_plan_validator::Limits;
use std::collections::BTreeMap;

use std::sync::atomic::Ordering;

impl Fixture {
    pub async fn coding_admit(&self) -> Vec<LeaseRef> {
        let plan = self
            .store
            .load_active(&self.project, &self.policy, Limits::default())
            .await
            .unwrap();
        let mut leases = vec![];
        for t in plan
            .content
            .tasks
            .iter()
            .filter(|t| self.contexts.contains_key(&t.id))
        {
            self.remote
                .pin_coding_source(
                    &self.co,
                    &TaskRef {
                        plan: t.plan.clone(),
                        task_id: t.id.clone(),
                    },
                    &self.contexts[&t.id],
                )
                .await
                .unwrap();
            let assignment = self
                .policy
                .assignment(t, 1, self.clock.now().unwrap())
                .unwrap();
            let admitted = self
                .store
                .admit(
                    &AdmissionRequest {
                        task: TaskRef {
                            plan: t.plan.clone(),
                            task_id: t.id.clone(),
                        },
                        fingerprint: self.co.fingerprint().into(),
                        expected_epoch: 0,
                        request_id: crate::RequestId::new(format!(
                            "coding-admit-{}",
                            t.id.as_str()
                        ))
                        .unwrap(),
                        actor: common::owner(),
                        permit: assignment.permit,
                    },
                    &self.policy,
                    self.clock.as_ref(),
                    Limits::default(),
                )
                .await
                .unwrap();
            let dispatcher = OutboxDispatcher::new(self.store.clone());
            let ticket = dispatcher
                .claim(
                    &admitted.lease,
                    &common::owner(),
                    &self.policy,
                    self.clock.as_ref(),
                    Limits::default(),
                )
                .await
                .unwrap();
            assert_eq!(
                dispatcher
                    .dispatch(
                        &ticket,
                        &SyntheticProvider::new(self.store.clone()),
                        &self.policy,
                        self.clock.as_ref(),
                        Limits::default()
                    )
                    .await
                    .unwrap(),
                DispatchState::Active
            );
            leases.push(admitted.lease);
        }
        leases
    }
    pub async fn coding_result(&self, l: &LeaseRef) -> CodingResult {
        let ctx = &self.contexts[&l.task.task_id];
        let source = self
            .remote
            .read_pinned(&self.project, &ctx.base_commit)
            .await
            .unwrap();
        let change = if l.task.task_id.as_str() == "task-a" {
            vec![
                FileChange::Modify {
                    path: "src/alpha.rs".into(),
                    original_sha256: commitment_bytes(
                        &source
                            .files
                            .iter()
                            .find(|f| f.path == "src/alpha.rs")
                            .unwrap()
                            .bytes,
                    ),
                    content: "pub fn alpha()->u8{11}\r\n".into(),
                },
                FileChange::Add {
                    path: "src/added.rs".into(),
                    content: "pub const ADDED:u8=4;\n".into(),
                },
                FileChange::Delete {
                    path: "src/obsolete.rs".into(),
                    original_sha256: commitment_bytes(
                        &source
                            .files
                            .iter()
                            .find(|f| f.path == "src/obsolete.rs")
                            .unwrap()
                            .bytes,
                    ),
                },
            ]
        } else {
            let p = &ctx.paths[0];
            vec![FileChange::Modify {
                path: p.clone(),
                original_sha256: commitment_bytes(
                    &source.files.iter().find(|f| &f.path == p).unwrap().bytes,
                ),
                content: "pub fn beta()->u8{22}\n".into(),
            }]
        };
        let cmd: String = sqlx::query_scalar(
            "SELECT command FROM dispatch_operations WHERE project_id=? AND attempt_id=?",
        )
        .bind(self.project.as_str())
        .bind(l.attempt_id.as_str())
        .fetch_one(&self.store.pool)
        .await
        .unwrap();
        let cmd: LaunchCommand = serde_json::from_str(&cmd).unwrap();
        CodingResult {
            format: CodingFormat::V1,
            installation: ctx.installation.clone(),
            binding: cmd.binding().clone(),
            repository: ctx.repository,
            base_commit: ctx.base_commit.clone(),
            base_tree: ctx.base_tree.clone(),
            source_sha256: ctx.source_sha256.clone(),
            inputs: vec![],
            financial_event: cmd.financial_event(),
            operation: Digest::new(cmd.digest().unwrap()).unwrap(),
            resulting_tree_sha256: tree_commitment(
                &apply_changes(&source, &change, &ctx.paths).unwrap(),
            )
            .unwrap(),
            changes: change,
            worker_execution: format!("private-synthetic-worker-{}", l.task.task_id.as_str()),
            verification_execution: format!("private-synthetic-run-{}", l.task.task_id.as_str()),
        }
    }
    pub fn coding_report(&self, r: &CodingResult) -> VerificationReport {
        let p = self.contexts[&r.binding.task.task_id].verification.clone();
        let raw = serde_json::to_vec(r).unwrap();
        VerificationReport {
            format: CodingFormat::V1,
            binding: r.binding.clone(),
            artifact_sha256: commitment_bytes(&raw),
            source_sha256: r.source_sha256.clone(),
            change_set_sha256: commitment(&r.changes).unwrap(),
            policy_sha256: commitment(&p).unwrap(),
            verifier: "private-synthetic-verifier".into(),
            verifier_version: 1,
            environment: "private-synthetic-isolated-environment-fixture".into(),
            execution: r.verification_execution.clone(),
            started_at: self.clock.now().unwrap(),
            completed_at: self.clock.now().unwrap(),
            terminal: VerificationTerminal::Completed,
            exit_code: 0,
            tests_completed: p.required_tests.clone(),
            outputs: vec![VerificationOutput {
                identity: "synthetic-test-output".into(),
                content: "fixture completed declared tests; no source executed\n".into(),
                sha256: commitment_bytes(b"fixture completed declared tests; no source executed\n"),
            }],
            limits: p,
        }
    }
    pub async fn coding_receive(&self, l: &LeaseRef, result: &CodingResult) {
        self.coding_receive_many(&[(l, result)]).await;
    }
    pub async fn coding_receive_many(&self, pairs: &[(&LeaseRef, &CodingResult)]) {
        let mut scripts = Vec::new();
        for (l, r) in pairs {
            let raw = serde_json::to_vec(r).unwrap();
            self.co
                .submit_coding_artifact(l, &raw, self.clock.as_ref())
                .await
                .unwrap();
            let script = FakeWorker::new(r.binding.clone(), synthetic::caps(), 1)
                .unwrap()
                .script(FakeBehavior::Success, self.clock.now().unwrap())
                .unwrap();
            let messages: Vec<_> = script
                .events
                .into_iter()
                .filter_map(|event| match event.input {
                    SimulatedInput::Message(bytes) => Some(bytes),
                    _ => None,
                })
                .map(|bytes| {
                    let mut m = decode_worker_message(&bytes).unwrap();
                    if matches!(m.body, WorkerBody::SubmitResult { .. }) {
                        m.body = WorkerBody::SubmitResult {
                            outcome: ExecutionOutcome::Succeeded {
                                manifest_sha256: commitment_bytes(&raw),
                            },
                        };
                    }
                    encode_worker_message(&m).unwrap()
                })
                .collect();
            scripts.push((r.binding.clone(), messages));
        }
        for index in 0..scripts.iter().map(|(_, m)| m.len()).max().unwrap_or(0) {
            for (binding, messages) in &scripts {
                if let Some(message) = messages.get(index) {
                    DurableWorkerController::new(self.store.clone())
                        .receive(
                            &self.policy.peer(binding),
                            message,
                            &self.policy,
                            &self.policy,
                            self.clock.as_ref(),
                            Limits::default(),
                        )
                        .await
                        .unwrap();
                }
            }
        }
    }
    pub async fn coding_verify(&self, l: &LeaseRef, r: &CodingResult) {
        let report = self.coding_report(r);
        self.co.fixture_coding_run(report.clone());
        self.co
            .verify_coding_execution(l, &report, self.clock.as_ref())
            .await
            .unwrap();
    }
    pub async fn coding_accept(&self, l: &LeaseRef) {
        assert!(matches!(
            self.co
                .accept_coding_result(l, &self.policy, self.clock.as_ref())
                .await
                .unwrap(),
            CodingState::Accepted(_)
        ));
    }
}
fn commitment_bytes(b: &[u8]) -> Digest {
    Digest::new(hash(b)).unwrap()
}
async fn published_files(f: &Fixture, receipt: &PublicationReceipt) -> BTreeMap<String, Vec<u8>> {
    let mut stack = vec![(String::new(), receipt.commit.as_str().to_owned(), true)];
    let mut files = BTreeMap::new();
    while let Some((path, sha, commit)) = stack.pop() {
        let text: String = sqlx::query_scalar("SELECT body FROM objects WHERE sha=?")
            .bind(&sha)
            .fetch_one(&f.world.db)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        if commit {
            stack.push((path, v["tree"]["sha"].as_str().unwrap().into(), false));
            continue;
        }
        if let Some(entries) = v["tree"].as_array() {
            for e in entries {
                let name = format!("{path}{}", e["path"].as_str().unwrap());
                let sha = e["sha"].as_str().unwrap().to_owned();
                if e["type"] == "tree" {
                    stack.push((format!("{name}/"), sha, false));
                } else {
                    let text: String = sqlx::query_scalar("SELECT body FROM objects WHERE sha=?")
                        .bind(sha)
                        .fetch_one(&f.world.db)
                        .await
                        .unwrap();
                    let b: serde_json::Value = serde_json::from_str(&text).unwrap();
                    files.insert(
                        name,
                        base64::engine::general_purpose::STANDARD
                            .decode(b["content"].as_str().unwrap())
                            .unwrap(),
                    );
                }
            }
        }
    }
    files
}
#[tokio::test]
async fn coding_end_to_end_real_r06_r08_r09_multifile_two_refs_group_and_unchanged_dag() {
    let f = Fixture::new_coding().await;
    let fingerprint = f.co.fingerprint().to_owned();
    let leases = f.coding_admit().await;
    assert_eq!(leases.len(), 2);
    let before = f.store.financial_snapshot(&f.project).await.unwrap();
    let a = f.coding_result(&leases[0]).await;
    let b = f.coding_result(&leases[1]).await;
    // Independent workers coexist; delivery is deliberately interleaved by artifact/evidence stage.
    f.coding_receive_many(&[(&leases[0], &a), (&leases[1], &b)])
        .await;
    assert_eq!(f.count("task_acceptances").await, 0);
    assert_eq!(
        f.remote
            .prepare_coding(&f.co, &leases[0])
            .await
            .unwrap_err(),
        GitError::Unaccepted
    );
    let snapshot = f.co.snapshot().await.unwrap();
    assert_eq!(snapshot.waiting_dependencies, 1);
    for (l, r) in leases.iter().zip([&a, &b]) {
        f.coding_verify(l, r).await;
        f.coding_accept(l).await;
    }
    let (pa, pb) = tokio::join!(
        f.remote.prepare_coding(&f.co, &leases[0]),
        f.remote.prepare_coding(&f.co, &leases[1])
    );
    let pa = pa.unwrap();
    let pb = pb.unwrap();
    assert_ne!(pa.reference, pb.reference);
    let (ra, rb) = tokio::join!(
        f.remote.publish(&f.co, &f.project, &pa.operation),
        f.remote.publish(&f.co, &f.project, &pb.operation)
    );
    let ra = ra.unwrap();
    let rb = rb.unwrap();
    assert_eq!(ra.state, PublicationState::Published);
    assert_eq!(rb.state, PublicationState::Published);
    let fa = published_files(&f, &ra).await;
    let fb = published_files(&f, &rb).await;
    let expected = |files: Vec<(&str, &[u8])>| {
        files
            .into_iter()
            .map(|(p, b)| (p.to_owned(), b.to_vec()))
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(
        fa,
        expected(vec![
            ("src/alpha.rs", b"pub fn alpha()->u8{11}\r\n"),
            ("src/added.rs", b"pub const ADDED:u8=4;\n"),
            ("src/beta.rs", b"pub fn beta()->u8{2}\n"),
            ("src/shared.rs", b"pub const SHARED:u8=3;\n"),
            ("tests/alpha_test.rs", b"// alpha test fixture\n"),
            ("tests/beta_test.rs", b"// beta test fixture\n")
        ])
    );
    assert_eq!(
        fb,
        expected(vec![
            ("src/alpha.rs", b"pub fn alpha()->u8{1}\n"),
            ("src/beta.rs", b"pub fn beta()->u8{22}\n"),
            ("src/shared.rs", b"pub const SHARED:u8=3;\n"),
            ("src/obsolete.rs", b"// delete me\n"),
            ("tests/alpha_test.rs", b"// alpha test fixture\n"),
            ("tests/beta_test.rs", b"// beta test fixture\n")
        ])
    );
    assert_eq!(f.world.ref_count().await, 2);
    assert_eq!(f.co.snapshot().await.unwrap().waiting_dependencies, 1);
    let snapshot = f.root.path().join("data/coding-ordinary.sqlite");
    f.store.snapshot(&snapshot).await.unwrap();
    let mut protected: Vec<Vec<u8>> = f
        .world
        .tokens
        .lock()
        .unwrap()
        .keys()
        .map(|s| s.as_bytes().to_vec())
        .collect();
    protected.extend([
        fixture::signing_key().0.clone(),
        base64::engine::general_purpose::STANDARD
            .encode(&fixture::signing_key().0)
            .into_bytes(),
        f.webhook.clone(),
        f.remote.key.material.to_vec(),
    ]);
    for file in [
        f.root.path().join("data/state.sqlite"),
        f.root.path().join("data/state.sqlite-wal"),
        snapshot,
    ] {
        if file.exists() {
            let bytes = std::fs::read(file).unwrap();
            for value in &protected {
                assert!(
                    !bytes.windows(value.len()).any(|w| w == value),
                    "credential material must stay outside ordinary coding persistence"
                );
            }
        }
    }
    let plan = f
        .store
        .load_active(&f.project, &f.policy, Limits::default())
        .await
        .unwrap();
    let c_task = plan
        .content
        .tasks
        .iter()
        .find(|t| t.id.as_str() == "task-c")
        .unwrap();
    let assignment = f
        .policy
        .assignment(c_task, 1, f.clock.now().unwrap())
        .unwrap();
    let blocked = f
        .store
        .admit(
            &AdmissionRequest {
                task: TaskRef {
                    plan: c_task.plan.clone(),
                    task_id: c_task.id.clone(),
                },
                fingerprint: f.co.fingerprint().into(),
                expected_epoch: 0,
                request_id: crate::RequestId::new("coding-c-before-group").unwrap(),
                actor: common::owner(),
                permit: assignment.permit,
            },
            &f.policy,
            f.clock.as_ref(),
            Limits::default(),
        )
        .await;
    assert_eq!(blocked.unwrap_err(), AdmissionError::DependencyIncomplete);
    assert_eq!(f.count("task_attempts").await, 2);
    let group = TaskRef {
        plan: f.co.plan_key().clone(),
        task_id: TaskId::new("group-g").unwrap(),
    };
    f.co.validator()
        .aggregate(&group, &f.policy, f.clock.as_ref())
        .await
        .unwrap();
    f.co.validator().verify_accepted(&group).await.unwrap();
    assert_eq!(f.co.snapshot().await.unwrap().waiting_dependencies, 0);
    assert_eq!(f.count("task_attempts").await, 2);
    assert_eq!(f.co.fingerprint(), fingerprint);
    assert_eq!(
        f.store.financial_snapshot(&f.project).await.unwrap(),
        before
    );
    let group_digest = f.co.validator().verify_accepted(&group).await.unwrap();
    for offset in [0, 1, 2, 30, 42, 50] {
        f.clock.advance_to(NOW + offset).unwrap();
        f.co.pass(
            &f.policy,
            f.clock.as_ref(),
            PassLimits {
                admissions: if offset == 0 { 1 } else { 0 },
                events: 64,
                validations: 16,
                groups: 0,
            },
        )
        .await
        .unwrap();
    }
    let c = f.lease("task-c").await;
    f.co.validator().verify_accepted(&c.task).await.unwrap();
    let raw: Vec<u8> =
        sqlx::query_scalar("SELECT content FROM synthetic_artifacts WHERE attempt_id=?")
            .bind(c.attempt_id.as_str())
            .fetch_one(&f.store.pool)
            .await
            .unwrap();
    let output: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(
        output["inputs"][0]["manifest_sha256"],
        group_digest.as_str()
    );
    assert_eq!(f.co.fingerprint(), fingerprint);
    assert_eq!(f.count("task_attempts").await, 3);
    assert_eq!(f.world.ref_count().await, 2);
}

#[tokio::test]
async fn coding_untrusted_reports_and_stale_context_never_accept() {
    let f = Fixture::new_coding().await;
    let leases = f.coding_admit().await;
    let l = &leases[0];
    let r = f.coding_result(l).await;
    f.coding_receive(l, &r).await;
    let report = f.coding_report(&r);
    assert_eq!(
        f.co.verify_coding_execution(l, &report, f.clock.as_ref())
            .await
            .unwrap_err(),
        OrchestrationError::VerificationUnavailable
    );
    assert!(
        f.co.accept_coding_result(l, &f.policy, f.clock.as_ref())
            .await
            .is_err()
    );
    assert_eq!(f.count("task_acceptances").await, 0);
    assert_eq!(f.world.ref_count().await, 0);
    for changed in 0..8 {
        let mut bad = report.clone();
        match changed {
            0 => bad.verifier = "worker-self-report".into(),
            1 => bad.source_sha256 = commitment_bytes(b"forged source"),
            2 => bad.change_set_sha256 = commitment_bytes(b"forged changes"),
            3 => bad.policy_sha256 = commitment_bytes(b"forged policy"),
            4 => bad.tests_completed.clear(),
            5 => bad.terminal = VerificationTerminal::Interrupted,
            6 => bad.exit_code = 1,
            _ => bad.binding.epoch += 1,
        }
        // The fixture authority may attest unsuccessful/mismatched runs too; the
        // independent Rust validator still rejects their actual contents.
        if bad.verifier.starts_with("private-synthetic-") {
            f.co.fixture_coding_run(bad.clone());
        }
        assert!(
            f.co.verify_coding_execution(l, &bad, f.clock.as_ref())
                .await
                .is_err()
        );
    }
    assert_eq!(f.count("coding_verifications").await, 0);
}
#[tokio::test]
async fn coding_wrong_scope_hash_epoch_original_and_finance_rejected_without_artifact_rows() {
    let f = Fixture::new_coding().await;
    let leases = f.coding_admit().await;
    let l = &leases[0];
    let r = f.coding_result(l).await;
    for n in 0..13 {
        let mut bad = r.clone();
        match n {
            0 => bad.repository = RepositoryId::new(100).unwrap(),
            1 => bad.installation = "0".repeat(32),
            2 => bad.binding.task.plan.project_id = ProjectId::new("other-project").unwrap(),
            3 => bad.binding.epoch += 1,
            4 => bad.binding.plan_hash = commitment_bytes(b"wrong plan"),
            5 => bad.binding.context_hash = commitment_bytes(b"wrong task"),
            6 => bad.base_commit = GitSha::parse("f".repeat(40)).unwrap(),
            7 => bad.source_sha256 = commitment_bytes(b"wrong source"),
            8 => bad.financial_event += 1,
            9 => bad.operation = commitment_bytes(b"wrong operation"),
            10 => bad.resulting_tree_sha256 = commitment_bytes(b"wrong output"),
            11 => {
                bad.changes = vec![FileChange::Add {
                    path: "src/beta.rs".into(),
                    content: "scope escape".into(),
                }]
            }
            _ => {
                bad.changes = vec![FileChange::Modify {
                    path: "src/alpha.rs".into(),
                    original_sha256: commitment_bytes(b"wrong old"),
                    content: "new".into(),
                }]
            }
        }
        assert!(
            f.co.submit_coding_artifact(l, &serde_json::to_vec(&bad).unwrap(), f.clock.as_ref())
                .await
                .is_err(),
            "case {n}"
        );
        assert_eq!(f.count("coding_artifacts").await, 0);
    }
    let mut v = serde_json::to_value(&r).unwrap();
    v["accepted"] = json!(true);
    assert!(
        f.co.submit_coding_artifact(l, &serde_json::to_vec(&v).unwrap(), f.clock.as_ref())
            .await
            .is_err()
    );
    assert_eq!(f.count("task_acceptances").await, 0);
}
#[tokio::test]
async fn coding_concurrent_acceptance_and_duplicate_publication_are_idempotent() {
    let f = Fixture::new_coding().await;
    let leases = f.coding_admit().await;
    let l = &leases[0];
    let r = f.coding_result(l).await;
    f.coding_receive(l, &r).await;
    f.coding_verify(l, &r).await;
    let (a, b) = tokio::join!(
        f.co.accept_coding_result(l, &f.policy, f.clock.as_ref()),
        f.co.accept_coding_result(l, &f.policy, f.clock.as_ref())
    );
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(f.count("task_acceptances").await, 1);
    assert_eq!(f.count("coding_decisions").await, 1);
    let (a, b) = tokio::join!(
        f.remote.prepare_coding(&f.co, l),
        f.remote.prepare_coding(&f.co, l)
    );
    let a = a.unwrap();
    assert_eq!(a, b.unwrap());
    let (a, b) = tokio::join!(
        f.remote.publish(&f.co, &f.project, &a.operation),
        f.remote.publish(&f.co, &f.project, &a.operation)
    );
    assert!(a.is_ok() || b.is_ok());
    let p = f.remote.prepare_coding(&f.co, l).await.unwrap();
    f.remote
        .reconcile(&f.co, &f.project, &p.operation)
        .await
        .unwrap();
    assert_eq!(f.world.ref_count().await, 1);
    assert_eq!(f.count("task_attempts").await, 2);
}
#[tokio::test]
async fn coding_lost_remote_ack_reconciles_and_sibling_remains_accepted() {
    let f = Fixture::new_coding().await;
    let leases = f.coding_admit().await;
    for l in &leases {
        let r = f.coding_result(l).await;
        f.coding_receive(l, &r).await;
        f.coding_verify(l, &r).await;
        f.coding_accept(l).await;
    }
    let before = f.store.financial_snapshot(&f.project).await.unwrap();
    let p = f.remote.prepare_coding(&f.co, &leases[0]).await.unwrap();
    f.world.lose_ref.store(true, Ordering::SeqCst);
    assert_eq!(
        f.remote
            .publish(&f.co, &f.project, &p.operation)
            .await
            .unwrap_err(),
        GitError::UnknownOutcome
    );
    assert_eq!(
        f.remote.state(&f.project, &p.operation).await.unwrap(),
        PublicationState::Unknown
    );
    assert_eq!(f.world.ref_count().await, 1);
    let writes = f.world.attempts.load(Ordering::SeqCst);
    f.world.lose_ref.store(false, Ordering::SeqCst);
    let reconciled = f
        .remote
        .reconcile(&f.co, &f.project, &p.operation)
        .await
        .unwrap();
    assert_eq!(reconciled.state, PublicationState::Published);
    assert_eq!(f.world.attempts.load(Ordering::SeqCst), writes);
    f.co.validator()
        .verify_accepted(&leases[1].task)
        .await
        .unwrap();
    assert_eq!(
        f.store.financial_snapshot(&f.project).await.unwrap(),
        before
    );
}
#[tokio::test]
async fn coding_revocation_permission_downgrade_and_fencing_preserve_accepted_bytes() {
    for mode in 0..3 {
        let f = Fixture::new_coding().await;
        let leases = f.coding_admit().await;
        let l = &leases[0];
        let r = f.coding_result(l).await;
        f.coding_receive(l, &r).await;
        f.coding_verify(l, &r).await;
        f.coding_accept(l).await;
        let p = f.remote.prepare_coding(&f.co, l).await.unwrap();
        let before = f.store.financial_snapshot(&f.project).await.unwrap();
        match mode {
            0 => {
                sqlx::query("UPDATE human_users SET security_epoch=security_epoch+1")
                    .execute(&f.store.pool)
                    .await
                    .unwrap();
            }
            1 => f.world.permission_read.store(true, Ordering::SeqCst),
            _ => {
                sqlx::query("UPDATE task_fences SET epoch=epoch+1,current_attempt=NULL")
                    .execute(&f.store.pool)
                    .await
                    .unwrap();
            }
        }
        assert!(
            f.remote
                .publish(&f.co, &f.project, &p.operation)
                .await
                .is_err()
        );
        assert_eq!(f.world.ref_count().await, 0);
        assert_eq!(f.count("coding_artifacts").await, 1);
        assert_eq!(f.count("task_acceptances").await, 1);
        assert_eq!(
            f.store.financial_snapshot(&f.project).await.unwrap(),
            before
        );
    }
}
#[tokio::test]
async fn coding_sqlite_ignored_writes_and_aborts_roll_back_acceptance_together() {
    for action in ["IGNORE", "ABORT,'private injected rollback'"] {
        let f = Fixture::new_coding().await;
        let leases = f.coding_admit().await;
        let l = &leases[0];
        let r = f.coding_result(l).await;
        f.coding_receive(l, &r).await;
        f.coding_verify(l, &r).await;
        sqlx::raw_sql(&format!("CREATE TRIGGER coding_test_abort BEFORE INSERT ON task_acceptances BEGIN SELECT RAISE({action}); END;")).execute(&f.store.pool).await.unwrap();
        assert!(
            f.co.accept_coding_result(l, &f.policy, f.clock.as_ref())
                .await
                .is_err()
        );
        for table in [
            "task_acceptances",
            "synthetic_validations",
            "coding_decisions",
        ] {
            assert_eq!(f.count(table).await, 0, "{table}");
        }
        sqlx::raw_sql("DROP TRIGGER coding_test_abort")
            .execute(&f.store.pool)
            .await
            .unwrap();
        f.coding_accept(l).await;
    }
}
#[tokio::test]
async fn coding_raw_content_tamper_fails_closed() {
    let f = Fixture::new_coding().await;
    let leases = f.coding_admit().await;
    let l = &leases[0];
    let r = f.coding_result(l).await;
    f.coding_receive(l, &r).await;
    f.coding_verify(l, &r).await;
    f.coding_accept(l).await;
    sqlx::raw_sql(
        "DROP TRIGGER coding_artifacts_update; UPDATE coding_artifacts SET content=X'7b7d';",
    )
    .execute(&f.store.pool)
    .await
    .unwrap();
    assert!(f.co.validator().verify_accepted(&l.task).await.is_err());
    assert_eq!(
        f.remote.prepare_coding(&f.co, l).await.unwrap_err(),
        GitError::Unaccepted
    );
    assert_eq!(f.world.ref_count().await, 0);
}
#[tokio::test]
async fn coding_synthetic_acceptance_cannot_borrow_code_publication_authority() {
    let f = Fixture::new().await;
    assert_eq!(
        f.remote
            .prepare_coding(&f.co, &f.lease("task-a").await)
            .await
            .unwrap_err(),
        GitError::Unaccepted
    );
    assert_eq!(f.count("coding_decisions").await, 0);
    assert_eq!(f.world.ref_count().await, 0);
}
#[tokio::test]
async fn coding_verifier_execution_cannot_predate_admitted_attempt() {
    let f = Fixture::new_coding().await;
    let leases = f.coding_admit().await;
    let l = &leases[0];
    let r = f.coding_result(l).await;
    f.coding_receive(l, &r).await;
    let mut report = f.coding_report(&r);
    report.started_at = 0;
    report.completed_at = 0;
    f.co.fixture_coding_run(report.clone());
    assert!(
        f.co.verify_coding_execution(l, &report, f.clock.as_ref())
            .await
            .is_err()
    );
    assert_eq!(f.count("coding_verifications").await, 0);
}
#[tokio::test]
async fn coding_accepted_overlap_is_explicit_and_never_combines_refs() {
    let f = Fixture::new_coding_overlap().await;
    let leases = f.coding_admit().await;
    for l in &leases {
        let r = f.coding_result(l).await;
        f.coding_receive(l, &r).await;
        f.coding_verify(l, &r).await;
        f.coding_accept(l).await;
    }
    let conflicts = f.co.coding_conflicts(&leases[0], &leases[1]).await.unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].left_path, "src/alpha.rs");
    assert_eq!(conflicts[0].right_path, "src/alpha.rs");
    assert_ne!(conflicts[0].left_artifact, conflicts[0].right_artifact);
    for l in &leases {
        let p = f.remote.prepare_coding(&f.co, l).await.unwrap();
        f.remote
            .publish(&f.co, &f.project, &p.operation)
            .await
            .unwrap();
    }
    assert_eq!(f.world.ref_count().await, 2);
    assert_eq!(f.count("task_acceptances").await, 2);
    assert_eq!(f.count("task_attempts").await, 2);
}

mod process;
#[tokio::test]
async fn coding_legacy_trusted_policy_acceptance_without_sealed_validator_never_publishes() {
    let f = Fixture::new_coding().await;
    let leases = f.coding_admit().await;
    let l = &leases[0];
    let r = f.coding_result(l).await;
    f.coding_receive(l, &r).await;
    let identity = crate::RequestId::new("private-fake-legacy-validation").unwrap();
    let manifest = json!({"format":2,"task":l.task,"plan_hash":f.co.fingerprint(),"context_hash":r.binding.context_hash,"lease":l,"artifact":commitment_bytes(&serde_json::to_vec(&r).unwrap()),"inputs":[],"children":[],"criteria":[CODING_CRITERION],"scope":f.contexts[&l.task.task_id].paths,"operation":r.operation,"coding":commitment_bytes(b"worker claims accepted")});
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let sha = hash(&bytes);
    // Exercise the historical explicitly trusted SchedulerPolicy seam, not an
    // attacker-accessible production API. Its acceptance alone is insufficient.
    sqlx::query("INSERT INTO synthetic_validations VALUES(?,?,?,?,?,?,?,?,?)")
        .bind(f.project.as_str())
        .bind(l.task.plan.plan_id.as_str())
        .bind(l.task.plan.version.get() as i64)
        .bind(l.task.task_id.as_str())
        .bind(identity.as_str())
        .bind(l.attempt_id.as_str())
        .bind(bytes)
        .bind(&sha)
        .bind(NOW)
        .execute(&f.store.pool)
        .await
        .unwrap();
    f.store
        .accept_task(
            &ValidationRequest {
                task: l.task.clone(),
                fingerprint: f.co.fingerprint().into(),
                lease: Some(l.clone()),
                request_id: identity.clone(),
                actor: PrincipalId::new("coding-validator").unwrap(),
                evidence: synthetic::e(identity.as_str()),
                manifest_sha256: sha,
            },
            &f.policy,
            f.clock.as_ref(),
            Limits::default(),
        )
        .await
        .unwrap();
    assert!(f.co.validator().verify_accepted(&l.task).await.is_err());
    assert_eq!(
        f.remote.prepare_coding(&f.co, l).await.unwrap_err(),
        GitError::Unaccepted
    );
    assert_eq!(f.count("coding_decisions").await, 0);
    assert_eq!(f.world.ref_count().await, 0);
}
#[tokio::test]
async fn coding_source_and_verification_seals_detect_corruption_without_erasing_siblings() {
    for table in ["coding_sources", "coding_verifications", "coding_decisions"] {
        let f = Fixture::new_coding().await;
        let leases = f.coding_admit().await;
        for l in &leases {
            let r = f.coding_result(l).await;
            f.coding_receive(l, &r).await;
            f.coding_verify(l, &r).await;
            f.coding_accept(l).await;
        }
        let predicate = if table == "coding_sources" {
            format!("task_id='{}'", leases[0].task.task_id.as_str())
        } else {
            format!("attempt_id='{}'", leases[0].attempt_id.as_str())
        };
        sqlx::raw_sql(&format!(
            "DROP TRIGGER {table}_update; UPDATE {table} SET seal=zeroblob(32) WHERE {predicate};"
        ))
        .execute(&f.store.pool)
        .await
        .unwrap();
        assert!(
            f.co.validator()
                .verify_accepted(&leases[0].task)
                .await
                .is_err()
        );
        assert_eq!(
            f.remote
                .prepare_coding(&f.co, &leases[0])
                .await
                .unwrap_err(),
            GitError::Unaccepted
        );
        f.co.validator()
            .verify_accepted(&leases[1].task)
            .await
            .unwrap();
        assert_eq!(f.world.ref_count().await, 0);
        assert_eq!(f.count("task_acceptances").await, 2);
    }
}
#[tokio::test]
async fn coding_known_submitting_snapshot_reconciles_remote_effect_without_rewriting() {
    let f = Fixture::new_coding().await;
    let leases = f.coding_admit().await;
    let l = &leases[0];
    let r = f.coding_result(l).await;
    f.coding_receive(l, &r).await;
    f.coding_verify(l, &r).await;
    f.coding_accept(l).await;
    let p = f.remote.prepare_coding(&f.co, l).await.unwrap();
    let mut tx = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    publication::event(
        &mut tx,
        &f.remote.key,
        &f.project,
        &p.operation,
        "submitting",
        None,
        Some(&p.commit),
        None,
        NOW,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let snapshot = f.root.path().join("data/known-submitting.sqlite");
    f.store.snapshot(&snapshot).await.unwrap();
    let i = f
        .remote
        .load(&f.project, &p.operation)
        .await
        .unwrap()
        .unwrap();
    let mut tx = f.store.pool.begin().await.unwrap();
    let b = consent::current(&mut tx, &f.remote.key, &f.project, NOW)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let token = f.remote.token(&b, "publication").await.unwrap();
    f.remote.remote_write(&f.co, &i, &token).await.unwrap();
    let writes = f.world.attempts.load(Ordering::SeqCst);
    let restored = PlanStore::open(&snapshot).await.unwrap();
    let bootstrap = ControllerBootstrap::open(
        restored.clone(),
        &f.root.path().join("keys"),
        ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let co = bootstrap
        .coding_coordinator(&f.project, &f.policy, Limits::default())
        .await
        .unwrap();
    let (origin, server) = f.world.server().await;
    let mut remote = GitHubRemote::open(&bootstrap).await.unwrap();
    remote.clock = f.clock.clone();
    remote.http = http::Api::fixture(origin);
    co.validator().verify_accepted(&l.task).await.unwrap();
    assert_eq!(
        remote
            .reconcile(&co, &f.project, &p.operation)
            .await
            .unwrap()
            .state,
        PublicationState::Published
    );
    assert_eq!(f.world.attempts.load(Ordering::SeqCst), writes);
    assert_eq!(f.world.ref_count().await, 1);
    server.abort();
    restored.close().await;
}
#[tokio::test]
async fn coding_wrong_key_missing_key_and_no_runner_root_remain_closed() {
    let mut f = Fixture::new_coding().await;
    let leases = f.coding_admit().await;
    let l = &leases[0];
    let r = f.coding_result(l).await;
    f.coding_receive(l, &r).await;
    f.coding_verify(l, &r).await;
    f.coding_accept(l).await;
    let other = Fixture::new().await;
    f.co.attach_coding_key(other.bootstrap.load_existing().await.unwrap());
    assert!(f.co.validator().verify_accepted(&l.task).await.is_err());
    assert_eq!(
        f.remote.prepare_coding(&f.co, l).await.unwrap_err(),
        GitError::Unaccepted
    );
    std::fs::remove_file(f.root.path().join("keys/controller.kek")).unwrap();
    assert!(
        f.bootstrap
            .coding_coordinator(&f.project, &f.policy, Limits::default())
            .await
            .is_err()
    );
    assert_eq!(f.count("task_acceptances").await, 1);
}
#[tokio::test]
async fn coding_transport_upload_is_untrusted_bounded_and_receipt_bound() {
    let f = Fixture::new_coding().await;
    let leases = f.coding_admit().await;
    let l = &leases[0];
    let r = f.coding_result(l).await;
    let raw = serde_json::to_vec(&r).unwrap();
    let mut tx = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    crate::orchestration::coding::stage_untrusted(&mut tx, &r.binding, &raw, NOW)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(f.count("coding_artifacts").await, 1);
    assert_eq!(f.count("task_acceptances").await, 0);
    assert_eq!(
        f.remote.prepare_coding(&f.co, l).await.unwrap_err(),
        GitError::Unaccepted
    );
    let frame = crate::controller_keys::worker_auth::https::wire::Request {
        version: 1,
        request_id: crate::RequestId::new("coding-network-result").unwrap(),
        operation: crate::controller_keys::worker_auth::https::wire::Operation::Message {
            message: Box::new(WorkerMessage {
                protocol_version: 1,
                message_id: MessageId::new("coding-network-result").unwrap(),
                binding: r.binding.clone(),
                sequence: 4,
                body: WorkerBody::SubmitResult {
                    outcome: ExecutionOutcome::Succeeded {
                        manifest_sha256: commitment_bytes(&raw),
                    },
                },
            }),
            artifact: Some(
                crate::controller_keys::worker_auth::https::wire::ArtifactUpload {
                    sha256: commitment_bytes(&raw),
                    data: base64::engine::general_purpose::STANDARD.encode(&raw),
                },
            ),
        },
    };
    frame.validate().unwrap();
    assert!(
        serde_json::to_vec(&frame).unwrap().len()
            < crate::controller_keys::worker_auth::https::wire::MAX_HTTP_BODY
    );
    f.coding_receive(l, &r).await;
    f.coding_verify(l, &r).await;
    f.coding_accept(l).await;
}
#[tokio::test]
async fn coding_incomplete_symlink_or_corrupt_source_never_pins_authority() {
    for mode in 0..3 {
        let f = Fixture::new_coding().await;
        let task = TaskRef {
            plan: f.co.plan_key().clone(),
            task_id: TaskId::new("task-a").unwrap(),
        };
        let ctx = &f.contexts[&task.task_id];
        match mode {
            0 => f.world.bad_blob.store(true, Ordering::SeqCst),
            1 => f.world.symlink.store(true, Ordering::SeqCst),
            _ => {
                sqlx::query(
                    "UPDATE objects SET body=json_set(body,'$.truncated',json('true')) WHERE sha=?",
                )
                .bind(ctx.base_tree.as_str())
                .execute(&f.world.db)
                .await
                .unwrap();
            }
        }
        assert!(f.remote.pin_coding_source(&f.co, &task, ctx).await.is_err());
        assert_eq!(f.count("coding_sources").await, 0);
        assert_eq!(f.count("task_attempts").await, 0);
        assert_eq!(f.world.ref_count().await, 0);
    }
}
#[tokio::test]
async fn coding_evidence_limits_output_hash_and_current_chronology_are_rechecked() {
    let f = Fixture::new_coding().await;
    let leases = f.coding_admit().await;
    let l = &leases[0];
    let r = f.coding_result(l).await;
    f.coding_receive(l, &r).await;
    for n in 0..7 {
        let mut report = f.coding_report(&r);
        match n {
            0 => report.outputs[0].sha256 = commitment_bytes(b"wrong output"),
            1 => report.completed_at = NOW + 61,
            2 => report.completed_at = NOW + 1,
            3 => report.limits.memory_mib += 1,
            4 => report
                .tests_completed
                .push(report.tests_completed[0].clone()),
            5 => {
                report.outputs[0].content = "x".repeat(1025);
                report.outputs[0].sha256 = commitment_bytes(report.outputs[0].content.as_bytes());
            }
            _ => report.verifier_version = 2,
        }
        f.co.fixture_coding_run(report.clone());
        assert!(
            f.co.verify_coding_execution(l, &report, f.clock.as_ref())
                .await
                .is_err()
        );
        assert_eq!(f.count("coding_verifications").await, 0);
    }
}
#[tokio::test]
async fn coding_sqlite_contention_is_bounded_and_leaves_no_acceptance_effect() {
    let f = Fixture::new_coding().await;
    let leases = f.coding_admit().await;
    let l = &leases[0];
    let r = f.coding_result(l).await;
    f.coding_receive(l, &r).await;
    f.coding_verify(l, &r).await;
    let lock = f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        f.co.accept_coding_result(l, &f.policy, f.clock.as_ref()),
    )
    .await
    .unwrap();
    assert_eq!(
        result.unwrap_err(),
        OrchestrationError::Storage(crate::StoreError::Busy)
    );
    assert_eq!(f.count("task_acceptances").await, 0);
    lock.rollback().await.unwrap();
    f.coding_accept(l).await;
}
#[tokio::test]
async fn coding_prepared_snapshot_checks_existing_ref_before_any_repeated_object_write() {
    let f = Fixture::new_coding().await;
    let leases = f.coding_admit().await;
    let l = &leases[0];
    let r = f.coding_result(l).await;
    f.coding_receive(l, &r).await;
    f.coding_verify(l, &r).await;
    f.coding_accept(l).await;
    let p = f.remote.prepare_coding(&f.co, l).await.unwrap();
    let snapshot = f.root.path().join("data/known-prepared.sqlite");
    f.store.snapshot(&snapshot).await.unwrap();
    f.remote
        .publish(&f.co, &f.project, &p.operation)
        .await
        .unwrap();
    let writes = f.world.attempts.load(Ordering::SeqCst);
    let restored = PlanStore::open(&snapshot).await.unwrap();
    let bootstrap = ControllerBootstrap::open(
        restored.clone(),
        &f.root.path().join("keys"),
        ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let co = bootstrap
        .coding_coordinator(&f.project, &f.policy, Limits::default())
        .await
        .unwrap();
    let (origin, server) = f.world.server().await;
    let mut remote = GitHubRemote::open(&bootstrap).await.unwrap();
    remote.clock = f.clock.clone();
    remote.http = http::Api::fixture(origin);
    assert_eq!(
        remote
            .publish(&co, &f.project, &p.operation)
            .await
            .unwrap()
            .state,
        PublicationState::Published
    );
    assert_eq!(
        f.world.attempts.load(Ordering::SeqCst),
        writes,
        "restored prepared intent must inspect existing remote state before writing objects"
    );
    assert_eq!(f.world.ref_count().await, 1);
    server.abort();
    restored.close().await;
}
