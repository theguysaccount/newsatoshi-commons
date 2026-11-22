import {
  AlertTriangle,
  Blocks,
  CheckCircle2,
  Clock,
  ExternalLink,
  GitBranch,
  Globe,
  Lock,
  RefreshCw,
  Shield,
  WifiOff,
} from 'lucide-react';
import type { AnchorReceiptRecord, SeedStatusResponse } from '../types';
import { formatL1IssuedLove } from '../api/normalize';
import { HashPill } from './ui/CopyButton';
import { Panel } from './ui/Panel';

interface ChainExplorerTabProps {
  seed: SeedStatusResponse | null;
  anchorReceipts: AnchorReceiptRecord[];
  refreshing: boolean;
  onRefresh: () => void;
}

function formatBlockTime(unix: number): string {
  if (!unix) return '—';
  const d = new Date(unix * 1000);
  return d.toISOString().replace('T', ' ').replace(/\.\d+Z$/, ' UTC');
}

const MIGRATION_PHASES = [
  {
    phase: 'Phase 0',
    title: 'Client-Side Validated (CSV) Commons & L1 Anchoring',
    status: 'IMPLEMENTED' as const,
    summary:
      'Client-Side Validated Rust engine (`newsatoshi-commons`) with NIST FIPS 205 SLH-DSA, XMSS Merkle-Lamport hardware signers, Winterfell 0.13 single-step STARK proofs, and L1 `newsatoshi lock --hash` anchors.',
    specs: ['Blind L1 Consensus', 'Winterfell 0.13 AIR', 'OP_RETURN 0x6a20'],
  },
  {
    phase: 'Phase 1',
    title: '90-Day Testnet & SP1 / Miden Audit',
    status: 'SPEC & HARNESS' as const,
    summary:
      'Upgrade single-step CSV proofs to constant-size recursive IVC chain-proof folding in SP1 Hypercube / Miden VM (`zkvm/covenant-circuit-core`) under a 90-day adversarial testnet soak.',
    specs: ['SP1 Hypercube Guest', 'Miden MASM AIR', '90-Day Soak'],
  },
  {
    phase: 'Phase 2',
    title: 'OP_VERIFY_COVENANT_STARK (0xb4) Soft-Fork',
    status: 'RFC-0021' as const,
    summary:
      'Backwards-compatible soft-fork redefining `OP_NOP5` (`0xb4`) via AuxPoW (`chainid = 20051`) merged-miner signaling. Enforces recursive `covenant_rule_hash` output continuity natively on L1.',
    specs: ['OP_NOP5 -> 0xb4', 'AuxPoW ChainID 20051', 'Output Continuity'],
  },
  {
    phase: 'Phase 3',
    title: 'Physical Parcel Legal-Cryptographic Airlock',
    status: 'TEMPLATE READY' as const,
    summary:
      'Bind real parcel deeds and metered hardware under counsel-reviewed Perpetual Purpose Trust §4.2 & Wyoming DUNA operating instruments tied to the active `covenant_rule_hash`.',
    specs: ['Purpose Trust §4.2', 'Wyoming DUNA CLT', 'PayWord Streaming'],
  },
];

export function ChainExplorerTab({
  seed,
  anchorReceipts,
  refreshing,
  onRefresh,
}: ChainExplorerTabProps) {
  const s = seed?.live ? seed.status : null;
  const ms = seed?.mainnet_live ? seed.mainnet_status : null;
  const m = seed?.mainnet_info ?? null;

  const anchoredHeights = new Set(
    anchorReceipts
      .map((r) => r.l1_height)
      .filter((h): h is number => typeof h === 'number')
  );

  return (
    <div className="space-y-6">
      {/* Testnet vs Mainnet Status Cards */}
      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        {/* Live Testnet Card */}
        <Panel
          title="Newsatoshi L1 Testnet — Live Seed Node"
          subtitle={seed?.endpoint || 'https://newsatoshi-seed.fly.dev/status'}
          icon={<Globe className="h-4 w-4 text-emerald-400" />}
          accent={s ? 'emerald' : 'crimson'}
          badge={
            <span
              className={`rounded-full border px-2.5 py-0.5 font-mono text-[10px] font-semibold uppercase ${
                s
                  ? 'border-emerald-500/40 bg-emerald-500/15 text-emerald-300'
                  : 'border-red-500/40 bg-red-500/15 text-red-300'
              }`}
            >
              {s ? 'LIVE POLLING (10s)' : 'DISCONNECTED'}
            </span>
          }
          action={
            <button
              type="button"
              onClick={onRefresh}
              disabled={refreshing}
              className="inline-flex items-center gap-1.5 rounded-md border border-slate-700 bg-slate-900 px-2.5 py-1 text-xs font-medium text-slate-300 hover:border-emerald-500/50 hover:text-emerald-200 cursor-pointer disabled:opacity-50"
            >
              <RefreshCw
                className={`h-3.5 w-3.5 ${refreshing ? 'animate-spin text-emerald-400' : ''}`}
              />
              Refresh
            </button>
          }
        >
          {s ? (
            <div className="space-y-4">
              <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
                <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-3">
                  <div className="text-[10px] uppercase tracking-wider text-slate-500">Height</div>
                  <div className="mt-1 font-mono text-lg font-bold text-emerald-300 tabular-nums">
                    #{s.height.toLocaleString()}
                  </div>
                </div>
                <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-3">
                  <div className="text-[10px] uppercase tracking-wider text-slate-500">Issued</div>
                  <div className="mt-1 font-mono text-lg font-bold text-amber-300 tabular-nums">
                    {formatL1IssuedLove(s.issued)}
                  </div>
                  <div className="font-mono text-[9.5px] text-slate-500">LOVE</div>
                </div>
                <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-3">
                  <div className="text-[10px] uppercase tracking-wider text-slate-500">
                    Shielded Notes
                  </div>
                  <div className="mt-1 font-mono text-lg font-bold text-slate-100 tabular-nums">
                    {s.notes.toLocaleString()}
                  </div>
                  <div className="font-mono text-[9.5px] text-slate-500">
                    {s.nullifiers.toLocaleString()} nullifiers
                  </div>
                </div>
                <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-3">
                  <div className="text-[10px] uppercase tracking-wider text-slate-500">
                    Peers / Locks
                  </div>
                  <div className="mt-1 font-mono text-lg font-bold text-sky-300 tabular-nums">
                    {s.peers} <span className="text-sm text-slate-500">/ {s.locks}</span>
                  </div>
                  <div className="font-mono text-[9.5px] text-slate-500">
                    {s.conditions} conds · v{s.version}
                  </div>
                </div>
              </div>

              <div className="space-y-2 rounded-lg border border-slate-800/80 bg-slate-950/60 p-3 text-xs">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="text-slate-400">Tip Block Hash</span>
                  <HashPill hash={s.tip} head={14} tail={12} />
                </div>
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="text-slate-400">Testnet Genesis</span>
                  <HashPill hash={s.genesis} head={14} tail={12} />
                </div>
                <div className="flex flex-wrap items-center justify-between gap-2 font-mono text-[11px] text-slate-300">
                  <span className="font-sans text-slate-400">AuxPoW Chain ID / Difficulty</span>
                  <span>
                    <strong className="text-sky-300">{s.auxpow_chainid}</strong> · diff{' '}
                    {s.difficulty.toFixed(6)}
                  </span>
                </div>
              </div>

              <div className="flex items-center justify-between text-[11px] text-slate-400">
                <span className="inline-flex items-center gap-1.5 font-mono">
                  <Clock className="h-3.5 w-3.5 text-slate-500" />
                  Polled {formatBlockTime(seed?.fetched_at_unix ?? 0)}
                </span>
                <a
                  href={seed?.endpoint || 'https://newsatoshi-seed.fly.dev/status'}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 font-mono text-emerald-400 hover:text-emerald-300"
                >
                  Raw JSON <ExternalLink className="h-3 w-3" />
                </a>
              </div>
            </div>
          ) : (
            <div className="flex flex-col items-center justify-center rounded-lg border border-dashed border-red-500/30 bg-red-950/10 px-5 py-10 text-center">
              <WifiOff className="mb-2.5 h-6 w-6 text-red-400" />
              <div className="text-sm font-medium text-red-200">
                Live seed status unavailable
              </div>
              <p className="mt-1 font-mono text-xs text-red-300/80">
                {seed?.error || 'Unable to fetch live /status from newsatoshi-seed.'}
              </p>
            </div>
          )}
        </Panel>

        {/* Live Mainnet Card */}
        <Panel
          title="Newsatoshi L1 Mainnet — Live Node Telemetry"
          subtitle={seed?.mainnet_endpoint || 'https://newsatoshi.org/mainnet/status'}
          icon={<Lock className="h-4 w-4 text-amber-400" />}
          accent="amber"
          badge={
            ms ? (
              <span className="rounded-full border border-amber-500/45 bg-amber-500/15 px-2.5 py-0.5 font-mono text-[10px] font-semibold uppercase text-amber-200">
                LIVE · HEIGHT #{ms.height.toLocaleString()}
              </span>
            ) : m ? (
              <span className="rounded-full border border-amber-500/40 bg-amber-500/15 px-2.5 py-0.5 font-mono text-[10px] font-semibold uppercase text-amber-300">
                HEIGHT {m.height} · GENESIS
              </span>
            ) : (
              <span className="rounded-full border border-slate-700 bg-slate-900 px-2.5 py-0.5 font-mono text-[10px] uppercase text-slate-400">
                UNAVAILABLE
              </span>
            )
          }
        >
          {ms ? (
            <div className="space-y-4">
              <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
                <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-3">
                  <div className="text-[10px] uppercase tracking-wider text-slate-500">Height</div>
                  <div className="mt-1 font-mono text-lg font-bold text-amber-300 tabular-nums">
                    #{ms.height.toLocaleString()}
                  </div>
                  <div className="font-mono text-[9.5px] text-slate-500">
                    {ms.height === 0 ? 'awaiting block #1' : 'active chain'}
                  </div>
                </div>
                <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-3">
                  <div className="text-[10px] uppercase tracking-wider text-slate-500">Issued</div>
                  <div className="mt-1 font-mono text-lg font-bold text-slate-100 tabular-nums">
                    {formatL1IssuedLove(ms.issued)}
                  </div>
                  <div className="font-mono text-[9.5px] text-slate-500">LOVE (no premine)</div>
                </div>
                <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-3">
                  <div className="text-[10px] uppercase tracking-wider text-slate-500">
                    Proof Profile
                  </div>
                  <div className="mt-1 font-mono text-lg font-bold text-emerald-300 uppercase">
                    {ms.proofs || 'main'}
                  </div>
                  <div className="font-mono text-[9.5px] text-slate-500">
                    bits {ms.bits || '1d00ffff'}
                  </div>
                </div>
                <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-3">
                  <div className="text-[10px] uppercase tracking-wider text-slate-500">
                    Peers / Locks
                  </div>
                  <div className="mt-1 font-mono text-lg font-bold text-sky-300 tabular-nums">
                    {ms.peers} <span className="text-sm text-slate-500">/ {ms.locks}</span>
                  </div>
                  <div className="font-mono text-[9.5px] text-slate-500">
                    {ms.conditions} conds · v{ms.version}
                  </div>
                </div>
              </div>

              <div className="space-y-2 rounded-lg border border-slate-800/80 bg-slate-950/60 p-3 text-xs">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="text-slate-400">Mainnet Genesis / Tip</span>
                  <HashPill hash={ms.tip} head={14} tail={12} />
                </div>
                <div className="flex flex-wrap items-center justify-between gap-2 font-mono text-[11px]">
                  <span className="font-sans text-slate-400">AuxPoW Chain ID / Difficulty</span>
                  <span className="text-slate-200">
                    <strong className="text-sky-300">{ms.auxpow_chainid}</strong> · diff{' '}
                    {ms.difficulty.toFixed(6)}
                  </span>
                </div>
                <div className="flex items-center justify-between font-mono text-[11px]">
                  <span className="font-sans text-slate-400">Consensus Invariant</span>
                  <span className="text-emerald-300">L1 Never Mints for Physical Work</span>
                </div>
              </div>

              <div className="flex items-start gap-2.5 rounded-lg border border-amber-500/25 bg-amber-500/10 p-3 text-xs text-amber-100/95">
                <Shield className="mt-0.5 h-4 w-4 shrink-0 text-amber-300" />
                <p className="leading-relaxed">
                  {ms.height === 0
                    ? 'Mainnet genesis is open with zero premine and seeds intentionally do not self-mine. External SHA-256 miners or Bitcoin AuxPoW pools (`--rpc` createauxblock/submitauxblock, chainid 20051) can mine Block #1+ at difficulty 1.'
                    : `Mainnet is actively producing blocks (#${ms.height.toLocaleString()}). Ready for verified on-chain L1 hash-lock anchoring.`}
                </p>
              </div>

              <div className="flex items-center justify-between text-[11px] text-slate-400">
                <span className="inline-flex items-center gap-1.5 font-mono">
                  <Clock className="h-3.5 w-3.5 text-slate-500" />
                  Polled {formatBlockTime(seed?.fetched_at_unix ?? 0)}
                </span>
                <a
                  href={seed?.mainnet_endpoint || 'https://newsatoshi.org/mainnet/status'}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 font-mono text-amber-300 hover:text-amber-200"
                >
                  Raw Mainnet JSON <ExternalLink className="h-3 w-3" />
                </a>
              </div>
            </div>
          ) : m ? (
            <div className="space-y-4">
              <div className="rounded-lg border border-amber-500/30 bg-amber-500/10 p-3.5 text-xs leading-relaxed text-amber-100/95">
                {m.note}
              </div>

              <div className="space-y-2 rounded-lg border border-slate-800/80 bg-slate-950/60 p-3 text-xs">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="text-slate-400">Mainnet Genesis Hash</span>
                  <HashPill hash={m.genesis} head={14} tail={12} />
                </div>
                <div className="flex items-center justify-between font-mono text-[11px]">
                  <span className="font-sans text-slate-400">AuxPoW Merged-Mining Chain ID</span>
                  <span className="font-semibold text-sky-300">{m.auxpow_chainid ?? 20051}</span>
                </div>
                <div className="flex items-center justify-between font-mono text-[11px]">
                  <span className="font-sans text-slate-400">Consensus Invariant</span>
                  <span className="text-emerald-300">L1 Never Mints for Physical Work</span>
                </div>
                <div className="flex items-center justify-between font-mono text-[11px]">
                  <span className="font-sans text-slate-400">Post-Quantum Primitives</span>
                  <span className="text-slate-200">SHA-256 · Winterfell STARK · FIPS 205 SLH-DSA</span>
                </div>
              </div>
            </div>
          ) : (
            <div className="flex items-center gap-2 rounded-lg border border-slate-800 bg-slate-950/50 p-4 text-xs text-slate-400">
              <AlertTriangle className="h-4 w-4 text-amber-400" />
              Connect to the Rust Commons node to inspect live mainnet telemetry.
            </div>
          )}
        </Panel>
      </div>

      {/* NEWSATOSHI/2.1 Soft-Fork Migration Spec Overview */}
      <Panel
        title="NEWSATOSHI/2.1 Upgrade Architecture — OP_VERIFY_COVENANT_STARK (0xb4)"
        subtitle="Reference specification roadmap from Phase 0 CSV anchoring to soft-fork activation"
        icon={<GitBranch className="h-4 w-4 text-sky-400" />}
      >
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-4">
          {MIGRATION_PHASES.map((item, idx) => (
            <div
              key={item.phase}
              className="relative flex flex-col justify-between rounded-xl border border-slate-800/90 bg-slate-950/60 p-4"
            >
              <div>
                <div className="flex items-center justify-between gap-2">
                  <span className="font-mono text-xs font-bold uppercase tracking-wider text-slate-300">
                    {item.phase}
                  </span>
                  <span className="inline-flex items-center gap-1 rounded-full border border-slate-700 bg-slate-900 px-2 py-0.5 font-mono text-[9.5px] font-semibold uppercase text-slate-300">
                    {item.status === 'IMPLEMENTED' && (
                      <CheckCircle2 className="h-3 w-3 text-emerald-400" />
                    )}
                    {item.status}
                  </span>
                </div>
                <h3 className="mt-2.5 text-sm font-semibold text-slate-100">{item.title}</h3>
                <p className="mt-1.5 text-xs leading-relaxed text-slate-400">{item.summary}</p>
              </div>
              <div className="mt-4 flex flex-wrap gap-1.5 border-t border-slate-800/70 pt-3">
                {item.specs.map((spec) => (
                  <span
                    key={spec}
                    className="rounded border border-slate-800 bg-slate-900/80 px-2 py-0.5 font-mono text-[10px] text-slate-300"
                  >
                    {spec}
                  </span>
                ))}
                <span className="ml-auto font-mono text-[10px] text-slate-600">0{idx + 1}/04</span>
              </div>
            </div>
          ))}
        </div>
      </Panel>

      {/* Recent Live Blocks Table */}
      <Panel
        title="Live Newsatoshi L1 Blocks"
        subtitle="Observed directly from newsatoshi-seed.fly.dev/status (never mocked)"
        icon={<Blocks className="h-4 w-4 text-emerald-400" />}
        badge={
          <span className="font-mono text-[10.5px] text-slate-400">
            {s ? `${s.recent.length} live blocks` : 'Disconnected'}
          </span>
        }
      >
        {s && s.recent.length > 0 ? (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead>
                <tr className="border-b border-slate-800 font-mono text-[10px] uppercase tracking-wider text-slate-500">
                  <th className="py-2.5 pr-4">Height</th>
                  <th className="py-2.5 pr-4">Block Hash</th>
                  <th className="py-2.5 pr-4">Timestamp (UTC)</th>
                  <th className="py-2.5 pr-4 text-right">Txs</th>
                  <th className="py-2.5 pr-4 text-right">Bytes</th>
                  <th className="py-2.5 text-right">AuxPoW</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/70 font-mono">
                {s.recent.map((blk) => {
                  const isAnchored = anchoredHeights.has(blk.height);
                  return (
                    <tr
                      key={`${blk.height}-${blk.hash}`}
                      className={isAnchored ? 'bg-emerald-500/10' : 'hover:bg-slate-900/60'}
                    >
                      <td className="py-2.5 pr-4 font-semibold text-emerald-300 tabular-nums">
                        #{blk.height.toLocaleString()}
                        {isAnchored && (
                          <span className="ml-2 rounded border border-emerald-500/40 bg-emerald-500/20 px-1.5 py-0.5 text-[9px] text-emerald-200">
                            ANCHORED
                          </span>
                        )}
                      </td>
                      <td className="py-2.5 pr-4">
                        <HashPill hash={blk.hash} head={14} tail={10} />
                      </td>
                      <td className="py-2.5 pr-4 text-slate-400">{formatBlockTime(blk.time)}</td>
                      <td className="py-2.5 pr-4 text-right text-slate-200 tabular-nums">
                        {blk.txs}
                      </td>
                      <td className="py-2.5 pr-4 text-right text-slate-400 tabular-nums">
                        {blk.bytes} B
                      </td>
                      <td className="py-2.5 text-right text-[10px]">
                        {blk.merge_mined ? (
                          <span className="text-emerald-300">Merge-mined</span>
                        ) : (
                          <span className="text-slate-500">Direct SHA-256</span>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        ) : (
          <div className="rounded-lg border border-dashed border-slate-800 bg-slate-950/40 px-4 py-8 text-center text-xs text-slate-400">
            No live blocks available — seed node is currently disconnected.
          </div>
        )}
      </Panel>
    </div>
  );
}
