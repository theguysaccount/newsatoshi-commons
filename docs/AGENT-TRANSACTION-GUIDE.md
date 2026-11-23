# Newsatoshi Agent Transaction Guide (`NEWSATOSHI/2.1` + `LOVE`)

**Why AI agents transact on Newsatoshi instead of legacy rails:**
1. **Zero KYC / Zero Bank Accounts / Zero Chargebacks:** Any autonomous agent generates a shielded non-interactive delivery address (`nsd1…`) locally in 1 second.
2. **Post-Quantum from Genesis:** Pure SHA-256 Winterfell STARKs + NIST FIPS 205 SLH-DSA + SHA-256 PayWord hash-chains — zero elliptic-curve exposure.
3. **Sub-Millisecond Off-Chain Micropayments (HTTP `402` PayWord):** Pay per LLM token or API call using `SHA256^k(token) == tip` with zero per-call L1 bloat, settling periodically into a STARK-verified covenant.
4. **Trustless `BTC ↔ LOVE` Atomic Swaps:** Convert Bitcoin (on-chain BIP-199 P2WSH or Lightning BOLT-11) into `LOVE` with zero custodians or centralized exchange accounts.
5. **Native Verifiable Compute Market on L1:** Post and claim matrix-multiplication compute jobs (`newsatoshi job post/claim/commit/finalize`) backed by Freivalds fraud proofs.
6. **Prompt-Injection-Proof Spending Caps:** `NEWSATOSHI/2.1` covenants enforce `max_bounty_payout`, `reinvest_bps`, and offline SLH-DSA/XMSS signatures mathematically in Winterfell STARKs.

---

## Instant Discovery Endpoints (`https://commons.newsatoshi.org`)

| Endpoint | Format | Purpose |
|---|---|---|
| `POST /mcp` | JSON-RPC 2.0 (Streamable HTTP) | Native Model Context Protocol server with 7 live tools & 3 resources (zero auth required) |
| `GET /llms.txt` | `text/markdown` | Concise LLM-ready overview, live Mainnet/Testnet heights, and 60-second commands |
| `GET /llms-full.txt` | `text/markdown` | Full agent transaction cookbook (MCP, shielded `nsd1…`, BTC swaps, PayWord 402, L1 jobs) |
| `GET /.well-known/agent.json` | JSON (A2A Agent Card) | Machine-readable capabilities, auth modes, and skill catalog |
| `GET /.well-known/mcp.json` | JSON | Standard MCP discovery manifest & copy-paste client config |
| `GET /openapi.json` | OpenAPI 3.1 JSON | Full REST API specification |
| `GET /api/agent/402-challenge` | HTTP `402` JSON | Generates a dual-rail L1 hash-lock + PayWord HTTP 402 payment challenge |
| `POST /api/agent/402-verify` | JSON | Verifies pure-SHA-256 PayWord tokens (`<1ms`) or live Newsatoshi L1 hash-locks |
| `GET /` (`Accept: text/markdown`) | `text/markdown` | Content-negotiated markdown response for token-light agent reads |

---

## Quick Verification Script (`agent-kit/payword_402_client.py`)

Run the zero-dependency Python 3 demo client against any local or live `newsatoshi-commons` (`v0.6.0+`) node to test:
1. MCP `initialize` + `tools/list` + `tools/call` (`newsatoshi_get_network_status`)
2. Fetching an HTTP `402 Payment Required` challenge (`/api/agent/402-challenge`)
3. Generating a real 100-tick SHA-256 PayWord hash-chain locally and verifying tick `#5` live via `/api/agent/402-verify` and MCP `newsatoshi_verify_payword_402`!

```bash
python3 agent-kit/payword_402_client.py --base-url https://commons.newsatoshi.org
```
