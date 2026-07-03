// TEE Prover Server — Entry Point
//
// Fastify server that runs inside the TEE (Docker / GCP Confidential Space).
//
// Endpoints:
//   GET  /pubkey       — enclave's X25519 public key
//   GET  /attestation  — GCP OIDC attestation token (proves TEE identity)
//   POST /prove        — decrypt inputs, generate UltraHonk proof, return result
//   GET  /health       — liveness check
//
// ## Security Model
// - Private key generated inside enclave, never leaves
// - Inputs decrypted inside enclave, processed, then discarded
// - Proof bytes are public (anyone can verify)
// - Host operator sees: encrypted requests + proof responses only

import Fastify from "fastify";
import { enclavePubkeyB64, decryptInputs, getAttestationToken } from "./enclave.ts";
import { generateProof } from "./prover.ts";
import type { EnclaveBox } from "./types.ts";

const PORT = parseInt(process.env.PORT ?? "3000");
const CIRCUIT_PATH = process.env.CIRCUIT_PATH ?? "/circuit/position_proof.json";

const app = Fastify({ logger: true });

// ── CORS (for browser client dev) ──────────────────────────────────
app.addHook("onRequest", (req, reply, done) => {
  reply.header("Access-Control-Allow-Origin", "*");
  reply.header("Access-Control-Allow-Methods", "GET, POST, OPTIONS");
  reply.header("Access-Control-Allow-Headers", "Content-Type");
  if (req.method === "OPTIONS") {
    reply.code(204).send();
    return;
  }
  done();
});

// ── GET /pubkey ────────────────────────────────────────────────────
// Returns the enclave's public key. Clients use this to encrypt inputs.
// Key is stable for the lifetime of this process.
app.get("/pubkey", async () => ({
  pubkey: enclavePubkeyB64,
  algorithm: "x25519-xsalsa20-poly1305",
}));

// ── GET /attestation ───────────────────────────────────────────────
// In production: returns a GCP OIDC token signed by Google, proving
// this is the expected container running on AMD SEV-SNP hardware.
//
// Client verification:
//   1. Verify token signature against Google's OIDC JWKS
//   2. Check token contains expected container image digest
//   3. Check AMD SEV-SNP measurements match open-source build
//   4. If all pass: trust the pubkey and encrypt inputs
app.get("/attestation", async () => {
  const token = await getAttestationToken();
  return { token, pubkey: enclavePubkeyB64 };
});

// ── POST /prove ────────────────────────────────────────────────────
// Core endpoint. Client sends encrypted trade inputs, gets back proof.
//
// Request:  EnclaveBox { ephemeralPubkey, nonce, ciphertext }
// Response: ProofResult { commitment, nullifier, proof, publicInputs }
//
// The commitment + nullifier go to Stellar open()/close().
// The proof goes to Stellar close() for on-chain verification.
app.post<{ Body: EnclaveBox }>("/prove", async (req, reply) => {
  // Decrypt inside enclave memory
  const inputs = decryptInputs(req.body);
  if (!inputs) {
    return reply.code(400).send({
      error: "decryption failed — wrong pubkey or tampered ciphertext",
    });
  }

  // Validate
  if (inputs.direction !== "0" && inputs.direction !== "1") {
    return reply.code(400).send({ error: "direction must be 0 or 1" });
  }

  try {
    // Generate proof inside enclave.
    // Private inputs exist only in this function scope —
    // never logged, never written to disk, never sent out.
    const result = await generateProof(inputs, CIRCUIT_PATH);
    return result;
  } catch (err) {
    req.log.error(err, "proof generation failed");
    return reply.code(500).send({ error: "proof generation failed" });
  }
});

// ── GET /health ────────────────────────────────────────────────────
app.get("/health", async () => ({ status: "ok" }));

// ── Start ──────────────────────────────────────────────────────────
try {
  await app.listen({ port: PORT, host: "0.0.0.0" });
  console.log(`TEE Prover listening on :${PORT}`);
  console.log(`Pubkey: ${enclavePubkeyB64}`);
} catch (err) {
  app.log.error(err);
  process.exit(1);
}
