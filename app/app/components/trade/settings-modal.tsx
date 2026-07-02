import { IconEye, IconEyeOff, IconX } from '@tabler/icons-react'
import { useSettings, type OrderPrivacy } from '../../context/settings-context'

const OPTIONS: {
  id: OrderPrivacy
  label: string
  icon: React.ReactNode
  description: string
}[] = [
  {
    id: 'public',
    label: 'Public',
    icon: <IconEye size={18} stroke={1.8} />,
    description: 'Your address, collateral, and position size are visible on-chain.',
  },
  {
    id: 'private',
    label: 'Private (shielded)',
    icon: <IconEyeOff size={18} stroke={1.8} />,
    description: 'Collateral is drawn from a shielded note; your address is never attached to the position.',
  },
]

export default function SettingsModal({ onClose }: { onClose: () => void }) {
  const { orderPrivacy, setOrderPrivacy } = useSettings()

  return (
    <div
      className="fixed inset-0 z-[80] flex items-center justify-center bg-black/35 p-6 backdrop-blur-sm"
      onMouseDown={(event) => {
        if (event.currentTarget === event.target) onClose()
      }}
    >
      <div className="flex h-[min(560px,86vh)] w-[min(560px,94vw)] flex-col overflow-hidden rounded-[14px] border border-border-subtle bg-surface-primary shadow-2xl">
        <div className="flex shrink-0 items-center gap-3 border-b border-border-subtle px-6 py-4">
          <div>
            <h1 className="text-[15px] font-semibold uppercase tracking-widest text-text-primary">
              Settings
            </h1>
            <p className="mt-0.5 text-[12px] text-text-quaternary">Trading preferences</p>
          </div>
          <button
            onClick={onClose}
            className="ml-auto grid h-9 w-9 place-items-center rounded-[8px] text-text-tertiary hover:bg-surface-hover hover:text-text-primary"
          >
            <IconX size={18} stroke={2} />
          </button>
        </div>

        <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-auto bg-page px-6 py-5">
          <div className="mb-1">
            <h2 className="text-[12px] font-semibold uppercase tracking-widest text-text-secondary">
              Order Privacy
            </h2>
            <p className="mt-1 text-[12px] text-text-quaternary">
              Choose what new orders reveal by default. You can still pick a mode per-order later.
            </p>
          </div>

          {OPTIONS.map((opt) => {
            const selected = orderPrivacy === opt.id
            return (
              <button
                key={opt.id}
                onClick={() => setOrderPrivacy(opt.id)}
                className={`flex flex-col gap-1.5 rounded-[10px] border px-4 py-3 text-left transition-colors ${
                  selected
                    ? 'border-brand-violet/50 bg-brand-violet/10'
                    : 'border-border-subtle bg-surface-primary hover:bg-surface-hover'
                }`}
              >
                <div className="flex items-center gap-2">
                  <span className={selected ? 'text-brand-violet' : 'text-text-tertiary'}>{opt.icon}</span>
                  <span className="text-[13px] font-semibold text-text-primary">{opt.label}</span>
                  {selected && (
                    <span className="ml-auto rounded-[4px] bg-brand-violet px-1.5 py-0.5 text-[10px] font-semibold text-white">
                      Default
                    </span>
                  )}
                </div>
                <p className="text-[12px] text-text-secondary">{opt.description}</p>
              </button>
            )
          })}
        </div>
      </div>
    </div>
  )
}
