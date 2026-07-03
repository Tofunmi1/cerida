import { useMarket } from '../../context/market-context'
import { formatCompactUsd, formatUsd } from './format'

export default function MarketStats() {
  const { mark, index, funding, openInterest, volume24h } = useMarket()
  const basis = index > 0 ? ((mark - index) / index) * 100 : 0

  return (
    <div className="grid h-full grid-cols-2 gap-px bg-border-subtle">
      <Tile label="Oracle index" value={formatUsd(index)} />
      <Tile label="Mark basis" value={`${basis >= 0 ? '+' : ''}${basis.toFixed(4)}%`} />
      <Tile label="Funding / 8h" value={`${funding >= 0 ? '+' : ''}${(funding * 100).toFixed(4)}%`} accent />
      <Tile label="Next funding" value="—" />
      <Tile label="Open interest" value={openInterest != null ? formatCompactUsd(openInterest) : '—'} />
      <Tile label="24h volume"    value={volume24h   != null ? formatCompactUsd(volume24h)   : '—'} />
    </div>
  )
}

function Tile({ label, value, accent = false }: { label: string; value: string; accent?: boolean }) {
  return (
    <div className="flex min-w-0 flex-col justify-center gap-1 bg-surface-primary px-3 py-2">
      <span className="text-[10px] uppercase tracking-widest text-text-quaternary">{label}</span>
      <span className={`truncate text-[14px] font-semibold tabular-nums ${accent ? 'text-brand-violet' : 'text-text-secondary'}`}>
        {value}
      </span>
    </div>
  )
}
