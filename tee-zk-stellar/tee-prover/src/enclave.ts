// TEE Enclave Boundary
//
// This module manages the enclave's identity and input security.
//
// ## Key Generation
// On startup, a NaCl X25519 keypair is generated. The private key lives
// only in process memory — in a real GCP Confidential Space deployment,
// this memory is encrypted by AMD SEV-SNP hardware.
//
// ## Attestation (production)
// GCP Confidential Space provides an OIDC token signed by Google that
// proves to clients: "my container image hash is X, running on SEV-SNP."
// Clients verify this before sending encrypted inputs.
//
// ## Dev mode
// When SIMULATE_TEE=true, the attestation endpoint returns a placeholder.
// Clients should skip attestation verification in dev mode.

import nacl from "tweetnacl";
import { encodeBase64, decodeBase64 } from "@std/encoding/base64";
import type { EnclaveBox, TradeInputs } from "./types.ts";

// ── Singleton enclave keypair ────────────────────────────────────────
// Generated once per process. In production, this memory is hardware-
// encrypted — the host OS and cloud provider cannot read it.
const _keypair = nacl.box.keyPair();

export const enclavePubkeyB64 = encodeBase64(_keypair.publicKey);
export const enclaveSecretKey = _keypair.secretKey;

// ── Decryption ───────────────────────────────────────────────────────
// Called by POST /prove. Decrypts client-encrypted trade inputs inside
// the enclave. Returns null if authentication fails.

export function decryptInputs(box: EnclaveBox): TradeInputs | null {
  const ephemeralPubkey = decodeBase64(box.ephemeralPubkey);
  const nonce = decodeBase64(box.nonce);
  const ciphertext = decodeBase64(box.ciphertext);

  const plaintext = nacl.box.open(
    ciphertext,
    nonce,
    ephemeralPubkey,
    enclaveSecretKey,
  );

  if (!plaintext) return null;
  return JSON.parse(new TextDecoder().decode(plaintext));
}

// ── Attestation ──────────────────────────────────────────────────────
// In production, this fetches a GCP OIDC token from the instance
// metadata server. The token proves the container image hash and
// SEV-SNP measurements to clients.
//
// In dev mode (SIMULATE_TEE=true), returns a placeholder.

export async function getAttestationToken(): Promise<string> {
  if (process.env.SIMULATE_TEE === "true") {
    return "dev-mode:simulated-tee-no-attestation";
  }

  const metadataUrl =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/identity" +
    "?audience=https://example.com&format=full";

  try {
    const res = await fetch(metadataUrl, {
      headers: { "Metadata-Flavor": "Google" },
    });
    if (res.ok) {
      return await res.text();
    }
  } catch {
    // Not running on GCP
  }

  return "dev-mode:no-attestation";
}
