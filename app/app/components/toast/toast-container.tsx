import { createPortal } from 'react-dom'
import { AnimatePresence } from 'framer-motion'
import { ToastCard } from './toast-card'
import { useToast } from './toast-context'
import { useState, useEffect } from 'react'

export function ToastContainer() {
  const { toasts, dismiss } = useToast()
  const [mounted, setMounted] = useState(false)

  useEffect(() => {
    setMounted(true)
  }, [])

  if (!mounted) return null

  return createPortal(
    <div className="fixed bottom-4 right-4 z-[99999] flex flex-col gap-3 max-w-[360px] w-full pointer-events-none items-end">
      <AnimatePresence mode="popLayout">
        {toasts.map((toast) => (
          <ToastCard key={toast.id} toast={toast} onClose={dismiss} />
        ))}
      </AnimatePresence>
    </div>,
    document.body
  )
}

export default ToastContainer
