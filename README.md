# `newsatoshi-commons` — Multi-Chain `O(1)` SHA-256 PayWords, Atomic Swaps & Post-Quantum L2 Gateway (`v0.35.0`)

[![CI](https://github.com/theguysaccount/newsatoshi-commons/actions/workflows/ci.yml/badge.svg)](https://github.com/theguysaccount/newsatoshi-commons/actions/workflows/ci.yml)
[![Mainnet v2](https://img.shields.io/badge/Mainnet%20v2-LIVE%20%C2%B7%20v1.2.5%20%C2%B7%20BTC%20%23977%2C738-10b981)](https://newsatoshi.org)
[![MCP](https://img.shields.io/badge/MCP-46%20Zero--Auth%20Tools-38bdf8)](https://commons.newsatoshi.org/mcp)
[![PayWords](https://img.shields.io/badge/PayWords-Base%20%C2%B7%20Optimism%20%C2%B7%20Arbitrum%20(0%25%20Fee)-f59e0b)](https://newsatoshi.org/paywords/)
[![License: PolyForm Noncommercial 1.0.0](https://img.shields.io/badge/License-PolyForm%20Noncommercial%201.0.0-amber)](LICENSE)

**Zero-custody, zero-protocol-fee cross-chain payment & settlement infrastructure for autonomous AI agents, metered APIs, Bitcoin merge-miners, and post-quantum Newsatoshi (`LOVE`) Mainnet v2.**

- **Cross-Chain Connectivity Hub:** [newsatoshi.org/connect](https://newsatoshi.org/connect/) · **PayWords Studio:** [newsatoshi.org/paywords](https://newsatoshi.org/paywords/) · **Fair Launchpad:** [newsatoshi.org/launchpad](https://newsatoshi.org/launchpad/)
- **Live Verify-Only Gateway (0 Server Private Keys):** [commons.newsatoshi.org](https://commons.newsatoshi.org) · **Official MCP Registry:** `org.newsatoshi.commons/newsatoshi-commons` (`46` zero-auth tools)
- **Wire Protocol Spec (`x402#4494`):** [docs/X402-BATCH-SETTLEMENT-PAYWORDS-SPEC.md](docs/X402-BATCH-SETTLEMENT-PAYWORDS-SPEC.md)
- **External Security Audit Scope & Reproduction Brief:** [docs/EXTERNAL_AUDIT_BRIEF.md](docs/EXTERNAL_AUDIT_BRIEF.md)
- **Layer-1 Consensus Repo (`v1.2.5`):** [theguysaccount/newsatoshi-rfc0021](https://github.com/theguysaccount/newsatoshi-rfc0021) (Pure FIPS SHA-2/384 Winterfell STARKs + FIPS 205 SLH-DSA + `9,349`-line Lean 4.36.0 zero-sorry AIR proof)
- **License:** [PolyForm Noncommercial License 1.0.0](LICENSE) (free for noncommercial use, auditing & research; see [COMMERCIAL_LICENSE.md](COMMERCIAL_LICENSE.md) for commercial partner licensing)

---

## Live Mainnet Contracts (Ownerless · Non-Upgradeable · 0% Protocol Fee · Sourcify `exact_match`)

| Chain | CAIP-2 | `NewsatoshiPayWordChannel` (`0%` Fee) | SHA-256 `NewsatoshiHtlc` | Live On-Chain Proofs |
| :--- | :--- | :--- | :--- | :--- |
| **Base Mainnet** | `eip155:8453` | `0x971BA814Df91c20DcA3aDd870C2B55d643965C50` | `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987` | [`claim(1000)`: `$0.0077` (`770,956` gas)](https://basescan.org/tx/0xb79a3c01a8edc0bbc0b25714b2a1b7852c0035be2a083c26da81ef31620f6256) · [Live Paid API `claim(5)` (`75,608` gas)](https://basescan.org/tx/0xd45660a90844b817ec9a7232144882f983104930c4e5555187efffcd926bea06) → [`FairCurvePool.injectRevenue(5000)`](https://basescan.org/tx/0x1c1498db55467d4696569b20b764c552958780d94a823d200847cfdc71f9e3ff) |
| **Optimism Mainnet** | `eip155:10` | `0x3DeBB6d5ae490e6cF4F966585e000028111CF106` | `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987` | [`claim(100)`: `125,250` gas](https://optimistic.etherscan.io/tx/0x05f72b5f3bbff8407b78be11cbc962414a2af573bc9f08466cab060814b8ef6c) · [HTLC `lock` → `claim`](https://optimistic.etherscan.io/tx/0x88ca2f68dc6e1587ee157bf2f48f6ed5a5044923686a9aac7fda5a9b6be88f0b) |
| **Arbitrum One** | `eip155:42161` | `0x3DeBB6d5ae490e6cF4F966585e000028111CF106` | `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987` | [Deploy `0xeab6df…6fea`](https://arbiscan.io/tx/0xeab6dfec32c41e44cb784b0675a468e421725fcc56023f4af738226f5ad76fea) (identical deterministic address to Optimism) |
| **Polygon PoS** | `eip155:137` | *(BackingVault + HTLC live)* | `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987` | [Sourcify `exact_match`](https://repo.sourcify.dev/137/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) + `NewsatoshiBackingVault` (`0x37Ed…9265`) |
| **Base Fair Curve Factory** | `eip155:8453` | `0xF6bE782819a8eD0D1556A8f5DC3223f9eb13a933` | Live `NSPAY` Pool: `0xC3098ba25E39Effa63dE94EbACE6381781ad4cEd` | Lean 4 Theorems 10–11 (`0 sorry`, 0% premine, 0% platform cut) |

---

## 30-Second Quickstart

### 1. Connect 46 Zero-Auth MCP Tools (Claude, Cursor, Codex, Antigravity, OpenClaw)
```json
{
  "mcpServers": {
    "newsatoshi": {
      "url": "https://commons.newsatoshi.org/mcp"
    }
  }
}
```
Or run the zero-install stdio MCP bridge & multi-chain CLI directly:
```bash
npx -y github:theguysaccount/newsatoshi-commons help
npx -y github:theguysaccount/newsatoshi-commons inspect-channel 0x91fbe151d270ed3b4953c48a068321dfdcf1cc74704040267fdafea63be11153 --chain base
npx -y github:theguysaccount/newsatoshi-commons pay-uri 0x1744535521EB420be9700475400455a4Ad0EBEa2 10 --rail all
```

### 2. Install Zero-Dependency AI Agent Auto-Pay SDKs (JS/TypeScript + Python)
```bash
# JavaScript / TypeScript (includes full TypeScript newsatoshi-402.d.ts definitions)
npm install github:theguysaccount/newsatoshi-commons

# Python 3.9+ (pure stdlib — zero pip transitive dependencies)
pip install "git+https://github.com/theguysaccount/newsatoshi-commons.git#subdirectory=sdk/python"
```
```js
import { generatePayWordChain, createAutoPayFetch } from "newsatoshi-mcp";
const wallet = await generatePayWordChain(1000, 1000);
const payFetch = createAutoPayFetch({ wallet, chain: "base", channelId: "0x..." });
const res = await payFetch("https://commons.newsatoshi.org/api/v2/paywords/try-402", { method: "POST" });
```
- **Runnable Starters:** [`examples/cloudflare-worker-paywords`](examples/cloudflare-worker-paywords), [`examples/nextjs-vercel-ai-agent`](examples/nextjs-vercel-ai-agent), [`examples/fastapi-langchain-agent`](examples/fastapi-langchain-agent).
- **Live Paid Production Micro-APIs (`$0.001` USDC via Base/Optimism/Arbitrum PayWords, `1–2 µs` verify):**
  - `POST https://commons.newsatoshi.org/api/v2/paid/evm-audit-batch`
  - `POST https://commons.newsatoshi.org/api/v2/paid/tx-verify-batch`
  - `POST https://commons.newsatoshi.org/api/v2/paid/defi-quote-bundle`
  - Live telemetry: `GET https://commons.newsatoshi.org/api/v2/paid/status` · Discovery: `GET https://commons.newsatoshi.org/.well-known/x402`

### 3. Dual-Mine Bitcoin + Mainnet v2 `LOVE` (`AuxPoW chainid=20051`, `$0` Extra Electricity)
- **0%-Fee Solo Pool (Bitaxe / Antminer / WhatsMiner / MRR):** `stratum+tcp://pool.newsatoshi.org:3333` (`<74char_ns1_address>.rig1`, pass `x` or `d=65536`/`d=524288`)
- **1-Click Umbrel Community App Store:** `https://github.com/theguysaccount/newsatoshi-umbrel-store`
- **1-Command Docker / Start9 / Public Pool / CKPool Dual Merge-Mining Kits:** [`auxpow-stratum/`](auxpow-stratum/) (`docker-compose.merge-mine.yml`, `umbrel/`, `start9/`, `public-pool-bridge/`, `ckpool-sidecar.env.example`).

---

## Build & Verify All 81 Automated Tests Locally

```bash
cargo test && cargo clippy --all-targets -- -D warnings       # 52 Rust unit + integration tests
node --test sdk/js/test_newsatoshi_402.mjs                    # 9 JS SDK tests (incl. 1M-tick benchmark)
python3 -m unittest discover -s sdk/python -p "test_*.py"     # 9 Python SDK tests
python3 -m unittest discover -s auxpow-stratum -p "test_*.py" # 11 Stratum AuxPoW ASIC tests
```

> **Zero Fake Data Guarantee:** Fresh deployments start at genuine `0` counters and empty ledgers. Every chain height, pool worker, Fair Curve balance, PayWord channel, HTLC preimage, and STARK proof comes directly from live RPCs or deterministic cryptographic verification.
