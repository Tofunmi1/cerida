import { lazy, memo, Suspense, useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { useNavigate, useParams } from 'react-router'
import ReactGridLayout, { type Layout, type LayoutItem } from 'react-grid-layout/legacy'
import 'react-grid-layout/css/styles.css'
import { IconPlus, IconX } from '@tabler/icons-react'
import { LevelsProvider } from '../../context/levels-context'
import { MarketProvider, slugToSymbol } from '../../context/market-context'
import { SettingsProvider } from '../../context/settings-context'
import { ThemeProvider } from '../../context/theme-context'
import { WalletProvider } from '../../context/wallet-context'
import { ToastContainer } from '../../components/toast/toast-container'
import { ToastProvider } from '../../components/toast/toast-context'
import MarketBar from '../../components/trade/market-bar'
import Sidebar from '../../components/trade/sidebar'
import PortfolioPage from '../../components/trade/portfolio-page'
import SettingsModal from '../../components/trade/settings-modal'
import ShieldedPoolModal from '../../components/trade/shielded-pool-modal'

export const meta = () => [{ title: 'Cerida Perp' }]

const PriceChart = lazy(() => import('../../components/trade/price-chart'))
const TradingPanel = lazy(() => import('../../components/trade/trading-panel'))
const OrderBook = lazy(() => import('../../components/trade/order-book'))
const PositionsPanel = lazy(() => import('../../components/trade/positions-panel'))
const MarketStats = lazy(() => import('../../components/trade/market-stats'))
const TradesTape = lazy(() => import('../../components/trade/trades-tape'))

const COLS = 24
const TOTAL_ROWS = 12
const GAP = 6
const PAD = 6

type WidgetType = 'chart' | 'trade' | 'book' | 'positions' | 'stats' | 'tape'

interface WidgetSpec {
  label: string
  w: number
  h: number
  minW: number
  minH: number
  render: () => React.ReactNode
}

const Skeleton = () => (
  <div className="flex h-full flex-col gap-3 p-3">
    <div className="skeleton h-5 w-1/3 rounded-[6px]" />
    <div className="skeleton min-h-0 flex-1 rounded-[8px]" />
  </div>
)

const CATALOG: Record<WidgetType, WidgetSpec> = {
  chart: {
    label: 'Chart',
    w: 14,
    h: 8,
    minW: 8,
    minH: 5,
    render: () => (
      <Suspense fallback={<Skeleton />}>
        <PriceChart />
      </Suspense>
    ),
  },
  trade: {
    label: 'Trade',
    w: 5,
    h: 8,
    minW: 4,
    minH: 6,
    render: () => (
      <Suspense fallback={<Skeleton />}>
        <TradingPanel />
      </Suspense>
    ),
  },
  book: {
    label: 'Order Book',
    w: 5,
    h: 8,
    minW: 4,
    minH: 5,
    render: () => (
      <Suspense fallback={<Skeleton />}>
        <OrderBook />
      </Suspense>
    ),
  },
  positions: {
    label: 'Positions',
    w: 14,
    h: 4,
    minW: 8,
    minH: 3,
    render: () => (
      <Suspense fallback={<Skeleton />}>
        <PositionsPanel />
      </Suspense>
    ),
  },
  stats: {
    label: 'Market Stats',
    w: 5,
    h: 4,
    minW: 4,
    minH: 3,
    render: () => (
      <Suspense fallback={<Skeleton />}>
        <MarketStats />
      </Suspense>
    ),
  },
  tape: {
    label: 'Trades',
    w: 5,
    h: 4,
    minW: 4,
    minH: 3,
    render: () => (
      <Suspense fallback={<Skeleton />}>
        <TradesTape />
      </Suspense>
    ),
  },
}

const ADD_OPTIONS = (Object.keys(CATALOG) as WidgetType[]).map((type) => ({
  type,
  label: CATALOG[type].label,
}))

const WidgetContent = memo(function WidgetContent({ type }: { type: WidgetType }) {
  return <>{CATALOG[type].render()}</>
})

interface Tab {
  id: string
  type: WidgetType
}

interface Item {
  id: string
  tabs: Tab[]
  active: number
}

const one = (id: string, type: WidgetType): Item => ({
  id,
  tabs: [{ id: `${id}-t0`, type }],
  active: 0,
})

const INITIAL_ITEMS: Item[] = [
  one('chart', 'chart'),
  one('book', 'book'),
  one('trade', 'trade'),
  one('positions', 'positions'),
  one('stats', 'stats'),
  one('tape', 'tape'),
]

const INITIAL_LAYOUT: Layout = [
  { i: 'chart', x: 0, y: 0, w: 14, h: 8, minW: 8, minH: 5 },
  { i: 'book', x: 14, y: 0, w: 5, h: 8, minW: 4, minH: 5 },
  { i: 'trade', x: 19, y: 0, w: 5, h: 8, minW: 4, minH: 6 },
  { i: 'positions', x: 0, y: 8, w: 14, h: 4, minW: 8, minH: 3 },
  { i: 'stats', x: 14, y: 8, w: 5, h: 4, minW: 4, minH: 3 },
  { i: 'tape', x: 19, y: 8, w: 5, h: 4, minW: 4, minH: 3 },
]

function useGridSize() {
  const ref = useRef<HTMLDivElement>(null)
  const [size, setSize] = useState({ width: 1200, rowHeight: 60 })

  useEffect(() => {
    if (!ref.current) return
    const update = (w: number, h: number) => {
      const rowHeight = Math.floor((h - PAD * 2 - GAP * (TOTAL_ROWS - 1)) / TOTAL_ROWS)
      setSize({ width: w, rowHeight: Math.max(rowHeight, 28) })
    }
    const ro = new ResizeObserver(([entry]) => {
      if (entry) update(entry.contentRect.width, entry.contentRect.height)
    })
    ro.observe(ref.current)
    update(ref.current.clientWidth, ref.current.clientHeight)
    return () => ro.disconnect()
  }, [])

  return { ref, ...size }
}

function overlaps(a: { x: number; y: number; w: number; h: number }, b: LayoutItem) {
  return a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
}

function findFirstFit(layout: Layout, w: number, h: number, cols: number) {
  for (let y = 0; ; y++) {
    for (let x = 0; x + w <= cols; x++) {
      const candidate = { x, y, w, h }
      if (!layout.some((item) => overlaps(candidate, item))) return { x, y }
    }
  }
}

function Widget({
  tabs,
  active,
  content,
  onSelect,
  onAddTab,
  onCloseTab,
  onClose,
}: {
  tabs: { id: string; label: string }[]
  active: number
  content: React.ReactNode
  onSelect: (index: number) => void
  onAddTab: (type: WidgetType) => void
  onCloseTab: (index: number) => void
  onClose: () => void
}) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null)

  return (
    <div className="panel-widget flex h-full flex-col overflow-hidden rounded-[8px] border border-border-subtle bg-surface-primary">
      <div className="widget-handle flex h-9 shrink-0 cursor-grab select-none items-center gap-1 border-b border-border-subtle px-2 active:cursor-grabbing">
        <div className="no-scrollbar flex min-w-0 items-center gap-1 overflow-x-auto">
          {tabs.map((tab, index) => (
            <button
              key={tab.id}
              onClick={() => onSelect(index)}
              className={`flex shrink-0 items-center gap-1.5 rounded-[6px] px-2 py-1 text-[11px] font-medium uppercase tracking-widest transition-colors ${
                active === index
                  ? 'bg-surface-card text-text-primary'
                  : 'text-text-quaternary hover:text-text-secondary'
              }`}
            >
              <span className="max-w-28 truncate">{tab.label}</span>
              {active === index && tabs.length > 1 && (
                <span
                  onClick={(e) => {
                    e.stopPropagation()
                    onCloseTab(index)
                  }}
                  className="text-text-quaternary hover:text-text-primary"
                >
                  <IconX size={10} stroke={2.5} />
                </span>
              )}
            </button>
          ))}

          <button
            onClick={(e) => {
              const rect = e.currentTarget.getBoundingClientRect()
              setMenu(menu ? null : { x: rect.left, y: rect.bottom + 4 })
            }}
            className="grid h-6 w-6 shrink-0 place-items-center rounded-[6px] text-text-quaternary transition-colors hover:bg-surface-card hover:text-text-primary"
            title="Add tab"
          >
            <IconPlus size={12} stroke={2.25} />
          </button>
        </div>

        <button
          onClick={onClose}
          className="ml-auto grid h-6 w-6 shrink-0 place-items-center rounded-[6px] text-text-quaternary transition-colors hover:bg-surface-card hover:text-bearish-red"
          title="Remove widget"
        >
          <IconX size={13} stroke={2} />
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-hidden">{content}</div>

      {menu &&
        createPortal(
          <>
            <div className="fixed inset-0 z-[60]" onClick={() => setMenu(null)} />
            <div
              className="fixed z-[61] min-w-36 rounded-[8px] border border-border-subtle bg-surface-card py-1 shadow-xl"
              style={{ left: menu.x, top: menu.y }}
            >
              <div className="px-3 py-1 text-[9px] uppercase tracking-widest text-text-quaternary">
                Add tab
              </div>
              {ADD_OPTIONS.map((option) => (
                <button
                  key={option.type}
                  onClick={() => {
                    onAddTab(option.type)
                    setMenu(null)
                  }}
                  className="block w-full px-3 py-1.5 text-left text-[11px] text-text-secondary transition-colors hover:bg-surface-hover hover:text-text-primary"
                >
                  {option.label}
                </button>
              ))}
            </div>
          </>,
          document.body,
        )}
    </div>
  )
}

function TradeBoard({
  active,
  onActive,
  onNavigate,
}: {
  active: string
  onActive: (label: string) => void
  onNavigate: (path: string) => void
}) {
  const [items, setItems] = useState(INITIAL_ITEMS)
  const [layout, setLayout] = useState(INITIAL_LAYOUT)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const { ref, width, rowHeight } = useGridSize()
  const nextId = useRef(0)

  const addWidget = (type: WidgetType) => {
    const spec = CATALOG[type]
    const id = `${type}-${++nextId.current}`
    setLayout((prev) => {
      const { x, y } = findFirstFit(prev, spec.w, spec.h, COLS)
      return [...prev, { i: id, x, y, w: spec.w, h: spec.h, minW: spec.minW, minH: spec.minH }]
    })
    setItems((prev) => [...prev, one(id, type)])
  }

  const removeWidget = (id: string) => {
    setItems((prev) => prev.filter((item) => item.id !== id))
    setLayout((prev) => prev.filter((item) => item.i !== id))
  }

  const selectTab = (itemId: string, index: number) =>
    setItems((prev) =>
      prev.map((item) => (item.id === itemId ? { ...item, active: index } : item)),
    )

  const addTab = (itemId: string, type: WidgetType) =>
    setItems((prev) =>
      prev.map((item) =>
        item.id === itemId
          ? {
              ...item,
              tabs: [...item.tabs, { id: `tab-${++nextId.current}`, type }],
              active: item.tabs.length,
            }
          : item,
      ),
    )

  const closeTab = (itemId: string, index: number) =>
    setItems((prev) =>
      prev.map((item) => {
        if (item.id !== itemId || item.tabs.length === 1) return item
        const tabs = item.tabs.filter((_, idx) => idx !== index)
        const active =
          index < item.active || item.active > tabs.length - 1
            ? Math.max(0, item.active - 1)
            : item.active
        return { ...item, tabs, active }
      }),
    )

  return (
    <div className="flex h-screen min-w-0 bg-page">
      <Sidebar active={active} onActive={onActive} />
      <div className="flex min-w-0 flex-1 flex-col">
        {active === 'Portfolio' && <PortfolioPage onClose={() => onActive('Perps')} />}
        {active === 'Pool' && <ShieldedPoolModal onClose={() => onActive('Perps')} />}
        {settingsOpen && <SettingsModal onClose={() => setSettingsOpen(false)} />}
        <div className="flex min-w-0 flex-1 flex-col">
          <MarketBar active={active} onActive={onActive} onOpenSettings={() => setSettingsOpen(true)} onNavigate={onNavigate} />
          <div ref={ref} className="min-h-0 flex-1 overflow-auto">
            <ReactGridLayout
              layout={layout}
              onLayoutChange={(next: Layout) => setLayout(next)}
              cols={COLS}
              rowHeight={rowHeight}
              width={width}
              margin={[GAP, GAP]}
              containerPadding={[PAD, PAD]}
              draggableHandle=".widget-handle"
              draggableCancel="input,button,select,textarea,a"
              resizeHandles={['s', 'e', 'se', 'w', 'n', 'sw', 'ne', 'nw']}
              compactType="vertical"
              preventCollision={false}
              allowOverlap={false}
              useCSSTransforms
            >
              {items.map((item) => {
                const activeTab = item.tabs[item.active] ?? item.tabs[0]!
                if (activeTab.type === 'chart') {
                  return (
                    <div key={item.id} className="h-full overflow-hidden rounded-[8px] bg-surface-primary">
                      <WidgetContent type="chart" />
                    </div>
                  )
                }
                return (
                  <div key={item.id} className="h-full">
                    <Widget
                      tabs={item.tabs.map((tab) => ({ id: tab.id, label: CATALOG[tab.type].label }))}
                      active={item.active}
                      content={<WidgetContent type={activeTab.type} />}
                      onSelect={(index) => selectTab(item.id, index)}
                      onAddTab={(type) => addTab(item.id, type)}
                      onCloseTab={(index) => closeTab(item.id, index)}
                      onClose={() => removeWidget(item.id)}
                    />
                  </div>
                )
              })}
            </ReactGridLayout>
          </div>
        </div>
      </div>
    </div>
  )
}

export default function TradeRoute() {
  const { asset } = useParams<{ asset: string }>()
  const navigate = useNavigate()
  const [nav, setNav] = useState('Perps')

  const initialSymbol = slugToSymbol(asset ?? 'btc')

  return (
    <ThemeProvider>
      <ToastProvider>
        <WalletProvider>
          <MarketProvider initialSymbol={initialSymbol} key={initialSymbol}>
            <SettingsProvider>
              <LevelsProvider>
                <TradeBoard active={nav} onActive={setNav} onNavigate={navigate} />
                <ToastContainer />
              </LevelsProvider>
            </SettingsProvider>
          </MarketProvider>
        </WalletProvider>
      </ToastProvider>
    </ThemeProvider>
  )
}
