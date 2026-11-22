import { useState } from 'react';
import { Activity, Inbox } from 'lucide-react';
import type { TelemetrySeriesPoint } from '../types';
import { Panel } from './ui/Panel';

interface CostCurveChartProps {
  points: TelemetrySeriesPoint[];
}

export function CostCurveChart({ points }: CostCurveChartProps) {
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);

  if (points.length === 0) {
    return (
      <Panel
        title="Verified Commons Ledger & Metered Energy History"
        subtitle="Plots only real persisted state transitions from the connected Rust Commons node"
        icon={<Activity className="h-4 w-4 text-emerald-400" />}
        accent="emerald"
      >
        <div className="flex flex-col items-center justify-center rounded-lg border border-dashed border-slate-800 bg-slate-950/50 px-6 py-12 text-center">
          <Inbox className="mb-3 h-7 w-7 text-slate-600" />
          <div className="text-sm font-medium text-slate-200">No verified ledger transitions yet</div>
          <p className="mt-1.5 max-w-lg text-xs leading-relaxed text-slate-400">
            This ledger starts at genuine <code className="font-mono text-slate-200">0 LOVE</code> and{' '}
            <code className="font-mono text-slate-200">0 kWh</code>. Record a Newsatoshi L1 funding
            deposit, stream signed meter readings via{' '}
            <code className="font-mono text-emerald-300">newsatoshi-hw-signer</code>, or settle a
            PayWord channel to plot real verified telemetry.
          </p>
        </div>
      </Panel>
    );
  }

  const activeIdx =
    hoveredIdx !== null && hoveredIdx < points.length ? hoveredIdx : points.length - 1;
  const active = points[activeIdx];

  const width = 880;
  const height = 300;
  const padL = 72;
  const padR = 68;
  const padT = 24;
  const plotBottom = 252;
  const chartW = width - padL - padR;
  const chartH = plotBottom - padT;

  const maxBalance = Math.max(...points.map((p) => p.commons_balance_love), 1);
  const maxKwh = Math.max(...points.map((p) => p.total_kwh), 1);

  const xPos = (i: number) =>
    points.length === 1 ? padL + chartW / 2 : padL + (i / (points.length - 1)) * chartW;
  const yBal = (bal: number) => plotBottom - (bal / maxBalance) * chartH;
  const yKwh = (kwh: number) => plotBottom - (kwh / maxKwh) * chartH;

  const balPath = points
    .map((p, i) => `${i === 0 ? 'M' : 'L'} ${xPos(i).toFixed(1)} ${yBal(p.commons_balance_love).toFixed(1)}`)
    .join(' ');
  const kwhPath = points
    .map((p, i) => `${i === 0 ? 'M' : 'L'} ${xPos(i).toFixed(1)} ${yKwh(p.total_kwh).toFixed(1)}`)
    .join(' ');

  const slotW = points.length > 1 ? chartW / (points.length - 1) : 64;
  const barWidth = Math.max(12, Math.min(34, slotW * 0.42));

  return (
    <Panel
      title="Verified Commons Ledger & Metered Energy History"
      subtitle="Observed treasury covenant balance (LOVE) and cumulative hardware-verified energy (kWh)"
      icon={<Activity className="h-4 w-4 text-emerald-400" />}
      accent="emerald"
      badge={
        <div className="flex items-center gap-3 font-mono text-[10px]">
          <span className="inline-flex items-center gap-1.5 text-emerald-300">
            <span className="h-2 w-2 rounded-full bg-emerald-400" />
            Treasury (LOVE)
          </span>
          <span className="inline-flex items-center gap-1.5 text-amber-300">
            <span className="h-2 w-2 rounded-full bg-amber-400" />
            Verified kWh
          </span>
        </div>
      }
    >
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2 rounded-lg border border-slate-800/90 bg-slate-950/75 px-3.5 py-2 text-xs">
        <div className="flex flex-wrap items-center gap-2.5">
          <span className="rounded bg-emerald-500/15 px-2 py-0.5 font-mono text-[10px] font-semibold text-emerald-300">
            Step #{active.step}
          </span>
          <span className="font-medium text-slate-200">{active.label}</span>
        </div>
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 font-mono text-[11px] text-slate-400 tabular-nums">
          <span>
            Treasury:{' '}
            <strong className="text-emerald-300">
              {active.commons_balance_love.toLocaleString()} LOVE
            </strong>
          </span>
          <span>
            Verified:{' '}
            <strong className="text-amber-300">{active.total_kwh.toLocaleString()} kWh</strong>
          </span>
          <span>
            Parcels: <strong className="text-slate-100">{active.parcels}</strong>
          </span>
          <span>
            Rate:{' '}
            <strong className="text-sky-300">
              {active.verified_love_per_kwh !== null
                ? `${active.verified_love_per_kwh.toFixed(4)} LOVE/kWh`
                : 'No metered telemetry'}
            </strong>
          </span>
        </div>
      </div>

      <div className="relative w-full overflow-x-auto">
        <svg
          viewBox={`0 0 ${width} ${height}`}
          className="h-auto w-full min-w-[540px] select-none"
          role="img"
          aria-label="Verified Commons Treasury Balance and Cumulative Metered kWh"
          onMouseLeave={() => setHoveredIdx(null)}
        >
          {[0, 0.25, 0.5, 0.75, 1].map((frac) => {
            const y = plotBottom - frac * chartH;
            return (
              <g key={frac}>
                <line
                  x1={padL}
                  x2={width - padR}
                  y1={y}
                  y2={y}
                  stroke="rgba(51, 65, 85, 0.45)"
                  strokeDasharray={frac === 0 ? undefined : '3 4'}
                  strokeWidth={1}
                />
                <text
                  x={padL - 10}
                  y={y + 3.5}
                  textAnchor="end"
                  className="fill-emerald-400/80 font-mono text-[9.5px]"
                >
                  {Math.round(maxBalance * frac).toLocaleString()}
                </text>
                <text
                  x={width - padR + 10}
                  y={y + 3.5}
                  textAnchor="start"
                  className="fill-amber-400/80 font-mono text-[9.5px]"
                >
                  {Math.round(maxKwh * frac).toLocaleString()}
                </text>
              </g>
            );
          })}

          {points.length > 1 && (
            <>
              <path d={balPath} fill="none" stroke="#10b981" strokeWidth={2.25} />
              <path
                d={kwhPath}
                fill="none"
                stroke="#f59e0b"
                strokeWidth={2}
                strokeDasharray="5 3"
              />
            </>
          )}

          {points.map((p, i) => {
            const cx = xPos(i);
            const cyBal = yBal(p.commons_balance_love);
            const cyKwh = yKwh(p.total_kwh);
            const isSelected = i === activeIdx;
            const bHeight = Math.max(2, plotBottom - cyBal);

            return (
              <g
                key={`${p.step}-${i}`}
                onMouseEnter={() => setHoveredIdx(i)}
                className="cursor-pointer"
              >
                <rect
                  x={cx - barWidth / 2}
                  y={cyBal}
                  width={barWidth}
                  height={bHeight}
                  rx={3}
                  fill="#10b981"
                  fillOpacity={isSelected ? 0.35 : 0.18}
                />
                <circle
                  cx={cx}
                  cy={cyBal}
                  r={isSelected ? 5.5 : 4}
                  fill="#06090f"
                  stroke="#10b981"
                  strokeWidth={2}
                />
                {p.total_kwh > 0 && (
                  <circle
                    cx={cx}
                    cy={cyKwh}
                    r={isSelected ? 5 : 3.5}
                    fill="#06090f"
                    stroke="#f59e0b"
                    strokeWidth={2}
                  />
                )}
                <text
                  x={cx}
                  y={plotBottom + 18}
                  textAnchor="middle"
                  className={`font-mono text-[9.5px] ${
                    isSelected ? 'fill-emerald-300 font-semibold' : 'fill-slate-500'
                  }`}
                >
                  S{p.step}
                </text>
              </g>
            );
          })}
        </svg>
      </div>
    </Panel>
  );
}
