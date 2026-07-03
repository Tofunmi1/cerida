import { useEffect, useState } from 'react'
import { useMarket } from '../../context/market-context'
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
      toast.update(progressId, { description: 'Generating cancel proof…', progress: 30 })
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
      await submitAndWait(await sign(tx.toXDR()))

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
        description: 'Collateral refunded to a shielded note',
        progress: undefined,
        duration: 5000,
      })
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      console.error('close error:', { err, msg, cmt })
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
      console.log('positions-panel: stored from localStorage:', stored.length, stored.map(s => s.commitment.slice(0, 12)))
      const results = await Promise.all(
        stored.map(async (s) => {
          const meta = await getPosition(s.commitment, publicKey || undefined)
          console.log('positions-panel: getPosition for', s.commitment.slice(0,12), '->', meta ? 'found' : 'null')
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

  const active = positions.filter(
    (p) => !p.meta || Number(p.meta.status) < 2
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
          <div className="grid grid-cols-[1fr_90px_90px_90px_90px_90px_90px_70px_70px] border-b border-border-subtle px-3 py-2 text-[10px] uppercase tracking-widest text-text-quaternary">
            <span>Market</span>
            <span>Side</span>
            <span className="text-right">Entry</span>
            <span className="text-right">Mark</span>
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
              <div className="flex h-full items-center justify-center text-[12px] text-text-quaternary">
                No open positions
              </div>
            ) : (
              active.map(({ stored, meta }) => {
                const entry = meta ? Number(meta.entryPrice) / PRICE_SCALE : 0
                const col = meta ? Number(meta.effectiveCollateral) / PRICE_SCALE : 0
                const lev = meta ? Number(meta.leverage) : stored.leverage
                const isLong = meta ? Number(meta.side) === 0 : stored.side === 0
                const pnl = meta ? calcPnl(meta, mark) : 0
                const liqPrice = isLong
                  ? entry * (1 - 0.92 / (lev || 1))
                  : entry * (1 + 0.92 / (lev || 1))
                const statusNum = meta ? Number(meta.status) : 0

                return (
                  <div
                    key={stored.commitment}
                    className="grid grid-cols-[1fr_90px_90px_90px_90px_90px_90px_70px_70px] border-b border-border-subtle/60 px-3 py-2 text-[11px] tabular-nums"
                  >
                    <span className="font-semibold text-text-secondary">{stored.symbol}</span>
                    <span className={isLong ? 'text-bullish-green' : 'text-bearish-red'}>
                      {isLong ? 'Long' : 'Short'} {lev}x
                    </span>
                    <span className="text-right text-text-tertiary">{meta ? formatUsd(entry) : '—'}</span>
                    <span className="text-right text-text-tertiary">{meta ? formatUsd(mark) : '—'}</span>
                    <span className="text-right text-text-tertiary">{meta ? formatUsd(col, 0) : '—'}</span>
                    <span
                      className={`text-right font-medium ${pnl >= 0 ? 'text-bullish-green' : 'text-bearish-red'}`}
                    >
                      {meta ? (pnl >= 0 ? '+' : '') + formatUsd(pnl) : '—'}
                    </span>
                    <span className="text-right text-text-quaternary">{meta ? formatUsd(liqPrice) : '—'}</span>
                    <span className="text-right text-text-quaternary">
                      {meta ? statusLabel(meta.status) : 'syncing…'}
                    </span>
                    <span className="flex items-center justify-end">
                      {statusNum === 0 ? (
                        <button
                          onClick={() => handleClose(stored.commitment, stored.symbol)}
                          disabled={closing === stored.commitment}
                          className="rounded-[5px] px-2 py-0.5 text-[10px] font-medium text-bearish-red hover:bg-bearish-red/10 disabled:cursor-not-allowed disabled:opacity-50"
                        >
                          {closing === stored.commitment ? 'Closing…' : 'Close'}
                        </button>
                      ) : (
                        <span className="text-text-quaternary">—</span>
                      )}
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
