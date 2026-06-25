import { useEffect, useRef, useState } from 'react'
import {
  CandlestickSeries,
  ColorType,
  createChart,
  CrosshairMode,
  HistogramSeries,
  LineSeries,
  LineStyle,
  type IChartApi,
  type IPriceLine,
  type ISeriesApi,
  type UTCTimestamp,
} from 'lightweight-charts'
import {
  IconActivity,
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

const UP = '#00967d'
const DOWN = '#f23546'
const ENTRY = '#807dfe'
const MA_FAST = '#b7791f'
const MA_SLOW = '#6d68f2'

const chartTheme = {
  light: {
    panel: '#ffffff',
    toolbar: 'bg-surface-primary',
    grid: 'rgba(17,24,39,0.075)',
    gridOff: 'rgba(17,24,39,0)',
    text: 'rgba(17,24,39,0.68)',
    border: 'rgba(17,24,39,0.10)',
  },
  dark: {
    panel: '#06070d',
    toolbar: 'bg-[#0a0b12]',
    grid: 'rgba(255,255,255,0.04)',
    gridOff: 'rgba(255,255,255,0)',
    text: 'rgba(255,255,255,0.70)',
    border: 'rgba(255,255,255,0.08)',
  },
} as const

const INTERVALS = [
  { label: '15m', bucket: 60 * 15 },
  { label: '30m', bucket: 60 * 30 },
  { label: '1h', bucket: 60 * 60 },
  { label: '4h', bucket: 60 * 60 * 4 },
] as const

interface Candle {
  time: UTCTimestamp
  open: number
  high: number
  low: number
  close: number
  volume: number
}

function bucketCandles(points: MarketCandle[], bucket: number): Candle[] {
  const map = new Map<number, Candle>()

  for (const point of points) {
    const time = Math.floor(point.time / bucket) * bucket
    const candle = map.get(time)
    if (!candle) {
      map.set(time, {
        time: time as UTCTimestamp,
        open: point.open,
        high: point.high,
        low: point.low,
        close: point.close,
        volume: point.volume,
      })
    } else {
      candle.high = Math.max(candle.high, point.high)
      candle.low = Math.min(candle.low, point.low)
      candle.close = point.close
      candle.volume += point.volume
    }
  }

  return [...map.values()].sort((a, b) => (a.time as number) - (b.time as number))
}

function movingAverage(data: Candle[], length: number) {
  const out: { time: UTCTimestamp; value: number }[] = []
  let sum = 0

  data.forEach((candle, index) => {
    sum += candle.close
    if (index >= length) sum -= data[index - length]!.close
    if (index >= length - 1) out.push({ time: candle.time, value: sum / length })
  })

  return out
}

function usePriceLine(
  seriesRef: React.RefObject<ISeriesApi<'Candlestick'> | null>,
  color: string,
  title: string,
  style: LineStyle = LineStyle.Dashed,
) {
  const ref = useRef<IPriceLine | null>(null)

  const set = (price: number | null) => {
    const series = seriesRef.current
    if (!series) return
    if (ref.current) {
      series.removePriceLine(ref.current)
      ref.current = null
    }
    if (price !== null) {
      ref.current = series.createPriceLine({
        price,
        color,
        lineWidth: 1,
        lineStyle: style,
        axisLabelVisible: true,
        title,
        axisLabelColor: color,
        axisLabelTextColor: '#05060a',
      })
    }
  }

  return { set }
}

export default function PriceChart() {
  const wrapRef = useRef<HTMLDivElement>(null)
  const chartRef = useRef<IChartApi | null>(null)
  const candleRef = useRef<ISeriesApi<'Candlestick'> | null>(null)
  const volumeRef = useRef<ISeriesApi<'Histogram'> | null>(null)
  const maFastRef = useRef<ISeriesApi<'Line'> | null>(null)
  const maSlowRef = useRef<ISeriesApi<'Line'> | null>(null)
  const lastBucketRef = useRef<number | null>(null)
  const [interval, setInterval] = useState<(typeof INTERVALS)[number]>(INTERVALS[0])
  const lastIntervalRef = useRef<string>(INTERVALS[0].label)
  const [last, setLast] = useState<Candle | null>(null)
  const [showVolume, setShowVolume] = useState(true)
  const [showGrid, setShowGrid] = useState(true)
  const [showMA, setShowMA] = useState(false)
  const [autoFit, setAutoFit] = useState(true)
  const [hollowCandles, setHollowCandles] = useState(true)
  const { symbol, candles, index, funding } = useMarket()
  const { tp, sl, entry } = useLevels()
  const { theme } = useTheme()
  const colors = chartTheme[theme]
  const market = MARKET_CATALOG.find((item) => item.symbol === symbol) ?? MARKET_CATALOG[0]!

  const tpLine = usePriceLine(candleRef, UP, 'TP')
  const slLine = usePriceLine(candleRef, DOWN, 'SL')
  const entryLine = usePriceLine(candleRef, ENTRY, 'Entry', LineStyle.Solid)

  useEffect(() => {
    if (!wrapRef.current) return

    const chart = createChart(wrapRef.current, {
      autoSize: true,
      layout: {
        background: { type: ColorType.Solid, color: colors.panel },
        textColor: colors.text,
        fontFamily: 'var(--font-mono)',
        fontSize: 11,
        attributionLogo: false,
      },
      grid: {
        vertLines: { color: colors.grid },
        horzLines: { color: colors.grid },
      },
      crosshair: {
        mode: CrosshairMode.Normal,
        vertLine: {
          color: 'rgba(128,125,254,0.55)',
          width: 1,
          style: LineStyle.Dashed,
          labelBackgroundColor: ENTRY,
        },
        horzLine: {
          color: 'rgba(128,125,254,0.55)',
          width: 1,
          style: LineStyle.Dashed,
          labelBackgroundColor: ENTRY,
        },
      },
      rightPriceScale: {
        borderColor: colors.border,
        scaleMargins: { top: 0.1, bottom: 0.22 },
      },
      timeScale: {
        borderColor: colors.border,
        timeVisible: true,
        secondsVisible: false,
        rightOffset: 6,
        barSpacing: 8,
      },
    })

    const candlesSeries = chart.addSeries(CandlestickSeries, {
      upColor: 'rgba(0,0,0,0)',
      downColor: DOWN,
      wickUpColor: UP,
      wickDownColor: DOWN,
      borderUpColor: UP,
      borderDownColor: DOWN,
      priceFormat: { type: 'price', precision: 2, minMove: 0.01 },
    })

    const volumeSeries = chart.addSeries(HistogramSeries, {
      priceFormat: { type: 'volume' },
      priceScaleId: '',
    })
    volumeSeries.priceScale().applyOptions({
      scaleMargins: { top: 0.93, bottom: 0 },
    })

    const maFast = chart.addSeries(LineSeries, {
      color: MA_FAST,
      lineWidth: 1,
      priceLineVisible: false,
      lastValueVisible: false,
    })
    const maSlow = chart.addSeries(LineSeries, {
      color: MA_SLOW,
      lineWidth: 1,
      priceLineVisible: false,
      lastValueVisible: false,
    })

    chartRef.current = chart
    candleRef.current = candlesSeries
    volumeRef.current = volumeSeries
    maFastRef.current = maFast
    maSlowRef.current = maSlow

    return () => {
      chart.remove()
      chartRef.current = null
      candleRef.current = null
      volumeRef.current = null
      maFastRef.current = null
      maSlowRef.current = null
    }
  }, [])

  useEffect(() => {
    chartRef.current?.applyOptions({
      layout: {
        background: { type: ColorType.Solid, color: colors.panel },
        textColor: colors.text,
      },
      grid: {
        vertLines: { color: showGrid ? colors.grid : colors.gridOff },
        horzLines: { color: showGrid ? colors.grid : colors.gridOff },
      },
      rightPriceScale: { borderColor: colors.border },
      timeScale: { borderColor: colors.border },
    })
  }, [colors, showGrid])

  useEffect(() => {
    const data = bucketCandles(candles, interval.bucket)
    if (!data.length) return

    const latest = data[data.length - 1]!
    const intervalChanged = lastIntervalRef.current !== interval.label
    const newBucket = lastBucketRef.current !== (latest.time as number)
    const shouldReset = intervalChanged || newBucket || lastBucketRef.current === null

    if (shouldReset) {
      candleRef.current?.setData(data)
      volumeRef.current?.setData(
        showVolume
          ? data.map((candle) => ({
              time: candle.time,
              value: candle.volume,
              color: candle.close >= candle.open ? 'rgba(0,150,125,0.18)' : 'rgba(242,53,70,0.20)',
            }))
          : [],
      )
      maFastRef.current?.setData(showMA ? movingAverage(data, 9) : [])
      maSlowRef.current?.setData(showMA ? movingAverage(data, 21) : [])
      if (autoFit && intervalChanged) chartRef.current?.timeScale().fitContent()
    } else {
      candleRef.current?.update(latest)
      if (showVolume) {
        volumeRef.current?.update({
          time: latest.time,
          value: latest.volume,
          color: latest.close >= latest.open ? 'rgba(0,150,125,0.18)' : 'rgba(242,53,70,0.20)',
        })
      }
      if (showMA) {
        maFastRef.current?.setData(movingAverage(data, 9))
        maSlowRef.current?.setData(movingAverage(data, 21))
      }
    }

    lastIntervalRef.current = interval.label
    lastBucketRef.current = latest.time as number
    setLast(latest)
  }, [autoFit, candles, interval, showMA, showVolume])

  useEffect(() => {
    candleRef.current?.applyOptions({
      upColor: hollowCandles ? 'rgba(0,0,0,0)' : UP,
      downColor: DOWN,
      borderUpColor: UP,
      borderDownColor: DOWN,
      wickUpColor: UP,
      wickDownColor: DOWN,
    })
  }, [hollowCandles])

  useEffect(() => {
    chartRef.current?.applyOptions({
      grid: {
        vertLines: { color: showGrid ? colors.grid : colors.gridOff },
        horzLines: { color: showGrid ? colors.grid : colors.gridOff },
      },
    })
  }, [colors, showGrid])

  useEffect(() => {
    tpLine.set(tp)
  }, [tp])
  useEffect(() => {
    slLine.set(sl)
  }, [sl])
  useEffect(() => {
    entryLine.set(entry)
  }, [entry])

  const delta = last ? last.close - last.open : 0
  const deltaPct = last ? (delta / last.open) * 100 : 0
  const tvSymbol = symbol.replace('-PERP', 'USD')

  return (
    <div className="flex h-full min-h-0 flex-col bg-surface-primary">
      <div className={`flex min-h-[78px] shrink-0 flex-col border-b border-border-subtle ${colors.toolbar}`}>
        <div className="flex h-10 items-center gap-3 px-3 text-[13px] text-text-secondary">
          <div className="flex items-center gap-2 rounded-[7px] border border-border-subtle bg-surface-card px-2.5 py-1.5">
            <span
              className="grid h-4 w-4 place-items-center rounded-full text-[8px] font-bold text-white"
              style={{ backgroundColor: market.color }}
            >
              {market.icon.length <= 2 ? market.icon : ''}
            </span>
            <span className="text-[13px] font-semibold text-text-primary">{tvSymbol}</span>
          </div>

          <div className="flex items-center gap-1">
            {INTERVALS.map((item) => (
              <button
                key={item.label}
                onClick={() => setInterval(item)}
                className={`rounded-[6px] px-2.5 py-1.5 transition-colors ${
                  interval.label === item.label
                    ? 'bg-brand-violet text-white'
                    : 'text-text-tertiary hover:bg-surface-hover hover:text-text-primary'
                }`}
              >
                {item.label}
              </button>
            ))}
          </div>

          <div className="h-5 w-px bg-border-subtle" />

          <ToolButton active title="Candles">
            <IconChartCandle size={15} stroke={1.8} />
          </ToolButton>
          <ToolButton active={hollowCandles} onClick={() => setHollowCandles((value) => !value)} title="Hollow candles">
            <IconRectangle size={15} stroke={1.8} />
            <span>Hollow</span>
          </ToolButton>
          <ToolButton active={showMA} onClick={() => setShowMA((value) => !value)} title="Moving averages">
            <IconTrendingUp size={15} stroke={1.8} />
            <span>MA</span>
          </ToolButton>
          <ToolButton active={showVolume} onClick={() => setShowVolume((value) => !value)} title="Volume">
            <IconActivity size={15} stroke={1.8} />
            <span>Vol</span>
          </ToolButton>
          <ToolButton active={showGrid} onClick={() => setShowGrid((value) => !value)} title="Grid">
            <IconGridDots size={15} stroke={1.8} />
          </ToolButton>
          <ToolButton active={autoFit} onClick={() => setAutoFit((value) => !value)} title="Auto fit">
            <IconLine size={15} stroke={1.8} />
            <span>Live</span>
          </ToolButton>
          <ToolButton onClick={() => chartRef.current?.timeScale().fitContent()} title="Fit chart">
            <IconFocusCentered size={15} stroke={1.8} />
          </ToolButton>

          <div className="ml-auto hidden items-center gap-3 text-[11px] text-text-tertiary xl:flex">
            <Stat label="Index" value={index.toLocaleString('en-US', { maximumFractionDigits: 2 })} />
            <Stat label="Funding" value={`${funding.toFixed(4)}%`} accent />
          </div>
        </div>

        <div className="flex min-w-0 items-center gap-2 px-4 pb-2 text-[12px]">
          <span className="truncate text-text-secondary">
            {symbol.replace('-', ' / ')} · {interval.label} · Cerida Perpetual
          </span>
          {last && (
            <>
              <Value label="O" value={last.open} tone={delta >= 0 ? 'up' : 'down'} />
              <Value label="H" value={last.high} tone="up" />
              <Value label="L" value={last.low} tone="down" />
              <Value label="C" value={last.close} tone={delta >= 0 ? 'up' : 'down'} />
              <span className={delta >= 0 ? 'text-bullish-green' : 'text-bearish-red'}>
                {delta >= 0 ? '+' : ''}
                {delta.toFixed(1)} ({deltaPct >= 0 ? '+' : ''}
                {deltaPct.toFixed(2)}%)
              </span>
              {showMA && (
                <span className="ml-2 flex items-center gap-2 text-[11px]">
                  <span style={{ color: MA_FAST }}>MA 9</span>
                  <span style={{ color: MA_SLOW }}>MA 21</span>
                </span>
              )}
            </>
          )}
        </div>
      </div>

      <div ref={wrapRef} className="min-h-0 flex-1" />
    </div>
  )
}

function ToolButton({
  children,
  active = false,
  title,
  onClick,
}: {
  children: React.ReactNode
  active?: boolean
  title: string
  onClick?: () => void
}) {
  return (
    <button
      onClick={onClick}
      title={title}
      className={`flex h-7 items-center gap-1 rounded-[6px] px-2 text-[11px] font-medium transition-colors ${
        active
          ? 'bg-surface-hover text-text-primary'
          : 'text-text-tertiary hover:bg-surface-hover hover:text-text-primary'
      }`}
    >
      {children}
    </button>
  )
}

function Stat({ label, value, accent = false }: { label: string; value: string; accent?: boolean }) {
  return (
    <span>
      <span className="text-text-quaternary">{label}</span>{' '}
      <span className={accent ? 'text-brand-violet' : 'text-text-secondary'}>{value}</span>
    </span>
  )
}

function Value({ label, value, tone }: { label: string; value: number; tone: 'up' | 'down' }) {
  return (
    <span className={tone === 'up' ? 'text-bullish-green' : 'text-bearish-red'}>
      <span className="text-text-secondary">{label}</span>
      {value.toLocaleString('en-US', { minimumFractionDigits: 1, maximumFractionDigits: 1 })}
    </span>
  )
}
