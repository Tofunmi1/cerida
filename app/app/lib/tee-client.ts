// ── TEE Server Client ─────────────────────────────────────────────
// Connects to the TEE match server via HTTP for order commitment
// generation and proof creation.

// In production (HTTPS) use the Vercel edge rewrite at /tee to avoid mixed-content.
// In local dev the vite proxy rewrites /tee → http://127.0.0.1:9721 too.
const TEE_URL = import.meta.env.VITE_TEE_URL ?? '/tee'

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
  tx_hash?: string
  error?: string
  best_bid?: string
  best_ask?: string
  spread?: number
  order_count?: number
  volume_24h?: number
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
  const resp = await fetch(`${TEE_URL}/${endpoint}`, { cache: 'no-store' })
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
    protocol?: boolean
    asset_id_hex?: string
    collateral_amount?: number
    is_close?: boolean
    close_position_cmt?: string
    tp_price?: number
    sl_price?: number
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
      protocol: params.protocol ?? false,
      asset_id_hex: params.asset_id_hex,
      collateral_amount: params.collateral_amount ?? 0,
      is_close: params.is_close ?? false,
      close_position_cmt: params.close_position_cmt,
      tp_price: params.tp_price ?? 0,
      sl_price: params.sl_price ?? 0,
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
    protocol?: boolean
    asset_id_hex?: string
    collateral_amount?: number
    is_close?: boolean
    close_position_cmt?: string
    tp_price?: number
    sl_price?: number
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
      protocol: params.protocol ?? false,
      asset_id_hex: params.asset_id_hex,
      collateral_amount: params.collateral_amount ?? 0,
      is_close: params.is_close ?? false,
      close_position_cmt: params.close_position_cmt,
      tp_price: params.tp_price ?? 0,
      sl_price: params.sl_price ?? 0,
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

  /**
   * Relay place_order + open_position_from_note via the TEE's own key.
   * User only signs deposit_note. TEE handles both orderbook placement and
   * position opening — user address never appears in those TXs.
   *
   * collateral_amount and collateral_blinding are TEE-internal — not sent to contract
   * settlement_commitment: hex32 pre-committed note for settlement fund destination
   */
  /**
   * Relay a full cancel + withdraw for a position.
   * TEE handles: cancel_position_to_note → ZK note proof → withdraw_note → tokens back to recipient.
   * This is the only way to close a position since cancel_position_to_note requires TEE auth.
   */
  async relayDepositNote(signedXdr: string): Promise<{ ok: boolean; queued: boolean }> {
    const resp = await fetch(`${TEE_URL}/relay/deposit-note`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ signed_xdr: signedXdr }),
      cache: 'no-store',
    })
    return resp.json()
  },

  async relayCancel(params: {
    perp: string
    position_cmt: string
    cancel_nullifier: string
    cancel_proof: string   // JSON proof string
    recipient: string      // Stellar address to receive refunded tokens
  }): Promise<{ tx_hash: string }> {
    const resp = await fetch(`${TEE_URL}/relay/cancel-position`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(params),
    })
    const data = await resp.json() as TeeResponse
    console.log('[tee] relayCancel response:', data)
    if (!data.ok) throw new Error(data.error ?? 'cancel relay failed')
    return { tx_hash: data.tx_hash! }
  },

  async relayLimitOrder(params: {
    perp: string
    orderbook: string
    note_cmt: string
    note_null: string
    position_cmt: string
    sealed_params?: string
    collateral_amount: number
    collateral_blinding: string
    settlement_commitment: string
    portfolio_key?: string
    asset_id?: string
    note_proof: string
    commit_proof: string
  }): Promise<{ queued: boolean; filled: boolean }> {
    const resp = await fetch(`${TEE_URL}/relay/place-limit`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(params),
    })
    const data = await resp.json() as TeeResponse & { queued?: boolean; filled?: boolean }
    if (!data.ok) throw new Error(data.error ?? 'place-limit failed')
    return { queued: !!data.queued, filled: !!data.filled }
  },

  async relayOpenPosition(params: {
    perp: string
    orderbook: string
    note_cmt: string
    note_null: string
    position_cmt: string
    sealed_params?: string
    collateral_amount: number       // TEE stores this; not forwarded to contract
    collateral_blinding: string     // hex32 — TEE stores this; not forwarded to contract
    settlement_commitment: string   // hex32
    portfolio_key?: string
    asset_id?: string
    note_proof: string
    commit_proof: string
  }): Promise<{ queued: boolean; tx_hash?: string }> {
    const resp = await fetch(`${TEE_URL}/relay/open-position`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(params),
    })
    const data = await resp.json() as TeeResponse & { queued?: boolean }
    if (!data.ok) throw new Error(data.error ?? 'relay failed')
    return { queued: !!data.queued, tx_hash: data.tx_hash }
  },

  async pollPositionTx(cmt: string, timeoutMs = 30000): Promise<string | null> {
    const deadline = Date.now() + timeoutMs
    while (Date.now() < deadline) {
      await new Promise(r => setTimeout(r, 2000))
      try {
        const resp = await fetch(`${TEE_URL}/relay/position-tx?cmt=${encodeURIComponent(cmt)}`, { cache: 'no-store' })
        const data = await resp.json() as { ok: boolean; tx_hash?: string | null }
        if (data.ok && data.tx_hash) return data.tx_hash
      } catch { /* network hiccup, keep polling */ }
    }
    return null
  },

  /** Place a closing order against the CLOB and settle the original position. */
  async relayClosePosition(params: {
    perp: string
    close_cmt: string
    position_cmt: string
    position_secret: number
    settlement_commitment: string
  }): Promise<{ filled: boolean }> {
    const resp = await fetch(`${TEE_URL}/relay/close-position`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(params),
    })
    const data = await resp.json() as TeeResponse & { filled?: boolean }
    if (!data.ok) throw new Error(data.error ?? 'close-position relay failed')
    return { filled: !!data.filled }
  },

  /** Claim funds from a settled/liquidated position. */
  async relayWithdrawSettlement(params: {
    perp: string
    position_cmt: string
    recipient: string
  }): Promise<{ tx_hash: string }> {
    const resp = await fetch(`${TEE_URL}/relay/withdraw-settlement`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(params),
    })
    const data = await resp.json() as TeeResponse
    if (!data.ok) throw new Error(data.error ?? 'withdraw settlement failed')
    return { tx_hash: data.tx_hash! }
  },
}
