//! S2-03A primary proofs. No sessions, roles, TOTP or privileged action authority.
//! All production authority adapters are deliberately unavailable. Private seams
//! admit only reviewed future integrations; synthetic implementations live in tests.
use super::{ControllerBootstrap, ControllerKey};
use crate::{
    PlanStore, RequestId,
    admission::{Clock, SystemClock},
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::{Row, Sqlite, Transaction, sqlite::SqliteRow};
use std::{fmt, future::Future, net::IpAddr, pin::Pin, sync::Arc};
use zeroize::Zeroizing;
mod credentials;
mod email;
mod identity;
#[cfg(test)]
mod tests;
use credentials::PasswordEngine;
pub use credentials::PasswordParameters;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    InvalidInput,
    InvalidPolicy,
    Unavailable,
    Denied,
    Conflict,
    StaleVersion,
    Busy,
    Randomness,
    Storage,
    KeyUnavailable,
    RecoveryRequired,
}
impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "human authentication: {self:?}")
    }
}
impl std::error::Error for AuthError {}
impl From<sqlx::Error> for AuthError {
    fn from(e: sqlx::Error) -> Self {
        match crate::StoreError::from(e) {
            crate::StoreError::Busy => Self::Busy,
            _ => Self::Storage,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailAddress(String);
impl EmailAddress {
    /// Conservative ASCII dot-atom mailbox support. Local-part case is preserved;
    /// domain case is normalized. SMTPUTF8, quoted locals and literals unsupported.
    pub fn parse(s: &str) -> Result<Self, AuthError> {
        if !s.is_ascii() || s.len() > 254 {
            return Err(AuthError::InvalidInput);
        }
        let (local, domain) = s.split_once('@').ok_or(AuthError::InvalidInput)?;
        if local.is_empty()
            || local.len() > 64
            || local.starts_with('.')
            || local.ends_with('.')
            || local.contains("..")
            || !local
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&b))
            || !domain.contains('.')
            || domain.split('.').any(|l| {
                l.is_empty()
                    || l.len() > 63
                    || l.starts_with('-')
                    || l.ends_with('-')
                    || !l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
        {
            return Err(AuthError::InvalidInput);
        }
        Ok(Self(format!("{local}@{}", domain.to_ascii_lowercase())))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanId(String);
impl HumanId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
/// Input only. Never serializable, cloneable, or displayed.
/// ```compile_fail
/// use rolvya_persistence_sqlite::human_auth::Password;
/// fn leak(p:&Password) { let _=serde_json::to_string(p); }
/// ```
pub struct Password(Zeroizing<Vec<u8>>);
impl Password {
    pub fn new(bytes: Vec<u8>) -> Result<Self, AuthError> {
        let bytes = Zeroizing::new(bytes);
        if !(12..=1024).contains(&bytes.len()) || bytes.contains(&0) {
            return Err(AuthError::InvalidInput);
        }
        Ok(Self(bytes))
    }
}
impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password([redacted])")
    }
}
pub struct ChallengeSecret(Zeroizing<String>);
impl ChallengeSecret {
    pub fn parse(s: String) -> Result<Self, AuthError> {
        let s = Zeroizing::new(s);
        if !((s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            || (s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit())))
        {
            return Err(AuthError::InvalidInput);
        }
        Ok(Self(s))
    }
}
impl fmt::Debug for ChallengeSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChallengeSecret([redacted])")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginPolicy {
    password: bool,
    verified_email: bool,
}
impl LoginPolicy {
    pub fn new(password: bool, verified_email: bool) -> Result<Self, AuthError> {
        if !password && !verified_email {
            Err(AuthError::InvalidPolicy)
        } else {
            Ok(Self {
                password,
                verified_email,
            })
        }
    }
    pub fn from_methods(methods: &[&str]) -> Result<Self, AuthError> {
        let mut p = false;
        let mut e = false;
        for m in methods {
            match *m {
                "password" => p = true,
                "verified_email" => e = true,
                _ => return Err(AuthError::InvalidPolicy),
            }
        }
        Self::new(p, e)
    }
    pub fn password_enabled(self) -> bool {
        self.password
    }
    pub fn email_enabled(self) -> bool {
        self.verified_email
    }
}
/// Evidence references are data, never an administrative grant.
#[derive(Debug, Clone)]
pub struct Administration {
    pub request_id: RequestId,
    pub evidence: RequestId,
}
#[derive(Debug, Clone)]
pub struct ClientContext {
    pub request_id: RequestId,
    pub transport_evidence: RequestId,
}
#[derive(Debug, Clone)]
pub struct Enrollment {
    pub administration: Administration,
    pub email: EmailAddress,
    pub email_login: bool,
    pub initial_policy: Option<LoginPolicy>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountRestriction {
    Disabled,
    Compromised,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimaryMethod {
    Password,
    MagicLink,
    Otp,
}
impl PrimaryMethod {
    fn name(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::MagicLink => "magic_link",
            Self::Otp => "otp",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthenticationStage {
    PrimaryVerified,
    AdditionalFactorRequired,
}
/// Not a session, membership or secret-use capability. No public constructor or
/// serialization; S2-03B must consume durable evidence and recheck current state.
#[derive(Debug)]
pub struct PrimaryAuthentication {
    user: HumanId,
    evidence: String,
    method: PrimaryMethod,
    credential_version: i64,
    security_epoch: i64,
    policy_version: i64,
    verified_at: i64,
    stage: AuthenticationStage,
}
impl PrimaryAuthentication {
    pub fn user(&self) -> &HumanId {
        &self.user
    }
    pub fn evidence_reference(&self) -> &str {
        &self.evidence
    }
    pub fn method(&self) -> PrimaryMethod {
        self.method
    }
    pub fn credential_version(&self) -> i64 {
        self.credential_version
    }
    pub fn security_epoch(&self) -> i64 {
        self.security_epoch
    }
    pub fn policy_version(&self) -> i64 {
        self.policy_version
    }
    pub fn verified_at(&self) -> i64 {
        self.verified_at
    }
    pub fn stage(&self) -> AuthenticationStage {
        self.stage
    }
}
#[derive(Debug)]
pub enum AuthenticationOutcome {
    Denied,
    Verified(PrimaryAuthentication),
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeKind {
    #[default]
    MagicLink,
    Otp,
}
impl ChallengeKind {
    fn name(self) -> &'static str {
        match self {
            Self::MagicLink => "magic_link",
            Self::Otp => "otp",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengePurpose {
    Login,
    VerifyEmail,
}
impl ChallengePurpose {
    fn name(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::VerifyEmail => "verify_email",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    Preview,
    Affirmative,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnTarget {
    PrimaryAuthentication,
}
impl ReturnTarget {
    pub fn parse(s: &str) -> Result<Self, AuthError> {
        if s == "primary-authentication" {
            Ok(Self::PrimaryAuthentication)
        } else {
            Err(AuthError::InvalidInput)
        }
    }
}
#[derive(Debug, Clone)]
pub struct ChallengeRequest {
    pub client: ClientContext,
    pub email: EmailAddress,
    pub kind: ChallengeKind,
    pub purpose: ChallengePurpose,
    pub return_target: ReturnTarget,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeReceipt {
    reference: String,
}
impl ChallengeReceipt {
    pub fn reference(&self) -> &str {
        &self.reference
    }
}
/// Prepared data is sealed to this instance. No public token/recipient getter.
/// Return its generic receipt before separately scheduling mail dispatch.
pub struct PreparedChallenge {
    receipt: ChallengeReceipt,
    message: Option<MailMessage>,
    seal: [u8; 32],
    client: ClientContext,
}
impl PreparedChallenge {
    pub fn receipt(&self) -> &ChallengeReceipt {
        &self.receipt
    }
}
impl fmt::Debug for PreparedChallenge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PreparedChallenge([redacted])")
    }
}
#[derive(Debug)]
pub enum Redemption {
    Denied,
    ConfirmationRequired,
    EmailVerified,
    PrimaryVerified(PrimaryAuthentication),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailDispatch {
    NotConfirmed,
    TransportAccepted,
    DeliveryVerified,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStatus {
    UnavailableContactAdministrator,
    IndependentVerificationRequired,
}
impl RecoveryStatus {
    pub fn guidance(self) -> &'static str {
        "Self-service password reset is unavailable. Contact the instance administrator."
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdminAction {
    FirstEnrollment,
    AdditionalEnrollment,
    PasswordChange,
    EmailChange,
    PolicyChange,
    EmailLoginChange,
    Restrict,
    Maintenance,
}
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "trusted administrative adapters deliberately unavailable in S2-03A"
    )
)]
struct AdminContext<'a> {
    request: &'a Administration,
    action: AdminAction,
    target: Option<&'a str>,
    payload: [u8; 32],
    now: i64,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum EmailEvidence {
    IndependentlyVerified,
    Pending,
}
trait EnrollmentAuthority: Send + Sync {
    fn check(&self, c: &AdminContext<'_>) -> Result<EmailEvidence, AuthError>;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportOperation {
    Password,
    IssueEmail,
    ConfirmEmail,
}
#[derive(Clone)]
struct VerifiedTransport {
    ip: IpAddr,
    browser: [u8; 32],
}
trait TransportAuthority: Send + Sync {
    fn verify(
        &self,
        c: &ClientContext,
        op: TransportOperation,
        now: i64,
    ) -> Result<VerifiedTransport, AuthError>;
}
struct MailMessage {
    challenge: String,
    recipient: EmailAddress,
    kind: ChallengeKind,
    purpose: ChallengePurpose,
    token: ChallengeSecret,
}
#[derive(Clone, Copy)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "only test transports can confirm mail until real delivery is independently verified"
    )
)]
enum DeliveryObservation {
    Accepted,
    Delivered,
    Failed,
    Uncertain,
}
type MailFuture<'a> = Pin<Box<dyn Future<Output = DeliveryObservation> + Send + 'a>>;
trait MailTransport: Send + Sync {
    fn ready(&self) -> bool;
    fn send<'a>(&'a self, message: &'a MailMessage) -> MailFuture<'a>;
}
/// Separate from a sender's assertion: a future reviewed delivery/readiness verifier.
trait DeliveryAuthority: Send + Sync {
    fn ready(&self) -> bool;
    fn delivered(&self, challenge: &str, now: i64) -> bool;
}
struct Unimplemented;
impl EnrollmentAuthority for Unimplemented {
    fn check(&self, _: &AdminContext<'_>) -> Result<EmailEvidence, AuthError> {
        Err(AuthError::Unavailable)
    }
}
impl TransportAuthority for Unimplemented {
    fn verify(
        &self,
        _: &ClientContext,
        _: TransportOperation,
        _: i64,
    ) -> Result<VerifiedTransport, AuthError> {
        Err(AuthError::Unavailable)
    }
}
impl MailTransport for Unimplemented {
    fn ready(&self) -> bool {
        false
    }
    fn send<'a>(&'a self, _: &'a MailMessage) -> MailFuture<'a> {
        Box::pin(async { DeliveryObservation::Failed })
    }
}
impl DeliveryAuthority for Unimplemented {
    fn ready(&self) -> bool {
        false
    }
    fn delivered(&self, _: &str, _: i64) -> bool {
        false
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Checkpoint {
    EnrollmentClaimed,
    BeforeChallengeCommit,
    AfterChallengeCommit,
    BeforeMailSend,
    AfterMailSend,
    BeforeConsumptionCommit,
    AfterConsumptionCommit,
}
/// No override/allow-all constructor. Production enrollment, transport and mail are
/// blocked until independent trusted integrations have been implemented and reviewed.
pub struct HumanAuth {
    store: PlanStore,
    key: ControllerKey,
    passwords: PasswordEngine,
    authority: Arc<dyn EnrollmentAuthority>,
    transport: Arc<dyn TransportAuthority>,
    mail: Arc<dyn MailTransport>,
    delivery: Arc<dyn DeliveryAuthority>,
    clock: Arc<dyn Clock>,
    #[cfg(test)]
    hook: Box<dyn Fn(Checkpoint) -> Result<(), AuthError> + Send + Sync>,
}
impl HumanAuth {
    pub async fn open(bootstrap: &ControllerBootstrap) -> Result<Self, AuthError> {
        Self::open_with_parameters(bootstrap, PasswordParameters::default()).await
    }
    pub async fn open_with_parameters(
        bootstrap: &ControllerBootstrap,
        parameters: PasswordParameters,
    ) -> Result<Self, AuthError> {
        let key = bootstrap
            .load_existing()
            .await
            .map_err(|_| AuthError::KeyUnavailable)?;
        Ok(Self {
            store: bootstrap.store.clone(),
            key,
            passwords: PasswordEngine::new(parameters)?,
            authority: Arc::new(Unimplemented),
            transport: Arc::new(Unimplemented),
            mail: Arc::new(Unimplemented),
            delivery: Arc::new(Unimplemented),
            clock: Arc::new(SystemClock),
            #[cfg(test)]
            hook: Box::new(|_| Ok(())),
        })
    }
    pub fn password_recovery_status(&self) -> RecoveryStatus {
        if self.mail_ready() {
            RecoveryStatus::IndependentVerificationRequired
        } else {
            RecoveryStatus::UnavailableContactAdministrator
        }
    }
    fn mail_ready(&self) -> bool {
        self.mail.ready() && self.delivery.ready()
    }
    fn now(&self) -> Result<i64, AuthError> {
        self.clock
            .now()
            .map_err(|_| AuthError::Unavailable)
            .and_then(|t| {
                if (0..i64::MAX - 900).contains(&t) {
                    Ok(t)
                } else {
                    Err(AuthError::Unavailable)
                }
            })
    }
    fn checkpoint(&self, p: Checkpoint) -> Result<(), AuthError> {
        #[cfg(test)]
        {
            (self.hook)(p)
        }
        #[cfg(not(test))]
        {
            let _ = p;
            Ok(())
        }
    }
    fn mac(&self, domain: &[u8], fields: &[&[u8]]) -> [u8; 32] {
        // Key material stays in the same restricted boundary; no signing oracle.
        let mut h = <Hmac<Sha256> as Mac>::new_from_slice(&self.key.material[..])
            .expect("HMAC accepts every key length");
        h.update(b"rolvya/human-primary/v1\0");
        for f in [
            &self.key.metadata.installation.0[..],
            &self.key.metadata.key.0[..],
            &self.key.metadata.key_version.to_be_bytes()[..],
            domain,
        ]
        .into_iter()
        .chain(fields.iter().copied())
        {
            h.update(&(f.len() as u64).to_be_bytes());
            h.update(f);
        }
        h.finalize().into_bytes().into()
    }
    async fn record_primary(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        r: PrimaryRecord<'_>,
        request: &RequestId,
    ) -> Result<PrimaryAuthentication, AuthError> {
        let id = new_id()?;
        let binding = self.mac(
            b"primary-evidence",
            &[
                id.as_bytes(),
                r.user.as_bytes(),
                r.method.name().as_bytes(),
                &r.version.to_be_bytes(),
                &r.epoch.to_be_bytes(),
                &r.policy.to_be_bytes(),
                &r.now.to_be_bytes(),
                &r.factor.to_be_bytes(),
                r.source.as_bytes(),
            ],
        );
        sqlx::query("INSERT INTO human_authentications VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind(&id)
            .bind(r.user)
            .bind(r.method.name())
            .bind(r.version)
            .bind(r.epoch)
            .bind(r.policy)
            .bind(r.now)
            .bind(r.factor)
            .bind(r.source)
            .bind(binding.as_slice())
            .execute(&mut **tx)
            .await?;
        audit(tx, Some(r.user), request, "primary_verified", r.now).await?;
        Ok(PrimaryAuthentication {
            user: HumanId(r.user.to_owned()),
            evidence: id,
            method: r.method,
            credential_version: r.version,
            security_epoch: r.epoch,
            policy_version: r.policy,
            verified_at: r.now,
            stage: if r.factor == 0 {
                AuthenticationStage::PrimaryVerified
            } else {
                AuthenticationStage::AdditionalFactorRequired
            },
        })
    }
}
struct PrimaryRecord<'a> {
    user: &'a str,
    method: PrimaryMethod,
    version: i64,
    epoch: i64,
    policy: i64,
    factor: i64,
    source: &'a str,
    now: i64,
}
fn random<const N: usize>() -> Result<Zeroizing<[u8; N]>, AuthError> {
    super::secret_broker::os_random().map_err(|_| AuthError::Randomness)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn new_id() -> Result<String, AuthError> {
    Ok(hex(&random::<16>()?[..]))
}
fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    a.ct_eq(b).into()
}
async fn policy(tx: &mut Transaction<'_, Sqlite>) -> Result<(i64, LoginPolicy), AuthError> {
    let r = sqlx::query("SELECT * FROM human_login_policy WHERE singleton=1")
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(AuthError::Unavailable)?;
    Ok((
        r.try_get("version")?,
        LoginPolicy::new(
            r.try_get::<i64, _>("password")? == 1,
            r.try_get::<i64, _>("verified_email")? == 1,
        )?,
    ))
}
async fn audit(
    tx: &mut Transaction<'_, Sqlite>,
    user: Option<&str>,
    request: &RequestId,
    event: &str,
    now: i64,
) -> Result<(), AuthError> {
    sqlx::query(
        "INSERT INTO human_security_events(user_id,request_id,event,occurred_at) VALUES(?,?,?,?)",
    )
    .bind(user)
    .bind(request.as_str())
    .bind(event)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
