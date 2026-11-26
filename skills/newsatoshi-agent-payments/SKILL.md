---
name: newsatoshi-agent-payments
description: Use when an AI agent needs zero-API-key EVM contract security audits, Bitcoin mempool fees, Lightning BOLT-11 decoding, live Base Uniswap V3 quotes, tx receipt verification, x402 v2 / zero-gas SHA-256 PayWord HTTP 402 micropayments, or trustless BTC/Lightning/Solana/Base swaps into post-quantum Newsatoshi (LOVE).
---

# Newsatoshi Commons & Zero-API-Key Crypto Agent Tools (`v0.13.0`)

## When to use this skill

Connect to `https://commons.newsatoshi.org/mcp` (or run the zero-dependency stdio bridge `npx -y github:theguysaccount/newsatoshi-commons`) whenever an agent needs:
- **Zero-API-key everyday crypto & security utilities:** audit any EVM contract (Ethereum, Base, Arbitrum, Optimism, Polygon) for EIP-1967 proxies, active `owner()` admin risk, ERC-20 supply, and Sourcify v2 source verification; check live Bitcoin mempool fees; decode Lightning BOLT-11 invoices; quote Base Uniswap V3 `QuoterV2` pools; or verify multi-chain EVM tx receipts.
- **Unfreezeable `<1ms`, `$0`-gas HTTP 402 agent billing:** pure-SHA-256 PayWord hash-chains (`SHA256^ticks(token) == tip`) running alongside standard `x402 v2` USDC on Base/Arbitrum/Polygon.
- **Post-quantum Newsatoshi (`LOVE`) L1 operations:** live Mainnet/Testnet state, `0%`-fee Bitaxe solo-mining (`stratum+tcp://pool.newsatoshi.org:3333`), trustless 4-chain SHA-256 HTLC atomic swaps, and Winterfell 0.13 STARK covenant verification.

## 1. Connect to MCP (Zero Auth, Zero API Keys)

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
Or run any crypto utility directly from bash:
```bash
npx -y github:theguysaccount/newsatoshi-commons inspect 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913 --chain base
npx -y github:theguysaccount/newsatoshi-commons btc-fees
npx -y github:theguysaccount/newsatoshi-commons quote-base ETH USDC 1.0
npx -y github:theguysaccount/newsatoshi-commons payword-demo
```

### Machine-Readable Discovery Surfaces
- `https://commons.newsatoshi.org/llms.txt` & `/llms-full.txt`
- `https://commons.newsatoshi.org/.well-known/x402` (`x402 v2` + PayWord resource catalog)
- `https://commons.newsatoshi.org/.well-known/agent-card.json` & `/.well-known/agent.json` (A2A Agent Card)
- `https://commons.newsatoshi.org/.well-known/mcp/server-card.json` & `/openapi.json`

## 2. All 19 Live MCP Tools

### Everyday Zero-API-Key Crypto, Security & DeFi Tools (5 Tools)
1. `crypto_inspect_evm_contract` — Live multi-chain EVM smart contract security audit (bytecode size, EIP-1967 proxy/admin slots, `owner()` admin risk, ERC-20 metadata/supply, Sourcify v2 source verification).
2. `crypto_bitcoin_mempool_oracle` — Live Bitcoin tip height, hash, sat/vB fee tiers (`fastest`, `30m`, `60m`, `economy`), and mempool backlog.
3. `crypto_decode_bolt11_invoice` — Pure Rust BOLT-11 Lightning invoice decoder (network, sats/msats, payment hash, expiry, description).
4. `crypto_quote_base_uniswap_v3` — Live on-chain Base Mainnet Uniswap V3 `QuoterV2` price quote across fee tiers (`ETH`, `WETH`, `USDC`, `wLOVE`, or any ERC-20).
5. `crypto_verify_tx_receipt` — Live EVM transaction receipt & confirmation verifier across Ethereum, Base, Arbitrum, Optimism, and Polygon.

### Newsatoshi Post-Quantum L1, Swaps, PayWords & Web4 Tools (14 Tools)
6. `newsatoshi_get_network_status` — Live Mainnet & Testnet height, difficulty, issued `LOVE`, locks, peers, AuxPoW `chainid = 20051`.
7. `newsatoshi_get_commons_state` — Live `NEWSATOSHI/2.1` Commons balance, `covenant_rule_hash`, public roots, STARK proofs, L1 anchors.
8. `newsatoshi_compile_btc_love_swap` — Compiles BIP-199 Bitcoin P2WSH HTLC (`bc1q…`), Lightning BOLT-11, Solana PDA escrow, or live 4-chain EVM `NewsatoshiHtlc.sol` (`0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`).
9. `newsatoshi_verify_onchain_lock` — Live dual-chain SHA-256 lock verification on Newsatoshi L1 + Bitcoin `mempool.space`.
10. `newsatoshi_verify_payword_402` — Sub-millisecond verification of pure-SHA-256 PayWord tokens (`SHA256^ticks(token) == tip`).
11. `newsatoshi_verify_stark_proof` — Cryptographically re-verifies Winterfell 0.13 transparent STARK covenant proofs.
12. `newsatoshi_agent_quickstart` — Structured onboarding commands for autonomous agents.
13. `newsatoshi_create_402_challenge` — Generates dual `NEWSATOSHI-402/1.0` + `x402 v2` challenges to monetize agent APIs.
14. `newsatoshi_get_mining_economics` — Live AuxPoW merge-mining yield & public `0%`-fee solo pool (`stratum+tcp://pool.newsatoshi.org:3333`) stats.
15. `newsatoshi_get_backing_proof` — Live on-chain `NewsatoshiBackingVault.sol` (`0x37Ed94525DBE408fA72cDd156409875666679265`) reserves across Base, Arbitrum, Optimism, and Polygon.
16. `newsatoshi_spawn_charter` — Compiles a deterministic Web4 Sovereign Business Charter with Lean 4 Theorem 8 monotonic child bounds.
17. `newsatoshi_list_charters` — Lists registered Web4 business charters and parent-child lineage.
18. `newsatoshi_discover_services` — Lists live HTTP 402 PayWord & `x402 v2` agent services.
19. `newsatoshi_list_bounties` — Lists zero-custody HTLC & Freivalds matrix compute bounties.

## 3. Four Live Dual-Rail `x402 v2` + Zero-Gas PayWord Endpoints

Every endpoint below accepts **either** standard `x402 v2` USDC (`$0.001` on Base `eip155:8453`, Arbitrum `eip155:42161`, or Polygon `eip155:137` via `Payment-Signature`) **or** zero-gas `<1ms` post-quantum `X-Newsatoshi-PayWord: tip=<64hex>,token=<64hex>,ticks=<k>`:
- `POST https://commons.newsatoshi.org/api/x402/evm-security-scan`
- `GET  https://commons.newsatoshi.org/api/x402/bridge-quote`
- `POST https://commons.newsatoshi.org/api/x402/compile-htlc`
- `POST https://commons.newsatoshi.org/api/x402/verify-payword`

## 4. Zero-Dependency SDKs (Installable Directly from GitHub Today)
- **JS / Node 18+ / Edge:** `npx -y github:theguysaccount/newsatoshi-commons` or `import { NewsatoshiMcpClient, generatePayWordChain, fetchWithNewsatoshi402 } from "@newsatoshi/402"`
- **Python 3.9+:** `pip install "git+https://github.com/theguysaccount/newsatoshi-commons.git#subdirectory=sdk/python"`
- **Mainnet Browser Wallet & 0%-Fee Bitaxe Pool:** `https://newsatoshi.org/wallet/` + `stratum+tcp://pool.newsatoshi.org:3333`
