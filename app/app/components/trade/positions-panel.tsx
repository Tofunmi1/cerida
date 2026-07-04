import { useEffect, useState } from 'react'
import { MARKET_CATALOG, useMarket } from '../../context/market-context'
import { useWallet } from '../../context/wallet-context'
import { buildCancelPositionTx, getPosition, proofJsonToScVal, submitAndWait, type PositionMeta } from '../../lib/contracts'
import { positionsStore, type StoredPosition } from '../../lib/positions-store'
import { tee } from '../../lib/tee-client'
import { toast } from '../toast/toast-context'
import { formatUsd } from './format'

const PRICE_SCALE = 1e7
type Tab = 'Positions' | 'Orders' | 'Trades'

interface LivePosition {
  stored: StoredPosition
  meta: PositionMeta | null
}

function statusLabel(status: bigint): { text: string; color: string } {
  switch (Number(status)) {
    case 0: return { text: 'Open', color: 'text-bullish-green' }
    case 1: return { text: 'Matched', color: 'text-brand-violet' }
    case 2: return { text: 'Closed', color: 'text-text-quaternary' }
    case 3: return { text: 'Cancelled', color: 'text-text-quaternary' }
    default: return { text: '—', color: 'text-text-quaternary' }
  }
}

function calcPnl(meta: PositionMeta, markPrice: number): number {
  const entry = Number(meta.entryPrice) / PRICE_SCALE
  const col = Number(meta.effectiveCollateral) / PRICE_SCALE
  const lev = Number(meta.leverage)
  const side = Number(meta.side) === 0 ? 1 : -1
  if (entry === 0) return 0
  return col * lev * side * (markPrice - entry) / entry
}

function pnlPct(meta: PositionMeta, markPrice: number): number {
  const entry = Number(meta.entryPrice) / PRICE_SCALE
  const lev = Number(meta.leverage)
  const side = Number(meta.side) === 0 ? 1 : -1
  if (entry === 0) return 0
  return lev * side * (markPrice - entry) / entry * 100
}

function markForSymbol(symbol: string, allPrices: Map<string, number>): number {
  const market = MARKET_CATALOG.find((m) => m.symbol === symbol)
  if (!market?.pythId) return 0
  return allPrices.get(market.pythId) ?? 0
}

export default function PositionsPanel() {
  const { allPrices } = useMarket()
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
      toast.update(progressId, { description: 'Generating refund note…', progress: 50 })

      const settleSecret = Math.floor(Math.random() * Number.MAX_SAFE_INTEGER)
      const recipient = await tee.noteCmt(0, settleSecret)

      toast.update(progressId, { description: 'Sign — close position…', progress: 70 })
      const tx = await buildCancelPositionTx(publicKey, {
        positionCmt: cmt,
        cancelNullifier: nullifier,
        recipientNote: recipient.note_cmt,
        cancelProof: proofJsonToScVal(proof),
      })
      const closeTxHash = await submitAndWait(await sign(tx.toXDR()))

      const notes = JSON.parse(localStorage.getItem('cerida-notes') ?? '[]')
      notes.push({
        note_cmt: recipient.note_cmt,
        secret: settleSecret,
        amount: 0,
        depositedAt: Date.now(),
        source: 'close',
        fromCmt: cmt,
      })
      localStorage.setItem('cerida-notes', JSON.stringify(notes))

      positionsStore.remove(cmt)
      setPositions((prev) => prev.filter((p) => p.stored.commitment !== cmt))

      toast.update(progressId, {
        type: 'success',
        title: `${symbol} closed`,
        description: 'Collateral refunded to shielded note',
        progress: undefined,
        duration: 5000,
      })
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      toast.update(progressId, {
        type: 'error',
        title: 'Close failed',
        description: msg.slice(0, 120),
        progress: undefined,
        duration: 6000,
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

  const active = positions.filter((p) => !p.meta || Number(p.meta.status) < 2)

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
          </button>
        ))}
        {loading && (
          <span className="ml-auto animate-pulse text-[10px] text-text-quaternary">syncing…</span>
        )}
      </div>

      {tab === 'Positions' && (
        <>
          {/* Column headers */}
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
                const mark = markForSymbol(stored.symbol, allPrices)
                const entry = meta ? Number(meta.entryPrice) / PRICE_SCALE : 0
                const col = meta ? Number(meta.effectiveCollateral) / PRICE_SCALE : 0
                const lev = meta ? Number(meta.leverage) : stored.leverage
                const isLong = meta ? Number(meta.side) === 0 : stored.side === 0
                const notional = col * lev
                const pnl = meta ? calcPnl(meta, mark) : 0
                const pct = meta ? pnlPct(meta, mark) : 0
                const liqPrice = entry > 0
                  ? isLong
                    ? entry * (1 - 0.92 / (lev || 1))
                    : entry * (1 + 0.92 / (lev || 1))
                  : 0
                const status = meta ? statusLabel(meta.status) : null
                const cmt = stored.commitment
                const cmtShort = `${cmt.slice(0, 6)}…${cmt.slice(-4)}`
                const canClose = meta && Number(meta.status) === 0

                return (
                  <div key={cmt} className="border-b border-border-subtle/50 last:border-0">
                    <div className="grid grid-cols-[1fr_80px_80px_80px_80px_80px_70px_60px_64px] px-3 py-2 text-[11px] tabular-nums hover:bg-surface-hover/30">
                      {/* Market + side badge */}
                      <span className="flex items-center gap-1.5">
                        <span className="font-semibold text-text-secondary">{stored.symbol}</span>
                        <span
                          className={`rounded-[3px] px-1 py-0.5 text-[9px] font-bold uppercase leading-none ${
                            isLong
                              ? 'bg-bullish-green/15 text-bullish-green'
                              : 'bg-bearish-red/15 text-bearish-red'
                          }`}
                        >
                          {isLong ? 'Long' : 'Short'} {lev}×
                        </span>
                      </span>

                      <span className="text-right text-text-tertiary">
                        {entry > 0 ? formatUsd(entry) : '—'}
                      </span>
                      <span className="text-right text-text-tertiary">
                        {meta ? formatUsd(mark) : '—'}
                      </span>
                      <span className="text-right text-text-tertiary">
                        {notional > 0 ? formatUsd(notional, 0) : '—'}
                      </span>
                      <span className="text-right text-text-tertiary">
                        {col > 0 ? formatUsd(col, 0) : '—'}
                      </span>
                      <span
                        className={`text-right font-medium ${
                          !meta ? 'text-text-quaternary' : pnl >= 0 ? 'text-bullish-green' : 'text-bearish-red'
                        }`}
                      >
                        {meta
                          ? `${pnl >= 0 ? '+' : ''}${formatUsd(pnl)} (${pct >= 0 ? '+' : ''}${pct.toFixed(1)}%)`
                          : '—'}
                      </span>
                      <span className="text-right text-text-quaternary">
                        {liqPrice > 0 ? formatUsd(liqPrice) : '—'}
                      </span>
                      <span className={`text-right text-[10px] font-medium ${status?.color ?? 'text-text-quaternary animate-pulse'}`}>
                        {status?.text ?? 'syncing'}
                      </span>
                      <span className="flex items-center justify-end">
                        {canClose ? (
                          <button
                            onClick={() => handleClose(cmt, stored.symbol)}
                            disabled={closing === cmt}
                            className="rounded-[5px] px-2 py-0.5 text-[10px] font-medium text-bearish-red hover:bg-bearish-red/10 disabled:cursor-not-allowed disabled:opacity-50"
                          >
                            {closing === cmt ? '…' : 'Close'}
                          </button>
                        ) : (
                          <span className="text-text-quaternary">—</span>
                        )}
                      </span>
                    </div>

                    {/* Sub-row: commitment hash + Stellar Expert link */}
                    <div className="flex items-center gap-2 px-3 pb-1.5 text-[10px] text-text-quaternary">
                      <span className="font-mono opacity-60">{cmtShort}</span>
                      <a
                        href={`https://stellar.expert/explorer/testnet/contract/${import.meta.env.VITE_PERP_ENGINE_ID}`}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="opacity-50 hover:opacity-100 hover:text-brand-violet"
                      >
                        View on-chain ↗
                      </a>
                    </div>
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
