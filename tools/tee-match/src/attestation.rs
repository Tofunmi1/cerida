// ── GCP Confidential Space attestation ──
// All behind `cfg(feature = "secure")`.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Claims in a GCA OIDC attestation token.
#[derive(Debug, Deserialize)]
pub struct AttestationClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub exp: u64,
    pub iat: u64,
    pub nbf: u64,
    #[serde(default)]
    pub eat_nonce: Vec<String>,
    #[serde(default)]
    pub hwmodel: String,
    #[serde(default)]
    pub dbgstat: String,
    #[serde(default)]
    pub secboot: bool,
    #[serde(default)]
    pub oemid: u64,
    #[serde(default)]
    pub swname: String,
    #[serde(default)]
    pub submods: Option<Submods>,
    #[serde(default)]
    pub google_service_accounts: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Submods {
    pub container: Option<ContainerClaims>,
}

#[derive(Debug, Deserialize)]
pub struct ContainerClaims {
    pub image_digest: Option<String>,
}

/// Expected values for a production deployment. Tune per deployment.
pub struct AttestationPolicy {
    pub expected_hwmodel: String,
    pub expected_dbgstat: String,
    pub expected_secboot: bool,
    pub expected_image_digest: String,
    pub expected_issuer: String,
    pub allowed_audiences: Vec<String>,
}

impl Default for AttestationPolicy {
    fn default() -> Self {
        Self {
            expected_hwmodel: "GCP_AMD_SEV_SNP".into(),
            expected_dbgstat: "disabled-since-boot".into(),
            expected_secboot: true,
            expected_image_digest: String::new(), // must be set
            expected_issuer: "https://confidentialcomputing.googleapis.com".into(),
            allowed_audiences: vec![],
        }
    }
}

/// Fetch the JWKS URI for GCA OIDC tokens.
#[cfg(feature = "secure")]
pub fn gca_jwks_uri() -> &'static str {
    "https://www.googleapis.com/service_accounts/v1/metadata/jwk/signer@confidentialspace-sign.iam.gserviceaccount.com"
}

/// Verify an attestation token against the given policy and nonce.
/// Returns the verified claims on success.
#[cfg(feature = "secure")]
pub fn verify_attestation_token(
    token: &str,
    policy: &AttestationPolicy,
    expected_nonce: &[u8],
) -> Result<AttestationClaims> {
    use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};

    // Fetch JWKS keys from Google
    let jwks_uri = gca_jwks_uri();
    let resp = reqwest::blocking::get(jwks_uri)?;
    let jwks: jsonwebtoken::jwk::JwkSet = resp.json()?;

    // Decode header to find the key ID
    let header = jsonwebtoken::decode_header(token)?;
    let kid = header.kid.ok_or_else(|| anyhow::anyhow!("no kid in token header"))?;

    // Find matching key in JWKS
    let jwk = jwks
        .keys
        .iter()
        .find(|k| k.common.key_id.as_deref() == Some(&kid))
        .ok_or_else(|| anyhow::anyhow!("key {kid} not found in JWKS"))?;

    let key = DecodingKey::from_jwk(jwk)?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[&policy.expected_issuer]);
    validation.set_audience(&policy.allowed_audiences);
    // Reject if expired
    validation.validate_exp = true;
    // Reject if not yet valid
    validation.validate_nbf = true;

    let token_data = decode::<AttestationClaims>(token, &key, &validation)?;
    let claims = token_data.claims;

    // Check hardware model
    if claims.hwmodel != policy.expected_hwmodel {
        bail!(
            "hwmodel mismatch: expected {}, got {}",
            policy.expected_hwmodel,
            claims.hwmodel
        );
    }

    // Check debug status
    if claims.dbgstat != policy.expected_dbgstat {
        bail!(
            "dbgstat mismatch: expected {}, got {}",
            policy.expected_dbgstat,
            claims.dbgstat
        );
    }

    // Check secure boot
    if claims.secboot != policy.expected_secboot {
        bail!(
            "secboot mismatch: expected {}, got {}",
            policy.expected_secboot,
            claims.secboot
        );
    }

    // Check container image digest
    if !policy.expected_image_digest.is_empty() {
        let actual = claims
            .submods
            .as_ref()
            .and_then(|s| s.container.as_ref())
            .and_then(|c| c.image_digest.as_ref())
            .ok_or_else(|| anyhow::anyhow!("no image_digest in token claims"))?;
        if actual != &policy.expected_image_digest {
            bail!(
                "image_digest mismatch: expected {}, got {}",
                policy.expected_image_digest,
                actual
            );
        }
    }

    // Check nonce (TLS EKM channel binding)
    if !claims.eat_nonce.is_empty() {
        let expected_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            expected_nonce,
        );
        let nonce_match = claims.eat_nonce.iter().any(|n| n == &expected_b64);
        if !nonce_match {
            bail!("nonce mismatch: expected {expected_b64}, got {:?}", claims.eat_nonce);
        }
    }

    Ok(claims)
}

/// Request an attestation token from the GCP Confidential Space local API.
/// The Confidential Space launcher exposes an API at a well-known endpoint.
#[cfg(feature = "secure")]
pub fn request_attestation_token(
    audience: &str,
    nonces: &[String],
    token_type: &str,
) -> Result<String> {
    #[derive(Serialize)]
    struct TokenRequest {
        audience: String,
        token_type: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        nonces: Vec<String>,
    }

    let body = TokenRequest {
        audience: audience.to_string(),
        token_type: token_type.to_string(),
        nonces: nonces.to_vec(),
    };

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post("http://localhost:8080/v1/token")
        .json(&body)
        .send()?;

    if !resp.status().is_success() {
        bail!(
            "attestation token request failed: {} {}",
            resp.status(),
            resp.text().unwrap_or_default()
        );
    }

    #[derive(Deserialize)]
    struct TokenResponse {
        token: String,
    }

    let tr: TokenResponse = resp.json()?;
    Ok(tr.token)
}

/// Verify an attestation token without full JWKS fetch (for development).
/// Only checks claims that don't require signature verification.
pub fn verify_attestation_claims(
    claims: &AttestationClaims,
    policy: &AttestationPolicy,
    expected_nonce_b64: &str,
) -> Result<()> {
    if claims.hwmodel != policy.expected_hwmodel {
        bail!("hwmodel mismatch");
    }
    if claims.dbgstat != policy.expected_dbgstat {
        bail!("dbgstat mismatch");
    }
    if claims.secboot != policy.expected_secboot {
        bail!("secboot mismatch");
    }
    // Check nonce
    if !claims.eat_nonce.is_empty() {
        let nonce_match = claims.eat_nonce.iter().any(|n| n == expected_nonce_b64);
        if !nonce_match {
            bail!("nonce mismatch");
        }
    }
    Ok(())
}

#[cfg(not(feature = "secure"))]
pub fn verify_attestation_token(
    _token: &str,
    _policy: &AttestationPolicy,
    _expected_nonce: &[u8],
) -> Result<AttestationClaims> {
    anyhow::bail!("secure feature not enabled")
}

#[cfg(not(feature = "secure"))]
pub fn request_attestation_token(
    _audience: &str,
    _nonces: &[String],
    _token_type: &str,
) -> Result<String> {
    anyhow::bail!("secure feature not enabled")
}
