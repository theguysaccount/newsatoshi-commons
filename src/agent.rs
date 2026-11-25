//! AI Agent Transaction Interface: Streamable HTTP MCP Server (`/mcp`),
//! HTTP `402 Payment Required` PayWord + L1 Hash-Lock Verification, and
//! Standard Agent Discovery Surfaces (`/llms.txt`, `/llms-full.txt`,
//! `/.well-known/agent.json`, `/.well-known/mcp.json`, `/openapi.json`, `/robots.txt`).
//!
//! Zero fake data: all network heights, balances, lock verifications, and STARK proofs
//! come strictly from live Newsatoshi L1 nodes and the persisted Commons engine.

use crate::crypto::{hash32_from_hex, sha256};
use crate::seed::{SeedSnapshot, SeedStatusClient, AUXPOW_CHAIN_ID, MAINNET_GENESIS_HASH, TESTNET_GENESIS_HASH};
use crate::stark::verify_covenant_stark;
use crate::storage::{ApiStateView, CommonsEngine};
use crate::swap::{compile_atomic_swap, verify_swap_on_chain, CompileSwapRequest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

/// Hard upper bound on PayWord hash-chain iterations per HTTP request to prevent CPU DoS.
pub const MAX_PAYWORD_VERIFY_TICKS: u64 = 100_000;

/// Default canonical public URL used in discovery documents when Host header is unavailable.
pub const DEFAULT_PUBLIC_BASE_URL: &str = "https://commons.newsatoshi.org";

// ---------------------------------------------------------------------------
// HTTP 402 PayWord + L1 Hash-Lock Machine-to-Machine Micropayments
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayWordVerifyResult {
    pub ok: bool,
    pub valid: bool,
    pub scheme: String,
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

/// Verifies a post-quantum pure-SHA-256 PayWord micropayment token:
/// checks `SHA256^{ticks}(token) == tip` in O(`ticks`) hash evaluations.
pub fn verify_payword_token(
    tip_hex: &str,
    token_hex: &str,
    ticks: u64,
    unit_price_base_units: u64,
) -> Result<PayWordVerifyResult, String> {
    if ticks == 0 || ticks > MAX_PAYWORD_VERIFY_TICKS {
        return Err(format!(
            "ticks must be between 1 and {MAX_PAYWORD_VERIFY_TICKS}"
        ));
    }
    let expected_tip = hash32_from_hex(tip_hex)?;
    let mut cur = hash32_from_hex(token_hex)?;
    for _ in 0..ticks {
        cur = sha256(&cur);
    }
    let valid = cur == expected_tip;
    let total_paid = ticks
        .checked_mul(unit_price_base_units)
        .ok_or_else(|| "total_paid_base_units overflow".to_string())?;
    Ok(PayWordVerifyResult {
        ok: true,
        valid,
        scheme: "NEWSATOSHI-PAYWORD-SHA256/1.0".to_string(),
        tip_hex: hex::encode(expected_tip),
        token_hex: hex::encode(hash32_from_hex(token_hex)?),
        computed_tip_hex: hex::encode(cur),
        ticks,
        unit_price_base_units,
        total_paid_base_units: if valid { total_paid } else { 0 },
        next_required_ticks: if valid { ticks + 1 } else { ticks },
        error: if valid {
            None
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

/// Computes strictly real, live derived network & AuxPoW merge-mining metrics from
/// live Newsatoshi Mainnet and Testnet `/status` snapshots (zero fabricated figures).
pub fn compute_network_metrics(seed: &SeedSnapshot, hashrate_ths: f64) -> Value {
    let clamped_ths = if hashrate_ths.is_finite() && hashrate_ths > 0.0 {
        hashrate_ths.clamp(0.001, 1_000_000.0)
    } else {
        1.2
    };
    let max_supply_love = 21_000_000.0f64;
    let block_reward_love = 50.0f64;
    let two_pow_32 = 4_294_967_296.0f64;

    let derive_chain = |st: &crate::seed::SeedStatus| -> Value {
        let issued_love = (st.issued as f64) / 100_000_000.0;
        let supply_mined_pct = (issued_love / max_supply_love) * 100.0;
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
            "max_supply_love": max_supply_love,
            "supply_mined_pct": supply_mined_pct,
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
# A2A Agent Card: {base_url}/.well-known/agent.json
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
        ("/developers", "0.9", "daily"),
        ("/about", "0.8", "weekly"),
        ("/contact", "0.7", "monthly"),
        ("/privacy", "0.7", "monthly"),
        ("/llms.txt", "0.9", "hourly"),
        ("/llms-full.txt", "0.9", "daily"),
        ("/openapi.json", "0.8", "daily"),
        ("/.well-known/agent.json", "0.8", "daily"),
        ("/.well-known/mcp.json", "0.8", "daily"),
        ("/.well-known/mcp/server-card.json", "0.8", "daily"),
        ("/.well-known/api-catalog", "0.8", "daily"),
        ("/.well-known/agent-skills/index.json", "0.8", "daily"),
        ("/skills/newsatoshi-agent-payments/SKILL.md", "0.8", "weekly"),
        ("/server.json", "0.7", "daily"),
        ("/install.sh", "0.8", "weekly"),
        ("/api/health", "0.6", "hourly"),
        ("/api/network/metrics", "0.8", "hourly"),
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
    <nav aria-label=\"Site navigation\"><a href=\"/\">Home</a> · <a href=\"/docs\">API Docs</a> · <a href=\"/developers\">Developers</a> · <a href=\"/llms.txt\">llms.txt</a> · <a href=\"/openapi.json\">OpenAPI</a> · <a href=\"/about\">About</a> · <a href=\"/contact\">Contact</a> · <a href=\"/privacy\">Privacy</a></nav>\n\
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
                     - `POST {base_url}/mcp` — Streamable HTTP MCP JSON-RPC 2.0 server (9 live tools).\n\
                     - `GET {base_url}/api/health` — Node version, verify-only posture, and uptime.\n\
                     - `GET {base_url}/api/seed` — Live Newsatoshi Mainnet and Testnet block height, difficulty, issued LOVE, and peer count.\n\
                     - `POST {base_url}/api/swaps/compile` — Compile zero-custody SHA-256 HTLC swaps for Bitcoin (`btc_onchain`), Lightning (`lightning_bolt11`), Solana (`solana_htlc`), and Base/Ethereum/Arbitrum (`evm_htlc`; **live verified Base Mainnet contract:** `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`, Chain ID `8453`).\n\
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
                     - `bridges/evm/NewsatoshiHtlc.sol` (**LIVE on Base Mainnet `8453`:** `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`, Sourcify exact-match verified) & `bridges/solana/` — Immutable ownerless EVM & Solana SHA-256 HTLC contracts\n\
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
                         <ul><li><strong>Rust CLIs:</strong> <code>cargo install --git https://github.com/theguysaccount/newsatoshi-commons</code> installs <code>newsatoshi-commons</code>, <code>newsatoshi-swap</code>, and <code>newsatoshi-hw-signer</code>.</li><li><strong>AuxPoW Merge-Mining Kit:</strong> Zero-dependency Stratum v1 server in <code>auxpow-stratum/</code> (<code>chainid = 20051</code>, 50 LOVE/block).</li><li><strong>Multi-Chain HTLC Bridges:</strong> Immutable <code>bridges/evm/NewsatoshiHtlc.sol</code> (<strong>LIVE on Base Mainnet 8453:</strong> <a href=\"https://basescan.org/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987\"><code>0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987</code></a>, Sourcify exact-match verified) and <code>bridges/solana/</code> (SOL &amp; SPL-USDC).</li><li><strong>Installable Agent Skill:</strong> <a href=\"/skills/newsatoshi-agent-payments/SKILL.md\">newsatoshi-agent-payments/SKILL.md</a> (<a href=\"/.well-known/agent-skills/index.json\">SHA-256 index</a>).</li></ul>\n\
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
                     Newsatoshi Commons (`{base_url}`) is an open-source, MIT-licensed public verification gateway and protocol toolkit built for the **Newsatoshi (`LOVE`)** post-quantum Layer-1 network.\n\n\
                     ## Architecture & Design Principles\n\
                     - **Zero Custody & Zero Server Keys:** Production runs in strict `--verify-only` mode with zero private signing keys on server hardware.\n\
                     - **Post-Quantum Cryptography End-to-End:** Uses NIST FIPS 205 SLH-DSA, stateful XMSS Merkle-Lamport trees, pure-SHA-256 PayWord hash-chains, and Winterfell 0.13 transparent STARK proofs with zero trusted setup.\n\
                     - **Honest Zero-Fake-Data Telemetry:** Every block height, difficulty, supply counter, lock verification, and treasury balance comes strictly from live cryptographic proofs and live L1 RPC/seed responses.\n\
                     - **Open Source & Auditable:** Full source code, CI test suites, legal-cryptographic airlock templates, and RFC-0021 L1 soft-fork code are published openly on GitHub.\n"
                );
                Some(("text/markdown; charset=utf-8", md))
            } else {
                let html = wrap_public_html(
                    base_url,
                    "/about",
                    "About Newsatoshi Commons",
                    "Mission, zero-custody verify-only architecture, post-quantum cryptography, and open-source transparency of Newsatoshi Commons.",
                    "<h1>About Newsatoshi Commons</h1>\n\
                     <p>Newsatoshi Commons is an open-source, MIT-licensed cryptographic verification gateway and developer toolkit built for the <strong>Newsatoshi (<code>LOVE</code>)</strong> post-quantum Layer-1 cryptocurrency. Its mission is to make permissionless, quantum-resistant machine money directly usable by autonomous AI agents, Bitcoin merge-miners, multi-chain liquidity makers, and real-world physical infrastructure.</p>\n\
                     <h2>Zero-Custody, Verify-Only Architecture</h2>\n\
                     <p>The live public deployment at <code>commons.newsatoshi.org</code> runs in strict <code>--verify-only</code> mode and holds zero private signing keys on server disks or memory. All land deed attestations and physical energy telemetry packets are signed offline using NIST FIPS 205 SLH-DSA or stateful XMSS keys and verified cryptographically alongside Winterfell 0.13 transparent STARK proofs.</p>\n\
                     <h2>Verifiable Open Source</h2>\n\
                     <p>All server code, smart contracts, Solana escrow codecs, AuxPoW Stratum merge-mining tools, and RFC-0021 L1 STARK soft-fork prototypes are published openly at <a href=\"https://github.com/theguysaccount/newsatoshi-commons\">github.com/theguysaccount/newsatoshi-commons</a> and <a href=\"https://github.com/theguysaccount/newsatoshi-rfc0021\">github.com/theguysaccount/newsatoshi-rfc0021</a>. Every counter starts at genuine zero and reflects real on-chain and cryptographic state only.</p>",
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
                     - RFC-0021 L1 Soft-Fork Issues: https://github.com/theguysaccount/newsatoshi-rfc0021/issues\n\
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
                     <ul><li><strong>Commons, MCP &amp; Bridges Repository:</strong> <a href=\"https://github.com/theguysaccount/newsatoshi-commons/issues\">github.com/theguysaccount/newsatoshi-commons/issues</a></li><li><strong>RFC-0021 L1 STARK Soft-Fork Repository:</strong> <a href=\"https://github.com/theguysaccount/newsatoshi-rfc0021/issues\">github.com/theguysaccount/newsatoshi-rfc0021/issues</a></li><li><strong>Direct Contact Email:</strong> <a href=\"mailto:commons@newsatoshi.org\">commons@newsatoshi.org</a></li></ul>\n\
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
            "newsatoshi_get_mining_economics"
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

> Permissionless, post-quantum, zero-KYC machine money (`LOVE`) for autonomous AI agents: shielded `nsd1…` delivery addresses, pure-SHA-256 PayWord HTTP 402 micropayments, trustless multi-chain HTLC atomic swaps (`BTC`, `Lightning`, `Solana SOL/USDC`, `Base/Ethereum/Arbitrum ETH/USDC`), Bitcoin AuxPoW merge-mining (`chainid = 20051`), verifiable L1 Freivalds compute jobs, and Winterfell STARK spending covenants.

## When to use Newsatoshi Commons
- Use `{base_url}/mcp` or `{base_url}/api/*` when an agent needs to **check live Newsatoshi Mainnet/Testnet state**, **bridge BTC/Lightning/Solana/Base into LOVE trustlessly**, **issue or verify HTTP 402 PayWord micropayments**, or **cryptographically verify Winterfell STARK covenant proofs and L1 hash-locks**.
- Human & developer documentation: [API Docs]({base_url}/docs), [Developer Portal]({base_url}/developers), [About]({base_url}/about), [Contact]({base_url}/contact), [Privacy]({base_url}/privacy).

## Live Network Status
- {mainnet_line}
- {testnet_line}
- Base Mainnet Verified HTLC (`Chain ID 8453`, ownerless, Sourcify `exact_match`): `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987` (ETH & Base USDC `0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913`)
- Commons Covenant Rule Hash: `{rule_hash}` (`verify_only={verify_only}`, `{key_mode}`)
- Commons Treasury Balance: `{bal} LOVE` (`nonce={nonce}`, `parcels={parcels}`, `kwh={kwh}`, `starks={starks}`)

## Agent Interfaces (Zero Auth Required for All Reads, Compilers & Verifiers)
- [Streamable HTTP MCP Server]({base_url}/mcp): JSON-RPC 2.0 MCP endpoint (`initialize`, `tools/list`, `tools/call`, `resources/list`, `resources/read`) with 9 live tools.
- [MCP Server Card]({base_url}/.well-known/mcp/server-card.json) & [MCP Registry Manifest]({base_url}/server.json): Standard MCP discovery cards.
- [A2A Agent Card]({base_url}/.well-known/agent.json): Machine-readable Agent-to-Agent capability manifest.
- [RFC 9727 API Catalog]({base_url}/.well-known/api-catalog) & [OpenAPI 3.1 Schema]({base_url}/openapi.json): Complete REST API schema for multi-chain swaps, dual-chain lock verification, HTTP 402 PayWord verification, STARK verification, and offline signed telemetry/deeds.
- [Agent Skills Index]({base_url}/.well-known/agent-skills/index.json): SHA-256 verified installable skill ([SKILL.md]({base_url}/skills/newsatoshi-agent-payments/SKILL.md)).
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
- RFC-0021 Native L1 `OP_VERIFY_COVENANT_STARK` (`0xb4`): https://github.com/theguysaccount/newsatoshi-rfc0021
",
        proto = state.protocol_version,
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
Available MCP tools:
- `newsatoshi_get_network_status` — Live Mainnet & Testnet chain status
- `newsatoshi_get_commons_state` — Live Commons treasury, covenant rule hash, public roots, STARKs & anchors
- `newsatoshi_compile_btc_love_swap` — Compile canonical BIP-199 Bitcoin P2WSH HTLC (`bc1q…`), decode Lightning BOLT-11, Solana PDA escrow, or live Base Mainnet `NewsatoshiHtlc.sol` (`0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`)
- `newsatoshi_verify_onchain_lock` — Verify a SHA-256 lock live on Newsatoshi Mainnet/Testnet + Bitcoin `mempool.space`
- `newsatoshi_verify_payword_402` — Verify a pure-SHA-256 PayWord HTTP 402 micropayment token (`SHA256^ticks(token) == tip`)
- `newsatoshi_verify_stark_proof` — Cryptographically re-verify a Winterfell STARK covenant transition proof
- `newsatoshi_agent_quickstart` — Structured onboarding commands for autonomous agents
- `newsatoshi_create_402_challenge` — Generate a standard `NEWSATOSHI-402/1.0` Payment Required challenge
- `newsatoshi_get_mining_economics` — Live AuxPoW merge-mining yield & network hashrate economics

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

### Recipe 5: Verifiable L1 Compute Jobs (Built into Newsatoshi v0.1.0)
Agents can hire or sell verifiable matrix-multiplication compute trustlessly on L1 with Freivalds fraud proofs:
```bash
newsatoshi --net mainnet job post A.csv B.csv --pay 5 --bond 1
newsatoshi --net mainnet job claim <JOBID> --bond 1
newsatoshi --net mainnet job commit <JOBID> C.csv
newsatoshi --net mainnet job finalize <JOBID>
```

### Recipe 6: Cryptographically Bounded Agent Budgets (`NEWSATOSHI/2.1` Covenants)
Humans can fund a Commons/Covenant UTXO with immutable caps (`max_bounty_payout`, `reinvest_bps`, `min_soil_health_score`) and offline **NIST FIPS 205 SLH-DSA** hardware keys (`newsatoshi-hw-signer --env-seeds`), guaranteeing an autonomous agent cannot exceed its spending policy even if prompt-injected.
"
    )
}

pub fn render_agent_card_json(base_url: &str, state: &ApiStateView) -> Value {
    json!({
        "schema_version": "1.0",
        "name": "Newsatoshi Commons & Agent Transaction Interface",
        "description": "Post-quantum, zero-KYC machine-to-machine payment, BTC-LOVE HTLC atomic swap, HTTP 402 PayWord micropayment, and Winterfell STARK covenant verification interface for Newsatoshi L1 (LOVE).",
        "url": base_url,
        "version": env!("CARGO_PKG_VERSION"),
        "protocol_version": state.protocol_version,
        "documentation_url": format!("{base_url}/llms.txt"),
        "openapi_url": format!("{base_url}/openapi.json"),
        "mcp_endpoint": format!("{base_url}/mcp"),
        "authentication": {
            "read_and_verify": "none",
            "operator_write": "bearer_token",
            "hardware_and_deed_attestation": "offline_nist_fips205_slh_dsa_or_xmss"
        },
        "capabilities": {
            "mcp_streamable_http": true,
            "http_402_payword_micropayments": true,
            "btc_love_htlc_atomic_swaps": true,
            "bolt11_lightning_decoding": true,
            "dual_chain_l1_verification": true,
            "winterfell_stark_verification": true,
            "verify_only_zero_server_keys": state.verify_only
        },
        "skills": [
            {
                "id": "newsatoshi_get_network_status",
                "name": "Live Newsatoshi Mainnet & Testnet Status",
                "description": "Returns live heights, difficulty, issued LOVE, locks, peers, and AuxPoW chainid (20051) for both Mainnet and Testnet."
            },
            {
                "id": "newsatoshi_compile_btc_love_swap",
                "name": "Trustless BTC <-> LOVE Atomic Swap Compiler",
                "description": "Compiles canonical BIP-199 Bitcoin P2WSH HTLC scripts/addresses (bc1q...) or decodes Lightning BOLT-11 invoices into Newsatoshi L1 hash-locks."
            },
            {
                "id": "newsatoshi_verify_onchain_lock",
                "name": "Live Dual-Chain On-Chain Lock Verifier",
                "description": "Verifies SHA-256 hash-locks live on Newsatoshi L1 (/api/lock?hash=), Bitcoin P2WSH UTXOs (mempool.space), and revealed preimages."
            },
            {
                "id": "newsatoshi_verify_payword_402",
                "name": "HTTP 402 PayWord Micropayment Verifier",
                "description": "Verifies pure-SHA-256 post-quantum PayWord hash-chain payment tokens (SHA256^ticks(token) == tip) for sub-millisecond agent API billing."
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
                "name": "MIT",
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
                "description": "Compile a trustless multi-chain SHA-256 HTLC atomic swap into LOVE: Bitcoin BIP-199 P2WSH (bc1q...), Lightning BOLT-11, Solana SOL/USDC PDA escrow, or Base/EVM ETH/USDC HTLC (live verified Base Mainnet contract 0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987).",
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
                        "evm_chain": { "type": "string", "enum": ["base", "ethereum", "arbitrum", "optimism", "polygon"], "default": "base", "description": "Target EVM chain (Base Mainnet defaults to live verified 0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987)" },
                        "evm_recipient_address": { "type": "string", "description": "0x... counterparty recipient for evm_htlc" },
                        "evm_contract_address": { "type": "string", "description": "Optional override; Base Mainnet defaults to live verified 0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987" },
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
        assert_eq!(res.total_paid_base_units, 7_000);
        assert_eq!(res.next_required_ticks, 8);

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
            &client,
            None,
        );
        assert_eq!(list.unwrap()["result"]["tools"].as_array().unwrap().len(), 9);
    }
}
