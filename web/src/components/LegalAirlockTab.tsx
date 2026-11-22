import { useMemo } from 'react';
import { Building2, Download, KeyRound, Scale, ShieldCheck } from 'lucide-react';
import type { CommonsState, SeedStatusResponse } from '../types';
import { CopyButton, HashPill, truncateHash } from './ui/CopyButton';
import { Panel } from './ui/Panel';

interface LegalAirlockTabProps {
  state: CommonsState;
  seed: SeedStatusResponse | null;
}

export function LegalAirlockTab({ state, seed }: LegalAirlockTabProps) {
  const { covenant_rule: rule, keys } = state;

  const boundSchedule = useMemo(
    () => ({
      protocol_version: state.protocol_version,
      exported_at_utc: new Date().toISOString(),
      covenant_rule_hash: state.covenant_rule_hash,
      covenant_rule: {
        max_bounty_payout: rule.max_bounty_payout,
        reinvest_bps: rule.reinvest_bps,
        min_soil_health_score: rule.min_soil_health_score,
        authorized_deed_oracle_pk: rule.authorized_deed_oracle_pk,
        authorized_hardware_pk: rule.authorized_hardware_pk,
      },
      keys: {
        deed_oracle_merkle_root: keys.deed_oracle_merkle_root,
        hardware_merkle_root: keys.hardware_merkle_root,
        deed_oracle_slh_dsa_pk_hex: keys.deed_oracle_slh_dsa_pk_hex,
        hardware_slh_dsa_pk_hex: keys.hardware_slh_dsa_pk_hex,
        deed_oracle_remaining_leaves: keys.deed_oracle_remaining_leaves,
        hardware_remaining_leaves: keys.hardware_remaining_leaves,
      },
      l1_anchors: {
        mainnet_genesis: seed?.mainnet_info?.genesis ?? null,
        testnet_genesis: seed?.status?.genesis ?? null,
        testnet_observed_height: seed?.status?.height ?? null,
        testnet_observed_tip: seed?.status?.tip ?? null,
        auxpow_chainid: seed?.status?.auxpow_chainid ?? seed?.mainnet_info?.auxpow_chainid ?? null,
      },
      ledger_snapshot: {
        tx_nonce: state.ledger_summary.tx_nonce,
        state_root: state.ledger_summary.state_root,
        parcels_acquired_count: state.ledger_summary.parcels_acquired_count,
        balance_love: state.ledger_summary.balance_love,
      },
    }),
    [state, seed, rule, keys]
  );

  const scheduleJson = useMemo(() => JSON.stringify(boundSchedule, null, 2), [boundSchedule]);

  const handleDownloadSchedule = () => {
    const blob = new Blob([scheduleJson], { type: 'application/json;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `newsatoshi-commons-legal-schedule-${state.covenant_rule_hash.slice(0, 10) || 'bound'}.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  const Token = ({ children }: { children: string }) => (
    <span className="mx-0.5 inline-block rounded border border-emerald-500/35 bg-emerald-500/15 px-1.5 py-0.5 font-mono text-[11px] font-semibold text-emerald-200">
      {children}
    </span>
  );

  return (
    <div className="space-y-6">
      <div className="glass-panel flex flex-wrap items-center justify-between gap-4 rounded-xl border-emerald-500/30 p-5">
        <div className="space-y-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/40 bg-emerald-500/15 px-2.5 py-0.5 font-mono text-[10px] font-bold uppercase text-emerald-300">
              <ShieldCheck className="h-3.5 w-3.5" />
              Live-Bound to Active Rust Engine
            </span>
            <span className="rounded-full border border-slate-700 bg-slate-900 px-2.5 py-0.5 font-mono text-[10px] text-slate-300">
              Open-Source Legal Template · Counsel Review Required
            </span>
          </div>
          <h2 className="text-base font-semibold text-slate-100">
            Legal-Cryptographic Airlock — Perpetual Purpose Trust &amp; Wyoming DUNA
          </h2>
          <p className="text-xs text-slate-400">
            Binds real county land deeds and marginal-cost offtake directly to the immutable{' '}
            <code className="font-mono text-emerald-300">{state.protocol_version}</code> rule hash
            and post-quantum signer roots served by this Rust node.
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2.5">
          <CopyButton value={scheduleJson} label="Copy JSON" size="sm" />
          <button
            type="button"
            onClick={handleDownloadSchedule}
            className="inline-flex items-center gap-2 rounded-lg bg-emerald-500 px-4 py-2 text-xs font-semibold text-slate-950 shadow-lg shadow-emerald-500/15 transition hover:bg-emerald-400 cursor-pointer"
          >
            <Download className="h-4 w-4" />
            Download Bound Schedule JSON
          </button>
        </div>
      </div>

      <Panel
        title="Schedules A & B — Live-Bound Cryptographic Covenant & Key Registry"
        subtitle="Read directly from /api/state — compatible with legal/render_airlock.py"
        icon={<KeyRound className="h-4 w-4 text-emerald-400" />}
        badge={<span className="font-mono text-[10.5px] text-emerald-300">{state.protocol_version}</span>}
      >
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          <div className="space-y-3 rounded-lg border border-slate-800/90 bg-slate-950/70 p-4">
            <div className="flex items-center justify-between border-b border-slate-800 pb-2">
              <span className="font-mono text-xs font-bold uppercase tracking-wider text-emerald-300">
                Schedule A — Immutable Covenant Rule
              </span>
              <span className="font-mono text-[10px] text-slate-500">
                State Nonce #{state.ledger_summary.tx_nonce}
              </span>
            </div>

            <div className="space-y-2.5 text-xs">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-slate-400">covenant_rule_hash</span>
                <HashPill hash={state.covenant_rule_hash} head={14} tail={10} />
              </div>
              <div className="flex items-center justify-between font-mono">
                <span className="font-sans text-slate-400">reinvest_bps</span>
                <span className="font-semibold text-emerald-300">
                  {rule.reinvest_bps} bps ({(rule.reinvest_bps / 100).toFixed(2)}%)
                </span>
              </div>
              <div className="flex items-center justify-between font-mono">
                <span className="font-sans text-slate-400">max_bounty_payout</span>
                <span className="font-semibold text-amber-300">
                  {rule.max_bounty_payout.toLocaleString()} LOVE
                </span>
              </div>
              <div className="flex items-center justify-between font-mono">
                <span className="font-sans text-slate-400">min_soil_health_score</span>
                <span className="font-semibold text-emerald-300">
                  ≥ {rule.min_soil_health_score} / 100
                </span>
              </div>
              <div className="flex flex-wrap items-center justify-between gap-2 border-t border-slate-800/70 pt-1">
                <span className="text-slate-400">Mainnet Genesis Anchor</span>
                {seed?.mainnet_info?.genesis ? (
                  <HashPill hash={seed.mainnet_info.genesis} head={12} tail={10} />
                ) : (
                  <span className="font-mono text-slate-500">Seed disconnected</span>
                )}
              </div>
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-slate-400">Testnet Genesis Anchor</span>
                {seed?.status?.genesis ? (
                  <HashPill hash={seed.status.genesis} head={12} tail={10} />
                ) : (
                  <span className="font-mono text-slate-500">Seed disconnected</span>
                )}
              </div>
            </div>
          </div>

          <div className="space-y-3 rounded-lg border border-slate-800/90 bg-slate-950/70 p-4">
            <div className="flex items-center justify-between border-b border-slate-800 pb-2">
              <span className="font-mono text-xs font-bold uppercase tracking-wider text-sky-300">
                Schedule B — Authorized Post-Quantum Signer Roots
              </span>
              <span className="font-mono text-[10px] text-slate-500">SHA-256 PQ Floor</span>
            </div>

            <div className="space-y-2.5 text-xs">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <div className="text-slate-300">Deed Oracle XMSS Merkle Root</div>
                  <div className="font-mono text-[10px] text-slate-500">
                    {keys.deed_oracle_remaining_leaves} one-time leaves remaining
                  </div>
                </div>
                <HashPill hash={keys.deed_oracle_merkle_root} head={12} tail={10} />
              </div>

              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <div className="text-slate-300">Hardware TEE XMSS Merkle Root</div>
                  <div className="font-mono text-[10px] text-slate-500">
                    {keys.hardware_remaining_leaves} one-time leaves remaining
                  </div>
                </div>
                <HashPill hash={keys.hardware_merkle_root} head={12} tail={10} />
              </div>

              <div className="flex flex-wrap items-center justify-between gap-2 border-t border-slate-800/70 pt-1">
                <div>
                  <div className="text-slate-300">Deed Oracle SLH-DSA Public Key</div>
                  <div className="font-mono text-[10px] text-sky-400/80">
                    NIST FIPS 205 (SHA2-128f)
                  </div>
                </div>
                <HashPill hash={keys.deed_oracle_slh_dsa_pk_hex} head={12} tail={10} />
              </div>

              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <div className="text-slate-300">Hardware TEE SLH-DSA Public Key</div>
                  <div className="font-mono text-[10px] text-sky-400/80">
                    NIST FIPS 205 (SHA2-128f)
                  </div>
                </div>
                <HashPill hash={keys.hardware_slh_dsa_pk_hex} head={12} tail={10} />
              </div>
            </div>
          </div>
        </div>
      </Panel>

      <div className="grid grid-cols-1 gap-5 xl:grid-cols-2">
        <Panel
          title="Perpetual Purpose Trust — Section 4.2 (Deterministic Cryptographic Execution)"
          subtitle="Statutory Non-Charitable Purpose Trust (DE 12 Del. C. § 3556 / WY W.S. § 4-10-410 / OR ORS 130.193)"
          icon={<Scale className="h-4 w-4 text-emerald-400" />}
        >
          <div className="space-y-3 rounded-lg border border-slate-800 bg-slate-950/75 p-4 text-xs leading-relaxed text-slate-300">
            <p>
              <strong className="text-slate-100">4.2(a) Exclusive Execution Trigger.</strong> The
              Trustee holds bare legal title with zero discretionary conveyance or disbursement
              authority, acting exclusively upon independent verification of a valid{' '}
              <Token>{state.protocol_version}</Token> state transition (and upon soft-fork
              activation, an <Token>OP_VERIFY_COVENANT_STARK (0xb4)</Token> proof) referencing
              immutable covenant rule hash{' '}
              <Token>{truncateHash(state.covenant_rule_hash, 12, 8)}</Token>.
            </p>
            <p>
              <strong className="text-slate-100">4.2(b) Non-Enclosure &amp; Compounding.</strong>{' '}
              All Trust real property is held in perpetual non-enclosable stewardship; net PayWord
              surplus allocated at <Token>{`${rule.reinvest_bps} bps (${(rule.reinvest_bps / 100).toFixed(2)}%)`}</Token>{' '}
              cannot be diverted to private dividends or creditors.
            </p>
            <p>
              <strong className="text-slate-100">4.2(c) Ecological Coherence Floor.</strong>{' '}
              Physical deployment bounties (capped at{' '}
              <Token>{`${rule.max_bounty_payout.toLocaleString()} LOVE`}</Token>) require
              post-quantum hardware telemetry under Schedule B root{' '}
              <Token>{truncateHash(keys.hardware_merkle_root, 10, 6)}</Token> or SLH-DSA key{' '}
              <Token>{truncateHash(keys.hardware_slh_dsa_pk_hex, 10, 6)}</Token> verifying{' '}
              <code className="font-mono text-emerald-300">soil_health_score</code>{' '}
              <Token>{`≥ ${rule.min_soil_health_score}/100`}</Token>.
            </p>
          </div>
        </Panel>

        <Panel
          title="Wyoming DUNA Charter & 99-Year CLT Ground Lease"
          subtitle="Wyoming Decentralized Unincorporated Nonprofit Association Act (W.S. 17-32-101 et seq.)"
          icon={<Building2 className="h-4 w-4 text-amber-400" />}
        >
          <div className="space-y-3 rounded-lg border border-slate-800 bg-slate-950/75 p-4 text-xs leading-relaxed text-slate-300">
            <p>
              <strong className="text-slate-100">Nonprofit Operating Bridge.</strong> Operates under
              Wyoming W.S. 17-32-101 et seq. (providing legal personhood, EIN tax compliance, vendor
              contracting, and member liability protection while prohibiting private dividend
              distributions under W.S. 17-32-109).
            </p>
            <p>
              <strong className="text-slate-100">CLI Rendering Workflow.</strong> Render full
              execution drafts bound to these exact hashes using{' '}
              <code className="font-mono text-emerald-300">
                python3 legal/render_airlock.py --schedule legal/schedule-generated.json
              </code>
              .
            </p>
          </div>
        </Panel>
      </div>
    </div>
  );
}
