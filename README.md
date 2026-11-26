# `newsatoshi-commons` — Sovereign Money Made Alive (`LOVE` · Autonomous Abundance Engines)

[![CI](https://github.com/theguysaccount/newsatoshi-commons/actions/workflows/ci.yml/badge.svg)](https://github.com/theguysaccount/newsatoshi-commons/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/theguysaccount/newsatoshi-commons)](https://github.com/theguysaccount/newsatoshi-commons/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-emerald.svg)](LICENSE)
[![Live Gateway](https://img.shields.io/badge/Live-commons.newsatoshi.org-10b981)](https://commons.newsatoshi.org)
[![MCP](https://img.shields.io/badge/MCP-19%20Tools%20(HTTP%20%2B%20npx%20stdio)-38bdf8)](https://commons.newsatoshi.org/mcp)
[![Newsatoshi Commons MCP connector](https://glama.ai/mcp/connectors/org.newsatoshi.commons/newsatoshi-commons/badges/score.svg)](https://glama.ai/mcp/connectors/org.newsatoshi.commons/newsatoshi-commons)
[![ERC-8004](https://img.shields.io/badge/ERC--8004-Web4%20Sovereign%20Identity-a855f7)](https://commons.newsatoshi.org/.well-known/erc8004.json)
[![x402 v2](https://img.shields.io/badge/x402%20v2-USDC%20%2B%20Zero--Gas%20PayWord-f59e0b)](https://commons.newsatoshi.org/.well-known/x402)

### What We Actually Built (Beyond Passive Sound Money)
Bitcoin freed money from banks — and left it sitting idle in a vault. The second you spend Bitcoin into a charity, energy project, land trust, or company, humans take custody and can drift, embezzle, or capture the treasury.

**Newsatoshi (`LOVE` / `NEWSATOSHI/2.1`) makes sovereign money alive** — turning passive hard money into **incorruptible, self-compounding real-world capital**:

1. **Money With a Mathematical Conscience (`OP_VERIFY_COVENANT_STARK`):** Lock capital to a real-world mission (land, renewable power, compute) and **every spend and every change output is mathematically forced to obey the exact same rule forever** — verified in `< 10ms` on a 5-dollar off-grid microchip. No CEO or trustee can strip the covenant.
2. **Closed-Loop Real World ↔ Machines ↔ Capital (Autonomous Abundance Engines):** Tamper-resistant hardware meters sign verified kWh/soil/compute output (`SLH-DSA`/`XMSS`), AI agents and users pay per call in under a millisecond (`HTTP 402` PayWords, up to 1M calls/lock, zero gas), and a hash-bound Legal Airlock (Perpetual Purpose Trust §4.2 + Wyoming DUNA) auto-compounds revenue into acquiring the next physical asset.
3. **Blind, Bitcoin-Format AuxPoW Base Money (Zero Extra Electricity):** Unlike failed “Proof of Useful Work” coins that collapse to wash-farming, L1 issuance stays 100% blind and ossified (Bitcoin-like halvings to `20,917,968.75 LOVE` by halving 8, then perpetual `0.1953125 LOVE/blk` tail emission `<0.05%/yr`, no hard cap) while targeting Bitcoin’s SHA-256 merge-mining format (`AuxPoW chainid = 20051`).
4. **Centuries-Grade Privacy & Vaultless Multi-Chain Gravity:** Pure hash math (`SHA-256`, `SLH-DSA`, transparent STARKs — zero elliptic curves, zero trusted setup), default cash privacy (`nsd1…`), and zero-custody SHA-256 atomic buy roads from Bitcoin, Lightning, Solana, and Base.
5. **Honest Status Disclosure:** Mainnet `v0.1.0` is an early pre-audit chain (4,214 blocks, low difficulty, 0 shielded spends, not yet secured by real Bitcoin mining pools); `wLOVE` is an independent Base ERC-20 token with no on-chain L1↔EVM bridge; the live x402 endpoints currently price/settle in Base/Arbitrum/Polygon USDC alongside the experimental PayWord sandbox; and the Lean 4.36.0 kernel verifies the covenant arithmetic/conservation model (while the L1 spend AIR is separately machine-checked by `extract_constraints` + Z3 across 249 constraints).

---

**Technical Stack:** Complete open-source, zero-custody infrastructure for **Newsatoshi (`LOVE` / `NEWSATOSHI/2.1`)** — NIST FIPS 205 SLH-DSA + XMSS covenants, **Winterfell 0.13** transparent STARK proofs, immutable 4-chain EVM **HTLC & Revenue Vault** (`NewsatoshiBackingVault.sol`), 4 trustless multi-chain SHA-256 HTLC buy roads, Web4 Business Charter Forge + ERC-8004 Dual Identity Bridge, 1-command AuxPoW Stratum merge-mining, Streamable HTTP MCP (**19 live tools**), zero-dependency JS & Python dual `PayWord + x402` HTTP `402` SDKs, runnable `web4-kit/` sovereign agent business daemon, and a Live Verify-Only Telemetry Mirror & Cockpit.

- **Live Verify-Only Gateway:** `https://commons.newsatoshi.org` (`0` server private keys, security-audited `v0.11.0`)
- **Whitepaper & Lean 4.36.0 Formal Proof (9 Theorems, 355 Lines, 0 `sorry`, 0 Mathlib):** [`https://commons.newsatoshi.org/whitepaper`](https://commons.newsatoshi.org/whitepaper) · [`formal/NewsatoshiCovenants.lean`](formal/NewsatoshiCovenants.lean) · [`docs/newsatoshi_2_1_whitepaper.tex`](docs/newsatoshi_2_1_whitepaper.tex)
- **Live 4-Chain Sovereign Backing Vault (`NewsatoshiBackingVault.sol`, ownerless, 0 withdrawal path):** `0x37Ed94525DBE408fA72cDd156409875666679265` ([BaseScan](https://basescan.org/address/0x37Ed94525DBE408fA72cDd156409875666679265) · [Arbiscan](https://arbiscan.io/address/0x37Ed94525DBE408fA72cDd156409875666679265) · [OP Etherscan](https://optimistic.etherscan.io/address/0x37Ed94525DBE408fA72cDd156409875666679265) · [Polygonscan](https://polygonscan.com/address/0x37Ed94525DBE408fA72cDd156409875666679265) — live genesis Charter #1 + `0.001 ETH` backing deposit proven on Base: [`0xc354e0…bf5c`](https://basescan.org/tx/0xc354e0cca5bd2f0e9ac22193ef6f4ea505393ab4be4177251f9f47ca1c10bf5c) · live API: `/api/backing/summary`)
- **Live Multi-Chain EVM HTLC (`Base 8453 · Arbitrum 42161 · Optimism 10 · Polygon 137`, ownerless, all 4 Sourcify `exact_match`):** `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987` ([BaseScan](https://basescan.org/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) · [Arbiscan](https://arbiscan.io/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) · [OP Etherscan](https://optimistic.etherscan.io/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) · [Polygonscan](https://polygonscan.com/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) — live SHA-256 atomic `lockEth` → `claim` proven on all 4 chains; see [`bridges/README.md`](bridges/README.md))
- **AI Agent & Web4 Discovery (`v0.14.0`):** `POST /mcp` (**19 live tools** — including 5 everyday zero-API-key EVM security audit, Bitcoin mempool, Lightning BOLT-11, Base Uniswap V3 & tx-receipt tools; published on Official MCP Registry) · **Zero-install stdio MCP & CLI:** `npx -y github:theguysaccount/newsatoshi-commons` · `/.well-known/x402` (**4 live `x402 v2` USDC + incremental O(1) SHA-256 PayWord endpoints**) · `/.well-known/agent-card.json` & `/.well-known/agent.json` · `/.well-known/erc8004.json` · `/llms.txt` · `/llms-full.txt` · `/openapi.json`
- **Live 0%-Fee Bitaxe / SHA-256 Solo Pool:** `stratum+tcp://pool.newsatoshi.org:3333` (`https://pool.newsatoshi.org/status`, direct coinbase payouts to any Mainnet `ns1…` address from `https://newsatoshi.org/wallet/`)
- **Live L1 Seed Reference:** `https://newsatoshi.org/mainnet/status` (Mainnet, `auxpow_chainid = 20051`, `50 LOVE/block`, `0.006 LOVE` default L1 fee; note: experimental & awaiting external audit + recursive chain proofs) & `https://newsatoshi-seed.fly.dev/status` (Testnet)
- **RFC-0021 & RFC-0022 Height-Gated L1 Hard-Fork Prototypes (128-bit Proven `0xb4` Covenants + Seller-Signed Unilateral Partial-Claim `1,000 → 237 / 763` PayWord Channels & 64× Batched Opens):** `https://github.com/theguysaccount/newsatoshi-rfc0021` ([`RFC-0022-PAYWORD-CHANNELS.md`](https://github.com/theguysaccount/newsatoshi-rfc0021/blob/main/RFC-0022-PAYWORD-CHANNELS.md))

### Incremental O(1) PayWord Metering & On-Chain Funding Verification (`v0.14.0`)
PayWord hash-chain micropayments were introduced by **Rivest & Shamir (1996)** and HTTP `402` API metering by **Lightning Labs L402 (2020)**. Newsatoshi combines those primitives with fair-launched Bitcoin AuxPoW merge-mining, a shielded post-quantum browser wallet (`newsatoshi.org/wallet/`), and no issuer freeze switch on native `LOVE` (miners still control block inclusion).

- **True `O(1)` incremental verification (up to `1,000,000` ticks per channel):** Each sequential voucher verifies `SHA256^(ticks - lastTicks)(token) == lastToken` — **strictly 1 SHA-256 evaluation per call**, with pluggable durable watermark persistence across restarts (`createFilePayWordStore` in JS, `state_file` in Python).
- **Reproducible local verification benchmark (Node.js v24.22.1, median of 9 runs via `sdk/js/newsatoshi-402.mjs`):**
  - Voucher `#1`: `0.034 ms` (1 hash) · `#100`: `0.021 ms` (1 hash) · `#1,000`: `0.018 ms` (1 hash) · `#10,000`: `0.018 ms` (1 hash) · `#1,000,000`: `0.013 ms` (1 hash).
- **Strict solvency honesty (`collectibleOnChain`):** Local hash verification alone does **not** prove channel solvency. Production merchants configure `createPayWordGate({ requireFunding: true })` (JS) or `PayWordGate(require_funding=True)` (Python), which queries live Newsatoshi L1 `/api/lock?hash=<tip>` (or a custom EVM HTLC verifier) to confirm an open lock paying the merchant with sufficient balance and timeout margin (`fundingStatus: "verified_onchain_lock"`). Unfunded sandbox tests must opt into `allowUnfundedDemo: true` and are explicitly tagged `fundingStatus: "unfunded_demo"`, `collectibleOnChain: false` (and `settled: false` on `commons.newsatoshi.org/api/x402/*`).

### Instant Zero-API-Key Agent Setup (1 Command — No npm Account or Keys Required)
```bash
# Stdio MCP server for Claude Desktop, Cursor, Cline, Codex & OpenClaw (19 live tools):
npx -y github:theguysaccount/newsatoshi-commons

# Or run everyday crypto security / DeFi / BTC utilities directly from any shell:
npx -y github:theguysaccount/newsatoshi-commons inspect 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913 --chain base
npx -y github:theguysaccount/newsatoshi-commons btc-fees
npx -y github:theguysaccount/newsatoshi-commons quote-base ETH USDC 1.0
npx -y github:theguysaccount/newsatoshi-commons payword-demo

# Python zero-dependency SDK (MCP + HTTP 402 PayWord + x402):
pip install "git+https://github.com/theguysaccount/newsatoshi-commons.git#subdirectory=sdk/python"
```

### 1-Line Rust CLI Install (macOS & Linux — Verified SHA-256 Prebuilt Binaries)
```bash
curl -fsSL https://commons.newsatoshi.org/install.sh | sh
```
Installs `newsatoshi-commons`, `newsatoshi-swap`, and `newsatoshi-hw-signer` into `~/.local/bin` (with automatic `SHA256SUMS.txt` verification and `cargo install` fallback).

### Transacting, Monetizing & Launching a Web4 Business (60-Second Setup)
1. **Connect Streamable HTTP MCP (`POST https://commons.newsatoshi.org/mcp`):** Zero auth required for **14 live tools** (network status, live 4-chain Proof-of-Backing reserves, Web4 Business Charter compilation with Lean 4 monotonic child bounds, ERC-8004 identity registration, 402 service & HTLC bounty discovery, live AuxPoW mining yield economics, multi-chain BTC/Lightning/Solana/Base HTLC swap compilation, dual-chain lock verification, HTTP `402` challenge generation + PayWord verification, and Winterfell STARK verification).
2. **Spawn a Sovereign Autonomous Web4 Business Daemon (`web4-kit/`):**
   ```bash
   node web4-kit/agent-business.mjs --check   # Live 4-chain Proof-of-Backing & survival-tier check
   node web4-kit/agent-business.mjs           # Runs a live PayWord + x402 paid API + ERC-8004 manifest
   ```
3. **Install the Agent Skill:** `npx skills add theguysaccount/newsatoshi-commons` (`https://commons.newsatoshi.org/skills/newsatoshi-agent-payments/SKILL.md`).
4. **Add a 3-Line Dual `PayWord + x402` HTTP `402` Paywall (Zero Dependencies):**
   - **Node 18+ / Cloudflare Workers / Vercel Edge:** `sdk/js/newsatoshi-402.mjs` (`generatePayWordChain`, `createPayWordGate`, `createDual402Gate`, `fetchWithNewsatoshi402`, `NewsatoshiMcpClient`)
   - **Python 3 (FastAPI / Flask / stdlib):** `sdk/python/newsatoshi_402.py` (`generate_payword_chain`, `PayWordGate`, `NewsatoshiMcpClient`)

### 4 Trustless Multi-Chain Buy Roads (Zero Exchanges · Zero Wrapped IOUs · Zero Custody)

| Source Chain / Asset | Contract / Script Standard | Shared Atomic Invariant | Location |
|---|---|---|---|
| **Bitcoin L1 (BTC)** | Canonical BIP-199 SegWit v0 P2WSH (`bc1q…`) | `SHA256(preimage) == payment_hash` | `src/swap.rs`, `newsatoshi-swap` |
| **Lightning Network (BTC)** | BOLT-11 Invoice Decoding → L1 Hash-Lock | `SHA256(preimage) == payment_hash` | `src/swap.rs`, `bridges/lightning/` |
| **Solana (SOL & SPL-USDC)** | Deterministic PDA Escrow (`ns_htlc` seeds) | `SHA256(preimage) == payment_hash` | `bridges/solana/src/lib.rs` |
| **Base / Arb / OP / Polygon** | Immutable Ownerless `NewsatoshiHtlc.sol` (`0x02`) | `SHA256(preimage) == payment_hash` | **LIVE on 4 Mainnets:** `0xF564A8F6…8987` (`bridges/evm/`) |

> **Core Invariant:** Keep Newsatoshi L1 issuance and SHA-256 merged-mining ordering (`auxpow_chainid = 20051`) **blind and ossified** — never mint base currency directly for physical work (avoiding Budish 2018 rented-hashpower 51% attacks and Sybil wash-farming). Run recursive Commons treasuries as a CSV engine anchored to live L1 blocks today, then activate `OP_VERIFY_COVENANT_STARK` (`0xb4`) and `RFC-0022` channels via a height-gated consensus upgrade (hard fork) or audited Mainnet v2 launch.

> **Zero Fake Data Invariant:** Fresh deployments start at genuine `0 LOVE`, `0` UTXOs, `0` parcels, `0` verified kWh, `0` STARK proofs, and empty telemetry histories. Every balance, parcel acquisition, signed telemetry packet, STARK proof, and L1 anchor receipt reflects a real state transition or live upstream L1 node reading.

---

## Repository Architecture (All 4 Pillars)

```text
newsatoshi-commons/
├── Cargo.toml                         # v0.5.0: slh-dsa 0.2.0-rc.5, winterfell 0.13.1, tiny_http, ureq
├── Dockerfile                         # 3-stage Node 22 + Rust + debian-slim production container (COMMONS_VERIFY_ONLY=1 + dual-chain polling)
├── fly.toml                           # Fly.io deployment config (persistent /data volume + /api/health check)
├── auxpow-stratum/                    # 1-command Docker + zero-dep Python 3 Stratum v1 AuxPoW merge-mining kit (chainid = 20051, Bitaxe/ASIC/bitcoind)
├── src/
│   ├── crypto.rs                      # Pure SHA-256 Lamport OTS, XMSS Merkle-Lamport (64–4,096 leaves), NIST FIPS 205 SLH-DSA-SHA2-128f
│   ├── lib.rs                         # CommonsLedger, CovenantRule, CheckedWorkProof, PayWord StreamingChannel, UTXO consolidation
│   ├── stark.rs                       # Real Winterfell 0.13 AIR prover & verifier (15-col x 64-row, 63-bit bit-decomp, 99-bit PQ security)
│   ├── swap.rs                        # Trustless BTC <-> LOVE atomic swaps: BIP-199 P2WSH HTLCs, BIP-173 Bech32, BOLT-11 decoder, dual-chain verifier
│   ├── storage.rs                     # Atomic fsync+rename JSON persistence + strict --verify-only key isolation (zero private seeds on server)
│   ├── seed.rs                        # Live dual-chain HTTPS poller (testnet + mainnet /status) & live /api/lock?hash= on-chain verifier
│   ├── rpc.rs                         # Deterministic AnchorCommitment bound to live L1 tip + on-chain lock verification metadata
│   ├── hardware.rs                    # HardwareTelemetrySigner + ExternalTelemetryPacket + ExternalDeedPacket (offline XMSS/SLH-DSA)
│   ├── server.rs                      # CORS HTTP API + COMMONS_ADMIN_TOKEN + --verify-only + /api/deeds + /api/swaps* + /api/actions/verify-anchor
│   ├── main.rs                        # CLI (`serve`, `simulate`, `telemetry-sign`, `anchor`, `derive-public-roots`, `export-legal-schedule`)
│   ├── bin/newsatoshi-hw-signer.rs    # Zero-disk-secret field meter daemon (`provision --env-seeds`, `sign`, `daemon`, `poll-meter`, `sign-deed`)
│   └── bin/newsatoshi-swap.rs         # Standalone trustless BTC <-> LOVE atomic swap CLI (`keygen`, `htlc`, `decode-bolt11`, `verify-preimage`, `watch`)
├── tests/integration_tests.rs         # 26 Rust tests (SLH-DSA, STARKs, BIP-173/BOLT-11/BIP-199 swaps, verify-only, dual-chain, L1 lock verify)
├── docs/
│   ├── RFC-0021-OP_VERIFY_COVENANT_STARK.md  # Formal NEWSATOSHI/2.1 soft-fork specification & AuxPoW signaling
│   └── AUXPOW-MAINNET-MINING-GUIDE.md        # Live-verified Mainnet & Bitcoin AuxPoW merged-mining pool onboarding guide (chainid 20051)
├── zkvm/
│   ├── covenant-circuit-core/         # Pure-Rust `no_std`-compatible recursive IVC verification kernel (14/14 tests, 0 clippy warnings)
│   ├── sp1-covenant-guest/            # SP1 Hypercube RISC-V guest & host prover/verifier driver
│   ├── miden-covenant/                # Native Goldilocks STARK Miden Assembly (`covenant_transition.masm`)
│   └── testnet-harness/               # 90-day soak & adversarial verification script (`adversarial_suite.sh`)
├── web/                               # Live Thermodynamic Telemetry Mirror, Swaps & Mining Cockpit (Vite + React + Tailwind v4)
└── legal/
    ├── perpetual-purpose-trust-charter.md     # Perpetual Purpose Trust Instrument (Section 4.2 Deterministic Cryptographic Execution)
    ├── wyoming-duna-clt-operating-agreement.md # Wyoming DUNA + 99-Year CLT Ground Lease + PayWord Offtake Addendum
    ├── parcel-1-unit-economics.md             # Illustrative sub-$50k ($49,500) Parcel #1 planning worksheet (pre-deployment assumptions)
    ├── parcel-1-model.json                    # Machine-readable exact-Decimal planning model
    ├── render_airlock.py                      # Zero-dep renderer binding live crypto hashes into legal execution drafts
    ├── schedule-generated.json                # Live cryptographic schedule exported by `newsatoshi-commons export-legal-schedule`
    └── rendered/                              # Hash-bound legal drafts + SHA-256 `airlock-manifest.json`
```

---

## Live Newsatoshi L1 Infrastructure Reference

| Network | Endpoint / Port | Genesis Hash | Status |
|---|---|---|---|
| **Testnet** (`auxpow_chainid = 20051`) | `https://newsatoshi-seed.fly.dev/status` (`:18444` P2P) | `00000fd814d823a0bc64a84135868402998cff81e28a85b2bede5f1125d0a394` | Live & mining (faucet enabled, height `22,500+`) |
| **Mainnet** (`auxpow_chainid = 20051`) | `https://newsatoshi.org/mainnet/status` (`newsatoshi-seed.fly.dev:8444` P2P) | `000000006ba30cc3c86ecaabcb962287e9d5fd0877c27f4b33661b4d1d3033f7` | Live & merge-mined (`50 LOVE/block`, AuxPoW `chainid = 20051`) |

---

## Quickstart

### 1. Run All Rust & zkVM Tests
```bash
export PATH="$HOME/.cargo/bin:$PATH"

# Core engine, NIST FIPS 205 SLH-DSA, Winterfell 0.13 STARKs, auth, persistence, HTTP API, hw-signer (17 tests)
cargo test && cargo clippy --all-targets -- -D warnings

# Pure-Rust no_std zkVM circuit verification kernel (14 tests)
(cd zkvm/covenant-circuit-core && cargo test && cargo check --no-default-features)
```

### 2. Launch the Live Thermodynamic Telemetry Mirror + Rust Commons Engine
```bash
# Build frontend bundle
(cd web && npm ci && npm run build)

# Start unified Rust API + live newsatoshi-seed poller + Telemetry Mirror UI
# Set COMMONS_ADMIN_TOKEN (or --admin-token) on public hosts to protect operator write actions
# while keeping reads, STARK verification, and cryptographically signed hardware telemetry public.
COMMONS_ADMIN_TOKEN="replace-with-strong-random-token" \
cargo run --bin newsatoshi-commons -- serve \
  --port 8791 \
  --state-file ./data/commons-ledger.json \
  --seed-url https://newsatoshi-seed.fly.dev/status \
  --static-dir ./web/dist
```
Then open `http://127.0.0.1:8791` in your browser.

### 3. Run the Standalone Hardware Telemetry Signer (`newsatoshi-hw-signer`)
```bash
# Provision persistent device keyfile (XMSS Merkle-Lamport + NIST FIPS 205 SLH-DSA-SHA2-128f)
cargo run --bin newsatoshi-hw-signer -- provision \
  --keyfile ./data/hw-inverter-1.json \
  --device-id pi-solar-parcel-1 \
  --parcel 1 \
  --default-engine-seed --json

# Stream real JSONL meter readings into the running Commons Engine (never synthesizes readings)
printf '{"kwh_delivered":420,"soil_health_score":91,"payout_love":100}\n' | \
  cargo run --bin newsatoshi-hw-signer -- daemon \
    --keyfile ./data/hw-inverter-1.json \
    --endpoint http://127.0.0.1:8791/api/telemetry \
    --scheme slh_dsa \
    --stdin-jsonl
```

### 4. Export Live Cryptographic Schedule & Render Bound Legal Airlock
```bash
cargo run --bin newsatoshi-commons -- export-legal-schedule --out legal/schedule-generated.json
python3 legal/render_airlock.py --schedule legal/schedule-generated.json
```

### 5. Run the 90-Day Adversarial Testnet Verification Harness
```bash
./zkvm/testnet-harness/adversarial_suite.sh --api-url http://127.0.0.1:8791
```

### 6. Production Deployment (Fly.io)
```bash
flyctl apps create newsatoshi-commons --org personal
flyctl volumes create commons_data --region iad --size 1 -a newsatoshi-commons -y
flyctl secrets set COMMONS_ADMIN_TOKEN="$(openssl rand -hex 32)" -a newsatoshi-commons
flyctl deploy -a newsatoshi-commons
```
