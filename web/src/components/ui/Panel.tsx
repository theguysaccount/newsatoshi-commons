import type { ReactNode } from 'react';

interface PanelProps {
  title?: string;
  subtitle?: string;
  icon?: ReactNode;
  badge?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
  bodyClassName?: string;
  accent?: 'none' | 'emerald' | 'amber' | 'crimson' | 'sky';
}

const ACCENT_RING: Record<NonNullable<PanelProps['accent']>, string> = {
  none: '',
  emerald: 'border-emerald-500/30',
  amber: 'border-amber-500/30',
  crimson: 'border-red-500/40',
  sky: 'border-sky-500/30',
};

export function Panel({
  title,
  subtitle,
  icon,
  badge,
  action,
  children,
  className = '',
  bodyClassName = 'p-5',
  accent = 'none',
}: PanelProps) {
  return (
    <section className={`glass-panel rounded-xl ${ACCENT_RING[accent]} ${className}`}>
      {(title || badge || action) && (
        <header className="flex flex-wrap items-center justify-between gap-3 border-b border-slate-800/80 px-5 py-3.5">
          <div className="flex min-w-0 items-center gap-2.5">
            {icon && <div className="shrink-0 text-emerald-400">{icon}</div>}
            <div className="min-w-0">
              {title && (
                <h2 className="truncate text-sm font-semibold tracking-tight text-slate-100">
                  {title}
                </h2>
              )}
              {subtitle && (
                <p className="truncate text-xs text-slate-400">{subtitle}</p>
              )}
            </div>
          </div>
          <div className="flex items-center gap-2">
            {badge}
            {action}
          </div>
        </header>
      )}
      <div className={bodyClassName}>{children}</div>
    </section>
  );
}
