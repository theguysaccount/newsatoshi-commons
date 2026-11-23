import { useState } from 'react';
import {
  ArrowRight,
  ChevronDown,
  ChevronUp,
  Cpu,
  ExternalLink,
  LockKeyhole,
  ShieldCheck,
  Sparkles,
  Zap,
} from 'lucide-react';
import type { SeedStatusResponse, WorkspaceTab } from '../types';
import { formatL1IssuedLove } from '../api/normalize';

interface InnovationHeroProps {
  seed: SeedStatusResponse | null;
  onNavigateTab: (tab: WorkspaceTab) => void;
}

const BREAKTHROUGHS = [
  {
    num: '01',
    icon: Cpu,
    accent: 'text-amber-300 border-amber-500/30 bg-amber-500/10',
    title: 'Free Bitcoin-Grade Security',
    tagline: 'Same hash. Zero extra watts.',
    body: 'Every new coin usually starts weak or begs VCs for security. Newsatoshi piggybacks directly on Bitcoin’s global SHA-256 mining army — any Bitaxe or Antminer can protect Newsatoshi and earn LOVE using the exact same electricity it already spends.',
  },
  {
    num: '02',
    icon: ShieldCheck,
    accent: 'text-emerald-300 border-emerald-500/30 bg-emerald-500/10',
    title: 'Quantum-Proof & Private Like Cash',
    tagline: 'No elliptic curves. No exposed balances.',
    body: 'Elliptic-curve locks on legacy chains are built to crack under quantum computers, and public ledgers broadcast your net worth. Newsatoshi runs end-to-end on pure hash math (SHA-256, SLH-DSA & STARKs) with shielded wallets and zero secret setup ceremonies.',
  },
  {
    num: '03',
    icon: Zap,
    accent: 'text-sky-300 border-sky-500/30 bg-sky-500/10',
    title: 'Pay by the Thought for AI Agents',
    tagline: 'Up to 1,000,000 instant calls per lock.',
    body: 'AI agents can’t open a bank account, swipe a credit card, or pay a $5 fee for a $0.0001 API call. With HTTP 402 PayWords and live MCP tools, an agent peels off a cryptographic hash bead in under a millisecond — zero accounts, zero subscriptions, $0 gas.',
  },
  {
    num: '04',
    icon: LockKeyhole,
    accent: 'text-violet-300 border-violet-500/30 bg-violet-500/10',
    title: 'Zero-Vault Multi-Chain Buy Roads',
    tagline: 'Nothing to hack. No wrapped IOUs.',
    body: 'Crypto bridges lose billions because they pool everyone’s money in one central vault. Newsatoshi holds $0 in custody: Bitcoin, Lightning, Solana, and Base all snap onto one shared math lock — reveal the key to swap instantly, or walk away for a 100% refund.',
  },
] as const;

export function InnovationHero({ seed, onNavigateTab }: InnovationHeroProps) {
  const [expanded, setExpanded] = useState(true);
  const ms = seed?.mainnet_live ? seed.mainnet_status : null;

  return (
    <section className="mb-6 overflow-hidden rounded-2xl border border-emerald-500/25 bg-gradient-to-br from-[#091224]/95 via-[#070d1b]/95 to-[#061217]/95 shadow-2xl shadow-emerald-950/20">
      <div className="flex flex-wrap items-start justify-between gap-4 px-5 py-4 sm:px-6 sm:py-5">
        <div className="max-w-3xl">
          <div className="inline-flex flex-wrap items-center gap-2 rounded-full border border-emerald-500/30 bg-emerald-500/10 px-2.5 py-0.5 font-mono text-[10px] font-semibold uppercase tracking-wider text-emerald-300">
            <Sparkles className="h-3 w-3" />
            <span>Plain-English Overview</span>
            <span className="text-slate-500">·</span>
            <span className="text-slate-200">
              {ms
                ? `Mainnet Block #${ms.height.toLocaleString()} · ${formatL1IssuedLove(ms.issued)} LOVE Fair-Minted`
                : 'Live Open-Source Protocol · Zero Pre-Mine'}
            </span>
          </div>

          <h2 className="mt-2.5 text-lg font-bold tracking-tight text-slate-50 sm:text-2xl">
            Bitcoin invented digital gold for humans.{' '}
            <span className="bg-gradient-to-r from-emerald-300 via-teal-200 to-sky-300 bg-clip-text text-transparent">
              Newsatoshi upgrades it for the Quantum &amp; AI era.
            </span>
          </h2>

          <p className="mt-1.5 text-xs leading-relaxed text-slate-300 sm:text-[13px]">
            Two shocks are breaking legacy blockchains: quantum computers targeting elliptic-curve
            keys, and billions of autonomous AI agents that can’t open bank accounts or pay gas
            fees for microscopic API calls. Newsatoshi solves both without venture pre-mines,
            custodians, or wasted energy.
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={() => onNavigateTab('swaps')}
            className="inline-flex items-center gap-1.5 rounded-lg bg-emerald-500 px-3 py-1.5 text-xs font-semibold text-slate-950 transition hover:bg-emerald-400 cursor-pointer"
          >
            <span>Bridge or Merge-Mine</span>
            <ArrowRight className="h-3.5 w-3.5" />
          </button>
          <a
            href="/developers"
            className="inline-flex items-center gap-1.5 rounded-lg border border-sky-500/35 bg-sky-500/10 px-3 py-1.5 text-xs font-medium text-sky-200 transition hover:bg-sky-500/20"
          >
            <span>AI Agent Setup</span>
            <ExternalLink className="h-3 w-3" />
          </a>
          <button
            type="button"
            onClick={() => setExpanded((v) => !v)}
            className="inline-flex items-center gap-1 rounded-lg border border-slate-700/80 bg-slate-900/70 px-2.5 py-1.5 font-mono text-[11px] text-slate-300 transition hover:border-slate-500 hover:text-white cursor-pointer"
            aria-expanded={expanded}
          >
            <span>{expanded ? 'Hide 4 Pillars' : 'Show 4 Pillars'}</span>
            {expanded ? <ChevronUp className="h-3.5 w-3.5" /> : <ChevronDown className="h-3.5 w-3.5" />}
          </button>
        </div>
      </div>

      {expanded && (
        <div className="grid grid-cols-1 gap-3 border-t border-slate-800/80 bg-[#050912]/60 px-5 py-4 sm:grid-cols-2 sm:px-6 xl:grid-cols-4">
          {BREAKTHROUGHS.map((item) => {
            const Icon = item.icon;
            return (
              <div
                key={item.num}
                className="rounded-xl border border-slate-800/90 bg-slate-950/75 p-3.5 transition hover:border-slate-700"
              >
                <div className="flex items-center justify-between gap-2">
                  <span
                    className={`inline-flex items-center gap-1.5 rounded-md border px-2 py-0.5 font-mono text-[10px] font-bold ${item.accent}`}
                  >
                    <Icon className="h-3 w-3" />
                    <span>{item.num}</span>
                  </span>
                  <span className="font-mono text-[10px] text-slate-400">{item.tagline}</span>
                </div>
                <h3 className="mt-2.5 text-sm font-semibold text-slate-100">{item.title}</h3>
                <p className="mt-1.5 text-[11.5px] leading-relaxed text-slate-400">{item.body}</p>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}
