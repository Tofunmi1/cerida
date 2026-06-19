import { useMemo } from 'react'
import { useMarket } from '../../context/market-context'

export default function TradesTape() {
  const { mark } = useMarket()
  const rows = useMemo(
    () =>
      Array.from({ length: 18 }, (_, i) => {
        const buy = i % 3 !== 0
        return {
          side: buy ? 'Buy' : 'Sell',
          price: mark + (buy ? 1 : -1) * ((i * 3.7) % 28),
          size: 0.01 + ((i * 13) % 120) / 1000,
          time: `${String(new Date().getHours()).padStart(2, '0')}:${String(
            (new Date().getMinutes() + i) % 60,
          ).padStart(2, '0')}`,
        }
      }),
    [mark],
  )

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
        {rows.map((row, i) => (
          <div key={i} className="grid grid-cols-3 px-3 py-0.5 text-[11px] tabular-nums">
            <span className={row.side === 'Buy' ? 'text-bullish-green' : 'text-bearish-red'}>
              {row.price.toFixed(2)}
            </span>
            <span className="text-right text-text-secondary">{row.size.toFixed(3)}</span>
            <span className="text-right text-text-quaternary">{row.time}</span>
          </div>
        ))}
      </div>
    </div>
  )
}
