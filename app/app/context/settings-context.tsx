import { createContext, useContext, useEffect, useMemo, useState } from 'react'

export type OrderPrivacy = 'public' | 'private'

const STORAGE_KEY = 'cerida-settings'

interface StoredSettings {
  orderPrivacy: OrderPrivacy
}

const DEFAULTS: StoredSettings = {
  orderPrivacy: 'public',
}

function loadStored(): StoredSettings {
  if (typeof window === 'undefined') return DEFAULTS
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return DEFAULTS
    const parsed = JSON.parse(raw)
    return {
      orderPrivacy: parsed.orderPrivacy === 'private' ? 'private' : 'public',
    }
  } catch {
    return DEFAULTS
  }
}

interface SettingsContextValue {
  orderPrivacy: OrderPrivacy
  setOrderPrivacy: (value: OrderPrivacy) => void
}

const SettingsContext = createContext<SettingsContextValue | undefined>(undefined)

export function SettingsProvider({ children }: { children: React.ReactNode }) {
  const [orderPrivacy, setOrderPrivacy] = useState<OrderPrivacy>(() => loadStored().orderPrivacy)

  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ orderPrivacy }))
  }, [orderPrivacy])

  const value = useMemo(() => ({ orderPrivacy, setOrderPrivacy }), [orderPrivacy])

  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>
}

export function useSettings() {
  const ctx = useContext(SettingsContext)
  if (!ctx) throw new Error('useSettings must be inside SettingsProvider')
  return ctx
}
