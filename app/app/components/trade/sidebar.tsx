import { useEffect, useState } from 'react'
import { Link } from 'react-router'
import {
  IconActivity,
  IconBook,
  IconBriefcase,
  IconChevronLeft,
  IconChevronRight,
  IconLayoutDashboard,
  IconLock,
  IconSettings,
  IconShieldCheck,
} from '@tabler/icons-react'

const Divider = () => <div className="mx-3 my-1.5 h-px bg-border-subtle" />

const CollapseIcon = ({ collapsed }: { collapsed: boolean }) =>
  collapsed ? <IconChevronRight size={16} stroke={2} /> : <IconChevronLeft size={16} stroke={2} />

function NavButton({
  icon,
  label,
  active = false,
  collapsed,
  onClick,
  href,
}: {
  icon: React.ReactNode
  label: string
  active?: boolean
  collapsed: boolean
  onClick?: () => void
  href?: string
}) {
  const Tag = href ? 'a' : 'button'
  return (
    <Tag
      {...(href ? { href, target: '_blank', rel: 'noopener noreferrer' } : { onClick })}
      title={collapsed ? label : undefined}
      className={`mx-2 flex items-center gap-3 rounded-[6px] px-2 py-2.5 text-[13px] font-medium transition-colors no-underline ${
        active
          ? 'bg-surface-card text-text-primary'
          : 'text-text-secondary hover:bg-surface-card/60 hover:text-text-primary'
      } ${collapsed ? 'justify-center' : ''}`}
    >
      <span className="shrink-0">{icon}</span>
      {!collapsed && <span className="truncate">{label}</span>}
    </Tag>
  )
}

export default function Sidebar({
  active,
  onActive,
}: {
  active: string
  onActive: (label: string) => void
}) {
  const [collapsed, setCollapsed] = useState(true)

  useEffect(() => {
    const stored = localStorage.getItem('perp-sidebar-collapsed')
    if (stored !== null) setCollapsed(stored === 'true')
  }, [])

  const toggle = () =>
    setCollapsed((prev) => {
      const next = !prev
      localStorage.setItem('perp-sidebar-collapsed', String(next))
      return next
    })

  return (
    <aside
      className={`flex h-screen shrink-0 flex-col overflow-hidden border-r border-border-subtle bg-surface-primary transition-[width] duration-200 ease-in-out ${
        collapsed ? 'w-14' : 'w-[200px]'
      }`}
    >
      <div
        className={`flex h-[46px] shrink-0 items-center border-b border-border-subtle ${
          collapsed ? 'justify-center' : 'px-4'
        }`}
      >
        <Link
          to="/"
          className="flex min-w-0 items-center gap-2 no-underline"
          title={collapsed ? 'Cerida' : undefined}
        >
          <img src="/apple-touch-icon.png" alt="Cerida" className="h-7 w-7 shrink-0 rounded-[6px] object-cover" />
          {!collapsed && <span className="truncate text-[14px] font-semibold text-text-primary">cerida</span>}
        </Link>
      </div>

      <nav className="shrink-0 overflow-y-auto border-b border-border-subtle py-3">
        <NavButton
          icon={<IconLayoutDashboard size={18} stroke={1.75} />}
          label="Perps"
          active={active === 'Perps'}
          collapsed={collapsed}
          onClick={() => onActive('Perps')}
        />
        <NavButton
          icon={<IconActivity size={18} stroke={1.75} />}
          label="Markets"
          active={active === 'Markets'}
          collapsed={collapsed}
          onClick={() => onActive('Markets')}
        />

        <Divider />

        <NavButton
          icon={<IconBriefcase size={18} stroke={1.75} />}
          label="Portfolio"
          active={active === 'Portfolio'}
          collapsed={collapsed}
          onClick={() => onActive('Portfolio')}
        />
        <NavButton
          icon={<IconLock size={18} stroke={1.75} />}
          label="Pool"
          active={active === 'Pool'}
          collapsed={collapsed}
          onClick={() => onActive('Pool')}
        />
        <NavButton
          icon={<IconShieldCheck size={18} stroke={1.75} />}
          label="Risk"
          collapsed={collapsed}
          href="https://github.com/Tofunmi1/cerida/blob/main/audit/readme.md"
        />

        <Divider />

        <NavButton
          icon={<IconBook size={18} stroke={1.75} />}
          label="Docs"
          collapsed={collapsed}
          href="https://github.com/Tofunmi1/cerida"
        />
        <NavButton
          icon={<IconSettings size={18} stroke={1.75} />}
          label="Settings"
          active={active === 'Settings'}
          collapsed={collapsed}
          onClick={() => onActive('Settings')}
        />
      </nav>

      <div className="flex-1" />

      {!collapsed && (
        <div className="mx-3 mb-3 rounded-[8px] border border-border-subtle bg-surface-card p-3">
          <div className="text-[10px] uppercase tracking-widest text-text-quaternary">Margin mode</div>
          <div className="mt-1 flex items-center justify-between">
            <span className="text-[13px] font-semibold text-text-secondary">Cross</span>
            <span className="rounded-[4px] bg-brand-violet/15 px-1.5 py-0.5 text-[10px] font-semibold text-brand-violet">
              Live
            </span>
          </div>
        </div>
      )}

      <div className="flex shrink-0 flex-col gap-1 border-t border-border-subtle p-2">
        <button
          onClick={toggle}
          className={`flex w-full items-center gap-2 rounded-[6px] px-2 py-2 text-text-secondary transition-colors hover:bg-surface-card hover:text-text-primary ${
            collapsed ? 'justify-center' : ''
          }`}
        >
          <CollapseIcon collapsed={collapsed} />
          {!collapsed && <span className="text-[13px] font-medium">Collapse</span>}
        </button>
      </div>
    </aside>
  )
}
