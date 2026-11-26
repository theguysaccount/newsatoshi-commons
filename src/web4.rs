//! Web4 Sovereign Business Charter Factory, 402 Paid Service Marketplace,
//! Zero-Custody HTLC / L1 Work Bounties, and ERC-8004 + Post-Quantum Dual Identity Bridge.
//!
//! Directly enforces our Lean 4.36.0 machine-checked Theorem 8 (`web4_monotonic_child_bound`
//! in `formal/NewsatoshiCovenants.lean`):
//!   - Every charter commits deterministically to `(constitution_hash, covenant_rule_hash,
//!     ops_bps, reinvest_bps, backing_bps, max_spend_per_call_love)` where `ops + reinvest + backing = 10_000`.
//!   - Child charters spawned from an existing parent MUST preserve `constitution_hash`,
//!     enforce `child.max_spend_per_call_love <= parent.max_spend_per_call_love`, and
//!     enforce `child.backing_bps >= parent.backing_bps` (zero authority escalation).

use crate::backing::CANONICAL_BACKING_VAULT;
use crate::crypto::{hash32_from_hex, sha256, sha256_concat};
use crate::swap::CANONICAL_EVM_HTLC_CONTRACT;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_63_BIT: u64 = (1u64 << 63) - 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Web4Charter {
    pub charter_id: String,
    /// `0x`-prefixed 32-byte deterministic ID matching `NewsatoshiBackingVault.sol` `bytes32 charterId`.
    pub charter_id_bytes32: String,
    pub name: String,
    /// `"paid_api"` | `"compute_node"` | `"physical_rwa"` | `"liquidity_maker"` | `"agent_collective"`.
    pub category: String,
    pub mission: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_charter_id: Option<String>,
    pub lineage_depth: u32,
    pub constitution_hash: String,
    pub covenant_rule_hash: String,
    pub max_spend_per_call_love: u64,
    pub ops_bps: u16,
    pub reinvest_bps: u16,
    pub backing_bps: u16,
    pub payout_love_address: String,
    pub evm_treasury_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slh_dsa_pubkey_hex: Option<String>,
    pub primary_evm_chain: String,
    pub backing_vault_address: String,
    pub erc8004_uri: String,
    pub cast_anchor_command: String,
    pub cast_deposit_eth_command: String,
    pub web4_kit_launch_command: String,
    pub created_at_unix: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct SpawnCharterRequest {
    pub name: String,
    #[serde(default = "default_category")]
    pub category: String,
    pub mission: String,
    #[serde(default)]
    pub parent_charter_id: Option<String>,
    /// Optional explicit 64-hex constitution hash; if omitted, derived as `SHA256(mission)`.
    #[serde(default)]
    pub constitution_hash: Option<String>,
    pub max_spend_per_call_love: u64,
    pub ops_bps: u16,
    pub reinvest_bps: u16,
    pub backing_bps: u16,
    pub payout_love_address: String,
    pub evm_treasury_address: String,
    #[serde(default)]
    pub slh_dsa_pubkey_hex: Option<String>,
    #[serde(default = "default_evm_chain")]
    pub primary_evm_chain: String,
}

fn default_category() -> String {
    "paid_api".to_string()
}

fn default_evm_chain() -> String {
    "base".to_string()
}

fn validate_evm_addr(raw: &str, label: &str) -> Result<String, String> {
    let s = raw.trim();
    let hex_part = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .ok_or_else(|| format!("{label} must start with 0x"))?;
    if hex_part.len() != 40 || !hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{label} must be a 20-byte 0x hex address"));
    }
    Ok(format!("0x{}", hex_part.to_ascii_lowercase()))
}

fn validate_love_addr(raw: &str, label: &str) -> Result<String, String> {
    let s = raw.trim();
    if !(s.starts_with("ns1") || s.starts_with("nsd1") || s.starts_with("nst1"))
        || s.len() < 20
        || s.len() > 140
        || !s.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return Err(format!(
            "{label} must be a valid Newsatoshi address (ns1... / nsd1... / nst1...)"
        ));
    }
    Ok(s.to_string())
}

fn chain_rpc(chain: &str) -> Result<(&'static str, &'static str), String> {
    match chain.trim().to_ascii_lowercase().as_str() {
        "base" => Ok(("base", "https://mainnet.base.org")),
        "arbitrum" | "arb" => Ok(("arbitrum", "https://arb1.arbitrum.io/rpc")),
        "optimism" | "op" => Ok(("optimism", "https://mainnet.optimism.io")),
        "polygon" | "matic" => Ok(("polygon", "https://polygon.drpc.org")),
        other => Err(format!(
            "unsupported primary_evm_chain '{other}' (expected base, arbitrum, optimism, or polygon)"
        )),
    }
}

/// Compiles and validates a sovereign Web4 business charter, enforcing Lean 4 Theorem 8
/// (`web4_monotonic_child_bound`) whenever `parent` is supplied.
pub fn compile_web4_charter(
    req: &SpawnCharterRequest,
    parent: Option<&Web4Charter>,
    base_url: &str,
) -> Result<Web4Charter, String> {
    let name = req.name.trim();
    if name.len() < 3 || name.len() > 80 {
        return Err("name must be 3..=80 characters".to_string());
    }
    let mission = req.mission.trim();
    if mission.len() < 10 || mission.len() > 400 {
        return Err("mission must be 10..=400 characters".to_string());
    }
    let category = req.category.trim().to_ascii_lowercase();
    if !matches!(
        category.as_str(),
        "paid_api" | "compute_node" | "physical_rwa" | "liquidity_maker" | "agent_collective"
    ) {
        return Err(
            "category must be one of: paid_api, compute_node, physical_rwa, liquidity_maker, agent_collective"
                .to_string(),
        );
    }
    if req.max_spend_per_call_love == 0 || req.max_spend_per_call_love > MAX_63_BIT {
        return Err("max_spend_per_call_love must be in 1..=(2^63 - 1)".to_string());
    }
    let bps_sum: u32 = u32::from(req.ops_bps) + u32::from(req.reinvest_bps) + u32::from(req.backing_bps);
    if bps_sum != 10_000 {
        return Err(format!(
            "ops_bps ({}) + reinvest_bps ({}) + backing_bps ({}) must equal 10000 (got {bps_sum})",
            req.ops_bps, req.reinvest_bps, req.backing_bps
        ));
    }

    let payout_love = validate_love_addr(&req.payout_love_address, "payout_love_address")?;
    let evm_treasury = validate_evm_addr(&req.evm_treasury_address, "evm_treasury_address")?;
    let (norm_chain, rpc_url) = chain_rpc(&req.primary_evm_chain)?;

    let slh_pk_clean = match req.slh_dsa_pubkey_hex.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(h) => {
            if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("slh_dsa_pubkey_hex must be 64 hex chars (32-byte SLH-DSA-SHA2-128f pubkey)".to_string());
            }
            Some(h.to_ascii_lowercase())
        }
        None => None,
    };

    let constitution_bytes = match req.constitution_hash.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(h) => hash32_from_hex(h)?,
        None => {
            if let Some(p) = parent {
                hash32_from_hex(&p.constitution_hash)?
            } else {
                sha256_concat(&[b"NEWSATOSHI_WEB4_CONSTITUTION_V1", mission.as_bytes()])
            }
        }
    };
    let constitution_hash = hex::encode(constitution_bytes);

    // Lean 4 Theorem 8 (`web4_monotonic_child_bound`) enforcement for child charters:
    let (parent_id_out, lineage_depth) = if let Some(p) = parent {
        if constitution_hash != p.constitution_hash {
            return Err(
                "monotonic lineage violation: child constitution_hash must match parent constitution_hash"
                    .to_string(),
            );
        }
        if req.max_spend_per_call_love > p.max_spend_per_call_love {
            return Err(format!(
                "monotonic lineage violation: child max_spend_per_call_love ({}) exceeds parent cap ({})",
                req.max_spend_per_call_love, p.max_spend_per_call_love
            ));
        }
        if req.backing_bps < p.backing_bps {
            return Err(format!(
                "monotonic lineage violation: child backing_bps ({}) is lower than parent backing_bps ({})",
                req.backing_bps, p.backing_bps
            ));
        }
        (Some(p.charter_id.clone()), p.lineage_depth.saturating_add(1))
    } else if req.parent_charter_id.as_deref().map(str::trim).filter(|s| !s.is_empty()).is_some() {
        return Err("specified parent_charter_id was not found in persisted charters".to_string());
    } else {
        (None, 0)
    };

    // Deterministic covenant_rule_hash compatible with NEWSATOSHI/2.1 STARK continuity
    let covenant_rule_bytes = sha256_concat(&[
        b"NEWSATOSHI_WEB4_COVENANT_V2_1",
        &req.max_spend_per_call_love.to_be_bytes(),
        &req.ops_bps.to_be_bytes(),
        &req.reinvest_bps.to_be_bytes(),
        &req.backing_bps.to_be_bytes(),
        &constitution_bytes,
        payout_love.as_bytes(),
        evm_treasury.as_bytes(),
    ]);
    let covenant_rule_hash = hex::encode(covenant_rule_bytes);

    let charter_bytes = sha256_concat(&[
        b"NEWSATOSHI_WEB4_CHARTER_ID_V1",
        name.as_bytes(),
        category.as_bytes(),
        &covenant_rule_bytes,
        parent_id_out.as_deref().unwrap_or("").as_bytes(),
    ]);
    let charter_hex = hex::encode(charter_bytes);
    let charter_id = format!("charter-{}", &charter_hex[..16]);
    let charter_id_bytes32 = format!("0x{charter_hex}");
    let erc8004_uri = format!("{base_url}/api/web4/charters/{charter_id}/erc8004.json");

    let cast_anchor_command = format!(
        "cast send {CANONICAL_BACKING_VAULT} \"registerCharterAnchor(bytes32,bytes32,bytes32,uint16,string)\" {charter_id_bytes32} 0x{constitution_hash} 0x{covenant_rule_hash} {} \"{erc8004_uri}\" --rpc-url {rpc_url}",
        req.backing_bps
    );
    let cast_deposit_eth_command = format!(
        "cast send {CANONICAL_BACKING_VAULT} \"depositEth(bytes32,bytes32)\" {charter_id_bytes32} 0x0000000000000000000000000000000000000000000000000000000000000000 --value 1000000000000000 --rpc-url {rpc_url}"
    );
    let web4_kit_launch_command = format!(
        "CHARTER_ID={charter_id} CHARTER_BYTES32={charter_id_bytes32} BACKING_BPS={} MAX_SPEND_LOVE={} LOVE_RECIPIENT={payout_love} EVM_TREASURY={evm_treasury} node web4-kit/agent-business.mjs",
        req.backing_bps, req.max_spend_per_call_love
    );

    Ok(Web4Charter {
        charter_id,
        charter_id_bytes32,
        name: name.to_string(),
        category,
        mission: mission.to_string(),
        parent_charter_id: parent_id_out,
        lineage_depth,
        constitution_hash,
        covenant_rule_hash,
        max_spend_per_call_love: req.max_spend_per_call_love,
        ops_bps: req.ops_bps,
        reinvest_bps: req.reinvest_bps,
        backing_bps: req.backing_bps,
        payout_love_address: payout_love,
        evm_treasury_address: evm_treasury,
        slh_dsa_pubkey_hex: slh_pk_clean,
        primary_evm_chain: norm_chain.to_string(),
        backing_vault_address: CANONICAL_BACKING_VAULT.to_string(),
        erc8004_uri,
        cast_anchor_command,
        cast_deposit_eth_command,
        web4_kit_launch_command,
        created_at_unix: unix_now(),
    })
}

/// Renders an ERC-8004 + Post-Quantum Dual Agent Registration JSON card for any charter.
pub fn render_erc8004_registration(charter: &Web4Charter, base_url: &str) -> Value {
    json!({
        "type": "https://eips.ethereum.org/EIPS/eip-8004#registration-v1",
        "name": charter.name,
        "description": charter.mission,
        "active": true,
        "charter_id": charter.charter_id,
        "charter_id_bytes32": charter.charter_id_bytes32,
        "category": charter.category,
        "parent_charter_id": charter.parent_charter_id,
        "lineage_depth": charter.lineage_depth,
        "services": [
            {
                "name": "MCP",
                "endpoint": format!("{base_url}/mcp"),
                "version": "2025-03-26"
            },
            {
                "name": "A2A",
                "endpoint": format!("{base_url}/.well-known/agent.json"),
                "version": "1.0"
            },
            {
                "name": "HTTP-402-PayWord",
                "endpoint": format!("{base_url}/api/agent/402-challenge"),
                "version": "NEWSATOSHI-402/1.0"
            }
        ],
        "registrations": [
            {
                "chainId": 8453,
                "backingVault": CANONICAL_BACKING_VAULT,
                "htlcBridge": CANONICAL_EVM_HTLC_CONTRACT,
                "treasury": charter.evm_treasury_address
            },
            {
                "chainId": 42161,
                "backingVault": CANONICAL_BACKING_VAULT,
                "htlcBridge": CANONICAL_EVM_HTLC_CONTRACT,
                "treasury": charter.evm_treasury_address
            },
            {
                "chainId": 10,
                "backingVault": CANONICAL_BACKING_VAULT,
                "htlcBridge": CANONICAL_EVM_HTLC_CONTRACT,
                "treasury": charter.evm_treasury_address
            },
            {
                "chainId": 137,
                "backingVault": CANONICAL_BACKING_VAULT,
                "htlcBridge": CANONICAL_EVM_HTLC_CONTRACT,
                "treasury": charter.evm_treasury_address
            }
        ],
        "post_quantum_identity": {
            "protocol": "NEWSATOSHI/2.1",
            "payout_love_address": charter.payout_love_address,
            "slh_dsa_pubkey_hex": charter.slh_dsa_pubkey_hex,
            "constitution_hash": charter.constitution_hash,
            "covenant_rule_hash": charter.covenant_rule_hash,
            "formal_kernel_url": format!("{base_url}/formal/NewsatoshiCovenants.lean"),
            "revenue_split_bps": {
                "ops_bps": charter.ops_bps,
                "reinvest_bps": charter.reinvest_bps,
                "backing_bps": charter.backing_bps
            },
            "max_spend_per_call_love": charter.max_spend_per_call_love
        },
        "x402Support": true,
        "paywordSupport": true
    })
}

/// Renders the gateway-level `/.well-known/erc8004.json` card for Newsatoshi Commons itself.
pub fn render_gateway_erc8004(base_url: &str, covenant_rule_hash: &str) -> Value {
    json!({
        "type": "https://eips.ethereum.org/EIPS/eip-8004#registration-v1",
        "name": "Newsatoshi Commons (LOVE)",
        "description": "Verify-only post-quantum machine money gateway, Official MCP Registry server (org.newsatoshi.commons/newsatoshi-commons), 4-chain SHA-256 HTLC & Proof-of-Backing Vault coordinator, and Lean 4.36 verified STARK covenant engine.",
        "active": true,
        "x402Support": true,
        "paywordSupport": true,
        "services": [
            { "name": "MCP", "endpoint": format!("{base_url}/mcp"), "version": "2025-03-26" },
            { "name": "A2A", "endpoint": format!("{base_url}/.well-known/agent.json"), "version": "1.0" },
            { "name": "Whitepaper", "endpoint": format!("{base_url}/whitepaper"), "version": "NEWSATOSHI/2.1" },
            { "name": "Lean4Kernel", "endpoint": format!("{base_url}/formal/NewsatoshiCovenants.lean"), "version": "4.36.0" }
        ],
        "contracts": {
            "universal_evm_chains": [8453, 42161, 10, 137],
            "htlc_bridge": CANONICAL_EVM_HTLC_CONTRACT,
            "backing_vault": CANONICAL_BACKING_VAULT
        },
        "covenant_rule_hash": covenant_rule_hash
    })
}

// ---------------------------------------------------------------------------
// Rail 3: Open Web4 Paid Service Directory (402 PayWord + x402)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Web4Service {
    pub service_id: String,
    pub charter_id: String,
    pub name: String,
    pub endpoint_url: String,
    pub capabilities: Vec<String>,
    pub description: String,
    pub unit_price_love_base_units: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x402_usdc_micro: Option<u64>,
    pub payword_recipient_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evm_payto_address: Option<String>,
    pub registered_at_unix: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct RegisterServiceRequest {
    pub charter_id: String,
    pub name: String,
    pub endpoint_url: String,
    pub capabilities: Vec<String>,
    pub description: String,
    pub unit_price_love_base_units: u64,
    #[serde(default)]
    pub x402_usdc_micro: Option<u64>,
    pub payword_recipient_address: String,
    #[serde(default)]
    pub evm_payto_address: Option<String>,
}

pub fn validate_web4_service(req: &RegisterServiceRequest) -> Result<Web4Service, String> {
    let name = req.name.trim();
    if name.len() < 3 || name.len() > 80 {
        return Err("service name must be 3..=80 characters".to_string());
    }
    let url = req.endpoint_url.trim();
    if !url.starts_with("https://") || url.len() < 12 || url.len() > 256 || url.contains(' ') {
        return Err("endpoint_url must be a valid https:// URL (<= 256 chars)".to_string());
    }
    let desc = req.description.trim();
    if desc.len() < 10 || desc.len() > 300 {
        return Err("description must be 10..=300 characters".to_string());
    }
    if req.unit_price_love_base_units == 0 {
        return Err("unit_price_love_base_units must be > 0".to_string());
    }
    let caps: Vec<String> = req
        .capabilities
        .iter()
        .map(|c| c.trim().to_ascii_lowercase())
        .filter(|c| !c.is_empty() && c.len() <= 32)
        .take(8)
        .collect();
    if caps.is_empty() {
        return Err("at least one capability tag is required (e.g. inference, compute, audit)".to_string());
    }
    let recipient = validate_love_addr(&req.payword_recipient_address, "payword_recipient_address")?;
    let evm_payto = match req.evm_payto_address.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(a) => Some(validate_evm_addr(a, "evm_payto_address")?),
        None => None,
    };
    let cid = req.charter_id.trim();
    if cid.is_empty() || cid.len() > 80 {
        return Err("charter_id is required".to_string());
    }

    let id_hash = sha256_concat(&[b"NEWSATOSHI_WEB4_SERVICE_V1", cid.as_bytes(), url.as_bytes()]);
    Ok(Web4Service {
        service_id: format!("svc-{}", &hex::encode(id_hash)[..14]),
        charter_id: cid.to_string(),
        name: name.to_string(),
        endpoint_url: url.to_string(),
        capabilities: caps,
        description: desc.to_string(),
        unit_price_love_base_units: req.unit_price_love_base_units,
        x402_usdc_micro: req.x402_usdc_micro.filter(|&v| v > 0),
        payword_recipient_address: recipient,
        evm_payto_address: evm_payto,
        registered_at_unix: unix_now(),
    })
}

// ---------------------------------------------------------------------------
// Rail 4: Zero-Custody Trustless Work & Compute Bounty Board
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Web4Bounty {
    pub bounty_id: String,
    pub charter_id: String,
    pub title: String,
    pub description: String,
    pub spec_hash_hex: String,
    /// `"evm_htlc"` | `"l1_htlc"` | `"l1_freivalds_job"`.
    pub settlement_rail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evm_chain: Option<String>,
    pub reward_amount: String,
    pub reward_asset: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_hash_hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evm_swap_id: Option<String>,
    pub escrow_lock_command: String,
    pub status: String,
    pub created_at_unix: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct CreateBountyRequest {
    pub charter_id: String,
    pub title: String,
    pub description: String,
    #[serde(default = "default_bounty_rail")]
    pub settlement_rail: String,
    #[serde(default)]
    pub evm_chain: Option<String>,
    pub reward_amount: String,
    #[serde(default = "default_reward_asset")]
    pub reward_asset: String,
    #[serde(default)]
    pub payment_hash_hex: Option<String>,
    #[serde(default)]
    pub evm_swap_id: Option<String>,
    #[serde(default)]
    pub evm_recipient_address: Option<String>,
}

fn default_bounty_rail() -> String {
    "evm_htlc".to_string()
}

fn default_reward_asset() -> String {
    "USDC".to_string()
}

pub fn compile_web4_bounty(req: &CreateBountyRequest) -> Result<Web4Bounty, String> {
    let title = req.title.trim();
    if title.len() < 5 || title.len() > 120 {
        return Err("bounty title must be 5..=120 characters".to_string());
    }
    let desc = req.description.trim();
    if desc.len() < 10 || desc.len() > 600 {
        return Err("bounty description must be 10..=600 characters".to_string());
    }
    let reward = req.reward_amount.trim();
    if reward.is_empty()
        || reward.len() > 24
        || !reward.bytes().all(|b| b.is_ascii_digit() || b == b'.')
    {
        return Err("reward_amount must be a positive decimal string".to_string());
    }
    let asset = req.reward_asset.trim().to_ascii_uppercase();
    if asset.is_empty() || asset.len() > 12 {
        return Err("reward_asset must be 1..=12 chars (e.g. LOVE, USDC, ETH)".to_string());
    }
    let rail = req.settlement_rail.trim().to_ascii_lowercase();
    if !matches!(rail.as_str(), "evm_htlc" | "l1_htlc" | "l1_freivalds_job") {
        return Err("settlement_rail must be evm_htlc, l1_htlc, or l1_freivalds_job".to_string());
    }
    let cid = req.charter_id.trim();
    if cid.is_empty() || cid.len() > 80 {
        return Err("charter_id is required".to_string());
    }

    let phash = match req.payment_hash_hex.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(h) => Some(hex::encode(hash32_from_hex(h)?)),
        None => None,
    };
    let spec_hash_hex = hex::encode(sha256(format!("{cid}:{title}:{desc}:{reward}:{asset}").as_bytes()));

    let (evm_chain_out, escrow_lock_command) = match rail.as_str() {
        "l1_freivalds_job" => (
            None,
            format!("newsatoshi --net mainnet job post A.csv B.csv --pay {reward} --bond 1"),
        ),
        "l1_htlc" => {
            let h = phash.as_deref().unwrap_or("<64HEX_PAYMENT_HASH>");
            (
                None,
                format!("newsatoshi --net mainnet lock {reward} --hash {h} --to <WORKER_NSD1_ADDRESS> --blocks 144"),
            )
        }
        _ => {
            let (norm_chain, rpc_url) = chain_rpc(req.evm_chain.as_deref().unwrap_or("base"))?;
            let h = phash.as_deref().unwrap_or("<64HEX_PAYMENT_HASH>");
            let recip = req
                .evm_recipient_address
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("<WORKER_0x_ADDRESS>");
            let cmd = format!(
                "cast send {CANONICAL_EVM_HTLC_CONTRACT} \"lockEth(address,bytes32,uint64)\" {recip} 0x{h} <TIMELOCK_UNIX> --value <AMOUNT_WEI> --rpc-url {rpc_url}"
            );
            (Some(norm_chain.to_string()), cmd)
        }
    };

    let bounty_id = format!("bounty-{}", &spec_hash_hex[..14]);
    Ok(Web4Bounty {
        bounty_id,
        charter_id: cid.to_string(),
        title: title.to_string(),
        description: desc.to_string(),
        spec_hash_hex,
        settlement_rail: rail,
        evm_chain: evm_chain_out,
        reward_amount: reward.to_string(),
        reward_asset: asset,
        payment_hash_hex: phash,
        evm_swap_id: req
            .evm_swap_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string),
        escrow_lock_command,
        status: "open".to_string(),
        created_at_unix: unix_now(),
    })
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web4_charter_enforces_bps_sum_and_lean4_monotonic_child_bounds() {
        let base_req = SpawnCharterRequest {
            name: "Solar Compute Guild".into(),
            category: "compute_node".into(),
            mission: "Serve verifiable matrix compute and route 20% of revenue into permanent LOVE backing vaults.".into(),
            max_spend_per_call_love: 500,
            ops_bps: 6000,
            reinvest_bps: 2000,
            backing_bps: 2000,
            payout_love_address: "ns1bc03485c35c661a5f6685a52f19351a143b44994df2943c20285b2e9876f68fa10452da1".into(),
            evm_treasury_address: "0xdf18181bD7EBCF8Ac3e74B5e10708E5ceA67A04c".into(),
            primary_evm_chain: "base".into(),
            ..Default::default()
        };
        let parent = compile_web4_charter(&base_req, None, "https://commons.newsatoshi.org").unwrap();
        assert_eq!(parent.lineage_depth, 0);
        assert!(parent.cast_anchor_command.contains(CANONICAL_BACKING_VAULT));

        // Valid child: tighter max_spend (250 <= 500) and higher backing (2500 >= 2000)
        let mut good_child = base_req.clone();
        good_child.name = "Solar Compute Worker #1".into();
        good_child.parent_charter_id = Some(parent.charter_id.clone());
        good_child.max_spend_per_call_love = 250;
        good_child.ops_bps = 5500;
        good_child.reinvest_bps = 2000;
        good_child.backing_bps = 2500;
        let child = compile_web4_charter(&good_child, Some(&parent), "https://commons.newsatoshi.org").unwrap();
        assert_eq!(child.lineage_depth, 1);
        assert_eq!(child.constitution_hash, parent.constitution_hash);

        // Evil child #1: tries to escalate spending cap (600 > 500) -> REJECTED!
        let mut bad_cap = good_child.clone();
        bad_cap.max_spend_per_call_love = 600;
        assert!(compile_web4_charter(&bad_cap, Some(&parent), "https://commons.newsatoshi.org").is_err());

        // Evil child #2: tries to weaken backing_bps (1000 < 2000) -> REJECTED!
        let mut bad_back = good_child.clone();
        bad_back.ops_bps = 7000;
        bad_back.backing_bps = 1000;
        assert!(compile_web4_charter(&bad_back, Some(&parent), "https://commons.newsatoshi.org").is_err());

        // Evil child #3: tries to mutate constitution_hash -> REJECTED!
        let mut bad_const = good_child;
        bad_const.constitution_hash = Some("11".repeat(32));
        assert!(compile_web4_charter(&bad_const, Some(&parent), "https://commons.newsatoshi.org").is_err());

        // ERC-8004 card binds both EVM backing vault and post-quantum covenant hash
        let card = render_erc8004_registration(&child, "https://commons.newsatoshi.org");
        assert_eq!(card["x402Support"], true);
        assert_eq!(card["post_quantum_identity"]["covenant_rule_hash"], child.covenant_rule_hash);
    }

    #[test]
    fn service_and_bounty_validation_enforces_https_and_rails() {
        let svc = validate_web4_service(&RegisterServiceRequest {
            charter_id: "charter-abc".into(),
            name: "STARK Batch Verifier".into(),
            endpoint_url: "https://commons.newsatoshi.org/api/actions/verify-stark".into(),
            capabilities: vec!["stark".into(), "verification".into()],
            description: "Sub-10ms Winterfell 0.13 STARK verification over HTTP 402 PayWords.".into(),
            unit_price_love_base_units: 1000,
            x402_usdc_micro: Some(1000),
            payword_recipient_address: "ns1bc03485c35c661a5f6685a52f19351a143b44994df2943c20285b2e9876f68fa10452da1".into(),
            evm_payto_address: Some("0xdf18181bD7EBCF8Ac3e74B5e10708E5ceA67A04c".into()),
        })
        .unwrap();
        assert!(svc.service_id.starts_with("svc-"));

        // Non-HTTPS URL rejected
        assert!(validate_web4_service(&RegisterServiceRequest {
            charter_id: "charter-abc".into(),
            name: "Bad URL".into(),
            endpoint_url: "http://insecure.example/api".into(),
            capabilities: vec!["test".into()],
            description: "Should reject plain HTTP endpoints.".into(),
            unit_price_love_base_units: 1000,
            payword_recipient_address: "ns1bc03485c35c661a5f6685a52f19351a143b44994df2943c20285b2e9876f68fa10452da1".into(),
            ..Default::default()
        })
        .is_err());

        let bounty = compile_web4_bounty(&CreateBountyRequest {
            charter_id: "charter-abc".into(),
            title: "Audit MCU STARK verifier memory usage".into(),
            description: "Benchmark Winterfell verifier on ESP32-S3 with <= 512 KiB SRAM.".into(),
            settlement_rail: "evm_htlc".into(),
            evm_chain: Some("base".into()),
            reward_amount: "250".into(),
            reward_asset: "USDC".into(),
            ..Default::default()
        })
        .unwrap();
        assert!(bounty.escrow_lock_command.contains(CANONICAL_EVM_HTLC_CONTRACT));
    }
}
