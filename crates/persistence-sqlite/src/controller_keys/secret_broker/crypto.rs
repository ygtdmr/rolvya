//! Canonical AEAD envelopes. No public plaintext/key or envelope export capability.
use super::super::{InstallationIdentity, KeyIdentifier, KeyMetadata};
use super::*;
use chacha20poly1305::{KeyInit, Tag, XChaCha20Poly1305, XNonce, aead::AeadInPlace};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
#[derive(Clone)]
pub(super) struct Envelope {
    pub scope: RecordScope,
    pub version: SecretVersion,
    pub envelope_version: u16,
    pub cipher: u8,
    pub origin: KeyMetadata,
    pub wrapping: KeyMetadata,
    pub data_nonce: [u8; 24],
    pub wrap_nonce: [u8; 24],
    pub wrapped: Vec<u8>,
    pub ciphertext: Vec<u8>,
}
fn field(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}
fn connection(out: &mut Vec<u8>, c: &ConnectionScope) {
    for s in [
        c.project.as_str(),
        c.account.as_str(),
        c.connection.as_str(),
        c.provider.as_str(),
        c.mode.name(),
    ] {
        field(out, s.as_bytes());
    }
}
fn key(out: &mut Vec<u8>, k: &KeyMetadata) {
    out.extend_from_slice(&k.installation.0);
    out.extend_from_slice(&k.key.0);
    out.extend_from_slice(&k.format_version.to_be_bytes());
    out.extend_from_slice(&k.key_version.to_be_bytes());
}
impl Envelope {
    // Stable content-origin key metadata remains bound to the data. A future
    // separately authorized wrapping revision can bind its own KEK without
    // rewriting content AAD. This version only supports the current loaded KEK.
    pub(super) fn data_aad(&self) -> Vec<u8> {
        let mut a = Vec::with_capacity(640);
        a.extend_from_slice(b"rolvya/secret/content\0");
        a.extend_from_slice(&self.envelope_version.to_be_bytes());
        a.push(self.cipher);
        key(&mut a, &self.origin);
        connection(&mut a, &self.scope.connection);
        field(&mut a, self.scope.record.as_str().as_bytes());
        field(&mut a, self.scope.purpose.as_str().as_bytes());
        a.extend_from_slice(&self.version.0.to_be_bytes());
        a
    }
    pub(super) fn wrap_aad(&self) -> Vec<u8> {
        let mut a = self.data_aad();
        field(&mut a, b"rolvya/secret/dek-wrap/v1\0");
        key(&mut a, &self.wrapping);
        a.extend_from_slice(&self.data_nonce);
        a.extend_from_slice(&Sha256::digest(&self.ciphertext));
        a
    }
    fn validate(&self, k: &ControllerKey) -> Result<(), BrokerError> {
        if self.envelope_version != 1
            || self.cipher != 1
            || self.origin.format_version != 1
            || self.wrapping.format_version != 1
        {
            return Err(BrokerError::UnsupportedVersion);
        }
        if self.origin != *k.metadata() || self.wrapping != *k.metadata() {
            return Err(BrokerError::Integrity);
        }
        if self.ciphertext.len() < 16
            || self.ciphertext.len() > MAX_SECRET_BYTES + 16
            || self.wrapped.len() != 48
            || self.wrap_nonce == self.data_nonce
        {
            return Err(BrokerError::Integrity);
        }
        Ok(())
    }
}
fn encrypt(
    key: &[u8; 32],
    nonce: &[u8; 24],
    aad: &[u8],
    plain: &[u8],
) -> Result<Vec<u8>, BrokerError> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut b = Zeroizing::new(plain.to_vec());
    let tag = cipher
        .encrypt_in_place_detached(XNonce::from_slice(nonce), aad, b.as_mut_slice())
        .map_err(|_| BrokerError::Integrity)?;
    b.extend_from_slice(&tag);
    Ok(std::mem::take(&mut *b))
}
fn decrypt(
    key: &[u8; 32],
    nonce: &[u8; 24],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Zeroizing<Vec<u8>>, BrokerError> {
    if ciphertext.len() < 16 || ciphertext.len() > MAX_SECRET_BYTES + 16 {
        return Err(BrokerError::Integrity);
    }
    let mut b = Zeroizing::new(ciphertext[..ciphertext.len() - 16].to_vec());
    let tag = Tag::from_slice(&ciphertext[ciphertext.len() - 16..]);
    XChaCha20Poly1305::new(key.into())
        .decrypt_in_place_detached(XNonce::from_slice(nonce), aad, b.as_mut_slice(), tag)
        .map_err(|_| BrokerError::Integrity)?;
    Ok(b)
}
pub(super) fn seal(
    k: &ControllerKey,
    scope: &RecordScope,
    version: SecretVersion,
    value: &SecretValue,
    random: impl Fn(&mut [u8]) -> Result<(), BrokerError>,
) -> Result<Envelope, BrokerError> {
    let mut dek = Zeroizing::new([0u8; 32]);
    let mut dn = [0u8; 24];
    let mut wn = [0u8; 24];
    random(dek.as_mut())?;
    random(&mut dn)?;
    random(&mut wn)?;
    if dn == wn {
        return Err(BrokerError::Randomness);
    }
    let mut e = Envelope {
        scope: scope.clone(),
        version,
        envelope_version: 1,
        cipher: 1,
        origin: k.metadata().clone(),
        wrapping: k.metadata().clone(),
        data_nonce: dn,
        wrap_nonce: wn,
        wrapped: vec![],
        ciphertext: vec![],
    };
    e.ciphertext = encrypt(&dek, &dn, &e.data_aad(), &value.0)?;
    e.wrapped = encrypt(&k.material, &wn, &e.wrap_aad(), dek.as_ref())?;
    Ok(e)
}
pub(super) fn open(k: &ControllerKey, e: &Envelope) -> Result<SecretValue, BrokerError> {
    e.validate(k)?;
    let dek = unwrap(k, e)?;
    Ok(SecretValue(decrypt(
        &dek,
        &e.data_nonce,
        &e.data_aad(),
        &e.ciphertext,
    )?))
}
pub(super) fn unwrap(k: &ControllerKey, e: &Envelope) -> Result<Zeroizing<[u8; 32]>, BrokerError> {
    let raw = decrypt(&k.material, &e.wrap_nonce, &e.wrap_aad(), &e.wrapped)?;
    if raw.len() != 32 {
        return Err(BrokerError::Integrity);
    }
    let mut dek = Zeroizing::new([0u8; 32]);
    dek.copy_from_slice(&raw);
    Ok(dek)
}
pub(super) fn commitment(
    k: &ControllerKey,
    c: &ConnectionScope,
    record: Option<&RecordScope>,
    action: BrokerAction,
    version: SecretVersion,
    administration: &AdminRequest,
    value: Option<&SecretValue>,
) -> [u8; 32] {
    let mut a = Vec::with_capacity(640);
    a.extend_from_slice(b"rolvya/secret/request/v1\0");
    key(&mut a, k.metadata());
    connection(&mut a, c);
    a.push(action as u8);
    a.extend_from_slice(&version.0.to_be_bytes());
    field(&mut a, administration.actor.as_str().as_bytes());
    field(&mut a, administration.request_id.as_str().as_bytes());
    a.push(u8::from(record.is_some()));
    if let Some(r) = record {
        field(&mut a, r.record.as_str().as_bytes());
        field(&mut a, r.purpose.as_str().as_bytes());
    }
    a.push(u8::from(value.is_some()));
    // HMAC keys always have the fixed supported length, so initialization cannot fail.
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(k.material.as_ref())
        .unwrap_or_else(|_| unreachable!("fixed key length"));
    mac.update(&a);
    if let Some(v) = value {
        mac.update(&(v.0.len() as u64).to_be_bytes());
        mac.update(&v.0);
    }
    mac.finalize().into_bytes().into()
}
pub(super) fn metadata(
    installation: Vec<u8>,
    id: Vec<u8>,
    version: i64,
) -> Result<KeyMetadata, BrokerError> {
    Ok(KeyMetadata {
        installation: InstallationIdentity(
            installation
                .try_into()
                .map_err(|_| BrokerError::Integrity)?,
        ),
        key: KeyIdentifier(id.try_into().map_err(|_| BrokerError::Integrity)?),
        format_version: 1,
        key_version: u64::try_from(version).map_err(|_| BrokerError::Integrity)?,
    })
}
pub(super) fn use_commitment(
    k: &ControllerKey,
    prior: &[u8; 32],
    r: &SecretUseRequest,
) -> Result<[u8; 32], BrokerError> {
    let mut a = Vec::with_capacity(640);
    a.extend_from_slice(b"rolvya/secret/use/v1\0");
    a.extend_from_slice(prior);
    a.push(match r.operation {
        CredentialOperation::LocalIntegrityInspection => 0,
        CredentialOperation::ProxyModelRequest => 1,
        CredentialOperation::RefreshConnection => 2,
    });
    a.push(u8::from(r.worker.is_some()));
    if let Some(w) = &r.worker {
        w.binding
            .validate()
            .map_err(|_| BrokerError::InvalidInput)?;
        let b = &w.binding;
        for s in [
            b.task.plan.project_id.as_str(),
            b.task.plan.plan_id.as_str(),
            b.task.task_id.as_str(),
            b.attempt_id.as_str(),
            b.worker_id.as_str(),
            b.plan_hash.as_str(),
            b.context_hash.as_str(),
            w.peer.reference.as_str(),
        ] {
            field(&mut a, s.as_bytes());
        }
        a.extend_from_slice(&b.task.plan.version.get().to_be_bytes());
        a.extend_from_slice(&b.epoch.to_be_bytes());
    }
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(k.material.as_ref())
        .map_err(|_| BrokerError::Integrity)?;
    mac.update(&a);
    Ok(mac.finalize().into_bytes().into())
}
