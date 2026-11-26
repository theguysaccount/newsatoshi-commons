//! Zero-Custody Bonded zkTLS P2P Fiat/USDC <-> LOVE Market-Maker Desk Compiler.
//!
//! Enables real market makers to quote borderless fiat/USDC <-> native shielded LOVE swaps
//! backed by L1 `RFC-0024` Universal Trust Bonds (slashable on default/equivocation) and
//! settled via real cryptographic `reclaim_v1`, `tlsnotary_v1`, or `newsatoshi_pq_notary_v1`
//! zkTLS receipts (`src/zktls.rs`) — with ZERO fake seeded offers or synthetic liquidity.

use crate::crypto::sha256;
use crate::zktls::{verify_zktls_proof, ZkTlsProofPayload};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SUPPORTED_FIAT_RAILS: [&str; 7] = [
    "wise",
    "revolut",
    "zelle",
    "venmo",
    "sepa_instant",
    "faster_payments",
    "base_usdc",
];

#[derive(Clone, Debug, Deserialize)]
pub struct BondedP2pOfferRequest {
    pub maker_label: String,
    pub maker_bond_id_hex: String,
    pub maker_love_address: String,
    /// `"sell_love"` (maker sells LOVE for fiat/USDC) or `"buy_love"` (maker buys LOVE with fiat/USDC).
    #[serde(default = "default_sell_love")]
    pub side: String,
    /// Payment rail: `"wise"`, `"revolut"`, `"zelle"`, `"venmo"`, `"sepa_instant"`, `"faster_payments"`, `"base_usdc"`.
    pub payment_rail: String,
    /// ISO currency code (e.g., `"USD"`, `"EUR"`, `"GBP"`, `"USDC"`).
    #[serde(default = "default_usd")]
    pub fiat_currency: String,
    /// Quoted unit price in fiat cents per 1 LOVE (integer >= 1).
    pub price_fiat_cents_per_love: u64,
    pub min_trade_love_base_units: u64,
    pub max_trade_love_base_units: u64,
    /// Minimum RFC-0024 Trust Bond collateral backing this desk (must be >= max_trade_love_base_units).
    pub declared_bond_stake_base_units: u64,
    /// Required zkTLS protocol (`"reclaim_v1"`, `"tlsnotary_v1"`, `"newsatoshi_pq_notary_v1"`).
    #[serde(default = "default_zktls_proto")]
    pub required_zktls_protocol: String,
    /// Expected HTTPS API hostname proven in the zkTLS receipt (e.g., `"api.wise.com"`, `"api.revolut.com"`).
    pub expected_tls_server_name: String,
}

fn default_sell_love() -> String {
    "sell_love".into()
}
fn default_usd() -> String {
    "USD".into()
}
fn default_zktls_proto() -> String {
    "tlsnotary_v1".into()
}

#[derive(Clone, Debug, Serialize)]
pub struct CompiledP2pOffer {
    pub offer_id_hex: String,
    pub maker_label: String,
    pub maker_bond_id_hex: String,
    pub maker_love_address: String,
    pub side: String,
    pub payment_rail: String,
    pub fiat_currency: String,
    pub price_fiat_cents_per_love: u64,
    pub min_trade_love_base_units: u64,
    pub max_trade_love_base_units: u64,
    pub declared_bond_stake_base_units: u64,
    pub collateral_coverage_bps: u64,
    pub required_zktls_protocol: String,
    pub expected_tls_server_name: String,
    pub settlement_flow: Value,
    pub created_at_unix: u64,
}

pub fn compile_bonded_p2p_offer(
    req: &BondedP2pOfferRequest,
    base_url: &str,
) -> Result<CompiledP2pOffer, String> {
    let label = req.maker_label.trim();
    if label.len() < 2 || label.len() > 64 {
        return Err("maker_label must be 2..=64 characters".into());
    }
    let bid = req.maker_bond_id_hex.trim().to_ascii_lowercase();
    if bid.len() != 64 || !bid.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("maker_bond_id_hex must be 64 hex characters".into());
    }
    let addr = req.maker_love_address.trim();
    if !(addr.starts_with("ns1") || addr.starts_with("nsd1")) || addr.len() < 16 {
        return Err("maker_love_address must be a valid ns1... or nsd1... shielded address".into());
    }
    let side = req.side.trim().to_ascii_lowercase();
    if side != "sell_love" && side != "buy_love" {
        return Err("side must be 'sell_love' or 'buy_love'".into());
    }
    let rail = req.payment_rail.trim().to_ascii_lowercase();
    if !SUPPORTED_FIAT_RAILS.contains(&rail.as_str()) {
        return Err(format!(
            "unsupported payment_rail '{rail}' (supported: {:?})",
            SUPPORTED_FIAT_RAILS
        ));
    }
    let currency = req.fiat_currency.trim().to_ascii_uppercase();
    if currency.len() < 3 || currency.len() > 6 {
        return Err("fiat_currency must be 3..=6 chars".into());
    }
    if req.price_fiat_cents_per_love == 0 {
        return Err("price_fiat_cents_per_love must be > 0".into());
    }
    if req.min_trade_love_base_units == 0
        || req.max_trade_love_base_units < req.min_trade_love_base_units
    {
        return Err("invalid min/max trade bounds".into());
    }
    if req.declared_bond_stake_base_units < req.max_trade_love_base_units {
        return Err(
            "declared_bond_stake_base_units must be >= max_trade_love_base_units (100%+ bond coverage required)"
                .into(),
        );
    }
    let proto = req.required_zktls_protocol.trim().to_ascii_lowercase();
    if !["reclaim_v1", "tlsnotary_v1", "newsatoshi_pq_notary_v1"].contains(&proto.as_str()) {
        return Err("required_zktls_protocol must be reclaim_v1, tlsnotary_v1, or newsatoshi_pq_notary_v1".into());
    }
    let server_name = req.expected_tls_server_name.trim().to_ascii_lowercase();
    if server_name.len() < 4 || !server_name.contains('.') {
        return Err("expected_tls_server_name must be a valid HTTPS hostname".into());
    }

    let coverage_bps = ((req.declared_bond_stake_base_units as u128 * 10_000)
        / (req.max_trade_love_base_units as u128)) as u64;
    let canon = format!(
        "nsP2POFFER1|{label}|{bid}|{addr}|{side}|{rail}|{currency}|{}|{}|{}|{}|{proto}|{server_name}",
        req.price_fiat_cents_per_love,
        req.min_trade_love_base_units,
        req.max_trade_love_base_units,
        req.declared_bond_stake_base_units
    );
    let offer_id_hex = hex::encode(sha256(canon.as_bytes()));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    Ok(CompiledP2pOffer {
        offer_id_hex: offer_id_hex.clone(),
        maker_label: label.into(),
        maker_bond_id_hex: bid.clone(),
        maker_love_address: addr.into(),
        side,
        payment_rail: rail,
        fiat_currency: currency,
        price_fiat_cents_per_love: req.price_fiat_cents_per_love,
        min_trade_love_base_units: req.min_trade_love_base_units,
        max_trade_love_base_units: req.max_trade_love_base_units,
        declared_bond_stake_base_units: req.declared_bond_stake_base_units,
        collateral_coverage_bps: coverage_bps,
        required_zktls_protocol: proto.clone(),
        expected_tls_server_name: server_name.clone(),
        settlement_flow: json!({
            "step_1_verify_maker_bond": format!("Call MCP newsatoshi_inspect_trust_bond({bid}) and verify active stake >= max_trade and slash_count == 0"),
            "step_2_lock_escrow": "Maker commits shielded LOVE lock/escrow tied to offer_id_hex + taker address",
            "step_3_prove_fiat_transfer": format!("Taker executes payment and generates a cryptographic {proto} zkTLS presentation for '{server_name}'"),
            "step_4_verify_and_attest": format!("POST {base_url}/api/p2p/verify-settlement with offer_id_hex + zkTLS presentation -> yields canonical RFC-0024 BondAttest receipt_hash_hex (or triggers instant BondSlashEquivocation / BondSlashQuorum on default)")
        }),
        created_at_unix: now,
    })
}

/// Verify a P2P order settlement by cryptographically verifying its attached zkTLS proof
/// and binding `(offer_id_hex, maker_bond_id_hex, l1_receipt_hash_hex)` into an atomic settlement digest.
pub fn verify_p2p_settlement_with_zktls(
    offer_id_hex: &str,
    maker_bond_id_hex: &str,
    settled_love_base_units: u64,
    zktls_proof: &ZkTlsProofPayload,
) -> Result<Value, String> {
    let oid = offer_id_hex.trim().to_ascii_lowercase();
    let bid = maker_bond_id_hex.trim().to_ascii_lowercase();
    if oid.len() != 64 || !oid.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("offer_id_hex must be 64 hex characters".into());
    }
    if bid.len() != 64 || !bid.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("maker_bond_id_hex must be 64 hex characters".into());
    }
    if settled_love_base_units == 0 {
        return Err("settled_love_base_units must be > 0".into());
    }

    let zktls_report = verify_zktls_proof(zktls_proof)?;
    let receipt_hash = zktls_report["l1_receipt_hash_hex"]
        .as_str()
        .ok_or("missing l1_receipt_hash_hex from zkTLS verifier")?;
    let settlement_digest = hex::encode(sha256(
        format!("nsP2PSETTLE1|{oid}|{bid}|{settled_love_base_units}|{receipt_hash}").as_bytes(),
    ));
    Ok(json!({
        "ok": true,
        "verified": true,
        "offer_id_hex": oid,
        "maker_bond_id_hex": bid,
        "settled_love_base_units": settled_love_base_units,
        "l1_bond_attest_receipt_hash_hex": receipt_hash,
        "p2p_settlement_digest_hex": settlement_digest,
        "zktls_verification": zktls_report
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bonded_p2p_offer_enforces_full_collateral_coverage() {
        let mut req = BondedP2pOfferRequest {
            maker_label: "EU-SEPA-Desk-01".into(),
            maker_bond_id_hex: "aa".repeat(32),
            maker_love_address: "nsd1maker000000000000000000000000000000000000000000000000000000".into(),
            side: "sell_love".into(),
            payment_rail: "wise".into(),
            fiat_currency: "USD".into(),
            price_fiat_cents_per_love: 50,
            min_trade_love_base_units: 100_000_000,
            max_trade_love_base_units: 1_000_000_000,
            declared_bond_stake_base_units: 500_000_000, // undercollateralized!
            required_zktls_protocol: "tlsnotary_v1".into(),
            expected_tls_server_name: "api.wise.com".into(),
        };
        assert!(compile_bonded_p2p_offer(&req, "https://commons.newsatoshi.org").is_err());

        req.declared_bond_stake_base_units = 1_500_000_000; // 150% bond coverage
        let compiled = compile_bonded_p2p_offer(&req, "https://commons.newsatoshi.org").unwrap();
        assert_eq!(compiled.collateral_coverage_bps, 15_000);
        assert_eq!(compiled.offer_id_hex.len(), 64);
    }
}
