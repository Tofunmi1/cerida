// ── TEE Server Client ─────────────────────────────────────────────
// Connects to the TEE match server via HTTP for order commitment
// generation and proof creation.

const TEE_URL = import.meta.env.VITE_TEE_URL ?? 'http://127.0.0.1:9721'

interface TeeResponse {
  ok: boolean
  commitment?: string
  proof?: string
  error?: string
  best_bid?: string
  best_ask?: string
  spread?: number
  order_count?: number
  fills?: Array<{ maker_id: string; price: number; size: number }>
}

async function call(endpoint: string, body?: unknown): Promise<TeeResponse> {
  const resp = await fetch(`${TEE_URL}/${endpoint}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: body ? JSON.stringify(body) : undefined,
  })
  return resp.json()
}

async function getCall(endpoint: string): Promise<TeeResponse> {
  const resp = await fetch(`${TEE_URL}/${endpoint}`)
  return resp.json()
}

export const tee = {
  /** Generate an order commitment (slow, ~9s ZK proof). */
  async init(params: {
    side: number
    price: number
    size: number
    leverage: number
    nonce: number
    secret: number
    asset?: number
  }): Promise<{ commitment: string }> {
    const resp = await call('init', {
      cmd: 'init',
      side: params.side,
      price: params.price,
      size: params.size,
      leverage: params.leverage,
      nonce: params.nonce,
      secret: params.secret,
      asset: params.asset ?? 0,
    })
    if (!resp.ok || !resp.commitment) throw new Error(resp.error ?? 'init failed')
    return { commitment: resp.commitment }
  },

  /** Fast init — commitment hash only, no proof (sub-ms). */
  async fastInit(params: {
    side: number
    price: number
    size: number
    leverage: number
    nonce: number
    secret: number
    asset?: number
  }): Promise<{ commitment: string }> {
    const resp = await call('fast-init', {
      cmd: 'fast-init',
      side: params.side,
      price: params.price,
      size: params.size,
      leverage: params.leverage,
      nonce: params.nonce,
      secret: params.secret,
      asset: params.asset ?? 0,
    })
    if (!resp.ok || !resp.commitment) throw new Error(resp.error ?? 'fast-init failed')
    return { commitment: resp.commitment }
  },

  /** Get a Groth16 commitment proof for on-chain submission. */
  async commitProof(cmt: string): Promise<{ proof: string }> {
    const resp = await call('commit-proof', { cmd: 'commit-proof', cmt })
    if (!resp.ok || !resp.proof) throw new Error(resp.error ?? 'commit-proof failed')
    return { proof: resp.proof }
  },

  /** Place an order on the CLOB (no on-chain submission). */
  async place(cmt: string, orderType: string, price: number, size: number): Promise<TeeResponse> {
    return call('place', { cmd: 'place', cmt, order_type: orderType, price, size })
  },

  /** Get market state. */
  async getMarket(): Promise<TeeResponse> {
    return getCall('get-market')
  },
}
