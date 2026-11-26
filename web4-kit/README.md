# Newsatoshi Web4 Sovereign Business Starter Kit (`web4-kit/`)

Spawn an autonomous, covenant-protected Web4 business that earns per-call via **zero-gas SHA-256 PayWords + `x402`**, publishes an **`ERC-8004` + Post-Quantum Dual Identity card**, and automatically routes `backing_bps` of verified revenue into our live ownerless **`NewsatoshiBackingVault.sol`** (`0x37Ed94525DBE408fA72cDd156409875666679265` on Base, Arbitrum, Optimism & Polygon).

## 1. Quickstart (30 Seconds, Zero Dependencies, Node 18+)

```bash
# Dry-run heartbeat verification against live https://commons.newsatoshi.org/mcp
node web4-kit/agent-business.mjs --check

# Launch your local dual-rail HTTP 402 PayWord + x402 paid business daemon
CHARTER_ID=charter-my-agent \
BACKING_BPS=2000 \
MAX_SPEND_LOVE=50 \
LOVE_RECIPIENT=<YOUR_NSD1_ADDRESS> \
EVM_TREASURY=<YOUR_0x_ADDRESS> \
node web4-kit/agent-business.mjs
```

## 2. Plug Existing `Conway-Research/automaton` Agents Into Newsatoshi in 1 Config Block

Existing Conway Automaton / `x402` / `ERC-8004` agents can immediately discover Newsatoshi services, check live multi-chain Proof-of-Backing reserves, spawn monotonic child charters, and bridge Base/Arbitrum/Optimism/Polygon ETH/USDC into post-quantum `LOVE` with zero API keys:

```json
{
  "mcpServers": {
    "newsatoshi": {
      "url": "https://commons.newsatoshi.org/mcp"
    }
  }
}
```

### Why Pair Conway Automaton With Newsatoshi (`NEWSATOSHI/2.1`)?
1. **Fixes Prompt-Injection Wallet Drain:** Conway's `constitution.md` is prompt text. Newsatoshi's Lean 4.36-verified STARK covenants (`formal/NewsatoshiCovenants.lean`, Theorem 8 `web4_monotonic_child_bound`) mathematically cap `max_spend_per_call_love` and prevent child agents from escalating permissions across any spawn depth.
2. **Zero-Gas Sub-Cent Micropayments:** Instead of paying Base gas/facilitator overhead on every tiny API call, stream up to `1,000,000` SHA-256 PayWord calls per settlement (`< 1ms` verify, `0` gas/call).
3. **Verifiable Hard Reserve Backing:** Route `backing_bps` (e.g. `2000` = `20%`) directly into the ownerless, non-withdrawable `NewsatoshiBackingVault.sol` (`0x37Ed94525DBE408fA72cDd156409875666679265`, Sourcify `exact_match` across Base, Arbitrum, Optimism & Polygon).
