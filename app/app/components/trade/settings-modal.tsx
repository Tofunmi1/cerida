import { IconEye, IconEyeOff, IconX } from '@tabler/icons-react'
import { useSettings, type OrderPrivacy, type StoredSettings } from '../../context/settings-context'

const PRIVACY_OPTIONS: {
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

const LEVERAGE_PRESETS = [1, 2, 5, 10, 20, 50]
const SLIPPAGE_PRESETS = [0.1, 0.25, 0.5, 1, 2]

export default function SettingsModal({ onClose }: { onClose: () => void }) {
  const { settings, updateSetting } = useSettings()

  return (
    <div
      className="fixed inset-0 z-[80] flex items-center justify-center bg-black/35 p-6 backdrop-blur-sm"
      onMouseDown={(event) => {
        if (event.currentTarget === event.target) onClose()
      }}
    >
      <div className="flex h-[min(640px,86vh)] w-[min(560px,94vw)] flex-col overflow-hidden rounded-[14px] border border-border-subtle bg-surface-primary shadow-2xl">
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

        <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-auto bg-page px-6 py-5">
          {/* Order type */}
          <section>
            <h2 className="text-[12px] font-semibold uppercase tracking-widest text-text-secondary">
              Default Order Type
            </h2>
            <div className="mt-2 flex gap-2">
              {(['market', 'limit'] as const).map((t) => (
                <button
                  key={t}
                  onClick={() => updateSetting('defaultOrderType', t)}
                  className={`flex-1 rounded-[10px] border px-4 py-2.5 text-[13px] font-semibold capitalize transition-colors ${
                    settings.defaultOrderType === t
                      ? 'border-brand-violet/50 bg-brand-violet/10 text-brand-violet'
                      : 'border-border-subtle bg-surface-primary text-text-secondary hover:bg-surface-hover'
                  }`}
                >
                  {t}
                </button>
              ))}
            </div>
          </section>

          {/* Default leverage */}
          <section>
            <h2 className="text-[12px] font-semibold uppercase tracking-widest text-text-secondary">
              Default Leverage
            </h2>
            <div className="mt-2 flex flex-wrap gap-2">
              {LEVERAGE_PRESETS.map((lev) => (
                <button
                  key={lev}
                  onClick={() => updateSetting('defaultLeverage', lev)}
                  className={`rounded-[8px] border px-3.5 py-1.5 text-[12px] font-semibold transition-colors ${
                    settings.defaultLeverage === lev
                      ? 'border-brand-violet/50 bg-brand-violet/10 text-brand-violet'
                      : 'border-border-subtle bg-surface-primary text-text-secondary hover:bg-surface-hover'
                  }`}
                >
                  {lev}x
                </button>
              ))}
            </div>
          </section>

          {/* Slippage tolerance */}
          <section>
            <h2 className="text-[12px] font-semibold uppercase tracking-widest text-text-secondary">
              Slippage Tolerance
            </h2>
            <div className="mt-2 flex flex-wrap gap-2">
              {SLIPPAGE_PRESETS.map((s) => (
                <button
                  key={s}
                  onClick={() => updateSetting('slippageTolerance', s)}
                  className={`rounded-[8px] border px-3.5 py-1.5 text-[12px] font-semibold transition-colors ${
                    settings.slippageTolerance === s
                      ? 'border-brand-violet/50 bg-brand-violet/10 text-brand-violet'
                      : 'border-border-subtle bg-surface-primary text-text-secondary hover:bg-surface-hover'
                  }`}
                >
                  {s}%
                </button>
              ))}
            </div>
          </section>

          {/* Order privacy */}
          <section>
            <h2 className="text-[12px] font-semibold uppercase tracking-widest text-text-secondary">
              Order Privacy
            </h2>
            <p className="mt-0.5 text-[12px] text-text-quaternary">
              Choose what new orders reveal by default.
            </p>
            <div className="mt-2 flex flex-col gap-2">
              {PRIVACY_OPTIONS.map((opt) => {
                const selected = settings.orderPrivacy === opt.id
                const disabled = opt.id === 'public'
                return (
                  <button
                    key={opt.id}
                    disabled={disabled}
                    tabIndex={disabled ? -1 : 0}
                    className={`flex flex-col gap-1.5 rounded-[10px] border px-4 py-3 text-left transition-colors ${
                      disabled
                        ? 'cursor-not-allowed border-border-subtle bg-surface-primary opacity-40 pointer-events-none'
                        : selected
                          ? 'border-brand-violet/50 bg-brand-violet/10'
                          : 'border-border-subtle bg-surface-primary hover:bg-surface-hover'
                    }`}
                    title={disabled ? 'Public orders are not available — using shielded pool' : opt.label}
                  >
                    <div className="flex items-center gap-2">
                      <span className={selected ? 'text-brand-violet' : 'text-text-tertiary'}>{opt.icon}</span>
                      <span className="text-[13px] font-semibold text-text-primary">{opt.label}</span>
                      {selected && (
                        <span className="ml-auto rounded-[4px] bg-brand-violet px-1.5 py-0.5 text-[10px] font-semibold text-white">
                          Default
                        </span>
                      )}
                      {disabled && !selected && (
                        <span className="ml-auto rounded-[4px] bg-warning/15 px-1.5 py-0.5 text-[10px] font-semibold text-warning">
                          Disabled
                        </span>
                      )}
                    </div>
                    <p className="text-[12px] text-text-secondary">{opt.description}</p>
                  </button>
                )
              })}
            </div>
          </section>
        </div>
      </div>
    </div>
  )
}
