import { useMemo, useState } from 'react'
import {
  IconBell,
  IconChevronDown,
  IconMoon,
  IconSearch,
  IconSettings,
  IconSun,
  IconWallet,
  IconX,
} from '@tabler/icons-react'
import { MARKET_CATALOG, useMarket, type MarketDefinition } from '../../context/market-context'
import { useTheme } from '../../context/theme-context'
import { formatCompactUsd, formatUsd } from './format'
import { toast } from '../toast/toast-context'

export default function MarketBar() {
  const { symbol, setSymbol, mark, index, changePct, funding, openInterest, volume24h } = useMarket()
  const { theme, toggleTheme } = useTheme()
  const [marketOpen, setMarketOpen] = useState(false)
  const activeMarket = MARKET_CATALOG.find((market) => market.symbol === symbol) ?? MARKET_CATALOG[0]!
  const positive = changePct >= 0

  return (
    <>
      <div className="flex h-14 shrink-0 items-center gap-3 border-b border-border-subtle bg-page px-3">
        <button
          onClick={() => setMarketOpen(true)}
          className="flex min-w-[210px] items-center justify-between gap-3 rounded-[10px] border border-border-subtle bg-surface-primary px-3 py-2 transition-colors hover:bg-surface-hover"
        >
          <span className="flex min-w-0 items-center gap-2">
            <span
              className="grid h-7 w-7 shrink-0 place-items-center rounded-full text-[12px] font-bold text-white"
              style={{ backgroundColor: activeMarket.color }}
            >
              {activeMarket.icon}
            </span>
            <span className="min-w-0 text-left">
              <span className="block truncate text-[13px] font-bold text-text-primary">{activeMarket.symbol}</span>
              <span className="block truncate text-[10px] uppercase tracking-widest text-text-quaternary">
                {activeMarket.category}
              </span>
            </span>
          </span>
          <IconChevronDown size={14} stroke={2} className="shrink-0 text-text-tertiary" />
        </button>

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
          <IconButton label={theme === 'light' ? 'Dark mode' : 'Light mode'} onClick={toggleTheme}>
            {theme === 'light' ? <IconMoon size={15} stroke={1.8} /> : <IconSun size={15} stroke={1.8} />}
          </IconButton>
          <IconButton label="Alerts" onClick={() => toast.info('Alerts', 'Price alerts are not configured in this build yet.')}>
            <IconBell size={15} stroke={1.8} />
          </IconButton>
          <IconButton label="Settings" onClick={() => toast.info('Settings', 'Trading preferences are coming next.')}>
            <IconSettings size={15} stroke={1.8} />
          </IconButton>
          <button
            onClick={() => toast.info('Wallet connection', 'Wallet integration is not connected in this build yet.')}
            className="flex items-center gap-2 rounded-[8px] bg-brand-violet px-3 py-2 text-[12px] font-semibold text-white"
          >
            <IconWallet size={15} stroke={2} />
            Connect
          </button>
        </div>
      </div>

      {marketOpen && (
        <MarketModal
          activeSymbol={symbol}
          onSelect={(next) => {
            const market = MARKET_CATALOG.find((item) => item.symbol === next)
            setSymbol(next)
            setMarketOpen(false)
            toast.success('Market selected', `${market?.name ?? next} perpetual is now active.`, { duration: 3000 })
          }}
          onClose={() => setMarketOpen(false)}
        />
      )}
    </>
  )
}

function MarketModal({
  activeSymbol,
  onSelect,
  onClose,
}: {
  activeSymbol: string
  onSelect: (symbol: string) => void
  onClose: () => void
}) {
  const [query, setQuery] = useState('')
  const [category, setCategory] = useState<'All' | 'Crypto' | 'RWA'>('All')

  const markets = useMemo(
    () =>
      MARKET_CATALOG.filter((market) => {
        const matchesCategory = category === 'All' || market.category === category
        const normalized = `${market.symbol} ${market.name} ${market.category}`.toLowerCase()
        return matchesCategory && normalized.includes(query.toLowerCase())
      }),
    [category, query],
  )

  return (
    <div
      className="fixed inset-0 z-[80] flex items-center justify-center bg-black/35 p-6 backdrop-blur-sm"
      onMouseDown={(event) => {
        if (event.currentTarget === event.target) onClose()
      }}
    >
      <div className="flex h-[min(720px,86vh)] w-[min(980px,94vw)] flex-col overflow-hidden rounded-[14px] border border-border-subtle bg-surface-primary shadow-2xl">
        <div className="flex shrink-0 items-center gap-3 border-b border-border-subtle px-5 py-4">
          <div>
            <h2 className="text-[18px] font-bold text-text-primary">Markets</h2>
            <p className="text-[12px] text-text-tertiary">Crypto and RWA perpetuals</p>
          </div>
          <button
            onClick={onClose}
            className="ml-auto grid h-9 w-9 place-items-center rounded-[8px] text-text-tertiary hover:bg-surface-hover hover:text-text-primary"
          >
            <IconX size={18} stroke={2} />
          </button>
        </div>

        <div className="flex shrink-0 flex-wrap items-center gap-3 border-b border-border-subtle px-5 py-3">
          <div className="flex h-10 min-w-[280px] flex-1 items-center gap-2 rounded-[10px] border border-border-subtle bg-surface-card px-3">
            <IconSearch size={16} stroke={2} className="text-text-quaternary" />
            <input
              autoFocus
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search BTC, SpaceX, Oil..."
              className="min-w-0 flex-1 bg-transparent text-[13px] text-text-primary outline-none placeholder:text-text-quaternary"
            />
          </div>
          <div className="flex items-center gap-1 rounded-[10px] border border-border-subtle bg-surface-card p-1">
            {(['All', 'Crypto', 'RWA'] as const).map((item) => (
              <button
                key={item}
                onClick={() => setCategory(item)}
                className={`rounded-[7px] px-3 py-1.5 text-[12px] font-semibold transition-colors ${
                  category === item
                    ? 'bg-brand-violet text-white'
                    : 'text-text-tertiary hover:bg-surface-hover hover:text-text-primary'
                }`}
              >
                {item}
              </button>
            ))}
          </div>
        </div>

        <div className="grid shrink-0 grid-cols-[1fr_110px_110px_120px] border-b border-border-subtle px-5 py-2 text-[10px] uppercase tracking-widest text-text-quaternary">
          <span>Market</span>
          <span>Category</span>
          <span className="text-right">Oracle</span>
          <span className="text-right">Status</span>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto">
          {markets.map((market) => (
            <MarketRow
              key={market.symbol}
              market={market}
              active={market.symbol === activeSymbol}
              onSelect={() => onSelect(market.symbol)}
            />
          ))}
        </div>
      </div>
    </div>
  )
}

function MarketRow({
  market,
  active,
  onSelect,
}: {
  market: MarketDefinition
  active: boolean
  onSelect: () => void
}) {
  return (
    <button
      onClick={onSelect}
      className={`grid w-full grid-cols-[1fr_110px_110px_120px] items-center border-b border-border-subtle/70 px-5 py-3 text-left transition-colors ${
        active ? 'bg-surface-hover' : 'hover:bg-surface-card'
      }`}
    >
      <span className="flex min-w-0 items-center gap-3">
        <span
          className="grid h-10 w-10 shrink-0 place-items-center rounded-full text-[13px] font-bold text-white"
          style={{ backgroundColor: market.color }}
        >
          {market.icon}
        </span>
        <span className="min-w-0">
          <span className="block truncate text-[14px] font-bold text-text-primary">{market.symbol}</span>
          <span className="block truncate text-[12px] text-text-tertiary">{market.name} perpetual</span>
        </span>
      </span>
      <span className="text-[12px] font-semibold text-text-secondary">{market.category}</span>
      <span className="text-right text-[12px] tabular-nums text-text-secondary">
        {formatUsd(market.basePrice)}
      </span>
      <span className="text-right">
        <span className="rounded-[5px] bg-bullish-green/10 px-2 py-1 text-[10px] font-bold uppercase tracking-widest text-bullish-green">
          Live
        </span>
      </span>
    </button>
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

function IconButton({
  label,
  children,
  onClick,
}: {
  label: string
  children: React.ReactNode
  onClick?: () => void
}) {
  return (
    <button
      onClick={onClick}
      aria-label={label}
      title={label}
      className="grid h-9 w-9 place-items-center rounded-[8px] border border-border-subtle bg-surface-primary text-text-tertiary transition-colors hover:text-text-primary"
    >
      {children}
    </button>
  )
}
