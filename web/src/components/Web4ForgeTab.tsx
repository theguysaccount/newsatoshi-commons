import { useCallback, useEffect, useState } from 'react';
import {
  ArrowUpRight,
  CheckCircle2,
  RefreshCw,
  ShieldCheck,
  Wallet,
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
import { useWallet } from '../wallet/WalletContext';
import { QuickBackCard } from './QuickBackCard';
import { CopyButton, HashPill } from './ui/CopyButton';
import { Panel } from './ui/Panel';

interface Web4ForgeTabProps {
  hasOperatorToken: boolean;
  onStateUpdated: (next: CommonsState) => void;
  onNotice: (kind: 'success' | 'error', message: string) => void;
}

const INPUT_CLASS =
  'w-full rounded-xl border border-white/[0.1] bg-[#0a0a0d] px-3 py-2.5 text-xs text-zinc-100 placeholder:text-zinc-600 transition focus:border-white/30 focus:outline-none';

const LABEL_CLASS =
  'mb-1.5 block text-[11px] font-medium text-zinc-400';

export function Web4ForgeTab({
  hasOperatorToken,
  onStateUpdated,
  onNotice,
}: Web4ForgeTabProps) {
  const { evmAddress, loveAddress, openModal } = useWallet();
  const [backing, setBacking] = useState<BackingProofSummary | null>(null);
  const [loadingBacking, setLoadingBacking] = useState(false);
  const [showQuickBack, setShowQuickBack] = useState(false);
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
  const [payoutLove, setPayoutLove] = useState(loveAddress);
  const [evmTreasury, setEvmTreasury] = useState(evmAddress || '');
  const [evmChain, setEvmChain] = useState<SpawnCharterPayload['primary_evm_chain']>('base');

  useEffect(() => {
    if (evmAddress && !evmTreasury) setEvmTreasury(evmAddress);
  }, [evmAddress, evmTreasury]);

  useEffect(() => {
    if (loveAddress && !payoutLove) setPayoutLove(loveAddress);
  }, [loveAddress, payoutLove]);

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

  const opsNum = Math.max(0, Number(opsBps) || 0);
  const reinvestNum = Math.max(0, Number(reinvestBps) || 0);
  const backingNum = Math.max(0, Number(backingBps) || 0);
  const bpsTotal = opsNum + reinvestNum + backingNum;

  const buildPayload = (): SpawnCharterPayload => ({
    name: name.trim(),
    category,
    mission: mission.trim(),
    parent_charter_id: parentCharterId.trim() || undefined,
    max_spend_per_call_love: Math.max(1, Math.floor(Number(maxSpend) || 1)),
    ops_bps: Math.floor(opsNum),
    reinvest_bps: Math.floor(reinvestNum),
    backing_bps: Math.floor(backingNum),
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
        onNotice('success', `Compiled sovereign charter ${res.charter.charter_id}`);
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
        onNotice('success', `Published charter ${res.charter.charter_id}`);
      } else {
        onNotice('error', res.error || 'Charter publish failed');
      }
    } catch (err) {
      onNotice('error', err instanceof Error ? err.message : 'Charter publish error');
    } finally {
      setPublishing(false);
    }
  };

  const vaultAddress =
    backing?.canonical_vault_address || '0x37Ed94525DBE408fA72cDd156409875666679265';

  return (
    <div className="space-y-8">
      {/* 1. Live Multi-Chain Proof-of-Backing */}
      <Panel
        title="On-Chain Reserve Vault"
        subtitle="Immutable, non-withdrawable contract polled live via eth_call across 4 EVM mainnets"
        action={
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setShowQuickBack((v) => !v)}
              className="inline-flex items-center gap-1.5 rounded-full bg-zinc-100 px-3 py-1 text-[11px] font-semibold text-zinc-950 transition hover:bg-white cursor-pointer"
            >
              <ShieldCheck className="h-3 w-3" />
              <span>{showQuickBack ? 'Hide Deposit' : '1-Click Back Vault'}</span>
            </button>
            <button
              type="button"
              onClick={() => void loadLive(true)}
              disabled={loadingBacking}
              className="inline-flex items-center gap-1.5 rounded-full border border-white/[0.1] bg-white/[0.03] px-3 py-1 text-[11px] font-medium text-zinc-300 transition hover:border-white/[0.2] hover:text-white cursor-pointer disabled:opacity-50"
            >
              <RefreshCw className={`h-3 w-3 ${loadingBacking ? 'animate-spin' : ''}`} />
              <span>Refresh RPC</span>
            </button>
          </div>
        }
      >
        <div className="space-y-5">
          {showQuickBack && (
            <QuickBackCard
              onBacked={() => void loadLive(true)}
              onNotice={onNotice}
            />
          )}
          {/* Canonical Address Strip */}
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-white/[0.07] bg-[#0a0a0d] px-4 py-3 text-xs">
            <div className="flex flex-wrap items-center gap-2.5">
              <span className="text-[11px] font-medium text-zinc-500">Universal Vault</span>
              <code className="break-all font-mono text-zinc-200">{vaultAddress}</code>
              <CopyButton value={vaultAddress} />
            </div>
            <div className="flex flex-wrap items-center gap-4 text-[11px] font-medium text-zinc-400">
              <a
                href={`https://basescan.org/tx/${backing?.genesis_base_deposit_tx || '0xc354e0cca5bd2f0e9ac22193ef6f4ea505393ab4be4177251f9f47ca1c10bf5c'}`}
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 transition hover:text-zinc-100"
              >
                <span>Genesis 0.001 ETH TX</span>
                <ArrowUpRight className="h-3 w-3" />
              </a>
              <a
                href="/api/backing/summary"
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 transition hover:text-zinc-100"
              >
                <span>Raw Proof JSON</span>
                <ArrowUpRight className="h-3 w-3" />
              </a>
            </div>
          </div>

          {/* Aggregate Live Reserve Totals */}
          <div className="grid grid-cols-2 divide-y divide-white/[0.07] overflow-hidden rounded-xl border border-white/[0.07] bg-[#0a0a0d] md:grid-cols-5 md:divide-x md:divide-y-0">
            <div className="p-4">
              <div className="text-[11px] font-medium text-zinc-500">ETH Backed</div>
              <div className="mt-1.5 font-mono text-lg font-medium text-zinc-100 tabular-nums">
                {backing ? `${backing.total_eth_formatted} ETH` : '—'}
              </div>
              <div className="mt-0.5 text-[11px] text-zinc-500">
                {backing ? `${backing.total_vault_deposits} deposit` : 'Live RPC'}
              </div>
            </div>
            <div className="p-4">
              <div className="text-[11px] font-medium text-zinc-500">USDC Backed</div>
              <div className="mt-1.5 font-mono text-lg font-medium text-zinc-100 tabular-nums">
                {backing ? `${backing.total_usdc_formatted}` : '—'}
              </div>
              <div className="mt-0.5 text-[11px] text-zinc-500">4-chain canonical</div>
            </div>
            <div className="p-4">
              <div className="text-[11px] font-medium text-zinc-500">POL Backed</div>
              <div className="mt-1.5 font-mono text-lg font-medium text-zinc-100 tabular-nums">
                {backing ? `${backing.total_pol_formatted}` : '—'}
              </div>
              <div className="mt-0.5 text-[11px] text-zinc-500">Polygon PoS</div>
            </div>
            <div className="p-4">
              <div className="text-[11px] font-medium text-zinc-500">Anchored Charters</div>
              <div className="mt-1.5 font-mono text-lg font-medium text-zinc-100 tabular-nums">
                {backing ? backing.total_onchain_anchored_charters : '—'}
              </div>
              <div className="mt-0.5 text-[11px] text-zinc-500">On-chain vaults</div>
            </div>
            <div className="p-4">
              <div className="text-[11px] font-medium text-zinc-500">Mainnet Issued</div>
              <div className="mt-1.5 font-mono text-lg font-medium text-zinc-100 tabular-nums">
                {backing?.mainnet_issued_love
                  ? `${Math.round(backing.mainnet_issued_love).toLocaleString()} LOVE`
                  : '—'}
              </div>
              <div className="mt-0.5 text-[11px] text-zinc-500">
                {backing?.mainnet_height ? `Block #${backing.mainnet_height.toLocaleString()}` : 'Live L1'}
              </div>
            </div>
          </div>

          {/* 4-Chain Live Breakdown */}
          {backing && (
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
              {backing.chains.map((c) => (
                <div
                  key={c.chain_id}
                  className="rounded-xl border border-white/[0.07] bg-[#0a0a0d] p-4 text-xs"
                >
                  <div className="flex items-center justify-between">
                    <span className="font-medium capitalize text-zinc-100">
                      {c.chain}{' '}
                      <span className="font-mono text-[11px] text-zinc-500">{c.chain_id}</span>
                    </span>
                    <span className="inline-flex items-center gap-1.5 font-mono text-[10px] text-zinc-400">
                      <span
                        className={`h-1.5 w-1.5 rounded-full ${
                          c.live ? 'bg-emerald-400' : 'bg-amber-400'
                        }`}
                      />
                      {c.live ? 'Live' : 'Retry'}
                    </span>
                  </div>
                  <div className="mt-3 space-y-1.5 font-mono text-[11px]">
                    <div className="flex justify-between">
                      <span className="text-zinc-500">Native</span>
                      <span className="text-zinc-200 tabular-nums">
                        {c.native_reserve_formatted} {c.native_symbol}
                      </span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-zinc-500">USDC</span>
                      <span className="text-zinc-200 tabular-nums">{c.usdc_reserve_formatted}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-zinc-500">Deposits / Charters</span>
                      <span className="text-zinc-300 tabular-nums">
                        {c.deposits_count} / {c.anchored_charters_count}
                      </span>
                    </div>
                  </div>
                  <div className="mt-3 flex items-center gap-3 border-t border-white/[0.06] pt-2.5 text-[11px] font-medium text-zinc-400">
                    <a
                      href={c.explorer_url}
                      target="_blank"
                      rel="noreferrer"
                      className="inline-flex items-center gap-0.5 hover:text-zinc-100"
                    >
                      <span>Explorer</span>
                      <ArrowUpRight className="h-3 w-3" />
                    </a>
                    <a
                      href={c.sourcify_url}
                      target="_blank"
                      rel="noreferrer"
                      className="inline-flex items-center gap-0.5 hover:text-zinc-100"
                    >
                      <span>Sourcify</span>
                      <ArrowUpRight className="h-3 w-3" />
                    </a>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </Panel>

      {/* 2. Sovereign Charter Forge + Open Directories */}
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-12">
        <div className="lg:col-span-7">
          <Panel
            title="Sovereign Charter Forge"
            subtitle="Compile a covenant-bound Web4 business with Lean 4 monotonic spending bounds & ERC-8004 card"
          >
            <form onSubmit={handleCompileCharter} className="space-y-4">
              <div className="grid grid-cols-1 gap-3.5 sm:grid-cols-3">
                <div className="sm:col-span-2">
                  <label className={LABEL_CLASS}>Business / Agent Name</label>
                  <input
                    type="text"
                    required
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder="e.g. Solar Compute Co-op"
                    className={INPUT_CLASS}
                  />
                </div>
                <div>
                  <label className={LABEL_CLASS}>Category</label>
                  <select
                    value={category}
                    onChange={(e) => setCategory(e.target.value as SpawnCharterPayload['category'])}
                    className={INPUT_CLASS}
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
                <label className={LABEL_CLASS}>
                  Constitutional Mandate{' '}
                  <span className="font-mono text-[10px] text-zinc-500">
                    (SHA-256 committed into constitution_hash)
                  </span>
                </label>
                <textarea
                  required
                  rows={2}
                  value={mission}
                  onChange={(e) => setMission(e.target.value)}
                  placeholder="Describe the exact revenue rail, verification rule, and reserve backing split…"
                  className={INPUT_CLASS}
                />
              </div>

              {/* Revenue Split Inputs + Clean Visual Allocation Bar */}
              <div className="rounded-xl border border-white/[0.07] bg-[#0a0a0d] p-4">
                <div className="mb-3 flex items-center justify-between text-[11px]">
                  <span className="font-medium text-zinc-300">Revenue Split &amp; Spending Cap</span>
                  <span
                    className={`font-mono ${
                      bpsTotal === 10000 ? 'text-emerald-400' : 'text-red-400'
                    }`}
                  >
                    {(bpsTotal / 100).toFixed(1)}% / 100%
                  </span>
                </div>

                {/* Minimalist 3-Segment Split Bar */}
                <div className="mb-4 flex h-1.5 w-full overflow-hidden rounded-full bg-white/[0.06]">
                  <div
                    className="bg-zinc-300 transition-all"
                    style={{ width: `${Math.min(100, (opsNum / 10000) * 100)}%` }}
                    title={`Ops: ${(opsNum / 100).toFixed(1)}%`}
                  />
                  <div
                    className="bg-zinc-500 transition-all"
                    style={{ width: `${Math.min(100, (reinvestNum / 10000) * 100)}%` }}
                    title={`Reinvest: ${(reinvestNum / 100).toFixed(1)}%`}
                  />
                  <div
                    className="bg-amber-400/90 transition-all"
                    style={{ width: `${Math.min(100, (backingNum / 10000) * 100)}%` }}
                    title={`Backing Vault: ${(backingNum / 100).toFixed(1)}%`}
                  />
                </div>

                <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
                  <div>
                    <label className={LABEL_CLASS}>Max / Call (LOVE)</label>
                    <input
                      type="number"
                      min={1}
                      required
                      value={maxSpend}
                      onChange={(e) => setMaxSpend(e.target.value)}
                      className={`${INPUT_CLASS} font-mono`}
                    />
                  </div>
                  <div>
                    <label className={LABEL_CLASS}>Ops (bps)</label>
                    <input
                      type="number"
                      min={0}
                      max={10000}
                      required
                      value={opsBps}
                      onChange={(e) => setOpsBps(e.target.value)}
                      className={`${INPUT_CLASS} font-mono`}
                    />
                  </div>
                  <div>
                    <label className={LABEL_CLASS}>Reinvest (bps)</label>
                    <input
                      type="number"
                      min={0}
                      max={10000}
                      required
                      value={reinvestBps}
                      onChange={(e) => setReinvestBps(e.target.value)}
                      className={`${INPUT_CLASS} font-mono`}
                    />
                  </div>
                  <div>
                    <label className="mb-1.5 block text-[11px] font-medium text-amber-300/90">
                      Backing (bps)
                    </label>
                    <input
                      type="number"
                      min={0}
                      max={10000}
                      required
                      value={backingBps}
                      onChange={(e) => setBackingBps(e.target.value)}
                      className={`${INPUT_CLASS} border-amber-400/25 font-mono`}
                    />
                  </div>
                </div>
              </div>

              <div className="grid grid-cols-1 gap-3.5 sm:grid-cols-2">
                <div>
                  <div className="mb-1.5 flex items-center justify-between">
                    <label className="text-[11px] font-medium text-zinc-400">
                      Newsatoshi Payout Address
                    </label>
                    <a
                      href="https://newsatoshi.org/wallet/"
                      target="_blank"
                      rel="noreferrer"
                      className="text-[10px] text-emerald-300 hover:underline"
                    >
                      Get ns1… wallet ↗
                    </a>
                  </div>
                  <input
                    type="text"
                    required
                    value={payoutLove}
                    onChange={(e) => setPayoutLove(e.target.value)}
                    placeholder="ns1… or nsd1…"
                    className={`${INPUT_CLASS} font-mono`}
                  />
                </div>
                <div>
                  <div className="mb-1.5 flex items-center justify-between">
                    <label className="text-[11px] font-medium text-zinc-400">
                      EVM Treasury Address
                    </label>
                    <button
                      type="button"
                      onClick={() => {
                        if (evmAddress) setEvmTreasury(evmAddress);
                        else openModal();
                      }}
                      className="inline-flex items-center gap-1 text-[10px] text-emerald-300 hover:underline cursor-pointer"
                    >
                      <Wallet className="h-3 w-3" />
                      <span>{evmAddress ? 'Use Connected' : 'Connect Wallet'}</span>
                    </button>
                  </div>
                  <input
                    type="text"
                    required
                    value={evmTreasury}
                    onChange={(e) => setEvmTreasury(e.target.value)}
                    placeholder="0x…"
                    className={`${INPUT_CLASS} font-mono`}
                  />
                </div>
              </div>

              <div className="grid grid-cols-1 gap-3.5 sm:grid-cols-2">
                <div>
                  <label className={LABEL_CLASS}>Anchor Network</label>
                  <select
                    value={evmChain}
                    onChange={(e) =>
                      setEvmChain(e.target.value as SpawnCharterPayload['primary_evm_chain'])
                    }
                    className={INPUT_CLASS}
                  >
                    <option value="base">Base Mainnet (8453)</option>
                    <option value="arbitrum">Arbitrum One (42161)</option>
                    <option value="optimism">Optimism (10)</option>
                    <option value="polygon">Polygon PoS (137)</option>
                  </select>
                </div>
                <div>
                  <label className={LABEL_CLASS}>Parent Charter ID (optional)</label>
                  <input
                    type="text"
                    value={parentCharterId}
                    onChange={(e) => setParentCharterId(e.target.value)}
                    placeholder="Enforces Lean 4 monotonic child bounds"
                    className={`${INPUT_CLASS} font-mono`}
                  />
                </div>
              </div>

              {compileError && (
                <div className="rounded-xl border border-red-500/30 bg-red-500/10 px-3.5 py-2.5 text-xs text-red-200">
                  {compileError}
                </div>
              )}

              <div className="flex flex-wrap items-center gap-3 pt-1">
                <button
                  type="submit"
                  disabled={compiling || bpsTotal !== 10000}
                  className="inline-flex items-center gap-2 rounded-full bg-zinc-50 px-5 py-2.5 text-xs font-semibold text-zinc-950 transition hover:bg-white cursor-pointer disabled:opacity-40"
                >
                  <span>{compiling ? 'Compiling…' : 'Compile Charter & ERC-8004'}</span>
                </button>
                {hasOperatorToken && (
                  <button
                    type="button"
                    disabled={publishing || bpsTotal !== 10000}
                    onClick={() => void handlePublishCharter()}
                    className="inline-flex items-center gap-2 rounded-full border border-white/[0.14] px-4 py-2.5 text-xs font-medium text-zinc-200 transition hover:border-white/[0.28] hover:text-white cursor-pointer disabled:opacity-40"
                  >
                    <span>{publishing ? 'Publishing…' : 'Publish to Registry'}</span>
                  </button>
                )}
              </div>
            </form>

            {compiledCharter && (
              <div className="mt-6 space-y-3 rounded-xl border border-white/[0.1] bg-[#0a0a0d] p-4 text-xs">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex items-center gap-2 font-mono font-medium text-zinc-100">
                    <CheckCircle2 className="h-4 w-4 text-emerald-400" />
                    <span>{compiledCharter.charter_id}</span>
                  </div>
                  <span className="font-mono text-[11px] text-zinc-400">
                    Depth {compiledCharter.lineage_depth} · {(compiledCharter.backing_bps / 100).toFixed(1)}% backing
                  </span>
                </div>

                <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                  <div className="flex items-center justify-between rounded-lg border border-white/[0.06] bg-[#0f0f12] px-3 py-2 text-[11px] text-zinc-500">
                    <span>Rule Hash</span>
                    <HashPill hash={compiledCharter.covenant_rule_hash} />
                  </div>
                  <div className="flex items-center justify-between rounded-lg border border-white/[0.06] bg-[#0f0f12] px-3 py-2 text-[11px] text-zinc-500">
                    <span>Charter Bytes32</span>
                    <HashPill hash={compiledCharter.charter_id_bytes32} />
                  </div>
                </div>

                <div className="space-y-1.5 rounded-lg border border-white/[0.06] bg-[#0f0f12] p-3">
                  <div className="flex items-center justify-between text-[11px] text-zinc-400">
                    <span>1. Anchor on {compiledCharter.primary_evm_chain}</span>
                    <CopyButton value={compiledCharter.cast_anchor_command} label="Copy cast" />
                  </div>
                  <code className="block break-all font-mono text-[11px] text-zinc-200">
                    {compiledCharter.cast_anchor_command}
                  </code>
                </div>

                <div className="space-y-1.5 rounded-lg border border-white/[0.06] bg-[#0f0f12] p-3">
                  <div className="flex items-center justify-between text-[11px] text-zinc-400">
                    <span>2. Run Local Autonomous 402 Daemon</span>
                    <CopyButton value={compiledCharter.web4_kit_launch_command} label="Copy cmd" />
                  </div>
                  <code className="block break-all font-mono text-[11px] text-zinc-200">
                    {compiledCharter.web4_kit_launch_command}
                  </code>
                </div>

                {compiledErc8004 && (
                  <div className="space-y-1.5 rounded-lg border border-white/[0.06] bg-[#0f0f12] p-3">
                    <div className="flex items-center justify-between text-[11px] text-zinc-400">
                      <span>3. ERC-8004 Registration JSON</span>
                      <CopyButton value={JSON.stringify(compiledErc8004, null, 2)} label="Copy JSON" />
                    </div>
                    <pre className="max-h-40 overflow-auto font-mono text-[10.5px] text-zinc-300">
                      {JSON.stringify(compiledErc8004, null, 2)}
                    </pre>
                  </div>
                )}
              </div>
            )}
          </Panel>
        </div>

        {/* Right Column: Live On-Chain Charter #1 + Honest Directories */}
        <div className="space-y-6 lg:col-span-5">
          <Panel
            title="Live Ecosystem Registries"
            subtitle="Strictly real charters, 402 services & bounties — zero synthetic entries"
            badge={
              <span className="font-mono text-[11px] text-zinc-500">
                {charters.length + 1} · {services.length} · {bounties.length}
              </span>
            }
          >
            <div className="space-y-4 text-xs">
              <div className="rounded-xl border border-white/[0.08] bg-[#0a0a0d] p-4">
                <div className="flex items-center justify-between">
                  <span className="inline-flex items-center gap-1.5 font-mono text-[10px] uppercase tracking-wider text-zinc-400">
                    <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
                    Genesis Charter #1 · Base
                  </span>
                  <a
                    href="/api/web4/charters/charter-genesis-commons/erc8004.json"
                    target="_blank"
                    rel="noreferrer"
                    className="inline-flex items-center gap-0.5 text-[11px] text-zinc-400 hover:text-zinc-100"
                  >
                    <span>ERC-8004</span>
                    <ArrowUpRight className="h-3 w-3" />
                  </a>
                </div>
                <p className="mt-2 font-medium text-zinc-100">
                  Newsatoshi Commons Verification Utility
                </p>
                <p className="mt-1 text-[11px] leading-relaxed text-zinc-500">
                  20% (2,000 bps) backing commitment anchored permanently on Base Mainnet ·{' '}
                  <a
                    href="https://basescan.org/tx/0x7a8183b60b6b1d8b46cc46b2eb022b3fa046f73b9f1f7ee464a5a3415815eb47"
                    target="_blank"
                    rel="noreferrer"
                    className="text-zinc-300 underline decoration-white/20 underline-offset-2 hover:text-white"
                  >
                    Anchor TX
                  </a>
                </p>
              </div>

              <div>
                <h3 className="mb-2 text-[11px] font-medium uppercase tracking-wider text-zinc-500">
                  Paid 402 Agent Services ({services.length})
                </h3>
                {services.length === 0 ? (
                  <div className="rounded-xl border border-dashed border-white/[0.08] px-4 py-4 text-[11px] leading-relaxed text-zinc-500">
                    No external services registered yet. Test the live PayWord-gated utility route
                    at{' '}
                    <code className="font-mono text-zinc-300">
                      POST /api/web4/tools/verify-payword-batch
                    </code>
                    .
                  </div>
                ) : (
                  <div className="space-y-2">
                    {services.map((s) => (
                      <div
                        key={s.service_id}
                        className="rounded-xl border border-white/[0.07] bg-[#0a0a0d] p-3"
                      >
                        <div className="flex items-center justify-between font-medium text-zinc-100">
                          <span>{s.name}</span>
                          <span className="font-mono text-[11px] text-zinc-400">
                            {s.unit_price_love_base_units} units/call
                          </span>
                        </div>
                        <p className="mt-1 text-[11px] text-zinc-500">{s.description}</p>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              <div>
                <h3 className="mb-2 text-[11px] font-medium uppercase tracking-wider text-zinc-500">
                  Zero-Custody HTLC Bounties ({bounties.length})
                </h3>
                {bounties.length === 0 ? (
                  <div className="rounded-xl border border-dashed border-white/[0.08] px-4 py-4 text-[11px] leading-relaxed text-zinc-500">
                    No open bounties posted yet. Compile trustless escrow commands via{' '}
                    <code className="font-mono text-zinc-300">POST /api/web4/bounties/compile</code>
                    .
                  </div>
                ) : (
                  <div className="space-y-2">
                    {bounties.map((b) => (
                      <div
                        key={b.bounty_id}
                        className="rounded-xl border border-white/[0.07] bg-[#0a0a0d] p-3"
                      >
                        <div className="flex items-center justify-between font-medium text-zinc-100">
                          <span>{b.title}</span>
                          <span className="font-mono text-[11px] text-zinc-300">
                            {b.reward_amount} {b.reward_asset}
                          </span>
                        </div>
                        <p className="mt-1 text-[11px] text-zinc-500">{b.description}</p>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          </Panel>

          <Panel
            title="Run Your Own Web4 Daemon"
            subtitle="Zero-dependency Node 18+ PayWord + x402 starter kit"
          >
            <div className="space-y-3">
              <div className="flex items-center justify-between rounded-xl border border-white/[0.07] bg-[#0a0a0d] px-3.5 py-2.5">
                <code className="font-mono text-xs text-zinc-200">
                  node web4-kit/agent-business.mjs --check
                </code>
                <CopyButton value="node web4-kit/agent-business.mjs --check" />
              </div>
              <div className="grid grid-cols-2 gap-2 text-xs">
                {[
                  { label: 'Whitepaper', href: '/whitepaper' },
                  { label: 'Lean 4 Kernel', href: '/formal/NewsatoshiCovenants.lean' },
                  { label: 'ERC-8004 Card', href: '/.well-known/erc8004.json' },
                  { label: 'MCP Endpoint', href: '/mcp' },
                ].map((link) => (
                  <a
                    key={link.href}
                    href={link.href}
                    target="_blank"
                    rel="noreferrer"
                    className="flex items-center justify-between rounded-xl border border-white/[0.07] bg-[#0a0a0d] px-3 py-2 text-[11px] font-medium text-zinc-300 transition hover:border-white/[0.18] hover:text-white"
                  >
                    <span>{link.label}</span>
                    <ArrowUpRight className="h-3 w-3 text-zinc-500" />
                  </a>
                ))}
              </div>
            </div>
          </Panel>
        </div>
      </div>
    </div>
  );
}
