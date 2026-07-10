import { createContext, useContext, useEffect, useMemo, useState } from 'react'

export type OrderPrivacy = 'public' | 'private'

const STORAGE_KEY = 'cerida-settings'

export interface StoredSettings {
  orderPrivacy: OrderPrivacy
  defaultLeverage: number
  slippageTolerance: number
  defaultOrderType: 'market' | 'limit'
}

const DEFAULTS: StoredSettings = {
  orderPrivacy: 'private',
  defaultLeverage: 10,
  slippageTolerance: 0.5,
  defaultOrderType: 'market',
}

function loadStored(): StoredSettings {
  if (typeof window === 'undefined') return DEFAULTS
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return DEFAULTS
    const parsed = JSON.parse(raw)
    return {
      orderPrivacy: parsed.orderPrivacy === 'private' ? 'private' : 'private',
      defaultLeverage: typeof parsed.defaultLeverage === 'number' ? parsed.defaultLeverage : DEFAULTS.defaultLeverage,
      slippageTolerance: typeof parsed.slippageTolerance === 'number' ? parsed.slippageTolerance : DEFAULTS.slippageTolerance,
      defaultOrderType: parsed.defaultOrderType === 'limit' ? 'limit' : 'market',
    }
  } catch {
    return DEFAULTS
  }
}

interface SettingsContextValue {
  settings: StoredSettings
  updateSetting: <K extends keyof StoredSettings>(key: K, value: StoredSettings[K]) => void
}

const SettingsContext = createContext<SettingsContextValue | undefined>(undefined)

export function SettingsProvider({ children }: { children: React.ReactNode }) {
  const [settings, setSettings] = useState<StoredSettings>(() => loadStored())

  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings))
  }, [settings])

  const updateSetting = <K extends keyof StoredSettings>(key: K, value: StoredSettings[K]) => {
    setSettings((prev) => ({ ...prev, [key]: value }))
  }

  const value = useMemo(() => ({ settings, updateSetting }), [settings])

  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>
}

export function useSettings() {
  const ctx = useContext(SettingsContext)
  if (!ctx) throw new Error('useSettings must be inside SettingsProvider')
  return ctx
}
