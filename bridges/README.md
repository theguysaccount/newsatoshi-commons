# Multi-Chain Trustless Buy Roads into Newsatoshi (`LOVE`)

Every buy road in this directory uses **identical SHA-256 HTLC cryptography (`SHA256(32-byte preimage) == payment_hash`)** matching `newsatoshi-0.1.0/node/src/lock.rs`: **zero custodians, zero multisig bridges, zero wrapped tokens.**

| Buy Road | Source Wallets / Fiat On-Ramps | Cryptographic Primitive | Status |
|---|---|---|---|
| **1. Bitcoin Lightning (`bridges/lightning/`)** | Cash App, Strike, Coinbase LN, Phoenix, Zeus, Wallet of Satoshi (Apple Pay / debit → LN → LOVE in 30s) | Native `newsatoshi-0.1.0` `--swap-lnd` Hold Invoices + BOLT-11 `payment_hash` | Ready to run (`docker compose up -d`) |
| **2. Bitcoin On-Chain (`src/swap.rs`)** | Sparrow, Electrum, Ledger, Trezor, Bitcoin Core | Canonical BIP-199 SegWit v0 `P2WSH` (`bc1q…`) `OP_SHA256` HTLC | Live in `newsatoshi-commons` & `newsatoshi-swap` |
| **3. Solana (`bridges/solana/`)** | Phantom, Backpack, Solflare + **Jupiter** (any SOL/SPL token → USDC → HTLC → LOVE) | Native `sol_sha256` deterministic PDA Escrow (`["ns_htlc", hash, sender]`) | Contract & instruction codec ready (`cargo test` verified) |
| **4. Base / Arbitrum / Optimism / Polygon (`bridges/evm/`)** | MetaMask, Coinbase Smart Wallet (Apple Pay → Base USDC), Rabby, Rainbow | Immutable ownerless `NewsatoshiHtlc.sol` using EVM `0x02` `sha256()` precompile | **LIVE ON 4 MAINNETS**: Universal address [`0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`](https://basescan.org/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) (all 4 Sourcify `exact_match` + live atomic claims proven) |
| **5. Direct Hashrate (`auxpow-stratum/`)** | Bitaxe, NerdQAxe, Antminer S19/S21, solo `bitcoind` | AuxPoW merge-mining (`chainid = 20051`, `50 LOVE/block`) | Live E2E verified against `newsatoshi-0.1.0` |
| **6. ERC-7683 Solver Intents (`bridges/evm/NewsatoshiErc7683Settler.sol`)** | UniswapX, Across, CoW, Superchain solvers | Immutable `IOriginSettler` + EIP-712 `openFor()` + `sha256` precompile claim | Foundry tested (including 256-run fuzz) + live `/api/v2/interop/erc7683-intent` |
| **7. Cosmos IBC (`bridges/cosmos/`)** | Keplr, Leap, Noble USDC, Osmosis (`ICS-20`) | Immutable CosmWasm 2.x `sha2::Sha256` HTLC (`LockHtlc`/`ClaimHtlc`/`RefundHtlc`) | Contract ready + live `/api/v2/interop/universal-htlc` |
| **8. Sui & Aptos Move (`bridges/move/`)** | Sui Wallet, Petra, Martian (`USDC` / `SUI` / `APT`) | Native `std::hash::sha2_256` Move shared-object / resource-account HTLC | Move modules ready + live `/api/v2/interop/universal-htlc` |

## Live Multi-Chain EVM Deployments (Universal Ownerless Addresses)

All four mainnets share the **exact same immutable, ownerless contract addresses** (`0` constructor args, `0` admin/owner keys, verified on Sourcify):

- **Universal Atomic Swap HTLC (`NewsatoshiHtlc.sol`):** `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`
- **Universal Sovereign Backing Vault (`NewsatoshiBackingVault.sol`):** `0x37Ed94525DBE408fA72cDd156409875666679265`

### 1. `NewsatoshiHtlc.sol` (`0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987`)

| Mainnet | Chain ID | Explorer & Sourcify (`exact_match`) | Live Atomic `lockEth` TX | Live Atomic `claim` TX |
|---|---|---|---|---|
| **Base** | `8453` | [BaseScan](https://basescan.org/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) · [Sourcify](https://repo.sourcify.dev/8453/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) | [`0x10bbd4…11ed`](https://basescan.org/tx/0x10bbd471e3c5dd34342b30530e5c1238950372728a21270e2a763f2aebea11ed) | [`0x0f6cbf…f595`](https://basescan.org/tx/0x0f6cbfd277610b112d79fe36138dd549931f45f4ffa2894f7ea9e5831a15f595) |
| **Arbitrum One** | `42161` | [Arbiscan](https://arbiscan.io/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) · [Sourcify](https://repo.sourcify.dev/42161/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) | [`0xc34179…75c1`](https://arbiscan.io/tx/0xc34179e7e08775d392d51313f476a8367312dceb325cd405da0eedbdb21475c1) | [`0xd11efc…4d6e`](https://arbiscan.io/tx/0xd11efcb1750aae5d5d443f4d446b84b609f6d253ca5152ebf1ea279a13de4d6e) |
| **Optimism** | `10` | [OP Etherscan](https://optimistic.etherscan.io/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) · [Sourcify](https://repo.sourcify.dev/10/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) | [`0xf231d7…2981`](https://optimistic.etherscan.io/tx/0xf231d7bfba9d8f54ae90af40f6c7eea484607122a554026972fb09671d522981) | [`0xdb4ee9…81b8`](https://optimistic.etherscan.io/tx/0xdb4ee991b86febd16a60c6a8a29a6ed4ac2d0dd92aebbc6a835f1d9c195881b8) |
| **Polygon PoS** | `137` | [Polygonscan](https://polygonscan.com/address/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) · [Sourcify](https://repo.sourcify.dev/137/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987) | [`0x62362d…a756`](https://polygonscan.com/tx/0x62362df325ce2ce22f9d83cf945778699f75dd8445ce5d0c453c63b6cd2ea756) | [`0x868f34…4fb4`](https://polygonscan.com/tx/0x868f347586d34f4c09bb6aae2b6b9c0f0ec657ab5f2dbfa87b22197aa8f94fb4) |

### 2. `NewsatoshiBackingVault.sol` (`0x37Ed94525DBE408fA72cDd156409875666679265`)

Non-custodial, ownerless Proof-of-Backing reserve & Web4 business charter anchor contract (`12/12` Foundry tests including 256 fuzz runs). Assets deposited with `depositEth` / `depositErc20` permanently back the Newsatoshi ecosystem — there is **zero owner, zero admin, zero proxy, and zero withdrawal path**.

| Mainnet | Chain ID | Explorer & Sourcify | Deploy TX | Live Proven On-Chain Backing |
|---|---|---|---|---|
| **Base** | `8453` | [BaseScan](https://basescan.org/address/0x37Ed94525DBE408fA72cDd156409875666679265) · [Sourcify](https://repo.sourcify.dev/8453/0x37Ed94525DBE408fA72cDd156409875666679265) | [`0x3a3f1f…cfa3`](https://basescan.org/tx/0x3a3f1fefe0f1f9bd22b5fcfc003e3f4ece4d04e4c86a1d76311d8193a412cfa3) | Charter #1 anchor [`0x7a8183…eb47`](https://basescan.org/tx/0x7a8183b60b6b1d8b46cc46b2eb022b3fa046f73b9f1f7ee464a5a3415815eb47) + **`0.001 ETH`** genesis reserve [`0xc354e0…bf5c`](https://basescan.org/tx/0xc354e0cca5bd2f0e9ac22193ef6f4ea505393ab4be4177251f9f47ca1c10bf5c) |
| **Arbitrum One** | `42161` | [Arbiscan](https://arbiscan.io/address/0x37Ed94525DBE408fA72cDd156409875666679265) · [Sourcify](https://repo.sourcify.dev/42161/0x37Ed94525DBE408fA72cDd156409875666679265) | [`0x720252…58e3`](https://arbiscan.io/tx/0x720252fa3d1d69a39d750ec2209cc94b26f0c983e39cbbd19e712a057d1c58e3) | Live & ready for `depositEth` / `depositErc20` / `registerCharterAnchor` |
| **Optimism** | `10` | [OP Etherscan](https://optimistic.etherscan.io/address/0x37Ed94525DBE408fA72cDd156409875666679265) · [Sourcify](https://repo.sourcify.dev/10/0x37Ed94525DBE408fA72cDd156409875666679265) | [`0x153204…cca4`](https://optimistic.etherscan.io/tx/0x15320488f4a23e999e1a5036f8eb629c3259357be139bd14cd258e26e7e8cca4) | Live & ready for `depositEth` / `depositErc20` / `registerCharterAnchor` |
| **Polygon PoS** | `137` | [Polygonscan](https://polygonscan.com/address/0x37Ed94525DBE408fA72cDd156409875666679265) · [Sourcify](https://repo.sourcify.dev/137/0x37Ed94525DBE408fA72cDd156409875666679265) | [`0xbe08c5…e97`](https://polygonscan.com/tx/0xbe08c50f1d29aab8efcfa017d6832ee56dc24f95f796f0458296caaf84dc0e97) | Live & ready for `depositEth` / `depositErc20` / `registerCharterAnchor` |

### 3. `NewsatoshiBaseMirror.sol` (`wLOVE`) & Burned Uniswap V3 Pool on Base Mainnet

- **Token (`wLOVE`, 0 owner, 0 mint, 0 tax, Sourcify `exact_match`):** [`0x0A3ccFb3fF981a22A264Aa05CD8b77d06C2732A3`](https://basescan.org/address/0x0A3ccFb3fF981a22A264Aa05CD8b77d06C2732A3) · [Sourcify](https://repo.sourcify.dev/8453/0x0A3ccFb3fF981a22A264Aa05CD8b77d06C2732A3)
- **Uniswap V3 `wLOVE / WETH` (1%) Full-Range Pool:** [`0x08ddc8C3fc09ACdab6802a13B26631f47ABcB888`](https://basescan.org/address/0x08ddc8C3fc09ACdab6802a13B26631f47ABcB888) · [Uniswap](https://app.uniswap.org/swap?chain=base&outputCurrency=0x0A3ccFb3fF981a22A264Aa05CD8b77d06C2732A3) · [DEXScreener](https://dexscreener.com/base/0x08ddc8C3fc09ACdab6802a13B26631f47ABcB888) · [GeckoTerminal](https://www.geckoterminal.com/base/pools/0x08ddc8c3fc09acdab6802a13b26631f47abcb888)
- **Permanent LP Burn Proof:** 100% of the `21,000,000 wLOVE` supply + `0.006 WETH` seeded into LP NFT `#6689936` ([Mint TX](https://basescan.org/tx/0xd4e83efbae91d6aa92b4cb4c34b7731df8f8508530224879e1186f7173f30ea8)) and permanently burned to `0x000000000000000000000000000000000000dEaD` ([Burn TX](https://basescan.org/tx/0x378a7f94ea9d6d4dcb4d3baa8e46e9b0936d4aecb1aca0420d94f2671b64c4bc)). Deployer holds `0 wLOVE`. First live Uniswap V3 swap ([TX](https://basescan.org/tx/0xbf082f81c1a3f1b0ef1783583011627b02c8a4e8437d7e1197258a318f09f24a)) deposited straight into `NewsatoshiBackingVault` ([TX](https://basescan.org/tx/0xfc60f6cee788aa85e31ddba179790019e002fe6135918287d5c21047f8ed011d)).

### 4. Official On-Chain ERC-8004 Agent Identity (`0x8004A169FB4a3325136EB29fA0ceB6D2e539a432`)

- **Base (`8453`):** Agent NFT `#1229992` ([TX](https://basescan.org/tx/0x0eda4629e7976d29edeccaef5bae3453e8fe434a66c77e6443b8047257468bbe))
- **Arbitrum One (`42161`):** Agent NFT `#1692` ([TX](https://arbiscan.io/tx/0x5f915f240194e285c97f874ab25c569a87fa0279a6f59f7c0c3cf1634ad351e2))
- **Optimism (`10`):** Agent NFT `#42519` ([TX](https://optimistic.etherscan.io/tx/0x0a469889afe20774a1210c4fd93f27924c0c7043884b1177e51d22506bc27ac6))
- **Polygon PoS (`137`):** Agent NFT `#715` ([TX](https://polygonscan.com/tx/0x2d097bf8a65e48a283c75c1f6771d0401cc501921f841fc14463227c6d5cfe9a))

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
