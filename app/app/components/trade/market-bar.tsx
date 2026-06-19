import { IconBell, IconSettings, IconWallet } from '@tabler/icons-react'
import { useMarket } from '../../context/market-context'
import { formatCompactUsd, formatUsd } from './format'

const MARKETS = ['BTC-PERP', 'ETH-PERP', 'SOL-PERP']

export default function MarketBar() {
  const { symbol, setSymbol, mark, index, changePct, funding, openInterest, volume24h } = useMarket()
  const positive = changePct >= 0

  return (
    <div className="flex h-14 shrink-0 items-center gap-3 border-b border-border-subtle bg-page px-3">
      <div className="flex items-center gap-1 rounded-[8px] border border-border-subtle bg-surface-primary p-1">
        {MARKETS.map((market) => (
          <button
            key={market}
            onClick={() => setSymbol(market)}
            className={`rounded-[6px] px-3 py-1.5 text-[12px] font-semibold transition-colors ${
              symbol === market
                ? 'bg-surface-hover text-text-primary'
                : 'text-text-tertiary hover:text-text-primary'
            }`}
          >
            {market}
          </button>
        ))}
      </div>

      <div className="hidden min-w-0 items-center gap-6 md:flex">
        <Stat label="Mark">{formatUsd(mark)}</Stat>
        <Stat label="Index">{formatUsd(index)}</Stat>
        <Stat label="24h">
          <span className={positive ? 'text-bullish-green' : 'text-bearish-red'}>
            {positive ? '+' : ''}
            {changePct.toFixed(2)}%
          </span>
        </Stat>
        <Stat label="Funding">
          <span className="text-brand-violet">{funding.toFixed(4)}%</span>
        </Stat>
        <Stat label="Open Interest">{formatCompactUsd(openInterest)}</Stat>
        <Stat label="Volume">{formatCompactUsd(volume24h)}</Stat>
      </div>

      <div className="ml-auto flex items-center gap-1">
        <IconButton label="Alerts">
          <IconBell size={15} stroke={1.8} />
        </IconButton>
        <IconButton label="Settings">
          <IconSettings size={15} stroke={1.8} />
        </IconButton>
        <button className="flex items-center gap-2 rounded-[8px] bg-brand-violet px-3 py-2 text-[12px] font-semibold text-white">
          <IconWallet size={15} stroke={2} />
          Connect
        </button>
      </div>
    </div>
  )
}

function Stat({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[9px] uppercase tracking-widest text-text-quaternary">{label}</span>
      <span className="text-[12px] font-medium tabular-nums text-text-secondary">{children}</span>
    </div>
  )
}

function IconButton({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <button
      aria-label={label}
      title={label}
      className="grid h-9 w-9 place-items-center rounded-[8px] border border-border-subtle bg-surface-primary text-text-tertiary transition-colors hover:text-text-primary"
    >
      {children}
    </button>
  )
}
