use super::super::human_auth::{
    Administration, EmailAddress, Enrollment, HumanAuth, HumanId, LoginPolicy, Password,
};
use super::super::{InitializationRequest, LocalSetupAuthorization, ServiceIdentity, SetupState};
use super::*;
use crate::{ProjectId, RequestId};
use base64::Engine;

/// Host-local capability. The constructor validates actual UID and protected paths;
/// it is never installed in the HTTP router and grants no human/project role.
pub struct HostOperator {
    pub(super) config: DeploymentConfig,
    pub(super) store: PlanStore,
    pub(super) bootstrap: ControllerBootstrap,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentSpec {
    pub email: String,
    pub methods: Vec<String>,
    pub email_login: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorRequest {
    pub id: String,
    pub kind: String,
    pub payload: serde_json::Value,
    pub sha256: String,
    pub created_at: i64,
    pub expires_at: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentStatement {
    pub version: u32,
    pub request_id: String,
    pub installation: String,
    pub recipient: String,
    pub request_sha256: String,
    pub identity_evidence_sha256: String,
    pub verified_at: i64,
    pub expires_at: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedEnrollment {
    pub statement: EnrollmentStatement,
    pub signature: String,
}
impl SignedEnrollment {
    pub fn load(path: &Path) -> Result<Self, OperationError> {
        serde_json::from_slice(&files::private_file(path, 8192)?)
            .map_err(|_| OperationError::InvalidConfiguration)
    }
}
pub(crate) struct VerifiedEnrollment {
    pub(crate) request: Administration,
    pub(crate) payload: [u8; 32],
    pub(crate) expires: i64,
    pub(crate) not_before: i64,
    pub(crate) restore_checkpoint: Option<String>,
    pub(crate) evidence_sha256: String,
}
pub(crate) struct VerifiedBinding {
    pub(crate) request: Administration,
    pub(crate) project: ProjectId,
    pub(crate) user: String,
    pub(crate) legacy: String,
    pub(crate) expires: i64,
    pub(crate) not_before: i64,
    pub(crate) restore_checkpoint: Option<String>,
}
impl HostOperator {
    pub async fn open(config: DeploymentConfig) -> Result<Self, OperationError> {
        config.validate_host()?;
        let existed = config
            .database
            .try_exists()
            .map_err(|_| OperationError::UnsafeFilesystem)?;
        if !existed {
            let parent = config
                .database
                .parent()
                .ok_or(OperationError::UnsafeFilesystem)?;
            for directory in [parent, config.custody.as_path()] {
                if std::fs::read_dir(directory)
                    .map_err(|_| OperationError::UnsafeFilesystem)?
                    .next()
                    .is_some()
                {
                    return Err(OperationError::RecoveryRequired);
                }
            }
        }
        let identity = if existed {
            Some(files::database_identity(&config.database)?)
        } else {
            None
        };
        let store = if existed {
            PlanStore::open_existing(&config.database).await?
        } else {
            PlanStore::open(&config.database).await?
        };
        if identity
            .is_some_and(|before| files::database_identity(&config.database).ok() != Some(before))
        {
            store.close().await;
            return Err(OperationError::UnsafeFilesystem);
        }
        Self::from_store(config, store).await
    }
    pub(super) async fn open_existing(config: DeploymentConfig) -> Result<Self, OperationError> {
        config.validate_host()?;
        let identity = files::database_identity(&config.database)?;
        let store = PlanStore::open_existing(&config.database).await?;
        if files::database_identity(&config.database)? != identity {
            store.close().await;
            return Err(OperationError::UnsafeFilesystem);
        }
        Self::from_store(config, store).await
    }
    async fn from_store(
        config: DeploymentConfig,
        store: PlanStore,
    ) -> Result<Self, OperationError> {
        let bootstrap = ControllerBootstrap::open(
            store.clone(),
            &config.custody,
            ServiceIdentity::current(),
            &[
                config.scratch.clone(),
                config.assets.clone(),
                config.backup_destination.clone(),
                config
                    .backup_key
                    .parent()
                    .ok_or(OperationError::InvalidConfiguration)?
                    .to_owned(),
            ],
        )
        .await
        .map_err(|_| OperationError::RecoveryRequired)?;
        Ok(Self {
            config,
            store,
            bootstrap,
        })
    }
    /// Explicit stopped-service upgrade only. Unknown/checksum-mismatched schemas
    /// are rejected by PlanStore; this command never creates or replaces a key.
    pub async fn migrate(config: &DeploymentConfig) -> Result<(), OperationError> {
        config.validate_host()?;
        let identity = files::database_identity(&config.database)?;
        let store = PlanStore::open(&config.database).await?;
        if files::database_identity(&config.database)? != identity {
            store.close().await;
            return Err(OperationError::UnsafeFilesystem);
        }
        store.check_integrity().await?;
        store.close().await;
        Ok(())
    }
    pub async fn state(&self) -> Result<String, OperationError> {
        self.config.validate_host()?;
        Ok(format!(
            "{:?}",
            self.bootstrap
                .state()
                .await
                .map_err(|_| OperationError::RecoveryRequired)?
        ))
    }
    pub async fn close(&self) {
        self.store.close().await
    }
    pub async fn prepare_setup(&self) -> Result<OperatorRequest, OperationError> {
        self.config.validate_host()?;
        if self
            .bootstrap
            .state()
            .await
            .map_err(|_| OperationError::RecoveryRequired)?
            != SetupState::Fresh
        {
            return Err(OperationError::Conflict);
        }
        let birth: Vec<u8> = sqlx::query_scalar(
            "SELECT birth_identity FROM controller_installation WHERE singleton=1",
        )
        .fetch_one(&self.store.pool)
        .await?;
        self.prepare("setup", serde_json::json!({"birth":hex(&birth)}), None)
            .await
    }
    pub async fn initialize(&self, id: &str, approved_sha256: &str) -> Result<(), OperationError> {
        self.config.validate_host()?;
        let r = self.request(id, None).await?;
        if r.kind != "setup" || r.sha256 != approved_sha256 {
            return Err(OperationError::Unauthorized);
        }
        self.event(id, "setup_started", &hex_decode(&r.sha256)?, None)
            .await?;
        struct Setup {
            request: RequestId,
            expires: i64,
            uid: u32,
        }
        impl LocalSetupAuthorization for Setup {
            fn permits(&self, r: &InitializationRequest) -> bool {
                r.request_id() == &self.request
                    && rustix::process::geteuid().as_raw() == self.uid
                    && now().is_ok_and(|t| t < self.expires)
            }
        }
        let request = RequestId::new(id).map_err(|_| OperationError::Integrity)?;
        let key = self
            .bootstrap
            .initialize(
                &InitializationRequest::new(request.clone()),
                &Setup {
                    request,
                    expires: r.expires_at,
                    uid: self.config.service_uid,
                },
            )
            .await
            .map_err(|_| OperationError::RecoveryRequired)?;
        self.event(
            id,
            "setup_verified",
            &key.coding_record_seal(b"ops/setup", &canonical(&r)?),
            None,
        )
        .await?;
        Ok(())
    }
    pub async fn prepare_enrollment(
        &self,
        spec: &EnrollmentSpec,
        password: Option<&Password>,
    ) -> Result<OperatorRequest, OperationError> {
        self.config.validate_host()?;
        let auth = HumanAuth::open(&self.bootstrap)
            .await
            .map_err(|_| OperationError::Unavailable)?;
        let id = random_id()?;
        let r = enrollment(spec, &id)?;
        let payload = auth.host_enrollment_commitment(&r, password);
        let key = self
            .bootstrap
            .load_existing()
            .await
            .map_err(|_| OperationError::RecoveryRequired)?;
        self.prepare_id(&id,"enrollment",serde_json::json!({"spec":spec,"enrollment_commitment":hex(&payload),"installation":key.coding_installation(),"verifier":self.config.identity_verifier_sha256}),Some(&key)).await
    }
    pub async fn enroll(
        &self,
        proof: &SignedEnrollment,
        password: Option<&Password>,
    ) -> Result<HumanId, OperationError> {
        self.config.validate_host()?;
        let key = self
            .bootstrap
            .load_existing()
            .await
            .map_err(|_| OperationError::RecoveryRequired)?;
        let r = self
            .request(&proof.statement.request_id, Some(&key))
            .await?;
        if r.kind != "enrollment" {
            return Err(OperationError::Unauthorized);
        }
        let spec: EnrollmentSpec = serde_json::from_value(r.payload["spec"].clone())
            .map_err(|_| OperationError::Integrity)?;
        let mut auth = HumanAuth::open(&self.bootstrap)
            .await
            .map_err(|_| OperationError::Unavailable)?;
        let enrollment = enrollment(&spec, &r.id)?;
        let payload = auth.host_enrollment_commitment(&enrollment, password);
        if r.payload["enrollment_commitment"].as_str() != Some(&hex(&payload))
            || r.payload["verifier"].as_str() != Some(&self.config.identity_verifier_sha256)
        {
            return Err(OperationError::Unauthorized);
        }
        let statement = &proof.statement;
        let t = now()?;
        if statement.version != 1
            || statement.installation != key.coding_installation()
            || statement.recipient != spec.email
            || statement.request_sha256 != r.sha256
            || !config::digest(&statement.identity_evidence_sha256)
            || statement.verified_at < r.created_at
            || statement.verified_at > t
            || statement.expires_at <= t
            || statement.expires_at > r.expires_at
            || statement.expires_at > statement.verified_at.saturating_add(600)
        {
            return Err(OperationError::Unauthorized);
        }
        let public = files::private_file(&self.config.identity_verifier_key, 32)?;
        if public.len() != 32
            || hex(&Sha256::digest(&public)) != self.config.identity_verifier_sha256
        {
            return Err(OperationError::Unauthorized);
        }
        let signature = base64::engine::general_purpose::STANDARD
            .decode(&proof.signature)
            .map_err(|_| OperationError::Unauthorized)?;
        let mut message = b"rolvya/independent-first-human/v1\0".to_vec();
        message.extend(canonical(statement)?);
        ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, &public[..])
            .verify(&message, &signature)
            .map_err(|_| OperationError::Unauthorized)?;
        // Retain independently signed nonsecret provenance before the existing enrollment.
        // The sealed authority rechecks this exact receipt in each enrollment transaction.
        let proof_bytes = canonical(proof)?;
        let proof_text =
            std::str::from_utf8(&proof_bytes).map_err(|_| OperationError::Integrity)?;
        let mut digest = Sha256::new();
        digest.update(b"rolvya/host-enrollment-evidence/v1\0");
        digest.update(&*public);
        digest.update(&proof_bytes);
        let evidence_sha256 = hex(&digest.finalize());
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(previous)=sqlx::query("SELECT proof,verifier,evidence_sha256 FROM controller_operator_evidence WHERE request_id=?").bind(&r.id).fetch_optional(&mut *tx).await? {
            if previous.try_get::<String,_>("proof")?!=proof_text || previous.try_get::<Vec<u8>,_>("verifier")?!=*public || previous.try_get::<String,_>("evidence_sha256")?!=evidence_sha256 {return Err(OperationError::Conflict)}
        } else {
            one(sqlx::query("INSERT INTO controller_operator_evidence VALUES(?,?,?,?,?)").bind(&r.id).bind(proof_text).bind(&*public).bind(&evidence_sha256).bind(t).execute(&mut *tx).await?.rows_affected())?;
        }
        tx.commit().await?;
        auth.host_enrollment_authority(VerifiedEnrollment {
            request: enrollment.administration.clone(),
            payload,
            evidence_sha256,
            expires: statement.expires_at,
            not_before: statement.verified_at,
            restore_checkpoint: r.payload["restore_checkpoint"].as_str().map(str::to_owned),
        });
        let human = auth
            .enroll(&enrollment, password)
            .await
            .map_err(|_| OperationError::Unauthorized)?;
        self.event(
            &r.id,
            "enrollment_verified",
            &key.coding_record_seal(b"ops/enrollment-result", human.as_str().as_bytes()),
            None,
        )
        .await?;
        Ok(human)
    }
    pub async fn prepare_binding(
        &self,
        project: &ProjectId,
        human: &HumanId,
        legacy: &str,
    ) -> Result<OperatorRequest, OperationError> {
        self.config.validate_host()?;
        let actual: String =
            sqlx::query_scalar("SELECT owner_actor FROM projects WHERE project_id=?")
                .bind(project.as_str())
                .fetch_one(&self.store.pool)
                .await?;
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM human_users WHERE user_id=? AND state='active')",
        )
        .bind(human.as_str())
        .fetch_one(&self.store.pool)
        .await?;
        if actual != legacy || !active {
            return Err(OperationError::Unauthorized);
        }
        let key = self
            .bootstrap
            .load_existing()
            .await
            .map_err(|_| OperationError::RecoveryRequired)?;
        let r=self.prepare("binding",serde_json::json!({"installation":key.coding_installation(),"project":project.as_str(),"human":human.as_str(),"legacy":legacy}),Some(&key)).await?;
        self.event(&r.id, "binding_prepared", &hex_decode(&r.sha256)?, None)
            .await?;
        Ok(r)
    }
    async fn prepare(
        &self,
        kind: &str,
        payload: serde_json::Value,
        key: Option<&ControllerKey>,
    ) -> Result<OperatorRequest, OperationError> {
        self.prepare_id(&random_id()?, kind, payload, key).await
    }
    pub(super) async fn prepare_id(
        &self,
        id: &str,
        kind: &str,
        mut payload: serde_json::Value,
        key: Option<&ControllerKey>,
    ) -> Result<OperatorRequest, OperationError> {
        let checkpoint: Option<String> = sqlx::query_scalar(
            "SELECT restore_id FROM controller_restore_holds ORDER BY rowid DESC LIMIT 1",
        )
        .fetch_optional(&self.store.pool)
        .await?;
        payload
            .as_object_mut()
            .ok_or(OperationError::Integrity)?
            .insert(
                "restore_checkpoint".to_owned(),
                serde_json::to_value(checkpoint).map_err(|_| OperationError::Integrity)?,
            );
        let t = now()?;
        let mut r = OperatorRequest {
            id: id.to_owned(),
            kind: kind.to_owned(),
            payload,
            sha256: String::new(),
            created_at: t,
            expires_at: t.checked_add(600).ok_or(OperationError::Integrity)?,
        };
        let body = canonical(&r)?;
        let tag = key
            .map(|k| k.coding_record_seal(b"ops/operator-request", &body))
            .unwrap_or_else(|| Sha256::digest(&body).into());
        r.sha256 = hex(&tag);
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        one(
            sqlx::query("INSERT INTO controller_operator_requests VALUES(?,?,?,?,?,?,?)")
                .bind(&r.id)
                .bind(&r.kind)
                .bind(
                    canonical(&r.payload).and_then(|b| {
                        String::from_utf8(b).map_err(|_| OperationError::Integrity)
                    })?,
                )
                .bind(tag.as_slice())
                .bind(self.config.service_uid as i64)
                .bind(t)
                .bind(r.expires_at)
                .execute(&mut *tx)
                .await?
                .rows_affected(),
        )?;
        tx.commit().await?;
        Ok(r)
    }
    pub(super) async fn request(
        &self,
        id: &str,
        key: Option<&ControllerKey>,
    ) -> Result<OperatorRequest, OperationError> {
        load_request(&self.store, id, key, self.config.service_uid).await
    }
    pub(super) async fn event(
        &self,
        id: &str,
        kind: &str,
        commitment: &[u8],
        transaction: Option<&mut sqlx::SqliteConnection>,
    ) -> Result<(), OperationError> {
        if let Some(c) = transaction {
            return event(c, id, kind, commitment).await;
        }
        let mut tx = self.store.pool.begin_with("BEGIN IMMEDIATE").await?;
        event(&mut tx, id, kind, commitment).await?;
        tx.commit().await?;
        Ok(())
    }
}
pub(super) async fn event(
    c: &mut sqlx::SqliteConnection,
    id: &str,
    kind: &str,
    commitment: &[u8],
) -> Result<(), OperationError> {
    one(sqlx::query("INSERT INTO controller_operation_events(request_id,kind,commitment,recorded_at) VALUES(?,?,?,?)").bind(id).bind(kind).bind(commitment).bind(now()?).execute(c).await?.rows_affected())
}
async fn load_request(
    store: &PlanStore,
    id: &str,
    key: Option<&ControllerKey>,
    uid: u32,
) -> Result<OperatorRequest, OperationError> {
    let row = sqlx::query("SELECT * FROM controller_operator_requests WHERE request_id=?")
        .bind(id)
        .fetch_optional(&store.pool)
        .await?
        .ok_or(OperationError::Unauthorized)?;
    let mut r = OperatorRequest {
        id: id.to_owned(),
        kind: row.try_get("kind")?,
        payload: serde_json::from_str(row.try_get("payload")?)
            .map_err(|_| OperationError::Integrity)?,
        sha256: String::new(),
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
    };
    let body = canonical(&r)?;
    let tag = key
        .map(|k| k.coding_record_seal(b"ops/operator-request", &body))
        .unwrap_or_else(|| Sha256::digest(&body).into());
    if row.try_get::<Vec<u8>, _>("commitment")? != tag
        || row.try_get::<i64, _>("service_uid")? != i64::from(uid)
        || now()? < r.created_at
        || now()? >= r.expires_at
    {
        return Err(OperationError::Unauthorized);
    }
    r.sha256 = hex(&tag);
    let checkpoint: Option<String> = sqlx::query_scalar(
        "SELECT restore_id FROM controller_restore_holds ORDER BY rowid DESC LIMIT 1",
    )
    .fetch_optional(&store.pool)
    .await?;
    if r.payload.get("restore_checkpoint")
        != Some(&serde_json::to_value(checkpoint).map_err(|_| OperationError::Integrity)?)
    {
        return Err(OperationError::Unauthorized);
    }
    Ok(r)
}
pub(crate) async fn binding(
    store: &PlanStore,
    key: &ControllerKey,
    id: &str,
) -> Result<VerifiedBinding, OperationError> {
    let r = load_request(store, id, Some(key), rustix::process::geteuid().as_raw()).await?;
    let field = |s: &str| r.payload[s].as_str().ok_or(OperationError::Integrity);
    if r.kind != "binding" || field("installation")? != key.coding_installation() {
        return Err(OperationError::Unauthorized);
    }
    Ok(VerifiedBinding {
        request: Administration {
            request_id: RequestId::new(id).map_err(|_| OperationError::Integrity)?,
            evidence: RequestId::new(&r.sha256).map_err(|_| OperationError::Integrity)?,
        },
        project: ProjectId::new(field("project")?).map_err(|_| OperationError::Integrity)?,
        user: field("human")?.to_owned(),
        legacy: field("legacy")?.to_owned(),
        expires: r.expires_at,
        not_before: r.created_at,
        restore_checkpoint: r.payload["restore_checkpoint"].as_str().map(str::to_owned),
    })
}
fn enrollment(s: &EnrollmentSpec, id: &str) -> Result<Enrollment, OperationError> {
    Ok(Enrollment {
        administration: Administration {
            request_id: RequestId::new(id).map_err(|_| OperationError::Integrity)?,
            evidence: RequestId::new(id).map_err(|_| OperationError::Integrity)?,
        },
        email: EmailAddress::parse(&s.email).map_err(|_| OperationError::InvalidConfiguration)?,
        email_login: s.email_login,
        initial_policy: Some(
            LoginPolicy::from_methods(&s.methods.iter().map(String::as_str).collect::<Vec<_>>())
                .map_err(|_| OperationError::InvalidConfiguration)?,
        ),
    })
}
pub(super) fn hex_decode(s: &str) -> Result<[u8; 32], OperationError> {
    if !config::digest(s) {
        return Err(OperationError::Integrity);
    }
    let mut b = [0; 32];
    for (i, pair) in s.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        b[i] = u8::from_str_radix(
            std::str::from_utf8(pair).map_err(|_| OperationError::Integrity)?,
            16,
        )
        .map_err(|_| OperationError::Integrity)?
    }
    Ok(b)
}
