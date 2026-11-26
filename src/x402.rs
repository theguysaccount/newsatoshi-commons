//! Spec-compliant x402 v2 + Newsatoshi PayWord Dual-Rail Payment & Discovery Engine.
//!
//! Supports:
//! - Standard HTTP `402 Payment Required` with base64-encoded `Payment-Required` header
//!   (`x402Version: 2`, `scheme: "exact"`, CAIP-2 `eip155:8453` Base USDC + Arbitrum + Polygon,
//!   and `extensions.bazaar` discovery metadata for Coinbase x402 Bazaar, x402scan, and 402index.io).
//! - Live settlement via `POST /verify` + `POST /settle` against an open x402 facilitator
//!   (default `https://facilitator.payai.network`, overrideable via `X402_FACILITATOR_URL`).
//! - Zero-gas, post-quantum pure-SHA-256 `X-Newsatoshi-PayWord` fast path (`<1ms`, 0 gas).

use crate::agent::verify_payword_token;
use crate::backing::{BackingProofSummary, CANONICAL_BACKING_VAULT};
use crate::swap::CANONICAL_EVM_HTLC_CONTRACT;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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
            "maxAmountRequired": spec.amount_usdc_atomic,
            "asset": BASE_USDC_ADDRESS,
            "payTo": pay_to,
            "maxTimeoutSeconds": 300,
            "extra": {
                "name": "USD Coin",
                "version": "2"
            }
        },
        {
            "scheme": "exact",
            "network": "eip155:42161",
            "amount": spec.amount_usdc_atomic,
            "maxAmountRequired": spec.amount_usdc_atomic,
            "asset": ARBITRUM_USDC_ADDRESS,
            "payTo": pay_to,
            "maxTimeoutSeconds": 300,
            "extra": {
                "name": "USD Coin",
                "version": "2"
            }
        },
        {
            "scheme": "exact",
            "network": "eip155:137",
            "amount": spec.amount_usdc_atomic,
            "maxAmountRequired": spec.amount_usdc_atomic,
            "asset": POLYGON_USDC_ADDRESS,
            "payTo": pay_to,
            "maxTimeoutSeconds": 300,
            "extra": {
                "name": "USD Coin",
                "version": "2"
            }
        }
    ]);

    let payment_required_obj = json!({
        "x402Version": 2,
        "error": "Payment required",
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
        "accepts": payment_required_obj["accepts"],
        "resource": payment_required_obj["resource"],
        "payword_fast_path": {
            "protocol": "NEWSATOSHI-PAYWORD-SHA256/1.0",
            "header": "X-Newsatoshi-PayWord: tip=<64hex>,token=<64hex>,ticks=<n>",
            "gas_usd": 0,
            "latency_ms": "<1",
            "note": "Zero-gas post-quantum pure-SHA-256 alternative to on-chain x402 settlement"
        },
        "backing_vault": CANONICAL_BACKING_VAULT
    });

    (payment_required_obj, header_b64, body)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettlementReceipt {
    pub rail: String,
    pub settled: bool,
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

/// Attempts to authorize a request via EITHER:
/// 1. `X-Newsatoshi-PayWord: tip=...,token=...,ticks=...` (zero-gas pure SHA-256 local verification), OR
/// 2. `PAYMENT-SIGNATURE` / `X-PAYMENT` (x402 v2 / v1 Base/Arbitrum/Polygon USDC verified & settled live via facilitator).
pub fn authorize_dual_402(
    payword_header: Option<&str>,
    x402_header: Option<&str>,
    challenge_obj: &Value,
) -> Result<Option<SettlementReceipt>, String> {
    // Path 1: Zero-gas Newsatoshi PayWord fast path
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
        let res = verify_payword_token(&tip_h, &tok_h, n, 10_000)?;
        if !res.valid {
            return Err("invalid PayWord hash-chain proof".to_string());
        }
        return Ok(Some(SettlementReceipt {
            rail: "NEWSATOSHI-PAYWORD-SHA256/1.0".to_string(),
            settled: true,
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

        // For x402 v1 payloads, adapt network alias ("base") so v1 facilitators accept seamlessly
        if x402_version == 1 {
            if let Some(map) = req_obj.as_object_mut() {
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
        description: "Real-time multi-chain bridge & backing intelligence for Newsatoshi (LOVE): live Base Uniswap V3 wLOVE/WETH pool metrics, 4-chain ownerless NewsatoshiBackingVault reserves (Base/Arbitrum/Optimism/Polygon), and live Mainnet L1 AuxPoW issuance. Accepts Base/Arb/Polygon USDC via x402 ($0.001) or zero-gas SHA-256 PayWords.",
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
        description: "Compiles zero-custody SHA-256 atomic swap HTLC contracts across Bitcoin BIP-199 P2WSH, Lightning BOLT-11, Solana PDA, and Base/Arbitrum/Optimism/Polygon NewsatoshiHtlc.sol (0xF564…8987). Accepts x402 USDC ($0.001) or zero-gas SHA-256 PayWords.",
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
        description: "Verifies pure-SHA-256 PayWord micropayment hash-chains (SHA256^ticks(token) == tip) for sub-millisecond, zero-gas AI agent billing. Accepts x402 USDC ($0.001) or PayWord.",
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

/// Renders `/.well-known/x402.json` discovery manifest for x402 indexers & crawlers.
pub fn render_well_known_x402(base_url: &str) -> Value {
    let specs = [
        route_spec_bridge_quote(),
        route_spec_compile_htlc(),
        route_spec_verify_payword(),
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
        "website": "https://newsatoshi.org",
        "commons": base_url.trim_end_matches('/'),
        "mcp": format!("{}/mcp", base_url.trim_end_matches('/')),
        "erc8004": format!("{}/.well-known/erc8004.json", base_url.trim_end_matches('/')),
        "facilitator": x402_facilitator_url(),
        "payTo": x402_pay_to(),
        "resources": resources
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
                        "disclosure": "wLOVE is an ownerless, LP-burned Base ERC-20 discovery & HTLC bridge mirror. Native L1 LOVE is separate, fair-mined via AuxPoW (chainid 20051), and traded peer-to-peer via SHA-256 HTLCs."
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
        "disclosure": "wLOVE is an ownerless, LP-burned Base ERC-20 discovery & HTLC bridge mirror. Native L1 LOVE is separate and traded peer-to-peer via SHA-256 HTLCs."
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
}
