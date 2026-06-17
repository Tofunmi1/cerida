// UltraHonk Proof Generation (runs inside TEE)
//
// Uses @aztec/bb.js (Barretenberg WASM) to generate UltraHonk proofs
// from Noir circuit witnesses.
//
// ## Flow
// 1. Load compiled circuit JSON (baked into Docker image)
// 2. Noir.execute() — compute witness (commitment + nullifier)
// 3. UltraHonkBackend.generateProof() — cryptographic proof
//
// ## Performance
// - Witness generation: ~100ms (pure JS, no crypto)
// - Proof generation: ~5-15s (Barretenberg WASM, CPU-bound)
// - Inside a TEE: same timing, but inputs stay encrypted in memory

import { Noir } from "@noir-lang/noir_js";
import { UltraHonkBackend } from "@aztec/bb.js";
import type { CompiledCircuit } from "@noir-lang/types";
import type { TradeInputs, ProofResult } from "./types.ts";

// Cache circuit + backend across requests (singleton)
let _circuit: CompiledCircuit | null = null;
let _backend: UltraHonkBackend | null = null;
let _noir: Noir | null = null;

async function getProver(circuitPath: string) {
  if (_circuit && _backend && _noir) {
    return { backend: _backend, noir: _noir };
  }

  const { readFileSync } = await import("fs");
  _circuit = JSON.parse(
    readFileSync(circuitPath, "utf8"),
  ) as CompiledCircuit;
  _backend = new UltraHonkBackend(_circuit.bytecode);
  _noir = new Noir(_circuit);

  return { backend: _backend, noir: _noir };
}

// Generate an UltraHonk proof for a private position.
// Inputs have already been decrypted from the NaCl Box —
// they exist in enclave memory only during this call.
export async function generateProof(
  inputs: TradeInputs,
  circuitPath: string,
): Promise<ProofResult> {
  const { backend, noir } = await getProver(circuitPath);

  // Step 1: Witness generation.
  // Runs the circuit's arithmetic to compute commitment + nullifier
  // consistent with the circuit's Poseidon2 constraints.
  const { returnValue } = await noir.execute({
    amount: inputs.amount,
    direction: inputs.direction,
    entry_price: inputs.entry_price,
    salt: inputs.salt,
    secret: inputs.secret,
  });

  const [commitmentField, nullifierField] = returnValue as [string, string];

  // Step 2: UltraHonk proof generation.
  // Barretenberg's UltraHonkBackend computes the proof that binds
  // the private witnesses to the public outputs. This proof is what
  // the Soroban contract verifies using Protocol 26 BN254 host functions.
  const proof = await backend.generateProof();

  const toHex = (n: string) =>
    BigInt(n).toString(16).padStart(64, "0");

  return {
    commitment: toHex(commitmentField),
    nullifier: toHex(nullifierField),
    proof: Buffer.from(proof).toString("base64"),
    publicInputs: [toHex(commitmentField), toHex(nullifierField)],
  };
}
