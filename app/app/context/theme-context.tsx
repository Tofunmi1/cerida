import { createContext, useContext, useEffect, useMemo, useState } from 'react'

export type ThemeMode =
  | 'gruvbox'
  | 'light'
  | 'dark'
  | 'nord'
  | 'solarized'
  | 'tokyo'
  | 'dracula'
  | 'matrix'
  | 'sepia'
  | 'slate'
  | 'contrast'

export const THEMES: { id: ThemeMode; label: string }[] = [
  { id: 'gruvbox', label: 'Gruvbox' },
  { id: 'light', label: 'Light' },
  { id: 'dark', label: 'Dark' },
  { id: 'nord', label: 'Nord' },
  { id: 'solarized', label: 'Solarized' },
  { id: 'tokyo', label: 'Tokyo' },
  { id: 'dracula', label: 'Dracula' },
  { id: 'matrix', label: 'Matrix' },
  { id: 'sepia', label: 'Sepia' },
  { id: 'slate', label: 'Slate' },
  { id: 'contrast', label: 'Contrast' },
]

interface ThemeContextValue {
  theme: ThemeMode
  setTheme: (theme: ThemeMode) => void
}

const ThemeContext = createContext<ThemeContextValue | undefined>(undefined)

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setTheme] = useState<ThemeMode>(() => {
    if (typeof window === 'undefined') return 'dark'
    const stored = localStorage.getItem('cerida-theme') as ThemeMode | null
    return stored && THEMES.some((item) => item.id === stored) ? stored : 'dark'
  })

  useEffect(() => {
    document.documentElement.dataset.theme = theme
    localStorage.setItem('cerida-theme', theme)
  }, [theme])

  const value = useMemo(
    () => ({
      theme,
      setTheme,
    }),
    [theme],
  )

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
}

export function useTheme() {
  const context = useContext(ThemeContext)
  if (!context) throw new Error('useTheme must be used inside ThemeProvider')
  return context
}
