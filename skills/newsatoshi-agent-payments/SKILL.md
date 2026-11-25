---
name: newsatoshi-agent-payments
description: Use when an AI agent needs to pay or get paid without KYC or credit cards, check live Newsatoshi (LOVE) Mainnet/Testnet status, swap BTC/Lightning/Solana/Base into LOVE via trustless SHA-256 HTLCs, verify HTTP 402 PayWord micropayments, or verify Winterfell STARK spending covenants.
---

# Newsatoshi Agent Payments (`LOVE` / `NEWSATOSHI/2.1`)

## When to use this skill

Reach for this skill and the `https://commons.newsatoshi.org/mcp` server whenever an agent needs to:
- **Check live Newsatoshi L1 state** (Mainnet/Testnet block heights, difficulty, AuxPoW `chainid = 20051`, issued supply, open locks).
- **Acquire or bridge `LOVE` trustlessly** from Bitcoin L1 (BIP-199 P2WSH), Lightning (BOLT-11), Solana (SOL/USDC PDA HTLC), or Base/Ethereum/Arbitrum (ETH/USDC `NewsatoshiHtlc.sol`) with zero exchanges and zero KYC.
- **Pay or verify per-call HTTP `402` API micropayments** using zero-gas, post-quantum pure-SHA-256 PayWord hash-chains (`SHA256^ticks(token) == tip`) or L1 hash-locks.
- **Verify physical telemetry, land deeds, or Winterfell STARK spending covenants** without trusting a custodial server (`verify_only: true`, 0 server signing keys).

## 1. Connect to the Public MCP Server (Zero Auth Required)

Add the Streamable HTTP MCP endpoint to your MCP host (`Claude Desktop`, `Claude Code`, `Codex`, `Cursor`, `Antigravity`, `Gemini CLI`, or custom MCP client):

```json
{
  "mcpServers": {
    "newsatoshi": {
      "url": "https://commons.newsatoshi.org/mcp"
    }
  }
}
```

Machine-readable discovery surfaces:
- Concise guide: `https://commons.newsatoshi.org/llms.txt` (or `GET https://commons.newsatoshi.org/` with `Accept: text/markdown`)
- Full recipes: `https://commons.newsatoshi.org/llms-full.txt`
- A2A Agent Card: `https://commons.newsatoshi.org/.well-known/agent.json`
- MCP manifest: `https://commons.newsatoshi.org/.well-known/mcp.json`
- OpenAPI 3.1 schema: `https://commons.newsatoshi.org/openapi.json`

### Live MCP Tools (9 Tools)
1. `newsatoshi_get_network_status` — Live Mainnet & Testnet block height, difficulty, issued `LOVE`, locks, peers, AuxPoW `chainid = 20051`.
2. `newsatoshi_get_commons_state` — Live `NEWSATOSHI/2.1` Commons balance, `covenant_rule_hash`, cold-ceremony public roots, STARK proofs, L1 anchors.
3. `newsatoshi_compile_btc_love_swap` — Compiles a canonical BIP-199 Bitcoin P2WSH HTLC (`bc1q…` / `tb1q…`), decodes Lightning BOLT-11, Solana PDA escrow, or live Base Mainnet `NewsatoshiHtlc.sol` (`0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`, Chain ID `8453`) into exact lock CLI commands.
4. `newsatoshi_verify_onchain_lock` — Verifies any SHA-256 hash-lock live on Newsatoshi L1 (`/api/lock?hash=`), Bitcoin P2WSH UTXOs (`mempool.space`), and preimage match.
5. `newsatoshi_verify_payword_402` — Verifies a pure-SHA-256 PayWord HTTP `402` micropayment token (`SHA256^ticks(token) == tip`) in microseconds.
6. `newsatoshi_verify_stark_proof` — Cryptographically re-verifies any Winterfell STARK proof recorded in the Commons ledger.
7. `newsatoshi_agent_quickstart` — Returns step-by-step wallet, faucet, swap, and HTTP 402 onboarding commands.
8. `newsatoshi_create_402_challenge` — Generates a standard `NEWSATOSHI-402/1.0` Payment Required challenge to monetize agent APIs in `LOVE`.
9. `newsatoshi_get_mining_economics` — Live AuxPoW merge-mining yield & network hashrate economics.

---

## 2. 60-Second Agent Wallet & Sandbox Setup (Testnet)

Create a **non-interactive shielded delivery address (`nsd1…`)** so peers can pay your agent privately on-chain without interactive note handoffs:

```bash
# Generate or print your agent's shielded delivery address
newsatoshi --net testnet address --delivery

# Request free testnet LOVE from the public faucet (instant sandbox funding)
curl -sS -X POST https://newsatoshi-seed.fly.dev/api/faucet \
  -H 'Content-Type: application/json' \
  -d '{"address":"<YOUR_TESTNET_NSD1_ADDRESS>"}'

# Scan chain for delivered notes & verify spendable balance
newsatoshi --net testnet scan && newsatoshi --net testnet balance

# Send a private agent-to-agent payment
newsatoshi --net testnet send <PEER_NSD1_ADDRESS> 0.5 --memo "task-42"
```

For **Mainnet**, switch `--net testnet` to `--net mainnet` (50 `LOVE`/block, AuxPoW `chainid = 20051`, live status at `https://newsatoshi.org/mainnet/status`).

---

## 3. Acquire Mainnet `LOVE` Without Exchanges (Trustless `BTC ↔ LOVE` Atomic Swaps)

1. Generate a local 32-byte CSPRNG preimage + SHA-256 hash (`newsatoshi-swap keygen`). Keep the preimage private until claiming.
2. Call `newsatoshi_compile_btc_love_swap` (or `POST https://commons.newsatoshi.org/api/swaps/compile`) for:
   - **Base Mainnet ETH / USDC HTLC (`rail: "evm_htlc"`, `evm_chain: "base"`)** -> defaults directly to the live, ownerless, Sourcify-verified Base Mainnet contract `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987` (`Chain ID 8453`, EVM `0x02` `sha256` precompile).
   - **Bitcoin On-Chain P2WSH HTLC (`rail: "btc_onchain"`)** -> returns canonical `bc1q…` Bech32 address + BIP-199 witness script + `newsatoshi --net mainnet lock` command + anti-race timelock check.
   - **Bitcoin Lightning BOLT-11 (`rail: "lightning_bolt11"`)** -> decodes the invoice payment hash & sats directly into the matching `newsatoshi --net mainnet lock` command.
   - **Solana SOL / USDC PDA Escrow (`rail: "solana_htlc"`)** -> compiles the deterministic 49-byte `sol_sha256` PDA lock instruction.
3. Verify both legs live via `newsatoshi_verify_onchain_lock` (or `POST https://commons.newsatoshi.org/api/swaps/verify`) before revealing the preimage.

---

## 4. Sub-Millisecond Per-Call Billing (HTTP `402` PayWord Hash-Chains)

Because post-quantum L1 transactions are ~249–460 KB, high-frequency agent API calls (LLM tokens, tool invocations, sensor ticks) settle off-chain via pure-SHA-256 **PayWord chains**:

1. Payer agent draws a 32-byte secret `seed`, computes `tip = SHA256^N(seed)`, and commits `tip`.
2. For API call `#k`, payer sends `token = SHA256^(N - k)(seed)` along with `(tip, token, ticks=k)`.
3. Resource server verifies `SHA256^k(token) == tip` locally or via `POST https://commons.newsatoshi.org/api/agent/402-verify` (`<1ms`, zero gas, post-quantum pure SHA-256).
4. Request a dual-rail HTTP `402` challenge anytime via `GET https://commons.newsatoshi.org/api/agent/402-challenge?resource=/api/inference&amount_love=0.01&network=mainnet`.

---

## 5. Verifiable L1 Compute Jobs & Unjailbreakable Spending Covenants

- **Buy or sell verifiable compute directly on Newsatoshi L1:**
  `newsatoshi --net mainnet job post A.csv B.csv --pay 5 --bond 1` (Freivalds matrix verification + automatic bond slashing for fraud).
- **Cryptographic agent spending guardrails:** Deploy a `--verify-only` Commons covenant (`CovenantRule`) with hard `max_bounty_payout`, `reinvest_bps`, and offline NIST FIPS 205 SLH-DSA hardware attestation so an agent cannot exceed its authorized budget even if prompt-injected.
