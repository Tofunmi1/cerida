// ── TEE Server Client ─────────────────────────────────────────────
// Connects to the TEE match server via HTTP for order commitment
// generation and proof creation.

const TEE_URL = import.meta.env.VITE_TEE_URL ?? 'http://127.0.0.1:9721'

export interface OrderBookLevel {
  price: number
  size: number
  orders: number
}

interface TeeResponse {
  ok: boolean
  commitment?: string
  nullifier?: string
  note_cmt?: string
  note_null?: string
  proof?: string
  error?: string
  best_bid?: string
  best_ask?: string
  spread?: number
  order_count?: number
  fills?: Array<{ maker_id: string; price: number; size: number }>
  bids?: OrderBookLevel[]
  asks?: OrderBookLevel[]
  depth?: OrderBookLevel[]
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

  /** Generate a cancel/close proof for a position commitment. Returns proof + nullifier. */
  async cancelProof(cmt: string): Promise<{ proof: string; nullifier: string }> {
    const resp = await call('cancel-proof', { cmd: 'cancel-proof', cmt })
    if (!resp.ok || !resp.proof || !resp.commitment) {
      throw new Error(resp.error ?? 'cancel-proof failed')
    }
    return { proof: resp.proof, nullifier: resp.commitment }
  },

  /**
   * Generate a NoteSpend Groth16 proof for a shielded deposit note (~9s).
   * Returns note_cmt, note_null, and proof JSON — all three needed for open_position_from_note.
   */
  async noteProof(amount: number, secret: number): Promise<{ note_cmt: string; note_null: string; proof: string }> {
    const resp = await call('note-proof', { cmd: 'note-proof', amount, secret })
    if (!resp.ok || !resp.note_cmt || !resp.note_null || !resp.proof) throw new Error(resp.error ?? 'note-proof failed')
    return { note_cmt: resp.note_cmt, note_null: resp.note_null, proof: resp.proof }
  },

  /**
   * Fast note commitment hash — Poseidon2 only, no proof (~1ms).
   * Use this during deposit to get the commitment without a ZK proof.
   */
  async noteCmt(amount: number, secret: number): Promise<{ note_cmt: string; note_null: string }> {
    const resp = await call('note-cmt', { cmd: 'note-cmt', amount, secret })
    if (!resp.ok || !resp.note_cmt || !resp.note_null) throw new Error(resp.error ?? 'note-cmt failed')
    return { note_cmt: resp.note_cmt, note_null: resp.note_null }
  },

  /** Place an order on the CLOB (no on-chain submission). */
  async place(cmt: string, orderType: string, price: number, size: number): Promise<TeeResponse> {
    return call('place', { cmd: 'place', cmt, order_type: orderType, price, size })
  },

  /** Get market state (32-level depth). Asset 0 = BTC (DEFAULT_ASSET). */
  async getMarket(asset?: number): Promise<TeeResponse> {
    const qs = asset !== undefined ? `?asset=${asset}` : ''
    return getCall(`get-market${qs}`)
  },
}
