import { useState, useEffect } from 'react'
import { IconX, IconWallet, IconCoin, IconCheck, IconArrowRight } from '@tabler/icons-react'
import { useWallet } from '../../context/wallet-context'
import { toast } from '../toast/toast-context'
import { mintUsdcFromIssuer, getUsdcBalance, buildTrustUsdcTx, submitAndWait } from '../../lib/contracts'

const STORAGE_KEY = 'cerida-onboarded'
const MINT_AMOUNT = 10_000_000_000n
const PRICE_SCALE = 1e7

type Step = 'welcome' | 'connect' | 'mint' | 'done'

export default function OnboardingModal({ onClose }: { onClose: () => void }) {
  const { connected, connecting, publicKey, sign, connect, refreshBalance } = useWallet()
  const [step, setStep] = useState<Step>('welcome')
  const [minting, setMinting] = useState(false)
  const [balance, setBalance] = useState<bigint>(0n)

  const alreadyOnboarded = localStorage.getItem(STORAGE_KEY)

  useEffect(() => {
    if (alreadyOnboarded) onClose()
  }, [alreadyOnboarded, onClose])

  useEffect(() => {
    if (connected && step === 'connect') setStep('mint')
  }, [connected, step])

  useEffect(() => {
    if (connected && publicKey && step === 'mint') {
      getUsdcBalance(publicKey).then((b: bigint | null) => { if (b !== null) setBalance(b) })
    }
  }, [connected, publicKey, step])

  const handleMint = async () => {
    if (!publicKey) return
    setMinting(true)
    try {
      const trustTx = await buildTrustUsdcTx(publicKey)
      const signedTrust = await sign(trustTx.toXDR())
      await submitAndWait(signedTrust)
      await mintUsdcFromIssuer(publicKey, MINT_AMOUNT)
      const newBal = await getUsdcBalance(publicKey)
      setBalance(newBal ?? 0n)
      refreshBalance()
      setStep('done')
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      console.error('mint flow error:', e)
      toast.error('Mint failed', msg.slice(0, 120))
    } finally {
      setMinting(false)
    }
  }

  const finish = () => {
    localStorage.setItem(STORAGE_KEY, 'true')
    onClose()
  }

  return (
    <div
      className="fixed inset-0 z-[80] flex items-center justify-center bg-black/35 p-6 backdrop-blur-sm"
      onMouseDown={(e) => { if (e.currentTarget === e.target && !minting) onClose() }}
    >
      <div className="flex h-[min(520px,86vh)] w-[min(520px,94vw)] flex-col overflow-hidden rounded-[14px] border border-border-subtle bg-surface-primary shadow-2xl">
        <div className="flex shrink-0 items-center gap-3 border-b border-border-subtle px-6 py-4">
          <div>
            <h1 className="text-[15px] font-semibold uppercase tracking-widest text-text-primary">
              {step === 'welcome' && 'Welcome to Cerida'}
              {step === 'connect' && 'Connect Wallet'}
              {step === 'mint' && 'Get Test USDC'}
              {step === 'done' && "You're Ready"}
            </h1>
            <p className="mt-0.5 text-[12px] text-text-quaternary">
              {step === 'welcome' && 'Private perpetuals on Stellar'}
              {step === 'connect' && 'Connect a Stellar wallet to begin'}
              {step === 'mint' && 'Mint testnet collateral'}
              {step === 'done' && "Start trading — you're all set"}
            </p>
          </div>
          {!minting && (
            <button
              onClick={onClose}
              className="ml-auto grid h-9 w-9 place-items-center rounded-[8px] text-text-tertiary hover:bg-surface-hover hover:text-text-primary"
            >
              <IconX size={18} stroke={2} />
            </button>
          )}
        </div>

        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-auto bg-page px-6 py-5">
          {step === 'welcome' && <WelcomeStep onNext={() => setStep('connect')} />}
          {step === 'connect' && <ConnectStep connecting={connecting} onConnect={connect} />}
          {step === 'mint' && (
            <MintStep
              publicKey={publicKey}
              balance={balance}
              minting={minting}
              onMint={handleMint}
              onSkip={() => setStep('done')}
            />
          )}
          {step === 'done' && <DoneStep balance={balance} onFinish={finish} />}
        </div>

        <div className="flex shrink-0 items-center gap-2 border-t border-border-subtle px-6 py-3">
          {(['welcome', 'connect', 'mint', 'done'] as Step[]).map((s, i) => {
            const stepOrder = ['welcome', 'connect', 'mint', 'done'].indexOf(step)
            const isActive = s === step
            const isPast = i < stepOrder
            return (
              <div
                key={s}
                className={`h-1.5 flex-1 rounded-full transition-colors ${
                  isActive ? 'bg-brand-violet' : isPast ? 'bg-brand-violet/40' : 'bg-border-subtle'
                }`}
              />
            )
          })}
        </div>
      </div>
    </div>
  )
}

function WelcomeStep({ onNext }: { onNext: () => void }) {
  return (
    <div className="flex flex-col gap-5">
      <p className="text-[13px] leading-relaxed text-text-secondary">
        <strong className="text-text-primary">Cerida</strong> is a privacy-first perpetual
        futures exchange built on Stellar. Trade BTC, Gold, equities, and more with
        up to <strong className="text-text-primary">50× leverage</strong> — all verified
        on-chain with zero-knowledge proofs.
      </p>

      <div className="flex flex-col gap-2">
        {[
          { title: 'ZK-Verified', body: 'Every order matched off-chain, verified on-chain with Groth16' },
          { title: 'Shielded Notes', body: 'Your positions and collateral are privacy-preserving by default' },
          { title: 'Non-Custodial', body: 'Collateral lives in smart contracts you audit. Connect and trade.' },
        ].map((f) => (
          <div
            key={f.title}
            className="rounded-[10px] border border-border-subtle bg-surface-primary p-4"
          >
            <div className="text-[12px] font-semibold text-text-primary">{f.title}</div>
            <div className="mt-0.5 text-[12px] leading-relaxed text-text-tertiary">{f.body}</div>
          </div>
        ))}
      </div>

      <button
        onClick={onNext}
        className="flex items-center justify-center gap-2 rounded-[8px] bg-brand-violet px-4 py-2.5 text-[13px] font-semibold text-white transition-opacity hover:opacity-90"
      >
        Get Started
        <IconArrowRight size={16} stroke={2.5} />
      </button>
    </div>
  )
}

function ConnectStep({ connecting, onConnect }: { connecting: boolean; onConnect: () => void }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-6">
      <div className="grid h-16 w-16 place-items-center rounded-[14px] border border-border-subtle bg-surface-card">
        <IconWallet size={32} stroke={1.5} className="text-brand-violet" />
      </div>
      <div className="text-center">
        <p className="text-[13px] text-text-secondary">
          Connect a Stellar wallet to start trading.
        </p>
        <p className="mt-1 text-[12px] text-text-quaternary">
          Supports Freighter, xBull, Albedo, Rabet, LOBSTR, and Hana.
        </p>
      </div>
      <button
        onClick={onConnect}
        disabled={connecting}
        className="flex items-center justify-center gap-2 rounded-[8px] bg-brand-violet px-6 py-2.5 text-[13px] font-semibold text-white transition-opacity hover:opacity-90 disabled:opacity-50"
      >
        {connecting ? 'Connecting…' : 'Connect Wallet'}
      </button>
    </div>
  )
}

function MintStep({
  publicKey,
  balance,
  minting,
  onMint,
  onSkip,
}: {
  publicKey: string | null
  balance: bigint
  minting: boolean
  onMint: () => void
  onSkip: () => void
}) {
  const hasBalance = balance > 0n
  const displayBal = (Number(balance) / PRICE_SCALE).toLocaleString(undefined, { maximumFractionDigits: 2 })

  return (
    <div className="flex flex-1 flex-col gap-5">
      <div className="grid h-14 w-14 place-items-center rounded-[12px] border border-border-subtle bg-surface-card mx-auto">
        <IconCoin size={28} stroke={1.5} className="text-warning" />
      </div>

      <p className="text-center text-[13px] text-text-secondary">
        {hasBalance
          ? `You already have ${displayBal} USDC`
          : `Mint 1,000 testnet USDC to use as trading collateral.`}
      </p>

      {publicKey && (
        <div className="rounded-[10px] border border-border-subtle bg-surface-primary p-3">
          <div className="text-[10px] uppercase tracking-widest text-text-quaternary">Connected Account</div>
          <div className="mt-0.5 text-[12px] text-text-secondary" style={{ fontFamily: 'var(--font-mono)' }}>
            {publicKey.slice(0, 8)}…{publicKey.slice(-8)}
          </div>
        </div>
      )}

      {hasBalance ? (
        <button
          onClick={onSkip}
          className="flex items-center justify-center gap-2 rounded-[8px] bg-brand-violet px-4 py-2.5 text-[13px] font-semibold text-white transition-opacity hover:opacity-90"
        >
          Continue
          <IconArrowRight size={16} stroke={2.5} />
        </button>
      ) : (
        <button
          onClick={onMint}
          disabled={minting}
          className="flex items-center justify-center gap-2 rounded-[8px] bg-brand-violet px-4 py-2.5 text-[13px] font-semibold text-white transition-opacity hover:opacity-90 disabled:opacity-50"
        >
          {minting ? 'Minting…' : `Mint 1,000 USDC`}
        </button>
      )}
    </div>
  )
}

function DoneStep({ balance, onFinish }: { balance: bigint; onFinish: () => void }) {
  const displayBal = (Number(balance) / PRICE_SCALE).toLocaleString(undefined, { maximumFractionDigits: 2 })

  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-6">
      <div className="grid h-16 w-16 place-items-center rounded-[14px] border border-bullish-green/30 bg-bullish-green/10">
        <IconCheck size={32} stroke={2} className="text-bullish-green" />
      </div>
      <div className="text-center">
        <p className="text-[14px] font-semibold text-text-primary">You're all set</p>
        <p className="mt-1 text-[13px] text-text-secondary">
          {balance > 0n ? `${displayBal} USDC ready to trade` : 'Start trading with shielded notes'}
        </p>
      </div>
      <div className="rounded-[10px] border border-border-subtle bg-surface-primary p-4 text-[12px] leading-relaxed text-text-tertiary">
        When you place an order, Cerida generates a zero-knowledge proof inside a
        trusted execution environment (TEE). Your position is linked to a shielded
        note — not your wallet address.
      </div>
      <button
        onClick={onFinish}
        className="flex items-center justify-center gap-2 rounded-[8px] bg-brand-violet px-6 py-2.5 text-[13px] font-semibold text-white transition-opacity hover:opacity-90"
      >
        Start Trading
        <IconArrowRight size={16} stroke={2.5} />
      </button>
    </div>
  )
}