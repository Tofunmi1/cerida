// ── AEAD encryption, key derivation, secure memory ──
// All behind `cfg(feature = "secure")`.

use anyhow::Result;

/// Number of bytes in an AES-256 key.
pub const KEY_SIZE: usize = 32;
/// Number of bytes in a GCM nonce (96 bits).
pub const NONCE_SIZE: usize = 12;
/// Number of bytes in the GCM tag (appended to ciphertext).
pub const TAG_SIZE: usize = 16;

/// A256-GCM encrypted payload: nonce || ciphertext || tag.
#[derive(Clone)]
pub struct EncryptedPayload {
    pub nonce: [u8; NONCE_SIZE],
    pub ciphertext: Vec<u8>,
}

/// Derive an AES-256 session key from TLS exported keying material (EKM)
/// using HKDF-SHA256 with a protocol-specific salt.
#[cfg(feature = "secure")]
pub fn derive_session_key(tls_ekm: &[u8]) -> Result<[u8; KEY_SIZE]> {
    use ring::hkdf::{Salt, HKDF_SHA256};
    let salt = Salt::new(HKDF_SHA256, b"cer-perp-session-key-v1");
    let prk = salt.extract(tls_ekm);
    let mut okm = [0u8; KEY_SIZE];
    prk.expand(&[b"cer-perp-v1-session"], HKDF_SHA256)
        .map_err(|e| anyhow::anyhow!("HKDF expand failed: {e}"))?
        .fill(&mut okm)
        .map_err(|e| anyhow::anyhow!("HKDF fill failed: {e}"))?;
    Ok(okm)
}

/// Encrypt `plaintext` with AES-256-GCM using the given key.
/// Generates a random nonce internally.
#[cfg(feature = "secure")]
pub fn encrypt(key: &[u8; KEY_SIZE], plaintext: &[u8]) -> Result<EncryptedPayload> {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("AES-256-GCM init failed: {e}"))?;
    let mut nonce = [0u8; NONCE_SIZE];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext)
        .map_err(|e| anyhow::anyhow!("AES-256-GCM encrypt failed: {e}"))?;
    Ok(EncryptedPayload { nonce, ciphertext })
}

/// Decrypt an `EncryptedPayload` with AES-256-GCM.
#[cfg(feature = "secure")]
pub fn decrypt(key: &[u8; KEY_SIZE], payload: &EncryptedPayload) -> Result<Vec<u8>> {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("AES-256-GCM init failed: {e}"))?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&payload.nonce),
            payload.ciphertext.as_ref(),
        )
        .map_err(|e| anyhow::anyhow!("AES-256-GCM decrypt failed: {e}"))?;
    Ok(plaintext)
}

/// Zero out sensitive memory.
#[cfg(feature = "secure")]
pub fn zeroize(buf: &mut [u8]) {
    use zeroize::Zeroize;
    buf.zeroize();
}

#[cfg(not(feature = "secure"))]
mod fallback {
    use super::*;
    use anyhow::bail;

    pub fn derive_session_key(_tls_ekm: &[u8]) -> Result<[u8; KEY_SIZE]> {
        bail!("secure feature not enabled");
    }

    pub fn encrypt(_key: &[u8; KEY_SIZE], _plaintext: &[u8]) -> Result<EncryptedPayload> {
        bail!("secure feature not enabled");
    }

    pub fn decrypt(_key: &[u8; KEY_SIZE], _payload: &EncryptedPayload) -> Result<Vec<u8>> {
        bail!("secure feature not enabled");
    }

    pub fn zeroize(buf: &mut [u8]) {
        // In non-secure mode, just overwrite with zeros as best effort.
        buf.fill(0);
    }
}
