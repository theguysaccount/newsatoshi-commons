# Multi-Chain Trustless Buy Roads into Newsatoshi (`LOVE`)

Every buy road in this directory uses **identical SHA-256 HTLC cryptography (`SHA256(32-byte preimage) == payment_hash`)** matching `newsatoshi-0.1.0/node/src/lock.rs`: **zero custodians, zero multisig bridges, zero wrapped tokens.**

| Buy Road | Source Wallets / Fiat On-Ramps | Cryptographic Primitive | Status |
|---|---|---|---|
| **1. Bitcoin Lightning (`bridges/lightning/`)** | Cash App, Strike, Coinbase LN, Phoenix, Zeus, Wallet of Satoshi (Apple Pay / debit → LN → LOVE in 30s) | Native `newsatoshi-0.1.0` `--swap-lnd` Hold Invoices + BOLT-11 `payment_hash` | Ready to run (`docker compose up -d`) |
| **2. Bitcoin On-Chain (`src/swap.rs`)** | Sparrow, Electrum, Ledger, Trezor, Bitcoin Core | Canonical BIP-199 SegWit v0 `P2WSH` (`bc1q…`) `OP_SHA256` HTLC | Live in `newsatoshi-commons` & `newsatoshi-swap` |
| **3. Solana (`bridges/solana/`)** | Phantom, Backpack, Solflare + **Jupiter** (any SOL/SPL token → USDC → HTLC → LOVE) | Native `sol_sha256` deterministic PDA Escrow (`["ns_htlc", hash, sender]`) | Contract & instruction codec ready (`cargo test` verified) |
| **4. Base / Ethereum / Arbitrum / Optimism / Polygon (`bridges/evm/`)** | MetaMask, Coinbase Smart Wallet (Apple Pay → Base USDC), Rabby, Rainbow | Immutable ownerless `NewsatoshiHtlc.sol` using EVM `0x02` `sha256()` precompile | Contract ready (`forge create` deployable in 1 cmd) |
| **5. Direct Hashrate (`auxpow-stratum/`)** | Bitaxe, NerdQAxe, Antminer S19/S21, solo `bitcoind` | AuxPoW merge-mining (`chainid = 20051`, `50 LOVE/block`) | Live E2E verified against `newsatoshi-0.1.0` |

## Deploying the Immutable Base / EVM HTLC (`NewsatoshiHtlc.sol`)

Zero constructor arguments, zero owner, zero upgradeability:
```bash
# Base Mainnet (Chain ID 8453 — Coinbase Smart Wallet / Base USDC on-ramp)
forge create bridges/evm/NewsatoshiHtlc.sol:NewsatoshiHtlc \
  --rpc-url https://mainnet.base.org \
  --private-key "$DEPLOYER_KEY" \
  --verify
```
Key canonical USDC token addresses for `lockErc20`:
- **Base Mainnet USDC:** `0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913`
- **Arbitrum One USDC:** `0xaf88d065e77c8cC2239327C5EDb3A432268e5831`
- **Ethereum Mainnet USDC:** `0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48`
- **Solana Mainnet USDC SPL Mint:** `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`

## Running the Multi-Chain Maker Watcher Bot

Lets Mainnet AuxPoW miners (`7,200 LOVE/day`) monitor incoming BTC / Solana / Base HTLC locks and verify preimages:
```bash
python3 bridges/maker-bot/maker_watcher.py \
  --hash <64HEX_PAYMENT_HASH> \
  --love-net mainnet \
  --evm-chain base --evm-contract 0x... --evm-swap-id 0x... \
  --solana-cluster mainnet-beta --solana-pda <PDA_BASE58>
```
