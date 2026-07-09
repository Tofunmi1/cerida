import { useEffect, useState } from 'react'
import { useMarket } from '../../context/market-context'
import { formatCompactUsd, formatUsd } from './format'

function getNextFundingCountdown(): string {
  const now = new Date()
  const s = now.getUTCHours() * 3600 + now.getUTCMinutes() * 60 + now.getUTCSeconds()
  const next = [0, 8, 16].map(h => h * 3600).find(w => w > s) ?? 86400
  const rem = next - s
  const h = Math.floor(rem / 3600)
  const m = Math.floor((rem % 3600) / 60)
  const sec = rem % 60
  if (h > 0) return `${h}h ${String(m).padStart(2, '0')}m`
  if (m > 0) return `${m}m ${String(sec).padStart(2, '0')}s`
  return `${sec}s`
}

export default function MarketStats() {
  const { mark, index, funding, openInterest, volume24h } = useMarket()
  const basis = index > 0 ? ((mark - index) / index) * 100 : 0
  const [nextFunding, setNextFunding] = useState(getNextFundingCountdown)

  useEffect(() => {
    const id = window.setInterval(() => setNextFunding(getNextFundingCountdown()), 1000)
    return () => window.clearInterval(id)
  }, [])

  return (
    <div className="grid h-full grid-cols-2 gap-px bg-border-subtle">
      <Tile label="Oracle index" value={formatUsd(index)} />
      <Tile label="Mark basis" value={`${basis >= 0 ? '+' : ''}${basis.toFixed(4)}%`} />
      <Tile label="Funding / 8h" value={`${funding >= 0 ? '+' : ''}${(funding * 100).toFixed(4)}%`} accent />
      <Tile label="Next funding" value={nextFunding} />
      <Tile label="Open interest" value={openInterest != null && openInterest > 0 ? formatCompactUsd(openInterest) : '—'} />
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
