import {
  Contract,
  Keypair,
  Networks,
  rpc as SorobanRpc,
  TransactionBuilder,
  BASE_FEE,
  nativeToScVal,
  scValToNative,
  Address,
  xdr,
} from '@stellar/stellar-sdk'

export const NETWORK_PASSPHRASE =
  import.meta.env.VITE_NETWORK_PASSPHRASE ?? Networks.TESTNET

export const RPC_URL =
  import.meta.env.VITE_SOROBAN_RPC_URL ?? 'https://soroban-testnet.stellar.org'

export const CONTRACT_IDS = {
  perpEngine: import.meta.env.VITE_PERP_ENGINE_ID as string,
  shieldedPool: import.meta.env.VITE_SHIELDED_POOL_ID as string,
  orderbook: import.meta.env.VITE_ORDERBOOK_ID as string,
  collateralToken: import.meta.env.VITE_COLLATERAL_TOKEN_ID as string,
} as const

export const rpc = new SorobanRpc.Server(RPC_URL)

// ── helpers ──────────────────────────────────────────────────────────────────

function i128ToScVal(value: bigint): xdr.ScVal {
  return nativeToScVal(value, { type: 'i128' })
}

function u64ToScVal(value: number | bigint): xdr.ScVal {
  return nativeToScVal(BigInt(value), { type: 'u64' })
}

function addressToScVal(addr: string): xdr.ScVal {
  return new Address(addr).toScVal()
}

function hexToBytes(hex: string): Uint8Array {
  const bytes = new Uint8Array(hex.length / 2)
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(hex.substring(i * 2, i * 2 + 2), 16)
  }
  return bytes
}

function bytes32ToScVal(hex: string): xdr.ScVal {
  return xdr.ScVal.scvBytes(Buffer.from(hex, 'hex'))
}

// TimeInForce enum values matching the contract
export const TIF = { GTC: 0, IOC: 1, FOK: 2, GTD: 3 } as const

export const DEFAULT_ASSET = '0000000000000000000000000000000000000000000000000000000000000000'

// ── transaction builder ───────────────────────────────────────────────────────

export interface ContractCall {
  contractId: string
  method: string
  args: xdr.ScVal[]
}

export async function buildTx(
  sourcePublicKey: string,
  contractId: string,
  method: string,
  args: xdr.ScVal[],
) {
  return buildBundleTx(sourcePublicKey, [{ contractId, method, args }])
}

/** Build a single transaction containing multiple contract calls.
 * Operations execute sequentially in order; storage writes by earlier ops
 * are visible to later ops within the same transaction. */
export async function buildBundleTx(
  sourcePublicKey: string,
  calls: ContractCall[],
) {
  const account = await rpc.getAccount(sourcePublicKey)

  const builder = new TransactionBuilder(account, {
    fee: (BigInt(BASE_FEE) * BigInt(calls.length)).toString(),
    networkPassphrase: NETWORK_PASSPHRASE,
  })

  for (const { contractId, method, args } of calls) {
    builder.addOperation(new Contract(contractId).call(method, ...args))
  }
  builder.setTimeout(300)

  const tx = builder.build()
  const simResult = await rpc.simulateTransaction(tx)
  if (SorobanRpc.Api.isSimulationError(simResult)) {
    throw new Error(`Simulation failed: ${simResult.error}`)
  }

  return SorobanRpc.assembleTransaction(tx, simResult).build()
}

/**
 * Compute amount_commitment = SHA256(amount_le_16bytes || blinding_32bytes).
 * Matches the contract's `note_amount_commitment` function exactly.
 * Must be revealed as (collateral_amount, collateral_blinding) to the TEE relay.
 */
export async function computeAmountCommitment(amount: bigint, blinding: Uint8Array): Promise<string> {
  const amountLe = new Uint8Array(16)
  let rem = amount
  for (let i = 0; i < 16; i++) {
    amountLe[i] = Number(rem & 0xffn)
    rem >>= 8n
  }
  const preimage = new Uint8Array(48)
  preimage.set(amountLe, 0)
  preimage.set(blinding, 16)
  const digest = await crypto.subtle.digest('SHA-256', preimage)
  return Array.from(new Uint8Array(digest)).map(b => b.toString(16).padStart(2, '0')).join('')
}

// ── Note generation (client-side, for deposit) ────────────────────────────────
// Note: commitment is random bytes for now. Real ZK commitment would use
// Poseidon2 hash from the circuits. The contract stores the note by commitment
// and verifies ownership via ZK proof on spend.

export function generateNote(): { secret: string; commitment: string; nullifier: string } {
  const bytes = new Uint8Array(32)
  crypto.getRandomValues(bytes)
  const commitment = Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('')

  crypto.getRandomValues(bytes)
  const nullifier = Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('')

  crypto.getRandomValues(bytes)
  const secret = Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('')

  return { secret, commitment, nullifier }
}

export function randomCommitment(): string {
  return generateNote().commitment
}

// TimeInForce is `#[repr(u32)]` in the contract, so it serializes as ScVal::U32,
// NOT as a Map variant. GTC=0, IOC=1, FOK=2, GTD=3 (matches `TIF` below).
function tifToScVal(value: number): xdr.ScVal {
  return xdr.ScVal.scvU32(value & 0xffffffff)
}

// Groth16Proof: placeholder zero proof (will fail on-chain until WASM prover is wired)
function zeroProof(): xdr.ScVal {
  return xdr.ScVal.scvMap([
    new xdr.ScMapEntry({ key: xdr.ScVal.scvSymbol('a'), val: xdr.ScVal.scvBytes(Buffer.alloc(64)) }),
    new xdr.ScMapEntry({ key: xdr.ScVal.scvSymbol('b'), val: xdr.ScVal.scvBytes(Buffer.alloc(128)) }),
    new xdr.ScMapEntry({ key: xdr.ScVal.scvSymbol('c'), val: xdr.ScVal.scvBytes(Buffer.alloc(64)) }),
  ])
}

/**
 * Convert a TEE proof JSON string to an on-chain Groth16Proof ScVal.
 * The TEE returns proofs as {a:"<128-char hex>", b:"<256-char hex>", c:"<128-char hex>"}.
 * The contract expects ScVal::Map({a: Bytes(64), b: Bytes(128), c: Bytes(64)}).
 */
export function proofJsonToScVal(proofJson: string): xdr.ScVal {
  const p = JSON.parse(proofJson) as { a: string; b: string; c: string }
  return xdr.ScVal.scvMap([
    new xdr.ScMapEntry({ key: xdr.ScVal.scvSymbol('a'), val: xdr.ScVal.scvBytes(Buffer.from(p.a, 'hex')) }),
    new xdr.ScMapEntry({ key: xdr.ScVal.scvSymbol('b'), val: xdr.ScVal.scvBytes(Buffer.from(p.b, 'hex')) }),
    new xdr.ScMapEntry({ key: xdr.ScVal.scvSymbol('c'), val: xdr.ScVal.scvBytes(Buffer.from(p.c, 'hex')) }),
  ])
}

export function crossMarginKey(walletAddress: string): string {
  const storageKey = `cerida-cross-key-${walletAddress}`
  const existing = localStorage.getItem(storageKey)
  if (existing) return existing
  const bytes = new Uint8Array(32)
  crypto.getRandomValues(bytes)
  const key = Array.from(bytes).map((b) => b.toString(16).padStart(2, '0')).join('')
  localStorage.setItem(storageKey, key)
  return key
}

// ── Position Meta ─────────────────────────────────────────────────────────────

// On-chain position data — only what the contract actually stores.
// Financial data (entry price, collateral, size) is kept in StoredPosition (localStorage).
export interface PositionMeta {
  status: bigint          // 0=Open 1=Matched 2=Closed 4=Liquidated
  createdAt: bigint
  partialLiqDone: boolean
  assetId: string         // hex32
}

// Sentinel: position exists on-chain but fields couldn't be parsed (shouldn't happen normally)
export const POSITION_NOT_FOUND = 'not_found' as const

export async function getPosition(
  commitment: string,
  sourcePublicKey?: string,
): Promise<PositionMeta | null | typeof POSITION_NOT_FOUND> {
  try {
    const source = sourcePublicKey ?? 'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN'
    const account = await rpc.getAccount(source)
    const contract = new Contract(CONTRACT_IDS.perpEngine)
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: NETWORK_PASSPHRASE,
    })
      .addOperation(contract.call('get_position', bytes32ToScVal(commitment)))
      .setTimeout(300)
      .build()

    const result = await rpc.simulateTransaction(tx)
    // Simulation error (e.g. contract panicked because position doesn't exist)
    if (!SorobanRpc.Api.isSimulationSuccess(result)) return POSITION_NOT_FOUND
    if (!result.result?.retval) return POSITION_NOT_FOUND

    const raw = scValToNative(result.result.retval)
    if (!raw || typeof raw !== 'object') return POSITION_NOT_FOUND

    // PositionStatus enum: Open=0, Matched=1, Closed=2, Liquidated=4
    const statusVal = raw.status
    const statusNum = typeof statusVal === 'object' && statusVal !== null
      ? Object.keys(statusVal as Record<string, unknown>).length === 0 ? 0n
        : BigInt(Object.keys(statusVal as Record<string, unknown>)[0] === 'Open' ? 0
          : Object.keys(statusVal as Record<string, unknown>)[0] === 'Matched' ? 1
          : Object.keys(statusVal as Record<string, unknown>)[0] === 'Closed' ? 2 : 4)
      : BigInt(statusVal ?? 0)

    const assetIdRaw = raw.asset_id
    const assetIdHex = assetIdRaw instanceof Uint8Array
      ? Array.from(assetIdRaw).map(b => b.toString(16).padStart(2, '0')).join('')
      : typeof assetIdRaw === 'string' ? assetIdRaw
      : '0'.repeat(64)

    return {
      status: statusNum,
      createdAt: BigInt(raw.created_at ?? 0),
      partialLiqDone: !!raw.partial_liq_done,
      assetId: assetIdHex,
    }
  } catch (e) {
    console.warn('getPosition failed for cmt', commitment.slice(0, 12), ':', e)
    return null
  }
}

// ── Note APIs ─────────────────────────────────────────────────────────────────

/** Pure op-builder: deposit collateral as a shielded note. No ZK proof needed — only when spending. */
export function depositNoteCall(
  sourcePublicKey: string,
  noteCommitment: string,
  amount: bigint,
  amountCommitment: string,  // SHA256(amount_le_16bytes || blinding_32bytes) — see computeAmountCommitment
): ContractCall {
  return {
    contractId: CONTRACT_IDS.perpEngine,
    method: 'deposit_note',
    args: [
      addressToScVal(sourcePublicKey),
      bytes32ToScVal(noteCommitment),
      i128ToScVal(amount),
      bytes32ToScVal(amountCommitment),
    ],
  }
}

/** Deposit collateral as a shielded note. No ZK proof needed — only when spending. */
export async function buildDepositNoteTx(
  sourcePublicKey: string,
  noteCommitment: string,
  amount: bigint,
  amountCommitment: string,
) {
  return buildBundleTx(sourcePublicKey, [depositNoteCall(sourcePublicKey, noteCommitment, amount, amountCommitment)])
}

/** Withdraw from a shielded note to a recipient. Requires a valid NoteSpend proof. TEE-only. */
export async function buildWithdrawNoteTx(
  sourcePublicKey: string,
  noteCmt: string,
  noteNull: string,
  recipientPk: string,
  amount: bigint,
  blinding: string,  // hex32 — the blinding used in amount_commitment at deposit time
  noteProof?: xdr.ScVal,
) {
  return buildTx(sourcePublicKey, CONTRACT_IDS.perpEngine, 'withdraw_note', [
    bytes32ToScVal(noteCmt),
    bytes32ToScVal(noteNull),
    addressToScVal(recipientPk),
    i128ToScVal(amount),
    bytes32ToScVal(blinding),
    noteProof ?? zeroProof(),
  ])
}

/** Query whether a note exists. Returns the stored amount_commitment hex (BytesN<32>) or null. */
export async function getNoteBalance(noteCommitment: string): Promise<string | null> {
  try {
    const account = await rpc.getAccount('GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN')
    const contract = new Contract(CONTRACT_IDS.perpEngine)
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: NETWORK_PASSPHRASE,
    })
      .addOperation(contract.call('get_note', bytes32ToScVal(noteCommitment)))
      .setTimeout(300)
      .build()

    const result = await rpc.simulateTransaction(tx)
    if (!SorobanRpc.Api.isSimulationSuccess(result) || !result.result?.retval) return null
    const native = scValToNative(result.result.retval)
    if (!native) return null
    return Buffer.from(native as Uint8Array).toString('hex')
  } catch {
    return null
  }
}

// ── Position APIs ─────────────────────────────────────────────────────────────

/** Pure op-builder: open a position from a shielded note. Called by the TEE relay, not the frontend directly. */
export function openPositionFromNoteCall(
  opts: {
    noteCmt: string
    noteNull: string
    commitment: string
    sealedParams?: string           // hex-encoded Bytes blob (TEE-generated encrypted params)
    liquidationRecipientNote?: string
    portfolioKey?: string
    assetId?: string
    collateralAmount: bigint
    collateralBlinding: string      // hex32 — revealed to verify amount_commitment on-chain
    settlementCommitment: string    // hex32 — pre-committed settlement note destination
    noteProof?: xdr.ScVal
    commitProof?: xdr.ScVal
  },
): ContractCall {
  const zeroNote = bytes32ToScVal('0'.repeat(64))
  const pk = bytes32ToScVal(opts.portfolioKey ?? '0'.repeat(64))
  const aid = bytes32ToScVal(opts.assetId ?? DEFAULT_ASSET)
  const sealedParams = opts.sealedParams
    ? xdr.ScVal.scvBytes(Buffer.from(opts.sealedParams, 'hex'))
    : xdr.ScVal.scvBytes(Buffer.alloc(0))

  return {
    contractId: CONTRACT_IDS.perpEngine,
    method: 'open_position_from_note',
    args: [
      bytes32ToScVal(opts.noteCmt),
      bytes32ToScVal(opts.noteNull),
      bytes32ToScVal(opts.commitment),
      sealedParams,
      opts.liquidationRecipientNote ? bytes32ToScVal(opts.liquidationRecipientNote) : zeroNote,
      pk,
      aid,
      i128ToScVal(opts.collateralAmount),
      bytes32ToScVal(opts.collateralBlinding),
      bytes32ToScVal(opts.settlementCommitment),
      opts.noteProof ?? zeroProof(),
      opts.commitProof ?? zeroProof(),
    ],
  }
}

/** Open a position from a shielded note. Requires NoteSpend + OrderCommitment proofs. */
export async function buildOpenPositionFromNoteTx(
  sourcePublicKey: string,
  opts: Parameters<typeof openPositionFromNoteCall>[0],
) {
  return buildBundleTx(sourcePublicKey, [openPositionFromNoteCall(opts)])
}

// ── Order APIs ────────────────────────────────────────────────────────────────

/** Pure op-builder: place an order in the orderbook. Called by the TEE relay, not the frontend directly. */
export function placeOrderCall(
  opts: {
    commitment: string
    portfolioKey?: string
    encryptedHints: string  // hex-encoded Bytes blob (TEE-generated, contains price/side/size/leverage)
    revealed?: number
    tif?: number
    expiryLedger?: number
    assetId?: string
    proof?: xdr.ScVal
  },
): ContractCall {
  const pk = bytes32ToScVal(opts.portfolioKey ?? '0'.repeat(64))
  const aid = bytes32ToScVal(opts.assetId ?? DEFAULT_ASSET)
  return {
    contractId: CONTRACT_IDS.orderbook,
    method: 'place_order',
    args: [
      bytes32ToScVal(opts.commitment),
      pk,
      xdr.ScVal.scvBytes(Buffer.from(opts.encryptedHints, 'hex')),
      u64ToScVal(opts.revealed ?? 15),
      tifToScVal(opts.tif ?? TIF.GTC),
      u64ToScVal(opts.expiryLedger ?? 0),
      aid,
      opts.proof ?? zeroProof(),
    ],
  }
}

/** Place an order in the orderbook. Requires an OrderCommitment proof. */
export async function buildPlaceOrderTx(
  sourcePublicKey: string,
  opts: Parameters<typeof placeOrderCall>[0],
) {
  return buildBundleTx(sourcePublicKey, [placeOrderCall(opts)])
}

/**
 * Bundle place_order + deposit_note into a single user-signed TX.
 * Used with the relay flow — the user signs only this TX, and the relayer
 * submits open_position_from_note separately (no user address in that TX).
 */
export async function buildDepositAndPlaceTx(
  sourcePublicKey: string,
  opts: {
    commitment: string
    encryptedHints: string  // TEE-generated encrypted hints blob (hex)
    portfolioKey?: string
    assetId?: string
    proof: xdr.ScVal
    noteCmt: string
    noteAmount: bigint
    noteAmountCommitment: string  // SHA256(amount_le || blinding)
  },
) {
  return buildBundleTx(sourcePublicKey, [
    placeOrderCall({
      commitment: opts.commitment,
      encryptedHints: opts.encryptedHints,
      portfolioKey: opts.portfolioKey,
      assetId: opts.assetId,
      proof: opts.proof,
    }),
    depositNoteCall(sourcePublicKey, opts.noteCmt, opts.noteAmount, opts.noteAmountCommitment),
  ])
}

/**
 * Bundle the full shielded trade into a single signed transaction:
 *   1. place_order      (orderbook) — registers the order commitment
 *   2. deposit_note     (perp-engine) — stores the shielded note collateral
 *   3. open_position    (perp-engine) — spends the note + opens the position
 * In the relay flow, prefer the separate deposit + relayOpenPosition path instead.
 */
export async function buildTradeBundleTx(
  sourcePublicKey: string,
  opts: {
    commitment: string
    encryptedHints: string          // TEE-generated encrypted hints blob (hex)
    portfolioKey?: string
    noteCmt: string
    noteNull: string
    noteAmount: bigint
    noteAmountCommitment: string    // SHA256(amount_le || collateralBlinding)
    collateralAmount: bigint
    collateralBlinding: string      // hex32
    settlementCommitment: string    // hex32
    sealedParams?: string           // TEE-generated encrypted params blob (hex)
    assetId?: string
    noteProof: xdr.ScVal
    commitProof: xdr.ScVal
  },
) {
  return buildBundleTx(sourcePublicKey, [
    placeOrderCall({
      commitment: opts.commitment,
      encryptedHints: opts.encryptedHints,
      portfolioKey: opts.portfolioKey,
      assetId: opts.assetId,
      proof: opts.commitProof,
    }),
    depositNoteCall(sourcePublicKey, opts.noteCmt, opts.noteAmount, opts.noteAmountCommitment),
    openPositionFromNoteCall({
      noteCmt: opts.noteCmt,
      noteNull: opts.noteNull,
      commitment: opts.commitment,
      sealedParams: opts.sealedParams,
      portfolioKey: opts.portfolioKey,
      assetId: opts.assetId,
      collateralAmount: opts.collateralAmount,
      collateralBlinding: opts.collateralBlinding,
      settlementCommitment: opts.settlementCommitment,
      noteProof: opts.noteProof,
      commitProof: opts.commitProof,
    }),
  ])
}

/** Cancel an order in the orderbook. Requires an OrderCancel proof. */
export async function buildCancelOrderTx(
  sourcePublicKey: string,
  commitment: string,
  nullifier: string,
  proof?: xdr.ScVal,
) {
  return buildTx(sourcePublicKey, CONTRACT_IDS.orderbook, 'cancel_order', [
    bytes32ToScVal(commitment),
    bytes32ToScVal(nullifier),
    proof ?? zeroProof(),
  ])
}

/** Pure op-builder: cancel a position on the perp engine and credit refund to a shielded note. TEE-only. */
export function cancelPositionToNoteCall(opts: {
  positionCmt: string
  cancelNullifier: string
  recipientNote: string
  refundAmount: bigint
  refundBlinding: string  // hex32 — blinding for the recipient note amount_commitment
  cancelProof: xdr.ScVal
}): ContractCall {
  return {
    contractId: CONTRACT_IDS.perpEngine,
    method: 'cancel_position_to_note',
    args: [
      bytes32ToScVal(opts.positionCmt),
      bytes32ToScVal(opts.cancelNullifier),
      bytes32ToScVal(opts.recipientNote),
      i128ToScVal(opts.refundAmount),
      bytes32ToScVal(opts.refundBlinding),
      opts.cancelProof,
    ],
  }
}

/** Cancel an open position; collateral refunds to a shielded note. TEE-only — use TEE relay endpoint instead. */
export async function buildCancelPositionTx(
  sourcePublicKey: string,
  opts: Parameters<typeof cancelPositionToNoteCall>[0],
) {
  return buildBundleTx(sourcePublicKey, [cancelPositionToNoteCall(opts)])
}

// ── List assets ──────────────────────────────────────────────────────────────

/** Get list of registered asset IDs from the perp engine. */
export async function listAssets(): Promise<string[][] | null> {
  try {
    const account = await rpc.getAccount('GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN')
    const contract = new Contract(CONTRACT_IDS.perpEngine)
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: NETWORK_PASSPHRASE,
    })
      .addOperation(contract.call('list_assets'))
      .setTimeout(300)
      .build()

    const result = await rpc.simulateTransaction(tx)
    if (!SorobanRpc.Api.isSimulationSuccess(result) || !result.result?.retval) return null
    return scValToNative(result.result.retval) as string[][]
  } catch {
    return null
  }
}

// ── USDC balance ─────────────────────────────────────────────────────────────

/** Query the USDC SAC balance for a Stellar address. Returns amount in stroop-scale (7 decimals). */
export async function getUsdcBalance(address: string): Promise<bigint | null> {
  try {
    const account = await rpc.getAccount('GAZ7LYN2ROIKRVKK4BIL5S4PVRED2YD6YNB4BA5LYB4TSQGN4BZKHTTP')
    const contract = new Contract(CONTRACT_IDS.collateralToken)
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: NETWORK_PASSPHRASE,
    })
      .addOperation(contract.call('balance', addressToScVal(address)))
      .setTimeout(300)
      .build()
    const result = await rpc.simulateTransaction(tx)
    if (!SorobanRpc.Api.isSimulationSuccess(result) || !result.result?.retval) return null
    return scValToNative(result.result.retval) as bigint
  } catch {
    return null
  }
}

// ── Submit ────────────────────────────────────────────────────────────────────

export async function submitAndWait(signedXdr: string): Promise<string> {
  const tx = TransactionBuilder.fromXDR(signedXdr, NETWORK_PASSPHRASE)
  const sendResult = await rpc.sendTransaction(tx)
  if (sendResult.status === 'ERROR') {
    throw new Error(`Submit failed: ${JSON.stringify(sendResult.errorResult)}`)
  }

  const hash = sendResult.hash
  for (let i = 0; i < 20; i++) {
    await new Promise((r) => setTimeout(r, 1500))
    const status = await rpc.getTransaction(hash)
    if (status.status === SorobanRpc.Api.GetTransactionStatus.SUCCESS) return hash
    if (status.status === SorobanRpc.Api.GetTransactionStatus.FAILED) {
      throw new Error('Transaction failed on-chain')
    }
  }
  throw new Error('Transaction timeout')
}

/** Mint testnet USDC to a recipient. Only the token issuer can mint. */
export async function buildMintUsdcTx(
  sourcePublicKey: string,
  recipientPublicKey: string,
  amount: bigint,
) {
  return buildTx(sourcePublicKey, CONTRACT_IDS.collateralToken, 'mint', [
    addressToScVal(recipientPublicKey),
    i128ToScVal(amount),
  ])
}

/** Establish trustline for the USDC SAC token. Required before minting or receiving USDC. */
export async function buildTrustUsdcTx(sourcePublicKey: string) {
  return buildTx(sourcePublicKey, CONTRACT_IDS.collateralToken, 'trust', [
    addressToScVal(sourcePublicKey),
  ])
}

/** Mint USDC using the issuer key (from VITE_MINTER_SECRET env). Signs and submits directly. */
export async function mintUsdcFromIssuer(recipient: string, amount: bigint) {
  const secret = import.meta.env.VITE_MINTER_SECRET as string
  if (!secret) throw new Error('VITE_MINTER_SECRET not set')

  const issuer = Keypair.fromSecret(secret)
  const account = await rpc.getAccount(issuer.publicKey())
  const contract = new Contract(CONTRACT_IDS.collateralToken)

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(contract.call('mint', addressToScVal(recipient), i128ToScVal(amount)))
    .setTimeout(300)
    .build()

  const simResult = await rpc.simulateTransaction(tx)
  if (SorobanRpc.Api.isSimulationError(simResult)) {
    throw new Error(`Mint simulation failed: ${simResult.error}`)
  }

  const assembled = SorobanRpc.assembleTransaction(tx, simResult).build()
  assembled.sign(issuer)

  const sendResult = await rpc.sendTransaction(assembled)
  if (sendResult.status === 'ERROR') {
    throw new Error(`Mint submit failed: ${JSON.stringify(sendResult.errorResult)}`)
  }

  for (let i = 0; i < 20; i++) {
    await new Promise((r) => setTimeout(r, 1500))
    const status = await rpc.getTransaction(sendResult.hash)
    if (status.status === SorobanRpc.Api.GetTransactionStatus.SUCCESS) return status
    if (status.status === SorobanRpc.Api.GetTransactionStatus.FAILED) {
      throw new Error('Mint transaction failed on-chain')
    }
  }
  throw new Error('Mint transaction timeout')
}
