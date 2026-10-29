//! Primitives for the vault key hierarchy (PLAN.md §5).

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroizing;

use super::VaultError;

/// A 256-bit key that is wiped from memory on drop.
#[derive(Clone)]
pub struct SecretKey(Zeroizing<[u8; 32]>);

impl SecretKey {
    pub fn random() -> Self {
        let mut bytes = Zeroizing::new([0u8; 32]);
        random_fill(bytes.as_mut());
        Self(bytes)
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        let arr: [u8; 32] = bytes.try_into().ok()?;
        Some(Self::from_bytes(arr))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Derives an independent subkey for one purpose (HKDF-SHA256).
    pub(crate) fn derive(&self, info: &str) -> SecretKey {
        let hk = Hkdf::<Sha256>::new(None, self.as_bytes());
        let mut out = Zeroizing::new([0u8; 32]);
        hk.expand(info.as_bytes(), out.as_mut()).expect("32 bytes is a valid HKDF length");
        SecretKey(out)
    }
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey(<redacted>)")
    }
}

pub(crate) fn random_fill(buf: &mut [u8]) {
    getrandom::fill(buf).expect("OS random number generator failed");
}

/// Argon2id parameters stored next to the password slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub m_kib: u32,
    pub t: u32,
    pub p: u32,
    salt: String,
}

impl KdfParams {
    /// Production cost: ~0.5 s on Apple Silicon, deliberately slow for brute force.
    pub fn recommended() -> Self {
        Self::with_cost(128 * 1024, 3, 4)
    }

    /// Cheap parameters for tests only.
    pub fn insecure_fast() -> Self {
        Self::with_cost(64, 1, 1)
    }

    fn with_cost(m_kib: u32, t: u32, p: u32) -> Self {
        let mut salt = [0u8; 16];
        random_fill(&mut salt);
        Self { m_kib, t, p, salt: B64.encode(salt) }
    }

    /// Same cost, fresh salt.
    pub(crate) fn reseeded(&self) -> Self {
        Self::with_cost(self.m_kib, self.t, self.p)
    }

    pub(crate) fn derive(&self, password: &str) -> Result<SecretKey, VaultError> {
        let salt = B64.decode(&self.salt).map_err(|_| VaultError::Corrupt("kdf salt".into()))?;
        let params = Params::new(self.m_kib, self.t, self.p, Some(32))
            .map_err(|e| VaultError::Corrupt(format!("kdf params: {e}")))?;
        let mut out = Zeroizing::new([0u8; 32]);
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(password.as_bytes(), &salt, out.as_mut())
            .map_err(|e| VaultError::Corrupt(format!("kdf: {e}")))?;
        Ok(SecretKey(out))
    }
}

/// An encrypted copy of the data key (DEK), bound to its slot via AAD.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Wrapped {
    nonce: String,
    ciphertext: String,
}

pub(crate) fn wrap(kek: &SecretKey, dek: &SecretKey, aad: &str) -> Wrapped {
    let mut nonce = [0u8; 24];
    random_fill(&mut nonce);
    let cipher = XChaCha20Poly1305::new(&(*kek.as_bytes()).into());
    let ciphertext = cipher
        .encrypt(&XNonce::from(nonce), Payload { msg: dek.as_bytes(), aad: aad.as_bytes() })
        .expect("encryption of 32 bytes cannot fail");
    Wrapped { nonce: B64.encode(nonce), ciphertext: B64.encode(ciphertext) }
}

/// Returns `None` if the key is wrong or the slot was tampered with.
pub(crate) fn unwrap(kek: &SecretKey, wrapped: &Wrapped, aad: &str) -> Option<SecretKey> {
    let nonce: [u8; 24] = B64.decode(&wrapped.nonce).ok()?.try_into().ok()?;
    let ciphertext = B64.decode(&wrapped.ciphertext).ok()?;
    let cipher = XChaCha20Poly1305::new(&(*kek.as_bytes()).into());
    let plain = Zeroizing::new(
        cipher
            .decrypt(&XNonce::from(nonce), Payload { msg: &ciphertext, aad: aad.as_bytes() })
            .ok()?,
    );
    SecretKey::from_slice(&plain)
}
