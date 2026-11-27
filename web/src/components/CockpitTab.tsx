import { useMemo, useState } from 'react';
import {
  Activity,
  Anchor,
  ArrowDownToLine,
  CheckCircle2,
  Coins,
  Cpu,
  KeyRound,
  Landmark,
  Layers,
  Loader2,
  Radio,
  ShieldCheck,
  Terminal,
  Zap,
} from 'lucide-react';
import type {
  AcquireParcelPayload,
  CommonsState,
  ConfirmAnchorPayload,
  RecordFundingPayload,
  SeedStatusResponse,
  SignatureScheme,
  StreamSettlePayload,
  TelemetryPayload,
  VerifyStarkResponse,
} from '../types';
import { CostCurveChart } from './CostCurveChart';
import { CopyButton, HashPill, truncateHash } from './ui/CopyButton';
import { Panel } from './ui/Panel';

interface CockpitTabProps {
  state: CommonsState;
  seed: SeedStatusResponse | null;
  busyAction: string | null;
  onRecordFunding: (payload: RecordFundingPayload) => Promise<void>;
  onAcquireParcel: (payload: AcquireParcelPayload) => Promise<void>;
  onSubmitTelemetry: (payload: TelemetryPayload) => Promise<void>;
  onSettleStream: (payload: StreamSettlePayload) => Promise<void>;
  onAnchorL1: (network: 'testnet' | 'mainnet') => Promise<void>;
  onConfirmAnchor: (payload: ConfirmAnchorPayload) => Promise<void>;
  onVerifyAnchor: (anchorHash: string) => Promise<void>;
  onVerifyStark: (proofId: string) => Promise<VerifyStarkResponse | null>;
}

function formatTs(unix: number): string {
  if (!unix) return '—';
  return new Date(unix * 1000).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

export function CockpitTab({
  state,
  seed,
  busyAction,
  onRecordFunding,
  onAcquireParcel,
  onSubmitTelemetry,
  onSettleStream,
  onAnchorL1,
  onConfirmAnchor,
  onVerifyAnchor,
  onVerifyStark,
}: CockpitTabProps) {
  const { ledger_summary: summary, covenant_rule: rule, keys } = state;

  // Stage 0: Record L1 Funding (all inputs start blank)
  const [fundNetwork, setFundNetwork] = useState<'testnet' | 'mainnet' | 'regtest'>('testnet');
  const [fundTxid, setFundTxid] = useState<string>('');
  const [fundAmount, setFundAmount] = useState<string>('');
  const [fundNote, setFundNote] = useState<string>('');

  // Stage 1: Acquire Parcel (all inputs start blank)
  const [parcelId, setParcelId] = useState<string>('');
  const [acres, setAcres] = useState<string>('');
  const [priceLove, setPriceLove] = useState<string>('');
  const [deedLabel, setDeedLabel] = useState<string>('');
  const [deedScheme, setDeedScheme] = useState<SignatureScheme>('slh_dsa');

  // Stage 2: Hardware Telemetry (all inputs start blank)
  const [telParcelId, setTelParcelId] = useState<string>('');
  const [deviceId, setDeviceId] = useState<string>('');
  const [kwhDelivered, setKwhDelivered] = useState<string>('');
  const [soilScore, setSoilScore] = useState<string>('');
  const [payoutLove, setPayoutLove] = useState<string>('');
  const [telScheme, setTelScheme] = useState<SignatureScheme>('slh_dsa');

  // Stage 3: PayWord Streaming (all inputs start blank)
  const [ticks, setTicks] = useState<string>('');
  const [unitPrice, setUnitPrice] = useState<string>('');
  const [operatorTag, setOperatorTag] = useState<string>('');

  // Stage 4: Anchor + Confirm broadcast txid
  const [anchorNet, setAnchorNet] = useState<'testnet' | 'mainnet'>('testnet');
  const [confirmTxid, setConfirmTxid] = useState<string>('');

  // STARK Re-verify state
  const [verifyingProofId, setVerifyingProofId] = useState<string | null>(null);
  const [verifiedResults, setVerifiedResults] = useState<Record<string, VerifyStarkResponse>>({});

  const parsedTicks = Number(ticks);
  const parsedUnit = Number(unitPrice);
  const grossStream =
    Number.isFinite(parsedTicks) && parsedTicks > 0 && Number.isFinite(parsedUnit) && parsedUnit > 0
      ? Math.floor(parsedTicks) * Math.floor(parsedUnit)
      : 0;
  const reinvestStream = Math.floor((grossStream * rule.reinvest_bps) / 10000);
  const operatorStream = grossStream - reinvestStream;

  const latestAnchor =
    state.anchor_receipts.length > 0
      ? state.anchor_receipts[state.anchor_receipts.length - 1]
      : null;

  const auditFeed = useMemo(() => {
    const items: {
      id: string;
      kind: 'funding' | 'parcel' | 'telemetry' | 'stream' | 'anchor';
      title: string;
      subtitle: string;
      badge: string;
      timestamp: number;
      hash?: string;
    }[] = [];

    for (const f of state.funding_records) {
      items.push({
        id: f.id,
        kind: 'funding',
        title: `L1 Funding Recorded (+${f.amount_love.toLocaleString()} LOVE)`,
        subtitle: `${f.l1_network.toUpperCase()}${f.note ? ` • ${f.note}` : ''}`,
        badge: 'L1 FUNDING',
        timestamp: f.recorded_at_unix,
        hash: f.l1_txid,
      });
    }
    for (const p of state.parcels) {
      items.push({
        id: `parcel-${p.txid || p.parcel_id}`,
        kind: 'parcel',
        title: `Acquired Parcel #${p.parcel_id} (${p.acres} ac) — ${p.deed_label}`,
        subtitle: `${p.price_love.toLocaleString()} LOVE • ${p.signature_scheme.toUpperCase()}`,
        badge: 'DEED',
        timestamp: p.acquired_at_unix,
        hash: p.txid,
      });
    }
    for (const t of state.telemetry_history) {
      items.push({
        id: t.id,
        kind: 'telemetry',
        title: t.blocked
          ? `Rejected Telemetry (${t.device_id})`
          : `Verified +${t.kwh_delivered.toLocaleString()} kWh (Parcel #${t.parcel_id})`,
        subtitle: t.blocked
          ? t.error || 'Rejected by covenant predicate'
          : `Soil ${t.soil_health_score}/100 • ${t.payout_love.toLocaleString()} LOVE • ${t.signature_scheme.toUpperCase()}`,
        badge: t.blocked ? 'REJECTED' : 'TELEMETRY',
        timestamp: t.timestamp_unix,
        hash: t.txid,
      });
    }
    for (const s of state.streaming_history) {
      items.push({
        id: s.id,
        kind: 'stream',
        title: `PayWord Settled (${s.ticks.toLocaleString()} × ${s.unit_price} LOVE)`,
        subtitle: `+${s.reinvested_love.toLocaleString()} LOVE to Commons • ${s.operator_paid_love.toLocaleString()} LOVE to ${s.operator_tag}`,
        badge: 'PAYWORD',
        timestamp: s.timestamp_unix,
        hash: s.payword_tip,
      });
    }
    for (const r of state.anchor_receipts) {
      items.push({
        id: r.id,
        kind: 'anchor',
        title: `L1 Anchor (${r.network.toUpperCase()}${r.l1_height !== null ? ` #${r.l1_height.toLocaleString()}` : ''})`,
        subtitle: `Nonce #${r.tx_nonce} • ${r.parcels_acquired} parcels • ${r.total_kwh_verified.toLocaleString()} kWh • ${r.broadcast_status.toUpperCase()}`,
        badge: r.broadcast_status === 'broadcast' ? 'CONFIRMED' : 'UNBROADCAST',
        timestamp: r.timestamp_unix,
        hash: r.l1_txid ?? r.anchor_hash,
      });
    }
    return items.sort((a, b) => b.timestamp - a.timestamp);
  }, [state]);

  const handleReverify = async (proofId: string) => {
    setVerifyingProofId(proofId);
    try {
      const res = await onVerifyStark(proofId);
      if (res) setVerifiedResults((prev) => ({ ...prev, [proofId]: res }));
    } finally {
      setVerifyingProofId(null);
    }
  };

  const ingestUrl =
    typeof window !== 'undefined'
      ? `${window.location.origin}/api/telemetry`
      : 'http://127.0.0.1:8791/api/telemetry';

  const hwPipelineCmd = `newsatoshi-hw-signer daemon --keyfile ./hw-key.json --endpoint ${ingestUrl} --scheme slh_dsa --stdin-jsonl`;

  return (
    <div className="space-y-6">
      {/* 5 Honest Zero/Observed KPI Cards */}
      <div className="grid grid-cols-1 gap-3.5 sm:grid-cols-2 lg:grid-cols-5">
        <div className="glass-panel rounded-xl p-4">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-medium uppercase tracking-wider text-slate-400">
              Commons Treasury
            </span>
            <Coins className="h-4 w-4 text-emerald-400" />
          </div>
          <div className="mt-2 font-mono text-2xl font-bold text-slate-50 tabular-nums">
            {summary.balance_love.toLocaleString()}{' '}
            <span className="text-xs font-semibold text-emerald-400">LOVE</span>
          </div>
          <div className="mt-1.5 flex items-center justify-between font-mono text-[10.5px] text-slate-400">
            <span>Nonce #{summary.tx_nonce}</span>
            <span>{summary.funding_deposits_count} L1 deposits</span>
          </div>
        </div>

        <div className="glass-panel rounded-xl p-4">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-medium uppercase tracking-wider text-slate-400">
              Verified Rate
            </span>
            <Activity className="h-4 w-4 text-sky-400" />
          </div>
          <div className="mt-2 font-mono text-2xl font-bold text-sky-300 tabular-nums">
            {summary.verified_love_per_kwh !== null
              ? summary.verified_love_per_kwh.toFixed(4)
              : '—'}
          </div>
          <div className="mt-1.5 font-mono text-[10.5px] text-slate-400">
            {summary.verified_love_per_kwh !== null
              ? 'LOVE / verified kWh'
              : 'No metered telemetry yet'}
          </div>
        </div>

        <div className="glass-panel rounded-xl p-4">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-medium uppercase tracking-wider text-slate-400">
              Parcels in Trust
            </span>
            <Landmark className="h-4 w-4 text-emerald-400" />
          </div>
          <div className="mt-2 font-mono text-2xl font-bold text-slate-50 tabular-nums">
            {summary.parcels_acquired_count}
          </div>
          <div className="mt-1.5 font-mono text-[10.5px] text-slate-400">
            {state.parcels.length > 0
              ? `${state.parcels.reduce((a, p) => a + p.acres, 0).toFixed(1)} acres recorded`
              : 'No parcels acquired yet'}
          </div>
        </div>

        <div className="glass-panel rounded-xl p-4">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-medium uppercase tracking-wider text-slate-400">
              Verified Clean Energy
            </span>
            <Zap className="h-4 w-4 text-amber-400" />
          </div>
          <div className="mt-2 font-mono text-2xl font-bold text-amber-300 tabular-nums">
            {summary.total_kwh_verified.toLocaleString()}{' '}
            <span className="text-xs font-semibold">kWh</span>
          </div>
          <div className="mt-1.5 font-mono text-[10.5px] text-slate-400">
            Soil floor ≥ {rule.min_soil_health_score}/100
          </div>
        </div>

        <div className="glass-panel rounded-xl p-4">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-medium uppercase tracking-wider text-slate-400">
              Winterfell STARKs
            </span>
            <ShieldCheck className="h-4 w-4 text-sky-400" />
          </div>
          <div className="mt-2 font-mono text-2xl font-bold text-sky-300 tabular-nums">
            {summary.stark_proofs_verified}
          </div>
          <div className="mt-1.5 font-mono text-[10.5px] text-slate-400">
            SLH-DSA: {summary.slh_dsa_signatures_verified} · XMSS: {summary.merkle_leaves_consumed}
          </div>
        </div>
      </div>

      {/* Real Persisted Ledger History Chart (Empty State when 0) */}
      <CostCurveChart points={state.telemetry_series} />

      {/* Stage 0: Explicit L1 Funding Deposit */}
      <Panel
        title="0. Record Newsatoshi L1 Covenant Funding"
        subtitle="Treasury starts at 0 LOVE — record an actual Newsatoshi L1 funding txid / lock hash to credit the covenant UTXO"
        icon={<ArrowDownToLine className="h-4 w-4 text-emerald-400" />}
        accent="emerald"
        badge={
          <span className="font-mono text-[10px] text-slate-400">
            {state.funding_records.length} recorded
          </span>
        }
      >
        <form
          className="grid grid-cols-1 gap-3 md:grid-cols-12"
          onSubmit={(e) => {
            e.preventDefault();
            const amt = Number(fundAmount);
            if (!fundTxid.trim() || !Number.isFinite(amt) || amt <= 0) return;
            void onRecordFunding({
              l1_network: fundNetwork,
              l1_txid: fundTxid.trim(),
              amount_love: Math.floor(amt),
              note: fundNote.trim(),
            });
          }}
        >
          <label className="space-y-1 md:col-span-2">
            <span className="block text-[11px] text-slate-400">Network</span>
            <select
              value={fundNetwork}
              onChange={(e) => setFundNetwork(e.target.value as 'testnet' | 'mainnet' | 'regtest')}
              className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-2.5 py-2 font-mono text-xs text-slate-100 focus:border-emerald-500 focus:outline-none"
            >
              <option value="testnet">testnet</option>
              <option value="mainnet">mainnet</option>
              <option value="regtest">regtest</option>
            </select>
          </label>
          <label className="space-y-1 md:col-span-5">
            <span className="block text-[11px] text-slate-400">Real L1 Txid / Lock Hash (64 hex)</span>
            <input
              type="text"
              required
              placeholder="64-character hex Newsatoshi L1 transaction or lock hash"
              value={fundTxid}
              onChange={(e) => setFundTxid(e.target.value)}
              className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600 focus:border-emerald-500 focus:outline-none"
            />
          </label>
          <label className="space-y-1 md:col-span-2">
            <span className="block text-[11px] text-slate-400">Amount (LOVE)</span>
            <input
              type="number"
              min={1}
              step={1}
              required
              placeholder="LOVE units"
              value={fundAmount}
              onChange={(e) => setFundAmount(e.target.value)}
              className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600 focus:border-emerald-500 focus:outline-none"
            />
          </label>
          <label className="space-y-1 md:col-span-2">
            <span className="block text-[11px] text-slate-400">Memo / Source</span>
            <input
              type="text"
              placeholder="Optional note"
              value={fundNote}
              onChange={(e) => setFundNote(e.target.value)}
              className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 text-xs text-slate-100 placeholder:text-slate-600 focus:border-emerald-500 focus:outline-none"
            />
          </label>
          <div className="flex items-end md:col-span-1">
            <button
              type="submit"
              disabled={busyAction !== null}
              className="w-full rounded-lg bg-emerald-500 px-3 py-2 text-xs font-semibold text-slate-950 transition hover:bg-emerald-400 cursor-pointer disabled:opacity-50"
            >
              {busyAction === 'fund' ? '…' : 'Record'}
            </button>
          </div>
        </form>
      </Panel>

      {/* 4 Core Operations */}
      <div className="grid grid-cols-1 gap-5 xl:grid-cols-2">
        {/* Stage 1: Acquire Land Parcel */}
        <Panel
          title="1. Record Verified Land Parcel Acquisition"
          subtitle="Spends funded covenant UTXO, enforces recursive output continuity, and generates a Winterfell 0.13 STARK proof"
          icon={<Landmark className="h-4 w-4 text-emerald-400" />}
        >
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault();
              void onAcquireParcel({
                parcel_id: Number(parcelId),
                acres: Number(acres),
                price_love: Number(priceLove),
                deed_label: deedLabel.trim(),
                signature_scheme: deedScheme,
              });
            }}
          >
            <div className="grid grid-cols-3 gap-3">
              <label className="space-y-1">
                <span className="block text-[11px] text-slate-400">Parcel ID</span>
                <input
                  type="number"
                  min={1}
                  step={1}
                  required
                  placeholder="Parcel #"
                  value={parcelId}
                  onChange={(e) => setParcelId(e.target.value)}
                  className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-emerald-500 focus:outline-none"
                />
              </label>
              <label className="space-y-1">
                <span className="block text-[11px] text-slate-400">Acres</span>
                <input
                  type="number"
                  min={0.1}
                  step="any"
                  required
                  placeholder="Acres"
                  value={acres}
                  onChange={(e) => setAcres(e.target.value)}
                  className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-emerald-500 focus:outline-none"
                />
              </label>
              <label className="space-y-1">
                <span className="block text-[11px] text-slate-400">Price (LOVE)</span>
                <input
                  type="number"
                  min={1}
                  step={1}
                  required
                  placeholder="LOVE"
                  value={priceLove}
                  onChange={(e) => setPriceLove(e.target.value)}
                  className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-emerald-500 focus:outline-none"
                />
              </label>
            </div>
            <label className="block space-y-1">
              <span className="block text-[11px] text-slate-400">County Deed Label / Assessor Parcel Reference</span>
              <input
                type="text"
                required
                placeholder="Enter county parcel number / deed title"
                value={deedLabel}
                onChange={(e) => setDeedLabel(e.target.value)}
                className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 text-sm text-slate-100 placeholder:text-slate-600 focus:border-emerald-500 focus:outline-none"
              />
            </label>
            <div className="grid grid-cols-2 gap-2">
              {(['slh_dsa', 'xmss'] as const).map((scheme) => (
                <button
                  key={scheme}
                  type="button"
                  onClick={() => setDeedScheme(scheme)}
                  className={`rounded-lg border px-3 py-1.5 font-mono text-xs transition cursor-pointer ${
                    deedScheme === scheme
                      ? 'border-emerald-500/60 bg-emerald-500/15 text-emerald-200'
                      : 'border-slate-800 bg-slate-950/60 text-slate-400'
                  }`}
                >
                  {scheme === 'slh_dsa' ? 'SLH-DSA (FIPS 205)' : 'XMSS Merkle-Lamport'}
                </button>
              ))}
            </div>
            <button
              type="submit"
              disabled={busyAction !== null || summary.balance_love === 0}
              className="flex w-full items-center justify-center gap-2 rounded-lg bg-emerald-500 px-4 py-2 text-xs font-semibold text-slate-950 transition hover:bg-emerald-400 cursor-pointer disabled:cursor-not-allowed disabled:opacity-45"
            >
              {busyAction === 'acquire' ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Landmark className="h-4 w-4" />
              )}
              {summary.balance_love === 0
                ? 'Fund Covenant (Step 0) Before Recording Deed'
                : 'Sign Deed & Generate Winterfell STARK'}
            </button>
          </form>
        </Panel>

        {/* Stage 2: Hardware Telemetry Ingest */}
        <Panel
          title="2. Hardware Telemetry Ingest (POST /api/telemetry)"
          subtitle="Connect real field solar/soil gateways via newsatoshi-hw-signer or submit verified meter readings"
          icon={<Cpu className="h-4 w-4 text-amber-400" />}
          accent="amber"
        >
          <div className="space-y-3">
            <div className="rounded-lg border border-slate-800 bg-[#04070d] p-2.5">
              <div className="mb-1 flex items-center justify-between">
                <span className="inline-flex items-center gap-1.5 font-mono text-[10px] font-semibold uppercase text-amber-300">
                  <Terminal className="h-3 w-3" />
                  Live Hardware Gateway Ingest CLI
                </span>
                <CopyButton value={hwPipelineCmd} label="Copy" />
              </div>
              <code className="block break-all font-mono text-[10.5px] text-slate-300">
                {hwPipelineCmd}
              </code>
            </div>

            <form
              className="space-y-3"
              onSubmit={(e) => {
                e.preventDefault();
                void onSubmitTelemetry({
                  parcel_id: Number(telParcelId),
                  device_id: deviceId.trim(),
                  kwh_delivered: Number(kwhDelivered),
                  soil_health_score: Number(soilScore),
                  payout_love: Number(payoutLove),
                  signature_scheme: telScheme,
                });
              }}
            >
              <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
                <label className="space-y-1">
                  <span className="block text-[11px] text-slate-400">Parcel ID</span>
                  <input
                    type="number"
                    min={1}
                    step={1}
                    required
                    placeholder="Parcel #"
                    value={telParcelId}
                    onChange={(e) => setTelParcelId(e.target.value)}
                    className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-2.5 py-1.5 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-amber-500 focus:outline-none"
                  />
                </label>
                <label className="space-y-1">
                  <span className="block text-[11px] text-slate-400">Measured kWh</span>
                  <input
                    type="number"
                    min={1}
                    step={1}
                    required
                    placeholder="kWh"
                    value={kwhDelivered}
                    onChange={(e) => setKwhDelivered(e.target.value)}
                    className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-2.5 py-1.5 font-mono text-sm text-amber-200 placeholder:text-slate-600 focus:border-amber-500 focus:outline-none"
                  />
                </label>
                <label className="space-y-1">
                  <span className="block text-[11px] text-slate-400">
                    Soil (≥{rule.min_soil_health_score})
                  </span>
                  <input
                    type="number"
                    min={0}
                    max={100}
                    step={1}
                    required
                    placeholder="0–100"
                    value={soilScore}
                    onChange={(e) => setSoilScore(e.target.value)}
                    className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-2.5 py-1.5 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-amber-500 focus:outline-none"
                  />
                </label>
                <label className="space-y-1">
                  <span className="block text-[11px] text-slate-400">
                    Bounty (≤{rule.max_bounty_payout})
                  </span>
                  <input
                    type="number"
                    min={1}
                    step={1}
                    required
                    placeholder="LOVE"
                    value={payoutLove}
                    onChange={(e) => setPayoutLove(e.target.value)}
                    className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-2.5 py-1.5 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-amber-500 focus:outline-none"
                  />
                </label>
              </div>
              <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
                <input
                  type="text"
                  required
                  placeholder="Hardware meter / inverter device ID"
                  value={deviceId}
                  onChange={(e) => setDeviceId(e.target.value)}
                  className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-1.5 font-mono text-xs text-slate-100 placeholder:text-slate-600 focus:border-amber-500 focus:outline-none"
                />
                <select
                  value={telScheme}
                  onChange={(e) => setTelScheme(e.target.value as SignatureScheme)}
                  className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-1.5 font-mono text-xs text-slate-100 focus:border-amber-500 focus:outline-none"
                >
                  <option value="slh_dsa">NIST FIPS 205 SLH-DSA</option>
                  <option value="xmss">XMSS Merkle-Lamport</option>
                </select>
              </div>
              <button
                type="submit"
                disabled={busyAction !== null || summary.balance_love === 0}
                className="flex w-full items-center justify-center gap-2 rounded-lg bg-amber-500 px-4 py-2 text-xs font-semibold text-slate-950 transition hover:bg-amber-400 cursor-pointer disabled:cursor-not-allowed disabled:opacity-45"
              >
                {busyAction === 'telemetry' ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <Zap className="h-4 w-4" />
                )}
                {summary.balance_love === 0
                  ? 'Fund Covenant (Step 0) Before Disbursing Bounties'
                  : 'Verify Meter Reading & Prove STARK'}
              </button>
            </form>
          </div>
        </Panel>

        {/* Stage 3: PayWord Streaming Channel */}
        <Panel
          title="3. Settle PayWord Machine Streaming Channel"
          subtitle={`Routes ${(rule.reinvest_bps / 100).toFixed(2)}% (${rule.reinvest_bps} bps) permanently into the Land Covenant`}
          icon={<Radio className="h-4 w-4 text-sky-400" />}
        >
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault();
              void onSettleStream({
                ticks: Number(ticks),
                unit_price: Number(unitPrice),
                operator_tag: operatorTag.trim(),
              });
            }}
          >
            <div className="grid grid-cols-3 gap-3">
              <label className="space-y-1">
                <span className="block text-[11px] text-slate-400">PayWord Ticks</span>
                <input
                  type="number"
                  min={1}
                  step={1}
                  required
                  placeholder="Ticks"
                  value={ticks}
                  onChange={(e) => setTicks(e.target.value)}
                  className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-sky-500 focus:outline-none"
                />
              </label>
              <label className="space-y-1">
                <span className="block text-[11px] text-slate-400">Unit Price (LOVE)</span>
                <input
                  type="number"
                  min={1}
                  step={1}
                  required
                  placeholder="LOVE / tick"
                  value={unitPrice}
                  onChange={(e) => setUnitPrice(e.target.value)}
                  className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-3 py-2 font-mono text-sm text-slate-100 placeholder:text-slate-600 focus:border-sky-500 focus:outline-none"
                />
              </label>
              <label className="space-y-1">
                <span className="block text-[11px] text-slate-400">Operator ID</span>
                <input
                  type="text"
                  required
                  placeholder="Operator tag"
                  value={operatorTag}
                  onChange={(e) => setOperatorTag(e.target.value)}
                  className="w-full rounded-lg border border-slate-700/80 bg-slate-950/80 px-2.5 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600 focus:border-sky-500 focus:outline-none"
                />
              </label>
            </div>
            {grossStream > 0 && (
              <div className="flex items-center justify-between rounded-lg border border-slate-800 bg-slate-950/70 px-3 py-2 font-mono text-[11px] text-slate-300">
                <span>Gross: {grossStream.toLocaleString()} LOVE</span>
                <span className="text-emerald-300">
                  +{reinvestStream.toLocaleString()} Commons
                </span>
                <span className="text-sky-300">{operatorStream.toLocaleString()} Operator</span>
              </div>
            )}
            <button
              type="submit"
              disabled={busyAction !== null}
              className="flex w-full items-center justify-center gap-2 rounded-lg bg-sky-500 px-4 py-2 text-xs font-semibold text-slate-950 transition hover:bg-sky-400 cursor-pointer disabled:opacity-45"
            >
              {busyAction === 'stream' ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Radio className="h-4 w-4" />
              )}
              Settle Hash-Chain &amp; Prove STARK
            </button>
          </form>
        </Panel>

        {/* Stage 4: Anchor to Live Newsatoshi L1 */}
        <Panel
          title="4. Prepare & Confirm Live L1 State-Root Anchor"
          subtitle="Binds state_root to live newsatoshi-seed tip — marked Unbroadcast until confirmed with a real L1 txid"
          icon={<Anchor className="h-4 w-4 text-emerald-400" />}
        >
          <div className="space-y-3">
            <div className="flex flex-wrap items-center gap-2">
              {(['testnet', 'mainnet'] as const).map((net) => (
                <button
                  key={net}
                  type="button"
                  onClick={() => setAnchorNet(net)}
                  className={`rounded-md border px-3 py-1.5 font-mono text-xs uppercase cursor-pointer ${
                    anchorNet === net
                      ? 'border-emerald-500/60 bg-emerald-500/15 text-emerald-200'
                      : 'border-slate-800 bg-slate-950/70 text-slate-400'
                  }`}
                >
                  {net}
                </button>
              ))}
              <button
                type="button"
                disabled={busyAction !== null || (anchorNet === 'testnet' && !seed?.live)}
                onClick={() => void onAnchorL1(anchorNet)}
                className="ml-auto inline-flex items-center gap-2 rounded-lg bg-emerald-500 px-4 py-1.5 text-xs font-semibold text-slate-950 hover:bg-emerald-400 cursor-pointer disabled:cursor-not-allowed disabled:opacity-45"
              >
                {busyAction === 'anchor' ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <Anchor className="h-4 w-4" />
                )}
                Prepare {anchorNet.toUpperCase()} Anchor
              </button>
            </div>

            {latestAnchor ? (
              <div className="space-y-2.5 rounded-lg border border-slate-800 bg-slate-950/80 p-3 text-xs">
                <div className="flex flex-wrap items-center justify-between gap-2 border-b border-slate-800 pb-2">
                  <span className="font-mono text-[11px] font-semibold text-emerald-300">
                    Nonce #{latestAnchor.tx_nonce} ({latestAnchor.network})
                  </span>
                  <div className="flex flex-wrap items-center gap-1.5">
                    {latestAnchor.verified_on_l1 ? (
                      <span className="inline-flex items-center gap-1 rounded bg-emerald-500/25 px-1.5 py-0.5 font-mono text-[10px] font-semibold uppercase text-emerald-200 border border-emerald-500/40">
                        <ShieldCheck className="h-3 w-3" />
                        L1 VERIFIED · BLOCK #{latestAnchor.l1_confirmed_height?.toLocaleString() ?? '?'} (
                        {latestAnchor.l1_confirmations ?? 0} CONFS)
                      </span>
                    ) : (
                      <span
                        className={`rounded px-1.5 py-0.5 font-mono text-[10px] font-semibold uppercase ${
                          latestAnchor.broadcast_status === 'broadcast'
                            ? 'bg-emerald-500/20 text-emerald-300'
                            : 'bg-amber-500/20 text-amber-300'
                        }`}
                      >
                        {latestAnchor.broadcast_status === 'broadcast'
                          ? 'BROADCAST CONFIRMED'
                          : 'UNBROADCAST — CLI READY'}
                      </span>
                    )}
                    <button
                      type="button"
                      disabled={busyAction !== null}
                      onClick={() => void onVerifyAnchor(latestAnchor.anchor_hash)}
                      className="inline-flex items-center gap-1 rounded border border-sky-500/40 bg-sky-500/10 px-2 py-0.5 font-mono text-[10px] font-semibold text-sky-300 hover:bg-sky-500/20 cursor-pointer disabled:opacity-50"
                      title="Public live cross-check against Newsatoshi L1 /api/lock?hash=..."
                    >
                      {busyAction === 'verify-anchor' ? (
                        <Loader2 className="h-3 w-3 animate-spin" />
                      ) : (
                        <CheckCircle2 className="h-3 w-3" />
                      )}
                      Verify On-Chain
                    </button>
                  </div>
                </div>
                <div className="space-y-1.5">
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-[11px] text-slate-400">Bound L1 Height</span>
                    <span className="font-mono text-slate-100">
                      {latestAnchor.l1_height !== null
                        ? `#${latestAnchor.l1_height.toLocaleString()}`
                        : '—'}
                    </span>
                  </div>
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-[11px] text-slate-400">anchor_hash</span>
                    <HashPill hash={latestAnchor.anchor_hash} head={12} tail={10} />
                  </div>
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-[11px] text-slate-400">Confirmed L1 Txid</span>
                    {latestAnchor.l1_txid ? (
                      <HashPill hash={latestAnchor.l1_txid} head={12} tail={10} />
                    ) : (
                      <span className="font-mono text-[11px] text-amber-300">Unbroadcast</span>
                    )}
                  </div>
                  {latestAnchor.l1_lock_id && (
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-[11px] text-slate-400">
                        L1 On-Chain Lock ({latestAnchor.l1_lock_state || 'open'})
                      </span>
                      <HashPill hash={latestAnchor.l1_lock_id} head={12} tail={10} />
                    </div>
                  )}
                </div>
                <div className="rounded border border-emerald-500/25 bg-[#04070d] p-2">
                  <div className="mb-1 flex items-center justify-between">
                    <span className="font-mono text-[10px] uppercase text-emerald-400">
                      newsatoshi v1.2.1 L1 Lock CLI
                    </span>
                    <CopyButton value={latestAnchor.cli_lock_command} label="Copy CLI" />
                  </div>
                  <code className="block break-all font-mono text-[10.5px] text-emerald-200/95">
                    {latestAnchor.cli_lock_command}
                  </code>
                </div>
                {latestAnchor.broadcast_status !== 'broadcast' && (
                  <form
                    className="flex gap-2 pt-1"
                    onSubmit={(e) => {
                      e.preventDefault();
                      if (!confirmTxid.trim()) return;
                      void onConfirmAnchor({
                        anchor_hash: latestAnchor.anchor_hash,
                        l1_txid: confirmTxid.trim(),
                      });
                      setConfirmTxid('');
                    }}
                  >
                    <input
                      type="text"
                      placeholder="Paste real broadcast 64-hex L1 txid to confirm"
                      value={confirmTxid}
                      onChange={(e) => setConfirmTxid(e.target.value)}
                      className="flex-1 rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-[11px] text-slate-100 placeholder:text-slate-600 focus:border-emerald-500 focus:outline-none"
                    />
                    <button
                      type="submit"
                      disabled={busyAction !== null}
                      className="rounded bg-slate-800 px-3 py-1.5 font-mono text-[11px] text-emerald-300 hover:bg-slate-700 cursor-pointer"
                    >
                      Confirm Txid
                    </button>
                  </form>
                )}
              </div>
            ) : (
              <div className="rounded-lg border border-dashed border-slate-800 bg-slate-950/40 p-6 text-center text-xs text-slate-400">
                No L1 state-root anchors prepared yet.
              </div>
            )}
          </div>
        </Panel>
      </div>

      {/* Bottom: Audit Feed + Winterfell STARK Inspector + UTXOs/Keys */}
      <div className="grid grid-cols-1 gap-5 xl:grid-cols-12">
        <div className="xl:col-span-6">
          <Panel
            title="Persisted Cryptographic Audit Feed"
            subtitle="Strictly real events recorded in commons-ledger.json"
            icon={<Activity className="h-4 w-4 text-emerald-400" />}
            badge={<span className="font-mono text-[10px] text-slate-400">{auditFeed.length} records</span>}
          >
            {auditFeed.length === 0 ? (
              <div className="rounded-lg border border-dashed border-slate-800 bg-slate-950/40 px-4 py-10 text-center text-xs text-slate-400">
                No transactions recorded yet.
              </div>
            ) : (
              <div className="max-h-[420px] space-y-2 overflow-y-auto pr-1">
                {auditFeed.map((item) => (
                  <div
                    key={item.id}
                    className="rounded-lg border border-slate-800/90 bg-slate-950/75 p-3 text-xs"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="font-medium text-slate-100">{item.title}</span>
                      <span className="rounded border border-slate-700 bg-slate-900 px-1.5 py-0.5 font-mono text-[9.5px] text-slate-300">
                        {item.badge}
                      </span>
                    </div>
                    <div className="mt-1 flex flex-wrap items-center justify-between gap-2 text-[11px] text-slate-400">
                      <span>{item.subtitle}</span>
                      <span className="font-mono text-[10px] text-slate-500">
                        {formatTs(item.timestamp)}
                      </span>
                    </div>
                    {item.hash && (
                      <div className="mt-1.5 font-mono text-[10px] text-slate-500">
                        {truncateHash(item.hash, 16, 12)}
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}
          </Panel>
        </div>

        <div className="space-y-5 xl:col-span-6">
          <Panel
            title="Winterfell 0.13 STARK Proof Inspector"
            subtitle="Cryptographic FRI proofs generated and verified by the Rust node"
            icon={<ShieldCheck className="h-4 w-4 text-sky-400" />}
            badge={
              <span className="font-mono text-[10px] text-sky-300">
                {state.stark_proofs.length} proofs
              </span>
            }
          >
            {state.stark_proofs.length === 0 ? (
              <div className="rounded-lg border border-dashed border-slate-800 bg-slate-950/40 px-4 py-8 text-center text-xs text-slate-400">
                No Winterfell STARK proofs generated yet.
              </div>
            ) : (
              <div className="max-h-[260px] space-y-2.5 overflow-y-auto pr-1">
                {[...state.stark_proofs].reverse().map((proof) => {
                  const re = verifiedResults[proof.proof_id];
                  return (
                    <div
                      key={proof.proof_id}
                      className="rounded-lg border border-slate-800 bg-slate-950/80 p-3 text-xs"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-mono font-semibold text-sky-300">
                          {proof.proof_id}
                        </span>
                        <button
                          type="button"
                          disabled={verifyingProofId === proof.proof_id}
                          onClick={() => void handleReverify(proof.proof_id)}
                          className="inline-flex items-center gap-1 rounded border border-sky-500/40 bg-sky-500/15 px-2.5 py-1 font-mono text-[10.5px] font-semibold text-sky-200 hover:bg-sky-500/25 cursor-pointer"
                        >
                          {verifyingProofId === proof.proof_id ? (
                            <Loader2 className="h-3 w-3 animate-spin" />
                          ) : (
                            <CheckCircle2 className="h-3 w-3" />
                          )}
                          Re-Verify STARK
                        </button>
                      </div>
                      <div className="mt-1.5 flex flex-wrap gap-3 font-mono text-[10.5px] text-slate-400">
                        <span>{(proof.proof_bytes_len / 1024).toFixed(1)} KB</span>
                        <span>Prove {proof.prove_ms}ms</span>
                        <span>Verify {proof.verify_ms}ms</span>
                        <span>{proof.security_bits}-bit PQ</span>
                      </div>
                      {re && (
                        <div className="mt-1.5 font-mono text-[10.5px] text-emerald-300">
                          {re.valid
                            ? `VERIFIED VALID (${re.verify_ms}ms · ${re.security_bits}-bit)`
                            : `FAILED: ${re.error}`}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </Panel>

          <Panel
            title="Active UTXOs & Post-Quantum Signer Roots"
            subtitle="Live covenant rule hash, UTXO set, and XMSS / SLH-DSA public keys"
            icon={<KeyRound className="h-4 w-4 text-emerald-400" />}
          >
            <div className="space-y-2.5 text-xs">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-slate-400">covenant_rule_hash</span>
                <HashPill hash={state.covenant_rule_hash} head={12} tail={8} />
              </div>
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-slate-400">Hardware XMSS Root ({keys.hardware_remaining_leaves} leaves left)</span>
                <HashPill hash={keys.hardware_merkle_root} head={12} tail={8} />
              </div>
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-slate-400">Hardware SLH-DSA PK (FIPS 205)</span>
                <HashPill hash={keys.hardware_slh_dsa_pk_hex} head={12} tail={8} />
              </div>
              <div className="border-t border-slate-800/80 pt-2">
                <div className="mb-1.5 flex items-center gap-1.5 font-mono text-[10.5px] uppercase text-slate-500">
                  <Layers className="h-3.5 w-3.5" />
                  Covenant UTXOs ({state.utxos.length})
                </div>
                {state.utxos.length === 0 ? (
                  <div className="font-mono text-[11px] text-slate-500">
                    No UTXOs in ledger (0 LOVE).
                  </div>
                ) : (
                  <div className="space-y-1">
                    {state.utxos.map((u) => (
                      <div
                        key={`${u.txid}:${u.vout}`}
                        className="flex items-center justify-between rounded border border-slate-800 bg-slate-950/70 px-2.5 py-1.5 font-mono text-[11px]"
                      >
                        <span className="text-slate-400">{truncateHash(u.txid, 10, 6)}:{u.vout}</span>
                        <span className="text-emerald-300">{u.amount_love.toLocaleString()} LOVE</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          </Panel>
        </div>
      </div>
    </div>
  );
}
