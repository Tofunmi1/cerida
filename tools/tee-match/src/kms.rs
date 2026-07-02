// ── Cloud KMS key management ──
// All behind `cfg(feature = "secure")`.

use anyhow::{bail, Result};

/// Wrapped DEK stored alongside the encrypted database.
#[derive(Clone)]
pub struct WrappedKey {
    /// Cloud KMS resource name (e.g. `projects/p/locations/global/keyRings/k/cryptoKeys/k`)
    pub kms_key_name: String,
    /// Ciphertext returned by Cloud KMS encrypt
    pub ciphertext: Vec<u8>,
}

/// Unwrap a DEK from Cloud KMS using the current workload's attestation-gated
/// service account. The caller must have `cloudkms.cryptoKeyDecrypter` on `kms_key_name`.
#[cfg(feature = "secure")]
pub fn unwrap_dek(wrapped: &WrappedKey) -> Result<[u8; 32]> {
    use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

    // Obtain an OAuth2 token from the GCP metadata server (runs inside Confidential Space).
    let token = gcp_metadata_token()?;

    let url = format!(
        "https://cloudkms.googleapis.com/v1/{}:decrypt",
        wrapped.kms_key_name
    );

    let body = serde_json::json!({
        "ciphertext": base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &wrapped.ciphertext,
        ),
    });

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()?;

    if !resp.status().is_success() {
        bail!(
            "KMS decrypt failed: {} {}",
            resp.status(),
            resp.text().unwrap_or_default()
        );
    }

    #[derive(serde::Deserialize)]
    struct DecryptResponse {
        plaintext: String,
    }

    let dr: DecryptResponse = resp.json()?;
    let plaintext_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &dr.plaintext,
    )?;

    if plaintext_bytes.len() != 32 {
        bail!("KMS returned key of unexpected length {}", plaintext_bytes.len());
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&plaintext_bytes);
    Ok(key)
}

/// Wrap a DEK with Cloud KMS.
#[cfg(feature = "secure")]
pub fn wrap_dek(kms_key_name: &str, dek: &[u8; 32]) -> Result<WrappedKey> {
    use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

    let token = gcp_metadata_token()?;

    let url = format!(
        "https://cloudkms.googleapis.com/v1/{}:encrypt",
        kms_key_name
    );

    let body = serde_json::json!({
        "plaintext": base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            dek,
        ),
    });

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(&url)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .json(&body)
        .send()?;

    if !resp.status().is_success() {
        bail!(
            "KMS encrypt failed: {} {}",
            resp.status(),
            resp.text().unwrap_or_default()
        );
    }

    #[derive(serde::Deserialize)]
    struct EncryptResponse {
        ciphertext: String,
    }

    let er: EncryptResponse = resp.json()?;
    let ciphertext = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &er.ciphertext,
    )?;

    Ok(WrappedKey {
        kms_key_name: kms_key_name.to_string(),
        ciphertext,
    })
}

/// Fetch a GCP access token from the metadata server inside Confidential Space.
#[cfg(feature = "secure")]
fn gcp_metadata_token() -> Result<String> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get("http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token")
        .header("Metadata-Flavor", "Google")
        .send()?;

    if !resp.status().is_success() {
        bail!("metadata token request failed: {}", resp.status());
    }

    #[derive(serde::Deserialize)]
    struct TokenResponse {
        access_token: String,
    }

    let tr: TokenResponse = resp.json()?;
    Ok(tr.access_token)
}

/// Generate a fresh random DEK (data encryption key).
pub fn generate_dek() -> [u8; 32] {
    let mut key = [0u8; 32];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut key);
    key
}

#[cfg(not(feature = "secure"))]
pub fn unwrap_dek(_wrapped: &WrappedKey) -> Result<[u8; 32]> {
    anyhow::bail!("secure feature not enabled")
}

#[cfg(not(feature = "secure"))]
pub fn wrap_dek(_kms_key_name: &str, _dek: &[u8; 32]) -> Result<WrappedKey> {
    anyhow::bail!("secure feature not enabled")
}
