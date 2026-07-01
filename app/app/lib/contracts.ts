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

// ── contract calls ────────────────────────────────────────────────────────────

export async function getBalance(userAddress: string): Promise<bigint> {
  try {
    const account = await rpc.getAccount(userAddress)
    const contract = new Contract(CONTRACT_IDS.perpEngine)
    const tx = new TransactionBuilder(account, {
      fee: BASE_FEE,
      networkPassphrase: NETWORK_PASSPHRASE,
    })
      .addOperation(contract.call('get_balance', addressToScVal(userAddress)))
      .setTimeout(30)
      .build()

    const result = await rpc.simulateTransaction(tx)
    if (SorobanRpc.Api.isSimulationSuccess(result) && result.result?.retval) {
      return scValToNative(result.result.retval) as bigint
    }
  } catch {
    // account may not exist on testnet yet
  }
  return 0n
}

export async function buildDepositTx(sourcePublicKey: string, amount: bigint) {
  return buildTx(sourcePublicKey, CONTRACT_IDS.perpEngine, 'deposit', [
    addressToScVal(sourcePublicKey),
    i128ToScVal(amount),
  ])
}

/** Generate a random 32-byte hex commitment for a new position */
export function randomCommitment(): string {
  const bytes = new Uint8Array(32)
  crypto.getRandomValues(bytes)
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}

export async function buildOpenPositionTx(
  sourcePublicKey: string,
  opts: {
    commitment: string // 32-byte hex
    hintPrice: number  // oracle price as integer (e.g. 63347_0000000 for $63347 with 7 decimals)
    side: 0 | 1        // 0=long, 1=short
    leverage: number
    size: number       // in base units
    collateral: bigint
    tpPrice?: number
    slPrice?: number
  },
) {
  const zeroNote = bytes32ToScVal('0'.repeat(64))
  const tifScVal = nativeToScVal(
    xdr.ScVal.scvVec([
      xdr.ScVal.scvSymbol('GTC'),
    ]),
  )

  // TimeInForce as enum: { GTC: {} }
  const tifVal = xdr.ScVal.scvVec([xdr.ScVal.scvSymbol('GTC')])

  return buildTx(sourcePublicKey, CONTRACT_IDS.perpEngine, 'open_position', [
    addressToScVal(sourcePublicKey),           // owner
    bytes32ToScVal(opts.commitment),           // position_commitment
    u64ToScVal(opts.hintPrice),               // hint_price
    u64ToScVal(opts.side),                    // hint_side
    u64ToScVal(opts.leverage),                // hint_leverage
    u64ToScVal(opts.size),                    // hint_size
    tifVal,                                    // tif: GTC
    u64ToScVal(0),                            // expiry_ledger
    u64ToScVal(opts.tpPrice ?? 0),            // tp_price
    u64ToScVal(opts.slPrice ?? 0),            // sl_price
    zeroNote,                                  // liquidation_recipient_note (public path)
    i128ToScVal(opts.collateral),             // collateral
  ])
}

/** Submit a signed XDR transaction and wait for confirmation */
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
