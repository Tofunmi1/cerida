import { Link } from 'react-router'
import { MARKET_CATALOG } from '../context/market-context'
import { symbolToSlug } from '../context/market-context'

export const meta = () => [
  { title: 'Cerida — On-Chain Perpetuals' },
  { name: 'description', content: 'Trade perpetual futures on any asset, on-chain. Powered by ZK proofs on Stellar.' },
]

const STATS = [
  { label: 'Open Interest', value: '$24.8M' },
  { label: '24h Volume', value: '$183.4M' },
  { label: 'Markets', value: '7' },
  { label: 'Max Leverage', value: '50×' },
]

const FEATURES = [
  {
    title: 'ZK-Verified Matching',
    body: 'Every trade is matched off-chain and verified on-chain with a zero-knowledge proof. No trust required.',
  },
  {
    title: 'Real-World Assets',
    body: 'Trade perpetuals on equities, commodities, and crypto from a single unified interface.',
  },
  {
    title: 'Non-Custodial',
    body: 'Your keys, your funds. Collateral lives in smart contracts you can audit. Connect Freighter and go.',
  },
]

export default function HomePage() {
  return (
    <div className="min-h-screen bg-page text-text-primary">
      {/* Nav */}
      <header className="fixed inset-x-0 top-0 z-50 flex h-14 items-center border-b border-border-subtle bg-page/80 px-6 backdrop-blur-md">
        <Link to="/" className="flex items-center gap-2">
          <img src="/apple-touch-icon.png" alt="Cerida" className="h-7 w-7 rounded-[6px] object-cover" />
          <span className="text-[15px] font-semibold text-text-primary">cerida</span>
        </Link>
        <nav className="ml-8 hidden items-center gap-6 text-[13px] text-text-tertiary sm:flex">
          <a href="#markets" className="transition-colors hover:text-text-primary">Markets</a>
          <a href="#features" className="transition-colors hover:text-text-primary">Features</a>
          <a
            href="https://docs.cerida.xyz"
            target="_blank"
            rel="noreferrer"
            className="transition-colors hover:text-text-primary"
          >
            Docs
          </a>
        </nav>
        <div className="ml-auto flex items-center gap-3">
          <Link
            to="/trade/btc"
            className="rounded-[8px] bg-brand-violet px-4 py-1.5 text-[13px] font-semibold text-white transition-opacity hover:opacity-90"
          >
            Launch App
          </Link>
        </div>
      </header>

      {/* Hero */}
      <section className="relative flex min-h-screen flex-col items-center justify-center px-6 pt-14 text-center">
        {/* Glow */}
        <div
          className="pointer-events-none absolute left-1/2 top-1/3 h-[480px] w-[480px] -translate-x-1/2 -translate-y-1/2 rounded-full opacity-20 blur-[120px]"
          style={{ background: 'radial-gradient(circle, #807dfe 0%, transparent 70%)' }}
        />

        <div className="relative max-w-3xl">
          <div className="mb-5 inline-flex items-center gap-2 rounded-full border border-brand-violet/30 bg-brand-violet/10 px-3.5 py-1.5 text-[11px] font-semibold uppercase tracking-widest text-brand-violet">
            <span className="h-1.5 w-1.5 rounded-full bg-brand-violet" />
            Testnet Live on Stellar
          </div>

          <h1 className="text-[52px] font-extrabold leading-[1.08] tracking-tight text-text-primary sm:text-[68px]">
            Trade Anything.
            <br />
            <span style={{ background: 'linear-gradient(135deg, #807dfe 0%, #34d399 100%)', WebkitBackgroundClip: 'text', WebkitTextFillColor: 'transparent' }}>
              On-Chain.
            </span>
          </h1>

          <p className="mx-auto mt-6 max-w-xl text-[16px] leading-relaxed text-text-tertiary">
            Perpetual futures on crypto and real-world assets, settled with zero-knowledge proofs. No counterparty risk. No compromises.
          </p>

          <div className="mt-10 flex flex-col items-center gap-4 sm:flex-row sm:justify-center">
            <Link
              to="/trade/btc"
              className="inline-flex items-center gap-2 rounded-[10px] px-7 py-3.5 text-[14px] font-bold text-white shadow-lg transition-transform hover:scale-[1.02] active:scale-[0.99]"
              style={{ background: 'linear-gradient(135deg, #807dfe 0%, #6366f1 100%)' }}
            >
              Start Trading
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                <path d="M2.5 7h9M7.5 3l4 4-4 4" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </Link>
            <a
              href="#markets"
              className="inline-flex items-center gap-2 rounded-[10px] border border-border-default px-7 py-3.5 text-[14px] font-medium text-text-secondary transition-colors hover:bg-surface-hover hover:text-text-primary"
            >
              View Markets
            </a>
          </div>
        </div>

        {/* Stats bar */}
        <div className="relative mt-20 flex w-full max-w-2xl flex-wrap justify-center gap-x-10 gap-y-4 border-t border-border-subtle pt-8">
          {STATS.map((s) => (
            <div key={s.label} className="flex flex-col items-center gap-1">
              <span className="text-[22px] font-bold tabular-nums text-text-primary">{s.value}</span>
              <span className="text-[11px] uppercase tracking-widest text-text-quaternary">{s.label}</span>
            </div>
          ))}
        </div>
      </section>

      {/* Markets */}
      <section id="markets" className="mx-auto max-w-5xl px-6 py-24">
        <h2 className="mb-2 text-center text-[11px] uppercase tracking-widest text-text-quaternary">Markets</h2>
        <p className="mb-10 text-center text-[28px] font-bold text-text-primary">Trade Any Asset</p>

        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {MARKET_CATALOG.map((market) => (
            <Link
              key={market.symbol}
              to={`/trade/${symbolToSlug(market.symbol)}`}
              className="group flex items-center gap-4 rounded-[10px] border border-border-subtle bg-surface-primary p-4 transition-all hover:border-border-default hover:bg-surface-hover"
            >
              <span
                className="grid h-10 w-10 shrink-0 place-items-center rounded-full text-[14px] font-bold text-white"
                style={{ backgroundColor: market.color }}
              >
                {market.icon}
              </span>
              <div className="min-w-0 flex-1">
                <div className="flex items-baseline gap-2">
                  <span className="text-[14px] font-bold text-text-primary">{market.name}</span>
                  <span className="rounded-[4px] bg-surface-card px-1.5 py-0.5 text-[10px] uppercase tracking-widest text-text-quaternary">
                    {market.category}
                  </span>
                </div>
                <div className="mt-0.5 text-[12px] text-text-tertiary">{market.symbol}</div>
              </div>
              <div className="shrink-0 text-right">
                <div className="text-[13px] font-semibold tabular-nums text-text-primary">
                  ${market.basePrice.toLocaleString('en-US', { maximumFractionDigits: 2 })}
                </div>
                <div className="mt-0.5 text-[11px] font-medium text-bullish-green">Perp</div>
              </div>
            </Link>
          ))}
        </div>
      </section>

      {/* Features */}
      <section id="features" className="border-t border-border-subtle bg-surface-primary py-24">
        <div className="mx-auto max-w-5xl px-6">
          <h2 className="mb-2 text-center text-[11px] uppercase tracking-widest text-text-quaternary">Why Cerida</h2>
          <p className="mb-14 text-center text-[28px] font-bold text-text-primary">Built Different</p>
          <div className="grid gap-6 sm:grid-cols-3">
            {FEATURES.map((f) => (
              <div key={f.title} className="rounded-[10px] border border-border-subtle bg-page p-6">
                <h3 className="mb-3 text-[15px] font-semibold text-text-primary">{f.title}</h3>
                <p className="text-[13px] leading-relaxed text-text-tertiary">{f.body}</p>
              </div>
            ))}
          </div>
        </div>
      </section>

      {/* CTA */}
      <section className="relative overflow-hidden py-28 text-center">
        <div
          className="pointer-events-none absolute left-1/2 top-1/2 h-[400px] w-[700px] -translate-x-1/2 -translate-y-1/2 rounded-full opacity-15 blur-[100px]"
          style={{ background: 'radial-gradient(ellipse, #807dfe 0%, transparent 70%)' }}
        />
        <div className="relative mx-auto max-w-lg px-6">
          <p className="mb-3 text-[32px] font-extrabold text-text-primary">Ready to trade?</p>
          <p className="mb-8 text-[14px] text-text-tertiary">Connect your Freighter wallet and start in under a minute.</p>
          <Link
            to="/trade/btc"
            className="inline-flex items-center gap-2 rounded-[10px] px-8 py-3.5 text-[15px] font-bold text-white transition-transform hover:scale-[1.02]"
            style={{ background: 'linear-gradient(135deg, #807dfe 0%, #6366f1 100%)' }}
          >
            Open Trading App
          </Link>
        </div>
      </section>

      {/* Footer */}
      <footer className="border-t border-border-subtle px-6 py-8">
        <div className="mx-auto flex max-w-5xl items-center justify-between">
          <div className="flex items-center gap-2">
            <img src="/apple-touch-icon.png" alt="Cerida" className="h-5 w-5 rounded-[4px] object-cover" />
            <span className="text-[13px] font-semibold text-text-tertiary">cerida</span>
          </div>
          <p className="text-[12px] text-text-quaternary">© 2026 Cerida. Testnet only. Not financial advice.</p>
        </div>
      </footer>
    </div>
  )
}
