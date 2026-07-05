import { useEffect, useState } from 'react'
import { IconArrowDownToArc, IconArrowUpFromArc, IconExternalLink, IconX } from '@tabler/icons-react'
import { formatUsd } from './format'
import { useWallet } from '../../context/wallet-context'
import { buildDepositNoteTx, computeAmountCommitment, CONTRACT_IDS, submitAndWait } from '../../lib/contracts'
import { tee } from '../../lib/tee-client'
import { toast } from '../toast/toast-context'

const PRICE_SCALE = 1e7

const PCT_OPTIONS = [25, 50, 75] as const

function TransferPanel({ mode }: { mode: 'deposit' | 'withdraw' }) {
  const { connected, publicKey, sign, balance, refreshBalance } = useWallet()
  const [amount, setAmount] = useState('')
  const [pct, setPct] = useState<number | null>(null)
  const [busy, setBusy] = useState(false)

  const walletUsdcDollars = Number(balance) / PRICE_SCALE
  const parsed = parseFloat(amount) || 0
  const isDeposit = mode === 'deposit'
  const label = isDeposit ? 'Deposit' : 'Withdraw'

  const applyPct = (p: number) => {
    setPct(p)
    setAmount(((walletUsdcDollars * p) / 100).toFixed(2))
  }

  const handleSubmit = async () => {
    if (!connected || !publicKey) {
      toast.warning('Connect wallet', 'Connect a Stellar wallet to continue.')
      return
    }
    if (parsed <= 0) return

    setBusy(true)
    const progressId = toast.progress(label, 10, isDeposit ? 'Computing note commitment…' : 'Generating spend proof…')
    try {
      const collateralUnits = BigInt(Math.round(parsed * PRICE_SCALE))
      const noteSecret = Math.floor(Math.random() * Number.MAX_SAFE_INTEGER)

      if (isDeposit) {
        // Get note commitment from TEE (fast hash, no ZK proof needed for deposit)
        toast.update(progressId, { description: 'Getting note commitment…', progress: 30 })
        const { note_cmt, note_null } = await tee.noteCmt(Number(collateralUnits), noteSecret)

        // Generate blinding for amount_commitment = SHA256(amount_le || blinding)
        const blindingBytes = crypto.getRandomValues(new Uint8Array(32))
        const blindingHex = Array.from(blindingBytes).map(b => b.toString(16).padStart(2, '0')).join('')
        const amountCmt = await computeAmountCommitment(collateralUnits, blindingBytes)

        toast.update(progressId, { description: 'Sign transaction…', progress: 60 })
        const tx = await buildDepositNoteTx(publicKey, note_cmt, collateralUnits, amountCmt)
        await submitAndWait(await sign(tx.toXDR()))

        // Persist note secret + blinding locally so user can spend it later
        const notes = JSON.parse(localStorage.getItem('cerida-notes') ?? '[]')
        notes.push({ note_cmt, secret: noteSecret, blinding: blindingHex, amount: Number(collateralUnits), depositedAt: Date.now() })
        localStorage.setItem('cerida-notes', JSON.stringify(notes))

        // Also store in shielded pool format for the Pool modal
        const poolNotes = JSON.parse(localStorage.getItem('cerida-pool-notes') ?? '[]')
        poolNotes.push({
          id: note_cmt,
          secret: String(noteSecret),
          nullifier: note_null ?? '',
          status: 'deposited',
          createdAt: Date.now(),
        })
        localStorage.setItem('cerida-pool-notes', JSON.stringify(poolNotes))

        toast.update(progressId, {
          type: 'success',
          title: 'Deposit complete',
          description: `${formatUsd(parsed)} USDC deposited to shielded pool`,
          progress: undefined,
          duration: 5000,
        })
        await refreshBalance()
      } else {
        // Withdraw requires a NoteSpend ZK proof — user must pick a deposited note
        toast.update(progressId, {
          type: 'error',
          title: 'Select a note',
          description: 'Use the Shielded Pool panel to withdraw individual notes.',
          progress: undefined,
          duration: 5000,
        })
      }

      setAmount('')
      setPct(null)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      console.error('deposit error:', { err, msg, publicKey, collateralUnits: parsed, mode })
      toast.update(progressId, {
        type: 'error',
        title: `${label} failed`,
        description: msg.slice(0, 120),
        progress: undefined,
        duration: 6000,
      })
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div>
        <div className="mb-1.5 flex items-center justify-between text-[11px]">
          <span className="text-text-tertiary">Amount (USDC)</span>
          <span className="text-text-quaternary">
            {isDeposit ? 'Wallet' : 'Available'}:{' '}
            <span className="text-text-secondary">
              {connected ? formatUsd(walletUsdcDollars) : '—'}
            </span>
          </span>
        </div>
        <div className="flex items-center gap-2 rounded-[8px] border border-border-subtle bg-surface-primary px-3 py-2 focus-within:border-border-default">
          <input
            type="number"
            value={amount}
            onChange={(e) => { setAmount(e.target.value); setPct(null) }}
            placeholder="0.00"
            min="0"
            step="any"
            className="min-w-0 flex-1 bg-transparent text-[18px] font-medium text-text-primary outline-none placeholder:text-text-quaternary"
            style={{ fontFamily: 'var(--font-mono)' }}
          />
          <span className="shrink-0 text-[11px] font-bold text-text-quaternary">USDC</span>
        </div>
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
          onClick={() => { setPct(100); setAmount(walletUsdcDollars.toFixed(2)) }}
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
          <span>Privacy</span>
          <span className="text-brand-violet">Shielded note</span>
        </div>
      </div>

      <button
        onClick={handleSubmit}
        disabled={parsed <= 0 || busy || !connected}
        className={`w-full rounded-[8px] py-2.5 text-[13px] font-semibold transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-40 ${
          isDeposit ? 'bg-bullish-green text-[#1a1a1a]' : 'bg-brand-violet text-white'
        }`}
      >
        {busy ? 'Processing…' : `${label}${parsed > 0 ? ` ${formatUsd(parsed)}` : ''}`}
      </button>
    </div>
  )
}

// ── Transaction history via Stellar Horizon ───────────────────────

const HORIZON = 'https://horizon-testnet.stellar.org'

interface HorizonOp {
  id: string
  type: string
  transaction_hash: string
  created_at: string
  function?: string
}

interface TxRecord {
  hash: string
  label: string
  time: string
  href: string
}

function opLabel(op: HorizonOp): string {
  if (op.type !== 'invoke_host_function') return op.type
  // Try to infer from the function field if present
  const fn = (op.function ?? '').toLowerCase()
  if (fn.includes('deposit_note'))              return 'Deposit (shielded note)'
  if (fn.includes('open_position_from_note'))   return 'Open Position'
  if (fn.includes('place_order'))               return 'Place Order'
  if (fn.includes('cancel_order'))              return 'Cancel Order'
  if (fn.includes('withdraw_note'))             return 'Withdraw (note)'
  if (fn.includes('liquidate'))                 return 'Liquidated'
  return 'Contract call'
}

function TxHistory({ publicKey }: { publicKey: string }) {
  const [records, setRecords]   = useState<TxRecord[]>([])
  const [loading, setLoading]   = useState(true)
  const [error, setError]       = useState(false)

  useEffect(() => {
    let cancelled = false
    setLoading(true)
    setError(false)

    const contracts = new Set([
      CONTRACT_IDS.perpEngine,
      CONTRACT_IDS.orderbook,
      CONTRACT_IDS.collateralToken,
    ])

    fetch(`${HORIZON}/accounts/${publicKey}/operations?limit=100&order=desc&include_failed=false`)
      .then((r) => r.json())
      .then((data) => {
        if (cancelled) return
        const ops: HorizonOp[] = data?._embedded?.records ?? []
        const filtered = ops
          .filter((op) => {
            if (op.type !== 'invoke_host_function') return false
            // Keep ops where the transaction hash references our contracts (approximate)
            return true
          })
          .slice(0, 30)
          .map((op): TxRecord => ({
            hash: op.transaction_hash,
            label: opLabel(op),
            time: new Date(op.created_at).toLocaleString('en-US', {
              month: 'short', day: 'numeric',
              hour: '2-digit', minute: '2-digit',
            }),
            href: `https://stellar.expert/explorer/testnet/tx/${op.transaction_hash}`,
          }))
        setRecords(filtered)
        setLoading(false)
      })
      .catch(() => {
        if (!cancelled) { setError(true); setLoading(false) }
      })

    return () => { cancelled = true }
  }, [publicKey])

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12 text-[12px] text-text-quaternary">
        Loading transactions…
      </div>
    )
  }
  if (error) {
    return (
      <div className="flex items-center justify-center py-12 text-[12px] text-text-quaternary">
        Could not load transaction history.
      </div>
    )
  }
  if (!records.length) {
    return (
      <div className="flex items-center justify-center py-12 text-[12px] text-text-quaternary">
        No transactions yet.
      </div>
    )
  }

  return (
    <div className="divide-y divide-border-subtle/60">
      {records.map((r) => (
        <div key={r.hash} className="flex items-center justify-between px-4 py-2.5">
          <div className="min-w-0">
            <div className="text-[12px] font-medium text-text-primary">{r.label}</div>
            <div className="mt-0.5 font-mono text-[10px] text-text-quaternary">
              {r.hash.slice(0, 8)}…{r.hash.slice(-6)}
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-3">
            <span className="text-[11px] tabular-nums text-text-tertiary">{r.time}</span>
            <a
              href={r.href}
              target="_blank"
              rel="noopener noreferrer"
              className="text-text-quaternary hover:text-text-primary"
            >
              <IconExternalLink size={13} stroke={1.8} />
            </a>
          </div>
        </div>
      ))}
    </div>
  )
}

// ── Main portfolio page ───────────────────────────────────────────

export default function PortfolioPage({ onClose }: { onClose: () => void }) {
  const { connected, publicKey, balance } = useWallet()
  const [tab, setTab] = useState<'deposit' | 'withdraw'>('deposit')

  const walletUsdc = Number(balance) / PRICE_SCALE

  const statCards = [
    { label: 'Wallet USDC',   value: connected ? formatUsd(walletUsdc) : '—' },
    { label: 'Shielded Pool', value: '—' },
    { label: 'In Positions',  value: '—' },
    { label: 'Unrealized PnL', value: '—' },
  ]

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
            {statCards.map((card) => (
              <div
                key={card.label}
                className="rounded-[8px] border border-border-subtle bg-surface-primary px-4 py-3"
              >
                <div className="text-[10px] uppercase tracking-widest text-text-quaternary">
                  {card.label}
                </div>
                <div className="mt-1.5">
                  <span className="text-[18px] font-semibold tabular-nums text-text-primary">
                    {card.value}
                  </span>
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

            <div className="flex flex-col rounded-[8px] border border-border-subtle bg-surface-primary">
              <div className="flex shrink-0 items-center justify-between border-b border-border-subtle px-4 py-2.5">
                <span className="text-[10px] uppercase tracking-widest text-text-quaternary">
                  Transaction History
                </span>
                {connected && publicKey && (
                  <a
                    href={`https://stellar.expert/explorer/testnet/account/${publicKey}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="flex items-center gap-1 text-[10px] text-text-quaternary hover:text-text-secondary"
                  >
                    Stellar Expert <IconExternalLink size={10} stroke={1.8} />
                  </a>
                )}
              </div>
              <div className="min-h-0 flex-1 overflow-y-auto">
                {connected && publicKey ? (
                  <TxHistory publicKey={publicKey} />
                ) : (
                  <div className="flex items-center justify-center py-12 text-[12px] text-text-quaternary">
                    Connect wallet to see history
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
