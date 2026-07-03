import {
  Contract,
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

function bytes32ToScVal(hex: string): xdr.ScVal {
  const buf = Buffer.from(hex, 'hex')
  return xdr.ScVal.scvBytes(buf)
}

// TimeInForce enum values matching the contract
export const TIF = { GTC: 0, IOC: 1, FOK: 2, GTD: 3 } as const

export const DEFAULT_ASSET = '0000000000000000000000000000000000000000000000000000000000000000'

// ── transaction builder ───────────────────────────────────────────────────────

export async function buildTx(
  sourcePublicKey: string,
  contractId: string,
  method: string,
  args: xdr.ScVal[],
) {
  const account = await rpc.getAccount(sourcePublicKey)
  const contract = new Contract(contractId)

  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: NETWORK_PASSPHRASE,
  })
    .addOperation(contract.call(method, ...args))
    .setTimeout(30)
    .build()

  const simResult = await rpc.simulateTransaction(tx)
  if (SorobanRpc.Api.isSimulationError(simResult)) {
    throw new Error(`Simulation failed: ${simResult.error}`)
  }

  return SorobanRpc.assembleTransaction(tx, simResult).build()
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

// Soroban contracttype unit enum variants encode as Map { VariantName: Void }
function enumToScVal(variant: string): xdr.ScVal {
  return xdr.ScVal.scvMap([
    new xdr.ScMapEntry({
      key: xdr.ScVal.scvSymbol(variant),
      val: xdr.ScVal.scvVoid(),
    }),
  ])
}

// Groth16Proof: placeholder zero proof (will fail on-chain until WASM prover is wired)
function zeroProof(): xdr.ScVal {
  return xdr.ScVal.scvMap([
    new xdr.ScMapEntry({ key: xdr.ScVal.scvSymbol('a'), val: xdr.ScVal.scvBytes(Buffer.alloc(64)) }),
    new xdr.ScMapEntry({ key: xdr.ScVal.scvSymbol('b'), val: xdr.ScVal.scvBytes(Buffer.alloc(128)) }),
    new xdr.ScMapEntry({ key: xdr.ScVal.scvSymbol('c'), val: xdr.ScVal.scvBytes(Buffer.alloc(64)) }),
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

export interface PositionMeta {
  collateral: bigint
  entryPrice: bigint
  matchedPrice: bigint
  side: bigint
  leverage: bigint
  status: bigint
  createdAt: bigint
  matchId: bigint
  fundingAtOpen: bigint
  hintSize: bigint
  tpPrice: bigint
  slPrice: bigint
  effectiveCollateral: bigint
  partialLiqDone: boolean
  marginMode: bigint
  assetId: string
}

export async function getPosition(commitment: string): Promise<PositionMeta | null> {
  try {
    const account = await rpc.getAccount('GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN')
    const contract = new Contract(CONTRACT_IDS.perpEngine)
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: NETWORK_PASSPHRASE,
    })
      .addOperation(contract.call('get_position', bytes32ToScVal(commitment)))
      .setTimeout(30)
      .build()

    const result = await rpc.simulateTransaction(tx)
    if (!SorobanRpc.Api.isSimulationSuccess(result) || !result.result?.retval) return null

    const raw = scValToNative(result.result.retval)
    if (!raw) return null

    return {
      collateral: BigInt(raw.collateral),
      entryPrice: BigInt(raw.entry_price),
      matchedPrice: BigInt(raw.matched_price),
      side: BigInt(raw.side),
      leverage: BigInt(raw.leverage),
      status: BigInt(raw.status),
      createdAt: BigInt(raw.created_at),
      matchId: BigInt(raw.match_id),
      fundingAtOpen: BigInt(raw.funding_at_open),
      hintSize: BigInt(raw.hint_size),
      tpPrice: BigInt(raw.tp_price),
      slPrice: BigInt(raw.sl_price),
      effectiveCollateral: BigInt(raw.effective_collateral),
      partialLiqDone: raw.partial_liq_done,
      marginMode: BigInt(raw.margin_mode),
      assetId: raw.asset_id ?? '0000000000000000000000000000000000000000000000000000000000000000',
    }
  } catch {
    return null
  }
}

// ── Note APIs ─────────────────────────────────────────────────────────────────

/** Deposit collateral as a shielded note. No ZK proof needed — only when spending. */
export async function buildDepositNoteTx(
  sourcePublicKey: string,
  noteCommitment: string,
  amount: bigint,
) {
  return buildTx(sourcePublicKey, CONTRACT_IDS.perpEngine, 'deposit_note', [
    addressToScVal(sourcePublicKey),
    bytes32ToScVal(noteCommitment),
    i128ToScVal(amount),
  ])
}

/** Withdraw from a shielded note to a recipient. Requires a valid NoteSpend proof. */
export async function buildWithdrawNoteTx(
  sourcePublicKey: string,
  noteCmt: string,
  noteNull: string,
  recipientPk: string,
  noteProof?: xdr.ScVal,
) {
  return buildTx(sourcePublicKey, CONTRACT_IDS.perpEngine, 'withdraw_note', [
    bytes32ToScVal(noteCmt),
    bytes32ToScVal(noteNull),
    addressToScVal(recipientPk),
    noteProof ?? zeroProof(),
  ])
}

/** Query a note balance by commitment. Returns amount in stroops or null. */
export async function getNoteBalance(noteCommitment: string): Promise<bigint | null> {
  try {
    const account = await rpc.getAccount('GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN')
    const contract = new Contract(CONTRACT_IDS.perpEngine)
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: NETWORK_PASSPHRASE,
    })
      .addOperation(contract.call('get_note', bytes32ToScVal(noteCommitment)))
      .setTimeout(30)
      .build()

    const result = await rpc.simulateTransaction(tx)
    if (!SorobanRpc.Api.isSimulationSuccess(result) || !result.result?.retval) return null
    return scValToNative(result.result.retval) as bigint
  } catch {
    return null
  }
}

// ── Position APIs ─────────────────────────────────────────────────────────────

/** Open a position from a shielded note. Requires NoteSpend + OrderCommitment proofs. */
export async function buildOpenPositionFromNoteTx(
  sourcePublicKey: string,
  opts: {
    noteCmt: string
    noteNull: string
    commitment: string
    hintPrice: number
    side: 0 | 1
    leverage: number
    size: number
    tpPrice?: number
    slPrice?: number
    assetId?: string
    portfolioKey?: string
    noteProof?: xdr.ScVal
    commitProof?: xdr.ScVal
  },
) {
  const zeroNote = bytes32ToScVal('0'.repeat(64))
  const pk = bytes32ToScVal(opts.portfolioKey ?? '0'.repeat(64))
  const aid = bytes32ToScVal(opts.assetId ?? DEFAULT_ASSET)

  return buildTx(sourcePublicKey, CONTRACT_IDS.perpEngine, 'open_position_from_note', [
    bytes32ToScVal(opts.noteCmt),
    bytes32ToScVal(opts.noteNull),
    bytes32ToScVal(opts.commitment),
    u64ToScVal(opts.hintPrice),
    u64ToScVal(opts.side),
    u64ToScVal(opts.leverage),
    u64ToScVal(opts.size),
    enumToScVal('GTC'),
    u64ToScVal(0),
    u64ToScVal(opts.tpPrice ?? 0),
    u64ToScVal(opts.slPrice ?? 0),
    zeroNote,
    pk,
    aid,
    opts.noteProof ?? zeroProof(),
    opts.commitProof ?? zeroProof(),
  ])
}

// ── Order APIs ────────────────────────────────────────────────────────────────

/** Place an order in the orderbook. Requires an OrderCommitment proof. */
export async function buildPlaceOrderTx(
  sourcePublicKey: string,
  opts: {
    commitment: string
    hintPrice: number
    hintSide: number
    hintSize: number
    hintLeverage: number
    revealed?: number
    tif?: number
    expiryLedger?: number
    assetId?: string
    portfolioKey?: string
    proof?: xdr.ScVal
  },
) {
  const pk = bytes32ToScVal(opts.portfolioKey ?? '0'.repeat(64))
  const aid = bytes32ToScVal(opts.assetId ?? DEFAULT_ASSET)
  return buildTx(sourcePublicKey, CONTRACT_IDS.orderbook, 'place_order', [
    bytes32ToScVal(opts.commitment),
    pk,
    u64ToScVal(opts.hintPrice),
    u64ToScVal(opts.hintSide),
    u64ToScVal(opts.hintSize),
    u64ToScVal(opts.hintLeverage),
    u64ToScVal(opts.revealed ?? 15),
    enumToScVal('GTC'),
    u64ToScVal(opts.expiryLedger ?? 0),
    aid,
    opts.proof ?? zeroProof(),
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
      .setTimeout(30)
      .build()

    const result = await rpc.simulateTransaction(tx)
    if (!SorobanRpc.Api.isSimulationSuccess(result) || !result.result?.retval) return null
    return scValToNative(result.result.retval) as string[][]
  } catch {
    return null
  }
}

// ── Submit ────────────────────────────────────────────────────────────────────

export async function submitAndWait(signedXdr: string) {
  const tx = TransactionBuilder.fromXDR(signedXdr, NETWORK_PASSPHRASE)
  const sendResult = await rpc.sendTransaction(tx)
  if (sendResult.status === 'ERROR') {
    throw new Error(`Submit failed: ${JSON.stringify(sendResult.errorResult)}`)
  }

  const hash = sendResult.hash
  for (let i = 0; i < 20; i++) {
    await new Promise((r) => setTimeout(r, 1500))
    const status = await rpc.getTransaction(hash)
    if (status.status === SorobanRpc.Api.GetTransactionStatus.SUCCESS) return status
    if (status.status === SorobanRpc.Api.GetTransactionStatus.FAILED) {
      throw new Error('Transaction failed on-chain')
    }
  }
  throw new Error('Transaction timeout')
}
