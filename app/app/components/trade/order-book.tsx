import { useEffect, useMemo, useRef, useState } from 'react'
import { useMarket } from '../../context/market-context'
import { usePriceSelect } from '../../context/price-select-context'
import type { OrderBookLevel } from '../../lib/tee-client'

// Pixel constants
const ROW_H = 20
const HEAT_W = 10
const PW = 70
const SW = 50
const FONT = '11px "Berkeley Mono", ui-monospace, monospace'
const FONT_SM = '9px "Berkeley Mono", ui-monospace, monospace'
const PRICE_SCALE = 1e7

const ASK_STOPS: [number, number, number, number][] = [
  [120, 30, 10, 0.5],
  [190, 80, 20, 0.58],
  [235, 170, 40, 0.66],
]
const BID_STOPS: [number, number, number, number][] = [
  [0, 85, 72, 0.46],
  [0, 150, 120, 0.55],
  [60, 210, 150, 0.62],
]
const WHALE_FILL = 'rgba(222,226,62,0.78)'
const ASK_STROKE = '#e0a838'
const BID_STROKE = '#34d399'

type RowData = {
  price: number
  size: number
  cumulative: number
  key: string
  whale: boolean
  ticks: number[]
  heatRatio: number
}

type AnimEntry = { barCur: number; barTgt: number; stepCur: number; stepTgt: number }

// ── Seeded fallback row generation ────────────────────────────────
// Used when TEE is offline or orderbook is empty.

function rand(seed: number) {
  const x = Math.sin(seed * 127.1 + 311.7) * 43758.5453
  return x - Math.floor(x)
}

function dynamicTick(price: number): number {
  if (price > 10000) return 1.0
  if (price > 1000)  return 0.1
  if (price > 100)   return 0.01
  if (price > 1)     return 0.001
  return 0.0001
}

function buildSeededRows(base: number, dir: 1 | -1, count: number): RowData[] {
  const tick = dynamicTick(base)
  const raw = Array.from({ length: count }, (_, i) => {
    const price = +(base + dir * (i + 1) * tick).toFixed(4)
    const seed = Math.round(price * (1 / tick))
    const r = rand(seed)
    const r2 = rand(seed + 57)
    const bSize = 35 + r * 380
    const size = r2 > 0.86 ? Math.round(bSize * (2 + r2 * 3.5)) : Math.round(bSize)
    return { price, size, seed }
  })

  const display = dir === 1 ? [...raw].reverse() : raw
  let cum = 0
  const cums = raw.map((r) => (cum += r.size))
  const maxSize = Math.max(...raw.map((r) => r.size))
  const whaleCutoff = maxSize * 0.55

  return display.map((row, i) => {
    const rawIndex = dir === 1 ? count - 1 - i : i
    const whale = row.size >= whaleCutoff
    return {
      price: row.price,
      size: row.size,
      cumulative: cums[rawIndex]!,
      key: row.price.toFixed(4),
      whale,
      ticks: [],
      heatRatio: maxSize > 0 ? row.size / maxSize : 0,
    }
  })
}

// ── Convert TEE CLOB levels → RowData ─────────────────────────────
function levelsToRows(levels: OrderBookLevel[], reverse: boolean): RowData[] {
  if (!levels.length) return []
  let cum = 0
  const withCum = levels.map((l) => ({ ...l, cumulative: (cum += l.size) }))
  const display = reverse ? [...withCum].reverse() : withCum
  const sizes = levels.map((l) => l.size)
  const maxSize = Math.max(...sizes)
  const minSize = Math.min(...sizes)
  const range = maxSize - minSize
  // Range-normalize so gradient has contrast even with uniform market-maker sizes.
  // Only flag as whale if a row is truly a standout outlier (top 10% of range).
  const whaleCutoff = range > maxSize * 0.1 ? minSize + range * 0.90 : Infinity
  let idx = 0
  return display.map((l) => {
    const whale = l.size >= whaleCutoff
    const heatRatio = range > 0 ? (l.size - minSize) / range : 0.5
    return {
      price: l.price / PRICE_SCALE,
      size: l.size,
      cumulative: l.cumulative,
      key: String(l.price),
      whale,
      ticks: [],
      heatRatio,
      _idx: idx++,
    }
  })
}

function isBold(price: number) {
  const str = price.toFixed(4)
  return str.endsWith('000') || str.endsWith('500')
}

function fmtSize(n: number) {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n)
}

function lerpRGBA(a: [number, number, number, number], b: [number, number, number, number], t: number) {
  const r = a[0] + (b[0] - a[0]) * t
  const g = a[1] + (b[1] - a[1]) * t
  const bl = a[2] + (b[2] - a[2]) * t
  const al = a[3] + (b[3] - a[3]) * t
  return `rgba(${r | 0},${g | 0},${bl | 0},${al.toFixed(3)})`
}

function gradColor(stops: [number, number, number, number][], t: number) {
  const c = Math.max(0, Math.min(1, t))
  if (c <= 0.5) return lerpRGBA(stops[0]!, stops[1]!, c / 0.5)
  return lerpRGBA(stops[1]!, stops[2]!, (c - 0.5) / 0.5)
}

function fmtPrice(price: number): string {
  if (price > 1000) return price.toFixed(1)
  if (price > 10)   return price.toFixed(2)
  if (price > 1)    return price.toFixed(3)
  return price.toFixed(4)
}

type Tooltip = {
  x: number
  y: number
  price: number
  size: number
  cumulative: number
  side: 'ask' | 'bid'
}

export default function OrderBook() {
  const { mark, bids: liveBids, asks: liveAsks } = useMarket()
  const { emit } = usePriceSelect()
  const wrapRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const animRef = useRef<Map<string, AnimEntry>>(new Map())
  const rafRef = useRef<number | undefined>(undefined)
  const lastFrameRef = useRef(0)
  const dimsRef = useRef({ w: 0, h: 0, dpr: 1 })
  const markRef = useRef(mark)
  const dataRef = useRef<{
    asks: RowData[]
    bids: RowData[]
    maxSize: number
    maxCum: number
    spreadY: number
    colors: { primary: string; tertiary: string; quaternary: string; border: string }
  } | null>(null)
  const hoverRef = useRef<{ rowIndex: number; side: 'ask' | 'bid' } | null>(null)
  const [tooltip, setTooltip] = useState<Tooltip | null>(null)

  markRef.current = mark

  // Determine whether we have real TEE data
  const hasLiveBook = liveBids.length > 0 || liveAsks.length > 0

  // Derive rows from TEE data or seeded fallback
  const { askRows, bidRows } = useMemo(() => {
    if (hasLiveBook) {
      // TEE CLOB: asks sorted ascending (best ask = last), bids sorted descending (best bid = first)
      const sortedAsks = [...liveAsks].sort((a, b) => a.price - b.price)
      const sortedBids = [...liveBids].sort((a, b) => b.price - a.price)
      return {
        askRows: levelsToRows(sortedAsks, true),  // display high-to-low (asks reversed)
        bidRows: levelsToRows(sortedBids, false),  // display high-to-low (bids natural)
      }
    }
    return { askRows: null, bidRows: null }
  }, [liveBids, liveAsks, hasLiveBook])

  function rebuild() {
    const canvas = canvasRef.current
    const el = wrapRef.current
    if (!canvas || !el) return
    const { w, h } = dimsRef.current
    if (w <= 0 || h <= 0) return

    const rowsPerSide = Math.max(6, Math.min(14, Math.floor(h / ROW_H / 2)))
    const mid = markRef.current
    const base = dynamicTick(mid) * Math.round(mid / dynamicTick(mid))

    const asks = askRows ?? buildSeededRows(base, 1, rowsPerSide)
    const bids = bidRows ?? buildSeededRows(base, -1, rowsPerSide)

    // Trim to fit height
    const displayAsks = asks.slice(0, rowsPerSide)
    const displayBids = bids.slice(0, rowsPerSide)

    const maxSize = Math.max(...displayAsks.map((r) => r.size), ...displayBids.map((r) => r.size))
    const maxCum  = Math.max(displayAsks[0]?.cumulative ?? 0, displayBids.at(-1)?.cumulative ?? 0)
    const barW = Math.max(0, w - HEAT_W - PW - SW)

    const cs = getComputedStyle(el)
    const colors = {
      primary:    cs.getPropertyValue('--color-text-primary').trim() || '#e8e8e8',
      tertiary:   cs.getPropertyValue('--color-text-tertiary').trim() || '#9a9a9a',
      quaternary: cs.getPropertyValue('--color-text-quaternary').trim() || '#777',
      border:     cs.getPropertyValue('--color-border-subtle').trim() || 'rgba(255,255,255,0.1)',
    }

    const spreadY = displayAsks.length * ROW_H
    dataRef.current = { asks: displayAsks, bids: displayBids, maxSize, maxCum, spreadY, colors }

    const m = animRef.current
    const seen = new Set<string>()
    const setTarget = (row: RowData) => {
      seen.add(row.key)
      const barTgt  = row.heatRatio * barW
      const stepTgt = maxCum > 0  ? (row.cumulative / maxCum) * barW : 0
      const ex = m.get(row.key)
      if (ex) {
        ex.barTgt  = barTgt
        ex.stepTgt = stepTgt
      } else {
        m.set(row.key, { barCur: 0, barTgt, stepCur: 0, stepTgt })
      }
    }
    displayAsks.forEach(setTarget)
    displayBids.forEach(setTarget)
    for (const k of Array.from(m.keys())) if (!seen.has(k)) m.delete(k)

    startLoop()
  }

  function paint() {
    const canvas = canvasRef.current
    const d = dataRef.current
    if (!canvas || !d) return
    const ctx = canvas.getContext('2d')
    if (!ctx) return
    const { w, h, dpr } = dimsRef.current
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.clearRect(0, 0, w, h)

    const { asks, bids, maxSize, maxCum, spreadY, colors } = d
    const m = animRef.current
    const barX = HEAT_W + PW + SW
    const hover = hoverRef.current

    const stepXFor = (row: RowData) => m.get(row.key)?.stepCur ?? 0
    const barWFor  = (row: RowData) => m.get(row.key)?.barCur ?? 0

    function buildStepPath(rows: RowData[], startY: number) {
      const path = new Path2D()
      path.moveTo(barX, startY)
      rows.forEach((row, i) => {
        const y = startY + i * ROW_H
        const x = barX + stepXFor(row)
        path.lineTo(x, y)
        path.lineTo(x, y + ROW_H)
      })
      path.lineTo(barX, startY + rows.length * ROW_H)
      path.closePath()
      return path
    }
    const askPath = buildStepPath(asks, 0)
    const bidPath = buildStepPath(bids, spreadY)

    ctx.fillStyle = 'rgba(190,90,30,0.10)'
    ctx.fill(askPath)
    ctx.fillStyle = 'rgba(0,120,95,0.09)'
    ctx.fill(bidPath)

    const drawRow = (row: RowData, y: number, side: 'ask' | 'bid', rowIndex: number) => {
      const barWidth = barWFor(row)
      const stops = side === 'ask' ? ASK_STOPS : BID_STOPS
      const isHovered = hover?.side === side && hover.rowIndex === rowIndex

      // Hover row background
      if (isHovered) {
        ctx.fillStyle = side === 'ask' ? 'rgba(190,90,30,0.18)' : 'rgba(0,160,120,0.15)'
        ctx.fillRect(0, y, w, ROW_H)
      }

      ctx.fillStyle = row.whale ? WHALE_FILL : gradColor(stops, row.heatRatio)
      ctx.fillRect(0, y, HEAT_W, ROW_H)

      ctx.fillStyle = row.whale ? WHALE_FILL : gradColor(stops, row.heatRatio)
      ctx.fillRect(barX, y, barWidth, ROW_H)

      ctx.font = FONT
      ctx.textBaseline = 'middle'
      ctx.textAlign = 'left'
      ctx.fillStyle = isHovered
        ? (side === 'ask' ? '#f59e44' : '#34d399')
        : isBold(row.price) ? colors.primary : colors.tertiary
      ctx.fillText(fmtPrice(row.price), HEAT_W + 6, y + ROW_H / 2 + 1)

      ctx.textAlign = 'right'
      ctx.fillStyle = isHovered ? colors.primary : colors.quaternary
      ctx.fillText(fmtSize(row.size), HEAT_W + PW + SW - 6, y + ROW_H / 2 + 1)

      if (Math.round(row.price * 10) % 10 === 0) {
        ctx.fillStyle = colors.border
        ctx.fillRect(0, y, w, 1)
      }
    }

    asks.forEach((row, i) => drawRow(row, i * ROW_H, 'ask', i))
    bids.forEach((row, i) => drawRow(row, spreadY + i * ROW_H, 'bid', i))

    ctx.lineWidth = 1
    ctx.strokeStyle = ASK_STROKE
    ctx.stroke(askPath)
    ctx.strokeStyle = BID_STROKE
    ctx.stroke(bidPath)

    // Spread band
    const bestAsk = asks.at(-1)?.price   // asks display high→low, best ask is last
    const bestBid = bids[0]?.price       // bids display high→low, best bid is first
    const spreadAbs = bestAsk != null && bestBid != null ? bestAsk - bestBid : 0
    const spreadPct = bestBid != null && spreadAbs > 0 ? (spreadAbs / bestBid) * 100 : 0

    ctx.strokeStyle = colors.tertiary
    ctx.globalAlpha = 0.4
    ctx.beginPath()
    ctx.moveTo(0, spreadY)
    ctx.lineTo(w, spreadY)
    ctx.stroke()
    ctx.globalAlpha = 1

    if (spreadAbs > 0) {
      ctx.font = FONT_SM
      ctx.textBaseline = 'middle'
      ctx.fillStyle = colors.quaternary
      ctx.textAlign = 'left'
      ctx.fillText(`Spread  ${fmtPrice(spreadAbs)}  (${spreadPct.toFixed(3)}%)`, HEAT_W + 6, spreadY - 1)
    }

    ctx.font = FONT_SM
    ctx.textAlign = 'right'
    ctx.fillStyle = colors.primary
    ctx.shadowColor = 'rgba(0,0,0,0.6)'
    ctx.shadowBlur = 3
    ctx.fillText(fmtPrice(markRef.current), w - 6, spreadY - 8)
    ctx.shadowBlur = 0

    const drawWallLabel = (row: RowData | undefined, y: number, color: string) => {
      if (!row) return
      ctx.strokeStyle = color
      ctx.globalAlpha = 0.3
      ctx.setLineDash([2, 3])
      ctx.beginPath()
      ctx.moveTo(0, y)
      ctx.lineTo(w, y)
      ctx.stroke()
      ctx.setLineDash([])
      ctx.globalAlpha = 1
      ctx.fillStyle = color
      ctx.font = FONT_SM
      ctx.textAlign = 'right'
      ctx.shadowColor = 'rgba(0,0,0,0.6)'
      ctx.shadowBlur = 3
      ctx.fillText(fmtPrice(row.price), w - 6, y - 4)
      ctx.shadowBlur = 0
    }
    if (asks.length) {
      const askWall = asks.reduce((a, b) => (b.size > a.size ? b : a))
      drawWallLabel(askWall, asks.indexOf(askWall) * ROW_H + ROW_H / 2, ASK_STROKE)
    }
    if (bids.length) {
      const bidWall = bids.reduce((a, b) => (b.size > a.size ? b : a))
      drawWallLabel(bidWall, spreadY + bids.indexOf(bidWall) * ROW_H + ROW_H / 2, BID_STROKE)
    }
  }

  function startLoop() {
    paint()
    if (rafRef.current) return
    lastFrameRef.current = performance.now()
    const frame = (now: number) => {
      const dt = Math.min(48, now - lastFrameRef.current)
      lastFrameRef.current = now
      const ease = 1 - Math.exp(-dt / 110)
      let active = false
      for (const v of animRef.current.values()) {
        v.barCur  += (v.barTgt - v.barCur)  * ease
        v.stepCur += (v.stepTgt - v.stepCur) * ease
        if (Math.abs(v.barTgt - v.barCur) > 0.4 || Math.abs(v.stepTgt - v.stepCur) > 0.4) {
          active = true
        } else {
          v.barCur  = v.barTgt
          v.stepCur = v.stepTgt
        }
      }
      paint()
      rafRef.current = active ? requestAnimationFrame(frame) : undefined
    }
    rafRef.current = requestAnimationFrame(frame)
  }

  useEffect(() => {
    const el = wrapRef.current
    const canvas = canvasRef.current
    if (!el || !canvas) return
    const ro = new ResizeObserver(() => {
      const w = el.clientWidth
      const h = el.clientHeight
      const dpr = Math.min(2, window.devicePixelRatio || 1)
      dimsRef.current = { w, h, dpr }
      canvas.width  = Math.round(w * dpr)
      canvas.height = Math.round(h * dpr)
      canvas.style.width  = `${w}px`
      canvas.style.height = `${h}px`
      rebuild()
    })
    ro.observe(el)
    return () => ro.disconnect()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Mouse interactions: hover highlight + tooltip + click-to-fill
  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return

    const hitTest = (e: MouseEvent) => {
      const d = dataRef.current
      if (!d) return null
      const rect = canvas.getBoundingClientRect()
      const y = e.clientY - rect.top
      const { spreadY, asks, bids } = d
      if (y < spreadY) {
        const i = Math.floor(y / ROW_H)
        if (i >= 0 && i < asks.length) return { side: 'ask' as const, row: asks[i]!, rowIndex: i, y }
      } else {
        const i = Math.floor((y - spreadY) / ROW_H)
        if (i >= 0 && i < bids.length) return { side: 'bid' as const, row: bids[i]!, rowIndex: i, y }
      }
      return null
    }

    const onMove = (e: MouseEvent) => {
      const hit = hitTest(e)
      const rect = canvas.getBoundingClientRect()
      hoverRef.current = hit ? { rowIndex: hit.rowIndex, side: hit.side } : null
      if (hit) {
        setTooltip({
          x: e.clientX - rect.left,
          y: hit.y,
          price: hit.row.price,
          size: hit.row.size,
          cumulative: hit.row.cumulative,
          side: hit.side,
        })
      } else {
        setTooltip(null)
      }
      paint()
    }

    const onLeave = () => {
      hoverRef.current = null
      setTooltip(null)
      paint()
    }

    const onClick = (e: MouseEvent) => {
      const hit = hitTest(e)
      if (hit) emit(hit.row.price)
    }

    canvas.addEventListener('mousemove', onMove)
    canvas.addEventListener('mouseleave', onLeave)
    canvas.addEventListener('click', onClick)
    return () => {
      canvas.removeEventListener('mousemove', onMove)
      canvas.removeEventListener('mouseleave', onLeave)
      canvas.removeEventListener('click', onClick)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // Rebuild when live book data arrives or mark price changes
  useEffect(() => {
    rebuild()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mark, askRows, bidRows])

  useEffect(() => {
    return () => {
      if (rafRef.current) cancelAnimationFrame(rafRef.current)
    }
  }, [])

  return (
    <div className="flex h-full min-w-0 flex-col bg-surface-primary">
      <div className="flex shrink-0 items-center justify-between border-b border-border-subtle px-3 py-2">
        <span className="text-[10px] font-medium uppercase tracking-widest text-text-quaternary">
          Order Book
        </span>
        <span className={`text-[10px] uppercase tracking-widest ${hasLiveBook ? 'text-bullish-green' : 'text-text-quaternary'}`}>
          {hasLiveBook ? 'Live' : 'Depth'}
        </span>
      </div>
      <div
        className="flex shrink-0 border-b border-border-subtle text-[10px] uppercase tracking-widest text-text-quaternary"
        style={{ height: 20 }}
      >
        <span className="leading-5" style={{ width: HEAT_W }} />
        <span className="pl-1.5 leading-5" style={{ width: PW }}>Price</span>
        <span className="pr-1.5 text-right leading-5" style={{ width: SW }}>Qty</span>
        <span className="pl-1 leading-5">Depth</span>
      </div>
      <div ref={wrapRef} className="relative min-h-0 flex-1">
        <canvas ref={canvasRef} className="absolute inset-0 cursor-pointer" />
        {tooltip && <OrderBookTooltip tooltip={tooltip} mark={mark} />}
      </div>
    </div>
  )
}

function OrderBookTooltip({ tooltip, mark }: { tooltip: Tooltip; mark: number }) {
  const { price, size, cumulative, side, x, y } = tooltip
  const notional = price * size
  const PRICE_SCALE = 1e7
  const notionalUsd = notional / PRICE_SCALE
  const distFromMid = Math.abs(price - mark) / mark * 100

  // Flip tooltip left/right based on canvas x position
  const flipLeft = x > 120

  return (
    <div
      className="pointer-events-none absolute z-20 rounded-[6px] border border-border-subtle bg-surface-card px-2.5 py-1.5 shadow-lg"
      style={{
        top: Math.max(0, y - 4),
        ...(flipLeft ? { right: 4 } : { left: x + 12 }),
        minWidth: 140,
      }}
    >
      <div className={`mb-1 text-[10px] font-semibold uppercase tracking-widest ${side === 'ask' ? 'text-bearish-red' : 'text-bullish-green'}`}>
        {side === 'ask' ? 'Ask' : 'Bid'}
      </div>
      <TooltipRow label="Price" value={fmtPrice(price)} accent={side === 'ask' ? '#f59e44' : '#34d399'} />
      <TooltipRow label="Qty" value={fmtSize(size)} />
      <TooltipRow label="Total" value={fmtSize(cumulative)} />
      <TooltipRow label="Notional" value={`$${notionalUsd >= 1000 ? `${(notionalUsd / 1000).toFixed(1)}k` : notionalUsd.toFixed(0)}`} />
      <div className="mt-1.5 border-t border-border-subtle pt-1 text-[9px] text-text-quaternary">
        {distFromMid.toFixed(2)}% from mid · click to fill
      </div>
    </div>
  )
}

function TooltipRow({ label, value, accent }: { label: string; value: string; accent?: string }) {
  return (
    <div className="flex items-center justify-between gap-3">
      <span className="text-[10px] text-text-quaternary">{label}</span>
      <span className="text-[10px] font-medium" style={accent ? { color: accent } : undefined}>
        {value}
      </span>
    </div>
  )
}
