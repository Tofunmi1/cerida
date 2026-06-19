import { useMarket } from '../../context/market-context'
import { formatUsd } from './format'

const POSITIONS = [
  { side: 'Long', size: 0.24, entry: 62810.4, margin: 750, pnl: 182.16, leverage: 12 },
  { side: 'Short', size: 3.8, entry: 145.22, margin: 180, pnl: -24.9, leverage: 8 },
]

export default function PositionsPanel() {
  const { symbol, mark } = useMarket()

  return (
    <div className="flex h-full flex-col bg-surface-primary">
      <div className="flex shrink-0 items-center gap-4 border-b border-border-subtle px-3 py-2">
        {['Positions', 'Orders', 'Trades'].map((tab, index) => (
          <button
            key={tab}
            className={`text-[12px] font-semibold ${
              index === 0 ? 'text-text-primary' : 'text-text-quaternary hover:text-text-secondary'
            }`}
          >
            {tab}
          </button>
        ))}
      </div>

      <div className="grid grid-cols-[1fr_90px_90px_90px_90px_90px_90px] border-b border-border-subtle px-3 py-2 text-[10px] uppercase tracking-widest text-text-quaternary">
        <span>Market</span>
        <span>Side</span>
        <span className="text-right">Size</span>
        <span className="text-right">Entry</span>
        <span className="text-right">Mark</span>
        <span className="text-right">Margin</span>
        <span className="text-right">PnL</span>
      </div>

      <div className="min-h-0 flex-1 overflow-auto">
        {POSITIONS.map((position) => (
          <div
            key={`${position.side}-${position.entry}`}
            className="grid grid-cols-[1fr_90px_90px_90px_90px_90px_90px] border-b border-border-subtle/60 px-3 py-2 text-[11px] tabular-nums"
          >
            <span className="font-semibold text-text-secondary">{symbol}</span>
            <span className={position.side === 'Long' ? 'text-bullish-green' : 'text-bearish-red'}>
              {position.side} {position.leverage}x
            </span>
            <span className="text-right text-text-secondary">{position.size.toFixed(3)}</span>
            <span className="text-right text-text-tertiary">{formatUsd(position.entry)}</span>
            <span className="text-right text-text-tertiary">{formatUsd(mark)}</span>
            <span className="text-right text-text-tertiary">{formatUsd(position.margin, 0)}</span>
            <span
              className={`text-right ${
                position.pnl >= 0 ? 'text-bullish-green' : 'text-bearish-red'
              }`}
            >
              {position.pnl >= 0 ? '+' : ''}
              {formatUsd(position.pnl)}
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}
