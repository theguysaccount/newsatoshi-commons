import { useState } from 'react';
import { ArrowRight, ArrowUpRight, Check, ExternalLink } from 'lucide-react';
import type { BackingProofSummary, PrimaryView, ProtocolSubTab, SeedStatusResponse } from '../types';
import { formatL1IssuedLove } from '../api/normalize';
import { CopyButton, truncateHash } from './ui/CopyButton';

interface OverviewPageProps {
  seed: SeedStatusResponse | null;
  backing: BackingProofSummary | null;
  onNavigate: (view: PrimaryView, protocolTab?: ProtocolSubTab) => void;
}

type SnippetTab = 'mcp' | 'web4' | 'lean' | 'cli';

const SNIPPETS: Record<
  SnippetTab,
  { label: string; caption: string; code: string }
> = {
  mcp: {
    label: 'MCP Server',
    caption: '14 zero-auth tools on the Official MCP Registry (org.newsatoshi.commons/newsatoshi-commons)',
    code: `{
  "mcpServers": {
    "newsatoshi": {
      "url": "https://commons.newsatoshi.org/mcp"
    }
  }
}`,
  },
  web4: {
    label: 'Web4 Kit',
    caption: 'Zero-dependency Node 18+ sovereign business daemon (PayWord + x402 + ERC-8004)',
    code: `git clone https://github.com/theguysaccount/newsatoshi-commons.git
cd newsatoshi-commons

# Verify live 4-chain Proof-of-Backing & survival tier
node web4-kit/agent-business.mjs --check

# Start your paid PayWord + x402 API daemon
node web4-kit/agent-business.mjs`,
  },
  lean: {
    label: 'Lean 4 Proof',
    caption: 'Re-verify all 9 soundness theorems locally — 355 lines, 0 sorry, 0 Mathlib',
    code: `curl -fsSL https://commons.newsatoshi.org/formal/NewsatoshiCovenants.lean \\
  -o /tmp/NewsatoshiCovenants.lean

lean /tmp/NewsatoshiCovenants.lean
# Exit 0 — all 9 theorems machine-checked`,
  },
  cli: {
    label: 'Install CLI',
    caption: 'Verified SHA-256 release binaries for macOS & Linux (commons, swap, hw-signer)',
    code: `curl -fsSL https://commons.newsatoshi.org/install.sh | sh

# Verify live mainnet & compile a trustless atomic swap
newsatoshi-swap keygen`,
  },
};

const CANONICAL_CONTRACTS = [
  {
    name: 'NewsatoshiBackingVault',
    role: 'Ownerless multi-chain reserve & Web4 charter anchor (0 withdrawal path)',
    address: '0x37Ed94525DBE408fA72cDd156409875666679265',
    badge: '4 Chains · Live Reserves',
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
    role: 'Zero-custody SHA-256 atomic swap bridge (EVM 0x02 precompile)',
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
    role: 'Zero-owner, zero-tax Base ERC-20 mirror · 100% supply in Uniswap V3 WETH pool · LP NFT #6689936 burned to 0x…dEaD',
    address: '0x0A3ccFb3fF981a22A264Aa05CD8b77d06C2732A3',
    badge: 'Base Uniswap V3 · LP Burned',
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
    role: 'Registered across 4 mainnets: Base #1229992 · Arbitrum #1692 · Optimism #42519 · Polygon #715',
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

export function OverviewPage({ seed, backing, onNavigate }: OverviewPageProps) {
  const [snippetTab, setSnippetTab] = useState<SnippetTab>('mcp');
  const ms = seed?.mainnet_live ? seed.mainnet_status : null;
  const liveChainsCount = backing ? backing.chains.filter((c) => c.live).length : 0;
  const activeSnippet = SNIPPETS[snippetTab];

  return (
    <div className="animate-fade-in mx-auto max-w-[1120px] space-y-16 pb-20 pt-8 sm:pt-12">
      {/* 1. Editorial Hero */}
      <section className="max-w-3xl">
        <div className="inline-flex items-center gap-2.5 rounded-full border border-white/[0.09] bg-white/[0.025] px-3 py-1 text-[11px] font-medium tracking-wide text-zinc-400">
          <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
          <span className="font-mono uppercase text-zinc-300">NEWSATOSHI / 2.1</span>
          <span className="text-zinc-600">·</span>
          <span>Post-Quantum · Lean 4 Verified · 4 Mainnets</span>
        </div>

        <h1 className="font-display mt-6 text-[2.9rem] leading-[1.03] tracking-[-0.02em] text-zinc-50 sm:text-[4.35rem]">
          Money that proves its math, backs itself, and pays{' '}
          <span className="italic text-zinc-400">machines.</span>
        </h1>

        <p className="mt-6 max-w-2xl text-[15px] leading-[1.7] text-zinc-400 sm:text-[17px]">
          Newsatoshi (<span className="font-mono text-[14px] text-zinc-200">LOVE</span>) combines
          Bitcoin merge-mined security with post-quantum spending covenants, an ownerless 4-chain
          backing vault, and sub-millisecond HTTP 402 rails — so autonomous businesses and physical
          assets can earn, hold, and compound capital without a custodian.
        </p>

        <div className="mt-9 flex flex-wrap items-center gap-3">
          <button
            type="button"
            onClick={() => onNavigate('build')}
            className="inline-flex items-center gap-2 rounded-full bg-zinc-50 px-5 py-2.5 text-[13px] font-semibold text-zinc-950 transition hover:bg-white cursor-pointer"
          >
            <span>Launch a Web4 Business</span>
            <ArrowRight className="h-4 w-4" />
          </button>

          <button
            type="button"
            onClick={() => onNavigate('bridge')}
            className="inline-flex items-center gap-2 rounded-full border border-white/[0.12] bg-white/[0.03] px-5 py-2.5 text-[13px] font-medium text-zinc-200 transition hover:border-white/[0.24] hover:bg-white/[0.06] cursor-pointer"
          >
            <span>Bridge or Mine LOVE</span>
          </button>

          <a
            href="/whitepaper"
            className="inline-flex items-center gap-1.5 px-3 py-2.5 text-[13px] font-medium text-zinc-400 transition hover:text-zinc-100"
          >
            <span>Read Whitepaper</span>
            <ArrowUpRight className="h-3.5 w-3.5" />
          </a>
        </div>
      </section>

      {/* 2. Live Proof-of-Reality Strip (100% Live RPC & L1 State, Zero Fake Data) */}
      <section aria-label="Live network metrics">
        <div className="grid grid-cols-1 divide-y divide-white/[0.07] overflow-hidden rounded-2xl border border-white/[0.08] bg-[#0f0f12] sm:grid-cols-2 sm:divide-y-0 lg:grid-cols-4 lg:divide-x">
          <button
            type="button"
            onClick={() => onNavigate('protocol', 'explorer')}
            className="group p-5 text-left transition hover:bg-white/[0.02] cursor-pointer sm:p-6"
          >
            <div className="flex items-center justify-between text-[11px] font-medium uppercase tracking-[0.08em] text-zinc-500">
              <span>Mainnet Block</span>
              <span className="inline-flex items-center gap-1.5 font-mono text-[10px] text-emerald-400">
                <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
                {ms ? 'Live' : 'Syncing'}
              </span>
            </div>
            <div className="mt-2.5 font-mono text-2xl font-medium tracking-tight text-zinc-100 tabular-nums">
              {ms ? `#${ms.height.toLocaleString()}` : '—'}
            </div>
            <div className="mt-1.5 text-xs text-zinc-500">
              {ms
                ? `${formatL1IssuedLove(ms.issued)} LOVE fair-minted · 0 premine`
                : 'AuxPoW chainid 20051 · 50 LOVE/blk'}
            </div>
          </button>

          <button
            type="button"
            onClick={() => onNavigate('build')}
            className="group p-5 text-left transition hover:bg-white/[0.02] cursor-pointer sm:border-l sm:border-white/[0.07] sm:p-6 lg:border-l-0"
          >
            <div className="flex items-center justify-between text-[11px] font-medium uppercase tracking-[0.08em] text-zinc-500">
              <span>On-Chain Backing</span>
              <span className="font-mono text-[10px] text-amber-300/90">
                {backing ? `${liveChainsCount}/4 EVM` : 'Polling'}
              </span>
            </div>
            <div className="mt-2.5 font-mono text-2xl font-medium tracking-tight text-zinc-100 tabular-nums">
              {backing ? `${backing.total_eth_formatted} ETH` : '—'}
            </div>
            <div className="mt-1.5 text-xs text-zinc-500">
              {backing
                ? `${backing.total_vault_deposits} deposit · ${backing.total_onchain_anchored_charters} anchored charter`
                : 'Ownerless 4-chain reserve vault'}
            </div>
          </button>

          <a
            href="/formal/NewsatoshiCovenants.lean"
            className="group block p-5 text-left transition hover:bg-white/[0.02] sm:border-t sm:border-white/[0.07] sm:p-6 lg:border-t-0"
          >
            <div className="flex items-center justify-between text-[11px] font-medium uppercase tracking-[0.08em] text-zinc-500">
              <span>Formal Kernel</span>
              <ArrowUpRight className="h-3.5 w-3.5 text-zinc-600 transition group-hover:text-zinc-300" />
            </div>
            <div className="mt-2.5 font-mono text-2xl font-medium tracking-tight text-zinc-100">
              9 Theorems
            </div>
            <div className="mt-1.5 text-xs text-zinc-500">
              Lean 4.36.0 · 0 sorry · 0 Mathlib
            </div>
          </a>

          <div className="p-5 sm:border-l sm:border-t sm:border-white/[0.07] sm:p-6 lg:border-l-0 lg:border-t-0">
            <div className="flex items-center justify-between text-[11px] font-medium uppercase tracking-[0.08em] text-zinc-500">
              <span>Agent Interface</span>
              <span className="font-mono text-[10px] text-zinc-400">MCP · ERC-8004</span>
            </div>
            <div className="mt-2.5 font-mono text-2xl font-medium tracking-tight text-zinc-100">
              14 Live Tools
            </div>
            <div className="mt-1.5 flex items-center justify-between gap-2 text-xs text-zinc-500">
              <span className="truncate font-mono text-[11px]">commons.newsatoshi.org/mcp</span>
              <CopyButton value="https://commons.newsatoshi.org/mcp" />
            </div>
          </div>
        </div>
      </section>

      {/* 3. Three Clear Actions — Zero Jargon Salad */}
      <section>
        <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
          <div>
            <p className="font-mono text-[11px] uppercase tracking-[0.12em] text-zinc-500">
              Start Here
            </p>
            <h2 className="font-display mt-1.5 text-3xl tracking-tight text-zinc-100 sm:text-4xl">
              Three rails, one verifiable system.
            </h2>
          </div>
          <p className="max-w-md text-xs leading-relaxed text-zinc-500">
            Every contract is ownerless. This gateway holds zero signing keys and only renders live
            L1 and EVM state.
          </p>
        </div>

        <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
          <button
            type="button"
            onClick={() => onNavigate('build')}
            className="surface-card surface-card-hover group flex flex-col justify-between rounded-2xl p-6 text-left cursor-pointer"
          >
            <div>
              <div className="flex items-center justify-between">
                <span className="font-mono text-[11px] text-zinc-500">01</span>
                <span className="rounded-full border border-white/[0.08] px-2.5 py-0.5 text-[10px] font-medium text-zinc-400">
                  Web4 &amp; Backing
                </span>
              </div>
              <h3 className="mt-5 text-[17px] font-medium tracking-tight text-zinc-100">
                Spawn an asset-backed business
              </h3>
              <p className="mt-2 text-[13px] leading-relaxed text-zinc-400">
                Compile a covenant-locked charter with mathematically bounded child spending, anchor
                it on Base, Arbitrum, Optimism, or Polygon, and collect PayWord + x402 API revenue.
              </p>
            </div>
            <div className="mt-7 inline-flex items-center gap-1.5 text-[13px] font-medium text-zinc-200 transition group-hover:text-white">
              <span>Open Charter Forge</span>
              <ArrowRight className="h-3.5 w-3.5 transition-transform group-hover:translate-x-0.5" />
            </div>
          </button>

          <button
            type="button"
            onClick={() => onNavigate('bridge')}
            className="surface-card surface-card-hover group flex flex-col justify-between rounded-2xl p-6 text-left cursor-pointer"
          >
            <div>
              <div className="flex items-center justify-between">
                <span className="font-mono text-[11px] text-zinc-500">02</span>
                <span className="rounded-full border border-white/[0.08] px-2.5 py-0.5 text-[10px] font-medium text-zinc-400">
                  Zero-Custody Liquidity
                </span>
              </div>
              <h3 className="mt-5 text-[17px] font-medium tracking-tight text-zinc-100">
                Bridge or merge-mine LOVE
              </h3>
              <p className="mt-2 text-[13px] leading-relaxed text-zinc-400">
                Swap directly from Bitcoin, Lightning, Solana, Base, Arbitrum, Optimism, or Polygon
                via SHA-256 atomic HTLCs — or point existing SHA-256 hashpower with zero extra watts.
              </p>
            </div>
            <div className="mt-7 inline-flex items-center gap-1.5 text-[13px] font-medium text-zinc-200 transition group-hover:text-white">
              <span>Open Bridge &amp; Mining</span>
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
                Inspect proofs &amp; live chain
              </h3>
              <p className="mt-2 text-[13px] leading-relaxed text-zinc-400">
                Verify Winterfell STARK covenants, inspect live Mainnet and Testnet blocks, model
                parcel economics, and audit the hash-bound Purpose Trust / Wyoming DUNA airlock.
              </p>
            </div>
            <div className="mt-7 inline-flex items-center gap-1.5 text-[13px] font-medium text-zinc-200 transition group-hover:text-white">
              <span>Explore Protocol</span>
              <ArrowRight className="h-3.5 w-3.5 transition-transform group-hover:translate-x-0.5" />
            </div>
          </button>
        </div>
      </section>

      {/* 4. Developer & AI Agent Quickstart — Clean Hairline Code Bench */}
      <section className="grid grid-cols-1 items-start gap-8 border-t border-white/[0.07] pt-16 lg:grid-cols-12">
        <div className="lg:col-span-5">
          <p className="font-mono text-[11px] uppercase tracking-[0.12em] text-zinc-500">
            For Builders &amp; AI Agents
          </p>
          <h2 className="font-display mt-2 text-3xl tracking-tight text-zinc-100 sm:text-4xl">
            Built to be called by software in one line.
          </h2>
          <p className="mt-3 text-[14px] leading-relaxed text-zinc-400">
            Connect any MCP-compatible agent with zero API keys, deploy a self-paying HTTP 402
            service in Node or Python, or independently check our formal Lean 4 soundness kernel on
            your laptop.
          </p>

          <ul className="mt-6 space-y-2.5 text-[13px] text-zinc-300">
            <li className="flex items-center gap-2.5">
              <Check className="h-3.5 w-3.5 text-emerald-400 shrink-0" />
              <span>Pure SHA-256 PayWords — up to 1,000,000 paid API calls per lock, $0 gas</span>
            </li>
            <li className="flex items-center gap-2.5">
              <Check className="h-3.5 w-3.5 text-emerald-400 shrink-0" />
              <span>Dual ERC-8004 + Post-Quantum SLH-DSA agent identity standard</span>
            </li>
            <li className="flex items-center gap-2.5">
              <Check className="h-3.5 w-3.5 text-emerald-400 shrink-0" />
              <span>OpenAPI 3.1, A2A Agent Card, and /llms.txt auto-discovery built in</span>
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
              href="/.well-known/erc8004.json"
              className="inline-flex items-center gap-1 hover:text-zinc-100"
            >
              <span>ERC-8004 Card</span>
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

      {/* 5. Canonical Deployed Contracts — Verifiable Proof Table */}
      <section className="border-t border-white/[0.07] pt-16">
        <div className="mb-6 flex flex-wrap items-end justify-between gap-4">
          <div>
            <p className="font-mono text-[11px] uppercase tracking-[0.12em] text-zinc-500">
              Verified On-Chain
            </p>
            <h2 className="font-display mt-1.5 text-3xl tracking-tight text-zinc-100">
              Same address across Base, Arbitrum, Optimism &amp; Polygon.
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
