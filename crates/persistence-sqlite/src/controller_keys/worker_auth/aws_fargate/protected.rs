//! Purpose-specific protected proof storage; no fabricated Broker connection or
//! plaintext export. Reuses reviewed XChaCha20-Poly1305 encrypt/decrypt primitives.
use super::super::{BootstrapSecret, random};
use super::{AwsIdentityError as E, PossessionChallenge};
use crate::controller_keys::{
    ControllerKey,
    secret_broker::crypto::{decrypt, encrypt},
};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;
pub(super) struct Envelope {
    pub dn: [u8; 24],
    pub wn: [u8; 24],
    pub wrapped: Vec<u8>,
    pub ciphertext: Vec<u8>,
}
fn aad(k: &ControllerKey, c: &PossessionChallenge) -> Result<Vec<u8>, E> {
    let mut a = b"rolvya/aws-protected-bootstrap/content/v1\0".to_vec();
    for v in [
        &k.metadata.installation.0[..],
        &k.metadata.key.0[..],
        &k.metadata.key_version.to_be_bytes()[..],
        &c.scope_bytes()?[..],
    ] {
        a.extend_from_slice(&(v.len() as u32).to_be_bytes());
        a.extend_from_slice(v);
    }
    Ok(a)
}
fn wrap(aad: &[u8], dn: &[u8], cipher: &[u8]) -> Vec<u8> {
    let mut a = aad.to_vec();
    a.extend_from_slice(b"\0rolvya/aws-protected-bootstrap/dek-wrap/v1\0");
    a.extend_from_slice(dn);
    a.extend_from_slice(&Sha256::digest(cipher));
    a
}
pub(super) fn seal(
    k: &ControllerKey,
    c: &PossessionChallenge,
    s: &BootstrapSecret,
) -> Result<Envelope, E> {
    let d = random::<32>().map_err(|_| E::KeyUnavailable)?;
    let dn = *random::<24>().map_err(|_| E::KeyUnavailable)?;
    let wn = *random::<24>().map_err(|_| E::KeyUnavailable)?;
    if dn == wn {
        return Err(E::KeyUnavailable);
    }
    let a = aad(k, c)?;
    let ciphertext = encrypt(&d, &dn, &a, s.0.0.as_bytes()).map_err(|_| E::Integrity)?;
    let wrapped =
        encrypt(&k.material, &wn, &wrap(&a, &dn, &ciphertext), &d[..]).map_err(|_| E::Integrity)?;
    Ok(Envelope {
        dn,
        wn,
        wrapped,
        ciphertext,
    })
}
pub(super) fn open(
    k: &ControllerKey,
    c: &PossessionChallenge,
    e: &Envelope,
) -> Result<BootstrapSecret, E> {
    if e.dn == e.wn || e.wrapped.len() != 48 || e.ciphertext.len() != 80 {
        return Err(E::Integrity);
    }
    let a = aad(k, c)?;
    let dek = decrypt(
        &k.material,
        &e.wn,
        &wrap(&a, &e.dn, &e.ciphertext),
        &e.wrapped,
    )
    .map_err(|_| E::Integrity)?;
    let key: &[u8; 32] = dek.as_slice().try_into().map_err(|_| E::Integrity)?;
    let plain: Zeroizing<Vec<u8>> =
        decrypt(key, &e.dn, &a, &e.ciphertext).map_err(|_| E::Integrity)?;
    let text = std::str::from_utf8(&plain).map_err(|_| E::Integrity)?;
    BootstrapSecret::parse(text.to_owned()).map_err(|_| E::Integrity)
}
