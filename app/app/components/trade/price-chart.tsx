import { lazy, Suspense, useEffect, useRef, useState } from 'react'
const FootprintChart = lazy(() => import('./footprint-chart'))
import {
  CandlestickSeries,
  ColorType,
  createChart,
  CrosshairMode,
  HistogramSeries,
  LineSeries,
  LineStyle,
  type BarData,
  type IChartApi,
  type IPriceLine,
  type ISeriesApi,
  type UTCTimestamp,
} from 'lightweight-charts'
import {
  IconActivity,
  IconBolt,
  IconChartCandle,
  IconFocusCentered,
  IconGridDots,
  IconLine,
  IconRectangle,
  IconTrendingUp,
} from '@tabler/icons-react'
import { useLevels } from '../../context/levels-context'
import { MARKET_CATALOG, type Candle as MarketCandle, useMarket } from '../../context/market-context'
import { useTheme } from '../../context/theme-context'

// ─── Order-book-matched candle palette ───────────────────────────────────────
const UP       = '#34d399'
const DOWN     = '#c8461e'
const UP_FILL  = 'rgba(0,0,0,0)'
const ENTRY    = '#a78bfa'
const MA_FAST  = '#e0a838'
const MA_SLOW  = '#60a5fa'
const CROSS    = 'rgba(167,139,250,0.55)'
const RSI_COL  = '#a78bfa'
const BB_COL   = 'rgba(167,139,250,0.6)'

// ─── Per-theme chart surface ──────────────────────────────────────────────────
const chartTheme = {
  gruvbox:   { panel: '#fbf1c7', toolbar: 'bg-surface-primary', grid: 'rgba(60,56,54,0.10)',    gridOff: 'rgba(60,56,54,0)',    text: 'rgba(60,56,54,0.72)',    border: 'rgba(60,56,54,0.16)'    },
  light:     { panel: '#ffffff', toolbar: 'bg-surface-primary', grid: 'rgba(17,24,39,0.07)',    gridOff: 'rgba(17,24,39,0)',    text: 'rgba(17,24,39,0.68)',    border: 'rgba(17,24,39,0.10)'    },
  dark:      { panel: '#06070d', toolbar: 'bg-[#0a0b12]',       grid: 'rgba(255,255,255,0.04)', gridOff: 'rgba(255,255,255,0)', text: 'rgba(255,255,255,0.70)', border: 'rgba(255,255,255,0.08)' },
  nord:      { panel: '#eceff4', toolbar: 'bg-surface-primary', grid: 'rgba(46,52,64,0.10)',    gridOff: 'rgba(46,52,64,0)',    text: 'rgba(46,52,64,0.72)',    border: 'rgba(46,52,64,0.14)'    },
  solarized: { panel: '#fdf6e3', toolbar: 'bg-surface-primary', grid: 'rgba(7,54,66,0.10)',     gridOff: 'rgba(7,54,66,0)',     text: 'rgba(7,54,66,0.72)',     border: 'rgba(7,54,66,0.14)'     },
  tokyo:     { panel: '#1a1b26', toolbar: 'bg-surface-primary', grid: 'rgba(192,202,245,0.07)', gridOff: 'rgba(192,202,245,0)', text: 'rgba(192,202,245,0.72)', border: 'rgba(192,202,245,0.10)' },
  dracula:   { panel: '#282a36', toolbar: 'bg-surface-primary', grid: 'rgba(248,248,242,0.07)', gridOff: 'rgba(248,248,242,0)', text: 'rgba(248,248,242,0.72)', border: 'rgba(248,248,242,0.10)' },
  matrix:    { panel: '#07110d', toolbar: 'bg-surface-primary', grid: 'rgba(105,240,174,0.08)', gridOff: 'rgba(105,240,174,0)', text: 'rgba(216,255,231,0.72)', border: 'rgba(105,240,174,0.12)' },
  sepia:     { panel: '#fff4dc', toolbar: 'bg-surface-primary', grid: 'rgba(47,38,31,0.10)',    gridOff: 'rgba(47,38,31,0)',    text: 'rgba(47,38,31,0.72)',    border: 'rgba(47,38,31,0.13)'    },
  slate:     { panel: '#171d23', toolbar: 'bg-surface-primary', grid: 'rgba(240,244,248,0.07)', gridOff: 'rgba(240,244,248,0)', text: 'rgba(240,244,248,0.72)', border: 'rgba(240,244,248,0.09)' },
  contrast:  { panel: '#ffffff', toolbar: 'bg-surface-primary', grid: 'rgba(0,0,0,0.13)',       gridOff: 'rgba(0,0,0,0)',       text: 'rgba(0,0,0,0.82)',       border: 'rgba(0,0,0,0.18)'       },
} as const

const INTERVALS = [
  { label: '15m', bucket: 60 * 15 },
  { label: '30m', bucket: 60 * 30 },
  { label: '1h',  bucket: 60 * 60 },
  { label: '4h',  bucket: 60 * 60 * 4 },
] as const

interface Candle {
  time: UTCTimestamp
  open: number; high: number; low: number; close: number; volume: number
}

// ─── Indicator calculations ───────────────────────────────────────────────────
function calcRSI(data: Candle[], period = 14) {
  const out: { time: UTCTimestamp; value: number }[] = []
  let avgGain = 0, avgLoss = 0
  for (let i = 1; i < data.length; i++) {
    const change = data[i]!.close - data[i - 1]!.close
    const gain = Math.max(0, change)
    const loss = Math.max(0, -change)
    if (i <= period) {
      avgGain += gain / period
      avgLoss += loss / period
      if (i === period) {
        const rs = avgLoss === 0 ? 100 : avgGain / avgLoss
        out.push({ time: data[i]!.time, value: +(100 - 100 / (1 + rs)).toFixed(2) })
      }
    } else {
      avgGain = (avgGain * (period - 1) + gain) / period
      avgLoss = (avgLoss * (period - 1) + loss) / period
      const rs = avgLoss === 0 ? 100 : avgGain / avgLoss
      out.push({ time: data[i]!.time, value: +(100 - 100 / (1 + rs)).toFixed(2) })
    }
  }
  return out
}

function calcBB(data: Candle[], period = 20, mult = 2) {
  const upper: { time: UTCTimestamp; value: number }[] = []
  const middle: { time: UTCTimestamp; value: number }[] = []
  const lower: { time: UTCTimestamp; value: number }[] = []
  for (let i = period - 1; i < data.length; i++) {
    const slice = data.slice(i - period + 1, i + 1)
    const sma = slice.reduce((s, c) => s + c.close, 0) / period
    const std = Math.sqrt(slice.reduce((s, c) => s + (c.close - sma) ** 2, 0) / period)
    upper.push({ time: data[i]!.time, value: +(sma + mult * std).toFixed(4) })
    middle.push({ time: data[i]!.time, value: +sma.toFixed(4) })
    lower.push({ time: data[i]!.time, value: +(sma - mult * std).toFixed(4) })
  }
  return { upper, middle, lower }
}

function volColor(c: Candle): string {
  const body = Math.abs(c.close - c.open)
  const range = Math.max(c.high - c.low, 0.001)
  const strength = Math.min(1, body / range)
  const alpha = (0.22 + strength * 0.45).toFixed(2)
  return c.close >= c.open
    ? `rgba(52,211,153,${alpha})`
    : `rgba(200,70,30,${alpha})`
}

function fmtVol(v: number) {
  if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(2)}M`
  if (v >= 1_000)     return `${(v / 1_000).toFixed(1)}K`
  return v.toFixed(0)
}

function bucketCandles(points: MarketCandle[], bucket: number): Candle[] {
  const map = new Map<number, Candle>()
  for (const p of points) {
    const t = Math.floor(p.time / bucket) * bucket
    const c = map.get(t)
    if (!c) {
      map.set(t, { time: t as UTCTimestamp, open: p.open, high: p.high, low: p.low, close: p.close, volume: p.volume })
    } else {
      c.high = Math.max(c.high, p.high); c.low = Math.min(c.low, p.low)
      c.close = p.close; c.volume += p.volume
    }
  }
  return [...map.values()].sort((a, b) => (a.time as number) - (b.time as number))
}

// Anchored VWAP + ±1σ / ±2σ deviation bands
function calcVWAP(data: Candle[]) {
  const vwap: { time: UTCTimestamp; value: number }[] = []
  const u1:   { time: UTCTimestamp; value: number }[] = []
  const l1:   { time: UTCTimestamp; value: number }[] = []
  const u2:   { time: UTCTimestamp; value: number }[] = []
  const l2:   { time: UTCTimestamp; value: number }[] = []
  let sv = 0, spv = 0, spv2 = 0
  for (const c of data) {
    const tp = (c.high + c.low + c.close) / 3
    sv += c.volume; spv += tp * c.volume; spv2 += tp * tp * c.volume
    if (sv === 0) continue
    const v  = spv / sv
    const sd = Math.sqrt(Math.max(0, spv2 / sv - v * v))
    vwap.push({ time: c.time, value: +v.toFixed(4) })
    u1.push({ time: c.time, value: +(v + sd).toFixed(4) })
    l1.push({ time: c.time, value: +(v - sd).toFixed(4) })
    u2.push({ time: c.time, value: +(v + 2 * sd).toFixed(4) })
    l2.push({ time: c.time, value: +(v - 2 * sd).toFixed(4) })
  }
  return { vwap, u1, l1, u2, l2 }
}

function movingAverage(data: Candle[], length: number) {
  const out: { time: UTCTimestamp; value: number }[] = []
  let sum = 0
  data.forEach((c, i) => {
    sum += c.close
    if (i >= length) sum -= data[i - length]!.close
    if (i >= length - 1) out.push({ time: c.time, value: +( sum / length).toFixed(4) })
  })
  return out
}

// ─── Volume Profile Primitive (draws on chart's own canvas via pane primitive) ─
type VPData = {
  bkts: Map<number, { buy: number; sell: number }>
  lo: number; hi: number; maxB: number; pocKey: number; tick: number
}

class VPRenderer {
  constructor(private _d: VPData) {}
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  draw(target: any): void {
    target.useMediaCoordinateSpace(({ context: ctx, mediaSize }: { context: CanvasRenderingContext2D; mediaSize: { width: number; height: number } }) => {
      const { bkts, lo, hi, maxB, pocKey, tick } = this._d
      if (!bkts.size || hi <= lo) return

      const H      = mediaSize.height
      const VW     = 44                        // narrower strip
      const right  = mediaSize.width           // right edge of pane (touches price axis)
      const left   = right - VW
      const topOff = H * 0.08
      const dataH  = H * 0.70
      const span   = hi - lo
      const yOf    = (p: number) => topOff + dataH * (1 - (p - lo) / span)
      const GAP    = 0.5

      // Subtle strip backdrop
      ctx.fillStyle = 'rgba(0,0,0,0.10)'
      ctx.fillRect(left, topOff, VW, dataH)

      for (const [k, b] of bkts.entries()) {
        const price  = k * tick
        const yT     = yOf(price + tick)
        const yB     = yOf(price)
        const cellH  = Math.max(1.5, yB - yT)
        if (yB < 0 || yT > H) continue

        const total  = b.buy + b.sell
        const barLen = (total / maxB) * (VW - 2)
        const sellW  = ((b.sell / total) * barLen)
        const buyW   = barLen - sellW
        const isPOC  = k === pocKey

        // POC: full-width glow highlight
        if (isPOC) {
          ctx.fillStyle = 'rgba(200,220,50,0.12)'
          ctx.fillRect(left, yT, VW, cellH)
        }

        // Bars grow LEFT from the right edge (right-anchored, like TradingView VP)
        // Sell portion (right, rust)
        ctx.fillStyle = isPOC ? 'rgba(220,160,30,0.92)' : 'rgba(200,72,32,0.68)'
        ctx.fillRect(right - sellW, yT + GAP, sellW, cellH - GAP * 2)
        // Buy portion (left of sell, teal)
        ctx.fillStyle = isPOC ? 'rgba(200,230,50,0.92)' : 'rgba(48,210,150,0.72)'
        ctx.fillRect(right - barLen, yT + GAP, buyW, cellH - GAP * 2)
      }
    })
  }
}

class VPPaneView {
  constructor(private _d: VPData) {}
  zOrder() { return 'top' as const }
  renderer() { return new VPRenderer(this._d) }
}

class VolProfilePrimitive {
  private _d: VPData | null = null
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  private _api: any = null
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  attached(params: any) { this._api = params }
  detached() { this._api = null }
  update(d: VPData | null) { this._d = d; this._api?.requestUpdate() }
  paneViews() { return this._d ? [new VPPaneView(this._d)] : [] }
}
// ─────────────────────────────────────────────────────────────────────────────

function usePriceLine(
  seriesRef: React.RefObject<ISeriesApi<'Candlestick'> | null>,
  color: string, title: string, style: LineStyle = LineStyle.Dashed,
) {
  const ref = useRef<IPriceLine | null>(null)
  const set = (price: number | null) => {
    const s = seriesRef.current; if (!s) return
    if (ref.current) { s.removePriceLine(ref.current); ref.current = null }
    if (price !== null)
      ref.current = s.createPriceLine({ price, color, lineWidth: 1, lineStyle: style, axisLabelVisible: true, title, axisLabelColor: color, axisLabelTextColor: '#05060a' })
  }
  return { set }
}

// ─── Volume profile utility ───────────────────────────────────────────────────
function vpStep(range: number, n = 38) {
  const raw = range / n, exp = Math.floor(Math.log10(Math.max(raw, 1e-10))), b = Math.pow(10, exp)
  return raw / b < 1.5 ? b : raw / b < 3.5 ? 2 * b : raw / b < 7.5 ? 5 * b : 10 * b
}

// ─── Component ────────────────────────────────────────────────────────────────
export default function PriceChart() {
  const wrapRef    = useRef<HTMLDivElement>(null)
  const chartRef   = useRef<IChartApi | null>(null)
  const candleRef  = useRef<ISeriesApi<'Candlestick'> | null>(null)
  const volumeRef  = useRef<ISeriesApi<'Histogram'> | null>(null)
  const maFastRef  = useRef<ISeriesApi<'Line'> | null>(null)
  const maSlowRef  = useRef<ISeriesApi<'Line'> | null>(null)
  const rsiRef     = useRef<ISeriesApi<'Line'> | null>(null)
  const bbUpperRef = useRef<ISeriesApi<'Line'> | null>(null)
  const bbMidRef   = useRef<ISeriesApi<'Line'> | null>(null)
  const bbLowRef   = useRef<ISeriesApi<'Line'> | null>(null)
  const vwapRef    = useRef<ISeriesApi<'Line'> | null>(null)
  const vwapU1Ref  = useRef<ISeriesApi<'Line'> | null>(null)
  const vwapL1Ref  = useRef<ISeriesApi<'Line'> | null>(null)
  const vwapU2Ref  = useRef<ISeriesApi<'Line'> | null>(null)
  const vwapL2Ref  = useRef<ISeriesApi<'Line'> | null>(null)
  const pocLineRef = useRef<IPriceLine | null>(null)
  const vahLineRef = useRef<IPriceLine | null>(null)
  const valLineRef = useRef<IPriceLine | null>(null)
  const lastDataRef       = useRef<Candle[]>([])
  const lastBucketRef     = useRef<number | null>(null)
  const lastIntervalRef   = useRef<string>(INTERVALS[0].label)
  const vpPrimRef         = useRef<VolProfilePrimitive | null>(null)
  const showVPVRRef       = useRef(true)

  // Tooltip refs (updated imperatively to avoid 60fps re-renders)
  const tooltipRef = useRef<HTMLDivElement>(null)
  const ttTime = useRef<HTMLSpanElement>(null)
  const ttDelta = useRef<HTMLSpanElement>(null)
  const ttO = useRef<HTMLSpanElement>(null)
  const ttH = useRef<HTMLSpanElement>(null)
  const ttL = useRef<HTMLSpanElement>(null)
  const ttC = useRef<HTMLSpanElement>(null)
  const ttV = useRef<HTMLSpanElement>(null)

  const [interval,      setInterval]      = useState<(typeof INTERVALS)[number]>(INTERVALS[0])
  const [last,          setLast]          = useState<Candle | null>(null)
  const [showVolume,    setShowVolume]    = useState(true)
  const [showGrid,      setShowGrid]      = useState(true)
  const [showMA,        setShowMA]        = useState(false)
  const [showRSI,       setShowRSI]       = useState(false)
  const [showBB,        setShowBB]        = useState(false)
  const [showFP,        setShowFP]        = useState(false)
  const [showVPVR,      setShowVPVR]      = useState(true)
  const [showVWAP,      setShowVWAP]      = useState(true)
  const [autoFit,       setAutoFit]       = useState(true)

  showVPVRRef.current = showVPVR
  const [hollowCandles, setHollowCandles] = useState(true)

  const { symbol, candles, index, funding } = useMarket()
  const { tp, sl, entry } = useLevels()
  const { theme } = useTheme()
  const colors = chartTheme[theme]
  const market = MARKET_CATALOG.find((m) => m.symbol === symbol) ?? MARKET_CATALOG[0]!

  const tpLine    = usePriceLine(candleRef, UP,    'TP')
  const slLine    = usePriceLine(candleRef, DOWN,  'SL')
  const entryLine = usePriceLine(candleRef, ENTRY, 'Entry', LineStyle.Solid)

  // ── Volume Profile — feeds data into the pane primitive ─────────────────────
  function updateVP() {
    const prim  = vpPrimRef.current
    const chart = chartRef.current
    if (!prim || !chart) return

    if (!showVPVRRef.current) { prim.update(null); return }

    const data = lastDataRef.current
    if (!data.length) return
    const lr = chart.timeScale().getVisibleLogicalRange()
    if (!lr) return

    const fromIdx = Math.max(0, Math.floor(lr.from))
    const toIdx   = Math.min(data.length - 1, Math.ceil(lr.to))
    if (fromIdx > toIdx) return

    let lo = Infinity, hi = -Infinity
    for (let i = fromIdx; i <= toIdx; i++) {
      lo = Math.min(lo, data[i]!.low); hi = Math.max(hi, data[i]!.high)
    }
    if (!isFinite(lo) || hi <= lo) return

    const tick = vpStep(hi - lo, 22) // fewer buckets → taller bars
    const bkts = new Map<number, { buy: number; sell: number }>()
    for (let i = fromIdx; i <= toIdx; i++) {
      const c = data[i]!
      const bi = Math.round((c.high + c.low) / 2 / tick)
      const b  = bkts.get(bi) ?? { buy: 0, sell: 0 }
      if (c.close >= c.open) b.buy += c.volume; else b.sell += c.volume
      bkts.set(bi, b)
    }
    if (!bkts.size) return

    let maxB = 1, pocKey = -1, pocV = 0, totalVol = 0
    for (const [k, b] of bkts.entries()) {
      const t = b.buy + b.sell; totalVol += t
      if (t > maxB) maxB = t; if (t > pocV) { pocV = t; pocKey = k }
    }

    prim.update({ bkts, lo, hi, maxB, pocKey, tick })

    // POC / VAH / VAL price lines
    const sorted = [...bkts.entries()].sort((a, b) => (b[1].buy + b[1].sell) - (a[1].buy + a[1].sell))
    let accVol = 0; const vaKeys: number[] = []
    for (const [k, b] of sorted) {
      accVol += b.buy + b.sell; vaKeys.push(k)
      if (accVol >= totalVol * 0.70) break
    }
    const vahKey = vaKeys.length ? Math.max(...vaKeys) : -1
    const valKey = vaKeys.length ? Math.min(...vaKeys) : -1
    const cs = candleRef.current
    if (cs) {
      if (pocLineRef.current) { cs.removePriceLine(pocLineRef.current); pocLineRef.current = null }
      if (vahLineRef.current) { cs.removePriceLine(vahLineRef.current); vahLineRef.current = null }
      if (valLineRef.current) { cs.removePriceLine(valLineRef.current); valLineRef.current = null }
      if (pocKey >= 0) pocLineRef.current = cs.createPriceLine({ price: pocKey * tick + tick / 2, color: 'rgba(200,220,50,0.70)', lineWidth: 1, lineStyle: LineStyle.Dashed, axisLabelVisible: true, title: 'POC', axisLabelColor: 'rgba(200,220,50,0.85)', axisLabelTextColor: '#000' })
      if (vahKey >= 0) vahLineRef.current = cs.createPriceLine({ price: vahKey * tick + tick, color: 'rgba(52,211,153,0.45)', lineWidth: 1, lineStyle: LineStyle.Dotted, axisLabelVisible: true, title: 'VAH', axisLabelColor: 'rgba(52,211,153,0.6)', axisLabelTextColor: '#000' })
      if (valKey >= 0) valLineRef.current = cs.createPriceLine({ price: valKey * tick, color: 'rgba(200,70,30,0.45)', lineWidth: 1, lineStyle: LineStyle.Dotted, axisLabelVisible: true, title: 'VAL', axisLabelColor: 'rgba(200,70,30,0.6)', axisLabelTextColor: '#000' })
    }
  }

  // ── Create chart ────────────────────────────────────────────────────────────
  useEffect(() => {
    if (!wrapRef.current) return
    const chart = createChart(wrapRef.current, {
      autoSize: true,
      layout: { background: { type: ColorType.Solid, color: colors.panel }, textColor: colors.text, fontFamily: 'var(--font-mono)', fontSize: 11, attributionLogo: false },
      grid: { vertLines: { color: colors.grid }, horzLines: { color: colors.grid } },
      crosshair: { mode: CrosshairMode.Normal, vertLine: { color: CROSS, width: 1, style: LineStyle.Dashed, labelBackgroundColor: ENTRY }, horzLine: { color: CROSS, width: 1, style: LineStyle.Dashed, labelBackgroundColor: ENTRY } },
      rightPriceScale: { borderColor: colors.border, scaleMargins: { top: 0.08, bottom: 0.22 } },
      timeScale: { borderColor: colors.border, timeVisible: true, secondsVisible: false, rightOffset: 8, barSpacing: 8 },
    })

    const candlesSeries = chart.addSeries(CandlestickSeries, {
      upColor: UP_FILL, downColor: DOWN,
      wickUpColor: UP, wickDownColor: DOWN,
      borderUpColor: UP, borderDownColor: DOWN,
      priceLineVisible: true, priceLineColor: 'rgba(167,139,250,0.7)',
      priceLineWidth: 1, priceLineStyle: LineStyle.Dashed,
      lastValueVisible: true,
      priceFormat: { type: 'price', precision: 2, minMove: 0.01 },
    })

    const volumeSeries = chart.addSeries(HistogramSeries, { priceFormat: { type: 'volume' }, priceScaleId: '' })
    volumeSeries.priceScale().applyOptions({ scaleMargins: { top: 0.88, bottom: 0 } })

    const maFast = chart.addSeries(LineSeries, { color: MA_FAST, lineWidth: 1, priceLineVisible: false, lastValueVisible: false })
    const maSlow = chart.addSeries(LineSeries, { color: MA_SLOW, lineWidth: 1, priceLineVisible: false, lastValueVisible: false })

    chartRef.current  = chart
    candleRef.current = candlesSeries
    volumeRef.current = volumeSeries
    maFastRef.current = maFast
    maSlowRef.current = maSlow

    // Crosshair → update tooltip imperatively
    chart.subscribeCrosshairMove((param) => {
      const tip = tooltipRef.current
      if (!tip) return

      if (!param.time || !param.point || param.point.x < 0) {
        tip.style.display = 'none'; return
      }

      const raw = param.seriesData.get(candlesSeries)
      if (!raw || !('open' in raw)) { tip.style.display = 'none'; return }
      const bar = raw as BarData

      const delta = bar.close - bar.open
      const pct   = (delta / bar.open) * 100
      const isUp  = delta >= 0
      const col   = isUp ? UP : DOWN
      const d = new Date((param.time as number) * 1000)
      const dateStr = d.toLocaleString('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false })

      if (ttTime.current)  ttTime.current.textContent = dateStr
      if (ttDelta.current) { ttDelta.current.textContent = `${isUp ? '+' : ''}${delta.toFixed(2)} (${isUp ? '+' : ''}${pct.toFixed(2)}%)`; ttDelta.current.style.color = col }
      if (ttO.current)     ttO.current.textContent = bar.open.toFixed(2)
      if (ttH.current)     ttH.current.textContent = bar.high.toFixed(2)
      if (ttL.current)     ttL.current.textContent = bar.low.toFixed(2)
      if (ttC.current)     ttC.current.textContent = bar.close.toFixed(2)

      // find volume from volume series
      const volRaw = param.seriesData.get(volumeSeries)
      if (ttV.current && volRaw && 'value' in volRaw) {
        ttV.current.textContent = fmtVol((volRaw as { value: number }).value)
      }

      // position tooltip
      const wrap = wrapRef.current!
      const TW = 160, TH = 90
      let x = param.point.x + 14
      let y = param.point.y - 10
      if (x + TW > wrap.clientWidth - 4)  x = param.point.x - TW - 10
      if (y + TH > wrap.clientHeight - 4) y = wrap.clientHeight - TH - 4
      if (y < 0) y = 4

      tip.style.left    = `${x}px`
      tip.style.top     = `${y}px`
      tip.style.display = 'block'
    })

    // Attach volume profile primitive to the main pane
    const vp = new VolProfilePrimitive()
    chart.panes()[0]?.attachPrimitive(vp)
    vpPrimRef.current = vp

    chart.timeScale().subscribeVisibleLogicalRangeChange(updateVP)

    return () => {
      chart.timeScale().unsubscribeVisibleLogicalRangeChange(updateVP)
      chart.panes()[0]?.detachPrimitive(vp)
      vpPrimRef.current = null
      chart.remove()
      chartRef.current = candleRef.current = volumeRef.current = maFastRef.current = maSlowRef.current = null
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // ── Theme sync ──────────────────────────────────────────────────────────────
  useEffect(() => {
    chartRef.current?.applyOptions({
      layout: { background: { type: ColorType.Solid, color: colors.panel }, textColor: colors.text },
      grid: { vertLines: { color: showGrid ? colors.grid : colors.gridOff }, horzLines: { color: showGrid ? colors.grid : colors.gridOff } },
      rightPriceScale: { borderColor: colors.border },
      timeScale: { borderColor: colors.border },
    })
  }, [colors, showGrid])

  useEffect(() => { updateVP() }, [showVPVR])

  // ── Candle data + indicators ────────────────────────────────────────────────
  useEffect(() => {
    const data = bucketCandles(candles, interval.bucket)
    if (!data.length) return
    lastDataRef.current = data

    const latest          = data[data.length - 1]!
    const isFirstLoad     = lastBucketRef.current === null
    const intervalChanged = lastIntervalRef.current !== interval.label
    const newBucket       = lastBucketRef.current !== (latest.time as number)
    const shouldReset     = intervalChanged || newBucket || isFirstLoad

    const volData = showVolume ? data.map((c) => ({ time: c.time, value: c.volume, color: volColor(c) })) : []

    if (shouldReset) {
      candleRef.current?.setData(data)
      volumeRef.current?.setData(volData)
      maFastRef.current?.setData(showMA ? movingAverage(data, 9)  : [])
      maSlowRef.current?.setData(showMA ? movingAverage(data, 21) : [])
      if (autoFit && (intervalChanged || isFirstLoad)) chartRef.current?.timeScale().fitContent()
    } else {
      candleRef.current?.update(latest)
      if (showVolume) volumeRef.current?.update({ time: latest.time, value: latest.volume, color: volColor(latest) })
      if (showMA) { maFastRef.current?.setData(movingAverage(data, 9)); maSlowRef.current?.setData(movingAverage(data, 21)) }
    }

    // Update optional indicators without needing them in deps
    if (rsiRef.current)     rsiRef.current.setData(calcRSI(data))
    if (bbUpperRef.current) { const bb = calcBB(data); bbUpperRef.current.setData(bb.upper); bbMidRef.current?.setData(bb.middle); bbLowRef.current?.setData(bb.lower) }
    if (vwapRef.current)    { const w = calcVWAP(data); vwapRef.current.setData(w.vwap); vwapU1Ref.current?.setData(w.u1); vwapL1Ref.current?.setData(w.l1); vwapU2Ref.current?.setData(w.u2); vwapL2Ref.current?.setData(w.l2) }

    lastIntervalRef.current = interval.label
    lastBucketRef.current   = latest.time as number
    setLast(latest)
    updateVP()
  }, [autoFit, candles, interval, showMA, showVolume])

  // ── RSI pane toggle ─────────────────────────────────────────────────────────
  useEffect(() => {
    const chart = chartRef.current
    if (!chart) return

    if (showRSI) {
      chart.addPane()
      const series = chart.addSeries(LineSeries, {
        color: RSI_COL, lineWidth: 1,
        priceLineVisible: false, lastValueVisible: true,
        crosshairMarkerRadius: 3,
        autoscaleInfoProvider: () => ({ priceRange: { minValue: 0, maxValue: 100 }, margins: { above: 0.08, below: 0.08 } }),
      }, 1)
      series.createPriceLine({ price: 70, color: `${DOWN}88`, lineWidth: 1, lineStyle: LineStyle.Dashed, axisLabelVisible: false, title: 'OB' })
      series.createPriceLine({ price: 30, color: `${UP}88`,   lineWidth: 1, lineStyle: LineStyle.Dashed, axisLabelVisible: false, title: 'OS' })
      series.createPriceLine({ price: 50, color: 'rgba(255,255,255,0.12)', lineWidth: 1, lineStyle: LineStyle.Solid, axisLabelVisible: false, title: '' })
      chart.panes()[1]?.setHeight(90)
      rsiRef.current = series
      const data = lastDataRef.current
      if (data.length) series.setData(calcRSI(data))
    } else {
      if (rsiRef.current) { chart.removeSeries(rsiRef.current); rsiRef.current = null }
    }
  }, [showRSI])

  // ── Bollinger Bands toggle ──────────────────────────────────────────────────
  useEffect(() => {
    const chart = chartRef.current
    if (!chart) return

    if (showBB) {
      const shared = { priceLineVisible: false, lastValueVisible: false, crosshairMarkerVisible: false }
      bbUpperRef.current = chart.addSeries(LineSeries, { color: BB_COL, lineWidth: 1, lineStyle: LineStyle.Dashed, ...shared })
      bbMidRef.current   = chart.addSeries(LineSeries, { color: 'rgba(167,139,250,0.35)', lineWidth: 1, ...shared })
      bbLowRef.current   = chart.addSeries(LineSeries, { color: BB_COL, lineWidth: 1, lineStyle: LineStyle.Dashed, ...shared })
      const data = lastDataRef.current
      if (data.length) {
        const bb = calcBB(data)
        bbUpperRef.current.setData(bb.upper)
        bbMidRef.current.setData(bb.middle)
        bbLowRef.current.setData(bb.lower)
      }
    } else {
      const chart2 = chartRef.current
      if (!chart2) return
      if (bbUpperRef.current) { chart2.removeSeries(bbUpperRef.current); bbUpperRef.current = null }
      if (bbMidRef.current)   { chart2.removeSeries(bbMidRef.current);   bbMidRef.current   = null }
      if (bbLowRef.current)   { chart2.removeSeries(bbLowRef.current);   bbLowRef.current   = null }
    }
  }, [showBB])

  // ── VWAP + deviation bands ───────────────────────────────────────────────────
  useEffect(() => {
    const chart = chartRef.current
    if (!chart) return
    const shared = { priceLineVisible: false, lastValueVisible: false, crosshairMarkerVisible: false }
    if (showVWAP) {
      vwapRef.current   = chart.addSeries(LineSeries, { color: '#e0a838', lineWidth: 1, ...shared })
      vwapU1Ref.current = chart.addSeries(LineSeries, { color: 'rgba(224,168,56,0.45)', lineWidth: 1, lineStyle: LineStyle.Dashed, ...shared })
      vwapL1Ref.current = chart.addSeries(LineSeries, { color: 'rgba(224,168,56,0.45)', lineWidth: 1, lineStyle: LineStyle.Dashed, ...shared })
      vwapU2Ref.current = chart.addSeries(LineSeries, { color: 'rgba(224,168,56,0.20)', lineWidth: 1, lineStyle: LineStyle.Dotted, ...shared })
      vwapL2Ref.current = chart.addSeries(LineSeries, { color: 'rgba(224,168,56,0.20)', lineWidth: 1, lineStyle: LineStyle.Dotted, ...shared })
      const data = lastDataRef.current
      if (data.length) {
        const w = calcVWAP(data)
        vwapRef.current.setData(w.vwap); vwapU1Ref.current.setData(w.u1); vwapL1Ref.current.setData(w.l1)
        vwapU2Ref.current.setData(w.u2); vwapL2Ref.current.setData(w.l2)
      }
    } else {
      const c2 = chartRef.current; if (!c2) return
      ;[vwapRef, vwapU1Ref, vwapL1Ref, vwapU2Ref, vwapL2Ref].forEach((r) => {
        if (r.current) { c2.removeSeries(r.current); r.current = null }
      })
    }
  }, [showVWAP])

  useEffect(() => {
    candleRef.current?.applyOptions({ upColor: hollowCandles ? UP_FILL : UP, downColor: DOWN, borderUpColor: UP, borderDownColor: DOWN, wickUpColor: UP, wickDownColor: DOWN })
  }, [hollowCandles])

  useEffect(() => { tpLine.set(tp) },       [tp])
  useEffect(() => { slLine.set(sl) },       [sl])
  useEffect(() => { entryLine.set(entry) }, [entry])

  const delta    = last ? last.close - last.open : 0
  const deltaPct = last ? (delta / last.open) * 100 : 0
  const isUp     = delta >= 0
  const tvSymbol = symbol.replace('-PERP', 'USD')

  return (
    <div className="flex h-full min-h-0 flex-col bg-surface-primary">
      {/* ── Toolbar ── */}
      <div className={`flex min-h-[78px] shrink-0 flex-col border-b border-border-subtle ${colors.toolbar}`}>
        <div className="flex h-10 items-center gap-2 px-3">
          {/* Symbol chip */}
          <div className="flex items-center gap-2 rounded-[7px] border border-border-subtle bg-surface-card px-2.5 py-1.5">
            <span className="grid h-4 w-4 place-items-center rounded-full text-[8px] font-bold text-white" style={{ backgroundColor: market.color }}>
              {market.icon.length <= 2 ? market.icon : ''}
            </span>
            <span className="text-[13px] font-semibold text-text-primary">{tvSymbol}</span>
          </div>

          {/* Intervals */}
          <div className="flex items-center gap-0.5">
            {INTERVALS.map((item) => (
              <button key={item.label} onClick={() => setInterval(item)}
                className={`rounded-[5px] px-2.5 py-1.5 text-[12px] font-medium transition-colors ${interval.label === item.label ? 'bg-brand-violet text-white' : 'text-text-tertiary hover:bg-surface-hover hover:text-text-primary'}`}>
                {item.label}
              </button>
            ))}
          </div>

          <div className="h-5 w-px bg-border-subtle" />

          <Btn active title="Candles"><IconChartCandle size={14} stroke={1.8} /></Btn>
          <Btn active={hollowCandles} onClick={() => setHollowCandles(v => !v)} title="Hollow candles">
            <IconRectangle size={14} stroke={1.8} /><span>Hollow</span>
          </Btn>
          <Btn active={showMA} onClick={() => setShowMA(v => !v)} title="Moving averages (9 / 21)">
            <IconTrendingUp size={14} stroke={1.8} /><span>MA</span>
          </Btn>
          <Btn active={showBB} onClick={() => setShowBB(v => !v)} title="Bollinger Bands (20, 2σ)">
            <IconBolt size={14} stroke={1.8} /><span>BB</span>
          </Btn>
          <Btn active={showVWAP} onClick={() => setShowVWAP(v => !v)} title="VWAP + ±1σ / ±2σ bands">
            <span className="text-[10px] font-bold">VWAP</span>
          </Btn>
          <Btn active={showRSI} onClick={() => setShowRSI(v => !v)} title="RSI (14)">
            <span className="text-[10px] font-bold">RSI</span>
          </Btn>
          <div className="h-5 w-px bg-border-subtle" />
          <Btn active={showVPVR} onClick={() => setShowVPVR(v => !v)} title="Volume Profile (visible range)">
            <span className="text-[10px] font-bold">VP</span>
          </Btn>
          <Btn active={showFP} onClick={() => setShowFP(v => !v)} title="Footprint — bid/ask volume per tick">
            <span className="text-[10px] font-bold">FP</span>
          </Btn>
          <Btn active={showVolume} onClick={() => setShowVolume(v => !v)} title="Volume">
            <IconActivity size={14} stroke={1.8} /><span>Vol</span>
          </Btn>
          <Btn active={showGrid} onClick={() => setShowGrid(v => !v)} title="Grid">
            <IconGridDots size={14} stroke={1.8} />
          </Btn>
          <Btn active={autoFit} onClick={() => setAutoFit(v => !v)} title="Auto fit">
            <IconLine size={14} stroke={1.8} /><span>Live</span>
          </Btn>
          <Btn onClick={() => chartRef.current?.timeScale().fitContent()} title="Fit all">
            <IconFocusCentered size={14} stroke={1.8} />
          </Btn>

          <div className="ml-auto hidden items-center gap-3 text-[11px] text-text-tertiary xl:flex">
            <Stat label="Index"   value={index.toLocaleString('en-US', { maximumFractionDigits: 2 })} />
            <Stat label="Funding" value={`${funding.toFixed(4)}%`} accent />
          </div>
        </div>

        {/* OHLCV subtitle */}
        <div className="flex min-w-0 items-baseline gap-2 px-4 pb-2 text-[12px]">
          <span className="shrink-0 text-text-tertiary">{symbol.replace('-', ' / ')} · {interval.label}</span>
          {last && (
            <>
              <span className="text-text-quaternary">·</span>
              <OV l="O" v={last.open}  c={isUp ? 'up' : 'down'} />
              <OV l="H" v={last.high}  c="up" />
              <OV l="L" v={last.low}   c="down" />
              <OV l="C" v={last.close} c={isUp ? 'up' : 'down'} />
              <span className={`font-medium ${isUp ? 'text-bullish-green' : 'text-bearish-red'}`}>
                {isUp ? '+' : ''}{delta.toFixed(2)} ({isUp ? '+' : ''}{deltaPct.toFixed(2)}%)
              </span>
              {showMA && <span className="ml-1 flex gap-2 text-[11px]"><span style={{ color: MA_FAST }}>MA9</span><span style={{ color: MA_SLOW }}>MA21</span></span>}
              {showBB && <span className="text-[11px]" style={{ color: BB_COL }}>BB(20)</span>}
              {showRSI && <span className="text-[11px]" style={{ color: RSI_COL }}>RSI(14)</span>}
            </>
          )}
        </div>
      </div>

      {/* ── Chart + tooltip overlay ── */}
      <div className="relative min-h-0 flex-1">
        {showFP ? (
          <Suspense fallback={<div className="h-full w-full" style={{ background: colors.panel }} />}>
            <FootprintChart />
          </Suspense>
        ) : (
          <>
            <div ref={wrapRef} className="h-full w-full" />
          </>
        )}

        {/* Crosshair OHLCV tooltip */}
        <div
          ref={tooltipRef}
          className="pointer-events-none absolute z-20 hidden rounded-[6px] border border-border-subtle bg-surface-card px-2.5 py-2 shadow-xl"
          style={{ minWidth: 148 }}
        >
          <div className="mb-1.5 flex items-center justify-between gap-2">
            <span ref={ttTime}  className="text-[9px] text-text-quaternary" />
            <span ref={ttDelta} className="text-[10px] font-semibold tabular-nums" />
          </div>
          <div className="grid grid-cols-[14px_1fr] gap-x-2 gap-y-0.5 text-[10px]">
            <span className="text-text-quaternary">O</span><span ref={ttO} className="tabular-nums text-text-secondary" />
            <span style={{ color: UP }}>H</span>          <span ref={ttH} className="tabular-nums" style={{ color: UP }} />
            <span style={{ color: DOWN }}>L</span>        <span ref={ttL} className="tabular-nums" style={{ color: DOWN }} />
            <span className="text-text-quaternary">C</span><span ref={ttC} className="tabular-nums text-text-secondary" />
            <span className="text-text-quaternary">V</span><span ref={ttV} className="tabular-nums text-text-quaternary" />
          </div>
        </div>
      </div>
    </div>
  )
}

// ── Small helpers ─────────────────────────────────────────────────────────────
function Btn({ children, active = false, title, onClick }: { children: React.ReactNode; active?: boolean; title: string; onClick?: () => void }) {
  return (
    <button onClick={onClick} title={title}
      className={`flex h-7 items-center gap-1 rounded-[6px] px-2 text-[11px] font-medium transition-colors ${active ? 'bg-surface-hover text-text-primary' : 'text-text-tertiary hover:bg-surface-hover hover:text-text-primary'}`}>
      {children}
    </button>
  )
}

function Stat({ label, value, accent = false }: { label: string; value: string; accent?: boolean }) {
  return (
    <span>
      <span className="text-text-quaternary">{label} </span>
      <span className={accent ? 'text-brand-violet' : 'text-text-secondary'}>{value}</span>
    </span>
  )
}

function OV({ l, v, c }: { l: string; v: number; c: 'up' | 'down' }) {
  return (
    <span className={c === 'up' ? 'text-bullish-green' : 'text-bearish-red'}>
      <span className="text-text-secondary">{l}</span>
      {v.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 })}
    </span>
  )
}
