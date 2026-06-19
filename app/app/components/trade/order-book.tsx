import { useMemo } from 'react'
import { useMarket } from '../../context/market-context'

export default function OrderBook() {
  const { mark } = useMarket()
  const rows = useMemo(() => {
    const asks = Array.from({ length: 12 }, (_, i) => ({
      price: mark + (i + 1) * 8.5,
      size: 0.42 + ((i * 19) % 80) / 100,
      total: 1.2 + i * 0.86,
    })).reverse()
    const bids = Array.from({ length: 12 }, (_, i) => ({
      price: mark - (i + 1) * 8.5,
      size: 0.38 + ((i * 23) % 90) / 100,
      total: 1.1 + i * 0.78,
    }))
    return { asks, bids }
  }, [mark])

  return (
    <div className="flex h-full min-w-0 flex-col bg-surface-primary">
      <Header />
      <BookSide rows={rows.asks} side="ask" />
      <div className="flex items-center justify-between border-y border-border-subtle px-3 py-2">
        <span className="text-[10px] uppercase tracking-widest text-text-quaternary">Mark</span>
        <span className="text-[17px] font-bold tabular-nums text-text-primary">
          {mark.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}
        </span>
      </div>
      <BookSide rows={rows.bids} side="bid" />
    </div>
  )
}

function Header() {
  return (
    <>
      <div className="flex shrink-0 items-center px-3 py-2.5">
        <span className="text-[12px] font-semibold uppercase tracking-widest text-text-tertiary">
          Order Book
        </span>
      </div>
      <div className="grid grid-cols-3 border-y border-border-subtle px-3 py-1 text-[10px] uppercase tracking-widest text-text-quaternary">
        <span>Price</span>
        <span className="text-right">Size</span>
        <span className="text-right">Total</span>
      </div>
    </>
  )
}

function BookSide({
  rows,
  side,
}: {
  rows: { price: number; size: number; total: number }[]
  side: 'bid' | 'ask'
}) {
  const maxTotal = Math.max(...rows.map((row) => row.total))

  return (
    <div className="min-h-0 flex-1 overflow-hidden">
      {rows.map((row, index) => (
        <div
          key={`${side}-${index}`}
          className="relative grid grid-cols-3 px-3 py-0.5 text-[11px] tabular-nums"
        >
          <span
            className={side === 'bid' ? 'text-bullish-green' : 'text-bearish-red'}
          >
            {row.price.toFixed(2)}
          </span>
          <span className="text-right text-text-secondary">{row.size.toFixed(3)}</span>
          <span className="text-right text-text-tertiary">{row.total.toFixed(3)}</span>
          <span
            className={`pointer-events-none absolute right-0 top-0 h-full ${
              side === 'bid' ? 'bg-bullish-green/10' : 'bg-bearish-red/10'
            }`}
            style={{ width: `${(row.total / maxTotal) * 82}%` }}
          />
        </div>
      ))}
    </div>
  )
}
