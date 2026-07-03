import { createContext, useContext, type ReactNode } from 'react'

interface NavCtx {
  openPortfolio: () => void
}

const NavContext = createContext<NavCtx>({ openPortfolio: () => {} })

export function NavProvider({ onActive, children }: { onActive: (label: string) => void; children: ReactNode }) {
  return (
    <NavContext.Provider value={{ openPortfolio: () => onActive('Portfolio') }}>
      {children}
    </NavContext.Provider>
  )
}

export function useNav() {
  return useContext(NavContext)
}
