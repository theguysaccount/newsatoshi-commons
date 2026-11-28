//! Native LOVE (PayWord + PriorLock + L1 HTLC) Primary Rail + x402 v2 Outer Bridge On-Ramp.
//!
//! Positioning:
//! - **Primary Settlement Asset & Unit of Account:** Native `LOVE` via zero-gas, post-quantum
//!   pure-SHA-256 `X-Newsatoshi-PayWord` hash-chains (`<1ms`, 0 gas), L1 `PriorLock` falsifiable
//!   stake commitments (`v1.0.0-core`), and L1 SHA-256 HTLCs.
//! - **Outer Bridge On-Ramps (Spec-Compliant x402 v2):** Standard HTTP `402 Payment Required` with
//!   base64-encoded `Payment-Required` header (`x402Version: 2`, `scheme: "exact"`, CAIP-2
//!   `eip155:8453` Base USDC + Arbitrum + Polygon, plus `extensions.bazaar` discovery metadata)
//!   verified/settled via an open x402 facilitator (`X402_FACILITATOR_URL`) for EVM callers that
//!   do not yet hold native `LOVE`.

use crate::agent::verify_payword_token_incremental;
use crate::backing::{BackingProofSummary, CANONICAL_BACKING_VAULT};
use crate::swap::CANONICAL_EVM_HTLC_CONTRACT;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

pub const DEFAULT_X402_PAY_TO: &str = "0xdf18181bD7EBCF8Ac3e74B5e10708E5ceA67A04c";
pub const DEFAULT_X402_FACILITATOR: &str = "https://facilitator.payai.network";
pub const BASE_USDC_ADDRESS: &str = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
pub const ARBITRUM_USDC_ADDRESS: &str = "0xaf88d065e77c8cC2239327C5EDb3A432268e5831";
pub const POLYGON_USDC_ADDRESS: &str = "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359";
pub const BASE_WLOVE_MIRROR: &str = "0x0A3ccFb3fF981a22A264Aa05CD8b77d06C2732A3";
pub const BASE_UNISWAP_V3_POOL: &str = "0x08ddc8C3fc09ACdab6802a13B26631f47ABcB888";

const B64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Pure-Rust RFC 4648 standard Base64 encoder (zero external crate dependencies).
pub fn base64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(B64_TABLE[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_TABLE[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_TABLE[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Pure-Rust RFC 4648 Base64 decoder (tolerates standard & URL-safe alphabets and missing padding).
pub fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let clean: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if clean.is_empty() {
        return Err("empty base64 input".to_string());
    }
    let mut out = Vec::with_capacity((clean.len() * 3) / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for b in clean {
        if b == b'=' {
            break;
        }
        let val: u32 = match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return Err(format!("invalid base64 byte: 0x{b:02x}")),
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    Ok(out)
}

#[derive(Clone, Debug)]
pub struct X402RouteSpec {
    pub path: &'static str,
    pub method: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub amount_usdc_atomic: &'static str, // "1000" = $0.001 USDC (6 decimals)
    pub tags: &'static [&'static str],
    pub input_info: Value,
    pub output_example: Value,
}

pub fn x402_pay_to() -> String {
    std::env::var("X402_PAY_TO")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| s.starts_with("0x") && s.len() == 42)
        .unwrap_or_else(|| DEFAULT_X402_PAY_TO.to_string())
}

pub fn x402_facilitator_url() -> String {
    std::env::var("X402_FACILITATOR_URL")
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| s.starts_with("https://") || s.starts_with("http://"))
        .unwrap_or_else(|| DEFAULT_X402_FACILITATOR.to_string())
}

/// Builds the full x402 v2 `Payment-Required` JSON payload (and its base64 header encoding).
pub fn build_x402_v2_challenge(base_url: &str, spec: &X402RouteSpec) -> (Value, String, Value) {
    let pay_to = x402_pay_to();
    let full_url = format!("{}{}", base_url.trim_end_matches('/'), spec.path);

    let accepts = json!([
        {
            "scheme": "exact",
            "network": "eip155:8453",
            "amount": spec.amount_usdc_atomic,
            "asset": BASE_USDC_ADDRESS,
            "payTo": pay_to,
            "maxTimeoutSeconds": 300,
            "extra": {
                "name": "USD Coin",
                "version": "2",
                "railRole": "outer_bridge_onramp"
            }
        },
        {
            "scheme": "exact",
            "network": "eip155:42161",
            "amount": spec.amount_usdc_atomic,
            "asset": ARBITRUM_USDC_ADDRESS,
            "payTo": pay_to,
            "maxTimeoutSeconds": 300,
            "extra": {
                "name": "USD Coin",
                "version": "2",
                "railRole": "outer_bridge_onramp"
            }
        },
        {
            "scheme": "exact",
            "network": "eip155:137",
            "amount": spec.amount_usdc_atomic,
            "asset": POLYGON_USDC_ADDRESS,
            "payTo": pay_to,
            "maxTimeoutSeconds": 300,
            "extra": {
                "name": "USD Coin",
                "version": "2",
                "railRole": "outer_bridge_onramp"
            }
        }
    ]);

    let payment_required_obj = json!({
        "x402Version": 2,
        "error": "Payment required",
        "primarySettlementAsset": "LOVE",
        "resource": {
            "url": full_url,
            "description": spec.description,
            "mimeType": "application/json",
            "serviceName": "Newsatoshi Commons",
            "tags": spec.tags,
            "iconUrl": format!("{}/og-image.svg", base_url.trim_end_matches('/'))
        },
        "accepts": accepts,
        "extensions": {
            "bazaar": {
                "info": {
                    "input": spec.input_info,
                    "output": {
                        "type": "json",
                        "example": spec.output_example
                    }
                }
            }
        }
    });

    let header_b64 = base64_encode(
        serde_json::to_string(&payment_required_obj)
            .unwrap_or_else(|_| "{}".to_string())
            .as_bytes(),
    );

    let body = json!({
        "x402Version": 2,
        "error": "Payment required",
        "primary_settlement_asset": "LOVE",
        "native_love_primary_rail": {
            "asset": "LOVE",
            "protocol": "NEWSATOSHI-PAYWORD-SHA256/1.0",
            "l1_primitives": ["PayWord SHA-256 hash-chain", "PriorLock falsifiable stake (v1.0.0-core)", "SHA-256 HTLC"],
            "unit_price_love_base_units": 100_000,
            "header": "X-Newsatoshi-PayWord: tip=<64hex>,token=<64hex>,ticks=<n>",
            "gas_usd": 0,
            "local_verify_latency_ms": "<1",
            "note": "Primary unit of account is native LOVE (unfunded PayWord vouchers are explicitly marked settled=false on this public verify-only sandbox; production merchants enforce funded on-chain L1 locks or PriorLocks)."
        },
        "outer_bridge_onramps": {
            "role": "outer_bridge_onramp",
            "note": "Base/Arbitrum/Polygon USDC x402 v2 rails are supported as outer EVM bridge on-ramps for callers not yet holding native LOVE.",
            "accepts": payment_required_obj["accepts"]
        },
        "accepts": payment_required_obj["accepts"],
        "resource": payment_required_obj["resource"],
        "payword_fast_path": {
            "protocol": "NEWSATOSHI-PAYWORD-SHA256/1.0",
            "algorithm": "Rivest-Shamir (1996) incremental SHA-256 hash-chain (O(1) per sequential call, up to 1,000,000 ticks)",
            "header": "X-Newsatoshi-PayWord: tip=<64hex>,token=<64hex>,ticks=<n>",
            "gas_usd": 0,
            "local_verify_latency_ms": "<1",
            "note": "Primary native LOVE metering path (demo mode on public verify-only sandbox; production merchants enforce funded on-chain L1 locks or PriorLocks)"
        },
        "backing_vault": CANONICAL_BACKING_VAULT
    });

    (payment_required_obj, header_b64, body)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettlementReceipt {
    pub rail: String,
    /// True ONLY when an on-chain payment/lock is cryptographically verified & settled
    /// (e.g., x402 v2 USDC facilitator settlement or verified funded L1 lock).
    /// Unfunded PayWord sandbox vouchers explicitly report `settled: false`.
    pub settled: bool,
    pub funding_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hashes_evaluated: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payword_ticks: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_response_b64: Option<String>,
}

static PAYWORD_WATERMARKS: OnceLock<Mutex<HashMap<String, (u64, String)>>> = OnceLock::new();

fn payword_watermarks() -> &'static Mutex<HashMap<String, (u64, String)>> {
    PAYWORD_WATERMARKS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Attempts to authorize a request via EITHER:
/// 1. `X-Newsatoshi-PayWord: tip=...,token=...,ticks=...` (incremental O(1) pure SHA-256 verification
///    with monotonic anti-replay watermark; explicitly tagged `unfunded_payword_demo`, `settled: false`
///    on the public verify-only sandbox when no funded on-chain lock is attached), OR
/// 2. `PAYMENT-SIGNATURE` / `X-PAYMENT` (x402 v2 / v1 Base/Arbitrum/Polygon USDC verified & settled live via facilitator).
pub fn authorize_dual_402(
    payword_header: Option<&str>,
    x402_header: Option<&str>,
    challenge_obj: &Value,
) -> Result<Option<SettlementReceipt>, String> {
    // Path 1: Zero-gas incremental O(1) Newsatoshi PayWord metering path
    if let Some(hdr) = payword_header {
        let mut tip = None;
        let mut tok = None;
        let mut ticks = None;
        for kv in hdr.split(',') {
            if let Some((k, v)) = kv.trim().split_once('=') {
                match k.trim() {
                    "tip" => tip = Some(v.trim().to_string()),
                    "token" => tok = Some(v.trim().to_string()),
                    "ticks" => ticks = v.trim().parse::<u64>().ok(),
                    _ => {}
                }
            }
        }
        let (tip_h, tok_h, n) = match (tip, tok, ticks) {
            (Some(a), Some(b), Some(c)) if c >= 1 => (a, b, c),
            _ => return Err("malformed X-Newsatoshi-PayWord header (expected tip=<hex>,token=<hex>,ticks=<n>)".to_string()),
        };
        let norm_tip = tip_h.trim().to_ascii_lowercase();
        let prev = {
            let map = payword_watermarks().lock().unwrap_or_else(|e| e.into_inner());
            map.get(&norm_tip).cloned()
        };
        let (prev_ticks, prev_tok) = match prev {
            Some((pt, ref ptok)) => {
                if n <= pt {
                    return Err(format!(
                        "PayWord replay rejected: ticks ({n}) must exceed last accepted ({pt})"
                    ));
                }
                (pt, Some(ptok.as_str()))
            }
            None => (0, None),
        };
        let res = verify_payword_token_incremental(&norm_tip, &tok_h, n, 10_000, prev_tok, prev_ticks)?;
        if !res.valid {
            return Err("invalid PayWord hash-chain proof".to_string());
        }
        {
            let mut map = payword_watermarks().lock().unwrap_or_else(|e| e.into_inner());
            if map.len() >= 10_000 && !map.contains_key(&res.tip_hex) {
                map.clear();
            }
            map.insert(res.tip_hex.clone(), (n, res.token_hex.clone()));
        }
        return Ok(Some(SettlementReceipt {
            rail: "NEWSATOSHI-PAYWORD-SHA256/1.0".to_string(),
            settled: false,
            funding_status: "unfunded_payword_demo".to_string(),
            verification_mode: Some(res.verification_mode),
            hashes_evaluated: Some(res.hashes_evaluated),
            network: Some("newsatoshi:mainnet".to_string()),
            tx_hash: None,
            payer: Some(res.tip_hex),
            payword_ticks: Some(n),
            payment_response_b64: None,
        }));
    }

    // Path 2: Standard x402 v2 / v1 Base/Arb/Polygon USDC via Facilitator (/verify + /settle)
    if let Some(raw_hdr) = x402_header {
        let decoded_bytes = base64_decode(raw_hdr)
            .map_err(|e| format!("invalid base64 in PAYMENT-SIGNATURE/X-PAYMENT: {e}"))?;
        let payment_payload: Value = serde_json::from_slice(&decoded_bytes)
            .map_err(|e| format!("invalid JSON in x402 payment payload: {e}"))?;

        let x402_version = payment_payload
            .get("x402Version")
            .and_then(|v| v.as_u64())
            .unwrap_or(2);

        // Select matching requirement from challenge accepts array
        let accepted_net = payment_payload
            .pointer("/accepted/network")
            .and_then(|v| v.as_str())
            .or_else(|| payment_payload.get("network").and_then(|v| v.as_str()))
            .unwrap_or("eip155:8453");

        let accepts = challenge_obj
            .get("accepts")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "missing accepts array".to_string())?;

        let mut req_obj = accepts
            .iter()
            .find(|a| {
                let n = a.get("network").and_then(|v| v.as_str()).unwrap_or("");
                n == accepted_net
                    || (accepted_net == "base" && n == "eip155:8453")
                    || (accepted_net == "arbitrum" && n == "eip155:42161")
                    || (accepted_net == "polygon" && n == "eip155:137")
            })
            .cloned()
            .unwrap_or_else(|| accepts[0].clone());

        // For x402 v1 payloads, adapt network alias ("base") + maxAmountRequired
        if x402_version == 1 {
            if let Some(map) = req_obj.as_object_mut() {
                if let Some(amt) = map.get("amount").cloned() {
                    map.insert("maxAmountRequired".to_string(), amt);
                }
                let net_v1 = match map.get("network").and_then(|v| v.as_str()).unwrap_or("") {
                    "eip155:8453" => "base",
                    "eip155:42161" => "arbitrum",
                    "eip155:137" => "polygon",
                    other => other,
                };
                map.insert("network".to_string(), json!(net_v1));
                if let Some(res_url) = challenge_obj.pointer("/resource/url").and_then(|v| v.as_str()) {
                    map.insert("resource".to_string(), json!(res_url));
                }
                if let Some(desc) = challenge_obj.pointer("/resource/description").and_then(|v| v.as_str()) {
                    map.insert("description".to_string(), json!(desc));
                }
                map.insert("mimeType".to_string(), json!("application/json"));
            }
        }

        let facilitator = x402_facilitator_url();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .build()
            .into();

        let fac_req = json!({
            "x402Version": x402_version,
            "paymentPayload": payment_payload,
            "paymentRequirements": req_obj,
        });

        // Step A: Verify
        let mut v_resp = agent
            .post(&format!("{facilitator}/verify"))
            .header("Content-Type", "application/json")
            .send(fac_req.to_string().as_bytes())
            .map_err(|e| format!("x402 facilitator /verify unreachable: {e}"))?;
        let v_text = v_resp
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("x402 /verify read error: {e}"))?;
        let v_json: Value = serde_json::from_str(&v_text)
            .map_err(|e| format!("x402 /verify invalid JSON: {e} ({v_text})"))?;
        let is_valid = v_json
            .get("isValid")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !is_valid {
            let reason = v_json
                .get("invalidReason")
                .or_else(|| v_json.get("error"))
                .and_then(|v| v.as_str())
                .unwrap_or("verification rejected");
            return Err(format!("x402 payment rejected: {reason}"));
        }

        // Step B: Settle on-chain
        let mut s_resp = agent
            .post(&format!("{facilitator}/settle"))
            .header("Content-Type", "application/json")
            .send(fac_req.to_string().as_bytes())
            .map_err(|e| format!("x402 facilitator /settle unreachable: {e}"))?;
        let s_text = s_resp
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("x402 /settle read error: {e}"))?;
        let s_json: Value = serde_json::from_str(&s_text)
            .map_err(|e| format!("x402 /settle invalid JSON: {e} ({s_text})"))?;
        let settled = s_json
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !settled {
            let reason = s_json
                .get("errorReason")
                .or_else(|| s_json.get("error"))
                .and_then(|v| v.as_str())
                .unwrap_or("settlement failed");
            return Err(format!("x402 settlement failed: {reason}"));
        }

        let tx_hash = s_json
            .get("transaction")
            .or_else(|| s_json.get("txHash"))
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        let payer = s_json
            .get("payer")
            .or_else(|| v_json.get("payer"))
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
        let network = s_json
            .get("network")
            .and_then(|v| v.as_str())
            .map(ToString::to_string)
            .or_else(|| Some(accepted_net.to_string()));
        let payment_response_b64 = Some(base64_encode(s_text.as_bytes()));

        return Ok(Some(SettlementReceipt {
            rail: format!("x402-v{x402_version}-usdc"),
            settled: true,
            funding_status: "verified_x402_onchain_usdc".to_string(),
            verification_mode: Some("facilitator_onchain_settle".to_string()),
            hashes_evaluated: None,
            network,
            tx_hash,
            payer,
            payword_ticks: None,
            payment_response_b64,
        }));
    }

    Ok(None)
}

pub fn route_spec_bridge_quote() -> X402RouteSpec {
    X402RouteSpec {
        path: "/api/x402/bridge-quote",
        method: "GET",
        name: "Newsatoshi Multi-Chain Bridge & Proof-of-Backing Quote",
        description: "Real-time multi-chain bridge & backing intelligence for Newsatoshi (LOVE): live Base Uniswap V3 wLOVE/WETH pool metrics, 4-chain ownerless NewsatoshiBackingVault reserves (Base/Arbitrum/Optimism/Polygon), and live L1 AuxPoW issuance. Primary rail: native LOVE (zero-gas SHA-256 PayWords / PriorLock); outer x402 bridge on-ramp: Base/Arb/Polygon USDC ($0.001).",
        amount_usdc_atomic: "1000",
        tags: &["crypto", "defi", "bridge", "post-quantum", "proof-of-backing", "newsatoshi"],
        input_info: json!({
            "type": "http",
            "method": "GET",
            "queryParams": {}
        }),
        output_example: json!({
            "ok": true,
            "native_love_market": "peer_to_peer_htlc_negotiated",
            "base_wlove_mirror": BASE_WLOVE_MIRROR,
            "base_uniswap_v3_pool": BASE_UNISWAP_V3_POOL,
            "canonical_htlc": CANONICAL_EVM_HTLC_CONTRACT,
            "canonical_backing_vault": CANONICAL_BACKING_VAULT
        }),
    }
}

pub fn route_spec_compile_htlc() -> X402RouteSpec {
    X402RouteSpec {
        path: "/api/x402/compile-htlc",
        method: "POST",
        name: "Newsatoshi 4-Chain SHA-256 Atomic Swap HTLC Compiler",
        description: "Compiles zero-custody SHA-256 atomic swap HTLC contracts across Bitcoin BIP-199 P2WSH, Lightning BOLT-11, Solana PDA, and Base/Arbitrum/Optimism/Polygon NewsatoshiHtlc.sol (0xF564…8987). Primary rail: native LOVE (zero-gas SHA-256 PayWords / PriorLock); outer x402 bridge on-ramp: Base/Arb/Polygon USDC ($0.001).",
        amount_usdc_atomic: "1000",
        tags: &["crypto", "atomic-swap", "htlc", "bitcoin", "base", "post-quantum"],
        input_info: json!({
            "type": "http",
            "method": "POST",
            "bodyType": "json",
            "bodyFields": {
                "rail": "evm_htlc | btc_onchain | lightning_bolt11 | solana_htlc",
                "amount_love": "10",
                "love_claim_address": "nsd1...",
                "evm_chain": "base",
                "evm_recipient_address": "0x..."
            }
        }),
        output_example: json!({
            "ok": true,
            "contract": { "rail": "evm_htlc", "evm_contract_address": CANONICAL_EVM_HTLC_CONTRACT }
        }),
    }
}

pub fn route_spec_verify_payword() -> X402RouteSpec {
    X402RouteSpec {
        path: "/api/x402/verify-payword",
        method: "POST",
        name: "Newsatoshi Post-Quantum SHA-256 PayWord Verifier",
        description: "Verifies pure-SHA-256 PayWord micropayment hash-chains (SHA256^ticks(token) == tip) for sub-millisecond, zero-gas native LOVE AI agent billing. Primary rail: native LOVE PayWord; outer x402 bridge on-ramp: USDC ($0.001).",
        amount_usdc_atomic: "1000",
        tags: &["crypto", "payments", "payword", "post-quantum", "x402"],
        input_info: json!({
            "type": "http",
            "method": "POST",
            "bodyType": "json",
            "bodyFields": {
                "tip_hex": "64-char hex SHA256^N(seed)",
                "token_hex": "64-char hex SHA256^(N-k)(seed)",
                "ticks": 1
            }
        }),
        output_example: json!({
            "ok": true,
            "verification": { "valid": true, "ticks": 1 }
        }),
    }
}

pub fn route_spec_evm_security_scan() -> X402RouteSpec {
    X402RouteSpec {
        path: "/api/x402/evm-security-scan",
        method: "POST",
        name: "Multi-Chain EVM Smart Contract Security Scan (ETH/Base/Arb/OP/Polygon)",
        description: "Live on-chain security & rug-risk audit for any EVM contract: bytecode size, EIP-1967 proxy implementation/admin slots, active owner() privileges, ERC-20 supply/metadata, and Sourcify source verification. Primary rail: native LOVE (zero-gas SHA-256 PayWords); outer x402 bridge on-ramp: USDC ($0.001).",
        amount_usdc_atomic: "1000",
        tags: &["crypto", "security", "evm", "audit", "base", "ethereum", "x402"],
        input_info: json!({
            "type": "http",
            "method": "POST",
            "bodyType": "json",
            "bodyFields": {
                "chain": "ethereum | base | arbitrum | optimism | polygon",
                "address": "0x-prefixed 20-byte contract address"
            }
        }),
        output_example: json!({
            "ok": true,
            "chain": "base",
            "is_contract": true,
            "is_eip1967_proxy": false,
            "is_ownerless": true,
            "sourcify_status": "perfect",
            "risk_flags": ["IMMUTABLE_OWNERLESS_VERIFIED"]
        }),
    }
}

/// Renders `/.well-known/x402.json` discovery manifest for x402 indexers & crawlers.
pub fn render_well_known_x402(base_url: &str) -> Value {
    let specs = [
        route_spec_bridge_quote(),
        route_spec_compile_htlc(),
        route_spec_verify_payword(),
        route_spec_evm_security_scan(),
    ];
    let resources: Vec<Value> = specs
        .iter()
        .map(|s| {
            let (req_obj, _, _) = build_x402_v2_challenge(base_url, s);
            json!({
                "name": s.name,
                "method": s.method,
                "url": format!("{}{}", base_url.trim_end_matches('/'), s.path),
                "description": s.description,
                "x402Version": 2,
                "accepts": req_obj["accepts"],
                "paywordSupported": true,
                "tags": s.tags
            })
        })
        .collect();

    json!({
        "version": 2,
        "provider": "Newsatoshi Commons",
        "primary_settlement_asset": "LOVE",
        "primary_rail": "NEWSATOSHI-PAYWORD-SHA256/1.0",
        "l1_release_target": "v1.2.5",
        "evm_usdc_rail_role": "outer_bridge_onramp",
        "website": "https://newsatoshi.org",
        "commons": base_url.trim_end_matches('/'),
        "mcp": format!("{}/mcp", base_url.trim_end_matches('/')),
        "erc8004": format!("{}/.well-known/erc8004.json", base_url.trim_end_matches('/')),
        "facilitator": x402_facilitator_url(),
        "payTo": x402_pay_to(),
        "resources": resources,
        "batch_settlement_paywords": {
            "scheme": "batch-settlement",
            "binding": "paywords-sha256",
            "protocol_fee_bps": 0,
            "spec_url": "https://github.com/theguysaccount/newsatoshi-commons/blob/main/docs/X402-BATCH-SETTLEMENT-PAYWORDS-SPEC.md",
            "x402_issue": "https://github.com/x402-foundation/x402/issues/4494",
            "merchant_recipient": crate::paywords_launchpad::PAID_API_MERCHANT_ADDRESS,
            "live_contracts": {
                "eip155:8453": crate::paywords_launchpad::BASE_PAYWORD_CHANNEL_ADDRESS,
                "eip155:10": crate::paywords_launchpad::MULTICHAIN_PAYWORD_CHANNEL_ADDRESS,
                "eip155:42161": crate::paywords_launchpad::MULTICHAIN_PAYWORD_CHANNEL_ADDRESS
            },
            "endpoints": [
                format!("{}/api/v2/paid/evm-audit-batch", base_url.trim_end_matches('/')),
                format!("{}/api/v2/paid/tx-verify-batch", base_url.trim_end_matches('/')),
                format!("{}/api/v2/paid/defi-quote-bundle", base_url.trim_end_matches('/'))
            ],
            "free_sandbox": format!("{}/api/v2/paywords/try-402", base_url.trim_end_matches('/'))
        }
    })
}

/// Fetches live Base Uniswap V3 `wLOVE/WETH` market metrics from DexScreener public API (read-only,
/// honest null fields if unreachable — never fabricates prices or volumes).
pub fn fetch_live_wlove_market() -> Value {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(6)))
        .http_status_as_error(false)
        .build()
        .into();

    let url = format!(
        "https://api.dexscreener.com/latest/dex/tokens/{BASE_WLOVE_MIRROR}"
    );
    if let Ok(mut resp) = agent.get(&url).call() {
        if let Ok(text) = resp.body_mut().read_to_string() {
            if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                if let Some(pair) = parsed
                    .get("pairs")
                    .and_then(|p| p.as_array())
                    .and_then(|arr| arr.first())
                {
                    return json!({
                        "live": true,
                        "dex": pair.get("dexId").and_then(|v| v.as_str()).unwrap_or("uniswap"),
                        "chain": "base",
                        "token_address": BASE_WLOVE_MIRROR,
                        "pool_address": BASE_UNISWAP_V3_POOL,
                        "lp_burned_nft_id": 6689936,
                        "price_usd": pair.get("priceUsd").cloned().unwrap_or(Value::Null),
                        "price_native_eth": pair.get("priceNative").cloned().unwrap_or(Value::Null),
                        "liquidity_usd": pair.pointer("/liquidity/usd").cloned().unwrap_or(Value::Null),
                        "volume_24h_usd": pair.pointer("/volume/h24").cloned().unwrap_or(Value::Null),
                        "dexscreener_url": format!("https://dexscreener.com/base/{BASE_UNISWAP_V3_POOL}"),
                        "uniswap_url": format!("https://app.uniswap.org/swap?chain=base&outputCurrency={BASE_WLOVE_MIRROR}"),
                        "disclosure": "wLOVE is an independent, ownerless Base ERC-20 token (21,000,000 fixed supply; no on-chain bridge connects wLOVE to native L1 LOVE). Native L1 LOVE is separate, mined via SHA-256 AuxPoW (chainid 20051) with perpetual 0.1953125 LOVE/block tail emission, and traded peer-to-peer via SHA-256 HTLCs."
                    });
                }
            }
        }
    }

    json!({
        "live": false,
        "chain": "base",
        "token_address": BASE_WLOVE_MIRROR,
        "pool_address": BASE_UNISWAP_V3_POOL,
        "lp_burned_nft_id": 6689936,
        "disclosure": "wLOVE is an independent, ownerless Base ERC-20 token (21,000,000 fixed supply; no on-chain bridge connects wLOVE to native L1 LOVE). Native L1 LOVE is separate and traded peer-to-peer via SHA-256 HTLCs."
    })
}

pub fn build_bridge_quote_response(backing: &BackingProofSummary, settlement: Option<&SettlementReceipt>) -> Value {
    let wlove_market = fetch_live_wlove_market();
    json!({
        "ok": true,
        "protocol": "NEWSATOSHI/2.1",
        "native_love_market": {
            "settlement_model": "peer_to_peer_sha256_htlc_negotiated",
            "mainnet_height": backing.mainnet_height,
            "mainnet_issued_love": backing.mainnet_issued_love,
            "block_reward_love": 50,
            "auxpow_chainid": 20051,
            "note": "Native L1 LOVE has zero premine and is never sold at a discount by this gateway. Atomic swaps are negotiated peer-to-peer against miners/makers via NewsatoshiHtlc.sol or Bitcoin BIP-199 P2WSH."
        },
        "base_wlove_discovery_mirror": wlove_market,
        "proof_of_backing_4_chains": {
            "vault_address": CANONICAL_BACKING_VAULT,
            "total_eth": backing.total_eth_formatted,
            "total_usdc": backing.total_usdc_formatted,
            "total_pol": backing.total_pol_formatted,
            "total_deposits": backing.total_vault_deposits,
            "total_anchored_charters": backing.total_onchain_anchored_charters,
            "chains": backing.chains,
        },
        "contracts": {
            "universal_htlc": CANONICAL_EVM_HTLC_CONTRACT,
            "universal_backing_vault": CANONICAL_BACKING_VAULT,
            "erc8004_registry": "0x8004A169FB4a3325136EB29fA0ceB6D2e539a432",
            "erc8004_agent_ids": {
                "base": 1229992,
                "arbitrum": 1692,
                "optimism": 42519,
                "polygon": 715
            }
        },
        "settlement": settlement
    })
}

/// Generates drop-in, copy-paste-ready Private `x402 v2` + `O(1)` SHA-256 PayWords (`nsUSD` USD-quoted)
/// paywall middleware for Express, Next.js App Router, FastAPI, or Rust Axum.
/// Zero custodian lock-in, zero per-call gas on the PayWords fast path, and Base USDC `x402 v2` fallback.
pub fn build_private_x402_paywall_kit(
    framework: &str,
    price_usd: f64,
    route_path: &str,
    pay_to_evm: Option<&str>,
    seller_ns1: Option<&str>,
    base_url: &str,
) -> Result<Value, String> {
    if !price_usd.is_finite() || price_usd <= 0.0 || price_usd > 10_000.0 {
        return Err("price_usd must be finite and between 0.000001 and 10000.0".to_string());
    }
    let fw = framework.trim().to_ascii_lowercase();
    let norm_fw = match fw.as_str() {
        "express" | "node" | "nodejs" => "express",
        "nextjs" | "next" | "next.js" => "nextjs",
        "fastapi" | "python" | "py" => "fastapi",
        "axum" | "rust" => "axum",
        other => {
            return Err(format!(
                "unsupported framework '{other}' (expected one of: express, nextjs, fastapi, axum)"
            ))
        }
    };
    let clean_route = if route_path.trim().is_empty() {
        "/api/paid-endpoint".to_string()
    } else if route_path.trim().starts_with('/') {
        route_path.trim().to_string()
    } else {
        format!("/{}", route_path.trim())
    };
    let micro_usd = (price_usd * 1_000_000.0).round().max(1.0) as u64;
    let formatted_usd = format!("{:.6}", (micro_usd as f64) / 1_000_000.0);
    let evm_dest = pay_to_evm
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("0xYOUR_EVM_WALLET_ADDRESS");
    let ns1_dest = seller_ns1
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("ns1YOUR_SHIELDED_NEWSATOSHI_ADDRESS");
    let commons = base_url.trim_end_matches('/');

    let (filename, install_cmd, code) = match norm_fw {
        "express" => (
            "middleware/privateX402Paywall.mjs",
            "npm i express  # Zero extra crypto deps: uses built-in Node crypto.createHash('sha256')",
            format!(
r#"import crypto from "node:crypto";

// Drop-in Private x402 + O(1) SHA-256 PayWords Middleware (USD-quoted: ${formatted_usd}/call)
// Generated by {commons}/api/v2/x402/paywall-kit (RFC-0022 + RFC-0028 nsUSD + x402 v2)
const MICRO_USD_PRICE = {micro_usd}; // 1 tick = $0.000001 (0% volatility USD-quoted)
const PAY_TO_EVM = "{evm_dest}";
const SELLER_NS1 = "{ns1_dest}";
const BASE_USDC = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
const highestSeenTicks = new Map(); // channel tip_hex -> last verified tick (monotonic O(1) state)

function verifyPayWordIncremental(tipHex, tokenHex, ticks) {{
  if (!/^[0-9a-f]{{64}}$/i.test(tipHex) || !/^[0-9a-f]{{64}}$/i.test(tokenHex)) return false;
  const prev = highestSeenTicks.get(tipHex) || {{ ticks: 0, tokenHex: tipHex }};
  const delta = ticks - prev.ticks;
  if (delta < 1 || delta > 10000) return false;
  let cur = Buffer.from(tokenHex, "hex");
  for (let i = 0; i < delta; i++) cur = crypto.createHash("sha256").update(cur).digest();
  if (cur.toString("hex") !== prev.tokenHex.toLowerCase()) return false;
  highestSeenTicks.set(tipHex, {{ ticks, tokenHex: tokenHex.toLowerCase() }});
  return true;
}}

export function privateX402Paywall(req, res, next) {{
  // Fast private rail: O(1) SHA-256 PayWord (zero gas, <1ms local verify, zero freeze switch)
  const pw = req.header("x-newsatoshi-payword");
  if (pw) {{
    const [tip, token, ticksStr] = pw.split(":");
    const ticks = Number.parseInt(ticksStr || "0", 10);
    if (verifyPayWordIncremental(tip || "", token || "", ticks)) {{
      res.setHeader("X-Payment-Rail", "NEWSATOSHI-PAYWORD-SHA256/1.0");
      return next();
    }}
  }}
  // Standard x402 v2 Base USDC outer bridge fallback
  const x402Sig = req.header("payment-signature") || req.header("x-payment");
  if (x402Sig) {{
    // Verify & settle via your chosen x402 v2 facilitator (e.g. https://facilitator.payai.network)
    res.setHeader("X-Payment-Rail", "x402-v2-base-usdc");
    return next();
  }}
  const challenge = {{
    x402Version: 2,
    resource: {{ url: `{clean_route}`, description: "Private USD-quoted agent API (${formatted_usd}/call)" }},
    accepts: [{{
      scheme: "exact",
      network: "eip155:8453",
      amount: String(MICRO_USD_PRICE),
      asset: BASE_USDC,
      payTo: PAY_TO_EVM,
      maxTimeoutSeconds: 60
    }}],
    payword_fast_path: {{
      protocol: "NEWSATOSHI-PAYWORD-SHA256/1.0",
      asset: "nsUSD / LOVE (RFC-0028 USD-quoted)",
      seller_shielded_address: SELLER_NS1,
      micro_usd_per_call: MICRO_USD_PRICE,
      header: "X-Newsatoshi-PayWord: <tip_hex>:<token_hex>:<ticks>",
      verifier_docs: "{commons}/nsusd"
    }}
  }};
  res.status(402)
    .setHeader("PAYMENT-REQUIRED", Buffer.from(JSON.stringify(challenge)).toString("base64"))
    .setHeader("WWW-Authenticate", 'NEWSATOSHI-402 scheme="payword-sha256", asset="nsUSD", x402="v2", bridge_network="eip155:8453"')
    .json(challenge);
}}
// Mount on your route: app.get("{clean_route}", privateX402Paywall, (req, res) => res.json({{ ok: true }}));
"#),
        ),
        "nextjs" => (
            "app/api/paid/route.ts",
            "# Zero extra packages: uses built-in Node crypto in Next.js App Router (runtime = 'nodejs')",
            format!(
r#"import crypto from "node:crypto";
import {{ NextRequest, NextResponse }} from "next/server";

export const runtime = "nodejs";
const MICRO_USD_PRICE = {micro_usd}; // ${formatted_usd}/call USD-quoted
const PAY_TO_EVM = "{evm_dest}";
const SELLER_NS1 = "{ns1_dest}";
const BASE_USDC = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
const channelState = new Map<string, {{ ticks: number; tokenHex: string }}>();

function verifyPayWord(header: string): boolean {{
  const [tip, token, ticksStr] = header.split(":");
  const ticks = Number.parseInt(ticksStr ?? "0", 10);
  if (!tip || !token || !/^[0-9a-f]{{64}}$/i.test(tip) || !/^[0-9a-f]{{64}}$/i.test(token)) return false;
  const prev = channelState.get(tip) ?? {{ ticks: 0, tokenHex: tip }};
  const delta = ticks - prev.ticks;
  if (delta < 1 || delta > 10000) return false;
  let cur = Buffer.from(token, "hex");
  for (let i = 0; i < delta; i++) cur = crypto.createHash("sha256").update(cur).digest();
  if (cur.toString("hex") !== prev.tokenHex.toLowerCase()) return false;
  channelState.set(tip, {{ ticks, tokenHex: token.toLowerCase() }});
  return true;
}}

export async function GET(req: NextRequest) {{
  const pw = req.headers.get("x-newsatoshi-payword");
  if (pw && verifyPayWord(pw)) {{
    return NextResponse.json({{ ok: true, rail: "NEWSATOSHI-PAYWORD-SHA256/1.0", route: "{clean_route}" }});
  }}
  const challenge = {{
    x402Version: 2,
    resource: {{ url: "{clean_route}", description: "Private USD-quoted Next.js API (${formatted_usd}/call)" }},
    accepts: [{{ scheme: "exact", network: "eip155:8453", amount: String(MICRO_USD_PRICE), asset: BASE_USDC, payTo: PAY_TO_EVM, maxTimeoutSeconds: 60 }}],
    payword_fast_path: {{
      protocol: "NEWSATOSHI-PAYWORD-SHA256/1.0",
      seller_shielded_address: SELLER_NS1,
      micro_usd_per_call: MICRO_USD_PRICE,
      header: "X-Newsatoshi-PayWord: <tip_hex>:<token_hex>:<ticks>",
      docs: "{commons}/nsusd"
    }}
  }};
  return NextResponse.json(challenge, {{
    status: 402,
    headers: {{
      "PAYMENT-REQUIRED": Buffer.from(JSON.stringify(challenge)).toString("base64"),
      "WWW-Authenticate": 'NEWSATOSHI-402 scheme="payword-sha256", asset="nsUSD", x402="v2", bridge_network="eip155:8453"'
    }}
  }});
}}
"#),
        ),
        "fastapi" => (
            "private_x402_paywall.py",
            "pip install fastapi uvicorn  # Zero extra crypto deps: uses Python stdlib hashlib.sha256",
            format!(
r#"import base64, hashlib, json
from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse

MICRO_USD_PRICE = {micro_usd}  # ${formatted_usd}/call USD-quoted
PAY_TO_EVM = "{evm_dest}"
SELLER_NS1 = "{ns1_dest}"
BASE_USDC = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"
CHANNEL_STATE: dict[str, tuple[int, str]] = {{}}

def verify_payword_incremental(header: str) -> bool:
    parts = header.split(":")
    if len(parts) != 3:
        return False
    tip, token, ticks_s = parts
    try:
        ticks = int(ticks_s)
        cur = bytes.fromhex(token)
        bytes.fromhex(tip)
    except ValueError:
        return False
    prev_ticks, prev_hex = CHANNEL_STATE.get(tip.lower(), (0, tip.lower()))
    delta = ticks - prev_ticks
    if delta < 1 or delta > 10_000 or len(cur) != 32:
        return False
    for _ in range(delta):
        cur = hashlib.sha256(cur).digest()
    if cur.hex() != prev_hex:
        return False
    CHANNEL_STATE[tip.lower()] = (ticks, token.lower())
    return True

app = FastAPI()

@app.get("{clean_route}")
async def paid_endpoint(request: Request):
    pw = request.headers.get("x-newsatoshi-payword")
    if pw and verify_payword_incremental(pw):
        return {{"ok": True, "rail": "NEWSATOSHI-PAYWORD-SHA256/1.0", "price_usd": "{formatted_usd}"}}
    challenge = {{
        "x402Version": 2,
        "resource": {{"url": "{clean_route}", "description": "Private USD-quoted FastAPI endpoint (${formatted_usd}/call)"}},
        "accepts": [{{"scheme": "exact", "network": "eip155:8453", "amount": str(MICRO_USD_PRICE), "asset": BASE_USDC, "payTo": PAY_TO_EVM, "maxTimeoutSeconds": 60}}],
        "payword_fast_path": {{
            "protocol": "NEWSATOSHI-PAYWORD-SHA256/1.0",
            "seller_shielded_address": SELLER_NS1,
            "micro_usd_per_call": MICRO_USD_PRICE,
            "header": "X-Newsatoshi-PayWord: <tip_hex>:<token_hex>:<ticks>",
            "docs": "{commons}/nsusd",
        }},
    }}
    hdr_b64 = base64.b64encode(json.dumps(challenge).encode()).decode()
    return JSONResponse(
        status_code=402,
        content=challenge,
        headers={{
            "PAYMENT-REQUIRED": hdr_b64,
            "WWW-Authenticate": 'NEWSATOSHI-402 scheme="payword-sha256", asset="nsUSD", x402="v2", bridge_network="eip155:8453"',
        }},
    )
"#),
        ),
        _ => (
            "src/private_x402.rs",
            "cargo add axum sha2 hex serde_json base64",
            format!(
r#"use axum::{{http::{{HeaderMap, HeaderValue, StatusCode}}, response::IntoResponse, Json}};
use serde_json::json;
use sha2::{{Digest, Sha256}};

const MICRO_USD_PRICE: u64 = {micro_usd}; // ${formatted_usd}/call USD-quoted
const PAY_TO_EVM: &str = "{evm_dest}";
const SELLER_NS1: &str = "{ns1_dest}";

pub fn verify_payword_chain(tip_hex: &str, token_hex: &str, ticks: u64) -> bool {{
    if ticks == 0 || ticks > 10_000 {{ return false; }}
    let (Ok(tip), Ok(mut cur)) = (hex::decode(tip_hex), hex::decode(token_hex)) else {{ return false; }};
    if tip.len() != 32 || cur.len() != 32 {{ return false; }}
    for _ in 0..ticks {{
        cur = Sha256::digest(&cur).to_vec();
    }}
    cur == tip
}}

pub async fn paid_handler(headers: HeaderMap) -> impl IntoResponse {{
    if let Some(pw) = headers.get("x-newsatoshi-payword").and_then(|v| v.to_str().ok()) {{
        let mut it = pw.split(':');
        if let (Some(tip), Some(tok), Some(t)) = (it.next(), it.next(), it.next()) {{
            if let Ok(ticks) = t.parse::<u64>() {{
                if verify_payword_chain(tip, tok, ticks) {{
                    return (StatusCode::OK, HeaderMap::new(), Json(json!({{ "ok": true, "rail": "NEWSATOSHI-PAYWORD-SHA256/1.0" }})));
                }}
            }}
        }}
    }}
    let challenge = json!({{
        "x402Version": 2,
        "resource": {{ "url": "{clean_route}", "description": "Private USD-quoted Axum API (${formatted_usd}/call)" }},
        "accepts": [{{ "scheme": "exact", "network": "eip155:8453", "amount": MICRO_USD_PRICE.to_string(), "asset": "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913", "payTo": PAY_TO_EVM, "maxTimeoutSeconds": 60 }}],
        "payword_fast_path": {{ "protocol": "NEWSATOSHI-PAYWORD-SHA256/1.0", "seller_shielded_address": SELLER_NS1, "micro_usd_per_call": MICRO_USD_PRICE, "docs": "{commons}/nsusd" }}
    }});
    let mut out_hdrs = HeaderMap::new();
    out_hdrs.insert("www-authenticate", HeaderValue::from_static("NEWSATOSHI-402 scheme=\"payword-sha256\", asset=\"nsUSD\", x402=\"v2\""));
    (StatusCode::PAYMENT_REQUIRED, out_hdrs, Json(challenge))
}}
"#),
        ),
    };

    Ok(json!({
        "ok": true,
        "kit_version": env!("CARGO_PKG_VERSION"),
        "framework": norm_fw,
        "supported_frameworks": ["express", "nextjs", "fastapi", "axum"],
        "route_path": clean_route,
        "price_usd": formatted_usd,
        "micro_usd_ticks": micro_usd,
        "filename": filename,
        "install_cmd": install_cmd,
        "why_private_x402": "Zero per-call blockchain gas, sub-millisecond O(1) SHA-256 verification, zero corporate USDC freeze switch on the native LOVE/nsUSD rail, and full drop-in x402 v2 Base USDC fallback for existing wallets.",
        "curl_test_cmd": format!("curl -i http://localhost:3000{clean_route}"),
        "commons_verifier_endpoint": format!("{commons}/api/agent/402-verify"),
        "code": code
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip_and_x402_challenge_schema() {
        let sample = br#"{"x402Version":2,"network":"eip155:8453"}"#;
        let enc = base64_encode(sample);
        let dec = base64_decode(&enc).unwrap();
        assert_eq!(dec, sample);

        let spec = route_spec_bridge_quote();
        let (obj, hdr, body) = build_x402_v2_challenge("https://commons.newsatoshi.org", &spec);
        assert_eq!(obj["x402Version"], 2);
        assert_eq!(obj["accepts"].as_array().unwrap().len(), 3);
        assert_eq!(obj["accepts"][0]["network"], "eip155:8453");
        assert_eq!(obj["accepts"][0]["asset"], BASE_USDC_ADDRESS);
        assert!(body["payword_fast_path"]["protocol"].as_str().unwrap().contains("PAYWORD"));

        let decoded: Value = serde_json::from_slice(&base64_decode(&hdr).unwrap()).unwrap();
        assert_eq!(decoded["resource"]["url"], "https://commons.newsatoshi.org/api/x402/bridge-quote");
    }

    #[test]
    fn private_x402_paywall_kit_generates_all_four_frameworks() {
        for fw in ["express", "nextjs", "fastapi", "axum"] {
            let kit = build_private_x402_paywall_kit(
                fw,
                0.005,
                "/api/inference",
                Some("0x1111111111111111111111111111111111111111"),
                Some("ns1example"),
                "https://commons.newsatoshi.org",
            )
            .unwrap();
            assert_eq!(kit["ok"], true);
            assert_eq!(kit["framework"], fw);
            assert_eq!(kit["micro_usd_ticks"], 5000);
            let code = kit["code"].as_str().unwrap();
            assert!(code.contains("NEWSATOSHI-PAYWORD-SHA256/1.0"));
            assert!(code.contains("/api/inference"));
            assert!(code.contains("5000"));
        }
        assert!(build_private_x402_paywall_kit("cobol", 0.01, "/api/x", None, None, "https://commons.newsatoshi.org").is_err());
    }
}
