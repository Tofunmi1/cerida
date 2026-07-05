import { useEffect } from 'react';
import { Link } from 'react-router';
import { MARKET_CATALOG, symbolToSlug } from '../context/market-context';

export const meta = () => [
  { title: 'Cerida — On-Chain Perpetuals' },
  {
    name: 'description',
    content:
      'Perpetual futures on any asset, settled on-chain with zero-knowledge proofs.',
  },
  { property: 'og:title', content: 'Cerida — On-Chain Perpetuals' },
  { property: 'og:description', content: 'Perpetual futures on any asset, settled on-chain with zero-knowledge proofs.' },
  { property: 'og:image', content: '/prev_x.png' },
  { property: 'og:type', content: 'website' },
  { name: 'twitter:card', content: 'summary_large_image' },
  { name: 'twitter:image', content: '/prev_x.png' },
];

const PILLS = [
  '7 Markets',
  'Up to 50× Leverage',
  'ZK-Verified',
  'Non-Custodial',
  'TEE Matching',
  'Shielded Pool',
  'Real-World Assets',
];

const STATS = [
  { v: '$128,345', l: '24h Volume' },
  { v: '$1M', l: 'Open Interest' },
  { v: '7', l: 'Markets' },
  { v: '50×', l: 'Max Leverage' },
];

const D = 'rgba(255,255,255,0.45)';
const B = 'rgba(255,255,255,0.08)';

export default function HomePage() {
  useEffect(() => {
    const els = document.querySelectorAll('[data-reveal]');
    const io = new IntersectionObserver(
      (entries) => {
        entries.forEach((e) => {
          if (e.isIntersecting) {
            e.target.classList.add('in');
            io.unobserve(e.target);
          }
        });
      },
      { threshold: 0.12 },
    );
    els.forEach((el) => io.observe(el));
    return () => io.disconnect();
  }, []);


  return (
    <div className="lp-outer">
      <div className="lp-shell">
        <style>{`
        /* ── Utilities ── */
        .ch-link:hover { color: #fff !important; }
        .ch-mkt:hover  { border-color: rgba(128,125,254,0.35) !important; background: rgba(128,125,254,0.06) !important; transform: translateY(-2px); }

        @keyframes fade-up {
          from { opacity: 0; transform: translateY(16px); }
          to   { opacity: 1; transform: none; }
        }
        .hero-stagger > * { animation: fade-up 0.7s ease both; }
        .hero-stagger > *:nth-child(2) { animation-delay: 0.1s; }
        .hero-stagger > *:nth-child(3) { animation-delay: 0.2s; }
        .hero-stagger > *:nth-child(4) { animation-delay: 0.3s; }
        .hero-shot { animation: fade-up 0.9s ease 0.35s both; }

        [data-reveal] { opacity: 0; transform: translateY(20px); transition: opacity 0.65s ease, transform 0.65s ease; }
        [data-reveal].in { opacity: 1; transform: none; }

        @keyframes dash-flow { to { stroke-dashoffset: -504; } }
        .dashflow { animation: dash-flow 42s linear infinite; }

        @keyframes live-pulse {
          0%   { box-shadow: 0 0 0 0 rgba(52,211,153,0.45); }
          70%  { box-shadow: 0 0 0 6px rgba(52,211,153,0); }
          100% { box-shadow: 0 0 0 0 rgba(52,211,153,0); }
        }
        .live-dot { animation: live-pulse 2.4s ease-out infinite; }

        .cta-btn { transition: transform 0.18s ease, box-shadow 0.18s ease !important; }
        .cta-btn:hover { transform: translateY(-2px); box-shadow: 0 8px 24px rgba(255,255,255,0.12); }
        .cta-btn:active { transform: translateY(0); }

        @keyframes mascot-blink {
          0%, 90%, 100% { transform: scaleY(1); }
          95%           { transform: scaleY(0.08); }
        }
        @keyframes mascot-bob {
          0%,100% { transform: translateY(0px); }
          50%      { transform: translateY(-3px); }
        }

        /* ── Layout ── */
        .lp-outer {
          background: #fff;
          min-height: 100vh;
          padding: 20px;
          font-family: var(--font-sans, ui-monospace, monospace);
          position: relative;
        }
        .lp-shell {
          background: #0c0c14;
          color: #fff;
          border-radius: 24px;
          overflow: hidden;
          border: 1px solid rgba(255,255,255,0.10);
          box-shadow: 0 0 0 1px rgba(0,0,0,1);
          min-height: calc(100vh - 40px);
        }

        /* ── Nav ── */
        .lp-nav {
          display: flex;
          align-items: center;
          height: 60px;
          padding: 0 32px;
          border-bottom: 1px solid ${B};
        }
        .lp-nav-logo {
          display: flex;
          align-items: center;
          gap: 9px;
          text-decoration: none;
          flex-shrink: 0;
        }
        .lp-nav-center {
          display: flex;
          align-items: center;
          gap: 4px;
          margin: 0 auto;
        }
        .lp-nav-link {
          padding: 6px 14px;
          font-size: 13px;
          color: ${D};
          text-decoration: none;
          border-radius: 8px;
          transition: color 0.15s;
        }
        .lp-nav-actions {
          display: flex;
          align-items: center;
          gap: 8px;
          flex-shrink: 0;
          margin-left: auto;
        }
        .lp-nav-login {
          font-size: 13px;
          color: ${D};
          text-decoration: none;
          padding: 6px 12px;
          transition: color 0.15s;
        }
        .lp-nav-signup {
          font-size: 13px;
          font-weight: 700;
          color: #0c0c14;
          background: #fff;
          text-decoration: none;
          padding: 8px 20px;
          border-radius: 9999px;
          transition: opacity 0.15s;
        }

        /* ── Hero ── */
        .lp-hero {
          position: relative;
          overflow: hidden;
          min-height: calc(100vh - 100px);
          display: flex;
          flex-direction: column;
        }
        .lp-hero-content {
          padding: 40px 48px 0;
          max-width: 800px;
        }
        .lp-hero-shot {
          margin-top: auto;
          padding: 60px 48px 0;
          position: relative;
        }

        /* ── Stats ── */
        .lp-stats {
          display: flex;
          justify-content: center;
          flex-wrap: wrap;
          border-top: 1px solid ${B};
          border-bottom: 1px solid ${B};
        }
        .lp-stat {
          flex: 1 1 140px;
          display: flex;
          flex-direction: column;
          align-items: center;
          padding: 28px 20px;
          border-right: 1px solid ${B};
        }
        .lp-stat:last-child { border-right: none; }

        /* ── Markets ── */
        .lp-markets {
          padding: 80px 48px;
          max-width: 1100px;
          margin: 0 auto;
        }
        .lp-markets-grid {
          display: grid;
          grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
          gap: 8px;
        }

        /* ── Features ── */
        .lp-features {
          background: #f7f7f9;
          padding: 80px 48px;
        }
        .lp-features-inner { max-width: 1100px; margin: 0 auto; }
        .lp-feature-row {
          display: flex;
          gap: 64px;
          align-items: center;
          padding: 56px 0;
          border-top: 1px solid rgba(0,0,0,0.08);
        }
        .lp-feature-row.reversed { flex-direction: row-reverse; }
        .lp-feature-text { flex: 0 0 42%; }
        .lp-feature-visual {
          flex: 1;
          background: #fff;
          border: 1px solid rgba(0,0,0,0.07);
          border-radius: 18px;
          padding: 40px 48px;
          display: flex;
          justify-content: center;
          align-items: center;
        }

        /* ── CTA ── */
        .lp-cta {
          padding: 100px 48px;
          border-top: 1px solid ${B};
        }

        /* ── Footer ── */
        .lp-footer {
          border-top: 1px solid ${B};
          padding: 20px 32px;
          display: flex;
          align-items: center;
          justify-content: space-between;
          flex-wrap: wrap;
          gap: 12px;
        }

        /* ════════════ RESPONSIVE ════════════ */

        /* Tablet (≤900px) */
        @media (max-width: 900px) {
          .lp-nav-center { display: none; }
          .lp-hero-content { padding: 36px 28px 0; }
          .lp-hero-shot { padding: 40px 24px 0; }
          .lp-markets { padding: 60px 28px; }
          .lp-features { padding: 60px 28px; }
          .lp-cta { padding: 72px 28px; }
          .lp-feature-row { flex-direction: column !important; gap: 36px; padding: 48px 0; }
          .lp-feature-text { flex: none; width: 100%; }
          .lp-feature-visual { flex: none; width: 100%; box-sizing: border-box; }
        }

        /* Mobile (≤480px) */
        @media (max-width: 480px) {
          .lp-outer { padding: 10px; }
          .lp-shell { border-radius: 16px; min-height: calc(100vh - 20px); }
          .lp-nav { padding: 0 16px; height: 52px; }
          .lp-nav-login { display: none; }
          .lp-nav-signup { padding: 7px 14px; font-size: 12px; }
          .lp-hero { min-height: auto; }
          .lp-hero-content { padding: 28px 20px 0; max-width: 100%; }
          .lp-hero-shot { padding: 28px 0 0; }
          .lp-stats { flex-wrap: wrap; }
          .lp-stat { flex: 1 1 50%; border-right: none; border-bottom: 1px solid ${B}; }
          .lp-stat:nth-child(odd) { border-right: 1px solid ${B}; }
          .lp-stat:nth-last-child(-n+2) { border-bottom: none; }
          .lp-markets { padding: 48px 16px; }
          .lp-markets-grid { grid-template-columns: 1fr; }
          .lp-features { padding: 48px 16px; }
          .lp-feature-row { gap: 28px; padding: 36px 0; }
          .lp-feature-visual { padding: 24px 16px; }
          .lp-cta { padding: 56px 20px; }
          .lp-footer { padding: 16px 20px; flex-direction: column; align-items: flex-start; }
        }

        @media (prefers-reduced-motion: reduce) {
          .hero-stagger > *, .hero-shot, .dashflow, .live-dot { animation: none !important; }
          [data-reveal] { opacity: 1; transform: none; transition: none; }
        }
      `}</style>

        {/* ── NAV ─────────────────────────────────────────────────────────── */}
        <header className="lp-nav">
          <Link to="/" className="lp-nav-logo">
            <img
              src="/apple-touch-icon.png"
              alt="Cerida"
              style={{ height: 36, width: 36, borderRadius: 9, objectFit: 'cover' }}
            />
            <span style={{ fontSize: 16, fontWeight: 700, color: '#fff', letterSpacing: '0.02em' }}>
              cerida
            </span>
          </Link>

          <nav className="lp-nav-center">
            {[['#markets', 'Markets'], ['#features', 'Features']].map(([h, l]) => (
              <a key={l} href={h} className="lp-nav-link ch-link">{l}</a>
            ))}
            <Link to="/docs" className="lp-nav-link ch-link">Docs</Link>
          </nav>

          <div className="lp-nav-actions">
            <Link to="/trade/btc" className="lp-nav-login ch-link">Log in</Link>
            <Link to="/trade/btc" className="lp-nav-signup">Sign up</Link>
          </div>
        </header>

        {/* ── HERO ────────────────────────────────────────────────────────── */}
        <section className="lp-hero">
          <div className="hero-stagger lp-hero-content">
            <div style={{ marginBottom: 18, display: 'inline-flex', alignItems: 'center', gap: 10 }}>
              <span style={{
                display: 'inline-flex', alignItems: 'center', gap: 6,
                fontSize: 11, fontWeight: 600, letterSpacing: '0.07em', textTransform: 'uppercase',
                color: '#34d399', background: 'rgba(52,211,153,0.08)',
                border: '1px solid rgba(52,211,153,0.2)', borderRadius: 9999, padding: '5px 12px',
              }}>
                <span className="live-dot" style={{ width: 6, height: 6, borderRadius: '50%', background: '#34d399', display: 'inline-block' }} />
                Live on Stellar Testnet
              </span>
              <div style={{ pointerEvents: 'none', lineHeight: 0 }}>
                <svg width="32" height="42" viewBox="0 0 9 13"
                  style={{ imageRendering: 'pixelated', display: 'block', animation: 'mascot-bob 2.2s ease-in-out infinite' }}
                  fill="none">
                  <rect x="4" y="0" width="1" height="2" fill="#a5a3ff" />
                  <rect x="3" y="0" width="3" height="1" fill="#34d399" />
                  <rect x="2" y="2" width="5" height="4" fill="#807dfe" />
                  <rect x="3" y="3" width="1" height="1" fill="#fff" />
                  <rect x="5" y="3" width="1" height="1" fill="#fff" />
                  <g style={{ transformOrigin: '3.5px 3.5px', animation: 'mascot-blink 3.5s ease-in-out infinite' }}>
                    <rect x="3.1" y="3.1" width="0.8" height="0.8" fill="#1a1a2e" />
                  </g>
                  <g style={{ transformOrigin: '5.5px 3.5px', animation: 'mascot-blink 3.5s ease-in-out infinite' }}>
                    <rect x="5.1" y="3.1" width="0.8" height="0.8" fill="#1a1a2e" />
                  </g>
                  <rect x="3" y="5" width="3" height="0.5" fill="#4340c4" />
                  <rect x="3" y="5.5" width="0.8" height="0.5" fill="#4340c4" />
                  <rect x="5.2" y="5.5" width="0.8" height="0.5" fill="#4340c4" />
                  <rect x="2" y="6" width="5" height="3" fill="#6366f1" />
                  <rect x="3" y="6.8" width="3" height="1.5" fill="#807dfe" />
                  <rect x="4" y="7.1" width="1" height="0.9" fill="#34d399" />
                  <rect x="1" y="6" width="1" height="2.5" fill="#6366f1" />
                  <rect x="7" y="6" width="1" height="2.5" fill="#6366f1" />
                  <rect x="2" y="9" width="2" height="2.5" fill="#4340c4" />
                  <rect x="5" y="9" width="2" height="2.5" fill="#4340c4" />
                  <rect x="1.5" y="11.5" width="2.5" height="1" fill="#2d2a8f" />
                  <rect x="5" y="11.5" width="2.5" height="1" fill="#2d2a8f" />
                </svg>
              </div>
            </div>

            <h1 style={{
              fontSize: 'clamp(28px, 5vw, 60px)', fontWeight: 800,
              lineHeight: 1.08, letterSpacing: '-0.03em', margin: '0 0 24px',
            }}>
              High Speed Privacy first perpetuals,
              <br /> on Stellar
            </h1>

            <div style={{ display: 'flex', flexWrap: 'wrap', gap: 10, marginBottom: 40 }}>
              {PILLS.map((p) => (
                <span key={p} style={{ display: 'inline-flex', alignItems: 'center', gap: 7, fontSize: 13, color: D }}>
                  <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                    <circle cx="7" cy="7" r="6" stroke="rgba(255,255,255,0.2)" strokeWidth="1.25" />
                    <path d="M4.5 7l2 2 3-3" stroke="rgba(255,255,255,0.5)" strokeWidth="1.25" strokeLinecap="round" strokeLinejoin="round" />
                  </svg>
                  {p}
                </span>
              ))}
            </div>

            <Link to="/trade/btc" className="cta-btn" style={{
              display: 'inline-flex', alignItems: 'center', gap: 10,
              padding: '16px 32px', borderRadius: 9999, fontSize: 15, fontWeight: 700,
              color: '#0c0c14', background: '#fff', textDecoration: 'none',
            }}>
              Start trading
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                <path d="M3 8h10M9 4l4 4-4 4" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </Link>
          </div>

          {/* Screenshot */}
          <div className="hero-shot lp-hero-shot">
            <div style={{
              borderRadius: '14px 14px 0 0', overflow: 'hidden',
              border: `1px solid ${B}`, borderBottom: 'none',
              boxShadow: '0 -8px 40px rgba(0,0,0,0.5)', position: 'relative',
            }}>
              <div style={{
                display: 'flex', alignItems: 'center', gap: 8,
                padding: '9px 14px', background: '#111118', borderBottom: `1px solid ${B}`,
              }}>
                <div style={{ display: 'flex', gap: 5 }}>
                  {['#ff5f57', '#febc2e', '#28c840'].map((c) => (
                    <div key={c} style={{ width: 9, height: 9, borderRadius: '50%', background: c, opacity: 0.65 }} />
                  ))}
                </div>
                <div style={{
                  flex: 1, marginLeft: 8, height: 20, borderRadius: 5,
                  background: 'rgba(255,255,255,0.05)', display: 'flex', alignItems: 'center', padding: '0 9px',
                }}>
                  <span style={{ fontSize: 10, color: 'rgba(255,255,255,0.22)' }}>app.cerida.xyz/trade/btc</span>
                </div>
              </div>
              <div style={{ position: 'relative' }}>
                <img src="/prev_x.png" alt="Cerida trading interface" style={{ width: '100%', display: 'block', filter: 'brightness(0.82)' }} />
                <div style={{
                  position: 'absolute', bottom: 0, left: 0, right: 0, height: '45%',
                  background: 'linear-gradient(to top, #0c0c14 0%, transparent 100%)', pointerEvents: 'none',
                }} />
              </div>
            </div>
          </div>
        </section>

        {/* ── STATS ───────────────────────────────────────────────────────── */}
        <section data-reveal className="lp-stats">
          {STATS.map((s) => (
            <div key={s.l} className="lp-stat">
              <span style={{ fontSize: 26, fontWeight: 800, letterSpacing: '-0.02em' }}>{s.v}</span>
              <span style={{ fontSize: 11, color: D, marginTop: 5, letterSpacing: '0.08em', textTransform: 'uppercase' }}>{s.l}</span>
            </div>
          ))}
        </section>

        {/* ── MARKETS ─────────────────────────────────────────────────────── */}
        <section id="markets" data-reveal className="lp-markets">
          <p style={{ fontSize: 11, letterSpacing: '0.12em', textTransform: 'uppercase', color: D, marginBottom: 8 }}>Markets</p>
          <h2 style={{ fontSize: 'clamp(22px, 3vw, 34px)', fontWeight: 800, margin: '0 0 40px', letterSpacing: '-0.02em' }}>
            Trade Any Asset
          </h2>
          <div className="lp-markets-grid">
            {MARKET_CATALOG.map((m) => (
              <Link key={m.symbol} to={`/trade/${symbolToSlug(m.symbol)}`} className="ch-mkt"
                style={{
                  display: 'flex', alignItems: 'center', gap: 14, padding: '14px 16px',
                  border: `1px solid ${B}`, borderRadius: 12, textDecoration: 'none', color: '#fff',
                  transition: 'border-color 0.15s, background 0.15s, transform 0.18s',
                }}>
                {m.logo ? (
                  <img src={m.logo} alt={m.name} style={{ width: 38, height: 38, borderRadius: '50%', flexShrink: 0, objectFit: 'cover' }} />
                ) : (
                  <span style={{
                    width: 38, height: 38, borderRadius: '50%', flexShrink: 0,
                    display: 'grid', placeItems: 'center', fontSize: 10, fontWeight: 800, color: '#fff', background: m.color,
                  }}>{m.icon}</span>
                )}
                <div style={{ minWidth: 0, flex: 1 }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
                    <span style={{ fontSize: 13, fontWeight: 700 }}>{m.name}</span>
                    <span style={{
                      fontSize: 9, fontWeight: 700, letterSpacing: '0.1em', textTransform: 'uppercase',
                      padding: '2px 5px', borderRadius: 3, background: 'rgba(255,255,255,0.07)', color: D,
                    }}>{m.category}</span>
                  </div>
                  <div style={{ fontSize: 11, color: D, marginTop: 2 }}>{m.symbol}</div>
                </div>
                <div style={{ flexShrink: 0, textAlign: 'right' }}>
                  <div style={{ fontSize: 13, fontWeight: 700, fontVariantNumeric: 'tabular-nums' }}>
                    ${m.basePrice.toLocaleString('en-US', { maximumFractionDigits: 2 })}
                  </div>
                  <div style={{ fontSize: 11, color: '#34d399', marginTop: 2, fontWeight: 600 }}>Perp</div>
                </div>
              </Link>
            ))}
          </div>
        </section>

        {/* ── FEATURES ────────────────────────────────────────────────────── */}
        <section id="features" className="lp-features">
          <div className="lp-features-inner">
            <p style={{ fontSize: 11, letterSpacing: '0.12em', textTransform: 'uppercase', color: '#9ca3af', marginBottom: 8 }}>
              Why Cerida
            </p>
            <h2 style={{ fontSize: 'clamp(22px, 3vw, 34px)', fontWeight: 800, margin: '0 0 64px', letterSpacing: '-0.02em', color: '#0c0c14' }}>
              Built Different
            </h2>

            {/* Row 1: ZK Privacy */}
            <div data-reveal className="lp-feature-row">
              <div className="lp-feature-text">
                <span style={{ fontSize: 11, letterSpacing: '0.12em', textTransform: 'uppercase', color: '#9ca3af', display: 'block', marginBottom: 14 }}>
                  01 — Privacy
                </span>
                <h3 style={{ fontSize: 26, fontWeight: 800, margin: '0 0 16px', letterSpacing: '-0.02em', lineHeight: 1.2, color: '#0c0c14' }}>
                  Zero-Knowledge Positions
                </h3>
                <p style={{ fontSize: 14, lineHeight: 1.8, color: '#6b7280', margin: 0 }}>
                  Every position is a Poseidon2 commitment on Stellar. Your wallet address never appears on-chain.
                  Prove ownership with a ZK nullifier — open, close, and settle completely anonymously.
                </p>
              </div>
              <div className="lp-feature-visual">
                <svg width="100%" height="110" viewBox="0 0 380 110" fill="none">
                  <rect x="0" y="20" width="100" height="70" rx="12" fill="rgba(0,0,0,0.03)" stroke="rgba(0,0,0,0.15)" strokeWidth="1.5" />
                  <rect x="14" y="36" width="26" height="20" rx="4" fill="rgba(0,0,0,0.08)" stroke="rgba(0,0,0,0.2)" strokeWidth="1" />
                  <rect x="12" y="31" width="8" height="8" rx="2" fill="rgba(0,0,0,0.2)" />
                  <rect x="22" y="31" width="8" height="8" rx="2" fill="rgba(0,0,0,0.2)" />
                  <text x="74" y="51" textAnchor="middle" fill="rgba(0,0,0,0.3)" fontSize="9" fontFamily="monospace">WALLET</text>
                  <text x="74" y="68" textAnchor="middle" fill="rgba(0,0,0,0.55)" fontSize="9" fontFamily="monospace">0x742d...f3a1</text>
                  <line className="dashflow" x1="103" y1="55" x2="146" y2="55" stroke="rgba(0,0,0,0.2)" strokeWidth="1.5" strokeDasharray="5 4" />
                  <polygon points="144,51 152,55 144,59" fill="rgba(0,0,0,0.35)" />
                  <path d="M190 12 L224 23 L224 57 C224 71 207 79 190 84 C173 79 156 71 156 57 L156 23 Z" fill="rgba(128,125,254,0.08)" stroke="#807dfe" strokeWidth="2" />
                  <rect x="179" y="44" width="22" height="16" rx="3" fill="#807dfe" opacity="0.85" />
                  <path d="M183 44 L183 39 C183 34 201 34 201 39 L201 44" stroke="#807dfe" strokeWidth="1.5" fill="none" strokeLinecap="round" />
                  <circle cx="190" cy="52" r="2.5" fill="#fff" />
                  <line className="dashflow" x1="228" y1="55" x2="272" y2="55" stroke="rgba(0,0,0,0.2)" strokeWidth="1.5" strokeDasharray="5 4" />
                  <polygon points="270,51 278,55 270,59" fill="rgba(0,0,0,0.35)" />
                  <rect x="280" y="20" width="100" height="70" rx="12" fill="rgba(0,0,0,0.03)" stroke="rgba(0,0,0,0.15)" strokeWidth="1.5" />
                  <text x="330" y="46" textAnchor="middle" fill="rgba(0,0,0,0.3)" fontSize="9" fontFamily="monospace">COMMITMENT</text>
                  <text x="330" y="62" textAnchor="middle" fill="rgba(0,0,0,0.6)" fontSize="9" fontFamily="monospace">0xf8a2c3d1...</text>
                  <text x="330" y="75" textAnchor="middle" fill="rgba(0,0,0,0.3)" fontSize="8" fontFamily="monospace">nullifier: 0x4a...</text>
                </svg>
              </div>
            </div>

            {/* Row 2: Shielded Pool */}
            <div data-reveal className="lp-feature-row reversed">
              <div className="lp-feature-text">
                <span style={{ fontSize: 11, letterSpacing: '0.12em', textTransform: 'uppercase', color: '#9ca3af', display: 'block', marginBottom: 14 }}>
                  02 — Anonymity
                </span>
                <h3 style={{ fontSize: 26, fontWeight: 800, margin: '0 0 16px', letterSpacing: '-0.02em', lineHeight: 1.2, color: '#0c0c14' }}>
                  Shielded Pool
                </h3>
                <p style={{ fontSize: 14, lineHeight: 1.8, color: '#6b7280', margin: 0 }}>
                  Deposit collateral anonymously via Merkle-tree notes. Withdraw to any address.
                  The on-chain link between your deposit and your trades is cryptographically severed — every time.
                </p>
              </div>
              <div className="lp-feature-visual">
                <svg width="240" height="120" viewBox="0 0 240 120" fill="none">
                  <circle cx="40" cy="18" r="14" fill="rgba(0,0,0,0.04)" stroke="rgba(0,0,0,0.2)" strokeWidth="1.5" />
                  <text x="40" y="23" textAnchor="middle" fill="rgba(0,0,0,0.4)" fontSize="12" fontWeight="700">$</text>
                  <circle cx="120" cy="10" r="14" fill="rgba(0,0,0,0.04)" stroke="rgba(0,0,0,0.2)" strokeWidth="1.5" />
                  <text x="120" y="15" textAnchor="middle" fill="rgba(0,0,0,0.4)" fontSize="12" fontWeight="700">$</text>
                  <circle cx="200" cy="18" r="14" fill="rgba(0,0,0,0.04)" stroke="rgba(0,0,0,0.2)" strokeWidth="1.5" />
                  <text x="200" y="23" textAnchor="middle" fill="rgba(0,0,0,0.4)" fontSize="12" fontWeight="700">$</text>
                  <line className="dashflow" x1="40" y1="33" x2="90" y2="58" stroke="rgba(0,0,0,0.15)" strokeWidth="1.2" strokeDasharray="3 3" />
                  <line className="dashflow" x1="120" y1="25" x2="120" y2="58" stroke="rgba(0,0,0,0.15)" strokeWidth="1.2" strokeDasharray="3 3" />
                  <line className="dashflow" x1="200" y1="33" x2="150" y2="58" stroke="rgba(0,0,0,0.15)" strokeWidth="1.2" strokeDasharray="3 3" />
                  <ellipse cx="120" cy="68" rx="44" ry="13" fill="rgba(128,125,254,0.07)" stroke="#807dfe" strokeWidth="1.5" />
                  <ellipse cx="120" cy="68" rx="26" ry="7" fill="rgba(128,125,254,0.1)" stroke="rgba(128,125,254,0.4)" strokeWidth="1" />
                  <text x="120" y="71" textAnchor="middle" fill="#807dfe" fontSize="7.5" fontWeight="700" fontFamily="monospace">POOL</text>
                  <line className="dashflow" x1="120" y1="81" x2="120" y2="97" stroke="rgba(0,0,0,0.2)" strokeWidth="1.5" strokeDasharray="3 3" />
                  <polygon points="115,94 120,104 125,94" fill="rgba(0,0,0,0.3)" />
                  <text x="154" y="72" fill="rgba(0,0,0,0.25)" fontSize="8" fontFamily="monospace">anon</text>
                </svg>
              </div>
            </div>

            {/* Row 3: TEE Matching */}
            <div data-reveal className="lp-feature-row">
              <div className="lp-feature-text">
                <span style={{ fontSize: 11, letterSpacing: '0.12em', textTransform: 'uppercase', color: '#9ca3af', display: 'block', marginBottom: 14 }}>
                  03 — Fairness
                </span>
                <h3 style={{ fontSize: 26, fontWeight: 800, margin: '0 0 16px', letterSpacing: '-0.02em', lineHeight: 1.2, color: '#0c0c14' }}>
                  TEE Fair Matching
                </h3>
                <p style={{ fontSize: 14, lineHeight: 1.8, color: '#6b7280', margin: 0 }}>
                  Orders are matched inside a Trusted Execution Environment — a hardware-sealed enclave nobody can
                  tamper with. An attestation token proves the keeper saw only what it was supposed to.
                  Verified on-chain with a Groth16 proof.
                </p>
              </div>
              <div className="lp-feature-visual">
                <svg width="300" height="110" viewBox="0 0 300 110" fill="none">
                  <rect x="0" y="22" width="72" height="28" rx="8" fill="rgba(0,0,0,0.03)" stroke="rgba(0,0,0,0.15)" strokeWidth="1.2" />
                  <text x="36" y="34" textAnchor="middle" fill="rgba(0,0,0,0.45)" fontSize="8" fontFamily="monospace" fontWeight="700">Order A</text>
                  <text x="36" y="45" textAnchor="middle" fill="rgba(0,0,0,0.28)" fontSize="7">BUY 0.5 BTC</text>
                  <rect x="0" y="62" width="72" height="28" rx="8" fill="rgba(0,0,0,0.03)" stroke="rgba(0,0,0,0.15)" strokeWidth="1.2" />
                  <text x="36" y="74" textAnchor="middle" fill="rgba(0,0,0,0.45)" fontSize="8" fontFamily="monospace" fontWeight="700">Order B</text>
                  <text x="36" y="85" textAnchor="middle" fill="rgba(0,0,0,0.28)" fontSize="7">SELL 0.5 BTC</text>
                  <line className="dashflow" x1="74" y1="36" x2="108" y2="52" stroke="rgba(0,0,0,0.18)" strokeWidth="1.2" strokeDasharray="4 3" />
                  <line className="dashflow" x1="74" y1="76" x2="108" y2="60" stroke="rgba(0,0,0,0.18)" strokeWidth="1.2" strokeDasharray="4 3" />
                  <rect x="110" y="30" width="80" height="52" rx="10" fill="rgba(128,125,254,0.06)" stroke="#807dfe" strokeWidth="2" />
                  {[0,1,2,3].map((i) => <line key={i} x1={118+i*15} y1="36" x2={118+i*15} y2="78" stroke="rgba(128,125,254,0.12)" strokeWidth="0.7" />)}
                  {[0,1,2].map((i) => <line key={i} x1="115" y1={41+i*13} x2="185" y2={41+i*13} stroke="rgba(128,125,254,0.12)" strokeWidth="0.7" />)}
                  <text x="150" y="59" textAnchor="middle" fill="#807dfe" fontSize="10" fontWeight="800">TEE</text>
                  {[0,1,2].map((i) => <rect key={i} x={120+i*18} y="23" width="6" height="7" fill="rgba(128,125,254,0.45)" rx="1" />)}
                  {[0,1,2].map((i) => <rect key={i} x={120+i*18} y="82" width="6" height="7" fill="rgba(128,125,254,0.45)" rx="1" />)}
                  <line className="dashflow" x1="192" y1="56" x2="222" y2="56" stroke="rgba(0,0,0,0.18)" strokeWidth="1.5" strokeDasharray="4 3" />
                  <polygon points="220,52 228,56 220,60" fill="rgba(0,0,0,0.3)" />
                  <rect x="230" y="36" width="42" height="42" rx="8" fill="rgba(0,0,0,0.03)" stroke="rgba(0,0,0,0.15)" strokeWidth="1.5" />
                  <text x="251" y="60" textAnchor="middle" fill="rgba(0,0,0,0.55)" fontSize="18">✓</text>
                  <text x="251" y="72" textAnchor="middle" fill="rgba(0,0,0,0.3)" fontSize="6.5" fontFamily="monospace">PROOF</text>
                </svg>
              </div>
            </div>

            {/* Row 4: Markets + Orders */}
            <div data-reveal className="lp-feature-row reversed">
              <div className="lp-feature-text">
                <span style={{ fontSize: 11, letterSpacing: '0.12em', textTransform: 'uppercase', color: '#9ca3af', display: 'block', marginBottom: 14 }}>
                  04 — Markets
                </span>
                <h3 style={{ fontSize: 26, fontWeight: 800, margin: '0 0 16px', letterSpacing: '-0.02em', lineHeight: 1.2, color: '#0c0c14' }}>
                  Any Asset, Any Order
                </h3>
                <p style={{ fontSize: 14, lineHeight: 1.8, color: '#6b7280', margin: '0 0 24px' }}>
                  Trade BTC, ETH, SOL alongside SpaceX, Tesla, Gold and Oil — all with unified margin.
                  GTC, IOC, FOK, GTD order types. Keeper-executed TP/SL at oracle prices.
                  Up to 50× leverage with isolated or cross-margin modes.
                </p>
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 7 }}>
                  {['GTC','IOC','FOK','GTD','TP / SL','50× Lev','Cross','Isolated'].map((t) => (
                    <span key={t} style={{
                      fontSize: 10, fontWeight: 700, letterSpacing: '0.07em', textTransform: 'uppercase',
                      padding: '4px 10px', borderRadius: 6, background: 'rgba(0,0,0,0.05)',
                      border: '1px solid rgba(0,0,0,0.1)', color: '#374151',
                    }}>{t}</span>
                  ))}
                </div>
              </div>
              <div className="lp-feature-visual" style={{ padding: '28px', display: 'grid', gridTemplateColumns: 'repeat(3,1fr)', gap: 8 }}>
                {MARKET_CATALOG.map((m) => (
                  <div key={m.symbol} style={{
                    border: '1px solid rgba(0,0,0,0.08)', borderRadius: 10, padding: '16px 12px',
                    background: 'rgba(0,0,0,0.02)', textAlign: 'center',
                  }}>
                    {m.logo ? (
                      <img src={m.logo} alt={m.name} style={{ width: 28, height: 28, borderRadius: '50%', objectFit: 'cover', margin: '0 auto 6px' }} />
                    ) : (
                      <div style={{ width: 28, height: 28, borderRadius: '50%', background: m.color, display: 'grid', placeItems: 'center', margin: '0 auto 6px', fontSize: 9, fontWeight: 800, color: '#fff' }}>
                        {m.icon}
                      </div>
                    )}
                    <div style={{ fontSize: 11, fontWeight: 700, color: '#1a1a2e' }}>{m.symbol.replace('-PERP', '')}</div>
                    <div style={{ fontSize: 9, color: '#9ca3af', letterSpacing: '0.06em', marginTop: 3 }}>
                      {m.category === 'Crypto' ? 'PERP' : 'RWA'}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </section>

        {/* ── FINAL CTA ───────────────────────────────────────────────────── */}
        <section data-reveal className="lp-cta">
          <h2 style={{ fontSize: 'clamp(28px, 4vw, 52px)', fontWeight: 800, margin: '0 0 16px', letterSpacing: '-0.025em', maxWidth: 600 }}>
            Ready to trade?
          </h2>
          <p style={{ fontSize: 15, color: D, margin: '0 0 36px', lineHeight: 1.65, maxWidth: 440 }}>
            Connect your Freighter wallet and open your first position in under a minute.
          </p>
          <Link to="/trade/btc" className="cta-btn" style={{
            display: 'inline-flex', alignItems: 'center', gap: 10,
            padding: '16px 32px', borderRadius: 9999, fontSize: 15, fontWeight: 700,
            color: '#0c0c14', background: '#fff', textDecoration: 'none',
          }}>
            Open Trading App
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
              <path d="M3 8h10M9 4l4 4-4 4" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </Link>
        </section>

        {/* ── FOOTER ──────────────────────────────────────────────────────── */}
        <footer className="lp-footer">
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <img src="/apple-touch-icon.png" alt="Cerida" style={{ height: 18, width: 18, borderRadius: 4, objectFit: 'cover' }} />
            <span style={{ fontSize: 13, fontWeight: 600, color: D }}>cerida</span>
          </div>
          <p style={{ fontSize: 12, color: 'rgba(255,255,255,0.22)', margin: 0 }}>
            © 2026 Cerida. Testnet only — not financial advice.
          </p>
        </footer>
      </div>
    </div>
  );
}
