import { useCallback, useEffect, useRef, useState } from 'react'
import {
  AnimatePresence,
  animate,
  motion,
  useMotionValue,
} from 'framer-motion'
import { useLevels } from '../../context/levels-context'
import { type Side, useMarket } from '../../context/market-context'
import { useNav } from '../../context/nav-context'
import { usePriceSelect } from '../../context/price-select-context'
import { useWallet } from '../../context/wallet-context'
import {
  buildDepositNoteTx,
  buildOpenPositionFromNoteTx,
  buildPlaceOrderTx,
  crossMarginKey,
  proofJsonToScVal,
  submitAndWait,
} from '../../lib/contracts'
import { tee } from '../../lib/tee-client'
import { positionsStore } from '../../lib/positions-store'
import { formatUsd } from './format'
import { toast } from '../toast/toast-context'

const MIN_LEV = 1
const MAX_LEV = 50
const STEPS = MAX_LEV - MIN_LEV
const LABEL_MARKS = [1, 3, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50]
const barH = (step: number) => 10 + (step / STEPS) * 54

function LeverageSlider({
  value,
  onChange,
  maxValue = MAX_LEV,
}: {
  value: number
  onChange: (v: number) => void
  maxValue?: number
}) {
  const trackRef = useRef<HTMLDivElement>(null)
  const thumbX = useMotionValue(0)
  const dragging = useRef(false)
  const [showHandle, setShowHandle] = useState(false)

  const getW = () => trackRef.current?.clientWidth ?? 0
  const levToX = (lev: number) => ((lev - MIN_LEV) / STEPS) * getW()
  const xToLev = (x: number) =>
    Math.min(maxValue, Math.round(Math.max(0, Math.min(1, x / (getW() || 1))) * STEPS) + MIN_LEV)

  useEffect(() => {
    if (!dragging.current) thumbX.set(levToX(value))
  })

  const springTo = useCallback(
    (lev: number) => {
      const clamped = Math.min(maxValue, lev)
      onChange(clamped)
      animate(thumbX, levToX(clamped), {
        type: 'spring',
        stiffness: 600,
        damping: 38,
        mass: 0.4,
      })
    },
    [onChange, maxValue],
  )

  const handleThumbDown = (e: React.PointerEvent) => {
    e.stopPropagation()
    dragging.current = true
    setShowHandle(true)
    const origin = { clientX: e.clientX, x: thumbX.get() }

    const onMove = (e: PointerEvent) => {
      const nx = Math.max(0, Math.min(levToX(maxValue), origin.x + e.clientX - origin.clientX))
      thumbX.set(nx)
      onChange(xToLev(nx))
    }
    const onUp = (e: PointerEvent) => {
      dragging.current = false
      setShowHandle(false)
      const nx = Math.max(0, Math.min(levToX(maxValue), origin.x + e.clientX - origin.clientX))
      springTo(xToLev(nx))
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
    }
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
  }

  const majors = LABEL_MARKS.map((mark, i) => ({
    pos: (mark - MIN_LEV) / STEPS,
    h: barH(mark - MIN_LEV),
    active: mark <= value,
    step: i,
  }))

  const minors: { pos: number; h: number }[] = []
  for (let i = 0; i < LABEL_MARKS.length - 1; i++) {
    const aStep = (LABEL_MARKS[i] ?? MIN_LEV) - MIN_LEV
    const bStep = (LABEL_MARKS[i + 1] ?? MAX_LEV) - MIN_LEV
    for (let j = 1; j <= 3; j++) {
      const frac = j / 4
      const step = aStep + frac * (bStep - aStep)
      minors.push({
        pos: step / STEPS,
        h: barH(aStep) + frac * (barH(bStep) - barH(aStep)),
      })
    }
  }

  return (
    <div>
      <div className="mb-1 flex items-baseline gap-1">
        <p className="text-[11px] font-medium uppercase tracking-widest text-text-tertiary">
          Leverage
        </p>
        <motion.span
          key={value}
          initial={{ opacity: 0, y: -4 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ type: 'spring', stiffness: 500, damping: 30 }}
          className="ml-1.5 text-[20px] font-semibold leading-none text-text-primary"
          style={{ fontFamily: 'var(--font-mono)' }}
        >
          {value}
        </motion.span>
        <span
          className="text-[13px] font-light text-text-secondary"
          style={{ fontFamily: 'var(--font-mono)' }}
        >
          x
        </span>
      </div>

      <div
        ref={trackRef}
        className="relative select-none"
        style={{ height: 84, cursor: 'pointer' }}
        onPointerEnter={() => setShowHandle(true)}
        onPointerLeave={() => {
          if (!dragging.current) setShowHandle(false)
        }}
        onClick={(e) => {
          const rect = trackRef.current!.getBoundingClientRect()
          springTo(Math.min(maxValue, xToLev(e.clientX - rect.left)))
        }}
      >
        <div style={{ position: 'absolute', inset: '0 0 18px 0' }}>
          {majors.map(({ pos, h, active }, i) => (
            <div
              key={`mj${i}`}
              style={{
                position: 'absolute',
                left: `${pos * 100}%`,
                bottom: 6,
                width: 2,
                height: h,
                transform: 'translateX(-1px)',
                backgroundColor: active ? '#807dfe' : 'var(--color-border-default)',
                pointerEvents: 'none',
                transition: 'background-color 0.1s',
              }}
            />
          ))}

          {minors.map(({ pos, h }, i) => (
            <div
              key={`mn${i}`}
              style={{
                position: 'absolute',
                left: `${pos * 100}%`,
                bottom: 6,
                width: 1.5,
                height: h,
                transform: 'translateX(-0.75px)',
                backgroundColor: 'var(--color-border-subtle)',
                opacity: 0.5,
                pointerEvents: 'none',
              }}
            />
          ))}

          {majors.map(({ pos, active }, i) => (
            <div
              key={`tmj${i}`}
              style={{
                position: 'absolute',
                left: `${pos * 100}%`,
                bottom: 0,
                width: 1.5,
                height: 5,
                transform: 'translateX(-0.75px)',
                backgroundColor: active ? '#807dfe' : 'var(--color-text-quaternary)',
                opacity: active ? 0.7 : 0.8,
                pointerEvents: 'none',
              }}
            />
          ))}

          {minors.map(({ pos }, i) => (
            <div
              key={`tmn${i}`}
              style={{
                position: 'absolute',
                left: `${pos * 100}%`,
                bottom: 0,
                width: 1.5,
                height: 5,
                transform: 'translateX(-0.75px)',
                backgroundColor: 'var(--color-border-subtle)',
                opacity: 0.4,
                pointerEvents: 'none',
              }}
            />
          ))}

          <motion.div
            onPointerDown={handleThumbDown}
            style={{
              position: 'absolute',
              left: 0,
              bottom: 6,
              x: thumbX,
              width: 12,
              height: 50,
              translateX: '-6px',
              backgroundColor: 'transparent',
              cursor: 'grab',
              zIndex: 10,
            }}
          >
            <div
              style={{
                position: 'absolute',
                left: '50%',
                top: 0,
                bottom: 0,
                width: 2,
                transform: 'translateX(-1px)',
                backgroundColor: '#9998ff',
              }}
            />
          </motion.div>

          <AnimatePresence>
            {showHandle && (
              <motion.div
                initial={{ opacity: 0, scale: 0.7, y: 6 }}
                animate={{ opacity: 1, scale: 1, y: 0 }}
                exit={{ opacity: 0, scale: 0.7, y: 6 }}
                transition={{
                  type: 'spring',
                  stiffness: 500,
                  damping: 28,
                  mass: 0.4,
                }}
                style={{
                  position: 'absolute',
                  left: 0,
                  top: 0,
                  x: thumbX,
                  translateX: '-50%',
                  pointerEvents: 'none',
                  zIndex: 20,
                  display: 'flex',
                  flexDirection: 'column',
                  alignItems: 'center',
                }}
              >
                <div
                  style={{
                    width: 22,
                    height: 28,
                    backgroundColor: 'rgb(201,200,255)',
                    borderRadius: 6,
                    display: 'grid',
                    gridTemplateColumns: 'repeat(2, 4px)',
                    gridTemplateRows: 'repeat(3, 4px)',
                    gap: 2,
                    placeContent: 'center',
                    boxShadow:
                      'rgba(153,152,255,0.3) 0px 2px 10px, rgba(0,0,0,0.1) 0px 1px 3px',
                  }}
                >
                  {Array.from({ length: 6 }).map((_, i) => (
                    <div
                      key={i}
                      style={{
                        width: 4,
                        height: 4,
                        borderRadius: '50%',
                        backgroundColor: 'rgb(153,152,255)',
                        opacity: 0.7,
                      }}
                    />
                  ))}
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </div>

        <div
          style={{
            position: 'absolute',
            bottom: 0,
            left: 4,
            right: 4,
            height: 18,
          }}
        >
          {LABEL_MARKS.map((lev) => (
            <button
              key={lev}
              onClick={(e) => {
                e.stopPropagation()
                springTo(Math.min(maxValue, lev))
              }}
              disabled={lev > maxValue}
              style={{
                position: 'absolute',
                left: `${((lev - MIN_LEV) / STEPS) * 100}%`,
                transform: 'translateX(-50%)',
                background: 'none',
                border: 'none',
                cursor: lev > maxValue ? 'default' : 'pointer',
                fontFamily: 'var(--font-mono)',
                fontSize: 10,
                fontWeight: lev === value ? 600 : 500,
                color:
                  lev > maxValue
                    ? 'var(--color-text-quaternary)'
                    : lev === value
                      ? '#807dfe'
                      : 'var(--color-text-tertiary)',
                padding: 0,
                lineHeight: 1,
                transition: 'color 0.15s',
              }}
            >
              {lev}x
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}

function PriceInput({
  label,
  value,
  onChange,
  placeholder,
}: {
  label: string
  value: string
  onChange: (v: string) => void
  placeholder: string
}) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-[10px] text-text-tertiary">{label}</span>
      <input
        type="number"
        value={value}
        placeholder={placeholder}
        step="0.01"
        min="0"
        onChange={(e) => onChange(e.target.value)}
        className="w-full border-b border-border-subtle bg-transparent pb-1 text-[13px] text-text-primary outline-none placeholder:text-text-quaternary focus:border-border-default"
        style={{ fontFamily: 'var(--font-mono)' }}
      />
    </div>
  )
}

// Prices in the contract use 7 decimal places (stroop-scale)
const PRICE_SCALE = 1e7

export default function TradingPanel() {
  const { connected, publicKey, sign, balance, refreshBalance } = useWallet()
  const { symbol, mark } = useMarket()
  const { openPortfolio } = useNav()
  const { subscribe } = usePriceSelect()
  const levels = useLevels()
  const [side, setSide] = useState<Side>('long')
  const [marginMode, setMarginMode] = useState<'isolated' | 'cross'>('isolated')
  const [orderType, setOrderType] = useState<'market' | 'limit' | 'stop'>('market')
  const [pctSelected, setPctSelected] = useState<number | null>(null)
  const [takeProfitEnabled, setTakeProfitEnabled] = useState(false)
  const [leverage, setLeverage] = useState(5)
  const [amount, setAmount] = useState('')
  const [limitPrice, setLimitPrice] = useState('')
  const [tpInput, setTpInput] = useState('')
  const [slInput, setSlInput] = useState('')
  const [submitting, setSubmitting] = useState(false)

  const balanceDollars = Number(balance) / PRICE_SCALE

  const margin = Number(amount) || 0
  const notional = margin * leverage
  const fee = notional * 0.00045
  const liquidation =
    side === 'long'
      ? mark * (1 - 0.92 / leverage)
      : mark * (1 + 0.92 / leverage)

  // Receive price clicks from the order book — switch to limit and prefill price
  useEffect(() => {
    return subscribe((price) => {
      setOrderType('limit')
      setLimitPrice(price.toFixed(price < 1 ? 4 : price < 100 ? 3 : price < 1000 ? 2 : 1))
    })
  }, [subscribe])

  useEffect(() => {
    if (!takeProfitEnabled) {
      levels.setTp(null)
      levels.setSl(null)
      setTpInput('')
      setSlInput('')
    }
  }, [takeProfitEnabled])

  const handleTpChange = (v: string) => {
    setTpInput(v)
    const n = parseFloat(v)
    levels.setTp(Number.isNaN(n) ? null : +Math.max(0, n).toFixed(2))
  }

  const handleSlChange = (v: string) => {
    setSlInput(v)
    const n = parseFloat(v)
    levels.setSl(Number.isNaN(n) ? null : +Math.max(0, n).toFixed(2))
  }

  const actionLabel = side === 'long' ? 'Long' : 'Short'
  const pctOptions = [10, 25, 50, 75]

  const handleSubmit = async () => {
    if (!connected || !publicKey) {
      toast.warning('Connect wallet', `Connect a wallet before placing a ${actionLabel.toLowerCase()} order.`)
      return
    }

    if (!amount || margin <= 0 || Number.isNaN(margin)) {
      toast.warning('Enter margin', 'Choose an amount before submitting the order.')
      return
    }

    if (balanceDollars <= 0) {
      openPortfolio()
      return
    }

    if (margin > balanceDollars) {
      openPortfolio()
      toast.warning(
        'Insufficient balance',
        `You need ${formatUsd(margin)} but only have ${formatUsd(balanceDollars)}.`,
      )
      return
    }

    if (orderType !== 'market') {
      const price = Number(limitPrice)
      if (!limitPrice || price <= 0 || Number.isNaN(price)) {
        toast.warning(
          orderType === 'limit' ? 'Set limit price' : 'Set trigger price',
          `A ${orderType} order needs a valid execution price.`,
        )
        return
      }
    }

    setSubmitting(true)
    const progressId = toast.progress(`${actionLabel} ${symbol}`, 5, 'Generating ZK proofs…')

    try {
      const collateralUnits = BigInt(Math.round(margin * PRICE_SCALE))
      const markPrice = Math.round(mark * PRICE_SCALE)
      const hintPrice = orderType === 'market' ? markPrice : Math.round(Number(limitPrice) * PRICE_SCALE)
      const tpUnits = tpInput ? Math.round(parseFloat(tpInput) * PRICE_SCALE) : 0
      const slUnits = slInput ? Math.round(parseFloat(slInput) * PRICE_SCALE) : 0
      const portfolioKey = marginMode === 'cross' ? crossMarginKey(publicKey) : undefined
      const sideNum: 0 | 1 = side === 'long' ? 0 : 1

      // Random secrets for the order commitment and the note
      const orderNonce = Math.floor(Math.random() * Number.MAX_SAFE_INTEGER)
      const orderSecret = Math.floor(Math.random() * Number.MAX_SAFE_INTEGER)
      const noteSecret = Math.floor(Math.random() * Number.MAX_SAFE_INTEGER)
      // collateralUnits is bigint; note circuit uses u64 — safe if < 2^53
      const noteAmount = Number(collateralUnits)

      // TEE encodes is_market via side>=2: 0=limit-bid, 1=limit-ask, 2=market-bid, 3=market-ask
      const rawSide = orderType === 'market' ? sideNum + 2 : sideNum

      // Step 1: Ask TEE to store order secrets + generate both proofs in parallel
      // init stores secrets so the TEE can later match this order
      console.log('step: calling tee.init + tee.noteProof…')
      const [initResult, noteResult] = await Promise.all([
        tee.init({ side: rawSide, price: hintPrice, size: 1_000_000_000, leverage, nonce: orderNonce, secret: orderSecret }),
        tee.noteProof(noteAmount, noteSecret),
      ])
      console.log('step: tee proofs done, commitment=', initResult.commitment.slice(0,16))
      const commitment = initResult.commitment
      toast.update(progressId, { description: 'Getting commitment proof…', progress: 30 })

      console.log('step: calling tee.commitProof…')
      const commitProofResult = await tee.commitProof(commitment)
      console.log('step: commitProof done')

      const commitScVal = proofJsonToScVal(commitProofResult.proof)
      const noteScVal   = proofJsonToScVal(noteResult.proof)

      // Soroban's simulator rejects multi-op transactions, so we send three
      // separate signing prompts. The ops are independent at build-time
      // (open_position reads deposit_note's note via on-chain storage, but
      // noteCmt is known client-side), so we don't need inter-tx confirmation.
      toast.update(progressId, { description: 'Sign 1/3 — place order…', progress: 45 })
      const placeTx = await buildPlaceOrderTx(publicKey, {
        commitment,
        hintPrice,
        hintSide: sideNum,
        hintSize: 1_000_000_000,
        hintLeverage: leverage,
        portfolioKey,
        proof: commitScVal,
      })
      await submitAndWait(await sign(placeTx.toXDR()))

      toast.update(progressId, { description: 'Sign 2/3 — deposit collateral…', progress: 62 })
      const depositTx = await buildDepositNoteTx(publicKey, noteResult.note_cmt, collateralUnits)
      await submitAndWait(await sign(depositTx.toXDR()))

      toast.update(progressId, { description: 'Sign 3/3 — open position…', progress: 80 })
      const openTx = await buildOpenPositionFromNoteTx(publicKey, {
        noteCmt: noteResult.note_cmt,
        noteNull: noteResult.note_null,
        commitment,
        hintPrice,
        side: sideNum,
        leverage,
        size: 1_000_000_000,
        tpPrice: tpUnits,
        slPrice: slUnits,
        portfolioKey,
        noteProof: noteScVal,
        commitProof: commitScVal,
      })
      const openTxHash = await submitAndWait(await sign(openTx.toXDR()))

      positionsStore.add({ commitment, symbol, side: sideNum, leverage, openedAt: Date.now() })
      levels.setEntry(mark)
      refreshBalance()

      const txUrl = `https://stellar.expert/explorer/testnet/tx/${openTxHash}`
      toast.update(progressId, {
        type: 'success',
        title: `${actionLabel} ${symbol} opened`,
        description: (
          <span>
            {leverage}x · {formatUsd(notional)} notional{' · '}
            <a
              href={txUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="underline hover:opacity-80"
            >
              View tx ↗
            </a>
          </span>
        ),
        progress: undefined,
        duration: 8000,
      })
      setAmount('')
      setPctSelected(null)
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      console.error('trade error:', { err, msg, step: 'unknown' })
      toast.update(progressId, {
        type: 'error',
        title: 'Order failed',
        description: msg.slice(0, 120),
        progress: undefined,
        duration: 6000,
      })
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="flex h-full min-w-0 flex-col bg-surface-primary">
      <div className="flex shrink-0 border-b border-border-subtle">
        <button
          onClick={() => setSide('long')}
          className={`flex flex-1 items-center justify-center gap-2 py-2 transition-colors ${
            side === 'long'
              ? 'bg-surface-hover text-bullish-green'
              : 'text-text-tertiary hover:text-text-secondary'
          }`}
        >
          <span className="text-[15px] font-bold tracking-wide">LONG</span>
          <span className="text-[13px] font-medium tabular-nums">{formatUsd(mark)}</span>
        </button>
        <button
          onClick={() => setSide('short')}
          className={`flex flex-1 items-center justify-center gap-2 py-2 transition-colors ${
            side === 'short'
              ? 'bg-surface-hover text-bearish-red'
              : 'text-text-tertiary hover:text-text-secondary'
          }`}
        >
          <span className="text-[15px] font-bold tracking-wide">SHORT</span>
          <span className="text-[13px] font-medium tabular-nums">{formatUsd(mark)}</span>
        </button>
      </div>

      <div className="flex shrink-0 items-center gap-0 border-b border-border-subtle px-3 pb-1.5 pt-2">
        {(['market', 'limit', 'stop'] as const).map((type) => (
          <button
            key={type}
            onClick={() => setOrderType(type)}
            className={`relative flex items-center gap-1 rounded-[5px] px-3 py-1.5 text-[14px] font-medium capitalize transition-colors ${
              orderType === type
                ? 'text-text-primary'
                : 'text-text-tertiary hover:text-text-secondary'
            }`}
          >
            {type}
            {orderType === type && (
              <span className="absolute bottom-0 left-0 right-0 h-[2px] rounded-full bg-text-primary" />
            )}
          </button>
        ))}
        <button
          onClick={() => setMarginMode((m) => (m === 'isolated' ? 'cross' : 'isolated'))}
          className={`ml-auto rounded-[5px] px-2 py-1 text-[12px] font-medium transition-colors ${
            marginMode === 'cross'
              ? 'bg-brand-violet/15 text-brand-violet'
              : 'text-text-tertiary hover:text-text-secondary'
          }`}
        >
          {marginMode === 'cross' ? 'Cross' : 'Isolated'}
        </button>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-hidden px-3 py-2">
        <div className="flex items-center justify-between">
          <span className="text-[13px] text-text-secondary">Margin</span>
          <span className="text-[13px] text-text-tertiary">
            Bal.{' '}
            <span className="text-text-secondary">
              {connected ? formatUsd(balanceDollars) : '—'}
            </span>
          </span>
        </div>

        <div className="flex items-center gap-2 rounded-[8px] border border-border-subtle bg-surface-primary px-3 py-1.5">
          <input
            type="number"
            value={amount}
            onChange={(e) => {
              setAmount(e.target.value)
              setPctSelected(null)
            }}
            placeholder="0.00"
            min="0"
            step="1"
            className="min-w-0 flex-1 bg-transparent text-[20px] font-medium tracking-tight text-text-primary outline-none placeholder:text-text-quaternary"
            style={{ fontFamily: 'var(--font-mono)' }}
          />
          <span className="ml-auto flex shrink-0 items-center justify-center rounded-[4px] border border-border-subtle bg-surface-card px-2.5 py-0.5 text-[12px] font-bold leading-none text-text-primary">
            USDC
          </span>
        </div>

        {orderType !== 'market' && (
          <div className="flex items-center gap-2 rounded-[8px] border border-border-subtle bg-surface-primary px-3 py-1.5">
            <span className="shrink-0 text-[11px] uppercase tracking-widest text-text-tertiary">
              {orderType === 'limit' ? 'Limit' : 'Trigger'}
            </span>
            <input
              type="number"
              value={limitPrice}
              onChange={(e) => setLimitPrice(e.target.value)}
              placeholder={mark.toFixed(2)}
              min="0"
              step="0.01"
              className="min-w-0 flex-1 bg-transparent text-right text-[14px] font-medium text-text-primary outline-none placeholder:text-text-quaternary"
              style={{ fontFamily: 'var(--font-mono)' }}
            />
          </div>
        )}

        <div className="flex items-center gap-1.5">
          {pctOptions.map((pct) => (
            <button
              key={pct}
              onClick={() => {
                setPctSelected(pct === pctSelected ? null : pct)
                setAmount(((balanceDollars * pct) / 100).toFixed(2))
              }}
              className={`flex-1 rounded-[5px] py-1.5 text-[13px] font-medium transition-colors ${
                pctSelected === pct
                  ? 'bg-surface-hover text-text-primary'
                  : 'bg-surface-primary text-text-tertiary hover:bg-surface-hover/60 hover:text-text-secondary'
              }`}
            >
              {pct}%
            </button>
          ))}
          <button
            onClick={() => {
              setPctSelected(100)
              setAmount(balanceDollars.toFixed(2))
            }}
            className={`flex-1 rounded-[5px] py-1.5 text-[13px] font-medium transition-colors ${
              pctSelected === 100
                ? 'bg-surface-hover text-text-primary'
                : 'bg-surface-primary text-text-tertiary hover:bg-surface-hover/60 hover:text-text-secondary'
            }`}
          >
            MAX
          </button>
        </div>

        <LeverageSlider value={leverage} onChange={setLeverage} maxValue={50} />

        <div className="flex items-center justify-between">
          <span className="text-[13px] text-text-secondary">Take profit / Stop loss</span>
          <button
            onClick={() => setTakeProfitEnabled(!takeProfitEnabled)}
            aria-pressed={takeProfitEnabled}
            className={`h-5 w-9 rounded-pill border p-0.5 transition-colors ${
              takeProfitEnabled
                ? 'border-brand-violet bg-brand-violet'
                : 'border-border-default bg-surface-card'
            }`}
          >
            <span
              className={`block h-3.5 w-3.5 rounded-pill bg-white transition-transform ${
                takeProfitEnabled ? 'translate-x-4' : 'translate-x-0'
              }`}
            />
          </button>
        </div>

        <AnimatePresence>
          {takeProfitEnabled && (
            <motion.div
              initial={{ height: 0, opacity: 0 }}
              animate={{ height: 'auto', opacity: 1 }}
              exit={{ height: 0, opacity: 0 }}
              transition={{ type: 'spring', stiffness: 400, damping: 30 }}
              className="overflow-hidden"
            >
              <div className="flex gap-4 pt-1">
                <div className="flex-1">
                  <PriceInput
                    label="Take Profit"
                    value={tpInput}
                    onChange={handleTpChange}
                    placeholder={(mark * (side === 'long' ? 1.03 : 0.97)).toFixed(2)}
                  />
                </div>
                <div className="flex-1">
                  <PriceInput
                    label="Stop Loss"
                    value={slInput}
                    onChange={handleSlChange}
                    placeholder={(mark * (side === 'long' ? 0.985 : 1.015)).toFixed(2)}
                  />
                </div>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        <div className="mt-auto grid gap-1.5 border-t border-border-subtle pt-2 text-[11px] text-text-tertiary">
          <SummaryRow label="Notional" value={formatUsd(notional)} />
          <SummaryRow label="Est. fee" value={formatUsd(fee)} />
          <SummaryRow label="Liq. price" value={formatUsd(liquidation)} />
          <SummaryRow
            label="Margin"
            value={marginMode === 'cross' ? 'Cross' : 'Isolated'}
          />
          <div className="flex items-center justify-between">
            <span>Privacy</span>
            <span className="text-brand-violet">Private (shielded)</span>
          </div>
        </div>
      </div>

      <div className="flex shrink-0 flex-col gap-1.5 px-3 pb-3">
        <button
          onClick={handleSubmit}
          disabled={submitting}
          className={`relative w-full overflow-hidden rounded-[8px] py-2.5 text-[13px] font-semibold transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-70 ${
            side === 'long' ? 'bg-bullish-green text-[#1a1a1a]' : 'bg-bearish-red text-white'
          }`}
        >
          {submitting && (
            <motion.span
              className="absolute inset-0 opacity-20"
              animate={{ x: ['-100%', '100%'] }}
              transition={{ duration: 1.2, repeat: Infinity, ease: 'linear' }}
              style={{
                background:
                  'linear-gradient(90deg, transparent 0%, white 50%, transparent 100%)',
              }}
            />
          )}
          <span className="relative">
            {submitting ? 'Signing…' : `${actionLabel} ${symbol}`}
          </span>
        </button>
      </div>
    </div>
  )
}

function SummaryRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between">
      <span>{label}</span>
      <span className="tabular-nums text-text-secondary">{value}</span>
    </div>
  )
}
