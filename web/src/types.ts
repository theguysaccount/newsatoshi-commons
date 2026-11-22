export type SignatureScheme = 'xmss' | 'slh_dsa';

export type WorkspaceTab = 'cockpit' | 'explorer' | 'economics' | 'legal';

export interface HealthResponse {
  ok: boolean;
  version: string;
  protocol_version: string;
  uptime_secs: number;
  auth_required?: boolean;
  verify_only?: boolean;
  signing_keys_loaded?: boolean;
  mainnet_seed_url?: string | null;
}

export interface CovenantRule {
  max_bounty_payout: number;
  reinvest_bps: number;
  min_soil_health_score: number;
  authorized_deed_oracle_pk: string;
  authorized_hardware_pk: string;
}

export interface LedgerSummary {
  balance_love: number;
  tx_nonce: number;
  prev_state_root: string;
  state_root: string;
  funding_deposits_count: number;
  parcels_acquired_count: number;
  total_kwh_verified: number;
  total_energy_bounty_love: number;
  verified_love_per_kwh: number | null;
  total_streaming_gross_love: number;
  total_streaming_reinvested_love: number;
  merkle_leaves_consumed: number;
  slh_dsa_signatures_verified: number;
  stark_proofs_verified: number;
}

export interface FundingRecord {
  id: string;
  l1_network: string;
  l1_txid: string;
  amount_love: number;
  note: string;
  utxo_txid: string;
  recorded_at_unix: number;
}

export interface ParcelRecord {
  parcel_id: number;
  acres: number;
  price_love: number;
  deed_label: string;
  txid: string;
  signature_scheme: string;
  leaf_index?: number | null;
  slh_nonce?: number | null;
  acquired_at_unix: number;
  stark_proof_id?: string | null;
}

export interface UtxoRecord {
  txid: string;
  vout: number;
  amount_love: number;
  covenant_rule_hash: string | null;
  owner_tag: string;
}

export interface TelemetryRecord {
  id: string;
  parcel_id: number;
  kwh_delivered: number;
  soil_health_score: number;
  payout_love: number;
  device_id: string;
  signature_scheme: string;
  leaf_index?: number | null;
  slh_nonce?: number | null;
  txid?: string;
  timestamp_unix: number;
  stark_proof_id?: string | null;
  verified: boolean;
  blocked: boolean;
  attack_type?: string | null;
  error?: string | null;
}

export interface StreamingRecord {
  id: string;
  ticks: number;
  unit_price: number;
  gross_love: number;
  reinvested_love: number;
  operator_paid_love: number;
  operator_tag: string;
  payword_tip?: string;
  timestamp_unix: number;
  stark_proof_id?: string | null;
}

export interface StarkProofRecord {
  proof_id: string;
  transition_kind: string;
  proof_bytes_len: number;
  prove_ms: number;
  verify_ms: number;
  security_bits: number;
  verified: boolean;
  covenant_rule_hash: string;
  state_root: string;
  proof_digest: string;
  timestamp_unix: number;
  last_reverified_ms?: number;
}

export interface AnchorReceiptRecord {
  id: string;
  mode: string;
  broadcast_status: 'unbroadcast' | 'broadcast';
  network: string;
  l1_txid: string | null;
  anchor_hash: string;
  op_return_hex: string;
  stark_witness_hex: string;
  tx_nonce: number;
  parcels_acquired: number;
  total_kwh_verified: number;
  l1_height: number | null;
  l1_tip_hash: string | null;
  l1_genesis: string | null;
  auxpow_chainid: number | null;
  cli_lock_command: string;
  cli_memo_command: string;
  timestamp_unix: number;
  verified_on_l1: boolean;
  l1_lock_id: string | null;
  l1_confirmed_height: number | null;
  l1_confirmations: number | null;
  l1_lock_state: string | null;
  l1_verified_at_unix: number | null;
}

export interface TelemetrySeriesPoint {
  step: number;
  label: string;
  parcels: number;
  total_kwh: number;
  commons_balance_love: number;
  verified_love_per_kwh: number | null;
}

export interface KeysSummary {
  deed_oracle_merkle_root: string;
  hardware_merkle_root: string;
  deed_oracle_slh_dsa_pk_hex: string;
  hardware_slh_dsa_pk_hex: string;
  hardware_remaining_leaves: number;
  deed_oracle_remaining_leaves: number;
}

export interface CommonsState {
  protocol_version: string;
  verify_only: boolean;
  signing_keys_loaded: boolean;
  covenant_rule: CovenantRule;
  covenant_rule_hash: string;
  ledger_summary: LedgerSummary;
  funding_records: FundingRecord[];
  parcels: ParcelRecord[];
  utxos: UtxoRecord[];
  telemetry_history: TelemetryRecord[];
  streaming_history: StreamingRecord[];
  stark_proofs: StarkProofRecord[];
  anchor_receipts: AnchorReceiptRecord[];
  telemetry_series: TelemetrySeriesPoint[];
  keys: KeysSummary;
}

export interface SeedRecentBlock {
  height: number;
  hash: string;
  time: number;
  txs: number;
  bytes: number;
  merge_mined: boolean;
}

export interface SeedChainStatus {
  network: string;
  proofs?: string;
  bits?: string;
  version: string;
  height: number;
  tip: string;
  genesis: string;
  difficulty: number;
  peers: number;
  issued: number;
  notes: number;
  nullifiers: number;
  locks: number;
  conditions: number;
  auxpow_chainid: number;
  mempool?: number;
  chainwork?: string;
  recent: SeedRecentBlock[];
}

export interface MainnetInfo {
  network: string;
  genesis: string;
  height: number;
  note: string;
  auxpow_chainid?: number;
}

export interface SeedStatusResponse {
  live: boolean;
  endpoint: string;
  fetched_at_unix: number;
  status: SeedChainStatus | null;
  mainnet_info: MainnetInfo | null;
  mainnet_live: boolean;
  mainnet_endpoint: string | null;
  mainnet_status: SeedChainStatus | null;
  mainnet_error?: string | null;
  error?: string | null;
}

export interface VerifyAnchorResponse {
  ok: boolean;
  verified_on_l1: boolean;
  receipt?: AnchorReceiptRecord;
  state?: CommonsState;
  error?: string;
}

export interface RecordFundingPayload {
  l1_network: 'testnet' | 'mainnet' | 'regtest';
  l1_txid: string;
  amount_love: number;
  note: string;
}

export interface RecordFundingResponse {
  ok: boolean;
  error?: string;
  funding?: FundingRecord;
  state?: CommonsState;
}

export interface ConfirmAnchorPayload {
  anchor_hash: string;
  l1_txid: string;
}

export interface ConfirmAnchorResponse {
  ok: boolean;
  error?: string;
  receipt?: AnchorReceiptRecord;
  state?: CommonsState;
}

export interface AcquireParcelPayload {
  parcel_id: number;
  acres: number;
  price_love: number;
  deed_label: string;
  signature_scheme: SignatureScheme;
}

export interface AcquireParcelResponse {
  ok: boolean;
  error?: string;
  txid?: string;
  parcel?: ParcelRecord;
  stark_proof?: StarkProofRecord;
  state?: CommonsState;
}

export interface TelemetryPayload {
  parcel_id: number;
  kwh_delivered: number;
  soil_health_score: number;
  payout_love: number;
  device_id: string;
  signature_scheme: SignatureScheme;
}

export interface TelemetryResponse {
  ok: boolean;
  blocked?: boolean;
  error?: string;
  packet?: TelemetryRecord;
  stark_proof?: StarkProofRecord;
  state?: CommonsState;
}

export interface StreamSettlePayload {
  ticks: number;
  unit_price: number;
  operator_tag: string;
}

export interface StreamSettleResponse {
  ok: boolean;
  error?: string;
  gross_love?: number;
  reinvested_love?: number;
  operator_paid_love?: number;
  stark_proof?: StarkProofRecord;
  state?: CommonsState;
}

export interface AnchorResponse {
  ok: boolean;
  error?: string;
  receipt?: AnchorReceiptRecord;
  state?: CommonsState;
}

export interface VerifyStarkResponse {
  ok: boolean;
  valid: boolean;
  proof_id: string;
  verify_ms: number;
  proof_bytes_len: number;
  security_bits: number;
  error?: string;
}
