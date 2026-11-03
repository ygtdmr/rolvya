use super::*;
use ring::signature::KeyPair;
use std::{
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
};
fn private(path: &std::path::Path, bytes: &[u8]) {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    f.write_all(bytes).unwrap();
}
#[tokio::test]
async fn smtp_acceptance_requires_separate_signed_delivery_before_email_redemption() {
    let mut f = super::super::tests::Fixture::new().await;
    f.advance(crate::controller_keys::operations::now().unwrap() - f.now());
    let (_, _, email) = f.owner().await;
    let directory = f.root.path().join("mail");
    std::fs::create_dir(&directory).unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    let rng = ring::rand::SystemRandom::new();
    let encoded = ring::signature::Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let signing = ring::signature::Ed25519KeyPair::from_pkcs8(encoded.as_ref()).unwrap();
    private(
        &directory.join("delivery.pub"),
        signing.public_key().as_ref(),
    );
    private(
        &directory.join("smtp.secret"),
        b"synthetic-smtp-only-credential",
    );
    let config = SmtpConfiguration {
        host: "mail.example.test".to_owned(),
        port: 465,
        username: "synthetic-sender".to_owned(),
        sender: "control@example.test".to_owned(),
        credential_file: directory.join("smtp.secret"),
        delivery_public_key: directory.join("delivery.pub"),
        delivery_key_sha256: hex(&sha2::Sha256::digest(signing.public_key().as_ref())),
        provider_id: "offline-smtp".to_owned(),
    };
    let bootstrap = ControllerBootstrap::open(
        f.store.clone(),
        &f.keys,
        super::super::super::ServiceIdentity::current(),
        &[],
    )
    .await
    .unwrap();
    let mut adapter = OperationalEmail::inspect(&bootstrap, config, "https://control.example.test")
        .await
        .unwrap();
    assert!(!MailTransport::ready(&adapter));
    assert!(!DeliveryAuthority::ready(&adapter));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let certificate = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(certificate.cert.der().clone()).unwrap();
    adapter.tls = Arc::new(
        rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth(),
    );
    adapter.config.host = "localhost".to_owned();
    adapter.config.port = listener.local_addr().unwrap().port();
    adapter.qualification = Some(Qualification);
    let server = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![certificate.cert.der().clone()],
        rustls::pki_types::PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der()).into(),
    )
    .unwrap();
    let smtp = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let tls = tokio_rustls::TlsAcceptor::from(Arc::new(server))
            .accept(socket)
            .await
            .unwrap();
        let mut stream = BufReader::new(tls);
        stream
            .get_mut()
            .write_all(b"220 offline SMTP\r\n")
            .await
            .unwrap();
        for (prefix, response) in [
            ("EHLO ", "250 offline\r\n"),
            ("AUTH PLAIN ", "235 authenticated\r\n"),
            ("MAIL FROM:", "250 ok\r\n"),
            ("RCPT TO:", "250 ok\r\n"),
            ("DATA", "354 send\r\n"),
        ] {
            let mut line = Zeroizing::new(String::new());
            stream.read_line(&mut line).await.unwrap();
            assert!(line.starts_with(prefix));
            stream
                .get_mut()
                .write_all(response.as_bytes())
                .await
                .unwrap();
        }
        let mut operation = None;
        let mut commitment = None;
        let mut total = 0;
        loop {
            let mut line = Zeroizing::new(String::new());
            stream.read_line(&mut line).await.unwrap();
            if let Some(v) = line.strip_prefix("X-Rolvya-Operation: ") {
                operation = Some(v.trim_end().to_owned());
            }
            if let Some(v) = line.strip_prefix("X-Rolvya-Commitment: ") {
                commitment = Some(v.trim_end().to_owned());
            }
            total += line.len();
            assert!(total < 8192);
            if line.as_str() == ".\r\n" {
                break;
            }
        }
        stream
            .get_mut()
            .write_all(b"250 transport accepted\r\n")
            .await
            .unwrap();
        (operation.unwrap(), commitment.unwrap())
    });
    let adapter = Arc::new(adapter);
    f.auth.mail = adapter.clone();
    f.auth.delivery = adapter.clone();
    let client = f.client(1, 1);
    let prepared = f
        .auth
        .prepare_email(&ChallengeRequest {
            client: client.clone(),
            email: email.clone(),
            kind: ChallengeKind::Otp,
            purpose: ChallengePurpose::Login,
            return_target: ReturnTarget::PrimaryAuthentication,
        })
        .await
        .unwrap();
    let id = prepared.receipt.reference.clone();
    let token = ChallengeSecret(Zeroizing::new(
        prepared.message.as_ref().unwrap().token.0.to_string(),
    ));
    assert_eq!(
        f.auth.dispatch_email(prepared).await.unwrap(),
        MailDispatch::TransportAccepted
    );
    let (observed_operation, commitment) = smtp.await.unwrap();
    assert_eq!(observed_operation, id);
    let original_commitment = commitment.clone();
    let receipt = ChallengeReceipt {
        reference: id.clone(),
    };
    assert!(matches!(
        f.auth
            .redeem_email(
                &receipt,
                &token,
                ChallengeKind::Otp,
                ChallengePurpose::Login,
                Confirmation::Affirmative,
                &client
            )
            .await
            .unwrap(),
        Redemption::Denied
    ));
    let t = crate::controller_keys::operations::now().unwrap();
    let statement = DeliveryStatement {
        version: 1,
        delivery_id: "aa".repeat(32),
        installation: adapter.key.coding_installation(),
        operation_id: id,
        message_commitment: commitment,
        provider_id: "offline-smtp".to_owned(),
        delivered_at: t,
        expires_at: t + 300,
    };
    let mut message = b"rolvya/independent-mail-delivery/v1\0".to_vec();
    message.extend(canonical(&statement).unwrap());
    let signature =
        base64::engine::general_purpose::STANDARD.encode(signing.sign(&message).as_ref());
    let mut proof = SignedDelivery {
        statement,
        signature,
    };
    proof.statement.message_commitment = "00".repeat(32);
    assert!(adapter.record_delivery(&proof).await.is_err());
    proof.statement.message_commitment = original_commitment;
    let mut premature = proof.statement.clone();
    premature.delivered_at = t - 300;
    let mut bytes = b"rolvya/independent-mail-delivery/v1\0".to_vec();
    bytes.extend(canonical(&premature).unwrap());
    let premature = SignedDelivery {
        statement: premature,
        signature: base64::engine::general_purpose::STANDARD.encode(signing.sign(&bytes).as_ref()),
    };
    assert!(
        adapter.record_delivery(&premature).await.is_err(),
        "a valid signature cannot assert delivery before dispatch"
    );
    adapter.record_delivery(&proof).await.unwrap();
    adapter.record_delivery(&proof).await.unwrap();
    assert!(matches!(
        f.auth
            .redeem_email(
                &receipt,
                &token,
                ChallengeKind::Otp,
                ChallengePurpose::Login,
                Confirmation::Affirmative,
                &client
            )
            .await
            .unwrap(),
        Redemption::PrimaryVerified(_)
    ));
    assert!(matches!(
        f.auth
            .redeem_email(
                &receipt,
                &token,
                ChallengeKind::Otp,
                ChallengePurpose::Login,
                Confirmation::Affirmative,
                &client
            )
            .await
            .unwrap(),
        Redemption::Denied
    ));
    f.store.close().await;
}
#[test]
fn smtp_configuration_rejects_insecure_transport_and_header_injection() {
    let mut c = SmtpConfiguration {
        host: "mail.example.test".to_owned(),
        port: 25,
        username: "user".to_owned(),
        sender: "control@example.test".to_owned(),
        credential_file: "/private/smtp".into(),
        delivery_public_key: "/private/delivery.pub".into(),
        delivery_key_sha256: "00".repeat(32),
        provider_id: "smtp".to_owned(),
    };
    assert!(c.validate().is_err());
    c.port = 465;
    c.sender = "control@example.test\r\nBcc: other@example.test".to_owned();
    assert!(c.validate().is_err());
}
