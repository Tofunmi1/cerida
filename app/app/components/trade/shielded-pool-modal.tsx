import { useCallback, useState } from 'react'
import {
  IconCheck,
  IconCopy,
  IconPlus,
  IconShield,
  IconTrash,
  IconX,
} from '@tabler/icons-react'

// ── Note storage ──────────────────────────────────────────────────────────────

const NOTES_KEY = 'cerida-pool-notes'

interface PoolNote {
  id: string
  secret: string
  nullifier: string
  status: 'generated' | 'deposited' | 'withdrawn'
  createdAt: number
}

function loadNotes(): PoolNote[] {
  try {
    return JSON.parse(localStorage.getItem(NOTES_KEY) ?? '[]')
  } catch {
    return []
  }
}

function saveNotes(notes: PoolNote[]) {
  localStorage.setItem(NOTES_KEY, JSON.stringify(notes))
}

function randomU64Decimal(): string {
  const arr = new Uint8Array(8)
  crypto.getRandomValues(arr)
  // Read as u64 — stay within safe JS integer range by using high 6 bytes only
  let v = 0n
  for (const b of arr.slice(0, 6)) v = (v << 8n) | BigInt(b)
  return v.toString()
}

// ── Copy button ───────────────────────────────────────────────────────────────

function CopyButton({ text, className }: { text: string; className?: string }) {
  const [copied, setCopied] = useState(false)
  const copy = () => {
    navigator.clipboard.writeText(text).catch(() => {})
    setCopied(true)
    setTimeout(() => setCopied(false), 1800)
  }
  return (
    <button
      onClick={copy}
      title="Copy"
      className={`grid h-6 w-6 shrink-0 place-items-center rounded-[5px] text-text-tertiary transition-colors hover:bg-surface-hover hover:text-text-primary ${className ?? ''}`}
    >
      {copied ? <IconCheck size={13} stroke={2.5} /> : <IconCopy size={13} stroke={2} />}
    </button>
  )
}

// ── Code block ────────────────────────────────────────────────────────────────

function CodeBlock({ code }: { code: string }) {
  return (
    <div className="relative rounded-[8px] border border-border-subtle bg-page px-3 py-2.5">
      <pre
        className="overflow-x-auto whitespace-pre-wrap break-all text-[11px] leading-relaxed text-text-secondary"
        style={{ fontFamily: 'var(--font-mono)' }}
      >
        {code}
      </pre>
      <CopyButton text={code} className="absolute right-2 top-2" />
    </div>
  )
}

// ── Note badge ────────────────────────────────────────────────────────────────

const STATUS_STYLE: Record<PoolNote['status'], string> = {
  generated: 'bg-surface-card text-text-tertiary',
  deposited: 'bg-brand-violet/15 text-brand-violet',
  withdrawn: 'bg-bullish-green/10 text-bullish-green',
}

function StatusBadge({ status }: { status: PoolNote['status'] }) {
  return (
    <span className={`rounded-[4px] px-1.5 py-0.5 text-[10px] font-semibold ${STATUS_STYLE[status]}`}>
      {status}
    </span>
  )
}

// ── Deposit tab ───────────────────────────────────────────────────────────────

function DepositTab({
  notes,
  onNotesChange,
}: {
  notes: PoolNote[]
  onNotesChange: (n: PoolNote[]) => void
}) {
  const [latest, setLatest] = useState<PoolNote | null>(null)

  const generate = useCallback(() => {
    const note: PoolNote = {
      id: crypto.randomUUID(),
      secret: randomU64Decimal(),
      nullifier: randomU64Decimal(),
      status: 'generated',
      createdAt: Date.now(),
    }
    const next = [note, ...notes]
    saveNotes(next)
    onNotesChange(next)
    setLatest(note)
  }, [notes, onNotesChange])

  const cliCommand = latest
    ? `cargo run --release --manifest-path tools/e2e/Cargo.toml -- \\\n  shielded-pool \\\n  --denomination 1000000 \\\n  --pool-secret ${latest.secret} \\\n  --pool-nullifier ${latest.nullifier}`
    : null

  return (
    <div className="flex flex-col gap-4">
      <div className="rounded-[10px] border border-border-subtle bg-surface-primary p-4">
        <p className="text-[12px] leading-relaxed text-text-secondary">
          A <strong className="text-text-primary">shielded deposit</strong> mixes your USDC into a
          Merkle-tree pool. You receive a secret note — anyone holding the note can later withdraw
          to any address, breaking the on-chain link between sender and recipient.
        </p>
      </div>

      <div>
        <p className="mb-2 text-[11px] uppercase tracking-widest text-text-tertiary">
          Step 1 — Generate a note
        </p>
        <button
          onClick={generate}
          className="flex items-center gap-2 rounded-[8px] bg-brand-violet px-4 py-2 text-[13px] font-semibold text-white transition-opacity hover:opacity-90"
        >
          <IconPlus size={15} stroke={2.5} />
          New note
        </button>
      </div>

      {latest && (
        <div className="flex flex-col gap-3">
          <p className="text-[11px] uppercase tracking-widest text-text-tertiary">
            Step 2 — Save your note (you need it to withdraw)
          </p>
          <div className="grid grid-cols-2 gap-2">
            {(
              [
                ['Secret', latest.secret],
                ['Nullifier', latest.nullifier],
              ] as const
            ).map(([label, val]) => (
              <div
                key={label}
                className="flex items-center justify-between rounded-[8px] border border-border-subtle bg-page px-3 py-2"
              >
                <div className="min-w-0">
                  <div className="text-[10px] uppercase tracking-widest text-text-quaternary">
                    {label}
                  </div>
                  <div
                    className="mt-0.5 truncate text-[12px] font-medium text-text-primary"
                    style={{ fontFamily: 'var(--font-mono)' }}
                  >
                    {val}
                  </div>
                </div>
                <CopyButton text={val} />
              </div>
            ))}
          </div>

          <div>
            <p className="mb-1.5 text-[11px] uppercase tracking-widest text-text-tertiary">
              Step 3 — Run the CLI to deposit
            </p>
            <CodeBlock code={cliCommand!} />
            <p className="mt-1.5 text-[11px] text-text-quaternary">
              This generates the ZK proof and submits the deposit transaction.
            </p>
          </div>
        </div>
      )}
    </div>
  )
}

// ── Withdraw tab ──────────────────────────────────────────────────────────────

function WithdrawTab({
  notes,
  onNotesChange,
}: {
  notes: PoolNote[]
  onNotesChange: (n: PoolNote[]) => void
}) {
  const eligible = notes.filter((n) => n.status === 'deposited')
  const [selected, setSelected] = useState<string>(eligible[0]?.id ?? '')

  const note = notes.find((n) => n.id === selected) ?? null

  const markWithdrawn = () => {
    if (!note) return
    const next = notes.map((n) => (n.id === note.id ? { ...n, status: 'withdrawn' as const } : n))
    saveNotes(next)
    onNotesChange(next)
    setSelected('')
  }

  const cliCommand = note
    ? `cargo run --release --manifest-path tools/e2e/Cargo.toml -- \\\n  shielded-pool \\\n  --denomination 1000000 \\\n  --pool-secret ${note.secret} \\\n  --pool-nullifier ${note.nullifier}`
    : null

  if (eligible.length === 0) {
    return (
      <div className="flex flex-col items-center gap-3 py-10 text-center">
        <IconShield size={32} stroke={1.5} className="text-text-quaternary" />
        <p className="text-[13px] text-text-tertiary">No deposited notes yet.</p>
        <p className="text-[12px] text-text-quaternary">
          Deposit first, then mark a note as deposited from the Notes tab.
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-4">
      <div>
        <p className="mb-2 text-[11px] uppercase tracking-widest text-text-tertiary">
          Select note to withdraw
        </p>
        <div className="flex flex-col gap-1.5">
          {eligible.map((n) => (
            <button
              key={n.id}
              onClick={() => setSelected(n.id)}
              className={`flex items-center justify-between rounded-[8px] border px-3 py-2.5 text-left transition-colors ${
                selected === n.id
                  ? 'border-brand-violet/50 bg-brand-violet/10'
                  : 'border-border-subtle bg-surface-primary hover:bg-surface-hover'
              }`}
            >
              <div>
                <div
                  className="text-[12px] font-medium text-text-primary"
                  style={{ fontFamily: 'var(--font-mono)' }}
                >
                  Secret: {n.secret.slice(0, 10)}…
                </div>
                <div className="mt-0.5 text-[11px] text-text-quaternary">
                  {new Date(n.createdAt).toLocaleDateString()}
                </div>
              </div>
              <StatusBadge status={n.status} />
            </button>
          ))}
        </div>
      </div>

      {note && (
        <>
          <div>
            <p className="mb-1.5 text-[11px] uppercase tracking-widest text-text-tertiary">
              Run the CLI to withdraw
            </p>
            <CodeBlock code={cliCommand!} />
            <p className="mt-1.5 text-[11px] text-text-quaternary">
              The CLI generates the ZK membership proof and submits the withdrawal. USDC is sent to
              the recipient address without revealing which deposit it came from.
            </p>
          </div>

          <button
            onClick={markWithdrawn}
            className="rounded-[8px] border border-bullish-green/40 bg-bullish-green/10 py-2 text-[13px] font-semibold text-bullish-green transition-opacity hover:opacity-80"
          >
            Mark as withdrawn
          </button>
        </>
      )}
    </div>
  )
}

// ── Notes list tab ────────────────────────────────────────────────────────────

function NotesTab({
  notes,
  onNotesChange,
}: {
  notes: PoolNote[]
  onNotesChange: (n: PoolNote[]) => void
}) {
  const markDeposited = (id: string) => {
    const next = notes.map((n) => (n.id === id ? { ...n, status: 'deposited' as const } : n))
    saveNotes(next)
    onNotesChange(next)
  }

  const remove = (id: string) => {
    const next = notes.filter((n) => n.id !== id)
    saveNotes(next)
    onNotesChange(next)
  }

  if (notes.length === 0) {
    return (
      <div className="flex flex-col items-center gap-3 py-10 text-center">
        <IconShield size={32} stroke={1.5} className="text-text-quaternary" />
        <p className="text-[13px] text-text-tertiary">No notes yet.</p>
        <p className="text-[12px] text-text-quaternary">Generate one in the Deposit tab.</p>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-2">
      {notes.map((n) => (
        <div
          key={n.id}
          className="rounded-[10px] border border-border-subtle bg-surface-primary p-3"
        >
          <div className="flex items-center justify-between">
            <StatusBadge status={n.status} />
            <div className="flex items-center gap-1">
              {n.status === 'generated' && (
                <button
                  onClick={() => markDeposited(n.id)}
                  className="rounded-[5px] bg-brand-violet/10 px-2 py-0.5 text-[11px] font-semibold text-brand-violet transition-opacity hover:opacity-80"
                >
                  Mark deposited
                </button>
              )}
              <button
                onClick={() => remove(n.id)}
                className="grid h-6 w-6 place-items-center rounded-[5px] text-text-quaternary transition-colors hover:bg-bearish-red/10 hover:text-bearish-red"
              >
                <IconTrash size={13} stroke={2} />
              </button>
            </div>
          </div>
          <div className="mt-2 grid grid-cols-2 gap-2">
            {(
              [
                ['Secret', n.secret],
                ['Nullifier', n.nullifier],
              ] as const
            ).map(([label, val]) => (
              <div key={label} className="min-w-0">
                <div className="text-[10px] uppercase tracking-widest text-text-quaternary">
                  {label}
                </div>
                <div className="flex items-center gap-1">
                  <span
                    className="truncate text-[11px] text-text-secondary"
                    style={{ fontFamily: 'var(--font-mono)' }}
                  >
                    {val}
                  </span>
                  <CopyButton text={val} />
                </div>
              </div>
            ))}
          </div>
          <div className="mt-1.5 text-[10px] text-text-quaternary">
            {new Date(n.createdAt).toLocaleString()}
          </div>
        </div>
      ))}
    </div>
  )
}

// ── Modal ─────────────────────────────────────────────────────────────────────

type Tab = 'deposit' | 'withdraw' | 'notes'

export default function ShieldedPoolModal({ onClose }: { onClose: () => void }) {
  const [tab, setTab] = useState<Tab>('deposit')
  const [notes, setNotes] = useState<PoolNote[]>(() => loadNotes())

  const TABS: { id: Tab; label: string }[] = [
    { id: 'deposit', label: 'Deposit' },
    { id: 'withdraw', label: 'Withdraw' },
    { id: 'notes', label: `Notes${notes.length ? ` (${notes.length})` : ''}` },
  ]

  return (
    <div
      className="fixed inset-0 z-[80] flex items-center justify-center bg-black/35 p-6 backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.currentTarget === e.target) onClose()
      }}
    >
      <div className="flex h-[min(640px,88vh)] w-[min(560px,94vw)] flex-col overflow-hidden rounded-[14px] border border-border-subtle bg-surface-primary shadow-2xl">
        {/* Header */}
        <div className="flex shrink-0 items-center gap-3 border-b border-border-subtle px-6 py-4">
          <IconShield size={20} stroke={1.8} className="text-brand-violet" />
          <div>
            <h1 className="text-[15px] font-semibold uppercase tracking-widest text-text-primary">
              Privacy Pool
            </h1>
            <p className="mt-0.5 text-[12px] text-text-quaternary">
              Shielded USDC — ZK Merkle mixer
            </p>
          </div>
          <button
            onClick={onClose}
            className="ml-auto grid h-9 w-9 place-items-center rounded-[8px] text-text-tertiary hover:bg-surface-hover hover:text-text-primary"
          >
            <IconX size={18} stroke={2} />
          </button>
        </div>

        {/* Tabs */}
        <div className="flex shrink-0 border-b border-border-subtle px-6">
          {TABS.map((t) => (
            <button
              key={t.id}
              onClick={() => setTab(t.id)}
              className={`relative py-2.5 pr-4 text-[13px] font-medium transition-colors ${
                tab === t.id ? 'text-text-primary' : 'text-text-tertiary hover:text-text-secondary'
              }`}
            >
              {t.label}
              {tab === t.id && (
                <span className="absolute bottom-0 left-0 right-4 h-[2px] rounded-full bg-brand-violet" />
              )}
            </button>
          ))}
        </div>

        {/* Body */}
        <div className="min-h-0 flex-1 overflow-y-auto bg-page px-6 py-5">
          {tab === 'deposit' && <DepositTab notes={notes} onNotesChange={setNotes} />}
          {tab === 'withdraw' && <WithdrawTab notes={notes} onNotesChange={setNotes} />}
          {tab === 'notes' && <NotesTab notes={notes} onNotesChange={setNotes} />}
        </div>
      </div>
    </div>
  )
}
