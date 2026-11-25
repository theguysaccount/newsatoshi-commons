# Newsatoshi (`LOVE`) Launch, Bitaxe Miner & AI Agent Distribution Kit (`v0.10.0`)

> **Zero Fake Data Guarantee:** Every contract address, transaction hash, MCP Registry ID, release binary, and network figure below is live and independently verifiable on-chain or via public HTTP endpoints.

---

## 1. Live Verifiable Proof Links (Copy Anywhere)

- **Live Verify-Only Gateway & Cockpit:** `https://commons.newsatoshi.org` (`verify_only: true`, `0` server signing keys)
- **Official MCP Registry Listing (`status: active`):** `org.newsatoshi.commons/newsatoshi-commons` (`https://registry.modelcontextprotocol.io/v0/servers?search=newsatoshi`)
- **Streamable HTTP MCP Endpoint (Zero Auth, 9 Tools):** `https://commons.newsatoshi.org/mcp`
- **1-Line CLI Install (macOS/Linux, SHA-256 verified):** `curl -fsSL https://commons.newsatoshi.org/install.sh | sh`
- **GitHub Release & 60fps 3D Launch Films (`v0.10.0`):** `https://github.com/theguysaccount/newsatoshi-commons/releases/tag/v0.10.0`
- **Universal Ownerless EVM SHA-256 HTLC (`0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`, Sourcify `exact_match` across all 4 chains):**
  - **Base (`8453`):** [Contract](https://basescan.org/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) · [Atomic Claim Proof TX](https://basescan.org/tx/0x0f6cbfd277610b112d79fe36138dd549931f45f4ffa2894f7ea9e5831a15f595)
  - **Arbitrum One (`42161`):** [Contract](https://arbiscan.io/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) · [Atomic Claim Proof TX](https://arbiscan.io/tx/0xd11efcb1750aae5d5d443f4d446b84b609f6d253ca5152ebf1ea279a13de4d6e)
  - **Optimism (`10`):** [Contract](https://optimistic.etherscan.io/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) · [Atomic Claim Proof TX](https://optimistic.etherscan.io/tx/0xdb4ee991b86febd16a60c6a8a29a6ed4ac2d0dd92aebbc6a835f1d9c195881b8)
  - **Polygon PoS (`137`):** [Contract](https://polygonscan.com/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) · [Atomic Claim Proof TX](https://polygonscan.com/tx/0x868f347586d34f4c09bb6aae2b6b9c0f0ec657ab5f2dbfa87b22197aa8f94fb4)

---

## 2. Bitaxe / Home Bitcoin Miner Post (`r/Bitaxe`, `r/BitcoinMining`, Nostr `#bitaxe`, Solo Mining Discords)

**Title:** Point a single Bitaxe Gamma (1.2 TH/s) at Newsatoshi Mainnet (`auxpow_chainid = 20051`) — zero extra electricity, 50 LOVE/block, 1-command Docker Stratum

**Body:**
> If you run a Bitaxe / NerdQAxe / Antminer at home, Newsatoshi (`LOVE`) is a fair-launch, zero-premine, post-quantum Layer-1 (`21M` cap, `50 LOVE/block`, `aserti3-2d` ASERT difficulty) that supports Namecoin-compatible Bitcoin AuxPoW merged mining (`chainid = 20051`, coinbase magic `fabe6d6d`).
>
> **Current live Mainnet state (`https://newsatoshi.org/mainnet/status`):**
> - Height: `#4,214` (`210,700 LOVE` issued — ~1.0% of 21M supply)
> - Seed nodes intentionally do not mine Mainnet (`LAUNCH.md` fair-launch rule). Because no active ASIC is currently pointed at `#4,215`, ASERT (`48h` half-life) has decayed effective difficulty from `220.16` down toward `~128`.
> - A single **1.2 TH/s Bitaxe Gamma** can solve block `#4,215` in under a second and earn `50 LOVE/block` (`7,200 LOVE/day` global steady-state emission) while still hashing Bitcoin on the exact same SHA-256 shares with zero extra power draw.
>
> **1-command Docker Stratum v1 bridge (`auxpow-stratum/`, MIT-licensed, BIP-320 version rolling supported):**
> ```bash
> git clone https://github.com/theguysaccount/newsatoshi-commons.git
> cd newsatoshi-commons/auxpow-stratum
> export NS_RPC_AUTH="auxpow:$(openssl rand -hex 16)"
> docker compose up -d
> # Point your Bitaxe at stratum+tcp://<YOUR_HOST>:3333 (live status at :3334/status)
> # Pass --btc-rpc-url to merge-mine Bitcoin and Newsatoshi simultaneously
> ```
>
> Live yield calculator (computed strictly from live Mainnet difficulty, zero fake USD estimates): https://commons.newsatoshi.org (Swaps & Mining tab)
> Open-source repo & AuxPoW guide: https://github.com/theguysaccount/newsatoshi-commons

---

## 3. X / Twitter & Nostr Launch Thread (Attach `newsatoshi_launch_16x9_social.mp4` or `newsatoshi_launch_9x16_social.mp4`)

**Post 1 (attach 58-second 60fps launch film):**
> Bitcoin solved storing money. Not spending it.
>
> The moment you spend money into a charity, solar grid, land trust, or AI agent, humans take custody — and the rules break.
>
> Newsatoshi (`LOVE`) fixes that with math: money that keeps its promise forever.
>
> Live today (`v0.10.0`) 👇

**Post 2 (Live On-Chain Proofs — Zero Custodians, Zero Wrapped Tokens):**
> One immutable, ownerless SHA-256 HTLC contract deployed to the exact same address across 4 production mainnets — all Sourcify `exact_match`-verified, all proven with live atomic `lockEth` → `claim` transactions:
>
> `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`
>
> • Base (8453): https://basescan.org/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987
> • Arbitrum (42161): https://arbiscan.io/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987
> • Optimism (10): https://optimistic.etherscan.io/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987
> • Polygon (137): https://polygonscan.com/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987

**Post 3 (For AI Agents & Developers):**
> Autonomous AI agents can connect with ZERO API keys or KYC in 5 seconds:
>
> • Official MCP Registry (`active`): `org.newsatoshi.commons/newsatoshi-commons`
> • Streamable HTTP MCP (9 tools): `https://commons.newsatoshi.org/mcp`
> • Zero-dep HTTP 402 PayWord SDKs (`<1ms`, pure SHA-256, 0 gas/call) for JS & Python
> • 1-line CLI: `curl -fsSL https://commons.newsatoshi.org/install.sh | sh`
>
> Full MIT source: https://github.com/theguysaccount/newsatoshi-commons

---

## 4. Show HN / `r/mcp` / `r/LocalLLaMA` Technical Post

**Title:** Show HN: Newsatoshi Commons – Post-quantum MCP server, HTTP 402 PayWord micropayments & 4-chain SHA-256 HTLC bridges (Rust, MIT)

**Body:**
> We built and deployed **Newsatoshi Commons** (`https://commons.newsatoshi.org`), an open-source, MIT-licensed Rust gateway (`verify_only: true`, zero server private keys) for Newsatoshi (`LOVE`) — a post-quantum, fair-launch Layer-1 using NIST FIPS 205 SLH-DSA, Winterfell 0.13 transparent STARKs, and Bitcoin AuxPoW merge-mining (`chainid = 20051`).
>
> **What is live today (`v0.10.0`):**
> 1. **Official MCP Registry (`org.newsatoshi.commons/newsatoshi-commons`):** Zero-auth Streamable HTTP MCP server (`https://commons.newsatoshi.org/mcp`) exposing 9 tools for live L1 status, AuxPoW mining economics, multi-chain HTLC swap compilation, dual-chain lock verification, HTTP 402 PayWord verification, and STARK proof re-verification.
> 2. **Sub-millisecond HTTP 402 PayWord micropayments:** Pure-SHA-256 hash chains (`SHA256^k(token) == tip`) for per-call AI agent API billing with zero gas per call and zero external dependencies (`sdk/js/newsatoshi-402.mjs` & `sdk/python/newsatoshi_402.py`).
> 3. **Trustless multi-chain SHA-256 HTLC buy roads:** Canonical BIP-199 Bitcoin P2WSH (`bc1q…`), Lightning BOLT-11 decoding, Solana PDA escrow codec, and an immutable, ownerless Solidity HTLC (`NewsatoshiHtlc.sol`, EVM `0x02` `sha256` precompile) deployed identically at `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987` across **Base, Arbitrum One, Optimism, and Polygon** (all 4 Sourcify `exact_match` + live atomic claims proven).
> 4. **1-command AuxPoW Stratum v1 kit (`auxpow-stratum/`):** Lets Bitaxe / Antminer SHA-256 miners merge-mine Newsatoshi alongside Bitcoin with zero extra power.
>
> Try it directly (`POST https://commons.newsatoshi.org/mcp` or `GET https://commons.newsatoshi.org/llms.txt`) or inspect the code: https://github.com/theguysaccount/newsatoshi-commons
