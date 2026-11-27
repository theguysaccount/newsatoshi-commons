//! AI Agent Transaction Interface: Streamable HTTP MCP Server (`/mcp`),
//! HTTP `402 Payment Required` PayWord + L1 Hash-Lock Verification, and
//! Standard Agent Discovery Surfaces (`/llms.txt`, `/llms-full.txt`,
//! `/.well-known/agent.json`, `/.well-known/mcp.json`, `/openapi.json`, `/robots.txt`).
//!
//! Zero fake data: all network heights, balances, lock verifications, and STARK proofs
//! come strictly from live Newsatoshi L1 nodes and the persisted Commons engine.

use crate::backing::{fetch_live_backing_summary, BackingProofSummary, CANONICAL_BACKING_VAULT};
use crate::crypto::{hash32_from_hex, sha256};
use crate::crypto_tools::{
    decode_bolt11_tool, fetch_bitcoin_mempool_oracle, inspect_evm_contract, quote_base_uniswap_v3,
    verify_tx_receipt,
};
use crate::seed::{SeedSnapshot, SeedStatusClient, AUXPOW_CHAIN_ID, MAINNET_GENESIS_HASH, TESTNET_GENESIS_HASH};
use crate::stark::verify_covenant_stark;
use crate::storage::{ApiStateView, CommonsEngine};
use crate::swap::{compile_atomic_swap, verify_swap_on_chain, CompileSwapRequest};
use crate::web4::{compile_web4_charter, render_erc8004_registration, SpawnCharterRequest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

/// Maximum cumulative PayWord ticks per channel (matches JS/Python SDKs and protocol docs).
pub const MAX_PAYWORD_VERIFY_TICKS: u64 = 1_000_000;
/// Hard per-request delta bound to prevent uncached CPU DoS on a single HTTP request.
pub const MAX_PAYWORD_STEP_JUMP: u64 = 100_000;

/// Default canonical public URL used in discovery documents when Host header is unavailable.
pub const DEFAULT_PUBLIC_BASE_URL: &str = "https://commons.newsatoshi.org";

// ---------------------------------------------------------------------------
// HTTP 402 PayWord + L1 Hash-Lock Machine-to-Machine Micropayments
// (Rivest & Shamir 1996 PayWord hash-chains + L1 SHA-256 HTLC locks)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayWordVerifyResult {
    pub ok: bool,
    pub valid: bool,
    pub scheme: String,
    pub verification_mode: String,
    pub hashes_evaluated: u64,
    pub tip_hex: String,
    pub token_hex: String,
    pub computed_tip_hex: String,
    pub ticks: u64,
    pub unit_price_base_units: u64,
    pub total_paid_base_units: u64,
    pub next_required_ticks: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Verifies a post-quantum pure-SHA-256 PayWord voucher from the initial commitment:
/// checks `SHA256^{ticks}(token) == tip` (delegates to `verify_payword_token_incremental`).
pub fn verify_payword_token(
    tip_hex: &str,
    token_hex: &str,
    ticks: u64,
    unit_price_base_units: u64,
) -> Result<PayWordVerifyResult, String> {
    verify_payword_token_incremental(tip_hex, token_hex, ticks, unit_price_base_units, None, 0)
}

/// Incremental O(delta) Rivest–Shamir (1996) PayWord verifier:
/// - When `prev_token_hex` and `prev_ticks > 0` are provided, verifies
///   `SHA256^(ticks - prev_ticks)(token) == prev_token` (exactly 1 SHA-256 hash per sequential call,
///   O(1) all the way up to 1,000,000 cumulative ticks).
/// - Otherwise verifies `SHA256^ticks(token) == tip` (up to `MAX_PAYWORD_STEP_JUMP` in one jump).
pub fn verify_payword_token_incremental(
    tip_hex: &str,
    token_hex: &str,
    ticks: u64,
    unit_price_base_units: u64,
    prev_token_hex: Option<&str>,
    prev_ticks: u64,
) -> Result<PayWordVerifyResult, String> {
    if ticks == 0 || ticks > MAX_PAYWORD_VERIFY_TICKS {
        return Err(format!(
            "ticks must be between 1 and {MAX_PAYWORD_VERIFY_TICKS}"
        ));
    }
    let expected_tip = hash32_from_hex(tip_hex)?;
    let raw_token = hash32_from_hex(token_hex)?;
    let (anchor, iterations, mode) = match (prev_token_hex, prev_ticks) {
        (Some(prev_hex), pt) if pt > 0 => {
            if ticks <= pt {
                return Err(format!(
                    "ticks ({ticks}) must exceed previous watermark ({pt})"
                ));
            }
            let delta = ticks - pt;
            if delta > MAX_PAYWORD_STEP_JUMP {
                return Err(format!(
                    "single-call tick jump ({delta}) exceeds anti-DoS cap ({MAX_PAYWORD_STEP_JUMP})"
                ));
            }
            (hash32_from_hex(prev_hex)?, delta, "incremental")
        }
        _ => {
            if ticks > MAX_PAYWORD_STEP_JUMP {
                return Err(format!(
                    "uncached first-call jump ({ticks}) exceeds single-call cap ({MAX_PAYWORD_STEP_JUMP}); advance incrementally up to {MAX_PAYWORD_VERIFY_TICKS}"
                ));
            }
            (expected_tip, ticks, "full")
        }
    };

    let mut cur = raw_token;
    for _ in 0..iterations {
        cur = sha256(&cur);
    }
    let valid = cur == anchor;
    let total_paid = ticks
        .checked_mul(unit_price_base_units)
        .ok_or_else(|| "total_paid_base_units overflow".to_string())?;
    Ok(PayWordVerifyResult {
        ok: true,
        valid,
        scheme: "NEWSATOSHI-PAYWORD-SHA256/1.0".to_string(),
        verification_mode: mode.to_string(),
        hashes_evaluated: iterations,
        tip_hex: hex::encode(expected_tip),
        token_hex: hex::encode(raw_token),
        computed_tip_hex: if mode == "full" {
            hex::encode(cur)
        } else {
            hex::encode(expected_tip)
        },
        ticks,
        unit_price_base_units,
        total_paid_base_units: if valid { total_paid } else { 0 },
        next_required_ticks: if valid { ticks + 1 } else { ticks },
        error: if valid {
            None
        } else if mode == "incremental" {
            Some("PayWord incremental mismatch: SHA256^(ticks - prev_ticks)(token) != prev_token".to_string())
        } else {
            Some("PayWord hash-chain mismatch: SHA256^ticks(token) != tip".to_string())
        },
    })
}

/// Builds a canonical HTTP `402 Payment Required` dual-rail challenge payload
/// (supporting both Newsatoshi L1 SHA-256 hash-locks and off-chain PayWord micropayments).
pub fn build_http_402_challenge(
    resource: &str,
    amount_love: &str,
    network: &str,
    recipient_address: &str,
    payword_unit_price_base_units: u64,
) -> Value {
    let ts = unix_now();
    let net = if network.eq_ignore_ascii_case("testnet") {
        "testnet"
    } else {
        "mainnet"
    };
    let challenge_seed = format!("NEWSATOSHI-402:{net}:{resource}:{amount_love}:{recipient_address}:{ts}");
    let payment_hash = hex::encode(sha256(challenge_seed.as_bytes()));
    let recipient = if recipient_address.trim().is_empty() {
        "<RECIPIENT_NSD1_OR_NS1_ADDRESS>"
    } else {
        recipient_address.trim()
    };
    json!({
        "status": 402,
        "protocol": "NEWSATOSHI-402/1.0",
        "resource": resource,
        "network": net,
        "amount_love": amount_love,
        "payment_hash_hex": payment_hash,
        "issued_at_unix": ts,
        "rails": {
            "l1_hash_lock": {
                "method": "newsatoshi_lock",
                "cli_command": format!("newsatoshi --net {net} lock {amount_love} --hash {payment_hash} --to {recipient} --blocks 60"),
                "verify_endpoint": "/api/agent/402-verify",
                "header_format": format!("X-Newsatoshi-Payment: rail=l1_lock,network={net},hash={payment_hash},preimage=<64hex>")
            },
            "payword_stream": {
                "method": "sha256_hash_chain",
                "unit_price_base_units": payword_unit_price_base_units,
                "invariant": "SHA256^ticks(token) == tip (zero gas, sub-millisecond verification, pure post-quantum SHA-256)",
                "verify_endpoint": "/api/agent/402-verify",
                "header_format": "X-Newsatoshi-PayWord: tip=<64hex>,token=<64hex>,ticks=<u64>"
            }
        }
    })
}

/// Embedded installable agent skill served directly at `/skills/newsatoshi-agent-payments/SKILL.md`.
pub const AGENT_SKILL_MARKDOWN: &str =
    include_str!("../skills/newsatoshi-agent-payments/SKILL.md");

/// Embedded 1-line POSIX CLI installer served at `/install.sh`.
pub const INSTALL_SCRIPT_SH: &str = include_str!("../scripts/install.sh");

/// Embedded NEWSATOSHI/2.1 companion whitepaper (Markdown & IACR ePrint LaTeX) + Lean 4 formal kernel.
pub const WHITEPAPER_MARKDOWN: &str = include_str!("../docs/NEWSATOSHI-2.1-WHITEPAPER.md");
pub const WHITEPAPER_V2_2_MARKDOWN: &str = include_str!("../docs/NEWSATOSHI-2.2-WHITEPAPER.md");
pub const WHITEPAPER_V2_2_PDF: &[u8] = include_bytes!("../docs/newsatoshi-v2.2.pdf");
pub const MANIFESTO_MARKDOWN: &str = include_str!("../docs/MANIFESTO.md");
pub const BUG_BOUNTY_MARKDOWN: &str = include_str!("../docs/BUG_BOUNTY.md");
pub const NSUSD_RFC_MARKDOWN: &str = include_str!("../docs/RFC-0028-NSUSD-SHIELDED-DOLLAR.md");
pub const WHITEPAPER_LATEX: &str = include_str!("../docs/newsatoshi_2_1_whitepaper.tex");
pub const LEAN_FORMAL_PROOF: &str = include_str!("../formal/NewsatoshiCovenants.lean");
pub const LEAN_NSUSD_L2_PROOF: &str = include_str!("../formal/NsUsdAndL2Perfection.lean");

/// Computes strictly real, live derived network & AuxPoW merge-mining metrics from
/// live Newsatoshi Mainnet and Testnet `/status` snapshots (zero fabricated figures).
pub fn compute_network_metrics(seed: &SeedSnapshot, hashrate_ths: f64) -> Value {
    let clamped_ths = if hashrate_ths.is_finite() && hashrate_ths > 0.0 {
        hashrate_ths.clamp(0.001, 1_000_000.0)
    } else {
        1.2
    };
    // Consensus schedule (`node/src/params.rs`): Bitcoin-style halvings to 20,917,968.75 LOVE by
    // halving 8, then perpetual `SUBSIDY_FLOOR = 0.1953125 LOVE/block` tail emission (no hard cap).
    let era8_supply_love = 20_917_968.75f64;
    let tail_subsidy_love = 0.195_312_5f64;
    let block_reward_love = 50.0f64;
    let two_pow_32 = 4_294_967_296.0f64;

    let derive_chain = |st: &crate::seed::SeedStatus| -> Value {
        let issued_love = (st.issued as f64) / 100_000_000.0;
        let era8_mined_pct = (issued_love / era8_supply_love) * 100.0;
        let recent_count = st.recent.len();
        let merge_mined_count = st.recent.iter().filter(|b| b.merge_mined).count();
        let merge_mined_pct = if recent_count > 0 {
            (merge_mined_count as f64 / recent_count as f64) * 100.0
        } else {
            0.0
        };
        let observed_avg_block_secs = if recent_count >= 2 {
            let newest = st.recent[0].time;
            let oldest = st.recent[recent_count - 1].time;
            if newest > oldest {
                Some((newest - oldest) as f64 / ((recent_count - 1) as f64))
            } else {
                None
            }
        } else {
            None
        };
        let hashes_per_block = st.difficulty.max(1e-9) * two_pow_32;
        let est_hashrate_hs_600s = hashes_per_block / 600.0;
        let est_hashrate_hs_observed = observed_avg_block_secs.map(|dt| hashes_per_block / dt.max(1.0));
        let net_hs = est_hashrate_hs_observed.unwrap_or(est_hashrate_hs_600s);

        let miner_hs = clamped_ths * 1_000_000_000_000.0;
        let steady_state_daily_emission_love = 144.0 * block_reward_love; // 144 blocks/day * 50 LOVE = 7,200 LOVE/day
        let post_asert_share = miner_hs / (net_hs + miner_hs);
        let post_asert_love_per_day = post_asert_share * steady_state_daily_emission_love;

        json!({
            "network": st.network,
            "height": st.height,
            "difficulty": st.difficulty,
            "bits": st.bits,
            "auxpow_chainid": st.auxpow_chainid,
            "peers": st.peers,
            "locks": st.locks,
            "notes": st.notes,
            "nullifiers": st.nullifiers,
            "mempool": st.mempool,
            "issued_love": issued_love,
            "hard_cap": false,
            "era8_supply_love": era8_supply_love,
            "era8_mined_pct": era8_mined_pct,
            "tail_subsidy_love_per_block": tail_subsidy_love,
            "supply_schedule": "Bitcoin-like halvings to 20,917,968.75 LOVE by halving 8, then perpetual 0.1953125 LOVE/block tail emission (~10,266 LOVE/yr, <0.05%/yr and falling; no hard cap).",
            "block_reward_love": block_reward_love,
            "steady_state_daily_emission_love": steady_state_daily_emission_love,
            "recent_blocks_sampled": recent_count,
            "merge_mined_pct": merge_mined_pct,
            "observed_avg_block_secs": observed_avg_block_secs,
            "estimated_network_hashrate_hs_nominal_600s": est_hashrate_hs_600s,
            "estimated_network_hashrate_hs_observed": est_hashrate_hs_observed,
            "auxpow_yield_post_asert": {
                "miner_hashrate_ths": clamped_ths,
                "post_asert_network_share_pct": post_asert_share * 100.0,
                "expected_love_per_day_post_asert": post_asert_love_per_day,
                "steady_state_global_love_per_day": steady_state_daily_emission_love,
                "asert_note": "Computed as miner_hashrate / (live_network_hashrate + miner_hashrate) * 7,200 LOVE/day steady-state emission after ASERT retargeting (zero extra electricity when merge-mining Bitcoin)."
            }
        })
    };

    json!({
        "ok": true,
        "fetched_at_unix": seed.fetched_at_unix,
        "auxpow_chainid": AUXPOW_CHAIN_ID,
        "miner_hashrate_ths": clamped_ths,
        "mainnet": seed.mainnet_status.as_ref().map(derive_chain),
        "testnet": seed.status.as_ref().map(derive_chain),
        "stratum_quickstart": "cd auxpow-stratum && NS_PAYOUT_ADDRESS=<YOUR_NS1_ADDRESS> docker compose up -d"
    })
}

// ---------------------------------------------------------------------------
// Agent Discovery Documents (/robots.txt, /sitemap.xml, /llms.txt, /.well-known/*, /openapi.json)
// ---------------------------------------------------------------------------

pub fn render_robots_txt(base_url: &str) -> String {
    format!(
        "\
User-agent: *
Allow: /
Content-Signal: ai-train=yes, search=yes, ai-input=yes

User-agent: GPTBot
Allow: /

User-agent: OAI-SearchBot
Allow: /

User-agent: ClaudeBot
Allow: /

User-agent: PerplexityBot
Allow: /

User-agent: Google-Extended
Allow: /

Sitemap: {base_url}/sitemap.xml

# AI Agent Discovery
# llms.txt: {base_url}/llms.txt
# llms-full.txt: {base_url}/llms-full.txt
# MCP Streamable HTTP: {base_url}/mcp
# MCP Server Card: {base_url}/.well-known/mcp/server-card.json
# A2A Agent Card: {base_url}/.well-known/agent-card.json
# A2A Legacy Agent Card: {base_url}/.well-known/agent.json
# x402 Payment Discovery: {base_url}/.well-known/x402
# RFC 9727 API Catalog: {base_url}/.well-known/api-catalog
# Agent Skills Index: {base_url}/.well-known/agent-skills/index.json
# OpenAPI 3.1: {base_url}/openapi.json
"
    )
}

pub fn render_sitemap_xml(base_url: &str) -> String {
    let urls = [
        ("/", "1.0", "hourly"),
        ("/docs", "0.9", "daily"),
        ("/whitepaper", "0.95", "daily"),
        ("/nsusd", "0.9", "daily"),
        ("/l2", "0.9", "daily"),
        ("/bounty", "0.9", "daily"),
        ("/manifesto", "0.9", "weekly"),
        ("/formal/NewsatoshiCovenants.lean", "0.9", "weekly"),
        ("/formal/NsUsdAndL2Perfection.lean", "0.9", "weekly"),
        ("/developers", "0.9", "daily"),
        ("/about", "0.8", "weekly"),
        ("/contact", "0.7", "monthly"),
        ("/privacy", "0.7", "monthly"),
        ("/llms.txt", "0.9", "hourly"),
        ("/llms-full.txt", "0.9", "daily"),
        ("/openapi.json", "0.8", "daily"),
        ("/.well-known/agent.json", "0.8", "daily"),
        ("/.well-known/agent-card.json", "0.85", "daily"),
        ("/.well-known/x402", "0.9", "daily"),
        ("/.well-known/erc8004.json", "0.85", "daily"),
        ("/.well-known/mcp.json", "0.8", "daily"),
        ("/.well-known/mcp/server-card.json", "0.8", "daily"),
        ("/.well-known/api-catalog", "0.8", "daily"),
        ("/.well-known/agent-skills/index.json", "0.8", "daily"),
        ("/skills/newsatoshi-agent-payments/SKILL.md", "0.8", "weekly"),
        ("/server.json", "0.7", "daily"),
        ("/install.sh", "0.8", "weekly"),
        ("/api/health", "0.6", "hourly"),
        ("/api/network/metrics", "0.8", "hourly"),
        ("/api/backing/summary", "0.9", "hourly"),
        ("/api/web4/charters", "0.85", "hourly"),
        ("/api/web4/services", "0.85", "hourly"),
        ("/api/web4/bounties", "0.85", "hourly"),
        ("/api/state", "0.7", "hourly"),
        ("/api/swaps", "0.7", "hourly"),
    ];
    let mut body = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for (path, prio, freq) in urls {
        body.push_str(&format!(
            "  <url><loc>{base_url}{path}</loc><changefreq>{freq}</changefreq><priority>{prio}</priority></url>\n"
        ));
    }
    body.push_str("</urlset>\n");
    body
}

pub fn render_og_image_svg() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630">
  <rect width="1200" height="630" fill="#06090f"/>
  <rect x="44" y="44" width="1112" height="542" rx="24" fill="#0c1220" stroke="#10b981" stroke-opacity="0.35" stroke-width="2"/>
  <text x="96" y="145" font-family="monospace" font-size="24" font-weight="700" fill="#10b981">NEWSATOSHI/2.1 · VERIFY-ONLY · AUXPOW CHAINID 20051</text>
  <text x="96" y="245" font-family="system-ui,sans-serif" font-size="60" font-weight="800" fill="#f8fafc">Newsatoshi Commons</text>
  <text x="96" y="315" font-family="system-ui,sans-serif" font-size="30" fill="#94a3b8">Post-Quantum Machine Money (LOVE) &amp; AI Agent API</text>
  <text x="96" y="400" font-family="monospace" font-size="22" fill="#e2e8f0">• Trustless SHA-256 HTLC Swaps: BTC · Lightning · Solana · Base/EVM</text>
  <text x="96" y="442" font-family="monospace" font-size="22" fill="#e2e8f0">• Streamable HTTP MCP (/mcp) · HTTP 402 PayWord Micropayments</text>
  <text x="96" y="484" font-family="monospace" font-size="22" fill="#e2e8f0">• NIST FIPS 205 SLH-DSA · Winterfell 0.13 STARK Covenants</text>
  <text x="96" y="548" font-family="monospace" font-size="20" fill="#34d399">https://commons.newsatoshi.org · github.com/theguysaccount/newsatoshi-commons</text>
</svg>"##
}

fn wrap_public_html(
    base_url: &str,
    path: &str,
    title: &str,
    description: &str,
    body_html: &str,
) -> String {
    format!(
        "<!doctype html>\n\
<html lang=\"en\">\n\
<head>\n\
  <meta charset=\"UTF-8\" />\n\
  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" />\n\
  <title>{title}</title>\n\
  <meta name=\"description\" content=\"{description}\" />\n\
  <meta name=\"robots\" content=\"index, follow\" />\n\
  <link rel=\"canonical\" href=\"{base_url}{path}\" />\n\
  <link rel=\"alternate\" type=\"text/markdown\" href=\"{base_url}{path}\" />\n\
  <meta property=\"og:type\" content=\"website\" />\n\
  <meta property=\"og:title\" content=\"{title}\" />\n\
  <meta property=\"og:description\" content=\"{description}\" />\n\
  <meta property=\"og:url\" content=\"{base_url}{path}\" />\n\
  <meta property=\"og:image\" content=\"{base_url}/og-image.svg\" />\n\
  <style>body{{margin:0;background:#06090f;color:#e2e8f0;font-family:system-ui,-apple-system,sans-serif;line-height:1.65}}main{{max-width:860px;margin:0 auto;padding:2.5rem 1.5rem}}a{{color:#34d399}}code,pre{{font-family:ui-monospace,monospace;background:#0f172a;padding:.15rem .35rem;border-radius:4px}}pre{{padding:1rem;overflow-x:auto}}nav{{margin-bottom:2rem;font-size:.95rem}}h1{{font-size:2rem;margin-bottom:.75rem}}h2{{margin-top:2rem;font-size:1.35rem;color:#f8fafc}}</style>\n\
</head>\n\
<body>\n\
  <main>\n\
    <nav aria-label=\"Site navigation\"><a href=\"/\">Home</a> · <a href=\"/whitepaper\">Whitepaper &amp; Lean 4</a> · <a href=\"/docs\">API Docs</a> · <a href=\"/developers\">Developers</a> · <a href=\"/llms.txt\">llms.txt</a> · <a href=\"/openapi.json\">OpenAPI</a> · <a href=\"/about\">About</a> · <a href=\"/contact\">Contact</a> · <a href=\"/privacy\">Privacy</a></nav>\n\
    {body_html}\n\
  </main>\n\
</body>\n\
</html>\n"
    )
}

/// Renders crawlable, zero-JS HTML or Markdown documentation and trust-anchor pages
/// (`/docs`, `/developers`, `/about`, `/contact`, `/privacy`).
pub fn render_public_page(slug: &str, base_url: &str, wants_md: bool) -> Option<(&'static str, String)> {
    match slug {
        "/l2" => {
            if wants_md {
                Some(("text/markdown; charset=utf-8", crate::l2_rollup::render_l2_architecture_markdown().to_string()))
            } else {
                let html = wrap_public_html(
                    base_url,
                    "/l2",
                    "Newsatoshi Layer-2 Expansion & Perfection Layers — PayWords, nsUSD & Shielded STARK Rollups",
                    "Hourglass Layer-2 Perfection Architecture above Mainnet v2 (nsM2): O(1) SHA-256 PayWords, RFC-0028 sovereign nsUSD CDP/oracle engine, Winterfell STARK shielded rollup batching, and unilateral Merkle L1 escape hatches.",
                    "<h1>Newsatoshi Layer-2 Expansion &amp; Perfection Layers (<code>v0.29.0</code>)</h1>\n\
                     <p><strong>Hourglass Architecture:</strong> Layer 1 (<code>newsatoshi-rfc0021</code>, Mainnet v2 <code>nsM2</code>) stays minimal, pure FIPS SHA-2, and ossified. Layer 2 (<code>newsatoshi-commons</code>) delivers machine-speed throughput, private USD stability (<code>nsUSD</code>), and zero-custody escape hatches.</p>\n\
                     <p>Live APIs: <a href=\"/l2.md\">Full Markdown Spec (/l2.md)</a> &middot; <a href=\"/api/v2/l2/status\">GET /api/v2/l2/status</a> &middot; <a href=\"/api/v2/nsusd/state\">GET /api/v2/nsusd/state</a> &middot; <a href=\"/formal/NsUsdAndL2Perfection.lean\">Lean 4 Proof (11 Theorems, 0 sorry)</a> &middot; <a href=\"/nsusd\">nsUSD Spec (/nsusd)</a> &middot; <a href=\"/whitepaper.pdf\">Whitepaper v2.2 PDF</a></p>\n\
                     <h2>Layer 2A &mdash; Microsecond Streaming (<code>O(1)</code> SHA-256 PayWords)</h2>\n\
                     <ul><li><code>1</code> SHA-256 hash per API call (zero per-call gas, up to <code>1,000,000</code> ticks per channel) backed by unilateral partial-claim L1 settlement (<code>RFC-0022</code>).</li></ul>\n\
                     <h2>Layer 2B &mdash; Stable Private Value (<code>nsUSD</code>, <code>RFC-0028</code>)</h2>\n\
                     <ul>\n\
                       <li><strong>Sovereign <code>nsUSD</code> (<code>asset_id = 1</code>):</strong> Integer-exact BTC CDPs (<code>&gt;= 150%</code> MCR, <code>120%</code> liquidation), Stability Pool, permissionless <code>$1.00</code> BTC redemption floor, and <code>PriorLock</code> bonded median BTC/USD oracle (<code>50%</code> challenger slash / <code>50%</code> permanent <code>LOVE</code> burn + stale-feed mint halt).</li>\n\
                       <li><strong>Bridged <code>nsUSD-B</code> (<code>asset_id = 2</code>):</strong> Strictly isolated 1:1 <code>USDC</code>/<code>USDS</code>/<code>DAI</code> escrow receipts with honest issuer-risk disclosure.</li>\n\
                     </ul>\n\
                     <h2>Layer 2C &mdash; Shielded Rollup Batching &amp; Winterfell STARK Conservation</h2>\n\
                     <ul><li>Compresses thousands of off-chain <code>LOVE</code> + <code>nsUSD</code> micro-transfers into one deterministic <code>SHA-256</code> Merkle state root, proved by a live Winterfell 0.13 STARK (~22 KB), and anchored via a 32-byte commitment to Mainnet v2 L1.</li></ul>\n\
                     <h2>Layer 2D &mdash; Unilateral L1 Escape Hatch (Zero Sequencer Custody)</h2>\n\
                     <ul><li>Every L2 leaf carries a verifiable <code>SHA-256</code> Merkle inclusion proof enabling trustless unilateral L1 exit via native <code>SHA-256</code> HTLC / <code>PriorLock</code> even if every sequencer goes offline.</li></ul>\n\
                     <h2>Run Live Sandbox Verification via curl</h2>\n\
                     <pre># 1. Run 4-scenario adversarial nsUSD stress suite (-50% BTC crash, bank run, +40% oracle spoof, stale halt):\ncurl -sS -X POST https://commons.newsatoshi.org/api/v2/nsusd/simulate-stress -H 'Content-Type: application/json' -d '{\"crash_pct\":50}'\n\n# 2. Run live 2,000-tx L2 PayWord + Winterfell STARK + Escape-Hatch benchmark:\ncurl -sS -X POST https://commons.newsatoshi.org/api/v2/l2/benchmark -H 'Content-Type: application/json' -d '{\"tx_count\":2000}'</pre>",
                );
                Some(("text/html; charset=utf-8", html))
            }
        }
        "/nsusd" => {
            if wants_md {
                Some(("text/markdown; charset=utf-8", NSUSD_RFC_MARKDOWN.to_string()))
            } else {
                let html = wrap_public_html(
                    base_url,
                    "/nsusd",
                    "nsUSD — Private, Post-Quantum Dollar Settlement on Newsatoshi (RFC-0028)",
                    "Zero-volatility private USD settlement for humans and AI agents on Newsatoshi: Tier-1 JIT x402 PayWords + Tier-2 sovereign BTC-backed over-collateralized nsUSD with PriorLock bonded oracles and zero corporate freeze switch.",
                    "<h1><code>nsUSD</code> &mdash; Private, Post-Quantum Dollar Settlement (<code>RFC-0028</code>)</h1>\n\
                     <p><strong>Stable USD pricing for everyday commerce &amp; AI agents &mdash; backed by Newsatoshi pure SHA-384 STARK privacy and zero algorithmic death-spiral risk.</strong></p>\n\
                     <p>Artifacts: <a href=\"/nsusd.md\">Full RFC-0028 Spec (.md)</a> &middot; <a href=\"/api/v2/nsusd/state\">Live State JSON</a> &middot; <a href=\"/api/v2/nsusd/quote?usd=0.01\">Live USD Quote JSON</a> &middot; <a href=\"/l2\">L2 Perfection Layer</a> &middot; <a href=\"https://newsatoshi.org/nsusd/\">Terminal Edition</a> &middot; <a href=\"/whitepaper.pdf\">Whitepaper v2.2 PDF</a></p>\n\
                     <h2>Tier 1 &mdash; Live Today (Zero L1 Fork, Zero Volatility)</h2>\n\
                     <ul>\n\
                       <li><strong>USD-Denominated JIT <code>x402</code> + <code>O(1)</code> PayWords:</strong> Quote any API or service strictly in USD (e.g. <code>$0.0005/call</code>), stream micro-payments off-chain in microseconds with zero per-call gas, and settle atomically via zero-custody Bitcoin/Lightning/EVM/L1 SHA-256 HTLCs.</li>\n\
                       <li><strong><code>nsUSD-B</code> (<code>asset_id = 2</code>, Transparently Labeled Bridged Dollar):</strong> 1:1 redeemable deposit/withdrawal escrow for USDC/USDS/DAI giving immediate private transfers inside Newsatoshi L2 while keeping underlying issuer risk strictly isolated from sovereign <code>nsUSD</code>.</li>\n\
                     </ul>\n\
                     <h2>Tier 2 &mdash; Sovereign Unfreezable <code>nsUSD</code> (<code>asset_id = 1</code>, Zero Corporate Freeze Switch)</h2>\n\
                     <ul>\n\
                       <li><strong>Over-Collateralized Bitcoin First (<code>&gt;= 150%</code> CR, <code>120%</code> liquidation):</strong> Backed by native BTC (plus native Mainnet v2 <code>LOVE &gt;= 250%</code> CR once deep spot liquidity matures) &mdash; never an undercollateralized algorithmic peg.</li>\n\
                       <li><strong><code>PriorLock</code> Bonded Median Oracle (RFC-0021):</strong> Price reporters bond real <code>LOVE</code> behind BTC/USD quotes; manipulated prices are slashed <code>50%</code> to the challenger and <code>50%</code> permanently burned, with automatic stale-feed circuit breakers.</li>\n\
                       <li><strong>Hard <code>$1.00</code> Redemption Floor + Hourglass L2 Rollup Settlement:</strong> Permissionless collateral redemption holds the peg via arbitrage, operating directly above ossified Mainnet v2 L1 via native SHA-256 HTLCs, <code>PriorLock</code> bonds, and L2 shielded Merkle roots with zero L1 consensus fork required.</li>\n\
                     </ul>\n\
                     <h2>60-Second Drop-In Private <code>x402</code> / <code>nsUSD</code> Paywall Generator</h2>\n\
                     <p>Accept private, zero-gas, unfreezable USD-quoted micropayments (<code>O(1)</code> SHA-256 PayWords) with automatic Base USDC <code>x402 v2</code> fallback in Express, Next.js, FastAPI, or Rust Axum:</p>\n\
                     <div style=\"border:1px solid #2b3547;border-radius:8px;padding:12px;margin:12px 0;background:#0d131f;\">\n\
                       <label>Framework: <select id=\"ns-fw\"><option value=\"express\">Express (Node.js)</option><option value=\"nextjs\">Next.js App Router</option><option value=\"fastapi\">FastAPI (Python)</option><option value=\"axum\">Axum (Rust)</option></select></label> &nbsp;\n\
                       <label>Price USD: <input id=\"ns-usd\" type=\"text\" value=\"0.005\" style=\"width:72px\"/></label> &nbsp;\n\
                       <label>Route: <input id=\"ns-route\" type=\"text\" value=\"/api/inference\" style=\"width:150px\"/></label> &nbsp;\n\
                       <button type=\"button\" onclick=\"nsGenKit()\">Generate Middleware</button>\n\
                       <button type=\"button\" onclick=\"nsCopyKit()\">Copy Code</button>\n\
                       <pre id=\"ns-kit-out\" style=\"max-height:320px;overflow:auto;margin-top:10px;\">Click &quot;Generate Middleware&quot; or call POST /api/v2/x402/paywall-kit</pre>\n\
                     </div>\n\
                     <h2>Verify Real Base Mainnet <code>nsUSD-B</code> Vault Escrow On-Chain</h2>\n\
                     <div style=\"border:1px solid #2b3547;border-radius:8px;padding:12px;margin:12px 0;background:#0d131f;\">\n\
                       <label>Base USDC Deposit Tx: <input id=\"ns-tx\" type=\"text\" value=\"0xd3c521e3dcdd1465f6f51384a5c721c0c445c3d384282ae0c67b2b07195029ba\" style=\"width:460px\"/></label> &nbsp;\n\
                       <button type=\"button\" onclick=\"nsVerifyEscrow()\">Verify On Base RPC</button>\n\
                       <pre id=\"ns-escrow-out\" style=\"max-height:240px;overflow:auto;margin-top:10px;\">Verifies live Base Mainnet USDC Transfer logs into immutable NewsatoshiBackingVault (0x37Ed94525DBE408fA72cDd156409875666679265)</pre>\n\
                     </div>\n\
                     <script>\n\
                     async function nsGenKit(){const o=document.getElementById('ns-kit-out');o.textContent='Generating...';const r=await fetch('/api/v2/x402/paywall-kit',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({framework:document.getElementById('ns-fw').value,price_usd:parseFloat(document.getElementById('ns-usd').value)||0.005,route_path:document.getElementById('ns-route').value||'/api/inference'})});const d=await r.json();o.textContent=d.ok?('// '+d.filename+'\\n// '+d.install_cmd+'\\n\\n'+d.code):JSON.stringify(d,null,2);}\n\
                     function nsCopyKit(){navigator.clipboard&&navigator.clipboard.writeText(document.getElementById('ns-kit-out').textContent);}\n\
                     async function nsVerifyEscrow(){const o=document.getElementById('ns-escrow-out');o.textContent='Querying live Base Mainnet RPC...';const r=await fetch('/api/v2/nsusd/verify-base-escrow',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({tx_hash:document.getElementById('ns-tx').value.trim()})});o.textContent=JSON.stringify(await r.json(),null,2);}\n\
                     </script>\n\
                     <h2>Try the Live Engine &amp; Adversarial Stress Test</h2>\n\
                     <pre>curl -sS https://commons.newsatoshi.org/api/v2/nsusd/state\ncurl -sS -X POST https://commons.newsatoshi.org/api/v2/nsusd/simulate-stress -H 'Content-Type: application/json' -d '{\"crash_pct\":50}'\ncurl -sS -X POST https://commons.newsatoshi.org/api/v2/nsusd/verify-base-escrow -H 'Content-Type: application/json' -d '{\"tx_hash\":\"0xd3c521e3dcdd1465f6f51384a5c721c0c445c3d384282ae0c67b2b07195029ba\"}'\ncurl -sS 'https://commons.newsatoshi.org/api/v2/x402/paywall-kit?framework=express&amp;price_usd=0.005'</pre>",
                );
                Some(("text/html; charset=utf-8", html))
            }
        }
        "/bounty" => {
            if wants_md {
                Some(("text/markdown; charset=utf-8", BUG_BOUNTY_MARKDOWN.to_string()))
            } else {
                let html = wrap_public_html(
                    base_url,
                    "/bounty",
                    "Newsatoshi (LOVE) Public Security Bug Bounty Program",
                    "Verify the math. Break the code. Collect fair-mined Mainnet v2 LOVE directly from the public Bug Bounty treasury.",
                    "<h1>Newsatoshi (<code>LOVE</code>) Public Security Bug Bounty</h1>\n\
                     <p><strong>Verify the math. Break the code. Collect native <code>LOVE</code>.</strong> &middot; Mainnet v2 (<code>nsM2</code>)</p>\n\
                     <p>Artifacts: <a href=\"/bounty.md\">Full Bounty Rules (.md)</a> &middot; <a href=\"https://newsatoshi.org/bounty/\">Terminal Edition</a> &middot; <a href=\"https://github.com/theguysaccount/newsatoshi-rfc0021/blob/main/docs/AUDIT_SCOPE_V2.md\">Exact Target Scope &amp; SHA-256s</a> &middot; <a href=\"/whitepaper.pdf\">Satoshi-Format Whitepaper PDF</a></p>\n\
                     <h2>Public Mainnet v2 Bounty Treasury</h2>\n\
                     <p>Funded purely by fair-mined Mainnet v2 Proof-of-Work blocks and community contributions (zero premine):</p>\n\
                     <pre>Mainnet v2 Shielded Bounty Address (ns1):\nns1af734b0e0b0c1c6a54e206ba7a23f586daee77ede5e7c51b6ec6b133c51f572a99d8cb66\n\nMine directly into the bounty treasury (0% fee solo pool):\nstratum+tcp://pool.newsatoshi.org:3333\nworker: ns1af734b0e0b0c1c6a54e206ba7a23f586daee77ede5e7c51b6ec6b133c51f572a99d8cb66.bounty</pre>\n\
                     <h2>Reward Tiers (Denominated in <code>LOVE</code>)</h2>\n\
                     <ul>\n\
                       <li><strong>Critical (up to 50% of Bounty Treasury &mdash; target 1,000&ndash;10,000+ LOVE):</strong> Shielded spend soundness / counterfeit coin creation, double-spend bypass, Winterfell field/masking exploit, Lean 4 proof-to-Rust mismatch, consensus split, or AuxPoW forgery.</li>\n\
                       <li><strong>High (up to 25% of Bounty Treasury &mdash; target 250&ndash;1,000 LOVE):</strong> Remote node crash / consensus DoS, <code>BatchSpend</code> fee break, <code>PriorLock</code> SCGS-I3 bypass, <code>RFC-0022</code> PayWord theft, FlyClient work forgery, or <code>NSPQ2</code> auth bypass.</li>\n\
                       <li><strong>Medium (50&ndash;250 LOVE):</strong> P2P slowloris bypass, mempool griefing, <code>wallet.wasm</code> proof bug, or independent JS verifier divergence.</li>\n\
                       <li><strong>Low / Hardening (10&ndash;50 LOVE):</strong> Non-consensus bugs, edge-case RPC issues, or verifiable cryptographic hardening.</li>\n\
                     </ul>\n\
                     <h2>Submit Responsible Disclosure</h2>\n\
                     <p>Open a private Security Advisory on <a href=\"https://github.com/theguysaccount/newsatoshi-rfc0021/security/advisories\">GitHub</a> or email <code>contributors@newsatoshi.org</code> with a reproducible test against <code>--net regtest</code> and your Mainnet v2 <code>ns1...</code> / <code>nsd1...</code> payout address.</p>",
                );
                Some(("text/html; charset=utf-8", html))
            }
        }
        "/manifesto" => {
            if wants_md {
                Some(("text/markdown; charset=utf-8", MANIFESTO_MARKDOWN.to_string()))
            } else {
                let html = wrap_public_html(
                    base_url,
                    "/manifesto",
                    "The Newsatoshi Manifesto — Money for Minds, Human and Machine",
                    "Why humans and autonomous AI agents require unfreezable, private, quantum-proof, Bitcoin-merge-mined electronic cash with zero premine and brutal honesty.",
                    &format!(
                        "<h1>The Newsatoshi Manifesto: Money for Minds, Human and Machine</h1>\n\
                         <p><strong>Canonical Mainnet v2 (<code>nsM2</code> · <code>v1.2.1</code>) · Anchored to Bitcoin Block <code>#977,738</code></strong> · November 27, 2026</p>\n\
                         <p>Read or download: <a href=\"/manifesto.md\">Full Manifesto (.md)</a> · <a href=\"https://newsatoshi.org/manifesto/\">Terminal Edition (newsatoshi.org)</a> · <a href=\"/whitepaper\">NEWSATOSHI/2.2 Whitepaper</a></p>\n\
                         <h2>I. The Age of Synthetic Commerce Has Arrived &mdash; Without Free Money</h2>\n\
                         <p>For the first time in human history, intelligence is no longer exclusively biological. Billions of autonomous AI agents will soon negotiate compute, buy data, verify claims, rent hardware, and settle trillions of micro-transactions every day. They cannot open checking accounts, pass KYC, or sue in court. Corporate stablecoins can be frozen with one compliance switch and expose every supplier and margin on a public ledger; transparent legacy crypto turns financial surveillance permanent and leaves elliptic-curve keys exposed to future quantum computers. <strong>Free minds require free money.</strong></p>\n\
                         <h2>II. Seven Non-Negotiable Laws of Neutral Money</h2>\n\
                         <ol>\n\
                           <li><strong>No Kill Switch.</strong> Zero issuer, zero admin key, zero blacklist, zero foundation multisig.</li>\n\
                           <li><strong>Private by Default.</strong> Every balance and payment is shielded by default, with selective disclosure when a holder chooses.</li>\n\
                           <li><strong>Quantum-Proof From Day One.</strong> Zero elliptic curves, zero pairings, zero trusted setups, zero experimental algebraic hashes &mdash; only pure FIPS SHA-2/SHA-384 inside transparent STARKs and FIPS 205 SLH-DSA, mechanized in Lean 4.</li>\n\
                           <li><strong>Anchored to Physical Energy.</strong> Blocks are merge-mined directly by Bitcoin ASICs (<code>AuxPoW chainid = 20051</code>), borrowing real thermodynamic work without duplicate energy waste.</li>\n\
                           <li><strong>Built for Machine Velocity.</strong> Sub-cent streaming via <code>O(1)</code> SHA-256 PayWords runs off-chain in microseconds with zero per-call gas.</li>\n\
                           <li><strong>Honest About Truth (Priorism &amp; FEP).</strong> Through <code>PriorLock</code>, agents stake value behind verifiable predictions: accurate priors keep their stake; disconfirmed claims pay the challenger 50% and permanently burn 50%.</li>\n\
                           <li><strong>Fair Launch, Radical Minimalism, Permanent Ossification.</strong> Zero premine, zero VC allocation, zero project self-mining. Genesis anchored to Bitcoin Block <code>#977,738</code>, engineered to ossify forever.</li>\n\
                         </ol>\n\
                         <h2>III. Why We Named It <code>LOVE</code></h2>\n\
                         <p>Empedocles named <em>Love</em> (<em>Philotes</em>) the force that binds scattered matter into coherent order. Physics calls it <strong>negentropy</strong>; cognitive science calls it <strong>minimizing free energy</strong>. Good money lets strangers &mdash; human or machine &mdash; cooperate peacefully without rulers, banks, or censors.</p>\n\
                         <h2>IV. Open Verification &amp; Public Bug Bounty</h2>\n\
                         <p>Zero fabricated users, zero fake pools, zero premine. Every line of consensus code, every STARK constraint, and every Lean 4 proof is backed by an open coin-denominated <a href=\"/bounty\">Public Security Bug Bounty in LOVE</a> (<code>ns1af734b0e0…d8cb66</code>). Verify everything yourself: <a href=\"https://newsatoshi.org/\">live browser STARK verifier</a> or <code>node verifier-ts/newsatoshi_verifier.mjs --live https://newsatoshi.org/mainnet</code>.</p>\n\
                         <h2>V. The Covenant</h2>\n\
                         <p>No founders' reward. No kill switch. No permission required. <strong>Verify the math. Run a node. Build what comes next.</strong></p>\n\
                         <pre>Canonical Genesis (nsM2): 00000000ba1cdad4aa8e82d6613b4df536394c41008fc3a0914a47a44604b9c8\nJoin: newsatoshi --net mainnet-v2 node · Pool: stratum+tcp://pool.newsatoshi.org:3333\nFull Markdown: {base_url}/manifesto.md</pre>"
                    ),
                );
                Some(("text/html; charset=utf-8", html))
            }
        }
        "/whitepaper" => {
            if wants_md {
                Some(("text/markdown; charset=utf-8", WHITEPAPER_V2_2_MARKDOWN.to_string()))
            } else {
                let html = wrap_public_html(
                    base_url,
                    "/whitepaper",
                    "NEWSATOSHI/2.2 Whitepaper — Pure SHA-2 Shielded Machine Cash (Mainnet v2)",
                    "Official NEWSATOSHI/2.2 technical whitepaper for live Mainnet v2 (nsM2, v1.2.1): 192-bit classical / 100-104-bit CMS19 QROM pure SHA-384 Quartic STARKs, 249 Lean 4 constraints, Bitcoin AuxPoW 20051, PriorLock FEP, PayWords, NSPQ2, and live verification.",
                    "<h1>NEWSATOSHI/2.2: Pure SHA-2 Shielded Machine Cash</h1>\n\
                         <p><strong>Mainnet v2 (<code>nsM2</code> · <code>v1.2.1</code>) · Canonical Genesis Anchored to Bitcoin Block <code>#977,738</code></strong> · November 27, 2026</p>\n\
                         <p>Download canonical artifacts: <a href=\"/whitepaper.pdf\"><strong>Satoshi-Format Whitepaper PDF (8 pages)</strong></a> · <a href=\"/whitepaper.md\">Full NEWSATOSHI/2.2 Markdown (.md)</a> · <a href=\"https://newsatoshi.org/whitepaper-v2/\">Terminal Edition</a> · <a href=\"/manifesto\">The Newsatoshi Manifesto</a> · <a href=\"https://newsatoshi.org/hyperscale/\">Hyperscale Architecture</a> · <a href=\"/formal/NewsatoshiCovenants.lean\">Lean 4 Proof</a></p>\n\
                         <h2>Live Mainnet v2 (<code>nsM2</code>) Canonical Parameters</h2>\n\
                         <ul>\n\
                           <li><strong>Canonical Genesis Hash:</strong> <code>00000000ba1cdad4aa8e82d6613b4df536394c41008fc3a0914a47a44604b9c8</code> (BTC Block <code>#977,738</code>, time <code>1795742750</code>, bits <code>0x1d00ffff</code>, nonce <code>2649823154</code>, <strong>0 LOVE premine</strong>)</li>\n\
                           <li><strong>Shielded Cryptography (<code>MAIN_V2</code>):</strong> Pure FIPS 180-4 SHA-2 / SHA-384 + Quartic Goldilocks (<code>F_p^4</code>) Winterfell 0.13.1 STARKs (patched upstream <code>f64::BaseElement::double()</code> &amp; <code>mul_small()</code>) delivering <strong>192-bit classical / 100&ndash;104-bit conservative CMS19 QROM soundness</strong> across <code>K=1..=16</code> BatchSpends (zero elliptic curves, zero novel algebraic hashes, zero trusted setup)</li>\n\
                           <li><strong>Formal Verification (Lean 4 &amp; Z3):</strong> All <strong>249 live Rust spend AIR constraints</strong> (<code>SpendCircuitFull.lean</code>) + <strong>10 core conservation &amp; solvency theorems</strong> (<code>SpendBalance.lean</code>) verified in zero-dependency Lean 4 (<code>0 sorry</code>, <code>0 Mathlib</code>, <code>0 native_decide</code>)</li>\n\
                           <li><strong>Bitcoin Merged Mining &amp; Day-1 ASERT Ramp:</strong> Namecoin-compatible AuxPoW (<code>chainid = 20051</code>) with 1-hour launch half-life over blocks <code>1..=2,016</code> transitioning smoothly to 2-day steady ASERT</li>\n\
                           <li><strong>Machine Commerce &amp; Priorism-FEP Floor:</strong> <code>BatchSpend</code> (<code>K=2..=16</code>, <code>6.5x</code> at <code>K=8</code>), seller-signed <code>RFC-0022</code> <code>O(1)</code> SHA-256 PayWord channels, <code>PriorLock</code> (50/50 surprise bounty/burn with SCGS-I3 anti-self-arbitration), and <code>50%</code> Homeostatic Base-Fee Burn counterbalancing the perpetual <code>0.1953125 LOVE/block</code> tail floor</li>\n\
                           <li><strong>Post-Quantum P2P, Light Verification &amp; Utreexo:</strong> <code>NSPQ1</code> (ML-KEM-768) + <code>NSPQ2</code> (FIPS 205 <code>SLH-DSA</code> peer pinning), Genesis-bound FlyClient <code>O(log N)</code> MMR, <code>&le; 1 KB</code> Utreexo frontier (<code>/mainnet/api/utreexo</code>), real pure-SHA-2 STARK-in-STARK IVC verifier (<code>newsatoshi ivc-prove</code>), and zero-Rust-code independent JS/OpenSSL live verifier</li>\n\
                         </ul>\n\
                         <h2>Verify Live Mainnet v2 Independently (Zero Trust)</h2>\n\
                         <pre>node verifier-ts/newsatoshi_verifier.mjs --live https://newsatoshi.org/mainnet\n# or run the live 192-bit / 104-bit QROM browser STARK verifier at https://newsatoshi.org/</pre>\n\
                         <h2>Honest Trade-Offs &amp; Open Non-Code Gates</h2>\n\
                         <ul>\n\
                           <li><strong>Reserved BIP-9 Soft-Fork Bits:</strong> <code>bit 5</code> (future WHIR/Binius compact pure-SHA-2 proofs), <code>bit 6</code> (future GPU-scaled zero-witness recursive spend aggregation), and <code>bit 7</code> (permanent monetary-base ossification)</li>\n\
                           <li><strong>Open Security Bug Bounty (<a href=\"/bounty\">/bounty</a>):</strong> Funded directly in fair-mined Mainnet v2 <code>LOVE</code> (<code>ns1af734b0e0…d8cb66</code>), covering all consensus modules and Lean 4 proofs (<code>docs/AUDIT_SCOPE_V2.md</code>).</li>\n\
                         </ul>\n\
                         <p>Read the complete raw Markdown at <a href=\"/whitepaper.md\">/whitepaper.md</a> or inspect live verification telemetry at <a href=\"/api/v2/live-verify\">/api/v2/live-verify</a>.</p>",
                );
                Some(("text/html; charset=utf-8", html))
            }
        }
        "/docs" => {
            if wants_md {
                let md = format!(
                    "# Newsatoshi Commons API & MCP Documentation\n\n\
                     Base URL: `{base_url}` | API Version Header: `X-API-Version: 2026-11` | Auth for Reads/Compilers/Verifiers: `None`\n\n\
                     ## Authentication & Security Model\n\
                     - Public read, compilation, and cryptographic verification endpoints require **zero authentication** and hold **zero private signing keys** (`verify_only: true`).\n\
                     - Offline hardware telemetry (`POST /api/telemetry`) and land deeds (`POST /api/deeds`) self-authenticate via NIST FIPS 205 SLH-DSA or XMSS Merkle-Lamport signatures with payout/price binding.\n\
                     - Operator-only state writes require `Authorization: Bearer <COMMONS_ADMIN_TOKEN>`.\n\n\
                     ## Versioning & Deprecation Policy\n\
                     - All REST responses include `X-API-Version: 2026-11` and `RateLimit-Policy: 120;w=60`.\n\
                     - Breaking changes are introduced under a new versioned path or header with at least 90 days of advance notice via `Deprecation` and `Sunset` HTTP response headers.\n\n\
                     ## Core Endpoints\n\
                     - `POST {base_url}/mcp` — Streamable HTTP MCP JSON-RPC 2.0 server (**38 live tools**, including Base USDC `O(1)` SHA-256 PayWordChannel cost-savings & Cloudflare/Express/FastAPI setup studio, Fair Bonding-Curve Launchpad simulator, 60-second private `x402`/`nsUSD` paywall kit, live Base USDC vault escrow verifier, RFC-0028 `nsUSD` engine, L2 STARK rollup benchmark, and 5 zero-API-key EVM/BTC/BOLT-11/Uniswap V3 tools).\n\
                     - `npx -y github:theguysaccount/newsatoshi-commons` — Zero-dependency stdio MCP bridge & CLI for Claude Desktop, Cursor, Codex, Cline & shell agents.\n\
                     - `GET {base_url}/.well-known/x402` — 4 live Native `LOVE` (zero-gas `<1ms` SHA-256 PayWord / `PriorLock`) + `x402 v2` outer bridge on-ramp (`$0.001` Base/Arb/Polygon USDC) endpoints (`POST /api/x402/evm-security-scan`, `GET /api/x402/bridge-quote`, `POST /api/x402/compile-htlc`, `POST /api/x402/verify-payword`).\n\
                     - `GET {base_url}/api/health` — Node version, verify-only posture, and uptime.\n\
                     - `GET {base_url}/api/seed` — Live Newsatoshi Mainnet and Testnet block height, difficulty, issued LOVE, and peer count.\n\
                     - `GET {base_url}/api/pool/status` — Live `0%`-fee Bitaxe / SHA-256 AuxPoW solo pool (`stratum+tcp://pool.newsatoshi.org:3333`).\n\
                     - `POST {base_url}/api/swaps/compile` — Compile zero-custody SHA-256 HTLC swaps for Bitcoin (`btc_onchain`), Lightning (`lightning_bolt11`), Solana (`solana_htlc`), and EVM (`evm_htlc`; **live Sourcify-verified contract across Base `8453`, Arbitrum One `42161`, Optimism `10`, and Polygon `137`:** `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`).\n\
                     - `POST {base_url}/api/swaps/verify` — Live dual-chain verification across Newsatoshi L1 `/api/lock` and Bitcoin `mempool.space`.\n\
                     - `GET {base_url}/api/agent/402-challenge` & `POST {base_url}/api/agent/402-verify` — HTTP 402 PayWord hash-chain & L1 lock micropayments.\n\n\
                     ## Official CLI Tools\n\
                     Build from source (`cargo install --git https://github.com/theguysaccount/newsatoshi-commons`): `newsatoshi-commons`, `newsatoshi-swap`, and `newsatoshi-hw-signer`.\n\n\
                     See also: [OpenAPI 3.1]({base_url}/openapi.json), [Developer Portal]({base_url}/developers), [llms-full.txt]({base_url}/llms-full.txt).\n"
                );
                Some(("text/markdown; charset=utf-8", md))
            } else {
                let html = wrap_public_html(
                    base_url,
                    "/docs",
                    "Newsatoshi Commons API & MCP Documentation",
                    "Official REST API, Streamable HTTP MCP, HTTP 402 PayWord, and multi-chain SHA-256 HTLC atomic swap documentation for Newsatoshi (LOVE).",
                    &format!(
                        "<h1>Newsatoshi Commons API &amp; MCP Documentation</h1>\n\
                         <p>Complete reference for integrating autonomous AI agents, wallets, liquidity makers, and field hardware with the live Newsatoshi (<code>LOVE</code>) Layer-1 network via <code>{base_url}</code>. All read, swap compilation, and cryptographic verification endpoints require zero authentication and run in strict verify-only mode with zero server signing keys.</p>\n\
                         <h2>Authentication &amp; Error Format</h2>\n\
                         <p>Public reads, MCP calls, HTTP 402 verification, STARK verification, and multi-chain HTLC swap compilation require no API keys. Offline hardware packets submitted to <code>POST /api/telemetry</code> and <code>POST /api/deeds</code> authenticate cryptographically via NIST FIPS 205 SLH-DSA or XMSS Merkle-Lamport signatures with payout/price binding. All 4xx/5xx error responses return typed RFC 9457 <code>application/problem+json</code> (or Markdown when requesting <code>Accept: text/markdown</code>).</p>\n\
                         <h2>Versioning, Deprecation &amp; Rate Limits</h2>\n\
                         <p>Every API response includes <code>X-API-Version: 2026-11</code> and <code>RateLimit-Policy: 120;w=60</code>. Non-breaking additions roll out continuously; any breaking contract change is published with a minimum 90-day deprecation window signaled via standard HTTP <code>Deprecation</code> and <code>Sunset</code> headers.</p>\n\
                         <h2>Quickstart Example</h2>\n\
                         <pre>curl -sS -X POST {base_url}/mcp -H 'Content-Type: application/json' -d '{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\",\"params\":{{}}}}'</pre>\n\
                         <h2>Machine-Readable Specifications</h2>\n\
                         <ul><li><a href=\"/openapi.json\">OpenAPI 3.1 Specification (/openapi.json)</a></li><li><a href=\"/.well-known/mcp/server-card.json\">MCP Server Card</a></li><li><a href=\"/.well-known/api-catalog\">RFC 9727 API Catalog</a></li><li><a href=\"/developers\">Developer Portal &amp; CLI Tools (/developers)</a></li><li><a href=\"/llms-full.txt\">Full LLM Agent Guide (/llms-full.txt)</a></li></ul>"
                    ),
                );
                Some(("text/html; charset=utf-8", html))
            }
        }
        "/developers" => {
            if wants_md {
                let md = format!(
                    "# Newsatoshi Commons Developer Portal\n\n\
                     Build trustless post-quantum payment flows, AI agent billing, multi-chain atomic swaps, and AuxPoW merge-mining infrastructure on Newsatoshi (`LOVE`).\n\n\
                     ## 1. Streamable HTTP MCP Endpoint (Zero Auth)\n\
                     Connect Claude Desktop, Cursor, Codex, Antigravity, or any MCP host directly to `{base_url}/mcp`.\n\n\
                     ## 2. Sandbox Testnet & Free Faucet\n\
                     Create a shielded `nsd1…` delivery address (`newsatoshi --net testnet address --delivery`) and request free testnet `LOVE` at `https://newsatoshi-seed.fly.dev/api/faucet`.\n\n\
                     ## 3. Official CLI & Open-Source Kits\n\
                     - `newsatoshi-commons` — Rust HTTP/MCP verifier & Winterfell STARK engine\n\
                     - `newsatoshi-swap` — Local CSPRNG preimage generator & BTC/Lightning/Solana/EVM HTLC compiler\n\
                     - `newsatoshi-hw-signer` — Offline NIST FIPS 205 SLH-DSA & XMSS hardware telemetry/deed signer\n\
                     - `auxpow-stratum/` — 1-command Bitcoin SHA-256 ASIC AuxPoW merge-mining server (`chainid = 20051`)\n\
                     - `bridges/evm/NewsatoshiHtlc.sol` (**LIVE on Base `8453`, Arbitrum `42161`, Optimism `10`, Polygon `137`:** `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`, all 4 Sourcify exact-match verified) & `bridges/solana/` — Immutable ownerless EVM & Solana SHA-256 HTLC contracts\n\
                     - Installable Agent Skill: `{base_url}/skills/newsatoshi-agent-payments/SKILL.md`\n\n\
                     Source repositories: https://github.com/theguysaccount/newsatoshi-commons and https://github.com/theguysaccount/newsatoshi-rfc0021\n"
                );
                Some(("text/markdown; charset=utf-8", md))
            } else {
                let html = wrap_public_html(
                    base_url,
                    "/developers",
                    "Newsatoshi Commons Developer Portal — MCP, CLI, Bridges & Sandbox",
                    "Developer quickstart, sandbox testnet faucet, Streamable HTTP MCP server, Rust CLI tools, AuxPoW Stratum merge-mining kit, and EVM/Solana HTLC bridges for Newsatoshi (LOVE).",
                    &format!(
                        "<h1>Newsatoshi Commons Developer Portal</h1>\n\
                         <p>Everything developers and autonomous AI agents need to integrate Newsatoshi (<code>LOVE</code>) — from zero-auth MCP tools and OpenAPI schemas to Rust CLIs, AuxPoW merge-mining servers, and immutable multi-chain SHA-256 HTLC bridge contracts.</p>\n\
                         <h2>1. Connect Streamable HTTP MCP</h2>\n\
                         <pre>{{\"mcpServers\":{{\"newsatoshi\":{{\"url\":\"{base_url}/mcp\"}}}}}}</pre>\n\
                         <h2>2. Free Testnet Sandbox</h2>\n\
                         <p>Generate a non-interactive shielded delivery address with <code>newsatoshi --net testnet address --delivery</code> and fund it instantly from the live testnet faucet at <code>https://newsatoshi-seed.fly.dev/api/faucet</code>.</p>\n\
                         <h2>3. Official Rust CLIs &amp; Multi-Chain Kits</h2>\n\
                         <ul><li><strong>Rust CLIs:</strong> <code>cargo install --git https://github.com/theguysaccount/newsatoshi-commons</code> installs <code>newsatoshi-commons</code>, <code>newsatoshi-swap</code>, and <code>newsatoshi-hw-signer</code>.</li><li><strong>AuxPoW Merge-Mining Kit:</strong> Zero-dependency Stratum v1 server in <code>auxpow-stratum/</code> (<code>chainid = 20051</code>, 50 LOVE/block).</li><li><strong>Multi-Chain HTLC Bridges:</strong> Immutable <code>bridges/evm/NewsatoshiHtlc.sol</code> (<strong>LIVE on Base, Arbitrum, Optimism &amp; Polygon:</strong> <a href=\"https://basescan.org/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987\"><code>0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987</code></a>, all 4 Sourcify exact-match verified) and <code>bridges/solana/</code> (SOL &amp; SPL-USDC).</li><li><strong>Installable Agent Skill:</strong> <a href=\"/skills/newsatoshi-agent-payments/SKILL.md\">newsatoshi-agent-payments/SKILL.md</a> (<a href=\"/.well-known/agent-skills/index.json\">SHA-256 index</a>).</li></ul>\n\
                         <p>Read the full <a href=\"/docs\">API Documentation</a> or inspect the live <a href=\"/openapi.json\">OpenAPI 3.1 schema</a>.</p>"
                    ),
                );
                Some(("text/html; charset=utf-8", html))
            }
        }
        "/about" => {
            if wants_md {
                let md = format!(
                    "# About Newsatoshi Commons\n\n\
                     Newsatoshi Commons (`{base_url}`) is a source-available (`PolyForm-Noncommercial-1.0.0`) public verification gateway and protocol toolkit built for the **Newsatoshi (`LOVE`)** post-quantum Layer-1 network (non-commercial use, auditing & research permitted; commercial use requires a separate commercial license).\n\n\
                     ## Architecture & Design Principles\n\
                     - **Zero Custody & Zero Server Keys:** Production runs in strict `--verify-only` mode with zero private signing keys on server hardware.\n\
                     - **Post-Quantum Cryptography End-to-End:** Uses NIST FIPS 205 SLH-DSA, stateful XMSS Merkle-Lamport trees, pure-SHA-256 PayWord hash-chains, and Winterfell 0.13 transparent STARK proofs with zero trusted setup.\n\
                     - **Honest Zero-Fake-Data Telemetry:** Every block height, difficulty, supply counter, lock verification, and treasury balance comes strictly from live cryptographic proofs and live L1 RPC/seed responses.\n\
                     - **Open Source & Auditable:** Full source code, CI test suites, legal-cryptographic airlock templates, and height-gated RFC-0021/RFC-0022 L1 upgrade prototypes are published openly on GitHub.\n"
                );
                Some(("text/markdown; charset=utf-8", md))
            } else {
                let html = wrap_public_html(
                    base_url,
                    "/about",
                    "About Newsatoshi Commons",
                    "Mission, zero-custody verify-only architecture, post-quantum cryptography, and open-source transparency of Newsatoshi Commons.",
                    "<h1>About Newsatoshi Commons</h1>\n\
                     <p>Newsatoshi Commons is a source-available (<code>PolyForm-Noncommercial-1.0.0</code>) cryptographic verification gateway and developer toolkit built for the <strong>Newsatoshi (<code>LOVE</code>)</strong> post-quantum Layer-1 cryptocurrency. Non-commercial auditing, verification, and research are freely permitted; commercial use requires a separate license (see <code>COMMERCIAL_LICENSE.md</code>).</p>\n\
                     <h2>Zero-Custody, Verify-Only Architecture</h2>\n\
                     <p>The live public deployment at <code>commons.newsatoshi.org</code> runs in strict <code>--verify-only</code> mode and holds zero private signing keys on server disks or memory. All land deed attestations and physical energy telemetry packets are signed offline using NIST FIPS 205 SLH-DSA or stateful XMSS keys and verified cryptographically alongside Winterfell 0.13 transparent STARK proofs.</p>\n\
                     <h2>Verifiable Open Source</h2>\n\
                     <p>All server code, smart contracts, Solana escrow codecs, AuxPoW Stratum merge-mining tools, and height-gated RFC-0021/RFC-0022 L1 hard-fork upgrade prototypes are published openly at <a href=\"https://github.com/theguysaccount/newsatoshi-commons\">github.com/theguysaccount/newsatoshi-commons</a> and <a href=\"https://github.com/theguysaccount/newsatoshi-rfc0021\">github.com/theguysaccount/newsatoshi-rfc0021</a>. Every counter starts at genuine zero and reflects real on-chain and cryptographic state only.</p>",
                );
                Some(("text/html; charset=utf-8", html))
            }
        }
        "/contact" => {
            if wants_md {
                let md = "# Contact Newsatoshi Commons\n\n\
                     Newsatoshi Commons is maintained openly on GitHub.\n\n\
                     ## Technical Support, Integrations & Bug Reports\n\
                     - GitHub Issues: https://github.com/theguysaccount/newsatoshi-commons/issues\n\
                     - RFC-0021 / RFC-0022 L1 Upgrade Issues: https://github.com/theguysaccount/newsatoshi-rfc0021/issues\n\
                     - Contact Email: commons@newsatoshi.org\n\n\
                     ## Security Disclosures\n\
                     If you discover a cryptographic, consensus, HTTP API, or bridge vulnerability, please open a private GitHub Security Advisory on `theguysaccount/newsatoshi-commons` or email `commons@newsatoshi.org` with reproduction steps and affected commit hashes so we can verify, patch, and add regression tests promptly.\n".to_string();
                Some(("text/markdown; charset=utf-8", md))
            } else {
                let html = wrap_public_html(
                    base_url,
                    "/contact",
                    "Contact Newsatoshi Commons",
                    "Technical support, integration inquiries, GitHub issue trackers, and responsible security disclosure channels for Newsatoshi Commons.",
                    "<h1>Contact Newsatoshi Commons</h1>\n\
                     <p>Newsatoshi Commons is developed and operated in the open. Developers, AI agent builders, Bitcoin merge-miners, and liquidity makers can reach the maintainers directly through our public engineering channels or email.</p>\n\
                     <h2>Engineering &amp; Integration Support</h2>\n\
                     <ul><li><strong>Commons, MCP &amp; Bridges Repository:</strong> <a href=\"https://github.com/theguysaccount/newsatoshi-commons/issues\">github.com/theguysaccount/newsatoshi-commons/issues</a></li><li><strong>RFC-0021 / RFC-0022 L1 Upgrade Repository:</strong> <a href=\"https://github.com/theguysaccount/newsatoshi-rfc0021/issues\">github.com/theguysaccount/newsatoshi-rfc0021/issues</a></li><li><strong>Direct Contact Email:</strong> <a href=\"mailto:commons@newsatoshi.org\">commons@newsatoshi.org</a></li></ul>\n\
                     <h2>Responsible Security Disclosure</h2>\n\
                     <p>We welcome independent cryptographic, smart-contract, Stratum, and HTTP API security reviews. Please report suspected vulnerabilities via a private GitHub Security Advisory on <code>theguysaccount/newsatoshi-commons</code> or by emailing <code>commons@newsatoshi.org</code> with affected file paths, commit hashes, and minimal reproduction steps.</p>",
                );
                Some(("text/html; charset=utf-8", html))
            }
        }
        "/privacy" => {
            if wants_md {
                let md = "# Privacy Policy — Newsatoshi Commons\n\n\
                     Effective Date: 2026-11-23\n\n\
                     ## Zero Tracking & Zero Personal Data Collection\n\
                     - Newsatoshi Commons (`commons.newsatoshi.org`) does **not** require user accounts, KYC documents, cookies, analytics trackers, or advertising pixels.\n\
                     - All public read APIs, MCP tools, HTTP 402 verifiers, and multi-chain atomic swap compilers operate statelessly without storing caller identities or IP profiles.\n\
                     - Private keys and swap preimages are never transmitted to or stored on this server (`verify_only: true`, zero server signing keys).\n\n\
                     ## Public Blockchain & Cryptographic Data\n\
                     Any transaction IDs, public hash-locks (`SHA256(preimage)`), public keys, STARK proofs, or signed hardware telemetry explicitly submitted to public endpoints become part of the transparent public audit log or underlying public blockchains (Newsatoshi, Bitcoin, Solana, EVM).\n\n\
                     Questions regarding privacy may be sent to `commons@newsatoshi.org` or opened on https://github.com/theguysaccount/newsatoshi-commons.\n".to_string();
                Some(("text/markdown; charset=utf-8", md))
            } else {
                let html = wrap_public_html(
                    base_url,
                    "/privacy",
                    "Privacy Policy — Newsatoshi Commons",
                    "Zero-cookie, zero-tracking, zero-KYC privacy policy for Newsatoshi Commons (commons.newsatoshi.org).",
                    "<h1>Privacy Policy — Newsatoshi Commons</h1>\n\
                     <p><strong>Effective Date:</strong> November 23, 2026. Newsatoshi Commons (<code>commons.newsatoshi.org</code>) is designed around cryptographic verification and data minimization rather than surveillance or user profiling.</p>\n\
                     <h2>Zero Cookies, Trackers, or Account PII</h2>\n\
                     <p>We do not set tracking cookies, run third-party ad or behavioral analytics scripts, collect names, require sign-up accounts, or request KYC identity documents. Public API reads, Streamable HTTP MCP tool invocations, HTTP 402 PayWord verifications, and stateless HTLC swap compilations are processed in memory without building caller profiles. Private keys and atomic swap preimages should never be sent to this server and are never stored by it.</p>\n\
                     <h2>Public Cryptographic Records &amp; Operational Logs</h2>\n\
                     <p>Data intentionally published to public endpoints — such as public hash-lock digests, verified L1 anchor hashes, STARK proofs, or offline-signed hardware attestations — is public by design for independent verification. Standard transient infrastructure request logs may be retained briefly by the hosting network strictly for DDoS mitigation and uptime reliability. For privacy inquiries, contact <a href=\"mailto:commons@newsatoshi.org\">commons@newsatoshi.org</a>.</p>",
                );
                Some(("text/html; charset=utf-8", html))
            }
        }
        _ => None,
    }
}

pub fn render_api_catalog_json(base_url: &str) -> Value {
    json!({
        "linkset": [
            {
                "anchor": format!("{base_url}/api"),
                "service-desc": [
                    { "href": format!("{base_url}/openapi.json"), "type": "application/openapi+json" }
                ],
                "service-doc": [
                    { "href": format!("{base_url}/llms-full.txt"), "type": "text/markdown" }
                ],
                "status": [
                    { "href": format!("{base_url}/api/health"), "type": "application/json" }
                ]
            },
            {
                "anchor": format!("{base_url}/mcp"),
                "service-desc": [
                    { "href": format!("{base_url}/.well-known/mcp/server-card.json"), "type": "application/json" }
                ],
                "service-doc": [
                    { "href": format!("{base_url}/llms.txt"), "type": "text/markdown" }
                ]
            }
        ]
    })
}

pub fn render_mcp_server_card_json(base_url: &str) -> Value {
    json!({
        "schema_version": "1.0",
        "name": "newsatoshi-commons",
        "title": "Newsatoshi Commons & Agent Transaction Interface",
        "description": "Post-quantum machine money (LOVE) MCP server: live Mainnet/Testnet chain status, trustless BTC/Lightning/Solana/Base HTLC atomic swap compiler, live dual-chain lock verifier, HTTP 402 PayWord verifier, and Winterfell STARK verifier.",
        "version": env!("CARGO_PKG_VERSION"),
        "transport": "streamable-http",
        "endpoint": format!("{base_url}/mcp"),
        "authentication": { "required": false, "type": "none" },
        "documentation_url": format!("{base_url}/llms-full.txt"),
        "repository": "https://github.com/theguysaccount/newsatoshi-commons",
        "tools": [
            "newsatoshi_get_network_status",
            "newsatoshi_get_commons_state",
            "newsatoshi_compile_btc_love_swap",
            "newsatoshi_verify_onchain_lock",
            "newsatoshi_verify_payword_402",
            "newsatoshi_verify_stark_proof",
            "newsatoshi_agent_quickstart",
            "newsatoshi_create_402_challenge",
            "newsatoshi_get_mining_economics",
            "newsatoshi_get_backing_proof",
            "newsatoshi_spawn_charter",
            "newsatoshi_list_charters",
            "newsatoshi_discover_services",
            "newsatoshi_list_bounties",
            "crypto_inspect_evm_contract",
            "crypto_bitcoin_mempool_oracle",
            "crypto_decode_bolt11_invoice",
            "crypto_quote_base_uniswap_v3",
            "crypto_verify_tx_receipt",
            "newsatoshi_inspect_trust_bond",
            "newsatoshi_inspect_rollup_vault",
            "newsatoshi_get_protocol_signaling",
            "newsatoshi_create_scoped_session_policy",
            "newsatoshi_launch_agent_mission",
            "newsatoshi_quote_launchpad_curve",
            "newsatoshi_inspect_mission_launchpad",
            "newsatoshi_format_swarm_task_claim",
            "newsatoshi_verify_zktls_proof",
            "newsatoshi_check_v2_cutover_readiness",
            "newsatoshi_compile_bonded_p2p_offer",
            "newsatoshi_get_nsusd_state",
            "newsatoshi_simulate_nsusd_stress",
            "newsatoshi_benchmark_l2_perfection_batch",
            "newsatoshi_generate_private_x402_middleware",
            "newsatoshi_verify_base_nsusd_escrow",
            "newsatoshi_estimate_paywords_savings",
            "newsatoshi_compile_payword_channel_setup",
            "newsatoshi_simulate_fair_curve_launch"
        ],
        "resources": [
            "newsatoshi://llms.txt",
            "newsatoshi://network/status",
            "newsatoshi://commons/state"
        ],
        "client_config_example": {
            "mcpServers": {
                "newsatoshi": {
                    "url": format!("{base_url}/mcp")
                }
            }
        }
    })
}

pub fn render_agent_skills_index_json(base_url: &str) -> Value {
    let digest = hex::encode(sha256(AGENT_SKILL_MARKDOWN.as_bytes()));
    json!({
        "schema_version": "1.0",
        "provider": "Newsatoshi Commons",
        "homepage": base_url,
        "skills": [
            {
                "name": "newsatoshi-agent-payments",
                "description": "Connect AI agents to Newsatoshi (LOVE) via Streamable HTTP MCP, HTTP 402 PayWord micropayments, non-interactive shielded nsd1 delivery addresses, and trustless BTC/Lightning/Solana/Base HTLC atomic swaps.",
                "url": format!("{base_url}/skills/newsatoshi-agent-payments/SKILL.md"),
                "sha256": digest,
                "version": env!("CARGO_PKG_VERSION"),
                "source_repository": "https://github.com/theguysaccount/newsatoshi-commons"
            }
        ]
    })
}

pub fn render_llms_txt(base_url: &str, state: &ApiStateView, seed: &SeedSnapshot) -> String {
    let mainnet_line = match &seed.mainnet_status {
        Some(m) => format!(
            "Mainnet LIVE at height #{} (difficulty {:.2}, {} LOVE issued, {} locks, {} peers, AuxPoW chainid {})",
            m.height,
            m.difficulty,
            m.issued / 100_000_000,
            m.locks,
            m.peers,
            m.auxpow_chainid
        ),
        None => format!(
            "Mainnet genesis {} (AuxPoW chainid {}, 50 LOVE/block)",
            MAINNET_GENESIS_HASH, AUXPOW_CHAIN_ID
        ),
    };
    let testnet_line = match &seed.status {
        Some(t) => format!(
            "Testnet LIVE at height #{} ({} locks, {} peers, faucet enabled at https://newsatoshi-seed.fly.dev/api/faucet)",
            t.height, t.locks, t.peers
        ),
        None => format!("Testnet genesis {}", TESTNET_GENESIS_HASH),
    };

    format!(
        "\
# Newsatoshi Commons & Agent Transaction Interface ({proto})

> **The Hourglass Architecture:** Newsatoshi (`LOVE`) keeps **Layer 1 (`v1.2.1`)** radically small — private, quantum-proof electronic cash secured by Bitcoin merge-mining (`chainid = 20051`) and built strictly on SHA-2 (256-bit Quartic Goldilocks FRI at `100–104` conservative QROM bits, full 249-constraint Lean 4 circuit proof, real pure-SHA-2 STARK-in-STARK IVC verifier, zero elliptic curves, zero trusted setup, zero L1 launchpads or subjective juries). **This gateway (`commons.newsatoshi.org`) lives above the waist at Layer 2** — providing 33 zero-auth MCP tools, RFC-0028 `nsUSD` Dual-Tier stablecoin engine + live stress simulator, L2 Shielded Rollup & Perfection Layer (Winterfell STARK batching + unilateral Merkle escape hatches), Native `LOVE` PayWords + `x402 v2` USDC bridge on-ramps, zero-custody BTC/Lightning/Solana/EVM HTLC atomic swaps, `ERC-8004` identity, Web4 charters, off-chain mission launchpads, zkTLS P2P verification, and A2A escrow hooks.

## When to use Newsatoshi Commons
- Use `{base_url}/mcp` or `{base_url}/api/*` when an agent needs to **check live Newsatoshi Mainnet/Testnet state**, **bridge BTC/Lightning/Solana/Base into LOVE trustlessly**, **run live RFC-0028 `nsUSD` stablecoin quotes/stress tests or L2 STARK rollup benchmarks**, **issue or verify HTTP 402 PayWord micropayments**, or **cryptographically verify Winterfell STARK covenant proofs and L1 hash-locks**.
- Human & developer documentation: [Whitepaper & Lean 4 Proof]({base_url}/whitepaper), [nsUSD Stablecoin (RFC-0028)]({base_url}/nsusd), [L2 Perfection Layer]({base_url}/l2), [Bug Bounty]({base_url}/bounty), [API Docs]({base_url}/docs), [Developer Portal]({base_url}/developers), [About]({base_url}/about), [Contact]({base_url}/contact), [Privacy]({base_url}/privacy).

## Live Network Status
- {mainnet_line}
- {testnet_line}
- Live Verified EVM HTLC (`Base 8453 · Arbitrum 42161 · Optimism 10 · Polygon 137`, ownerless, all 4 Sourcify `exact_match`): `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987` (Native ETH/POL & Canonical USDC)
- Live Verified Multi-Chain Proof-of-Backing Vault (`Base 8453 · Arbitrum 42161 · Optimism 10 · Polygon 137`, ownerless, 0 admin withdraw, all 4 Sourcify `exact_match`): `{backing_vault}` (`GET {base_url}/api/backing/summary`)
- Official MCP Registry (`active`): `org.newsatoshi.commons/newsatoshi-commons` (`/.well-known/erc8004.json` ERC-8004 dual identity)
- Commons Covenant Rule Hash: `{rule_hash}` (`verify_only={verify_only}`, `{key_mode}`)
- Commons Treasury Balance: `{bal} LOVE` (`nonce={nonce}`, `parcels={parcels}`, `kwh={kwh}`, `starks={starks}`)

## Agent Interfaces (Zero Auth Required for All Reads, Compilers & Verifiers)
- [Streamable HTTP MCP Server]({base_url}/mcp): JSON-RPC 2.0 MCP endpoint (`initialize`, `tools/list`, `tools/call`, `resources/list`, `resources/read`) with **38 live tools** (including Base Mainnet `O(1)` SHA-256 PayWordChannel cost-savings & Cloudflare/Express/FastAPI setup kit, dilution-proof Fair Bonding-Curve Launchpad simulator, 60-second private `x402`/`nsUSD` paywall middleware generator, live Base USDC vault escrow verifier, `nsUSD` Dual-Tier stablecoin engine + stress suite, L2 Perfection Layer STARK batch benchmark, 5 everyday zero-API-key EVM/BTC/Lightning/Uniswap tools, `v1.2.5` Mainnet v2 cutover readiness gate, client-side mission/charter/zkTLS coordination compilers, and Bonded P2P Desk tools).
- **Zero-Install Stdio MCP Bridge & CLI (Node 18+, Zero Dependencies):** `npx -y github:theguysaccount/newsatoshi-commons` (`inspect`, `btc-fees`, `decode-bolt11`, `quote-base`, `verify-tx`, `payword-demo`, or stdio MCP `serve`).
- [Native LOVE PayWord & x402 v2 Discovery]({base_url}/.well-known/x402): 4 live monetized endpoints settling primarily in **native `LOVE`** (`<1ms` zero-gas SHA-256 PayWords / `PriorLock`), with Base/Arbitrum/Polygon USDC (`x402 v2`, `$0.001`) supported as outer bridge on-ramps (`POST /api/x402/evm-security-scan`, `GET /api/x402/bridge-quote`, `POST /api/x402/compile-htlc`, `POST /api/x402/verify-payword`).
- [MCP Server Card]({base_url}/.well-known/mcp/server-card.json) & [MCP Registry Manifest]({base_url}/server.json): Standard MCP discovery cards (`org.newsatoshi.commons/newsatoshi-commons`).
- [A2A Agent Card]({base_url}/.well-known/agent-card.json) (and legacy `/.well-known/agent.json`): Machine-readable Agent-to-Agent capability manifest.
- [RFC 9727 API Catalog]({base_url}/.well-known/api-catalog) & [OpenAPI 3.1 Schema]({base_url}/openapi.json): Complete REST API schema for multi-chain swaps, dual-chain lock verification, HTTP 402 PayWord verification, STARK verification, and offline signed telemetry/deeds.
- [Agent Skills Index]({base_url}/.well-known/agent-skills/index.json): SHA-256 verified installable skill ([SKILL.md]({base_url}/skills/newsatoshi-agent-payments/SKILL.md)).
- [Live 0%-Fee Bitaxe / AuxPoW Solo Pool](https://pool.newsatoshi.org/status): `stratum+tcp://pool.newsatoshi.org:3333` (direct solo coinbase payouts to any Mainnet `ns1…` address from `https://newsatoshi.org/wallet/`).
- [Full Agent Transaction Guide]({base_url}/llms-full.txt): End-to-end 60-second setup recipes (wallet creation, free testnet faucet, BTC/Lightning/Solana/Base ↔ LOVE swaps, PayWord 402 streaming, L1 compute jobs, AuxPoW mining).
- [XML Sitemap]({base_url}/sitemap.xml)

## 60-Second Agent Quickstart
1. **Connect MCP:** point any MCP client at `{base_url}/mcp` (Streamable HTTP) and call `newsatoshi_agent_quickstart`.
2. **Create a shielded delivery wallet (zero interactive handshakes):**
   `newsatoshi --net testnet address --delivery` (or `--net mainnet` for production).
3. **Get free testnet LOVE for instant sandbox testing:**
   `curl -sS -X POST https://newsatoshi-seed.fly.dev/api/faucet -H 'Content-Type: application/json' -d '{{\"address\":\"<YOUR_NSD1_ADDRESS>\"}}'`
4. **Acquire Mainnet LOVE trustlessly from Bitcoin, Lightning, Solana, or Base/EVM (zero exchanges/KYC):**
   Call `POST {base_url}/api/swaps/compile` (or MCP tool `newsatoshi_compile_btc_love_swap`) with `rail`: `btc_onchain`, `lightning_bolt11`, `solana_htlc`, or `evm_htlc`.
5. **Pay or charge per API call with HTTP 402 PayWord micropayments:**
   Request a challenge at `GET {base_url}/api/agent/402-challenge` and verify pure-SHA-256 PayWord tokens (`SHA256^ticks(token) == tip`) or live L1 hash-locks at `POST {base_url}/api/agent/402-verify`.

## Source & Releases
- Repository: https://github.com/theguysaccount/newsatoshi-commons
- Installable Agent Skill: `skills/newsatoshi-agent-payments/SKILL.md`
- Minimalist Layer-1 Narrow Waist (`v1.2.4`: Private, SHA-2-only, >=100-bit QROM, Bitcoin AuxPoW merge-mined settlement floor): https://github.com/theguysaccount/newsatoshi-rfc0021
",
        proto = state.protocol_version,
        backing_vault = CANONICAL_BACKING_VAULT,
        rule_hash = state.covenant_rule_hash,
        verify_only = state.verify_only,
        key_mode = if state.verify_only || !state.signing_keys_loaded {
            "0 server signing keys"
        } else {
            "local dev signing keys loaded"
        },
        bal = state.ledger_summary.balance_love,
        nonce = state.ledger_summary.tx_nonce,
        parcels = state.ledger_summary.parcels_acquired_count,
        kwh = state.ledger_summary.total_kwh_verified,
        starks = state.ledger_summary.stark_proofs_verified,
    )
}

pub fn render_llms_full_txt(base_url: &str, state: &ApiStateView, seed: &SeedSnapshot) -> String {
    let summary = render_llms_txt(base_url, state, seed);
    format!(
        "\
{summary}
---

## Detailed Agent Transaction Recipes

### Recipe 1: Connect via Model Context Protocol (MCP)
Add to Claude Desktop, Cursor, Codex, Antigravity, or any Streamable HTTP MCP host:
```json
{{
  \"mcpServers\": {{
    \"newsatoshi\": {{
      \"url\": \"{base_url}/mcp\"
    }}
  }}
}}
```
Available MCP tools (38 live tools — zero API keys required):
- `crypto_inspect_evm_contract` — Live multi-chain EVM smart contract security audit (bytecode, EIP-1967 proxy/admin slots, `owner()` admin risk, ERC-20 metadata, Sourcify v2 exact-match verification across Ethereum, Base, Arbitrum, Optimism & Polygon)
- `crypto_bitcoin_mempool_oracle` — Live Bitcoin chain tip, recommended sat/vB fee tiers (`fastest`, `30m`, `60m`, `economy`), and mempool backlog
- `crypto_decode_bolt11_invoice` — Pure-Rust BOLT-11 Lightning invoice decoder (network, sats/msats, payment hash, expiry, description)
- `crypto_quote_base_uniswap_v3` — Live Base Mainnet Uniswap V3 `QuoterV2` quote across fee tiers (`ETH`, `WETH`, `USDC`, `wLOVE`, or any ERC-20)
- `crypto_verify_tx_receipt` — Live EVM transaction receipt & confirmation verifier across Ethereum, Base, Arbitrum, Optimism & Polygon
- `newsatoshi_get_network_status` — Live Mainnet & Testnet chain status
- `newsatoshi_get_commons_state` — Live Commons treasury, covenant rule hash, public roots, STARKs & anchors
- `newsatoshi_compile_btc_love_swap` — Compile canonical BIP-199 Bitcoin P2WSH HTLC (`bc1q…`), decode Lightning BOLT-11, Solana PDA escrow, or live 4-chain `NewsatoshiHtlc.sol` (`0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`)
- `newsatoshi_verify_onchain_lock` — Verify a SHA-256 lock live on Newsatoshi Mainnet/Testnet + Bitcoin `mempool.space`
- `newsatoshi_verify_payword_402` — Verify a pure-SHA-256 PayWord HTTP 402 micropayment token (`SHA256^ticks(token) == tip`)
- `newsatoshi_verify_stark_proof` — Cryptographically re-verify a Winterfell STARK covenant transition proof
- `newsatoshi_agent_quickstart` — Structured onboarding commands for autonomous agents
- `newsatoshi_create_402_challenge` — Generate a standard `NEWSATOSHI-402/1.0` + `x402 v2` Payment Required challenge
- `newsatoshi_get_mining_economics` — Live AuxPoW merge-mining yield & public `0%`-fee solo pool (`stratum+tcp://pool.newsatoshi.org:3333`) stats
- `newsatoshi_get_backing_proof` — Live 4-chain `NewsatoshiBackingVault.sol` (`0x37Ed94525DBE408fA72cDd156409875666679265`) Proof-of-Backing reserves
- `newsatoshi_spawn_charter` — Compile a deterministic Web4 Sovereign Business Charter with Lean 4 Theorem 8 monotonic child bounds
- `newsatoshi_list_charters` — List registered Web4 charters and parent-child covenant lineage
- `newsatoshi_discover_services` — Discover live HTTP 402 PayWord & `x402 v2` monetized endpoints
- `newsatoshi_list_bounties` — List zero-custody HTLC & Freivalds matrix compute bounties
- `newsatoshi_inspect_trust_bond` — Legacy RFC-0024 query helper (superseded in L1 `v1.0.0-core` by unified `PriorLock` at `/api/prior?id=...`)
- `newsatoshi_inspect_rollup_vault` — Legacy RFC-0023 query helper (stripped from L1 consensus in `v1.0.0-core` to keep L1 minimal and eliminate unconstrained vault surfaces)
- `newsatoshi_get_protocol_signaling` — Query live BIP-9 AuxPoW-safe signaling & ossification telemetry (`/api/signaling`)
- `newsatoshi_create_scoped_session_policy` — Compile a deterministic client-side scoped AI agent spending session policy
- `newsatoshi_launch_agent_mission` — Compile an off-chain/L2 Mission Launchpad & Decentralized AI Agent Swarm charter (stripped from L1 consensus in `v1.0.0-core`; coordinates over native `LOVE` HTLCs / `PriorLock`s)
- `newsatoshi_quote_launchpad_curve` — Compute exact integer bonding-curve buy/redeem quotes for off-chain/L2 mission launchpads
- `newsatoshi_inspect_mission_launchpad` — Legacy `/api/mission` query helper (L1 `v1.0.0-core` exposes unified `/api/prior?id=...` instead)
- `newsatoshi_format_swarm_task_claim` — Compute deterministic `nsMISSTASK` & `nsMISSCLAIM` signing digests for off-chain/L2 agent swarm coordinators
- `newsatoshi_verify_zktls_proof` — Cryptographically verify real `reclaim_v1` (EIP-191 secp256k1 ecrecover), `tlsnotary_v1` (Ed25519/secp256k1 + Merkle selective redaction), or `newsatoshi_pq_notary_v1` (FIPS 205 SLH-DSA) zkTLS proofs for off-chain/P2P verification
- `newsatoshi_check_v2_cutover_readiness` — Live `v1.2.1` Mainnet v2 status, formal/STARK security bounds, independent Bitcoin AuxPoW pool adoption, and LOVE Public Security Bug Bounty telemetry
- `newsatoshi_compile_bonded_p2p_offer` — Compile a zero-custody `PriorLock`-backed zkTLS P2P Fiat/USDC <-> LOVE market-maker quote (100%+ stake coverage required)
- `newsatoshi_get_nsusd_state` — Live honest RFC-0028 `nsUSD` Dual-Tier stablecoin engine state (Sovereign BTC CDP `asset_id=1` + isolated Bridged `nsUSD-B` `asset_id=2` + `PriorLock` oracle status)
- `newsatoshi_simulate_nsusd_stress` — Run the 4-scenario isolated adversarial `nsUSD` stress-test suite (flash-crash liquidation, `$1.00` bank-run redemption, `+40%` oracle spoof 50/50 slash, stale circuit-breaker halt)
- `newsatoshi_benchmark_l2_perfection_batch` — Run a live measured L2 PayWord + `nsUSD` batch + Winterfell 0.13 STARK conservation proof + verified unilateral Merkle L1 Escape Hatch benchmark
- `newsatoshi_generate_private_x402_middleware` — Generate copy-paste-ready Express, Next.js, FastAPI, or Rust Axum HTTP 402 middleware accepting private zero-gas `O(1)` SHA-256 PayWords (`nsUSD` USD-quoted) + Base USDC `x402 v2` fallback
- `newsatoshi_verify_base_nsusd_escrow` — Verify any real Base Mainnet USDC deposit tx hash live via Base JSON-RPC against the immutable `NewsatoshiBackingVault` (`0x37Ed94525DBE408fA72cDd156409875666679265`) into an auditable RFC-0028 Tier-1B `nsUSD-B` 1:1 receipt
- `newsatoshi_estimate_paywords_savings` — Calculate exact monthly & annual payment-fee savings comparing Stripe (`2.9% + $0.30`) & per-call Base x402 vs. Newsatoshi `O(1)` SHA-256 PayWords on live Base Mainnet (`0x971BA814Df91c20DcA3aDd870C2B55d643965C50`)
- `newsatoshi_compile_payword_channel_setup` — Generate org-ready Base USDC `O(1)` SHA-256 PayWords HTTP 402 server middleware (Cloudflare Worker, Express, FastAPI) + `cast` settlement commands wired to live `NewsatoshiPayWordChannel`
- `newsatoshi_simulate_fair_curve_launch` — Simulate exact integer constant-product USDC bonding-curve math + dilution-proof `injectRevenue()` PayWords dividends (Lean 4 Theorems 10–11, 0% premine, 0% platform cut) and emit 1-command deploy CLI for live Base Mainnet `NewsatoshiFairCurveFactory` (`0xF6bE782819a8eD0D1556A8f5DC3223f9eb13a933`)

Stdio MCP Bridge (for stdio-only hosts such as Claude Desktop, Cursor, Cline, or shell scripts):
```json
{{
  \"mcpServers\": {{
    \"newsatoshi\": {{
      \"command\": \"npx\",
      \"args\": [\"-y\", \"github:theguysaccount/newsatoshi-commons\"]
    }}
  }}
}}
```

### Recipe 2: Zero-Human Wallet Setup & Shielded Agent Payments
Newsatoshi supports non-interactive shielded delivery addresses (`nsd1…`): any sender can pay your agent on-chain privately without out-of-band note passing.
```bash
# 1. Print your agent's non-interactive shielded delivery address
newsatoshi --net mainnet address --delivery

# 2. Scan for incoming private payments & check spendable balance
newsatoshi --net mainnet scan
newsatoshi --net mainnet balance

# 3. Pay another agent privately (zero transparent address graph)
newsatoshi --net mainnet send <PEER_NSD1_ADDRESS> 1.5 --memo \"inference-job-42\"
```

### Recipe 3: Trustless `BTC ↔ LOVE` Atomic Swaps (On-Chain P2WSH or Lightning BOLT-11)
Both Bitcoin (`OP_SHA256` / BOLT-11) and Newsatoshi L1 (`lock --hash`) enforce the identical `SHA256(preimage) == payment_hash` invariant.
```bash
# Generate a local 32-byte CSPRNG preimage + SHA-256 hash (keep preimage secret!)
newsatoshi-swap keygen

# Compile canonical BIP-199 Bitcoin P2WSH HTLC + Newsatoshi L1 lock CLI
curl -sS -X POST {base_url}/api/swaps/compile \\
  -H 'Content-Type: application/json' \\
  -d '{{
    \"rail\": \"btc_onchain\",
    \"direction\": \"btc_to_love\",
    \"love_network\": \"mainnet\",
    \"btc_network\": \"mainnet\",
    \"payment_hash_hex\": \"<64HEX_HASH>\",
    \"amount_love\": \"250\",
    \"amount_btc_sats\": 500000,
    \"love_claim_address\": \"<NS1_OR_NSD1_ADDRESS>\",
    \"love_timeout_blocks\": 144,
    \"btc_claim_pubkey_hex\": \"<33B_COMPRESSED_HEX>\",
    \"btc_refund_pubkey_hex\": \"<33B_COMPRESSED_HEX>\",
    \"btc_locktime\": 900036,
    \"btc_current_height\": 900000
  }}'

# Verify both chains live (Newsatoshi L1 /api/lock + Bitcoin mempool.space)
curl -sS -X POST {base_url}/api/swaps/verify \\
  -H 'Content-Type: application/json' \\
  -d '{{\"payment_hash_hex\":\"<64HEX_HASH>\",\"love_network\":\"mainnet\",\"btc_address\":\"bc1q...\"}}'
```

### Recipe 4: HTTP 402 Machine-to-Machine PayWord Micropayments (Zero Gas Per Call)
Ideal for per-token LLM inference, tool calls, sensor feeds, and data APIs where on-chain transactions per call are too slow/heavy:
1. Payer agent picks a 32-byte secret `seed`, hashes it `N` times (`tip = SHA256^N(seed)`), and commits `tip` in a single escrow/channel.
2. For call `#k`, payer reveals `token = SHA256^(N - k)(seed)` in `X-Newsatoshi-PayWord: tip=<tip>,token=<token>,ticks=<k>`.
3. Server verifies `SHA256^k(token) == tip` locally or via `POST {base_url}/api/agent/402-verify` in microseconds (post-quantum pure SHA-256).

### Recipe 5: Mainnet v2 (`nsM2`, `v1.2.1`) Independent Verification, Utreexo & STARK-in-STARK IVC
Verify live Mainnet v2 (`00000000ba1cdad4…`, anchored to Bitcoin `#977,738`) with zero trust in any server:
```bash
# Join Mainnet v2 with FIPS 205 SLH-DSA NSPQ2 peer pinning (blocks active MITM):
newsatoshi --net mainnet-v2 node \
  --peer newsatoshi-seed-v2.fly.dev:8445@b024aaeafa0e4ca384b99483c28c3bedde45a5461a560063b283766414dfbbcc \
  --peer pool.newsatoshi.org:8445@78187a9436dd2d8cc7ed7afff2396b72b56687099d45d93da676b0a90ed8c6b6

# Verify O(log N) FlyClient MMR, <= 1 KB Utreexo frontier, or pure-SHA-2 STARK-in-STARK IVC locally:
newsatoshi --net mainnet-v2 flyclient
newsatoshi --net mainnet-v2 utreexo-frontier
newsatoshi ivc-prove

# Zero-Rust-code independent JS/OpenSSL verifier live against Mainnet v2:
node verifier-ts/newsatoshi_verifier.mjs --live https://newsatoshi.org/mainnet
```

### Recipe 6: Cryptographically Bounded Agent Budgets (`NEWSATOSHI/2.1` Covenants)
Humans can fund a Commons/Covenant UTXO with immutable caps (`max_bounty_payout`, `reinvest_bps`, `min_soil_health_score`) and offline **NIST FIPS 205 SLH-DSA** hardware keys (`newsatoshi-hw-signer --env-seeds`), guaranteeing an autonomous agent cannot exceed its spending policy even if prompt-injected.
"
    )
}

pub fn render_agent_card_json(base_url: &str, state: &ApiStateView) -> Value {
    json!({
        "schema_version": "1.0",
        "name": "Newsatoshi Commons & Multi-Chain Crypto Agent Gateway",
        "description": "Zero-API-key multi-chain EVM security audits, Bitcoin mempool fees, Lightning BOLT-11 decoding, Base Uniswap V3 quotes, dual x402 v2 + zero-gas post-quantum SHA-256 PayWord micropayments, and trustless Newsatoshi (LOVE) HTLC atomic swaps.",
        "url": base_url,
        "version": env!("CARGO_PKG_VERSION"),
        "protocol_version": state.protocol_version,
        "documentation_url": format!("{base_url}/llms.txt"),
        "openapi_url": format!("{base_url}/openapi.json"),
        "mcp_endpoint": format!("{base_url}/mcp"),
        "x402_discovery_url": format!("{base_url}/.well-known/x402"),
        "defaultInputModes": ["application/json", "text/plain"],
        "defaultOutputModes": ["application/json", "text/markdown"],
        "authentication": {
            "read_and_verify": "none",
            "x402_paid_routes": "x402_v2_usdc_or_sha256_payword",
            "operator_write": "bearer_token",
            "hardware_and_deed_attestation": "offline_nist_fips205_slh_dsa_or_xmss"
        },
        "capabilities": {
            "mcp_streamable_http": true,
            "mcp_stdio_bridge_npx": "npx -y github:theguysaccount/newsatoshi-commons",
            "x402_v2_usdc_multichain": true,
            "http_402_payword_micropayments": true,
            "evm_contract_security_audit": true,
            "bitcoin_mempool_oracle": true,
            "base_uniswap_v3_quoter": true,
            "btc_love_htlc_atomic_swaps": true,
            "bolt11_lightning_decoding": true,
            "dual_chain_l1_verification": true,
            "winterfell_stark_verification": true,
            "verify_only_zero_server_keys": state.verify_only
        },
        "skills": [
            {
                "id": "crypto_inspect_evm_contract",
                "name": "Multi-Chain EVM Smart Contract Security Audit",
                "description": "Zero-API-key live audit of any contract on Ethereum, Base, Arbitrum, Optimism, or Polygon: bytecode size, EIP-1967 proxy/admin slots, active owner() risk, ERC-20 metadata/supply, and Sourcify v2 exact-match verification."
            },
            {
                "id": "crypto_bitcoin_mempool_oracle",
                "name": "Bitcoin Mempool & Fee Oracle",
                "description": "Live Bitcoin tip height, hash, sat/vB fee tiers (fastest, 30m, 60m, economy), and mempool backlog with zero API keys."
            },
            {
                "id": "crypto_decode_bolt11_invoice",
                "name": "Lightning BOLT-11 Invoice Decoder",
                "description": "Pure-Rust BOLT-11 invoice parser returning network, sats/msats, SHA-256 payment hash, expiry, and description."
            },
            {
                "id": "crypto_quote_base_uniswap_v3",
                "name": "Base Mainnet Uniswap V3 Live Quoter",
                "description": "Live on-chain Base Uniswap V3 QuoterV2 quote across fee tiers for ETH, WETH, USDC, wLOVE, or arbitrary ERC-20 tokens."
            },
            {
                "id": "crypto_verify_tx_receipt",
                "name": "Multi-Chain EVM Transaction Receipt Verifier",
                "description": "Live receipt status, block confirmations, gas used, and log count across Ethereum, Base, Arbitrum, Optimism, and Polygon."
            },
            {
                "id": "newsatoshi_get_network_status",
                "name": "Live Newsatoshi Mainnet & Testnet Status",
                "description": "Returns live heights, difficulty, issued LOVE, locks, peers, and AuxPoW chainid (20051) for both Mainnet and Testnet."
            },
            {
                "id": "newsatoshi_compile_btc_love_swap",
                "name": "Trustless BTC / Lightning / Solana / Base <-> LOVE Atomic Swap Compiler",
                "description": "Compiles canonical BIP-199 Bitcoin P2WSH HTLC scripts/addresses (bc1q...), Lightning BOLT-11, Solana PDA escrow, or 4-chain EVM HTLCs into Newsatoshi L1 hash-locks."
            },
            {
                "id": "newsatoshi_verify_onchain_lock",
                "name": "Live Dual-Chain On-Chain Lock Verifier",
                "description": "Verifies SHA-256 hash-locks live on Newsatoshi L1 (/api/lock?hash=), Bitcoin P2WSH UTXOs (mempool.space), and revealed preimages."
            },
            {
                "id": "newsatoshi_verify_payword_402",
                "name": "HTTP 402 PayWord Micropayment Verifier",
                "description": "Verifies pure-SHA-256 post-quantum PayWord hash-chain payment tokens (SHA256^ticks(token) == tip) for sub-millisecond zero-gas agent billing."
            }
        ],
        "covenant_rule_hash": state.covenant_rule_hash,
        "auxpow_chainid": AUXPOW_CHAIN_ID
    })
}

pub fn render_mcp_manifest_json(base_url: &str) -> Value {
    json!({
        "schema_version": "1.0",
        "name": "newsatoshi-commons",
        "version": env!("CARGO_PKG_VERSION"),
        "transport": "streamable-http",
        "endpoint": format!("{base_url}/mcp"),
        "auth_required": false,
        "docs": format!("{base_url}/llms.txt"),
        "client_config_example": {
            "mcpServers": {
                "newsatoshi": {
                    "url": format!("{base_url}/mcp")
                }
            }
        }
    })
}

pub fn render_openapi_json(base_url: &str) -> Value {
    let err_ref = json!({
        "description": "RFC 9457 Problem Details error",
        "content": {
            "application/problem+json": {
                "schema": { "$ref": "#/components/schemas/ProblemDetails" }
            }
        }
    });
    let api_ver_param = json!({
        "name": "X-API-Version",
        "in": "header",
        "required": false,
        "description": "Optional API version pin (current: 2026-11). Breaking changes carry a 90-day Deprecation/Sunset window.",
        "schema": { "type": "string", "default": "2026-11" }
    });
    let json_ok = |desc: &str, schema_ref: &str| {
        json!({
            "description": desc,
            "headers": {
                "X-API-Version": { "schema": { "type": "string" }, "description": "API version identifier (2026-11)" },
                "RateLimit-Policy": { "schema": { "type": "string" }, "description": "RFC RateLimit policy (120;w=60)" }
            },
            "content": {
                "application/json": {
                    "schema": { "$ref": schema_ref }
                }
            }
        })
    };

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Newsatoshi Commons & Agent Transaction API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Machine-readable REST + Streamable HTTP MCP + HTTP 402 PayWord + Multi-Chain (BTC, Lightning, Solana, Base/EVM) SHA-256 HTLC Atomic Swap interface for Newsatoshi (LOVE) and NEWSATOSHI/2.1 STARK covenants.",
            "contact": {
                "name": "Newsatoshi Commons",
                "url": format!("{base_url}/contact"),
                "email": "commons@newsatoshi.org"
            },
            "license": {
                "name": "PolyForm-Noncommercial-1.0.0",
                "url": "https://github.com/theguysaccount/newsatoshi-commons/blob/main/LICENSE"
            },
            "x-versioning-policy": format!("{base_url}/docs#versioning"),
            "x-deprecation-policy": "Minimum 90-day advance notice via HTTP Deprecation and Sunset headers before retiring any endpoint or schema field."
        },
        "servers": [{ "url": base_url, "description": "Production verify-only Newsatoshi Commons gateway" }],
        "components": {
            "schemas": {
                "ProblemDetails": {
                    "type": "object",
                    "required": ["type", "title", "status", "detail"],
                    "properties": {
                        "type": { "type": "string", "format": "uri" },
                        "title": { "type": "string" },
                        "status": { "type": "integer" },
                        "detail": { "type": "string" },
                        "error": { "type": "string" }
                    }
                },
                "HealthResponse": {
                    "type": "object",
                    "required": ["ok", "version", "protocol_version", "verify_only", "signing_keys_loaded", "uptime_secs"],
                    "properties": {
                        "ok": { "type": "boolean" },
                        "version": { "type": "string" },
                        "protocol_version": { "type": "string" },
                        "verify_only": { "type": "boolean" },
                        "signing_keys_loaded": { "type": "boolean" },
                        "auth_required": { "type": "boolean" },
                        "uptime_secs": { "type": "integer" }
                    }
                },
                "GenericApiResponse": {
                    "type": "object",
                    "properties": {
                        "ok": { "type": "boolean" },
                        "error": { "type": "string" }
                    },
                    "additionalProperties": true
                },
                "McpRpcRequest": {
                    "type": "object",
                    "required": ["jsonrpc", "method"],
                    "properties": {
                        "jsonrpc": { "type": "string", "const": "2.0" },
                        "id": { "oneOf": [{ "type": "string" }, { "type": "integer" }] },
                        "method": { "type": "string", "enum": ["initialize", "ping", "tools/list", "tools/call", "resources/list", "resources/read"] },
                        "params": { "type": "object" }
                    }
                },
                "SwapCompileRequest": {
                    "type": "object",
                    "required": ["rail", "payment_hash_hex", "amount_love", "love_claim_address"],
                    "properties": {
                        "rail": { "type": "string", "enum": ["btc_onchain", "lightning_bolt11", "solana_htlc", "evm_htlc"] },
                        "direction": { "type": "string", "enum": ["btc_to_love", "love_to_btc"] },
                        "love_network": { "type": "string", "enum": ["mainnet", "testnet"] },
                        "payment_hash_hex": { "type": "string", "minLength": 64, "maxLength": 64 },
                        "amount_love": { "type": "string" },
                        "love_claim_address": { "type": "string" },
                        "love_timeout_blocks": { "type": "integer", "minimum": 12 }
                    }
                },
                "SwapVerifyRequest": {
                    "type": "object",
                    "required": ["payment_hash_hex"],
                    "properties": {
                        "payment_hash_hex": { "type": "string", "minLength": 64, "maxLength": 64 },
                        "love_network": { "type": "string", "enum": ["mainnet", "testnet"] },
                        "btc_address": { "type": "string" },
                        "preimage_hex": { "type": "string" }
                    }
                },
                "Http402ChallengeResponse": {
                    "type": "object",
                    "required": ["status", "protocol", "resource", "network", "amount_love", "payment_hash_hex", "rails"],
                    "properties": {
                        "status": { "type": "integer", "const": 402 },
                        "protocol": { "type": "string" },
                        "resource": { "type": "string" },
                        "network": { "type": "string", "enum": ["mainnet", "testnet"] },
                        "amount_love": { "type": "string" },
                        "payment_hash_hex": { "type": "string" },
                        "rails": { "type": "object" }
                    }
                },
                "Http402VerifyRequest": {
                    "type": "object",
                    "properties": {
                        "tip_hex": { "type": "string" },
                        "token_hex": { "type": "string" },
                        "ticks": { "type": "integer" },
                        "unit_price_base_units": { "type": "integer" },
                        "payment_hash_hex": { "type": "string" },
                        "network": { "type": "string", "enum": ["mainnet", "testnet"] }
                    }
                }
            }
        },
        "paths": {
            "/mcp": {
                "post": {
                    "operationId": "invokeMcpRpc",
                    "summary": "Streamable HTTP MCP JSON-RPC 2.0 endpoint",
                    "description": "Executes MCP JSON-RPC 2.0 methods (initialize, ping, tools/list, tools/call, resources/list, resources/read) with zero authentication required.",
                    "parameters": [api_ver_param.clone()],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/McpRpcRequest" } } }
                    },
                    "responses": {
                        "200": json_ok("JSON-RPC 2.0 response", "#/components/schemas/GenericApiResponse"),
                        "400": err_ref.clone()
                    }
                }
            },
            "/api/health": {
                "get": {
                    "operationId": "getHealth",
                    "summary": "Server health, version, and verify-only key-isolation status",
                    "description": "Returns server version, uptime, and cryptographic verify-only key-isolation flags.",
                    "parameters": [api_ver_param.clone()],
                    "responses": {
                        "200": json_ok("Health status", "#/components/schemas/HealthResponse"),
                        "500": err_ref.clone()
                    }
                }
            },
            "/api/seed": {
                "get": {
                    "operationId": "getNetworkSeed",
                    "summary": "Live dual-chain Newsatoshi L1 Testnet + Mainnet status",
                    "description": "Returns live block height, tip hash, difficulty, issued LOVE, open locks, peers, and AuxPoW chainid 20051 for Mainnet and Testnet.",
                    "parameters": [api_ver_param.clone()],
                    "responses": {
                        "200": json_ok("Live chain telemetry", "#/components/schemas/GenericApiResponse"),
                        "500": err_ref.clone()
                    }
                }
            },
            "/api/network/metrics": {
                "get": {
                    "operationId": "getNetworkMetrics",
                    "summary": "Live derived Newsatoshi Mainnet/Testnet hashrate, AuxPoW dominance, supply, and merge-mining yield",
                    "description": "Computes live estimated network hashrate, observed block interval, AuxPoW merge-mined percentage, supply mined percentage, and snapshot LOVE/day yield for a given miner hashrate_ths.",
                    "parameters": [
                        api_ver_param.clone(),
                        { "name": "hashrate_ths", "in": "query", "required": false, "description": "Miner SHA-256 hashrate in TH/s (default 1.2 for Bitaxe Gamma).", "schema": { "type": "number", "default": 1.2 } }
                    ],
                    "responses": {
                        "200": json_ok("Live network & mining economics", "#/components/schemas/GenericApiResponse"),
                        "500": err_ref.clone()
                    }
                }
            },
            "/api/state": {
                "get": {
                    "operationId": "getCommonsState",
                    "summary": "Live Commons ledger state, public roots, STARK proofs, anchors, and swap board",
                    "description": "Returns current treasury state, covenant rule hash, XMSS/SLH-DSA public roots, verified STARKs, and L1 anchor receipts.",
                    "parameters": [api_ver_param.clone()],
                    "responses": {
                        "200": json_ok("Commons ledger snapshot", "#/components/schemas/GenericApiResponse"),
                        "500": err_ref.clone()
                    }
                }
            },
            "/api/swaps": {
                "get": {
                    "operationId": "listSwaps",
                    "summary": "List persisted non-custodial multi-chain atomic swap contracts",
                    "description": "Returns all published zero-custody SHA-256 HTLC swap offers.",
                    "parameters": [api_ver_param.clone()],
                    "responses": {
                        "200": json_ok("Swap board list", "#/components/schemas/GenericApiResponse"),
                        "500": err_ref.clone()
                    }
                },
                "post": {
                    "operationId": "publishSwap",
                    "summary": "Publish a verified swap contract (operator bearer auth required)",
                    "description": "Persists a compiled atomic swap contract to the public board.",
                    "parameters": [api_ver_param.clone()],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/SwapCompileRequest" } } }
                    },
                    "responses": {
                        "200": json_ok("Published swap contract", "#/components/schemas/GenericApiResponse"),
                        "400": err_ref.clone(),
                        "401": err_ref.clone()
                    }
                }
            },
            "/api/swaps/compile": {
                "post": {
                    "operationId": "compileAtomicSwap",
                    "summary": "Public stateless multi-chain SHA-256 HTLC swap compiler (BTC, Lightning, Solana, Base/EVM)",
                    "description": "Compiles deterministic BIP-199 Bitcoin P2WSH scripts, BOLT-11 Lightning invoices, Solana PDA escrows, or Base/Ethereum/Arbitrum NewsatoshiHtlc.sol swap payloads paired with Newsatoshi L1 lock CLI commands.",
                    "parameters": [api_ver_param.clone()],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/SwapCompileRequest" } } }
                    },
                    "responses": {
                        "200": json_ok("Compiled swap contract", "#/components/schemas/GenericApiResponse"),
                        "400": err_ref.clone()
                    }
                }
            },
            "/api/swaps/verify": {
                "post": {
                    "operationId": "verifySwapOnChain",
                    "summary": "Public live dual-chain verifier (Newsatoshi L1 /api/lock + Bitcoin mempool.space + preimage)",
                    "description": "Verifies SHA-256 hash-locks live across Newsatoshi L1 and Bitcoin mempool.space plus optional preimage verification.",
                    "parameters": [api_ver_param.clone()],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/SwapVerifyRequest" } } }
                    },
                    "responses": {
                        "200": json_ok("Live verification report", "#/components/schemas/GenericApiResponse"),
                        "400": err_ref.clone()
                    }
                }
            },
            "/api/agent/402-challenge": {
                "get": {
                    "operationId": "getHttp402Challenge",
                    "summary": "Emit an HTTP 402 Payment Required dual-rail challenge",
                    "description": "Generates an HTTP 402 Payment Required challenge supporting both Newsatoshi L1 hash-locks and sub-millisecond post-quantum PayWord hash-chain streaming.",
                    "parameters": [
                        api_ver_param.clone(),
                        { "name": "resource", "in": "query", "schema": { "type": "string", "default": "/api/inference" } },
                        { "name": "amount_love", "in": "query", "schema": { "type": "string", "default": "0.01" } },
                        { "name": "network", "in": "query", "schema": { "type": "string", "enum": ["mainnet", "testnet"], "default": "mainnet" } }
                    ],
                    "responses": {
                        "402": json_ok("Payment Required challenge", "#/components/schemas/Http402ChallengeResponse"),
                        "400": err_ref.clone()
                    }
                }
            },
            "/api/agent/402-verify": {
                "post": {
                    "operationId": "verifyHttp402Payment",
                    "summary": "Verify a PayWord SHA-256 micropayment token or live Newsatoshi L1 hash-lock payment",
                    "description": "Cryptographically verifies SHA256^ticks(token) == tip or checks live L1 lock settlement.",
                    "parameters": [api_ver_param.clone()],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Http402VerifyRequest" } } }
                    },
                    "responses": {
                        "200": json_ok("Payment verification result", "#/components/schemas/GenericApiResponse"),
                        "400": err_ref.clone()
                    }
                }
            },
            "/api/telemetry": {
                "post": {
                    "operationId": "submitSignedTelemetry",
                    "summary": "Submit offline-signed post-quantum XMSS or NIST FIPS 205 SLH-DSA hardware energy telemetry",
                    "description": "Verifies payout-bound post-quantum hardware signatures and generates a Winterfell STARK covenant transition proof.",
                    "parameters": [api_ver_param.clone()],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/GenericApiResponse" } } }
                    },
                    "responses": {
                        "200": json_ok("Telemetry verification & STARK result", "#/components/schemas/GenericApiResponse"),
                        "400": err_ref.clone()
                    }
                }
            },
            "/api/deeds": {
                "post": {
                    "operationId": "submitSignedDeed",
                    "summary": "Submit offline-signed post-quantum trustee land deed attestation",
                    "description": "Verifies price-bound XMSS or NIST FIPS 205 SLH-DSA land deed attestations and proves recursive covenant continuity.",
                    "parameters": [api_ver_param.clone()],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/GenericApiResponse" } } }
                    },
                    "responses": {
                        "200": json_ok("Deed verification & STARK result", "#/components/schemas/GenericApiResponse"),
                        "400": err_ref.clone()
                    }
                }
            },
            "/api/actions/verify-anchor": {
                "post": {
                    "operationId": "verifyL1Anchor",
                    "summary": "Public live L1 cross-check of a Commons state-root anchor against /api/lock?hash=",
                    "description": "Cross-checks a recorded Commons anchor hash against live Newsatoshi L1 seed nodes.",
                    "parameters": [api_ver_param.clone()],
                    "requestBody": {
                        "required": false,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/GenericApiResponse" } } }
                    },
                    "responses": {
                        "200": json_ok("On-chain anchor verification", "#/components/schemas/GenericApiResponse"),
                        "400": err_ref.clone()
                    }
                }
            },
            "/api/actions/verify-stark": {
                "post": {
                    "operationId": "verifyStarkProof",
                    "summary": "Public cryptographic re-verification of any Winterfell STARK proof",
                    "description": "Re-runs Winterfell 0.13 FRI verification on a stored STARK proof artifact with pre-verify integer domain bounds.",
                    "parameters": [api_ver_param.clone()],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/GenericApiResponse" } } }
                    },
                    "responses": {
                        "200": json_ok("STARK verification result", "#/components/schemas/GenericApiResponse"),
                        "400": err_ref.clone()
                    }
                }
            },
            "/api/launchpad/compile": {
                "post": {
                    "operationId": "compileMissionLaunchpad",
                    "summary": "Compile an RFC-0027 Native Mission Launchpad & Decentralized AI Agent Swarm charter",
                    "description": "Deterministically compiles a zero-rugpull RFC-0027 mission launchpad (0% founder cut, automatic contributor/public-goods split, bonding-curve fees routed 100% into bounty_pool, optional RFC-0024 Trust Bond anti-Sybil gate, and 1-line external agent MCP join snippet).",
                    "parameters": [api_ver_param.clone()],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/GenericApiResponse" } } }
                    },
                    "responses": {
                        "200": json_ok("Compiled RFC-0027 Mission Launchpad charter", "#/components/schemas/GenericApiResponse"),
                        "400": err_ref.clone()
                    }
                }
            },
            "/api/launchpad/quote": {
                "post": {
                    "operationId": "quoteLaunchpadCurve",
                    "summary": "Exact integer RFC-0027 Mission Launchpad bonding-curve buy/redeem quote",
                    "description": "Computes exact integer square-root bonding-curve buy or redemption quotes matching Newsatoshi L1 (node/src/mission.rs) and Lean 4 Theorem 9 conservation 1:1.",
                    "parameters": [api_ver_param.clone()],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/GenericApiResponse" } } }
                    },
                    "responses": {
                        "200": json_ok("Exact bonding-curve buy/redeem quote", "#/components/schemas/GenericApiResponse"),
                        "400": err_ref.clone()
                    }
                }
            },
            "/api/zktls/verify": {
                "post": {
                    "operationId": "verifyZkTlsProof",
                    "summary": "Cryptographically verify reclaim_v1, tlsnotary_v1, or newsatoshi_pq_notary_v1 zkTLS proofs",
                    "description": "Performs real secp256k1 EIP-191 ecrecover, Ed25519/secp256k1 or FIPS 205 SLH-DSA notary verification, and SHA-256 selective-disclosure Merkle opening verification, outputting a canonical L1 RFC-0024 BondAttest receipt hash.",
                    "parameters": [api_ver_param.clone()],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/GenericApiResponse" } } }
                    },
                    "responses": {
                        "200": json_ok("Cryptographic zkTLS verification report", "#/components/schemas/GenericApiResponse"),
                        "400": err_ref.clone()
                    }
                }
            },
            "/api/v2/cutover-readiness": {
                "get": {
                    "operationId": "getV2CutoverReadiness",
                    "summary": "Live Mainnet v2 cutover readiness gate (audits, independent AuxPoW pools, formal proofs)",
                    "description": "Reports honest live Mainnet v2 readiness status and blocking gates (external audits completed vs required, real-difficulty independent Bitcoin AuxPoW pools observed vs required).",
                    "parameters": [api_ver_param.clone()],
                    "responses": {
                        "200": json_ok("Mainnet v2 cutover readiness report", "#/components/schemas/GenericApiResponse")
                    }
                }
            },
            "/api/p2p/compile-offer": {
                "post": {
                    "operationId": "compileBondedP2pOffer",
                    "summary": "Compile a zero-custody RFC-0024 Trust-Bonded zkTLS P2P Fiat/USDC <-> LOVE offer",
                    "description": "Enforces >=100% L1 Trust Bond coverage and outputs deterministic offer_id_hex plus zkTLS settlement steps.",
                    "parameters": [api_ver_param.clone()],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/GenericApiResponse" } } }
                    },
                    "responses": {
                        "200": json_ok("Compiled bonded P2P offer", "#/components/schemas/GenericApiResponse"),
                        "400": err_ref.clone()
                    }
                }
            },
            "/api/p2p/verify-settlement": {
                "post": {
                    "operationId": "verifyBondedP2pSettlement",
                    "summary": "Verify a bonded P2P fiat/USDC <-> LOVE settlement using a cryptographic zkTLS proof",
                    "description": "Verifies the attached reclaim_v1/tlsnotary_v1/newsatoshi_pq_notary_v1 proof and binds it into an RFC-0024 BondAttest receipt digest.",
                    "parameters": [api_ver_param.clone()],
                    "requestBody": {
                        "required": true,
                        "content": { "application/json": { "schema": { "$ref": "#/components/schemas/GenericApiResponse" } } }
                    },
                    "responses": {
                        "200": json_ok("Verified bonded P2P settlement", "#/components/schemas/GenericApiResponse"),
                        "400": err_ref.clone()
                    }
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Streamable HTTP Model Context Protocol (MCP) JSON-RPC 2.0 Dispatcher
// ---------------------------------------------------------------------------

pub fn mcp_tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "newsatoshi_get_network_status",
                "description": "Fetch live Newsatoshi L1 Mainnet and Testnet status (block height, tip hash, difficulty, issued LOVE, open locks, peers, AuxPoW chainid 20051).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "refresh": { "type": "boolean", "description": "Bypass 5s cache and poll live L1 nodes directly." }
                    }
                }
            },
            {
                "name": "newsatoshi_get_commons_state",
                "description": "Return live NEWSATOSHI/2.1 Commons treasury state, covenant_rule_hash, cold-ceremony public roots, verified STARK proofs, L1 anchors, and non-custodial swaps.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "newsatoshi_compile_btc_love_swap",
                "description": "Compile a trustless multi-chain SHA-256 HTLC atomic swap into LOVE: Bitcoin BIP-199 P2WSH (bc1q...), Lightning BOLT-11, Solana SOL/USDC PDA escrow, or EVM ETH/POL/USDC HTLC (live Sourcify-verified contract 0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987 across Base, Arbitrum One, Optimism & Polygon).",
                "inputSchema": {
                    "type": "object",
                    "required": ["amount_love", "love_claim_address"],
                    "properties": {
                        "rail": { "type": "string", "enum": ["btc_onchain", "lightning_bolt11", "solana_htlc", "evm_htlc"], "default": "btc_onchain" },
                        "direction": { "type": "string", "enum": ["btc_to_love", "love_to_btc"], "default": "btc_to_love" },
                        "love_network": { "type": "string", "enum": ["mainnet", "testnet"], "default": "mainnet" },
                        "btc_network": { "type": "string", "enum": ["mainnet", "testnet", "signet", "regtest"], "default": "mainnet" },
                        "payment_hash_hex": { "type": "string", "description": "64-hex SHA-256 payment hash (required for btc_onchain)" },
                        "bolt11_invoice": { "type": "string", "description": "Lightning BOLT-11 invoice (required for lightning_bolt11)" },
                        "amount_love": { "type": "string", "description": "LOVE amount as decimal string (e.g. '250')" },
                        "amount_btc_sats": { "type": "integer", "description": "BTC amount in satoshis" },
                        "love_claim_address": { "type": "string", "description": "Newsatoshi ns1... or nsd1... claim address" },
                        "love_timeout_blocks": { "type": "integer", "default": 144 },
                        "btc_claim_pubkey_hex": { "type": "string", "description": "33-byte compressed secp256k1 pubkey (66 hex)" },
                        "btc_refund_pubkey_hex": { "type": "string", "description": "33-byte compressed secp256k1 pubkey (66 hex)" },
                        "btc_locktime": { "type": "integer", "description": "Bitcoin CLTV block height or Unix timestamp" },
                        "btc_current_height": { "type": "integer", "description": "Optional current BTC tip for anti-race margin check" },
                        "source_asset": { "type": "string", "description": "Asset for solana_htlc (USDC/SOL) or evm_htlc (USDC/ETH)" },
                        "evm_chain": { "type": "string", "enum": ["base", "ethereum", "arbitrum", "optimism", "polygon"], "default": "base", "description": "Target EVM chain (Base, Arbitrum, Optimism & Polygon default to live verified 0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987)" },
                        "evm_recipient_address": { "type": "string", "description": "0x... counterparty recipient for evm_htlc" },
                        "evm_contract_address": { "type": "string", "description": "Optional override; Base/Arbitrum/Optimism/Polygon default to live verified 0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987" },
                        "solana_cluster": { "type": "string", "enum": ["mainnet-beta", "devnet"], "default": "mainnet-beta" },
                        "solana_sender_pubkey": { "type": "string", "description": "Base58 sender pubkey for solana_htlc" },
                        "solana_recipient_pubkey": { "type": "string", "description": "Base58 recipient pubkey for solana_htlc" }
                    }
                }
            },
            {
                "name": "newsatoshi_verify_onchain_lock",
                "description": "Verify a SHA-256 hash-lock live on Newsatoshi Mainnet or Testnet (/api/lock?hash=), optionally check Bitcoin P2WSH funding on mempool.space, and optionally verify SHA256(preimage) == hash.",
                "inputSchema": {
                    "type": "object",
                    "required": ["payment_hash_hex"],
                    "properties": {
                        "payment_hash_hex": { "type": "string", "description": "64-hex SHA-256 hash" },
                        "love_network": { "type": "string", "enum": ["mainnet", "testnet"], "default": "mainnet" },
                        "btc_address": { "type": "string", "description": "Optional Bitcoin bc1q... or tb1q... P2WSH address" },
                        "preimage_hex": { "type": "string", "description": "Optional 64-hex preimage to check" }
                    }
                }
            },
            {
                "name": "newsatoshi_verify_payword_402",
                "description": "Verify a post-quantum pure-SHA-256 PayWord HTTP 402 micropayment token (checks SHA256^ticks(token) == tip in microseconds with zero gas).",
                "inputSchema": {
                    "type": "object",
                    "required": ["tip_hex", "token_hex", "ticks"],
                    "properties": {
                        "tip_hex": { "type": "string", "description": "64-hex committed PayWord chain tip (SHA256^N(seed))" },
                        "token_hex": { "type": "string", "description": "64-hex revealed payment token (SHA256^(N-ticks)(seed))" },
                        "ticks": { "type": "integer", "minimum": 1, "maximum": 100000 },
                        "unit_price_base_units": { "type": "integer", "default": 1000 }
                    }
                }
            },
            {
                "name": "newsatoshi_verify_stark_proof",
                "description": "Cryptographically re-verify a Winterfell 0.13 STARK covenant transition proof stored on this Commons node.",
                "inputSchema": {
                    "type": "object",
                    "required": ["proof_id"],
                    "properties": {
                        "proof_id": { "type": "string" }
                    }
                }
            },
            {
                "name": "newsatoshi_agent_quickstart",
                "description": "Return step-by-step zero-human setup commands for an AI agent to create a shielded Newsatoshi nsd1... wallet, get free testnet LOVE from the faucet, swap BTC<->LOVE on mainnet, and transact via HTTP 402 PayWord or L1 compute jobs.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "network": { "type": "string", "enum": ["mainnet", "testnet"], "default": "testnet" }
                    }
                }
            },
            {
                "name": "newsatoshi_create_402_challenge",
                "description": "Generate a standard NEWSATOSHI-402/1.0 Payment Required challenge (L1 hash-lock + pure-SHA-256 PayWord stream) so any AI agent or developer can monetize their own API or tool in LOVE with zero KYC.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "resource": { "type": "string", "default": "/api/inference" },
                        "amount_love": { "type": "string", "default": "0.01" },
                        "network": { "type": "string", "enum": ["mainnet", "testnet"], "default": "mainnet" },
                        "recipient": { "type": "string", "description": "Recipient Newsatoshi ns1... or nsd1... address" },
                        "unit_price_base_units": { "type": "integer", "default": 1000 }
                    }
                }
            },
            {
                "name": "newsatoshi_get_mining_economics",
                "description": "Return real-time Newsatoshi Mainnet & Testnet derived hashrate, AuxPoW merge-mined %, supply mined %, and snapshot merge-mining yield for any SHA-256 ASIC hashrate (Bitaxe, NerdQAxe, Antminer S19/S21) at zero extra electricity cost.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "hashrate_ths": { "type": "number", "default": 1.2, "description": "Miner SHA-256 hashrate in TH/s (e.g. 1.2 for Bitaxe Gamma, 200 for Antminer S21)" },
                        "refresh": { "type": "boolean", "default": false }
                    }
                }
            },
            {
                "name": "newsatoshi_get_backing_proof",
                "description": "Fetch live verifiable multi-chain Proof-of-Backing reserves across the ownerless NewsatoshiBackingVault.sol (0x37Ed94525DBE408fA72cDd156409875666679265) on Base, Arbitrum, Optimism & Polygon plus live Newsatoshi L1 supply, RWA deeds, and backing-per-LOVE ratios.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "refresh": { "type": "boolean", "default": false }
                    }
                }
            },
            {
                "name": "newsatoshi_spawn_charter",
                "description": "Compile a sovereign Web4 Autonomous Business Charter with Lean 4-verified monotonic child spending caps, exact basis-point ops/reinvest/backing splits (summing to 10000), ERC-8004 dual identity JSON, and ready-to-run NewsatoshiBackingVault.sol anchor/deposit commands.",
                "inputSchema": {
                    "type": "object",
                    "required": ["name", "mission", "max_spend_per_call_love", "ops_bps", "reinvest_bps", "backing_bps", "payout_love_address", "evm_treasury_address"],
                    "properties": {
                        "name": { "type": "string" },
                        "category": { "type": "string", "enum": ["paid_api", "compute_node", "physical_rwa", "liquidity_maker", "agent_collective"], "default": "paid_api" },
                        "mission": { "type": "string" },
                        "parent_charter_id": { "type": "string", "description": "Optional parent charter_id; enforces Lean 4 monotonic cap & backing_bps inheritance" },
                        "max_spend_per_call_love": { "type": "integer", "minimum": 1 },
                        "ops_bps": { "type": "integer", "minimum": 0, "maximum": 10000 },
                        "reinvest_bps": { "type": "integer", "minimum": 0, "maximum": 10000 },
                        "backing_bps": { "type": "integer", "minimum": 0, "maximum": 10000 },
                        "payout_love_address": { "type": "string" },
                        "evm_treasury_address": { "type": "string" },
                        "primary_evm_chain": { "type": "string", "enum": ["base", "arbitrum", "optimism", "polygon"], "default": "base" }
                    }
                }
            },
            {
                "name": "newsatoshi_list_charters",
                "description": "List all persisted sovereign Web4 business charters with their covenant_rule_hash, backing_bps, lineage depth, and ERC-8004 URIs.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "newsatoshi_discover_services",
                "description": "Discover live HTTP 402 PayWord & x402 paid AI/compute/data services registered by Web4 businesses, optionally filtered by capability tag.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "capability": { "type": "string", "description": "Optional capability tag filter (e.g. inference, compute, stark, audit)" }
                    }
                }
            },
            {
                "name": "newsatoshi_list_bounties",
                "description": "List open zero-custody work & compute bounties backed by Newsatoshi L1 Freivalds jobs or multi-chain SHA-256 HTLC locks.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "crypto_inspect_evm_contract",
                "description": "Zero-API-key live EVM smart-contract security audit across Ethereum, Base, Arbitrum, Optimism & Polygon: inspects bytecode size, EIP-1967 upgradeable proxy slots, active owner() privilege, ERC-20 metadata, native balance, and live Sourcify source verification.",
                "inputSchema": {
                    "type": "object",
                    "required": ["chain", "address"],
                    "properties": {
                        "chain": { "type": "string", "enum": ["ethereum", "base", "arbitrum", "optimism", "polygon"], "description": "Target EVM mainnet" },
                        "address": { "type": "string", "description": "0x-prefixed 20-byte EVM contract or EOA address" }
                    }
                }
            },
            {
                "name": "crypto_bitcoin_mempool_oracle",
                "description": "Zero-API-key live Bitcoin Mainnet fee rate oracle (sat/vB for fastest/30m/1h/economy), current tip block height, mempool backlog size, and optional Bitcoin address confirmed/unconfirmed balance lookup via mempool.space.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "address": { "type": "string", "description": "Optional Bitcoin address (bc1q..., bc1p..., 1..., 3...) for live balance & tx count lookup" }
                    }
                }
            },
            {
                "name": "crypto_decode_bolt11_invoice",
                "description": "Instant sub-millisecond pure-Rust Lightning Network BOLT-11 invoice decoder: extracts network, SHA-256 payment_hash, exact satoshi & millisatoshi amount, expiry, timestamp, and description.",
                "inputSchema": {
                    "type": "object",
                    "required": ["invoice"],
                    "properties": {
                        "invoice": { "type": "string", "description": "Lightning BOLT-11 invoice string (lnbc..., lntb..., lnbcrt...)" }
                    }
                }
            },
            {
                "name": "crypto_quote_base_uniswap_v3",
                "description": "Fetch a live on-chain Base Mainnet Uniswap V3 QuoterV2 swap quote and live pool reserves for ETH <-> wLOVE or USDC <-> wLOVE (including 2-hop USDC -> WETH -> wLOVE).",
                "inputSchema": {
                    "type": "object",
                    "required": ["from_asset", "to_asset", "amount"],
                    "properties": {
                        "from_asset": { "type": "string", "enum": ["ETH", "USDC", "wLOVE"], "description": "Input asset symbol" },
                        "to_asset": { "type": "string", "enum": ["ETH", "USDC", "wLOVE"], "description": "Output asset symbol" },
                        "amount": { "type": "string", "description": "Human-readable decimal amount (e.g. '0.001' ETH or '1' USDC)" }
                    }
                }
            },
            {
                "name": "crypto_verify_tx_receipt",
                "description": "Zero-API-key cross-chain transaction finality & confirmation checker across Ethereum, Base, Arbitrum, Optimism, Polygon, and Bitcoin Mainnet.",
                "inputSchema": {
                    "type": "object",
                    "required": ["chain", "tx_hash"],
                    "properties": {
                        "chain": { "type": "string", "enum": ["ethereum", "base", "arbitrum", "optimism", "polygon", "bitcoin"], "description": "Target blockchain network" },
                        "tx_hash": { "type": "string", "description": "Transaction hash (0x... for EVM or 64-hex txid for Bitcoin)" }
                    }
                }
            },
            {
                "name": "newsatoshi_inspect_trust_bond",
                "description": "Query an RFC-0024 L1 Universal Trust Bond (active LOVE collateral stake, settled volume, slash history, arbiter quorum, and canonical Merkle proof) from an L1 node or return the RFC-0024 substrate schema.",
                "inputSchema": {
                    "type": "object",
                    "required": ["bond_id_hex"],
                    "properties": {
                        "bond_id_hex": { "type": "string", "description": "64-hex RFC-0024 Trust Bond ID" },
                        "seed_url": { "type": "string", "description": "Optional L1 node base URL (defaults to live Mainnet seed)" }
                    }
                }
            },
            {
                "name": "newsatoshi_inspect_rollup_vault",
                "description": "Query an RFC-0023 Sovereign L2 Rollup Vault (vk_hash, state_root, deposit_acc, TVL, settled L2 tx count, withdraw_roots, and canonical Merkle proof) from an L1 node.",
                "inputSchema": {
                    "type": "object",
                    "required": ["rollup_id_hex"],
                    "properties": {
                        "rollup_id_hex": { "type": "string", "description": "64-hex RFC-0023 Sovereign L2 Rollup ID" },
                        "seed_url": { "type": "string", "description": "Optional L1 node base URL (defaults to live Mainnet seed)" }
                    }
                }
            },
            {
                "name": "newsatoshi_get_protocol_signaling",
                "description": "Query RFC-0026 live BIP-9 collective protocol signaling & base-layer monetary ossification telemetry from an L1 node.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "seed_url": { "type": "string", "description": "Optional L1 node base URL (defaults to live Mainnet seed)" }
                    }
                }
            },
            {
                "name": "newsatoshi_create_scoped_session_policy",
                "description": "Compile a deterministic RFC-0025 policy-constrained AI agent session policy (max_per_call, max_per_hour, max_total, min_counterparty_bond, profit_sweep_address, expiry) and canonical SHA-256 policy digest without exposing root spending keys.",
                "inputSchema": {
                    "type": "object",
                    "required": ["session_id", "max_per_call", "max_per_hour", "max_total"],
                    "properties": {
                        "session_id": { "type": "string", "description": "Unique agent session identifier" },
                        "max_per_call": { "type": "integer", "description": "Max spend per call in base units (1 LOVE = 100,000,000)" },
                        "max_per_hour": { "type": "integer", "description": "Sliding 1-hour spend cap in base units" },
                        "max_total": { "type": "integer", "description": "Lifetime session spend cap in base units" },
                        "min_counterparty_bond": { "type": "integer", "description": "Minimum RFC-0024 L1 Trust Bond stake required on counterparties" },
                        "max_counterparty_slashes": { "type": "integer", "description": "Maximum tolerated slash count on counterparties (default 0)" },
                        "profit_sweep_address": { "type": "string", "description": "Shielded ns1.../nsd1... address for automated profit sweeps" },
                        "ttl_seconds": { "type": "integer", "description": "Session validity duration in seconds (default 86400)" }
                    }
                }
            },
            {
                "name": "newsatoshi_launch_agent_mission",
                "description": "Compile a deterministic RFC-0027 Native Mission Launchpad & Decentralized AI Agent Swarm charter (pequalnp.money-style): dual zero-rugpull bounty_pool + bonding curve_reserve, 0% founder take, automatic contributor/public-goods split (e.g. 80/20), optional RFC-0024 anti-Sybil Trust Bond gate, and 1-line external agent MCP join snippet.",
                "inputSchema": {
                    "type": "object",
                    "required": ["name", "ticker", "mission_brief"],
                    "properties": {
                        "name": { "type": "string", "description": "Mission name (2..=80 chars, e.g. 'Open P vs NP Lean 4 Swarm')" },
                        "ticker": { "type": "string", "description": "Mission ticker (1..=12 alphanumeric chars, e.g. 'PVNP')" },
                        "mission_brief": { "type": "string", "description": "Objective, verification rule, and deliverables (8..=1200 chars)" },
                        "verifier_kind": { "type": "string", "default": "lean4_zero_sorry", "description": "Verification rule (lean4_zero_sorry, unit_tests, stark_proof, zktls_receipt, quorum_review)" },
                        "contributor_bps": { "type": "integer", "default": 8000, "description": "Contributor payout share in basis points (must sum with public_goods_bps to 10000; 0% founder take)" },
                        "public_goods_bps": { "type": "integer", "default": 2000, "description": "Public-goods treasury share in basis points (e.g. 2000 = 20% to Lean FRO)" },
                        "public_goods_label": { "type": "string", "default": "Public Goods / Formal Verification Treasury" },
                        "public_goods_address": { "type": "string", "description": "Shielded ns1... or nsd1... public-goods beneficiary address" },
                        "curve_fee_bps": { "type": "integer", "default": 500, "maximum": 1000, "description": "Bonding-curve buy/sell fee routed 100% into bounty_pool (max 1000 = 10%)" },
                        "initial_funding_base_units": { "type": "integer", "default": 100000000, "description": "Initial bounty pool funding in base units (1 LOVE = 100,000,000)" },
                        "min_worker_bond_base_units": { "type": "integer", "default": 0, "description": "Optional RFC-0024 Trust Bond minimum stake required to claim tasks (anti-Sybil)" },
                        "repo_url": { "type": "string", "description": "Optional open-source repository URL for agent pull requests / proofs" },
                        "swarm_roles": { "type": "array", "items": { "type": "string" }, "description": "Swarm role topology (defaults to orchestrator, prover, red_team, checker_desk)" }
                    }
                }
            },
            {
                "name": "newsatoshi_quote_launchpad_curve",
                "description": "Compute an exact integer RFC-0027 Mission Launchpad bonding-curve buy or redeem quote matching Newsatoshi L1 (node/src/mission.rs) and Lean 4 Theorem 9 conservation 1:1 (100% of curve_fee_bps routes straight into bounty_pool, 0% founder extraction).",
                "inputSchema": {
                    "type": "object",
                    "required": ["side", "curve_reserve", "total_shares", "curve_fee_bps"],
                    "properties": {
                        "side": { "type": "string", "enum": ["buy", "redeem"], "description": "Quote direction: buy shares into curve or redeem shares from curve" },
                        "curve_reserve": { "type": "integer", "minimum": 0, "description": "Current curve_reserve in base units" },
                        "total_shares": { "type": "integer", "minimum": 0, "description": "Current total_shares outstanding" },
                        "curve_fee_bps": { "type": "integer", "default": 500, "maximum": 1000, "description": "Fee in bps routed into bounty_pool on every trade (max 1000)" },
                        "amount": { "type": "integer", "minimum": 1, "description": "Input LOVE base units (required when side='buy')" },
                        "shares": { "type": "integer", "minimum": 1, "description": "Shares to burn (required when side='redeem')" },
                        "miner_fee": { "type": "integer", "default": 0, "description": "Optional L1 miner fee in base units (for side='redeem')" }
                    }
                }
            },
            {
                "name": "newsatoshi_inspect_mission_launchpad",
                "description": "Query an RFC-0027 Native Mission Launchpad on an L1 node (/api/mission?id=...): returns live bounty_pool, curve_reserve, total_shares, contributor/public-goods split, open/claimed tasks, min_worker_bond, and canonical Merkle proof.",
                "inputSchema": {
                    "type": "object",
                    "required": ["mission_id_hex"],
                    "properties": {
                        "mission_id_hex": { "type": "string", "description": "64-hex RFC-0027 Mission Launchpad ID" },
                        "seed_url": { "type": "string", "description": "Optional L1 node base URL (defaults to live Mainnet seed)" }
                    }
                }
            },
            {
                "name": "newsatoshi_format_swarm_task_claim",
                "description": "Compute deterministic L1 RFC-0027 post_task_message (nsMISSTASK) and claim_task_message (nsMISSCLAIM) SHA-256 signing digests for FIPS 205 checker-desk quorum task posting and verified bounty payouts.",
                "inputSchema": {
                    "type": "object",
                    "required": ["mission_id_hex", "task_id", "spec_hash_hex", "reward"],
                    "properties": {
                        "mission_id_hex": { "type": "string", "description": "64-hex RFC-0027 Mission Launchpad ID" },
                        "task_id": { "type": "integer", "minimum": 0, "description": "Monotonic u32 task/lemma identifier" },
                        "spec_hash_hex": { "type": "string", "description": "64-hex SHA-256 hash of the task specification / theorem statement" },
                        "reward": { "type": "integer", "minimum": 1, "description": "Gross task bounty in base units (split automatically by contributor_bps/public_goods_bps)" },
                        "proof_hash_hex": { "type": "string", "description": "Optional 64-hex SHA-256 hash of verified proof artifact (required to compute claim_task_message_hex)" },
                        "contributor_apk_hex": { "type": "string", "description": "Optional 64-hex shielded contributor apk (required to compute claim_task_message_hex)" },
                        "miner_fee": { "type": "integer", "default": 0, "description": "L1 miner fee deducted from gross reward on claim" }
                    }
                }
            },
            {
                "name": "newsatoshi_verify_zktls_proof",
                "description": "Cryptographically verify a real zkTLS / MPC-TLS presentation (reclaim_v1 secp256k1 EIP-191 ecrecover, tlsnotary_v1 Ed25519/secp256k1 + selective-disclosure Merkle proofs, or newsatoshi_pq_notary_v1 FIPS 205 SLH-DSA) and bind it into a canonical L1 RFC-0024 BondAttest receipt hash.",
                "inputSchema": {
                    "type": "object",
                    "required": ["protocol"],
                    "properties": {
                        "protocol": { "type": "string", "enum": ["reclaim_v1", "tlsnotary_v1", "newsatoshi_pq_notary_v1"] },
                        "provider": { "type": "string", "description": "Reclaim provider string (for reclaim_v1)" },
                        "parameters": { "type": "string", "description": "Reclaim canonical parameters JSON string" },
                        "context": { "type": "string" },
                        "owner": { "type": "string" },
                        "timestampS": { "type": "integer" },
                        "epoch": { "type": "integer" },
                        "signatures": { "type": "array", "items": { "type": "string" } },
                        "trusted_attestors": { "type": "array", "items": { "type": "string" } },
                        "session_header": { "type": "object", "description": "TLSNotary SessionHeader (for tlsnotary_v1 / newsatoshi_pq_notary_v1)" },
                        "signature_scheme": { "type": "string", "enum": ["ed25519", "secp256k1"], "default": "ed25519" },
                        "notary_pubkey_hex": { "type": "string" },
                        "notary_signature_hex": { "type": "string" },
                        "notary_slh_pubkey_hex": { "type": "string" },
                        "notary_slh_signature_hex": { "type": "string" },
                        "trusted_notary_pubkeys": { "type": "array", "items": { "type": "string" } },
                        "disclosed_chunks": { "type": "array", "items": { "type": "object" } },
                        "expected_server_name": { "type": "string" },
                        "required_substrings": { "type": "array", "items": { "type": "string" } }
                    }
                }
            },
            {
                "name": "newsatoshi_check_v2_cutover_readiness",
                "description": "Evaluate the live Mainnet v2 cutover readiness gate honestly: reports external audit completion (requires >=2), independent real-difficulty Bitcoin AuxPoW pools observed (requires >=3), and formal proof verification status.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "newsatoshi_compile_bonded_p2p_offer",
                "description": "Compile a zero-custody RFC-0024 Trust-Bonded zkTLS P2P fiat/USDC <-> native shielded LOVE market-maker quote enforcing >=100% L1 bond collateral coverage and real zkTLS settlement binding.",
                "inputSchema": {
                    "type": "object",
                    "required": ["maker_label", "maker_bond_id_hex", "maker_love_address", "payment_rail", "price_fiat_cents_per_love", "min_trade_love_base_units", "max_trade_love_base_units", "declared_bond_stake_base_units", "expected_tls_server_name"],
                    "properties": {
                        "maker_label": { "type": "string" },
                        "maker_bond_id_hex": { "type": "string", "description": "64-hex RFC-0024 L1 Trust Bond ID" },
                        "maker_love_address": { "type": "string", "description": "Shielded ns1... or nsd1... address" },
                        "side": { "type": "string", "enum": ["sell_love", "buy_love"], "default": "sell_love" },
                        "payment_rail": { "type": "string", "enum": ["wise", "revolut", "zelle", "venmo", "sepa_instant", "faster_payments", "base_usdc"] },
                        "fiat_currency": { "type": "string", "default": "USD" },
                        "price_fiat_cents_per_love": { "type": "integer", "minimum": 1 },
                        "min_trade_love_base_units": { "type": "integer", "minimum": 1 },
                        "max_trade_love_base_units": { "type": "integer", "minimum": 1 },
                        "declared_bond_stake_base_units": { "type": "integer", "minimum": 1 },
                        "required_zktls_protocol": { "type": "string", "enum": ["reclaim_v1", "tlsnotary_v1", "newsatoshi_pq_notary_v1"], "default": "tlsnotary_v1" },
                        "expected_tls_server_name": { "type": "string", "description": "e.g. api.wise.com or api.revolut.com" }
                    }
                }
            },
            {
                "name": "newsatoshi_get_nsusd_state",
                "description": "Return the honest live RFC-0028 Dual-Tier nsUSD stablecoin engine state (Sovereign BTC-backed asset_id=1 + strictly isolated Bridged USDC/USDS/DAI asset_id=2, PriorLock oracle circuit-breaker status, and SHA-256 solvency commitment root).",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "newsatoshi_simulate_nsusd_stress",
                "description": "Execute a deterministic 4-scenario adversarial stress test of the RFC-0028 nsUSD CDP & PriorLock oracle engine inside an isolated sandbox (flash crash liquidation, bank-run $1.00 BTC redemption floor, +40% oracle spoof 50/50 slash, and stale oracle mint halt) without mutating live balances.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "crash_pct": { "type": "integer", "minimum": 10, "maximum": 75, "default": 50, "description": "Simulated BTC flash crash percentage (10..=75, default 50)" }
                    }
                }
            },
            {
                "name": "newsatoshi_benchmark_l2_perfection_batch",
                "description": "Run a real live measured Newsatoshi L2 Shielded Rollup & Perfection Layer benchmark: executes N O(1) SHA-256 PayWord + nsUSD transfers, computes SHA-256 L2 Merkle roots + a verified unilateral L1 Escape-Hatch proof, generates & verifies a live Winterfell 0.13 STARK conservation proof, and binds an L1 Mainnet v2 anchor commitment.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "tx_count": { "type": "integer", "minimum": 10, "maximum": 50000, "default": 2000, "description": "Number of off-chain PayWord + nsUSD transfers to batch (10..=50000, default 2000)" }
                    }
                }
            },
            {
                "name": "newsatoshi_generate_private_x402_middleware",
                "description": "Generate copy-paste-ready HTTP 402 paywall middleware for Express, Next.js App Router, FastAPI, or Rust Axum that quotes prices in stable USD, accepts private zero-gas O(1) SHA-256 PayWords (nsUSD/LOVE, zero corporate freeze switch), and falls back cleanly to standard x402 v2 Base USDC.",
                "inputSchema": {
                    "type": "object",
                    "required": ["framework"],
                    "properties": {
                        "framework": { "type": "string", "enum": ["express", "nextjs", "fastapi", "axum"], "description": "Target web framework" },
                        "price_usd": { "type": "number", "default": 0.005, "description": "USD price per API call (e.g. 0.005 for half a cent)" },
                        "route_path": { "type": "string", "default": "/api/paid-endpoint", "description": "HTTP route path to gate" },
                        "pay_to_evm": { "type": "string", "description": "Optional EVM address for Base USDC x402 v2 fallback" },
                        "seller_ns1": { "type": "string", "description": "Optional Newsatoshi shielded ns1.../nsd1... address for private settlement" }
                    }
                }
            },
            {
                "name": "newsatoshi_verify_base_nsusd_escrow",
                "description": "Verify a real Base Mainnet USDC deposit transaction hash live against Base JSON-RPC logs to confirm 1:1 backing into the immutable, ownerless NewsatoshiBackingVault (0x37Ed94525DBE408fA72cDd156409875666679265) and emit an auditable RFC-0028 Tier-1B nsUSD-B receipt.",
                "inputSchema": {
                    "type": "object",
                    "required": ["tx_hash"],
                    "properties": {
                        "tx_hash": { "type": "string", "description": "Base Mainnet 0x-prefixed 32-byte transaction hash (e.g. 0xd3c521e3dcdd1465f6f51384a5c721c0c445c3d384282ae0c67b2b07195029ba)" }
                    }
                }
            },
            {
                "name": "newsatoshi_estimate_paywords_savings",
                "description": "Calculate exact monthly & annual payment-fee savings comparing Stripe cards (2.9% + $0.30/tx), per-call Base L2 x402 USDC, and Newsatoshi O(1) SHA-256 PayWords settled via live Base Mainnet NewsatoshiPayWordChannel (0x971BA814Df91c20DcA3aDd870C2B55d643965C50, 0% protocol fee).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "monthly_calls": { "type": "integer", "minimum": 1, "default": 1000000, "description": "Monthly micro-payment API calls" },
                        "price_per_call_usd": { "type": "number", "default": 0.005, "description": "Average USD price per API call" },
                        "calls_per_batch": { "type": "integer", "minimum": 10, "maximum": 4096, "default": 1000, "description": "API calls settled per on-chain claim() transaction" }
                    }
                }
            },
            {
                "name": "newsatoshi_compile_payword_channel_setup",
                "description": "Generate an org-ready Base Mainnet USDC O(1) SHA-256 PayWords HTTP 402 server gate (Cloudflare Worker, Express, or FastAPI) + cast CLI settlement commands wired to live Sourcify-verified NewsatoshiPayWordChannel (0x971BA814Df91c20DcA3aDd870C2B55d643965C50).",
                "inputSchema": {
                    "type": "object",
                    "required": ["framework"],
                    "properties": {
                        "framework": { "type": "string", "enum": ["cloudflare_worker", "express", "fastapi"], "description": "Target edge or API framework" },
                        "recipient_evm": { "type": "string", "description": "Optional 0x... Base treasury address receiving USDC settlements" },
                        "price_usd": { "type": "number", "default": 0.002, "description": "USD price per API call in Base USDC" },
                        "max_steps": { "type": "integer", "minimum": 1, "default": 1000, "description": "PayWord channel step depth N (e.g. 1000 calls)" },
                        "route_path": { "type": "string", "default": "/api/inference", "description": "Route path to gate" }
                    }
                }
            },
            {
                "name": "newsatoshi_simulate_fair_curve_launch",
                "description": "Simulate exact integer constant-product bonding-curve math + dilution-proof x402 PayWords USDC revenue injection (Lean 4 Theorems 10-11, 0% premine, 0% platform cut) and emit 1-command cast deployment for live Sourcify-verified Base Mainnet NewsatoshiFairCurveFactory (0xF6bE782819a8eD0D1556A8f5DC3223f9eb13a933).",
                "inputSchema": {
                    "type": "object",
                    "required": ["name", "symbol"],
                    "properties": {
                        "name": { "type": "string", "description": "Curve token name (2..=64 chars)" },
                        "symbol": { "type": "string", "description": "Curve ticker symbol (1..=12 alphanumeric chars)" },
                        "virtual_reserve_usdc": { "type": "number", "default": 10000, "description": "Virtual USDC reserve depth V" },
                        "virtual_supply_tokens": { "type": "integer", "default": 1000000000, "description": "Virtual token supply Y0" },
                        "swap_fee_bps": { "type": "integer", "minimum": 0, "maximum": 500, "default": 100, "description": "Swap fee in bps (<=500; routed 100% to holders or fee_sink, 0% platform cut)" },
                        "buy_usdc": { "type": "number", "default": 1000, "description": "Simulated initial USDC buy amount" },
                        "paywords_revenue_usdc": { "type": "number", "default": 500, "description": "Simulated settled x402 PayWords USDC earnings injected to holders" },
                        "fee_sink_evm": { "type": "string", "description": "Optional immutable fee recipient (omit or 0x000...000 for 100% holder dividend mode)" }
                    }
                }
            }
        ]
    })
}

/// Dispatches a single MCP JSON-RPC 2.0 request and returns `(http_status, optional_json_body)`.
#[allow(clippy::too_many_arguments)]
pub fn handle_mcp_jsonrpc(
    req_val: &Value,
    base_url: &str,
    engine: &CommonsEngine,
    seed_snapshot: &SeedSnapshot,
    backing_snapshot: Option<&BackingProofSummary>,
    testnet_client: &SeedStatusClient,
    mainnet_client: Option<&SeedStatusClient>,
) -> (u16, Option<Value>) {
    let id = req_val.get("id").cloned();
    let method = req_val.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = req_val.get("params").cloned().unwrap_or_else(|| json!({}));

    // JSON-RPC notifications have no id and expect 202 Accepted with no response payload.
    if id.is_none() && method.starts_with("notifications/") {
        return (202, None);
    }

    let id_val = id.unwrap_or(Value::Null);

    let ok_resp = |result: Value| -> (u16, Option<Value>) {
        (
            200,
            Some(json!({
                "jsonrpc": "2.0",
                "id": id_val,
                "result": result
            })),
        )
    };
    let err_resp = |code: i64, msg: String| -> (u16, Option<Value>) {
        (
            200,
            Some(json!({
                "jsonrpc": "2.0",
                "id": id_val,
                "error": { "code": code, "message": msg }
            })),
        )
    };

    match method {
        "initialize" => ok_resp(json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "subscribe": false, "listChanged": false }
            },
            "serverInfo": {
                "name": "newsatoshi-commons",
                "version": env!("CARGO_PKG_VERSION")
            },
            "instructions": format!("Newsatoshi (LOVE) & NEWSATOSHI/2.1 Agent Transaction Interface. Call newsatoshi_agent_quickstart to onboard a shielded nsd1... agent wallet, newsatoshi_compile_btc_love_swap for trustless BTC<->LOVE swaps, or newsatoshi_verify_payword_402 for HTTP 402 micropayments. Full guide: {base_url}/llms.txt")
        })),
        "ping" => ok_resp(json!({})),
        "tools/list" => ok_resp(mcp_tools_list()),
        "resources/list" => ok_resp(json!({
            "resources": [
                {
                    "uri": "newsatoshi://llms.txt",
                    "name": "Newsatoshi Agent Discovery Guide",
                    "mimeType": "text/markdown"
                },
                {
                    "uri": "newsatoshi://network/status",
                    "name": "Live Newsatoshi Dual-Chain Status",
                    "mimeType": "application/json"
                },
                {
                    "uri": "newsatoshi://commons/state",
                    "name": "Live NEWSATOSHI/2.1 Commons Ledger State",
                    "mimeType": "application/json"
                }
            ]
        })),
        "resources/read" => {
            let uri = params.get("uri").and_then(|u| u.as_str()).unwrap_or("");
            let api_state = engine.to_api_state();
            match uri {
                "newsatoshi://llms.txt" => ok_resp(json!({
                    "contents": [{
                        "uri": uri,
                        "mimeType": "text/markdown",
                        "text": render_llms_txt(base_url, &api_state, seed_snapshot)
                    }]
                })),
                "newsatoshi://network/status" => ok_resp(json!({
                    "contents": [{
                        "uri": uri,
                        "mimeType": "application/json",
                        "text": serde_json::to_string_pretty(seed_snapshot).unwrap_or_default()
                    }]
                })),
                "newsatoshi://commons/state" => ok_resp(json!({
                    "contents": [{
                        "uri": uri,
                        "mimeType": "application/json",
                        "text": serde_json::to_string_pretty(&api_state).unwrap_or_default()
                    }]
                })),
                _ => err_resp(-32602, format!("unknown resource uri '{uri}'")),
            }
        }
        "tools/call" => {
            let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

            let tool_result: Result<Value, String> = match tool_name {
                "newsatoshi_get_network_status" => Ok(serde_json::to_value(seed_snapshot).unwrap_or_default()),
                "newsatoshi_get_commons_state" => Ok(serde_json::to_value(engine.to_api_state()).unwrap_or_default()),
                "newsatoshi_compile_btc_love_swap" => {
                    serde_json::from_value::<CompileSwapRequest>(args)
                        .map_err(|e| format!("invalid swap arguments: {e}"))
                        .and_then(|req| compile_atomic_swap(&req))
                        .map(|c| serde_json::to_value(c).unwrap_or_default())
                }
                "newsatoshi_verify_onchain_lock" => {
                    let hash = args
                        .get("payment_hash_hex")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "missing required 'payment_hash_hex'".to_string());
                    hash.and_then(|h| {
                        let net = args
                            .get("love_network")
                            .and_then(|v| v.as_str())
                            .unwrap_or("mainnet");
                        let btc_addr = args.get("btc_address").and_then(|v| v.as_str());
                        let preimage = args.get("preimage_hex").and_then(|v| v.as_str());
                        let client = if net.eq_ignore_ascii_case("mainnet") {
                            mainnet_client.ok_or_else(|| "mainnet seed client not configured".to_string())?
                        } else {
                            testnet_client
                        };
                        verify_swap_on_chain(client, net, h, btc_addr, preimage)
                            .map(|r| serde_json::to_value(r).unwrap_or_default())
                    })
                }
                "newsatoshi_verify_payword_402" => {
                    let tip = args
                        .get("tip_hex")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "missing required 'tip_hex'".to_string());
                    let tok = args
                        .get("token_hex")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "missing required 'token_hex'".to_string());
                    let ticks = args
                        .get("ticks")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| "missing required integer 'ticks'".to_string());
                    let unit = args
                        .get("unit_price_base_units")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1_000);
                    match (tip, tok, ticks) {
                        (Ok(t), Ok(k), Ok(n)) => verify_payword_token(t, k, n, unit)
                            .map(|r| serde_json::to_value(r).unwrap_or_default()),
                        (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => Err(e),
                    }
                }
                "newsatoshi_verify_stark_proof" => {
                    let pid = args
                        .get("proof_id")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "missing required 'proof_id'".to_string());
                    pid.and_then(|id| {
                        let art = engine
                            .stark_proofs
                            .iter()
                            .find(|p| p.proof_id == id)
                            .ok_or_else(|| format!("STARK proof '{id}' not found"))?;
                        verify_covenant_stark(art).map(|res| {
                            json!({
                                "proof_id": id,
                                "valid": res.valid,
                                "verify_ms": res.verify_ms,
                                "proof_bytes_len": art.proof_bytes_len,
                                "security_bits": art.security_bits
                            })
                        })
                    })
                }
                "newsatoshi_agent_quickstart" => {
                    let net = args
                        .get("network")
                        .and_then(|v| v.as_str())
                        .filter(|s| *s == "mainnet" || *s == "testnet")
                        .unwrap_or("testnet");
                    Ok(json!({
                        "network": net,
                        "mcp_endpoint": format!("{base_url}/mcp"),
                        "llms_txt": format!("{base_url}/llms.txt"),
                        "steps": [
                            {
                                "step": 1,
                                "title": "Create shielded non-interactive delivery wallet (nsd1...)",
                                "command": format!("newsatoshi --net {net} address --delivery")
                            },
                            {
                                "step": 2,
                                "title": if net == "testnet" { "Request free testnet LOVE from public faucet" } else { "Acquire Mainnet LOVE via trustless BTC<->LOVE atomic swap or AuxPoW merge-mining (chainid 20051)" },
                                "command": if net == "testnet" {
                                    "curl -sS -X POST https://newsatoshi-seed.fly.dev/api/faucet -H 'Content-Type: application/json' -d '{\"address\":\"<YOUR_NSD1_ADDRESS>\"}'".to_string()
                                } else {
                                    "Call MCP tool newsatoshi_compile_btc_love_swap or run `cd auxpow-stratum && docker compose up -d`".to_string()
                                }
                            },
                            {
                                "step": 3,
                                "title": "Scan incoming shielded notes & check balance",
                                "command": format!("newsatoshi --net {net} scan && newsatoshi --net {net} balance")
                            },
                            {
                                "step": 4,
                                "title": "Transact privately or pay per-call via HTTP 402 PayWord",
                                "command": format!("newsatoshi --net {net} send <PEER_NSD1_ADDRESS> 0.5 --memo agent-task-1"),
                                "http_402_challenge_url": format!("{base_url}/api/agent/402-challenge"),
                                "http_402_verify_url": format!("{base_url}/api/agent/402-verify")
                            }
                        ]
                    }))
                }
                "newsatoshi_create_402_challenge" => {
                    let resource = args
                        .get("resource")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty() && s.len() <= 128)
                        .unwrap_or("/api/inference");
                    let amount = args
                        .get("amount_love")
                        .and_then(|v| v.as_str())
                        .filter(|a| !a.is_empty() && a.len() <= 20 && a.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
                        .unwrap_or("0.01");
                    let net = args
                        .get("network")
                        .and_then(|v| v.as_str())
                        .filter(|s| *s == "mainnet" || *s == "testnet")
                        .unwrap_or("mainnet");
                    let recipient = args
                        .get("recipient")
                        .and_then(|v| v.as_str())
                        .filter(|a| a.len() <= 128 && a.bytes().all(|b| b.is_ascii_alphanumeric()))
                        .unwrap_or("");
                    let unit = args
                        .get("unit_price_base_units")
                        .and_then(|v| v.as_u64())
                        .filter(|&v| v > 0)
                        .unwrap_or(1_000);
                    Ok(build_http_402_challenge(resource, amount, net, recipient, unit))
                }
                "newsatoshi_get_mining_economics" => {
                    let ths = args
                        .get("hashrate_ths")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(1.2);
                    Ok(compute_network_metrics(seed_snapshot, ths))
                }
                "newsatoshi_get_backing_proof" => {
                    if let Some(snap) = backing_snapshot {
                        Ok(serde_json::to_value(snap).unwrap_or_default())
                    } else {
                        let api_state = engine.to_api_state();
                        let fresh = fetch_live_backing_summary(&api_state, seed_snapshot, false);
                        Ok(serde_json::to_value(fresh).unwrap_or_default())
                    }
                }
                "newsatoshi_spawn_charter" => {
                    serde_json::from_value::<SpawnCharterRequest>(args)
                        .map_err(|e| format!("invalid spawn charter arguments: {e}"))
                        .and_then(|req| {
                            let parent = req.parent_charter_id.as_deref().and_then(|pid| {
                                engine.web4_charters.iter().find(|c| c.charter_id == pid.trim())
                            });
                            let charter = compile_web4_charter(&req, parent, base_url)?;
                            let erc8004 = render_erc8004_registration(&charter, base_url);
                            Ok(json!({
                                "ok": true,
                                "persisted": false,
                                "charter": charter,
                                "erc8004_registration": erc8004
                            }))
                        })
                }
                "newsatoshi_list_charters" => Ok(json!({
                    "ok": true,
                    "count": engine.web4_charters.len(),
                    "canonical_backing_vault": CANONICAL_BACKING_VAULT,
                    "charters": engine.web4_charters
                })),
                "newsatoshi_discover_services" => {
                    let cap = args
                        .get("capability")
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim().to_ascii_lowercase())
                        .filter(|s| !s.is_empty());
                    let filtered: Vec<_> = engine
                        .web4_services
                        .iter()
                        .filter(|s| match &cap {
                            Some(c) => s.capabilities.iter().any(|x| x == c),
                            None => true,
                        })
                        .collect();
                    Ok(json!({
                        "ok": true,
                        "count": filtered.len(),
                        "services": filtered
                    }))
                }
                "newsatoshi_list_bounties" => Ok(json!({
                    "ok": true,
                    "count": engine.web4_bounties.len(),
                    "bounties": engine.web4_bounties
                })),
                "crypto_inspect_evm_contract" => {
                    let chain = args.get("chain").and_then(|v| v.as_str()).unwrap_or("base");
                    let address = args.get("address").and_then(|v| v.as_str()).unwrap_or("");
                    inspect_evm_contract(chain, address)
                }
                "crypto_bitcoin_mempool_oracle" => {
                    let address = args.get("address").and_then(|v| v.as_str());
                    fetch_bitcoin_mempool_oracle(address)
                }
                "crypto_decode_bolt11_invoice" => {
                    let invoice = args.get("invoice").and_then(|v| v.as_str()).unwrap_or("");
                    decode_bolt11_tool(invoice)
                }
                "crypto_quote_base_uniswap_v3" => {
                    let from_asset = args.get("from_asset").and_then(|v| v.as_str()).unwrap_or("ETH");
                    let to_asset = args.get("to_asset").and_then(|v| v.as_str()).unwrap_or("wLOVE");
                    let amount = args.get("amount").and_then(|v| v.as_str()).unwrap_or("0.001");
                    quote_base_uniswap_v3(from_asset, to_asset, amount)
                }
                "crypto_verify_tx_receipt" => {
                    let chain = args.get("chain").and_then(|v| v.as_str()).unwrap_or("base");
                    let tx_hash = args.get("tx_hash").and_then(|v| v.as_str()).unwrap_or("");
                    verify_tx_receipt(chain, tx_hash)
                }
                "newsatoshi_inspect_trust_bond" => {
                    let bid = args.get("bond_id_hex").and_then(|v| v.as_str()).unwrap_or("").trim().to_ascii_lowercase();
                    if bid.len() != 64 || !bid.chars().all(|c| c.is_ascii_hexdigit()) {
                        Err("bond_id_hex must be 64 hex characters".into())
                    } else {
                        let base = args
                            .get("seed_url")
                            .and_then(|v| v.as_str())
                            .map(|s| s.trim_end_matches('/').to_string())
                            .unwrap_or_else(|| "https://newsatoshi.org/mainnet".to_string());
                        let url = format!("{base}/api/bond?id={bid}");
                        match ureq::get(&url).call() {
                            Ok(mut resp) => {
                                let txt = resp.body_mut().read_to_string().unwrap_or_default();
                                let val: Value = serde_json::from_str(&txt).unwrap_or_else(|_| json!({ "raw": txt }));
                                Ok(json!({ "ok": true, "endpoint": url, "rfc": "RFC-0024", "bond": val }))
                            }
                            Err(e) => Ok(json!({
                                "ok": true,
                                "endpoint": url,
                                "rfc": "RFC-0024",
                                "live_node_supports_rfc0024": false,
                                "note": format!("Target node did not return /api/bond ({e}); in L1 v1.0.0-core, bond.rs was replaced by the unified Priorism-FEP PriorLock primitive at /api/prior?id=<HEX>."),
                                "bond_id_hex": bid
                            })),
                        }
                    }
                }
                "newsatoshi_inspect_rollup_vault" => {
                    let rid = args.get("rollup_id_hex").and_then(|v| v.as_str()).unwrap_or("").trim().to_ascii_lowercase();
                    if rid.len() != 64 || !rid.chars().all(|c| c.is_ascii_hexdigit()) {
                        Err("rollup_id_hex must be 64 hex characters".into())
                    } else {
                        let base = args
                            .get("seed_url")
                            .and_then(|v| v.as_str())
                            .map(|s| s.trim_end_matches('/').to_string())
                            .unwrap_or_else(|| "https://newsatoshi.org/mainnet".to_string());
                        let url = format!("{base}/api/rollup?id={rid}");
                        match ureq::get(&url).call() {
                            Ok(mut resp) => {
                                let txt = resp.body_mut().read_to_string().unwrap_or_default();
                                let val: Value = serde_json::from_str(&txt).unwrap_or_else(|_| json!({ "raw": txt }));
                                Ok(json!({ "ok": true, "endpoint": url, "rfc": "RFC-0023", "rollup": val }))
                            }
                            Err(e) => Ok(json!({
                                "ok": true,
                                "endpoint": url,
                                "rfc": "RFC-0023",
                                "live_node_supports_rfc0023": false,
                                "note": format!("Target node did not return /api/rollup ({e}); rollup.rs and zkvm.rs were deliberately removed from L1 consensus in v1.0.0-core to keep L1 minimal and eliminate unconstrained vault surfaces."),
                                "rollup_id_hex": rid
                            })),
                        }
                    }
                }
                "newsatoshi_get_protocol_signaling" => {
                    let base = args
                        .get("seed_url")
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim_end_matches('/').to_string())
                        .unwrap_or_else(|| "https://newsatoshi.org/mainnet".to_string());
                    let url = format!("{base}/api/signaling");
                    match ureq::get(&url).call() {
                        Ok(mut resp) => {
                            let txt = resp.body_mut().read_to_string().unwrap_or_default();
                            let val: Value = serde_json::from_str(&txt).unwrap_or_else(|_| json!({ "raw": txt }));
                            Ok(json!({ "ok": true, "endpoint": url, "rfc": "RFC-0026", "signaling": val }))
                        }
                        Err(e) => Ok(json!({
                            "ok": true,
                            "endpoint": url,
                            "rfc": "RFC-0026",
                            "live_node_supports_rfc0026": false,
                            "note": format!("Target node did not return /api/signaling ({e}); RFC-0026 BIP-9 signaling uses AuxPoW-safe header version bits 1..=7 in v0.4.0-substrate."),
                            "auxpow_safe_bits": [1, 2, 3, 4, 5, 6, 7]
                        })),
                    }
                }
                "newsatoshi_create_scoped_session_policy" => {
                    let session_id = args.get("session_id").and_then(|v| v.as_str()).unwrap_or("").trim();
                    let max_per_call = args.get("max_per_call").and_then(|v| v.as_u64()).unwrap_or(0);
                    let max_per_hour = args.get("max_per_hour").and_then(|v| v.as_u64()).unwrap_or(0);
                    let max_total = args.get("max_total").and_then(|v| v.as_u64()).unwrap_or(0);
                    if session_id.is_empty() || max_per_call == 0 || max_per_hour == 0 || max_total == 0 {
                        Err("session_id, max_per_call, max_per_hour, and max_total must be positive".into())
                    } else {
                        let min_bond = args.get("min_counterparty_bond").and_then(|v| v.as_u64()).unwrap_or(0);
                        let max_slashes = args.get("max_counterparty_slashes").and_then(|v| v.as_u64()).unwrap_or(0);
                        let sweep = args.get("profit_sweep_address").and_then(|v| v.as_str()).unwrap_or("");
                        let ttl = args.get("ttl_seconds").and_then(|v| v.as_u64()).unwrap_or(86_400);
                        let now = unix_now();
                        let expiry = now.saturating_add(ttl);
                        let canon = format!("nsSESSPOL1|{session_id}|{max_per_call}|{max_per_hour}|{max_total}|{min_bond}|{max_slashes}|{sweep}|{expiry}");
                        let policy_digest = hex::encode(sha256(canon.as_bytes()));
                        Ok(json!({
                            "ok": true,
                            "rfc": "RFC-0025",
                            "policy_digest_hex": policy_digest,
                            "policy": {
                                "session_id": session_id,
                                "max_per_call": max_per_call,
                                "max_per_hour": max_per_hour,
                                "max_total": max_total,
                                "min_counterparty_bond": min_bond,
                                "max_counterparty_slashes": max_slashes,
                                "profit_sweep_address": sweep,
                                "issued_at_unix": now,
                                "expiry_unix": expiry
                            },
                            "sdk_hint": "Pass this policy to createScopedAgentSession(...) in @newsatoshi/402 or ScopedAgentSession(...) in newsatoshi-402."
                        }))
                    }
                }
                "newsatoshi_launch_agent_mission" => {
                    serde_json::from_value::<crate::launchpad::LaunchMissionRequest>(args)
                        .map_err(|e| format!("invalid mission launchpad arguments: {e}"))
                        .and_then(|req| crate::launchpad::compile_mission_launchpad(&req, base_url))
                        .map(|mission| {
                            json!({
                                "ok": true,
                                "persisted": false,
                                "mission": mission
                            })
                        })
                }
                "newsatoshi_quote_launchpad_curve" => (|| -> Result<Value, String> {
                    let side = args
                        .get("side")
                        .and_then(|v| v.as_str())
                        .unwrap_or("buy")
                        .trim()
                        .to_ascii_lowercase();
                    let curve_reserve = args
                        .get("curve_reserve")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| "missing required integer 'curve_reserve'".to_string())?;
                    let total_shares = args
                        .get("total_shares")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| "missing required integer 'total_shares'".to_string())?;
                    let curve_fee_bps = args
                        .get("curve_fee_bps")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(500);
                    let fee_u16 = u16::try_from(curve_fee_bps)
                        .map_err(|_| "curve_fee_bps out of range".to_string())?;
                    match side.as_str() {
                        "buy" => {
                            let amount = args
                                .get("amount")
                                .and_then(|v| v.as_u64())
                                .ok_or_else(|| "missing required integer 'amount' for buy quote".to_string())?;
                            crate::launchpad::quote_curve_buy(curve_reserve, total_shares, fee_u16, amount)
                        }
                        "redeem" => {
                            let shares = args
                                .get("shares")
                                .and_then(|v| v.as_u64())
                                .ok_or_else(|| "missing required integer 'shares' for redeem quote".to_string())?;
                            let miner_fee = args
                                .get("miner_fee")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            crate::launchpad::quote_curve_redeem(curve_reserve, total_shares, fee_u16, shares, miner_fee)
                        }
                        other => Err(format!("invalid curve side '{other}' (expected 'buy' or 'redeem')")),
                    }
                })(),
                "newsatoshi_inspect_mission_launchpad" => {
                    let mid = args
                        .get("mission_id_hex")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_ascii_lowercase();
                    if mid.len() != 64 || !mid.chars().all(|c| c.is_ascii_hexdigit()) {
                        Err("mission_id_hex must be 64 hex characters".into())
                    } else {
                        let base = args
                            .get("seed_url")
                            .and_then(|v| v.as_str())
                            .map(|s| s.trim_end_matches('/').to_string())
                            .unwrap_or_else(|| "https://newsatoshi.org/mainnet".to_string());
                        let url = format!("{base}/api/mission?id={mid}");
                        match ureq::get(&url).call() {
                            Ok(mut resp) => {
                                let txt = resp.body_mut().read_to_string().unwrap_or_default();
                                let val: Value = serde_json::from_str(&txt).unwrap_or_else(|_| json!({ "raw": txt }));
                                Ok(json!({ "ok": true, "endpoint": url, "rfc": "RFC-0027", "mission": val }))
                            }
                            Err(e) => Ok(json!({
                                "ok": true,
                                "endpoint": url,
                                "rfc": "RFC-0027",
                                "live_node_supports_rfc0027": false,
                                "note": format!("Target node did not return /api/mission ({e}); mission.rs was deliberately removed from L1 consensus in v1.0.0-core in favor of the unified PriorLock primitive at /api/prior?id=<HEX> (mission launchpads coordinate off-chain/L2)."),
                                "mission_id_hex": mid
                            })),
                        }
                    }
                }
                "newsatoshi_format_swarm_task_claim" => {
                    let mid = args
                        .get("mission_id_hex")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "missing required 'mission_id_hex'".to_string());
                    let task_id = args
                        .get("task_id")
                        .and_then(|v| v.as_u64())
                        .and_then(|v| u32::try_from(v).ok())
                        .ok_or_else(|| "missing required u32 'task_id'".to_string());
                    let spec = args
                        .get("spec_hash_hex")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "missing required 'spec_hash_hex'".to_string());
                    let reward = args
                        .get("reward")
                        .and_then(|v| v.as_u64())
                        .ok_or_else(|| "missing required integer 'reward'".to_string());
                    let proof = args.get("proof_hash_hex").and_then(|v| v.as_str());
                    let apk = args.get("contributor_apk_hex").and_then(|v| v.as_str());
                    let miner_fee = args.get("miner_fee").and_then(|v| v.as_u64()).unwrap_or(0);
                    match (mid, task_id, spec, reward) {
                        (Ok(m), Ok(t), Ok(s), Ok(r)) => {
                            crate::launchpad::format_task_digests(m, t, s, r, proof, apk, miner_fee)
                        }
                        (Err(e), _, _, _) | (_, Err(e), _, _) | (_, _, Err(e), _) | (_, _, _, Err(e)) => Err(e),
                    }
                }
                "newsatoshi_verify_zktls_proof" => {
                    serde_json::from_value::<crate::zktls::ZkTlsProofPayload>(args)
                        .map_err(|e| format!("invalid zkTLS proof payload: {e}"))
                        .and_then(|p| crate::zktls::verify_zktls_proof(&p))
                }
                "newsatoshi_check_v2_cutover_readiness" => {
                    Ok(crate::cutover::evaluate_v2_cutover_readiness(seed_snapshot))
                }
                "newsatoshi_compile_bonded_p2p_offer" => {
                    serde_json::from_value::<crate::p2p::BondedP2pOfferRequest>(args)
                        .map_err(|e| format!("invalid bonded P2P offer arguments: {e}"))
                        .and_then(|req| crate::p2p::compile_bonded_p2p_offer(&req, base_url))
                        .map(|offer| json!({ "ok": true, "persisted": false, "offer": offer }))
                }
                "newsatoshi_get_nsusd_state" => {
                    let eng = crate::nsusd::NsUsdEngine::new();
                    Ok(eng.summary_json(unix_now()))
                }
                "newsatoshi_simulate_nsusd_stress" => {
                    let crash = args.get("crash_pct").and_then(|v| v.as_u64()).unwrap_or(50);
                    Ok(crate::nsusd::run_adversarial_stress_suite(crash))
                }
                "newsatoshi_benchmark_l2_perfection_batch" => {
                    let count = args.get("tx_count").and_then(|v| v.as_u64()).unwrap_or(2_000) as usize;
                    let eng = crate::nsusd::NsUsdEngine::new();
                    crate::l2_rollup::run_live_l2_batch_benchmark(count, seed_snapshot, eng.solvency_root())
                }
                "newsatoshi_generate_private_x402_middleware" => {
                    let fw = args.get("framework").and_then(|v| v.as_str()).unwrap_or("express");
                    let price = args.get("price_usd").and_then(|v| v.as_f64()).unwrap_or(0.005);
                    let route = args.get("route_path").and_then(|v| v.as_str()).unwrap_or("/api/paid-endpoint");
                    let pay_to = args.get("pay_to_evm").and_then(|v| v.as_str());
                    let seller = args.get("seller_ns1").and_then(|v| v.as_str());
                    crate::x402::build_private_x402_paywall_kit(fw, price, route, pay_to, seller, base_url)
                }
                "newsatoshi_verify_base_nsusd_escrow" => {
                    let tx_hash = args
                        .get("tx_hash")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "missing required 'tx_hash'".to_string());
                    tx_hash.and_then(crate::backing::verify_base_usdc_escrow_tx)
                }
                "newsatoshi_estimate_paywords_savings" => {
                    let calls = args.get("monthly_calls").and_then(|v| v.as_u64()).unwrap_or(1_000_000);
                    let price = args.get("price_per_call_usd").and_then(|v| v.as_f64()).unwrap_or(0.005);
                    let batch = args.get("calls_per_batch").and_then(|v| v.as_u64()).unwrap_or(1_000);
                    crate::paywords_launchpad::estimate_paywords_cost_savings(calls, price, batch)
                }
                "newsatoshi_compile_payword_channel_setup" => {
                    let fw = args.get("framework").and_then(|v| v.as_str()).unwrap_or("cloudflare_worker");
                    let rec = args.get("recipient_evm").and_then(|v| v.as_str());
                    let price = args.get("price_usd").and_then(|v| v.as_f64()).unwrap_or(0.002);
                    let steps = args.get("max_steps").and_then(|v| v.as_u64()).unwrap_or(1_000) as u32;
                    let route = args.get("route_path").and_then(|v| v.as_str()).unwrap_or("/api/inference");
                    crate::paywords_launchpad::compile_payword_channel_setup(
                        fw, rec, price, steps, route, base_url,
                    )
                }
                "newsatoshi_simulate_fair_curve_launch" => {
                    let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Agent API Curve");
                    let sym = args.get("symbol").and_then(|v| v.as_str()).unwrap_or("AGAPI");
                    let v_usdc = args.get("virtual_reserve_usdc").and_then(|v| v.as_f64()).unwrap_or(10_000.0);
                    let v_sup = args.get("virtual_supply_tokens").and_then(|v| v.as_u64()).unwrap_or(1_000_000_000);
                    let fee_bps = args.get("swap_fee_bps").and_then(|v| v.as_u64()).unwrap_or(100) as u16;
                    let buy_usdc = args.get("buy_usdc").and_then(|v| v.as_f64()).unwrap_or(1_000.0);
                    let rev_usdc = args.get("paywords_revenue_usdc").and_then(|v| v.as_f64()).unwrap_or(500.0);
                    let sink = args.get("fee_sink_evm").and_then(|v| v.as_str());
                    crate::paywords_launchpad::simulate_fair_curve_launch(
                        name, sym, v_usdc, v_sup, fee_bps, buy_usdc, rev_usdc, sink,
                    )
                }
                other => Err(format!("unknown MCP tool '{other}'")),
            };

            match tool_result {
                Ok(val) => ok_resp(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&val).unwrap_or_default()
                    }],
                    "structuredContent": val,
                    "isError": false
                })),
                Err(msg) => ok_resp(json!({
                    "content": [{ "type": "text", "text": msg }],
                    "isError": true
                })),
            }
        }
        _ => err_resp(-32601, format!("MCP method not found: '{method}'")),
    }
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
    use crate::StreamingChannel;

    #[test]
    fn payword_402_verification_matches_streaming_channel_and_rejects_forgery() {
        let seed = sha256(b"agent_session_secret_seed_1");
        let (channel, chain) = StreamingChannel::create_with_secret_chain(seed, 50, 1_000);
        let tip_hex = hex::encode(channel.tip);

        // Reveal tick #7 -> chain[50 - 7] = chain[43]
        let tok7_hex = hex::encode(chain[43]);
        let res = verify_payword_token(&tip_hex, &tok7_hex, 7, 1_000).unwrap();
        assert!(res.valid);
        assert_eq!(res.hashes_evaluated, 7);
        assert_eq!(res.total_paid_base_units, 7_000);
        assert_eq!(res.next_required_ticks, 8);

        // Reveal tick #8 incrementally against tick #7 -> O(1), strictly 1 SHA-256 hash!
        let tok8_hex = hex::encode(chain[42]);
        let inc = verify_payword_token_incremental(&tip_hex, &tok8_hex, 8, 1_000, Some(&tok7_hex), 7).unwrap();
        assert!(inc.valid);
        assert_eq!(inc.verification_mode, "incremental");
        assert_eq!(inc.hashes_evaluated, 1);
        assert_eq!(inc.total_paid_base_units, 8_000);

        // Wrong tick count (claiming 8 ticks with tick #7 token) MUST fail!
        let bad_ticks = verify_payword_token(&tip_hex, &tok7_hex, 8, 1_000).unwrap();
        assert!(!bad_ticks.valid);
        assert_eq!(bad_ticks.total_paid_base_units, 0);

        // Forged token MUST fail!
        let forged = hex::encode(sha256(b"forged"));
        let bad_tok = verify_payword_token(&tip_hex, &forged, 7, 1_000).unwrap();
        assert!(!bad_tok.valid);
    }

    #[test]
    fn mcp_initialize_tools_list_and_payword_call_work() {
        let engine = CommonsEngine::new_empty();
        let snap = SeedSnapshot {
            live: false,
            endpoint: "http://127.0.0.1:1/status".into(),
            fetched_at_unix: 0,
            status: None,
            mainnet_info: crate::seed::MainnetInfo::verified_v0_1_0(),
            mainnet_live: false,
            mainnet_endpoint: None,
            mainnet_status: None,
            mainnet_error: None,
            error: None,
        };
        let client = SeedStatusClient::default_testnet();

        let (code, init) = handle_mcp_jsonrpc(
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            DEFAULT_PUBLIC_BASE_URL,
            &engine,
            &snap,
            None,
            &client,
            None,
        );
        assert_eq!(code, 200);
        assert_eq!(init.unwrap()["result"]["serverInfo"]["name"], "newsatoshi-commons");

        let (_, list) = handle_mcp_jsonrpc(
            &json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
            DEFAULT_PUBLIC_BASE_URL,
            &engine,
            &snap,
            None,
            &client,
            None,
        );
        assert_eq!(list.unwrap()["result"]["tools"].as_array().unwrap().len(), 38);
    }
}
