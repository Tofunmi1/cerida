import { useEffect, useState } from 'react'
import { useMarket } from '../../context/market-context'
import { useWallet } from '../../context/wallet-context'
import { getPosition, POSITION_NOT_FOUND, type PositionMeta } from '../../lib/contracts'
import { positionsStore, type StoredPosition } from '../../lib/positions-store'
import { tee } from '../../lib/tee-client'
import { toast } from '../toast/toast-context'
import { formatUsd } from './format'

type Tab = 'Positions' | 'Orders' | 'Trades'

interface LivePosition {
  stored: StoredPosition
  meta: PositionMeta | null | typeof POSITION_NOT_FOUND
}

function statusLabel(status: bigint): { text: string; color: string } {
  switch (Number(status)) {
    case 0: return { text: 'Open', color: 'text-bullish-green' }
    case 1: return { text: 'Matched', color: 'text-brand-violet' }
    case 2: return { text: 'Closed', color: 'text-text-quaternary' }
    case 4: return { text: 'Liquidated', color: 'text-bearish-red' }
    default: return { text: 'Open', color: 'text-bullish-green' }
  }
}

function calcPnl(stored: StoredPosition, markPrice: number): number {
  const side = stored.side === 0 ? 1 : -1
  if (stored.entryPrice === 0) return 0
  return stored.collateral * stored.leverage * side * (markPrice - stored.entryPrice) / stored.entryPrice
}

function calcPnlPct(stored: StoredPosition, markPrice: number): number {
  const side = stored.side === 0 ? 1 : -1
  if (stored.entryPrice === 0) return 0
  return stored.leverage * side * (markPrice - stored.entryPrice) / stored.entryPrice * 100
}

function calcLiqPrice(stored: StoredPosition): number {
  if (stored.entryPrice === 0 || stored.leverage === 0) return 0
  const isLong = stored.side === 0
  return isLong
    ? stored.entryPrice * (1 - 0.92 / stored.leverage)
    : stored.entryPrice * (1 + 0.92 / stored.leverage)
}

export default function PositionsPanel() {
  const { symbolPrices } = useMarket()
  const { connected, publicKey, sign } = useWallet()
  const [tab, setTab] = useState<Tab>('Positions')
  const [positions, setPositions] = useState<LivePosition[]>([])
  const [loading, setLoading] = useState(false)
  const [closing, setClosing] = useState<string | null>(null)

  const handleClose = async (cmt: string, symbol: string) => {
    if (!publicKey) return
    setClosing(cmt)
    const progressId = toast.progress(`Close ${symbol}`, 10, 'Generating cancel proof…')
    try {
      const { proof, nullifier } = await tee.cancelProof(cmt)

      toast.update(progressId, { description: 'TEE relaying cancel + refund…', progress: 50 })
      const { tx_hash } = await tee.relayCancel({
        perp: import.meta.env.VITE_PERP_ENGINE_ID ?? '',
        position_cmt: cmt,
        cancel_nullifier: nullifier,
        cancel_proof: proof,
        recipient: publicKey,
      })

      positionsStore.remove(cmt)
      setPositions((prev) => prev.filter((p) => p.stored.commitment !== cmt))

      toast.update(progressId, {
        type: 'success',
        title: `${symbol} closed`,
        description: 'Collateral returned to your wallet',
        progress: undefined,
        duration: 8000,
        action: {
          label: 'View TX',
          onClick: () => window.open(`https://stellar.expert/explorer/testnet/tx/${tx_hash}`, '_blank', 'noopener'),
        },
      })
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      console.error('close position error:', { cmt, err, msg })
      toast.update(progressId, {
        type: 'error',
        title: 'Close failed',
        description: msg.slice(0, 200),
        progress: undefined,
        duration: 8000,
      })
    } finally {
      setClosing(null)
    }
  }

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
        stored.map(async (s) => {
          const meta = await getPosition(s.commitment, publicKey || undefined)
          return { stored: s, meta }
        })
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

  const pendingOrders = positions.filter((p) => p.stored.orderType === 'limit' && (p.meta === null || p.meta === POSITION_NOT_FOUND))
  const active = positions.filter((p) => p.meta !== POSITION_NOT_FOUND && p.meta !== null && Number(p.meta.status) < 2)

  return (
    <div className="flex h-full flex-col bg-surface-primary">
      {/* Tab bar */}
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
            {t === 'Orders' && pendingOrders.length > 0 && (
              <span className="ml-1.5 rounded-full bg-brand-violet px-1.5 py-0.5 text-[10px] font-bold text-white">
                {pendingOrders.length}
              </span>
            )}
          </button>
        ))}
        {loading && (
          <span className="ml-auto animate-pulse text-[10px] text-text-quaternary">syncing…</span>
        )}
      </div>

      {tab === 'Positions' && (
        <>
          <div className="grid grid-cols-[1fr_80px_80px_80px_80px_80px_70px_60px_64px] shrink-0 border-b border-border-subtle px-3 py-1.5 text-[10px] uppercase tracking-widest text-text-quaternary">
            <span>Market</span>
            <span className="text-right">Entry</span>
            <span className="text-right">Mark</span>
            <span className="text-right">Size</span>
            <span className="text-right">Margin</span>
            <span className="text-right">PnL</span>
            <span className="text-right">Liq.</span>
            <span className="text-right">Status</span>
            <span className="text-right">Action</span>
          </div>

          <div className="min-h-0 flex-1 overflow-auto">
            {!connected ? (
              <div className="flex h-full items-center justify-center text-[12px] text-text-quaternary">
                Connect wallet to see positions
              </div>
            ) : active.length === 0 && !loading ? (
              <div className="flex h-full flex-col items-center justify-center gap-1.5">
                <span className="text-[12px] text-text-quaternary">No open positions</span>
                <span className="text-[11px] text-text-quaternary opacity-60">
                  Open a trade to see your positions here
                </span>
              </div>
            ) : (
              active.map(({ stored, meta }) => {
                const mark = symbolPrices.get(stored.symbol) ?? 0
                const isLong = stored.side === 0
                const pnl = stored.entryPrice > 0 ? calcPnl(stored, mark) : 0
                const pct = stored.entryPrice > 0 ? calcPnlPct(stored, mark) : 0
                const liqPrice = calcLiqPrice(stored)
                const resolvedMeta = meta !== POSITION_NOT_FOUND ? meta : null
                const status = resolvedMeta ? statusLabel(resolvedMeta.status) : null
                const canClose = !resolvedMeta || Number(resolvedMeta.status) === 0 || Number(resolvedMeta.status) === 1
                const cmt = stored.commitment
                const cmtShort = `${cmt.slice(0, 6)}…${cmt.slice(-4)}`

                return (
                  <div key={cmt} className="border-b border-border-subtle/50 last:border-0">
                    <div className="grid grid-cols-[1fr_80px_80px_80px_80px_80px_70px_60px_64px] px-3 py-2 text-[11px] tabular-nums hover:bg-surface-hover/30">
                      <span className="flex items-center gap-1.5">
                        <span className="font-semibold text-text-secondary">{stored.symbol}</span>
                        <span className={`rounded-[3px] px-1 py-0.5 text-[9px] font-bold uppercase leading-none ${isLong ? 'bg-bullish-green/15 text-bullish-green' : 'bg-bearish-red/15 text-bearish-red'}`}>
                          {isLong ? 'Long' : 'Short'} {stored.leverage}×
                        </span>
                      </span>
                      <span className="text-right text-text-tertiary">{stored.entryPrice > 0 ? formatUsd(stored.entryPrice) : '—'}</span>
                      <span className="text-right text-text-tertiary">{mark > 0 ? formatUsd(mark) : '—'}</span>
                      <span className="text-right text-text-tertiary">{stored.size > 0 ? formatUsd(stored.size, 0) : '—'}</span>
                      <span className="text-right text-text-tertiary">{stored.collateral > 0 ? formatUsd(stored.collateral, 0) : '—'}</span>
                      <span className={`text-right font-medium ${stored.entryPrice === 0 ? 'text-text-quaternary' : pnl >= 0 ? 'text-bullish-green' : 'text-bearish-red'}`}>
                        {stored.entryPrice > 0 ? `${pnl >= 0 ? '+' : ''}${formatUsd(pnl)} (${pct >= 0 ? '+' : ''}${pct.toFixed(1)}%)` : '—'}
                      </span>
                      <span className="text-right text-text-quaternary">{liqPrice > 0 ? formatUsd(liqPrice) : '—'}</span>
                      <span className={`text-right text-[10px] font-medium ${resolvedMeta ? status?.color : 'text-text-quaternary animate-pulse'}`}>
                        {resolvedMeta ? status?.text : 'syncing'}
                      </span>
                      <span className="flex items-center justify-end">
                        {canClose ? (
                          <button onClick={() => handleClose(cmt, stored.symbol)} disabled={closing === cmt} className="rounded-[5px] px-2 py-0.5 text-[10px] font-medium text-bearish-red hover:bg-bearish-red/10 disabled:cursor-not-allowed disabled:opacity-50">
                            {closing === cmt ? '…' : 'Close'}
                          </button>
                        ) : (
                          <span className="text-text-quaternary">—</span>
                        )}
                      </span>
                    </div>
                    <div className="flex items-center gap-2 px-3 pb-1.5 text-[10px] text-text-quaternary">
                      <span className="font-mono opacity-60">{cmtShort}</span>
                      <a href={`https://stellar.expert/explorer/testnet/contract/${import.meta.env.VITE_PERP_ENGINE_ID}`} target="_blank" rel="noopener noreferrer" className="opacity-50 hover:opacity-100 hover:text-brand-violet">View on-chain ↗</a>
                    </div>
                  </div>
                )
              })
            )}
          </div>
        </>
      )}

      {tab === 'Orders' && (
        <>
          <div className="grid grid-cols-[1fr_80px_80px_80px_80px_80px] shrink-0 border-b border-border-subtle px-3 py-1.5 text-[10px] uppercase tracking-widest text-text-quaternary">
            <span>Market</span>
            <span className="text-right">Limit</span>
            <span className="text-right">Size</span>
            <span className="text-right">Margin</span>
            <span className="text-right">Status</span>
            <span className="text-right">Action</span>
          </div>
          <div className="min-h-0 flex-1 overflow-auto">
            {!connected ? (
              <div className="flex h-full items-center justify-center text-[12px] text-text-quaternary">
                Connect wallet to see orders
              </div>
            ) : pendingOrders.length === 0 ? (
              <div className="flex h-full flex-col items-center justify-center gap-1.5">
                <span className="text-[12px] text-text-quaternary">No open orders</span>
                <span className="text-[11px] text-text-quaternary opacity-60">Place a limit order to see it here</span>
              </div>
            ) : (
              pendingOrders.map(({ stored }) => {
                const isLong = stored.side === 0
                const cmt = stored.commitment
                const cmtShort = `${cmt.slice(0, 6)}…${cmt.slice(-4)}`
                return (
                  <div key={cmt} className="border-b border-border-subtle/50 last:border-0">
                    <div className="grid grid-cols-[1fr_80px_80px_80px_80px_80px] px-3 py-2 text-[11px] tabular-nums hover:bg-surface-hover/30">
                      <span className="flex items-center gap-1.5">
                        <span className="font-semibold text-text-secondary">{stored.symbol}</span>
                        <span className={`rounded-[3px] px-1 py-0.5 text-[9px] font-bold uppercase leading-none ${isLong ? 'bg-bullish-green/15 text-bullish-green' : 'bg-bearish-red/15 text-bearish-red'}`}>
                          {isLong ? 'Long' : 'Short'} {stored.leverage}×
                        </span>
                      </span>
                      <span className="text-right text-text-tertiary">{stored.limitPrice ? formatUsd(stored.limitPrice) : '—'}</span>
                      <span className="text-right text-text-tertiary">{stored.size > 0 ? formatUsd(stored.size, 0) : '—'}</span>
                      <span className="text-right text-text-tertiary">{stored.collateral > 0 ? formatUsd(stored.collateral, 0) : '—'}</span>
                      <span className="text-right text-[10px] font-medium text-text-quaternary animate-pulse">Pending</span>
                      <span className="flex items-center justify-end">
                        <button
                          onClick={() => { positionsStore.remove(cmt); setPositions(p => p.filter(x => x.stored.commitment !== cmt)) }}
                          className="rounded-[5px] px-2 py-0.5 text-[10px] font-medium text-text-quaternary hover:text-bearish-red hover:bg-bearish-red/10"
                        >
                          Cancel
                        </button>
                      </span>
                    </div>
                    <div className="flex items-center gap-2 px-3 pb-1.5 text-[10px] text-text-quaternary">
                      <span className="font-mono opacity-60">{cmtShort}</span>
                    </div>
                  </div>
                )
              })
            )}
          </div>
        </>
      )}

      {tab === 'Trades' && (
        <div className="flex h-full items-center justify-center text-[12px] text-text-quaternary">
          Trade history coming soon
        </div>
      )}
    </div>
  )
}
