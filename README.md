# `newsatoshi-commons` — Layer 2 Agent Commerce, `nsUSD` Private Dollar & Atomic Bridge Gateway (`v0.28.0`)

[![CI](https://github.com/theguysaccount/newsatoshi-commons/actions/workflows/ci.yml/badge.svg)](https://github.com/theguysaccount/newsatoshi-commons/actions/workflows/ci.yml)
[![Mainnet v2](https://img.shields.io/badge/Mainnet%20v2-LIVE%20%C2%B7%20BTC%20%23977%2C738-10b981)](https://commons.newsatoshi.org)
[![MCP](https://img.shields.io/badge/MCP-30%20Zero--Auth%20Tools-38bdf8)](https://commons.newsatoshi.org/mcp)
[![nsUSD](https://img.shields.io/badge/Stable%20Rail-nsUSD%20%2B%20USD%20x402%20PayWords-f59e0b)](https://commons.newsatoshi.org/nsusd)
[![Bug Bounty](https://img.shields.io/badge/Security%20Bounty-Paid%20in%20Mined%20LOVE-ef4444)](https://commons.newsatoshi.org/bounty)
[![License: MIT](https://img.shields.io/badge/License-MIT-zinc)](LICENSE)

**The open-source Layer-2 gateway for Newsatoshi (`LOVE`) Mainnet v2 — bringing private USD-denominated machine payments (`nsUSD` + `O(1)` PayWords), 30 zero-auth MCP tools, trustless 4-chain SHA-256 atomic swaps, and Bitcoin AuxPoW merge-mining to humans and autonomous AI agents.**

- **Live Verify-Only Gateway (0 Server Private Keys):** [commons.newsatoshi.org](https://commons.newsatoshi.org)
- **Satoshi-Format 8-Page Whitepaper PDF:** [commons.newsatoshi.org/whitepaper.pdf](https://commons.newsatoshi.org/whitepaper.pdf)
- **`nsUSD` Private Stablecoin Spec & Live USD Quote API:** [commons.newsatoshi.org/nsusd](https://commons.newsatoshi.org/nsusd) · `GET /api/v2/nsusd/quote?usd=0.01`
- **Public Security Bug Bounty (Fair-Mined `LOVE`):** [commons.newsatoshi.org/bounty](https://commons.newsatoshi.org/bounty)
- **Layer-1 Consensus Repo (`v1.2.1`):** [theguysaccount/newsatoshi-rfc0021](https://github.com/theguysaccount/newsatoshi-rfc0021)

---

## The Hourglass Architecture: Minimal L1 Waist · Infinite L2 Velocity

| Layer | Repository | What Lives Here |
| :--- | :--- | :--- |
| **Layer 2 · Agent Commerce, `nsUSD` & Atomic Bridges** *(This Repo)* | [`newsatoshi-commons`](https://github.com/theguysaccount/newsatoshi-commons) (`v0.28.0`) | **30 zero-auth MCP tools**, USD-denominated `x402` + `O(1)` SHA-256 `PayWords` streaming (`<1 ms`, zero gas), `nsUSD` private dollar settlement rails (`RFC-0028`), trustless Bitcoin / Lightning / Solana / 4-chain EVM SHA-256 atomic swaps into **native Mainnet v2 `LOVE`**, `ERC-8004` agent identity, Web4 charters, and 0%-fee AuxPoW Stratum pool kits. |
| **Layer 1 · Sovereign Base Money** *(The Narrow Waist)* | [`newsatoshi-rfc0021`](https://github.com/theguysaccount/newsatoshi-rfc0021) (`v1.2.1`) | **Live Mainnet v2 (`nsM2`, anchored to Bitcoin Block `#977,738`, `0 LOVE` premine).** Mandatory shielded privacy via pure FIPS SHA-2/SHA-384 Quartic Goldilocks STARKs (`192`-bit classical / `100–104`-bit conservative QROM), **249/249 spend constraints mechanized in Lean 4 (`0 sorry`)**, Bitcoin AuxPoW merge-mining (`chainid = 20051`), `BatchSpend` `6.5×` compression, `NSPQ2` SLH-DSA-pinned ML-KEM-768 P2P, FlyClient MMR, 50% fee burn, and `PriorLock` collateral. |

> **One Real Coin:** Native Mainnet v2 Layer-1 `LOVE` (mined via Bitcoin AuxPoW on `nsM2`) is the sole canonical base currency of Newsatoshi. Everything on this gateway routes directly to native L1 `LOVE`, Bitcoin/Lightning/EVM atomic HTLCs, and USD-denominated `nsUSD` settlement.

---

## 30-Second Quickstart for AI Agents & Developers

### 1. Connect 30 Zero-Auth MCP Tools (Claude, Cursor, Cline, Codex, OpenClaw)
```json
{
  "mcpServers": {
    "newsatoshi": {
      "url": "https://commons.newsatoshi.org/mcp"
    }
  }
}
```
Or run locally via zero-install stdio MCP & CLI:
```bash
npx -y github:theguysaccount/newsatoshi-commons
```

### 2. Price Any API in Stable USD (`nsUSD` + `O(1)` PayWords, Zero Volatility)
```bash
# Get a live USD-denominated JIT PayWords + multi-chain atomic settlement quote
curl -sS "https://commons.newsatoshi.org/api/v2/nsusd/quote?usd=0.005" | jq .
```
- **O(1) SHA-256 PayWord Verification (Node.js v24 median):** `0.013 ms` per call (1 SHA-256 hash) up to `1,000,000` ticks per channel — zero per-call gas, zero credit card fees, zero custody freeze risk.
- **Zero-Dependency SDKs:** `sdk/js/newsatoshi-402.mjs` (Node / Workers / Edge) and `sdk/python/newsatoshi_402.py` (FastAPI / Flask / stdlib).

### 3. Mine Real Native Mainnet v2 `LOVE` (0% Pool Fee)
```text
Stratum URL: stratum+tcp://pool.newsatoshi.org:3333
Worker:      <YOUR_MAINNET_V2_ns1_ADDRESS>.rig1
Password:    x
```

### 4. Trustless Multi-Chain Atomic Buy Roads into Native L1 `LOVE`
| Source Rail | Standard | Shared Cryptographic Invariant | Live Verification |
| :--- | :--- | :--- | :--- |
| **Bitcoin L1 (`BTC`)** | BIP-199 SegWit v0 P2WSH (`bc1q…`) | `SHA256(preimage) == payment_hash` | `src/swap.rs`, `newsatoshi-swap` |
| **Lightning (`BTC`)** | BOLT-11 Invoice → L1 Hash-Lock | `SHA256(preimage) == payment_hash` | `src/swap.rs`, `bridges/lightning/` |
| **Solana (`SOL`/`USDC`)** | Deterministic PDA Escrow (`ns_htlc`) | `SHA256(preimage) == payment_hash` | `bridges/solana/src/lib.rs` |
| **Base / Arb / OP / Polygon** | Ownerless `NewsatoshiHtlc.sol` (`0x02`) | `SHA256(preimage) == payment_hash` | Live on 4 Mainnets: `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987` |

---

## Live Infrastructure & Canonical Contracts

- **Mainnet v2 (`nsM2`) Live Seed:** `https://newsatoshi-seed-v2.fly.dev/status` (`genesis: 00000000ba1cdad4aa8e82d6613b4df536394c41008fc3a0914a47a44604b9c8`)
- **Zero-Fee AuxPoW Stratum Pool:** `stratum+tcp://pool.newsatoshi.org:3333` (`https://pool.newsatoshi.org/status`)
- **Ownerless 4-Chain EVM HTLC (`Base · Arb · OP · Polygon`, 4/4 Sourcify Exact Match):** `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`
- **Ownerless 4-Chain Backing Vault (`NewsatoshiBackingVault.sol`, 0 withdrawal path):** `0x37Ed94525DBE408fA72cDd156409875666679265`
- **ERC-8004 On-Chain Agent Identity:** `0x8004A169FB4a3325136EB29fA0ceB6D2e539a432` ([`/.well-known/erc8004.json`](https://commons.newsatoshi.org/.well-known/erc8004.json))
- **Agent Discovery:** `POST /mcp` · `/.well-known/x402` · `/.well-known/agent-card.json` · `/llms.txt` · `/openapi.json`

---

## Build, Test & Run Locally

```bash
# Install verified SHA-256 release binaries (macOS & Linux)
curl -fsSL https://commons.newsatoshi.org/install.sh | sh

# Or build & run all 43 unit + integration tests from source
(cd web && npm ci && npm run build)
cargo clippy --all-targets -- -D warnings && cargo test

# Start the live verify-only Commons gateway
cargo run --bin newsatoshi-commons -- serve \
  --port 8791 \
  --verify-only \
  --seed-url https://newsatoshi-seed-v2.fly.dev/status \
  --static-dir ./web/dist
```

> **Zero Fake Data Guarantee:** Fresh deployments start at genuine `0` counters and empty ledgers. Every chain height, vault balance, STARK proof, and swap receipt rendered on `commons.newsatoshi.org` comes directly from live Mainnet v2, EVM RPCs, or cryptographic verification.
