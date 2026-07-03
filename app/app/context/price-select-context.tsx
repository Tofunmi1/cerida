import { createContext, useContext, useRef, type ReactNode } from 'react'

interface PriceSelectCtx {
  subscribe: (fn: (price: number) => void) => () => void
  emit: (price: number) => void
}

const PriceSelectContext = createContext<PriceSelectCtx>({
  subscribe: () => () => {},
  emit: () => {},
})

export function PriceSelectProvider({ children }: { children: ReactNode }) {
  const listenersRef = useRef<Set<(price: number) => void>>(new Set())

  const subscribe = (fn: (price: number) => void) => {
    listenersRef.current.add(fn)
    return () => listenersRef.current.delete(fn)
  }

  const emit = (price: number) => {
    listenersRef.current.forEach((fn) => fn(price))
  }

  return (
    <PriceSelectContext.Provider value={{ subscribe, emit }}>
      {children}
    </PriceSelectContext.Provider>
  )
}

export function usePriceSelect() {
  return useContext(PriceSelectContext)
}
