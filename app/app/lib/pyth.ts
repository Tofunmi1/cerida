// ── Pyth Network utilities ─────────────────────────────────────────
// Benchmarks API: historical OHLCV candles from the Pyth oracle feed.
// Hermes WebSocket: real-time price ticks (sub-second).
// ──────────────────────────────────────────────────────────────────

export interface Candle {
  time: number   // unix seconds
  open: number
  high: number
  low: number
  close: number
  volume: number
}

export interface PriceTick {
  pythId: string
  price: number
  publishTime: number
}

// ── Pyth Benchmarks symbol names ───────────────────────────────────
// Maps frontend market symbols to Pyth Benchmarks TradingView UDF symbols.
export const BENCH_SYMBOL: Record<string, string> = {
  'BTC-PERP':    'Crypto.BTC/USD',
  'XRP-PERP':    'Crypto.XRP/USD',
  'XLM-PERP':    'Crypto.XLM/USD',
  'GOLD-PERP':   'Metal.XAU/USD',
  'OIL-PERP':    'Energy.WTI/USD',
  'TSLA-PERP':   'Equity.US.TSLA/USD',
  // SPACEX-PERP: private company, no Pyth feed — seeded fallback
}

const BENCHMARKS = 'https://benchmarks.pyth.network/v1/shims/tradingview/history'
const WS_URL     = 'wss://hermes.pyth.network/ws'

// ── Historical candles (Pyth Benchmarks TradingView UDF) ───────────
export async function fetchCandles(
  marketSymbol: string,
  resolution = '1',
  bars = 1440,
): Promise<Candle[]> {
  const benchSym = BENCH_SYMBOL[marketSymbol]
  if (!benchSym) return []

  const to   = Math.floor(Date.now() / 1000)
  const from = to - bars * 60  // 1-min bars × count

  const url = `${BENCHMARKS}?symbol=${encodeURIComponent(benchSym)}&resolution=${resolution}&from=${from}&to=${to}`

  try {
    const resp = await fetch(url, { signal: AbortSignal.timeout(12_000) })
    if (!resp.ok) return []
    const d = await resp.json() as {
      s: string
      t?: number[]
      o?: number[]
      h?: number[]
      l?: number[]
      c?: number[]
      v?: number[]
    }
    if (d.s !== 'ok' || !d.t?.length) return []

    return d.t.map((time, i) => ({
      time,
      open:   d.o![i] ?? 0,
      high:   d.h![i] ?? 0,
      low:    d.l![i] ?? 0,
      close:  d.c![i] ?? 0,
      volume: d.v![i] ?? 0,
    }))
  } catch {
    return []
  }
}

// ── Pyth Hermes REST — latest price for a single feed ID ──────────
const HERMES_REST = 'https://hermes.pyth.network/v2/updates/price/latest'

async function fetchOnePrice(pythId: string): Promise<number> {
  try {
    const resp = await fetch(`${HERMES_REST}?ids[]=${pythId}&parsed=true`, {
      signal: AbortSignal.timeout(8_000),
    })
    if (!resp.ok) return 0
    const json = await resp.json() as {
      parsed?: Array<{ id: string; price: { price: string; expo: number } }>
    }
    const feed = json.parsed?.[0]
    if (!feed) return 0
    const price = Number(feed.price.price) * Math.pow(10, feed.price.expo)
    return price > 0 ? price : 0
  } catch {
    return 0
  }
}

export async function fetchLatestPrices(
  pythIds: string[],
): Promise<Map<string, number>> {
  const out = new Map<string, number>()
  if (!pythIds.length) return out
  const results = await Promise.allSettled(pythIds.map(fetchOnePrice))
  results.forEach((r, i) => {
    if (r.status === 'fulfilled' && r.value > 0) out.set(pythIds[i], r.value)
  })
  return out
}

// ── Pyth Hermes WebSocket ──────────────────────────────────────────
// Subscribes to real-time price updates for a list of Pyth feed IDs.
// Handles reconnect automatically.
export function connectPythWs(
  pythIds: string[],
  onTick: (tick: PriceTick) => void,
  onConnect?: () => void,
): () => void {
  let ws: WebSocket | null = null
  let dead = false
  let retryMs = 2000

  function connect() {
    if (dead) return
    ws = new WebSocket(WS_URL)

    ws.onopen = () => {
      retryMs = 2000
      ws!.send(JSON.stringify({ type: 'subscribe', ids: pythIds }))
      onConnect?.()
    }

    ws.onmessage = (ev) => {
      try {
        const msg = JSON.parse(ev.data as string)
        if (msg.type !== 'price_update' || !msg.price_feed) return
        const feed = msg.price_feed
        const raw  = Number(feed.price.price)
        const price = raw * Math.pow(10, feed.price.expo as number)
        if (price > 0) {
          onTick({ pythId: feed.id as string, price, publishTime: feed.price.publish_time as number })
        }
      } catch {}
    }

    ws.onerror = () => {}

    ws.onclose = () => {
      if (dead) return
      retryMs = Math.min(retryMs * 2, 30_000)
      setTimeout(connect, retryMs)
    }
  }

  connect()

  return () => {
    dead = true
    ws?.close()
  }
}
