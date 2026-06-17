// Demo Client — Full E2E Flow
//
// Orchestrates the complete lifecycle:
//   1. Fetch TEE pubkey
//   2. Encrypt trade inputs
//   3. Send to /prove
//   4. Deploy contract (or use existing)
//   5. open() commitment on Stellar
//   6. close() with proof on Stellar
//
// Run: npx tsx src/demo.ts

import nacl from "tweetnacl";
import {
  Keypair,
  Contract,
  SorobanRpc,
  TransactionBuilder,
  Networks,
  BASE_FEE,
  nativeToScVal,
  scValToNative,
  xdr,
} from "@stellar/stellar-sdk";

// ── Config ─────────────────────────────────────────────────────────
const TEE_URL = process.env.TEE_PROVER_URL ?? "http://localhost:3000";
const STELLAR_RPC = process.env.STELLAR_RPC_URL ?? "https://soroban-testnet.stellar.org";
const SECRET_KEY = process.env.STELLAR_SECRET_KEY ?? "";
const CONTRACT_ID = process.env.CONTRACT_ID ?? "";

// Base64 helpers
const b64enc = (b: Uint8Array) => Buffer.from(b).toString("base64");
const b64dec = (s: string) => Buffer.from(s, "base64");

interface EnclaveBox {
  ephemeralPubkey: string;
  nonce: string;
  ciphertext: string;
}

interface ProofResult {
  commitment: string;
  nullifier: string;
  proof: string;
  publicInputs: string[];
}

// ── Step 1: Fetch TEE pubkey ───────────────────────────────────────
async function fetchPubkey(): Promise<string> {
  const res = await fetch(`${TEE_URL}/pubkey`);
  const data = await res.json() as { pubkey: string };
  console.log("TEE pubkey:", data.pubkey);
  return data.pubkey;
}

// ── Step 2: Encrypt inputs ─────────────────────────────────────────
function encryptInputs(
  inputs: Record<string, string>,
  pubkeyB64: string,
): EnclaveBox {
  const pubkey = b64dec(pubkeyB64);
  const ephemeral = nacl.box.keyPair();
  const nonce = nacl.randomBytes(nacl.box.nonceLength);
  const plaintext = new TextEncoder().encode(JSON.stringify(inputs));
  const ciphertext = nacl.box(plaintext, nonce, pubkey, ephemeral.secretKey);

  return {
    ephemeralPubkey: b64enc(ephemeral.publicKey),
    nonce: b64enc(nonce),
    ciphertext: b64enc(ciphertext),
  };
}

// ── Step 3: Generate proof ─────────────────────────────────────────
async function generateProof(box: EnclaveBox): Promise<ProofResult> {
  const res = await fetch(`${TEE_URL}/prove`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(box),
  });
  if (!res.ok) {
    const err = await res.text();
    throw new Error(`Proof generation failed: ${err}`);
  }
  return res.json() as Promise<ProofResult>;
}

// ── Steps 4-6: Stellar interaction ────────────────────────────────
async function deployContract(server: SorobanRpc.Server, kp: Keypair) {
  const { readFileSync } = await import("fs");
  const wasm = readFileSync(
    "../contract/target/wasm32-unknown-unknown/release/stellar_verifier.wasm",
  );

  const source = await server.getAccount(kp.publicKey());
  const tx = new TransactionBuilder(source, {
    fee: BASE_FEE,
    networkPassphrase: Networks.TESTNET,
  })
    .addOperation(
      // Deploy via Soroban
      xdr.OperationEntryExt.hostFunction(
        xdr.HostFunctionType.hostFunctionTypeCreateContract,
        wasm,
      ),
    )
    .setTimeout(30)
    .build();

  tx.sign(kp);
  const send = await server.sendTransaction(tx);
  if (send.errorResult) throw new Error(`Deploy failed: ${send.errorResult}`);

  // Poll for completion
  let hash = send.hash;
  while (true) {
    const status = await server.getTransaction(hash);
    if (status.status === "SUCCESS") {
      const contractId = status.returnValue?.value()?.toString() ?? "";
      console.log("Contract deployed at:", contractId);
      return contractId;
    }
    if (status.status === "FAILED") {
      throw new Error("Deploy failed");
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
}

async function openPosition(
  server: SorobanRpc.Server,
  kp: Keypair,
  contractId: string,
  commitment: string,
) {
  const contract = new Contract(contractId);
  const source = await server.getAccount(kp.publicKey());

  const tx = new TransactionBuilder(source, {
    fee: BASE_FEE,
    networkPassphrase: Networks.TESTNET,
  })
    .addOperation(contract.call("open", 
      nativeToScVal(kp.publicKey(), { type: "address" }),
      nativeToScVal(commitment, { type: "bytes" }),
      nativeToScVal(1_000_000, { type: "i128" }),
    ))
    .setTimeout(30)
    .build();

  tx.sign(kp);
  const send = await server.sendTransaction(tx);
  if (send.errorResult) throw new Error(`open() failed: ${send.errorResult}`);

  console.log("Position opened, tx:", send.hash);
}

async function closePosition(
  server: SorobanRpc.Server,
  kp: Keypair,
  contractId: string,
  commitment: string,
  nullifier: string,
  proofB64: string,
  publicInputs: string[],
) {
  const contract = new Contract(contractId);
  const source = await server.getAccount(kp.publicKey());

  // Convert proof from base64 to bytes
  const proofBytes = b64dec(proofB64);
  const proofVal = nativeToScVal(Array.from(proofBytes), { type: "bytes" });

  const tx = new TransactionBuilder(source, {
    fee: BASE_FEE,
    networkPassphrase: Networks.TESTNET,
  })
    .addOperation(
      contract.call("close",
        nativeToScVal(kp.publicKey(), { type: "address" }),
        nativeToScVal(commitment, { type: "bytes" }),
        nativeToScVal(nullifier, { type: "bytes" }),
        proofVal,
        nativeToScVal(publicInputs, { type: "vec" }),
      ),
    )
    .setTimeout(30)
    .build();

  tx.sign(kp);
  const send = await server.sendTransaction(tx);
  if (send.errorResult) throw new Error(`close() failed: ${send.errorResult}`);

  console.log("Position closed, tx:", send.hash);
}

// ── Main ───────────────────────────────────────────────────────────
async function main() {
  console.log("=== TEE + ZK + Stellar Demo ===\n");

  // 1. Fetch TEE pubkey
  console.log("1. Fetching TEE pubkey...");
  const pubkey = await fetchPubkey();

  // 2. Encrypt inputs
  console.log("2. Encrypting trade inputs...");
  const tradeInputs = {
    amount: "1000000",
    direction: "0",
    entry_price: "50000000",
    salt: "0xdeadbeef",
    secret: "0xc0ffee",
  };
  const box = encryptInputs(tradeInputs, pubkey);

  // 3. Generate proof
  console.log("3. Generating proof inside TEE...");
  const proof = await generateProof(box);
  console.log("   Commitment:", proof.commitment);
  console.log("   Nullifier:", proof.nullifier);

  // 4-6. Stellar (if credentials provided)
  if (SECRET_KEY) {
    console.log("\n4. Connecting to Stellar testnet...");
    const server = new SorobanRpc.Server(STELLAR_RPC);
    const kp = Keypair.fromSecret(SECRET_KEY);

    // 4. Deploy or use existing contract
    let contractId = CONTRACT_ID;
    if (!contractId) {
      console.log("   Deploying contract...");
      contractId = await deployContract(server, kp);
    } else {
      console.log("   Using existing contract:", contractId);
    }

    // 5. Open position
    console.log("5. Opening position...");
    await openPosition(server, kp, contractId, proof.commitment);

    // 6. Close position with proof
    console.log("6. Closing position with ZK proof...");
    await closePosition(
      server, kp, contractId,
      proof.commitment, proof.nullifier,
      proof.proof, proof.publicInputs,
    );

    console.log("\n Done! Full private trade lifecycle complete.");
  } else {
    console.log("\n Skip Stellar (set STELLAR_SECRET_KEY in .env)");
    console.log(" Proof generated successfully:", proof.proof.slice(0, 40) + "...");
  }
}

main().catch((err) => {
  console.error("Demo failed:", err);
  process.exit(1);
});
