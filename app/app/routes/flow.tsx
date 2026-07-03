import { useState } from 'react'
import { Link } from 'react-router'

export const meta = () => [{ title: 'System Flow — Cerida' }]

// ── Palette ────────────────────────────────────────────────────────
const C = {
  bg:      '#04040d',
  grid:    'rgba(255,255,255,0.03)',
  user:    '#22c55e',
  tee:     '#60a5fa',
  stellar: '#a78bfa',
  keeper:  '#fb923c',
  pyth:    '#facc15',
  zk:      '#f472b6',
  dim:     'rgba(255,255,255,0.35)',
  dimmer:  'rgba(255,255,255,0.14)',
  border:  'rgba(255,255,255,0.09)',
}

// ── Types ──────────────────────────────────────────────────────────
interface NodeDef {
  id: string
  x: number; y: number; w: number; h: number
  color: string
  title: string
  sub: string
  bullets: string[]
}

interface EdgeDef {
  id: string
  d: string          // SVG path
  color: string
  label: string
  labelX: number; labelY: number
  step?: number
}

// ── Layout constants ───────────────────────────────────────────────
// ViewBox: 0 0 1340 760
const VW = 1340
const VH = 760

const NODES: NodeDef[] = [
  {
    id: 'browser',
    x: 40, y: 270, w: 200, h: 180,
    color: C.user,
    title: 'Browser',
    sub: 'Freighter Wallet',
    bullets: ['Encrypt order → TEE pubkey', 'Sign Stellar txns', 'Store note secrets locally', 'Submit proof to Soroban'],
  },
  {
    id: 'tee',
    x: 480, y: 100, w: 280, h: 340,
    color: C.tee,
    title: 'TEE',
    sub: 'GCP Confidential Space · AMD SEV-SNP',
    bullets: ['CLOB engine (price-time priority)', 'Groth16 prover · arkworks BN254', 'Poseidon2 commitment hash', 'KMS-backed signing key', 'WAL persistence · RwLock'],
  },
  {
    id: 'stellar',
    x: 1040, y: 100, w: 260, h: 340,
    color: C.stellar,
    title: 'Stellar Soroban',
    sub: 'Protocol 26 · Testnet',
    bullets: ['verify_groth16() via BN254 host fns', 'perp-engine: positions, collateral', 'orderbook: commitments, nullifiers', 'collateral-token: USDC mint/burn', 'Oracle prices (set_asset_price)'],
  },
  {
    id: 'pyth',
    x: 40, y: 580, w: 180, h: 120,
    color: C.pyth,
    title: 'Pyth Network',
    sub: 'Hermes · REST + WebSocket',
    bullets: ['7 market price feeds', 'Sub-second ticks'],
  },
  {
    id: 'oracle',
    x: 280, y: 580, w: 190, h: 120,
    color: C.keeper,
    title: 'Oracle Keeper',
    sub: 'Rust · every 30s',
    bullets: ['Fetch Pyth prices', 'set_asset_price on-chain'],
  },
  {
    id: 'mm',
    x: 530, y: 580, w: 220, h: 120,
    color: C.keeper,
    title: 'Market Maker',
    sub: '32 levels/side',
    bullets: ['Geometric size · 1.08^level', 'Spread: 5–10 bps base'],
  },
  {
    id: 'liq',
    x: 810, y: 580, w: 200, h: 120,
    color: C.keeper,
    title: 'Liquidator',
    sub: 'Rust · every 30s',
    bullets: ['Scan position watchlist', 'Call liquidate() on-chain'],
  },
]

const EDGES: EdgeDef[] = [
  // Browser → TEE  (encrypted order)
  {
    id: 'e1',
    d: 'M 240 320 C 350 320 420 260 480 240',
    color: C.user,
    label: '① encrypted order',
    labelX: 290, labelY: 295,
    step: 1,
  },
  // TEE → Browser  (Groth16 proof returned)
  {
    id: 'e2',
    d: 'M 480 360 C 420 380 350 390 240 390',
    color: C.zk,
    label: '③ Groth16 proof',
    labelX: 290, labelY: 408,
    step: 3,
  },
  // Browser → Stellar  (submit tx)
  {
    id: 'e3',
    d: 'M 240 300 C 240 140 900 120 1040 160',
    color: C.user,
    label: '④ submit tx + proof',
    labelX: 590, labelY: 102,
    step: 4,
  },
  // TEE → Stellar  (match proof + position open)
  {
    id: 'e4',
    d: 'M 760 240 L 1040 240',
    color: C.tee,
    label: '② match proof + open_position',
    labelX: 772, labelY: 225,
    step: 2,
  },
  // Stellar → TEE  (mark price read)
  {
    id: 'e5',
    d: 'M 1040 300 L 760 300',
    color: C.stellar,
    label: 'mark price / oracle',
    labelX: 820, labelY: 318,
  },
  // Pyth → Oracle
  {
    id: 'e6',
    d: 'M 220 640 L 280 640',
    color: C.pyth,
    label: 'price ticks',
    labelX: 225, labelY: 628,
    step: 5,
  },
  // Oracle → Stellar
  {
    id: 'e7',
    d: 'M 375 580 C 375 440 1040 380 1100 440',
    color: C.keeper,
    label: '⑤ set_asset_price',
    labelX: 650, labelY: 480,
    step: 5,
  },
  // MM → TEE
  {
    id: 'e8',
    d: 'M 640 580 C 640 500 620 480 620 440',
    color: C.keeper,
    label: '⑥ place_order (32 levels)',
    labelX: 648, labelY: 530,
    step: 6,
  },
  // Liquidator → Stellar
  {
    id: 'e9',
    d: 'M 910 580 C 960 520 1060 480 1100 440',
    color: C.keeper,
    label: '⑦ liquidate()',
    labelX: 980, labelY: 530,
    step: 7,
  },
]

// ── Animated dot along a path ──────────────────────────────────────
function FlowDot({ pathId, color, dur, delay }: { pathId: string; color: string; dur: number; delay: number }) {
  return (
    <circle r="4" fill={color} opacity="0.9">
      <animateMotion dur={`${dur}s`} begin={`${delay}s`} repeatCount="indefinite" rotate="auto">
        <mpath href={`#${pathId}`} />
      </animateMotion>
    </circle>
  )
}

// ── Node box ───────────────────────────────────────────────────────
function NodeBox({ node, active, onClick }: { node: NodeDef; active: boolean; onClick: () => void }) {
  return (
    <g onClick={onClick} style={{ cursor: 'pointer' }}>
      {/* Glow when active */}
      {active && (
        <rect
          x={node.x - 6} y={node.y - 6}
          width={node.w + 12} height={node.h + 12}
          rx="18" ry="18"
          fill="none"
          stroke={node.color}
          strokeWidth="1.5"
          opacity="0.35"
        />
      )}
      {/* Box */}
      <rect
        x={node.x} y={node.y}
        width={node.w} height={node.h}
        rx="14" ry="14"
        fill="rgba(255,255,255,0.03)"
        stroke={active ? node.color : C.border}
        strokeWidth={active ? 1.5 : 1}
      />
      {/* Color bar */}
      <rect
        x={node.x} y={node.y}
        width={node.w} height="4"
        rx="14" ry="14"
        fill={node.color}
        opacity="0.8"
      />
      {/* Title */}
      <text
        x={node.x + 16} y={node.y + 30}
        fontSize="14" fontWeight="700" fill="#fff"
        fontFamily="ui-monospace, monospace"
      >
        {node.title}
      </text>
      {/* Sub */}
      <text
        x={node.x + 16} y={node.y + 48}
        fontSize="9.5" fill={node.color}
        fontFamily="ui-monospace, monospace"
        opacity="0.9"
      >
        {node.sub}
      </text>
      {/* Bullets */}
      {node.bullets.map((b, i) => (
        <text
          key={i}
          x={node.x + 22} y={node.y + 70 + i * 18}
          fontSize="10" fill={C.dim}
          fontFamily="ui-monospace, monospace"
        >
          {`· ${b}`}
        </text>
      ))}
    </g>
  )
}

// ── ZK Proof callout (between TEE and Stellar) ─────────────────────
function ZkCallout() {
  return (
    <g>
      <rect x="810" y="196" width="220" height="70" rx="10" ry="10"
        fill="rgba(244,114,182,0.07)" stroke="rgba(244,114,182,0.25)" strokeWidth="1"
      />
      <text x="920" y="218" fontSize="10" fontWeight="700" fill={C.zk}
        textAnchor="middle" fontFamily="ui-monospace, monospace">
        GROTH16 PROOF
      </text>
      <text x="920" y="233" fontSize="9" fill={C.dimmer}
        textAnchor="middle" fontFamily="ui-monospace, monospace">
        π = (A, B, C) ∈ BN254
      </text>
      <text x="920" y="248" fontSize="9" fill={C.dimmer}
        textAnchor="middle" fontFamily="ui-monospace, monospace">
        verify via g1_msm + pairing_check
      </text>
      <text x="920" y="263" fontSize="9" fill={C.dimmer}
        textAnchor="middle" fontFamily="ui-monospace, monospace">
        public inputs: [cmt, nullifier, ...]
      </text>
    </g>
  )
}

// ── Poseidon callout inside TEE ────────────────────────────────────
function PoseidonCallout() {
  return (
    <g>
      <rect x="492" y="310" width="254" height="118" rx="9" ry="9"
        fill="rgba(96,165,250,0.06)" stroke="rgba(96,165,250,0.18)" strokeWidth="1"
      />
      <text x="619" y="330" fontSize="10" fontWeight="700" fill={C.tee}
        textAnchor="middle" fontFamily="ui-monospace, monospace">
        COMMITMENT CHAIN
      </text>
      {[
        'h₁ = P₂(side, price, 1)',
        'h₂ = P₂(h₁, size, 2)',
        'h₃ = P₂(h₂, leverage, 3)',
        '   ···',
        'cmt = P₂(h₆, secret, 7)',
      ].map((line, i) => (
        <text key={i} x="507" y={348 + i * 16}
          fontSize="9.5" fill={C.dimmer}
          fontFamily="ui-monospace, monospace"
        >
          {line}
        </text>
      ))}
    </g>
  )
}

// ── Main component ─────────────────────────────────────────────────
export default function FlowPage() {
  const [active, setActive] = useState<string | null>(null)

  const toggle = (id: string) => setActive((p) => (p === id ? null : id))

  const activeNode = NODES.find((n) => n.id === active)

  return (
    <div style={{ background: C.bg, minHeight: '100vh', display: 'flex', flexDirection: 'column' }}>

      {/* Nav */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 12,
        padding: '12px 24px',
        borderBottom: `1px solid ${C.border}`,
        background: 'rgba(255,255,255,0.02)',
      }}>
        <Link to="/" style={{ display: 'flex', alignItems: 'center', gap: 8, textDecoration: 'none' }}>
          <img src="/apple-touch-icon.png" alt="Cerida" style={{ width: 24, height: 24, borderRadius: 6 }} />
          <span style={{ fontSize: 13, fontWeight: 700, color: '#fff', fontFamily: 'ui-monospace, monospace' }}>Cerida</span>
        </Link>
        <span style={{ color: C.dimmer, fontSize: 13 }}>/</span>
        <span style={{ fontSize: 13, color: C.dim, fontFamily: 'ui-monospace, monospace' }}>System Flow</span>
        <div style={{ marginLeft: 'auto', display: 'flex', gap: 8 }}>
          <Link to="/docs" style={{
            padding: '6px 14px', borderRadius: 7, fontSize: 12, fontWeight: 600,
            color: C.dim, textDecoration: 'none', border: `1px solid ${C.border}`,
            fontFamily: 'ui-monospace, monospace',
          }}>Docs</Link>
          <Link to="/trade/btc" style={{
            padding: '6px 14px', borderRadius: 7, fontSize: 12, fontWeight: 600,
            background: '#807dfe', color: '#fff', textDecoration: 'none',
            fontFamily: 'ui-monospace, monospace',
          }}>Launch App →</Link>
        </div>
      </div>

      {/* Header */}
      <div style={{ textAlign: 'center', padding: '28px 0 12px', fontFamily: 'ui-monospace, monospace' }}>
        <div style={{ fontSize: 11, fontWeight: 700, letterSpacing: '0.18em', color: C.dimmer, marginBottom: 6 }}>
          ARCHITECTURE
        </div>
        <h1 style={{ fontSize: 26, fontWeight: 800, color: '#fff', margin: 0, letterSpacing: '-0.02em' }}>
          End-to-End System Flow
        </h1>
        <p style={{ fontSize: 12, color: C.dimmer, marginTop: 8, fontFamily: 'ui-monospace, monospace' }}>
          Click any node to inspect · Animated flows show live data paths
        </p>
      </div>

      {/* Legend */}
      <div style={{
        display: 'flex', justifyContent: 'center', gap: 24, paddingBottom: 12,
        fontFamily: 'ui-monospace, monospace', fontSize: 11,
      }}>
        {[
          { color: C.user,    label: 'Client' },
          { color: C.tee,     label: 'TEE / Enclave' },
          { color: C.stellar, label: 'Stellar Soroban' },
          { color: C.keeper,  label: 'Keepers' },
          { color: C.pyth,    label: 'Pyth Oracle' },
          { color: C.zk,      label: 'ZK Proof' },
        ].map(({ color, label }) => (
          <div key={label} style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <span style={{ width: 10, height: 10, borderRadius: 3, background: color, display: 'inline-block' }} />
            <span style={{ color: C.dim }}>{label}</span>
          </div>
        ))}
      </div>

      {/* SVG Diagram */}
      <div style={{ flex: 1, overflow: 'auto', padding: '0 24px 32px' }}>
        <div style={{ maxWidth: 1340, margin: '0 auto' }}>
          <svg
            viewBox={`0 0 ${VW} ${VH}`}
            style={{ width: '100%', height: 'auto', display: 'block' }}
            fontFamily="ui-monospace, monospace"
          >
            {/* Background grid */}
            <defs>
              <pattern id="grid" width="40" height="40" patternUnits="userSpaceOnUse">
                <path d={`M 40 0 L 0 0 0 40`} fill="none" stroke={C.grid} strokeWidth="1" />
              </pattern>
              {/* Arrowhead markers */}
              {[
                { id: 'arr-user',    color: C.user },
                { id: 'arr-tee',     color: C.tee },
                { id: 'arr-stellar', color: C.stellar },
                { id: 'arr-keeper',  color: C.keeper },
                { id: 'arr-pyth',    color: C.pyth },
                { id: 'arr-zk',      color: C.zk },
              ].map(({ id, color }) => (
                <marker key={id} id={id} viewBox="0 0 10 10" refX="9" refY="5"
                  markerWidth="6" markerHeight="6" orient="auto-start-reverse">
                  <path d="M 0 0 L 10 5 L 0 10 z" fill={color} opacity="0.8" />
                </marker>
              ))}
              {/* Define edge paths for animateMotion */}
              {EDGES.map((e) => (
                <path key={`def-${e.id}`} id={e.id} d={e.d} fill="none" />
              ))}
            </defs>

            <rect width={VW} height={VH} fill="url(#grid)" />

            {/* Section labels */}
            <text x="140" y="55" fontSize="9" fontWeight="700" fill={C.user}
              textAnchor="middle" letterSpacing="0.15em">CLIENT LAYER</text>
            <text x="620" y="55" fontSize="9" fontWeight="700" fill={C.tee}
              textAnchor="middle" letterSpacing="0.15em">CONFIDENTIAL COMPUTE</text>
            <text x="1170" y="55" fontSize="9" fontWeight="700" fill={C.stellar}
              textAnchor="middle" letterSpacing="0.15em">SETTLEMENT LAYER</text>
            <text x="640" y="545" fontSize="9" fontWeight="700" fill={C.keeper}
              textAnchor="middle" letterSpacing="0.15em">KEEPER INFRASTRUCTURE</text>

            {/* Section bounding boxes */}
            <rect x="20" y="64" width="240" height="440" rx="16" fill="none"
              stroke={C.user} strokeWidth="0.5" opacity="0.15" strokeDasharray="6 4" />
            <rect x="460" y="64" width="320" height="440" rx="16" fill="none"
              stroke={C.tee} strokeWidth="0.5" opacity="0.15" strokeDasharray="6 4" />
            <rect x="1020" y="64" width="300" height="440" rx="16" fill="none"
              stroke={C.stellar} strokeWidth="0.5" opacity="0.15" strokeDasharray="6 4" />
            <rect x="20" y="554" width="1010" height="180" rx="16" fill="none"
              stroke={C.keeper} strokeWidth="0.5" opacity="0.12" strokeDasharray="6 4" />

            {/* Edges — static paths */}
            {EDGES.map((e) => (
              <path
                key={e.id}
                d={e.d}
                fill="none"
                stroke={e.color}
                strokeWidth="1.5"
                strokeDasharray="5 4"
                opacity="0.45"
                markerEnd={`url(#arr-${
                  e.color === C.user ? 'user' :
                  e.color === C.tee ? 'tee' :
                  e.color === C.stellar ? 'stellar' :
                  e.color === C.keeper ? 'keeper' :
                  e.color === C.pyth ? 'pyth' : 'zk'
                })`}
              />
            ))}

            {/* Edge labels */}
            {EDGES.map((e) => (
              <text
                key={`lbl-${e.id}`}
                x={e.labelX} y={e.labelY}
                fontSize="9.5" fill={e.color}
                fontWeight="600"
                opacity="0.85"
              >
                {e.label}
              </text>
            ))}

            {/* Animated flow dots */}
            {EDGES.map((e, i) => (
              <FlowDot
                key={`dot-${e.id}`}
                pathId={e.id}
                color={e.color}
                dur={2.2 + i * 0.3}
                delay={i * 0.7}
              />
            ))}

            {/* ZK proof callout */}
            <ZkCallout />

            {/* Poseidon hash chain callout inside TEE */}
            <PoseidonCallout />

            {/* Node boxes */}
            {NODES.map((n) => (
              <NodeBox
                key={n.id}
                node={n}
                active={active === n.id}
                onClick={() => toggle(n.id)}
              />
            ))}

            {/* Step numbers on edge midpoints */}
            {EDGES.filter((e) => e.step !== undefined).map((e) => (
              <g key={`step-${e.id}`}>
                <circle cx={e.labelX - 14} cy={e.labelY - 5} r="7"
                  fill={e.color} opacity="0.18" />
              </g>
            ))}

          </svg>
        </div>
      </div>

      {/* Active node detail panel */}
      {activeNode && (
        <div style={{
          position: 'fixed', bottom: 24, left: '50%', transform: 'translateX(-50%)',
          background: 'rgba(10,10,20,0.96)',
          border: `1px solid ${activeNode.color}`,
          borderRadius: 14,
          padding: '18px 24px',
          minWidth: 340, maxWidth: 560,
          boxShadow: `0 0 40px ${activeNode.color}22`,
          fontFamily: 'ui-monospace, monospace',
          zIndex: 100,
        }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 10 }}>
            <span style={{ fontSize: 13, fontWeight: 800, color: activeNode.color, letterSpacing: '0.08em' }}>
              {activeNode.title.toUpperCase()}
            </span>
            <button
              onClick={() => setActive(null)}
              style={{ background: 'none', border: 'none', color: C.dimmer, cursor: 'pointer', fontSize: 18, lineHeight: 1 }}
            >×</button>
          </div>
          <div style={{ fontSize: 10, color: activeNode.color, opacity: 0.7, marginBottom: 12, letterSpacing: '0.06em' }}>
            {activeNode.sub}
          </div>
          <ul style={{ margin: 0, padding: 0, listStyle: 'none', display: 'flex', flexDirection: 'column', gap: 6 }}>
            {activeNode.bullets.map((b, i) => (
              <li key={i} style={{ display: 'flex', alignItems: 'flex-start', gap: 8, fontSize: 11, color: C.dim }}>
                <span style={{ color: activeNode.color, marginTop: 1 }}>›</span>
                {b}
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* Flow step legend */}
      <div style={{
        display: 'flex', justifyContent: 'center', gap: 6, padding: '10px 24px 28px',
        flexWrap: 'wrap', fontFamily: 'ui-monospace, monospace',
      }}>
        {[
          { n: '①', label: 'Encrypt & send order to TEE', color: C.user },
          { n: '②', label: 'TEE generates proof + submits match', color: C.tee },
          { n: '③', label: 'Proof returned to client', color: C.zk },
          { n: '④', label: 'Client submits position tx', color: C.user },
          { n: '⑤', label: 'Oracle keeper pushes Pyth prices', color: C.keeper },
          { n: '⑥', label: 'MM seeds 32-level CLOB', color: C.keeper },
          { n: '⑦', label: 'Liquidator closes under-collateralised', color: C.keeper },
        ].map(({ n, label, color }) => (
          <div key={n} style={{
            display: 'flex', alignItems: 'center', gap: 6,
            padding: '4px 10px', borderRadius: 6,
            background: 'rgba(255,255,255,0.03)',
            border: `1px solid ${C.border}`,
            fontSize: 10,
          }}>
            <span style={{ color, fontWeight: 700 }}>{n}</span>
            <span style={{ color: C.dimmer }}>{label}</span>
          </div>
        ))}
      </div>
    </div>
  )
}
