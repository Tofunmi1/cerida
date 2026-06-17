// NaCl Box encryption utilities
// Used by client to encrypt trade inputs before sending to TEE.
// Used by server to decrypt inside the enclave.
//
// Algorithm: X25519 + XSalsa20-Poly1305 (NaCl Box)
//   - Ephemeral keypair generated per-request by client
//   - Shared secret via X25519 DH
//   - Authenticated encryption via XSalsa20-Poly1305

import nacl from "tweetnacl";
import { encodeBase64, decodeBase64 } from "@std/encoding/base64";
import type { EnclaveBox, TradeInputs } from "./types.ts";

// Encrypt trade inputs with TEE's public key.
// Returns the EnclaveBox to send to the server.
export function encryptTradeInputs(
  inputs: TradeInputs,
  teePubkeyB64: string,
): { box: EnclaveBox; ephemeralSecretKey: Uint8Array } {
  const teePubkey = decodeBase64(teePubkeyB64);
  const ephemeral = nacl.box.keyPair();
  const nonce = nacl.randomBytes(nacl.box.nonceLength);

  const plaintext = new TextEncoder().encode(JSON.stringify(inputs));
  const ciphertext = nacl.box(
    plaintext,
    nonce,
    teePubkey,
    ephemeral.secretKey,
  );

  return {
    box: {
      ephemeralPubkey: encodeBase64(ephemeral.publicKey),
      nonce: encodeBase64(nonce),
      ciphertext: encodeBase64(ciphertext),
    },
    ephemeralSecretKey: ephemeral.secretKey,
  };
}

// Decrypt an EnclaveBox with the TEE's private key.
// Returns null if authentication fails (tampered or wrong pubkey).
export function decryptTradeInputs(
  box: EnclaveBox,
  teeSecretKey: Uint8Array,
): TradeInputs | null {
  const ephemeralPubkey = decodeBase64(box.ephemeralPubkey);
  const nonce = decodeBase64(box.nonce);
  const ciphertext = decodeBase64(box.ciphertext);

  const plaintext = nacl.box.open(
    ciphertext,
    nonce,
    ephemeralPubkey,
    teeSecretKey,
  );

  if (!plaintext) return null;
  return JSON.parse(new TextDecoder().decode(plaintext));
}
