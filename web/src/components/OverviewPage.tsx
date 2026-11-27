import { useState } from 'react';
import { ArrowRight, ArrowUpRight, Check, ExternalLink, Layers, Loader2, Play, ShieldCheck, Zap } from 'lucide-react';
import type { BackingProofSummary, PrimaryView, ProtocolSubTab, SeedStatusResponse } from '../types';
import { formatL1IssuedLove } from '../api/normalize';
import type { SwapHubSection } from './SwapsMiningTab';
import { CopyButton, truncateHash } from './ui/CopyButton';

interface NsUsdStressResult {
  ok: boolean;
  sandbox_only: boolean;
  crash_pct_simulated: number;
  all_invariants_passed: boolean;
  scenarios: {
    '1_flash_crash_liquidation': {
      initial_btc_usd: number;
      crashed_btc_usd: number;
      vault_a_liquidated: boolean;
      satoshi_conservation_exact: boolean;
      post_liquidation_system_cr_bps: number;
    };
    '2_bank_run_redemption_floor': {
      remaining_debt_nsusd_micro: number;
      exact_conservation: boolean;
    };
    '3_priorlock_oracle_spoof_slash': {
      exact_50_50_bounty_burn: boolean;
      receipt: {
        deviation_bps: number;
        challenger_bounty_love_sats: number;
        burned_love_sats: number;
      };
    };
    '4_stale_oracle_circuit_breaker': {
      mint_halted: boolean;
    };
  };
}

interface L2BenchmarkResult {
  ok: boolean;
  sandbox_benchmark: boolean;
  tx_count: number;
  conservation_exact: boolean;
  payword_execution_us: number;
  measured_offchain_tps: number;
  new_l2_state_root_hex: string;
  winterfell_stark_batch_proof: {
    proof_id: string;
    verified: boolean;
    proof_bytes: number;
    security_bits: number;
    prove_ms: number;
    verify_ms: number;
  };
  unilateral_l1_escape_hatch?: {
    verified: boolean;
    leaf_index: number;
    siblings_hex: string[];
  };
  l1_mainnet_v2_anchor: {
    l1_height: number | null;
    anchor_commitment_hex: string;
  };
}

interface PaywallKitResult {
  ok: boolean;
  framework: string;
  filename: string;
  install_cmd: string;
  price_usd: string;
  micro_usd_ticks: number;
  code: string;
  error?: string;
}

interface BaseEscrowVerifyResult {
  ok: boolean;
  verified_onchain?: boolean;
  block_number?: number;
  verified_usdc_formatted?: string;
  deposit_path?: string;
  basescan_url?: string;
  receipt_commitment_sha256?: string;
  nsusd_b_asset_id?: number;
  error?: string;
}

interface OverviewPageProps {
  seed: SeedStatusResponse | null;
  backing: BackingProofSummary | null;
  onNavigate: (
    view: PrimaryView,
    protocolTab?: ProtocolSubTab,
    swapSection?: SwapHubSection
  ) => void;
  onNotice: (kind: 'success' | 'error', message: string) => void;
}

type SnippetTab = 'mcp' | 'web4' | 'lean' | 'cli';

const SNIPPETS: Record<
  SnippetTab,
  { label: string; caption: string; code: string }
> = {
  mcp: {
    label: 'MCP Server',
    caption: '35 zero-auth L2 gateway & crypto tools (60s private x402 paywall kit, Base USDC escrow verifier, nsUSD stress, L2 STARK rollup, EVM audit, BTC mempool, BOLT-11)',
    code: `{
  "mcpServers": {
    "newsatoshi": {
      "url": "https://commons.newsatoshi.org/mcp"
    }
  }
}`,
  },
  web4: {
    label: 'Agent Kit',
    caption: 'Zero-dependency Node 18+ L2 agent commerce daemon (Native LOVE PayWord primary + x402 USDC bridge + ERC-8004)',
    code: `git clone https://github.com/theguysaccount/newsatoshi-commons.git
cd newsatoshi-commons

# Verify live 4-chain Proof-of-Backing & survival tier
node web4-kit/agent-business.mjs --check

# Start your paid Native LOVE PayWord + x402 bridge API daemon
node web4-kit/agent-business.mjs`,
  },
  lean: {
    label: 'Lean 4 Checks',
    caption: 'Re-verify L2 covenant theorems + L1 full 249-constraint SHA-256 spend AIR proof (SpendCircuitFull.lean, 0 sorry, 0 native_decide)',
    code: `curl -fsSL https://commons.newsatoshi.org/formal/NewsatoshiCovenants.lean \\
  -o /tmp/NewsatoshiCovenants.lean

lean /tmp/NewsatoshiCovenants.lean
# L1 repo also verifies ALL 249 live Rust spend constraints:
# lean node/proof/SpendCircuitFull.lean  (0 sorry, pure kernel)`,
  },
  cli: {
    label: 'Install CLI',
    caption: 'Downloads GitHub Release binaries for macOS & Linux (commons, swap, hw-signer; inspect install.sh or build from source)',
    code: `curl -fsSL https://commons.newsatoshi.org/install.sh -o /tmp/ns-install.sh
less /tmp/ns-install.sh && sh /tmp/ns-install.sh

# Verify live mainnet & compile a trustless SHA-256 atomic swap
newsatoshi-swap keygen`,
  },
};

const CANONICAL_CONTRACTS = [
  {
    name: 'NewsatoshiBackingVault',
    role: 'L2/EVM · Ownerless multi-chain reserve & charter anchor (0 withdrawal path)',
    address: '0x37Ed94525DBE408fA72cDd156409875666679265',
    badge: '4 Chains · L2 Vault',
    explorers: [
      { label: 'Base', url: 'https://basescan.org/address/0x37Ed94525DBE408fA72cDd156409875666679265' },
      { label: 'Arbitrum', url: 'https://arbiscan.io/address/0x37Ed94525DBE408fA72cDd156409875666679265' },
      { label: 'Optimism', url: 'https://optimistic.etherscan.io/address/0x37Ed94525DBE408fA72cDd156409875666679265' },
      { label: 'Polygon', url: 'https://polygonscan.com/address/0x37Ed94525DBE408fA72cDd156409875666679265' },
    ],
    sourcify: 'https://repo.sourcify.dev/8453/0x37Ed94525DBE408fA72cDd156409875666679265',
  },
  {
    name: 'NewsatoshiHtlc',
    role: 'L2/EVM Bridge · Zero-custody SHA-256 atomic swap rail into native LOVE (EVM 0x02 precompile)',
    address: '0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987',
    badge: '4/4 Sourcify Exact Match',
    explorers: [
      { label: 'Base', url: 'https://basescan.org/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987' },
      { label: 'Arbitrum', url: 'https://arbiscan.io/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987' },
      { label: 'Optimism', url: 'https://optimistic.etherscan.io/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987' },
      { label: 'Polygon', url: 'https://polygonscan.com/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987' },
    ],
    sourcify: 'https://repo.sourcify.dev/8453/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987',
  },
  {
    name: 'ERC-8004 On-Chain Agent Identity',
    role: 'L2 Identity · Registered across 4 mainnets: Base #1229992 · Arbitrum #1692 · Optimism #42519 · Polygon #715',
    address: '0x8004A169FB4a3325136EB29fA0ceB6D2e539a432',
    badge: '4 Chains Indexed',
    explorers: [
      { label: 'Base #1229992', url: 'https://basescan.org/tx/0x0eda4629e7976d29edeccaef5bae3453e8fe434a66c77e6443b8047257468bbe' },
      { label: 'Arb #1692', url: 'https://arbiscan.io/tx/0x5f915f240194e285c97f874ab25c569a87fa0279a6f59f7c0c3cf1634ad351e2' },
      { label: 'OP #42519', url: 'https://optimistic.etherscan.io/tx/0x0a469889afe20774a1210c4fd93f27924c0c7043884b1177e51d22506bc27ac6' },
      { label: 'Pol #715', url: 'https://polygonscan.com/tx/0x2d097bf8a65e48a283c75c1f6771d0401cc501921f841fc14463227c6d5cfe9a' },
    ],
    sourcify: 'https://commons.newsatoshi.org/.well-known/erc8004.json',
  },
] as const;

export function OverviewPage({ seed, backing, onNavigate, onNotice }: OverviewPageProps) {
  const [snippetTab, setSnippetTab] = useState<SnippetTab>('mcp');
  const [stressBusy, setStressBusy] = useState(false);
  const [stressResult, setStressResult] = useState<NsUsdStressResult | null>(null);
  const [l2Busy, setL2Busy] = useState(false);
  const [l2Result, setL2Result] = useState<L2BenchmarkResult | null>(null);
  const [kitFw, setKitFw] = useState<'express' | 'nextjs' | 'fastapi' | 'axum'>('express');
  const [kitUsd, setKitUsd] = useState('0.005');
  const [kitRoute, setKitRoute] = useState('/api/inference');
  const [kitBusy, setKitBusy] = useState(false);
  const [kitResult, setKitResult] = useState<PaywallKitResult | null>(null);
  const [escrowTx, setEscrowTx] = useState(
    '0xd3c521e3dcdd1465f6f51384a5c721c0c445c3d384282ae0c67b2b07195029ba'
  );
  const [escrowBusy, setEscrowBusy] = useState(false);
  const [escrowResult, setEscrowResult] = useState<BaseEscrowVerifyResult | null>(null);
  const ms = seed?.mainnet_live ? seed.mainnet_status : null;
  const activeSnippet = SNIPPETS[snippetTab];

  const generatePaywallKit = async (fwOverride?: 'express' | 'nextjs' | 'fastapi' | 'axum') => {
    const fw = fwOverride ?? kitFw;
    if (fwOverride) setKitFw(fwOverride);
    setKitBusy(true);
    try {
      const resp = await fetch('/api/v2/x402/paywall-kit', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          framework: fw,
          price_usd: Number.parseFloat(kitUsd) || 0.005,
          route_path: kitRoute.trim() || '/api/inference',
        }),
      });
      const data = (await resp.json().catch(() => null)) as PaywallKitResult | null;
      if (!resp.ok || !data) {
        onNotice('error', data?.error || `Paywall kit request failed (HTTP ${resp.status})`);
        return;
      }
      setKitResult(data);
      onNotice(
        data.ok ? 'success' : 'error',
        data.ok
          ? `Generated ${data.framework} private x402 + PayWords middleware ($${data.price_usd}/call)`
          : data.error || 'Paywall kit generation failed'
      );
    } catch (err) {
      onNotice('error', err instanceof Error ? err.message : 'Paywall kit request failed');
    } finally {
      setKitBusy(false);
    }
  };

  const verifyBaseEscrow = async (txOverride?: string) => {
    const hash = (txOverride ?? escrowTx).trim();
    if (txOverride) setEscrowTx(txOverride);
    setEscrowBusy(true);
    try {
      const resp = await fetch('/api/v2/nsusd/verify-base-escrow', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ tx_hash: hash }),
      });
      const data = (await resp.json().catch(() => null)) as BaseEscrowVerifyResult | null;
      if (!resp.ok || !data) {
        const msg = data?.error || `Base escrow verification failed (HTTP ${resp.status})`;
        setEscrowResult({ ok: false, error: msg });
        onNotice('error', msg);
        return;
      }
      setEscrowResult(data);
      onNotice(
        data.ok ? 'success' : 'error',
        data.ok
          ? `Verified $${data.verified_usdc_formatted} USDC on Base block #${data.block_number}`
          : data.error || 'Base escrow verification failed'
      );
    } catch (err) {
      onNotice('error', err instanceof Error ? err.message : 'Base RPC request failed');
    } finally {
      setEscrowBusy(false);
    }
  };

  const runNsUsdStress = async () => {
    setStressBusy(true);
    try {
      const resp = await fetch('/api/v2/nsusd/simulate-stress', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ crash_pct: 50 }),
      });
      const data = (await resp.json().catch(() => null)) as NsUsdStressResult | null;
      if (!resp.ok || !data || !data.scenarios) {
        onNotice('error', `nsUSD stress test failed (HTTP ${resp.status})`);
        return;
      }
      setStressResult(data);
      onNotice(
        data.all_invariants_passed ? 'success' : 'error',
        data.all_invariants_passed
          ? 'nsUSD -50% BTC crash stress suite: 4/4 invariants verified (isolated sandbox)'
          : 'nsUSD stress invariant check failed'
      );
    } catch (err) {
      onNotice('error', err instanceof Error ? err.message : 'Stress test request failed');
    } finally {
      setStressBusy(false);
    }
  };

  const runL2Benchmark = async () => {
    setL2Busy(true);
    try {
      const resp = await fetch('/api/v2/l2/benchmark', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ tx_count: 2000 }),
      });
      const data = (await resp.json().catch(() => null)) as L2BenchmarkResult | null;
      if (!resp.ok || !data || !data.ok || !data.winterfell_stark_batch_proof) {
        onNotice('error', `L2 benchmark failed (HTTP ${resp.status})`);
        return;
      }
      setL2Result(data);
      const verified = Boolean(data.winterfell_stark_batch_proof.verified);
      const tps = Number(data.measured_offchain_tps ?? 0).toLocaleString();
      onNotice(
        verified ? 'success' : 'error',
        verified
          ? `L2 Sandbox Batch: ${tps} off-chain TPS + Winterfell STARK verified in ${data.winterfell_stark_batch_proof.verify_ms}ms`
          : `L2 Sandbox Batch: ${tps} off-chain TPS, but Winterfell STARK verification FAILED`
      );
    } catch (err) {
      onNotice('error', err instanceof Error ? err.message : 'L2 benchmark request failed');
    } finally {
      setL2Busy(false);
    }
  };

  return (
    <div className="animate-fade-in mx-auto max-w-[1120px] space-y-16 pb-20 pt-8 sm:pt-12">
      {/* 1. Plain-English Hourglass Hero + 1-Click Bridge/Discovery Card */}
      <section className="grid grid-cols-1 items-start gap-8 lg:grid-cols-12 lg:gap-10">
        <div className="lg:col-span-7">
          <div className="inline-flex flex-wrap items-center gap-2 rounded-full border border-white/[0.09] bg-white/[0.025] px-3 py-1 text-[11px] font-medium tracking-wide text-zinc-400">
            <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
            <span className="font-mono uppercase text-zinc-200">Mainnet v2 · L1 v1.2.5</span>
            <span className="text-zinc-600">·</span>
            <span>Native LOVE &amp; nsUSD Gateway</span>
          </div>

          <h1 className="font-display mt-6 text-[2.6rem] leading-[1.03] tracking-[-0.02em] text-zinc-50 sm:text-[3.85rem]">
            Private, quantum-proof cash{' '}
            <span className="italic text-zinc-400">for humans &amp; autonomous agents.</span>
          </h1>

          <p className="mt-5 max-w-xl text-[15px] leading-[1.7] text-zinc-400 sm:text-[16px]">
            <span className="font-mono text-[14px] text-zinc-200">Newsatoshi (LOVE)</span> is
            sovereign Layer-1 cash — default shielded privacy via pure FIPS SHA-2/SHA-384 STARKs,
            mechanized in Lean 4 (all 249 constraints, 114,688 periodic cells &amp; BatchSpend K=2..16),
            and merge-mined by Bitcoin ASICs (<span className="font-mono text-[13px] text-zinc-300">chainid 20051</span>).
            Above the waist, this gateway delivers stable USD settlement (<span className="font-mono text-[13px] text-sky-300">nsUSD</span>),
            a 60-second private x402 paywall kit, 35 zero-auth MCP tools, sub-millisecond PayWords, and trustless BTC/Lightning/EVM HTLC bridges.
          </p>

          <div className="mt-8 flex flex-wrap items-center gap-3">
            <button
              type="button"
              onClick={() => onNavigate('bridge', undefined, 'setup')}
              className="inline-flex items-center gap-2 rounded-full bg-zinc-50 px-5 py-2.5 text-[13px] font-semibold text-zinc-950 transition hover:bg-white cursor-pointer"
            >
              <span>Mine Real LOVE</span>
              <ArrowRight className="h-4 w-4" />
            </button>

            <a
              href="/whitepaper.pdf"
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-2 rounded-full border border-white/[0.12] bg-white/[0.03] px-5 py-2.5 text-[13px] font-medium text-zinc-200 transition hover:border-white/[0.24] hover:bg-white/[0.06]"
            >
              <span>Whitepaper PDF</span>
              <ArrowUpRight className="h-3.5 w-3.5" />
            </a>

            <a
              href="/nsusd"
              className="inline-flex items-center gap-1.5 rounded-full border border-sky-500/30 bg-sky-500/10 px-4 py-2.5 text-[13px] font-medium text-sky-200 transition hover:bg-sky-500/20"
            >
              <span>nsUSD Stable Rail</span>
              <ArrowUpRight className="h-3.5 w-3.5" />
            </a>

            <a
              href="/l2"
              className="inline-flex items-center gap-1.5 rounded-full border border-violet-500/30 bg-violet-500/10 px-4 py-2.5 text-[13px] font-medium text-violet-200 transition hover:bg-violet-500/20"
            >
              <span>L2 Perfection</span>
              <ArrowUpRight className="h-3.5 w-3.5" />
            </a>
          </div>
        </div>

        <div className="lg:col-span-5">
          <div className="surface-card rounded-2xl border border-emerald-500/20 p-6 space-y-5">
            <div className="flex items-center justify-between gap-2">
              <div className="inline-flex items-center gap-2 font-mono text-[11px] uppercase tracking-[0.1em] text-emerald-300">
                <span className="h-2 w-2 rounded-full bg-emerald-400" />
                <span>Native Mainnet v2 · One Real LOVE</span>
              </div>
              <span className="rounded-full border border-white/[0.1] bg-white/[0.04] px-2.5 py-0.5 font-mono text-[10px] text-zinc-300">
                0% Premine
              </span>
            </div>

            <div className="space-y-3">
              <div className="rounded-xl border border-white/[0.07] bg-[#0a0a0d] p-3.5">
                <div className="flex items-center justify-between text-[11px] text-zinc-400">
                  <span className="font-medium text-zinc-200">1. Mine Native LOVE (0% Fee Pool)</span>
                  <CopyButton value="stratum+tcp://pool.newsatoshi.org:3333" />
                </div>
                <code className="mt-1.5 block truncate font-mono text-xs text-emerald-300">
                  stratum+tcp://pool.newsatoshi.org:3333
                </code>
                <p className="mt-1 text-[11px] text-zinc-500">
                  Point any Bitaxe or SHA-256 ASIC · direct shielded coinbase payouts (`50 LOVE/block`).
                </p>
              </div>

              <div className="rounded-xl border border-sky-500/20 bg-sky-500/[0.04] p-3.5">
                <div className="flex items-center justify-between text-[11px] text-sky-200">
                  <span className="font-medium">2. Price &amp; Settle in Stable USD (nsUSD)</span>
                  <a href="/nsusd" className="inline-flex items-center gap-0.5 font-mono text-[10px] text-sky-300 hover:text-white">
                    <span>RFC-0028</span>
                    <ArrowUpRight className="h-3 w-3" />
                  </a>
                </div>
                <div className="mt-1.5 flex items-center justify-between gap-2">
                  <code className="truncate font-mono text-[11px] text-zinc-200">
                    GET /api/v2/nsusd/quote?usd=0.01
                  </code>
                  <CopyButton value="https://commons.newsatoshi.org/api/v2/nsusd/quote?usd=0.01" />
                </div>
                <p className="mt-1 text-[11px] text-zinc-400">
                  Zero-volatility USD quotes over O(1) PayWords + sovereign BTC-backed nsUSD roadmap.
                </p>
              </div>

              <div className="rounded-xl border border-white/[0.07] bg-[#0a0a0d] p-3.5">
                <div className="flex items-center justify-between text-[11px] text-zinc-300">
                  <span className="font-medium">3. Verify Math or Claim Bug Bounty</span>
                  <a href="/bounty" className="inline-flex items-center gap-0.5 font-mono text-[10px] text-emerald-300 hover:text-white">
                    <span>/bounty</span>
                    <ArrowUpRight className="h-3 w-3" />
                  </a>
                </div>
                <p className="mt-1 text-[11px] text-zinc-500">
                  Run the live browser SHA-384 STARK verifier on{' '}
                  <a href="https://newsatoshi.org/" target="_blank" rel="noreferrer" className="text-zinc-300 underline hover:text-white">
                    newsatoshi.org
                  </a>{' '}
                  or bridge trustlessly via BTC/Lightning/EVM SHA-256 HTLCs.
                </p>
              </div>
            </div>

            <div className="flex items-center justify-between gap-2 pt-1 text-[11px]">
              <button
                type="button"
                onClick={() => onNavigate('bridge', undefined, 'bridge')}
                className="inline-flex items-center gap-1 font-medium text-zinc-200 hover:text-white cursor-pointer"
              >
                <span>Open Native L1 Atomic Bridge</span>
                <ArrowRight className="h-3.5 w-3.5" />
              </button>
              <a
                href="https://newsatoshi.org/wallet/"
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 font-mono text-emerald-300 hover:text-emerald-200"
              >
                <span>Shielded Wallet</span>
                <ArrowUpRight className="h-3 w-3" />
              </a>
            </div>
          </div>
        </div>
      </section>

      {/* 2. Live Proof-of-Reality Strip (Honest Status, Zero Fake Data) */}
      <section aria-label="Live network metrics">
        <div className="grid grid-cols-1 divide-y divide-white/[0.07] overflow-hidden rounded-2xl border border-white/[0.08] bg-[#0f0f12] sm:grid-cols-2 sm:divide-y-0 lg:grid-cols-4 lg:divide-x">
          <button
            type="button"
            onClick={() => onNavigate('protocol', 'explorer')}
            className="group p-5 text-left transition hover:bg-white/[0.02] cursor-pointer sm:p-6"
          >
            <div className="flex items-center justify-between text-[11px] font-medium uppercase tracking-[0.08em] text-zinc-500">
              <span>Mainnet v2 (nsM2)</span>
              <span
                className={`inline-flex items-center gap-1.5 font-mono text-[10px] ${
                  ms ? 'text-emerald-300' : 'text-amber-300'
                }`}
              >
                <span
                  className={`h-1.5 w-1.5 rounded-full ${
                    ms ? 'bg-emerald-400' : 'bg-amber-400 animate-pulse'
                  }`}
                />
                {ms ? 'Live' : 'Syncing'}
              </span>
            </div>
            <div className="mt-2.5 font-mono text-2xl font-medium tracking-tight text-zinc-100 tabular-nums">
              {ms ? `#${ms.height.toLocaleString()}` : '—'}
            </div>
            <div className="mt-1.5 text-xs text-zinc-500">
              {ms
                ? `Diff ${ms.difficulty.toFixed(0)} · ${formatL1IssuedLove(ms.issued)} LOVE · BTC #977,738 genesis`
                : 'AuxPoW chainid 20051 · Mainnet v2'}
            </div>
          </button>

          <a
            href="/bounty"
            className="group block p-5 text-left transition hover:bg-white/[0.02] sm:border-l sm:border-white/[0.07] sm:p-6 lg:border-l-0"
          >
            <div className="flex items-center justify-between text-[11px] font-medium uppercase tracking-[0.08em] text-zinc-500">
              <span>Security Bounty</span>
              <span className="font-mono text-[10px] text-emerald-400">Open in LOVE</span>
            </div>
            <div className="mt-2.5 font-mono text-2xl font-medium tracking-tight text-zinc-100">
              LOVE Bug Bounty
            </div>
            <div className="mt-1.5 text-xs text-zinc-500">
              Break the math · claim mined LOVE · AUDIT_SCOPE_V2.md
            </div>
          </a>

          <a
            href="https://github.com/theguysaccount/newsatoshi-rfc0021/blob/main/docs/AUDIT_SCOPE_V2.md"
            target="_blank"
            rel="noreferrer"
            className="group block p-5 text-left transition hover:bg-white/[0.02] sm:border-t sm:border-white/[0.07] sm:p-6 lg:border-t-0"
          >
            <div className="flex items-center justify-between text-[11px] font-medium uppercase tracking-[0.08em] text-zinc-500">
              <span>L1 Verification</span>
              <ArrowUpRight className="h-3.5 w-3.5 text-zinc-600 transition group-hover:text-zinc-300" />
            </div>
            <div className="mt-2.5 font-mono text-2xl font-medium tracking-tight text-zinc-100">
              104b QROM · Lean 4 IFF
            </div>
            <div className="mt-1.5 text-xs text-zinc-500">
              249 AIR + 114,688 cells + BatchSpend K=2..16 (0 sorry)
            </div>
          </a>

          <div className="p-5 sm:border-l sm:border-t sm:border-white/[0.07] sm:p-6 lg:border-l-0 lg:border-t-0">
            <div className="flex items-center justify-between text-[11px] font-medium uppercase tracking-[0.08em] text-zinc-500">
              <span>L2 Agent Gateway</span>
              <span className="font-mono text-[10px] text-emerald-400">MCP Registry 0.28.4</span>
            </div>
            <div className="mt-2.5 font-mono text-2xl font-medium tracking-tight text-zinc-100">
              35 Live Tools
            </div>
            <div className="mt-1.5 flex items-center justify-between gap-2 text-xs text-zinc-500">
              <span className="truncate font-mono text-[11px]">commons.newsatoshi.org/mcp</span>
              <CopyButton value="https://commons.newsatoshi.org/mcp" />
            </div>
          </div>
        </div>
      </section>

      {/* 2B. Live nsUSD Stablecoin & L2 Perfection Layer Interactive Verification */}
      <section aria-label="Live nsUSD and L2 Perfection Verification">
        <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
          <div>
            <p className="font-mono text-[11px] uppercase tracking-[0.12em] text-sky-300/90">
              Live Cryptographic Lab · Zero Fake TVL (Isolated Sandboxes)
            </p>
            <h2 className="font-display mt-1.5 text-3xl tracking-tight text-zinc-100 sm:text-4xl">
              Verify nsUSD stability &amp; L2 STARK rollup perfection live.
            </h2>
          </div>
          <div className="flex items-center gap-3 text-xs">
            <a href="/nsusd" className="inline-flex items-center gap-1 font-mono text-sky-300 hover:text-white">
              <span>RFC-0028 Spec</span>
              <ArrowUpRight className="h-3.5 w-3.5" />
            </a>
            <a href="/l2" className="inline-flex items-center gap-1 font-mono text-violet-300 hover:text-white">
              <span>L2 Architecture</span>
              <ArrowUpRight className="h-3.5 w-3.5" />
            </a>
          </div>
        </div>

        <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
          {/* Card A: RFC-0028 nsUSD Adversarial Stress Test */}
          <div className="rounded-2xl border border-sky-500/20 bg-sky-500/[0.03] p-6 space-y-4">
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="inline-flex items-center gap-2 font-mono text-[11px] uppercase tracking-[0.08em] text-sky-300">
                  <ShieldCheck className="h-4 w-4" />
                  <span>RFC-0028 · Dual-Tier nsUSD Engine</span>
                </div>
                <h3 className="mt-2 text-lg font-medium text-zinc-100">
                  Sovereign BTC CDP (`asset_id=1`) &amp; PriorLock Oracle
                </h3>
              </div>
              <span className="shrink-0 rounded-full border border-sky-500/25 bg-sky-500/10 px-2.5 py-0.5 font-mono text-[10px] text-sky-200">
                0 Freeze Switch
              </span>
            </div>

            <p className="text-[13px] leading-relaxed text-zinc-400">
              Live vault ledger starts at honest <code className="font-mono text-zinc-200">0</code> debt.
              Run the real Rust integer engine in an isolated sandbox to verify a{' '}
              <strong className="text-zinc-200">-50% BTC flash crash</strong>, direct{' '}
              <strong className="text-zinc-200">$1.00 redemption floor</strong>,{' '}
              <strong className="text-zinc-200">+40% oracle spoof 50/50 slash</strong>, and stale-feed mint halt.
            </p>

            <div className="flex flex-wrap items-center gap-3">
              <button
                type="button"
                onClick={() => void runNsUsdStress()}
                disabled={stressBusy}
                className="inline-flex items-center gap-2 rounded-full bg-sky-400 px-4 py-2 text-xs font-semibold text-zinc-950 transition hover:bg-sky-300 cursor-pointer disabled:opacity-60"
              >
                {stressBusy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Play className="h-3.5 w-3.5" />}
                <span>Run -50% BTC Crash Stress Test</span>
              </button>
              <a
                href="/api/v2/nsusd/state"
                target="_blank"
                rel="noreferrer"
                className="font-mono text-[11px] text-zinc-400 hover:text-zinc-200"
              >
                GET /api/v2/nsusd/state
              </a>
            </div>

            {stressResult && (
              <div className="rounded-xl border border-sky-500/25 bg-[#090c12] p-4 font-mono text-[11px] space-y-1.5 text-zinc-300">
                <div
                  className={`flex items-center justify-between ${
                    stressResult.all_invariants_passed ? 'text-emerald-300' : 'text-rose-300'
                  }`}
                >
                  <span>ALL 4 ADVERSARIAL INVARIANTS</span>
                  <span>{stressResult.all_invariants_passed ? 'PASS (4/4)' : 'FAIL'}</span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-zinc-400">1. -{stressResult.crash_pct_simulated}% BTC Crash Liquidation</span>
                  <span>
                    {stressResult.scenarios['1_flash_crash_liquidation'].vault_a_liquidated ? 'LIQUIDATED' : 'HELD'} ·{' '}
                    CR {(stressResult.scenarios['1_flash_crash_liquidation'].post_liquidation_system_cr_bps / 100).toFixed(1)}%
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-zinc-400">2. $1.00 Bank-Run Redemption</span>
                  <span>
                    {stressResult.scenarios['2_bank_run_redemption_floor'].exact_conservation ? 'EXACT SATOSHI CONSERVATION' : 'ERR'}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-zinc-400">3. +40% PriorLock Oracle Spoof</span>
                  <span>
                    {stressResult.scenarios['3_priorlock_oracle_spoof_slash'].exact_50_50_bounty_burn
                      ? `SLASHED (${stressResult.scenarios['3_priorlock_oracle_spoof_slash'].receipt.deviation_bps} bps · 50% burn)`
                      : 'SLASH CHECK FAILED'}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-zinc-400">4. Stale Oracle Circuit Breaker</span>
                  <span>
                    {stressResult.scenarios['4_stale_oracle_circuit_breaker'].mint_halted ? 'MINT HALTED' : 'OPEN'}
                  </span>
                </div>
              </div>
            )}
          </div>

          {/* Card B: L2 Perfection Layer Live Winterfell STARK Batch Benchmark */}
          <div className="rounded-2xl border border-violet-500/20 bg-violet-500/[0.03] p-6 space-y-4">
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="inline-flex items-center gap-2 font-mono text-[11px] uppercase tracking-[0.08em] text-violet-300">
                  <Zap className="h-4 w-4" />
                  <span>L2A–L2D · Shielded Rollup &amp; Perfection</span>
                </div>
                <h3 className="mt-2 text-lg font-medium text-zinc-100">
                  PayWords + Winterfell STARK + Unilateral Escape Hatch
                </h3>
              </div>
              <span className="shrink-0 rounded-full border border-violet-500/25 bg-violet-500/10 px-2.5 py-0.5 font-mono text-[10px] text-violet-200">
                0 Sequencer Custody
              </span>
            </div>

            <p className="text-[13px] leading-relaxed text-zinc-400">
              Execute <strong className="text-zinc-200">2,000 live-measured sandbox SHA-256 PayWord + nsUSD transfers</strong> on the server,
              generate &amp; verify a real <strong className="text-zinc-200">Winterfell 0.13 STARK conservation proof</strong>,
              verify a unilateral <strong className="text-zinc-200">SHA-256 Merkle L1 Escape Hatch</strong>, and anchor to the live Mainnet v2 tip.
            </p>

            <div className="flex flex-wrap items-center gap-3">
              <button
                type="button"
                onClick={() => void runL2Benchmark()}
                disabled={l2Busy}
                className="inline-flex items-center gap-2 rounded-full bg-violet-400 px-4 py-2 text-xs font-semibold text-zinc-950 transition hover:bg-violet-300 cursor-pointer disabled:opacity-60"
              >
                {l2Busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Zap className="h-3.5 w-3.5" />}
                <span>Run 2,000-tx L2 STARK Sandbox</span>
              </button>
              <a
                href="/api/v2/l2/status"
                target="_blank"
                rel="noreferrer"
                className="font-mono text-[11px] text-zinc-400 hover:text-zinc-200"
              >
                GET /api/v2/l2/status
              </a>
            </div>

            {l2Result && (
              <div className="rounded-xl border border-violet-500/25 bg-[#090c12] p-4 font-mono text-[11px] space-y-1.5 text-zinc-300">
                <div
                  className={`flex items-center justify-between ${
                    l2Result.winterfell_stark_batch_proof?.verified ? 'text-emerald-300' : 'text-rose-300'
                  }`}
                >
                  <span>LIVE MEASURED SANDBOX THROUGHPUT</span>
                  <span>
                    {Number(l2Result.measured_offchain_tps ?? 0).toLocaleString()} TPS ({l2Result.payword_execution_us ?? 0} µs)
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-zinc-400">Winterfell 0.13 STARK Proof</span>
                  <span>
                    {l2Result.winterfell_stark_batch_proof?.verified ? 'VERIFIED' : 'FAIL'} ·{' '}
                    {((l2Result.winterfell_stark_batch_proof?.proof_bytes ?? 0) / 1024).toFixed(1)} KB ·{' '}
                    {l2Result.winterfell_stark_batch_proof?.verify_ms ?? 0} ms
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-zinc-400">Unilateral L1 Escape Hatch</span>
                  <span>
                    {l2Result.unilateral_l1_escape_hatch?.verified ? 'MERKLE PROOF VERIFIED' : 'N/A'}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-zinc-400">Mainnet v2 Anchor Commit</span>
                  <span>
                    {l2Result.l1_mainnet_v2_anchor.l1_height ? `#${l2Result.l1_mainnet_v2_anchor.l1_height} · ` : ''}
                    {truncateHash(l2Result.l1_mainnet_v2_anchor.anchor_commitment_hex, 8, 6)}
                  </span>
                </div>
              </div>
            )}
          </div>

          {/* Card C: 60-Second Drop-In Private x402 / nsUSD Paywall Middleware Generator */}
          <div className="rounded-2xl border border-emerald-500/20 bg-emerald-500/[0.03] p-6 space-y-4">
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="inline-flex items-center gap-2 font-mono text-[11px] uppercase tracking-[0.08em] text-emerald-300">
                  <Zap className="h-4 w-4" />
                  <span>60-Second Developer Utility · MCP #34</span>
                </div>
                <h3 className="mt-2 text-lg font-medium text-zinc-100">
                  Drop-In Private x402 + O(1) PayWords Middleware
                </h3>
              </div>
              <span className="shrink-0 rounded-full border border-emerald-500/25 bg-emerald-500/10 px-2.5 py-0.5 font-mono text-[10px] text-emerald-200">
                0 Per-Call Gas
              </span>
            </div>

            <p className="text-[13px] leading-relaxed text-zinc-400">
              Monetize any API route in stable USD (<code className="font-mono text-zinc-200">nsUSD</code>) over private{' '}
              <strong className="text-zinc-200">O(1) SHA-256 PayWords</strong> with automatic Base USDC{' '}
              <code className="font-mono text-zinc-200">x402 v2</code> fallback — zero custodian lock-in.
            </p>

            <div className="flex flex-wrap items-center gap-2">
              {(['express', 'nextjs', 'fastapi', 'axum'] as const).map((fw) => (
                <button
                  key={fw}
                  type="button"
                  onClick={() => void generatePaywallKit(fw)}
                  className={`rounded-full px-3 py-1 font-mono text-[11px] transition cursor-pointer ${
                    kitFw === fw
                      ? 'bg-emerald-400 font-semibold text-zinc-950'
                      : 'border border-white/[0.1] bg-white/[0.03] text-zinc-300 hover:bg-white/[0.08]'
                  }`}
                >
                  {fw}
                </button>
              ))}
              <input
                type="text"
                value={kitUsd}
                onChange={(e) => setKitUsd(e.target.value)}
                aria-label="Price in USD"
                className="w-20 rounded-lg border border-white/[0.1] bg-[#090c12] px-2.5 py-1 font-mono text-[11px] text-zinc-200"
                placeholder="0.005"
              />
              <input
                type="text"
                value={kitRoute}
                onChange={(e) => setKitRoute(e.target.value)}
                aria-label="Route path"
                className="w-36 rounded-lg border border-white/[0.1] bg-[#090c12] px-2.5 py-1 font-mono text-[11px] text-zinc-200"
                placeholder="/api/inference"
              />
              <button
                type="button"
                onClick={() => void generatePaywallKit()}
                disabled={kitBusy}
                className="inline-flex items-center gap-1.5 rounded-full bg-emerald-400 px-3.5 py-1.5 text-xs font-semibold text-zinc-950 transition hover:bg-emerald-300 cursor-pointer disabled:opacity-60"
              >
                {kitBusy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Play className="h-3.5 w-3.5" />}
                <span>Generate</span>
              </button>
            </div>

            {kitResult?.ok && (
              <div className="rounded-xl border border-emerald-500/25 bg-[#090c12] p-3.5 space-y-2">
                <div className="flex items-center justify-between gap-2 font-mono text-[11px] text-emerald-300">
                  <span>
                    {kitResult.filename} · ${kitResult.price_usd}/call ({kitResult.micro_usd_ticks} ticks)
                  </span>
                  <CopyButton value={kitResult.code} />
                </div>
                <pre className="max-h-52 overflow-auto font-mono text-[10.5px] leading-relaxed text-zinc-300">
                  {kitResult.code}
                </pre>
              </div>
            )}
          </div>

          {/* Card D: Live Base Mainnet Vault & nsUSD-B Escrow Verifier */}
          <div className="rounded-2xl border border-amber-500/20 bg-amber-500/[0.03] p-6 space-y-4">
            <div className="flex items-start justify-between gap-3">
              <div>
                <div className="inline-flex items-center gap-2 font-mono text-[11px] uppercase tracking-[0.08em] text-amber-300">
                  <ShieldCheck className="h-4 w-4" />
                  <span>Live Base Mainnet Reserves · MCP #35</span>
                </div>
                <h3 className="mt-2 text-lg font-medium text-zinc-100">
                  Verify Real Base USDC Vault Escrow &amp; HTLC Proofs
                </h3>
              </div>
              <span className="shrink-0 rounded-full border border-amber-500/25 bg-amber-500/10 px-2.5 py-0.5 font-mono text-[10px] text-amber-200">
                {backing ? `$${backing.total_usdc_formatted} USDC` : 'Live RPC'}
              </span>
            </div>

            <p className="text-[13px] leading-relaxed text-zinc-400">
              Ownerless <code className="font-mono text-zinc-200">NewsatoshiBackingVault</code> currently holds{' '}
              <strong className="text-zinc-200">
                {backing ? `${backing.total_usdc_formatted} USDC + ${backing.total_eth_formatted} ETH` : 'live on-chain reserves'}
              </strong>
              . Verify any Base USDC deposit tx live against Base RPC into an auditable Tier-1B{' '}
              <code className="font-mono text-zinc-200">nsUSD-B</code> receipt, or inspect our live Base SHA-256 HTLC{' '}
              <a
                href="https://basescan.org/tx/0xd90a9d873516ab92594ef71ada381ecc4b5852e9e70bd1652d60b0058e630f16"
                target="_blank"
                rel="noreferrer"
                className="text-amber-300 underline hover:text-white"
              >
                lock
              </a>{' '}
              →{' '}
              <a
                href="https://basescan.org/tx/0xdece20236b816eed13c358b923cf6a8a9e527b8ad72512ecd463c319b8d2d048"
                target="_blank"
                rel="noreferrer"
                className="text-amber-300 underline hover:text-white"
              >
                claim
              </a>{' '}
              proof.
            </p>

            <div className="flex flex-wrap items-center gap-2">
              <button
                type="button"
                onClick={() =>
                  void verifyBaseEscrow('0xd3c521e3dcdd1465f6f51384a5c721c0c445c3d384282ae0c67b2b07195029ba')
                }
                className="rounded-full border border-white/[0.1] bg-white/[0.03] px-2.5 py-1 font-mono text-[10.5px] text-zinc-300 hover:bg-white/[0.08] cursor-pointer"
              >
                $14.52 Direct Tx
              </button>
              <button
                type="button"
                onClick={() =>
                  void verifyBaseEscrow('0x21a8e80c95a4a7e6f4f459ea368764a74fe2c4a8a2d2d24a8a9f94c3b5dd7a2e')
                }
                className="rounded-full border border-white/[0.1] bg-white/[0.03] px-2.5 py-1 font-mono text-[10.5px] text-zinc-300 hover:bg-white/[0.08] cursor-pointer"
              >
                $0.10 RFC-0028 Tx
              </button>
              <input
                type="text"
                value={escrowTx}
                onChange={(e) => setEscrowTx(e.target.value)}
                aria-label="Base USDC deposit transaction hash"
                className="min-w-[180px] flex-1 rounded-lg border border-white/[0.1] bg-[#090c12] px-2.5 py-1 font-mono text-[11px] text-zinc-200"
              />
              <button
                type="button"
                onClick={() => void verifyBaseEscrow()}
                disabled={escrowBusy}
                className="inline-flex items-center gap-1.5 rounded-full bg-amber-400 px-3.5 py-1.5 text-xs font-semibold text-zinc-950 transition hover:bg-amber-300 cursor-pointer disabled:opacity-60"
              >
                {escrowBusy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ShieldCheck className="h-3.5 w-3.5" />}
                <span>Verify RPC</span>
              </button>
            </div>

            {escrowResult && (
              <div className="rounded-xl border border-amber-500/25 bg-[#090c12] p-3.5 font-mono text-[11px] space-y-1 text-zinc-300">
                {escrowResult.ok ? (
                  <>
                    <div className="flex items-center justify-between text-emerald-300">
                      <span>BASE MAINNET USDC VERIFIED</span>
                      <span>${escrowResult.verified_usdc_formatted} USDC</span>
                    </div>
                    <div className="flex items-center justify-between">
                      <span className="text-zinc-400">Block / Path</span>
                      <span>
                        #{escrowResult.block_number} · {escrowResult.deposit_path}
                      </span>
                    </div>
                    <div className="flex items-center justify-between">
                      <span className="text-zinc-400">nsUSD-B Receipt SHA-256</span>
                      <span>{truncateHash(escrowResult.receipt_commitment_sha256 || '', 10, 8)}</span>
                    </div>
                  </>
                ) : (
                  <div className="text-rose-300">{escrowResult.error || 'Verification failed'}</div>
                )}
              </div>
            )}
          </div>
        </div>
      </section>

      {/* 3. The Hourglass Architecture — Strict Separation of Core Settlement vs. Ecosystem Labs */}
      <section className="border-t border-white/[0.07] pt-16">
        <div className="mb-8 flex flex-wrap items-end justify-between gap-4">
          <div>
            <p className="font-mono text-[11px] uppercase tracking-[0.12em] text-zinc-500">
              The Hourglass Method
            </p>
            <h2 className="font-display mt-1.5 text-3xl tracking-tight text-zinc-100 sm:text-4xl">
              An ossified L1 waist. Infinite L2 innovation above it.
            </h2>
          </div>
          <p className="max-w-md text-xs leading-relaxed text-zinc-500">
            Protocols like TCP/IP and Bitcoin become Schelling points by keeping the base layer
            minimal and neutral — never burying application logic inside consensus.
          </p>
        </div>

        <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
          {/* Layer 1: The Narrow Waist */}
          <div className="rounded-2xl border border-emerald-500/20 bg-emerald-500/[0.03] p-6 sm:p-7">
            <div className="flex items-center justify-between gap-3">
              <div className="inline-flex items-center gap-2 font-mono text-[11px] uppercase tracking-[0.1em] text-emerald-300">
                <ShieldCheck className="h-4 w-4" />
                <span>Layer 1 · The Narrow Waist</span>
              </div>
              <a
                href="https://github.com/theguysaccount/newsatoshi-rfc0021"
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 rounded-full border border-emerald-500/25 bg-emerald-500/10 px-2.5 py-0.5 font-mono text-[10px] text-emerald-200 hover:bg-emerald-500/20"
              >
                <span>newsatoshi-rfc0021 · v1.2.5</span>
                <ArrowUpRight className="h-3 w-3" />
              </a>
            </div>

            <h3 className="mt-4 text-lg font-medium text-zinc-100">
              Unfreezable, private, SHA-2-only base money
            </h3>
            <p className="mt-2 text-[13px] leading-relaxed text-zinc-400">
              Everything required to be a credibly neutral settlement floor — and nothing that
              belongs in an app layer.
            </p>

            <ul className="mt-5 space-y-2.5 text-[13px] text-zinc-300">
              <li className="flex items-start gap-2.5">
                <Check className="mt-0.5 h-3.5 w-3.5 shrink-0 text-emerald-400" />
                <span>
                  <strong className="text-zinc-100">Private by default:</strong> shielded 2-in/2-out
                  notes so agents never leak suppliers, margins, or treasury flows.
                </span>
              </li>
              <li className="flex items-start gap-2.5">
                <Check className="mt-0.5 h-3.5 w-3.5 shrink-0 text-emerald-400" />
                <span>
                  <strong className="text-zinc-100">Pure SHA-2 only:</strong> SHA-256 / SHA-384 +
                  FIPS-205 SLH-DSA + Winterfell STARKs (0 elliptic curves, 0 trusted setup, 0 novel
                  ZK-hashes).
                </span>
              </li>
              <li className="flex items-start gap-2.5">
                <Check className="mt-0.5 h-3.5 w-3.5 shrink-0 text-emerald-400" />
                <span>
                  <strong className="text-zinc-100">Bitcoin AuxPoW + 100–104b QROM:</strong> borrows
                  Bitcoin SHA-256 ASIC security (<code className="font-mono text-xs">chainid 20051</code>),
                  proves Quartic <code className="font-mono text-xs">F_p4</code> SHA-384 STARKs past 100
                  conservative QROM bits, compresses K=8 spends <strong className="text-zinc-100">6.51×</strong> (~741 KB in ~1.28 s),
                  mechanizes all 249 AIR constraints + BatchSpend K=2..16 tiling in Lean 4, and includes an experimental 1-query STARK-in-STARK IVC R&amp;D prototype (production light clients use FlyClient MMR).
                </span>
              </li>
              <li className="flex items-start gap-2.5">
                <Check className="mt-0.5 h-3.5 w-3.5 shrink-0 text-emerald-400" />
                <span>
                  <strong className="text-zinc-100">Minimal lock floor:</strong> 50% fee burn,
                  SHA-256 HTLCs, PayWord channels, and one opt-in <code className="font-mono text-xs">PriorLock</code>{' '}
                  primitive. <em>Zero L1 launchpads, subjective juries, or rollup VMs.</em>
                </span>
              </li>
            </ul>
          </div>

          {/* Layer 2: Above the Waist */}
          <div className="rounded-2xl border border-white/[0.09] bg-[#0f0f12] p-6 sm:p-7">
            <div className="flex items-center justify-between gap-3">
              <div className="inline-flex items-center gap-2 font-mono text-[11px] uppercase tracking-[0.1em] text-zinc-300">
                <Layers className="h-4 w-4 text-amber-300" />
                <span>Layer 2 · Above the Waist (This Gateway)</span>
              </div>
              <a
                href="https://github.com/theguysaccount/newsatoshi-commons"
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 rounded-full border border-white/[0.1] bg-white/[0.04] px-2.5 py-0.5 font-mono text-[10px] text-zinc-300 hover:text-white"
              >
                <span>newsatoshi-commons · v0.28.4</span>
                <ArrowUpRight className="h-3 w-3" />
              </a>
            </div>

            <h3 className="mt-4 text-lg font-medium text-zinc-100">
              Agent commerce, multi-chain bridges &amp; experimental labs
            </h3>
            <p className="mt-2 text-[13px] leading-relaxed text-zinc-400">
              All the power tools, liquidity bridges, and autonomous-economy rails live safely
              above L1 so builders can iterate daily without touching consensus.
            </p>

            <ul className="mt-5 space-y-2.5 text-[13px] text-zinc-300">
              <li className="flex items-start gap-2.5">
                <Check className="mt-0.5 h-3.5 w-3.5 shrink-0 text-amber-300" />
                <span>
                  <strong className="text-zinc-100">Native LOVE Primary + x402 Bridges:</strong>{' '}
                  &lt;1 ms zero-gas SHA-256 PayWords primary; Base/Arbitrum/Polygon USDC{' '}
                  <code className="font-mono text-xs">x402 v2</code> supported as outer on-ramps.
                </span>
              </li>
              <li className="flex items-start gap-2.5">
                <Check className="mt-0.5 h-3.5 w-3.5 shrink-0 text-amber-300" />
                <span>
                  <strong className="text-zinc-100">4 Zero-Custody Atomic Buy Roads:</strong>{' '}
                  Bitcoin BIP-199 P2WSH, Lightning BOLT-11, Solana PDA, and 4-chain ownerless{' '}
                  <code className="font-mono text-xs">NewsatoshiHtlc.sol</code>.
                </span>
              </li>
              <li className="flex items-start gap-2.5">
                <Check className="mt-0.5 h-3.5 w-3.5 shrink-0 text-amber-300" />
                <span>
                  <strong className="text-zinc-100">A2A Commerce &amp; Identity Ready:</strong>{' '}
                  35 zero-auth MCP tools (Official MCP Registry <code className="font-mono text-xs">0.28.4</code>),{' '}
                  <code className="font-mono text-xs">ERC-8004</code> dual identity, 60s private{' '}
                  <code className="font-mono text-xs">x402</code> paywall kit, RFC-0028{' '}
                  <code className="font-mono text-xs">nsUSD</code> stability, and L2 STARK rollups.
                </span>
              </li>
              <li className="flex items-start gap-2.5">
                <Check className="mt-0.5 h-3.5 w-3.5 shrink-0 text-amber-300" />
                <span>
                  <strong className="text-zinc-100">Commons Labs &amp; Toolkits:</strong> Web4
                  charters, off-chain mission launchpads, zkTLS P2P market-maker verifier,
                  Backing Vaults, and Legal Airlock templates — all strictly above L1.
                </span>
              </li>
            </ul>
          </div>
        </div>
      </section>

      {/* 4. Three Clear Actions */}
      <section>
        <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
          <div>
            <p className="font-mono text-[11px] uppercase tracking-[0.12em] text-zinc-500">
              Start Here
            </p>
            <h2 className="font-display mt-1.5 text-3xl tracking-tight text-zinc-100 sm:text-4xl">
              Use the L2 tools or inspect the L1 core.
            </h2>
          </div>
          <p className="max-w-md text-xs leading-relaxed text-zinc-500">
            Every contract is ownerless. This gateway runs verify-only with zero server signing
            keys and renders live L1 and EVM state only.
          </p>
        </div>

        <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
          <button
            type="button"
            onClick={() => onNavigate('bridge', undefined, 'bridge')}
            className="surface-card surface-card-hover group flex flex-col justify-between rounded-2xl p-6 text-left cursor-pointer"
          >
            <div>
              <div className="flex items-center justify-between">
                <span className="font-mono text-[11px] text-zinc-500">01</span>
                <span className="rounded-full border border-white/[0.08] px-2.5 py-0.5 text-[10px] font-medium text-zinc-400">
                  Native L1 &amp; Mining
                </span>
              </div>
              <h3 className="mt-5 text-[17px] font-medium tracking-tight text-zinc-100">
                Bridge trustlessly or merge-mine
              </h3>
              <p className="mt-2 text-[13px] leading-relaxed text-zinc-400">
                Compile zero-custody SHA-256 HTLC swaps across Bitcoin, Lightning, Solana, and 4
                EVM chains directly into native Mainnet v2 LOVE, or point AuxPoW miners at 0% fee.
              </p>
            </div>
            <div className="mt-7 inline-flex items-center gap-1.5 text-[13px] font-medium text-zinc-200 transition group-hover:text-white">
              <span>Open Bridge &amp; Mining</span>
              <ArrowRight className="h-3.5 w-3.5 transition-transform group-hover:translate-x-0.5" />
            </div>
          </button>

          <button
            type="button"
            onClick={() => onNavigate('build')}
            className="surface-card surface-card-hover group flex flex-col justify-between rounded-2xl p-6 text-left cursor-pointer"
          >
            <div>
              <div className="flex items-center justify-between">
                <span className="font-mono text-[11px] text-zinc-500">02</span>
                <span className="rounded-full border border-white/[0.08] px-2.5 py-0.5 text-[10px] font-medium text-zinc-400">
                  L2 Agent Labs
                </span>
              </div>
              <h3 className="mt-5 text-[17px] font-medium tracking-tight text-zinc-100">
                Build Web4 agents &amp; charters
              </h3>
              <p className="mt-2 text-[13px] leading-relaxed text-zinc-400">
                Spin up covenant-bounded charters, register ERC-8004 identities, list Native LOVE
                PayWord services and HTLC bounties, and anchor reserves cleanly at Layer 2.
              </p>
            </div>
            <div className="mt-7 inline-flex items-center gap-1.5 text-[13px] font-medium text-zinc-200 transition group-hover:text-white">
              <span>Open L2 Forge</span>
              <ArrowRight className="h-3.5 w-3.5 transition-transform group-hover:translate-x-0.5" />
            </div>
          </button>

          <button
            type="button"
            onClick={() => onNavigate('protocol', 'explorer')}
            className="surface-card surface-card-hover group flex flex-col justify-between rounded-2xl p-6 text-left cursor-pointer"
          >
            <div>
              <div className="flex items-center justify-between">
                <span className="font-mono text-[11px] text-zinc-500">03</span>
                <span className="rounded-full border border-white/[0.08] px-2.5 py-0.5 text-[10px] font-medium text-zinc-400">
                  Audit &amp; Verify
                </span>
              </div>
              <h3 className="mt-5 text-[17px] font-medium tracking-tight text-zinc-100">
                Inspect Mainnet v2 &amp; STARK proofs
              </h3>
              <p className="mt-2 text-[13px] leading-relaxed text-zinc-400">
                Inspect live Mainnet v2 blocks, re-verify Winterfell SHA-384 STARK proofs, explore
                the LOVE Security Bug Bounty scope, and review the Legal Airlock templates.
              </p>
            </div>
            <div className="mt-7 inline-flex items-center gap-1.5 text-[13px] font-medium text-zinc-200 transition group-hover:text-white">
              <span>Inspect Protocol</span>
              <ArrowRight className="h-3.5 w-3.5 transition-transform group-hover:translate-x-0.5" />
            </div>
          </button>
        </div>
      </section>

      {/* 5. Developer & AI Agent Quickstart */}
      <section className="grid grid-cols-1 items-start gap-8 border-t border-white/[0.07] pt-16 lg:grid-cols-12">
        <div className="lg:col-span-5">
          <p className="font-mono text-[11px] uppercase tracking-[0.12em] text-zinc-500">
            For Builders &amp; AI Agents
          </p>
          <h2 className="font-display mt-2 text-3xl tracking-tight text-zinc-100 sm:text-4xl">
            Built to be called by software in one line.
          </h2>
          <p className="mt-3 text-[14px] leading-relaxed text-zinc-400">
            Connect any MCP-compatible agent with zero API keys, wire Native LOVE PayWords + x402
            USDC on-ramps in Node or Python, or check our live v2 cutover readiness endpoint.
          </p>

          <ul className="mt-6 space-y-2.5 text-[13px] text-zinc-300">
            <li className="flex items-center gap-2.5">
              <Check className="h-3.5 w-3.5 text-emerald-400 shrink-0" />
              <span>Native LOVE PayWords — up to 1,000,000 paid API calls per lock, $0 gas</span>
            </li>
            <li className="flex items-center gap-2.5">
              <Check className="h-3.5 w-3.5 text-emerald-400 shrink-0" />
              <span>Spec-compliant x402 v2 (Base/Arb/Polygon USDC) outer bridge on-ramp</span>
            </li>
            <li className="flex items-center gap-2.5">
              <Check className="h-3.5 w-3.5 text-emerald-400 shrink-0" />
              <span>ERC-8004, OpenAPI 3.1, A2A Agent Card, and /llms.txt auto-discovery</span>
            </li>
          </ul>

          <div className="mt-7 flex flex-wrap items-center gap-4 text-xs font-medium text-zinc-400">
            <a href="/developers" className="inline-flex items-center gap-1 hover:text-zinc-100">
              <span>Developer Portal</span>
              <ArrowUpRight className="h-3.5 w-3.5" />
            </a>
            <a href="/llms.txt" className="inline-flex items-center gap-1 hover:text-zinc-100">
              <span>llms.txt</span>
              <ArrowUpRight className="h-3.5 w-3.5" />
            </a>
            <a
              href="/api/v2/cutover-readiness"
              className="inline-flex items-center gap-1 hover:text-zinc-100"
            >
              <span>v2 Cutover Gate</span>
              <ArrowUpRight className="h-3.5 w-3.5" />
            </a>
            <a
              href="/.well-known/x402.json"
              className="inline-flex items-center gap-1 hover:text-zinc-100"
            >
              <span>x402 Manifest</span>
              <ArrowUpRight className="h-3.5 w-3.5" />
            </a>
          </div>
        </div>

        <div className="surface-card overflow-hidden rounded-2xl lg:col-span-7">
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-white/[0.07] px-4 py-2.5">
            <div className="flex items-center gap-1">
              {(Object.keys(SNIPPETS) as SnippetTab[]).map((key) => {
                const active = snippetTab === key;
                return (
                  <button
                    key={key}
                    type="button"
                    onClick={() => setSnippetTab(key)}
                    className={`rounded-lg px-2.5 py-1 text-xs font-medium transition cursor-pointer ${
                      active
                        ? 'bg-white/[0.09] text-zinc-100'
                        : 'text-zinc-500 hover:text-zinc-300'
                    }`}
                  >
                    {SNIPPETS[key].label}
                  </button>
                );
              })}
            </div>
            <CopyButton value={activeSnippet.code} label="Copy" />
          </div>

          <div className="bg-[#0a0a0d] p-5">
            <p className="mb-3 text-[11px] text-zinc-500">{activeSnippet.caption}</p>
            <pre className="overflow-x-auto font-mono text-[12.5px] leading-relaxed text-zinc-200">
              <code>{activeSnippet.code}</code>
            </pre>
          </div>
        </div>
      </section>

      {/* 6. Canonical Layer-2 / Bridge Deployed Contracts */}
      <section className="border-t border-white/[0.07] pt-16">
        <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
          <div>
            <p className="font-mono text-[11px] uppercase tracking-[0.12em] text-zinc-500">
              Layer 2 · Verified EVM Bridge &amp; Discovery Contracts
            </p>
            <h2 className="font-display mt-1.5 text-3xl tracking-tight text-zinc-100">
              Same ownerless address across Base, Arbitrum, Optimism &amp; Polygon.
            </h2>
          </div>
          {backing?.genesis_base_deposit_tx && (
            <a
              href={`https://basescan.org/tx/${backing.genesis_base_deposit_tx}`}
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1.5 rounded-full border border-white/[0.09] bg-white/[0.025] px-3 py-1.5 font-mono text-[11px] text-zinc-300 transition hover:border-white/[0.2] hover:text-white"
            >
              <span>Genesis Reserve TX: {truncateHash(backing.genesis_base_deposit_tx, 8, 6)}</span>
              <ExternalLink className="h-3 w-3 text-zinc-500" />
            </a>
          )}
        </div>

        <div className="divide-y divide-white/[0.07] overflow-hidden rounded-2xl border border-white/[0.08] bg-[#0f0f12]">
          {CANONICAL_CONTRACTS.map((c) => (
            <div
              key={c.name}
              className="flex flex-col justify-between gap-4 p-5 sm:p-6 lg:flex-row lg:items-center"
            >
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2.5">
                  <h3 className="font-mono text-[14px] font-medium text-zinc-100">{c.name}</h3>
                  <span className="rounded-full border border-white/[0.08] bg-white/[0.03] px-2.5 py-0.5 text-[10px] font-medium text-zinc-400">
                    {c.badge}
                  </span>
                </div>
                <p className="mt-1 text-xs text-zinc-500">{c.role}</p>
                <div className="mt-2.5 inline-flex items-center gap-2 font-mono text-xs text-zinc-300">
                  <span className="break-all">{c.address}</span>
                  <CopyButton value={c.address} />
                </div>
              </div>

              <div className="flex flex-wrap items-center gap-2 lg:shrink-0">
                {c.explorers.map((ex) => (
                  <a
                    key={ex.label}
                    href={ex.url}
                    target="_blank"
                    rel="noreferrer"
                    className="inline-flex items-center gap-1 rounded-lg border border-white/[0.08] bg-[#0a0a0d] px-2.5 py-1.5 text-[11px] font-medium text-zinc-300 transition hover:border-white/[0.2] hover:text-white"
                  >
                    <span>{ex.label}</span>
                    <ArrowUpRight className="h-3 w-3 text-zinc-500" />
                  </a>
                ))}
                <a
                  href={c.sourcify}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 rounded-lg border border-emerald-500/25 bg-emerald-500/[0.07] px-2.5 py-1.5 text-[11px] font-medium text-emerald-300 transition hover:bg-emerald-500/15"
                >
                  <span>Sourcify</span>
                  <ArrowUpRight className="h-3 w-3" />
                </a>
              </div>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
