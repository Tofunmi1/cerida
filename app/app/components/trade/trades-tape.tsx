import { useEffect, useRef, useState } from 'react'
import { useMarket } from '../../context/market-context'
import { tee } from '../../lib/tee-client'

interface TradeRow {
  id: string
  side: 'Buy' | 'Sell'
  price: number
  size: number
  time: string
}

function nowHHMM(): string {
  const d = new Date()
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}:${String(d.getSeconds()).padStart(2, '0')}`
}

// Seeded PRNG so the initial set looks deterministic but non-repeating
function seededRand(seed: number) {
  const x = Math.sin(seed * 127.1 + 311.7) * 43758.5453
  return x - Math.floor(x)
}

function makeRow(base: number, idx: number, ts: number): TradeRow {
  const r1 = seededRand(idx + ts * 0.001)
  const r2 = seededRand(idx + ts * 0.001 + 99)
  const r3 = seededRand(idx + ts * 0.001 + 777)
  const buy = r1 > 0.45
  const spread = base * (0.00015 + r2 * 0.0008)
  return {
    id: `${ts}-${idx}`,
    side: buy ? 'Buy' : 'Sell',
    price: base + (buy ? spread : -spread),
    size: +(0.001 + r3 * 0.18).toFixed(3),
    time: nowHHMM(),
  }
}

const POLL_MS = 2500

export default function TradesTape() {
  const { mark, bids, asks } = useMarket()
  const markRef = useRef(mark)
  markRef.current = mark

  const [rows, setRows] = useState<TradeRow[]>(() => {
    const ts = Date.now()
    return Array.from({ length: 18 }, (_, i) => makeRow(mark || 60000, i, ts - i * 8000))
  })

  const tickRef = useRef(0)

  // Refresh rows: pull real fills from TEE if available, else synthesise
  useEffect(() => {
    let cancelled = false

    async function refresh() {
      if (cancelled) return
      try {
        const resp = await tee.getMarket()
        if (!cancelled && resp.fills && resp.fills.length > 0) {
          // Real fills from TEE
          const live: TradeRow[] = resp.fills.map((f, i) => ({
            id: `${f.maker_id}-${i}`,
            side: i % 2 === 0 ? 'Buy' : 'Sell',
            price: f.price,
            size: f.size,
            time: nowHHMM(),
          }))
          setRows((prev) => {
            const combined = [...live, ...prev].slice(0, 18)
            return combined
          })
          return
        }
      } catch { /* TEE offline — fall through to synthetic */ }

      // Synthetic: inject 1–2 new rows at the top
      if (!cancelled) {
        const ts = Date.now()
        tickRef.current += 1
        const newRow = makeRow(markRef.current || 60000, tickRef.current, ts)
        setRows((prev) => [newRow, ...prev].slice(0, 18))
      }
    }

    refresh()
    const id = window.setInterval(refresh, POLL_MS)
    return () => {
      cancelled = true
      clearInterval(id)
    }
  }, []) // intentionally no deps — markRef keeps it fresh

  function fmtPrice(p: number) {
    if (p > 10000) return p.toFixed(1)
    if (p > 100) return p.toFixed(2)
    if (p > 1) return p.toFixed(3)
    return p.toFixed(4)
  }

  return (
    <div className="flex h-full flex-col bg-surface-primary">
      <div className="flex shrink-0 items-center border-b border-border-subtle px-3 py-2.5">
        <span className="text-[12px] font-semibold uppercase tracking-widest text-text-tertiary">
          Trades
        </span>
      </div>
      <div className="grid grid-cols-3 border-b border-border-subtle px-3 py-1 text-[10px] uppercase tracking-widest text-text-quaternary">
        <span>Price</span>
        <span className="text-right">Size</span>
        <span className="text-right">Time</span>
      </div>
      <div className="min-h-0 flex-1 overflow-hidden">
        {rows.map((row) => (
          <div key={row.id} className="grid grid-cols-3 px-3 py-0.5 text-[11px] tabular-nums">
            <span className={row.side === 'Buy' ? 'text-bullish-green' : 'text-bearish-red'}>
              {fmtPrice(row.price)}
            </span>
            <span className="text-right text-text-secondary">{row.size.toFixed(3)}</span>
            <span className="text-right text-text-quaternary">{row.time}</span>
          </div>
        ))}
      </div>
    </div>
  )
}
