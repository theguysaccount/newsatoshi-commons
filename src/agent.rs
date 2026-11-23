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
        ("/api/health", "0.6", "hourly"),
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
            "newsatoshi_agent_quickstart"
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

## Live Network Status
- {mainnet_line}
- {testnet_line}
- Commons Covenant Rule Hash: `{rule_hash}` (`verify_only={verify_only}`, `{key_mode}`)
- Commons Treasury Balance: `{bal} LOVE` (`nonce={nonce}`, `parcels={parcels}`, `kwh={kwh}`, `starks={starks}`)

## Agent Interfaces (Zero Auth Required for All Reads, Compilers & Verifiers)
- [Streamable HTTP MCP Server]({base_url}/mcp): JSON-RPC 2.0 MCP endpoint (`initialize`, `tools/list`, `tools/call`, `resources/list`, `resources/read`) with 7 live tools.
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
- `newsatoshi_compile_btc_love_swap` — Compile canonical BIP-199 Bitcoin P2WSH HTLC (`bc1q…`) or decode Lightning BOLT-11 invoice
- `newsatoshi_verify_onchain_lock` — Verify a SHA-256 lock live on Newsatoshi Mainnet/Testnet + Bitcoin `mempool.space`
- `newsatoshi_verify_payword_402` — Verify a pure-SHA-256 PayWord HTTP 402 micropayment token (`SHA256^ticks(token) == tip`)
- `newsatoshi_verify_stark_proof` — Cryptographically re-verify a Winterfell STARK covenant transition proof
- `newsatoshi_agent_quickstart` — Structured onboarding commands for autonomous agents

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
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Newsatoshi Commons & Agent Transaction API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Machine-readable REST + MCP + HTTP 402 PayWord + BTC-LOVE HTLC Atomic Swap interface for Newsatoshi (LOVE) and NEWSATOSHI/2.1 STARK covenants."
        },
        "servers": [{ "url": base_url }],
        "paths": {
            "/mcp": {
                "post": {
                    "summary": "Streamable HTTP MCP JSON-RPC 2.0 endpoint",
                    "description": "Supports initialize, ping, tools/list, tools/call, resources/list, and resources/read with zero authentication required.",
                    "responses": { "200": { "description": "JSON-RPC 2.0 response" } }
                }
            },
            "/api/health": {
                "get": { "summary": "Server health, version, and verify-only key-isolation status", "responses": { "200": { "description": "OK" } } }
            },
            "/api/seed": {
                "get": { "summary": "Live dual-chain Newsatoshi L1 Testnet + Mainnet status", "responses": { "200": { "description": "Live chain telemetry" } } }
            },
            "/api/state": {
                "get": { "summary": "Live Commons ledger state, public roots, STARK proofs, anchors, and swap board", "responses": { "200": { "description": "Commons state" } } }
            },
            "/api/swaps": {
                "get": { "summary": "List persisted non-custodial BTC-LOVE atomic swap contracts", "responses": { "200": { "description": "Swap list" } } },
                "post": { "summary": "Publish a verified swap contract (operator bearer auth required)", "responses": { "200": { "description": "Published" }, "401": { "description": "Unauthorized" } } }
            },
            "/api/swaps/compile": {
                "post": { "summary": "Public stateless BIP-199 Bitcoin P2WSH HTLC & Lightning BOLT-11 swap compiler", "responses": { "200": { "description": "Compiled swap contract" } } }
            },
            "/api/swaps/verify": {
                "post": { "summary": "Public live dual-chain verifier (Newsatoshi L1 /api/lock + Bitcoin mempool.space + preimage)", "responses": { "200": { "description": "Verification report" } } }
            },
            "/api/agent/402-challenge": {
                "get": { "summary": "Emit an HTTP 402 Payment Required dual-rail challenge (L1 hash-lock + PayWord hash-chain)", "responses": { "402": { "description": "Payment Required challenge" } } }
            },
            "/api/agent/402-verify": {
                "post": { "summary": "Verify a PayWord SHA-256 micropayment token or live Newsatoshi L1 hash-lock payment", "responses": { "200": { "description": "Cryptographic payment verification result" } } }
            },
            "/api/telemetry": {
                "post": { "summary": "Submit offline-signed post-quantum XMSS or NIST FIPS 205 SLH-DSA hardware energy telemetry", "responses": { "200": { "description": "Verified & STARK-proven" } } }
            },
            "/api/deeds": {
                "post": { "summary": "Submit offline-signed post-quantum trustee land deed attestation", "responses": { "200": { "description": "Verified & STARK-proven" } } }
            },
            "/api/actions/verify-anchor": {
                "post": { "summary": "Public live L1 cross-check of a Commons state-root anchor against /api/lock?hash=", "responses": { "200": { "description": "On-chain anchor verification" } } }
            },
            "/api/actions/verify-stark": {
                "post": { "summary": "Public cryptographic re-verification of any Winterfell STARK proof", "responses": { "200": { "description": "STARK verify result" } } }
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
                "description": "Compile a trustless BTC <-> LOVE atomic swap: canonical BIP-199 Bitcoin SegWit v0 P2WSH HTLC address (bc1q.../tb1q...) or decoded Lightning BOLT-11 invoice plus exact newsatoshi L1 lock CLI commands.",
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
                        "btc_current_height": { "type": "integer", "description": "Optional current BTC tip for anti-race margin check" }
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
        assert_eq!(list.unwrap()["result"]["tools"].as_array().unwrap().len(), 7);
    }
}
