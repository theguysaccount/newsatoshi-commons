//! Live Multi-Chain Proof-of-Backing Engine (`src/backing.rs`).
//!
//! Queries live JSON-RPC state of the immutable, ownerless `NewsatoshiBackingVault.sol`
//! contract deployed identically at `0x37Ed94525DBE408fA72cDd156409875666679265` across
//! Base (`8453`), Arbitrum One (`42161`), Optimism (`10`), and Polygon (`137`), combining
//! verified on-chain ETH/POL/USDC reserves with live Newsatoshi L1 supply and offline-signed
//! RWA deeds (`SLH-DSA` / `XMSS` + Winterfell STARKs).
//!
//! Zero fake data: every reserve figure is read strictly from live on-chain `eth_call`
//! responses (selector `0xa7d00c78` = `getReserveSummary(address)`). If an RPC fails,
//! `live` is honestly set to `false` with the exact error instead of fabricating values.

use crate::seed::SeedSnapshot;
use crate::storage::ApiStateView;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Canonical universal immutable, ownerless `NewsatoshiBackingVault.sol` deployed identically
/// at nonce 0 on Base (8453), Arbitrum One (42161), Optimism (10), and Polygon (137),
/// all four Sourcify `exact_match` verified.
pub const CANONICAL_BACKING_VAULT: &str = "0x37Ed94525DBE408fA72cDd156409875666679265";

/// Genesis Charter #1 anchored on Base Mainnet (`0x7a8183...eb47`) with first real `0.001 ETH` deposit (`0xc354e0...bf5c`).
pub const GENESIS_BASE_CHARTER_ID_HEX: &str =
    "0xc003ed8a9a9bbb064f32aa1c307c206cdc6487d469bbaaec3b4bd27c3a16ac65";
pub const GENESIS_BASE_DEPOSIT_TX: &str =
    "0xc354e0cca5bd2f0e9ac22193ef6f4ea505393ab4be4177251f9f47ca1c10bf5c";
pub const GENESIS_BASE_ANCHOR_TX: &str =
    "0x7a8183b60b6b1d8b46cc46b2eb022b3fa046f73b9f1f7ee464a5a3415815eb47";

/// 4-byte Keccak-256 selector for `getReserveSummary(address)`.
const GET_RESERVE_SUMMARY_SELECTOR: &str = "a7d00c78";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChainBackingSnapshot {
    pub chain: String,
    pub chain_id: u64,
    pub native_symbol: String,
    pub vault_address: String,
    pub explorer_url: String,
    pub sourcify_url: String,
    pub usdc_address: String,
    pub live: bool,
    /// Native reserve in wei (decimal string for exact 256-bit precision).
    pub native_reserve_wei: String,
    /// Human-formatted native reserve (18 decimals).
    pub native_reserve_formatted: String,
    /// Canonical USDC reserve in 6-decimal base units.
    pub usdc_reserve_raw: u64,
    /// Human-formatted USDC reserve (6 decimals).
    pub usdc_reserve_formatted: String,
    /// Cumulative deposits recorded on this chain's vault.
    pub deposits_count: u64,
    /// Immutable Web4 business charters anchored on this chain's vault.
    pub anchored_charters_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BackingProofSummary {
    pub ok: bool,
    pub fetched_at_unix: u64,
    pub canonical_vault_address: String,
    pub canonical_htlc_address: String,
    pub genesis_base_charter_id: String,
    pub genesis_base_anchor_tx: String,
    pub genesis_base_deposit_tx: String,
    pub chains: Vec<ChainBackingSnapshot>,
    pub total_eth_wei: String,
    pub total_eth_formatted: String,
    pub total_pol_wei: String,
    pub total_pol_formatted: String,
    pub total_usdc_raw: u64,
    pub total_usdc_formatted: String,
    pub total_vault_deposits: u64,
    pub total_onchain_anchored_charters: u64,
    pub mainnet_issued_love: Option<f64>,
    pub mainnet_height: Option<u64>,
    pub verified_rwa_parcels: usize,
    pub verified_rwa_kwh: u64,
    pub verified_stark_proofs: usize,
    pub commons_treasury_love: u64,
    /// Verifiable ETH wei locked per issued Mainnet LOVE (None if Mainnet status unavailable or issued == 0).
    pub eth_wei_per_issued_love: Option<f64>,
    /// Verifiable USDC micro-units per issued Mainnet LOVE.
    pub usdc_micro_per_issued_love: Option<f64>,
    pub registered_web4_charters: usize,
    pub registered_web4_services: usize,
    pub open_web4_bounties: usize,
}

struct ChainSpec {
    name: &'static str,
    chain_id: u64,
    native_symbol: &'static str,
    rpc_url: &'static str,
    usdc_address: &'static str,
    explorer_base: &'static str,
}

const LIVE_VAULT_CHAINS: [ChainSpec; 4] = [
    ChainSpec {
        name: "base",
        chain_id: 8453,
        native_symbol: "ETH",
        rpc_url: "https://mainnet.base.org",
        usdc_address: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
        explorer_base: "https://basescan.org/address/",
    },
    ChainSpec {
        name: "arbitrum",
        chain_id: 42161,
        native_symbol: "ETH",
        rpc_url: "https://arb1.arbitrum.io/rpc",
        usdc_address: "0xaf88d065e77c8cC2239327C5EDb3A432268e5831",
        explorer_base: "https://arbiscan.io/address/",
    },
    ChainSpec {
        name: "optimism",
        chain_id: 10,
        native_symbol: "ETH",
        rpc_url: "https://mainnet.optimism.io",
        usdc_address: "0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85",
        explorer_base: "https://optimistic.etherscan.io/address/",
    },
    ChainSpec {
        name: "polygon",
        chain_id: 137,
        native_symbol: "POL",
        rpc_url: "https://polygon.drpc.org",
        usdc_address: "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359",
        explorer_base: "https://polygonscan.com/address/",
    },
];

/// Encodes `getReserveSummary(address token)` calldata (`4 + 32 = 36` bytes).
pub fn encode_get_reserve_summary_calldata(token_addr: &str) -> Result<String, String> {
    let clean = token_addr
        .trim()
        .strip_prefix("0x")
        .or_else(|| token_addr.trim().strip_prefix("0X"))
        .ok_or_else(|| "token address must start with 0x".to_string())?;
    if clean.len() != 40 || !clean.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("token address must be 20-byte hex".to_string());
    }
    Ok(format!(
        "0x{GET_RESERVE_SUMMARY_SELECTOR}{:0>64}",
        clean.to_ascii_lowercase()
    ))
}

/// Decodes the `(uint256 totalReserve, uint256 depositsCount, uint256 anchoredChartersCount)` ABI tuple.
pub fn decode_reserve_summary_abi(hex_result: &str) -> Result<(u128, u64, u64), String> {
    let raw = hex_result
        .trim()
        .strip_prefix("0x")
        .or_else(|| hex_result.trim().strip_prefix("0X"))
        .unwrap_or(hex_result.trim());
    if raw.len() < 192 {
        return Err(format!(
            "unexpected ABI response length {} (expected >= 192 hex chars)",
            raw.len()
        ));
    }
    let word0 = &raw[0..64];
    let word1 = &raw[64..128];
    let word2 = &raw[128..192];

    let total_u128 = u128::from_str_radix(&word0[32..64], 16)
        .map_err(|e| format!("failed parsing totalReserve u128: {e}"))?;
    let deposits_u64 = u64::from_str_radix(&word1[48..64], 16)
        .map_err(|e| format!("failed parsing depositsCount u64: {e}"))?;
    let anchored_u64 = u64::from_str_radix(&word2[48..64], 16)
        .map_err(|e| format!("failed parsing anchoredChartersCount u64: {e}"))?;
    Ok((total_u128, deposits_u64, anchored_u64))
}

pub fn format_wei_18(wei: u128) -> String {
    let whole = wei / 1_000_000_000_000_000_000u128;
    let frac = (wei % 1_000_000_000_000_000_000u128) / 1_000_000_000_000u128; // 6 decimal places
    format!("{whole}.{frac:06}")
}

pub fn format_usdc_6(raw: u64) -> String {
    let whole = raw / 1_000_000;
    let frac = raw % 1_000_000;
    format!("{whole}.{frac:06}")
}

fn eth_call_reserve_summary(
    agent: &ureq::Agent,
    rpc_url: &str,
    token_addr: &str,
) -> Result<(u128, u64, u64), String> {
    let calldata = encode_get_reserve_summary_calldata(token_addr)?;
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_call",
        "params": [
            { "to": CANONICAL_BACKING_VAULT, "data": calldata },
            "latest"
        ]
    });
    let body_str = payload.to_string();
    let mut resp = agent
        .post(rpc_url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("User-Agent", "newsatoshi-commons-backing/0.11")
        .send(body_str.as_bytes())
        .map_err(|e| format!("RPC request failed: {e}"))?;
    let resp_text = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("RPC read failed: {e}"))?;
    let v: Value = serde_json::from_str(&resp_text)
        .map_err(|e| format!("RPC JSON decode failed: {e}"))?;
    if let Some(err) = v.get("error") {
        return Err(format!("RPC error: {err}"));
    }
    let result_hex = v
        .get("result")
        .and_then(|r| r.as_str())
        .ok_or_else(|| "missing result field in eth_call".to_string())?;
    decode_reserve_summary_abi(result_hex)
}

/// Polls live on-chain reserve balances across all 4 EVM mainnets and aggregates them with
/// live Newsatoshi L1 supply and Commons RWA/Web4 counts.
pub fn fetch_live_backing_summary(
    state: &ApiStateView,
    seed: &SeedSnapshot,
    offline_mode: bool,
) -> BackingProofSummary {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(4)))
        .build()
        .new_agent();

    let mut chains = Vec::with_capacity(LIVE_VAULT_CHAINS.len());
    let mut total_eth_wei: u128 = 0;
    let mut total_pol_wei: u128 = 0;
    let mut total_usdc_raw: u64 = 0;
    let mut total_deposits: u64 = 0;
    let mut total_anchored: u64 = 0;

    for spec in &LIVE_VAULT_CHAINS {
        if offline_mode {
            chains.push(ChainBackingSnapshot {
                chain: spec.name.to_string(),
                chain_id: spec.chain_id,
                native_symbol: spec.native_symbol.to_string(),
                vault_address: CANONICAL_BACKING_VAULT.to_string(),
                explorer_url: format!("{}{}", spec.explorer_base, CANONICAL_BACKING_VAULT),
                sourcify_url: format!(
                    "https://repo.sourcify.dev/{}/{}",
                    spec.chain_id, CANONICAL_BACKING_VAULT
                ),
                usdc_address: spec.usdc_address.to_string(),
                live: false,
                native_reserve_wei: "0".to_string(),
                native_reserve_formatted: "0.000000".to_string(),
                usdc_reserve_raw: 0,
                usdc_reserve_formatted: "0.000000".to_string(),
                deposits_count: 0,
                anchored_charters_count: 0,
                error: Some("offline test snapshot".to_string()),
            });
            continue;
        }

        let native_res = eth_call_reserve_summary(
            &agent,
            spec.rpc_url,
            "0x0000000000000000000000000000000000000000",
        );
        let usdc_res = eth_call_reserve_summary(&agent, spec.rpc_url, spec.usdc_address);

        match (native_res, usdc_res) {
            (Ok((nat_wei, deps, anchored)), Ok((usdc_u128, _, _))) => {
                let usdc_u64 = u64::try_from(usdc_u128).unwrap_or(u64::MAX);
                if spec.native_symbol == "ETH" {
                    total_eth_wei = total_eth_wei.saturating_add(nat_wei);
                } else {
                    total_pol_wei = total_pol_wei.saturating_add(nat_wei);
                }
                total_usdc_raw = total_usdc_raw.saturating_add(usdc_u64);
                total_deposits = total_deposits.saturating_add(deps);
                total_anchored = total_anchored.saturating_add(anchored);

                chains.push(ChainBackingSnapshot {
                    chain: spec.name.to_string(),
                    chain_id: spec.chain_id,
                    native_symbol: spec.native_symbol.to_string(),
                    vault_address: CANONICAL_BACKING_VAULT.to_string(),
                    explorer_url: format!("{}{}", spec.explorer_base, CANONICAL_BACKING_VAULT),
                    sourcify_url: format!(
                        "https://repo.sourcify.dev/{}/{}",
                        spec.chain_id, CANONICAL_BACKING_VAULT
                    ),
                    usdc_address: spec.usdc_address.to_string(),
                    live: true,
                    native_reserve_wei: nat_wei.to_string(),
                    native_reserve_formatted: format_wei_18(nat_wei),
                    usdc_reserve_raw: usdc_u64,
                    usdc_reserve_formatted: format_usdc_6(usdc_u64),
                    deposits_count: deps,
                    anchored_charters_count: anchored,
                    error: None,
                });
            }
            (Err(e), _) | (_, Err(e)) => {
                chains.push(ChainBackingSnapshot {
                    chain: spec.name.to_string(),
                    chain_id: spec.chain_id,
                    native_symbol: spec.native_symbol.to_string(),
                    vault_address: CANONICAL_BACKING_VAULT.to_string(),
                    explorer_url: format!("{}{}", spec.explorer_base, CANONICAL_BACKING_VAULT),
                    sourcify_url: format!(
                        "https://repo.sourcify.dev/{}/{}",
                        spec.chain_id, CANONICAL_BACKING_VAULT
                    ),
                    usdc_address: spec.usdc_address.to_string(),
                    live: false,
                    native_reserve_wei: "0".to_string(),
                    native_reserve_formatted: "0.000000".to_string(),
                    usdc_reserve_raw: 0,
                    usdc_reserve_formatted: "0.000000".to_string(),
                    deposits_count: 0,
                    anchored_charters_count: 0,
                    error: Some(e),
                });
            }
        }
    }

    let mainnet_issued_love = seed
        .mainnet_status
        .as_ref()
        .map(|m| (m.issued as f64) / 100_000_000.0);
    let mainnet_height = seed.mainnet_status.as_ref().map(|m| m.height);

    let eth_wei_per_issued_love = mainnet_issued_love
        .filter(|&s| s > 0.0)
        .map(|s| (total_eth_wei as f64) / s);
    let usdc_micro_per_issued_love = mainnet_issued_love
        .filter(|&s| s > 0.0)
        .map(|s| (total_usdc_raw as f64) / s);

    BackingProofSummary {
        ok: true,
        fetched_at_unix: now,
        canonical_vault_address: CANONICAL_BACKING_VAULT.to_string(),
        canonical_htlc_address: crate::swap::CANONICAL_EVM_HTLC_CONTRACT.to_string(),
        genesis_base_charter_id: GENESIS_BASE_CHARTER_ID_HEX.to_string(),
        genesis_base_anchor_tx: GENESIS_BASE_ANCHOR_TX.to_string(),
        genesis_base_deposit_tx: GENESIS_BASE_DEPOSIT_TX.to_string(),
        chains,
        total_eth_wei: total_eth_wei.to_string(),
        total_eth_formatted: format_wei_18(total_eth_wei),
        total_pol_wei: total_pol_wei.to_string(),
        total_pol_formatted: format_wei_18(total_pol_wei),
        total_usdc_raw,
        total_usdc_formatted: format_usdc_6(total_usdc_raw),
        total_vault_deposits: total_deposits,
        total_onchain_anchored_charters: total_anchored,
        mainnet_issued_love,
        mainnet_height,
        verified_rwa_parcels: state.ledger_summary.parcels_acquired_count,
        verified_rwa_kwh: state.ledger_summary.total_kwh_verified,
        verified_stark_proofs: state.ledger_summary.stark_proofs_verified,
        commons_treasury_love: state.ledger_summary.balance_love,
        eth_wei_per_issued_love,
        usdc_micro_per_issued_love,
        registered_web4_charters: state.web4_charters.len(),
        registered_web4_services: state.web4_services.len(),
        open_web4_bounties: state.web4_bounties.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_and_decode_vault_reserve_summary_abi() {
        let cd = encode_get_reserve_summary_calldata("0x0000000000000000000000000000000000000000")
            .unwrap();
        assert_eq!(cd.len(), 74); // "0x" + 8 selector + 64 padded address
        assert!(cd.starts_with("0xa7d00c78"));

        // 0.001 ETH = 1_000_000_000_000_000 = 0x38d7ea4c68000, deposits=1, anchored=1
        let abi = format!(
            "0x{:064x}{:064x}{:064x}",
            1_000_000_000_000_000u128, 1u64, 1u64
        );
        let (wei, deps, anchored) = decode_reserve_summary_abi(&abi).unwrap();
        assert_eq!(wei, 1_000_000_000_000_000);
        assert_eq!(deps, 1);
        assert_eq!(anchored, 1);
        assert_eq!(format_wei_18(wei), "0.001000");
        assert_eq!(format_usdc_6(50_250_000), "50.250000");
    }
}
