//! Implicit-TLS SMTP protocol and separately authenticated delivery receipts.
//! SMTP acceptance never establishes delivery. No production qualification is installed.
use super::*;
use crate::controller_keys::operations::{OperationError, canonical, files, hex};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{path::PathBuf, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmtpConfiguration {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub sender: String,
    pub credential_file: PathBuf,
    pub delivery_public_key: PathBuf,
    pub delivery_key_sha256: String,
    pub provider_id: String,
}
impl SmtpConfiguration {
    pub fn validate(&self) -> Result<(), OperationError> {
        if self.port != 465
            || self.host.len() > 253
            || !self.host.contains('.')
            || self
                .host
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && b != b'.' && b != b'-')
            || EmailAddress::parse(&self.sender).is_err()
            || self.username.is_empty()
            || self.username.len() > 254
            || self.username.bytes().any(|b| !(0x21..=0x7e).contains(&b))
            || !self.credential_file.is_absolute()
            || !self.delivery_public_key.is_absolute()
            || self.delivery_key_sha256.len() != 64
            || self.provider_id.is_empty()
            || self.provider_id.len() > 128
            || !self
                .provider_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(OperationError::InvalidConfiguration);
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryStatement {
    pub version: u32,
    pub delivery_id: String,
    pub installation: String,
    pub operation_id: String,
    pub message_commitment: String,
    pub provider_id: String,
    pub delivered_at: i64,
    pub expires_at: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedDelivery {
    pub statement: DeliveryStatement,
    pub signature: String,
}
pub struct OperationalEmail {
    store: PlanStore,
    key: ControllerKey,
    config: SmtpConfiguration,
    origin: String,
    tls: Arc<rustls::ClientConfig>,
    // A reviewed provider-specific delivery/identity qualification must establish this.
    // Ordinary configuration and incoming payloads cannot set it.
    qualification: Option<Qualification>,
}
struct Qualification;
impl OperationalEmail {
    #[cfg(test)]
    pub(in crate::controller_keys::human_auth) fn fixture_qualify_delivery(&mut self) {
        self.qualification = Some(Qualification);
    }
    pub async fn inspect(
        bootstrap: &ControllerBootstrap,
        config: SmtpConfiguration,
        origin: &str,
    ) -> Result<Self, OperationError> {
        config.validate()?;
        sessions::https::validated_operational_origin(origin)
            .map_err(|_| OperationError::InvalidConfiguration)?;
        let roots =
            rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|_| OperationError::Unavailable)?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let key = bootstrap
            .load_existing()
            .await
            .map_err(|_| OperationError::RecoveryRequired)?;
        let public = files::private_file(&config.delivery_public_key, 32)?;
        if public.len() != 32 || hex(&sha2::Sha256::digest(&public)) != config.delivery_key_sha256 {
            return Err(OperationError::Unauthorized);
        }
        Ok(Self {
            store: bootstrap.store.clone(),
            key,
            config,
            origin: origin.to_owned(),
            tls: Arc::new(tls),
            qualification: None,
        })
    }
    /// Host/provider adapter accepts signed evidence, never unsigned webhooks or SMTP status.
    /// This can complete an already accepted dispatch; it cannot issue/enroll a challenge.
    pub async fn record_delivery(&self, proof: &SignedDelivery) -> Result<(), OperationError> {
        let s = &proof.statement;
        let now = crate::controller_keys::operations::now()?;
        if s.version != 1
            || s.installation != self.key.coding_installation()
            || s.provider_id != self.config.provider_id
            || s.delivered_at > now
            || s.expires_at <= now
            || s.expires_at > s.delivered_at.saturating_add(600)
            || s.delivery_id.len() != 64
            || s.operation_id.len() != 32
        {
            return Err(OperationError::Unauthorized);
        }
        let public = files::private_file(&self.config.delivery_public_key, 32)?;
        if hex(&sha2::Sha256::digest(&public)) != self.config.delivery_key_sha256 {
            return Err(OperationError::Unauthorized);
        }
        let signature = base64::engine::general_purpose::STANDARD
            .decode(&proof.signature)
            .map_err(|_| OperationError::Unauthorized)?;
        let mut message = b"rolvya/independent-mail-delivery/v1\0".to_vec();
        message.extend(canonical(s)?);
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, &public[..])
            .verify(&message, &signature)
            .map_err(|_| OperationError::Unauthorized)?;
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let op = sqlx::query("SELECT * FROM controller_mail_operations WHERE operation_id=?")
            .bind(&s.operation_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(OperationError::Unauthorized)?;
        if op.try_get::<String, _>("message_sha256")? != s.message_commitment
            || op.try_get::<String, _>("provider_id")? != s.provider_id
            || op.try_get::<i64, _>("expires_at")? <= now
            || s.delivered_at < op.try_get::<i64, _>("created_at")?
        {
            return Err(OperationError::Unauthorized);
        }
        if let Some(previous) = sqlx::query(
            "SELECT evidence,signature FROM controller_mail_delivery WHERE operation_id=?",
        )
        .bind(&s.operation_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            if previous.try_get::<String, _>("evidence")?
                != String::from_utf8(canonical(s)?).map_err(|_| OperationError::Integrity)?
                || previous.try_get::<Vec<u8>, _>("signature")? != signature
            {
                return Err(OperationError::Conflict);
            }
            tx.commit().await?;
            return Ok(());
        }
        let statement = match op.try_get::<&str, _>("purpose")? {
            "login" | "verify_email" => {
                "UPDATE human_challenges SET state='delivered' WHERE challenge_id=? AND state='accepted' AND expires_at>?"
            }
            "owner_action" => {
                "UPDATE human_owner_challenges SET state='delivered' WHERE challenge_id=? AND state='accepted' AND expires_at>?"
            }
            _ => return Err(OperationError::Integrity),
        };
        if sqlx::query("INSERT INTO controller_mail_delivery VALUES(?,?,?,?,?)")
            .bind(&s.delivery_id)
            .bind(&s.operation_id)
            .bind(String::from_utf8(canonical(s)?).map_err(|_| OperationError::Integrity)?)
            .bind(signature)
            .bind(now)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            != 1
        {
            return Err(OperationError::Storage);
        }
        let updated = sqlx::query(statement)
            .bind(&s.operation_id)
            .bind(now)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if updated != 1 {
            return Err(OperationError::Conflict);
        }
        audit(
            &mut tx,
            None,
            &RequestId::new(&s.delivery_id).map_err(|_| OperationError::Integrity)?,
            "mail_delivered",
            now,
        )
        .await
        .map_err(|_| OperationError::Storage)?;
        tx.commit().await?;
        Ok(())
    }
    async fn smtp(&self, m: &MailMessage) -> Result<(), OperationError> {
        if self.qualification.is_none() {
            return Err(OperationError::Unavailable);
        }
        let secret = files::private_file(&self.config.credential_file, 1024)?;
        if secret.is_empty() || secret.contains(&0) {
            return Err(OperationError::Unauthorized);
        }
        let token = Zeroizing::new(match (m.kind, m.purpose) {
            (_, ChallengePurpose::OwnerAction) => m.token.0.to_string(),
            (ChallengeKind::MagicLink, _) => format!(
                "{}/#email-confirm={}.{}",
                self.origin,
                m.challenge,
                m.token.0.as_str()
            ),
            (ChallengeKind::Otp, _) => m.token.0.to_string(),
        });
        let body = Zeroizing::new(format!(
            "From: {}\r\nTo: {}\r\nSubject: Rolvya verification\r\nMessage-ID: <{}@rolvya.invalid>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nUse this one-use verification in the requesting browser and original action. It expires in ten minutes.\r\nPurpose: {}\r\n{}\r\n",
            self.config.sender,
            m.recipient.as_str(),
            m.challenge,
            m.purpose.name(),
            token.as_str()
        ));
        let commitment = hex(&self
            .key
            .coding_record_seal(b"ops/mail-message", body.as_bytes()));
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        let issued = crate::controller_keys::operations::now()?;
        if sqlx::query("INSERT INTO controller_mail_operations VALUES(?,?,?,?,?,?)")
            .bind(&m.challenge)
            .bind(&commitment)
            .bind(&self.config.provider_id)
            .bind(m.purpose.name())
            .bind(issued)
            .bind(issued.saturating_add(600))
            .execute(&mut *tx)
            .await?
            .rows_affected()
            != 1
        {
            return Err(OperationError::Storage);
        }
        tx.commit().await?;
        // No writer lock spans DNS, TLS or SMTP. One dispatch only; ambiguous DATA is never resent.
        let socket = tokio::net::TcpStream::connect((self.config.host.as_str(), self.config.port))
            .await
            .map_err(|_| OperationError::Unavailable)?;
        let name = rustls::pki_types::ServerName::try_from(self.config.host.clone())
            .map_err(|_| OperationError::InvalidConfiguration)?;
        let tls = tokio_rustls::TlsConnector::from(self.tls.clone())
            .connect(name, socket)
            .await
            .map_err(|_| OperationError::Unauthorized)?;
        let mut stream = BufReader::new(tls);
        reply(&mut stream, 220).await?;
        command(&mut stream, b"EHLO rolvya.invalid\r\n", 250).await?;
        let mut auth = Zeroizing::new(vec![0]);
        auth.extend(self.config.username.as_bytes());
        auth.push(0);
        auth.extend(&*secret);
        let encoded = Zeroizing::new(base64::engine::general_purpose::STANDARD.encode(&*auth));
        let authline = Zeroizing::new(format!("AUTH PLAIN {}\r\n", encoded.as_str()));
        command(&mut stream, authline.as_bytes(), 235).await?;
        command(
            &mut stream,
            format!("MAIL FROM:<{}>\r\n", self.config.sender).as_bytes(),
            250,
        )
        .await?;
        command(
            &mut stream,
            format!("RCPT TO:<{}>\r\n", m.recipient.as_str()).as_bytes(),
            250,
        )
        .await?;
        command(&mut stream, b"DATA\r\n", 354).await?;
        let wire = Zeroizing::new(format!(
            "X-Rolvya-Operation: {}\r\nX-Rolvya-Commitment: {}\r\n{}",
            m.challenge,
            commitment,
            body.as_str()
        ));
        stream
            .get_mut()
            .write_all(wire.as_bytes())
            .await
            .map_err(|_| OperationError::Unavailable)?;
        command(&mut stream, b".\r\n", 250).await?;
        Ok(())
    }
}
async fn command(
    stream: &mut BufReader<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>,
    bytes: &[u8],
    expected: u16,
) -> Result<(), OperationError> {
    stream
        .get_mut()
        .write_all(bytes)
        .await
        .map_err(|_| OperationError::Unavailable)?;
    stream
        .get_mut()
        .flush()
        .await
        .map_err(|_| OperationError::Unavailable)?;
    reply(stream, expected).await
}
async fn reply(
    stream: &mut BufReader<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>,
    expected: u16,
) -> Result<(), OperationError> {
    for _ in 0..32 {
        let mut line = Zeroizing::new(Vec::new());
        // read_until alone is unbounded: cap each SMTP line before accepting more input.
        loop {
            let available = stream
                .fill_buf()
                .await
                .map_err(|_| OperationError::Unavailable)?;
            if available.is_empty() {
                return Err(OperationError::Unavailable);
            }
            let n = available
                .iter()
                .position(|b| *b == b'\n')
                .map(|n| n + 1)
                .unwrap_or(available.len());
            if line.len() + n > 1024 {
                return Err(OperationError::ResourceLimit);
            }
            line.extend_from_slice(&available[..n]);
            stream.consume(n);
            if line.last() == Some(&b'\n') {
                break;
            }
        }
        if line.len() < 5 || &line[..3] != expected.to_string().as_bytes() {
            return Err(OperationError::Unavailable);
        }
        match line[3] {
            b' ' => return Ok(()),
            b'-' => {}
            _ => return Err(OperationError::Integrity),
        }
    }
    Err(OperationError::ResourceLimit)
}
impl MailTransport for OperationalEmail {
    fn ready(&self) -> bool {
        self.qualification.is_some()
    }
    fn send<'a>(&'a self, m: &'a MailMessage) -> MailFuture<'a> {
        Box::pin(async move {
            match tokio::time::timeout(Duration::from_secs(30), self.smtp(m)).await {
                Ok(Ok(())) => DeliveryObservation::Accepted,
                _ => DeliveryObservation::Uncertain,
            }
        })
    }
}
impl DeliveryAuthority for OperationalEmail {
    fn ready(&self) -> bool {
        self.qualification.is_some()
    }
    fn delivered(&self, _: &str, _: i64) -> bool {
        false
    }
    fn verified_in<'a>(
        &'a self,
        c: &'a mut sqlx::SqliteConnection,
        challenge: &'a str,
        now: i64,
    ) -> Pin<Box<dyn Future<Output = Result<bool, AuthError>> + Send + 'a>> {
        Box::pin(async move {
            let Some(row)=sqlx::query("SELECT d.evidence,d.signature,m.message_sha256,m.provider_id,m.created_at,m.expires_at FROM controller_mail_delivery d JOIN controller_mail_operations m ON m.operation_id=d.operation_id WHERE d.operation_id=?").bind(challenge).fetch_optional(c).await? else {return Ok(false)};
            let statement: DeliveryStatement =
                serde_json::from_str(row.try_get("evidence")?).map_err(|_| AuthError::Denied)?;
            if statement.version != 1
                || statement.installation != self.key.coding_installation()
                || statement.operation_id != challenge
                || statement.provider_id != self.config.provider_id
                || statement.provider_id != row.try_get::<String, _>("provider_id")?
                || statement.message_commitment != row.try_get::<String, _>("message_sha256")?
                || statement.delivered_at < row.try_get::<i64, _>("created_at")?
                || statement.delivered_at > now
                || now >= statement.expires_at
                || now >= row.try_get::<i64, _>("expires_at")?
            {
                return Ok(false);
            }
            let public = files::private_file(&self.config.delivery_public_key, 32)
                .map_err(|_| AuthError::Unavailable)?;
            if hex(&sha2::Sha256::digest(&public)) != self.config.delivery_key_sha256 {
                return Ok(false);
            }
            let mut bytes = b"rolvya/independent-mail-delivery/v1\0".to_vec();
            bytes.extend(canonical(&statement).map_err(|_| AuthError::Denied)?);
            Ok(
                ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, &public[..])
                    .verify(&bytes, &row.try_get::<Vec<u8>, _>("signature")?)
                    .is_ok(),
            )
        })
    }
}

#[cfg(test)]
mod tests;
