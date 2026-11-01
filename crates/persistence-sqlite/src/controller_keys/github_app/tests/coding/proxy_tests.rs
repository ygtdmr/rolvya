//! Real worker TLS, SQLite and encrypted Broker; only provider DNS/CA is private.
use super::*;
use crate::controller_keys::worker_auth::https::proxy::{ProxyReply, ProxyRequest};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
impl Session {
    fn proxy(&self, step: u16, previous: Vec<String>) -> ProxyRequest {
        ProxyRequest {
            version: 1,
            work: self.worker.work().identity.clone(),
            step,
            previous,
        }
    }
    async fn route(
        &self,
    ) -> (
        tokio::task::JoinHandle<()>,
        Arc<std::sync::atomic::AtomicUsize>,
    ) {
        let (endpoint, provider, count) =
            agent::fixture::proxy_provider(self.worker.work(), Adapter::OpenAiApi).await;
        crate::controller_keys::worker_auth::https::proxy::fixture_route(
            &self.server,
            self.worker.work(),
            endpoint,
        );
        (provider, count)
    }
    async fn call(
        &self,
        r: ProxyRequest,
    ) -> Result<ProxyReply, crate::controller_keys::worker_auth::https::client::ClientError> {
        self.worker.proxy_request(r).await
    }
}
#[tokio::test]
async fn proxy_duplicate_concurrent_requests_have_one_effect_and_one_observation() {
    let s = Session::open_proxy(Some(0), true).await;
    let (provider, count) = s.route().await;
    let before = s.f.store.financial_snapshot(&s.f.project).await.unwrap();
    let secret_events = s.f.count("secret_events").await;
    let r = s.proxy(1, vec![]);
    let (a, b) = tokio::join!(s.call(r.clone()), s.call(r.clone()));
    let replies: Vec<_> = [a, b]
        .into_iter()
        .filter_map(|outcome| match outcome {
            Ok(reply) => Some(reply),
            Err(e) => {
                assert!(matches!(
                    e,
                    crate::controller_keys::worker_auth::https::client::ClientError::Rejected(
                        crate::controller_keys::worker_auth::https::wire::ErrorCode::RateLimited
                    )
                ));
                None
            }
        })
        .collect();
    assert_eq!(replies.iter().filter(|r| r.receipt.execute).count(), 1);
    assert_eq!(replies.iter().filter(|r| r.response.is_ok()).count(), 1);
    let again = s.call(r).await.unwrap();
    assert!(!again.receipt.execute);
    assert_eq!(again.response, Err(agent::AgentError::Replay));
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(s.f.count("secret_events").await, secret_events + 1);
    assert_eq!(s.f.count("coding_agent_calls").await, 1);
    assert_eq!(s.f.count("coding_agent_observations").await, 1);
    assert_eq!(
        s.f.store.financial_snapshot(&s.f.project).await.unwrap(),
        before
    );
    provider.abort();
    s.close().await;
}
#[tokio::test]
async fn proxy_history_is_exact_controller_owned_and_terminal_cannot_continue() {
    let s = Session::open_proxy(Some(0), true).await;
    let (provider, count) = s.route().await;
    let first = s.call(s.proxy(1, vec![])).await.unwrap().response.unwrap();
    let mut modified = B64.decode(&first).unwrap();
    modified.push(b' ');
    assert!(
        s.call(s.proxy(2, vec![B64.encode(modified)]))
            .await
            .is_err()
    );
    assert!(
        s.call(s.proxy(2, vec![B64.encode(b"{\"verification_passed\":true}")]))
            .await
            .is_err()
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert!(
        s.step(s.start(2)).await.is_err(),
        "worker-origin calls cannot continue a proxy conversation"
    );
    let mut history = vec![first];
    for step in 2..=5 {
        let next = s
            .call(s.proxy(step, history.clone()))
            .await
            .unwrap()
            .response
            .unwrap();
        history.push(next);
    }
    assert!(s.call(s.proxy(6, history)).await.is_err());
    assert_eq!(count.load(Ordering::SeqCst), 5);
    assert_eq!(s.f.count("coding_agent_calls").await, 5);
    provider.abort();
    s.close().await;
}
#[tokio::test]
async fn proxy_unknown_runtime_and_unsupported_request_fields_are_denied_without_intent() {
    let s = Session::open_proxy(Some(0), true).await;
    assert!(s.call(s.proxy(1, vec![])).await.is_err());
    let mut r = s.proxy(1, vec![]);
    r.work = commitment_bytes(b"wrong-work");
    assert!(s.call(r).await.is_err());
    let base = serde_json::to_value(s.proxy(1, vec![])).unwrap();
    for (key, value) in [
        ("url", json!("https://attacker.invalid")),
        ("headers", json!({"authorization":"forged"})),
        ("account", json!("other")),
        ("model", json!("other")),
        ("budget", json!(999999)),
        ("verified", json!(true)),
    ] {
        let mut r = base.clone();
        r[key] = value;
        assert!(serde_json::from_value::<ProxyRequest>(r).is_err());
    }
    for (step, previous) in [
        (0, vec![]),
        (9, vec![]),
        (2, vec![]),
        (2, vec!["A".repeat(91000)]),
    ] {
        assert!(s.proxy(step, previous).validate().is_err());
    }
    assert_eq!(s.f.count("coding_agent_calls").await, 0);
    s.close().await;
}
#[tokio::test]
async fn proxy_revoked_secret_and_account_refresh_block_new_requests() {
    for mutation in 0..3 {
        let s = Session::open_proxy(Some(0), true).await;
        let (provider, count) = s.route().await;
        let before = s.f.store.financial_snapshot(&s.f.project).await.unwrap();
        let m = s.worker.work().model_account.as_ref().unwrap();
        match mutation {
            0 => {
                sqlx::query("UPDATE secret_connections SET revision=2,state='revoked' WHERE project_id=? AND connection_id=?")
                .bind(s.f.project.as_str()).bind(&m.credential_connection).execute(&s.f.store.pool).await.unwrap();
            }
            1 => {
                sqlx::query("INSERT INTO provider_refresh_locks VALUES(?, 'uncertain-refresh', ?, 'in_progress', ?)")
                .bind(&m.connection).bind(m.connection_version).bind(NOW+300).execute(&s.f.store.pool).await.unwrap();
            }
            _ => {
                s.f.clock.advance_to(NOW + 90).unwrap();
            }
        }
        assert!(s.call(s.proxy(1, vec![])).await.is_err());
        assert_eq!(count.load(Ordering::SeqCst), 0);
        assert_eq!(s.f.count("coding_agent_observations").await, 0);
        assert_eq!(
            s.f.store.financial_snapshot(&s.f.project).await.unwrap(),
            before
        );
        provider.abort();
        s.close().await;
    }
}
#[tokio::test]
async fn proxy_ignored_intent_or_observation_writes_cannot_claim_success() {
    for table in ["coding_agent_calls", "coding_agent_observations"] {
        let s = Session::open_proxy(Some(0), true).await;
        let (provider, count) = s.route().await;
        let before = s.f.store.financial_snapshot(&s.f.project).await.unwrap();
        sqlx::query(&format!(
            "CREATE TRIGGER ignore_proxy BEFORE INSERT ON {table} BEGIN SELECT RAISE(IGNORE); END"
        ))
        .execute(&s.f.store.pool)
        .await
        .unwrap();
        assert!(s.call(s.proxy(1, vec![])).await.is_err());
        assert_eq!(
            count.load(Ordering::SeqCst),
            usize::from(table == "coding_agent_observations")
        );
        assert_eq!(s.f.count("coding_agent_observations").await, 0);
        assert_eq!(
            s.f.store.financial_snapshot(&s.f.project).await.unwrap(),
            before
        );
        if table == "coding_agent_observations" {
            assert!(!s.call(s.proxy(1, vec![])).await.unwrap().receipt.execute);
            assert_eq!(count.load(Ordering::SeqCst), 1);
        }
        provider.abort();
        s.close().await;
    }
}
#[tokio::test]
async fn proxy_upstream_errors_and_credential_reflection_never_reach_coding_workspace() {
    for case in 0..3 {
        let s = Session::open_proxy(Some(0), true).await;
        let key = format!("synthetic-provider-key-{}", s.worker.work().connection);
        let (body, status) = match case {
            0 => (json!({"error":{"message":key}}).to_string(), 401),
            1 => {
                let escaped: String = key.bytes().map(|b| format!("\\u{b:04x}")).collect();
                let args = format!(" \n{{\"path\":\"src/alpha.rs\",\"content\":\"{escaped}\"}}");
                (json!({"id":"reflection","model":"synthetic/model","status":"completed","output":[{"type":"function_call","call_id":"stolen","name":"write_file","arguments":args}],"usage":{"input_tokens":1,"output_tokens":1}}).to_string(),200)
            }
            _ => ("{\"truncated\":".into(), 200),
        };
        let (endpoint, provider, count) = agent::fixture::proxy_response(body, status, None).await;
        crate::controller_keys::worker_auth::https::proxy::fixture_route(
            &s.server,
            s.worker.work(),
            endpoint,
        );
        let before = s.f.store.financial_snapshot(&s.f.project).await.unwrap();
        let reply = s.call(s.proxy(1, vec![])).await.unwrap();
        assert!(reply.response.is_err());
        assert!(!serde_json::to_string(&reply).unwrap().contains(&key));
        assert!(s.call(s.proxy(2, vec![B64.encode(b"{}")])).await.is_err());
        assert!(!s.call(s.proxy(1, vec![])).await.unwrap().receipt.execute);
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(
            s.f.store.financial_snapshot(&s.f.project).await.unwrap(),
            before
        );
        provider.abort();
        s.close().await;
    }
}
#[tokio::test]
async fn proxy_network_io_releases_writer_and_inflight_revocation_withholds_response() {
    let s = Session::open_proxy(Some(0), true).await;
    let arrived = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let (endpoint, provider, count) =
        agent::fixture::proxy_response("{}".into(), 200, Some((arrived.clone(), release.clone())))
            .await;
    crate::controller_keys::worker_auth::https::proxy::fixture_route(
        &s.server,
        s.worker.work(),
        endpoint,
    );
    let before = s.f.store.financial_snapshot(&s.f.project).await.unwrap();
    let changed = async {
        tokio::time::timeout(std::time::Duration::from_secs(4), arrived.notified())
            .await
            .unwrap();
        // This writer must finish while the upstream response remains paused.
        tokio::time::timeout(std::time::Duration::from_secs(2),async {
            let mut tx=s.f.store.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
            sqlx::query("UPDATE secret_connections SET revision=2,state='revoked' WHERE project_id=? AND connection_id=?")
                .bind(s.f.project.as_str()).bind(&s.worker.work().model_account.as_ref().unwrap().credential_connection)
                .execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
        }).await.unwrap();
        release.notify_one();
    };
    let (reply, ()) = tokio::join!(s.call(s.proxy(1, vec![])), changed);
    assert!(reply.is_err());
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(s.f.count("coding_agent_calls").await, 1);
    assert_eq!(s.f.count("coding_agent_observations").await, 0);
    assert!(s.call(s.proxy(1, vec![])).await.is_err());
    assert_eq!(
        s.f.store.financial_snapshot(&s.f.project).await.unwrap(),
        before
    );
    provider.abort();
    s.close().await;
}
#[tokio::test]
async fn proxy_timeout_preserves_intent_and_replay_never_repeats_provider_effect() {
    let s = Session::open_proxy(Some(0), true).await;
    let arrived = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let (endpoint, provider, count) =
        agent::fixture::proxy_response("{}".into(), 200, Some((arrived, release))).await;
    crate::controller_keys::worker_auth::https::proxy::fixture_route(
        &s.server,
        s.worker.work(),
        endpoint,
    );
    let before = s.f.store.financial_snapshot(&s.f.project).await.unwrap();
    let reply = s.call(s.proxy(1, vec![])).await.unwrap();
    assert_eq!(reply.response, Err(agent::AgentError::Timeout));
    assert!(!s.call(s.proxy(1, vec![])).await.unwrap().receipt.execute);
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(s.f.count("coding_agent_calls").await, 1);
    assert_eq!(s.f.count("coding_agent_observations").await, 1);
    assert_eq!(
        s.f.store.financial_snapshot(&s.f.project).await.unwrap(),
        before
    );
    provider.abort();
    s.close().await;
}
