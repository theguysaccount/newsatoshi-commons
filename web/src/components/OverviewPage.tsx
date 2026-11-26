import { useState } from 'react';
import { ArrowRight, ArrowUpRight, Check, ExternalLink, Layers, ShieldCheck } from 'lucide-react';
import type { BackingProofSummary, PrimaryView, ProtocolSubTab, SeedStatusResponse } from '../types';
import { formatL1IssuedLove } from '../api/normalize';
import { InstantSwapCard } from './InstantSwapCard';
import type { SwapHubSection } from './SwapsMiningTab';
import { CopyButton, truncateHash } from './ui/CopyButton';

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
    caption: '30 zero-auth L2 gateway & crypto tools (EVM audit, BTC mempool, BOLT-11, Uniswap V3, Native LOVE PayWords & v2 cutover gate)',
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
    caption: 'Re-verify the 9 L2 covenant model theorems locally (L1 SpendBalance.lean separately checks 10 accounting theorems; Z3 checks 249 AIR constraints)',
    code: `curl -fsSL https://commons.newsatoshi.org/formal/NewsatoshiCovenants.lean \\
  -o /tmp/NewsatoshiCovenants.lean

lean /tmp/NewsatoshiCovenants.lean
# Exit 0 — 9 covenant model theorems, 0 sorry`,
  },
  cli: {
    label: 'Install CLI',
    caption: 'Verified SHA-256 release binaries for macOS & Linux (commons, swap, hw-signer)',
    code: `curl -fsSL https://commons.newsatoshi.org/install.sh | sh

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
    name: 'NewsatoshiBaseMirror (wLOVE)',
    role: 'Independent Base ERC-20 discovery mirror (no on-chain L1↔EVM bridge; native L1 LOVE is separate & traded P2P via SHA-256 HTLCs) · LP NFT #6689936 burned',
    address: '0x0A3ccFb3fF981a22A264Aa05CD8b77d06C2732A3',
    badge: 'Base Uniswap V3 · Independent Mirror',
    explorers: [
      { label: 'Uniswap', url: 'https://app.uniswap.org/swap?chain=base&outputCurrency=0x0A3ccFb3fF981a22A264Aa05CD8b77d06C2732A3' },
      { label: 'DEXScreener', url: 'https://dexscreener.com/base/0x08ddc8C3fc09ACdab6802a13B26631f47ABcB888' },
      { label: 'GeckoTerminal', url: 'https://www.geckoterminal.com/base/pools/0x08ddc8c3fc09acdab6802a13b26631f47abcb888' },
      { label: 'BaseScan', url: 'https://basescan.org/address/0x0A3ccFb3fF981a22A264Aa05CD8b77d06C2732A3' },
    ],
    sourcify: 'https://repo.sourcify.dev/8453/0x0A3ccFb3fF981a22A264Aa05CD8b77d06C2732A3',
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
  const ms = seed?.mainnet_live ? seed.mainnet_status : null;
  const activeSnippet = SNIPPETS[snippetTab];

  return (
    <div className="animate-fade-in mx-auto max-w-[1120px] space-y-16 pb-20 pt-8 sm:pt-12">
      {/* 1. Plain-English Hourglass Hero + 1-Click Bridge/Discovery Card */}
      <section className="grid grid-cols-1 items-start gap-8 lg:grid-cols-12 lg:gap-10">
        <div className="lg:col-span-7">
          <div className="inline-flex flex-wrap items-center gap-2 rounded-full border border-white/[0.09] bg-white/[0.025] px-3 py-1 text-[11px] font-medium tracking-wide text-zinc-400">
            <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
            <span className="font-mono uppercase text-zinc-200">L1 v1.0.0-core</span>
            <span className="text-zinc-600">·</span>
            <span>Hourglass Architecture · L2 Agent &amp; Bridge Gateway</span>
          </div>

          <h1 className="font-display mt-6 text-[2.6rem] leading-[1.03] tracking-[-0.02em] text-zinc-50 sm:text-[3.85rem]">
            Private, quantum-proof cash{' '}
            <span className="italic text-zinc-400">for humans &amp; autonomous agents.</span>
          </h1>

          <p className="mt-5 max-w-xl text-[15px] leading-[1.7] text-zinc-400 sm:text-[16px]">
            <span className="font-mono text-[14px] text-zinc-200">Newsatoshi (LOVE)</span> keeps
            Layer 1 radically small — default shielded privacy, pure SHA-2 cryptography, and
            Bitcoin merge-mining (<span className="font-mono text-[13px] text-zinc-300">chainid 20051</span>).
            Everything rich — 30 MCP tools, Native LOVE PayWords, multi-chain HTLC swaps, x402
            USDC bridges, ERC-8004 identity, and Web4 agent toolkits — lives cleanly above the waist
            on this gateway.
          </p>

          <div className="mt-8 flex flex-wrap items-center gap-3">
            <button
              type="button"
              onClick={() => onNavigate('bridge', undefined, 'setup')}
              className="inline-flex items-center gap-2 rounded-full bg-zinc-50 px-5 py-2.5 text-[13px] font-semibold text-zinc-950 transition hover:bg-white cursor-pointer"
            >
              <span>Bridge &amp; Merge-Mine</span>
              <ArrowRight className="h-4 w-4" />
            </button>

            <button
              type="button"
              onClick={() => onNavigate('build')}
              className="inline-flex items-center gap-2 rounded-full border border-white/[0.12] bg-white/[0.03] px-5 py-2.5 text-[13px] font-medium text-zinc-200 transition hover:border-white/[0.24] hover:bg-white/[0.06] cursor-pointer"
            >
              <span>L2 Agent Labs</span>
            </button>

            <a
              href="https://github.com/theguysaccount/newsatoshi-rfc0021/releases/tag/v1.0.0-core"
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1.5 px-3 py-2.5 text-[13px] font-medium text-zinc-400 transition hover:text-zinc-100"
            >
              <span>L1 v1.0.0-core</span>
              <ArrowUpRight className="h-3.5 w-3.5" />
            </a>
          </div>
        </div>

        <div className="lg:col-span-5">
          <InstantSwapCard
            compact
            onNotice={onNotice}
            onOpenBridge={() => onNavigate('bridge', undefined, 'bridge')}
          />
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
              <span>v0.1.0 Canary</span>
              <span className="inline-flex items-center gap-1.5 font-mono text-[10px] text-amber-300">
                <span className="h-1.5 w-1.5 rounded-full bg-amber-300" />
                {ms ? 'Pre-Launch' : 'Syncing'}
              </span>
            </div>
            <div className="mt-2.5 font-mono text-2xl font-medium tracking-tight text-zinc-100 tabular-nums">
              {ms ? `#${ms.height.toLocaleString()}` : '—'}
            </div>
            <div className="mt-1.5 text-xs text-zinc-500">
              {ms
                ? `Diff ${ms.difficulty.toFixed(0)} · ${formatL1IssuedLove(ms.issued)} LOVE · retired for v2 genesis`
                : 'AuxPoW chainid 20051 · Canary'}
            </div>
          </button>

          <a
            href="/api/v2/cutover-readiness"
            target="_blank"
            rel="noreferrer"
            className="group block p-5 text-left transition hover:bg-white/[0.02] sm:border-l sm:border-white/[0.07] sm:p-6 lg:border-l-0"
          >
            <div className="flex items-center justify-between text-[11px] font-medium uppercase tracking-[0.08em] text-zinc-500">
              <span>Mainnet v2 Gate</span>
              <span className="font-mono text-[10px] text-amber-300/90">Honest Status</span>
            </div>
            <div className="mt-2.5 font-mono text-2xl font-medium tracking-tight text-zinc-100 tabular-nums">
              0/2 Audits
            </div>
            <div className="mt-1.5 text-xs text-zinc-500">
              0/3 real-difficulty BTC pools · blocked until ready
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
              10 Lean · 249 Z3
            </div>
            <div className="mt-1.5 text-xs text-zinc-500">
              Accounting theorems + Z3 spend AIR · 6.49× K=8 batch
            </div>
          </a>

          <div className="p-5 sm:border-l sm:border-t sm:border-white/[0.07] sm:p-6 lg:border-l-0 lg:border-t-0">
            <div className="flex items-center justify-between text-[11px] font-medium uppercase tracking-[0.08em] text-zinc-500">
              <span>L2 Agent Gateway</span>
              <span className="font-mono text-[10px] text-emerald-400">LOVE Primary</span>
            </div>
            <div className="mt-2.5 font-mono text-2xl font-medium tracking-tight text-zinc-100">
              30 Live Tools
            </div>
            <div className="mt-1.5 flex items-center justify-between gap-2 text-xs text-zinc-500">
              <span className="truncate font-mono text-[11px]">commons.newsatoshi.org/mcp</span>
              <CopyButton value="https://commons.newsatoshi.org/mcp" />
            </div>
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
                <span>newsatoshi-rfc0021 · v1.0.0-core</span>
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
                  <strong className="text-zinc-100">Bitcoin AuxPoW + BatchSpend:</strong> borrows
                  Bitcoin SHA-256 ASIC security (<code className="font-mono text-xs">chainid 20051</code>),
                  compresses K=8 spends <strong className="text-zinc-100">6.49×</strong> (641 KB),
                  and verifies light clients via work-proportional FlyClient MMR.
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
                <span>newsatoshi-commons · v0.22.1</span>
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
                  30 zero-auth MCP tools, <code className="font-mono text-xs">ERC-8004</code> dual
                  identity, and opt-in escrow/arbitration hooks for agent marketplaces (e.g. Psilo).
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
            onClick={() => onNavigate('bridge', undefined, 'swap')}
            className="surface-card surface-card-hover group flex flex-col justify-between rounded-2xl p-6 text-left cursor-pointer"
          >
            <div>
              <div className="flex items-center justify-between">
                <span className="font-mono text-[11px] text-zinc-500">01</span>
                <span className="rounded-full border border-white/[0.08] px-2.5 py-0.5 text-[10px] font-medium text-zinc-400">
                  Liquidity &amp; Mining
                </span>
              </div>
              <h3 className="mt-5 text-[17px] font-medium tracking-tight text-zinc-100">
                Bridge, swap, or merge-mine
              </h3>
              <p className="mt-2 text-[13px] leading-relaxed text-zinc-400">
                Compile zero-custody SHA-256 HTLC swaps across Bitcoin, Lightning, Solana, and 4
                EVM chains, connect AuxPoW miners, or trade the independent Base wLOVE mirror.
              </p>
            </div>
            <div className="mt-7 inline-flex items-center gap-1.5 text-[13px] font-medium text-zinc-200 transition group-hover:text-white">
              <span>Open Swaps &amp; Mining</span>
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
                Audit L1 state &amp; STARK proofs
              </h3>
              <p className="mt-2 text-[13px] leading-relaxed text-zinc-400">
                Inspect live canary &amp; testnet blocks, re-verify Winterfell STARK proofs, check
                the honest v2 audit/pool launch gate, and review the Legal Airlock templates.
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
