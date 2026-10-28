//! Revocable ordinary human sessions; never roles, Owner action or worker authority.
use super::*;
mod factors;
mod lifecycle;
pub mod owner;
mod proof;
#[cfg(test)]
mod tests;
pub use factors::{
    FactorActivation, FactorEnrollment, FactorId, RecoveryBundle, RecoveryCode, TotpCode,
};
pub const HUMAN_IDLE_SECONDS: i64 = 7 * 86400;
const TOKEN_SECONDS: i64 = 86400;
const FAMILY_SECONDS: i64 = 90 * 86400;
const PROOF_SECONDS: i64 = 300;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CookieContract {
    pub name: &'static str,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: &'static str,
    pub path: &'static str,
    pub domain: Option<&'static str>,
}
impl CookieContract {
    pub fn session() -> Self {
        Self {
            name: "__Host-rolvya",
            secure: true,
            http_only: true,
            same_site: "Strict",
            path: "/",
            domain: None,
        }
    }
}
/// Inbound cookie only; no serialization or raw getter. Outbound delivery remains
/// private to a future reviewed HTTPS adapter, which is currently unavailable.
pub struct SessionHandle(Zeroizing<String>);
impl SessionHandle {
    pub fn parse(value: String) -> Result<Self, AuthError> {
        let value = Zeroizing::new(value);
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(AuthError::InvalidInput);
        }
        Ok(Self(value))
    }
}
impl fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionHandle([redacted])")
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionId(String);
impl SessionId {
    pub fn parse(v: String) -> Result<Self, AuthError> {
        if v.len() != 32
            || !v
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(AuthError::InvalidInput);
        }
        Ok(Self(v))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone)]
pub struct BrowserContext {
    pub request_id: RequestId,
    pub evidence: RequestId,
}
#[derive(Debug, Clone)]
pub struct DeviceLabel(String);
impl DeviceLabel {
    pub fn new(v: String) -> Result<Self, AuthError> {
        if v.is_empty()
            || v.len() > 64
            || !v
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b" -_".contains(&b))
        {
            return Err(AuthError::InvalidInput);
        }
        Ok(Self(v))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assurance {
    Primary,
    Totp,
    Recovery,
}
impl Assurance {
    fn name(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Totp => "totp",
            Self::Recovery => "recovery",
        }
    }
}
#[derive(Debug)]
pub struct SessionMetadata {
    pub id: SessionId,
    pub user: HumanId,
    pub generation: i64,
    pub device: String,
    pub assurance: Assurance,
    pub created_at: i64,
    pub last_human_at: i64,
    pub absolute_expires: i64,
    pub revoked: bool,
}
/// Opaque outbound data: even a returned ticket cannot expose a cookie to an
/// unimplemented/untrusted HTTP transport. No public bearer/CSRF getter.
/// ```compile_fail
/// use rolvya_persistence_sqlite::human_auth::sessions::SessionTicket;
/// fn leak(t:&SessionTicket) { let _=serde_json::to_string(t); }
/// ```
pub struct SessionTicket {
    metadata: SessionMetadata,
    handle: SessionHandle,
    csrf: Zeroizing<String>,
    delivery: Option<DeliveryGrant>,
}
impl SessionTicket {
    pub fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }
}
impl fmt::Debug for SessionTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionTicket")
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}
struct DeliveryGrant {
    context: BrowserContext,
    operation: Operation,
    facts: BrowserFacts,
    expires: i64,
}
pub struct PendingLogin {
    id: String,
    secret: Zeroizing<String>,
}
impl PendingLogin {
    pub fn parse(reference: String, bearer: String) -> Result<Self, AuthError> {
        let id = SessionId::parse(reference)?.0;
        let secret = SessionHandle::parse(bearer)?.0;
        Ok(Self { id, secret })
    }
}
impl fmt::Debug for PendingLogin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PendingLogin([redacted])")
    }
}
#[derive(Debug)]
pub enum SessionStart {
    Authenticated(Box<SessionTicket>),
    FactorRequired(PendingLogin),
}
#[derive(Debug, Clone)]
pub struct IncidentEvidence {
    pub request_id: RequestId,
    pub evidence: RequestId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Operation {
    Issue,
    Read,
    Write,
    Renew,
    BeginFactor,
    ConfirmFactor,
    CompleteFactor,
    RotateRecovery,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Activity {
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "trusted HTTP activity classification is not configured"
        )
    )]
    Background,
    Human,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Method {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "trusted HTTP adapter is not configured")
    )]
    Get,
    Post,
}
/// Facts must come from the controller's trusted TLS/HTTP adapter, not caller
/// booleans or X-Forwarded-For. None exists in production yet.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct BrowserFacts {
    secure: bool,
    method: Method,
    origin: String,
    expected_origin: String,
    same_origin: bool,
    browser: [u8; 32],
    credential: Option<[u8; 32]>,
    csrf: Option<[u8; 32]>,
    preauth_csrf: bool,
    activity: Activity,
    client_key: [u8; 32],
}
pub(super) trait BrowserAuthority: Send + Sync {
    fn invitation(
        &self,
        _: &BrowserContext,
        _: &owner::InvitationTicket,
        _: i64,
    ) -> Result<(), AuthError> {
        Err(AuthError::Unavailable)
    }
    fn verify(
        &self,
        c: &BrowserContext,
        operation: Operation,
        now: i64,
    ) -> Result<BrowserFacts, AuthError>;
    fn deliver(
        &self,
        c: &BrowserContext,
        ticket: &SessionTicket,
        now: i64,
    ) -> Result<(), AuthError>;
    fn provision(
        &self,
        c: &BrowserContext,
        provisioning: &FactorEnrollment,
        now: i64,
    ) -> Result<(), AuthError>;
    fn recovery(
        &self,
        c: &BrowserContext,
        bundle: &RecoveryBundle,
        now: i64,
    ) -> Result<(), AuthError>;
    fn pending(
        &self,
        c: &BrowserContext,
        pending: &PendingLogin,
        now: i64,
    ) -> Result<(), AuthError>;
}
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "incident evaluator is not implemented; no caller-controlled risk flag"
    )
)]
pub(super) struct IncidentContext<'a> {
    evidence: &'a IncidentEvidence,
    user: &'a str,
    session: &'a str,
    generation: i64,
    now: i64,
}
pub(super) trait IncidentAuthority: Send + Sync {
    fn verify(&self, c: &IncidentContext<'_>) -> Result<(), AuthError>;
}
impl BrowserAuthority for Unimplemented {
    fn verify(&self, _: &BrowserContext, _: Operation, _: i64) -> Result<BrowserFacts, AuthError> {
        Err(AuthError::Unavailable)
    }
    fn deliver(&self, _: &BrowserContext, _: &SessionTicket, _: i64) -> Result<(), AuthError> {
        Err(AuthError::Unavailable)
    }
    fn provision(&self, _: &BrowserContext, _: &FactorEnrollment, _: i64) -> Result<(), AuthError> {
        Err(AuthError::Unavailable)
    }
    fn recovery(&self, _: &BrowserContext, _: &RecoveryBundle, _: i64) -> Result<(), AuthError> {
        Err(AuthError::Unavailable)
    }
    fn pending(&self, _: &BrowserContext, _: &PendingLogin, _: i64) -> Result<(), AuthError> {
        Err(AuthError::Unavailable)
    }
}
impl IncidentAuthority for Unimplemented {
    fn verify(&self, _: &IncidentContext<'_>) -> Result<(), AuthError> {
        Err(AuthError::Unavailable)
    }
}
impl HumanAuth {
    fn browser(
        &self,
        c: &BrowserContext,
        op: Operation,
        now: i64,
    ) -> Result<BrowserFacts, AuthError> {
        let f = self.browser_authority.verify(c, op, now)?;
        let origin = url::Url::parse(&f.origin).map_err(|_| AuthError::Denied)?;
        let expected = url::Url::parse(&f.expected_origin).map_err(|_| AuthError::Denied)?;
        if !f.secure
            || !f.same_origin
            || origin.scheme() != "https"
            || expected.scheme() != "https"
            || origin.host_str().is_none()
            || origin.origin() != expected.origin()
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
            || expected.path() != "/"
            || !expected.username().is_empty()
            || expected.password().is_some()
            || expected.query().is_some()
            || expected.fragment().is_some()
        {
            return Err(AuthError::Denied);
        }
        if op != Operation::Read && f.method != Method::Post {
            return Err(AuthError::Denied);
        }
        if f.credential.is_none() && !f.preauth_csrf && op != Operation::Read {
            return Err(AuthError::Denied);
        }
        Ok(f)
    }
    async fn browser_transaction<'a>(
        &'a self,
        c: &BrowserContext,
        op: Operation,
    ) -> Result<(Transaction<'a, Sqlite>, BrowserFacts, i64), AuthError> {
        self.browser(c, op, self.now()?)?;
        let tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let now = self.now()?;
        let f = self.browser(c, op, now)?;
        Ok((tx, f, now))
    }
    fn delivery_scope(
        &self,
        c: &BrowserContext,
        operation: Operation,
        facts: &BrowserFacts,
        expires: i64,
    ) -> Result<DeliveryGrant, AuthError> {
        Ok(DeliveryGrant {
            context: c.clone(),
            operation,
            facts: facts.clone(),
            expires: expires.min(self.now()? + PROOF_SECONDS),
        })
    }
    fn scoped_delivery(
        &self,
        mut ticket: SessionTicket,
        c: &BrowserContext,
        operation: Operation,
        facts: &BrowserFacts,
        expires: i64,
    ) -> Result<SessionTicket, AuthError> {
        ticket.delivery = Some(self.delivery_scope(c, operation, facts, expires)?);
        Ok(ticket)
    }
    fn check_delivery(
        &self,
        g: &DeliveryGrant,
        c: &BrowserContext,
        now: i64,
    ) -> Result<BrowserFacts, AuthError> {
        if now >= g.expires
            || g.context.request_id != c.request_id
            || g.context.evidence != c.evidence
        {
            return Err(AuthError::Denied);
        }
        let facts = self.browser(c, g.operation, now)?;
        if facts != g.facts {
            return Err(AuthError::Denied);
        }
        Ok(facts)
    }
    async fn delivery_transaction<'a>(
        &'a self,
        t: &SessionTicket,
        c: &BrowserContext,
    ) -> Result<(Transaction<'a, Sqlite>, SqliteRow, BrowserFacts, i64), AuthError> {
        let grant = t.delivery.as_ref().ok_or(AuthError::Denied)?;
        self.check_delivery(grant, c, self.now()?)?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let now = self.now()?;
        let facts = self.check_delivery(grant, c, now)?;
        let row = self.token_row(&mut tx, &t.handle, now).await?;
        let csrf: Vec<u8> =
            sqlx::query_scalar("SELECT csrf FROM human_session_tokens WHERE lookup=?")
                .bind(self.handle_lookup(&t.handle).as_slice())
                .fetch_one(&mut *tx)
                .await?;
        if !constant_eq(&csrf, &self.csrf_commitment(&t.csrf)) {
            return Err(AuthError::Denied);
        }
        Ok((tx, row, facts, now))
    }
    pub async fn deliver_session(
        &self,
        t: &SessionTicket,
        c: &BrowserContext,
    ) -> Result<(), AuthError> {
        let (tx, _, _, _) = self.delivery_transaction(t, c).await?;
        tx.commit().await?;
        self.check_delivery(
            t.delivery.as_ref().ok_or(AuthError::Denied)?,
            c,
            self.now()?,
        )?;
        self.browser_authority.deliver(c, t, self.now()?)
    }
    pub async fn deliver_pending_login(
        &self,
        pending: &PendingLogin,
        c: &BrowserContext,
    ) -> Result<(), AuthError> {
        let (mut tx, f, now) = self.browser_transaction(c, Operation::Issue).await?;
        let row=sqlx::query("SELECT * FROM human_pending_logins WHERE pending_id=? AND state='pending' AND failures<5").bind(&pending.id).fetch_optional(&mut *tx).await?.ok_or(AuthError::Denied)?;
        let proof = self
            .primary(&mut tx, row.try_get("evidence_id")?, &f.browser, now, false)
            .await?;
        let tag = self.mac(
            b"pending-factor",
            &[
                pending.id.as_bytes(),
                proof.user.as_bytes(),
                proof.evidence.as_bytes(),
                pending.secret.as_bytes(),
            ],
        );
        if !constant_eq(&tag, &row.try_get::<Vec<u8>, _>("secret_commitment")?) {
            return Err(AuthError::Denied);
        }
        self.session_checkpoint(
            c,
            Operation::Issue,
            proof.expires,
            Checkpoint::BeforeSessionCommit,
            (&f, now),
        )?;
        tx.commit().await?;
        self.browser_authority.pending(c, pending, self.now()?)
    }
    fn handle_lookup(&self, h: &SessionHandle) -> [u8; 32] {
        self.mac(b"session-lookup", &[h.0.as_bytes()])
    }
    fn csrf_commitment(&self, csrf: &str) -> [u8; 32] {
        self.mac(b"session-csrf", &[csrf.as_bytes()])
    }
    async fn session_event(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        user: &str,
        session: Option<&str>,
        request: &RequestId,
        event: &str,
        now: i64,
    ) -> Result<(), AuthError> {
        sqlx::query("INSERT INTO human_session_events(user_id,session_id,request_id,event,occurred_at) VALUES(?,?,?,?,?)").bind(user).bind(session).bind(request.as_str()).bind(event).bind(now).execute(&mut **tx).await?;
        Ok(())
    }
}
fn assurance(s: &str) -> Result<Assurance, AuthError> {
    match s {
        "primary" => Ok(Assurance::Primary),
        "totp" => Ok(Assurance::Totp),
        "recovery" => Ok(Assurance::Recovery),
        _ => Err(AuthError::Denied),
    }
}
fn metadata(r: &SqliteRow) -> Result<SessionMetadata, AuthError> {
    Ok(SessionMetadata {
        id: SessionId(r.try_get("session_id")?),
        user: HumanId(r.try_get("user_id")?),
        generation: r.try_get("generation")?,
        device: r.try_get("device")?,
        assurance: assurance(r.try_get("assurance")?)?,
        created_at: r.try_get("created_at")?,
        last_human_at: r.try_get("last_human_at")?,
        absolute_expires: r.try_get("absolute_expires")?,
        revoked: r.try_get::<Option<i64>, _>("revoked_at")?.is_some(),
    })
}
