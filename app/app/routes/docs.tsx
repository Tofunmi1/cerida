import { Link } from 'react-router'

export const meta = () => [
  { title: 'Docs — Cerida' },
  { name: 'description', content: 'How Cerida works: ZK circuits, TEE architecture, and live markets.' },
  { property: 'og:title', content: 'Docs — Cerida' },
  { property: 'og:description', content: 'How Cerida works: ZK circuits, TEE architecture, and live markets.' },
  { property: 'og:image', content: 'https://ceridapp.xyz/prev_x.png' },
  { property: 'og:type', content: 'website' },
  { name: 'twitter:card', content: 'summary_large_image' },
  { name: 'twitter:image', content: 'https://ceridapp.xyz/prev_x.png' },
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

function Table({ head, rows }: { head: React.ReactNode[]; rows: React.ReactNode[][] }) {
  return (
    <div className="mb-6 overflow-x-auto rounded-[10px] border border-border-subtle">
      <table className="w-full text-[13px]">
        <thead>
          <tr className="border-b border-border-subtle bg-surface-card">
            {head.map((h, idx) => (
              <th
                key={idx}
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
  { id: 'start',     label: 'Quick Start' },
  { id: 'privacy',   label: 'Privacy Model' },
  { id: 'how',       label: 'How it works' },
  { id: 'lifecycle', label: 'Position Lifecycle' },
  { id: 'trading',   label: 'Trading Features' },
  { id: 'circuits',  label: 'ZK Circuits' },
  { id: 'tee',       label: 'TEE Architecture' },
  { id: 'api',       label: 'API Reference' },
  { id: 'markets',   label: 'Live Markets' },
  { id: 'keepers',   label: 'Keepers' },
  { id: 'contract',  label: 'Contract Reference' },
  { id: 'faq',       label: 'Troubleshooting' },
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
               . The contract never stores plaintext amounts — every collateral figure, position value, and note balance is stored as a commitment hash. Only the TEE (Trusted Execution Environment) can decrypt and compute the actual values. An observer sees only hashes, nullifiers, and proof verifications.
             </P>

             <Table
               head={['Layer', 'Technology', 'What it does']}
               rows={[
                 ['ZK Proofs', 'Groth16 (arkworks BN254)', 'Proves order validity, note ownership, and cancellation — without revealing private inputs'],
                 ['TEE', 'GCP Confidential Space (AMD SEV-SNP)', 'Runs matching engine inside an encrypted enclave. Holds all plaintext values; computes PnL, liquidations, and settlements. Host operator sees nothing'],
                  ['Settlement', 'Stellar Soroban', 'Verifies every Groth16 ZK proof via BN254 MSM + pairing host functions (Protocol 26) before accepting any state transition. Anchors all positions and notes as commitment hashes; enforces nullifier uniqueness to prevent replays. Authorizes only the TEE account to settle positions and process withdrawals. Oracles and funding run off-chain inside the TEE — no on-chain price feeds required'],
               ]}
             />
           </section>

          {/* Quick Start */}
          <section id="start">
            <H2 id="start">Quick Start</H2>
            <ol className="ml-5 list-decimal space-y-2 text-[14px] leading-relaxed text-text-secondary">
              <li>
                <strong className="text-text-primary">Install Freighter</strong> — add the Freighter wallet extension and switch to Stellar testnet.
              </li>
              <li>
                <strong className="text-text-primary">Fund your wallet</strong> — claim testnet XLM from the{' '}
                <a href="https://laboratory.stellar.org/#account-creator?network=test" target="_blank" rel="noopener noreferrer" className="text-brand-violet underline-offset-2 hover:underline">
                  Stellar Laboratory friendbot
                </a>
                . Use the in-app faucet to mint test USDC if available.
              </li>
              <li>
                <strong className="text-text-primary">Connect and deposit</strong> — connect Freighter, choose an amount, and sign the <Code>deposit_note</Code> transaction. Your USDC moves into a shielded note.
              </li>
              <li>
                <strong className="text-text-primary">Open a position</strong> — pick a market, size, leverage, and direction. The browser builds a ZK OrderCommitment proof and queues the order for the TEE.
              </li>
              <li>
                <strong className="text-text-primary">Close or cancel</strong> — close at market price, or cancel a pending limit order before it fills. Settled collateral returns to a fresh shielded note.
              </li>
            </ol>
            <P>All ZK proving happens locally; expect ~9–18 s before the TEE queues your relay.</P>
          </section>

          {/* Privacy Model */}
          <section id="privacy">
            <H2 id="privacy">Privacy Model</H2>
            <P>
              All financially meaningful values on-chain are <strong>commitment hashes</strong> — SHA-256 or Poseidon2 outputs that reveal nothing about the underlying data. The contract never sees plaintext collateral, position values, or note balances.
            </P>

            <Table
              head={['On-chain', 'Store as', 'Plaintext lives in']}
              rows={[
                ['Note balance', <><Code>SHA256(amount || blinding)</Code></>, 'TEE DB (sled), keyed by note commitment'],
                ['Position financials', <><Code>settlement_commitment</Code> (SHA256)</>, 'TEE DB as PositionState struct'],
                ['Order parameters (side, price, leverage, size)', <><Code>sealed_params</Code> (AEAD-encrypted blob)</>, 'TEE memory, AES-256-GCM decrypted'],
                ['Portfolio membership', <><Code>portfolio_key</Code> (Poseidon2 hash)</>, 'Derived from secret — TEE can recompute'],
              ]}
            />

            <H3>Sealed position parameters</H3>
            <P>
              When a position is opened, the TEE encrypts order details (side, entry price, leverage, size, TP/SL, TIF, expiry) into a <Code>BytesN&lt;92&gt;</Code> blob stored on-chain. In dev builds this is plain big-endian u64 packing. In the secure build, it's AES-256-GCM encrypted under <Code>CER_DEK</Code> — only the same TEE instance can decrypt it. If the DEK changes, existing positions become unreadable.
            </P>

            <H3>Batch relay privacy</H3>
            <P>
              The TEE queues on-chain transactions into an in-memory buffer and flushes every <strong>10 seconds</strong>. Before submission, entries are shuffled (Fisher-Yates). This breaks timing correlation between user HTTP requests and on-chain transactions — deposit and position-open TXs appear in random order, masking which user opened which position.
            </P>

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

             <H3>4. Open Position — relay + note spend</H3>
             <P>
               The user signs <Code>deposit_note</Code> (shields collateral). The TEE then acts as a relayer — it holds a separate signing key and submits <Code>place_order</Code> + <Code>open_position_from_note</Code> on the user's behalf. This bundles two Groth16 proofs: a <strong>NoteSpend</strong> proof (proving knowledge of the shielded note) and an <strong>OrderCommitment</strong> proof (binding the position to the order). The note nullifier is published, collateral locked, and the position commitment stored on-chain — with the user's address never appearing in the position-opening transaction.
             </P>

             <H3>5. Match &amp; Settle</H3>
             <P>
               When two orders cross in the TEE's CLOB, the enclave computes PnL locally using the match price as the oracle. It calls <Code>settle_position</Code> for both sides — a single generic contract function that records the outcome (Closed or Liquidated) and stores settlement notes. The contract only verifies TEE authorization (<Code>require_tee_auth</Code>) and updates status/commitments — it performs no financial computation.
             </P>

             <H3>6. Claim Settlement</H3>
             <P>
               Settled positions generate a settlement note — a commitment to the payout amount. To claim, the user sends a request to the TEE. The TEE generates a NoteSpend ZK proof and submits <Code>withdraw_note</Code> on-chain, sending the payout to the user's wallet.
             </P>
           </section>

          {/* Position Lifecycle */}
          <section id="lifecycle">
            <H2 id="lifecycle">Position Lifecycle</H2>
            <Pre>{`1. USER → TEE:     tee.init()         → Groth16 OrderCommitment proof
2. USER → TEE:     tee.noteProof()    → Groth16 NoteSpend proof
3. USER signs:     deposit_note TX    → shields collateral on-chain
4. TEE relay:      place_order        → registers on orderbook
5. TEE relay:      open_position_from_note  → position stored (committed)
6. TEE CLOB:       match engine       → finds crossing orders
7. TEE relay:      settle_position ×2 → closes both positions on-chain
8. TEE store:      NoteAmount         → payout commitment in DB
9. FRONTEND:       "Claim" button     → user requests withdrawal
10. TEE relay:     gen_note_proof     → ZK NoteSpend proof (~9s)
11. TEE relay:     withdraw_note      → payout sent to user wallet`}</Pre>

            <H3>Liquidation flow</H3>
            <Pre>{`Liquidator thread (TEE, every N seconds):
  1. scan all pos_* entries in TEE DB
  2. fetch oracle price from Pyth Hermes
  3. compute PnL = notional × (oracle − entry) / entry
  4. if solvent (settlement ≥ 5% margin) → skip
  
  Partial (Tier 1): liquidate half, keep position alive
    → settle_partial  (marks partial_liq_done = true)
  
  Full (Tier 2): liquidate entire position
    → settle_position(status=4)  (marks Liquidated)
    → stores settlement NoteAmount in TEE DB`}</Pre>
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
             <P>
               A background thread in the TEE scans all tracked positions every N seconds:
             </P>
             <ul className="mb-6 space-y-1.5 pl-5 text-[14px] text-text-secondary">
               {[
                 <>Fetches live oracle prices from Pyth Hermes for each position's asset</>,
                 <>Computes unrealised PnL locally: <Code>pnl = notional × (oracle − entry) / entry</Code></>,
                 <>If <Code>settlement {'<'} maintenance_margin</Code> (5% of collateral), the position is under-collateralised</>,
                 <><strong>Tier 1 — Partial:</strong> liquidates half the collateral via <Code>settle_partial</Code>. Position stays open with reduced margin</>,
                 <><strong>Tier 2 — Full:</strong> liquidates the entire position via <Code>settle_position(status=4)</Code>. Settlement note stored in TEE DB for the user to claim</>,
                 'Liquidator reward: 1% (partial) or 1.5% (full) of collateral. Insurance fund fee: 0.5%',
               ].map((item, i) => (
                 <li key={i} className="flex items-start gap-2">
                   <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-brand-violet" />
                   <span>{item}</span>
                 </li>
               ))}
              </ul>

            <H3>Fees</H3>
            <P>
              Trading fees are taken from collateral on open/close. There are no per-trade gas fees on Soroban testnet beyond the base network fee.
            </P>
            <Table
              head={['Fee', 'Rate', 'Note']}
              rows={[
                ['Maker', '0.02%', 'Earned by limit orders that add liquidity to the CLOB'],
                ['Taker', '0.05%', 'Charged to market orders and immediately filled limit orders'],
                ['Liquidation (partial)', '1.0%', 'Deducted from collateral on tier-1 liquidation'],
                ['Liquidation (full)', '1.5%', 'Deducted from collateral on tier-2 liquidation'],
                ['Insurance fund', '0.5%', 'Charged on liquidations only'],
              ]}
            />
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
│  │   TCP :9720  — keepers (fast-init, place, cancel)    │   │
│  │   HTTP :9721 — frontend API (proofs, relay, depth)   │   │
│  │                                                      │   │
│  │   • CLOB engine (price-time priority)                │   │
│  │   • Groth16 prover (arkworks, ~9s per proof)         │   │
│  │   • Poseidon2 / SHA256 hashing                       │   │
│  │   • Liquidator thread (Pyth → PnL → settle_position) │   │
│  │   • Batch relay (10s shuffle, Fisher-Yates)          │   │
│  │   • sled DB (secrets, positions, notes, CLOB state)  │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                             │
│  The host sees: encrypted memory, nothing else.             │
└─────────────────────────────────────────────────────────────┘`}</Pre>

             <H3>TEE DB structure</H3>
              <Table
                head={['Key prefix', 'Value type', 'Stored when']}
                rows={[
                  [<><Code>sec_{'<cmt>'}</Code></>, 'Order secrets (side, price, size, leverage…)', 'User calls /init or /fast-init'],
                  [<><Code>pos_{'<cmt>'}</Code></>, 'PositionState (collateral, entry, leverage, side…)', 'Position opened via relay'],
                  [<><Code>note_{'<cmt>'}</Code></>, 'NoteAmount (amount, blinding, secret)', 'Settlement, cancel, or deposit'],
                  [<><Code>set_{'<cmt>'}</Code></>, 'Settlement note commitment', 'Position settled or liquidated'],
                  [<><Code>tx_{'<cmt>'}</Code></>, 'On-chain TX hash', 'Relay TX confirmed'],
                  [<><Code>book_{'<asset>'}</Code></>, 'Serialized OrderBook', 'Every CLOB write'],
                  [<><Code>fill_{'<id>'}</Code></>, 'Fill entry (taker, maker, price, size)', 'Every match'],
                ]}
              />
          </section>

          {/* API */}
          <section id="api">
            <H2 id="api">HTTP API Reference</H2>
            <P>All endpoints via <Code>/tee/</Code> (Vercel edge rewrite proxy to TEE port 9721).</P>

            <H3>Proof &amp; Commitment</H3>
            <Table
              head={['Endpoint', 'Method', 'Purpose', 'Time']}
              rows={[
                ['/init', 'POST', 'Generate commitment + Groth16 proof', '~9s'],
                ['/fast-init', 'POST', 'Commitment hash only, no proof', '<1ms'],
                ['/commit-proof', 'POST', 'Groth16 commit proof for existing cmt', '~9s'],
                ['/cancel-proof', 'POST', 'Groth16 cancel proof + nullifier', '~9s'],
                ['/note-proof', 'POST', 'NoteSpend Groth16 proof', '~9s'],
                ['/note-cmt', 'POST', 'Poseidon2 note commitment (no proof)', '<1ms'],
              ]}
            />

            <H3>Relay</H3>
            <Table
              head={['Endpoint', 'Method', 'Purpose']}
              rows={[
                ['/relay/open-position', 'POST', 'Queue market order for batch relay → open_position_from_note'],
                ['/relay/place-limit', 'POST', 'Store limit order params → CLOB insert; relay on fill'],
                ['/relay/cancel-position', 'POST', 'cancel_position_to_note + ZK proof + withdraw_note'],
                ['/relay/deposit-note', 'POST', 'Queue pre-signed deposit XDR for batch submission'],
                ['/relay/withdraw-settlement', 'POST', 'Claim settled/liquidated funds → withdraw_note'],
                ['/relay/position-tx', 'GET', 'Poll for on-chain TX hash after relay'],
              ]}
            />

            <H3>Data</H3>
            <Table
              head={['Endpoint', 'Method', 'Purpose']}
              rows={[
                ['/get-market?asset=N', 'GET', '32-level bid/ask depth snapshot'],
                [<>/note-amount?cmt={'<hex>'}</>, 'GET', 'Look up note amount in TEE DB'],
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
            <P>A separate Rust binary running on its own GCP VM:</P>

            <H3>Market maker</H3>
            <P>
              Maintains 32 bid + 32 ask levels across all 7 markets — 448 active quotes. Quotes are bootstrapped in one shot via <Code>/batch-fast-init</Code> so the book is live in seconds instead of minutes. There is no persistent pool; each tick regenerates missing levels from scratch and reconciles live depth against expected quotes to detect and replace filled orders.
            </P>
            <Pre>{`Crypto markets:  (5 + 3×level) bps,  size = base × 1.08^level
RWA markets:    (10 + 5×level) bps,  size = base × 1.08^level`}</Pre>
            <P>
              Re-quotes when mid moves {'>'} 0.5%, quotes exceed their TTL (300 s), or live-depth reconciliation detects a fill. On startup the keeper can optionally call <Code>/clear-book</Code> to wipe stale quotes from a previous run. Pyth prices are fetched every tick (60 s default).
            </P>

            <H3>Oracle keeper</H3>
            <P>
              Not yet implemented — Pyth prices are currently fetched directly by the TEE liquidator and the market maker. An on-chain oracle keeper (pushing prices to the contract via <Code>push_oracle_price</Code>) is planned for the pre-Phase 2 contract.
            </P>

            <H3>Liquidator</H3>
            <P>
              Now runs inside the TEE process itself — not in the keepers binary. A background thread scans all tracked positions, fetches Pyth prices, and calls <Code>settle_position</Code> or <Code>settle_partial</Code> when maintenance margin is breached.
            </P>
          </section>

          {/* Contract Reference */}
          <section id="contract">
            <H2 id="contract">Contract Reference</H2>
            <P>
              Main perp-engine contract on Stellar testnet. Deployed contract and TEE account share the same key.
            </P>

            <H3>Contract addresses (testnet)</H3>
            <Table
              head={['Contract', 'ID']}
              rows={[
                ['Perp engine', 'CCT476K37KCWZFXWMXXPUKH2FWJESOJVLMSGS2DKCFZFYSZII42XY4VW'],
                ['Shielded pool', 'CBXDBBHVA7EGFOWDWGLRM73EWIFKGIYJB7Z5R4NQEEJSKD5F5IHA6RND'],
                ['Orderbook', 'CC72USWIXJBFTXVIXKMQK7BMLU2FW53FTSHH3GA5XCDHEHQPGD45KMVG'],
                ['Collateral token (test USDC)', 'CA6SDT7HE7LMYFYUQZGTVG6QJNKKZC2CZWE6ZK7YBZFZA23UNIGLBKFA'],
              ]}
            />

            <H3>Authorization model</H3>
            <Table
              head={['Function', 'Auth required']}
              rows={[
                [<><Code>deposit_note</Code></>, <><Code>from.require_auth()</Code> — user signs with wallet</>],
                [<><Code>withdraw_note</Code></>, 'ZK NoteSpend proof — TEE relay only'],
                [<><Code>open_position_from_note</Code></>, 'ZK NoteSpend + OrderCommitment proofs'],
                [<><Code>open_position_from_pool</Code></>, 'ZK ShieldedWithdraw + OrderCommitment proofs'],
                [<><Code>cancel_position_to_note</Code></>, 'ZK OrderCancel proof'],
                [<><Code>settle_position</Code></>, <><Code>require_tee_auth</Code> — invoker must be stored TEE account</>],
                [<><Code>settle_partial</Code></>, <><Code>require_tee_auth</Code></>],
                [<><Code>add_margin_from_note</Code></>, <>ZK NoteSpend proof + <Code>require_tee_auth</Code></>],
                [<><Code>fund_insurance</Code></>, 'Anyone'],
                [<><Code>upgrade</Code></>, <>Admin <Code>require_auth()</Code></>],
                [<><Code>set_tee_account</Code></>, <>Admin <Code>require_auth()</Code></>],
              ]}
            />
            <P>
              <Code>require_tee_auth</Code> works via invoker auth: the TX source IS the TEE account, so auth passes without an explicit entry. If <Code>STELLAR_SOURCE_SECRET</Code> changes, <Code>set_tee_account</Code> must be called immediately or all settlements break.
            </P>

            <H3>PositionMeta (on-chain storage)</H3>
            <Table
              head={['Field', 'Visibility']}
              rows={[
                [<><Code>status</Code> (Open, Matched, Closed, Cancelled, Liquidated)</>, 'Public'],
                [<><Code>margin_mode</Code> (Isolated / Cross)</>, 'Public'],
                [<><Code>asset_id</Code></>, 'Public'],
                [<><Code>portfolio_key</Code> (Poseidon2 hash)</>, 'Commitment — secret hidden'],
                [<><Code>sealed_params</Code> (AEAD-encrypted order details)</>, 'Encrypted — TEE only'],
                [<><Code>settlement_commitment</Code> (SHA256)</>, 'Commitment — values hidden'],
                [<><Code>liquidation_recipient_note</Code></>, 'Commitment'],
                [<><Code>partial_liq_done</Code></>, 'Public flag'],
              ]}
            />
          </section>

          {/* Troubleshooting */}
          <section id="faq">
            <H2 id="faq">Troubleshooting</H2>

            <H3>settle_position auth failure</H3>
            <P>
              The contract's stored TEE account doesn't match the signing key. Run <Code>set-tee-account</Code> via the e2e tool to update it. This must be done after every contract upgrade.
            </P>

            <H3>Order book shows 0 quotes (market maker pool drained)</H3>
            <P>
              Keepers can't reach the TEE (port 9720) or fast-init is queued. Restart the keepers container after confirming TEE is up. The pool regenerates on the next tick.
            </P>

            <H3>Order book prices far from market</H3>
            <P>
              Pyth fetch is failing — market maker falls back to hardcoded <Code>base_price</Code>. Check Hermes API connectivity. Prices recover on next tick (60s).
            </P>

            <H3>Frontend shows positions but Orders tab empty</H3>
            <P>
              Limited orders are filtered by <Code>POSITION_NOT_FOUND</Code> (the string <Code>'not_found'</Code>) — distinct from <Code>null</Code>. Confirm the import is present. Pending limit orders are never on-chain until filled.
            </P>

            <H3>TEE container not starting</H3>
            <P>
              Check logs with <Code>sudo docker logs tee-match --tail=50</Code>. Common causes: ZK proving keys missing from <Code>/var/lib/tee-keys/</Code>, or sled DB locked from a prior unclean shutdown (<Code>fuser -k /var/lib/tee-keys/tee-db</Code>).
            </P>

            <H3>ZK proof generation is slow (~9s)</H3>
            <P>
              This is expected. Groth16 proving over BN254 with Poseidon2 takes ~9s per proof. Market orders require two proofs (commitment + note-spend), so ~18s from init to relay queued. The batch relay adds up to 10s more before on-chain confirmation.
            </P>
          </section>

          {/* Architecture */}
          <section className="mt-14">
            <H2 id="arch">Architecture at a Glance</H2>
            <Pre>{`Browser (Freighter Wallet)
      │
      │  1. tee.init()         → Groth16 OrderCommitment proof
      │  2. tee.noteProof()    → Groth16 NoteSpend proof
      │  3. deposit_note TX    → shields collateral on-chain (user-signed)
      │  4. tee.relay()        → queues relay (10s batch, shuffled)
      │
      ▼
TEE: tee-match  (GCP Confidential Space / AMD SEV-SNP)
      │
      │  CLOB engine matches orders
      │  Liquidator thread scans positions → settles under-collateralized
      │  Batch relay: place_order + open_position_from_note
      │  Match settlement: settle_position ×2
      │  User address never appears in relay TXs
      │
      ▼
Stellar Testnet  (Soroban)
      │
      │  verify_groth16(proof, public_inputs, vk)
      │    → BN254 multi-scalar multiplication
      │    → BN254 pairing check
      │
      ├── perp-engine       positions (committed), notes (committed), insurance fund
      ├── orderbook         order commitments, nullifiers
      └── collateral-token  USDC (mint / burn / transfer)
      
Keepers VM
      │  Market maker: 32 levels × 7 markets = 448 quotes
      │  Pyth Hermes → live prices → quote grid refresh (60s tick)`}</Pre>
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
