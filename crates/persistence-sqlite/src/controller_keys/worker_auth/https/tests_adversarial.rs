use super::*;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
async fn enrolled(
    f: &Fixture,
    s: &Arc<HttpsWorkerServer>,
    v: &SignedProvider,
    url: &str,
) -> WorkerHttpClient {
    let (b, secret, proof, binding, id) = f.prepare(s, v, "task-a").await;
    let mut c = f.cert.client(url);
    c.bootstrap(id, b.id, &secret, proof, &binding)
        .await
        .unwrap();
    c
}
async fn script(c: &WorkerHttpClient) -> FakeScript {
    let a = c
        .assignment(RequestId::new("assignment").unwrap())
        .await
        .unwrap();
    FakeWorker::new(a.binding, a.capabilities, a.seed)
        .unwrap()
        .with_inputs(a.inputs)
        .unwrap()
        .script(FakeBehavior::Success, synthetic::T)
        .unwrap()
}
async fn send_event(
    f: &Fixture,
    c: &WorkerHttpClient,
    script: &FakeScript,
    index: usize,
) -> ControllerMessage {
    let e = &script.events[index];
    f.clock.advance_to(e.at).unwrap();
    let SimulatedInput::Message(bytes) = &e.input else {
        unreachable!()
    };
    let m = serde_json::from_slice(bytes).unwrap();
    c.message(
        m,
        if index == 5 {
            Some(script.artifact.as_ref().unwrap().bytes.as_slice())
        } else {
            None
        },
    )
    .await
    .unwrap()
}
fn message(script: &FakeScript, index: usize) -> WorkerMessage {
    let SimulatedInput::Message(bytes) = &script.events[index].input else {
        unreachable!()
    };
    serde_json::from_slice(bytes).unwrap()
}
async fn renewal(
    f: &Fixture,
    s: &HttpsWorkerServer,
    c: &WorkerHttpClient,
    id: &str,
) -> (RequestId, Attestation) {
    let m = c.metadata().unwrap();
    let mut tx = f.store.pool.begin().await.unwrap();
    let record = s.security.enrollment(&mut tx, &m.enrollment).await.unwrap();
    tx.commit().await.unwrap();
    let request = RequestId::new(id).unwrap();
    let e = synthetic::e("signed-renewal");
    let proof = sign(
        &f.key,
        record
            .check(VerificationStage::Renew, &request, &e, Some(m))
            .scope(),
    );
    (request, proof)
}
struct Revoke(WorkerRevocation, WorkerBinding);
impl RevocationAuthority for Revoke {
    fn verify(
        &self,
        r: &WorkerRevocation,
        e: &EnrollmentRecord,
        _: i64,
    ) -> Result<(), WorkerAuthError> {
        if *r == self.0 && e.scope.binding == self.1 {
            Ok(())
        } else {
            Err(WorkerAuthError::Denied)
        }
    }
}

fn install_revoker(
    s: &mut Arc<HttpsWorkerServer>,
    binding: &WorkerBinding,
    enrollment: &WorkerEnrollmentId,
) {
    let r = WorkerRevocation {
        enrollment: enrollment.clone(),
        expected_security_epoch: 1,
        request_id: RequestId::new("scoped-revocation").unwrap(),
        evidence: synthetic::e("synthetic-incident"),
    };
    Arc::get_mut(s).unwrap().security.revocation = Arc::new(Revoke(r, binding.clone()));
}
#[tokio::test]
async fn standard_tls_rejects_wrong_name_untrusted_and_expired_certificates() {
    let f = Fixture::new().await;
    for mode in ["wrong-name", "untrusted", "expired"] {
        let (mut s, _) = f.server().await;
        let cert = Certificate::new(
            if mode == "wrong-name" {
                "other.invalid"
            } else {
                "localhost"
            },
            mode == "expired",
        );
        Arc::get_mut(&mut s).unwrap().identity = cert.server();
        let (url, stop, task) = listen(s).await;
        let config = if mode == "untrusted" {
            tls::client().unwrap()
        } else {
            tls::fixture_client(cert.cert.clone())
        };
        let client =
            WorkerHttpClient::with_tls(ControllerEndpoint::parse(&url).unwrap(), config).unwrap();
        let request = wire::Request {
            version: 1,
            request_id: RequestId::new("invalid-tls").unwrap(),
            operation: Operation::Assignment {},
        };
        let token = WorkerCredential::parse(hex(&super::super::super::random::<32>().unwrap()[..]))
            .unwrap();
        assert_eq!(
            client
                .fixture_unauthenticated_tls_probe(&request, &token)
                .await
                .unwrap_err(),
            ClientError::CredentialDeliveryUnknown,
            "must fail at TLS/transport, not an ordinary HTTP authentication rejection"
        );
        stop.send(()).unwrap();
        task.await.unwrap();
    }
    let (server, _) = f.server().await;
    let (url, stop, task) = listen(server).await;
    let client = f.cert.client(&url);
    let token =
        WorkerCredential::parse(hex(&super::super::super::random::<32>().unwrap()[..])).unwrap();
    let request = wire::Request {
        version: 1,
        request_id: RequestId::new("valid-tls-bad-token").unwrap(),
        operation: Operation::Assignment {},
    };
    assert_eq!(
        client
            .fixture_unauthenticated_tls_probe(&request, &token)
            .await
            .unwrap_err(),
        ClientError::Rejected(ErrorCode::Unauthenticated),
        "valid TLS reaches HTTP; 401 is not proof of certificate rejection"
    );
    stop.send(()).unwrap();
    task.await.unwrap();
    assert!(
        tls::ServerIdentity::from_der(
            vec![f.cert.cert.clone()],
            PrivatePkcs8KeyDer::from(f.cert.key.clone()).into(),
            "localhost"
        )
        .is_err(),
        "fixture CA is not production configuration"
    );
}
#[tokio::test]
async fn cleartext_and_forged_headers_do_not_establish_worker_authority() {
    let f = Fixture::new().await;
    let (s, v) = f.server().await;
    let (boot, secret, attestation, _, request_id) = f.prepare(&s, &v, "task-a").await;
    let (url, stop, task) = listen(s).await;
    let port = url::Url::parse(&url).unwrap().port().unwrap();
    let mut socket = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let frame = wire::Request {
        version: 1,
        request_id,
        operation: Operation::Bootstrap {
            challenge: boot.id,
            attestation,
        },
    };
    let body = serde_json::to_vec(&frame).unwrap();
    let request = Zeroizing::new(format!(
        "POST /v1/worker HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        secret.0.0.as_str(),
        body.len()
    ));
    let _ = socket.write_all(request.as_bytes()).await;
    let _ = socket.write_all(&body).await;
    let mut response = [0u8; 128];
    let read = tokio::time::timeout(Duration::from_secs(2), socket.read(&mut response))
        .await
        .expect("cleartext rejected within handshake bound");
    let received = read.unwrap_or(0);
    assert!(
        !response[..received].starts_with(b"HTTP/"),
        "TLS-only listener must not serve plaintext HTTP, even with a valid synthetic bootstrap"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_messages")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        0
    );
    let http = reqwest::Client::builder()
        .tls_backend_preconfigured(tls::fixture_client(f.cert.cert.clone()).as_ref().clone())
        .no_proxy()
        .build()
        .unwrap();
    let r = http
        .post(format!("{url}v1/worker"))
        .header("X-Worker-Id", "synthetic-worker")
        .header("X-Verified-Identity", "true")
        .header("X-Forwarded-For", "127.0.0.1")
        .header("Content-Type", "application/json")
        .body(r#"{"version":1,"request_id":"forged-header","operation":{"kind":"assignment"}}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status().as_u16(), 401);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_credentials")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        0
    );
    stop.send(()).unwrap();
    task.await.unwrap();
}
#[tokio::test]
async fn provider_signature_and_bootstrap_replay_are_rejected() {
    let f = Fixture::new().await;
    let (s, v) = f.server().await;
    let (b, secret, proof, binding, id) = f.prepare(&s, &v, "task-a").await;
    let (url, stop, task) = listen(s).await;
    let mut bad = proof.clone();
    bad.signature.replace_range(
        ..1,
        if bad.signature.starts_with('A') {
            "B"
        } else {
            "A"
        },
    );
    let mut c = f.cert.client(&url);
    assert!(
        c.bootstrap(id.clone(), b.id.clone(), &secret, bad, &binding)
            .await
            .is_err()
    );
    assert!(c.metadata().is_none());
    c.bootstrap(id.clone(), b.id.clone(), &secret, proof.clone(), &binding)
        .await
        .unwrap();
    let mut replay = f.cert.client(&url);
    assert!(
        replay
            .bootstrap(id, b.id, &secret, proof, &binding)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_credentials")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        1
    );
    stop.send(()).unwrap();
    task.await.unwrap();
}
#[tokio::test]
async fn production_provider_verifier_does_not_accept_test_signatures() {
    let f = Fixture::new().await;
    let (mut s, v) = f.server().await;
    let (b, secret, proof, binding, id) = f.prepare(&s, &v, "task-a").await;
    Arc::get_mut(&mut s).unwrap().security.identity = Arc::new(Unimplemented);
    let (url, stop, task) = listen(s).await;
    let mut c = f.cert.client(&url);
    assert_eq!(
        c.bootstrap(id, b.id, &secret, proof, &binding).await,
        Err(ClientError::Rejected(ErrorCode::Unavailable))
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_credentials")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        0
    );
    stop.send(()).unwrap();
    task.await.unwrap();
}
#[tokio::test]
async fn strict_frames_bearer_reflection_and_cross_attempt_scope_fail_closed() {
    let f = Fixture::new().await;
    let (s, v) = f.server().await;
    let (url, stop, task) = listen(s.clone()).await;
    let c = enrolled(&f, &s, &v, &url).await;
    let script = script(&c).await;
    let base = message(&script, 0);
    let frame = wire::Request {
        version: 1,
        request_id: RequestId::new(base.message_id.as_str()).unwrap(),
        operation: Operation::Message {
            message: Box::new(base.clone()),
            artifact: None,
        },
    };
    for body in [b"{malformed".to_vec(),vec![b' ';wire::MAX_HTTP_BODY+1],serde_json::to_vec(&serde_json::json!({"version":999,"request_id":"future","operation":{"kind":"assignment"}})).unwrap(),serde_json::to_vec(&serde_json::json!({"version":1,"request_id":"privileged","operation":{"kind":"assignment","approved_budget":999}})).unwrap()]{let(status,bytes)=c.fixture_request(|_|body).await;assert_eq!(status,400);assert!(bytes.len()<256);}
    let (status, _) = c
        .fixture_request(|token| {
            let mut value = serde_json::to_value(&frame).unwrap();
            value["request_id"] = token.into();
            value["operation"]["message"]["message_id"] = token.into();
            let body = serde_json::to_string(&value).unwrap();
            body.replace(
                token,
                &token
                    .bytes()
                    .map(|b| format!("\\u{b:04x}"))
                    .collect::<String>(),
            )
            .into_bytes()
        })
        .await;
    assert_eq!(status, 400);
    for field in ["attempt", "project", "context"] {
        let mut forged = base.clone();
        match field {
            "attempt" => forged.binding.attempt_id = AttemptId::new("another-attempt").unwrap(),
            "project" => {
                forged.binding.task.plan.project_id = ProjectId::new("another-project").unwrap()
            }
            _ => forged.binding.context_hash = Digest::new("f".repeat(64)).unwrap(),
        };
        let frame = wire::Request {
            version: 1,
            request_id: RequestId::new(forged.message_id.as_str()).unwrap(),
            operation: Operation::Message {
                message: Box::new(forged),
                artifact: None,
            },
        };
        let (status, _) = c
            .fixture_request(|_| serde_json::to_vec(&frame).unwrap())
            .await;
        assert_eq!(status, 403);
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_messages")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        0
    );
    stop.send(()).unwrap();
    task.await.unwrap();
}
#[tokio::test]
async fn duplicate_results_and_changed_payload_replay_never_duplicate_success() {
    let f = Fixture::new().await;
    let (s, v) = f.server().await;
    let (url, stop, task) = listen(s.clone()).await;
    let c = enrolled(&f, &s, &v, &url).await;
    let script = script(&c).await;
    assert!(
        c.message(message(&script, 4), None).await.is_err(),
        "out-of-order progress denied"
    );
    for index in 0..5 {
        send_event(&f, &c, &script, index).await;
    }
    let mut corrupt = script.artifact.as_ref().unwrap().bytes.clone();
    corrupt[0] ^= 1;
    assert_eq!(
        c.message(message(&script, 5), Some(&corrupt)).await,
        Err(ClientError::Rejected(ErrorCode::InvalidArtifact))
    );
    // Bypass the well-behaved client's validation to exercise both independent
    // server checks: actual bytes must hash correctly AND match trusted expected
    // synthetic provenance. A self-consistent forged manifest is insufficient.
    for consistent_forgery in [false, true] {
        let mut result = message(&script, 5);
        let sha = if consistent_forgery {
            Digest::new(format!("{:x}", <Sha256 as sha2::Digest>::digest(&corrupt))).unwrap()
        } else {
            script.artifact.as_ref().unwrap().sha256.clone()
        };
        result.body = WorkerBody::SubmitResult {
            outcome: ExecutionOutcome::Succeeded {
                manifest_sha256: sha.clone(),
            },
        };
        let frame = wire::Request {
            version: 1,
            request_id: RequestId::new(result.message_id.as_str()).unwrap(),
            operation: Operation::Message {
                message: Box::new(result),
                artifact: Some(ArtifactUpload {
                    sha256: sha,
                    data: B64.encode(&corrupt),
                }),
            },
        };
        frame.validate().unwrap();
        let (status, body) = c
            .fixture_request(|_| serde_json::to_vec(&frame).unwrap())
            .await;
        assert_eq!(status, 400);
        assert_eq!(
            serde_json::from_slice::<wire::ErrorResponse>(&body)
                .unwrap()
                .code,
            ErrorCode::InvalidArtifact
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_messages")
                .fetch_one(&f.store.pool)
                .await
                .unwrap(),
            5
        );
        assert_eq!(f.co.snapshot().await.unwrap().accepted, 0);
    }
    let first = send_event(&f, &c, &script, 5).await;
    let again = c
        .message(
            message(&script, 5),
            Some(&script.artifact.as_ref().unwrap().bytes),
        )
        .await
        .unwrap();
    assert_eq!(first, again);
    let r = wire::Request {
        version: 1,
        request_id: RequestId::new(message(&script, 5).message_id.as_str()).unwrap(),
        operation: Operation::Message {
            message: Box::new(message(&script, 5)),
            artifact: Some(ArtifactUpload {
                sha256: script.artifact.as_ref().unwrap().sha256.clone(),
                data: B64.encode(&script.artifact.as_ref().unwrap().bytes),
            }),
        },
    };
    let (status, body) = c.fixture_request(|_| serde_json::to_vec(&r).unwrap()).await;
    assert_eq!(status, 200);
    let response: wire::Response = serde_json::from_slice(&body).unwrap();
    assert!(matches!(
        response,
        wire::Response::Message {
            disposition: ReceiptDisposition::IdempotentReplay,
            ..
        }
    ));
    let mut conflict = message(&script, 5);
    conflict.body = WorkerBody::SubmitResult {
        outcome: ExecutionOutcome::Failed {
            class: FailureClass::Permanent,
        },
    };
    assert_eq!(
        c.message(conflict, None).await,
        Err(ClientError::Rejected(ErrorCode::ConflictingReplay))
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM worker_messages")
            .fetch_one(&f.store.pool)
            .await
            .unwrap(),
        6
    );
    assert_eq!(f.co.snapshot().await.unwrap().accepted, 0);
    stop.send(()).unwrap();
    task.await.unwrap();
}
#[tokio::test]
async fn lease_and_fencing_denials_preserve_financial_holds() {
    for fenced in [false, true] {
        let f = Fixture::new().await;
        let (s, v) = f.server().await;
        let (url, stop, task) = listen(s.clone()).await;
        let c = enrolled(&f, &s, &v, &url).await;
        let script = script(&c).await;
        for index in 0..3 {
            send_event(&f, &c, &script, index).await;
        }
        let before = f
            .store
            .financial_snapshot(&f.co.plan_key().project_id)
            .await
            .unwrap();
        if fenced {
            f.store
                .fence_attempt(
                    &admission::FenceRequest {
                        lease: f.lease("task-a").await,
                        request_id: RequestId::new("fence-http").unwrap(),
                        actor: common::owner(),
                        evidence: synthetic::e("synthetic-fence"),
                    },
                    f.policy.as_ref(),
                    f.clock.as_ref(),
                )
                .await
                .unwrap();
        } else {
            f.clock.advance_to(synthetic::T + 90).unwrap();
        }
        assert_eq!(
            c.message(message(&script, 3), None).await,
            Err(ClientError::Rejected(if fenced {
                ErrorCode::FencedEpoch
            } else {
                ErrorCode::LeaseExpired
            }))
        );
        assert_eq!(
            f.store
                .financial_snapshot(&f.co.plan_key().project_id)
                .await
                .unwrap()
                .held,
            before.held
        );
        stop.send(()).unwrap();
        task.await.unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn renewal_rechecks_revocation_and_graceful_shutdown_drains_active_response() {
    for revoked in [false, true] {
        let f = Fixture::new().await;
        let (mut s, v) = f.server().await;
        let (b, secret, proof, binding, id) = f.prepare(&s, &v, "task-a").await;
        install_revoker(&mut s, &binding, &b.enrollment);
        let (url, stop, task) = listen(s.clone()).await;
        let mut c = f.cert.client(&url);
        c.bootstrap(id, b.id, &secret, proof, &binding)
            .await
            .unwrap();
        let script = script(&c).await;
        for index in 0..4 {
            send_event(&f, &c, &script, index).await;
        }
        f.clock.advance_to(synthetic::T + 60).unwrap();
        let (id, proof) = renewal(&f, &s, &c, "renew-response").await;
        let lease_before=sqlx::query_scalar::<_,i64>("SELECT l.lease_until FROM worker_leases l JOIN task_attempts a USING(project_id,attempt_id) WHERE a.task_id='task-a'").fetch_one(&f.store.pool).await.unwrap();
        let mut stop = Some(stop);
        let pause = Arc::new((tokio::sync::Notify::new(), tokio::sync::Notify::new()));
        *v.pause.lock().unwrap() = Some(pause.clone());
        // Incident authority is explicitly scoped to this enrollment and binding.
        let revoke_request = WorkerRevocation {
            enrollment: c.metadata().unwrap().enrollment.clone(),
            expected_security_epoch: 1,
            request_id: RequestId::new("scoped-revocation").unwrap(),
            evidence: synthetic::e("synthetic-incident"),
        };
        let renewing = tokio::spawn(async move {
            let result = c.renew(id, proof).await;
            (c, result)
        });
        pause.0.notified().await;
        if revoked {
            s.security.revoke_worker(&revoke_request).await.unwrap();
        } else {
            stop.take().unwrap().send(()).unwrap();
        }
        pause.1.notify_one();
        let (c, result) = renewing.await.unwrap();
        if revoked {
            assert_eq!(
                result,
                Err(ClientError::Rejected(ErrorCode::RevokedCredential))
            );
            assert!(c.metadata().is_none());
            stop.take().unwrap().send(()).unwrap();
        } else {
            result.unwrap();
            assert_eq!(c.metadata().unwrap().generation, 2);
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT l.lease_until FROM worker_leases l JOIN task_attempts a USING(project_id,attempt_id) WHERE a.task_id='task-a'").fetch_one(&f.store.pool).await.unwrap(),lease_before,"credential renewal does not renew lease");
        }
        task.await.unwrap();
        assert_eq!(s.diagnostics().active, 0);
    }
}
#[tokio::test]
async fn financial_uncertainty_and_provider_expiry_block_control_without_releasing_holds() {
    for provider in [false, true] {
        let f = Fixture::new().await;
        let (s, v) = f.server().await;
        let (url, stop, task) = listen(s.clone()).await;
        let c = enrolled(&f, &s, &v, &url).await;
        let script = script(&c).await;
        for index in 0..3 {
            send_event(&f, &c, &script, index).await;
        }
        let before = f
            .store
            .financial_snapshot(&f.co.plan_key().project_id)
            .await
            .unwrap()
            .held;
        if provider {
            let registry = f.policy.provider_registry().unwrap();
            provider_accounts::fixture_limit(
                registry,
                &f.co.plan_key().project_id,
                synthetic::T + 20,
            )
            .await;
            f.clock.advance_to(synthetic::T + 20).unwrap();
        } else {
            f.store
                .apply_financial(
                    &FinancialRequest {
                        project: f.co.plan_key().project_id.clone(),
                        request_id: RequestId::new("network-unknown-cost").unwrap(),
                        actor: common::owner(),
                        reason: "Synthetic delayed billing; no external operation".into(),
                        command: FinancialCommand::Report {
                            liability: LiabilityId::new("network-uncertain-effect").unwrap(),
                            expected_revision: 0,
                            unreserved: synthetic::zero(),
                            uncertain: true,
                            evidence: synthetic::e("synthetic-unknown-exposure"),
                        },
                    },
                    &synthetic::Finance,
                )
                .await
                .unwrap();
        }
        assert_eq!(
            c.message(message(&script, 3), None).await,
            Err(ClientError::Rejected(if provider {
                ErrorCode::ProviderEntitlementUnavailable
            } else {
                ErrorCode::FinanciallyBlocked
            }))
        );
        if !provider {
            let mut stop_query = message(&script, 3);
            stop_query.message_id = MessageId::new("financial-stop-inspection").unwrap();
            stop_query.body = WorkerBody::GetStopInstruction {};
            let reply = c.message(stop_query, None).await.unwrap();
            let ControllerBody::Stop { instruction } = reply.body else {
                panic!("financial risk requires stop instruction")
            };
            assert_eq!(instruction.reason, StopReason::BudgetRisk);
            let mut ack = message(&script, 4);
            ack.message_id = MessageId::new("financial-stop-ack").unwrap();
            ack.body = WorkerBody::StopAcknowledged {
                instruction_id: instruction.instruction_id,
            };
            c.message(ack, None).await.unwrap();
        }
        assert_eq!(
            f.store
                .financial_snapshot(&f.co.plan_key().project_id)
                .await
                .unwrap()
                .held,
            before
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM synthetic_operations")
                .fetch_one(&f.store.pool)
                .await
                .unwrap(),
            2
        );
        stop.send(()).unwrap();
        task.await.unwrap();
    }
}
#[tokio::test]
async fn transport_limits_bound_tls_connections_and_unauthenticated_request_rate() {
    let f = Fixture::new().await;
    let (s, _) = f.server_limits(ResourceLimits::new(2, 1, 1).unwrap()).await;
    let (url, stop, task) = listen(s.clone()).await;
    let port = url::Url::parse(&url).unwrap().port().unwrap();
    let connector = tokio_rustls::TlsConnector::from(tls::fixture_client(f.cert.cert.clone()));
    let name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let one = connector
        .connect(
            name.clone(),
            tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap(),
        )
        .await
        .unwrap();
    let two = connector
        .connect(
            name.clone(),
            tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap(),
        )
        .await
        .unwrap();
    let socket = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    tokio::time::pause();
    let third = tokio::time::timeout(Duration::from_secs(1), connector.connect(name, socket)).await;
    assert!(
        third.is_err(),
        "third handshake remains backpressured while two connection slots are occupied; the five-second header deadlines have not expired"
    );
    tokio::time::resume();
    drop(one);
    drop(two);
    stop.send(()).unwrap();
    task.await.unwrap();
    let (s, v) = f.server_limits(ResourceLimits::new(8, 2, 1).unwrap()).await;
    let (url, stop, task) = listen(s.clone()).await;
    let (b, secret, proof, binding, id) = f.prepare(&s, &v, "task-a").await;
    let mut c = f.cert.client(&url);
    c.bootstrap(id, b.id, &secret, proof, &binding)
        .await
        .unwrap();
    assert_eq!(
        c.assignment(RequestId::new("throttled").unwrap())
            .await
            .unwrap_err(),
        ClientError::Rejected(ErrorCode::RateLimited)
    );
    assert_eq!(s.diagnostics().active, 0);
    stop.send(()).unwrap();
    task.await.unwrap();
}
#[tokio::test]
async fn network_bearers_are_absent_from_sqlite_wal_snapshot_and_redacted_errors() {
    let f = Fixture::new().await;
    let (s, v) = f.server().await;
    let (b, secret, proof, binding, id) = f.prepare(&s, &v, "task-a").await;
    let (url, stop, task) = listen(s.clone()).await;
    let mut c = f.cert.client(&url);
    c.bootstrap(id, b.id, &secret, proof, &binding)
        .await
        .unwrap();
    let mut markers = Zeroizing::new(vec![secret.0.0.as_bytes().to_vec()]);
    let (status, body) = c
        .fixture_request(|token| {
            markers.push(token.as_bytes().to_vec());
            b"{bad".to_vec()
        })
        .await;
    assert_eq!(status, 400);
    for marker in markers.iter() {
        assert!(!body.windows(marker.len()).any(|w| w == marker));
    }
    let backup = f.root.path().join("ordinary-snapshot.sqlite");
    sqlx::query("VACUUM INTO ?")
        .bind(backup.to_str().unwrap())
        .execute(&f.store.pool)
        .await
        .unwrap();
    for path in [
        f.root.path().join("data/state.sqlite"),
        f.root.path().join("data/state.sqlite-wal"),
        backup,
    ] {
        if path.exists() {
            let bytes = fs::read(path).unwrap();
            for marker in markers.iter() {
                assert!(
                    !bytes.windows(marker.len()).any(|w| w == marker),
                    "bearer material must not persist"
                );
            }
        }
    }
    let debug = format!(
        "{c:?} {:?}",
        ArtifactUpload {
            sha256: Digest::new("1".repeat(64)).unwrap(),
            data: "untrusted-sensitive-artifact".into()
        }
    );
    assert!(!debug.contains("untrusted-sensitive-artifact"));
    stop.send(()).unwrap();
    task.await.unwrap();
}

#[tokio::test]
async fn expired_revoked_and_replaced_credentials_have_distinct_network_denials() {
    for reason in ["expired", "revoked", "replaced"] {
        let f = Fixture::new().await;
        let (mut s, v) = f.server().await;
        let (b, secret, proof, binding, id) = f.prepare(&s, &v, "task-a").await;
        install_revoker(&mut s, &binding, &b.enrollment);
        let (url, stop, task) = listen(s.clone()).await;
        let mut c = f.cert.client(&url);
        c.bootstrap(id, b.id, &secret, proof, &binding)
            .await
            .unwrap();
        let script = script(&c).await;
        for index in 0..4 {
            send_event(&f, &c, &script, index).await;
        }
        let old = c.fixture_stolen_credential();
        let expected = match reason {
            "expired" => {
                f.clock.advance_to(synthetic::T + 900).unwrap();
                ErrorCode::ExpiredCredential
            }
            "revoked" => {
                s.security
                    .revoke_worker(&WorkerRevocation {
                        enrollment: c.metadata().unwrap().enrollment.clone(),
                        expected_security_epoch: 1,
                        request_id: RequestId::new("scoped-revocation").unwrap(),
                        evidence: synthetic::e("synthetic-incident"),
                    })
                    .await
                    .unwrap();
                ErrorCode::RevokedCredential
            }
            _ => {
                f.clock.advance_to(synthetic::T + 60).unwrap();
                let (id, proof) = renewal(&f, &s, &c, "rotate-current").await;
                c.renew(id, proof).await.unwrap();
                assert_eq!(c.metadata().unwrap().generation, 2);
                ErrorCode::StaleVersion
            }
        };
        assert_eq!(
            old.assignment(RequestId::new("denied-old-authority").unwrap())
                .await
                .unwrap_err(),
            ClientError::Rejected(expected)
        );
        stop.send(()).unwrap();
        task.await.unwrap();
    }
}

#[tokio::test]
async fn physical_tls_connection_age_is_bounded_independently_of_header_or_handler_activity() {
    let f = Fixture::new().await;
    let limits = ResourceLimits {
        connection_seconds: 2,
        ..ResourceLimits::default()
    };
    let (s, _) = f.server_limits(limits).await;
    let (url, stop, task) = listen(s).await;
    let port = url::Url::parse(&url).unwrap().port().unwrap();
    let connector = tokio_rustls::TlsConnector::from(tls::fixture_client(f.cert.cert.clone()));
    let name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let mut tls = connector
        .connect(
            name,
            tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .unwrap(),
        )
        .await
        .unwrap();
    tls.write_all(b"POST /v1/worker HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 0\r\n\r\n").await.unwrap();
    let mut response = Vec::new();
    while !response
        .windows(b"{\"version\":1,\"code\":\"unauthenticated\"}".len())
        .any(|b| b == br#"{"version":1,"code":"unauthenticated"}"#)
    {
        let mut chunk = [0u8; 256];
        let n = tokio::time::timeout(Duration::from_secs(2), tls.read(&mut chunk))
            .await
            .unwrap()
            .unwrap();
        assert!(n > 0);
        response.extend_from_slice(&chunk[..n]);
        assert!(response.len() < 2048);
    }
    assert!(response.starts_with(b"HTTP/1.1 401"));
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(3)).await;
    tokio::task::yield_now().await;
    let mut byte = [0u8; 1];
    let closed = tokio::time::timeout(Duration::from_millis(1), tls.read(&mut byte)).await;
    assert!(
        matches!(closed, Ok(Ok(0)) | Ok(Err(_))),
        "whole TLS connection must expire before the 5-second header deadline, even when no handler is running"
    );
    tokio::time::resume();
    stop.send(()).unwrap();
    task.await.unwrap();
}
