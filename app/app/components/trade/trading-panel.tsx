import { useCallback, useEffect, useRef, useState } from 'react'
import {
  AnimatePresence,
  animate,
  motion,
  useMotionValue,
} from 'framer-motion'
import { IconChevronDown } from '@tabler/icons-react'
import { useLevels } from '../../context/levels-context'
import { type Side, useMarket } from '../../context/market-context'
import { formatUsd } from './format'

const MIN_LEV = 1
const MAX_LEV = 50
const STEPS = MAX_LEV - MIN_LEV
const LABEL_MARKS = [1, 3, 5, 10, 15, 20, 25, 30, 35, 40, 45, 50]
const barH = (step: number) => 5 + (step / STEPS) * 27

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
        style={{ height: 58, cursor: 'pointer' }}
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
                backgroundColor: active ? '#9998ff' : 'rgba(255,255,255,0.08)',
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
                backgroundColor: 'rgba(255,255,255,0.08)',
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
                backgroundColor: active ? '#9998ff' : 'rgba(255,255,255,0.8)',
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
                backgroundColor: 'rgba(255,255,255,0.08)',
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
                    ? 'rgba(255,255,255,0.12)'
                    : lev === value
                      ? '#9998ff'
                      : 'rgba(255,255,255,0.35)',
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

export default function TradingPanel() {
  const { symbol, mark } = useMarket()
  const levels = useLevels()
  const [side, setSide] = useState<Side>('long')
  const [orderType, setOrderType] = useState<'market' | 'limit' | 'stop'>('market')
  const [pctSelected, setPctSelected] = useState<number | null>(null)
  const [takeProfitEnabled, setTakeProfitEnabled] = useState(false)
  const [leverage, setLeverage] = useState(5)
  const [amount, setAmount] = useState('')
  const [limitPrice, setLimitPrice] = useState('')
  const [tpInput, setTpInput] = useState('')
  const [slInput, setSlInput] = useState('')

  const margin = Number(amount) || 0
  const notional = margin * leverage
  const fee = notional * 0.00045
  const liquidation =
    side === 'long'
      ? mark * (1 - 0.92 / leverage)
      : mark * (1 + 0.92 / leverage)

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
        <div className="ml-auto flex items-center gap-1 text-[14px] text-text-tertiary hover:text-text-secondary">
          Cross
          <IconChevronDown size={12} stroke={2.5} />
        </div>
      </div>

      <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-hidden px-3 py-2">
        <div className="flex items-center justify-between">
          <span className="text-[13px] text-text-secondary">Margin</span>
          <span className="text-[13px] text-text-tertiary">
            Bal. <span className="text-text-secondary">$12,480.92</span>
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
                setAmount(((12480.92 * pct) / 100).toFixed(2))
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
              setAmount('12480.92')
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
        </div>
      </div>

      <div className="flex shrink-0 flex-col gap-1.5 px-3 pb-3">
        <button
          onClick={() => levels.setEntry(mark)}
          className={`w-full rounded-[8px] py-2.5 text-[13px] font-semibold transition-opacity hover:opacity-90 ${
            side === 'long' ? 'bg-bullish-green text-[#1a1a1a]' : 'bg-bearish-red text-white'
          }`}
        >
          {actionLabel} {symbol}
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
