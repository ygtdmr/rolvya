//! Private deterministic external-distribution model. It deliberately authorizes
//! secret reads by IAM ROLE, never by a self-asserted task ARN.
use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
type VerificationDelay =
    Arc<Mutex<Option<(Arc<crate::worker_simulation::DeterministicClock>, i64)>>>;
#[derive(Clone)]
pub(in crate::controller_keys::worker_auth) struct World {
    expected: ExpectedExecution,
    resource: SecretArn,
    policy: Arc<Mutex<Option<serde_json::Value>>>,
    values: Arc<Mutex<std::collections::BTreeMap<String, zeroize::Zeroizing<Vec<u8>>>>>,
    pub exclusive: Arc<AtomicBool>,
    pub revoked: Arc<AtomicBool>,
    pub lost_ack: Arc<AtomicBool>,
    pub publication_calls: Arc<std::sync::atomic::AtomicUsize>,
    pub advance_on_verify: VerificationDelay,
}
impl World {
    pub(in crate::controller_keys::worker_auth) fn new(p: &ProtectedDelivery) -> Arc<Self> {
        Arc::new(Self {
            expected: p.execution.clone(),
            resource: p.secret.clone(),
            policy: Arc::new(Mutex::new(None)),
            values: Arc::new(Mutex::new(std::collections::BTreeMap::new())),
            exclusive: Arc::new(AtomicBool::new(true)),
            revoked: Arc::new(AtomicBool::new(false)),
            lost_ack: Arc::new(AtomicBool::new(false)),
            publication_calls: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            advance_on_verify: Arc::new(Mutex::new(None)),
        })
    }
    pub(in crate::controller_keys::worker_auth) async fn read_as_role(
        &self,
        role: &RoleArn,
        plan: &ProtectedDelivery,
        now: i64,
    ) -> Result<DeliveredPossession, AwsIdentityError> {
        let client = super::super::secrets_sdk::SecretSdk::fixture(
            plan,
            super::super::sdk::tests::credentials(),
            self.http(plan, Some(role.clone())),
        );
        client.fetch(now).await
    }
    fn http(
        &self,
        p: &ProtectedDelivery,
        role: Option<RoleArn>,
    ) -> aws_smithy_runtime_api::client::http::SharedHttpClient {
        use aws_smithy_runtime_api::client::http::{SharedHttpConnector, http_client_fn};
        let c = SharedHttpConnector::new(SecretConnector {
            world: self.clone(),
            plan: p.clone(),
            role,
        });
        http_client_fn(move |_, _| c.clone())
    }
    pub(in crate::controller_keys::worker_auth) fn value_count(&self) -> usize {
        self.values.lock().unwrap().len()
    }
}
impl RestrictedDistribution for World {
    fn current(&self, p: &ProtectedDelivery, _: i64) -> Result<(), AwsIdentityError> {
        if self.revoked.load(Ordering::SeqCst) {
            return Err(AwsIdentityError::Revoked);
        }
        if !self.exclusive.load(Ordering::SeqCst)
            || p.execution != self.expected
            || p.secret != self.resource
        {
            return Err(AwsIdentityError::SecureDeliveryUnverified);
        }
        Ok(())
    }
    fn verify<'a>(
        &'a self,
        p: &'a ProtectedDelivery,
        now: i64,
    ) -> AwsFuture<'a, DeliveryVerification> {
        Box::pin(async move {
            p.validate()?;
            if self.revoked.load(Ordering::SeqCst) {
                return Err(AwsIdentityError::Revoked);
            }
            if !self.exclusive.load(Ordering::SeqCst)
                || p.execution != self.expected
                || p.secret != self.resource
            {
                return Err(AwsIdentityError::SecureDeliveryUnverified);
            }
            if let Some((clock, to)) = self.advance_on_verify.lock().unwrap().take() {
                clock.advance_to(to).unwrap();
            }
            Ok(DeliveryVerification {
                format: 1,
                origin: ProofOrigin::SyntheticAwsProtocolFixture,
                revision: EvidenceRef::new("private-world-revision-1").unwrap(),
                plan_digest: plan_digest(p)?,
                collected_at: now,
                expires_at: now + 15,
            })
        })
    }
    fn publish<'a>(
        &'a self,
        p: &'a ProtectedDelivery,
        material: &'a DeliveredPossession,
    ) -> AwsFuture<'a, ()> {
        Box::pin(async move {
            self.verify(p, material.challenge.issued_at).await?;
            let mut allowed: Vec<String> = self.values.lock().unwrap().keys().cloned().collect();
            allowed.push(p.version_id.clone());
            let sdk = super::super::secrets_sdk::SecretSdk::fixture(
                p,
                super::super::sdk::tests::credentials(),
                self.http(p, None),
            );
            self.publication_calls.fetch_add(1, Ordering::SeqCst);
            sdk.publish(material, &allowed).await?;
            if self.lost_ack.load(Ordering::SeqCst) {
                return Err(AwsIdentityError::Transient);
            }
            Ok(())
        })
    }
}
pub(in crate::controller_keys::worker_auth) fn service(
    identity: Arc<FargateIdentity>,
    world: Arc<World>,
) -> Arc<SecureFargateBootstrap> {
    Arc::new(SecureFargateBootstrap {
        identity,
        distribution: world,
    })
}

#[derive(Clone)]
struct SecretConnector {
    world: World,
    plan: ProtectedDelivery,
    role: Option<RoleArn>,
}
impl std::fmt::Debug for SecretConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrivateSecretsManagerProtocolFixture([redacted])")
    }
}
impl aws_smithy_runtime_api::client::http::HttpConnector for SecretConnector {
    fn call(
        &self,
        request: aws_smithy_runtime_api::client::orchestrator::HttpRequest,
    ) -> aws_smithy_runtime_api::client::http::HttpConnectorFuture {
        let c = self.clone();
        aws_smithy_runtime_api::client::http::HttpConnectorFuture::new(async move {
            use aws_smithy_types::body::SdkBody;
            use serde_json::json;
            let r = request.try_into_http1x().unwrap();
            assert!(
                r.headers()
                    .get("authorization")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("AWS4-HMAC-SHA256 ")
            );
            super::super::secrets_sdk::check_request(&c.plan, &r).unwrap();
            let target = r.headers().get("x-amz-target").unwrap().to_str().unwrap();
            let b: serde_json::Value = serde_json::from_slice(r.body().bytes().unwrap()).unwrap();
            let mut status = 200;
            let arn = c.plan.secret.as_str();
            let value = match target {
                "secretsmanager.DescribeSecret" => {
                    let versions = c
                        .world
                        .values
                        .lock()
                        .unwrap()
                        .keys()
                        .cloned()
                        .map(|v| (v, json!(["AWSPREVIOUS"])))
                        .collect::<serde_json::Map<_, _>>();
                    json!({"ARN":arn,"Name":"rolvya/bootstrap/attempt-a","RotationEnabled":false,"VersionIdsToStages":versions})
                }
                "secretsmanager.ValidateResourcePolicy" => {
                    json!({"PolicyValidationPassed":true,"ValidationErrors":[]})
                }
                "secretsmanager.PutResourcePolicy" => {
                    assert!(c.role.is_none());
                    let policy =
                        serde_json::from_str(b["ResourcePolicy"].as_str().unwrap()).unwrap();
                    c.plan
                        .check_policies(&policy, &c.plan.task_identity_policy().unwrap())
                        .unwrap();
                    *c.world.policy.lock().unwrap() = Some(policy);
                    json!({"ARN":arn,"Name":"rolvya/bootstrap/attempt-a"})
                }
                "secretsmanager.GetResourcePolicy" => {
                    json!({"ARN":arn,"Name":"rolvya/bootstrap/attempt-a","ResourcePolicy":c.world.policy.lock().unwrap().as_ref().unwrap().to_string()})
                }
                "secretsmanager.PutSecretValue" => {
                    assert!(c.role.is_none());
                    assert_eq!(
                        c.world.policy.lock().unwrap().as_ref(),
                        Some(&c.plan.resource_policy().unwrap())
                    );
                    let mut v = c.world.values.lock().unwrap();
                    let value = b["SecretString"].as_str().unwrap().as_bytes();
                    if let Some(old) = v.get(&c.plan.version_id) {
                        assert_eq!(old.as_slice(), value);
                    } else {
                        v.insert(
                            c.plan.version_id.clone(),
                            zeroize::Zeroizing::new(value.to_vec()),
                        );
                    }
                    json!({"ARN":arn,"Name":"rolvya/bootstrap/attempt-a","VersionId":c.plan.version_id})
                }
                "secretsmanager.GetSecretValue" => {
                    // Deliberately ROLE based: two tasks sharing this role can
                    // both read it. Exclusivity is an independent activation gate.
                    if c.role.as_ref() != Some(&c.world.expected.workload.task_role)
                        || c.world.policy.lock().unwrap().as_ref()
                            != Some(&c.plan.resource_policy().unwrap())
                    {
                        status = 400;
                        json!({"__type":"AccessDeniedException"})
                    } else {
                        let values = c.world.values.lock().unwrap();
                        if let Some(v) = values.get(&c.plan.version_id) {
                            json!({"ARN":arn,"Name":"rolvya/bootstrap/attempt-a","VersionId":c.plan.version_id,"SecretString":std::str::from_utf8(v).unwrap()})
                        } else {
                            status = 400;
                            json!({"__type":"ResourceNotFoundException"})
                        }
                    }
                }
                _ => panic!("unexpected private Secrets Manager operation"),
            };
            let body = zeroize::Zeroizing::new(value.to_string().into_bytes());
            let response = http::Response::builder()
                .status(status)
                .header("content-type", "application/x-amz-json-1.1")
                .body(SdkBody::from(axum::body::Bytes::from_owner(
                    crate::controller_keys::worker_auth::https::tls::SecretBytes(body),
                )))
                .unwrap();
            Ok(
                aws_smithy_runtime_api::client::orchestrator::HttpResponse::try_from(response)
                    .unwrap(),
            )
        })
    }
}
impl DeliveredPossession {
    pub(in crate::controller_keys::worker_auth) async fn probe(
        &self,
        client: &https::client::WorkerHttpClient,
        id: RequestId,
        changed: Option<&PossessionChallenge>,
        wrong_channel: bool,
    ) -> Result<(), https::client::ClientError> {
        client
            .fixture_native_probe(
                id,
                changed.unwrap_or(&self.challenge),
                &self.secret,
                wrong_channel,
            )
            .await
    }
    pub(in crate::controller_keys::worker_auth) fn raw_occurs_in(&self, bytes: &[u8]) -> bool {
        bytes
            .windows(self.secret.0.0.len())
            .any(|w| w == self.secret.0.0.as_bytes())
    }
}
impl DeliveredPossession {
    pub(in crate::controller_keys::worker_auth) async fn assert_encryption_boundary(
        &self,
        b: &ControllerBootstrap,
        other: &ControllerBootstrap,
    ) {
        let key = b.load_existing().await.unwrap();
        let wrong = other.load_existing().await.unwrap();
        let original = protected::seal(&key, &self.challenge, &self.secret).unwrap();
        assert!(protected::open(&key, &self.challenge, &original).is_ok());
        assert!(protected::open(&wrong, &self.challenge, &original).is_err());
        let fresh = protected::seal(&key, &self.challenge, &self.secret).unwrap();
        assert_ne!(original.dn, fresh.dn);
        assert_ne!(original.wn, fresh.wn);
        assert_ne!(original.ciphertext, fresh.ciphertext);
        assert_ne!(original.wrapped, fresh.wrapped);
        for field in 0..5 {
            let mut e = protected::seal(&key, &self.challenge, &self.secret).unwrap();
            match field {
                0 => e.ciphertext[0] ^= 1,
                1 => e.wrapped[0] ^= 1,
                2 => e.dn[0] ^= 1,
                3 => e.wn[0] ^= 1,
                _ => {
                    e.ciphertext.pop();
                }
            }
            assert!(protected::open(&key, &self.challenge, &e).is_err());
        }
        let mut changed = self.challenge.clone();
        changed.execution.installation = "f".repeat(32);
        assert!(protected::open(&key, &changed, &original).is_err());
    }
}
/// Check the actual production gate with a controlled clock; wall-clock expiry
/// must not accidentally substitute for missing effective-IAM evidence.
pub(in crate::controller_keys::worker_auth) async fn assert_production_gate_closed(
    service: &SecureFargateBootstrap,
    plan: &ProtectedDelivery,
    now: i64,
) {
    assert!(matches!(
        service.distribution.verify(plan, now).await,
        Err(AwsIdentityError::SecureDeliveryUnverified)
    ));
}
