import { Link } from 'react-router'

export const meta = () => [
  { title: 'Docs — Cerida' },
  { name: 'description', content: 'How Cerida works: ZK circuits, TEE architecture, and live markets.' },
]

// ── Shared prose components ───────────────────────────────────────

function H2({ id, children }: { id: string; children: React.ReactNode }) {
  return (
    <h2
      id={id}
      className="mt-14 mb-4 scroll-mt-24 text-[22px] font-semibold tracking-tight text-text-primary"
    >
      {children}
    </h2>
  )
}

function H3({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="mt-8 mb-3 text-[15px] font-semibold uppercase tracking-widest text-text-tertiary">
      {children}
    </h3>
  )
}

function P({ children }: { children: React.ReactNode }) {
  return <p className="mb-4 leading-7 text-text-secondary">{children}</p>
}

function Code({ children }: { children: React.ReactNode }) {
  return (
    <code className="rounded-[4px] border border-border-subtle bg-surface-card px-1.5 py-0.5 font-mono text-[13px] text-text-primary">
      {children}
    </code>
  )
}

function Pre({ children }: { children: string }) {
  return (
    <pre className="mb-6 overflow-x-auto rounded-[10px] border border-border-subtle bg-surface-card p-4 font-mono text-[12.5px] leading-6 text-text-secondary">
      {children}
    </pre>
  )
}

function Table({ head, rows }: { head: string[]; rows: string[][] }) {
  return (
    <div className="mb-6 overflow-x-auto rounded-[10px] border border-border-subtle">
      <table className="w-full text-[13px]">
        <thead>
          <tr className="border-b border-border-subtle bg-surface-card">
            {head.map((h) => (
              <th
                key={h}
                className="px-4 py-2.5 text-left font-semibold uppercase tracking-widest text-text-quaternary"
              >
                {h}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, i) => (
            <tr
              key={i}
              className="border-b border-border-subtle/50 last:border-0 hover:bg-surface-card/60"
            >
              {row.map((cell, j) => (
                <td key={j} className="px-4 py-2.5 font-mono text-text-secondary">
                  {cell}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

function Pill({ children, color }: { children: string; color?: string }) {
  return (
    <span
      className="inline-block rounded-full px-3 py-1 text-[11px] font-semibold uppercase tracking-widest"
      style={{ background: color ?? 'rgba(128,125,254,0.12)', color: color ? '#fff' : 'var(--color-brand-violet)' }}
    >
      {children}
    </span>
  )
}

// ── TOC ───────────────────────────────────────────────────────────

const TOC = [
  { id: 'what',      label: 'What is Cerida?' },
  { id: 'how',       label: 'How it works' },
  { id: 'trading',   label: 'Trading Features' },
  { id: 'circuits',  label: 'ZK Circuits' },
  { id: 'tee',       label: 'TEE Architecture' },
  { id: 'markets',   label: 'Live Markets' },
  { id: 'keepers',   label: 'Keepers' },
  { id: 'milestones', label: 'Milestones' },
  { id: 'stack',     label: 'Stack' },
]

// ── Page ─────────────────────────────────────────────────────────

export default function DocsPage() {
  return (
    <div className="min-h-screen bg-page text-text-primary">
      {/* Nav */}
      <header className="sticky top-0 z-50 flex items-center gap-4 border-b border-border-subtle bg-surface-primary/80 px-6 py-3 backdrop-blur-sm">
        <Link to="/" className="flex items-center gap-2.5 text-[13px] font-semibold text-text-primary hover:opacity-80">
          <img src="/apple-touch-icon.png" alt="Cerida" className="h-6 w-6 rounded-md" />
          Cerida
        </Link>
        <span className="text-text-quaternary">/</span>
        <span className="text-[13px] text-text-tertiary">Docs</span>
        <div className="ml-auto flex items-center gap-3">
          <Link
            to="/flow"
            className="rounded-[7px] border border-border-subtle px-4 py-1.5 text-[12px] font-semibold text-text-tertiary hover:text-text-primary"
          >
            System Flow
          </Link>
          <Link
            to="/trade/btc"
            className="rounded-[7px] bg-brand-violet px-4 py-1.5 text-[12px] font-semibold text-white hover:opacity-90"
          >
            Launch App →
          </Link>
        </div>
      </header>

      <div className="mx-auto flex max-w-[1100px] gap-10 px-6 py-12">
        {/* Sidebar TOC */}
        <aside className="hidden w-52 shrink-0 lg:block">
          <div className="sticky top-24 space-y-0.5">
            <p className="mb-3 text-[10px] uppercase tracking-widest text-text-quaternary">On this page</p>
            {TOC.map((item) => (
              <a
                key={item.id}
                href={`#${item.id}`}
                className="block rounded-[6px] px-3 py-1.5 text-[12px] text-text-tertiary transition-colors hover:bg-surface-hover hover:text-text-primary"
              >
                {item.label}
              </a>
            ))}
          </div>
        </aside>

        {/* Content */}
        <main className="min-w-0 flex-1">

          {/* Hero */}
          <div className="mb-12 flex items-start gap-5">
            <img src="/apple-touch-icon.png" alt="Cerida" className="h-16 w-16 rounded-2xl shadow-lg" />
            <div>
              <h1 className="text-[32px] font-bold tracking-tight text-text-primary">Cerida</h1>
              <p className="mt-1 text-[15px] text-text-tertiary">
                Zero-knowledge perpetual futures for real-world assets, on Stellar.
              </p>
              <div className="mt-3 flex flex-wrap gap-2">
                {['Groth16 / BN254', 'GCP Confidential Space', 'Stellar Soroban', 'Rust'].map((t) => (
                  <Pill key={t}>{t}</Pill>
                ))}
              </div>
            </div>
          </div>

          {/* What is Cerida */}
          <section id="what">
            <H2 id="what">What is Cerida?</H2>
            <P>
              Cerida is a privacy-preserving perpetuals DEX built on{' '}
              <a href="https://stellar.org/soroban" target="_blank" rel="noopener noreferrer" className="text-brand-violet underline-offset-2 hover:underline">
                Stellar Soroban
              </a>
              . It combines three layers of technology to give traders institutional-grade privacy: no position is ever stored in plaintext, no collateral amount is ever visible, and the chain only sees commitment hashes, nullifiers, and proof verification results.
            </P>

            <Table
              head={['Layer', 'Technology', 'What it does']}
              rows={[
                ['ZK Proofs', 'Groth16 (arkworks BN254)', 'Proves order validity, matching, and collateral spend on-chain — without revealing private inputs'],
                ['TEE', 'GCP Confidential Space (AMD SEV-SNP)', 'Runs the matching engine inside an encrypted enclave. Inputs are encrypted to the TEE; the host operator sees nothing'],
                ['Settlement', 'Stellar Soroban (Protocol 26)', 'Verifies Groth16 proofs via BN254 host functions. Stores commitments, nullifiers, and positions'],
              ]}
            />
          </section>

          {/* How it works */}
          <section id="how">
            <H2 id="how">How It Works</H2>

            <H3>1. Deposit — shielded notes</H3>
            <P>
              A user deposits USDC into the perp engine. The deposit creates a <strong>shielded note</strong>: a Poseidon2 commitment to{' '}
              <Code>(amount, secret)</Code> stored on-chain. Only the depositor knows the preimage. Funds in the shielded pool cannot be linked to any position or withdrawal without the secret.
            </P>
            <Pre>{`note_commitment = Poseidon2(amount, secret, domain=8)`}</Pre>

            <H3>2. Place an Order — OrderCommitment proof</H3>
            <P>
              The client sends encrypted order parameters to the TEE. The TEE generates a Groth16 proof inside the enclave — inputs never leave the SEV-SNP boundary — and returns a commitment built from a Poseidon2 hash chain over all order fields:
            </P>
            <Pre>{`h1 = Poseidon2(side,  price,     domain=1)
h2 = Poseidon2(h1,   size,      domain=2)
h3 = Poseidon2(h2,   leverage,  domain=3)
h4 = Poseidon2(h3,   asset,     domain=4)
h5 = Poseidon2(h4,   is_market, domain=5)
h6 = Poseidon2(h5,   nonce,     domain=6)
commitment = Poseidon2(h6, secret, domain=7)`}</Pre>
            <P>
              Stellar verifies the Groth16 proof via BN254 MSM and pairing host functions, then registers the order in the CLOB.
            </P>

            <H3>3. Match — OrderMatch proof</H3>
            <P>When two orders cross in the TEE's CLOB engine, the enclave generates an OrderMatch proof that proves in zero-knowledge:</P>
            <ul className="mb-6 space-y-1.5 pl-5 text-[14px] text-text-secondary">
              {[
                'Both order commitments are valid (the Poseidon2 chain holds for each)',
                <>Orders are on opposite sides — <Code>side_a + side_b = 1</Code></>,
                <>Same underlying asset — <Code>asset_a = asset_b</Code></>,
                'Not both market orders simultaneously',
                'Match price is within each limit\'s declared bounds',
                'Match size ≤ both order sizes',
                <>Nullifiers correctly derived: <Code>Poseidon2(commitment, match_price, match_size, domain=10)</Code></>,
              ].map((item, i) => (
                <li key={i} className="flex items-start gap-2">
                  <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-brand-violet" />
                  <span>{item}</span>
                </li>
              ))}
            </ul>
            <P>
              Public outputs: <Code>(cmt_a, cmt_b, match_price, match_size, nullifier_a, nullifier_b)</Code> — no private order details exposed.
            </P>

            <H3>4. Open Position — note spend</H3>
            <P>
              Opening a matched position requires two proofs in sequence: a <strong>NoteSpend</strong> proof (proving the trader knows the secret for a shielded note with enough collateral), and an <strong>OrderCommitment</strong> proof binding the position to the matched order. The note nullifier is published, collateral locked, and the position commitment stored on-chain.
            </P>

            <H3>5. Close / Withdraw</H3>
            <P>
              Closing a position or withdrawing from the shielded pool requires a NoteSpend proof. The nullifier is checked for uniqueness (replay protection), collateral returned, and the nullifier marked spent permanently.
            </P>
          </section>

          {/* Trading Features */}
          <section id="trading">
            <H2 id="trading">Trading Features</H2>

            <H3>Order Types</H3>
            <P>The CLOB engine inside the TEE supports six order types:</P>
            <Table
              head={['Type', 'Behaviour']}
              rows={[
                ['Market', 'Fills immediately at best available price. No price constraint in the ZK proof.'],
                ['Limit', 'Rests in the book until the market price crosses the declared limit.'],
                ['Stop-Limit', 'Dormant until mark price crosses the stop trigger, then activates as a limit order.'],
                ['Stop-Market', 'Dormant until mark price crosses the stop trigger, then activates as a market order.'],
                ['IOC (Immediate-or-Cancel)', 'Fills whatever is available now, cancels the rest. Never rests in the book.'],
                ['FOK (Fill-or-Kill)', 'Fills the entire size immediately or rejects entirely.'],
              ]}
            />
            <P>
              Stop orders sit in a separate stop book inside the TEE. The engine scans and triggers them on every mark price update:
            </P>
            <ul className="mb-6 space-y-1.5 pl-5 text-[14px] text-text-secondary">
              {[
                <>Bid stops trigger when <Code>mark_price ≥ stop_price</Code> — stop-loss on shorts, buy-stop breakouts</>,
                <>Ask stops trigger when <Code>mark_price ≤ stop_price</Code> — stop-loss on longs, sell-stop breakouts</>,
              ].map((item, i) => (
                <li key={i} className="flex items-start gap-2">
                  <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-brand-violet" />
                  <span>{item}</span>
                </li>
              ))}
            </ul>

            <H3>Take Profit / Stop Loss (TP/SL)</H3>
            <P>
              TP and SL are implemented as a pair of stop orders placed alongside the opening commitment:
            </P>
            <ul className="mb-6 space-y-1.5 pl-5 text-[14px] text-text-secondary">
              {[
                <><strong className="text-text-primary">Take Profit</strong> — Stop-Limit on the opposite side at your target price</>,
                <><strong className="text-text-primary">Stop Loss</strong> — Stop-Market on the opposite side at your risk limit</>,
              ].map((item, i) => (
                <li key={i} className="flex items-start gap-2">
                  <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-brand-violet" />
                  <span>{item}</span>
                </li>
              ))}
            </ul>
            <P>
              Both reference the same <Code>position_commitment</Code>. When either triggers and fills, the matching proof nullifies the position commitment so the other is automatically invalidated on settlement.
            </P>

            <H3>Leverage</H3>
            <P>
              Up to <strong>50× leverage</strong> on crypto markets, <strong>10–20× on RWA markets</strong>, enforced on-chain — the contract validates <Code>leverage ≤ asset.max_leverage</Code> and rejects if exceeded.
            </P>
            <Pre>{`notional          = collateral × leverage
liq_price (long)  = entry × (1 − 0.92 / leverage)
liq_price (short) = entry × (1 + 0.92 / leverage)`}</Pre>

            <H3>Isolated vs Cross Margin</H3>
            <P>
              Configured via a single boolean witness <Code>use_cross</Code> in the <Code>OrderCommitment</Code> circuit — proved in zero-knowledge, so margin mode is verifiable without revealing it.
            </P>
            <Table
              head={['Mode', 'Behaviour']}
              rows={[
                ['Isolated', 'Each position has its own collateral bucket. A loss cannot spill to other positions.'],
                ['Cross', 'Positions share a portfolio collateral pool, linked by portfolio_key = Poseidon2(secret, 0, domain=20).'],
              ]}
            />
            <P>
              The on-chain state only ever sees the <Code>portfolio_key</Code> hash — not the secret or the trader's identity.
            </P>

            <H3>Liquidation</H3>
            <P>Any address can call <Code>liquidate(position_commitment)</Code>. The contract:</P>
            <ul className="mb-6 space-y-1.5 pl-5 text-[14px] text-text-secondary">
              {[
                <>Reads the position's <Code>collateral</Code>, <Code>leverage</Code>, <Code>entry_price</Code>, and current <Code>mark_price</Code> from oracle</>,
                <>Computes unrealised PnL: <Code>pnl = (mark − entry) / entry × collateral × leverage × direction</Code></>,
                <>If <Code>collateral + pnl {'<'} maintenance_margin</Code>, the position is under-collateralised</>,
                'Liquidator receives a fee; remaining collateral (if any) is returned to the pool',
                'Position commitment marked Liquidated — nullifier cannot be spent again',
              ].map((item, i) => (
                <li key={i} className="flex items-start gap-2">
                  <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-brand-violet" />
                  <span>{item}</span>
                </li>
              ))}
            </ul>
          </section>

          {/* ZK Circuits */}
          <section id="circuits">
            <H2 id="circuits">ZK Circuits</H2>
            <P>
              All circuits are written in pure Rust using{' '}
              <a href="https://arkworks.rs" target="_blank" rel="noopener noreferrer" className="text-brand-violet underline-offset-2 hover:underline">arkworks</a>{' '}
              — BN254 + Groth16. No Circom, no NoirLang. Everything is native R1CS over the BN254 scalar field, hand-written with <Code>ark-r1cs-std</Code>.
              The hash primitive is <strong>Poseidon2</strong> (width-3 sponge, distinct domain separators per round), natively supported by Stellar Soroban's Protocol 26 BN254 host functions.
            </P>

            <Table
              head={['Circuit', 'Public Inputs', 'Proves']}
              rows={[
                ['OrderCommitment', 'commitment, portfolio_key', 'Poseidon2 chain over all 8 order fields = commitment. Optionally proves cross-margin group key.'],
                ['NoteSpend', 'note_commitment, nullifier', 'note_commitment = Poseidon2(amount, secret, 8) and nullifier = Poseidon2(note_commitment, secret, 9)'],
                ['OrderMatch', 'cmt_a, cmt_b, match_price, match_size, nullifier_a, nullifier_b', 'Both commitments valid, opposite sides, same asset, price constraints, size bounds, correct nullifiers'],
                ['OrderCancel', 'commitment, cancel_nullifier', 'Holder knows the secret behind the commitment and derives a valid cancel nullifier'],
                ['ShieldedInsert', 'root_before, root_after, leaf', 'Merkle tree insertion was performed correctly'],
                ['ShieldedWithdraw', 'root, nullifier', 'Leaf exists in tree and nullifier correctly derived from it'],
              ]}
            />

            <H3>OrderMatch: price constraints</H3>
            <P>
              For limit orders the circuit enforces fill-price correctness via conditional range checks using bit-decomposition over the 254-bit BN254 field. For a bid (<Code>side=0</Code>):
            </P>
            <Pre>{`match_price ≤ declared_limit_price    // buyer filled at or better`}</Pre>
            <P>For an ask (<Code>side=1</Code>):</P>
            <Pre>{`declared_limit_price ≤ match_price    // seller filled at or better`}</Pre>
            <P>
              The <Code>enforce_cond_le</Code> gadget gates these checks with <Code>is_market</Code> — market orders bypass the price constraint entirely.
            </P>

            <H3>Cross-margin extension</H3>
            <P>
              <Code>OrderCommitment</Code> supports isolated and cross-margin via a single boolean witness <Code>use_cross</Code>. When set, it additionally proves:
            </P>
            <Pre>{`portfolio_key = use_cross × Poseidon2(secret, 0, domain=20)`}</Pre>
            <P>
              A zero <Code>portfolio_key</Code> means isolated margin. Non-zero groups positions into a cross-margin portfolio. The secret is never revealed.
            </P>
          </section>

          {/* TEE */}
          <section id="tee">
            <H2 id="tee">TEE: The Matching Engine</H2>
            <P>
              <Code>tee-match</Code> is a Rust binary compiled into a Docker image and deployed to{' '}
              <a href="https://cloud.google.com/confidential-computing" target="_blank" rel="noopener noreferrer" className="text-brand-violet underline-offset-2 hover:underline">
                GCP Confidential Space
              </a>{' '}
              with AMD SEV-SNP attestation. The enclave holds the Groth16 proving keys for all circuits. It decrypts order inputs, runs the CLOB, and generates proofs — all inside the SEV-SNP boundary.
            </P>
            <Pre>{`┌─────────────────────────────────────────────────────────────┐
│               GCP Confidential Space (SEV-SNP)              │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐   │
│  │               tee-match (Rust binary)                │   │
│  │                                                      │   │
│  │   TCP :9720  — encrypted JSON-lines (orders/proofs)  │   │
│  │   HTTP :9721 — public REST (depth, mark price)       │   │
│  │                                                      │   │
│  │   • CLOB engine (price-time priority, RwLock + WAL)  │   │
│  │   • Groth16 prover (arkworks, in-process)            │   │
│  │   • Poseidon2 commitment hash                        │   │
│  │   • Stellar tx construction + submission             │   │
│  │   • KMS-backed signing key                           │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                             │
│  The host sees: encrypted memory, nothing else.             │
└─────────────────────────────────────────────────────────────┘`}</Pre>

            <H3>Public HTTP endpoints (port 9721)</H3>
            <Table
              head={['Endpoint', 'Returns']}
              rows={[
                ['GET /get-market?asset=N', '32-level bid/ask depth (prices in 7-decimal scale)'],
                ['GET /mark-price?asset=N', 'Current oracle mark price'],
                ['POST /place-order', 'Accepts order inputs, returns Groth16 proof + commitment'],
                ['POST /prove-note-cmt', 'Returns Poseidon2 note commitment'],
                ['POST /prove-note-spend', 'Returns NoteSpend Groth16 proof'],
              ]}
            />
          </section>

          {/* Markets */}
          <section id="markets">
            <H2 id="markets">Live Markets</H2>
            <P>
              Seven perpetual markets run on testnet, priced via{' '}
              <a href="https://hermes.pyth.network" target="_blank" rel="noopener noreferrer" className="text-brand-violet underline-offset-2 hover:underline">
                Pyth Hermes
              </a>{' '}
              real-time WebSocket feeds. Charts use Pyth Benchmarks for historical OHLCV and Hermes WebSocket for live 1-minute candle updates. The chart shows the <strong>index price</strong> — the oracle feed — which is standard for oracle-priced perp venues (GMX, dYdX, Hyperliquid all do this).
            </P>
            <Table
              head={['Market', 'Asset', 'Category']}
              rows={[
                ['BTC-PERP', 'Bitcoin', 'Crypto'],
                ['XRP-PERP', 'XRP / XRPL', 'Crypto'],
                ['XLM-PERP', 'Stellar (XLM)', 'Crypto'],
                ['SPACEX-PERP', 'SpaceX equity', 'RWA'],
                ['TSLA-PERP', 'Tesla', 'RWA'],
                ['OIL-PERP', 'WTI Crude Oil', 'RWA'],
                ['GOLD-PERP', 'Gold (XAU/USD)', 'RWA'],
              ]}
            />
          </section>

          {/* Keepers */}
          <section id="keepers">
            <H2 id="keepers">Keeper Infrastructure</H2>
            <P>A keeper binary runs alongside the TEE and handles three jobs:</P>

            <H3>Oracle keeper</H3>
            <P>
              Fetches Pyth prices every 30 seconds, submits <Code>set_asset_price</Code> on-chain for each market via <Code>stellar</Code> CLI + Soroban RPC.
            </P>

            <H3>Market maker</H3>
            <P>
              32 bid levels + 32 ask levels per market. Size grows geometrically: <Code>base_size × 1.08^level</Code>. Spread formula per category:
            </P>
            <Pre>{`Crypto markets:  (5 + 3×level) bps
RWA markets:    (10 + 5×level) bps`}</Pre>
            <P>
              Re-quotes when mid moves {'>'} 0.5% or quotes are stale ({'>'} 5 min TTL). Commitment proofs are pre-generated in a pool to avoid proof latency during re-balancing.
            </P>

            <H3>Liquidator</H3>
            <P>
              Scans a watchlist of matched positions and calls <Code>liquidate()</Code> on any commitment that falls below the maintenance margin threshold.
            </P>
          </section>

          {/* Milestones */}
          <section id="milestones">
            <H2 id="milestones">Hackathon Milestones</H2>
            <P>These shipped, they weren't planned.</P>

            <div className="space-y-4">
              {[
                {
                  label: 'ZK circuits from scratch',
                  body: '6 Groth16 circuits in pure Rust (arkworks). No Circom, no frameworks — R1CS constraints hand-written over BN254. Implemented Poseidon2 width-3 sponge natively in ark-r1cs-std. Proved on testnet: verifier accepts valid witnesses, panics on invalid ones.',
                },
                {
                  label: 'Soroban contracts + TEE wired end-to-end',
                  body: 'Deployed perp-engine, orderbook, and collateral-token to Stellar testnet. Built tee-match — a Rust CLOB engine + Groth16 prover inside a GCP Confidential Space Docker image. Wired the full flow: shielded deposit → commitment proof → order placement → TEE match → on-chain settlement. Real proofs. Real transactions. Verified on-chain.',
                },
                {
                  label: 'Keeper infrastructure',
                  body: 'Integrated Pyth Network (Hermes REST + WebSocket) for all 7 markets. Built the 32-level algorithmic market maker with geometric size scaling and per-category spread. Oracle keeper submits live prices every 30 seconds. Liquidator watches matched positions automatically.',
                },
                {
                  label: 'Frontend on testnet',
                  body: 'Full trading UI: Freighter wallet, live 32-level orderbook (TEE depth polling with seeded fallback), Pyth-powered candlestick charts (historical + live 1-min WebSocket ticks), funding rate, shielded deposit/withdraw, real transaction history from Stellar Horizon. End-to-end trade live on testnet with real ZK proofs.',
                },
              ].map((m, i) => (
                <div
                  key={i}
                  className="rounded-[10px] border border-border-subtle bg-surface-card p-5"
                >
                  <div className="mb-2 flex items-center gap-3">
                    <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-brand-violet text-[11px] font-bold text-white">
                      {i + 1}
                    </span>
                    <span className="text-[14px] font-semibold text-text-primary">{m.label}</span>
                  </div>
                  <p className="pl-9 text-[13px] leading-6 text-text-secondary">{m.body}</p>
                </div>
              ))}
            </div>
          </section>

          {/* Architecture */}
          <section className="mt-14">
            <H2 id="arch">Architecture at a Glance</H2>
            <Pre>{`Browser (Freighter Wallet)
      │
      │  encrypted order inputs → TEE pubkey
      ▼
TEE: tee-match  (GCP Confidential Space / AMD SEV-SNP)
      │
      │  1. decrypt inside enclave
      │  2. CLOB matching engine
      │  3. Groth16 proof generation (arkworks BN254)
      │  4. Stellar transaction construction
      │
      ▼
Stellar Testnet  (Soroban Protocol 26)
      │
      │  verify_groth16(proof, public_inputs, vk)
      │    → BN254 multi-scalar multiplication
      │    → BN254 pairing check
      │
      ├── perp-engine       positions, collateral, oracle prices
      ├── orderbook         order commitments, nullifiers
      └── collateral-token  USDC (mint / burn / transfer)`}</Pre>
          </section>

          {/* Stack */}
          <section id="stack">
            <H2 id="stack">Stack</H2>
            <Table
              head={['Component', 'Technology']}
              rows={[
                ['Smart contracts', 'Rust / Soroban SDK'],
                ['ZK proof system', 'arkworks (BN254, Groth16, ark-r1cs-std)'],
                ['Hash function', 'Poseidon2 (width-3, BN254 scalar field)'],
                ['TEE', 'GCP Confidential Space, AMD SEV-SNP'],
                ['Matching engine', 'Custom CLOB (Rust, price-time priority, WAL)'],
                ['Oracle', 'Pyth Network (Hermes REST + WebSocket)'],
                ['Wallet', 'Freighter (Stellar)'],
                ['Frontend', 'React, Remix, lightweight-charts, TailwindCSS'],
                ['Keepers', 'Rust (oracle + 32-level market maker + liquidator)'],
              ]}
            />
          </section>

          {/* CTA */}
          <div className="mt-16 flex items-center justify-between rounded-[14px] border border-brand-violet/20 bg-brand-violet/5 px-8 py-6">
            <div>
              <p className="text-[15px] font-semibold text-text-primary">Ready to trade?</p>
              <p className="mt-0.5 text-[13px] text-text-tertiary">
                Connect Freighter and open your first shielded position on Stellar testnet.
              </p>
            </div>
            <Link
              to="/trade/btc"
              className="shrink-0 rounded-[9px] bg-brand-violet px-6 py-2.5 text-[13px] font-semibold text-white shadow-md hover:opacity-90"
            >
              Launch App →
            </Link>
          </div>
        </main>
      </div>
    </div>
  )
}
