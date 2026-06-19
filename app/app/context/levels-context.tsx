import { createContext, useContext, useState } from 'react'

interface LevelsState {
  tp: number | null
  sl: number | null
  entry: number | null
  setTp: (v: number | null) => void
  setSl: (v: number | null) => void
  setEntry: (v: number | null) => void
  clearAll: () => void
}

const LevelsContext = createContext<LevelsState | undefined>(undefined)

export function LevelsProvider({ children }: { children: React.ReactNode }) {
  const [tp, setTp] = useState<number | null>(null)
  const [sl, setSl] = useState<number | null>(null)
  const [entry, setEntry] = useState<number | null>(null)

  return (
    <LevelsContext.Provider
      value={{
        tp,
        sl,
        entry,
        setTp,
        setSl,
        setEntry,
        clearAll: () => {
          setTp(null)
          setSl(null)
          setEntry(null)
        },
      }}
    >
      {children}
    </LevelsContext.Provider>
  )
}

export function useLevels() {
  const ctx = useContext(LevelsContext)
  if (!ctx) throw new Error('useLevels must be inside LevelsProvider')
  return ctx
}
