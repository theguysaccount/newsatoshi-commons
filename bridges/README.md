# Multi-Chain Trustless Buy Roads into Newsatoshi (`LOVE`)

Every buy road in this directory uses **identical SHA-256 HTLC cryptography (`SHA256(32-byte preimage) == payment_hash`)** matching `newsatoshi-0.1.0/node/src/lock.rs`: **zero custodians, zero multisig bridges, zero wrapped tokens.**

| Buy Road | Source Wallets / Fiat On-Ramps | Cryptographic Primitive | Status |
|---|---|---|---|
| **1. Bitcoin Lightning (`bridges/lightning/`)** | Cash App, Strike, Coinbase LN, Phoenix, Zeus, Wallet of Satoshi (Apple Pay / debit → LN → LOVE in 30s) | Native `newsatoshi-0.1.0` `--swap-lnd` Hold Invoices + BOLT-11 `payment_hash` | Ready to run (`docker compose up -d`) |
| **2. Bitcoin On-Chain (`src/swap.rs`)** | Sparrow, Electrum, Ledger, Trezor, Bitcoin Core | Canonical BIP-199 SegWit v0 `P2WSH` (`bc1q…`) `OP_SHA256` HTLC | Live in `newsatoshi-commons` & `newsatoshi-swap` |
| **3. Solana (`bridges/solana/`)** | Phantom, Backpack, Solflare + **Jupiter** (any SOL/SPL token → USDC → HTLC → LOVE) | Native `sol_sha256` deterministic PDA Escrow (`["ns_htlc", hash, sender]`) | Contract & instruction codec ready (`cargo test` verified) |
| **4. Base / Arbitrum / Optimism / Polygon (`bridges/evm/`)** | MetaMask, Coinbase Smart Wallet (Apple Pay → Base USDC), Rabby, Rainbow | Immutable ownerless `NewsatoshiHtlc.sol` using EVM `0x02` `sha256()` precompile | **LIVE ON 4 MAINNETS**: Universal address [`0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`](https://basescan.org/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) (all 4 Sourcify `exact_match` + live atomic claims proven) |
| **5. Direct Hashrate (`auxpow-stratum/`)** | Bitaxe, NerdQAxe, Antminer S19/S21, solo `bitcoind` | AuxPoW merge-mining (`chainid = 20051`, `50 LOVE/block`) | Live E2E verified against `newsatoshi-0.1.0` |

## Live Multi-Chain EVM Deployments (Single Universal Address)

All four mainnets share the **exact same immutable, ownerless `NewsatoshiHtlc.sol` contract address** (`0` constructor args, `0` admin/owner keys, Sourcify `exact_match` creation + runtime):

> **Universal Canonical Contract:** `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`

| Mainnet | Chain ID | Explorer & Sourcify (`exact_match`) | Live Atomic `lockEth` TX | Live Atomic `claim` TX |
|---|---|---|---|---|
| **Base** | `8453` | [BaseScan](https://basescan.org/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) · [Sourcify](https://repo.sourcify.dev/8453/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) | [`0x10bbd4…11ed`](https://basescan.org/tx/0x10bbd471e3c5dd34342b30530e5c1238950372728a21270e2a763f2aebea11ed) | [`0x0f6cbf…f595`](https://basescan.org/tx/0x0f6cbfd277610b112d79fe36138dd549931f45f4ffa2894f7ea9e5831a15f595) |
| **Arbitrum One** | `42161` | [Arbiscan](https://arbiscan.io/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) · [Sourcify](https://repo.sourcify.dev/42161/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) | [`0xc34179…75c1`](https://arbiscan.io/tx/0xc34179e7e08775d392d51313f476a8367312dceb325cd405da0eedbdb21475c1) | [`0xd11efc…4d6e`](https://arbiscan.io/tx/0xd11efcb1750aae5d5d443f4d446b84b609f6d253ca5152ebf1ea279a13de4d6e) |
| **Optimism** | `10` | [OP Etherscan](https://optimistic.etherscan.io/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) · [Sourcify](https://repo.sourcify.dev/10/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) | [`0xf231d7…2981`](https://optimistic.etherscan.io/tx/0xf231d7bfba9d8f54ae90af40f6c7eea484607122a554026972fb09671d522981) | [`0xdb4ee9…81b8`](https://optimistic.etherscan.io/tx/0xdb4ee991b86febd16a60c6a8a29a6ed4ac2d0dd92aebbc6a835f1d9c195881b8) |
| **Polygon PoS** | `137` | [Polygonscan](https://polygonscan.com/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) · [Sourcify](https://repo.sourcify.dev/137/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) | [`0x62362d…a756`](https://polygonscan.com/tx/0x62362df325ce2ce22f9d83cf945778699f75dd8445ce5d0c453c63b6cd2ea756) | [`0x868f34…4fb4`](https://polygonscan.com/tx/0x868f347586d34f4c09bb6aae2b6b9c0f0ec657ab5f2dbfa87b22197aa8f94fb4) |

## Deploying to Additional EVM Chains (`NewsatoshiHtlc.sol`)

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
