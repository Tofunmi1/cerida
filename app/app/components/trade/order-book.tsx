import { useEffect, useMemo, useRef, useState } from 'react'
import { useMarket } from '../../context/market-context'
import { usePriceSelect } from '../../context/price-select-context'
import type { OrderBookLevel } from '../../lib/tee-client'

// Pixel constants
const ROW_H = 16
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

type AnimEntry = {
  barCur: number; barTgt: number
  stepCur: number; stepTgt: number
  noise: number      // current display size multiplier offset
  noiseTgt: number   // target noise (jumps per level independently)
  flash: number      // ms remaining for row highlight flash
}

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

// ── Bucket TEE levels into readable price steps ──────────────────
// The backend sends one row per actual price level.  For BTC those levels are
// ~$19 apart, but the panel only fits ~10 rows, so the book looks shallow.
// We bucket nearby levels (e.g. $10 for BTC, $1 for GOLD) so each visible row
// shows more cumulative depth and fewer duplicate-looking prices.
function bucketTick(displayPrice: number): number {
  // Match the precision used by fmtPrice so we only aggregate rows that look
  // identical to the user.  Avoids creating fake mega-levels from unrelated orders.
  if (displayPrice > 10000) return 0.1
  if (displayPrice > 1000) return 0.01
  if (displayPrice > 100) return 0.001
  if (displayPrice > 1) return 0.0001
  return 0.00001
}

function aggregateByBucket(levels: OrderBookLevel[]): OrderBookLevel[] {
  const map = new Map<string, OrderBookLevel>()
  for (const l of levels) {
    const display = l.price / PRICE_SCALE
    const tick = bucketTick(display)
    const bucketed = Math.round(display / tick) * tick
    const key = fmtPrice(bucketed)
    const existing = map.get(key)
    if (existing) {
      existing.size += l.size
      existing.orders += l.orders
    } else {
      map.set(key, {
        price: Math.round(bucketed * PRICE_SCALE),
        size: l.size,
        orders: l.orders,
      })
    }
  }
  return Array.from(map.values())
}

/// Robust per-side scale cap.  A 95th-percentile keeps a giant wall from
/// squashing every normal row, while still giving that wall a full-width bar.
function percentile(sortedAsc: number[], p: number): number {
  if (!sortedAsc.length) return 0
  const idx = Math.min(sortedAsc.length - 1, Math.floor(p * (sortedAsc.length - 1)))
  return sortedAsc[idx]!
}

/// Build cumulative depth for each displayed row, capping each contribution so
/// whale walls don't dominate the stepped-area visualization.
/// `reverse` means row 0 is the outermost level (asks), so we accumulate
/// from the innermost row backwards.
function buildCappedCums(rows: RowData[], reverse: boolean, cap: number): number[] {
  const n = rows.length
  const cums = new Array(n).fill(0)
  let cum = 0
  if (reverse) {
    for (let i = n - 1; i >= 0; i--) {
      cum += Math.min(rows[i].size, cap)
      cums[i] = cum
    }
  } else {
    for (let i = 0; i < n; i++) {
      cum += Math.min(rows[i].size, cap)
      cums[i] = cum
    }
  }
  return cums
}

// ── Convert TEE CLOB levels → RowData ─────────────────────────────
function levelsToRows(levels: OrderBookLevel[], reverse: boolean): RowData[] {
  const aggregated = aggregateByBucket(levels)
  if (!aggregated.length) return []
  let cum = 0
  const withCum = aggregated.map((l) => ({ ...l, cumulative: (cum += l.size) }))
  const display = reverse ? [...withCum].reverse() : withCum
  const sizes = aggregated.map((l) => l.size)
  const sortedSizes = [...sizes].sort((a, b) => a - b)
  const scaleMax = percentile(sortedSizes, 0.95)
  let idx = 0
  return display.map((l) => {
    const whale = scaleMax > 0 && l.size > scaleMax * 1.5
    const heatRatio = scaleMax > 0 ? Math.min(l.size, scaleMax) / scaleMax : 0.5
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
  const s = n / PRICE_SCALE
  if (s >= 1000) return s.toFixed(1)
  if (s >= 1) return s.toFixed(2)
  if (s >= 0.001) return s.toFixed(4)
  return s.toPrecision(3)
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
    askCums: number[]
    bidCums: number[]
    scaleMax: number
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

    const allSizes = [...displayAsks, ...displayBids].map((r) => r.size).sort((a, b) => a - b)
    const scaleMax = percentile(allSizes, 0.95)

    const askScaleCums = buildCappedCums(displayAsks, true, scaleMax)
    const bidScaleCums = buildCappedCums(displayBids, false, scaleMax)
    const maxCum = Math.max(
      askScaleCums.length ? Math.max(...askScaleCums) : 0,
      bidScaleCums.length ? Math.max(...bidScaleCums) : 0
    )

    const barW = Math.max(0, w - HEAT_W - PW - SW)

    const cs = getComputedStyle(el)
    const colors = {
      primary:    cs.getPropertyValue('--color-text-primary').trim() || '#e8e8e8',
      tertiary:   cs.getPropertyValue('--color-text-tertiary').trim() || '#9a9a9a',
      quaternary: cs.getPropertyValue('--color-text-quaternary').trim() || '#777',
      border:     cs.getPropertyValue('--color-border-subtle').trim() || 'rgba(255,255,255,0.1)',
    }

    const spreadY = displayAsks.length * ROW_H
    dataRef.current = { asks: displayAsks, bids: displayBids, askCums: askScaleCums, bidCums: bidScaleCums, scaleMax, maxCum, spreadY, colors }

    const m = animRef.current
    const seen = new Set<string>()
    const setTarget = (row: RowData, i: number, side: 'ask' | 'bid') => {
      seen.add(row.key)
      // Both sides: bar grows away from mid price (innermost = narrowest, outward = wider)
      // For asks: askScaleCums[0] = total cum (widest at best ask? no — reverse gives
      // cum[0]=total, but that's the largest, making best ask the widest.)
      // Let's just use raw cumulative ordering: innermost = own size, outermost = total
      const barCum = side === 'ask' ? askScaleCums[i] : bidScaleCums[i]
      const barTgt  = maxCum > 0 ? (barCum / maxCum) * barW : 0
      const stepTgt = maxCum > 0 ? ((side === 'ask' ? askScaleCums[i] : bidScaleCums[i]) / maxCum) * barW : 0
      const ex = m.get(row.key)
      if (ex) {
        ex.barTgt  = barTgt
        ex.stepTgt = stepTgt
      } else {
        m.set(row.key, { barCur: 0, barTgt, stepCur: 0, stepTgt, noise: 0, noiseTgt: 0, flash: 0 })
      }
    }
    displayAsks.forEach((row, i) => setTarget(row, i, 'ask'))
    displayBids.forEach((row, i) => setTarget(row, i, 'bid'))
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

    const { asks, bids, askCums, bidCums, scaleMax, maxCum, spreadY, colors } = d
    const m = animRef.current
    const barX = HEAT_W + PW + SW
    const barW = Math.max(0, w - HEAT_W - PW - SW)
    const hover = hoverRef.current

    const stepXFor = (row: RowData) => {
      const e = m.get(row.key)
      return e ? Math.max(0, Math.min(barW, e.stepCur)) : 0
    }
    const barWFor  = (row: RowData) => {
      const e = m.get(row.key)
      return e ? Math.max(0, Math.min(barW, e.barCur * (1 + e.noise))) : 0
    }
    const sizeFor  = (row: RowData, i: number, side: 'ask' | 'bid') => {
      const c = side === 'ask' ? askCums[i] : bidCums[i]
      const e = m.get(row.key)
      return e ? Math.max(1, Math.round(c * (1 + e.noise))) : c
    }
    const flashFor = (row: RowData) => {
      const e = m.get(row.key)
      return e && e.flash > 0 ? e.flash / 220 : 0  // 0–1 intensity
    }

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
      const isHovered = hover?.side === side && rowIndex <= hover.rowIndex

      // Flash highlight when level updates
      const flashAlpha = flashFor(row)
      if (flashAlpha > 0) {
        ctx.fillStyle = side === 'ask'
          ? `rgba(224,168,56,${flashAlpha * 0.18})`
          : `rgba(52,211,153,${flashAlpha * 0.16})`
        ctx.fillRect(0, y, w, ROW_H)
      }
      // Hover: flood-fill all levels from mid price to hovered row
      if (hover?.side === side && rowIndex <= hover.rowIndex) {
        ctx.fillStyle = side === 'ask' ? 'rgba(190,90,30,0.08)' : 'rgba(0,160,120,0.07)'
        ctx.fillRect(0, y, w, ROW_H)
      }
      // Brighter highlight on the exact hovered row
      const isExact = hover?.side === side && rowIndex === hover.rowIndex

      const heatRatio = scaleMax > 0 ? Math.min(1, row.size / scaleMax) : 0.5
      ctx.fillStyle = row.whale ? WHALE_FILL : gradColor(stops, heatRatio)
      ctx.fillRect(0, y, HEAT_W, ROW_H)

      ctx.fillStyle = row.whale ? WHALE_FILL : gradColor(stops, heatRatio)
      ctx.fillRect(barX, y, barWidth, ROW_H)

      ctx.font = FONT
      ctx.textBaseline = 'middle'
      ctx.textAlign = 'left'
      ctx.fillStyle = isExact
        ? (side === 'ask' ? '#f59e44' : '#34d399')
        : isBold(row.price) ? colors.primary : colors.tertiary
      ctx.fillText(fmtPrice(row.price), HEAT_W + 6, y + ROW_H / 2 + 1)

      ctx.textAlign = 'right'
      ctx.fillStyle = isExact ? colors.primary : colors.quaternary
      ctx.fillText(fmtSize(sizeFor(row, rowIndex, side)), HEAT_W + PW + SW - 6, y + ROW_H / 2 + 1)

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
    if (rafRef.current) return
    lastFrameRef.current = performance.now()
    const frame = (now: number) => {
      const dt = Math.min(48, now - lastFrameRef.current)
      lastFrameRef.current = now
      const ease = 1 - Math.exp(-dt / 110)
      for (const v of animRef.current.values()) {
        // Smooth bar/step toward real target
        v.barCur  += (v.barTgt  - v.barCur)  * ease
        v.stepCur += (v.stepTgt - v.stepCur) * ease
        // Each level independently fires ~once every 1.5s (Poisson)
        if (Math.random() < dt * 0.00065) {
          v.noiseTgt = (Math.random() - 0.5) * 0.22  // ±11% size jump
          v.flash = 220                                // ms highlight
        }
        // Smooth noise toward its target (fast settle)
        v.noise += (v.noiseTgt - v.noise) * (1 - Math.exp(-dt / 60))
        if (v.flash > 0) v.flash -= dt
      }
      paint()
      rafRef.current = requestAnimationFrame(frame)
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
  const distFromMid = Math.abs(price - mark) / mark * 100
  const flipLeft = x > 120
  const accent = side === 'ask' ? '#f59e44' : '#34d399'

  return (
    <div
      className="pointer-events-none absolute z-20 rounded-[5px] border border-border-subtle bg-surface-card px-2 py-1 shadow-lg"
      style={{
        top: Math.max(0, y + 1),
        ...(flipLeft ? { right: 4 } : { left: x + 10 }),
      }}
    >
      <div className="flex items-center gap-2.5">
        <span className="text-[9px] font-semibold" style={{ color: accent }}>{fmtPrice(price)}</span>
        <span className="text-[9px] text-text-quaternary">{fmtSize(size)}</span>
        <span className="text-[9px] text-text-quaternary">∑{fmtSize(cumulative)}</span>
        <span className="text-[9px] text-text-quaternary">{distFromMid.toFixed(2)}%</span>
      </div>
    </div>
  )
}
