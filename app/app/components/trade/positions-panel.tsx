import { useEffect, useState } from 'react'
import { useMarket } from '../../context/market-context'
import { useWallet } from '../../context/wallet-context'
import { getPosition, type PositionMeta } from '../../lib/contracts'
import { positionsStore, type StoredPosition } from '../../lib/positions-store'
import { formatUsd } from './format'

const PRICE_SCALE = 1e7
type Tab = 'Positions' | 'Orders' | 'Trades'

interface LivePosition {
  stored: StoredPosition
  meta: PositionMeta | null
}

function statusLabel(status: bigint): string {
  switch (Number(status)) {
    case 0: return 'Open'
    case 1: return 'Matched'
    case 2: return 'Closed'
    case 3: return 'Cancelled'
    default: return '—'
  }
}

function calcPnl(meta: PositionMeta, markPrice: number): number {
  const entry = Number(meta.entryPrice) / PRICE_SCALE
  const col = Number(meta.effectiveCollateral) / PRICE_SCALE
  const lev = Number(meta.leverage)
  const side = Number(meta.side) === 0 ? 1 : -1
  return col * lev * side * (markPrice - entry) / entry
}

export default function PositionsPanel() {
  const { mark } = useMarket()
  const { connected, publicKey } = useWallet()
  const [tab, setTab] = useState<Tab>('Positions')
  const [positions, setPositions] = useState<LivePosition[]>([])
  const [loading, setLoading] = useState(false)

  useEffect(() => {
    if (!connected || !publicKey) {
      setPositions([])
      return
    }

    let cancelled = false

    async function fetchAll() {
      setLoading(true)
      const stored = positionsStore.forWallet(publicKey!)
      const results = await Promise.all(
        stored.map(async (s) => ({ stored: s, meta: await getPosition(s.commitment) }))
      )
      if (!cancelled) setPositions(results)
      setLoading(false)
    }

    fetchAll()
    const id = window.setInterval(fetchAll, 15_000)
    return () => {
      cancelled = true
      window.clearInterval(id)
    }
  }, [connected, publicKey])

  const active = positions.filter(
    (p) => p.meta && Number(p.meta.status) < 2
  )

  return (
    <div className="flex h-full flex-col bg-surface-primary">
      <div className="flex shrink-0 items-center gap-4 border-b border-border-subtle px-3 py-2">
        {(['Positions', 'Orders', 'Trades'] as Tab[]).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={`text-[12px] font-semibold transition-colors ${
              tab === t ? 'text-text-primary' : 'text-text-quaternary hover:text-text-secondary'
            }`}
          >
            {t}
            {t === 'Positions' && active.length > 0 && (
              <span className="ml-1.5 rounded-full bg-brand-violet px-1.5 py-0.5 text-[10px] font-bold text-white">
                {active.length}
              </span>
            )}
          </button>
        ))}
        {loading && (
          <span className="ml-auto text-[10px] text-text-quaternary animate-pulse">
            syncing…
          </span>
        )}
      </div>

      {tab === 'Positions' && (
        <>
          <div className="grid grid-cols-[1fr_90px_90px_90px_90px_90px_90px_70px] border-b border-border-subtle px-3 py-2 text-[10px] uppercase tracking-widest text-text-quaternary">
            <span>Market</span>
            <span>Side</span>
            <span className="text-right">Entry</span>
            <span className="text-right">Mark</span>
            <span className="text-right">Margin</span>
            <span className="text-right">PnL</span>
            <span className="text-right">Liq.</span>
            <span className="text-right">Status</span>
          </div>

          <div className="min-h-0 flex-1 overflow-auto">
            {!connected ? (
              <div className="flex h-full items-center justify-center text-[12px] text-text-quaternary">
                Connect wallet to see positions
              </div>
            ) : active.length === 0 && !loading ? (
              <div className="flex h-full items-center justify-center text-[12px] text-text-quaternary">
                No open positions
              </div>
            ) : (
              active.map(({ stored, meta }) => {
                if (!meta) return null
                const entry = Number(meta.entryPrice) / PRICE_SCALE
                const col = Number(meta.effectiveCollateral) / PRICE_SCALE
                const lev = Number(meta.leverage)
                const isLong = Number(meta.side) === 0
                const pnl = calcPnl(meta, mark)
                const liqPrice = isLong
                  ? entry * (1 - 0.92 / lev)
                  : entry * (1 + 0.92 / lev)

                return (
                  <div
                    key={stored.commitment}
                    className="grid grid-cols-[1fr_90px_90px_90px_90px_90px_90px_70px] border-b border-border-subtle/60 px-3 py-2 text-[11px] tabular-nums"
                  >
                    <span className="font-semibold text-text-secondary">{stored.symbol}</span>
                    <span className={isLong ? 'text-bullish-green' : 'text-bearish-red'}>
                      {isLong ? 'Long' : 'Short'} {lev}x
                    </span>
                    <span className="text-right text-text-tertiary">{formatUsd(entry)}</span>
                    <span className="text-right text-text-tertiary">{formatUsd(mark)}</span>
                    <span className="text-right text-text-tertiary">{formatUsd(col, 0)}</span>
                    <span
                      className={`text-right font-medium ${pnl >= 0 ? 'text-bullish-green' : 'text-bearish-red'}`}
                    >
                      {pnl >= 0 ? '+' : ''}{formatUsd(pnl)}
                    </span>
                    <span className="text-right text-text-quaternary">{formatUsd(liqPrice)}</span>
                    <span className="text-right text-text-quaternary">
                      {statusLabel(meta.status)}
                    </span>
                  </div>
                )
              })
            )}
          </div>
        </>
      )}

      {tab === 'Orders' && (
        <div className="flex h-full items-center justify-center text-[12px] text-text-quaternary">
          Open orders appear here once matched on-chain
        </div>
      )}

      {tab === 'Trades' && (
        <div className="flex h-full items-center justify-center text-[12px] text-text-quaternary">
          Trade history coming soon
        </div>
      )}
    </div>
  )
}
