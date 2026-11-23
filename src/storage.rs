use crate::crypto::{
    hash32_from_hex, sha256, Hash32, MerkleLamportKeypair, SlhDsaKeypair, SlhDsaPubKey,
    DEFAULT_TREE_HEIGHT,
};
use crate::rpc::{AnchorReceipt, PROTOCOL_VERSION};
use crate::seed::{AUXPOW_CHAIN_ID, MAINNET_GENESIS_HASH, TESTNET_GENESIS_HASH};
use crate::stark::StarkProofArtifact;
use crate::swap::AtomicSwapContract;
use crate::{CommonsLedger, CovenantRule, Utxo};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Deterministic default seed tags (shared with `newsatoshi-hw-signer provision --default-engine-seed`).
pub fn default_deed_oracle_xmss_seed() -> Hash32 {
    sha256(b"NEWSATOSHI_COMMONS_DEED_ORACLE_XMSS_V2_1")
}

pub fn default_hardware_xmss_seed() -> Hash32 {
    sha256(b"NEWSATOSHI_COMMONS_HARDWARE_TEE_XMSS_V2_1")
}

pub fn default_deed_oracle_slh_seed() -> Hash32 {
    sha256(b"NEWSATOSHI_COMMONS_DEED_ORACLE_SLH_DSA_V2_1")
}

pub fn default_hardware_slh_seed() -> Hash32 {
    sha256(b"NEWSATOSHI_COMMONS_HARDWARE_TEE_SLH_DSA_V2_1")
}

/// Computes the real observed LOVE disbursed per verified kWh strictly from recorded telemetry.
/// Returns `None` when no real metered kWh has been verified yet (never fabricates a baseline).
pub fn compute_verified_love_per_kwh(
    total_energy_bounty_love: u64,
    total_kwh_verified: u64,
) -> Option<f64> {
    if total_kwh_verified == 0 {
        None
    } else {
        let ratio = (total_energy_bounty_love as f64) / (total_kwh_verified as f64);
        Some((ratio * 10_000.0).round() / 10_000.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CovenantRuleJson {
    pub max_bounty_payout: u64,
    pub reinvest_bps: u16,
    pub min_soil_health_score: u16,
    pub authorized_deed_oracle_pk: String,
    pub authorized_hardware_pk: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorized_deed_oracle_slh_pk: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorized_hardware_slh_pk: Option<String>,
}

impl From<&CovenantRule> for CovenantRuleJson {
    fn from(r: &CovenantRule) -> Self {
        Self {
            max_bounty_payout: r.max_bounty_payout,
            reinvest_bps: r.reinvest_bps,
            min_soil_health_score: r.min_soil_health_score,
            authorized_deed_oracle_pk: hex::encode(r.authorized_deed_oracle_pk),
            authorized_hardware_pk: hex::encode(r.authorized_hardware_pk),
            authorized_deed_oracle_slh_pk: r.authorized_deed_oracle_slh_pk.map(hex::encode),
            authorized_hardware_slh_pk: r.authorized_hardware_slh_pk.map(hex::encode),
        }
    }
}

impl TryFrom<&CovenantRuleJson> for CovenantRule {
    type Error = String;

    fn try_from(j: &CovenantRuleJson) -> Result<Self, Self::Error> {
        let authorized_deed_oracle_pk = hash32_from_hex(&j.authorized_deed_oracle_pk)?;
        let authorized_hardware_pk = hash32_from_hex(&j.authorized_hardware_pk)?;
        let authorized_deed_oracle_slh_pk = match &j.authorized_deed_oracle_slh_pk {
            Some(h) => Some(hash32_from_hex(h)?),
            None => None,
        };
        let authorized_hardware_slh_pk = match &j.authorized_hardware_slh_pk {
            Some(h) => Some(hash32_from_hex(h)?),
            None => None,
        };
        Ok(Self {
            max_bounty_payout: j.max_bounty_payout,
            reinvest_bps: j.reinvest_bps,
            min_soil_health_score: j.min_soil_health_score,
            authorized_deed_oracle_pk,
            authorized_hardware_pk,
            authorized_deed_oracle_slh_pk,
            authorized_hardware_slh_pk,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UtxoJson {
    pub txid: String,
    pub vout: u32,
    pub amount: u64,
    pub owner_tag: String,
    pub covenant_hash: Option<String>,
}

impl From<&Utxo> for UtxoJson {
    fn from(u: &Utxo) -> Self {
        Self {
            txid: hex::encode(u.txid),
            vout: u.vout,
            amount: u.amount,
            owner_tag: u.owner_tag.clone(),
            covenant_hash: u.covenant_hash.map(hex::encode),
        }
    }
}

impl TryFrom<&UtxoJson> for Utxo {
    type Error = String;

    fn try_from(j: &UtxoJson) -> Result<Self, Self::Error> {
        Ok(Self {
            txid: hash32_from_hex(&j.txid)?,
            vout: j.vout,
            amount: j.amount,
            owner_tag: j.owner_tag.clone(),
            covenant_hash: match &j.covenant_hash {
                Some(h) => Some(hash32_from_hex(h)?),
                None => None,
            },
        })
    }
}

/// Complete JSON-serializable snapshot of `CommonsLedger`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerSnapshot {
    pub utxos: Vec<UtxoJson>,
    pub spent_one_time_keys: Vec<String>,
    pub merkle_next_min_leaf: BTreeMap<String, u32>,
    pub spent_merkle_leaves: BTreeMap<String, Vec<u32>>,
    #[serde(default)]
    pub spent_slh_digests: Vec<String>,
    #[serde(default)]
    pub slh_next_min_nonce: BTreeMap<String, u64>,
    pub prev_state_root: String,
    pub state_root: String,
    pub tx_nonce: u64,
    pub acquired_parcels: Vec<u64>,
    pub total_kwh_verified: u64,
    pub total_merkle_leaves_consumed: u64,
    #[serde(default)]
    pub total_slh_dsa_signatures_verified: u64,
}

impl From<&CommonsLedger> for LedgerSnapshot {
    fn from(l: &CommonsLedger) -> Self {
        let mut utxos: Vec<UtxoJson> = l.utxos.values().map(UtxoJson::from).collect();
        utxos.sort_by(|a, b| a.txid.cmp(&b.txid).then(a.vout.cmp(&b.vout)));

        let mut spent_one_time_keys: Vec<String> =
            l.spent_one_time_keys.iter().map(hex::encode).collect();
        spent_one_time_keys.sort();

        let merkle_next_min_leaf: BTreeMap<String, u32> = l
            .merkle_next_min_leaf
            .iter()
            .map(|(k, v)| (hex::encode(k), *v))
            .collect();

        let spent_merkle_leaves: BTreeMap<String, Vec<u32>> = l
            .spent_merkle_leaves
            .iter()
            .map(|(k, set)| {
                let mut v: Vec<u32> = set.iter().copied().collect();
                v.sort_unstable();
                (hex::encode(k), v)
            })
            .collect();

        let mut spent_slh_digests: Vec<String> =
            l.spent_slh_digests.iter().map(hex::encode).collect();
        spent_slh_digests.sort();

        let slh_next_min_nonce: BTreeMap<String, u64> = l
            .slh_next_min_nonce
            .iter()
            .map(|(k, v)| (hex::encode(k), *v))
            .collect();

        Self {
            utxos,
            spent_one_time_keys,
            merkle_next_min_leaf,
            spent_merkle_leaves,
            spent_slh_digests,
            slh_next_min_nonce,
            prev_state_root: hex::encode(l.prev_state_root),
            state_root: hex::encode(l.state_root),
            tx_nonce: l.tx_nonce,
            acquired_parcels: l.acquired_parcels.clone(),
            total_kwh_verified: l.total_kwh_verified,
            total_merkle_leaves_consumed: l.total_merkle_leaves_consumed,
            total_slh_dsa_signatures_verified: l.total_slh_dsa_signatures_verified,
        }
    }
}

impl TryFrom<&LedgerSnapshot> for CommonsLedger {
    type Error = String;

    fn try_from(s: &LedgerSnapshot) -> Result<Self, Self::Error> {
        let mut utxos = HashMap::with_capacity(s.utxos.len());
        for uj in &s.utxos {
            let u = Utxo::try_from(uj)?;
            utxos.insert((u.txid, u.vout), u);
        }

        let mut spent_one_time_keys = HashSet::with_capacity(s.spent_one_time_keys.len());
        for h in &s.spent_one_time_keys {
            spent_one_time_keys.insert(hash32_from_hex(h)?);
        }

        let mut merkle_next_min_leaf = HashMap::with_capacity(s.merkle_next_min_leaf.len());
        for (k, v) in &s.merkle_next_min_leaf {
            merkle_next_min_leaf.insert(hash32_from_hex(k)?, *v);
        }

        let mut spent_merkle_leaves = HashMap::with_capacity(s.spent_merkle_leaves.len());
        for (k, list) in &s.spent_merkle_leaves {
            spent_merkle_leaves.insert(hash32_from_hex(k)?, list.iter().copied().collect());
        }

        let mut spent_slh_digests = HashSet::with_capacity(s.spent_slh_digests.len());
        for h in &s.spent_slh_digests {
            spent_slh_digests.insert(hash32_from_hex(h)?);
        }

        let mut slh_next_min_nonce = HashMap::with_capacity(s.slh_next_min_nonce.len());
        for (k, v) in &s.slh_next_min_nonce {
            slh_next_min_nonce.insert(hash32_from_hex(k)?, *v);
        }

        Ok(Self {
            utxos,
            spent_one_time_keys,
            merkle_next_min_leaf,
            spent_merkle_leaves,
            spent_slh_digests,
            slh_next_min_nonce,
            prev_state_root: hash32_from_hex(&s.prev_state_root)?,
            state_root: hash32_from_hex(&s.state_root)?,
            tx_nonce: s.tx_nonce,
            acquired_parcels: s.acquired_parcels.clone(),
            total_kwh_verified: s.total_kwh_verified,
            total_merkle_leaves_consumed: s.total_merkle_leaves_consumed,
            total_slh_dsa_signatures_verified: s.total_slh_dsa_signatures_verified,
        })
    }
}

/// Persisted private signing key state (never leaked in `GET /api/state`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedKeysState {
    pub tree_height: u8,
    pub deed_oracle_xmss_seed_hex: String,
    pub deed_oracle_xmss_next_leaf: u32,
    pub hardware_xmss_seed_hex: String,
    pub hardware_xmss_next_leaf: u32,
    pub deed_oracle_slh_seed_hex: String,
    pub deed_oracle_slh_next_nonce: u64,
    pub hardware_slh_seed_hex: String,
    pub hardware_slh_next_nonce: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParcelRecord {
    pub parcel_id: u64,
    pub acres: f64,
    pub price_love: u64,
    pub deed_label: String,
    pub deed_document_hash: String,
    pub txid: String,
    pub signature_scheme: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaf_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slh_nonce: Option<u64>,
    pub stark_proof_id: String,
    pub acquired_at_unix: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryRecord {
    pub id: String,
    pub device_id: String,
    pub parcel_id: u64,
    pub kwh_delivered: u64,
    pub soil_health_score: u16,
    pub payout_love: u64,
    pub signature_scheme: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaf_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slh_nonce: Option<u64>,
    pub blocked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attack_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub txid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stark_proof_id: Option<String>,
    pub timestamp_unix: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamingRecord {
    pub id: String,
    pub ticks: u64,
    pub unit_price: u64,
    pub gross_love: u64,
    pub reinvested_love: u64,
    pub operator_paid_love: u64,
    pub operator_tag: String,
    pub channel_tip_hex: String,
    pub stark_proof_id: String,
    pub timestamp_unix: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundingRecord {
    pub id: String,
    pub l1_network: String,
    pub l1_txid: String,
    pub amount_love: u64,
    pub note: String,
    pub utxo_txid: String,
    pub recorded_at_unix: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TelemetrySeriesPoint {
    pub step: u64,
    pub label: String,
    pub parcels: usize,
    pub total_kwh: u64,
    pub commons_balance_love: u64,
    pub verified_love_per_kwh: Option<f64>,
}

/// External public-key configuration for `--verify-only` / Mainnet deployments.
/// Holds ONLY public roots and public keys (zero private signing keys on the server).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalPublicKeysConfig {
    pub deed_oracle_merkle_root: String,
    pub hardware_merkle_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deed_oracle_slh_dsa_pk_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hardware_slh_dsa_pk_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bounty_payout: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reinvest_bps: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_soil_health_score: Option<u16>,
}

/// Optional in-process signing keypairs (loaded ONLY in local/testnet development mode;
/// strictly `None` in `--verify-only` / Mainnet deployments).
pub struct InProcessSigningKeys {
    pub deed_oracle_xmss: MerkleLamportKeypair,
    pub hardware_xmss: MerkleLamportKeypair,
    pub deed_oracle_slh: SlhDsaKeypair,
    pub deed_oracle_slh_seed: Hash32,
    pub deed_oracle_slh_next_nonce: u64,
    pub hardware_slh: SlhDsaKeypair,
    pub hardware_slh_seed: Hash32,
    pub hardware_slh_next_nonce: u64,
}

/// On-disk atomic JSON representation of the entire Commons Engine state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PersistedEngineState {
    pub version: String,
    pub protocol_version: String,
    #[serde(default)]
    pub verify_only: bool,
    pub covenant_rule: CovenantRuleJson,
    pub ledger: LedgerSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys_state: Option<PersistedKeysState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deed_oracle_slh_pk_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hardware_slh_pk_hex: Option<String>,
    #[serde(default)]
    pub funding_records: Vec<FundingRecord>,
    pub parcels: Vec<ParcelRecord>,
    pub telemetry_history: Vec<TelemetryRecord>,
    pub streaming_history: Vec<StreamingRecord>,
    pub stark_proofs: Vec<StarkProofArtifact>,
    pub anchor_receipts: Vec<AnchorReceipt>,
    #[serde(default)]
    pub swap_offers: Vec<AtomicSwapContract>,
    #[serde(default)]
    pub telemetry_series: Vec<TelemetrySeriesPoint>,
    #[serde(default)]
    pub total_energy_bounty_love: u64,
    pub total_streaming_gross_love: u64,
    pub total_streaming_reinvested_love: u64,
}

/// Public summary section of `GET /api/state`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LedgerSummaryView {
    pub balance_love: u64,
    pub tx_nonce: u64,
    pub prev_state_root: String,
    pub state_root: String,
    pub funding_deposits_count: usize,
    pub parcels_acquired_count: usize,
    pub total_kwh_verified: u64,
    pub total_energy_bounty_love: u64,
    pub verified_love_per_kwh: Option<f64>,
    pub total_streaming_gross_love: u64,
    pub total_streaming_reinvested_love: u64,
    pub merkle_leaves_consumed: u64,
    pub slh_dsa_signatures_verified: u64,
    pub stark_proofs_verified: usize,
}

/// Public key material view exposed by `GET /api/state` (zero secrets).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKeysView {
    pub deed_oracle_merkle_root: String,
    pub hardware_merkle_root: String,
    pub deed_oracle_slh_dsa_pk_hex: String,
    pub hardware_slh_dsa_pk_hex: String,
    pub hardware_remaining_leaves: u32,
    pub deed_oracle_remaining_leaves: u32,
}

/// Exact JSON payload returned by `GET /api/state`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApiStateView {
    pub protocol_version: String,
    #[serde(default)]
    pub verify_only: bool,
    #[serde(default)]
    pub signing_keys_loaded: bool,
    pub covenant_rule: CovenantRuleJson,
    pub covenant_rule_hash: String,
    pub ledger_summary: LedgerSummaryView,
    pub funding_records: Vec<FundingRecord>,
    pub parcels: Vec<ParcelRecord>,
    pub utxos: Vec<UtxoJson>,
    pub telemetry_history: Vec<TelemetryRecord>,
    pub streaming_history: Vec<StreamingRecord>,
    pub stark_proofs: Vec<StarkProofArtifact>,
    pub anchor_receipts: Vec<AnchorReceipt>,
    #[serde(default)]
    pub swap_offers: Vec<AtomicSwapContract>,
    pub telemetry_series: Vec<TelemetrySeriesPoint>,
    pub keys: PublicKeysView,
}

/// Deterministic Legal Airlock schedule exported by `newsatoshi-commons export-legal-schedule`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalScheduleExport {
    pub protocol_version: String,
    pub covenant_rule_hash: String,
    pub covenant_rule: LegalScheduleRule,
    pub keys: LegalScheduleKeys,
    pub l1_anchors: LegalScheduleL1Anchors,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalScheduleRule {
    pub max_bounty_payout: u64,
    pub reinvest_bps: u16,
    pub min_soil_health_score: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalScheduleKeys {
    pub deed_oracle_merkle_root: String,
    pub hardware_merkle_root: String,
    pub deed_oracle_slh_dsa_pk_hex: String,
    pub hardware_slh_dsa_pk_hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalScheduleL1Anchors {
    pub mainnet_genesis: String,
    pub testnet_genesis: String,
    pub auxpow_chainid: u32,
}

/// Live in-memory Commons Engine combining `CommonsLedger`, optional in-process XMSS + SLH-DSA signers
/// (strictly `None` in `--verify-only` / Mainnet mode), STARK proofs, audit history, and atomic persistence.
pub struct CommonsEngine {
    pub verify_only: bool,
    pub rule: CovenantRule,
    pub ledger: CommonsLedger,
    pub signing_keys: Option<InProcessSigningKeys>,
    pub deed_oracle_slh_pk_hex: Option<String>,
    pub hardware_slh_pk_hex: Option<String>,
    pub funding_records: Vec<FundingRecord>,
    pub parcels: Vec<ParcelRecord>,
    pub telemetry_history: Vec<TelemetryRecord>,
    pub streaming_history: Vec<StreamingRecord>,
    pub stark_proofs: Vec<StarkProofArtifact>,
    pub anchor_receipts: Vec<AnchorReceipt>,
    pub swap_offers: Vec<AtomicSwapContract>,
    pub telemetry_series: Vec<TelemetrySeriesPoint>,
    pub total_energy_bounty_love: u64,
    pub total_streaming_gross_love: u64,
    pub total_streaming_reinvested_love: u64,
}

impl CommonsEngine {
    /// Initializes an honest empty Commons Engine (`0 LOVE`, `0` UTXOs, `0` parcels, `0` kWh, `0` proofs)
    /// with local testnet/development signing keys.
    /// Balance remains `0` until real L1 funding deposits or PayWord settlements are recorded.
    pub fn new_empty() -> Self {
        let deed_xmss_seed = default_deed_oracle_xmss_seed();
        let hw_xmss_seed = default_hardware_xmss_seed();
        let deed_slh_seed = default_deed_oracle_slh_seed();
        let hw_slh_seed = default_hardware_slh_seed();

        let deed_oracle_xmss = MerkleLamportKeypair::from_seed(deed_xmss_seed, DEFAULT_TREE_HEIGHT)
            .expect("default height valid");
        let hardware_xmss = MerkleLamportKeypair::from_seed(hw_xmss_seed, DEFAULT_TREE_HEIGHT)
            .expect("default height valid");
        let deed_oracle_slh = SlhDsaKeypair::from_seed(deed_slh_seed);
        let hardware_slh = SlhDsaKeypair::from_seed(hw_slh_seed);

        let deed_slh_pk_hex = deed_oracle_slh.pk.to_hex();
        let hw_slh_pk_hex = hardware_slh.pk.to_hex();

        let rule = CovenantRule {
            max_bounty_payout: 25_000,
            reinvest_bps: 1_500, // 15% autonomous land reinvestment spread
            min_soil_health_score: 80,
            authorized_deed_oracle_pk: deed_oracle_xmss.root(),
            authorized_hardware_pk: hardware_xmss.root(),
            authorized_deed_oracle_slh_pk: Some(deed_oracle_slh.pk.commitment()),
            authorized_hardware_slh_pk: Some(hardware_slh.pk.commitment()),
        };

        let ledger = CommonsLedger::new();

        Self {
            verify_only: false,
            rule,
            ledger,
            signing_keys: Some(InProcessSigningKeys {
                deed_oracle_xmss,
                hardware_xmss,
                deed_oracle_slh,
                deed_oracle_slh_seed: deed_slh_seed,
                deed_oracle_slh_next_nonce: 0,
                hardware_slh,
                hardware_slh_seed: hw_slh_seed,
                hardware_slh_next_nonce: 0,
            }),
            deed_oracle_slh_pk_hex: Some(deed_slh_pk_hex),
            hardware_slh_pk_hex: Some(hw_slh_pk_hex),
            funding_records: Vec::new(),
            parcels: Vec::new(),
            telemetry_history: Vec::new(),
            streaming_history: Vec::new(),
            stark_proofs: Vec::new(),
            anchor_receipts: Vec::new(),
            swap_offers: Vec::new(),
            telemetry_series: Vec::new(),
            total_energy_bounty_love: 0,
            total_streaming_gross_love: 0,
            total_streaming_reinvested_love: 0,
        }
    }

    /// Initializes a strict Mainnet-ready `--verify-only` engine from external public roots/keys only.
    /// Zero private keys or seeds are ever generated, held in memory, or written to disk.
    pub fn new_verify_only(ext: &ExternalPublicKeysConfig) -> Result<Self, String> {
        let authorized_deed_oracle_pk = hash32_from_hex(&ext.deed_oracle_merkle_root)
            .map_err(|e| format!("invalid deed_oracle_merkle_root: {e}"))?;
        let authorized_hardware_pk = hash32_from_hex(&ext.hardware_merkle_root)
            .map_err(|e| format!("invalid hardware_merkle_root: {e}"))?;

        let (deed_slh_commit, deed_slh_pk_hex) = match &ext.deed_oracle_slh_dsa_pk_hex {
            Some(h) if !h.trim().is_empty() => {
                let pk = SlhDsaPubKey::from_hex(h.trim())
                    .map_err(|e| format!("invalid deed_oracle_slh_dsa_pk_hex: {e}"))?;
                (Some(pk.commitment()), Some(pk.to_hex()))
            }
            _ => (None, None),
        };
        let (hw_slh_commit, hw_slh_pk_hex) = match &ext.hardware_slh_dsa_pk_hex {
            Some(h) if !h.trim().is_empty() => {
                let pk = SlhDsaPubKey::from_hex(h.trim())
                    .map_err(|e| format!("invalid hardware_slh_dsa_pk_hex: {e}"))?;
                (Some(pk.commitment()), Some(pk.to_hex()))
            }
            _ => (None, None),
        };

        let reinvest_bps = ext.reinvest_bps.unwrap_or(1_500);
        if reinvest_bps > 10_000 {
            return Err(format!("reinvest_bps ({reinvest_bps}) must be <= 10000"));
        }
        let min_soil_health_score = ext.min_soil_health_score.unwrap_or(80);
        if min_soil_health_score > 100 {
            return Err(format!(
                "min_soil_health_score ({min_soil_health_score}) must be <= 100"
            ));
        }

        let rule = CovenantRule {
            max_bounty_payout: ext.max_bounty_payout.unwrap_or(25_000),
            reinvest_bps,
            min_soil_health_score,
            authorized_deed_oracle_pk,
            authorized_hardware_pk,
            authorized_deed_oracle_slh_pk: deed_slh_commit,
            authorized_hardware_slh_pk: hw_slh_commit,
        };

        Ok(Self {
            verify_only: true,
            rule,
            ledger: CommonsLedger::new(),
            signing_keys: None,
            deed_oracle_slh_pk_hex: deed_slh_pk_hex,
            hardware_slh_pk_hex: hw_slh_pk_hex,
            funding_records: Vec::new(),
            parcels: Vec::new(),
            telemetry_history: Vec::new(),
            streaming_history: Vec::new(),
            stark_proofs: Vec::new(),
            anchor_receipts: Vec::new(),
            swap_offers: Vec::new(),
            telemetry_series: Vec::new(),
            total_energy_bounty_love: 0,
            total_streaming_gross_love: 0,
            total_streaming_reinvested_love: 0,
        })
    }

    /// Strips any in-process private signing keys from memory while preserving all public roots,
    /// covenant rules, UTXOs, proofs, and anchor receipts.
    pub fn enable_verify_only_strip_keys(&mut self) {
        if let Some(ref keys) = self.signing_keys {
            if self.deed_oracle_slh_pk_hex.is_none() {
                self.deed_oracle_slh_pk_hex = Some(keys.deed_oracle_slh.pk.to_hex());
            }
            if self.hardware_slh_pk_hex.is_none() {
                self.hardware_slh_pk_hex = Some(keys.hardware_slh.pk.to_hex());
            }
        }
        self.signing_keys = None;
        self.verify_only = true;
    }

    pub fn has_signing_keys(&self) -> bool {
        self.signing_keys.is_some()
    }

    /// Records a real L1 funding deposit referencing an actual 32-byte L1 txid / lock hash.
    pub fn record_l1_funding(
        &mut self,
        l1_network: String,
        l1_txid: String,
        amount_love: u64,
        note: String,
        now_unix: u64,
    ) -> Result<FundingRecord, String> {
        if amount_love == 0 {
            return Err("amount_love must be greater than 0".to_string());
        }
        let l1_txid_bytes = hash32_from_hex(&l1_txid)
            .map_err(|e| format!("l1_txid must be a valid 64-char hex transaction/lock hash: {e}"))?;
        let normalized_txid = hex::encode(l1_txid_bytes);
        if self
            .funding_records
            .iter()
            .any(|f| f.l1_txid.eq_ignore_ascii_case(&normalized_txid))
        {
            return Err(format!(
                "L1 funding txid '{normalized_txid}' has already been recorded"
            ));
        }

        let rule_hash = self.rule.rule_hash();
        let utxo_txid = crate::crypto::sha256_concat(&[
            b"NEWSATOSHI_L1_FUNDING_DEPOSIT_V2_1",
            l1_network.as_bytes(),
            &l1_txid_bytes,
            &amount_love.to_be_bytes(),
            &rule_hash,
        ]);
        let utxo = Utxo {
            txid: utxo_txid,
            vout: 0,
            amount: amount_love,
            owner_tag: format!("COMMONS_TREASURY_FUNDING_{}", self.funding_records.len() + 1),
            covenant_hash: Some(rule_hash),
        };
        self.ledger.utxos.insert((utxo_txid, 0), utxo);
        self.ledger.prev_state_root = self.ledger.state_root;
        self.ledger.tx_nonce += 1;
        self.ledger.state_root = crate::crypto::sha256_concat(&[
            &self.ledger.state_root,
            &utxo_txid,
            &self.ledger.tx_nonce.to_be_bytes(),
        ]);

        // Consolidate if multiple covenant UTXOs exist
        let refs = self.ledger.covenant_utxo_refs(&self.rule);
        if refs.len() >= 2 {
            let rule_clone = self.rule.clone();
            let _ = self.ledger.consolidate_covenant_utxos(&refs, &rule_clone);
        }

        let rec = FundingRecord {
            id: format!("fund-{}", self.funding_records.len() + 1),
            l1_network,
            l1_txid: normalized_txid,
            amount_love,
            note,
            utxo_txid: hex::encode(utxo_txid),
            recorded_at_unix: now_unix,
        };
        self.funding_records.push(rec.clone());
        self.record_telemetry_series_point(format!("L1 Funding +{amount_love} LOVE"));
        Ok(rec)
    }

    /// Reconstructs a live `CommonsEngine` from a persisted on-disk snapshot.
    pub fn from_persisted(p: &PersistedEngineState) -> Result<Self, String> {
        let rule = CovenantRule::try_from(&p.covenant_rule)?;
        let ledger = CommonsLedger::try_from(&p.ledger)?;

        let (signing_keys, mut deed_slh_pk_hex, mut hw_slh_pk_hex) = match (&p.keys_state, p.verify_only) {
            (Some(ks), false) => {
                let deed_xmss_seed = hash32_from_hex(&ks.deed_oracle_xmss_seed_hex)?;
                let hw_xmss_seed = hash32_from_hex(&ks.hardware_xmss_seed_hex)?;
                let deed_slh_seed = hash32_from_hex(&ks.deed_oracle_slh_seed_hex)?;
                let hw_slh_seed = hash32_from_hex(&ks.hardware_slh_seed_hex)?;

                let deed_oracle_xmss = MerkleLamportKeypair::from_seed_at_index(
                    deed_xmss_seed,
                    ks.tree_height,
                    ks.deed_oracle_xmss_next_leaf,
                )
                .map_err(|e| format!("failed to restore deed oracle XMSS keypair: {e}"))?;

                let hardware_xmss = MerkleLamportKeypair::from_seed_at_index(
                    hw_xmss_seed,
                    ks.tree_height,
                    ks.hardware_xmss_next_leaf,
                )
                .map_err(|e| format!("failed to restore hardware XMSS keypair: {e}"))?;

                let deed_oracle_slh = SlhDsaKeypair::from_seed(deed_slh_seed);
                let hardware_slh = SlhDsaKeypair::from_seed(hw_slh_seed);
                let d_hex = deed_oracle_slh.pk.to_hex();
                let h_hex = hardware_slh.pk.to_hex();

                (
                    Some(InProcessSigningKeys {
                        deed_oracle_xmss,
                        hardware_xmss,
                        deed_oracle_slh,
                        deed_oracle_slh_seed: deed_slh_seed,
                        deed_oracle_slh_next_nonce: ks.deed_oracle_slh_next_nonce,
                        hardware_slh,
                        hardware_slh_seed: hw_slh_seed,
                        hardware_slh_next_nonce: ks.hardware_slh_next_nonce,
                    }),
                    Some(d_hex),
                    Some(h_hex),
                )
            }
            _ => (None, p.deed_oracle_slh_pk_hex.clone(), p.hardware_slh_pk_hex.clone()),
        };

        if deed_slh_pk_hex.is_none() {
            deed_slh_pk_hex = p.deed_oracle_slh_pk_hex.clone();
        }
        if hw_slh_pk_hex.is_none() {
            hw_slh_pk_hex = p.hardware_slh_pk_hex.clone();
        }

        let verify_only = p.verify_only || signing_keys.is_none();

        Ok(Self {
            verify_only,
            rule,
            ledger,
            signing_keys,
            deed_oracle_slh_pk_hex: deed_slh_pk_hex,
            hardware_slh_pk_hex: hw_slh_pk_hex,
            funding_records: p.funding_records.clone(),
            parcels: p.parcels.clone(),
            telemetry_history: p.telemetry_history.clone(),
            streaming_history: p.streaming_history.clone(),
            stark_proofs: p.stark_proofs.clone(),
            anchor_receipts: p.anchor_receipts.clone(),
            swap_offers: p.swap_offers.clone(),
            telemetry_series: p.telemetry_series.clone(),
            total_energy_bounty_love: p.total_energy_bounty_love,
            total_streaming_gross_love: p.total_streaming_gross_love,
            total_streaming_reinvested_love: p.total_streaming_reinvested_love,
        })
    }

    /// Exports the full state for atomic persistence on disk.
    /// In `--verify-only` mode, `keys_state` is strictly `None` (zero private seeds persisted).
    pub fn to_persisted(&self) -> PersistedEngineState {
        let keys_state = if self.verify_only {
            None
        } else {
            self.signing_keys.as_ref().map(|k| PersistedKeysState {
                tree_height: k.hardware_xmss.height(),
                deed_oracle_xmss_seed_hex: hex::encode(k.deed_oracle_xmss.master_seed()),
                deed_oracle_xmss_next_leaf: k.deed_oracle_xmss.next_index(),
                hardware_xmss_seed_hex: hex::encode(k.hardware_xmss.master_seed()),
                hardware_xmss_next_leaf: k.hardware_xmss.next_index(),
                deed_oracle_slh_seed_hex: hex::encode(k.deed_oracle_slh_seed),
                deed_oracle_slh_next_nonce: k.deed_oracle_slh_next_nonce,
                hardware_slh_seed_hex: hex::encode(k.hardware_slh_seed),
                hardware_slh_next_nonce: k.hardware_slh_next_nonce,
            })
        };

        PersistedEngineState {
            version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION.to_string(),
            verify_only: self.verify_only,
            covenant_rule: CovenantRuleJson::from(&self.rule),
            ledger: LedgerSnapshot::from(&self.ledger),
            keys_state,
            deed_oracle_slh_pk_hex: self.deed_oracle_slh_pk_hex.clone(),
            hardware_slh_pk_hex: self.hardware_slh_pk_hex.clone(),
            funding_records: self.funding_records.clone(),
            parcels: self.parcels.clone(),
            telemetry_history: self.telemetry_history.clone(),
            streaming_history: self.streaming_history.clone(),
            stark_proofs: self.stark_proofs.clone(),
            anchor_receipts: self.anchor_receipts.clone(),
            swap_offers: self.swap_offers.clone(),
            telemetry_series: self.telemetry_series.clone(),
            total_energy_bounty_love: self.total_energy_bounty_love,
            total_streaming_gross_love: self.total_streaming_gross_love,
            total_streaming_reinvested_love: self.total_streaming_reinvested_love,
        }
    }

    /// Appends a real observed ledger transition point to `telemetry_series`.
    pub fn record_telemetry_series_point(&mut self, label: impl Into<String>) {
        let step = (self.telemetry_series.len() as u64) + 1;
        let balance = self.ledger.total_covenant_balance(&self.rule);
        let verified_love_per_kwh = compute_verified_love_per_kwh(
            self.total_energy_bounty_love,
            self.ledger.total_kwh_verified,
        );
        self.telemetry_series.push(TelemetrySeriesPoint {
            step,
            label: label.into(),
            parcels: self.parcels.len(),
            total_kwh: self.ledger.total_kwh_verified,
            commons_balance_love: balance,
            verified_love_per_kwh,
        });
    }

    /// Ensures all covenant UTXOs are consolidated into a single UTXO before a spend.
    pub fn ensure_single_covenant_utxo(&mut self) -> Result<(Hash32, u32), String> {
        let refs = self.ledger.covenant_utxo_refs(&self.rule);
        if refs.is_empty() {
            return Err(
                "Commons Treasury covenant balance is 0 LOVE — record an L1 funding deposit first"
                    .to_string(),
            );
        }
        if refs.len() == 1 {
            return Ok(refs[0]);
        }
        self.ledger
            .consolidate_covenant_utxos(&refs, &self.rule)
            .map_err(|e| e.to_string())
    }

    /// Builds the public `GET /api/state` response view (no private seeds exposed).
    pub fn to_api_state(&self) -> ApiStateView {
        let balance_love = self.ledger.total_covenant_balance(&self.rule);
        let verified_love_per_kwh = compute_verified_love_per_kwh(
            self.total_energy_bounty_love,
            self.ledger.total_kwh_verified,
        );
        let mut utxos: Vec<UtxoJson> = self.ledger.utxos.values().map(UtxoJson::from).collect();
        utxos.sort_by(|a, b| a.txid.cmp(&b.txid).then(a.vout.cmp(&b.vout)));

        let deed_oracle_slh_dsa_pk_hex = self
            .deed_oracle_slh_pk_hex
            .clone()
            .or_else(|| self.signing_keys.as_ref().map(|k| k.deed_oracle_slh.pk.to_hex()))
            .or_else(|| self.rule.authorized_deed_oracle_slh_pk.map(hex::encode))
            .unwrap_or_default();
        let hardware_slh_dsa_pk_hex = self
            .hardware_slh_pk_hex
            .clone()
            .or_else(|| self.signing_keys.as_ref().map(|k| k.hardware_slh.pk.to_hex()))
            .or_else(|| self.rule.authorized_hardware_slh_pk.map(hex::encode))
            .unwrap_or_default();

        ApiStateView {
            protocol_version: PROTOCOL_VERSION.to_string(),
            verify_only: self.verify_only,
            signing_keys_loaded: self.signing_keys.is_some(),
            covenant_rule: CovenantRuleJson::from(&self.rule),
            covenant_rule_hash: hex::encode(self.rule.rule_hash()),
            ledger_summary: LedgerSummaryView {
                balance_love,
                tx_nonce: self.ledger.tx_nonce,
                prev_state_root: hex::encode(self.ledger.prev_state_root),
                state_root: hex::encode(self.ledger.state_root),
                funding_deposits_count: self.funding_records.len(),
                parcels_acquired_count: self.parcels.len(),
                total_kwh_verified: self.ledger.total_kwh_verified,
                total_energy_bounty_love: self.total_energy_bounty_love,
                verified_love_per_kwh,
                total_streaming_gross_love: self.total_streaming_gross_love,
                total_streaming_reinvested_love: self.total_streaming_reinvested_love,
                merkle_leaves_consumed: self.ledger.total_merkle_leaves_consumed,
                slh_dsa_signatures_verified: self.ledger.total_slh_dsa_signatures_verified,
                stark_proofs_verified: self.stark_proofs.iter().filter(|p| p.verified).count(),
            },
            funding_records: self.funding_records.clone(),
            parcels: self.parcels.clone(),
            utxos,
            telemetry_history: self.telemetry_history.clone(),
            streaming_history: self.streaming_history.clone(),
            stark_proofs: self.stark_proofs.clone(),
            anchor_receipts: self.anchor_receipts.clone(),
            swap_offers: self.swap_offers.clone(),
            telemetry_series: self.telemetry_series.clone(),
            keys: PublicKeysView {
                deed_oracle_merkle_root: hex::encode(self.rule.authorized_deed_oracle_pk),
                hardware_merkle_root: hex::encode(self.rule.authorized_hardware_pk),
                deed_oracle_slh_dsa_pk_hex,
                hardware_slh_dsa_pk_hex,
                hardware_remaining_leaves: self
                    .signing_keys
                    .as_ref()
                    .map(|k| k.hardware_xmss.remaining_signatures())
                    .unwrap_or(0),
                deed_oracle_remaining_leaves: self
                    .signing_keys
                    .as_ref()
                    .map(|k| k.deed_oracle_xmss.remaining_signatures())
                    .unwrap_or(0),
            },
        }
    }

    /// Exports the deterministic JSON schedule consumed by `legal/render_airlock.py`.
    pub fn export_legal_schedule(&self) -> LegalScheduleExport {
        let deed_oracle_slh_dsa_pk_hex = self
            .deed_oracle_slh_pk_hex
            .clone()
            .or_else(|| self.signing_keys.as_ref().map(|k| k.deed_oracle_slh.pk.to_hex()))
            .or_else(|| self.rule.authorized_deed_oracle_slh_pk.map(hex::encode))
            .unwrap_or_default();
        let hardware_slh_dsa_pk_hex = self
            .hardware_slh_pk_hex
            .clone()
            .or_else(|| self.signing_keys.as_ref().map(|k| k.hardware_slh.pk.to_hex()))
            .or_else(|| self.rule.authorized_hardware_slh_pk.map(hex::encode))
            .unwrap_or_default();

        LegalScheduleExport {
            protocol_version: PROTOCOL_VERSION.to_string(),
            covenant_rule_hash: hex::encode(self.rule.rule_hash()),
            covenant_rule: LegalScheduleRule {
                max_bounty_payout: self.rule.max_bounty_payout,
                reinvest_bps: self.rule.reinvest_bps,
                min_soil_health_score: self.rule.min_soil_health_score,
            },
            keys: LegalScheduleKeys {
                deed_oracle_merkle_root: hex::encode(self.rule.authorized_deed_oracle_pk),
                hardware_merkle_root: hex::encode(self.rule.authorized_hardware_pk),
                deed_oracle_slh_dsa_pk_hex,
                hardware_slh_dsa_pk_hex,
            },
            l1_anchors: LegalScheduleL1Anchors {
                mainnet_genesis: MAINNET_GENESIS_HASH.to_string(),
                testnet_genesis: TESTNET_GENESIS_HASH.to_string(),
                auxpow_chainid: AUXPOW_CHAIN_ID,
            },
        }
    }
}

/// Atomically writes JSON state to `path` via a temporary file + `fs::rename`.
pub fn save_state_atomic(path: &Path, state: &PersistedEngineState) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create directory '{}': {e}", parent.display()))?;
        }
    }

    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|e| format!("failed to serialize Commons Engine state: {e}"))?;

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_name = format!(
        ".{}.tmp.{}.{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("commons-ledger"),
        std::process::id(),
        nonce
    );
    let tmp_path = path.with_file_name(tmp_name);

    {
        let mut file = File::create(&tmp_path)
            .map_err(|e| format!("failed to create temp file '{}': {e}", tmp_path.display()))?;
        file.write_all(&bytes)
            .map_err(|e| format!("failed to write temp file '{}': {e}", tmp_path.display()))?;
        file.sync_all()
            .map_err(|e| format!("failed to sync temp file '{}': {e}", tmp_path.display()))?;
    }

    fs::rename(&tmp_path, path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!(
            "failed to atomically rename '{}' -> '{}': {e}",
            tmp_path.display(),
            path.display()
        )
    })
}

/// Loads a `PersistedEngineState` JSON file from disk.
pub fn load_state(path: &Path) -> Result<PersistedEngineState, String> {
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("failed to read state file '{}': {e}", path.display()))?;
    serde_json::from_str::<PersistedEngineState>(&raw)
        .map_err(|e| format!("failed to parse state file '{}': {e}", path.display()))
}

/// Loads an existing `CommonsEngine` from `path`, or creates and persists an honest empty state (`0 LOVE`).
pub fn load_or_init_state(path: &Path) -> Result<CommonsEngine, String> {
    load_or_init_state_with_mode(path, false, None)
}

/// Loads or initializes a `CommonsEngine` with optional strict `--verify-only` key-isolation mode.
/// When `verify_only` is true, any private signing seeds are stripped immediately from memory and
/// omitted from disk persistence so the server strictly verifies external hardware/oracle signatures.
pub fn load_or_init_state_with_mode(
    path: &Path,
    verify_only: bool,
    external_keys: Option<&ExternalPublicKeysConfig>,
) -> Result<CommonsEngine, String> {
    if path.exists() {
        let persisted = load_state(path)?;
        let had_keys = persisted.keys_state.is_some();
        let mut engine = CommonsEngine::from_persisted(&persisted)?;
        if verify_only {
            engine.enable_verify_only_strip_keys();
            if had_keys || !persisted.verify_only {
                save_state_atomic(path, &engine.to_persisted())?;
            }
        }
        Ok(engine)
    } else {
        let mut engine = match external_keys {
            Some(ext) => CommonsEngine::new_verify_only(ext)?,
            None => CommonsEngine::new_empty(),
        };
        if verify_only {
            engine.enable_verify_only_strip_keys();
        }
        save_state_atomic(path, &engine.to_persisted())?;
        Ok(engine)
    }
}
