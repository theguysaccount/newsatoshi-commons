import type {
  AnchorReceiptRecord,
  CommonsState,
  CovenantRule,
  FundingRecord,
  KeysSummary,
  LedgerSummary,
  MainnetInfo,
  ParcelRecord,
  SeedChainStatus,
  SeedRecentBlock,
  SeedStatusResponse,
  StarkProofRecord,
  StreamingRecord,
  TelemetryRecord,
  TelemetrySeriesPoint,
  UtxoRecord,
} from '../types';

type Rec = Record<string, unknown>;

function asRec(v: unknown): Rec {
  return v && typeof v === 'object' && !Array.isArray(v) ? (v as Rec) : {};
}

function asArr(v: unknown): unknown[] {
  return Array.isArray(v) ? v : [];
}

function asNum(v: unknown, fallback = 0): number {
  if (typeof v === 'number' && Number.isFinite(v)) return v;
  if (typeof v === 'string' && v.trim() !== '') {
    const parsed = Number(v);
    if (Number.isFinite(parsed)) return parsed;
  }
  return fallback;
}

function asNullableNum(v: unknown): number | null {
  if (v === null || v === undefined) return null;
  if (typeof v === 'number' && Number.isFinite(v)) return v;
  if (typeof v === 'string' && v.trim() !== '') {
    const parsed = Number(v);
    if (Number.isFinite(parsed)) return parsed;
  }
  return null;
}

function asBool(v: unknown, fallback = false): boolean {
  if (typeof v === 'boolean') return v;
  return fallback;
}

export function asHex(v: unknown, fallback = ''): string {
  if (typeof v === 'string') return v;
  if (Array.isArray(v) && v.every((x) => typeof x === 'number')) {
    return (v as number[])
      .map((b) => (b & 0xff).toString(16).padStart(2, '0'))
      .join('');
  }
  return fallback;
}

function asStr(v: unknown, fallback = ''): string {
  if (typeof v === 'string') return v;
  if (typeof v === 'number') return String(v);
  return fallback;
}

export function normalizeCovenantRule(raw: unknown): CovenantRule {
  const r = asRec(raw);
  return {
    max_bounty_payout: asNum(r.max_bounty_payout, 0),
    reinvest_bps: asNum(r.reinvest_bps, 0),
    min_soil_health_score: asNum(r.min_soil_health_score, 0),
    authorized_deed_oracle_pk: asHex(r.authorized_deed_oracle_pk, ''),
    authorized_hardware_pk: asHex(r.authorized_hardware_pk, ''),
  };
}

export function normalizeLedgerSummary(raw: unknown): LedgerSummary {
  const r = asRec(raw);
  return {
    balance_love: asNum(r.balance_love, 0),
    tx_nonce: asNum(r.tx_nonce, 0),
    prev_state_root: asHex(r.prev_state_root, ''),
    state_root: asHex(r.state_root, ''),
    funding_deposits_count: asNum(r.funding_deposits_count, 0),
    parcels_acquired_count: asNum(r.parcels_acquired_count, 0),
    total_kwh_verified: asNum(r.total_kwh_verified, 0),
    total_energy_bounty_love: asNum(r.total_energy_bounty_love, 0),
    verified_love_per_kwh: asNullableNum(r.verified_love_per_kwh),
    total_streaming_gross_love: asNum(r.total_streaming_gross_love, 0),
    total_streaming_reinvested_love: asNum(r.total_streaming_reinvested_love, 0),
    merkle_leaves_consumed: asNum(r.merkle_leaves_consumed, 0),
    slh_dsa_signatures_verified: asNum(r.slh_dsa_signatures_verified, 0),
    stark_proofs_verified: asNum(r.stark_proofs_verified, 0),
  };
}

export function normalizeFunding(raw: unknown, idx: number): FundingRecord {
  const r = asRec(raw);
  return {
    id: asStr(r.id, `fund-${idx + 1}`),
    l1_network: asStr(r.l1_network, ''),
    l1_txid: asHex(r.l1_txid, ''),
    amount_love: asNum(r.amount_love, 0),
    note: asStr(r.note, ''),
    utxo_txid: asHex(r.utxo_txid, ''),
    recorded_at_unix: asNum(r.recorded_at_unix, 0),
  };
}

export function normalizeParcel(raw: unknown): ParcelRecord {
  const r = asRec(raw);
  return {
    parcel_id: asNum(r.parcel_id, 0),
    acres: asNum(r.acres, 0),
    price_love: asNum(r.price_love, 0),
    deed_label: asStr(r.deed_label, ''),
    txid: asHex(r.txid, ''),
    signature_scheme: asStr(r.signature_scheme, ''),
    leaf_index: asNullableNum(r.leaf_index),
    slh_nonce: asNullableNum(r.slh_nonce),
    acquired_at_unix: asNum(r.acquired_at_unix, 0),
    stark_proof_id: r.stark_proof_id ? asStr(r.stark_proof_id) : null,
  };
}

export function normalizeUtxo(raw: unknown): UtxoRecord {
  const r = asRec(raw);
  const covenant = r.covenant_rule_hash ?? r.covenant_hash;
  return {
    txid: asHex(r.txid, ''),
    vout: asNum(r.vout, 0),
    amount_love: asNum(r.amount_love ?? r.amount, 0),
    covenant_rule_hash: covenant ? asHex(covenant) : null,
    owner_tag: asStr(r.owner_tag, ''),
  };
}

export function normalizeTelemetry(raw: unknown, idx: number): TelemetryRecord {
  const r = asRec(raw);
  const blocked = asBool(r.blocked, false);
  return {
    id: asStr(r.id, `tel-${idx + 1}`),
    parcel_id: asNum(r.parcel_id, 0),
    kwh_delivered: asNum(r.kwh_delivered, 0),
    soil_health_score: asNum(r.soil_health_score, 0),
    payout_love: asNum(r.payout_love, 0),
    device_id: asStr(r.device_id, ''),
    signature_scheme: asStr(r.signature_scheme, ''),
    leaf_index: asNullableNum(r.leaf_index),
    slh_nonce: asNullableNum(r.slh_nonce),
    txid: r.txid ? asHex(r.txid) : undefined,
    timestamp_unix: asNum(r.timestamp_unix, 0),
    stark_proof_id: r.stark_proof_id ? asStr(r.stark_proof_id) : null,
    verified: !blocked,
    blocked,
    attack_type: r.attack_type ? asStr(r.attack_type) : null,
    error: r.error ? asStr(r.error) : null,
  };
}

export function normalizeStreaming(raw: unknown, idx: number): StreamingRecord {
  const r = asRec(raw);
  return {
    id: asStr(r.id, `stream-${idx + 1}`),
    ticks: asNum(r.ticks, 0),
    unit_price: asNum(r.unit_price, 0),
    gross_love: asNum(r.gross_love, 0),
    reinvested_love: asNum(r.reinvested_love, 0),
    operator_paid_love: asNum(r.operator_paid_love, 0),
    operator_tag: asStr(r.operator_tag, ''),
    payword_tip: r.channel_tip_hex ? asHex(r.channel_tip_hex) : undefined,
    timestamp_unix: asNum(r.timestamp_unix, 0),
    stark_proof_id: r.stark_proof_id ? asStr(r.stark_proof_id) : null,
  };
}

export function normalizeStarkProof(raw: unknown, idx: number): StarkProofRecord {
  const r = asRec(raw);
  const pub = asRec(r.public_inputs);
  const rawKind = r.transition_kind ?? pub.tx_kind;
  const kind =
    typeof rawKind === 'string' && rawKind.trim() !== ''
      ? rawKind
      : asStr(pub.label, 'Transition');
  return {
    proof_id: asStr(r.proof_id, `stark-${idx + 1}`),
    transition_kind: kind,
    proof_bytes_len: asNum(r.proof_bytes_len, 0),
    prove_ms: asNum(r.prove_ms, 0),
    verify_ms: asNum(r.verify_ms, 0),
    security_bits: asNum(r.security_bits, 0),
    verified: asBool(r.verified, false),
    covenant_rule_hash: asHex(r.covenant_rule_hash ?? pub.covenant_rule_hash, ''),
    state_root: asHex(r.state_root ?? pub.new_state_root, ''),
    proof_digest: asHex(r.proof_digest ?? r.proof_sha256, ''),
    timestamp_unix: asNum(r.timestamp_unix ?? r.created_at_unix, 0),
  };
}

export function normalizeAnchorReceipt(raw: unknown, idx: number): AnchorReceiptRecord {
  const r = asRec(raw);
  const broadcastStatus =
    asStr(r.broadcast_status, 'unbroadcast') === 'broadcast' ? 'broadcast' : 'unbroadcast';
  return {
    id: asStr(r.id, `anchor-${asNum(r.rpc_id, idx + 1)}`),
    mode: asStr(r.mode, 'unbroadcast'),
    broadcast_status: broadcastStatus,
    network: asStr(r.network, ''),
    l1_txid: r.l1_txid ? asHex(r.l1_txid) : null,
    anchor_hash: asHex(r.anchor_hash, ''),
    op_return_hex: asHex(r.op_return_script_hex ?? r.op_return_hex, ''),
    stark_witness_hex: asHex(r.stark_witness_commitment_hex ?? r.stark_witness_hex, ''),
    tx_nonce: asNum(r.tx_nonce, 0),
    parcels_acquired: asNum(r.parcels_acquired, 0),
    total_kwh_verified: asNum(r.total_kwh_verified, 0),
    l1_height: asNullableNum(r.l1_height),
    l1_tip_hash: r.l1_tip_hash ? asHex(r.l1_tip_hash) : null,
    l1_genesis: r.l1_genesis ? asHex(r.l1_genesis) : null,
    auxpow_chainid: asNullableNum(r.auxpow_chainid),
    cli_lock_command: asStr(r.cli_lock_command, ''),
    cli_memo_command: asStr(r.cli_send_memo_command ?? r.cli_memo_command, ''),
    timestamp_unix: asNum(r.created_at_unix ?? r.timestamp_unix, 0),
    verified_on_l1: asBool(r.verified_on_l1, false),
    l1_lock_id: r.l1_lock_id ? asHex(r.l1_lock_id) : null,
    l1_confirmed_height: asNullableNum(r.l1_confirmed_height),
    l1_confirmations: asNullableNum(r.l1_confirmations),
    l1_lock_state: r.l1_lock_state ? asStr(r.l1_lock_state) : null,
    l1_verified_at_unix: asNullableNum(r.l1_verified_at_unix),
  };
}

export function normalizeSeriesPoint(raw: unknown, idx: number): TelemetrySeriesPoint {
  const r = asRec(raw);
  return {
    step: asNum(r.step, idx + 1),
    label: asStr(r.label, `Step ${idx + 1}`),
    parcels: asNum(r.parcels, 0),
    total_kwh: asNum(r.total_kwh, 0),
    commons_balance_love: asNum(r.commons_balance_love, 0),
    verified_love_per_kwh: asNullableNum(r.verified_love_per_kwh),
  };
}

export function normalizeKeys(raw: unknown): KeysSummary {
  const r = asRec(raw);
  return {
    deed_oracle_merkle_root: asHex(r.deed_oracle_merkle_root, ''),
    hardware_merkle_root: asHex(r.hardware_merkle_root, ''),
    deed_oracle_slh_dsa_pk_hex: asHex(r.deed_oracle_slh_dsa_pk_hex, ''),
    hardware_slh_dsa_pk_hex: asHex(r.hardware_slh_dsa_pk_hex, ''),
    hardware_remaining_leaves: asNum(r.hardware_remaining_leaves, 0),
    deed_oracle_remaining_leaves: asNum(r.deed_oracle_remaining_leaves, 0),
  };
}

export function normalizeCommonsState(raw: unknown): CommonsState {
  const r = asRec(raw);
  return {
    protocol_version: asStr(r.protocol_version, ''),
    verify_only: asBool(r.verify_only, false),
    signing_keys_loaded: asBool(r.signing_keys_loaded, false),
    covenant_rule: normalizeCovenantRule(r.covenant_rule),
    covenant_rule_hash: asHex(r.covenant_rule_hash, ''),
    ledger_summary: normalizeLedgerSummary(r.ledger_summary),
    funding_records: asArr(r.funding_records).map(normalizeFunding),
    parcels: asArr(r.parcels).map(normalizeParcel),
    utxos: asArr(r.utxos).map(normalizeUtxo),
    telemetry_history: asArr(r.telemetry_history).map(normalizeTelemetry),
    streaming_history: asArr(r.streaming_history).map(normalizeStreaming),
    stark_proofs: asArr(r.stark_proofs).map(normalizeStarkProof),
    anchor_receipts: asArr(r.anchor_receipts).map(normalizeAnchorReceipt),
    telemetry_series: asArr(r.telemetry_series).map(normalizeSeriesPoint),
    keys: normalizeKeys(r.keys),
  };
}

function normalizeChainStatus(raw: unknown): SeedChainStatus | null {
  if (!raw || typeof raw !== 'object') return null;
  const rs = asRec(raw);
  return {
    network: asStr(rs.network, ''),
    proofs: rs.proofs ? asStr(rs.proofs) : undefined,
    bits: rs.bits ? asStr(rs.bits) : undefined,
    version: asStr(rs.version, ''),
    height: asNum(rs.height, 0),
    tip: asHex(rs.tip, ''),
    genesis: asHex(rs.genesis, ''),
    difficulty: asNum(rs.difficulty, 0),
    peers: asNum(rs.peers, 0),
    issued: asNum(rs.issued, 0),
    notes: asNum(rs.notes, 0),
    nullifiers: asNum(rs.nullifiers, 0),
    locks: asNum(rs.locks, 0),
    conditions: asNum(rs.conditions, 0),
    auxpow_chainid: asNum(rs.auxpow_chainid, 0),
    mempool: rs.mempool !== undefined ? asNum(rs.mempool, 0) : undefined,
    chainwork: rs.chainwork ? asStr(rs.chainwork) : undefined,
    recent: asArr(rs.recent).map((b): SeedRecentBlock => {
      const br = asRec(b);
      return {
        height: asNum(br.height, 0),
        hash: asHex(br.hash, ''),
        time: asNum(br.time, 0),
        txs: asNum(br.txs, 0),
        bytes: asNum(br.bytes, 0),
        merge_mined: asBool(br.merge_mined, false),
      };
    }),
  };
}

export function normalizeSeedResponse(raw: unknown): SeedStatusResponse {
  const r = asRec(raw);
  const rawMain = r.mainnet_info && typeof r.mainnet_info === 'object' ? asRec(r.mainnet_info) : null;

  const status = normalizeChainStatus(r.status);
  const mainnet_status = normalizeChainStatus(r.mainnet_status);

  const mainnet_info: MainnetInfo | null = rawMain
    ? {
        network: asStr(rawMain.network, 'mainnet'),
        genesis: asHex(rawMain.genesis, ''),
        height: asNum(rawMain.height, 0),
        note: asStr(rawMain.note, ''),
        auxpow_chainid:
          rawMain.auxpow_chainid !== undefined ? asNum(rawMain.auxpow_chainid, 0) : undefined,
      }
    : null;

  return {
    live: asBool(r.live, false) && status !== null,
    endpoint: asStr(r.endpoint, ''),
    fetched_at_unix: asNum(r.fetched_at_unix, 0),
    status,
    mainnet_info,
    mainnet_live: asBool(r.mainnet_live, false) && mainnet_status !== null,
    mainnet_endpoint: r.mainnet_endpoint ? asStr(r.mainnet_endpoint) : null,
    mainnet_status,
    mainnet_error: r.mainnet_error ? asStr(r.mainnet_error) : null,
    error: r.error ? asStr(r.error) : null,
  };
}

/** Formats Newsatoshi L1 atomic units (1 LOVE = 10^8 atomic units). */
export function formatL1IssuedLove(rawAtomic: number): string {
  const coins = rawAtomic / 1e8;
  return coins.toLocaleString(undefined, { maximumFractionDigits: 2 });
}
