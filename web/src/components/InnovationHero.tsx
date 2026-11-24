import { useState } from 'react';
import {
  ArrowRight,
  ChevronDown,
  ChevronUp,
  Cpu,
  ExternalLink,
  Infinity as InfinityIcon,
  LockKeyhole,
  ShieldCheck,
  Sparkles,
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
    icon: LockKeyhole,
    accent: 'text-emerald-300 border-emerald-500/30 bg-emerald-500/10',
    title: 'Money With a Mathematical Conscience',
    tagline: 'Change can never escape the mission.',
    body: 'Bitcoin stops working the second you spend it into an organization — humans take custody and drift or embezzle. Newsatoshi recursive STARK covenants lock capital to a real-world mission forever: every spend and every change output must obey the exact same rule, verified in <10ms on a $5 off-grid chip.',
  },
  {
    num: '02',
    icon: InfinityIcon,
    accent: 'text-sky-300 border-sky-500/30 bg-sky-500/10',
    title: 'Autonomous Abundance Loops',
    tagline: 'Real assets ↔ machines ↔ capital.',
    body: 'Field meters sign real energy, soil, or compute output with post-quantum hardware keys; machines & AI agents pay per call in under a millisecond via HTTP 402 PayWords ($0 gas, 1M calls/lock); and a hash-bound Purpose Trust / Wyoming DUNA legal airlock auto-compounds revenue into the next physical asset.',
  },
  {
    num: '03',
    icon: Cpu,
    accent: 'text-amber-300 border-amber-500/30 bg-amber-500/10',
    title: 'Blind, Bitcoin-Secured Base Money',
    tagline: 'Uncorruptible L1 · 0 extra watts.',
    body: 'Unlike failed “Proof of Useful Work” coins that collapse to wash-farming, Newsatoshi keeps base issuance 100% blind and ossified (21M LOVE cap, 50 LOVE/blk) while piggybacking on Bitcoin’s global SHA-256 miners at zero extra electricity (AuxPoW chainid 20051).',
  },
  {
    num: '04',
    icon: ShieldCheck,
    accent: 'text-violet-300 border-violet-500/30 bg-violet-500/10',
    title: 'Centuries-Grade Privacy & Vaultless Gravity',
    tagline: 'Pure hash math · $0 bridge custody.',
    body: 'Zero elliptic curves, zero trusted setup ceremonies, shielded cash privacy by default (nsd1…), and vaultless SHA-256 atomic buy roads from Bitcoin, Lightning, Solana, and Base — capital flows in without honeypot bridge custodians or wrapped IOUs.',
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
            <span>Autonomous Abundance Engine</span>
            <span className="text-slate-500">·</span>
            <span className="text-slate-200">
              {ms
                ? `Mainnet Block #${ms.height.toLocaleString()} · ${formatL1IssuedLove(ms.issued)} LOVE Fair-Minted`
                : 'Live Open-Source L1 · Zero Pre-Mine'}
            </span>
          </div>

          <h2 className="mt-2.5 text-lg font-bold tracking-tight text-slate-50 sm:text-2xl">
            Bitcoin freed money from banks — and left it idle in a vault.{' '}
            <span className="bg-gradient-to-r from-emerald-300 via-teal-200 to-sky-300 bg-clip-text text-transparent">
              Newsatoshi makes sovereign money alive.
            </span>
          </h2>

          <p className="mt-1.5 text-xs leading-relaxed text-slate-300 sm:text-[13px]">
            Incorruptible, mission-locked capital that verifies real-world work, pays machines and
            AI agents in milliseconds, and compounds physical abundance forever — without trusting a
            CEO, a bridge vault, or a single human custodian.
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={() => onNavigateTab('swaps')}
            className="inline-flex items-center gap-1.5 rounded-lg bg-emerald-500 px-3 py-1.5 text-xs font-semibold text-slate-950 transition hover:bg-emerald-400 cursor-pointer"
          >
            <span>Buy Roads &amp; Mining</span>
            <ArrowRight className="h-3.5 w-3.5" />
          </button>
          <a
            href="/developers"
            className="inline-flex items-center gap-1.5 rounded-lg border border-sky-500/35 bg-sky-500/10 px-3 py-1.5 text-xs font-medium text-sky-200 transition hover:bg-sky-500/20"
          >
            <span>AI Agent &amp; Dev Portal</span>
            <ExternalLink className="h-3 w-3" />
          </a>
          <button
            type="button"
            onClick={() => setExpanded((v) => !v)}
            className="inline-flex items-center gap-1 rounded-lg border border-slate-700/80 bg-slate-900/70 px-2.5 py-1.5 font-mono text-[11px] text-slate-300 transition hover:border-slate-500 hover:text-white cursor-pointer"
            aria-expanded={expanded}
          >
            <span>{expanded ? 'Hide Architecture' : 'Show Architecture'}</span>
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
