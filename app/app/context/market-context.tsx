import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react'
import { tee, type OrderBookLevel } from '../lib/tee-client'
import { fetchCandles, fetchLatestPrices, connectPythWs, type Candle as PythCandle } from '../lib/pyth'

export type Side = 'long' | 'short'

export interface Candle {
  time: number
  open: number
  high: number
  low: number
  close: number
  volume: number
}

export interface MarketState {
  symbol: string
  mark: number
  index: number
  changePct: number
  funding: number
  openInterest: number | null   // null = unknown / not indexed yet
  volume24h: number | null
  candles: Candle[]
  candlesLoading: boolean
  bids: OrderBookLevel[]
  asks: OrderBookLevel[]
}

interface MarketContextValue extends MarketState {
  setSymbol: (symbol: string) => void
  allPrices: Map<string, number>      // pythId → USD price for all markets
  symbolPrices: Map<string, number>   // symbol → USD price (e.g. 'BTC-PERP' → 62378)
}

const MarketContext = createContext<MarketContextValue | undefined>(undefined)

export interface MarketDefinition {
  symbol: string
  name: string
  category: 'Crypto' | 'RWA'
  basePrice: number
  icon: string
  color: string
  logo?: string   // path to SVG/PNG in /public/logos/
  assetId: number
  pythId: string
}

export const MARKET_CATALOG: MarketDefinition[] = [
  {
    symbol: 'BTC-PERP', name: 'Bitcoin', category: 'Crypto',
    basePrice: 61628.2, icon: '₿', color: '#f7931a',
    logo: '/logos/XTVCBTC--big.svg',
    assetId: 0,
    pythId: 'e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43',
  },
  {
    symbol: 'XRP-PERP', name: 'XRP', category: 'Crypto',
    basePrice: 0.52, icon: 'XRP', color: '#346aa9',
    assetId: 1,
    pythId: 'ec5d399846a9209f3fe5881d70aae9268c94339ff9a0ae1c6aebcb7f40e78acd',
  },
  {
    symbol: 'XLM-PERP', name: 'Stellar', category: 'Crypto',
    basePrice: 0.11, icon: 'XLM', color: '#7b1fa2',
    assetId: 2,
    pythId: 'b7a8eba68a997cd0210c2e1e4ee811ad2d174b3611c22d9ebf16f4cb7e9ba850',
  },
  {
    symbol: 'SPACEX-PERP', name: 'SpaceX', category: 'RWA',
    basePrice: 350.0, icon: 'SpX', color: '#111827',
    logo: '/logos/spacex--big.svg',
    assetId: 3,
    pythId: '',
  },
  {
    symbol: 'TSLA-PERP', name: 'Tesla', category: 'RWA',
    basePrice: 393.4, icon: 'TSLA', color: '#e82127',
    logo: '/logos/XTVCTESLAI--big.svg',
    assetId: 4,
    pythId: '16dad506d7db8da01c87581c87ca897a012a153557d4d578c3b9c9e1bc0632f1',
  },
  {
    symbol: 'OIL-PERP', name: 'Crude Oil', category: 'RWA',
    basePrice: 70.0, icon: 'OIL', color: '#0f766e',
    logo: '/logos/crude-oil--big.svg',
    assetId: 5,
    pythId: 'fe650f0367d4a7ef9815a593ea15d36593f0643aaaf0149bb04be67ab851decd',
  },
  {
    symbol: 'GOLD-PERP', name: 'Gold', category: 'RWA',
    basePrice: 4179.5, icon: 'Au', color: '#d4a017',
    logo: '/logos/gold--big.svg',
    assetId: 6,
    pythId: '765d2ba906dbc32ca17cc11f5310a89e9ee1f6420508c63861f2f8ba4ee34bb2',
  },
]

const BOOK_POLL_MS = 3000

export function symbolToSlug(symbol: string): string {
  return symbol.replace('-PERP', '').toLowerCase()
}

export function slugToSymbol(slug: string): string {
  const upper = slug.toUpperCase()
  const match = MARKET_CATALOG.find((m) => m.symbol === `${upper}-PERP`)
  return match ? match.symbol : 'BTC-PERP'
}

// ── Seeded fallback candles ────────────────────────────────────────
// Used only for SPACEX (no Pyth feed) or if Benchmarks fetch fails.

function seededNoise(seed: number) {
  const x = Math.sin(seed * 12.9898) * 43758.5453
  return x - Math.floor(x)
}
function seededNormal(seed: number) {
  const u1 = Math.max(seededNoise(seed), 0.000001)
  const u2 = seededNoise(seed + 31.4159)
  return Math.sqrt(-2 * Math.log(u1)) * Math.cos(2 * Math.PI * u2)
}

function makeSeededCandles(base: number): Candle[] {
  const now = Math.floor(Date.now() / 1000)
  const out: Candle[] = []
  let price = base
  let state = 0
  for (let i = 1439; i >= 0; i--) {
    const t = 1440 - i
    const open = price
    state = state * 0.982 + seededNormal(t + Math.round(base)) * 0.00052
    state = Math.max(-0.028, Math.min(0.028, state))
    const close = base * (1 + state)
    const body = Math.abs(close - open)
    const wickNoise = seededNoise(t * 8.91 + Math.round(base / 11))
    const wick = base * (0.00008 + wickNoise * 0.00012) + body * 0.18
    out.push({
      time: now - i * 60,
      open,
      high: Math.max(open, close) + wick,
      low:  Math.min(open, close) - wick * (0.75 + wickNoise * 0.3),
      close,
      volume: 45 + Math.round(wickNoise * 64) + Math.round((body / base) * 26000),
    })
    price = close
  }
  return out
}

// ── Funding rate ──────────────────────────────────────────────────
// Slowly oscillating ~0.01–0.03% per 8h, plus premium component.
function calcFunding(mark: number, index: number): number {
  const t = Date.now() / 1_000_000
  const base = 0.0001 + Math.sin(t * 3.7) * 0.00008 + Math.cos(t * 1.9) * 0.00005
  const premium = index > 0 ? ((mark - index) / index) * 0.2 : 0
  return Math.max(-0.001, Math.min(0.001, base + premium))
}

// ── Provider ──────────────────────────────────────────────────────

export function MarketProvider({
  children,
  initialSymbol = 'BTC-PERP',
}: {
  children: React.ReactNode
  initialSymbol?: string
}) {
  const [symbol, setSymbol] = useState(initialSymbol)

  const currentMarket = MARKET_CATALOG.find((m) => m.symbol === symbol)

  // ── Candle state ─────────────────────────────────────────────────
  const [candles, setCandles]               = useState<Candle[]>([])
  const [candlesLoading, setCandlesLoading] = useState(true)

  // Current open 1-min candle built from WebSocket ticks (ref = no re-render per tick)
  const liveCandle = useRef<Candle | null>(null)

  // ── Live price state ─────────────────────────────────────────────
  const [livePrice, setLivePrice]   = useState<number | null>(null)
  const allPricesRef = useRef<Map<string, number>>(new Map())
  const symbolPricesRef = useRef<Map<string, number>>(new Map())
  const [allPrices, setAllPrices]     = useState<Map<string, number>>(new Map())
  const [symbolPrices, setSymbolPrices] = useState<Map<string, number>>(new Map())

  // ── Orderbook state ──────────────────────────────────────────────
  const [bids, setBids] = useState<OrderBookLevel[]>([])
  const [asks, setAsks] = useState<OrderBookLevel[]>([])

  // ── Load Pyth Benchmarks candles when market changes ─────────────
  useEffect(() => {
    let cancelled = false
    setCandlesLoading(true)
    liveCandle.current = null
    setLivePrice(null)
    setBids([])
    setAsks([])

    fetchCandles(symbol).then((fetched) => {
      if (cancelled) return
      if (fetched.length > 0) {
        setCandles(fetched)
      } else {
        // Fallback: seeded candles around base price
        const base = currentMarket?.basePrice ?? 1000
        setCandles(makeSeededCandles(base))
      }
      setCandlesLoading(false)
    })

    return () => { cancelled = true }
  }, [symbol]) // eslint-disable-line react-hooks/exhaustive-deps

  // ── Seed symbolPrices from Hermes REST on mount ──────────────────
  useEffect(() => {
    const allIds = MARKET_CATALOG.map((m) => m.pythId).filter(Boolean)
    fetchLatestPrices(allIds).then((priceById) => {
      for (const mkt of MARKET_CATALOG) {
        if (!mkt.pythId) continue
        const price =
          priceById.get(mkt.pythId) ??
          priceById.get('0x' + mkt.pythId) ??
          priceById.get(mkt.pythId.replace(/^0x/, ''))
        if (price && price > 0) {
          symbolPricesRef.current.set(mkt.symbol, price)
          allPricesRef.current.set(mkt.pythId, price)
        }
      }
      if (symbolPricesRef.current.size > 0) {
        setSymbolPrices(new Map(symbolPricesRef.current))
        setAllPrices(new Map(allPricesRef.current))
      }
    })
  }, []) // once on mount

  // ── Pyth WebSocket — all markets ─────────────────────────────────
  useEffect(() => {
    const allIds = MARKET_CATALOG.map((m) => m.pythId).filter(Boolean)

    const disconnect = connectPythWs(allIds, (tick) => {
      // Update allPrices (pythId keyed) and symbolPrices (symbol keyed)
      allPricesRef.current.set(tick.pythId, tick.price)
      setAllPrices(new Map(allPricesRef.current))
      const mkt = MARKET_CATALOG.find((m) => m.pythId === tick.pythId || '0x' + m.pythId === tick.pythId || m.pythId === '0x' + tick.pythId)
      if (mkt) {
        symbolPricesRef.current.set(mkt.symbol, tick.price)
        setSymbolPrices(new Map(symbolPricesRef.current))
      }

      // Only update candles/livePrice for the current market
      if (tick.pythId !== currentMarket?.pythId) return

      setLivePrice(tick.price)

      // Build 1-min live candle
      const minuteStart = Math.floor(tick.publishTime / 60) * 60
      const cur = liveCandle.current

      if (!cur || cur.time !== minuteStart) {
        // Minute rolled over — push previous live candle into history
        if (cur) {
          setCandles((prev) => {
            const last = prev[prev.length - 1]
            // Replace if same minute, append if new
            if (last && last.time === cur.time) {
              return [...prev.slice(0, -1), cur]
            }
            return [...prev.slice(-1439), cur]
          })
        }
        liveCandle.current = {
          time: minuteStart,
          open: tick.price, high: tick.price,
          low:  tick.price, close: tick.price,
          volume: 0,
        }
      } else {
        liveCandle.current = {
          ...cur,
          high:  Math.max(cur.high,  tick.price),
          low:   Math.min(cur.low,   tick.price),
          close: tick.price,
        }
      }

      // Push live candle into candles so the chart updates
      const live = liveCandle.current
      setCandles((prev) => {
        const last = prev[prev.length - 1]
        if (last && last.time === live.time) {
          return [...prev.slice(0, -1), live]
        }
        return [...prev.slice(-1439), live]
      })
    })

    return disconnect
  }, [symbol, currentMarket?.pythId]) // reconnect on market switch

  // ── TEE orderbook polling (3s) ────────────────────────────────────
  const pollBook = useCallback(async () => {
    if (!currentMarket) return
    try {
      const resp = await tee.getMarket(currentMarket.assetId)
      if (resp.bids?.length) setBids(resp.bids)
      if (resp.asks?.length) setAsks(resp.asks)
    } catch { /* TEE offline — keep last known depth */ }
  }, [currentMarket])

  useEffect(() => {
    setBids([])
    setAsks([])
    pollBook()
    const id = window.setInterval(pollBook, BOOK_POLL_MS)
    return () => window.clearInterval(id)
  }, [pollBook])

  // ── Derived market state ──────────────────────────────────────────
  const value = useMemo<MarketContextValue>(() => {
    const first = candles[0]?.open  ?? currentMarket?.basePrice ?? 1
    const last  = candles[candles.length - 1]?.close ?? first

    const index = livePrice ?? last   // Pyth oracle = index price
    const mark  = index               // mark = oracle (no TWAP divergence yet)

    // Open interest estimated from CLOB book depth: Σ(price × size) / PRICE_SCALE²
    // price is in 7-decimal scale (1e7), size is in contract units (also 1e7 based)
    const PRICE_SCALE = 1e7
    const clobLevels = [...bids, ...asks]
    const openInterest = clobLevels.length > 0
      ? clobLevels.reduce((acc, l) => acc + (l.price / PRICE_SCALE) * (l.size / PRICE_SCALE), 0)
      : null

    return {
      symbol,
      mark,
      index,
      changePct: first > 0 ? ((last - first) / first) * 100 : 0,
      funding: calcFunding(mark, index),
      openInterest,
      volume24h: null,
      candles,
      candlesLoading,
      bids,
      asks,
      setSymbol,
      allPrices,
      symbolPrices,
    }
  }, [candles, candlesLoading, symbol, livePrice, bids, asks, currentMarket, allPrices, symbolPrices])

  return <MarketContext.Provider value={value}>{children}</MarketContext.Provider>
}

export function useMarket() {
  const ctx = useContext(MarketContext)
  if (!ctx) throw new Error('useMarket must be inside MarketProvider')
  return ctx
}
