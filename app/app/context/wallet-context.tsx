import { createContext, useCallback, useContext, useEffect, useState } from 'react'
import {
  isConnected,
  getAddress,
  signTransaction,
  setAllowed,
} from '@stellar/freighter-api'
import { getBalance } from '../lib/contracts'

interface WalletState {
  connected: boolean
  publicKey: string | null
  balance: bigint  // in stroops (7 decimals)
  connecting: boolean
  connect: () => Promise<void>
  disconnect: () => void
  sign: (xdr: string) => Promise<string>
  refreshBalance: () => Promise<void>
}

const WalletContext = createContext<WalletState | undefined>(undefined)

export function WalletProvider({ children }: { children: React.ReactNode }) {
  const [connected, setConnected] = useState(false)
  const [publicKey, setPublicKey] = useState<string | null>(null)
  const [balance, setBalance] = useState(0n)
  const [connecting, setConnecting] = useState(false)

  const refreshBalance = useCallback(async () => {
    if (!publicKey) return
    try {
      const bal = await getBalance(publicKey)
      setBalance(bal)
    } catch {
      // silently ignore RPC errors
    }
  }, [publicKey])

  // Auto-reconnect if Freighter already authorized
  useEffect(() => {
    ;(async () => {
      try {
        const { isConnected: ok } = await isConnected()
        if (!ok) return
        const { address, error } = await getAddress()
        if (error || !address) return
        setPublicKey(address)
        setConnected(true)
      } catch {
        // Freighter not installed
      }
    })()
  }, [])

  useEffect(() => {
    if (connected && publicKey) refreshBalance()
  }, [connected, publicKey, refreshBalance])

  const connect = useCallback(async () => {
    setConnecting(true)
    try {
      const { isConnected: ok } = await isConnected()
      if (!ok) {
        window.open('https://freighter.app', '_blank')
        return
      }
      await setAllowed()
      const { address, error } = await getAddress()
      if (error || !address) throw new Error(error ?? 'No address returned')
      setPublicKey(address)
      setConnected(true)
    } catch (e) {
      console.error('Wallet connect failed:', e)
    } finally {
      setConnecting(false)
    }
  }, [])

  const disconnect = useCallback(() => {
    setConnected(false)
    setPublicKey(null)
    setBalance(0n)
  }, [])

  const sign = useCallback(
    async (xdr: string): Promise<string> => {
      if (!publicKey) throw new Error('Wallet not connected')
      const result = await signTransaction(xdr, {
        networkPassphrase: import.meta.env.VITE_NETWORK_PASSPHRASE ?? 'Test SDF Network ; September 2015',
        address: publicKey,
      })
      if ('error' in result) throw new Error(result.error)
      return result.signedTxXdr
    },
    [publicKey],
  )

  return (
    <WalletContext.Provider
      value={{ connected, publicKey, balance, connecting, connect, disconnect, sign, refreshBalance }}
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
