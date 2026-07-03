// NaCl Box: encrypted payload sent by client to TEE
export interface EnclaveBox {
  ephemeralPubkey: string;  // base64, X25519
  nonce: string;            // base64, 24 bytes
  ciphertext: string;       // base64, XSalsa20-Poly1305
}

// Trade details — plaintext inside the NaCl Box
export interface TradeInputs {
  amount: string;
  direction: "0" | "1";
  entry_price: string;
  salt: string;
  secret: string;
}

// Proof generation result
export interface ProofResult {
  commitment: string;       // hex, 32 bytes
  nullifier: string;        // hex, 32 bytes
  proof: string;            // base64, UltraHonk proof bytes
  publicInputs: string[];   // hex, for contract verification
}
