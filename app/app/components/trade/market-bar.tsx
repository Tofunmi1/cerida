import { useMemo, useState } from 'react'
import {
  IconAlertTriangleFilled,
  IconBell,
  IconBook,
  IconBriefcase,
  IconChevronDown,
  IconCopy,
  IconExternalLink,
  IconLogout,
  IconSearch,
  IconSettings,
  IconPalette,
  IconWallet,
  IconX,
} from '@tabler/icons-react'
import { MARKET_CATALOG, useMarket, type MarketDefinition } from '../../context/market-context'
import { THEMES, useTheme } from '../../context/theme-context'
import { formatContractBalance, useWallet } from '../../context/wallet-context'
import { formatCompactUsd, formatUsd } from './format'
import { toast } from '../toast/toast-context'

export default function MarketBar({
  active,
  onActive,
  onOpenSettings,
}: {
  active: string
  onActive: (label: string) => void
  onOpenSettings: () => void
}) {
  const { symbol, setSymbol, mark, index, changePct, funding, openInterest, volume24h } = useMarket()
  const { theme, setTheme } = useTheme()
  const [marketOpen, setMarketOpen] = useState(false)
  const [themeOpen, setThemeOpen] = useState(false)
  const [alertsOpen, setAlertsOpen] = useState(false)
  const activeMarket = MARKET_CATALOG.find((market) => market.symbol === symbol) ?? MARKET_CATALOG[0]!
  const activeTheme = THEMES.find((item) => item.id === theme) ?? THEMES[0]!
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
          <NavPill
            icon={<IconBriefcase size={15} stroke={1.8} />}
            label="Portfolio"
            active={active === 'Portfolio'}
            onClick={() => onActive('Portfolio')}
          />
          <NavPill
            icon={<IconBook size={15} stroke={1.8} />}
            label="Docs"
            active={active === 'Docs'}
            onClick={() => onActive('Docs')}
          />
          <div className="mx-1 h-5 w-px bg-border-subtle" />
          <div className="relative">
            <button
              onClick={() => setThemeOpen((value) => !value)}
              className="flex h-9 items-center gap-2 rounded-[8px] border border-border-subtle bg-surface-primary px-2.5 text-[12px] font-semibold text-text-secondary transition-colors hover:text-text-primary"
              title="Theme"
            >
              <IconPalette size={15} stroke={1.8} />
              <span className="hidden lg:inline">{activeTheme.label}</span>
              <IconChevronDown size={12} stroke={2} />
            </button>
            {themeOpen && (
              <>
                <div className="fixed inset-0 z-[70]" onClick={() => setThemeOpen(false)} />
                <div className="absolute right-0 top-11 z-[71] w-44 rounded-[10px] border border-border-subtle bg-surface-primary p-1 shadow-xl">
                  {THEMES.map((item) => (
                    <button
                      key={item.id}
                      onClick={() => {
                        setTheme(item.id)
                        setThemeOpen(false)
                      }}
                      className={`flex w-full items-center justify-between rounded-[7px] px-3 py-2 text-left text-[12px] font-semibold transition-colors ${
                        theme === item.id
                          ? 'bg-surface-hover text-text-primary'
                          : 'text-text-tertiary hover:bg-surface-card hover:text-text-primary'
                      }`}
                    >
                      {item.label}
                      {theme === item.id && <span className="h-1.5 w-1.5 rounded-full bg-brand-violet" />}
                    </button>
                  ))}
                </div>
              </>
            )}
          </div>
          <div className="relative">
            <IconButton label="Alerts" onClick={() => setAlertsOpen((v) => !v)}>
              <IconBell size={15} stroke={1.8} />
            </IconButton>
            {alertsOpen && (
              <>
                <div className="fixed inset-0 z-[70]" onClick={() => setAlertsOpen(false)} />
                <div className="absolute right-0 top-11 z-[71] w-72 rounded-[10px] border border-border-subtle bg-surface-primary p-1 shadow-xl">
                  <div className="px-3 py-2 text-[11px] font-semibold uppercase tracking-widest text-text-quaternary">
                    Notifications
                  </div>
                  <div className="flex flex-col items-center gap-2 px-4 py-8 text-center">
                    <IconBell size={22} stroke={1.5} className="text-text-quaternary" />
                    <p className="text-[12px] text-text-tertiary">No notifications yet.</p>
                  </div>
                </div>
              </>
            )}
          </div>
          <IconButton label="Settings" onClick={onOpenSettings}>
            <IconSettings size={15} stroke={1.8} />
          </IconButton>
          <WalletButton />
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

function NavPill({
  icon,
  label,
  active,
  onClick,
}: {
  icon: React.ReactNode
  label: string
  active: boolean
  onClick: () => void
}) {
  return (
    <button
      onClick={onClick}
      className={`flex h-9 items-center gap-2 rounded-[8px] border px-2.5 text-[12px] font-semibold transition-colors ${
        active
          ? 'border-border-subtle bg-surface-card text-text-primary'
          : 'border-border-subtle bg-surface-primary text-text-secondary hover:text-text-primary'
      }`}
      title={label}
    >
      {icon}
      <span className="hidden lg:inline">{label}</span>
    </button>
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

function WalletButton() {
  const { connected, connecting, publicKey, balance, balanceLoading, wrongNetwork, connect, disconnect } =
    useWallet()
  const [open, setOpen] = useState(false)

  const copyAddress = async () => {
    if (!publicKey) return
    await navigator.clipboard.writeText(publicKey)
    toast.success('Copied', 'Address copied to clipboard.', { duration: 2000 })
  }

  if (!connected || !publicKey) {
    return (
      <button
        onClick={connect}
        disabled={connecting}
        className="flex items-center gap-2 rounded-[8px] bg-brand-violet px-3 py-2 text-[12px] font-semibold text-white disabled:opacity-60"
      >
        <IconWallet size={15} stroke={2} />
        {connecting ? 'Connecting…' : 'Connect'}
      </button>
    )
  }

  return (
    <div className="relative">
      <button
        onClick={() => setOpen((v) => !v)}
        className={`flex items-center gap-2 rounded-[8px] border px-3 py-2 text-[12px] font-semibold transition-colors ${
          wrongNetwork
            ? 'border-warning/40 bg-warning/10 text-warning'
            : 'border-border-subtle bg-surface-card text-text-primary hover:bg-surface-hover'
        }`}
        title={wrongNetwork ? 'Wrong network — click for details' : undefined}
      >
        {wrongNetwork ? (
          <IconAlertTriangleFilled size={15} />
        ) : (
          <IconWallet size={15} stroke={2} />
        )}
        <span className="tabular-nums">
          {balanceLoading ? '…' : `$${formatContractBalance(balance)}`}
        </span>
        <span className="max-w-[80px] truncate font-mono text-[10px] text-text-tertiary">
          {publicKey.slice(0, 4)}…{publicKey.slice(-4)}
        </span>
        <IconChevronDown size={12} stroke={2} />
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-[70]" onClick={() => setOpen(false)} />
          <div className="absolute right-0 top-11 z-[71] w-64 rounded-[10px] border border-border-subtle bg-surface-primary p-1 shadow-xl">
            {wrongNetwork && (
              <div className="m-1 flex items-start gap-2 rounded-[8px] bg-warning/10 px-3 py-2 text-[11px] text-warning">
                <IconAlertTriangleFilled size={14} className="mt-0.5 shrink-0" />
                <span>Freighter is on the wrong network. Switch to Testnet to trade.</span>
              </div>
            )}
            <div className="px-3 py-2">
              <div className="text-[10px] uppercase tracking-widest text-text-quaternary">Address</div>
              <div className="mt-0.5 truncate font-mono text-[12px] text-text-primary">{publicKey}</div>
            </div>
            <button
              onClick={copyAddress}
              className="flex w-full items-center gap-2.5 rounded-[7px] px-3 py-2 text-left text-[12px] font-medium text-text-secondary transition-colors hover:bg-surface-card hover:text-text-primary"
            >
              <IconCopy size={14} stroke={1.8} />
              Copy address
            </button>
            <button
              onClick={() =>
                window.open(
                  `https://stellar.expert/explorer/testnet/account/${publicKey}`,
                  '_blank',
                  'noopener,noreferrer',
                )
              }
              className="flex w-full items-center gap-2.5 rounded-[7px] px-3 py-2 text-left text-[12px] font-medium text-text-secondary transition-colors hover:bg-surface-card hover:text-text-primary"
            >
              <IconExternalLink size={14} stroke={1.8} />
              View on Stellar Expert
            </button>
            <div className="mx-1 my-1 h-px bg-border-subtle" />
            <button
              onClick={() => {
                disconnect()
                setOpen(false)
              }}
              className="flex w-full items-center gap-2.5 rounded-[7px] px-3 py-2 text-left text-[12px] font-medium text-bearish-red transition-colors hover:bg-surface-card"
            >
              <IconLogout size={14} stroke={1.8} />
              Disconnect
            </button>
          </div>
        </>
      )}
    </div>
  )
}
