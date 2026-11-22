import { Activity, Cpu, Lock, Radio, RefreshCw, Shield, Users, WifiOff } from 'lucide-react';
import type { SeedStatusResponse } from '../types';
import { formatL1IssuedLove } from '../api/normalize';
import { CopyButton, truncateHash } from './ui/CopyButton';

interface TopL1RailProps {
  seed: SeedStatusResponse | null;
  refreshing: boolean;
  onRefresh: () => void;
}

export function TopL1Rail({ seed, refreshing, onRefresh }: TopL1RailProps) {
  const s = seed?.live ? seed.status : null;
  const ms = seed?.mainnet_live ? seed.mainnet_status : null;
  const m = seed?.mainnet_info ?? null;

  return (
    <div className="border-b border-slate-800/90 bg-[#04070d]/95 backdrop-blur-md">
      <div className="mx-auto flex max-w-[1480px] flex-wrap items-center justify-between gap-x-5 gap-y-2 px-4 py-2 text-[11px] sm:px-6">
        {/* Left: Strictly Live L1 Seed Metrics or Honest Disconnected State */}
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5">
          <div className="flex items-center gap-2">
            {s ? (
              <span className="relative flex h-2 w-2">
                <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75" />
                <span className="relative inline-flex h-2 w-2 rounded-full bg-emerald-400" />
              </span>
            ) : (
              <WifiOff className="h-3.5 w-3.5 text-red-400" />
            )}
            <span className="font-mono font-semibold uppercase tracking-wider text-slate-200">
              newsatoshi-seed
            </span>
            <span
              className={`rounded px-1.5 py-0.5 font-mono text-[10px] font-semibold uppercase ${
                s
                  ? 'bg-emerald-500/15 text-emerald-300 border border-emerald-500/30'
                  : 'bg-red-500/15 text-red-300 border border-red-500/30'
              }`}
            >
              {s ? `${s.network} live` : 'disconnected'}
            </span>
          </div>

          {s ? (
            <>
              <div className="hidden h-3.5 w-px bg-slate-800 sm:block" />

              <div className="flex items-center gap-1.5 text-slate-400">
                <Activity className="h-3.5 w-3.5 text-emerald-400" />
                <span>Height</span>
                <span className="font-mono font-semibold text-slate-100 tabular-nums">
                  #{s.height.toLocaleString()}
                </span>
              </div>

              <div className="flex items-center gap-1 text-slate-400">
                <span>Tip</span>
                <span className="font-mono text-slate-200" title={s.tip}>
                  {truncateHash(s.tip, 8, 6)}
                </span>
                <CopyButton
                  value={s.tip}
                  className="border-0 bg-transparent px-1 py-0 hover:bg-slate-800/80"
                />
              </div>

              <div className="flex items-center gap-1.5 text-slate-400">
                <Cpu className="h-3.5 w-3.5 text-sky-400" />
                <span>AuxPoW ID</span>
                <span className="font-mono font-semibold text-sky-300 tabular-nums">
                  {s.auxpow_chainid}
                </span>
              </div>

              <div className="flex items-center gap-1.5 text-slate-400">
                <span>Issued</span>
                <span className="font-mono font-semibold text-amber-300 tabular-nums">
                  {formatL1IssuedLove(s.issued)} LOVE
                </span>
              </div>

              <div className="flex items-center gap-1.5 text-slate-400">
                <Shield className="h-3.5 w-3.5 text-emerald-400/90" />
                <span>Notes</span>
                <span className="font-mono text-slate-200 tabular-nums">
                  {s.notes.toLocaleString()}
                </span>
                <span className="text-slate-600">/</span>
                <span>Nullifiers</span>
                <span className="font-mono text-slate-200 tabular-nums">
                  {s.nullifiers.toLocaleString()}
                </span>
              </div>

              <div className="flex items-center gap-1 text-slate-400">
                <Users className="h-3.5 w-3.5 text-slate-400" />
                <span className="font-mono text-slate-200 tabular-nums">{s.peers}</span>
                <span>peers</span>
              </div>
            </>
          ) : (
            <span className="font-mono text-[11px] text-slate-400">
              {seed?.error || 'Polling live seed status via /api/seed…'}
            </span>
          )}
        </div>

        {/* Right: Live Mainnet Node Pill + Refresh */}
        <div className="flex items-center gap-2">
          {(ms || m) && (
            <div
              className={`flex items-center gap-2 rounded-full border px-2.5 py-0.5 text-[10px] ${
                ms
                  ? 'border-amber-500/45 bg-amber-500/15'
                  : 'border-amber-500/30 bg-amber-500/10'
              }`}
              title={
                ms
                  ? `Live Mainnet (${seed?.mainnet_endpoint ?? 'newsatoshi.org/mainnet/status'}): height #${ms.height}, proofs=${ms.proofs ?? 'main'}, diff=${ms.difficulty}, peers=${ms.peers}`
                  : m?.note
              }
            >
              {ms ? (
                <span className="h-1.5 w-1.5 rounded-full bg-amber-400 shrink-0" />
              ) : (
                <Lock className="h-3 w-3 text-amber-400 shrink-0" />
              )}
              <span className="font-semibold uppercase tracking-wide text-amber-200">
                {ms ? 'Mainnet Live' : 'Mainnet Genesis'}
              </span>
              <span className="font-mono text-amber-300/95">
                h={(ms?.height ?? m?.height ?? 0).toLocaleString()}
              </span>
              {ms?.proofs && (
                <span className="hidden font-mono text-emerald-300/90 md:inline">
                  proofs:{ms.proofs}
                </span>
              )}
              <span className="hidden font-mono text-slate-400 xl:inline">
                {truncateHash(ms?.tip || m?.genesis || '', 8, 6)}
              </span>
              <CopyButton
                value={ms?.tip || m?.genesis || ''}
                className="border-0 bg-transparent px-0.5 py-0 text-amber-300 hover:bg-amber-500/20"
              />
            </div>
          )}

          <button
            type="button"
            onClick={onRefresh}
            disabled={refreshing}
            title="Poll live /api/seed now (auto-polls every 10s)"
            className="inline-flex items-center gap-1 rounded-md border border-slate-800 bg-slate-900/80 px-2 py-1 text-[10px] font-medium text-slate-400 transition-colors hover:border-slate-700 hover:text-slate-200 cursor-pointer disabled:opacity-50"
          >
            <RefreshCw className={`h-3 w-3 ${refreshing ? 'animate-spin text-emerald-400' : ''}`} />
            <Radio className="hidden h-3 w-3 text-emerald-500/70 sm:inline" />
            <span className="hidden sm:inline">10s</span>
          </button>
        </div>
      </div>
    </div>
  );
}
