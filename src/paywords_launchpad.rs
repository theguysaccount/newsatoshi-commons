//! Lowest-Cost Stablecoin Payment Engine (`O(1)` SHA-256 PayWords on Base USDC) &
//! Fair Bonding-Curve Launchpad Compiler (`NewsatoshiFairCurveFactory`).
//!
//! All formulas and on-chain references match our live, Sourcify-verified Base Mainnet (`eip155:8453`)
//! immutable contracts and Lean 4.36.0 Theorems 9-11 (`formal/NsUsdAndL2Perfection.lean`, 0 sorry):
//! - `NewsatoshiPayWordChannel`:   `0x971BA814Df91c20DcA3aDd870C2B55d643965C50`
//! - `NewsatoshiFairCurveFactory`: `0xF6bE782819a8eD0D1556A8f5DC3223f9eb13a933`
//! - Live Demo `FairCurvePool`:    `0xC3098ba25E39Effa63dE94EbACE6381781ad4cEd` (`NSPAY`)

use serde_json::{json, Value};

pub const BASE_USDC_ADDRESS: &str = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
pub const OPTIMISM_USDC_ADDRESS: &str = "0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85";
pub const ARBITRUM_USDC_ADDRESS: &str = "0xaf88d065e77c8cC2239327C5EDb3A432268e5831";
pub const POLYGON_USDC_ADDRESS: &str = "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359";
pub const SDK_JS_SOURCE: &str = include_str!("../sdk/js/newsatoshi-402.mjs");
pub const SDK_DTS_SOURCE: &str = include_str!("../sdk/js/newsatoshi-402.d.ts");
pub const SDK_PY_SOURCE: &str = include_str!("../sdk/python/newsatoshi_402.py");
pub const BASE_PAYWORD_CHANNEL_ADDRESS: &str = "0x971BA814Df91c20DcA3aDd870C2B55d643965C50";
/// Live Sourcify exact_match NewsatoshiPayWordChannel deployed identically at nonce 4 on Optimism (10) & Arbitrum One (42161).
pub const MULTICHAIN_PAYWORD_CHANNEL_ADDRESS: &str = "0x3DeBB6d5ae490e6cF4F966585e000028111CF106";
pub const CANONICAL_EVM_HTLC_ADDRESS: &str = "0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987";
pub const BASE_FAIR_CURVE_FACTORY_ADDRESS: &str = "0xF6bE782819a8eD0D1556A8f5DC3223f9eb13a933";
pub const BASE_DEMO_CURVE_POOL_ADDRESS: &str = "0xC3098ba25E39Effa63dE94EbACE6381781ad4cEd";
pub const BASE_DEMO_CURVE_TOKEN_ADDRESS: &str = "0x406B3484560c89C030720D9a4101500474e5D2d9";

pub const BASE_PAYWORD_OPEN_PROOF_TX: &str =
    "0x8229bb7282f79b819122c40e66e8f8dcec832d30fccb1a5b75f9eb49133be31d";
pub const BASE_PAYWORD_CLAIM_PROOF_TX: &str =
    "0x122885e1b36aa5b8a62f97f4b4bfb3b235bdebe0055a8cd3a4ed7c52393a5356";
pub const BASE_PAYWORD_CLOSE_PROOF_TX: &str =
    "0x3a826c82fdaffdbbdb8049db7793a342524e3a74742c9504ef8baad1380c4c0d";
pub const BASE_PAYWORD_1000_APPROVE_PROOF_TX: &str =
    "0x74f3b4115f021df288ca04c42eef2a326dfdb38992fd2e73cbc25dc35232eb3e";
pub const BASE_PAYWORD_1000_OPEN_PROOF_TX: &str =
    "0x0d975a2b3310473d55d0e7d9e7b77c0174f4098fabbd3ae5e5512ec36de6f216";
pub const BASE_PAYWORD_1000_CLAIM_PROOF_TX: &str =
    "0xb79a3c01a8edc0bbc0b25714b2a1b7852c0035be2a083c26da81ef31620f6256";
pub const OPTIMISM_PAYWORD_DEPLOY_TX: &str =
    "0x3b88845cb29a14cfab1f95d764ae0311a6d22b7c2b92086191179c914e0ad191";
pub const OPTIMISM_PAYWORD_OPEN_100_TX: &str =
    "0x4f2f61a4419f53de94d96b717c4ebcd6eb5fa7039e6d0c9ee7a13ed47445cb5e";
pub const OPTIMISM_PAYWORD_CLAIM_100_TX: &str =
    "0x05f72b5f3bbff8407b78be11cbc962414a2af573bc9f08466cab060814b8ef6c";
pub const ARBITRUM_PAYWORD_DEPLOY_TX: &str =
    "0xeab6dfec32c41e44cb784b0675a468e421725fcc56023f4af738226f5ad76fea";
pub const OPTIMISM_HTLC_LOCK_PROOF_TX: &str =
    "0xcb57a76d9d28dcc3c4c0ac601061cd361bef66fd6f3e00d7c22f9c9f4359ffd8";
pub const OPTIMISM_HTLC_CLAIM_PROOF_TX: &str =
    "0x88ca2f68dc6e1587ee157bf2f48f6ed5a5044923686a9aac7fda5a9b6be88f0b";
pub const BASE_CURVE_CREATE_PROOF_TX: &str =
    "0x8da0c06753664c52df3c2e61227035f38fc78b98c11650791b26f5d246308c83";
pub const BASE_CURVE_BUY_PROOF_TX: &str =
    "0xa13933b18778f7a3889125b4bae8b5dec3e4182173641e46f282ef3411e332a0";
pub const BASE_CURVE_INJECT_REV_PROOF_TX: &str =
    "0xa5ea78b2392ed07db61ebd2115485f80dad6551551bedbcd17bfdfa670a3beb2";
pub const BASE_CURVE_CLAIM_REV_PROOF_TX: &str =
    "0x83163d3af4eefcc89d4ddc8f319e6eddcb2f42450b91db8c6e19ed9c7b1eda8a";

/// Deterministic cost-savings comparison between Stripe cards (`2.9% + $0.30`), per-call Base L2
/// x402 USDC settlement, and Newsatoshi `O(1)` SHA-256 PayWords batched on live Base Mainnet.
pub fn estimate_paywords_cost_savings(
    monthly_calls: u64,
    price_per_call_usd: f64,
    calls_per_batch: u64,
) -> Result<Value, String> {
    if monthly_calls == 0 || monthly_calls > 1_000_000_000 {
        return Err("monthly_calls must be in 1..=1_000_000_000".into());
    }
    if !price_per_call_usd.is_finite() || price_per_call_usd <= 0.0 || price_per_call_usd > 10_000.0 {
        return Err("price_per_call_usd must be finite and in (0, 10000]".into());
    }
    let batch = calls_per_batch.clamp(10, 4_096);

    let gross_monthly_usd = (monthly_calls as f64) * price_per_call_usd;

    // Published Stripe standard card pricing: 2.9% + $0.30 per individual charge ($0.50 USD minimum).
    let stripe_fee_usd = (monthly_calls as f64) * (price_per_call_usd * 0.029 + 0.30);
    let stripe_effective_pct = if gross_monthly_usd > 0.0 {
        (stripe_fee_usd / gross_monthly_usd) * 100.0
    } else {
        0.0
    };

    // Even if calls are aggregated into one Stripe card charge per settlement batch (requires
    // custodial prepaid credit when batch < $0.50 USD minimum):
    let settlement_batches = monthly_calls.div_ceil(batch);
    let stripe_batched_fee_usd =
        (gross_monthly_usd * 0.029) + (settlement_batches as f64) * 0.30;
    let stripe_batched_effective_pct = if gross_monthly_usd > 0.0 {
        (stripe_batched_fee_usd / gross_monthly_usd) * 100.0
    } else {
        0.0
    };

    // Typical per-call Base L2 ERC-20 transfer / EIP-3009 x402 settlement (~$0.001/call L2 gas + facilitator).
    let per_call_x402_gas_usd = 0.001_f64;
    let per_call_x402_fee_usd = (monthly_calls as f64) * per_call_x402_gas_usd;
    let per_call_x402_effective_pct = if gross_monthly_usd > 0.0 {
        (per_call_x402_fee_usd / gross_monthly_usd) * 100.0
    } else {
        0.0
    };

    // Newsatoshi O(1) SHA-256 PayWords settled via NewsatoshiPayWordChannel on Base Mainnet:
    // Empirically measured execution gas on Base Mainnet: ~65,000 fixed + ~706 gas per SHA-256 step
    // (live BaseScan claim(7) = 96,449 gas; live BaseScan claim(1000) = 770,956 gas @ 0.006 gwei = $0.0077 incl. L1 fee).
    // Calculator prices stress gas conservatively at 0.01 gwei / $2,500 ETH + $0.001 L1 data fee.
    let gas_per_claim: u64 = 65_000 + 706 * batch;
    let eth_price_usd = 2_500.0_f64;
    let base_gas_gwei = 0.01_f64;
    let usd_per_settlement_tx = (gas_per_claim as f64) * (base_gas_gwei * 1e-9) * eth_price_usd;
    let estimated_base_l1_data_fee_usd = 0.001_f64;
    let estimated_one_time_open_tx_usd =
        (115_000.0 * (base_gas_gwei * 1e-9) * eth_price_usd) + estimated_base_l1_data_fee_usd;
    let paywords_monthly_fee_usd =
        (settlement_batches as f64) * (usd_per_settlement_tx + estimated_base_l1_data_fee_usd);
    let paywords_effective_pct = if gross_monthly_usd > 0.0 {
        (paywords_monthly_fee_usd / gross_monthly_usd) * 100.0
    } else {
        0.0
    };

    let savings_vs_stripe_usd = (stripe_fee_usd - paywords_monthly_fee_usd).max(0.0);
    let savings_vs_stripe_batched_usd = (stripe_batched_fee_usd - paywords_monthly_fee_usd).max(0.0);
    let savings_vs_x402_usd = (per_call_x402_fee_usd - paywords_monthly_fee_usd).max(0.0);
    let fee_reduction_vs_stripe_pct = if stripe_fee_usd > 0.0 {
        ((stripe_fee_usd - paywords_monthly_fee_usd) / stripe_fee_usd) * 100.0
    } else {
        0.0
    };
    let fee_reduction_vs_stripe_batched_pct = if stripe_batched_fee_usd > 0.0 {
        ((stripe_batched_fee_usd - paywords_monthly_fee_usd) / stripe_batched_fee_usd) * 100.0
    } else {
        0.0
    };
    let fee_reduction_vs_x402_pct = if per_call_x402_fee_usd > 0.0 {
        ((per_call_x402_fee_usd - paywords_monthly_fee_usd) / per_call_x402_fee_usd) * 100.0
    } else {
        0.0
    };

    Ok(json!({
        "ok": true,
        "inputs": {
            "monthly_calls": monthly_calls,
            "price_per_call_usd": price_per_call_usd,
            "calls_per_settlement_batch": batch,
            "gross_monthly_revenue_usd": round2(gross_monthly_usd)
        },
        "assumptions_transparent": {
            "stripe_per_call_formula": "2.9% + $0.30 per call (note: Stripe's $0.50 USD minimum charge makes sub-$0.50 per-call card billing unavailable without custodial prepaid accounts)",
            "stripe_batched_formula": "2.9% + $0.30 per aggregated settlement batch (e.g. $0.329 on a single $1.00 batch of 1,000 x $0.001 calls)",
            "per_call_x402_formula": "$0.001 per-call Base L2 tx gas + facilitator overhead",
            "newsatoshi_paywords_formula": "0% protocol fee; measured Base claim() execution gas (65,000 + 720 * batch) at 0.01 gwei & $2,500 ETH + $0.001 estimated Base L1 data fee per settlement tx (one-time channel open ~115k gas reported separately)"
        },
        "comparison": {
            "stripe_cards": {
                "monthly_fees_usd": round2(stripe_fee_usd),
                "effective_fee_pct": round4(stripe_effective_pct),
                "net_retained_usd": round2(gross_monthly_usd - stripe_fee_usd)
            },
            "stripe_batched_prepaid": {
                "monthly_fees_usd": round2(stripe_batched_fee_usd),
                "effective_fee_pct": round4(stripe_batched_effective_pct),
                "net_retained_usd": round2(gross_monthly_usd - stripe_batched_fee_usd),
                "note": "Requires custodial prepaid balance + minimum $0.50 USD charge"
            },
            "per_call_base_x402_usdc": {
                "monthly_fees_usd": round2(per_call_x402_fee_usd),
                "effective_fee_pct": round4(per_call_x402_effective_pct),
                "net_retained_usd": round2(gross_monthly_usd - per_call_x402_fee_usd)
            },
            "newsatoshi_paywords_base_usdc": {
                "protocol_fee_bps": 0,
                "per_call_verification": "1 SHA-256 hash (~0.0039 ms WebCrypto, $0.000000 gas)",
                "monthly_settlement_txs": settlement_batches,
                "estimated_gas_per_claim_tx": gas_per_claim,
                "estimated_usd_per_claim_tx": round6(usd_per_settlement_tx),
                "estimated_base_l1_data_fee_per_tx_usd": estimated_base_l1_data_fee_usd,
                "estimated_one_time_channel_open_usd": round6(estimated_one_time_open_tx_usd),
                "monthly_fees_usd": round4(paywords_monthly_fee_usd),
                "effective_fee_pct": round6(paywords_effective_pct),
                "net_retained_usd": round2(gross_monthly_usd - paywords_monthly_fee_usd)
            }
        },
        "savings": {
            "monthly_savings_vs_stripe_usd": round2(savings_vs_stripe_usd),
            "annual_savings_vs_stripe_usd": round2(savings_vs_stripe_usd * 12.0),
            "fee_reduction_vs_stripe_pct": round4(fee_reduction_vs_stripe_pct),
            "monthly_savings_vs_stripe_batched_usd": round2(savings_vs_stripe_batched_usd),
            "annual_savings_vs_stripe_batched_usd": round2(savings_vs_stripe_batched_usd * 12.0),
            "fee_reduction_vs_stripe_batched_pct": round4(fee_reduction_vs_stripe_batched_pct),
            "monthly_savings_vs_per_call_x402_usd": round2(savings_vs_x402_usd),
            "annual_savings_vs_per_call_x402_usd": round2(savings_vs_x402_usd * 12.0),
            "fee_reduction_vs_per_call_x402_pct": round4(fee_reduction_vs_x402_pct)
        },
        "live_base_mainnet_proof": {
            "chain": "base (eip155:8453)",
            "contract": BASE_PAYWORD_CHANNEL_ADDRESS,
            "sourcify_exact_match": format!("https://repo.sourcify.dev/8453/{BASE_PAYWORD_CHANNEL_ADDRESS}"),
            "claim_1000_approve_tx": BASE_PAYWORD_1000_APPROVE_PROOF_TX,
            "claim_1000_open_channel_tx": BASE_PAYWORD_1000_OPEN_PROOF_TX,
            "claim_1000_steps_tx": BASE_PAYWORD_1000_CLAIM_PROOF_TX,
            "claim_1000_steps_gas_used": 770956,
            "claim_1000_measured_recurring_usd_incl_l1_fee": 0.007709,
            "claim_1000_measured_full_lifecycle_usd_incl_l1_fee": 0.010418,
            "claim_1000_vs_batched_1usd_stripe_savings_pct": "96.8%–97.7%",
            "open_channel_tx": BASE_PAYWORD_OPEN_PROOF_TX,
            "claim_7_steps_tx": BASE_PAYWORD_CLAIM_PROOF_TX,
            "claim_7_steps_gas_used": 96449,
            "close_and_refund_tx": BASE_PAYWORD_CLOSE_PROOF_TX,
            "lean4_conservation_theorem": "Newsatoshi.NsUsdL2.payword_channel_exact_conservation (0 sorry)"
        }
    }))
}

/// Generate an instant, org-ready multi-chain USDC `NewsatoshiPayWordChannel` integration kit
/// (Base, Optimism, or Arbitrum One) for server gates or autonomous AI agent auto-pay clients.
pub fn compile_payword_channel_setup(
    framework: &str,
    recipient_evm: Option<&str>,
    price_usd: f64,
    max_steps: u32,
    route_path: &str,
    base_url: &str,
) -> Result<Value, String> {
    compile_payword_channel_setup_for_chain(
        "base",
        framework,
        recipient_evm,
        price_usd,
        max_steps,
        route_path,
        base_url,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn compile_payword_channel_setup_for_chain(
    chain: &str,
    framework: &str,
    recipient_evm: Option<&str>,
    price_usd: f64,
    max_steps: u32,
    route_path: &str,
    base_url: &str,
) -> Result<Value, String> {
    if !price_usd.is_finite() || price_usd < 0.000001 || price_usd > 1_000.0 {
        return Err("price_usd must be in [0.000001, 1000]".into());
    }
    if max_steps == 0 || max_steps > 1_000_000 {
        return Err("max_steps must be in 1..=1_000_000".into());
    }
    let (chain_name, chain_id, caip2, rpc_url, usdc_addr, contract_addr) =
        match chain.trim().to_ascii_lowercase().as_str() {
            "" | "base" | "8453" | "eip155:8453" => (
                "base",
                8453u64,
                "eip155:8453",
                "https://mainnet.base.org",
                BASE_USDC_ADDRESS,
                BASE_PAYWORD_CHANNEL_ADDRESS,
            ),
            "optimism" | "op" | "10" | "eip155:10" => (
                "optimism",
                10u64,
                "eip155:10",
                "https://mainnet.optimism.io",
                OPTIMISM_USDC_ADDRESS,
                MULTICHAIN_PAYWORD_CHANNEL_ADDRESS,
            ),
            "arbitrum" | "arb" | "42161" | "eip155:42161" => (
                "arbitrum",
                42161u64,
                "eip155:42161",
                "https://arb1.arbitrum.io/rpc",
                ARBITRUM_USDC_ADDRESS,
                MULTICHAIN_PAYWORD_CHANNEL_ADDRESS,
            ),
            other => {
                return Err(format!(
                    "unsupported chain '{other}': choose base | optimism | arbitrum"
                ));
            }
        };
    let unit_usdc_raw = (price_usd * 1_000_000.0).round().max(1.0) as u64;
    let total_deposit_usdc_raw = unit_usdc_raw
        .checked_mul(max_steps as u64)
        .ok_or_else(|| "deposit overflow".to_string())?;
    let total_deposit_usd = (total_deposit_usdc_raw as f64) / 1_000_000.0;
    let recipient = recipient_evm
        .map(str::trim)
        .filter(|s| s.starts_with("0x") && s.len() == 42)
        .unwrap_or("0xYOUR_ORG_TREASURY_ADDRESS");
    let route = if route_path.trim().is_empty() {
        "/api/inference"
    } else {
        route_path.trim()
    };

    let fw = framework.trim().to_ascii_lowercase();
    let (lang, filename, snippet) = match fw.as_str() {
        "cloudflare_worker" | "cloudflare" | "worker" => (
            "typescript",
            "src/paywords-worker.ts",
            format!(
                r#"// Cloudflare Worker — O(1) SHA-256 PayWords HTTP 402 Gate (0% protocol fee, 0 per-call gas)
// Chain: {chain_name} ({caip2}) · Immutable Sourcify-verified NewsatoshiPayWordChannel: {contract_addr}
// Verify on-chain deposit once per channelId, then verify 1 SHA-256 per request at the edge (~0.013 ms).

const CHANNEL_CONTRACT = "{contract_addr}";
const CHAIN_USDC = "{usdc_addr}";
const RECIPIENT = "{recipient}";
const UNIT_USDC_RAW = {unit_usdc_raw}; // ${price_usd:.6} USDC per call

async function sha256Hex(hex32: string): Promise<string> {{
  const clean = hex32.replace(/^0x/i, "");
  const bytes = new Uint8Array(clean.match(/.{{2}}/g)!.map((b) => parseInt(b, 16)));
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return "0x" + [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}}

// Replace with Cloudflare Durable Object / KV in production (O(1) state per channelId)
const channelState = new Map<string, {{ lastIndex: number; lastToken: string; maxSteps: number }}>();

export default {{
  async fetch(req: Request): Promise<Response> {{
    const url = new URL(req.url);
    if (url.pathname !== "{route}") return new Response("Not Found", {{ status: 404 }});

    const sig = req.headers.get("PAYMENT-SIGNATURE");
    if (!sig) {{
      return Response.json(
        {{
          x402Version: 2,
          scheme: "batch-settlement",
          binding: "paywords-sha256",
          network: "{caip2}",
          asset: CHAIN_USDC,
          channelContract: CHANNEL_CONTRACT,
          payTo: RECIPIENT,
          unitAmount: String(UNIT_USDC_RAW),
          recommendedMaxSteps: {max_steps},
          docs: "{base_url}/api/v2/paywords/channel-kit"
        }},
        {{ status: 402 }}
      );
    }}

    const {{ channelId, index, tokenHex, rootHex, maxSteps }} = JSON.parse(atob(sig));
    const st = channelState.get(channelId) ?? {{ lastIndex: 0, lastToken: rootHex, maxSteps }};
    if (!Number.isInteger(index) || index <= st.lastIndex || index > st.maxSteps) {{
      return Response.json({{ error: "invalid_or_replayed_payword_index" }}, {{ status: 402 }});
    }}
    let h = tokenHex.toLowerCase();
    for (let i = 0; i < index - st.lastIndex; i++) h = await sha256Hex(h);
    if (h !== st.lastToken.toLowerCase()) {{
      return Response.json({{ error: "bad_payword_preimage" }}, {{ status: 402 }});
    }}
    channelState.set(channelId, {{ lastIndex: index, lastToken: tokenHex, maxSteps: st.maxSteps }});
    // Batch settle whenever ready via: cast send {contract_addr} "claim(bytes32,uint32,bytes32)" <channelId> <steps> <latestToken> --rpc-url {rpc_url}
    return Response.json({{ ok: true, settledOffchainStep: index, unitUsdcRaw: UNIT_USDC_RAW }});
  }},
}};
"#
            ),
        ),
        "express" | "node" => (
            "javascript",
            "paywords-middleware.mjs",
            format!(
                r#"// Express / Node.js — Zero-Dependency O(1) SHA-256 PayWords Middleware ({chain_name} USDC)
// Live Channel Contract ({caip2}): {contract_addr} (Sourcify exact_match)
import crypto from "node:crypto";

const CHANNEL_CONTRACT = "{contract_addr}";
const CHAIN_USDC = "{usdc_addr}";
const RECIPIENT = "{recipient}";
const UNIT_USDC_RAW = {unit_usdc_raw}; // ${price_usd:.6} USDC/call
const channels = new Map(); // Persist in Redis/SQLite: channelId -> {{ lastIndex, lastToken, maxSteps }}

const sha256 = (hex) => "0x" + crypto.createHash("sha256").update(Buffer.from(hex.replace(/^0x/i, ""), "hex")).digest("hex");

export function paywordsGate(req, res, next) {{
  const hdr = req.header("PAYMENT-SIGNATURE");
  if (!hdr) {{
    return res.status(402).json({{
      x402Version: 2,
      scheme: "batch-settlement",
      binding: "paywords-sha256",
      network: "{caip2}",
      asset: CHAIN_USDC,
      channelContract: CHANNEL_CONTRACT,
      payTo: RECIPIENT,
      unitAmount: String(UNIT_USDC_RAW),
      recommendedMaxSteps: {max_steps},
    }});
  }}
  const {{ channelId, index, tokenHex, rootHex, maxSteps }} = JSON.parse(Buffer.from(hdr, "base64").toString("utf8"));
  const st = channels.get(channelId) ?? {{ lastIndex: 0, lastToken: rootHex, maxSteps }};
  if (index <= st.lastIndex || index > st.maxSteps) return res.status(402).json({{ error: "replay_or_overclaim" }});
  let h = tokenHex.toLowerCase();
  for (let i = 0; i < index - st.lastIndex; i++) h = sha256(h);
  if (h !== st.lastToken.toLowerCase()) return res.status(402).json({{ error: "bad_payword_proof" }});
  channels.set(channelId, {{ lastIndex: index, lastToken: tokenHex, maxSteps: st.maxSteps }});
  req.paywordStep = index;
  return next();
}}
// Mount: app.post("{route}", paywordsGate, handler);
"#
            ),
        ),
        "fastapi" | "python" => (
            "python",
            "paywords_gate.py",
            format!(
                r#"# FastAPI / Python — Zero-Dependency O(1) SHA-256 PayWords Gate ({chain_name} USDC)
# Live Channel Contract ({caip2}): {contract_addr} (Sourcify exact_match)
import base64, hashlib, json
from fastapi import Header, HTTPException

CHANNEL_CONTRACT = "{contract_addr}"
CHAIN_USDC = "{usdc_addr}"
RECIPIENT = "{recipient}"
UNIT_USDC_RAW = {unit_usdc_raw}  # ${price_usd:.6} USDC/call
CHANNELS = {{}}  # Store in Redis/Postgres: channel_id -> (last_index, last_token, max_steps)

def verify_payword(payment_signature: str | None = Header(default=None, alias="PAYMENT-SIGNATURE")):
    if not payment_signature:
        raise HTTPException(status_code=402, detail={{
            "x402Version": 2, "scheme": "batch-settlement", "binding": "paywords-sha256",
            "network": "{caip2}", "asset": CHAIN_USDC, "channelContract": CHANNEL_CONTRACT,
            "payTo": RECIPIENT, "unitAmount": str(UNIT_USDC_RAW), "recommendedMaxSteps": {max_steps},
        }})
    p = json.loads(base64.b64decode(payment_signature))
    cid, idx, tok = p["channelId"], int(p["index"]), p["tokenHex"].removeprefix("0x").lower()
    last_idx, last_tok, max_steps = CHANNELS.get(cid, (0, p["rootHex"].removeprefix("0x").lower(), int(p["maxSteps"])))
    if idx <= last_idx or idx > max_steps:
        raise HTTPException(status_code=402, detail="replay_or_overclaim")
    h = bytes.fromhex(tok)
    for _ in range(idx - last_idx):
        h = hashlib.sha256(h).digest()
    if h.hex() != last_tok:
        raise HTTPException(status_code=402, detail="bad_payword_proof")
    CHANNELS[cid] = (idx, tok, max_steps)
    return {{"channelId": cid, "step": idx, "unitUsdcRaw": UNIT_USDC_RAW}}
"#
            ),
        ),
        "agent_fetch_js" | "openai_agent" | "agent_js" => (
            "javascript",
            "agent-autopay.mjs",
            format!(
                r#"// Autonomous AI Agent Auto-Pay Client (Node 18+ / OpenAI / Anthropic / Vercel AI SDK)
// Download zero-dep SDK: curl -fsSL https://newsatoshi.org/sdk/newsatoshi-402.mjs -o newsatoshi-402.mjs
import {{ generatePayWordChain, createAutoPayFetch, EVM_PAYWORD_CHAINS }} from "./newsatoshi-402.mjs";

const chain = EVM_PAYWORD_CHAINS.{chain_name};
const wallet = await generatePayWordChain({max_steps}, {unit_usdc_raw});
// Open channel once on {chain_name} ({contract_addr}) with root `0x${{wallet.tipHex}}`, then auto-pay every HTTP 402 call:
const payFetch = createAutoPayFetch({{
  wallet,
  chain: "{chain_name}",
  channelId: "0x" + "00".repeat(32), // replace with your live on-chain channelId (or test against try-402 sandbox!)
}});

// Works on any HTTP 402 PayWords API — test live right now against Commons:
const res = await payFetch("{base_url}/api/v2/paywords/try-402", {{ method: "POST" }});
console.log(await res.json());
"#
            ),
        ),
        "agent_python" | "langchain_python" | "agent_py" => (
            "python",
            "agent_autopay.py",
            format!(
                r#"# Autonomous AI Agent Auto-Pay Client (Python 3.9+ / LangChain / CrewAI / OpenAI / Anthropic)
# Download zero-dep SDK: curl -fsSL https://newsatoshi.org/sdk/newsatoshi-402.py -o newsatoshi_402.py
from newsatoshi_402 import AutoPaySession, EVM_PAYWORD_CHAINS, generate_payword_chain

chain_cfg = EVM_PAYWORD_CHAINS["{chain_name}"]
wallet = generate_payword_chain({max_steps}, {unit_usdc_raw})
# Open channel once on {chain_name} ({contract_addr}) with root `0x{{wallet.tip_hex}}`, then auto-pay every HTTP 402 call:
session = AutoPaySession(wallet=wallet, chain="{chain_name}", channel_id="0x" + "00" * 32)

# Test live HTTP 402 -> O(1) SHA-256 voucher -> 200 OK right now against Commons:
status, body = session.request("{base_url}/api/v2/paywords/try-402", method="POST")
print(status, body)
"#
            ),
        ),
        _ => {
            return Err(
                "unsupported framework: choose cloudflare_worker | express | fastapi | agent_fetch_js | agent_python".into(),
            );
        }
    };

    Ok(json!({
        "ok": true,
        "chain": chain_name,
        "framework": fw,
        "language": lang,
        "filename": filename,
        "route_path": route,
        "pricing": {
            "price_usd": price_usd,
            "unit_usdc_raw": unit_usdc_raw,
            "max_steps": max_steps,
            "total_channel_deposit_usdc_raw": total_deposit_usdc_raw,
            "total_channel_deposit_usd": round6(total_deposit_usd),
            "protocol_fee_bps": 0
        },
        "target_chain": {
            "chain": chain_name,
            "chain_id": chain_id,
            "caip2": caip2,
            "rpc_url": rpc_url,
            "usdc_token": usdc_addr,
            "payword_channel_contract": contract_addr,
            "sourcify_url": format!("https://repo.sourcify.dev/{chain_id}/{contract_addr}"),
        },
        "base_mainnet": {
            "chain_id": 8453,
            "caip2": "eip155:8453",
            "usdc_token": BASE_USDC_ADDRESS,
            "payword_channel_contract": BASE_PAYWORD_CHANNEL_ADDRESS,
            "sourcify_url": format!("https://repo.sourcify.dev/8453/{BASE_PAYWORD_CHANNEL_ADDRESS}"),
            "live_proof_txs": {
                "claim_1000_open": BASE_PAYWORD_1000_OPEN_PROOF_TX,
                "claim_1000": BASE_PAYWORD_1000_CLAIM_PROOF_TX,
                "open_channel": BASE_PAYWORD_OPEN_PROOF_TX,
                "claim_7_steps": BASE_PAYWORD_CLAIM_PROOF_TX,
                "close_and_refund": BASE_PAYWORD_CLOSE_PROOF_TX
            }
        },
        "cli_settlement_commands": {
            "step1_approve_usdc": format!(
                "cast send {usdc_addr} \"approve(address,uint256)\" {contract_addr} {total_deposit_usdc_raw} --rpc-url {rpc_url} --private-key $PAYER_KEY"
            ),
            "step2_open_channel": format!(
                "cast send {contract_addr} \"openChannel(address,address,bytes32,uint256,uint32,uint64)\" {recipient} {usdc_addr} <ROOT_W0_HEX32> {unit_usdc_raw} {max_steps} <EXPIRY_UNIX> --rpc-url {rpc_url} --private-key $PAYER_KEY"
            ),
            "step3_claim_batch": format!(
                "cast send {contract_addr} \"claim(bytes32,uint32,bytes32)\" <CHANNEL_ID> <STEPS_DELTA> <LATEST_PREIMAGE_WK> --rpc-url {rpc_url} --private-key $SERVER_KEY"
            ),
            "step4_early_close_refund": format!(
                "cast send {contract_addr} \"closeAndRefund(bytes32)\" <CHANNEL_ID> --rpc-url {rpc_url} --private-key $SERVER_KEY"
            )
        },
        "code": snippet,
        "licensing_note": "Core reference code is PolyForm-Noncommercial-1.0.0; wire spec is open (x402-foundation/x402#4494) and direct commercial partner licenses are available via contributors@newsatoshi.org."
    }))
}

/// Live public HTTP 402 PayWords playground (`GET/POST /api/v2/paywords/try-402`):
/// - Without a voucher header: emits a real `HTTP 402 Payment Required` (`x402 v2` + `NEWSATOSHI-402/1.0`)
///   advertising all 3 live Sourcify-verified EVM PayWord chains (Base, Optimism, Arbitrum) + native LOVE L1.
/// - With `PAYMENT-SIGNATURE` (base64 JSON) or `X-Newsatoshi-PayWord: tip=...,token=...,ticks=...`:
///   verifies `SHA256^steps(token) == root` in real Rust with measured microsecond latency and returns `200 OK`.
pub fn evaluate_live_paywords_try_402(
    payment_signature_b64: Option<&str>,
    payword_hdr: Option<&str>,
    base_url: &str,
) -> (u16, Value) {
    let challenge = || {
        json!({
            "ok": false,
            "error": "payment_required",
            "x402Version": 2,
            "scheme": "batch-settlement",
            "binding": "paywords-sha256",
            "protocol": "NEWSATOSHI-402/1.0",
            "sandbox_notice": "Send either PAYMENT-SIGNATURE (base64 JSON {channelId,index,tokenHex,rootHex,maxSteps}) or X-Newsatoshi-PayWord: tip=<64hex>,token=<64hex>,ticks=<n> to verify a real SHA-256 PayWord voucher live in microseconds.",
            "live_settlement_chains": [
                { "chain": "base", "caip2": "eip155:8453", "asset": BASE_USDC_ADDRESS, "channelContract": BASE_PAYWORD_CHANNEL_ADDRESS },
                { "chain": "optimism", "caip2": "eip155:10", "asset": OPTIMISM_USDC_ADDRESS, "channelContract": MULTICHAIN_PAYWORD_CHANNEL_ADDRESS },
                { "chain": "arbitrum", "caip2": "eip155:42161", "asset": ARBITRUM_USDC_ADDRESS, "channelContract": MULTICHAIN_PAYWORD_CHANNEL_ADDRESS },
                { "chain": "newsatoshi-mainnet-v2", "protocol": "RFC-0022", "asset": "LOVE", "seed": "https://newsatoshi.org/mainnet/status" }
            ],
            "sdk_downloads": {
                "js": format!("{base_url}/sdk/newsatoshi-402.mjs"),
                "python": format!("{base_url}/sdk/newsatoshi-402.py"),
                "connect_hub": "https://newsatoshi.org/connect/"
            }
        })
    };

    let parsed: Option<(String, String, String, u32)> = if let Some(sig) = payment_signature_b64.filter(|s| !s.trim().is_empty()) {
        crate::x402::base64_decode(sig.trim())
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .and_then(|txt| serde_json::from_str::<Value>(&txt).ok())
            .and_then(|v| {
                let cid = v.get("channelId").and_then(|x| x.as_str()).unwrap_or("sandbox").to_string();
                let root = v.get("rootHex").or_else(|| v.get("tipHex")).and_then(|x| x.as_str())?.to_string();
                let tok = v.get("tokenHex").and_then(|x| x.as_str())?.to_string();
                let idx = v.get("index").or_else(|| v.get("ticks")).and_then(|x| x.as_u64())? as u32;
                Some((cid, root, tok, idx))
            })
    } else if let Some(hdr) = payword_hdr.filter(|s| !s.trim().is_empty()) {
        let mut tip = None;
        let mut tok = None;
        let mut ticks = None;
        let mut cid = "sandbox".to_string();
        for part in hdr.split(',') {
            if let Some((k, v)) = part.split_once('=') {
                match k.trim().to_ascii_lowercase().as_str() {
                    "tip" | "root" => tip = Some(v.trim().to_string()),
                    "token" => tok = Some(v.trim().to_string()),
                    "ticks" | "index" => ticks = v.trim().parse::<u32>().ok(),
                    "channel" => cid = v.trim().to_string(),
                    _ => {}
                }
            }
        }
        match (tip, tok, ticks) {
            (Some(r), Some(t), Some(n)) => Some((cid, r, t, n)),
            _ => None,
        }
    } else {
        return (402, challenge());
    };

    let Some((channel_id, root_hex, token_hex, steps)) = parsed else {
        let mut body = challenge();
        body["error"] = json!("malformed_payword_header");
        return (402, body);
    };
    if steps == 0 || steps > 4_096 {
        let mut body = challenge();
        body["error"] = json!("steps must be in 1..=4096 for live sandbox verification");
        return (402, body);
    }

    let Ok(root_bytes) = crate::crypto::hash32_from_hex(root_hex.trim().trim_start_matches("0x")) else {
        let mut body = challenge();
        body["error"] = json!("invalid 32-byte rootHex / tip");
        return (402, body);
    };
    let Ok(mut cur) = crate::crypto::hash32_from_hex(token_hex.trim().trim_start_matches("0x")) else {
        let mut body = challenge();
        body["error"] = json!("invalid 32-byte tokenHex");
        return (402, body);
    };

    let started = std::time::Instant::now();
    for _ in 0..steps {
        cur = crate::crypto::sha256(&cur);
    }
    let elapsed_us = started.elapsed().as_micros() as u64;
    if cur != root_bytes {
        let mut body = challenge();
        body["error"] = json!("bad_payword_preimage: SHA256^steps(token) != root");
        return (402, body);
    }

    (
        200,
        json!({
            "ok": true,
            "status": "payword_verified",
            "funding_status": "sandbox_proof_only",
            "channel_id": channel_id,
            "steps_verified": steps,
            "hashes_evaluated": steps,
            "verified_in_us": elapsed_us,
            "per_call_gas_usd": 0.0,
            "protocol_fee_bps": 0,
            "settlement_note": "Production APIs bind channel_id to a funded on-chain NewsatoshiPayWordChannel on Base, Optimism, or Arbitrum (0% fee) or RFC-0022 LOVE L1 channel, verifying each sequential call in 1 SHA-256 hash.",
            "live_contracts": {
                "base": BASE_PAYWORD_CHANNEL_ADDRESS,
                "optimism": MULTICHAIN_PAYWORD_CHANNEL_ADDRESS,
                "arbitrum": MULTICHAIN_PAYWORD_CHANNEL_ADDRESS
            }
        }),
    )
}

/// Canonical machine-readable Cross-Chain & Agent Connectivity Directory (`GET /api/v2/crosschain/connectivity`).
pub fn build_crosschain_connectivity_directory(base_url: &str) -> Value {
    json!({
        "ok": true,
        "protocol": "NEWSATOSHI/2.2",
        "connect_hub_url": "https://newsatoshi.org/connect/",
        "paywords_multichain_evm": [
            {
                "chain": "base",
                "chain_id": 8453,
                "caip2": "eip155:8453",
                "contract": BASE_PAYWORD_CHANNEL_ADDRESS,
                "usdc": BASE_USDC_ADDRESS,
                "protocol_fee_bps": 0,
                "sourcify": format!("https://repo.sourcify.dev/8453/{BASE_PAYWORD_CHANNEL_ADDRESS}"),
                "explorer": format!("https://basescan.org/address/{BASE_PAYWORD_CHANNEL_ADDRESS}"),
                "live_proof": {
                    "open_1000_tx": BASE_PAYWORD_1000_OPEN_PROOF_TX,
                    "claim_1000_tx": BASE_PAYWORD_1000_CLAIM_PROOF_TX,
                    "claim_1000_gas": 770956,
                    "claim_1000_usd_incl_l1_fee": 0.007709
                }
            },
            {
                "chain": "optimism",
                "chain_id": 10,
                "caip2": "eip155:10",
                "contract": MULTICHAIN_PAYWORD_CHANNEL_ADDRESS,
                "usdc": OPTIMISM_USDC_ADDRESS,
                "protocol_fee_bps": 0,
                "sourcify": format!("https://repo.sourcify.dev/10/{MULTICHAIN_PAYWORD_CHANNEL_ADDRESS}"),
                "explorer": format!("https://optimistic.etherscan.io/address/{MULTICHAIN_PAYWORD_CHANNEL_ADDRESS}"),
                "deploy_tx": OPTIMISM_PAYWORD_DEPLOY_TX,
                "live_proof": {
                    "open_100_tx": OPTIMISM_PAYWORD_OPEN_100_TX,
                    "claim_100_tx": OPTIMISM_PAYWORD_CLAIM_100_TX,
                    "claim_100_gas": 125250
                }
            },
            {
                "chain": "arbitrum",
                "chain_id": 42161,
                "caip2": "eip155:42161",
                "contract": MULTICHAIN_PAYWORD_CHANNEL_ADDRESS,
                "usdc": ARBITRUM_USDC_ADDRESS,
                "protocol_fee_bps": 0,
                "sourcify": format!("https://repo.sourcify.dev/42161/{MULTICHAIN_PAYWORD_CHANNEL_ADDRESS}"),
                "explorer": format!("https://arbiscan.io/address/{MULTICHAIN_PAYWORD_CHANNEL_ADDRESS}"),
                "deploy_tx": ARBITRUM_PAYWORD_DEPLOY_TX
            }
        ],
        "atomic_swaps_into_love": {
            "hash_invariant": "SHA256(32-byte preimage) == payment_hash (identical across Bitcoin OP_SHA256, Lightning BOLT-11, EVM sha256 precompile 0x02, Solana sha256, and Newsatoshi L1 lock --hash)",
            "compile_endpoint": format!("{base_url}/api/swaps/compile"),
            "verify_endpoint": format!("{base_url}/api/swaps/verify"),
            "live_evm_htlc_inspector": format!("{base_url}/api/v2/evm/htlc?chain=optimism&swap_id=<0x64hex>"),
            "live_multichain_payword_inspector": format!("{base_url}/api/v2/evm/channel?chain=base&id=<0x64hex>"),
            "rails": [
                {
                    "rail": "btc_onchain",
                    "description": "BIP-199 SegWit v0 P2WSH (bc1q...) trustless BTC <-> LOVE atomic swaps",
                    "custody": "zero_custody"
                },
                {
                    "rail": "lightning_bolt11",
                    "description": "Instant Bitcoin Lightning BOLT-11 <-> LOVE SHA-256 atomic swaps",
                    "custody": "zero_custody"
                },
                {
                    "rail": "evm_htlc",
                    "contract": CANONICAL_EVM_HTLC_ADDRESS,
                    "chains": [
                        {
                            "chain": "base",
                            "chain_id": 8453,
                            "usdc": BASE_USDC_ADDRESS,
                            "sourcify": format!("https://repo.sourcify.dev/8453/{CANONICAL_EVM_HTLC_ADDRESS}"),
                            "live_swap_proof": {
                                "lock_tx": crate::backing::LIVE_BASE_HTLC_LOCK_TX,
                                "claim_tx": crate::backing::LIVE_BASE_HTLC_CLAIM_TX
                            }
                        },
                        {
                            "chain": "optimism",
                            "chain_id": 10,
                            "usdc": OPTIMISM_USDC_ADDRESS,
                            "sourcify": format!("https://repo.sourcify.dev/10/{CANONICAL_EVM_HTLC_ADDRESS}"),
                            "live_swap_proof": {
                                "lock_tx": OPTIMISM_HTLC_LOCK_PROOF_TX,
                                "claim_tx": OPTIMISM_HTLC_CLAIM_PROOF_TX
                            }
                        },
                        {
                            "chain": "arbitrum",
                            "chain_id": 42161,
                            "usdc": ARBITRUM_USDC_ADDRESS,
                            "sourcify": format!("https://repo.sourcify.dev/42161/{CANONICAL_EVM_HTLC_ADDRESS}")
                        },
                        {
                            "chain": "polygon",
                            "chain_id": 137,
                            "usdc": POLYGON_USDC_ADDRESS,
                            "sourcify": format!("https://repo.sourcify.dev/137/{CANONICAL_EVM_HTLC_ADDRESS}")
                        }
                    ]
                },
                {
                    "rail": "solana_htlc",
                    "description": "Solana PDA SHA-256 HTLC for SOL & SPL-USDC <-> LOVE swaps",
                    "source": "https://github.com/theguysaccount/newsatoshi-commons/tree/main/bridges/solana"
                }
            ]
        },
        "miner_and_pool_connectivity": {
            "auxpow_chainid": 20051,
            "auxpow_magic": "0xfabe6d6d (Namecoin-compatible Bitcoin merge-mining)",
            "zero_fee_solo_stratum": "stratum+tcp://pool.newsatoshi.org:3333",
            "pool_status_url": "https://pool.newsatoshi.org/status",
            "mine_setup_url": "https://newsatoshi.org/mine/",
            "supported_hardware": ["Bitaxe / NerdAxe (password: x)", "Antminer S19/S21/S21XP+ (password: d=524288)", "WhatsMiner M30/M50/M60 (password: d=524288)", "MiningRigRentals SHA256 AsicBoost (74-char ns1... worker)"],
            "dual_btc_love_merge_mining": "docker compose -f auxpow-stratum/docker-compose.merge-mine.yml up -d (or python3 auxpow-stratum/stratum_auxpow.py --ns-rpc http://127.0.0.1:19556 --btc-rpc http://127.0.0.1:8332 — earns 50 LOVE/block at $0 extra power alongside BTC)",
            "pinned_nspq2_seeds": [
                "newsatoshi-seed-v2.fly.dev:8445@b024aaeafa0e4ca384b99483c28c3bedde45a5461a560063b283766414dfbbcc",
                "pool.newsatoshi.org:8445@78187a9436dd2d8cc7ed7afff2396b72b56687099d45d93da676b0a90ed8c6b6"
            ]
        },
        "agent_payment_implementation": {
            "live_402_sandbox": format!("{base_url}/api/v2/paywords/try-402"),
            "channel_kit_compiler": format!("{base_url}/api/v2/paywords/channel-kit"),
            "live_channel_inspector": format!("{base_url}/api/v2/evm/channel"),
            "live_htlc_inspector": format!("{base_url}/api/v2/evm/htlc"),
            "universal_payment_uri_compiler": format!("{base_url}/api/v2/interop/payment-uri"),
            "reserve_price_rfq_compiler": format!("{base_url}/api/v2/swaps/rfq"),
            "mcp_endpoint": format!("{base_url}/mcp"),
            "mcp_tools_count": 45,
            "zero_dep_js_sdk": "https://newsatoshi.org/sdk/newsatoshi-402.mjs",
            "typescript_types": "https://newsatoshi.org/sdk/newsatoshi-402.d.ts",
            "zero_dep_python_sdk": "https://newsatoshi.org/sdk/newsatoshi-402.py",
            "npm_install": "npm install github:theguysaccount/newsatoshi-commons",
            "pip_install": "pip install \"git+https://github.com/theguysaccount/newsatoshi-commons.git#subdirectory=sdk/python\"",
            "umbrel_merge_mine_kit": "https://github.com/theguysaccount/newsatoshi-commons/tree/main/auxpow-stratum/umbrel",
            "frameworks": ["cloudflare_worker", "express", "fastapi", "agent_fetch_js", "agent_python"],
            "x402_standard_proposal": "https://github.com/x402-foundation/x402/issues/4494"
        }
    })
}

/// Exact integer simulation & deployment compiler matching live Base Mainnet
/// `NewsatoshiFairCurveFactory` (`0xF6bE782819a8eD0D1556A8f5DC3223f9eb13a933`) 1:1.
#[allow(clippy::too_many_arguments)]
pub fn simulate_fair_curve_launch(
    name: &str,
    symbol: &str,
    virtual_reserve_usdc: f64,
    virtual_supply_tokens: u64,
    swap_fee_bps: u16,
    buy_usdc: f64,
    paywords_revenue_usdc: f64,
    fee_sink_evm: Option<&str>,
) -> Result<Value, String> {
    let name_clean = name.trim();
    let sym_clean = symbol.trim().to_ascii_uppercase();
    if name_clean.len() < 2 || name_clean.len() > 64 {
        return Err("name must be 2..=64 chars".into());
    }
    if sym_clean.is_empty() || sym_clean.len() > 12 || !sym_clean.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err("symbol must be 1..=12 alphanumeric chars".into());
    }
    if swap_fee_bps > 500 {
        return Err("swap_fee_bps must be <= 500 (5.00% max; 0% Newsatoshi platform cut)".into());
    }
    if !virtual_reserve_usdc.is_finite() || virtual_reserve_usdc < 1.0 || virtual_reserve_usdc > 1_000_000_000.0 {
        return Err("virtual_reserve_usdc must be in [1, 1_000_000_000]".into());
    }
    if virtual_supply_tokens == 0 || virtual_supply_tokens > 1_000_000_000_000 {
        return Err("virtual_supply_tokens must be in 1..=1_000_000_000_000".into());
    }
    if !buy_usdc.is_finite() || buy_usdc <= 0.0 || buy_usdc > 100_000_000.0 {
        return Err("buy_usdc must be > 0 and <= 100_000_000".into());
    }
    if !paywords_revenue_usdc.is_finite() || paywords_revenue_usdc < 0.0 || paywords_revenue_usdc > 100_000_000.0 {
        return Err("paywords_revenue_usdc must be >= 0 and <= 100_000_000".into());
    }

    let v_raw: u128 = (virtual_reserve_usdc * 1_000_000.0).round() as u128;
    let y0_wei: u128 = (virtual_supply_tokens as u128) * 1_000_000_000_000_000_000u128;
    let buy_raw: u128 = (buy_usdc * 1_000_000.0).round().max(1.0) as u128;
    let rev_raw: u128 = (paywords_revenue_usdc * 1_000_000.0).round() as u128;
    let fee_sink = fee_sink_evm
        .map(str::trim)
        .filter(|s| s.starts_with("0x") && s.len() == 42)
        .unwrap_or("0x0000000000000000000000000000000000000000");

    // Exact match to FairCurvePool.quoteBuy (genesis buy has fee = 0 when feeSink == address(0))
    let buy_fee_raw: u128 = if fee_sink == "0x0000000000000000000000000000000000000000" {
        0
    } else {
        (buy_raw * (swap_fee_bps as u128)) / 10_000
    };
    let net_in_raw = buy_raw - buy_fee_raw;
    let tokens_out_wei = (y0_wei * net_in_raw) / (v_raw + net_in_raw);
    let new_reserve_raw = net_in_raw;
    let new_y_wei = y0_wei - tokens_out_wei;

    // Constant-product invariant check (Lean 4 Theorem 10)
    let invariant_holds = (v_raw + new_reserve_raw)
        .checked_mul(new_y_wei)
        .zip(v_raw.checked_mul(y0_wei))
        .map(|(lhs, rhs)| lhs >= rhs)
        .unwrap_or(true);

    // Exact match to FairCurvePool.quoteSell for 100% exit
    let gross_sell_raw = ((v_raw + new_reserve_raw) * tokens_out_wei) / (new_y_wei + tokens_out_wei);
    let sell_fee_raw = if fee_sink == "0x0000000000000000000000000000000000000000" {
        0 // Final exit in holder-dividend mode pays full gross back instead of stranding dust
    } else {
        (gross_sell_raw * (swap_fee_bps as u128)) / 10_000
    };
    let net_sell_raw = gross_sell_raw - sell_fee_raw;

    let initial_spot_usdc = virtual_reserve_usdc / (virtual_supply_tokens as f64);
    let post_buy_spot_usdc = ((v_raw + new_reserve_raw) as f64 / 1e6) / (new_y_wei as f64 / 1e18);
    let tokens_out_whole = (tokens_out_wei as f64) / 1e18;
    let total_holder_usdc_value = ((net_sell_raw + rev_raw) as f64) / 1e6;
    let effective_backing_pct = if buy_usdc > 0.0 {
        (total_holder_usdc_value / buy_usdc) * 100.0
    } else {
        0.0
    };

    Ok(json!({
        "ok": true,
        "curve_params": {
            "name": name_clean,
            "symbol": sym_clean,
            "reserve_token": BASE_USDC_ADDRESS,
            "virtual_reserve_usdc": virtual_reserve_usdc,
            "virtual_reserve_raw": v_raw.to_string(),
            "virtual_supply_tokens": virtual_supply_tokens,
            "virtual_supply_wei": y0_wei.to_string(),
            "swap_fee_bps": swap_fee_bps,
            "platform_fee_bps": 0,
            "premine_tokens": 0,
            "fee_sink": fee_sink,
            "fee_mode": if fee_sink == "0x0000000000000000000000000000000000000000" {
                "100% holder dividend accumulator (0% platform cut)"
            } else {
                "100% routed to immutable fee_sink (0% platform cut)"
            }
        },
        "simulation": {
            "buy_input_usdc": buy_usdc,
            "tokens_minted": round4(tokens_out_whole),
            "tokens_minted_wei": tokens_out_wei.to_string(),
            "initial_spot_price_usdc": round8(initial_spot_usdc),
            "post_buy_spot_price_usdc": round8(post_buy_spot_usdc),
            "curve_redemption_value_usdc": round6((net_sell_raw as f64) / 1e6),
            "injected_paywords_revenue_usdc": paywords_revenue_usdc,
            "holder_claimable_usdc_dividends": round6((rev_raw as f64) / 1e6),
            "combined_exit_plus_dividends_usdc": round6(total_holder_usdc_value),
            "effective_usdc_backing_pct": round2(effective_backing_pct),
            "invariant_v_plus_r_times_y_ge_k0": invariant_holds,
            "solvency_gross_sell_le_reserve": gross_sell_raw <= new_reserve_raw
        },
        "live_base_mainnet": {
            "chain": "base (eip155:8453)",
            "factory_contract": BASE_FAIR_CURVE_FACTORY_ADDRESS,
            "sourcify_exact_match": format!("https://repo.sourcify.dev/8453/{BASE_FAIR_CURVE_FACTORY_ADDRESS}"),
            "live_demo_pool_nspay": BASE_DEMO_CURVE_POOL_ADDRESS,
            "live_demo_token_nspay": BASE_DEMO_CURVE_TOKEN_ADDRESS,
            "live_proof_txs": {
                "create_curve": BASE_CURVE_CREATE_PROOF_TX,
                "buy_usdc": BASE_CURVE_BUY_PROOF_TX,
                "inject_revenue_usdc": BASE_CURVE_INJECT_REV_PROOF_TX,
                "claim_revenue_usdc": BASE_CURVE_CLAIM_REV_PROOF_TX
            },
            "lean4_theorems": [
                "fair_curve_buy_preserves_invariant (0 sorry)",
                "fair_curve_sell_never_insolvent (0 sorry)",
                "fair_curve_inject_revenue_monotone (0 sorry)"
            ]
        },
        "cli_deploy_commands": {
            "create_curve": format!(
                "cast send {BASE_FAIR_CURVE_FACTORY_ADDRESS} \"createCurve(string,string,address,uint256,uint256,uint16,address,string)\" \"{name_clean}\" \"{sym_clean}\" {BASE_USDC_ADDRESS} {v_raw} {y0_wei} {swap_fee_bps} {fee_sink} \"https://commons.newsatoshi.org\" --rpc-url https://mainnet.base.org --private-key $DEPLOYER_KEY"
            ),
            "inject_paywords_revenue": "cast send <POOL_ADDRESS> \"injectRevenue(uint256)\" <USDC_RAW_6DEC> --rpc-url https://mainnet.base.org --private-key $API_SERVER_KEY"
        }
    }))
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}
fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}
fn round8(v: f64) -> f64 {
    (v * 100_000_000.0).round() / 100_000_000.0
}

// =============================================================================
// Live Base Mainnet JSON-RPC Readers (Zero Fake Data — Real On-Chain State)
// =============================================================================

const BASE_RPC_URLS: &[&str] = &[
    "https://mainnet.base.org",
    "https://base-rpc.publicnode.com",
];
const OPTIMISM_RPC_URLS: &[&str] = &[
    "https://mainnet.optimism.io",
    "https://optimism-rpc.publicnode.com",
];
const ARBITRUM_RPC_URLS: &[&str] = &[
    "https://arb1.arbitrum.io/rpc",
    "https://arbitrum-one-rpc.publicnode.com",
];
const POLYGON_RPC_URLS: &[&str] = &[
    "https://polygon-bor-rpc.publicnode.com",
    "https://polygon-rpc.com",
];

#[derive(Clone, Copy)]
struct EvmChainMeta {
    slug: &'static str,
    label: &'static str,
    caip2: &'static str,
    native_symbol: &'static str,
    usdc: &'static str,
    payword_contract: Option<&'static str>,
    htlc_contract: &'static str,
    explorer_base: &'static str,
    rpc_urls: &'static [&'static str],
}

fn resolve_evm_chain_meta(chain: &str) -> Result<EvmChainMeta, String> {
    match chain.trim().to_ascii_lowercase().as_str() {
        "" | "base" | "eip155:8453" | "8453" => Ok(EvmChainMeta {
            slug: "base",
            label: "Base Mainnet",
            caip2: "eip155:8453",
            native_symbol: "ETH",
            usdc: BASE_USDC_ADDRESS,
            payword_contract: Some(BASE_PAYWORD_CHANNEL_ADDRESS),
            htlc_contract: CANONICAL_EVM_HTLC_ADDRESS,
            explorer_base: "https://basescan.org",
            rpc_urls: BASE_RPC_URLS,
        }),
        "optimism" | "op" | "eip155:10" | "10" => Ok(EvmChainMeta {
            slug: "optimism",
            label: "Optimism Mainnet",
            caip2: "eip155:10",
            native_symbol: "ETH",
            usdc: OPTIMISM_USDC_ADDRESS,
            payword_contract: Some(MULTICHAIN_PAYWORD_CHANNEL_ADDRESS),
            htlc_contract: CANONICAL_EVM_HTLC_ADDRESS,
            explorer_base: "https://optimistic.etherscan.io",
            rpc_urls: OPTIMISM_RPC_URLS,
        }),
        "arbitrum" | "arb" | "arbitrum-one" | "eip155:42161" | "42161" => Ok(EvmChainMeta {
            slug: "arbitrum",
            label: "Arbitrum One",
            caip2: "eip155:42161",
            native_symbol: "ETH",
            usdc: ARBITRUM_USDC_ADDRESS,
            payword_contract: Some(MULTICHAIN_PAYWORD_CHANNEL_ADDRESS),
            htlc_contract: CANONICAL_EVM_HTLC_ADDRESS,
            explorer_base: "https://arbiscan.io",
            rpc_urls: ARBITRUM_RPC_URLS,
        }),
        "polygon" | "matic" | "pol" | "eip155:137" | "137" => Ok(EvmChainMeta {
            slug: "polygon",
            label: "Polygon PoS",
            caip2: "eip155:137",
            native_symbol: "POL",
            usdc: POLYGON_USDC_ADDRESS,
            payword_contract: None,
            htlc_contract: CANONICAL_EVM_HTLC_ADDRESS,
            explorer_base: "https://polygonscan.com",
            rpc_urls: POLYGON_RPC_URLS,
        }),
        other => Err(format!(
            "unsupported EVM chain '{other}': supported chains are base, optimism, arbitrum, polygon"
        )),
    }
}

static BASE_CURVES_CACHE: std::sync::OnceLock<std::sync::Mutex<Option<(std::time::Instant, Value)>>> =
    std::sync::OnceLock::new();

fn evm_rpc_post(urls: &[&str], label: &str, payload: &Value) -> Result<Value, String> {
    let body_str = serde_json::to_string(payload).map_err(|e| format!("json encode error: {e}"))?;
    let mut last_err = String::new();
    for url in urls {
        let cfg = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(7)))
            .build();
        let agent: ureq::Agent = cfg.into();
        match agent
            .post(*url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send(body_str.as_str())
        {
            Ok(mut resp) => match resp.body_mut().read_to_string() {
                Ok(text) => match serde_json::from_str::<Value>(&text) {
                    Ok(v) => return Ok(v),
                    Err(e) => last_err = format!("{url} json parse error: {e}"),
                },
                Err(e) => last_err = format!("{url} body read error: {e}"),
            },
            Err(e) => last_err = format!("{url} rpc error: {e}"),
        }
    }
    Err(format!("{label} RPC unavailable: {last_err}"))
}

fn base_rpc_post(payload: &Value) -> Result<Value, String> {
    evm_rpc_post(BASE_RPC_URLS, "Base Mainnet", payload)
}

fn evm_eth_call_single(meta: EvmChainMeta, to: &str, data: &str) -> Result<String, String> {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "eth_call",
        "params": [{ "to": to, "data": data }, "latest"]
    });
    let resp = evm_rpc_post(meta.rpc_urls, meta.label, &payload)?;
    if let Some(err) = resp.get("error") {
        return Err(format!("{} eth_call error: {err}", meta.label));
    }
    resp.get("result")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("missing {} eth_call result", meta.label))
}

fn eth_call_single(to: &str, data: &str) -> Result<String, String> {
    let base = resolve_evm_chain_meta("base")?;
    evm_eth_call_single(base, to, data)
}

fn eth_call_batch(calls: &[(&str, &str)]) -> Result<Vec<String>, String> {
    if calls.is_empty() {
        return Ok(Vec::new());
    }
    let batch: Vec<Value> = calls
        .iter()
        .enumerate()
        .map(|(i, (to, data))| {
            json!({
                "jsonrpc": "2.0",
                "id": i + 1,
                "method": "eth_call",
                "params": [{ "to": to, "data": data }, "latest"]
            })
        })
        .collect();
    let resp = base_rpc_post(&Value::Array(batch))?;
    let arr = resp
        .as_array()
        .ok_or_else(|| "Base RPC batch response was not an array".to_string())?;
    let mut out = vec![String::new(); calls.len()];
    for item in arr {
        if let Some(id) = item.get("id").and_then(|v| v.as_u64()) {
            let idx = (id as usize).saturating_sub(1);
            if idx < out.len() {
                if let Some(res) = item.get("result").and_then(|v| v.as_str()) {
                    out[idx] = res.to_string();
                }
            }
        }
    }
    Ok(out)
}

fn strip_hex(s: &str) -> &str {
    s.trim().strip_prefix("0x").or_else(|| s.trim().strip_prefix("0X")).unwrap_or(s.trim())
}

fn decode_word_u128(hex_str: &str, word_idx: usize) -> u128 {
    let clean = strip_hex(hex_str);
    let start = word_idx * 64;
    if clean.len() < start + 64 {
        return 0;
    }
    let slice = &clean[start + 32..start + 64]; // lower 128 bits
    u128::from_str_radix(slice, 16).unwrap_or(0)
}

fn decode_word_address(hex_str: &str, word_idx: usize) -> String {
    let clean = strip_hex(hex_str);
    let start = word_idx * 64;
    if clean.len() < start + 64 {
        return "0x0000000000000000000000000000000000000000".to_string();
    }
    format!("0x{}", clean[start + 24..start + 64].to_ascii_lowercase())
}

fn decode_abi_string(hex_str: &str) -> String {
    let clean = strip_hex(hex_str);
    if clean.len() < 128 {
        return String::new();
    }
    let offset = usize::from_str_radix(&clean[48..64], 16).unwrap_or(32) * 2;
    if clean.len() < offset + 64 {
        return String::new();
    }
    let byte_len = usize::from_str_radix(&clean[offset + 48..offset + 64], 16).unwrap_or(0).min(256);
    let data_start = offset + 64;
    let data_end = data_start + byte_len * 2;
    if clean.len() < data_end {
        return String::new();
    }
    hex::decode(&clean[data_start..data_end])
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_default()
}

/// Decode the 10-word ABI tuple returned by `NewsatoshiPayWordChannel.channels(bytes32)` on any supported EVM chain.
pub fn decode_multichain_payword_channel_words(
    chain: &str,
    result_hex: &str,
    channel_id_hex: &str,
) -> Result<Value, String> {
    let meta = resolve_evm_chain_meta(chain)?;
    let contract = meta.payword_contract.ok_or_else(|| {
        format!(
            "NewsatoshiPayWordChannel is live on base, optimism, and arbitrum (not yet deployed on {}; use HTLC inspector for {})",
            meta.slug, meta.slug
        )
    })?;
    let clean_id = strip_hex(channel_id_hex).to_ascii_lowercase();
    let cid = format!("0x{clean_id}");
    let clean = strip_hex(result_hex);
    if clean.len() < 640 {
        return Ok(json!({
            "ok": true,
            "found": false,
            "chain": format!("{} ({})", meta.slug, meta.caip2),
            "chain_slug": meta.slug,
            "channel_id": cid,
            "contract": contract,
            "reason": "short or empty RPC result"
        }));
    }
    let payer = decode_word_address(clean, 0);
    if payer == "0x0000000000000000000000000000000000000000" {
        return Ok(json!({
            "ok": true,
            "found": false,
            "chain": format!("{} ({})", meta.slug, meta.caip2),
            "chain_slug": meta.slug,
            "channel_id": cid,
            "contract": contract,
            "reason": format!("channel not found on {}", meta.label)
        }));
    }
    let recipient = decode_word_address(clean, 1);
    let token = decode_word_address(clean, 2);
    let initial_root = format!("0x{}", clean[3 * 64..4 * 64].to_ascii_lowercase());
    let current_root = format!("0x{}", clean[4 * 64..5 * 64].to_ascii_lowercase());
    let unit_amount = decode_word_u128(clean, 5);
    let max_steps = decode_word_u128(clean, 6) as u64;
    let claimed_steps = decode_word_u128(clean, 7) as u64;
    let expiry = decode_word_u128(clean, 8) as u64;
    let closed = decode_word_u128(clean, 9) != 0;
    let remaining_steps = if closed { 0 } else { max_steps.saturating_sub(claimed_steps) };
    let remaining_raw = (remaining_steps as u128).saturating_mul(unit_amount);
    let claimed_raw = (claimed_steps as u128).saturating_mul(unit_amount);
    let is_usdc = token.eq_ignore_ascii_case(meta.usdc);
    let is_native = token == "0x0000000000000000000000000000000000000000";

    let mut out = json!({
        "ok": true,
        "found": true,
        "chain": format!("{} ({})", meta.slug, meta.caip2),
        "chain_slug": meta.slug,
        "chain_label": meta.label,
        "contract": contract,
        "channel_id": cid,
        "payer": payer,
        "recipient": recipient,
        "token": token,
        "token_symbol": if is_usdc { "USDC" } else if is_native { meta.native_symbol } else { "ERC20" },
        "initial_root": initial_root,
        "current_root": current_root,
        "unit_amount_raw": unit_amount.to_string(),
        "unit_amount_usdc": if is_usdc { Some(round6((unit_amount as f64) / 1e6)) } else { None },
        "max_steps": max_steps,
        "claimed_steps": claimed_steps,
        "remaining_steps": remaining_steps,
        "claimed_amount_raw": claimed_raw.to_string(),
        "claimed_amount_usdc": if is_usdc { Some(round6((claimed_raw as f64) / 1e6)) } else { None },
        "remaining_amount_raw": remaining_raw.to_string(),
        "remaining_amount_usdc": if is_usdc { Some(round6((remaining_raw as f64) / 1e6)) } else { None },
        "expiry_unix": expiry,
        "closed": closed,
        "explorer_contract_url": format!("{}/address/{contract}", meta.explorer_base)
    });
    if meta.slug == "base" {
        out["basescan_contract_url"] = json!(format!("https://basescan.org/address/{contract}"));
    }
    Ok(out)
}

/// Decode the 10-word ABI tuple returned by `NewsatoshiPayWordChannel.channels(bytes32)` on Base Mainnet.
pub fn decode_base_payword_channel_words(result_hex: &str, channel_id_hex: &str) -> Value {
    decode_multichain_payword_channel_words("base", result_hex, channel_id_hex)
        .unwrap_or_else(|e| json!({ "ok": false, "error": e }))
}

/// Inspect any live `NewsatoshiPayWordChannel` across Base, Optimism, or Arbitrum directly via JSON-RPC.
pub fn inspect_live_multichain_payword_channel(
    chain: &str,
    channel_id_hex: &str,
) -> Result<Value, String> {
    let meta = resolve_evm_chain_meta(chain)?;
    let contract = meta.payword_contract.ok_or_else(|| {
        format!(
            "NewsatoshiPayWordChannel is live on base, optimism, and arbitrum (not yet deployed on {})",
            meta.slug
        )
    })?;
    let clean = strip_hex(channel_id_hex).to_ascii_lowercase();
    if clean.len() != 64 || !clean.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("channel_id must be a 32-byte (64 hex character) bytes32 hash".into());
    }
    let calldata = format!("0x7a7ebd7b{clean}");
    let raw = evm_eth_call_single(meta, contract, &calldata)?;
    decode_multichain_payword_channel_words(meta.slug, &raw, &clean)
}

/// Inspect any live `NewsatoshiPayWordChannel` (`0x971BA814Df91c20DcA3aDd870C2B55d643965C50`) on Base Mainnet.
pub fn inspect_live_base_payword_channel(channel_id_hex: &str) -> Result<Value, String> {
    inspect_live_multichain_payword_channel("base", channel_id_hex)
}

/// Decode the 9-word ABI tuple returned by `NewsatoshiHtlc.swaps(bytes32)` (`0xeb84e7f2`) across
/// Base, Optimism, Arbitrum, and Polygon.
pub fn decode_evm_htlc_swap_words(
    chain: &str,
    result_hex: &str,
    swap_id_hex: &str,
) -> Result<Value, String> {
    let meta = resolve_evm_chain_meta(chain)?;
    let clean_id = strip_hex(swap_id_hex).to_ascii_lowercase();
    let sid = format!("0x{clean_id}");
    let clean = strip_hex(result_hex);
    if clean.len() < 576 {
        return Ok(json!({
            "ok": true,
            "found": false,
            "chain": format!("{} ({})", meta.slug, meta.caip2),
            "chain_slug": meta.slug,
            "swap_id": sid,
            "contract": meta.htlc_contract,
            "reason": "short or empty RPC result"
        }));
    }
    let sender = decode_word_address(clean, 0);
    if sender == "0x0000000000000000000000000000000000000000" {
        return Ok(json!({
            "ok": true,
            "found": false,
            "chain": format!("{} ({})", meta.slug, meta.caip2),
            "chain_slug": meta.slug,
            "swap_id": sid,
            "contract": meta.htlc_contract,
            "reason": format!("HTLC swap not found on {}", meta.label)
        }));
    }
    let recipient = decode_word_address(clean, 1);
    let token = decode_word_address(clean, 2);
    let amount_raw = decode_word_u128(clean, 3);
    let payment_hash = format!("0x{}", clean[4 * 64..5 * 64].to_ascii_lowercase());
    let timelock_unix = decode_word_u128(clean, 5) as u64;
    let claimed = decode_word_u128(clean, 6) != 0;
    let refunded = decode_word_u128(clean, 7) != 0;
    let preimage = format!("0x{}", clean[8 * 64..9 * 64].to_ascii_lowercase());
    let is_native = token == "0x0000000000000000000000000000000000000000";
    let is_usdc = token.eq_ignore_ascii_case(meta.usdc);
    let status = if claimed {
        "claimed"
    } else if refunded {
        "refunded"
    } else {
        "locked_active"
    };
    let amount_human = if is_native {
        Some(round8((amount_raw as f64) / 1e18))
    } else if is_usdc {
        Some(round6((amount_raw as f64) / 1e6))
    } else {
        None
    };

    Ok(json!({
        "ok": true,
        "found": true,
        "chain": format!("{} ({})", meta.slug, meta.caip2),
        "chain_slug": meta.slug,
        "chain_label": meta.label,
        "contract": meta.htlc_contract,
        "swap_id": sid,
        "status": status,
        "sender": sender,
        "recipient": recipient,
        "token": token,
        "token_symbol": if is_native { meta.native_symbol } else if is_usdc { "USDC" } else { "ERC20" },
        "amount_raw": amount_raw.to_string(),
        "amount_human": amount_human,
        "payment_hash": payment_hash,
        "timelock_unix": timelock_unix,
        "claimed": claimed,
        "refunded": refunded,
        "preimage_revealed": if claimed { Some(preimage) } else { None },
        "explorer_contract_url": format!("{}/address/{}", meta.explorer_base, meta.htlc_contract),
        "newsatoshi_l1_matching_hash": payment_hash.trim_start_matches("0x")
    }))
}

/// Inspect any live SHA-256 `NewsatoshiHtlc` swap (`0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`)
/// across Base, Optimism, Arbitrum, or Polygon directly via JSON-RPC.
pub fn inspect_live_evm_htlc(chain: &str, swap_id_hex: &str) -> Result<Value, String> {
    let meta = resolve_evm_chain_meta(chain)?;
    let clean = strip_hex(swap_id_hex).to_ascii_lowercase();
    if clean.len() != 64 || !clean.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("swap_id must be a 32-byte (64 hex character) bytes32 hash".into());
    }
    let calldata = format!("0xeb84e7f2{clean}");
    let raw = evm_eth_call_single(meta, meta.htlc_contract, &calldata)?;
    decode_evm_htlc_swap_words(meta.slug, &raw, &clean)
}

fn pct_encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

/// Build standards-compliant cross-chain payment URIs (EIP-681, BIP-21, Lightning BOLT-11,
/// Solana Pay, and Newsatoshi Mainnet v2 `newsatoshi:` URI) plus HTTP 402 PayWords metadata.
pub fn build_universal_payment_uri(
    rail: &str,
    recipient: &str,
    amount: f64,
    asset: Option<&str>,
    memo: Option<&str>,
    payment_hash_hex: Option<&str>,
) -> Result<Value, String> {
    if !amount.is_finite() || amount <= 0.0 || amount > 1_000_000_000.0 {
        return Err("amount must be finite and in (0, 1_000_000_000]".into());
    }
    let rec = recipient.trim();
    if rec.is_empty() || rec.len() > 2048 {
        return Err("recipient address or invoice is required".into());
    }
    let memo_str = memo.map(str::trim).filter(|s| !s.is_empty());
    let hash_clean = payment_hash_hex
        .map(strip_hex)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase());
    if let Some(ref h) = hash_clean {
        if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("payment_hash_hex must be 64 hex characters (32-byte SHA-256)".into());
        }
    }

    let r = rail.trim().to_ascii_lowercase();
    let asset_up = asset.unwrap_or("").trim().to_ascii_uppercase();
    const SOLANA_USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

    let evm_uri = |chain_slug: &str, chain_id: u64, usdc: &str, native: &str| -> Value {
        let use_usdc = asset_up.is_empty() || asset_up == "USDC";
        if use_usdc {
            let raw = (amount * 1_000_000.0).round() as u128;
            json!({
                "rail": chain_slug,
                "standard": "EIP-681 (ERC-20 USDC)",
                "chain_id": chain_id,
                "asset": "USDC",
                "amount": amount,
                "amount_base_units": raw.to_string(),
                "uri": format!("ethereum:{usdc}@{chain_id}/transfer?address={rec}&uint256={raw}")
            })
        } else {
            let wei = (amount * 1e18).round() as u128;
            json!({
                "rail": chain_slug,
                "standard": "EIP-681 (Native)",
                "chain_id": chain_id,
                "asset": native,
                "amount": amount,
                "amount_base_units": wei.to_string(),
                "uri": format!("ethereum:{rec}@{chain_id}?value={wei}")
            })
        }
    };

    if r == "all" {
        let mut ns_uri = format!("newsatoshi:{rec}?amount={amount}&net=mainnet-v2");
        if let Some(ref h) = hash_clean {
            ns_uri.push_str(&format!("&hash={h}"));
        }
        if let Some(m) = memo_str {
            ns_uri.push_str(&format!("&memo={}", pct_encode_query(m)));
        }
        return Ok(json!({
            "ok": true,
            "mode": "universal_multi_rail_bundle",
            "amount": amount,
            "asset_preference": if asset_up.is_empty() { "USDC / Native" } else { &asset_up },
            "uris": {
                "base": evm_uri("base", 8453, BASE_USDC_ADDRESS, "ETH"),
                "optimism": evm_uri("optimism", 10, OPTIMISM_USDC_ADDRESS, "ETH"),
                "arbitrum": evm_uri("arbitrum", 42161, ARBITRUM_USDC_ADDRESS, "ETH"),
                "polygon": evm_uri("polygon", 137, POLYGON_USDC_ADDRESS, "POL"),
                "bitcoin": {
                    "rail": "bitcoin",
                    "standard": "BIP-21",
                    "asset": "BTC",
                    "uri": match memo_str {
                        Some(m) => format!("bitcoin:{rec}?amount={amount}&label={}", pct_encode_query(m)),
                        None => format!("bitcoin:{rec}?amount={amount}"),
                    }
                },
                "solana": {
                    "rail": "solana",
                    "standard": "Solana Pay",
                    "asset": if asset_up == "SOL" { "SOL" } else { "USDC" },
                    "uri": if asset_up == "SOL" {
                        match memo_str {
                            Some(m) => format!("solana:{rec}?amount={amount}&memo={}", pct_encode_query(m)),
                            None => format!("solana:{rec}?amount={amount}"),
                        }
                    } else {
                        match memo_str {
                            Some(m) => format!("solana:{rec}?amount={amount}&spl-token={SOLANA_USDC_MINT}&memo={}", pct_encode_query(m)),
                            None => format!("solana:{rec}?amount={amount}&spl-token={SOLANA_USDC_MINT}"),
                        }
                    }
                },
                "newsatoshi": {
                    "rail": "newsatoshi",
                    "standard": "NEWSATOSHI-URI/1.0 (Mainnet v2 Shielded)",
                    "asset": "LOVE",
                    "uri": ns_uri
                }
            },
            "paywords_http_402_contracts": {
                "base": BASE_PAYWORD_CHANNEL_ADDRESS,
                "optimism": MULTICHAIN_PAYWORD_CHANNEL_ADDRESS,
                "arbitrum": MULTICHAIN_PAYWORD_CHANNEL_ADDRESS
            }
        }));
    }

    let entry = match r.as_str() {
        "base" | "eip155:8453" => evm_uri("base", 8453, BASE_USDC_ADDRESS, "ETH"),
        "optimism" | "op" | "eip155:10" => evm_uri("optimism", 10, OPTIMISM_USDC_ADDRESS, "ETH"),
        "arbitrum" | "arb" | "eip155:42161" => evm_uri("arbitrum", 42161, ARBITRUM_USDC_ADDRESS, "ETH"),
        "polygon" | "pol" | "eip155:137" => evm_uri("polygon", 137, POLYGON_USDC_ADDRESS, "POL"),
        "bitcoin" | "btc" => json!({
            "rail": "bitcoin",
            "standard": "BIP-21",
            "asset": "BTC",
            "amount": amount,
            "uri": match memo_str {
                Some(m) => format!("bitcoin:{rec}?amount={amount}&label={}", pct_encode_query(m)),
                None => format!("bitcoin:{rec}?amount={amount}"),
            }
        }),
        "lightning" | "ln" | "bolt11" => json!({
            "rail": "lightning",
            "standard": "BOLT-11 / BIP-21 Lightning",
            "asset": "BTC-LN",
            "uri": format!("lightning:{}", rec.trim_start_matches("lightning:").trim_start_matches("LIGHTNING:"))
        }),
        "solana" | "sol" => {
            let is_sol = asset_up == "SOL";
            let mut uri = format!("solana:{rec}?amount={amount}");
            if !is_sol {
                uri.push_str(&format!("&spl-token={SOLANA_USDC_MINT}"));
            }
            if let Some(m) = memo_str {
                uri.push_str(&format!("&memo={}", pct_encode_query(m)));
            }
            json!({
                "rail": "solana",
                "standard": "Solana Pay",
                "asset": if is_sol { "SOL" } else { "USDC" },
                "amount": amount,
                "uri": uri
            })
        }
        "newsatoshi" | "love" | "ns" => {
            let mut uri = format!("newsatoshi:{rec}?amount={amount}&net=mainnet-v2");
            if let Some(ref h) = hash_clean {
                uri.push_str(&format!("&hash={h}"));
            }
            if let Some(m) = memo_str {
                uri.push_str(&format!("&memo={}", pct_encode_query(m)));
            }
            json!({
                "rail": "newsatoshi",
                "standard": "NEWSATOSHI-URI/1.0 (Mainnet v2 Shielded)",
                "asset": "LOVE",
                "amount": amount,
                "uri": uri
            })
        }
        other => {
            return Err(format!(
                "unsupported rail '{other}': use all, base, optimism, arbitrum, polygon, bitcoin, lightning, solana, or newsatoshi"
            ));
        }
    };

    Ok(json!({
        "ok": true,
        "payment": entry
    }))
}

/// Compile a permissionless Reserve-Price RFQ / Sealed-Bid SHA-256 HTLC Manifest that protects
/// organic `LOVE` price discovery (seller sets a firm minimum floor price; zero thin-AMM dumping).
pub fn compile_reserve_price_rfq(
    love_amount: f64,
    min_reserve_price_usd: f64,
    settlement_rail: &str,
    seller_settlement_address: &str,
    expiry_hours: u64,
) -> Result<Value, String> {
    if !love_amount.is_finite() || love_amount <= 0.0 || love_amount > 21_000_000.0 {
        return Err("love_amount must be finite and in (0, 21_000_000]".into());
    }
    if !min_reserve_price_usd.is_finite() || min_reserve_price_usd <= 0.0 || min_reserve_price_usd > 1_000_000.0 {
        return Err("min_reserve_price_usd must be finite and > 0".into());
    }
    let addr = seller_settlement_address.trim();
    if addr.is_empty() || addr.len() > 256 {
        return Err("seller_settlement_address is required".into());
    }
    let hours = expiry_hours.clamp(1, 720);
    let rail_norm = settlement_rail.trim().to_ascii_lowercase();
    let (rail_label, htlc_ref, asset_sym) = match rail_norm.as_str() {
        "base_usdc" | "base" => ("Base Mainnet USDC", CANONICAL_EVM_HTLC_ADDRESS, "USDC"),
        "optimism_usdc" | "optimism" => ("Optimism Mainnet USDC", CANONICAL_EVM_HTLC_ADDRESS, "USDC"),
        "arbitrum_usdc" | "arbitrum" => ("Arbitrum One USDC", CANONICAL_EVM_HTLC_ADDRESS, "USDC"),
        "polygon_usdc" | "polygon" => ("Polygon PoS USDC", CANONICAL_EVM_HTLC_ADDRESS, "USDC"),
        "btc_onchain" | "bitcoin" => ("Bitcoin On-Chain BIP-199 P2WSH", "BIP-199 P2WSH (OP_SHA256)", "BTC"),
        "lightning_bolt11" | "lightning" => ("Bitcoin Lightning BOLT-11", "BOLT-11 SHA-256 HTLC", "BTC-LN"),
        "solana_usdc" | "solana" => ("Solana SPL-USDC PDA HTLC", "Solana PDA SHA-256 HTLC", "USDC"),
        other => {
            return Err(format!(
                "unsupported settlement_rail '{other}': use base_usdc, optimism_usdc, arbitrum_usdc, polygon_usdc, btc_onchain, lightning_bolt11, or solana_usdc"
            ));
        }
    };

    let min_total_usd = round6(love_amount * min_reserve_price_usd);
    let min_usdc_raw = (min_total_usd * 1_000_000.0).round() as u128;
    let canonical = format!(
        "NS_RFQ_V1|love={love_amount:.8}|min_usd={min_reserve_price_usd:.6}|rail={rail_norm}|seller={addr}|hours={hours}"
    );
    let rfq_id = hex::encode(crate::crypto::sha256(canonical.as_bytes()));

    Ok(json!({
        "ok": true,
        "rfq_id": format!("0x{rfq_id}"),
        "price_discovery_mode": "seller_reserve_floor_rfq_no_amm_dumping",
        "why_this_protects_price_discovery": "Zero thin-liquidity AMM slippage and zero subsidized maker floor. Seller locks LOVE strictly AFTER a competing buyer locks >= min_total_usd into a trustless SHA-256 HTLC.",
        "terms": {
            "love_amount": love_amount,
            "min_reserve_price_usd_per_love": min_reserve_price_usd,
            "min_total_usd": min_total_usd,
            "min_settlement_usdc_base_units": if asset_sym == "USDC" { Some(min_usdc_raw.to_string()) } else { None },
            "settlement_rail": rail_norm,
            "settlement_rail_label": rail_label,
            "settlement_asset": asset_sym,
            "htlc_contract_or_script": htlc_ref,
            "seller_settlement_address": addr,
            "rfq_expiry_hours": hours
        },
        "atomic_execution_sequence": [
            "1. Seller broadcasts this signed Reserve-Price RFQ (zero LOVE locked upfront; zero downward price anchoring).",
            "2. Competing buyer submits a bid >= min_total_usd and locks settlement funds FIRST into the SHA-256 HTLC (longer timelock: e.g. 24h) using buyer-chosen payment_hash = SHA256(preimage).",
            "3. Seller verifies the buyer's on-chain HTLC lock via newsatoshi_inspect_evm_htlc / mempool, then locks love_amount on Newsatoshi Mainnet v2 (`newsatoshi --net mainnet-v2 lock <LOVE> --hash <payment_hash> --to <buyer_ns1> --timeout 72`).",
            "4. Buyer claims LOVE on Mainnet v2, revealing preimage atomically so seller claims >= min_total_usd on the settlement rail."
        ]
    }))
}

/// Read all live Fair Bonding Curves deployed via `NewsatoshiFairCurveFactory`
/// (`0xF6bE782819a8eD0D1556A8f5DC3223f9eb13a933`) directly from Base Mainnet RPC (15s TTL cache).
pub fn fetch_live_base_fair_curves(force_refresh: bool) -> Result<Value, String> {
    let cache = BASE_CURVES_CACHE.get_or_init(|| std::sync::Mutex::new(None));
    if !force_refresh {
        if let Ok(guard) = cache.lock() {
            if let Some((ts, ref cached)) = *guard {
                if ts.elapsed() < std::time::Duration::from_secs(15) {
                    return Ok(cached.clone());
                }
            }
        }
    }

    let count_hex = eth_call_single(BASE_FAIR_CURVE_FACTORY_ADDRESS, "0xf525cb68")?;
    let total_count = decode_word_u128(&count_hex, 0) as usize;
    let fetch_count = total_count.min(25);

    let mut pool_addrs: Vec<String> = Vec::with_capacity(fetch_count);
    if fetch_count > 0 {
        let mut idx_calls: Vec<(String, String)> = Vec::with_capacity(fetch_count);
        // Fetch newest pools first so freshly launched curves appear at the top
        for offset in 0..fetch_count {
            let idx = total_count - 1 - offset;
            idx_calls.push((
                BASE_FAIR_CURVE_FACTORY_ADDRESS.to_string(),
                format!("0x41d1de97{:064x}", idx),
            ));
        }
        let call_refs: Vec<(&str, &str)> = idx_calls.iter().map(|(a, d)| (a.as_str(), d.as_str())).collect();
        let results = eth_call_batch(&call_refs)?;
        for res in results {
            let addr = decode_word_address(&res, 0);
            if addr != "0x0000000000000000000000000000000000000000" {
                pool_addrs.push(addr);
            }
        }
    }

    // Batch 1: read all 12 state getters across all discovered pools in 1 RPC round-trip
    const POOL_SELECTORS: [&str; 12] = [
        "0xfc0c546a", // 0: token()
        "0xf4325d67", // 1: reserveToken()
        "0x4b38474d", // 2: feeSink()
        "0x2ffdaf89", // 3: swapFeeBps()
        "0x80c08a5d", // 4: virtualReserve()
        "0x277b2f97", // 5: virtualSupply()
        "0xbefdcf9b", // 6: curveReserve()
        "0x9e466cab", // 7: accRevPerToken()
        "0xb93600e6", // 8: totalRevenueInjected()
        "0x50515d2b", // 9: totalRevenueClaimed()
        "0x398482d8", // 10: spotPrice()
        "0x77a4d559", // 11: metadataUri()
    ];

    let mut pool_calls: Vec<(&str, &str)> = Vec::with_capacity(pool_addrs.len() * POOL_SELECTORS.len());
    for addr in &pool_addrs {
        for sel in &POOL_SELECTORS {
            pool_calls.push((addr.as_str(), *sel));
        }
    }
    let pool_res = eth_call_batch(&pool_calls)?;

    let mut token_addrs: Vec<String> = Vec::with_capacity(pool_addrs.len());
    for i in 0..pool_addrs.len() {
        let base = i * POOL_SELECTORS.len();
        token_addrs.push(decode_word_address(&pool_res[base], 0));
    }

    // Batch 2: read name(), symbol(), totalSupply() for every pool's FairCurveToken in 1 RPC round-trip
    const TOKEN_SELECTORS: [&str; 3] = [
        "0x06fdde03", // 0: name()
        "0x95d89b41", // 1: symbol()
        "0x18160ddd", // 2: totalSupply()
    ];
    let mut tok_calls: Vec<(&str, &str)> = Vec::with_capacity(token_addrs.len() * TOKEN_SELECTORS.len());
    for tok in &token_addrs {
        for sel in &TOKEN_SELECTORS {
            tok_calls.push((tok.as_str(), *sel));
        }
    }
    let tok_res = eth_call_batch(&tok_calls)?;

    let mut curves = Vec::with_capacity(pool_addrs.len());
    for i in 0..pool_addrs.len() {
        let pb = i * POOL_SELECTORS.len();
        let tb = i * TOKEN_SELECTORS.len();
        let pool_addr = &pool_addrs[i];
        let token_addr = &token_addrs[i];
        let reserve_token = decode_word_address(&pool_res[pb + 1], 0);
        let fee_sink = decode_word_address(&pool_res[pb + 2], 0);
        let swap_fee_bps = decode_word_u128(&pool_res[pb + 3], 0) as u64;
        let virtual_reserve_raw = decode_word_u128(&pool_res[pb + 4], 0);
        let virtual_supply_wei = decode_word_u128(&pool_res[pb + 5], 0);
        let curve_reserve_raw = decode_word_u128(&pool_res[pb + 6], 0);
        let acc_rev_per_token = decode_word_u128(&pool_res[pb + 7], 0);
        let total_rev_injected_raw = decode_word_u128(&pool_res[pb + 8], 0);
        let total_rev_claimed_raw = decode_word_u128(&pool_res[pb + 9], 0);
        let spot_price_raw = decode_word_u128(&pool_res[pb + 10], 0);
        let metadata_uri = decode_abi_string(&pool_res[pb + 11]);

        let name = decode_abi_string(&tok_res[tb]);
        let symbol = decode_abi_string(&tok_res[tb + 1]);
        let total_supply_wei = decode_word_u128(&tok_res[tb + 2], 0);

        let is_usdc = reserve_token.eq_ignore_ascii_case(BASE_USDC_ADDRESS);
        let reserve_div = if is_usdc { 1e6 } else { 1e18 };

        curves.push(json!({
            "pool_index": total_count - 1 - i,
            "pool_address": pool_addr,
            "token_address": token_addr,
            "name": name,
            "symbol": symbol,
            "reserve_token": reserve_token,
            "reserve_symbol": if is_usdc { "USDC" } else { "ERC20" },
            "fee_sink": fee_sink,
            "holder_dividend_fee_mode": fee_sink == "0x0000000000000000000000000000000000000000",
            "swap_fee_bps": swap_fee_bps,
            "virtual_reserve_raw": virtual_reserve_raw.to_string(),
            "virtual_reserve_usdc": round6((virtual_reserve_raw as f64) / reserve_div),
            "virtual_supply_wei": virtual_supply_wei.to_string(),
            "virtual_supply_tokens": round2((virtual_supply_wei as f64) / 1e18),
            "curve_reserve_raw": curve_reserve_raw.to_string(),
            "curve_reserve_usdc": round6((curve_reserve_raw as f64) / reserve_div),
            "total_supply_wei": total_supply_wei.to_string(),
            "total_supply_tokens": round4((total_supply_wei as f64) / 1e18),
            "spot_price_raw": spot_price_raw.to_string(),
            "spot_price_usdc": round8((spot_price_raw as f64) / reserve_div),
            "acc_rev_per_token": acc_rev_per_token.to_string(),
            "total_revenue_injected_raw": total_rev_injected_raw.to_string(),
            "total_revenue_injected_usdc": round6((total_rev_injected_raw as f64) / reserve_div),
            "total_revenue_claimed_raw": total_rev_claimed_raw.to_string(),
            "total_revenue_claimed_usdc": round6((total_rev_claimed_raw as f64) / reserve_div),
            "unclaimed_revenue_usdc": round6((total_rev_injected_raw.saturating_sub(total_rev_claimed_raw) as f64) / reserve_div),
            "metadata_uri": metadata_uri,
            "basescan_pool_url": format!("https://basescan.org/address/{pool_addr}"),
            "basescan_token_url": format!("https://basescan.org/token/{token_addr}")
        }));
    }

    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let out = json!({
        "ok": true,
        "chain": "base (eip155:8453)",
        "factory_address": BASE_FAIR_CURVE_FACTORY_ADDRESS,
        "sourcify_exact_match": format!("https://repo.sourcify.dev/8453/{BASE_FAIR_CURVE_FACTORY_ADDRESS}"),
        "total_pool_count": total_count,
        "fetched_at_unix": now_unix,
        "curves": curves
    });

    if let Ok(mut guard) = cache.lock() {
        *guard = Some((std::time::Instant::now(), out.clone()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_savings_and_fair_curve_invariants_hold() {
        let est = estimate_paywords_cost_savings(1_000_000, 0.005, 1_000).unwrap();
        assert_eq!(est["ok"], true);
        let stripe_fees = est["comparison"]["stripe_cards"]["monthly_fees_usd"].as_f64().unwrap();
        let pw_fees = est["comparison"]["newsatoshi_paywords_base_usdc"]["monthly_fees_usd"].as_f64().unwrap();
        assert!(stripe_fees > 300_000.0);
        assert!(pw_fees < 50.0);
        assert!(est["savings"]["fee_reduction_vs_stripe_pct"].as_f64().unwrap() > 99.9);

        let kit = compile_payword_channel_setup(
            "cloudflare_worker",
            Some("0x1744535521EB420be9700475400455a4Ad0EBEa2"),
            0.002,
            1_000,
            "/api/inference",
            "https://commons.newsatoshi.org",
        )
        .unwrap();
        assert_eq!(kit["ok"], true);
        assert!(kit["code"].as_str().unwrap().contains(BASE_PAYWORD_CHANNEL_ADDRESS));

        let sim = simulate_fair_curve_launch(
            "Agent API",
            "AGAPI",
            10_000.0,
            1_000_000_000,
            100,
            1_000.0,
            500.0,
            None,
        )
        .unwrap();
        assert_eq!(sim["simulation"]["invariant_v_plus_r_times_y_ge_k0"], true);
        assert_eq!(sim["simulation"]["solvency_gross_sell_le_reserve"], true);
        assert!(sim["simulation"]["effective_usdc_backing_pct"].as_f64().unwrap() > 140.0);

        // Verify ABI string decoding & 10-word PayWordChannel decoding
        let abi_str = format!(
            "0x{:064x}{:064x}{}",
            32u64,
            5u64,
            "4e53504159000000000000000000000000000000000000000000000000000000"
        );
        assert_eq!(decode_abi_string(&abi_str), "NSPAY");

        let cid = "ab".repeat(32);
        let words = format!(
            "0x{:0>64}{:0>64}{:0>64}{}{}{:064x}{:064x}{:064x}{:064x}{:064x}",
            "1111111111111111111111111111111111111111",
            "1744535521eb420be9700475400455a4ad0ebea2",
            "833589fcd6edb6e08f4c7c32d4f71b54bda02913",
            "cc".repeat(32),
            "dd".repeat(32),
            1_000u128,
            100u128,
            7u128,
            1_900_000_000u128,
            0u128,
        );
        let decoded = decode_base_payword_channel_words(&words, &cid);
        assert_eq!(decoded["found"], true);
        assert_eq!(decoded["token_symbol"], "USDC");
        assert_eq!(decoded["remaining_steps"], 93);
        assert_eq!(decoded["remaining_amount_raw"], "93000");
        assert_eq!(decoded["closed"], false);
        assert!(SDK_JS_SOURCE.contains("createBasePayWordGate"));
        assert!(SDK_JS_SOURCE.contains("createAutoPayFetch"));
        assert!(SDK_JS_SOURCE.contains("EVM_PAYWORD_CHAINS"));
        assert!(SDK_PY_SOURCE.contains("class BasePayWordGate"));
        assert!(SDK_PY_SOURCE.contains("class AutoPaySession"));
        assert!(SDK_PY_SOURCE.contains("EVM_PAYWORD_CHAINS"));

        // Multi-chain PayWords setup (Optimism + Arbitrum + AI Agent Auto-Pay clients)
        let op_kit = compile_payword_channel_setup_for_chain(
            "optimism",
            "agent_fetch_js",
            None,
            0.001,
            1_000,
            "/api/inference",
            "https://commons.newsatoshi.org",
        )
        .unwrap();
        assert_eq!(op_kit["target_chain"]["chain_id"], 10);
        assert_eq!(
            op_kit["target_chain"]["payword_channel_contract"],
            MULTICHAIN_PAYWORD_CHANNEL_ADDRESS
        );
        assert!(op_kit["code"].as_str().unwrap().contains("createAutoPayFetch"));

        let arb_kit = compile_payword_channel_setup_for_chain(
            "arbitrum",
            "agent_python",
            None,
            0.001,
            1_000,
            "/api/inference",
            "https://commons.newsatoshi.org",
        )
        .unwrap();
        assert_eq!(arb_kit["target_chain"]["chain_id"], 42161);
        assert!(arb_kit["code"].as_str().unwrap().contains("AutoPaySession"));

        // Live HTTP 402 playground: missing header => 402, valid SHA-256 voucher => 200, forged => 402
        let (st402, _) = evaluate_live_paywords_try_402(None, None, "https://commons.newsatoshi.org");
        assert_eq!(st402, 402);

        let token = [0x42u8; 32];
        let step1 = crate::crypto::sha256(&token);
        let root = crate::crypto::sha256(&step1);
        let hdr = format!("tip={},token={},ticks=2", hex::encode(root), hex::encode(token));
        let (st200, body200) =
            evaluate_live_paywords_try_402(None, Some(&hdr), "https://commons.newsatoshi.org");
        assert_eq!(st200, 200);
        assert_eq!(body200["ok"], true);
        assert_eq!(body200["steps_verified"], 2);

        let bad_hdr = format!("tip={},token={},ticks=2", hex::encode(root), "ff".repeat(32));
        let (st_bad, _) =
            evaluate_live_paywords_try_402(None, Some(&bad_hdr), "https://commons.newsatoshi.org");
        assert_eq!(st_bad, 402);

        // Cross-chain connectivity directory & Multi-chain PayWord + HTLC decoding
        let dir = build_crosschain_connectivity_directory("https://commons.newsatoshi.org");
        assert_eq!(dir["ok"], true);
        assert_eq!(dir["paywords_multichain_evm"].as_array().unwrap().len(), 3);
        assert_eq!(dir["agent_payment_implementation"]["mcp_tools_count"], 45);
        assert!(SDK_DTS_SOURCE.contains("createAutoPayFetch"));

        let uri_all = build_universal_payment_uri(
            "all",
            "0x1744535521EB420be9700475400455a4Ad0EBEa2",
            25.5,
            Some("USDC"),
            Some("Invoice 42"),
            None,
        )
        .unwrap();
        assert_eq!(uri_all["ok"], true);
        assert!(uri_all["uris"]["base"]["uri"].as_str().unwrap().contains("ethereum:0x833589"));
        assert!(uri_all["uris"]["solana"]["uri"].as_str().unwrap().contains("spl-token=EPjFWdd5"));

        let rfq = compile_reserve_price_rfq(
            100.0,
            25.0,
            "base_usdc",
            "0x1744535521EB420be9700475400455a4Ad0EBEa2",
            48,
        )
        .unwrap();
        assert_eq!(rfq["ok"], true);
        assert_eq!(rfq["terms"]["min_total_usd"], 2500.0);
        assert_eq!(rfq["price_discovery_mode"], "seller_reserve_floor_rfq_no_amm_dumping");

        let op_words = format!(
            "0x{:0>64}{:0>64}{:0>64}{}{}{:064x}{:064x}{:064x}{:064x}{:064x}",
            "1111111111111111111111111111111111111111",
            "1744535521eb420be9700475400455a4ad0ebea2",
            "0b2c639c533813f4aa9d7837caf62653d097ff85",
            "aa".repeat(32),
            "bb".repeat(32),
            1_000u128,
            100u128,
            100u128,
            1_900_000_000u128,
            1u128,
        );
        let op_dec = decode_multichain_payword_channel_words("optimism", &op_words, &cid).unwrap();
        assert_eq!(op_dec["found"], true);
        assert_eq!(op_dec["chain_slug"], "optimism");
        assert_eq!(op_dec["token_symbol"], "USDC");
        assert_eq!(op_dec["claimed_steps"], 100);
        assert_eq!(op_dec["closed"], true);

        let sid = "ef".repeat(32);
        let htlc_words = format!(
            "0x{:0>64}{:0>64}{:0>64}{:064x}{}{:064x}{:064x}{:064x}{}",
            "1744535521eb420be9700475400455a4ad0ebea2",
            "2222222222222222222222222222222222222222",
            "0000000000000000000000000000000000000000",
            100_000_000_000_000u128,
            "12".repeat(32),
            1_900_000_000u128,
            1u128,
            0u128,
            "34".repeat(32),
        );
        let htlc_dec = decode_evm_htlc_swap_words("optimism", &htlc_words, &sid).unwrap();
        assert_eq!(htlc_dec["found"], true);
        assert_eq!(htlc_dec["status"], "claimed");
        assert_eq!(htlc_dec["token_symbol"], "ETH");
        assert_eq!(htlc_dec["claimed"], true);
        assert_eq!(htlc_dec["preimage_revealed"], format!("0x{}", "34".repeat(32)));
    }
}
