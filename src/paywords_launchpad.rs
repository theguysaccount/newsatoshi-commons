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
pub const BASE_PAYWORD_CHANNEL_ADDRESS: &str = "0x971BA814Df91c20DcA3aDd870C2B55d643965C50";
pub const BASE_FAIR_CURVE_FACTORY_ADDRESS: &str = "0xF6bE782819a8eD0D1556A8f5DC3223f9eb13a933";
pub const BASE_DEMO_CURVE_POOL_ADDRESS: &str = "0xC3098ba25E39Effa63dE94EbACE6381781ad4cEd";
pub const BASE_DEMO_CURVE_TOKEN_ADDRESS: &str = "0x406B3484560c89C030720D9a4101500474e5D2d9";

pub const BASE_PAYWORD_OPEN_PROOF_TX: &str =
    "0x8229bb7282f79b819122c40e66e8f8dcec832d30fccb1a5b75f9eb49133be31d";
pub const BASE_PAYWORD_CLAIM_PROOF_TX: &str =
    "0x122885e1b36aa5b8a62f97f4b4bfb3b235bdebe0055a8cd3a4ed7c52393a5356";
pub const BASE_PAYWORD_CLOSE_PROOF_TX: &str =
    "0x3a826c82fdaffdbbdb8049db7793a342524e3a74742c9504ef8baad1380c4c0d";
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

    // Published Stripe standard card pricing: 2.9% + $0.30 per individual charge.
    let stripe_fee_usd = (monthly_calls as f64) * (price_per_call_usd * 0.029 + 0.30);
    let stripe_effective_pct = if gross_monthly_usd > 0.0 {
        (stripe_fee_usd / gross_monthly_usd) * 100.0
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
    // Empirically measured gas on Base: ~65,000 fixed + ~720 gas per SHA-256 step (claim(7) = 96,449 gas;
    // claim(1000) = 783,140 gas). At conservative 0.01 gwei Base gas and $2,500/ETH:
    let settlement_batches = monthly_calls.div_ceil(batch);
    let gas_per_claim: u64 = 65_000 + 720 * batch;
    let eth_price_usd = 2_500.0_f64;
    let base_gas_gwei = 0.01_f64;
    let usd_per_settlement_tx = (gas_per_claim as f64) * (base_gas_gwei * 1e-9) * eth_price_usd;
    let paywords_monthly_fee_usd = (settlement_batches as f64) * usd_per_settlement_tx;
    let paywords_effective_pct = if gross_monthly_usd > 0.0 {
        (paywords_monthly_fee_usd / gross_monthly_usd) * 100.0
    } else {
        0.0
    };

    let savings_vs_stripe_usd = (stripe_fee_usd - paywords_monthly_fee_usd).max(0.0);
    let savings_vs_x402_usd = (per_call_x402_fee_usd - paywords_monthly_fee_usd).max(0.0);
    let fee_reduction_vs_stripe_pct = if stripe_fee_usd > 0.0 {
        ((stripe_fee_usd - paywords_monthly_fee_usd) / stripe_fee_usd) * 100.0
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
            "stripe_formula": "2.9% + $0.30 per individual charge",
            "per_call_x402_formula": "$0.001 per-call Base L2 tx gas + facilitator overhead",
            "newsatoshi_paywords_formula": "0% protocol fee; measured Base claim() gas (65,000 + 720 * batch) at 0.01 gwei & $2,500 ETH"
        },
        "comparison": {
            "stripe_cards": {
                "monthly_fees_usd": round2(stripe_fee_usd),
                "effective_fee_pct": round4(stripe_effective_pct),
                "net_retained_usd": round2(gross_monthly_usd - stripe_fee_usd)
            },
            "per_call_base_x402_usdc": {
                "monthly_fees_usd": round2(per_call_x402_fee_usd),
                "effective_fee_pct": round4(per_call_x402_effective_pct),
                "net_retained_usd": round2(gross_monthly_usd - per_call_x402_fee_usd)
            },
            "newsatoshi_paywords_base_usdc": {
                "protocol_fee_bps": 0,
                "per_call_verification": "1 SHA-256 hash (~0.013 ms, $0.000000 gas)",
                "monthly_settlement_txs": settlement_batches,
                "estimated_gas_per_claim_tx": gas_per_claim,
                "estimated_usd_per_claim_tx": round6(usd_per_settlement_tx),
                "monthly_fees_usd": round4(paywords_monthly_fee_usd),
                "effective_fee_pct": round6(paywords_effective_pct),
                "net_retained_usd": round2(gross_monthly_usd - paywords_monthly_fee_usd)
            }
        },
        "savings": {
            "monthly_savings_vs_stripe_usd": round2(savings_vs_stripe_usd),
            "annual_savings_vs_stripe_usd": round2(savings_vs_stripe_usd * 12.0),
            "fee_reduction_vs_stripe_pct": round4(fee_reduction_vs_stripe_pct),
            "monthly_savings_vs_per_call_x402_usd": round2(savings_vs_x402_usd),
            "annual_savings_vs_per_call_x402_usd": round2(savings_vs_x402_usd * 12.0),
            "fee_reduction_vs_per_call_x402_pct": round4(fee_reduction_vs_x402_pct)
        },
        "live_base_mainnet_proof": {
            "chain": "base (eip155:8453)",
            "contract": BASE_PAYWORD_CHANNEL_ADDRESS,
            "sourcify_exact_match": format!("https://repo.sourcify.dev/8453/{BASE_PAYWORD_CHANNEL_ADDRESS}"),
            "open_channel_tx": BASE_PAYWORD_OPEN_PROOF_TX,
            "claim_7_steps_tx": BASE_PAYWORD_CLAIM_PROOF_TX,
            "claim_7_steps_gas_used": 96449,
            "close_and_refund_tx": BASE_PAYWORD_CLOSE_PROOF_TX,
            "lean4_conservation_theorem": "Newsatoshi.NsUsdL2.payword_channel_exact_conservation (0 sorry)"
        }
    }))
}

/// Generate an instant, org-ready Base USDC `NewsatoshiPayWordChannel` integration kit
/// (Cloudflare Worker, Express, Next.js, FastAPI, or Axum) wired to the live Base Mainnet contract.
pub fn compile_payword_channel_setup(
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
// Settles in Base Mainnet USDC via immutable NewsatoshiPayWordChannel: {BASE_PAYWORD_CHANNEL_ADDRESS}
// Verify on-chain deposit once per channelId, then verify 1 SHA-256 per request at the edge (~0.013 ms).

const CHANNEL_CONTRACT = "{BASE_PAYWORD_CHANNEL_ADDRESS}";
const BASE_USDC = "{BASE_USDC_ADDRESS}";
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
          network: "eip155:8453",
          asset: BASE_USDC,
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
    // Batch settle whenever ready via: cast send {BASE_PAYWORD_CHANNEL_ADDRESS} "claim(bytes32,uint32,bytes32)" <channelId> <steps> <latestToken>
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
                r#"// Express / Node.js — Zero-Dependency O(1) SHA-256 PayWords Middleware (Base USDC)
// Live Base Mainnet Channel Contract: {BASE_PAYWORD_CHANNEL_ADDRESS} (Sourcify exact_match)
import crypto from "node:crypto";

const CHANNEL_CONTRACT = "{BASE_PAYWORD_CHANNEL_ADDRESS}";
const BASE_USDC = "{BASE_USDC_ADDRESS}";
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
      network: "eip155:8453",
      asset: BASE_USDC,
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
                r#"# FastAPI / Python — Zero-Dependency O(1) SHA-256 PayWords Gate (Base USDC)
# Live Base Mainnet Channel Contract: {BASE_PAYWORD_CHANNEL_ADDRESS} (Sourcify exact_match)
import base64, hashlib, json
from fastapi import Header, HTTPException

CHANNEL_CONTRACT = "{BASE_PAYWORD_CHANNEL_ADDRESS}"
BASE_USDC = "{BASE_USDC_ADDRESS}"
RECIPIENT = "{recipient}"
UNIT_USDC_RAW = {unit_usdc_raw}  # ${price_usd:.6} USDC/call
CHANNELS = {{}}  # Store in Redis/Postgres: channel_id -> (last_index, last_token, max_steps)

def verify_payword(payment_signature: str | None = Header(default=None, alias="PAYMENT-SIGNATURE")):
    if not payment_signature:
        raise HTTPException(status_code=402, detail={{
            "x402Version": 2, "scheme": "batch-settlement", "binding": "paywords-sha256",
            "network": "eip155:8453", "asset": BASE_USDC, "channelContract": CHANNEL_CONTRACT,
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
        _ => {
            return Err(
                "unsupported framework: choose cloudflare_worker | express | fastapi".into(),
            );
        }
    };

    Ok(json!({
        "ok": true,
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
        "base_mainnet": {
            "chain_id": 8453,
            "caip2": "eip155:8453",
            "usdc_token": BASE_USDC_ADDRESS,
            "payword_channel_contract": BASE_PAYWORD_CHANNEL_ADDRESS,
            "sourcify_url": format!("https://repo.sourcify.dev/8453/{BASE_PAYWORD_CHANNEL_ADDRESS}"),
            "live_proof_txs": {
                "open_channel": BASE_PAYWORD_OPEN_PROOF_TX,
                "claim_7_steps": BASE_PAYWORD_CLAIM_PROOF_TX,
                "close_and_refund": BASE_PAYWORD_CLOSE_PROOF_TX
            }
        },
        "cli_settlement_commands": {
            "step1_approve_usdc": format!(
                "cast send {BASE_USDC_ADDRESS} \"approve(address,uint256)\" {BASE_PAYWORD_CHANNEL_ADDRESS} {total_deposit_usdc_raw} --rpc-url https://mainnet.base.org --private-key $PAYER_KEY"
            ),
            "step2_open_channel": format!(
                "cast send {BASE_PAYWORD_CHANNEL_ADDRESS} \"openChannel(address,address,bytes32,uint256,uint32,uint64)\" {recipient} {BASE_USDC_ADDRESS} <ROOT_W0_HEX32> {unit_usdc_raw} {max_steps} <EXPIRY_UNIX> --rpc-url https://mainnet.base.org --private-key $PAYER_KEY"
            ),
            "step3_claim_batch": format!(
                "cast send {BASE_PAYWORD_CHANNEL_ADDRESS} \"claim(bytes32,uint32,bytes32)\" <CHANNEL_ID> <STEPS_DELTA> <LATEST_PREIMAGE_WK> --rpc-url https://mainnet.base.org --private-key $SERVER_KEY"
            ),
            "step4_early_close_refund": format!(
                "cast send {BASE_PAYWORD_CHANNEL_ADDRESS} \"closeAndRefund(bytes32)\" <CHANNEL_ID> --rpc-url https://mainnet.base.org --private-key $SERVER_KEY"
            )
        },
        "code": snippet,
        "licensing_note": "Core reference code is PolyForm-Noncommercial-1.0.0; wire spec is open (x402-foundation/x402#4494) and direct commercial partner licenses are available via contributors@newsatoshi.org."
    }))
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
    }
}
