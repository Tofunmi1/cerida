import { useEffect, useRef } from 'react'
import { useMarket } from '../../context/market-context'
import { useTheme } from '../../context/theme-context'

interface Candle { time: number; open: number; high: number; low: number; close: number; volume: number }
interface Level  { price: number; bid: number; ask: number }
interface FPBar  extends Candle { levels: Level[] }

const AXIS_W  = 64   // right price labels
const VPVR_W  = 52   // volume profile strip
const TIME_H  = 26
const PAD_L   = 4
const UP      = '#34d399'
const DOWN    = '#c8461e'

type Theme = keyof typeof BG
const BG   = { gruvbox:'#fbf1c7', light:'#ffffff', dark:'#06070d', nord:'#eceff4', solarized:'#fdf6e3', tokyo:'#1a1b26', dracula:'#282a36', matrix:'#07110d', sepia:'#fff4dc', slate:'#171d23', contrast:'#ffffff' } as const
const GRID = { gruvbox:'rgba(60,56,54,0.09)', light:'rgba(17,24,39,0.07)', dark:'rgba(255,255,255,0.04)', nord:'rgba(46,52,64,0.09)', solarized:'rgba(7,54,66,0.09)', tokyo:'rgba(192,202,245,0.06)', dracula:'rgba(248,248,242,0.06)', matrix:'rgba(105,240,174,0.07)', sepia:'rgba(47,38,31,0.09)', slate:'rgba(240,244,248,0.06)', contrast:'rgba(0,0,0,0.12)' } as const
const TXT  = { gruvbox:'rgba(60,56,54,0.72)', light:'rgba(17,24,39,0.68)', dark:'rgba(255,255,255,0.68)', nord:'rgba(46,52,64,0.72)', solarized:'rgba(7,54,66,0.72)', tokyo:'rgba(192,202,245,0.70)', dracula:'rgba(248,248,242,0.70)', matrix:'rgba(216,255,231,0.70)', sepia:'rgba(47,38,31,0.72)', slate:'rgba(240,244,248,0.70)', contrast:'rgba(0,0,0,0.82)' } as const

function rand(s: number) { const x = Math.sin(s * 127.1 + 311.7) * 43758.5453; return x - Math.floor(x) }

function niceStep(range: number, n = 12) {
  const raw = range / n
  const exp = Math.floor(Math.log10(Math.max(raw, 1e-10)))
  const b   = Math.pow(10, exp)
  return raw / b < 1.5 ? b : raw / b < 3.5 ? 2 * b : raw / b < 7.5 ? 5 * b : 10 * b
}

function fmtP(p: number, step: number) { return p.toFixed(Math.max(0, -Math.floor(Math.log10(step)))) }

function fmtT(t: number) {
  const d = new Date(t * 1000)
  return d.toLocaleString('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false })
}

function buildFP(c: Candle, tick: number): Level[] {
  const lo    = Math.floor(c.low  / tick) * tick
  const hi    = Math.ceil (c.high / tick) * tick
  const vwap  = (c.open + c.high + c.low + c.close) / 4
  const range = Math.max(c.high - c.low, tick)
  const seed  = Math.round(c.time / 60) & 0xffff
  const bull  = c.close >= c.open
  const out: Level[] = []
  const ws: number[] = []
  let tw = 0

  for (let p = lo; p <= hi + tick * 0.01; p = +(p + tick).toFixed(10)) {
    const w = Math.exp(-((Math.abs(p + tick / 2 - vwap) / range) ** 2) * 4) * (rand(seed + Math.round(p / tick)) * 0.35 + 0.65)
    ws.push(w); tw += w
  }

  let pi = 0
  for (let p = lo; p <= hi + tick * 0.01; p = +(p + tick).toFixed(10), pi++) {
    const vol = Math.round((ws[pi]! / (tw || 1)) * c.volume)
    const pos = (p + tick / 2 - c.low) / range
    const base = bull ? 0.3 + pos * 0.4 : 0.7 - pos * 0.4
    const r = Math.min(0.97, Math.max(0.03, base + (rand(seed + pi + 500) - 0.5) * 0.25))
    const bid = Math.round(vol * r)
    out.push({ price: +p.toFixed(10), bid, ask: vol - bid })
  }
  return out
}

export default function FootprintChart() {
  const { candles: raw, mark } = useMarket()
  const { theme } = useTheme()

  const canvasRef = useRef<HTMLCanvasElement>(null)
  const wrapRef   = useRef<HTMLDivElement>(null)
  const st        = useRef({ barW: 88, scroll: 0, drag: false, dragX: 0, dragS: 0, cx: -1, cy: -1 })
  const dataRef   = useRef<FPBar[]>([])
  const markRef   = useRef(mark)
  const themeRef  = useRef(theme)
  const raf       = useRef<number | undefined>(undefined)

  markRef.current = mark; themeRef.current = theme

  useEffect(() => {
    const map = new Map<number, Candle>()
    const bkt = 60 * 15
    for (const p of raw) {
      const t = Math.floor(p.time / bkt) * bkt
      const c = map.get(t)
      if (!c) map.set(t, { time: t, open: p.open, high: p.high, low: p.low, close: p.close, volume: p.volume })
      else { c.high = Math.max(c.high, p.high); c.low = Math.min(c.low, p.low); c.close = p.close; c.volume += p.volume }
    }
    const candles = [...map.values()].sort((a, b) => a.time - b.time)
    const ranges  = candles.map(c => c.high - c.low).filter(r => r > 0).sort((a, b) => a - b)
    const med     = ranges[Math.floor(ranges.length / 2)] ?? 10
    const tick    = niceStep(med, 14)
    dataRef.current = candles.map(c => ({ ...c, levels: buildFP(c, tick) }))
    paint()
  }, [raw])

  useEffect(() => { schedule() }, [theme, mark])

  function schedule() { if (raf.current) return; raf.current = requestAnimationFrame(() => { raf.current = undefined; paint() }) }

  function paint() {
    const canvas = canvasRef.current, wrap = wrapRef.current
    if (!canvas || !wrap) return
    const dpr = Math.min(2, window.devicePixelRatio || 1)
    const W = wrap.clientWidth, H = wrap.clientHeight
    if (W <= 0 || H <= 0) return
    if (canvas.width !== Math.round(W * dpr)) {
      canvas.width = Math.round(W * dpr); canvas.height = Math.round(H * dpr)
      canvas.style.width = `${W}px`; canvas.style.height = `${H}px`
    }
    const ctx = canvas.getContext('2d')!
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)

    const t   = themeRef.current as Theme
    const bg  = BG[t]; const grd = GRID[t]; const txt = TXT[t]

    ctx.fillStyle = bg; ctx.fillRect(0, 0, W, H)

    const data = dataRef.current
    if (!data.length) return

    const { barW, scroll } = st.current
    const chartW  = W - AXIS_W - VPVR_W
    const chartH  = H - TIME_H
    const rightIdx = Math.max(0, Math.min(data.length - 1, data.length - 1 - Math.round(Math.max(0, scroll))))
    const vis      = Math.ceil(chartW / barW) + 2
    const leftIdx  = Math.max(0, rightIdx - vis + 1)
    const xOf = (i: number) => chartW - (rightIdx - i + 0.5) * barW

    // Price range
    let lo = Infinity, hi = -Infinity
    for (let i = leftIdx; i <= rightIdx; i++) { const c = data[i]!; lo = Math.min(lo, c.low); hi = Math.max(hi, c.high) }
    if (!isFinite(lo)) return
    const pad = (hi - lo) * 0.12 || 10; lo -= pad; hi += pad
    const yOf = (p: number) => chartH * (1 - (p - lo) / (hi - lo))
    const pOf = (y: number) => lo + (1 - y / chartH) * (hi - lo)

    // Tick size from data
    const sampleLevels = data[leftIdx]?.levels
    const tick = (sampleLevels && sampleLevels.length > 1)
      ? Math.abs(+(sampleLevels[1]!.price - sampleLevels[0]!.price).toFixed(10))
      : niceStep(hi - lo, 14)

    // Max level vol for scaling
    let maxVol = 1
    for (let i = leftIdx; i <= rightIdx; i++)
      for (const lv of data[i]!.levels) { const tot = lv.bid + lv.ask; if (tot > maxVol) maxVol = tot }

    // ── VPVR: accumulate volume per price bucket ──────────────────────────────
    const vpvrBuckets = new Map<number, { buy: number; sell: number }>()
    for (let i = leftIdx; i <= rightIdx; i++) {
      const c = data[i]!
      for (const lv of c.levels) {
        const key = Math.round(lv.price / tick)
        const b   = vpvrBuckets.get(key) ?? { buy: 0, sell: 0 }
        b.buy  += lv.bid; b.sell += lv.ask
        vpvrBuckets.set(key, b)
      }
    }
    let maxBkt = 1
    for (const b of vpvrBuckets.values()) { const t2 = b.buy + b.sell; if (t2 > maxBkt) maxBkt = t2 }
    let pocKey = -1, pocVol = 0
    for (const [k, b] of vpvrBuckets.entries()) { const t2 = b.buy + b.sell; if (t2 > pocVol) { pocVol = t2; pocKey = k } }

    // ── Grid ─────────────────────────────────────────────────────────────────
    const priceStep = niceStep(hi - lo, 8)
    const pStart    = Math.ceil(lo / priceStep) * priceStep
    ctx.strokeStyle = grd; ctx.lineWidth = 1
    for (let p = pStart; p <= hi; p += priceStep) {
      const y = Math.round(yOf(p)) + 0.5
      ctx.beginPath(); ctx.moveTo(PAD_L, y); ctx.lineTo(chartW + VPVR_W, y); ctx.stroke()
    }

    // ── VPVR bars ─────────────────────────────────────────────────────────────
    const vx = chartW  // VPVR starts here
    ctx.fillStyle = bg; ctx.fillRect(vx, 0, VPVR_W, chartH)

    for (const [k, b] of vpvrBuckets.entries()) {
      const price  = k * tick
      const yTop   = yOf(price + tick)
      const yBot   = yOf(price)
      const cellH  = Math.max(1, yBot - yTop)
      const total  = b.buy + b.sell
      const barLen = (total / maxBkt) * (VPVR_W - 2)
      const buyW   = (b.buy  / total) * barLen
      const sellW  = barLen - buyW
      const isPOC  = k === pocKey
      const gap    = 0.5

      if (isPOC) {
        ctx.fillStyle = 'rgba(200,220,50,0.22)'
        ctx.fillRect(vx, yTop + gap, VPVR_W, cellH - gap * 2)
      }
      ctx.fillStyle = isPOC ? 'rgba(200,220,50,0.85)' : 'rgba(52,211,153,0.55)'
      ctx.fillRect(vx + 1, yTop + gap, buyW, cellH - gap * 2)
      ctx.fillStyle = isPOC ? 'rgba(220,150,30,0.85)' : 'rgba(200,70,30,0.50)'
      ctx.fillRect(vx + 1 + buyW, yTop + gap, sellW, cellH - gap * 2)
    }

    // VPVR right border
    ctx.strokeStyle = grd; ctx.lineWidth = 1
    ctx.beginPath(); ctx.moveTo(vx, 0); ctx.lineTo(vx, chartH); ctx.stroke()
    ctx.beginPath(); ctx.moveTo(vx + VPVR_W, 0); ctx.lineTo(vx + VPVR_W, chartH); ctx.stroke()

    // ── Footprint bars ────────────────────────────────────────────────────────
    for (let i = leftIdx; i <= rightIdx; i++) {
      const bar = data[i]!
      const x   = xOf(i)
      if (x + barW < 0 || x - barW > chartW) continue

      const bw       = barW - 2
      const halfW    = bw / 2
      const showNums = barW > 48
      const fontSize = Math.min(10, Math.max(8, Math.floor(barW / 10)))

      ctx.save()
      ctx.beginPath(); ctx.rect(Math.max(PAD_L, x - halfW), 0, Math.min(bw, chartW - Math.max(PAD_L, x - halfW)), chartH); ctx.clip()

      for (const lv of bar.levels) {
        const yTop  = yOf(lv.price + tick)
        const yBot  = yOf(lv.price)
        const cellH = yBot - yTop
        if (cellH < 1.5) continue

        const tot    = lv.bid + lv.ask
        const whale  = tot > maxVol * 0.32
        const alpha  = whale ? 0.80 : Math.min(0.55, 0.08 + (tot / maxVol) * 1.8)
        const gap    = 1

        // Bid cell
        ctx.fillStyle = whale ? `rgba(200,220,50,${alpha.toFixed(2)})` : `rgba(52,211,153,${alpha.toFixed(2)})`
        ctx.fillRect(x - halfW + gap, yTop + gap * 0.5, halfW - gap * 1.5, cellH - gap)
        // Ask cell
        ctx.fillStyle = whale ? `rgba(220,160,30,${alpha.toFixed(2)})` : `rgba(200,70,30,${alpha.toFixed(2)})`
        ctx.fillRect(x + gap * 0.5, yTop + gap * 0.5, halfW - gap * 1.5, cellH - gap)

        // Divider
        ctx.fillStyle = 'rgba(0,0,0,0.18)'
        ctx.fillRect(x - 0.5, yTop, 1, cellH)

        if (showNums && cellH >= 10) {
          ctx.font = `${fontSize}px "Berkeley Mono", monospace`
          ctx.textBaseline = 'middle'; ctx.textAlign = 'center'
          const cy  = yTop + cellH / 2 + 0.5
          const col = whale ? 'rgba(0,0,0,0.9)' : 'rgba(255,255,255,0.72)'
          ctx.fillStyle = col
          if (lv.bid > 0) ctx.fillText(String(lv.bid), x - halfW / 2, cy)
          if (lv.ask > 0) ctx.fillText(String(lv.ask), x + halfW / 2, cy)
        }
      }

      // Wick
      ctx.strokeStyle = bar.close >= bar.open ? UP : DOWN
      ctx.lineWidth   = Math.max(1, barW / 24)
      ctx.beginPath(); ctx.moveTo(Math.round(x) + 0.5, yOf(bar.high)); ctx.lineTo(Math.round(x) + 0.5, yOf(bar.low)); ctx.stroke()

      // Body
      const yO = yOf(bar.open), yC = yOf(bar.close)
      const bTop = Math.min(yO, yC), bH = Math.max(2, Math.abs(yC - yO))
      const bW   = Math.max(2, barW * 0.28)
      if (bar.close >= bar.open) {
        ctx.strokeStyle = UP; ctx.lineWidth = Math.max(1.5, barW * 0.022)
        ctx.strokeRect(Math.round(x - bW / 2) + 0.5, Math.round(bTop) + 0.5, Math.round(bW), Math.round(bH))
      } else {
        ctx.fillStyle = DOWN
        ctx.fillRect(Math.round(x - bW / 2), Math.round(bTop), Math.round(bW), Math.round(bH))
      }

      ctx.restore()
    }

    // ── Mark line ─────────────────────────────────────────────────────────────
    const my = yOf(markRef.current)
    if (my >= 0 && my <= chartH) {
      ctx.strokeStyle = 'rgba(167,139,250,0.7)'; ctx.lineWidth = 1; ctx.setLineDash([3, 3])
      ctx.beginPath(); ctx.moveTo(PAD_L, my); ctx.lineTo(chartW + VPVR_W, my); ctx.stroke()
      ctx.setLineDash([])
      const lbl = fmtP(markRef.current, priceStep)
      ctx.font = '10px "Berkeley Mono", monospace'
      const lw = ctx.measureText(lbl).width + 10
      ctx.fillStyle = '#a78bfa'; ctx.fillRect(chartW + VPVR_W + 1, my - 9, lw, 18)
      ctx.fillStyle = '#fff'; ctx.textAlign = 'left'; ctx.textBaseline = 'middle'
      ctx.fillText(lbl, chartW + VPVR_W + 6, my + 1)
    }

    // ── Crosshair ─────────────────────────────────────────────────────────────
    const { cx, cy } = st.current
    if (cx >= 0 && cx < chartW && cy >= 0 && cy < chartH) {
      ctx.strokeStyle = 'rgba(167,139,250,0.35)'; ctx.lineWidth = 1; ctx.setLineDash([3, 3])
      ctx.beginPath(); ctx.moveTo(cx, 0); ctx.lineTo(cx, chartH); ctx.stroke()
      ctx.beginPath(); ctx.moveTo(PAD_L, cy); ctx.lineTo(chartW + VPVR_W, cy); ctx.stroke()
      ctx.setLineDash([])
      const cLbl = fmtP(pOf(cy), priceStep)
      ctx.font = '10px "Berkeley Mono", monospace'
      const clw = ctx.measureText(cLbl).width + 10
      ctx.fillStyle = 'rgba(167,139,250,0.75)'; ctx.fillRect(chartW + VPVR_W + 1, cy - 8, clw, 16)
      ctx.fillStyle = '#fff'; ctx.textAlign = 'left'; ctx.textBaseline = 'middle'
      ctx.fillText(cLbl, chartW + VPVR_W + 6, cy)
    }

    // ── Price axis ────────────────────────────────────────────────────────────
    const axX = chartW + VPVR_W
    ctx.fillStyle = bg; ctx.fillRect(axX, 0, AXIS_W, H)
    ctx.strokeStyle = grd; ctx.lineWidth = 1
    ctx.beginPath(); ctx.moveTo(axX + 0.5, 0); ctx.lineTo(axX + 0.5, H); ctx.stroke()
    ctx.font = '10px "Berkeley Mono", monospace'; ctx.fillStyle = txt
    ctx.textAlign = 'left'; ctx.textBaseline = 'middle'
    for (let p = pStart; p <= hi; p += priceStep) {
      const y = yOf(p); if (y < 0 || y > chartH) continue
      ctx.fillText(fmtP(p, priceStep), axX + 8, y)
    }

    // ── Time axis ─────────────────────────────────────────────────────────────
    ctx.fillStyle = bg; ctx.fillRect(0, chartH, W, TIME_H)
    ctx.strokeStyle = grd; ctx.beginPath(); ctx.moveTo(0, chartH + 0.5); ctx.lineTo(W, chartH + 0.5); ctx.stroke()
    ctx.font = '9px "Berkeley Mono", monospace'; ctx.fillStyle = txt
    ctx.textAlign = 'center'; ctx.textBaseline = 'top'
    const tStep = Math.max(1, Math.round(80 / barW))
    for (let i = leftIdx; i <= rightIdx; i += tStep) {
      const bar = data[i]; if (!bar) continue
      const x = xOf(i); if (x < 40 || x > chartW - 20) continue
      ctx.fillText(fmtT(bar.time), x, chartH + 6)
    }
  }

  useEffect(() => {
    const el = wrapRef.current; if (!el) return
    const ro = new ResizeObserver(schedule); ro.observe(el)
    return () => ro.disconnect()
  }, [])

  const onWheel = (e: React.WheelEvent) => {
    e.preventDefault()
    st.current.barW = Math.max(12, Math.min(250, st.current.barW * (e.deltaY < 0 ? 1.12 : 0.89)))
    schedule()
  }
  const onDown  = (e: React.MouseEvent) => { st.current.drag = true; st.current.dragX = e.clientX; st.current.dragS = st.current.scroll }
  const onMove  = (e: React.MouseEvent) => {
    const r = canvasRef.current!.getBoundingClientRect()
    st.current.cx = e.clientX - r.left; st.current.cy = e.clientY - r.top
    if (st.current.drag) st.current.scroll = Math.max(0, st.current.dragS - (e.clientX - st.current.dragX) / st.current.barW)
    schedule()
  }
  const onLeave = () => { st.current.drag = false; st.current.cx = -1; st.current.cy = -1; schedule() }
  const onUp    = () => { st.current.drag = false }

  return (
    <div ref={wrapRef} className="h-full w-full overflow-hidden"
      style={{ cursor: st.current.drag ? 'grabbing' : 'crosshair' }}
      onWheel={onWheel} onMouseDown={onDown} onMouseMove={onMove} onMouseUp={onUp} onMouseLeave={onLeave}>
      <canvas ref={canvasRef} className="block" />
    </div>
  )
}
