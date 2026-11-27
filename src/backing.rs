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
/// Live Base Mainnet direct USDC deposit (14.517071 USDC, block 62907393) to `NewsatoshiBackingVault`.
pub const LIVE_BASE_DIRECT_USDC_TX: &str =
    "0xd3c521e3dcdd1465f6f51384a5c721c0c445c3d384282ae0c67b2b07195029ba";
/// Live Base Mainnet `depositErc20` (0.10 USDC, RFC-0028 nsUSD-B memoHash, block 62910185).
pub const LIVE_BASE_NSUSD_DEPOSIT_TX: &str =
    "0x21a8e80c95a4a7e6f4f459ea368764a74fe2c4a8a2d2d24a8a9f94c3b5dd7a2e";
/// Live Base Mainnet SHA-256 HTLC atomic swap proof (`lockEth` 0.0001 ETH at block 62910174 + `claim` preimage reveal at block 62910178).
pub const LIVE_BASE_HTLC_LOCK_TX: &str =
    "0xd90a9d873516ab92594ef71ada381ecc4b5852e9e70bd1652d60b0058e630f16";
pub const LIVE_BASE_HTLC_CLAIM_TX: &str =
    "0xdece20236b816eed13c358b923cf6a8a9e527b8ad72512ecd463c319b8d2d048";
pub const BASE_USDC_ADDRESS: &str = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";

/// 4-byte Keccak-256 selector for `getReserveSummary(address)`.
const GET_RESERVE_SUMMARY_SELECTOR: &str = "a7d00c78";
/// 4-byte Keccak-256 selector for ERC-20 `balanceOf(address)`.
const ERC20_BALANCE_OF_SELECTOR: &str = "70a08231";
/// Keccak-256 topic0 for ERC-20 `Transfer(address,address,uint256)`.
const ERC20_TRANSFER_TOPIC0: &str =
    "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

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
    /// Total verifiable USDC held in the immutable vault (max of contract-tracked `depositErc20` & live ERC-20 `balanceOf`).
    pub usdc_reserve_raw: u64,
    /// Human-formatted total USDC reserve (6 decimals).
    pub usdc_reserve_formatted: String,
    /// Portion recorded via `depositErc20()` (`totalReserveByToken[USDC]`).
    #[serde(default)]
    pub usdc_contract_tracked_raw: u64,
    /// Portion sent via direct ERC-20 `transfer()` into the immutable ownerless vault.
    #[serde(default)]
    pub usdc_direct_transfer_raw: u64,
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
    fallback_rpc_url: Option<&'static str>,
    usdc_address: &'static str,
    explorer_base: &'static str,
}

const LIVE_VAULT_CHAINS: [ChainSpec; 4] = [
    ChainSpec {
        name: "base",
        chain_id: 8453,
        native_symbol: "ETH",
        rpc_url: "https://base-rpc.publicnode.com",
        fallback_rpc_url: Some("https://mainnet.base.org"),
        usdc_address: BASE_USDC_ADDRESS,
        explorer_base: "https://basescan.org/address/",
    },
    ChainSpec {
        name: "arbitrum",
        chain_id: 42161,
        native_symbol: "ETH",
        rpc_url: "https://arb1.arbitrum.io/rpc",
        fallback_rpc_url: Some("https://arbitrum-one-rpc.publicnode.com"),
        usdc_address: "0xaf88d065e77c8cC2239327C5EDb3A432268e5831",
        explorer_base: "https://arbiscan.io/address/",
    },
    ChainSpec {
        name: "optimism",
        chain_id: 10,
        native_symbol: "ETH",
        rpc_url: "https://mainnet.optimism.io",
        fallback_rpc_url: Some("https://optimism-rpc.publicnode.com"),
        usdc_address: "0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85",
        explorer_base: "https://optimistic.etherscan.io/address/",
    },
    ChainSpec {
        name: "polygon",
        chain_id: 137,
        native_symbol: "POL",
        rpc_url: "https://polygon-bor-rpc.publicnode.com",
        fallback_rpc_url: Some("https://polygon.drpc.org"),
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

fn eth_json_rpc_call(agent: &ureq::Agent, rpc_url: &str, payload: &Value) -> Result<Value, String> {
    let body_str = payload.to_string();
    let mut resp = agent
        .post(rpc_url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("User-Agent", "newsatoshi-commons-backing/0.28.3")
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
    v.get("result")
        .cloned()
        .ok_or_else(|| "missing result field in JSON-RPC response".to_string())
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
    let res = eth_json_rpc_call(agent, rpc_url, &payload)?;
    let result_hex = res
        .as_str()
        .ok_or_else(|| "missing hex string in eth_call result".to_string())?;
    decode_reserve_summary_abi(result_hex)
}

fn eth_call_erc20_balance_of(
    agent: &ureq::Agent,
    rpc_url: &str,
    token_addr: &str,
    holder_addr: &str,
) -> Result<u128, String> {
    let clean = holder_addr
        .trim()
        .strip_prefix("0x")
        .or_else(|| holder_addr.trim().strip_prefix("0X"))
        .ok_or_else(|| "holder address must start with 0x".to_string())?;
    let calldata = format!("0x{ERC20_BALANCE_OF_SELECTOR}{:0>64}", clean.to_ascii_lowercase());
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_call",
        "params": [
            { "to": token_addr, "data": calldata },
            "latest"
        ]
    });
    let res = eth_json_rpc_call(agent, rpc_url, &payload)?;
    let hex_str = res
        .as_str()
        .unwrap_or("")
        .trim()
        .strip_prefix("0x")
        .unwrap_or("");
    if hex_str.len() < 64 {
        return Err(format!("short balanceOf response ({} chars)", hex_str.len()));
    }
    u128::from_str_radix(&hex_str[32..64], 16)
        .map_err(|e| format!("failed parsing balanceOf u128: {e}"))
}

/// Pure receipt parser verifying real Base Mainnet USDC `Transfer` logs into `CANONICAL_BACKING_VAULT`
/// (`0x37Ed94525DBE408fA72cDd156409875666679265`) for RFC-0028 Tier-1B `nsUSD-B` 1:1 escrow receipts.
pub fn parse_base_usdc_escrow_receipt(tx_hash: &str, receipt: &Value) -> Result<Value, String> {
    if receipt.is_null() {
        return Err("transaction receipt not found on Base Mainnet (tx pending or unknown)".to_string());
    }
    let status_hex = receipt.get("status").and_then(|v| v.as_str()).unwrap_or("");
    if !status_hex.eq_ignore_ascii_case("0x1") {
        return Err(format!("Base transaction did not succeed (status={status_hex})"));
    }
    let block_hex = receipt.get("blockNumber").and_then(|v| v.as_str()).unwrap_or("0x0");
    let block_number = u64::from_str_radix(block_hex.trim_start_matches("0x"), 16).unwrap_or(0);
    let tx_from = receipt.get("from").and_then(|v| v.as_str()).unwrap_or("");
    let vault_topic = format!(
        "0x{:0>64}",
        CANONICAL_BACKING_VAULT.trim_start_matches("0x").to_ascii_lowercase()
    );

    let logs = receipt
        .get("logs")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "receipt missing logs array".to_string())?;

    let mut total_usdc_micro: u64 = 0;
    let mut matched_transfers: Vec<Value> = Vec::new();
    let mut has_vault_event = false;

    for log in logs {
        let addr = log.get("address").and_then(|v| v.as_str()).unwrap_or("");
        if addr.eq_ignore_ascii_case(CANONICAL_BACKING_VAULT) {
            has_vault_event = true;
        }
        if !addr.eq_ignore_ascii_case(BASE_USDC_ADDRESS) {
            continue;
        }
        let topics = match log.get("topics").and_then(|v| v.as_array()) {
            Some(t) if t.len() >= 3 => t,
            _ => continue,
        };
        let t0 = topics[0].as_str().unwrap_or("");
        let t1 = topics[1].as_str().unwrap_or("");
        let t2 = topics[2].as_str().unwrap_or("");
        if !t0.eq_ignore_ascii_case(ERC20_TRANSFER_TOPIC0) || !t2.eq_ignore_ascii_case(&vault_topic) {
            continue;
        }
        let data_hex = log
            .get("data")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim_start_matches("0x");
        if data_hex.len() < 64 {
            continue;
        }
        let amt = u128::from_str_radix(&data_hex[32..64], 16).unwrap_or(0);
        let amt_u64 = u64::try_from(amt).unwrap_or(u64::MAX);
        if amt_u64 > 0 {
            total_usdc_micro = total_usdc_micro.saturating_add(amt_u64);
            let sender_hex = if t1.len() >= 42 {
                format!("0x{}", &t1[t1.len() - 40..])
            } else {
                t1.to_string()
            };
            matched_transfers.push(json!({
                "from": sender_hex,
                "to": CANONICAL_BACKING_VAULT,
                "usdc_micro": amt_u64,
                "usdc_formatted": format_usdc_6(amt_u64)
            }));
        }
    }

    if total_usdc_micro == 0 {
        return Err(format!(
            "no Base Mainnet USDC Transfer to immutable NewsatoshiBackingVault ({CANONICAL_BACKING_VAULT}) found in tx {tx_hash}"
        ));
    }

    let norm_tx = tx_hash.trim().to_ascii_lowercase();
    let commitment_input = format!("nsUSD-B:base:8453:{norm_tx}:{block_number}:{total_usdc_micro}");
    let receipt_commitment = hex::encode(crate::crypto::sha256(commitment_input.as_bytes()));

    Ok(json!({
        "ok": true,
        "verified_onchain": true,
        "chain": "base",
        "chain_id": 8453,
        "tx_hash": norm_tx,
        "basescan_url": format!("https://basescan.org/tx/{norm_tx}"),
        "block_number": block_number,
        "tx_sender": tx_from,
        "vault_address": CANONICAL_BACKING_VAULT,
        "usdc_token": BASE_USDC_ADDRESS,
        "deposit_path": if has_vault_event { "depositErc20_indexed_vault_event" } else { "direct_erc20_transfer_to_immutable_vault" },
        "verified_usdc_micro": total_usdc_micro,
        "verified_usdc_formatted": format_usdc_6(total_usdc_micro),
        "nsusd_b_mintable_micro": total_usdc_micro,
        "nsusd_b_asset_id": 2,
        "spec": "RFC-0028",
        "issuer_freeze_risk_disclosed": true,
        "sovereign_nsusd_isolated": true,
        "receipt_commitment_sha256": receipt_commitment,
        "matched_transfers": matched_transfers
    }))
}

/// Verifies a real Base Mainnet USDC deposit tx hash live against Base JSON-RPC and returns
/// an auditable RFC-0028 Tier-1B `nsUSD-B` 1:1 escrow verification receipt (zero fake data).
pub fn verify_base_usdc_escrow_tx(tx_hash_hex: &str) -> Result<Value, String> {
    let trimmed = tx_hash_hex.trim();
    let hex_part = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .ok_or_else(|| "tx_hash must start with 0x".to_string())?;
    if hex_part.len() != 64 || !hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("tx_hash must be a 32-byte 0x-prefixed hex hash (66 chars)".to_string());
    }
    let norm_tx = format!("0x{}", hex_part.to_ascii_lowercase());
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(6)))
        .build()
        .new_agent();
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_getTransactionReceipt",
        "params": [norm_tx]
    });
    let receipt = eth_json_rpc_call(&agent, "https://base-rpc.publicnode.com", &payload)
        .or_else(|_| eth_json_rpc_call(&agent, "https://mainnet.base.org", &payload))?;
    parse_base_usdc_escrow_receipt(&norm_tx, &receipt)
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
                usdc_contract_tracked_raw: 0,
                usdc_direct_transfer_raw: 0,
                deposits_count: 0,
                anchored_charters_count: 0,
                error: Some("offline test snapshot".to_string()),
            });
            continue;
        }

        type ChainReserveProbe = ((u128, u64, u64), u128, u128);
        let try_chain = |url: &str| -> Result<ChainReserveProbe, String> {
            let nat = eth_call_reserve_summary(&agent, url, "0x0000000000000000000000000000000000000000")?;
            let (usdc_tracked, _, _) = eth_call_reserve_summary(&agent, url, spec.usdc_address)?;
            let usdc_erc20 = eth_call_erc20_balance_of(&agent, url, spec.usdc_address, CANONICAL_BACKING_VAULT)
                .unwrap_or(usdc_tracked);
            Ok((nat, usdc_tracked, usdc_erc20))
        };

        let chain_res = try_chain(spec.rpc_url).or_else(|first_err| {
            if let Some(fb) = spec.fallback_rpc_url {
                try_chain(fb)
            } else {
                Err(first_err)
            }
        });

        match chain_res {
            Ok(((nat_wei, deps, anchored), usdc_tracked_u128, usdc_erc20_u128)) => {
                let tracked_u64 = u64::try_from(usdc_tracked_u128).unwrap_or(u64::MAX);
                let total_chain_usdc = u64::try_from(usdc_tracked_u128.max(usdc_erc20_u128)).unwrap_or(u64::MAX);
                let direct_u64 = total_chain_usdc.saturating_sub(tracked_u64);
                if spec.native_symbol == "ETH" {
                    total_eth_wei = total_eth_wei.saturating_add(nat_wei);
                } else {
                    total_pol_wei = total_pol_wei.saturating_add(nat_wei);
                }
                total_usdc_raw = total_usdc_raw.saturating_add(total_chain_usdc);
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
                    usdc_reserve_raw: total_chain_usdc,
                    usdc_reserve_formatted: format_usdc_6(total_chain_usdc),
                    usdc_contract_tracked_raw: tracked_u64,
                    usdc_direct_transfer_raw: direct_u64,
                    deposits_count: deps,
                    anchored_charters_count: anchored,
                    error: None,
                });
            }
            Err(e) => {
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
                    usdc_contract_tracked_raw: 0,
                    usdc_direct_transfer_raw: 0,
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

    #[test]
    fn parse_base_usdc_escrow_receipt_verifies_real_transfer_and_rejects_unrelated_tx() {
        let vault_topic = format!(
            "0x{:0>64}",
            CANONICAL_BACKING_VAULT.trim_start_matches("0x").to_ascii_lowercase()
        );
        let receipt = json!({
            "status": "0x1",
            "blockNumber": "0x3bfec31",
            "from": "0x48d1035900a10a5cd071dfd99fdbe7c3caa1cb38",
            "logs": [{
                "address": BASE_USDC_ADDRESS,
                "topics": [
                    ERC20_TRANSFER_TOPIC0,
                    "0x00000000000000000000000048d1035900a10a5cd071dfd99fdbe7c3caa1cb38",
                    vault_topic
                ],
                "data": "0x0000000000000000000000000000000000000000000000000000000000dd834f"
            }]
        });
        let parsed = parse_base_usdc_escrow_receipt(LIVE_BASE_DIRECT_USDC_TX, &receipt).unwrap();
        assert_eq!(parsed["verified_usdc_micro"], 14_517_071);
        assert_eq!(parsed["verified_usdc_formatted"], "14.517071");
        assert_eq!(parsed["nsusd_b_asset_id"], 2);
        assert_eq!(parsed["issuer_freeze_risk_disclosed"], true);

        let bad_receipt = json!({ "status": "0x0", "logs": [] });
        assert!(parse_base_usdc_escrow_receipt(LIVE_BASE_DIRECT_USDC_TX, &bad_receipt).is_err());
    }
}
