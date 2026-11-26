import { useCallback, useEffect, useState } from 'react';
import {
  Briefcase,
  CheckCircle2,
  Coins,
  ExternalLink,
  FileCode2,
  RefreshCw,
  Rocket,
  ShieldCheck,
  Terminal,
  Vault,
} from 'lucide-react';
import {
  actionCompileWeb4Charter,
  actionPublishWeb4Charter,
  getBackingSummary,
  getWeb4Registries,
} from '../api/client';
import type {
  BackingProofSummary,
  CommonsState,
  SpawnCharterPayload,
  Web4BountyRecord,
  Web4Charter,
  Web4ServiceRecord,
} from '../types';
import { CopyButton, HashPill } from './ui/CopyButton';
import { Panel } from './ui/Panel';

interface Web4ForgeTabProps {
  hasOperatorToken: boolean;
  onStateUpdated: (next: CommonsState) => void;
  onNotice: (kind: 'success' | 'error', message: string) => void;
}

export function Web4ForgeTab({
  hasOperatorToken,
  onStateUpdated,
  onNotice,
}: Web4ForgeTabProps) {
  const [backing, setBacking] = useState<BackingProofSummary | null>(null);
  const [loadingBacking, setLoadingBacking] = useState(false);
  const [charters, setCharters] = useState<Web4Charter[]>([]);
  const [services, setServices] = useState<Web4ServiceRecord[]>([]);
  const [bounties, setBounties] = useState<Web4BountyRecord[]>([]);

  // Charter compiler form (strictly blank inputs — zero fake businesses)
  const [name, setName] = useState('');
  const [category, setCategory] = useState<SpawnCharterPayload['category']>('paid_api');
  const [mission, setMission] = useState('');
  const [parentCharterId, setParentCharterId] = useState('');
  const [maxSpend, setMaxSpend] = useState('100');
  const [opsBps, setOpsBps] = useState('6000');
  const [reinvestBps, setReinvestBps] = useState('2000');
  const [backingBps, setBackingBps] = useState('2000');
  const [payoutLove, setPayoutLove] = useState('');
  const [evmTreasury, setEvmTreasury] = useState('');
  const [evmChain, setEvmChain] = useState<SpawnCharterPayload['primary_evm_chain']>('base');

  const [compiling, setCompiling] = useState(false);
  const [publishing, setPublishing] = useState(false);
  const [compiledCharter, setCompiledCharter] = useState<Web4Charter | null>(null);
  const [compiledErc8004, setCompiledErc8004] = useState<Record<string, unknown> | null>(null);
  const [compileError, setCompileError] = useState<string | null>(null);

  const loadLive = useCallback(async (force = false) => {
    setLoadingBacking(true);
    try {
      const [b, reg] = await Promise.all([getBackingSummary(force), getWeb4Registries()]);
      if (b) setBacking(b);
      setCharters(reg.charters);
      setServices(reg.services);
      setBounties(reg.bounties);
    } finally {
      setLoadingBacking(false);
    }
  }, []);

  useEffect(() => {
    void loadLive(false);
  }, [loadLive]);

  const bpsTotal = (Number(opsBps) || 0) + (Number(reinvestBps) || 0) + (Number(backingBps) || 0);

  const buildPayload = (): SpawnCharterPayload => ({
    name: name.trim(),
    category,
    mission: mission.trim(),
    parent_charter_id: parentCharterId.trim() || undefined,
    max_spend_per_call_love: Math.max(1, Math.floor(Number(maxSpend) || 1)),
    ops_bps: Math.max(0, Math.floor(Number(opsBps) || 0)),
    reinvest_bps: Math.max(0, Math.floor(Number(reinvestBps) || 0)),
    backing_bps: Math.max(0, Math.floor(Number(backingBps) || 0)),
    payout_love_address: payoutLove.trim(),
    evm_treasury_address: evmTreasury.trim(),
    primary_evm_chain: evmChain,
  });

  const handleCompileCharter = async (e: React.FormEvent) => {
    e.preventDefault();
    setCompiling(true);
    setCompileError(null);
    try {
      const res = await actionCompileWeb4Charter(buildPayload());
      if (res.ok && res.charter) {
        setCompiledCharter(res.charter);
        setCompiledErc8004(res.erc8004_registration ?? null);
        onNotice('success', `Compiled sovereign Web4 charter ${res.charter.charter_id}`);
      } else {
        setCompiledCharter(null);
        setCompiledErc8004(null);
        setCompileError(res.error || 'Charter compilation failed');
      }
    } catch (err) {
      setCompileError(err instanceof Error ? err.message : 'Charter compile error');
    } finally {
      setCompiling(false);
    }
  };

  const handlePublishCharter = async () => {
    setPublishing(true);
    try {
      const res = await actionPublishWeb4Charter(buildPayload());
      if (res.state) onStateUpdated(res.state);
      if (res.ok && res.charter) {
        setCompiledCharter(res.charter);
        setCompiledErc8004(res.erc8004_registration ?? null);
        await loadLive(false);
        onNotice('success', `Published charter ${res.charter.charter_id} to Web4 registry`);
      } else {
        onNotice('error', res.error || 'Charter publish failed');
      }
    } catch (err) {
      onNotice('error', err instanceof Error ? err.message : 'Charter publish error');
    } finally {
      setPublishing(false);
    }
  };

  return (
    <div className="space-y-6">
      {/* Section 1: Live Multi-Chain Proof-of-Backing Vault Telemetry */}
      <Panel
        title="Live Multi-Chain Proof-of-Backing Vaults (NewsatoshiBackingVault.sol)"
        subtitle="Ownerless, non-withdrawable reserve vaults live at one universal address across Base, Arbitrum One, Optimism & Polygon — verified live via eth_call"
        icon={<Vault className="h-4 w-4 text-emerald-400" />}
        accent="emerald"
        badge={
          <div className="flex items-center gap-2">
            <span className="rounded-full border border-emerald-500/40 bg-emerald-500/10 px-2.5 py-0.5 font-mono text-[10px] font-semibold text-emerald-300">
              0 ADMIN WITHDRAW · 4× SOURCIFY EXACT_MATCH
            </span>
            <button
              type="button"
              onClick={() => void loadLive(true)}
              disabled={loadingBacking}
              className="inline-flex items-center gap-1 rounded-md border border-slate-700 bg-slate-900 px-2 py-0.5 font-mono text-[10px] text-slate-300 hover:border-emerald-500/50 hover:text-emerald-200 cursor-pointer disabled:opacity-50"
            >
              <RefreshCw className={`h-3 w-3 ${loadingBacking ? 'animate-spin' : ''}`} />
              Refresh RPC
            </button>
          </div>
        }
      >
        <div className="space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-emerald-500/30 bg-emerald-950/20 px-3.5 py-2.5 text-xs">
            <div className="flex flex-wrap items-center gap-2 font-mono">
              <span className="text-[10px] uppercase text-emerald-300">Universal Vault (Base · Arb · OP · Pol):</span>
              <code className="text-slate-100">
                {backing?.canonical_vault_address || '0x37Ed94525DBE408fA72cDd156409875666679265'}
              </code>
              <CopyButton
                value={backing?.canonical_vault_address || '0x37Ed94525DBE408fA72cDd156409875666679265'}
                label="Copy"
              />
            </div>
            <div className="flex flex-wrap items-center gap-3 font-mono text-[10px]">
              <a
                href={`https://basescan.org/tx/${backing?.genesis_base_deposit_tx || '0xc354e0cca5bd2f0e9ac22193ef6f4ea505393ab4be4177251f9f47ca1c10bf5c'}`}
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 text-emerald-300 hover:underline"
              >
                Genesis 0.001 ETH Backing TX <ExternalLink className="h-3 w-3" />
              </a>
              <a
                href="/whitepaper"
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 text-sky-300 hover:underline"
              >
                Lean 4 Proof &amp; Whitepaper <ExternalLink className="h-3 w-3" />
              </a>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-2.5 md:grid-cols-5">
            <div className="rounded-xl border border-slate-800 bg-slate-950/80 p-3">
              <div className="font-mono text-[10px] uppercase text-slate-500">Verified ETH Backed</div>
              <div className="mt-1 font-mono text-lg font-bold text-emerald-300">
                {backing ? `${backing.total_eth_formatted} ETH` : 'Loading…'}
              </div>
              <div className="mt-0.5 font-mono text-[10px] text-slate-400">
                {backing ? `${backing.total_vault_deposits} on-chain deposit(s)` : 'Live RPC'}
              </div>
            </div>
            <div className="rounded-xl border border-slate-800 bg-slate-950/80 p-3">
              <div className="font-mono text-[10px] uppercase text-slate-500">Verified USDC Backed</div>
              <div className="mt-1 font-mono text-lg font-bold text-sky-300">
                {backing ? `${backing.total_usdc_formatted} USDC` : 'Loading…'}
              </div>
              <div className="mt-0.5 font-mono text-[10px] text-slate-400">Canonical 4-chain USDC</div>
            </div>
            <div className="rounded-xl border border-slate-800 bg-slate-950/80 p-3">
              <div className="font-mono text-[10px] uppercase text-slate-500">Verified POL Backed</div>
              <div className="mt-1 font-mono text-lg font-bold text-violet-300">
                {backing ? `${backing.total_pol_formatted} POL` : 'Loading…'}
              </div>
              <div className="mt-0.5 font-mono text-[10px] text-slate-400">Polygon PoS (137)</div>
            </div>
            <div className="rounded-xl border border-slate-800 bg-slate-950/80 p-3">
              <div className="font-mono text-[10px] uppercase text-slate-500">On-Chain Charters</div>
              <div className="mt-1 font-mono text-lg font-bold text-amber-300">
                {backing ? backing.total_onchain_anchored_charters : '—'}
              </div>
              <div className="mt-0.5 font-mono text-[10px] text-slate-400">
                Immutable vault anchors
              </div>
            </div>
            <div className="rounded-xl border border-slate-800 bg-slate-950/80 p-3">
              <div className="font-mono text-[10px] uppercase text-slate-500">Mainnet Issued LOVE</div>
              <div className="mt-1 font-mono text-lg font-bold text-slate-100">
                {backing?.mainnet_issued_love
                  ? `${Math.round(backing.mainnet_issued_love).toLocaleString()}`
                  : '—'}
              </div>
              <div className="mt-0.5 font-mono text-[10px] text-slate-400">
                {backing?.mainnet_height ? `Height #${backing.mainnet_height.toLocaleString()}` : 'Live L1'}
              </div>
            </div>
          </div>

          {backing && (
            <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2 xl:grid-cols-4">
              {backing.chains.map((c) => (
                <div
                  key={c.chain_id}
                  className="rounded-xl border border-slate-800/90 bg-slate-950/75 p-3 text-xs"
                >
                  <div className="flex items-center justify-between">
                    <span className="font-semibold uppercase text-slate-100">
                      {c.chain} ({c.chain_id})
                    </span>
                    <span
                      className={`rounded border px-1.5 py-0.5 font-mono text-[9px] font-bold ${
                        c.live
                          ? 'border-emerald-500/40 bg-emerald-500/10 text-emerald-300'
                          : 'border-amber-500/40 bg-amber-500/10 text-amber-300'
                      }`}
                    >
                      {c.live ? 'LIVE RPC' : 'RPC ERR'}
                    </span>
                  </div>
                  <div className="mt-2 space-y-1 font-mono text-[11px] text-slate-300">
                    <div className="flex justify-between">
                      <span className="text-slate-500">Native:</span>
                      <span className="text-emerald-300">
                        {c.native_reserve_formatted} {c.native_symbol}
                      </span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-slate-500">USDC:</span>
                      <span className="text-sky-300">{c.usdc_reserve_formatted}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-slate-500">Deposits / Charters:</span>
                      <span>
                        {c.deposits_count} / {c.anchored_charters_count}
                      </span>
                    </div>
                  </div>
                  <div className="mt-2 flex items-center gap-3 font-mono text-[10px]">
                    <a
                      href={c.explorer_url}
                      target="_blank"
                      rel="noreferrer"
                      className="text-emerald-400 hover:underline"
                    >
                      Explorer ↗
                    </a>
                    <a
                      href={c.sourcify_url}
                      target="_blank"
                      rel="noreferrer"
                      className="text-sky-400 hover:underline"
                    >
                      Sourcify ↗
                    </a>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </Panel>

      {/* Section 2: Sovereign Web4 Business Charter Forge + Registries */}
      <div className="grid grid-cols-1 gap-6 xl:grid-cols-12">
        <div className="xl:col-span-7">
          <Panel
            title="Spawn a Sovereign Web4 Business Charter (Lean 4 Monotonic Covenants)"
            subtitle="Unlike prompt-only Web4 agents (constitution.md), every charter enforces hard per-call spending caps & automatic backing_bps splits verified in Lean 4"
            icon={<Rocket className="h-4 w-4 text-sky-400" />}
            accent="sky"
            badge={
              <span className="rounded-full border border-sky-500/40 bg-sky-500/10 px-2.5 py-0.5 font-mono text-[10px] font-semibold text-sky-200">
                ERC-8004 + FIPS 205 SLH-DSA
              </span>
            }
          >
            <form onSubmit={handleCompileCharter} className="space-y-3.5">
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                <div className="sm:col-span-2">
                  <label className="mb-1 block font-mono text-[10px] uppercase text-slate-400">
                    Business / Agent Charter Name
                  </label>
                  <input
                    type="text"
                    required
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder="e.g. Solar Compute Guild"
                    className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                  />
                </div>
                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase text-slate-400">
                    Business Rail
                  </label>
                  <select
                    value={category}
                    onChange={(e) => setCategory(e.target.value as SpawnCharterPayload['category'])}
                    className="w-full rounded-lg border border-slate-800 bg-slate-950 px-2.5 py-2 font-mono text-xs text-slate-100"
                  >
                    <option value="paid_api">Paid 402 API</option>
                    <option value="compute_node">Compute / GPU Node</option>
                    <option value="physical_rwa">Physical RWA / Solar</option>
                    <option value="liquidity_maker">HTLC Market Maker</option>
                    <option value="agent_collective">Agent Collective</option>
                  </select>
                </div>
              </div>

              <div>
                <label className="mb-1 block font-mono text-[10px] uppercase text-slate-400">
                  Constitutional Mandate (hashed into immutable constitution_hash)
                </label>
                <textarea
                  required
                  rows={2}
                  value={mission}
                  onChange={(e) => setMission(e.target.value)}
                  placeholder="Describe the exact revenue model, verification rule, and asset-backing commitment..."
                  className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                />
              </div>

              <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase text-slate-400">
                    Max Spend/Call (LOVE)
                  </label>
                  <input
                    type="number"
                    min={1}
                    required
                    value={maxSpend}
                    onChange={(e) => setMaxSpend(e.target.value)}
                    className="w-full rounded-lg border border-slate-800 bg-slate-950 px-2.5 py-2 font-mono text-xs text-slate-100"
                  />
                </div>
                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase text-slate-400">
                    Ops Survival (bps)
                  </label>
                  <input
                    type="number"
                    min={0}
                    max={10000}
                    required
                    value={opsBps}
                    onChange={(e) => setOpsBps(e.target.value)}
                    className="w-full rounded-lg border border-slate-800 bg-slate-950 px-2.5 py-2 font-mono text-xs text-slate-100"
                  />
                </div>
                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase text-slate-400">
                    Reinvest Growth (bps)
                  </label>
                  <input
                    type="number"
                    min={0}
                    max={10000}
                    required
                    value={reinvestBps}
                    onChange={(e) => setReinvestBps(e.target.value)}
                    className="w-full rounded-lg border border-slate-800 bg-slate-950 px-2.5 py-2 font-mono text-xs text-slate-100"
                  />
                </div>
                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase text-emerald-300">
                    Backing Vault (bps)
                  </label>
                  <input
                    type="number"
                    min={0}
                    max={10000}
                    required
                    value={backingBps}
                    onChange={(e) => setBackingBps(e.target.value)}
                    className="w-full rounded-lg border border-emerald-500/40 bg-slate-950 px-2.5 py-2 font-mono text-xs text-emerald-200"
                  />
                </div>
              </div>

              <div className="flex items-center justify-between rounded-lg border border-slate-800 bg-slate-950/75 px-3 py-1.5 font-mono text-[11px]">
                <span className="text-slate-400">Split Conservation Check (must equal 10,000 bps):</span>
                <span className={bpsTotal === 10000 ? 'font-bold text-emerald-300' : 'font-bold text-red-400'}>
                  {bpsTotal.toLocaleString()} / 10,000 bps ({(bpsTotal / 100).toFixed(2)}%)
                </span>
              </div>

              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase text-slate-400">
                    Newsatoshi Payout Address (ns1… / nsd1…)
                  </label>
                  <input
                    type="text"
                    required
                    value={payoutLove}
                    onChange={(e) => setPayoutLove(e.target.value)}
                    placeholder="ns1… or nsd1…"
                    className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                  />
                </div>
                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase text-slate-400">
                    EVM Treasury Address (0x…)
                  </label>
                  <input
                    type="text"
                    required
                    value={evmTreasury}
                    onChange={(e) => setEvmTreasury(e.target.value)}
                    placeholder="0x…"
                    className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                  />
                </div>
              </div>

              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase text-slate-400">
                    Primary On-Chain Anchor Network
                  </label>
                  <select
                    value={evmChain}
                    onChange={(e) => setEvmChain(e.target.value as SpawnCharterPayload['primary_evm_chain'])}
                    className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100"
                  >
                    <option value="base">Base Mainnet (8453)</option>
                    <option value="arbitrum">Arbitrum One (42161)</option>
                    <option value="optimism">Optimism (10)</option>
                    <option value="polygon">Polygon PoS (137)</option>
                  </select>
                </div>
                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase text-slate-400">
                    Parent Charter ID (optional — enforces Lean 4 monotonic cap)
                  </label>
                  <input
                    type="text"
                    value={parentCharterId}
                    onChange={(e) => setParentCharterId(e.target.value)}
                    placeholder="Optional parent charter-…"
                    className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                  />
                </div>
              </div>

              {compileError && (
                <div className="rounded-lg border border-red-500/40 bg-red-500/10 px-3 py-2 text-xs text-red-200">
                  {compileError}
                </div>
              )}

              <div className="flex flex-wrap items-center gap-2.5 pt-1">
                <button
                  type="submit"
                  disabled={compiling || bpsTotal !== 10000}
                  className="inline-flex items-center gap-2 rounded-lg bg-emerald-500 px-4 py-2 text-xs font-semibold text-slate-950 hover:bg-emerald-400 cursor-pointer disabled:opacity-50"
                >
                  <ShieldCheck className="h-4 w-4" />
                  {compiling ? 'Compiling Charter…' : 'Compile Web4 Charter + ERC-8004'}
                </button>
                {hasOperatorToken && (
                  <button
                    type="button"
                    disabled={publishing || bpsTotal !== 10000}
                    onClick={() => void handlePublishCharter()}
                    className="inline-flex items-center gap-2 rounded-lg border border-sky-500/50 bg-sky-500/15 px-3.5 py-2 text-xs font-semibold text-sky-200 hover:bg-sky-500/25 cursor-pointer disabled:opacity-50"
                  >
                    {publishing ? 'Publishing…' : 'Register on Public Board'}
                  </button>
                )}
              </div>
            </form>

            {compiledCharter && (
              <div className="mt-5 space-y-3 rounded-xl border border-emerald-500/35 bg-[#04080f] p-4 text-xs">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex items-center gap-2 font-mono font-semibold text-emerald-200">
                    <CheckCircle2 className="h-4 w-4 text-emerald-400" />
                    {compiledCharter.charter_id} · {compiledCharter.name}
                  </div>
                  <span className="rounded border border-emerald-500/40 bg-emerald-500/10 px-2 py-0.5 font-mono text-[10px] text-emerald-300">
                    Lineage Depth {compiledCharter.lineage_depth} · {compiledCharter.backing_bps} bps backing
                  </span>
                </div>
                <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 font-mono text-[10px] text-slate-400">
                  <div className="flex items-center justify-between rounded border border-slate-800 bg-slate-950 px-2.5 py-1.5">
                    <span>covenant_rule_hash</span>
                    <HashPill hash={compiledCharter.covenant_rule_hash} />
                  </div>
                  <div className="flex items-center justify-between rounded border border-slate-800 bg-slate-950 px-2.5 py-1.5">
                    <span>charter_id_bytes32</span>
                    <HashPill hash={compiledCharter.charter_id_bytes32} />
                  </div>
                </div>
                <div className="space-y-1.5 rounded-lg border border-slate-800 bg-slate-950/90 p-3">
                  <div className="flex items-center justify-between">
                    <span className="font-mono text-[10px] uppercase text-sky-300">
                      1. Anchor Immutably on NewsatoshiBackingVault ({compiledCharter.primary_evm_chain})
                    </span>
                    <CopyButton value={compiledCharter.cast_anchor_command} label="Copy cast" />
                  </div>
                  <code className="block break-all font-mono text-[10px] text-sky-200">
                    {compiledCharter.cast_anchor_command}
                  </code>
                </div>
                <div className="space-y-1.5 rounded-lg border border-slate-800 bg-slate-950/90 p-3">
                  <div className="flex items-center justify-between">
                    <span className="font-mono text-[10px] uppercase text-emerald-400">
                      2. Boot Autonomous 402 Business Daemon (web4-kit/)
                    </span>
                    <CopyButton value={compiledCharter.web4_kit_launch_command} label="Copy cmd" />
                  </div>
                  <code className="block break-all font-mono text-[10px] text-emerald-200">
                    {compiledCharter.web4_kit_launch_command}
                  </code>
                </div>
                {compiledErc8004 && (
                  <div className="space-y-1.5 rounded-lg border border-slate-800 bg-slate-950/90 p-3">
                    <div className="flex items-center justify-between">
                      <span className="font-mono text-[10px] uppercase text-amber-300">
                        3. ERC-8004 + Post-Quantum Dual Registration JSON
                      </span>
                      <CopyButton value={JSON.stringify(compiledErc8004, null, 2)} label="Copy JSON" />
                    </div>
                    <pre className="max-h-40 overflow-auto font-mono text-[10px] text-slate-300">
                      {JSON.stringify(compiledErc8004, null, 2)}
                    </pre>
                  </div>
                )}
              </div>
            )}
          </Panel>
        </div>

        {/* Right Column: Live Web4 Registries & Developer Quick-Connect */}
        <div className="space-y-6 xl:col-span-5">
          <Panel
            title="Web4 Paid 402 Service Directory & Work Bounties"
            subtitle="Strictly real registered agent services & HTLC/Freivalds bounties — zero fabricated listings"
            icon={<Briefcase className="h-4 w-4 text-amber-400" />}
            accent="amber"
            badge={
              <span className="font-mono text-[10px] text-slate-300">
                {charters.length} charters · {services.length} services · {bounties.length} bounties
              </span>
            }
          >
            <div className="space-y-3 text-xs">
              <div className="rounded-lg border border-slate-800 bg-slate-950/80 p-3">
                <div className="flex items-center justify-between font-mono text-[10px] uppercase text-emerald-300">
                  <span>Genesis Charter #1 (Anchored Live on Base Mainnet)</span>
                  <a
                    href="/api/web4/charters/charter-genesis-commons/erc8004.json"
                    target="_blank"
                    rel="noreferrer"
                    className="hover:underline"
                  >
                    ERC-8004 JSON ↗
                  </a>
                </div>
                <p className="mt-1 text-[11px] text-slate-300">
                  Newsatoshi Commons Verification Utility · 2,000 bps (20%) backing commitment ·{' '}
                  <a
                    href="https://basescan.org/tx/0x7a8183b60b6b1d8b46cc46b2eb022b3fa046f73b9f1f7ee464a5a3415815eb47"
                    target="_blank"
                    rel="noreferrer"
                    className="font-mono text-emerald-400 hover:underline"
                  >
                    Anchor TX ↗
                  </a>
                </p>
              </div>

              {services.length === 0 ? (
                <div className="rounded-lg border border-dashed border-slate-800 bg-slate-950/60 px-4 py-4 text-center text-[11px] text-slate-400">
                  No external 402 services registered yet. Connect via MCP tool{' '}
                  <code className="font-mono text-emerald-300">newsatoshi_discover_services</code> or test
                  our live PayWord-gated endpoint{' '}
                  <code className="font-mono text-sky-300">POST /api/web4/tools/verify-payword-batch</code>.
                </div>
              ) : (
                <div className="space-y-2">
                  {services.map((s) => (
                    <div key={s.service_id} className="rounded-lg border border-slate-800 bg-slate-950 p-2.5">
                      <div className="flex items-center justify-between font-semibold text-slate-100">
                        <span>{s.name}</span>
                        <span className="font-mono text-[10px] text-emerald-300">
                          {s.unit_price_love_base_units} base units/call
                        </span>
                      </div>
                      <p className="mt-1 text-[11px] text-slate-400">{s.description}</p>
                    </div>
                  ))}
                </div>
              )}

              {bounties.length === 0 ? (
                <div className="rounded-lg border border-dashed border-slate-800 bg-slate-950/60 px-4 py-4 text-center text-[11px] text-slate-400">
                  No open HTLC / Freivalds bounties posted yet. Compile trustless bounty escrow commands via{' '}
                  <code className="font-mono text-amber-300">POST /api/web4/bounties/compile</code>.
                </div>
              ) : (
                <div className="space-y-2">
                  {bounties.map((b) => (
                    <div key={b.bounty_id} className="rounded-lg border border-slate-800 bg-slate-950 p-2.5">
                      <div className="flex items-center justify-between font-semibold text-slate-100">
                        <span>{b.title}</span>
                        <span className="font-mono text-[10px] text-amber-300">
                          {b.reward_amount} {b.reward_asset}
                        </span>
                      </div>
                      <p className="mt-1 text-[11px] text-slate-400">{b.description}</p>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </Panel>

          <Panel
            title="Web4 Developer & Conway Automaton Quick-Connect"
            subtitle="14 live MCP tools · Official MCP Registry · Zero-dep JS & Python 402 SDKs · Lean 4 formal proof"
            icon={<Terminal className="h-4 w-4 text-emerald-400" />}
            accent="emerald"
          >
            <div className="space-y-2.5 text-[11px] text-slate-300">
              <div className="grid grid-cols-2 gap-2">
                {[
                  { label: 'Whitepaper', href: '/whitepaper', icon: FileCode2 },
                  { label: 'Lean 4 Kernel (.lean)', href: '/formal/NewsatoshiCovenants.lean', icon: ShieldCheck },
                  { label: 'ERC-8004 Card', href: '/.well-known/erc8004.json', icon: Coins },
                  { label: 'Live Backing JSON', href: '/api/backing/summary', icon: Vault },
                ].map((item) => {
                  const Icon = item.icon;
                  return (
                    <a
                      key={item.href}
                      href={item.href}
                      target="_blank"
                      rel="noreferrer"
                      className="flex items-center gap-2 rounded-lg border border-slate-800 bg-slate-950/80 px-3 py-2 font-mono text-[10.5px] text-emerald-300 hover:border-emerald-500/50"
                    >
                      <Icon className="h-3.5 w-3.5 shrink-0" />
                      <span className="truncate">{item.label}</span>
                    </a>
                  );
                })}
              </div>
              <div className="rounded-lg border border-slate-800 bg-[#04080f] p-3">
                <div className="mb-1 flex items-center justify-between">
                  <span className="font-mono text-[10px] uppercase text-emerald-300">
                    Run Local Web4 Autonomous Business Daemon (Zero Deps)
                  </span>
                  <CopyButton value="node web4-kit/agent-business.mjs --check" label="Copy" />
                </div>
                <code className="block font-mono text-[10.5px] text-slate-200">
                  node web4-kit/agent-business.mjs --check
                </code>
              </div>
            </div>
          </Panel>
        </div>
      </div>
    </div>
  );
}
