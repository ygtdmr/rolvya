use super::*;
use axum::{Extension, body::to_bytes, extract::State};
use serde::Deserialize;
type Cookies = BTreeMap<String, Zeroizing<String>>;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PasswordInput {
    email: String,
    password: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FactorInput {
    kind: String,
    code: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmailInput {
    email: String,
    kind: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfirmInput {
    challenge: String,
    secret: String,
    kind: String,
    confirm: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InteractionInput {
    intent: String,
    issued: i64,
}
fn cookies(r: &Request<Body>) -> Result<Cookies, AuthError> {
    let mut out = BTreeMap::new();
    let mut n = 0;
    for header in r.headers().get_all(header::COOKIE) {
        n += header.len();
        if n > 4096 {
            return Err(AuthError::InvalidInput);
        }
        for part in header
            .to_str()
            .map_err(|_| AuthError::InvalidInput)?
            .split(';')
        {
            let (key, value) = part.trim().split_once('=').ok_or(AuthError::InvalidInput)?;
            if out
                .insert(key.to_owned(), Zeroizing::new(value.to_owned()))
                .is_some()
                || out.len() > 32
            {
                return Err(AuthError::InvalidInput);
            }
        }
    }
    Ok(out)
}
fn session(c: &Cookies) -> Result<SessionHandle, AuthError> {
    SessionHandle::parse(c.get("__Host-rolvya").ok_or(AuthError::Denied)?.to_string())
        .map_err(|_| AuthError::Denied)
}
fn decode<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, AuthError> {
    serde_json::from_slice(body).map_err(|_| AuthError::InvalidInput)
}
fn challenge_kind(k: &str) -> Result<ChallengeKind, AuthError> {
    match k {
        "magic_link" => Ok(ChallengeKind::MagicLink),
        "otp" => Ok(ChallengeKind::Otp),
        _ => Err(AuthError::InvalidInput),
    }
}
pub(super) async fn handle(
    State(state): State<Arc<HumanBrowserServer>>,
    Extension(channel): Extension<Channel>,
    request: Request<Body>,
) -> Response<Body> {
    match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        state.process(channel, request),
    )
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => failure(e),
        Err(_) => failure(AuthError::Unavailable),
    }
}
impl HumanBrowserServer {
    fn flow(&self, cookies: &Cookies, now: i64) -> Result<([u8; 32], String), AuthError> {
        let value = cookies.get("__Host-rolvya-flow").ok_or(AuthError::Denied)?;
        let fields: Vec<_> = value.split('.').collect();
        if fields.len() != 3 || fields[1].len() != 64 || fields[2].len() != 64 {
            return Err(AuthError::Denied);
        }
        let issued = fields[0].parse::<i64>().map_err(|_| AuthError::Denied)?;
        if issued < 0 || now < issued || now >= issued.saturating_add(600) {
            return Err(AuthError::Denied);
        }
        SessionHandle::parse(fields[1].to_owned()).map_err(|_| AuthError::Denied)?;
        let tag = hex(&self.auth.mac(
            b"browser-preauth-cookie",
            &[
                fields[0].as_bytes(),
                fields[1].as_bytes(),
                self.expected.as_bytes(),
            ],
        ));
        if !constant_eq(tag.as_bytes(), fields[2].as_bytes()) {
            return Err(AuthError::Denied);
        }
        Ok((
            self.auth
                .mac(b"browser-preauth-context", &[fields[1].as_bytes()]),
            hex(&self
                .auth
                .mac(b"browser-preauth-csrf", &[fields[1].as_bytes()])),
        ))
    }
    pub(super) async fn process(
        self: &Arc<Self>,
        channel: Channel,
        r: Request<Body>,
    ) -> Result<Response<Body>, AuthError> {
        let now = self.auth.now()?;
        let path = r.uri().path().to_owned();
        let class = if !path.starts_with("/api/control/v1/") {
            2
        } else if r.method() == axum::http::Method::GET && path != "/api/control/v1/auth/context" {
            1
        } else {
            0
        };
        self.authority.rate(channel.peer, now, class)?;
        if !path.starts_with("/api/control/v1/") {
            if r.method() != axum::http::Method::GET || r.uri().query().is_some() {
                return Err(AuthError::InvalidInput);
            }
            if r.headers().get(header::HOST).and_then(|h| h.to_str().ok())
                != self.expected.strip_prefix("https://")
            {
                return Err(AuthError::Denied);
            }
            return Ok(self.assets.response(&path).unwrap_or_else(|| {
                json(
                    StatusCode::NOT_FOUND,
                    &serde_json::json!({"schema":1,"error":"not_found"}),
                )
            }));
        }
        check_headers(&r, &self.expected)?;
        let method = r.method().clone();
        let c = cookies(&r)?;
        let query = r.uri().query().map(str::to_owned);
        if query.is_some() && path != "/api/control/v1/projects" {
            return Err(AuthError::InvalidInput);
        }
        if method == axum::http::Method::POST
            && (r.headers().get_all(header::CONTENT_TYPE).iter().count() != 1
                || r.headers()
                    .get(header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.split(';').next().unwrap_or_default())
                    != Some("application/json"))
        {
            return Err(AuthError::InvalidInput);
        }
        let submitted_csrf = r
            .headers()
            .get("x-rolvya-csrf")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        if r.headers().get_all("x-rolvya-csrf").iter().count() > 1 {
            return Err(AuthError::Denied);
        }
        let cursor = r
            .headers()
            .get("last-event-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let bytes = to_bytes(
            r.into_body(),
            if method == axum::http::Method::POST {
                4096
            } else {
                0
            },
        )
        .await
        .map_err(|_| AuthError::InvalidInput)?;
        let body = Zeroizing::new(bytes.to_vec());
        drop(bytes);
        if path == "/api/control/v1/auth/context" && method == axum::http::Method::GET {
            let mut tx = self.auth.store.pool.begin().await?;
            let (_, policy) = policy(&mut tx).await?;
            tx.commit().await?;
            // Reopening a delivered email link in its original browser must not
            // replace the ceremony cookie and invalidate the pending challenge.
            if let Ok((_, csrf)) = self.flow(&c, now) {
                return Ok(json(
                    StatusCode::OK,
                    &serde_json::json!({"schema":1,"state":"unauthenticated","csrf":csrf,"password":policy.password_enabled(),"email":policy.email_enabled()&&self.auth.mail_ready(),"recovery":self.auth.password_recovery_status().guidance(),"support":null}),
                ));
            }
            let nonce = Zeroizing::new(hex(&random::<32>()?[..]));
            let issued = now.to_string();
            let tag = hex(&self.auth.mac(
                b"browser-preauth-cookie",
                &[
                    issued.as_bytes(),
                    nonce.as_bytes(),
                    self.expected.as_bytes(),
                ],
            ));
            let csrf = hex(&self.auth.mac(b"browser-preauth-csrf", &[nonce.as_bytes()]));
            let mut response = json(
                StatusCode::OK,
                &serde_json::json!({"schema":1,"state":"unauthenticated","csrf":csrf,"password":policy.password_enabled(),"email":policy.email_enabled()&&self.auth.mail_ready(),"recovery":self.auth.password_recovery_status().guidance(),"support":null}),
            );
            let mut cookie=header::HeaderValue::from_str(&format!("__Host-rolvya-flow={issued}.{}.{tag}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=600",nonce.as_str())).map_err(|_|AuthError::Denied)?;
            cookie.set_sensitive(true);
            response.headers_mut().append(header::SET_COOKIE, cookie);
            return Ok(response);
        }
        let login = matches!(
            path.as_str(),
            "/api/control/v1/auth/password"
                | "/api/control/v1/auth/factor"
                | "/api/control/v1/auth/email"
                | "/api/control/v1/auth/email/confirm"
        );
        let (browser, preauth_csrf) = if login {
            if method != axum::http::Method::POST {
                return Ok(json(
                    StatusCode::METHOD_NOT_ALLOWED,
                    &serde_json::json!({"schema":1,"error":"method_not_allowed"}),
                ));
            }
            let (browser, csrf) = self.flow(&c, now)?;
            if !submitted_csrf
                .as_ref()
                .is_some_and(|v| constant_eq(v.as_bytes(), csrf.as_bytes()))
            {
                return Err(AuthError::Denied);
            }
            (browser, true)
        } else {
            (
                self.auth
                    .mac(b"browser-request-context", &[channel.exporter.as_slice()]),
                false,
            )
        };
        let op = match path.as_str() {
            "/api/control/v1/auth/password" | "/api/control/v1/auth/email/confirm" => {
                Operation::Issue
            }
            "/api/control/v1/auth/factor" => Operation::CompleteFactor,
            "/api/control/v1/auth/renew" => Operation::Renew,
            "/api/control/v1/auth/logout"
            | "/api/control/v1/auth/logout-all"
            | "/api/control/v1/auth/activity"
            | "/api/control/v1/auth/interaction" => Operation::Write,
            _ => Operation::Read,
        };
        if !login && (op == Operation::Read) != (method == axum::http::Method::GET) {
            return Err(AuthError::InvalidInput);
        }
        let h = if login { None } else { Some(session(&c)?) };
        let csrf = submitted_csrf
            .as_ref()
            .map(|v| self.auth.csrf_commitment(v));
        let facts = BrowserFacts {
            secure: true,
            method: if method == axum::http::Method::GET {
                Method::Get
            } else {
                Method::Post
            },
            origin: self.expected.clone(),
            expected_origin: self.expected.clone(),
            same_origin: true,
            browser,
            credential: h.as_ref().map(|h| self.auth.handle_lookup(h)),
            csrf,
            preauth_csrf,
            // Only this dedicated CSRF-protected, one-use ceremony can count as
            // foreground activity. Headers and ordinary reads cannot select it.
            activity: if path == "/api/control/v1/auth/interaction" {
                Activity::Human
            } else {
                Activity::Background
            },
            client_key: self
                .auth
                .mac(b"browser-client", &[channel.peer.to_string().as_bytes()]),
        };
        let commitment = self.auth.mac(
            b"browser-request",
            &[
                &channel.exporter,
                method.as_str().as_bytes(),
                path.as_bytes(),
                query.as_deref().unwrap_or("").as_bytes(),
                &Sha256::digest(body.as_slice()),
            ],
        );
        let guard = self.authority.reserve(Active {
            facts,
            operation: op,
            transport: match path.as_str() {
                "/api/control/v1/auth/password" => Some(TransportOperation::Password),
                "/api/control/v1/auth/email" => Some(TransportOperation::IssueEmail),
                "/api/control/v1/auth/email/confirm" => Some(TransportOperation::ConfirmEmail),
                _ => None,
            },
            peer: channel.peer,
            expires: now + if path.ends_with("/events") { 120 } else { 15 },
            evidence: RequestId::new(new_id()?).map_err(|_| AuthError::Randomness)?,
            commitment,
            cookies: None,
            sent: false,
        })?;
        let context = &guard.context;
        let client = ClientContext {
            request_id: context.request_id.clone(),
            transport_evidence: context.evidence.clone(),
        };
        let device = DeviceLabel::new("Web browser".to_owned())?;
        let mut response = match path.as_str() {
            "/api/control/v1/auth/password" => {
                let input: PasswordInput = decode(&body)?;
                let email = EmailAddress::parse(&input.email).map_err(|_| AuthError::Denied)?;
                let password =
                    Password::new(input.password.into_bytes()).map_err(|_| AuthError::Denied)?;
                let AuthenticationOutcome::Verified(proof) = self
                    .auth
                    .authenticate_password(&email, &password, &client)
                    .await?
                else {
                    return Err(AuthError::Denied);
                };
                self.finish_login(
                    self.auth.start_session(&proof, context, &device).await?,
                    context,
                )
                .await?
            }
            "/api/control/v1/auth/factor" => {
                let input: FactorInput = decode(&body)?;
                let raw = c.get("__Host-rolvya-factor").ok_or(AuthError::Denied)?;
                let (id, secret) = raw.split_once('.').ok_or(AuthError::Denied)?;
                let pending = PendingLogin::parse(id.to_owned(), secret.to_owned())
                    .map_err(|_| AuthError::Denied)?;
                let ticket = match input.kind.as_str() {
                    "totp" => {
                        self.auth
                            .complete_totp_login(
                                &pending,
                                &TotpCode::parse(input.code)?,
                                context,
                                &device,
                            )
                            .await?
                    }
                    "recovery" => {
                        self.auth
                            .complete_recovery_login(
                                &pending,
                                &RecoveryCode::parse(input.code)?,
                                context,
                                &device,
                            )
                            .await?
                    }
                    _ => return Err(AuthError::InvalidInput),
                };
                self.auth.deliver_session(&ticket, context).await?;
                json(
                    StatusCode::OK,
                    &serde_json::json!({"schema":1,"state":"authenticated"}),
                )
            }
            "/api/control/v1/auth/email" => {
                let input: EmailInput = decode(&body)?;
                if !self.auth.mail_ready() {
                    return Err(AuthError::Unavailable);
                }
                let challenge = self
                    .auth
                    .prepare_email(&ChallengeRequest {
                        client,
                        email: EmailAddress::parse(&input.email)?,
                        kind: challenge_kind(&input.kind)?,
                        purpose: ChallengePurpose::Login,
                        return_target: ReturnTarget::PrimaryAuthentication,
                    })
                    .await?;
                let reference = challenge.receipt().reference().to_owned();
                self.auth.dispatch_email(challenge).await?;
                json(
                    StatusCode::OK,
                    &serde_json::json!({"schema":1,"state":"email_pending","challenge":reference}),
                )
            }
            "/api/control/v1/auth/email/confirm" => {
                let input: ConfirmInput = decode(&body)?;
                if !input.confirm {
                    return Err(AuthError::Denied);
                }
                let receipt = ChallengeReceipt {
                    reference: SessionId::parse(input.challenge)?.0,
                };
                match self
                    .auth
                    .redeem_email(
                        &receipt,
                        &ChallengeSecret::parse(input.secret)?,
                        challenge_kind(&input.kind)?,
                        ChallengePurpose::Login,
                        Confirmation::Affirmative,
                        &client,
                    )
                    .await?
                {
                    Redemption::PrimaryVerified(proof) => {
                        self.finish_login(
                            self.auth.start_session(&proof, context, &device).await?,
                            context,
                        )
                        .await?
                    }
                    _ => return Err(AuthError::Denied),
                }
            }
            "/api/control/v1/auth/session" => {
                let h = h.as_ref().ok_or(AuthError::Denied)?;
                self.auth.inspect_session(h, context).await?;
                #[cfg(test)]
                self.auth.checkpoint(Checkpoint::AfterSessionCommit)?;
                let csrf = c.get("__Host-rolvya-csrf").ok_or(AuthError::Denied)?;
                let (mut tx, f, now) = self
                    .auth
                    .browser_transaction(context, Operation::Read)
                    .await?;
                let row = self.auth.session_row(&mut tx, h, &f, now, false).await?;
                let deadline = self.auth.original_deadline(&mut tx, h, &row).await?;
                let tag: Vec<u8> =
                    sqlx::query_scalar("SELECT csrf FROM human_session_tokens WHERE lookup=?")
                        .bind(self.auth.handle_lookup(h).as_slice())
                        .fetch_one(&mut *tx)
                        .await?;
                if !constant_eq(&tag, &self.auth.csrf_commitment(csrf)) {
                    return Err(AuthError::Denied);
                }
                let m = metadata(&row)?;
                let token_expires: i64 = sqlx::query_scalar(
                    "SELECT expires_at FROM human_session_tokens WHERE lookup=?",
                )
                .bind(self.auth.handle_lookup(h).as_slice())
                .fetch_one(&mut *tx)
                .await?;
                self.auth.session_checkpoint(
                    context,
                    Operation::Read,
                    deadline,
                    Checkpoint::BeforeSessionCommit,
                    (&f, now),
                )?;
                tx.commit().await?;
                json(
                    StatusCode::OK,
                    &serde_json::json!({"schema":1,"state":"authenticated","user":m.user.as_str(),"session":m.id.as_str(),"generation":m.generation,"assurance":m.assurance.name(),"csrf":csrf.as_str(),"absoluteExpires":m.absolute_expires,"lastHumanAt":m.last_human_at,"tokenExpiresAt":token_expires,"serverNow":now}),
                )
            }
            "/api/control/v1/auth/activity" => {
                let _: Empty = decode(&body)?;
                let (intent, issued) = self
                    .auth
                    .browser_interaction_intent(h.as_ref().ok_or(AuthError::Denied)?, context)
                    .await?;
                json(
                    StatusCode::OK,
                    &serde_json::json!({"schema":1,"intent":intent.as_str(),"issued":issued}),
                )
            }
            "/api/control/v1/auth/interaction" => {
                let input: InteractionInput = decode(&body)?;
                let last = self
                    .auth
                    .confirm_browser_interaction(
                        h.as_ref().ok_or(AuthError::Denied)?,
                        &input.intent,
                        input.issued,
                        context,
                    )
                    .await?;
                json(
                    StatusCode::OK,
                    &serde_json::json!({"schema":1,"state":"interaction_recorded","lastHumanAt":last}),
                )
            }
            "/api/control/v1/auth/renew" => {
                let _: Empty = decode(&body)?;
                let ticket = self
                    .auth
                    .renew_session(h.as_ref().ok_or(AuthError::Denied)?, context)
                    .await?;
                self.auth.deliver_session(&ticket, context).await?;
                json(
                    StatusCode::OK,
                    &serde_json::json!({"schema":1,"state":"authenticated"}),
                )
            }
            "/api/control/v1/auth/logout" | "/api/control/v1/auth/logout-all" => {
                let _: Empty = decode(&body)?;
                if path.ends_with("logout-all") {
                    self.auth
                        .revoke_all_sessions(h.as_ref().ok_or(AuthError::Denied)?, context)
                        .await?
                } else {
                    self.auth
                        .logout(h.as_ref().ok_or(AuthError::Denied)?, context)
                        .await?
                }
                let mut r = json(
                    StatusCode::OK,
                    &serde_json::json!({"schema":1,"state":"signed_out"}),
                );
                for c in clear_session()? {
                    r.headers_mut().append(header::SET_COOKIE, c);
                }
                r
            }
            "/api/control/v1/projects" => {
                let mut after = None;
                let mut limit = 100;
                if let Some(query) = &query {
                    let mut seen = std::collections::BTreeSet::new();
                    for (key, value) in url::form_urlencoded::parse(query.as_bytes()) {
                        if !seen.insert(key.to_string()) {
                            return Err(AuthError::InvalidInput);
                        }
                        match key.as_ref() {
                            "after" => {
                                after = Some(
                                    rolvya_domain::ids::ProjectId::new(value.as_ref())
                                        .map_err(|_| AuthError::InvalidInput)?,
                                )
                            }
                            "limit" => {
                                limit = value.parse::<u16>().map_err(|_| AuthError::InvalidInput)?
                            }
                            _ => return Err(AuthError::InvalidInput),
                        }
                    }
                }
                let list = self
                    .auth
                    .panel_projects(
                        h.as_ref().ok_or(AuthError::Denied)?,
                        after.as_ref(),
                        limit,
                        context,
                    )
                    .await?;
                json(
                    StatusCode::OK,
                    &serde_json::to_value(list).map_err(|_| AuthError::Storage)?,
                )
            }
            _ => {
                let tail = path
                    .strip_prefix("/api/control/v1/projects/")
                    .ok_or(AuthError::InvalidInput)?;
                let (p, kind) = tail.split_once('/').ok_or(AuthError::InvalidInput)?;
                let p =
                    rolvya_domain::ids::ProjectId::new(p).map_err(|_| AuthError::InvalidInput)?;
                let handle = h.as_ref().ok_or(AuthError::Denied)?;
                self.auth.inspect_session(handle, context).await?;
                if kind == "events" {
                    return self.events(handle, p, cursor.as_deref(), guard).await;
                }
                if kind != "snapshot" {
                    return Err(AuthError::InvalidInput);
                }
                match self.auth.panel_snapshot(handle, &p, context).await {
                    Ok(mut snapshot) => {
                        snapshot.capabilities.events = true;
                        json(
                            StatusCode::OK,
                            &serde_json::to_value(snapshot).map_err(|_| AuthError::Storage)?,
                        )
                    }
                    Err(AuthError::Denied) => json(
                        StatusCode::FORBIDDEN,
                        &serde_json::json!({"schema":1,"error":"project_access_denied"}),
                    ),
                    Err(e) => return Err(e),
                }
            }
        };
        for cookie in self.authority.cookies(context, self.auth.now()?)? {
            response.headers_mut().append(header::SET_COOKIE, cookie);
        }
        Ok(response)
    }
    async fn finish_login(
        &self,
        start: SessionStart,
        context: &BrowserContext,
    ) -> Result<Response<Body>, AuthError> {
        let state = match start {
            SessionStart::Authenticated(ticket) => {
                self.auth.deliver_session(&ticket, context).await?;
                "authenticated"
            }
            SessionStart::FactorRequired(pending) => {
                self.auth.deliver_pending_login(&pending, context).await?;
                "factor_required"
            }
        };
        Ok(json(
            StatusCode::OK,
            &serde_json::json!({"schema":1,"state":state}),
        ))
    }
}
