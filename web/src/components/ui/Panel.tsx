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
  emerald: 'border-emerald-500/20',
  amber: 'border-amber-500/20',
  crimson: 'border-red-500/30',
  sky: 'border-white/[0.12]',
};

export function Panel({
  title,
  subtitle,
  icon,
  badge,
  action,
  children,
  className = '',
  bodyClassName = 'p-5 sm:p-6',
  accent = 'none',
}: PanelProps) {
  return (
    <section className={`surface-card rounded-2xl ${ACCENT_RING[accent]} ${className}`}>
      {(title || badge || action) && (
        <header className="flex flex-wrap items-center justify-between gap-3 border-b border-white/[0.07] px-5 py-4 sm:px-6">
          <div className="flex min-w-0 items-center gap-2.5">
            {icon && <div className="shrink-0 text-zinc-400">{icon}</div>}
            <div className="min-w-0">
              {title && (
                <h2 className="truncate text-[14px] font-medium tracking-tight text-zinc-100">
                  {title}
                </h2>
              )}
              {subtitle && (
                <p className="mt-0.5 truncate text-xs text-zinc-500">{subtitle}</p>
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
