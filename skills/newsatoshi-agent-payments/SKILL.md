---
name: newsatoshi-agent-payments
description: Use when an AI agent needs multi-chain O(1) SHA-256 PayWord HTTP 402 micropayments (Base, Optimism, Arbitrum — 0% protocol fee), trustless cross-chain atomic swaps into post-quantum Newsatoshi (LOVE) from BTC/Lightning/Solana/Base/Optimism/Arbitrum/Polygon, Bitcoin AuxPoW merge-mining setup, or zero-API-key EVM security audits, Bitcoin mempool fees, Lightning BOLT-11 decoding, and Base Uniswap V3 quotes.
---

# Newsatoshi Commons — Multi-Chain PayWords, Atomic Swaps & Zero-API-Key Crypto MCP (`v0.35.0`)

## When to use this skill

Connect to `https://commons.newsatoshi.org/mcp` (46 zero-auth tools) or install directly (`npm i github:theguysaccount/newsatoshi-commons` / `pip install "git+https://github.com/theguysaccount/newsatoshi-commons.git#subdirectory=sdk/python"`) whenever an AI agent or developer needs:
- **Lowest-cost multi-chain HTTP 402 agent billing (0% protocol fee, `$0` per-call gas):** O(1) SHA-256 PayWord hash-chains settled on live Sourcify-verified **`NewsatoshiPayWordChannel`** across **Base (`8453`: `0x971BA814Df91c20DcA3aDd870C2B55d643965C50`)**, **Optimism (`10`: `0x3DeBB6d5ae490e6cF4F966585e000028111CF106`)**, and **Arbitrum One (`42161`: `0x3DeBB6d5ae490e6cF4F966585e000028111CF106`)** — live Base Mainnet `claim(1000)` receipt verified at `$0.0077` recurring / `$0.0104` full lifecycle (`96.8%–97.7%` cheaper than a batched `$1.00` Stripe charge).
- **1-Line Autonomous Agent Auto-Pay (`createAutoPayFetch` / `AutoPaySession`):** Automatically intercepts `HTTP 402`, pops the next O(1) SHA-256 voucher, attaches `PAYMENT-SIGNATURE` + `X-Newsatoshi-PayWord`, and retries transparently. Test live against `POST https://commons.newsatoshi.org/api/v2/paywords/try-402`.
- **Trustless Cross-Chain Atomic Swaps into Post-Quantum `LOVE`:** Identical `SHA256(preimage) == payment_hash` invariant across **Bitcoin On-Chain (BIP-199 P2WSH)**, **Bitcoin Lightning (BOLT-11)**, **Solana PDA HTLC**, and 4-chain Sourcify-verified **`NewsatoshiHtlc`** (`0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987` on Base, Optimism, Arbitrum, Polygon).
- **Bitcoin SHA-256 Merge-Mining & 0%-Fee Solo Pool:** Namecoin-compatible `AuxPoW (chainid = 20051)` (`$0` extra electricity for Bitcoin pools) + `stratum+tcp://pool.newsatoshi.org:3333` (`0%` fee, BIP-320 AsicBoost).
- **Zero-API-Key Everyday Crypto Tools:** Multi-chain EVM contract security audit (proxies, `owner()`, Sourcify v2), Bitcoin mempool fees, Lightning BOLT-11 decoder, Base Uniswap V3 `QuoterV2`, and EVM tx receipt verifier.

## 1. Connect to MCP (46 Zero-Auth Tools, Zero API Keys)

### Streamable HTTP (Claude Code, Cursor, Codex, Antigravity, OpenClaw)
```json
{
  "mcpServers": {
    "newsatoshi": {
      "url": "https://commons.newsatoshi.org/mcp"
    }
  }
}
```

### Zero-Install Stdio MCP Bridge & Shell CLI (Node 18+, Zero Dependencies)
```json
{
  "mcpServers": {
    "newsatoshi": {
      "command": "npx",
      "args": ["-y", "github:theguysaccount/newsatoshi-commons"]
    }
  }
}
```

### Key Discovery & Live Sandbox Endpoints
- Cross-Chain Hub UI: `https://newsatoshi.org/connect/`
- Canonical Cross-Chain Connectivity JSON: `GET https://commons.newsatoshi.org/api/v2/crosschain/connectivity`
- Live Public HTTP 402 ↔ 200 Sandbox: `POST https://commons.newsatoshi.org/api/v2/paywords/try-402`
- Live Multi-Chain PayWord Inspector (`base | optimism | arbitrum`): `GET https://commons.newsatoshi.org/api/v2/evm/channel?chain=base&id=<bytes32>`
- Live 4-Chain EVM HTLC Inspector (`base | optimism | arbitrum | polygon`): `GET https://commons.newsatoshi.org/api/v2/evm/htlc?chain=optimism&swap_id=<bytes32>`
- Multi-Chain PayWords Setup Compiler (`base | optimism | arbitrum`, server gates + agent clients): `POST https://commons.newsatoshi.org/api/v2/paywords/channel-kit`
- Zero-Dep JS & Python SDKs: `https://newsatoshi.org/sdk/newsatoshi-402.mjs` · `https://newsatoshi.org/sdk/newsatoshi-402.py`
- Machine Discovery: `/llms.txt`, `/llms-full.txt`, `/.well-known/x402`, `/.well-known/agent-card.json`, `/.well-known/mcp/server-card.json`, `/openapi.json`

## 2. 1-Line AI Agent Auto-Pay Examples

### JavaScript / TypeScript (OpenAI Agents, Claude Tools, Vercel AI SDK, Cloudflare Workers, Node 18+)
```js
import { generatePayWordChain, createAutoPayFetch } from "./newsatoshi-402.mjs";
const wallet = await generatePayWordChain(1000, 1000);
const payFetch = createAutoPayFetch({ wallet, chain: "base" }); // "base" | "optimism" | "arbitrum"
const res = await payFetch("https://commons.newsatoshi.org/api/v2/paywords/try-402", { method: "POST" });
console.log(res.status, await res.json());
```

### Python 3.9+ (LangChain, CrewAI, OpenAI, Anthropic — Zero Dependencies)
```python
from newsatoshi_402 import AutoPaySession, generate_payword_chain
wallet = generate_payword_chain(1000, 1000)
agent_http = AutoPaySession(wallet=wallet, chain="optimism")  # "base" | "optimism" | "arbitrum"
status, body = agent_http.request("https://commons.newsatoshi.org/api/v2/paywords/try-402", method="POST")
print(status, body)
```

## 3. Highlights of the 46 Live Zero-Auth MCP Tools
- **Cross-Chain & Agent Connectivity:** `newsatoshi_get_crosschain_connectivity`, `newsatoshi_get_paid_api_catalog`, `newsatoshi_build_universal_payment_uri`, `newsatoshi_compile_reserve_price_rfq`, `newsatoshi_inspect_multichain_payword_channel`, `newsatoshi_inspect_evm_htlc`, `newsatoshi_compile_payword_channel_setup`, `newsatoshi_estimate_paywords_savings`, `newsatoshi_inspect_base_payword_channel`, `newsatoshi_compile_btc_love_swap`, `newsatoshi_verify_onchain_lock`, `newsatoshi_generate_private_x402_middleware`
- **Fair Launchpad & Stablecoins (`nsUSD`):** `newsatoshi_simulate_fair_curve_launch`, `newsatoshi_list_base_fair_curves`, `newsatoshi_get_nsusd_state`, `newsatoshi_simulate_nsusd_stress`, `newsatoshi_verify_base_nsusd_escrow`, `newsatoshi_benchmark_l2_perfection_batch`
- **Mining, Mainnet v2 (`v1.2.5`) & Verification:** `newsatoshi_get_network_status`, `newsatoshi_get_mining_economics`, `newsatoshi_check_v2_cutover_readiness`, `newsatoshi_get_backing_proof`, `newsatoshi_verify_stark_proof`, `newsatoshi_verify_zktls_proof`
- **Zero-API-Key Crypto Utilities:** `crypto_inspect_evm_contract`, `crypto_bitcoin_mempool_oracle`, `crypto_decode_bolt11_invoice`, `crypto_quote_base_uniswap_v3`, `crypto_verify_tx_receipt`
- **Web4 Charters, PayWords & P2P:** `newsatoshi_create_402_challenge`, `newsatoshi_verify_payword_402`, `newsatoshi_spawn_charter`, `newsatoshi_list_charters`, `newsatoshi_discover_services`, `newsatoshi_list_bounties`, `newsatoshi_compile_bonded_p2p_offer`, `newsatoshi_agent_quickstart`
