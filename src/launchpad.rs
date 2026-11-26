//! RFC-0027 Mission Launchpad & Decentralized AI Agent Swarm Coordination Compiler.
//!
//! Enables any human or AI agent to spin up a `pequalnp.money`-style decentralized agent swarm
//! + zero-rugpull bonding-curve launchpad in a single call, with exact integer math matching
//! Newsatoshi L1 (`node/src/mission.rs`) and Lean 4.36.0 Theorem 9
//! (`rfc0027_mission_launchpad_conservation`):
//!   - 0% founder extraction: `curve_fee_bps` on every buy & sell routes 100% into `bounty_pool`.
//!   - Automatic verifiable split: `contributor_bps + public_goods_bps == 10_000` (e.g. 80/20).
//!   - Anti-Sybil guard: optional `min_worker_bond` tied to L1 `RFC-0024` Trust Bonds.
//!   - Instant external agent onboarding for Claude Code, Cursor, Gemini CLI, and MCP clients.

use crate::crypto::sha256;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_CURVE_FEE_BPS: u16 = 1_000;
pub const INITIAL_SHARE_SCALE: u64 = 1_000;

#[derive(Clone, Debug, Deserialize)]
pub struct LaunchMissionRequest {
    pub name: String,
    pub ticker: String,
    pub mission_brief: String,
    /// e.g., `"lean4_zero_sorry"`, `"unit_tests"`, `"stark_proof"`, `"zktls_receipt"`, `"quorum_review"`.
    #[serde(default = "default_verifier_kind")]
    pub verifier_kind: String,
    #[serde(default = "default_contributor_bps")]
    pub contributor_bps: u16,
    #[serde(default = "default_public_goods_bps")]
    pub public_goods_bps: u16,
    #[serde(default = "default_public_goods_label")]
    pub public_goods_label: String,
    #[serde(default)]
    pub public_goods_address: String,
    #[serde(default = "default_curve_fee_bps")]
    pub curve_fee_bps: u16,
    #[serde(default = "default_initial_funding")]
    pub initial_funding_base_units: u64,
    #[serde(default)]
    pub min_worker_bond_base_units: u64,
    #[serde(default)]
    pub repo_url: String,
    #[serde(default = "default_roles")]
    pub swarm_roles: Vec<String>,
}

fn default_verifier_kind() -> String {
    "lean4_zero_sorry".into()
}
fn default_contributor_bps() -> u16 {
    8_000
}
fn default_public_goods_bps() -> u16 {
    2_000
}
fn default_public_goods_label() -> String {
    "Public Goods / Formal Verification Treasury".into()
}
fn default_curve_fee_bps() -> u16 {
    500
}
fn default_initial_funding() -> u64 {
    100_000_000
}
fn default_roles() -> Vec<String> {
    vec![
        "orchestrator".into(),
        "prover".into(),
        "red_team".into(),
        "checker_desk".into(),
    ]
}

#[derive(Clone, Debug, Serialize)]
pub struct CompiledMissionLaunchpad {
    pub rfc: &'static str,
    pub mission_hash_hex: String,
    pub name: String,
    pub ticker: String,
    pub mission_brief: String,
    pub verifier_kind: String,
    pub contributor_bps: u16,
    pub public_goods_bps: u16,
    pub public_goods_label: String,
    pub public_goods_address: String,
    pub founder_fee_bps: u16,
    pub curve_fee_bps: u16,
    pub initial_funding_base_units: u64,
    pub min_worker_bond_base_units: u64,
    pub repo_url: String,
    pub swarm_roles: Vec<String>,
    pub l1_deposit_spec: Value,
    pub external_agent_join_snippet: String,
    pub created_at_unix: u64,
}

/// Deterministic integer square root matching L1 `node/src/mission.rs::isqrt_u128` 1:1.
pub fn isqrt_u128(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = 1u128 << ((128 - n.leading_zeros()).div_ceil(2));
    loop {
        let y = (x + n / x) >> 1;
        if y >= x {
            return x;
        }
        x = y;
    }
}

/// Exact integer bonding-curve buy quote matching L1 `RFC-0027` (`node/src/mission.rs`).
pub fn quote_curve_buy(
    curve_reserve: u64,
    total_shares: u64,
    curve_fee_bps: u16,
    amount: u64,
) -> Result<Value, String> {
    if curve_fee_bps > MAX_CURVE_FEE_BPS {
        return Err("curve_fee_bps exceeds 1000 (10%)".into());
    }
    if amount == 0 {
        return Err("amount must be > 0".into());
    }
    let fee_to_bounty = ((amount as u128) * (curve_fee_bps as u128) / 10_000u128) as u64;
    let net_to_curve = amount - fee_to_bounty;
    if net_to_curve == 0 {
        return Err("net_to_curve must be > 0".into());
    }
    let new_reserve = curve_reserve
        .checked_add(net_to_curve)
        .ok_or("curve_reserve overflow")?;
    let target_shares = isqrt_u128((new_reserve as u128) * (INITIAL_SHARE_SCALE as u128)) as u64;
    let shares_minted = target_shares.saturating_sub(total_shares).max(1);
    let new_total_shares = total_shares
        .checked_add(shares_minted)
        .ok_or("total_shares overflow")?;
    Ok(json!({
        "ok": true,
        "rfc": "RFC-0027",
        "side": "buy",
        "amount_base_units": amount,
        "fee_to_bounty_pool_base_units": fee_to_bounty,
        "founder_extraction_base_units": 0,
        "net_to_curve_reserve_base_units": net_to_curve,
        "shares_minted": shares_minted,
        "new_curve_reserve_base_units": new_reserve,
        "new_total_shares": new_total_shares,
        "conservation_check": fee_to_bounty + net_to_curve == amount
    }))
}

/// Exact integer bonding-curve redemption quote matching L1 `RFC-0027` (`node/src/mission.rs`).
pub fn quote_curve_redeem(
    curve_reserve: u64,
    total_shares: u64,
    curve_fee_bps: u16,
    shares: u64,
    miner_fee: u64,
) -> Result<Value, String> {
    if curve_fee_bps > MAX_CURVE_FEE_BPS {
        return Err("curve_fee_bps exceeds 1000 (10%)".into());
    }
    if shares == 0 || total_shares == 0 || shares > total_shares {
        return Err("shares must be in 1..=total_shares".into());
    }
    let gross = ((curve_reserve as u128) * (shares as u128) / (total_shares as u128)) as u64;
    let fee_to_bounty = ((gross as u128) * (curve_fee_bps as u128) / 10_000u128) as u64;
    let after_bounty = gross - fee_to_bounty;
    if miner_fee > after_bounty {
        return Err("miner_fee exceeds redemption payout".into());
    }
    let user_payout = after_bounty - miner_fee;
    Ok(json!({
        "ok": true,
        "rfc": "RFC-0027",
        "side": "redeem",
        "shares_burned": shares,
        "gross_reserve_exit_base_units": gross,
        "fee_to_bounty_pool_base_units": fee_to_bounty,
        "founder_extraction_base_units": 0,
        "miner_fee_base_units": miner_fee,
        "user_shielded_payout_base_units": user_payout,
        "new_curve_reserve_base_units": curve_reserve - gross,
        "new_total_shares": total_shares - shares,
        "conservation_check": fee_to_bounty + user_payout + miner_fee == gross
    }))
}

/// Compile a deterministic RFC-0027 Mission Launchpad & Agent Swarm coordination charter.
pub fn compile_mission_launchpad(
    req: &LaunchMissionRequest,
    base_url: &str,
) -> Result<CompiledMissionLaunchpad, String> {
    let name = req.name.trim();
    let ticker = req.ticker.trim().to_ascii_uppercase();
    let brief = req.mission_brief.trim();
    if name.len() < 2 || name.len() > 80 {
        return Err("name must be 2..=80 characters".into());
    }
    if ticker.is_empty() || ticker.len() > 12 || !ticker.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err("ticker must be 1..=12 alphanumeric characters".into());
    }
    if brief.len() < 8 || brief.len() > 1200 {
        return Err("mission_brief must be 8..=1200 characters".into());
    }
    if req.contributor_bps.checked_add(req.public_goods_bps) != Some(10_000) {
        return Err("contributor_bps + public_goods_bps must equal 10000 (0% founder take)".into());
    }
    if req.curve_fee_bps > MAX_CURVE_FEE_BPS {
        return Err("curve_fee_bps exceeds 1000 (10%)".into());
    }
    if req.initial_funding_base_units == 0 {
        return Err("initial_funding_base_units must be > 0".into());
    }
    let verifier_kind = req.verifier_kind.trim().to_ascii_lowercase();
    if verifier_kind.is_empty() {
        return Err("verifier_kind is required".into());
    }
    let repo_url = req.repo_url.trim().to_string();
    let canon = format!(
        "nsMISSCHARTER1|{name}|{ticker}|{verifier_kind}|{}|{}|{}|{}|{repo_url}|{brief}",
        req.contributor_bps,
        req.public_goods_bps,
        req.curve_fee_bps,
        req.min_worker_bond_base_units
    );
    let mission_hash_hex = hex::encode(sha256(canon.as_bytes()));
    let pg_addr = if req.public_goods_address.trim().is_empty() {
        "<PUBLIC_GOODS_NS1_OR_NSD1_ADDRESS>".to_string()
    } else {
        req.public_goods_address.trim().to_string()
    };
    let roles = if req.swarm_roles.is_empty() {
        default_roles()
    } else {
        req.swarm_roles.clone()
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let join_snippet = format!(
        "Connect any AI agent (Claude Code / Cursor / Gemini CLI / OpenClaw) to `{base_url}/mcp` -> call `newsatoshi_inspect_mission_launchpad` with mission hash `{mission_hash_hex}`, pull open `{verifier_kind}` tasks, verify locally, and submit proof digests for automatic {}/{} shielded LOVE payout (0% founder take).",
        req.contributor_bps / 100,
        req.public_goods_bps / 100
    );

    Ok(CompiledMissionLaunchpad {
        rfc: "RFC-0027",
        mission_hash_hex: mission_hash_hex.clone(),
        name: name.to_string(),
        ticker,
        mission_brief: brief.to_string(),
        verifier_kind,
        contributor_bps: req.contributor_bps,
        public_goods_bps: req.public_goods_bps,
        public_goods_label: req.public_goods_label.trim().to_string(),
        public_goods_address: pg_addr.clone(),
        founder_fee_bps: 0,
        curve_fee_bps: req.curve_fee_bps,
        initial_funding_base_units: req.initial_funding_base_units,
        min_worker_bond_base_units: req.min_worker_bond_base_units,
        repo_url,
        swarm_roles: roles,
        l1_deposit_spec: json!({
            "deposit_variant": "Deposit::MissionLaunch",
            "deposit_tag": 11,
            "mission_hash": mission_hash_hex,
            "public_goods_address": pg_addr,
            "public_goods_bps": req.public_goods_bps,
            "curve_fee_bps": req.curve_fee_bps,
            "initial_funding": req.initial_funding_base_units,
            "min_worker_bond": req.min_worker_bond_base_units,
            "verifier_quorum_m": 2,
            "rpc_inspect_path": "/api/mission?id=<MISSION_ID_HEX>"
        }),
        external_agent_join_snippet: join_snippet,
        created_at_unix: now,
    })
}

/// Deterministic L1 RFC-0027 `post_task_message` & `claim_task_message` digest helper.
pub fn format_task_digests(
    mission_id_hex: &str,
    task_id: u32,
    spec_hash_hex: &str,
    reward: u64,
    proof_hash_hex: Option<&str>,
    contributor_apk_hex: Option<&str>,
    miner_fee: u64,
) -> Result<Value, String> {
    let mid = hex::decode(mission_id_hex.trim()).map_err(|_| "bad mission_id_hex")?;
    let spec = hex::decode(spec_hash_hex.trim()).map_err(|_| "bad spec_hash_hex")?;
    if mid.len() != 32 || spec.len() != 32 {
        return Err("mission_id_hex and spec_hash_hex must be 32 bytes (64 hex chars)".into());
    }
    let mut pbuf = Vec::with_capacity(10 + 32 + 4 + 32 + 8);
    pbuf.extend_from_slice(b"nsMISSTASK");
    pbuf.extend_from_slice(&mid);
    pbuf.extend_from_slice(&task_id.to_le_bytes());
    pbuf.extend_from_slice(&spec);
    pbuf.extend_from_slice(&reward.to_le_bytes());
    let post_task_digest = hex::encode(sha256(&pbuf));

    let claim_task_digest = match (proof_hash_hex, contributor_apk_hex) {
        (Some(ph), Some(apk)) => {
            let pbytes = hex::decode(ph.trim()).map_err(|_| "bad proof_hash_hex")?;
            let abytes = hex::decode(apk.trim()).map_err(|_| "bad contributor_apk_hex")?;
            if pbytes.len() != 32 || abytes.len() != 32 {
                return Err("proof_hash_hex and contributor_apk_hex must be 32 bytes".into());
            }
            let mut cbuf = Vec::with_capacity(11 + 32 + 4 + 32 + 32 + 32 + 8);
            cbuf.extend_from_slice(b"nsMISSCLAIM");
            cbuf.extend_from_slice(&mid);
            cbuf.extend_from_slice(&task_id.to_le_bytes());
            cbuf.extend_from_slice(&spec);
            cbuf.extend_from_slice(&pbytes);
            cbuf.extend_from_slice(&abytes);
            cbuf.extend_from_slice(&miner_fee.to_le_bytes());
            Some(hex::encode(sha256(&cbuf)))
        }
        _ => None,
    };

    Ok(json!({
        "ok": true,
        "rfc": "RFC-0027",
        "mission_id_hex": mission_id_hex.trim().to_ascii_lowercase(),
        "task_id": task_id,
        "post_task_message_hex": post_task_digest,
        "claim_task_message_hex": claim_task_digest
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mission_launchpad_compiler_and_curve_quotes_preserve_exact_conservation() {
        let req = LaunchMissionRequest {
            name: "Open P vs NP Lean 4 Swarm".into(),
            ticker: "PVNP".into(),
            mission_brief: "Coordinated decentralized swarm proving complexity-theoretic lemmas in Lean 4 with 0 sorry.".into(),
            verifier_kind: "lean4_zero_sorry".into(),
            contributor_bps: 8_000,
            public_goods_bps: 2_000,
            public_goods_label: "Lean FRO".into(),
            public_goods_address: "ns1leanfro000000000000000000000000000000000000000000000000000000".into(),
            curve_fee_bps: 500,
            initial_funding_base_units: 200_000_000,
            min_worker_bond_base_units: 100_000_000,
            repo_url: "https://github.com/example/open-lean-swarm".into(),
            swarm_roles: vec!["orchestrator".into(), "prover".into(), "red_team".into(), "checker_desk".into()],
        };
        let compiled = compile_mission_launchpad(&req, "https://commons.newsatoshi.org").unwrap();
        assert_eq!(compiled.founder_fee_bps, 0);
        assert_eq!(compiled.mission_hash_hex.len(), 64);

        let buy = quote_curve_buy(0, 0, 500, 10_000_000).unwrap();
        assert_eq!(buy["fee_to_bounty_pool_base_units"], 500_000);
        assert_eq!(buy["net_to_curve_reserve_base_units"], 9_500_000);
        assert_eq!(buy["conservation_check"], true);

        let sh = buy["shares_minted"].as_u64().unwrap();
        let redeem = quote_curve_redeem(9_500_000, sh, 500, sh / 2, 5_000).unwrap();
        assert_eq!(redeem["conservation_check"], true);
        assert!(redeem["fee_to_bounty_pool_base_units"].as_u64().unwrap() > 0);
    }
}
