import { createContext, useContext, useEffect, useMemo, useState } from 'react'

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
  openInterest: number
  volume24h: number
  candles: Candle[]
}

interface MarketContextValue extends MarketState {
  setSymbol: (symbol: string) => void
}

const MarketContext = createContext<MarketContextValue | undefined>(undefined)

export interface MarketDefinition {
  symbol: string
  name: string
  category: 'Crypto' | 'RWA'
  basePrice: number
  icon: string
  color: string
}

export const MARKET_CATALOG: MarketDefinition[] = [
  { symbol: 'BTC-PERP', name: 'Bitcoin', category: 'Crypto', basePrice: 63347.1, icon: '₿', color: '#f7931a' },
  { symbol: 'ETH-PERP', name: 'Ethereum', category: 'Crypto', basePrice: 3412.5, icon: 'Ξ', color: '#627eea' },
  { symbol: 'SOL-PERP', name: 'Solana', category: 'Crypto', basePrice: 142.8, icon: '◎', color: '#14f195' },
  { symbol: 'SPACEX-PERP', name: 'SpaceX', category: 'RWA', basePrice: 216.4, icon: 'X', color: '#111827' },
  { symbol: 'TSLA-PERP', name: 'Tesla', category: 'RWA', basePrice: 184.75, icon: 'T', color: '#e82127' },
  { symbol: 'OIL-PERP', name: 'Crude Oil', category: 'RWA', basePrice: 78.42, icon: 'O', color: '#0f766e' },
  { symbol: 'GOLD-PERP', name: 'Gold', category: 'RWA', basePrice: 2328.6, icon: 'Au', color: '#d4a017' },
]

const START_PRICE = Object.fromEntries(MARKET_CATALOG.map((market) => [market.symbol, market.basePrice]))

function seededNoise(seed: number) {
  const x = Math.sin(seed * 12.9898) * 43758.5453
  return x - Math.floor(x)
}

function seededNormal(seed: number) {
  const u1 = Math.max(seededNoise(seed), 0.000001)
  const u2 = seededNoise(seed + 31.4159)
  return Math.sqrt(-2 * Math.log(u1)) * Math.cos(2 * Math.PI * u2)
}

function makeCandles(base: number): Candle[] {
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
    const high = Math.max(open, close) + wick
    const low = Math.min(open, close) - wick * (0.75 + wickNoise * 0.3)
    out.push({
      time: now - i * 60,
      open,
      high,
      low,
      close,
      volume: 45 + Math.round(wickNoise * 64) + Math.round((body / base) * 26000),
    })
    price = close
  }

  return out
}

export function MarketProvider({ children }: { children: React.ReactNode }) {
  const [symbol, setSymbol] = useState('BTC-PERP')
  const [candles, setCandles] = useState(() => makeCandles(START_PRICE['BTC-PERP']!))

  useEffect(() => {
    setCandles(makeCandles(START_PRICE[symbol] ?? 1000))
  }, [symbol])

  useEffect(() => {
    const id = window.setInterval(() => {
      setCandles((prev) => {
        const last = prev[prev.length - 1]
        if (!last) return prev
        const base = START_PRICE[symbol] ?? last.close
        const tick = Math.floor(Date.now() / 4000)
        const noise = seededNoise(tick + Math.round(last.close))
        const meanRevert = ((base - last.close) / base) * 0.08
        const ret = Math.max(
          -0.0009,
          Math.min(0.0009, seededNormal(tick + Math.round(last.close / 13)) * 0.00018 + meanRevert),
        )
        const close = Math.max(base * 0.96, Math.min(base * 1.04, last.close * Math.exp(ret)))
        const body = Math.abs(close - last.close)
        const next: Candle = {
          time: Math.floor(Date.now() / 1000),
          open: last.close,
          high: Math.max(last.close, close) + last.close * (0.0001 + noise * 0.00014),
          low: Math.min(last.close, close) - last.close * (0.0001 + noise * 0.00013),
          close,
          volume: 45 + Math.round(noise * 64) + Math.round((body / last.close) * 26000),
        }
        return [...prev.slice(-1439), next]
      })
    }, 4000)

    return () => window.clearInterval(id)
  }, [symbol])

  const value = useMemo<MarketContextValue>(() => {
    const first = candles[0]?.open ?? START_PRICE[symbol] ?? 1
    const last = candles[candles.length - 1]?.close ?? first
    const basis = last * 0.00014

    return {
      symbol,
      mark: last,
      index: last - basis,
      changePct: ((last - first) / first) * 100,
      funding: 0.0104,
      openInterest: 18400000000,
      volume24h: 93200000000,
      candles,
      setSymbol,
    }
  }, [candles, symbol])

  return <MarketContext.Provider value={value}>{children}</MarketContext.Provider>
}

export function useMarket() {
  const ctx = useContext(MarketContext)
  if (!ctx) throw new Error('useMarket must be inside MarketProvider')
  return ctx
}
