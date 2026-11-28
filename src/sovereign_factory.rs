//! Permissionless Branded Stablecoin & Sovereign STARK L2 Factory (`src/sovereign_factory.rs`).
//!
//! Enables any regulated bank, fintech, DAO, or independent builder to launch:
//! 1. **Custom Branded Stablecoins (`asset_id >= 100`)** on RFC-0028 rails at **0.00% protocol fee**:
//!    - `RegulatedBankReserve`: 1:1 bank-custodied FBO cash/T-bills + optional auditor viewing key commitment.
//!    - `OnChainEscrow`: 1:1 USDC/USDS/tokenized-T-bill escrow anchored to ownerless `NewsatoshiBackingVault`.
//!    - `SovereignCdp`: over-collateralized BTC / `LOVE` vault (>=150% MCR, zero freeze key).
//! 2. **Sovereign STARK-Proved L2 Rollups** anchored to canonical Mainnet v2 (`nsM2`):
//!    - Sentinel-padded `SHA-256` L2 Merkle genesis root, unilateral Escape Hatch (`>= 6` blocks),
//!      Winterfell STARK batch conservation compatibility, and copy-paste deployment kit.
//!
//! Zero fake data: live registry starts at `0` custom stablecoins and `0` custom L2 rollups until
//! real issuers register via `POST /api/v2/factory/stablecoin` or `POST /api/v2/factory/l2`.
//! Ephemeral sandbox demos run via `run_factory_sandbox_simulation()` without mutating live state.

use crate::crypto::{sha256_concat, Hash32};
use crate::l2_rollup::{
    compute_l1_anchor_commitment, compute_l2_state_root_and_escape_proof, L2ShieldedLeaf,
};
use crate::nsusd::NsUsdAssetId;
use crate::stark::{
    prove_covenant_transition, verify_covenant_stark, CovenantPublicInputs, CovenantTxKind,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Reserved protocol asset IDs: `0 = LOVE`, `1 = nsUSD`, `2 = nsUSD-B`, `3..99` reserved.
pub const FIRST_CUSTOM_ASSET_ID: u32 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StablecoinBackingModel {
    /// 1:1 regulated bank FBO cash / short-duration U.S. Treasuries + viewing keys for auditors.
    RegulatedBankReserve,
    /// 1:1 on-chain ERC-20 escrow inside ownerless `NewsatoshiBackingVault` (USDC/USDS/BUIDL).
    OnChainEscrow,
    /// Zero-freeze over-collateralized BTC or LOVE CDP vaults (>=150% MCR + PriorLock oracle).
    SovereignCdp,
}

impl StablecoinBackingModel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RegulatedBankReserve => "regulated_bank_reserve",
            Self::OnChainEscrow => "on_chain_escrow",
            Self::SovereignCdp => "sovereign_cdp",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "regulated_bank_reserve" | "bank" | "bank_reserve" | "fbo" => {
                Ok(Self::RegulatedBankReserve)
            }
            "on_chain_escrow" | "escrow" | "usdc_escrow" => Ok(Self::OnChainEscrow),
            "sovereign_cdp" | "cdp" | "btc_cdp" => Ok(Self::SovereignCdp),
            other => Err(format!(
                "invalid backing_model '{other}' (expected: regulated_bank_reserve | on_chain_escrow | sovereign_cdp)"
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomStablecoinSpec {
    pub asset_id: u32,
    pub ticker: String,
    pub name: String,
    pub peg_currency: String,
    pub issuer_name: String,
    pub issuer_pubkey_hex: String,
    pub backing_model: StablecoinBackingModel,
    pub min_collateral_ratio_bps: u64,
    pub regulator_viewing_key_enabled: bool,
    pub viewing_key_commitment_hex: Option<String>,
    pub settlement_chains: Vec<String>,
    pub genesis_commitment_hex: String,
    pub protocol_fee_bps: u16,
    pub created_at_unix: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SovereignL2Spec {
    pub rollup_id_hex: String,
    pub slug: String,
    pub name: String,
    pub operator_name: String,
    pub operator_pubkey_hex: String,
    pub settled_asset_ids: Vec<u32>,
    pub escape_hatch_delay_blocks: u64,
    pub genesis_state_root_hex: String,
    pub l1_anchor_commitment_hex: String,
    pub l1_anchor_height: u64,
    pub protocol_fee_bps: u16,
    pub created_at_unix: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SovereignFactoryRegistry {
    pub stablecoins: BTreeMap<String, CustomStablecoinSpec>,
    pub l2_rollups: BTreeMap<String, SovereignL2Spec>,
}

/// Derives a collision-resistant deterministic `asset_id >= 100` from
/// `SHA256("nsSTABLE1" || ticker || issuer_pubkey || backing_model)`.
pub fn derive_stablecoin_asset_id(
    ticker_upper: &str,
    issuer_pubkey_hex: &str,
    model: StablecoinBackingModel,
) -> u32 {
    let h = sha256_concat(&[
        b"nsSTABLE1",
        ticker_upper.as_bytes(),
        issuer_pubkey_hex.as_bytes(),
        model.as_str().as_bytes(),
    ]);
    let raw = u32::from_le_bytes([h[0], h[1], h[2], h[3]]);
    let span = u32::MAX - FIRST_CUSTOM_ASSET_ID;
    FIRST_CUSTOM_ASSET_ID + (raw % span)
}

fn validate_ticker(raw: &str) -> Result<String, String> {
    let t = raw.trim().to_ascii_uppercase();
    if t.len() < 3 || t.len() > 12 {
        return Err("ticker must be 3..12 alphanumeric characters".to_string());
    }
    if !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("ticker must contain only ASCII letters, digits, or '-'".to_string());
    }
    if matches!(t.as_str(), "LOVE" | "NSUSD" | "NSUSD-B" | "BTC" | "ETH") {
        return Err(format!("ticker '{t}' is reserved for canonical base assets"));
    }
    Ok(t)
}

fn validate_slug(raw: &str) -> Result<String, String> {
    let s = raw.trim().to_ascii_lowercase();
    if s.len() < 3 || s.len() > 32 {
        return Err("slug must be 3..32 lowercase alphanumeric/hyphen characters".to_string());
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("slug must contain only lowercase ASCII letters, digits, or '-'".to_string());
    }
    if matches!(s.as_str(), "mainnet" | "commons" | "nsm2" | "l1") {
        return Err(format!("slug '{s}' is reserved"));
    }
    Ok(s)
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub struct LaunchStablecoinInput<'a> {
    pub ticker: &'a str,
    pub name: &'a str,
    pub peg_currency: &'a str,
    pub issuer_name: &'a str,
    pub issuer_pubkey_hex: &'a str,
    pub backing_model: StablecoinBackingModel,
    pub regulator_viewing_key_enabled: bool,
    pub settlement_chains: Vec<String>,
}

pub struct LaunchL2Input<'a> {
    pub slug: &'a str,
    pub name: &'a str,
    pub operator_name: &'a str,
    pub operator_pubkey_hex: &'a str,
    pub settled_asset_ids: Vec<u32>,
    pub escape_hatch_delay_blocks: u64,
    pub l1_tip_hex: &'a str,
    pub l1_height: u64,
}

impl SovereignFactoryRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a permissionless custom branded stablecoin (`asset_id >= 100`) at 0% protocol fee.
    pub fn launch_stablecoin(
        &mut self,
        input: LaunchStablecoinInput<'_>,
    ) -> Result<CustomStablecoinSpec, String> {
        let ticker = validate_ticker(input.ticker)?;
        if self.stablecoins.contains_key(&ticker) {
            return Err(format!("stablecoin ticker '{ticker}' is already registered"));
        }
        let name = input.name.trim();
        if name.len() < 2 || name.len() > 64 {
            return Err("name must be 2..64 characters".to_string());
        }
        let issuer_name = input.issuer_name.trim();
        if issuer_name.len() < 2 || issuer_name.len() > 64 {
            return Err("issuer_name must be 2..64 characters".to_string());
        }
        let peg = input.peg_currency.trim().to_ascii_uppercase();
        if peg.len() < 3 || peg.len() > 8 {
            return Err("peg_currency must be 3..8 characters (e.g. USD, EUR, GBP, GOLD)".to_string());
        }
        let pk = input.issuer_pubkey_hex.trim().to_ascii_lowercase();
        if pk.len() < 16 || pk.len() > 130 {
            return Err("issuer_pubkey_hex must be 16..130 characters".to_string());
        }

        let mut asset_id = derive_stablecoin_asset_id(&ticker, &pk, input.backing_model);
        while self.stablecoins.values().any(|s| s.asset_id == asset_id) {
            asset_id = asset_id.saturating_add(1).max(FIRST_CUSTOM_ASSET_ID);
        }

        let min_cr_bps: u64 = match input.backing_model {
            StablecoinBackingModel::RegulatedBankReserve => 10_000, // 100% 1:1 FBO cash/T-bills
            StablecoinBackingModel::OnChainEscrow => 10_000,        // 100% 1:1 on-chain vault
            StablecoinBackingModel::SovereignCdp => 15_000,         // 150% over-collateralized CDP
        };

        let chains = if input.settlement_chains.is_empty() {
            vec![
                "newsatoshi-l1".to_string(),
                "base".to_string(),
                "optimism".to_string(),
                "arbitrum".to_string(),
            ]
        } else {
            input
                .settlement_chains
                .into_iter()
                .map(|c| c.trim().to_ascii_lowercase())
                .filter(|c| !c.is_empty())
                .take(8)
                .collect()
        };

        let viewing_key_commitment_hex = if input.regulator_viewing_key_enabled {
            Some(hex::encode(sha256_concat(&[
                b"nsVIEWKEY1",
                ticker.as_bytes(),
                pk.as_bytes(),
            ])))
        } else {
            None
        };

        let created_at_unix = now_unix_secs();
        let genesis_commitment = sha256_concat(&[
            b"nsSTABLE_GENESIS1",
            &asset_id.to_le_bytes(),
            ticker.as_bytes(),
            peg.as_bytes(),
            pk.as_bytes(),
            input.backing_model.as_str().as_bytes(),
            &min_cr_bps.to_le_bytes(),
            &created_at_unix.to_le_bytes(),
        ]);

        let spec = CustomStablecoinSpec {
            asset_id,
            ticker: ticker.clone(),
            name: name.to_string(),
            peg_currency: peg,
            issuer_name: issuer_name.to_string(),
            issuer_pubkey_hex: pk,
            backing_model: input.backing_model,
            min_collateral_ratio_bps: min_cr_bps,
            regulator_viewing_key_enabled: input.regulator_viewing_key_enabled,
            viewing_key_commitment_hex,
            settlement_chains: chains,
            genesis_commitment_hex: hex::encode(genesis_commitment),
            protocol_fee_bps: 0,
            created_at_unix,
        };
        self.stablecoins.insert(ticker, spec.clone());
        Ok(spec)
    }

    /// Registers a permissionless sovereign STARK L2 rollup anchored to Mainnet v2 (`nsM2`).
    pub fn launch_sovereign_l2(
        &mut self,
        input: LaunchL2Input<'_>,
    ) -> Result<SovereignL2Spec, String> {
        let slug = validate_slug(input.slug)?;
        if self.l2_rollups.contains_key(&slug) {
            return Err(format!("L2 rollup slug '{slug}' is already registered"));
        }
        let name = input.name.trim();
        if name.len() < 2 || name.len() > 64 {
            return Err("name must be 2..64 characters".to_string());
        }
        let op_name = input.operator_name.trim();
        if op_name.len() < 2 || op_name.len() > 64 {
            return Err("operator_name must be 2..64 characters".to_string());
        }
        let op_pk = input.operator_pubkey_hex.trim().to_ascii_lowercase();
        if op_pk.len() < 16 || op_pk.len() > 130 {
            return Err("operator_pubkey_hex must be 16..130 characters".to_string());
        }
        let delay = input.escape_hatch_delay_blocks.clamp(6, 4_320);

        let mut assets = input.settled_asset_ids;
        if assets.is_empty() {
            assets = vec![0, 1, 2]; // LOVE, nsUSD, nsUSD-B
        }
        assets.sort_unstable();
        assets.dedup();

        let rollup_id = sha256_concat(&[b"nsL2ROLLUP1", slug.as_bytes(), op_pk.as_bytes()]);
        let genesis_leaves = vec![L2ShieldedLeaf {
            account_hash: format!("genesis-treasury-{slug}"),
            asset_id: NsUsdAssetId::NativeLove,
            balance_units: 0,
            nonce: 0,
        }];
        let (genesis_root, _) = compute_l2_state_root_and_escape_proof(&genesis_leaves, None);
        let l1_anchor = compute_l1_anchor_commitment(
            [0u8; 32],
            genesis_root,
            rollup_id,
            input.l1_tip_hex,
            0,
        );

        let spec = SovereignL2Spec {
            rollup_id_hex: hex::encode(rollup_id),
            slug: slug.clone(),
            name: name.to_string(),
            operator_name: op_name.to_string(),
            operator_pubkey_hex: op_pk,
            settled_asset_ids: assets,
            escape_hatch_delay_blocks: delay,
            genesis_state_root_hex: hex::encode(genesis_root),
            l1_anchor_commitment_hex: hex::encode(l1_anchor),
            l1_anchor_height: input.l1_height,
            protocol_fee_bps: 0,
            created_at_unix: now_unix_secs(),
        };
        self.l2_rollups.insert(slug, spec.clone());
        Ok(spec)
    }

    /// Deterministic SHA-256 registry root committing to every registered stablecoin & L2 rollup.
    pub fn registry_root(&self) -> Hash32 {
        let mut parts: Vec<Vec<u8>> = vec![b"nsFACTORY_ROOT1".to_vec()];
        for s in self.stablecoins.values() {
            parts.push(s.genesis_commitment_hex.as_bytes().to_vec());
        }
        for r in self.l2_rollups.values() {
            parts.push(r.l1_anchor_commitment_hex.as_bytes().to_vec());
        }
        let refs: Vec<&[u8]> = parts.iter().map(|v| v.as_slice()).collect();
        sha256_concat(&refs)
    }

    pub fn summary_json(&self) -> Value {
        json!({
            "honest_initial_state": self.stablecoins.is_empty() && self.l2_rollups.is_empty(),
            "protocol_fee_bps": 0,
            "first_custom_asset_id": FIRST_CUSTOM_ASSET_ID,
            "registry_root_hex": hex::encode(self.registry_root()),
            "custom_stablecoins_count": self.stablecoins.len(),
            "sovereign_l2_rollups_count": self.l2_rollups.len(),
            "stablecoins": self.stablecoins.values().collect::<Vec<_>>(),
            "l2_rollups": self.l2_rollups.values().collect::<Vec<_>>(),
            "supported_backing_models": [
                "regulated_bank_reserve",
                "on_chain_escrow",
                "sovereign_cdp"
            ]
        })
    }
}

/// Builds a copy-paste deployment kit for a newly created Stablecoin or Sovereign L2.
pub fn build_l2_deployment_kit(spec: &SovereignL2Spec) -> Value {
    json!({
        "docker_run": format!(
            "docker run -d --name ns-l2-{slug} -p 8080:8080 \\\n  -e NEWSATOSHI_L2_ROLLUP_ID={rid} \\\n  -e NEWSATOSHI_L2_SLUG={slug} \\\n  -e NEWSATOSHI_MAINNET_SEED_URL=https://newsatoshi.org/mainnet/status \\\n  ghcr.io/theguysaccount/newsatoshi-paywall:v0.36.1",
            slug = spec.slug,
            rid = spec.rollup_id_hex
        ),
        "l1_anchor_cli": format!(
            "newsatoshi --net mainnet-v2 anchor --commitment {} --memo \"l2:{}\"",
            spec.l1_anchor_commitment_hex, spec.slug
        ),
        "escape_hatch_guarantee": format!(
            "Unilateral Merkle-inclusion L1 exit activates after {} Mainnet v2 blocks if sequencer halts (zero sequencer custody).",
            spec.escape_hatch_delay_blocks
        )
    })
}

/// Runs a 100% isolated ephemeral sandbox simulation (never mutates live registry):
/// launches a sample Regulated Bank Stablecoin + Sovereign STARK L2, verifies a real Winterfell
/// STARK conservation proof and unilateral Merkle Escape Hatch proof.
pub fn run_factory_sandbox_simulation(
    ticker: &str,
    issuer_name: &str,
    backing_model: StablecoinBackingModel,
    l2_slug: &str,
    l1_height: u64,
    l1_tip: &str,
) -> Result<Value, String> {
    let mut sandbox = SovereignFactoryRegistry::new();

    let stable = sandbox.launch_stablecoin(LaunchStablecoinInput {
        ticker,
        name: &format!("{issuer_name} Regulated Dollar"),
        peg_currency: "USD",
        issuer_name,
        issuer_pubkey_hex: "0x00000000000000000000000000000000sandboxissuer001",
        backing_model,
        regulator_viewing_key_enabled: true,
        settlement_chains: vec![
            "newsatoshi-l1".to_string(),
            "base".to_string(),
            "optimism".to_string(),
            "arbitrum".to_string(),
        ],
    })?;

    let l2 = sandbox.launch_sovereign_l2(LaunchL2Input {
        slug: l2_slug,
        name: &format!("{issuer_name} Sovereign STARK L2"),
        operator_name: issuer_name,
        operator_pubkey_hex: "0x00000000000000000000000000000000sandboxoperat001",
        settled_asset_ids: vec![0, 1, stable.asset_id],
        escape_hatch_delay_blocks: 72,
        l1_tip_hex: l1_tip,
        l1_height,
    })?;

    let prev_leaves = vec![
        L2ShieldedLeaf {
            account_hash: "treasury-mint-01".to_string(),
            asset_id: NsUsdAssetId::SovereignNsUsd,
            balance_units: 1_000_000,
            nonce: 0,
        },
        L2ShieldedLeaf {
            account_hash: "ai-merchant-01".to_string(),
            asset_id: NsUsdAssetId::SovereignNsUsd,
            balance_units: 0,
            nonce: 0,
        },
    ];
    let next_leaves = vec![
        L2ShieldedLeaf {
            account_hash: "treasury-mint-01".to_string(),
            asset_id: NsUsdAssetId::SovereignNsUsd,
            balance_units: 750_000,
            nonce: 1,
        },
        L2ShieldedLeaf {
            account_hash: "ai-merchant-01".to_string(),
            asset_id: NsUsdAssetId::SovereignNsUsd,
            balance_units: 250_000,
            nonce: 1,
        },
    ];
    let (prev_root, _) = compute_l2_state_root_and_escape_proof(&prev_leaves, None);
    let (next_root, escape_proof) = compute_l2_state_root_and_escape_proof(&next_leaves, Some(1));
    let rule_hash = hex::encode(sha256_concat(&[b"nsFACTORY-STARK-V1", l2.rollup_id_hex.as_bytes()]));
    let pub_inputs = CovenantPublicInputs {
        tx_kind: CovenantTxKind::StreamingSettlement,
        label: format!("factory-l2-{}-sandbox", l2.slug),
        tx_nonce: 1,
        covenant_rule_hash: rule_hash.clone(),
        change_covenant_rule_hash: rule_hash,
        prev_state_root: hex::encode(prev_root),
        new_state_root: hex::encode(next_root),
        input_amount: 1_000_000,
        payout_amount: 250_000,
        change_amount: 750_000,
        max_bounty_payout: 1_000_000,
        reinvest_bps: 7_500,
        min_soil_health_score: 0,
        soil_health_score: 0,
    };
    let stark_artifact = prove_covenant_transition(pub_inputs)?;
    let t_v = Instant::now();
    let stark_verify = verify_covenant_stark(&stark_artifact)?;
    let verify_ms = (t_v.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;

    Ok(json!({
        "ok": stark_verify.valid && escape_proof.as_ref().map(|p| p.verified).unwrap_or(false),
        "sandbox": true,
        "mutated_live_state": false,
        "stablecoin": stable,
        "sovereign_l2": l2,
        "deployment_kit": build_l2_deployment_kit(&l2),
        "cryptographic_verification": {
            "stark_valid": stark_verify.valid,
            "stark_proof_bytes": stark_artifact.proof_hex.len() / 2,
            "stark_verify_ms": verify_ms,
            "escape_hatch_proof": escape_proof
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stablecoin_and_l2_factory_deterministic_and_isolated() {
        let mut reg = SovereignFactoryRegistry::new();
        assert!(reg.stablecoins.is_empty());
        assert!(reg.l2_rollups.is_empty());

        let sc = reg
            .launch_stablecoin(LaunchStablecoinInput {
                ticker: "BANKUSD",
                name: "First Reserve Bank Dollar",
                peg_currency: "USD",
                issuer_name: "First Reserve Bank",
                issuer_pubkey_hex: "0x11112222333344445555666677778888",
                backing_model: StablecoinBackingModel::RegulatedBankReserve,
                regulator_viewing_key_enabled: true,
                settlement_chains: vec!["base".to_string(), "newsatoshi-l1".to_string()],
            })
            .expect("launch stablecoin");
        assert!(sc.asset_id >= FIRST_CUSTOM_ASSET_ID);
        assert_eq!(sc.protocol_fee_bps, 0);
        assert!(sc.viewing_key_commitment_hex.is_some());

        // Reject duplicate ticker and reserved ticker
        assert!(reg
            .launch_stablecoin(LaunchStablecoinInput {
                ticker: "bankusd",
                name: "Dup",
                peg_currency: "USD",
                issuer_name: "Other",
                issuer_pubkey_hex: "0x99998888777766665555444433332222",
                backing_model: StablecoinBackingModel::OnChainEscrow,
                regulator_viewing_key_enabled: false,
                settlement_chains: vec![],
            })
            .is_err());
        assert!(validate_ticker("LOVE").is_err());
        assert!(validate_ticker("nsUSD").is_err());

        let l2 = reg
            .launch_sovereign_l2(LaunchL2Input {
                slug: "bankusd-ai-l2",
                name: "BankUSD AI Settlement Rollup",
                operator_name: "First Reserve Bank",
                operator_pubkey_hex: "0x11112222333344445555666677778888",
                settled_asset_ids: vec![0, 1, sc.asset_id],
                escape_hatch_delay_blocks: 72,
                l1_tip_hex: "00000000ba1cdad4aa8e82d6613b4df536394c41008fc3a0914a47a44604b9c8",
                l1_height: 331,
            })
            .expect("launch l2");
        assert_eq!(l2.protocol_fee_bps, 0);
        assert_eq!(l2.escape_hatch_delay_blocks, 72);
        assert!(l2.settled_asset_ids.contains(&sc.asset_id));

        let sim = run_factory_sandbox_simulation(
            "TESTUSD",
            "Sandbox Bank",
            StablecoinBackingModel::RegulatedBankReserve,
            "sandbox-bank-l2",
            331,
            "00000000ba1cdad4aa8e82d6613b4df536394c41008fc3a0914a47a44604b9c8",
        )
        .expect("sandbox simulation");
        assert_eq!(sim["ok"], json!(true));
        assert_eq!(sim["sandbox"], json!(true));
        assert_eq!(sim["cryptographic_verification"]["stark_valid"], json!(true));
    }
}
