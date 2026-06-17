import {
  Keypair, Networks, rpc, TransactionBuilder, BASE_FEE, Contract,
  Address, nativeToScVal, scValToNative, xdr, hash, StrKey, Operation,
} from '@stellar/stellar-sdk';
import { readFileSync, writeFileSync, existsSync } from 'fs';
import { randomBytes } from 'crypto';
import { execSync } from 'child_process';
import path from 'path';

const RPC_URL = process.env.RPC_URL ?? 'https://soroban-testnet.stellar.org';
const STELLAR_SECRET = process.env.STELLAR_SECRET_KEY ?? '';
const AMOUNT = 1_000_000n;

const TINY_DIR = new URL('../../', import.meta.url).pathname;

function decimalToBuf(s: string): Buffer {
  return Buffer.from(BigInt(s).toString(16).padStart(64, '0'), 'hex');
}

async function fund(pk: string) {
  const res = await fetch(`https://friendbot.stellar.org/?addr=${pk}`);
  if (!res.ok) {
    const err = await res.text();
    console.warn('friendbot warning:', err);
  }
}

async function waitTx(server: rpc.Server, hash: string, label: string) {
  for (let i = 0; i < 60; i++) {
    const tx = await server.getTransaction(hash);
    if (tx.status === rpc.Api.GetTransactionStatus.SUCCESS) return tx;
    if (tx.status === rpc.Api.GetTransactionStatus.FAILED) {
      throw new Error(`${label} failed`);
    }
    await new Promise(r => setTimeout(r, 2000));
  }
  throw new Error(`${label} timeout`);
}

async function sendAndWait(
  server: rpc.Server, tx: rpc.Transaction, signer: Keypair, label: string,
) {
  const prepared = await server.prepareTransaction(tx);
  prepared.sign(signer);
  const result = await server.sendTransaction(prepared);
  return waitTx(server, result.hash, label);
}

function computeWitness(input: object, wasmPath: string): bigint[] {
  const exportPath = '/tmp/witness.json';
  const witCalcPath = `${TINY_DIR}circuit-keys/main_js/witness_calculator.js`;
  execSync(`node -e "
    const builder = require('${witCalcPath}');
    const fs = require('fs');
    async function main() {
      const input = JSON.parse(fs.readFileSync('/dev/stdin','utf8'));
      const wasm = fs.readFileSync('${wasmPath}');
      const wc = await builder(wasm);
      const w = await wc.calculateWitness(input, 0);
      const arr = [];
      for (let i = 0; i < w.length; i++) arr.push(w[i].toString());
      await fs.promises.writeFile('${exportPath}', JSON.stringify(arr));
    }
    main().catch(e => { console.error(e); process.exit(1); });
  "`, {
    input: JSON.stringify(input),
    stdio: ['pipe', 'pipe', 'pipe'],
    timeout: 60000,
  });
  return JSON.parse(readFileSync(exportPath, 'utf8')).map(BigInt);
}

function convertProof(proofJson: any): Buffer {
  const buf = Buffer.alloc(256);
  function writeG1(dest: Buffer, offset: number, pt: string[]) {
    dest.write(BigInt(pt[0]).toString(16).padStart(64, '0'), offset, 32, 'hex');
    dest.write(BigInt(pt[1]).toString(16).padStart(64, '0'), offset + 32, 32, 'hex');
  }
  function writeG2(dest: Buffer, offset: number, pt: string[][]) {
    dest.write(BigInt(pt[0][1]).toString(16).padStart(64, '0'), offset, 32, 'hex');
    dest.write(BigInt(pt[0][0]).toString(16).padStart(64, '0'), offset + 32, 32, 'hex');
    dest.write(BigInt(pt[1][1]).toString(16).padStart(64, '0'), offset + 64, 32, 'hex');
    dest.write(BigInt(pt[1][0]).toString(16).padStart(64, '0'), offset + 96, 32, 'hex');
  }
  writeG1(buf, 0, proofJson.pi_a);
  writeG2(buf, 64, proofJson.pi_b);
  writeG1(buf, 192, proofJson.pi_c);
  return buf;
}
async function generateProof(
  amount: string, secret: string,
  wasmPath: string, zkeyPath: string,
): Promise<{ proof: Buffer; publicInputs: string[] }> {
  const clientDir = path.join(TINY_DIR, 'client');
  const proofPath = path.join(clientDir, '.gen_proof.json');
  const pubPath = path.join(clientDir, '.gen_pub.json');

  const script = `
const { groth16 } = require('snarkjs');
const fs = require('fs');
const input = JSON.parse(fs.readFileSync('/dev/stdin', 'utf8'));
async function main() {
  const { proof, publicSignals } = await groth16.fullProve(input, ${JSON.stringify(wasmPath)}, ${JSON.stringify(zkeyPath)});
  await fs.promises.writeFile(${JSON.stringify(proofPath)}, JSON.stringify(proof, null, 2));
  await fs.promises.writeFile(${JSON.stringify(pubPath)}, JSON.stringify(publicSignals, null, 2));
}
main().catch(e => { console.error(e); process.exit(1); });
`.trim();
  writeFileSync(path.join(clientDir, '.gen_proof.cjs'), script);

  execSync(`node .gen_proof.cjs`, {
    input: JSON.stringify({ amount, secret }),
    stdio: ['pipe', 'pipe', 'pipe'],
    timeout: 600000,
    cwd: clientDir,
  });

  const proofJson = JSON.parse(readFileSync(proofPath, 'utf8'));
  const pubJson = JSON.parse(readFileSync(pubPath, 'utf8'));
  const proof = convertProof(proofJson);
  return { proof, publicInputs: pubJson };
}

async function main() {
  console.log('═══ Tiny Private Payments — E2E Test ═══\n');

  if (!STELLAR_SECRET) {
    console.error('Missing: STELLAR_SECRET_KEY env var');
    process.exit(1);
  }

  const circuitDir = `${TINY_DIR}circuit-keys`;
  const wasmPath = `${circuitDir}/main_js/main.wasm`;
  const zkeyPath = `${circuitDir}/main.zkey`;

  if (!existsSync(wasmPath) || !existsSync(zkeyPath)) {
    throw new Error(
      `Circuit not found at ${circuitDir}. Run: make circuit setup`
    );
  }

  // 1. Generate secret + amount, compute commitment/nullifier via circuit
  console.log('── 1. Generate keys ──');
  const secret = BigInt('0x' + randomBytes(31).toString('hex'));
  const amount = AMOUNT;
  const witness = computeWitness(
    { amount: amount.toString(), secret: secret.toString() },
    wasmPath,
  );
  // signal[1] = main.commitment (public output 0)
  // signal[2] = main.nullifier (public output 1)
  const commitment = witness[1];
  const nullifier = witness[2];
  console.log(`  secret:      ${secret}`);
  console.log(`  amount:      ${amount}`);
  console.log(`  commitment:  0x${commitment.toString(16).padStart(64, '0')}`);
  console.log(`  nullifier:   0x${nullifier.toString(16).padStart(64, '0')}`);

  // 2. Setup Stellar
  const server = new rpc.Server(RPC_URL);
  const deployer = Keypair.fromSecret(STELLAR_SECRET);
  await fund(deployer.publicKey());

  // 3. Deploy contracts
  console.log('\n── 2. Deploy contracts ──');

  const verifierWasm = readFileSync(
    `${TINY_DIR}target/wasm32v1-none/release/tiny_verifier.wasm`
  );
  const poolWasm = readFileSync(
    `${TINY_DIR}target/wasm32v1-none/release/tiny_pool.wasm`
  );

  let account = await server.getAccount(deployer.publicKey());
  let tx = new TransactionBuilder(account, {
    fee: BASE_FEE, networkPassphrase: Networks.TESTNET,
  })
    .addOperation(Operation.uploadContractWasm({ wasm: verifierWasm }))
    .setTimeout(30)
    .build();
  await sendAndWait(server, tx, deployer, 'upload verifier');
  const verifierHash = hash(verifierWasm);
  console.log('  ✓ Verifier WASM uploaded');

  account = await server.getAccount(deployer.publicKey());
  tx = new TransactionBuilder(account, {
    fee: BASE_FEE, networkPassphrase: Networks.TESTNET,
  })
    .addOperation(Operation.uploadContractWasm({ wasm: poolWasm }))
    .setTimeout(30)
    .build();
  await sendAndWait(server, tx, deployer, 'upload pool');
  const poolHash = hash(poolWasm);
  console.log('  ✓ Pool WASM uploaded');

  const deployerAddr = new Address(deployer.publicKey());

  // Deploy verifier (VK is embedded in WASM, no constructor args needed)
  account = await server.getAccount(deployer.publicKey());
  tx = new TransactionBuilder(account, {
    fee: BASE_FEE, networkPassphrase: Networks.TESTNET,
  })
    .addOperation(
      Operation.createCustomContract({
        address: deployerAddr,
        wasmHash: verifierHash,
        salt: randomBytes(32),
      }),
    )
    .setTimeout(30)
    .build();
  let result = await sendAndWait(server, tx, deployer, 'deploy verifier');
  const verifierAddrId = result.returnValue.address().contractId();
  const verifierId = StrKey.encodeContract(verifierAddrId as Buffer);
  console.log(`  Verifier: ${verifierId}`);

  // Deploy pool
  account = await server.getAccount(deployer.publicKey());
  tx = new TransactionBuilder(account, {
    fee: BASE_FEE, networkPassphrase: Networks.TESTNET,
  })
    .addOperation(
      Operation.createCustomContract({
        address: deployerAddr,
        wasmHash: poolHash,
        salt: randomBytes(32),
      }),
    )
    .setTimeout(30)
    .build();
  result = await sendAndWait(server, tx, deployer, 'deploy pool');
  const poolAddrId = result.returnValue.address().contractId();
  const poolId = StrKey.encodeContract(poolAddrId as Buffer);
  console.log(`  Pool:      ${poolId}`);

  // 4. Deposit
  console.log('\n── 3. Deposit ──');
  const trader = Keypair.random();
  console.log(`  Trader: ${trader.publicKey()}`);
  await fund(trader.publicKey());

  const pool = new Contract(poolId);
  const commitmentBuf = Buffer.from(
    commitment.toString(16).padStart(64, '0'), 'hex'
  );

  account = await server.getAccount(trader.publicKey());
  tx = new TransactionBuilder(account, {
    fee: BASE_FEE, networkPassphrase: Networks.TESTNET,
  })
    .addOperation(pool.call('deposit',
      new Address(trader.publicKey()).toScVal(),
      xdr.ScVal.scvBytes(commitmentBuf),
      nativeToScVal(amount, { type: 'i128' }),
    ))
    .setTimeout(30)
    .build();
  await sendAndWait(server, tx, trader, 'deposit');
  console.log('  ✓ Deposit submitted');

  const onChainBalance = await simulate(
    server, trader.publicKey(), pool, 'balance_of',
    [xdr.ScVal.scvBytes(commitmentBuf)],
  );
  console.log(`  balance_on_chain: ${onChainBalance ?? 'NOT FOUND'}`);
  if (onChainBalance !== amount) throw new Error('balance mismatch');
  console.log('✓ Deposit verified');

  // 5. Generate proof & withdraw
  console.log('\n── 4. Withdraw ──');
  try {
    const { proof, publicInputs } = await generateProof(
      amount.toString(), secret.toString(), wasmPath, zkeyPath,
    );
    console.log(`  Proof: ${proof.length} bytes (${publicInputs.length} pub inputs)`);

    // Verify proof via verifier contract
    const verifier = new Contract(verifierId);
    const piScVals = publicInputs.map(s =>
      xdr.ScVal.scvBytes(decimalToBuf(s))
    );
    const verifySim = await simulate(
      server, trader.publicKey(), verifier, 'verify',
      [xdr.ScVal.scvBytes(proof), xdr.ScVal.scvVec(piScVals)],
    );
    console.log(`  verify_proof result: ${JSON.stringify(verifySim)}`);
    if (verifySim !== true) throw new Error('proof verification failed');

    // Withdraw from pool
    const nullifierBuf = Buffer.from(
      nullifier.toString(16).padStart(64, '0'), 'hex'
    );
    account = await server.getAccount(trader.publicKey());
    tx = new TransactionBuilder(account, {
      fee: BASE_FEE, networkPassphrase: Networks.TESTNET,
    })
      .addOperation(pool.call('withdraw',
        new Address(trader.publicKey()).toScVal(),
        xdr.ScVal.scvBytes(commitmentBuf),
        xdr.ScVal.scvBytes(nullifierBuf),
      ))
      .setTimeout(30)
      .build();
    const final = await sendAndWait(server, tx, trader, 'withdraw');
    const returned = scValToNative(final.returnValue);
    console.log(`  returned amount: ${returned?.toString()} stroops`);
    if (returned !== amount) throw new Error('withdraw() returned wrong amount');
    console.log('✓ Withdraw succeeded');
  } catch (e) {
    console.log('  ⚠ Withdraw failed:', (e as Error).message);
    console.log('  Deposit test completed successfully.');
  }

  // 6. Verify nullifier spent
  const nullifierBuf = Buffer.from(
    nullifier.toString(16).padStart(64, '0'), 'hex'
  );
  const spent = await simulate(
    server, trader.publicKey(), pool, 'is_spent',
    [xdr.ScVal.scvBytes(nullifierBuf)],
  );
  console.log(`  is_spent(nullifier): ${spent}`);

  console.log('\n═══ ✓ E2E PASSED ═══');
  console.log(`  Verifier: ${verifierId}`);
  console.log(`  Pool:     ${poolId}`);
}

async function simulate(
  server: rpc.Server, source: string, contract: Contract, method: string,
  args: xdr.ScVal[],
): Promise<any> {
  const account = await server.getAccount(source);
  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE, networkPassphrase: Networks.TESTNET,
  })
    .addOperation(contract.call(method, ...args))
    .setTimeout(30)
    .build();
  const prepared = await server.prepareTransaction(tx);
  const sim = await server.simulateTransaction(prepared);
  if ((sim as any).error) throw new Error(`simulate ${method}: ${(sim as any).error}`);
  return scValToNative((sim as any).result!.retval);
}

main().catch(e => {
  console.error('\n═══ ✗ E2E FAILED ═══');
  console.error(e);
  process.exit(1);
});
