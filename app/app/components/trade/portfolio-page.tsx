import { useState } from 'react'
import { IconArrowDownToArc, IconArrowUpFromArc, IconChevronDown, IconX } from '@tabler/icons-react'
import { formatUsd } from './format'

const ASSETS = ['USDC', 'ETH', 'BTC', 'SOL'] as const
type Asset = (typeof ASSETS)[number]

const ASSET_BALANCE: Record<Asset, number> = {
  USDC: 12480.92,
  ETH: 2.418,
  BTC: 0.114,
  SOL: 48.5,
}

const ASSET_PRICE: Record<Asset, number> = {
  USDC: 1,
  ETH: 3412.5,
  BTC: 63200,
  SOL: 148.6,
}

const HISTORY = [
  { type: 'Deposit', asset: 'USDC', amount: 5000, time: '2026-06-29 14:22', status: 'Complete' },
  { type: 'Withdraw', asset: 'USDC', amount: 1200, time: '2026-06-28 09:11', status: 'Complete' },
  { type: 'Deposit', asset: 'ETH', amount: 1.5, time: '2026-06-27 18:03', status: 'Complete' },
  { type: 'Deposit', asset: 'USDC', amount: 8000, time: '2026-06-25 11:47', status: 'Complete' },
  { type: 'Withdraw', asset: 'SOL', amount: 12, time: '2026-06-23 16:30', status: 'Complete' },
] as const

const PCT_OPTIONS = [25, 50, 75] as const

function AssetPicker({
  value,
  onChange,
}: {
  value: Asset
  onChange: (a: Asset) => void
}) {
  const [open, setOpen] = useState(false)
  return (
    <div className="relative">
      <button
        onClick={() => setOpen((p) => !p)}
        className="flex items-center gap-1.5 rounded-[6px] border border-border-subtle bg-surface-card px-2.5 py-1 text-[12px] font-bold text-text-primary transition-colors hover:bg-surface-hover"
      >
        {value}
        <IconChevronDown size={11} stroke={2.5} />
      </button>
      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} />
          <div className="absolute right-0 top-full z-20 mt-1 w-28 rounded-[8px] border border-border-subtle bg-surface-card py-1 shadow-xl">
            {ASSETS.map((a) => (
              <button
                key={a}
                onClick={() => {
                  onChange(a)
                  setOpen(false)
                }}
                className={`block w-full px-3 py-1.5 text-left text-[12px] transition-colors hover:bg-surface-hover ${
                  a === value ? 'text-text-primary font-semibold' : 'text-text-secondary'
                }`}
              >
                {a}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  )
}

function TransferPanel({
  mode,
}: {
  mode: 'deposit' | 'withdraw'
}) {
  const [asset, setAsset] = useState<Asset>('USDC')
  const [amount, setAmount] = useState('')
  const [pct, setPct] = useState<number | null>(null)

  const balance = ASSET_BALANCE[asset]
  const price = ASSET_PRICE[asset]
  const parsed = parseFloat(amount) || 0
  const usdValue = parsed * price
  const isDeposit = mode === 'deposit'

  const applyPct = (p: number) => {
    setPct(p)
    setAmount(((balance * p) / 100).toFixed(asset === 'USDC' ? 2 : 6))
  }

  const handleMax = () => {
    setPct(100)
    setAmount(balance.toString())
  }

  const label = isDeposit ? 'Deposit' : 'Withdraw'

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <span className="text-[10px] uppercase tracking-widest text-text-quaternary">Asset</span>
        <AssetPicker value={asset} onChange={(a) => { setAsset(a); setAmount(''); setPct(null) }} />
      </div>

      <div>
        <div className="mb-1.5 flex items-center justify-between text-[11px]">
          <span className="text-text-tertiary">Amount</span>
          <span className="text-text-quaternary">
            {isDeposit ? 'Wallet' : 'Available'}:{' '}
            <span className="text-text-secondary">
              {balance} {asset}
            </span>
          </span>
        </div>
        <div className="flex items-center gap-2 rounded-[8px] border border-border-subtle bg-surface-primary px-3 py-2 focus-within:border-border-default">
          <input
            type="number"
            value={amount}
            onChange={(e) => {
              setAmount(e.target.value)
              setPct(null)
            }}
            placeholder="0.00"
            min="0"
            step="any"
            className="min-w-0 flex-1 bg-transparent text-[18px] font-medium text-text-primary outline-none placeholder:text-text-quaternary"
            style={{ fontFamily: 'var(--font-mono)' }}
          />
          <span className="shrink-0 text-[11px] font-bold text-text-quaternary">{asset}</span>
        </div>
        {parsed > 0 && (
          <div className="mt-1 text-right text-[11px] text-text-quaternary tabular-nums">
            ≈ {formatUsd(usdValue)}
          </div>
        )}
      </div>

      <div className="flex items-center gap-1.5">
        {PCT_OPTIONS.map((p) => (
          <button
            key={p}
            onClick={() => applyPct(p)}
            className={`flex-1 rounded-[5px] py-1.5 text-[12px] font-medium transition-colors ${
              pct === p
                ? 'bg-surface-hover text-text-primary'
                : 'bg-surface-card text-text-tertiary hover:bg-surface-hover hover:text-text-secondary'
            }`}
          >
            {p}%
          </button>
        ))}
        <button
          onClick={handleMax}
          className={`flex-1 rounded-[5px] py-1.5 text-[12px] font-medium transition-colors ${
            pct === 100
              ? 'bg-surface-hover text-text-primary'
              : 'bg-surface-card text-text-tertiary hover:bg-surface-hover hover:text-text-secondary'
          }`}
        >
          MAX
        </button>
      </div>

      <div className="rounded-[8px] border border-border-subtle bg-surface-card px-3 py-2.5 text-[11px]">
        <div className="flex justify-between text-text-tertiary">
          <span>Network</span>
          <span className="text-text-secondary">Stellar Testnet</span>
        </div>
        <div className="mt-1.5 flex justify-between text-text-tertiary">
          <span>Est. fee</span>
          <span className="text-text-secondary tabular-nums">~0.00001 XLM</span>
        </div>
        <div className="mt-1.5 flex justify-between text-text-tertiary">
          <span>Confirmation</span>
          <span className="text-text-secondary">~5 sec</span>
        </div>
      </div>

      <button
        className={`w-full rounded-[8px] py-2.5 text-[13px] font-semibold transition-opacity hover:opacity-90 ${
          isDeposit
            ? 'bg-bullish-green text-[#1a1a1a]'
            : 'bg-brand-violet text-white'
        } ${parsed <= 0 ? 'cursor-not-allowed opacity-40' : ''}`}
        disabled={parsed <= 0}
      >
        {label} {parsed > 0 ? `${amount} ${asset}` : asset}
      </button>
    </div>
  )
}

const STAT_CARDS = [
  { label: 'Total Value', value: '$18,241.30', delta: '+2.4%', positive: true },
  { label: 'Available Margin', value: '$12,480.92', delta: null, positive: null },
  { label: 'In Positions', value: '$5,760.38', delta: null, positive: null },
  { label: 'Unrealized PnL', value: '+$157.26', delta: null, positive: true },
]

export default function PortfolioPage({ onClose }: { onClose: () => void }) {
  const [tab, setTab] = useState<'deposit' | 'withdraw'>('deposit')

  return (
    <div
      className="fixed inset-0 z-[80] flex items-center justify-center bg-black/35 p-6 backdrop-blur-sm"
      onMouseDown={(event) => {
        if (event.currentTarget === event.target) onClose()
      }}
    >
      <div className="flex h-[min(820px,90vh)] w-[min(1080px,94vw)] flex-col overflow-hidden rounded-[14px] border border-border-subtle bg-surface-primary shadow-2xl">
        <div className="flex shrink-0 items-center gap-3 border-b border-border-subtle px-6 py-4">
          <div>
            <h1 className="text-[15px] font-semibold uppercase tracking-widest text-text-primary">
              Portfolio
            </h1>
            <p className="mt-0.5 text-[12px] text-text-quaternary">Manage balances and transfers</p>
          </div>
          <button
            onClick={onClose}
            className="ml-auto grid h-9 w-9 place-items-center rounded-[8px] text-text-tertiary hover:bg-surface-hover hover:text-text-primary"
          >
            <IconX size={18} stroke={2} />
          </button>
        </div>

        <div className="flex min-h-0 flex-1 flex-col overflow-auto bg-page px-6 py-5">
          <div className="mb-6 grid grid-cols-4 gap-3">
            {STAT_CARDS.map((card) => (
              <div
                key={card.label}
                className="rounded-[8px] border border-border-subtle bg-surface-primary px-4 py-3"
              >
                <div className="text-[10px] uppercase tracking-widest text-text-quaternary">
                  {card.label}
                </div>
                <div className="mt-1.5 flex items-baseline gap-2">
                  <span
                    className={`text-[18px] font-semibold tabular-nums ${
                      card.positive === true
                        ? 'text-bullish-green'
                        : card.positive === false
                          ? 'text-bearish-red'
                          : 'text-text-primary'
                    }`}
                  >
                    {card.value}
                  </span>
                  {card.delta && (
                    <span
                      className={`text-[11px] font-medium ${
                        card.positive ? 'text-bullish-green' : 'text-bearish-red'
                      }`}
                    >
                      {card.delta}
                    </span>
                  )}
                </div>
              </div>
            ))}
          </div>

          <div className="grid min-h-0 grid-cols-[380px_1fr] gap-4">
            <div className="rounded-[8px] border border-border-subtle bg-surface-primary">
              <div className="flex border-b border-border-subtle">
                {(['deposit', 'withdraw'] as const).map((t) => (
                  <button
                    key={t}
                    onClick={() => setTab(t)}
                    className={`relative flex flex-1 items-center justify-center gap-2 py-2.5 text-[12px] font-semibold uppercase tracking-widest transition-colors ${
                      tab === t ? 'text-text-primary' : 'text-text-quaternary hover:text-text-secondary'
                    }`}
                  >
                    {t === 'deposit' ? (
                      <IconArrowDownToArc size={13} stroke={2} />
                    ) : (
                      <IconArrowUpFromArc size={13} stroke={2} />
                    )}
                    {t}
                    {tab === t && (
                      <span className="absolute bottom-0 left-4 right-4 h-[2px] rounded-full bg-text-primary" />
                    )}
                  </button>
                ))}
              </div>
              <div className="p-4">
                <TransferPanel mode={tab} />
              </div>
            </div>

            <div className="rounded-[8px] border border-border-subtle bg-surface-primary">
              <div className="border-b border-border-subtle px-4 py-2.5">
                <span className="text-[10px] uppercase tracking-widest text-text-quaternary">
                  Transaction History
                </span>
              </div>
              <div className="grid grid-cols-[1fr_80px_110px_140px_80px] border-b border-border-subtle px-4 py-2 text-[10px] uppercase tracking-widest text-text-quaternary">
                <span>Type</span>
                <span>Asset</span>
                <span className="text-right">Amount</span>
                <span className="text-right">Time</span>
                <span className="text-right">Status</span>
              </div>
              <div className="divide-y divide-border-subtle/60">
                {HISTORY.map((row, i) => (
                  <div
                    key={i}
                    className="grid grid-cols-[1fr_80px_110px_140px_80px] px-4 py-2.5 text-[12px] tabular-nums"
                  >
                    <span
                      className={`flex items-center gap-1.5 font-medium ${
                        row.type === 'Deposit' ? 'text-bullish-green' : 'text-brand-violet'
                      }`}
                    >
                      {row.type === 'Deposit' ? (
                        <IconArrowDownToArc size={12} stroke={2} />
                      ) : (
                        <IconArrowUpFromArc size={12} stroke={2} />
                      )}
                      {row.type}
                    </span>
                    <span className="font-bold text-text-secondary">{row.asset}</span>
                    <span className="text-right text-text-primary">
                      {row.amount} {row.asset}
                    </span>
                    <span className="text-right text-text-tertiary">{row.time}</span>
                    <span className="text-right text-bullish-green">{row.status}</span>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
