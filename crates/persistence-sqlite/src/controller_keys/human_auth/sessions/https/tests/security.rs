use super::*;
const GET: reqwest::Method = reqwest::Method::GET;
const POST: reqwest::Method = reqwest::Method::POST;
#[tokio::test]
async fn genuine_totp_and_one_use_recovery_complete_only_the_bound_pending_login() {
    let r = Running::new(true).await;
    let mut b = Browser::new(&r);
    assert_eq!(b.login(&r, false).await.1["state"], "factor_required");
    let csrf = b.initialize().await;
    let code = super::super::super::tests::code(
        r.seed.totp.as_ref().unwrap(),
        r.seed.clock.load(Ordering::SeqCst),
    );
    let (status, result) = b
        .call(
            POST,
            "/api/control/v1/auth/factor",
            Some(serde_json::json!({"kind":"totp","code":code.0.as_str()})),
            Some(&csrf),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(result["state"], "authenticated");
    let (_, session) = b.session().await;
    assert_eq!(session["assurance"], "totp");
    let csrf = session["csrf"].as_str().unwrap();
    assert_eq!(
        b.call(
            POST,
            "/api/control/v1/auth/logout",
            Some(serde_json::json!({})),
            Some(csrf)
        )
        .await
        .0,
        200
    );
    assert_eq!(b.login(&r, false).await.1["state"], "factor_required");
    let csrf = b.initialize().await;
    let recovery = r.seed.recovery[0].as_str();
    assert_eq!(
        b.call(
            POST,
            "/api/control/v1/auth/factor",
            Some(serde_json::json!({"kind":"recovery","code":recovery})),
            Some(&csrf)
        )
        .await
        .0,
        200
    );
    assert_eq!(b.session().await.1["assurance"], "recovery");
    let mut other = Browser::new(&r);
    assert_eq!(other.login(&r, false).await.1["state"], "factor_required");
    let csrf = other.initialize().await;
    assert_eq!(
        other
            .call(
                POST,
                "/api/control/v1/auth/factor",
                Some(serde_json::json!({"kind":"recovery","code":recovery})),
                Some(&csrf)
            )
            .await
            .0,
        401
    );
    assert!(!other.cookies.contains_key("__Host-rolvya"));
    r.close().await;
}
#[tokio::test]
async fn transport_denials_are_uniform_scoped_and_have_no_session_side_effects() {
    let r = Running::new(false).await;
    let mut b = Browser::new(&r);
    let csrf = b.initialize().await;
    let mut denials = Vec::new();
    for email in [r.seed.email.as_str(), "unknown@example.invalid"] {
        denials.push(
            b.call(
                POST,
                "/api/control/v1/auth/password",
                Some(serde_json::json!({"email":email,"password":"synthetic-wrong-password"})),
                Some(&csrf),
            )
            .await,
        );
    }
    assert_eq!(denials[0], denials[1]);
    assert_eq!(denials[0].0, 401);
    let password = serde_json::json!({"email":r.seed.email.as_str(),"password":std::str::from_utf8(&r.seed.credential.0).unwrap()});
    assert_eq!(
        b.call(
            POST,
            "/api/control/v1/auth/password",
            Some(password.clone()),
            Some("wrong")
        )
        .await
        .0,
        401
    );
    let cookie = b
        .cookies
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ");
    for (key, value) in [
        ("origin", "https://evil.invalid"),
        ("x-forwarded-proto", "https"),
        ("authorization", "Bearer synthetic-worker-credential"),
    ] {
        let mut req = r
            .http
            .post(format!("{}/api/control/v1/auth/password", r.base))
            .header("sec-fetch-site", "same-origin")
            .header("cookie", &cookie)
            .header("x-rolvya-csrf", &csrf)
            .header("content-type", "application/json")
            .body(serde_json::to_vec(&password).unwrap());
        if key != "origin" {
            req = req.header("origin", &r.base)
        }
        assert_eq!(
            req.header(key, value)
                .send()
                .await
                .unwrap()
                .status()
                .as_u16(),
            401
        );
    }
    for body in [
        "{".to_owned(),
        "x".repeat(4097),
        "{\"unknown\":true}".to_owned(),
    ] {
        let response = r
            .http
            .post(format!("{}/api/control/v1/auth/password", r.base))
            .header("sec-fetch-site", "same-origin")
            .header("origin", &r.base)
            .header("cookie", &cookie)
            .header("x-rolvya-csrf", &csrf)
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 400);
    }
    assert!(!b.cookies.contains_key("__Host-rolvya"));
    assert_eq!(b.login(&r, true).await.0, 200);
    let (_, projects) = b.call(GET, "/api/control/v1/projects", None, None).await;
    assert_eq!(projects.as_array().unwrap().len(), 1);
    assert_eq!(
        b.call(
            GET,
            "/api/control/v1/projects/project-b/snapshot",
            None,
            None
        )
        .await
        .0,
        403
    );
    assert_eq!(
        b.call(
            GET,
            "/api/control/v1/projects/project-a/snapshot",
            None,
            None
        )
        .await
        .1["role"],
        "viewer"
    );
    r.close().await;
}
#[tokio::test]
async fn unavailable_outgoing_mail_never_claims_delivery_or_issues_an_email_session() {
    let mut seed = browser_seed(false).await;
    seed.auth.mail = Arc::new(Unimplemented);
    seed.auth.delivery = Arc::new(Unimplemented);
    let r = Running::from_seed(seed, None).await;
    let mut b = Browser::new(&r);
    let (_, context) = b
        .call(GET, "/api/control/v1/auth/context", None, None)
        .await;
    assert_eq!(context["email"], false);
    let csrf = context["csrf"].as_str().unwrap();
    assert_eq!(
        b.call(
            POST,
            "/api/control/v1/auth/email",
            Some(serde_json::json!({"email":r.seed.email.as_str(),"kind":"magic_link"})),
            Some(csrf)
        )
        .await
        .0,
        503
    );
    assert!(!b.cookies.contains_key("__Host-rolvya"));
    r.close().await;
}
#[tokio::test]
async fn concurrent_rotation_has_one_winner_and_lost_response_cannot_restore_old_generation() {
    let r = Running::new(false).await;
    let mut b = Browser::new(&r);
    assert_eq!(b.login(&r, false).await.0, 200);
    let csrf = b.session().await.1["csrf"].as_str().unwrap().to_owned();
    let mut stale = b.clone();
    let mut second = b.clone();
    let (a, c) = tokio::join!(
        b.call(
            POST,
            "/api/control/v1/auth/renew",
            Some(serde_json::json!({})),
            Some(&csrf)
        ),
        second.call(
            POST,
            "/api/control/v1/auth/renew",
            Some(serde_json::json!({})),
            Some(&csrf)
        )
    );
    let mut statuses = [a.0, c.0];
    statuses.sort();
    assert_eq!(statuses, [200, 401]);
    assert_eq!(stale.session().await.0, 401);
    let current = if a.0 == 200 { &mut b } else { &mut second };
    assert_eq!(current.session().await.1["generation"], 2);
    r.close().await;
}
#[tokio::test]
async fn interaction_expiry_generation_concurrency_and_ignored_writes_fail_closed() {
    let r = Running::new(false).await;
    let mut b = Browser::new(&r);
    assert_eq!(b.login(&r, false).await.0, 200);
    let csrf = b.session().await.1["csrf"].as_str().unwrap().to_owned();
    let prepare =
        |x: &serde_json::Value| serde_json::json!({"intent":x["intent"],"issued":x["issued"]});
    let (_, intent) = b
        .call(
            POST,
            "/api/control/v1/auth/activity",
            Some(serde_json::json!({})),
            Some(&csrf),
        )
        .await;
    let mut protected: Vec<Zeroizing<Vec<u8>>> = b
        .cookies
        .values()
        .map(|v| Zeroizing::new(v.as_bytes().to_vec()))
        .collect();
    protected.push(Zeroizing::new(r.seed.credential.0.to_vec()));
    protected.push(Zeroizing::new(csrf.as_bytes().to_vec()));
    protected.push(Zeroizing::new(
        intent["intent"].as_str().unwrap().as_bytes().to_vec(),
    ));
    let snapshot = r.seed._root.path().join("browser-secret-scan.sqlite");
    r.seed.store.snapshot(&snapshot).await.unwrap();
    let mut files = vec![snapshot];
    for entry in std::fs::read_dir(r.seed._root.path().join("data")).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            files.push(entry.path());
        }
    }
    for file in files {
        let bytes = tokio::fs::read(file).await.unwrap();
        assert!(
            protected.iter().all(|secret| !bytes
                .windows(secret.len())
                .any(|part| part == secret.as_slice())),
            "recognizable browser credential appeared in database/WAL/snapshot"
        );
    }
    r.seed.clock.fetch_add(60, Ordering::SeqCst);
    assert_eq!(
        b.call(
            POST,
            "/api/control/v1/auth/interaction",
            Some(prepare(&intent)),
            Some(&csrf)
        )
        .await
        .0,
        401
    );
    let (_, intent) = b
        .call(
            POST,
            "/api/control/v1/auth/activity",
            Some(serde_json::json!({})),
            Some(&csrf),
        )
        .await;
    let mut other = b.clone();
    let input = prepare(&intent);
    let (a, c) = tokio::join!(
        b.call(
            POST,
            "/api/control/v1/auth/interaction",
            Some(input.clone()),
            Some(&csrf)
        ),
        other.call(
            POST,
            "/api/control/v1/auth/interaction",
            Some(input),
            Some(&csrf)
        )
    );
    let mut statuses = [a.0, c.0];
    statuses.sort();
    assert_eq!(statuses, [200, 401]);
    let (_, intent) = b
        .call(
            POST,
            "/api/control/v1/auth/activity",
            Some(serde_json::json!({})),
            Some(&csrf),
        )
        .await;
    let before = b.session().await.1["lastHumanAt"].clone();
    r.seed.clock.fetch_add(10, Ordering::SeqCst);
    sqlx::query("CREATE TRIGGER browser_test_ignore BEFORE UPDATE ON human_sessions BEGIN SELECT RAISE(IGNORE); END").execute(&r.seed.store.pool).await.unwrap();
    assert_eq!(
        b.call(
            POST,
            "/api/control/v1/auth/interaction",
            Some(prepare(&intent)),
            Some(&csrf)
        )
        .await
        .0,
        401
    );
    sqlx::query("DROP TRIGGER browser_test_ignore")
        .execute(&r.seed.store.pool)
        .await
        .unwrap();
    assert_eq!(b.session().await.1["lastHumanAt"], before);
    // Rollback preserved the unconsumed intent; a fresh valid effect is allowed once.
    assert_eq!(
        b.call(
            POST,
            "/api/control/v1/auth/interaction",
            Some(prepare(&intent)),
            Some(&csrf)
        )
        .await
        .0,
        200
    );
    let (_, intent) = b
        .call(
            POST,
            "/api/control/v1/auth/activity",
            Some(serde_json::json!({})),
            Some(&csrf),
        )
        .await;
    assert_eq!(
        b.call(
            POST,
            "/api/control/v1/auth/renew",
            Some(serde_json::json!({})),
            Some(&csrf)
        )
        .await
        .0,
        200
    );
    let csrf = b.session().await.1["csrf"].as_str().unwrap().to_owned();
    assert_eq!(
        b.call(
            POST,
            "/api/control/v1/auth/interaction",
            Some(prepare(&intent)),
            Some(&csrf)
        )
        .await
        .0,
        401
    );
    r.close().await;
}
#[tokio::test]
async fn live_project_reads_do_not_exhaust_login_and_logout_rate_capacity() {
    let r = Running::new(false).await;
    let mut b = Browser::new(&r);
    assert_eq!(b.login(&r, false).await.0, 200);
    let csrf = b.session().await.1["csrf"].as_str().unwrap().to_owned();
    // Supported live reads across tabs must not consume the authentication bucket.
    for _ in 0..160 {
        assert_eq!(
            b.call(GET, "/api/control/v1/projects", None, None).await.0,
            200
        );
    }
    assert_eq!(
        b.call(
            POST,
            "/api/control/v1/auth/logout",
            Some(serde_json::json!({})),
            Some(&csrf)
        )
        .await
        .0,
        200
    );
    r.close().await;
}
#[tokio::test]
async fn logout_requires_actual_revocation_and_audit_even_when_sqlite_ignores_writes() {
    let r = Running::new(false).await;
    use sqlx::Connection;
    let mut control = sqlx::SqliteConnection::connect_with(&r.seed.store.pool.connect_options())
        .await
        .unwrap();
    let mut first = Browser::new(&r);
    let mut second = Browser::new(&r);
    let mut viewer = Browser::new(&r);
    assert_eq!(first.login(&r, false).await.0, 200);
    assert_eq!(second.login(&r, false).await.0, 200);
    assert_eq!(viewer.login(&r, true).await.0, 200);
    let (_, session) = first.session().await;
    let csrf = session["csrf"].as_str().unwrap().to_owned();
    let other = second.session().await.1["session"]
        .as_str()
        .unwrap()
        .to_owned();
    let events: i64 = sqlx::query_scalar("SELECT count(*) FROM human_session_events")
        .fetch_one(&r.seed.store.pool)
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER browser_ignore_revocation BEFORE UPDATE ON human_sessions WHEN NEW.revoked_at IS NOT NULL BEGIN SELECT RAISE(IGNORE); END").execute(&mut control).await.unwrap();
    assert_eq!(
        first
            .call(
                POST,
                "/api/control/v1/auth/logout",
                Some(serde_json::json!({})),
                Some(&csrf)
            )
            .await
            .0,
        503
    );
    assert_eq!(first.session().await.0, 200);
    sqlx::query("DROP TRIGGER browser_ignore_revocation")
        .execute(&mut control)
        .await
        .unwrap();
    // A partial all-device update must roll back the already-updated families.
    sqlx::query(&format!("CREATE TRIGGER browser_ignore_revocation BEFORE UPDATE ON human_sessions WHEN NEW.revoked_at IS NOT NULL AND OLD.session_id='{other}' BEGIN SELECT RAISE(IGNORE); END")).execute(&mut control).await.unwrap();
    assert_eq!(
        first
            .call(
                POST,
                "/api/control/v1/auth/logout-all",
                Some(serde_json::json!({})),
                Some(&csrf)
            )
            .await
            .0,
        503
    );
    assert_eq!(first.session().await.0, 200);
    assert_eq!(second.session().await.0, 200);
    sqlx::query("DROP TRIGGER browser_ignore_revocation")
        .execute(&mut control)
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER browser_ignore_audit BEFORE INSERT ON human_session_events BEGIN SELECT RAISE(IGNORE); END").execute(&mut control).await.unwrap();
    assert_eq!(
        first
            .call(
                POST,
                "/api/control/v1/auth/logout",
                Some(serde_json::json!({})),
                Some(&csrf)
            )
            .await
            .0,
        503
    );
    assert_eq!(first.session().await.0, 200);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM human_session_events")
            .fetch_one(&r.seed.store.pool)
            .await
            .unwrap(),
        events
    );
    sqlx::query("DROP TRIGGER browser_ignore_audit")
        .execute(&mut control)
        .await
        .unwrap();
    assert_eq!(
        first
            .call(
                POST,
                "/api/control/v1/auth/logout-all",
                Some(serde_json::json!({})),
                Some(&csrf)
            )
            .await
            .0,
        200
    );
    assert_eq!(second.session().await.0, 401);
    assert_eq!(viewer.session().await.0, 200);
    r.close().await;
}
#[tokio::test]
async fn logout_revokes_same_browser_pending_factor_authority_even_if_block_writes_are_ignored() {
    use sqlx::{Connection, Executor};
    let r = Running::new(true).await;
    let mut control = sqlx::SqliteConnection::connect_with(&r.seed.store.pool.connect_options())
        .await
        .unwrap();
    let mut b = Browser::new(&r);
    assert_eq!(b.login(&r, false).await.1["state"], "factor_required");
    let preauth = b.initialize().await;
    let code = super::super::super::tests::code(
        r.seed.totp.as_ref().unwrap(),
        r.seed.clock.load(Ordering::SeqCst),
    );
    assert_eq!(
        b.call(
            POST,
            "/api/control/v1/auth/factor",
            Some(serde_json::json!({"kind":"totp","code":code.0.as_str()})),
            Some(&preauth)
        )
        .await
        .0,
        200
    );
    let csrf = b.session().await.1["csrf"].as_str().unwrap().to_owned();
    assert_eq!(b.login(&r, false).await.1["state"], "factor_required");
    let mut pending = b.clone();
    r.seed.clock.fetch_add(30, Ordering::SeqCst);
    let next = super::super::super::tests::code(
        r.seed.totp.as_ref().unwrap(),
        r.seed.clock.load(Ordering::SeqCst),
    );
    control.execute("CREATE TRIGGER browser_ignore_primary_block BEFORE INSERT ON human_primary_blocks BEGIN SELECT RAISE(IGNORE); END").await.unwrap();
    let status = b
        .call(
            POST,
            "/api/control/v1/auth/logout",
            Some(serde_json::json!({})),
            Some(&csrf),
        )
        .await
        .0;
    if status == 200 {
        // Reproduce the unsafe effect, rather than only an unexpected status.
        assert_eq!(
            pending
                .call(
                    POST,
                    "/api/control/v1/auth/factor",
                    Some(serde_json::json!({"kind":"totp","code":next.0.as_str()})),
                    Some(&preauth)
                )
                .await
                .0,
            401,
            "logout claimed success but left same-browser pending factor authority usable"
        );
    }
    assert_eq!(status, 503);
    assert_eq!(b.session().await.0, 200); // Unconfirmed logout rolled back all effects.
    control
        .execute("DROP TRIGGER browser_ignore_primary_block")
        .await
        .unwrap();
    assert_eq!(
        b.call(
            POST,
            "/api/control/v1/auth/logout",
            Some(serde_json::json!({})),
            Some(&csrf)
        )
        .await
        .0,
        200
    );
    assert_eq!(
        pending
            .call(
                POST,
                "/api/control/v1/auth/factor",
                Some(serde_json::json!({"kind":"totp","code":next.0.as_str()})),
                Some(&preauth)
            )
            .await
            .0,
        401
    );
    r.close().await;
}
