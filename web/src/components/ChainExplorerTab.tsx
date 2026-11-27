import { useCallback, useEffect, useState } from 'react';
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
  Terminal,
  WifiOff,
} from 'lucide-react';
import type { AnchorReceiptRecord, SeedStatusResponse } from '../types';
import { formatL1IssuedLove } from '../api/normalize';
import { CopyButton, HashPill } from './ui/CopyButton';
import { Panel } from './ui/Panel';

interface LiveV2VerifyResponse {
  ok: boolean;
  endpoint_base: string;
  genesis_verified: boolean;
  live_height: number;
  asert_ramp: {
    phase: string;
    height: number;
    launch_blocks: number;
    launch_half_life_sec: number;
    steady_half_life_sec: number;
    progress_pct: number;
  };
  nspq2_pinned_peers: Array<{
    name: string;
    peer_arg: string;
    cli_cmd: string;
  }>;
  independent_verifier_cli: string;
  reserved_soft_fork_bits: Array<{
    bit: number;
    name: string;
    status: string;
  }>;
  verification: {
    genesis_matches_canonical: boolean;
    flyclient_header_mmr_bound: boolean;
    state_header_mmr_root: string | null;
    utreexo_frontier_verified: boolean | null;
  };
  flyclient?: {
    ok?: boolean;
    mmr_root?: string;
    peak_count?: number;
    sample_count?: number;
    total_work?: string;
    header_mmr_bound?: boolean;
  } | null;
  utreexo?: {
    ok?: boolean;
    note_count?: number;
    frontier_bytes?: number;
    max_frontier_bytes?: number;
    root?: string;
    root_matches_full_tree?: boolean;
  } | null;
  supply?: {
    ok?: boolean;
    issued?: string;
    burned?: string;
    locked?: string;
    base_fee_burn_bps?: number;
    open_priors?: number;
    open_channels?: number;
  } | null;
}

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
    phase: 'Layer 1 (Narrow Waist)',
    title: 'v1.2.5 Minimalist SHA-2 Settlement Floor',
    status: 'IMPLEMENTED' as const,
    summary:
      'Pure SHA-2 shielded cash (256-bit Quartic Goldilocks FRI at 100–104 conservative QROM bits with patched upstream f64 canonicalization, zero elliptic curves, zero novel ZK-hashes), all 249 spend AIR constraints + 114,688 periodic cells + 4-mode assertions + BatchSpend K=2..16 tiling mechanized in Lean 4 (0 sorry), Bitcoin AuxPoW (chainid 20051) + panic-free Day-14 ASERT ramp, O(log N) Merkleized state, NSPQ1/NSPQ2 SLH-DSA pinned PQ P2P, FlyClient MMR, 50% fee burn, PayWord channels, and minimal PriorLock.',
    specs: ['v1.2.5', '100-104b QROM', 'Lean 4 Full + Batch'],
  },
  {
    phase: 'Layer 2 (Above Waist)',
    title: 'Agent Commerce, Bridges & Labs Gateway (This Site)',
    status: 'IMPLEMENTED' as const,
    summary:
      'All rich application tooling segregated above L1: 35-tool zero-auth MCP server (Official MCP Registry 0.28.4), 60-second private x402/nsUSD paywall middleware generator, live Base Mainnet USDC vault escrow verifier, RFC-0028 nsUSD stress engine, Winterfell STARK L2 rollup benchmark, 4-chain ownerless EVM HTLCs & Backing Vaults, ERC-8004 identity, and zkTLS P2P verifier.',
    specs: ['35 MCP Tools', '60s x402 Paywall Kit', '4-Chain HTLCs'],
  },
  {
    phase: 'Security Bounty',
    title: 'Public LOVE Bug Bounty & Open BTC Merged Mining',
    status: 'IMPLEMENTED' as const,
    summary:
      'Open coin-denominated Public Security Bug Bounty funded by fair-mined Mainnet v2 LOVE (`ns1af734b0e0…d8cb66`, scope: `docs/AUDIT_SCOPE_V2.md`), paired with 0%-fee direct-payout Bitcoin AuxPoW solo pool (`stratum+tcp://pool.newsatoshi.org:3333`).',
    specs: ['LOVE Bug Bounty', 'AUDIT_SCOPE_V2.md', 'pool.newsatoshi.org'],
  },
  {
    phase: 'Mainnet v2 Genesis',
    title: 'LIVE — Bitcoin #977,738 Fair-Launch Genesis (nsM2)',
    status: 'IMPLEMENTED' as const,
    summary:
      'Canonical Mainnet v2 (v1.2.5) live with zero premine at Genesis 00000000ba1cdad4… anchored to Bitcoin block #977,738 (`newsatoshi --net mainnet-v2 node`, seeds `newsatoshi-seed-v2.fly.dev:8445` & `pool.newsatoshi.org:8445`).',
    specs: ['BTC #977,738', 'Day-1 ASERT Ramp', 'Zero Premine'],
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

  const [blocksNet, setBlocksNet] = useState<'mainnet-v2' | 'testnet'>('mainnet-v2');
  const [liveVerify, setLiveVerify] = useState<LiveV2VerifyResponse | null>(null);
  const [verifyingV2, setVerifyingV2] = useState(false);

  const loadLiveVerify = useCallback(async () => {
    setVerifyingV2(true);
    try {
      const resp = await fetch('/api/v2/live-verify', { cache: 'no-store' });
      if (resp.ok) {
        const data = (await resp.json()) as LiveV2VerifyResponse;
        setLiveVerify(data);
      }
    } catch {
      // keep previous snapshot
    } finally {
      setVerifyingV2(false);
    }
  }, []);

  useEffect(() => {
    void loadLiveVerify();
  }, [loadLiveVerify, ms?.height]);

  const activeBlocks =
    blocksNet === 'mainnet-v2'
      ? ms?.recent ?? []
      : s?.recent ?? [];

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

      {/* Live Mainnet v2 Cryptographic Frontier Verifier (FlyClient MMR · Utreexo · NSPQ2 · BIP-9) */}
      <Panel
        title="Mainnet v2 (nsM2) Live Cryptographic Verifier — FlyClient · Utreexo · NSPQ2"
        subtitle="Real-time verification against live newsatoshi.org/mainnet (zero mocked data)"
        icon={<Shield className="h-4 w-4 text-emerald-400" />}
        accent="emerald"
        badge={
          <span
            className={`rounded-full border px-2.5 py-0.5 font-mono text-[10px] font-semibold uppercase ${
              liveVerify?.verification.genesis_matches_canonical &&
              liveVerify?.verification.flyclient_header_mmr_bound
                ? 'border-emerald-500/40 bg-emerald-500/15 text-emerald-300'
                : 'border-amber-500/40 bg-amber-500/15 text-amber-300'
            }`}
          >
            {liveVerify?.verification.flyclient_header_mmr_bound
              ? `MMR BOUND · #${liveVerify.live_height.toLocaleString()}`
              : 'POLLING LIVE'}
          </span>
        }
        action={
          <button
            type="button"
            onClick={() => void loadLiveVerify()}
            disabled={verifyingV2}
            className="inline-flex items-center gap-1.5 rounded-md border border-slate-700 bg-slate-900 px-2.5 py-1 text-xs font-medium text-slate-300 hover:border-emerald-500/50 hover:text-emerald-200 cursor-pointer disabled:opacity-50"
          >
            <RefreshCw
              className={`h-3.5 w-3.5 ${verifyingV2 ? 'animate-spin text-emerald-400' : ''}`}
            />
            Verify Live
          </button>
        }
      >
        {liveVerify && liveVerify.ok ? (
          <div className="space-y-4">
            <div className="grid grid-cols-1 gap-3 md:grid-cols-4">
              <div className="rounded-lg border border-slate-800/80 bg-slate-950/60 p-3">
                <div className="font-mono text-[10px] uppercase text-slate-400">
                  Day-1 ASERT Launch Ramp
                </div>
                <div className="mt-1 font-mono text-base font-bold text-emerald-300 tabular-nums">
                  {liveVerify.asert_ramp.height.toLocaleString()} / {liveVerify.asert_ramp.launch_blocks.toLocaleString()}
                </div>
                <div className="mt-1.5 h-1.5 w-full overflow-hidden rounded-full bg-slate-800">
                  <div
                    className="h-full bg-emerald-400"
                    style={{ width: `${Math.max(2, Math.min(100, liveVerify.asert_ramp.progress_pct))}%` }}
                  />
                </div>
                <div className="mt-1 font-mono text-[9.5px] text-slate-500">
                  1h launch half-life → 2d steady
                </div>
              </div>

              <div className="rounded-lg border border-slate-800/80 bg-slate-950/60 p-3">
                <div className="font-mono text-[10px] uppercase text-slate-400">
                  FlyClient O(log N) MMR
                </div>
                <div className="mt-1 font-mono text-base font-bold text-sky-300">
                  {liveVerify.verification.flyclient_header_mmr_bound ? 'BOUND IN HEADER' : 'CHECKING'}
                </div>
                <div className="mt-1 font-mono text-[10px] text-slate-400">
                  {liveVerify.flyclient?.peak_count ?? 0} peaks · {liveVerify.flyclient?.sample_count ?? 0} samples
                </div>
              </div>

              <div className="rounded-lg border border-slate-800/80 bg-slate-950/60 p-3">
                <div className="font-mono text-[10px] uppercase text-slate-400">
                  Utreexo Shielded Frontier
                </div>
                <div className="mt-1 font-mono text-base font-bold text-violet-300 tabular-nums">
                  {liveVerify.utreexo?.ok
                    ? `${liveVerify.utreexo.frontier_bytes ?? 0} B / 1,024 B`
                    : 'v1.2.1 rolling…'}
                </div>
                <div className="mt-1 font-mono text-[10px] text-slate-400">
                  {liveVerify.utreexo?.root_matches_full_tree
                    ? 'Bit-exact NoteTree match'
                    : 'Constant <= 1 KB RAM'}
                </div>
              </div>

              <div className="rounded-lg border border-slate-800/80 bg-slate-950/60 p-3">
                <div className="font-mono text-[10px] uppercase text-slate-400">
                  Homeostatic Fee Burn
                </div>
                <div className="mt-1 font-mono text-base font-bold text-amber-300 tabular-nums">
                  {((liveVerify.supply?.base_fee_burn_bps ?? 5000) / 100).toFixed(0)}% Base Fee
                </div>
                <div className="mt-1 font-mono text-[10px] text-slate-400">
                  {liveVerify.supply?.open_priors ?? 0} priors · {liveVerify.supply?.open_channels ?? 0} PayWord ch.
                </div>
              </div>
            </div>

            {liveVerify.verification.state_header_mmr_root && (
              <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-slate-800/80 bg-slate-950/60 px-3 py-2 text-xs">
                <span className="text-slate-400">Verified Live Header MMR Commitment (`nsSTATE`)</span>
                <HashPill hash={liveVerify.verification.state_header_mmr_root} head={16} tail={14} />
              </div>
            )}

            <div className="grid grid-cols-1 gap-3 lg:grid-cols-2">
              {liveVerify.nspq2_pinned_peers.map((peer) => (
                <div
                  key={peer.name}
                  className="rounded-lg border border-emerald-500/20 bg-[#04070d] p-3"
                >
                  <div className="mb-1 flex items-center justify-between gap-2">
                    <span className="inline-flex items-center gap-1.5 font-mono text-[10px] uppercase text-emerald-400">
                      <Terminal className="h-3 w-3" />
                      NSPQ2 SLH-DSA Pinned Peer · {peer.name}
                    </span>
                    <CopyButton value={peer.cli_cmd} label="Copy Pin CLI" />
                  </div>
                  <code className="block break-all font-mono text-[10.5px] text-emerald-200/90">
                    {peer.cli_cmd}
                  </code>
                </div>
              ))}
            </div>

            <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-slate-800/80 bg-slate-950/60 p-3">
              <div>
                <div className="font-mono text-[10px] uppercase text-sky-300">
                  Zero-Rust-Code Independent JS/OpenSSL Live Verifier
                </div>
                <code className="mt-0.5 block font-mono text-[11px] text-slate-300">
                  {liveVerify.independent_verifier_cli}
                </code>
              </div>
              <CopyButton value={liveVerify.independent_verifier_cli} label="Copy Verifier" />
            </div>

            <div className="flex flex-wrap items-center gap-2 text-[10.5px] font-mono">
              <span className="text-slate-400">Reserved Future BIP-9 Soft-Fork Bits:</span>
              {liveVerify.reserved_soft_fork_bits.map((b) => (
                <span
                  key={b.bit}
                  className="rounded border border-slate-700 bg-slate-900 px-2 py-0.5 text-slate-300"
                >
                  bit {b.bit}: {b.name}
                </span>
              ))}
            </div>
          </div>
        ) : (
          <div className="text-xs text-slate-400">Loading live Mainnet v2 cryptographic state…</div>
        )}
      </Panel>

      {/* Recent Live Blocks Table */}
      <Panel
        title={
          blocksNet === 'mainnet-v2'
            ? 'Live Newsatoshi Mainnet v2 (nsM2) Blocks'
            : 'Live Newsatoshi Testnet (nsT0) Blocks'
        }
        subtitle={
          blocksNet === 'mainnet-v2'
            ? 'Observed directly from newsatoshi.org/mainnet/status (canonical BTC #977,738 chain, never mocked)'
            : 'Observed directly from newsatoshi-seed.fly.dev/status (never mocked)'
        }
        icon={<Blocks className="h-4 w-4 text-emerald-400" />}
        badge={
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setBlocksNet('mainnet-v2')}
              className={`rounded px-2 py-0.5 font-mono text-[10px] font-semibold cursor-pointer ${
                blocksNet === 'mainnet-v2'
                  ? 'border border-amber-500/50 bg-amber-500/20 text-amber-200'
                  : 'border border-slate-700 bg-slate-900 text-slate-400 hover:text-slate-200'
              }`}
            >
              Mainnet v2 ({ms?.recent.length ?? 0})
            </button>
            <button
              type="button"
              onClick={() => setBlocksNet('testnet')}
              className={`rounded px-2 py-0.5 font-mono text-[10px] font-semibold cursor-pointer ${
                blocksNet === 'testnet'
                  ? 'border border-emerald-500/50 bg-emerald-500/20 text-emerald-200'
                  : 'border border-slate-700 bg-slate-900 text-slate-400 hover:text-slate-200'
              }`}
            >
              Testnet ({s?.recent.length ?? 0})
            </button>
          </div>
        }
      >
        {activeBlocks.length > 0 ? (
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
                {activeBlocks.map((blk) => {
                  const isAnchored = anchoredHeights.has(blk.height);
                  return (
                    <tr
                      key={`${blocksNet}-${blk.height}-${blk.hash}`}
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
