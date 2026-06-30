import { useEffect } from 'react'
import {
  IconCircleCheckFilled,
  IconInfoCircleFilled,
  IconAlertTriangleFilled,
  IconAlertCircleFilled,
  IconHourglass,
  IconX,
} from '@tabler/icons-react'
import { motion } from 'framer-motion'
import type { Toast } from './toast-context'

interface ToastCardProps {
  toast: Toast
  onClose: (id: string) => void
}

export function ToastCard({ toast, onClose }: ToastCardProps) {
  useEffect(() => {
    if (toast.duration === null || toast.duration === undefined) return
    const timer = setTimeout(() => {
      onClose(toast.id)
    }, toast.duration)
    return () => clearTimeout(timer)
  }, [toast.id, toast.duration, onClose])

  // Get matching status icon and colors
  const getStatusDetails = () => {
    switch (toast.type) {
      case 'success':
        return {
          icon: <IconCircleCheckFilled size={18} className="text-[#00e676]" />,
          titleColor: 'text-[#00e676]',
        }
      case 'info':
        return {
          icon: <IconInfoCircleFilled size={18} className="text-[#2196f3]" />,
          titleColor: 'text-[#2196f3]',
        }
      case 'warning':
        return {
          icon: <IconAlertTriangleFilled size={18} className="text-[#ff9800]" />,
          titleColor: 'text-[#ff9800]',
        }
      case 'error':
        return {
          icon: <IconAlertCircleFilled size={18} className="text-[#ff5252]" />,
          titleColor: 'text-[#ff5252]',
        }
      case 'progress':
        return {
          icon: <IconHourglass size={18} className="text-[#ffca28] animate-pulse" />,
          titleColor: 'text-[#ffca28]',
        }
    }
  }

  const { icon, titleColor } = getStatusDetails()

  return (
    <motion.div
      layout
      initial={{ opacity: 0, x: 80, scale: 0.95 }}
      animate={{ opacity: 1, x: 0, scale: 1 }}
      exit={{ opacity: 0, x: 40, scale: 0.95, transition: { duration: 0.18 } }}
      transition={{ type: 'spring', stiffness: 350, damping: 28 }}
      className="relative w-[360px] bg-surface-primary/95 border border-border-subtle rounded-[10px] p-4 shadow-2xl flex flex-col gap-2.5 font-mono select-none pointer-events-auto"
      style={{ backdropFilter: 'blur(12px)', WebkitBackdropFilter: 'blur(12px)' }}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-start gap-2.5 flex-1 min-w-0">
          <span className="shrink-0 mt-[2px]">{icon}</span>
          <div className="flex-1 min-w-0">
            <h4 className={`text-[13px] font-bold tracking-tight leading-tight ${titleColor}`}>
              {toast.title}
            </h4>
            {toast.description && (
              <p className="mt-1.5 text-[11.5px] text-text-tertiary leading-[15px] font-medium">
                {toast.description}
              </p>
            )}
          </div>
        </div>
        <button
          onClick={() => onClose(toast.id)}
          className="text-text-quaternary hover:text-text-primary transition-colors shrink-0 mt-[2px] cursor-pointer"
        >
          <IconX size={14} />
        </button>
      </div>

      {toast.type === 'progress' && (
        <div className="flex flex-col gap-2 mt-1">
          {/* Progress bar track */}
          <div className="w-full h-1 bg-surface-hover rounded-full overflow-hidden">
            <div
              className="h-full bg-[#0b9981] rounded-full transition-all duration-300"
              style={{ width: `${toast.progress ?? 0}%` }}
            />
          </div>
          <div className="flex items-center justify-between text-[11px] text-text-tertiary font-medium">
            <span className="flex items-center gap-1.5">
              Progress
              <span className="text-text-primary tabular-nums">
                {toast.progress !== undefined ? `${toast.progress}%` : '-'}
              </span>
            </span>
            {toast.action && (
              <button
                onClick={() => {
                  toast.action?.onClick()
                  onClose(toast.id)
                }}
                className="px-2.5 py-0.5 text-[10px] font-bold text-text-primary bg-transparent border border-border-subtle rounded-[4px] hover:bg-surface-hover transition-all uppercase tracking-wider cursor-pointer"
              >
                {toast.action.label}
              </button>
            )}
          </div>
        </div>
      )}

      {toast.type !== 'progress' && toast.action && (
        <div className="flex justify-end mt-1">
          <button
            onClick={() => {
              toast.action?.onClick()
              onClose(toast.id)
            }}
            className="px-2.5 py-0.5 text-[10px] font-bold text-text-primary bg-transparent border border-border-subtle rounded-[4px] hover:bg-surface-hover transition-all uppercase tracking-wider cursor-pointer"
          >
            {toast.action.label}
          </button>
        </div>
      )}
    </motion.div>
  )
}
