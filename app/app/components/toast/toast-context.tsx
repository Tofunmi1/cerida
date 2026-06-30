import { createContext, useContext, useState, useEffect } from 'react'

export interface ToastAction {
  label: string
  onClick: () => void
}

export interface Toast {
  id: string
  type: 'success' | 'info' | 'warning' | 'error' | 'progress'
  title: string
  description?: string
  progress?: number // 0 to 100, or undefined for indeterminate/none
  action?: ToastAction
  duration?: number | null // duration in ms, or null/Infinity for persistent
}

type ToastListener = (toasts: Toast[]) => void
let listeners: ToastListener[] = []
let currentToasts: Toast[] = []

function emit() {
  listeners.forEach((l) => l([...currentToasts]))
}

// Global programmatic trigger
export const toast = {
  success: (
    title: string,
    description?: string,
    options?: Omit<Partial<Toast>, 'id' | 'type' | 'title' | 'description'>
  ) => {
    const id = Math.random().toString(36).substring(2, 9)
    const item: Toast = {
      id,
      type: 'success',
      title,
      description,
      duration: 5000,
      ...options,
    }
    currentToasts = [...currentToasts, item]
    emit()
    return id
  },

  info: (
    title: string,
    description?: string,
    options?: Omit<Partial<Toast>, 'id' | 'type' | 'title' | 'description'>
  ) => {
    const id = Math.random().toString(36).substring(2, 9)
    const item: Toast = {
      id,
      type: 'info',
      title,
      description,
      duration: 5000,
      ...options,
    }
    currentToasts = [...currentToasts, item]
    emit()
    return id
  },

  warning: (
    title: string,
    description?: string,
    options?: Omit<Partial<Toast>, 'id' | 'type' | 'title' | 'description'>
  ) => {
    const id = Math.random().toString(36).substring(2, 9)
    const item: Toast = {
      id,
      type: 'warning',
      title,
      description,
      duration: 6000,
      ...options,
    }
    currentToasts = [...currentToasts, item]
    emit()
    return id
  },

  error: (
    title: string,
    description?: string,
    options?: Omit<Partial<Toast>, 'id' | 'type' | 'title' | 'description'>
  ) => {
    const id = Math.random().toString(36).substring(2, 9)
    const item: Toast = {
      id,
      type: 'error',
      title,
      description,
      duration: null, // Error toasts are sticky by default
      ...options,
    }
    currentToasts = [...currentToasts, item]
    emit()
    return id
  },

  progress: (
    title: string,
    progress: number,
    description?: string,
    options?: Omit<Partial<Toast>, 'id' | 'type' | 'title' | 'description' | 'progress'>
  ) => {
    const id = Math.random().toString(36).substring(2, 9)
    const item: Toast = {
      id,
      type: 'progress',
      title,
      description,
      progress,
      duration: null, // Progress toasts are persistent by default
      ...options,
    }
    currentToasts = [...currentToasts, item]
    emit()
    return id
  },

  update: (id: string, updates: Partial<Omit<Toast, 'id'>>) => {
    currentToasts = currentToasts.map((t) => (t.id === id ? { ...t, ...updates } : t))
    emit()
  },

  dismiss: (id: string) => {
    currentToasts = currentToasts.filter((t) => t.id !== id)
    emit()
  },
}

interface ToastContextProps {
  toasts: Toast[]
  dismiss: (id: string) => void
}

const ToastContext = createContext<ToastContextProps | undefined>(undefined)

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([])

  useEffect(() => {
    const listener = (newToasts: Toast[]) => {
      setToasts(newToasts)
    }
    listeners.push(listener)
    setToasts([...currentToasts])

    return () => {
      listeners = listeners.filter((l) => l !== listener)
    }
  }, [])

  return (
    <ToastContext.Provider value={{ toasts, dismiss: toast.dismiss }}>
      {children}
    </ToastContext.Provider>
  )
}

export function useToast() {
  const context = useContext(ToastContext)
  if (!context) {
    throw new Error('useToast must be used within a ToastProvider')
  }
  return context
}
