import { createContext, useCallback, useContext, useEffect, useState } from 'react'
import { StellarWalletsKit, KitEventType, Networks, type SwkAppTheme } from '@creit.tech/stellar-wallets-kit'
import { FreighterModule } from '@creit.tech/stellar-wallets-kit/modules/freighter'
import { xBullModule } from '@creit.tech/stellar-wallets-kit/modules/xbull'
import { AlbedoModule } from '@creit.tech/stellar-wallets-kit/modules/albedo'
import { RabetModule } from '@creit.tech/stellar-wallets-kit/modules/rabet'
import { LobstrModule } from '@creit.tech/stellar-wallets-kit/modules/lobstr'
import { HanaModule } from '@creit.tech/stellar-wallets-kit/modules/hana'
import { toast } from '../components/toast/toast-context'
import { useTheme } from './theme-context'

const EXPECTED_PASSPHRASE: string =
  import.meta.env.VITE_NETWORK_PASSPHRASE ?? Networks.TESTNET

const resolvedNetwork = (Object.values(Networks) as string[]).includes(EXPECTED_PASSPHRASE)
  ? (EXPECTED_PASSPHRASE as Networks)
  : Networks.TESTNET

// Registered once per page load. The kit persists the active wallet/address
// in localStorage itself, so a reload silently resumes the last session
// unless the user disconnected.
StellarWalletsKit.init({
  modules: [
    new FreighterModule(),
    new xBullModule(),
    new AlbedoModule(),
    new RabetModule(),
    new LobstrModule(),
    new HanaModule(),
  ],
  network: resolvedNetwork,
})

type Status = 'connecting' | 'disconnected' | 'connected'

interface WalletState {
  status: Status
  connected: boolean
  connecting: boolean
  publicKey: string | null
  balance: bigint // in stroops (7 decimals)
  balanceLoading: boolean
  network: string | null
  wrongNetwork: boolean
  connect: () => Promise<void>
  disconnect: () => void
  sign: (xdr: string) => Promise<string>
  refreshBalance: () => Promise<void>
}

const WalletContext = createContext<WalletState | undefined>(undefined)

function explainKitError(error: unknown): string {
  const msg =
    error && typeof error === 'object' && 'message' in error
      ? String((error as { message: unknown }).message)
      : error instanceof Error
        ? error.message
        : String(error)
  if (/declined|rejected|denied/i.test(msg)) return 'Request was rejected in your wallet.'
  return msg.slice(0, 140)
}

/** Mirror the app's active CSS theme onto the kit's wallet-picker/profile modal. */
function readAppTheme(): SwkAppTheme {
  const styles = getComputedStyle(document.documentElement)
  const v = (name: string, fallback: string) => styles.getPropertyValue(name).trim() || fallback

  return {
    background: v('--color-surface-primary', '#ffffff'),
    'background-secondary': v('--color-surface-card', '#f7f8fc'),
    'foreground-strong': v('--color-text-primary', '#111827'),
    foreground: v('--color-text-secondary', '#374151'),
    'foreground-secondary': v('--color-text-tertiary', '#6b7280'),
    primary: v('--color-brand-violet', '#807dfe'),
    'primary-foreground': '#ffffff',
    transparent: 'transparent',
    lighter: v('--color-surface-hover', '#eef1f7'),
    light: v('--color-border-default', '#d7dce8'),
    'light-gray': v('--color-border-subtle', 'rgba(17, 24, 39, 0.10)'),
    gray: v('--color-text-quaternary', '#9ca3af'),
    danger: v('--color-bearish-red', '#f23546'),
    border: v('--color-border-subtle', 'rgba(17, 24, 39, 0.10)'),
    shadow: '0 10px 15px -3px rgba(0, 0, 0, 0.25), 0 4px 6px -4px rgba(0, 0, 0, 0.2)',
    'border-radius': '12px',
    'font-family': v('--font-sans', 'ui-monospace, monospace'),
  }
}

export function WalletProvider({ children }: { children: React.ReactNode }) {
  const { theme } = useTheme()
  const [status, setStatus] = useState<Status>('disconnected')
  const [publicKey, setPublicKey] = useState<string | null>(null)
  const [network, setNetwork] = useState<string | null>(null)
  const [balance, setBalance] = useState(0n)
  const [balanceLoading, setBalanceLoading] = useState(false)

  const wrongNetwork = network !== null && network !== EXPECTED_PASSPHRASE

  // Keep the wallet-picker/profile modal visually in sync with the app's active theme.
  useEffect(() => {
    StellarWalletsKit.setTheme(readAppTheme())
  }, [theme])

  const refreshBalance = useCallback(async () => {
    // Note-based: balance tracked locally, not on-chain
    setBalance(0n)
    setBalanceLoading(false)
  }, [])

  // The kit fires STATE_UPDATED immediately with the resumed session (if any),
  // and again whenever the address or network changes — covers reconnects,
  // account switches inside the wallet, and disconnects in one subscription.
  useEffect(() => {
    const offState = StellarWalletsKit.on(KitEventType.STATE_UPDATED, (event) => {
      const { address, networkPassphrase } = event.payload
      if (!address) {
        setPublicKey(null)
        setNetwork(null)
        setBalance(0n)
        setStatus('disconnected')
        return
      }
      setPublicKey((prev) => {
        if (prev && prev !== address) {
          toast.info('Account switched', `Now using ${address.slice(0, 4)}…${address.slice(-4)}`)
        }
        return address
      })
      setNetwork(networkPassphrase)
      setStatus('connected')
    })

    const offDisconnect = StellarWalletsKit.on(KitEventType.DISCONNECT, () => {
      setPublicKey(null)
      setNetwork(null)
      setBalance(0n)
      setStatus('disconnected')
    })

    return () => {
      offState()
      offDisconnect()
    }
  }, [])

  useEffect(() => {
    if (status === 'connected' && publicKey) refreshBalance()
  }, [status, publicKey, refreshBalance])

  const connect = useCallback(async () => {
    setStatus('connecting')
    try {
      StellarWalletsKit.setTheme(readAppTheme())
      const { address } = await StellarWalletsKit.authModal()
      const { networkPassphrase } = await StellarWalletsKit.getNetwork()

      setPublicKey(address)
      setNetwork(networkPassphrase)
      setStatus('connected')

      if (networkPassphrase !== EXPECTED_PASSPHRASE) {
        toast.warning('Wrong network', 'Your wallet is on a different network. Switch to Testnet to trade.', {
          duration: 7000,
        })
      } else {
        toast.success('Wallet connected', `${address.slice(0, 4)}…${address.slice(-4)}`, { duration: 3000 })
      }
    } catch (e) {
      setStatus('disconnected')
      // User closing the picker modal isn't an error worth surfacing.
      const msg = explainKitError(e)
      if (!/closed the modal/i.test(msg)) {
        toast.error('Connection failed', msg)
      }
    }
  }, [])

  const disconnect = useCallback(() => {
    StellarWalletsKit.disconnect().catch(() => {
      // best-effort; local state is cleared via the DISCONNECT event regardless
    })
  }, [])

  const sign = useCallback(
    async (xdr: string): Promise<string> => {
      if (!publicKey) throw new Error('Wallet not connected')
      if (wrongNetwork) {
        toast.warning('Wrong network', 'Switch your wallet to Testnet before signing.')
        throw new Error('Wrong network selected in wallet')
      }
      try {
        const result = await StellarWalletsKit.signTransaction(xdr, {
          networkPassphrase: EXPECTED_PASSPHRASE,
          address: publicKey,
        })
        return result.signedTxXdr
      } catch (e) {
        throw new Error(explainKitError(e))
      }
    },
    [publicKey, wrongNetwork],
  )

  return (
    <WalletContext.Provider
      value={{
        status,
        connected: status === 'connected',
        connecting: status === 'connecting',
        publicKey,
        balance,
        balanceLoading,
        network,
        wrongNetwork,
        connect,
        disconnect,
        sign,
        refreshBalance,
      }}
    >
      {children}
    </WalletContext.Provider>
  )
}

export function useWallet() {
  const ctx = useContext(WalletContext)
  if (!ctx) throw new Error('useWallet must be inside WalletProvider')
  return ctx
}

/** Format a Soroban i128 balance (7 decimals) to a USD display string */
export function formatContractBalance(stroops: bigint): string {
  const dollars = Number(stroops) / 1e7
  return dollars.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
}
