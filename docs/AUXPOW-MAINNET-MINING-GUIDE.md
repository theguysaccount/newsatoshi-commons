# Newsatoshi Mainnet Mining & Bitcoin AuxPoW Pool Onboarding Guide

> **Verified Against:** Official `newsatoshi` v0.1.0 source (`https://newsatoshi.org/newsatoshi-0.1.0-src.zip`, SHA-256 `3d022302f4120a446dc2cc505dcf51036d832e1d68d3855c0afc38aacb396134`) and live `https://newsatoshi.org/mainnet/status`.
>
> **Core Launch Principle (from `LAUNCH.md`):** Zero premine, zero developer allocation, zero foundation. The genesis coinbase pays no one, and the project's seed nodes intentionally do not mine Mainnet. External SHA-256 miners and Bitcoin merged-mining pools mine Block `#1` onward openly.

---

## 1. Verified Mainnet Consensus & AuxPoW Parameters

| Parameter | Verified Value (`newsatoshi` v0.1.0) |
|---|---|
| **Network Name** | `mainnet` (`--net mainnet`) |
| **Network Magic** | `nsM0` (`0x6e734d30`) |
| **Genesis Hash** | `000000006ba30cc3c86ecaabcb962287e9d5fd0877c27f4b33661b4d1d3033f7` |
| **Genesis Anchor** | Opened 10 Nov 2026 over Bitcoin block `#975,317` |
| **AuxPoW Chain ID** | `20051` (`0x4e53` — ASCII `"NS"`) |
| **AuxPoW Coinbase Magic** | `fabe6d6d` (Namecoin-compatible merged-mining format) |
| **Initial Difficulty / Bits** | `difficulty = 1.0` (`bits = 1d00ffff`, ~4.295B SHA-256d hashes/block) |
| **Difficulty Algorithm** | ASERT (`aserti3-2d`, 2-day half-life, 600s target block spacing) |
| **Block Subsidy** | `50 LOVE` (`5,000,000,000` atomic units), halving every `210,000` blocks down to a perpetual `0.1953125 LOVE` floor |
| **Coinbase Maturity** | `100` blocks |
| **P2P Port & Seed** | `8444` (`newsatoshi-seed.fly.dev:8444`) |
| **Live Public Status JSON** | `https://newsatoshi.org/mainnet/status` |
| **Proof Profile** | `main` (masked Winterfell 0.13 STARKs, `default_fee = 600000` atomic units) |

---

## 2. Build the Official `newsatoshi` v0.1.0 Binary from Source

```bash
curl -fsSL https://newsatoshi.org/newsatoshi-0.1.0-src.zip -o newsatoshi-0.1.0-src.zip
shasum -a 256 newsatoshi-0.1.0-src.zip
# Expected: 3d022302f4120a446dc2cc505dcf51036d832e1d68d3855c0afc38aacb396134
unzip newsatoshi-0.1.0-src.zip
cd newsatoshi-0.1.0/node
cargo build --release --bin newsatoshi
./target/release/newsatoshi --net mainnet address
```

---

## 3. Bitcoin Merged-Mining Pool Integration (Namecoin-Compatible AuxPoW RPC)

`newsatoshi` implements Namecoin's exact AuxPoW JSON-RPC interface (`src/rpc.rs`), so any Bitcoin pool stack that already merge-mines Namecoin (CKPool, Braiins/Stratum V1/V2 aux bridges, Nomad/Yiimp, or custom aux daemons) only needs a new chain entry with `chainid = 20051`.

### Start a Mainnet Node with AuxPoW JSON-RPC Enabled

```bash
./target/release/newsatoshi --net mainnet --datadir /var/lib/newsatoshi-main node \
  --port 8444 \
  --rpc 8332 \
  --rpcbind 127.0.0.1 \
  --rpcauth "pooluser:REPLACE_WITH_STRONG_RANDOM_PASSWORD"
```

*(If `--rpcauth` is omitted, `newsatoshi` automatically writes an HTTP Basic cookie to `<datadir>/.cookie`.)*

### Supported Pool JSON-RPC Methods (HTTP/1.1 POST + Basic Auth)

| Method | Parameters | Purpose |
|---|---|---|
| `createauxblock` | `["ns1..."]` | Build a block template paying the specified shielded `ns1...` pool address. Returns `{ hash, chainid: 20051, previousblockhash, coinbasevalue, bits, height, _target }`. |
| `getauxblock` | `[]` | Build a block template paying this node's own wallet address. |
| `submitauxblock` | `["<hash>", "<auxpow_hex>"]` | Submit the solved Bitcoin parent header + coinbase + Merkle branches in Namecoin's AuxPoW wire format. Returns `true` when accepted. |
| `getauxblock` | `["<hash>", "<auxpow_hex>"]` | Alias for `submitauxblock`. |
| `getblockchaininfo` / `getmininginfo` | `[]` | Returns `{ chain, blocks, bestblockhash, difficulty, chainwork, auxpow_chainid: 20051 }`. |
| `getblockcount` | `[]` | Current mainnet chain height. |
| `getbestblockhash` | `[]` | Current tip hash (display byte order). |
| `getdifficulty` | `[]` | Current Bitcoin-scale difficulty. |
| `validateaddress` | `["ns1..."]` | Validate a shielded payout address before calling `createauxblock`. |

### Coinbase Commitment Format (Namecoin AuxPoW)

1. Call `createauxblock` with your pool's shielded `ns1...` payout address (or `getauxblock` with `[]`).
2. Take `result.hash` (32-byte hex in display order, or the Merkle root of your multi-aux tree where slot index is computed from `chainid = 20051`).
3. Embed in the Bitcoin parent coinbase scriptSig:
   ```text
   0xfabe6d6d || <32-byte aux_root> || <u32_le tree_size> || <u32_le merkle_nonce>
   ```
4. Whenever a Bitcoin share hash (SHA-256d of the 80-byte parent header) satisfies `int(hash, little) <= int(_target, little)`, serialize the Namecoin AuxPoW struct (`parent_coinbase || block_hash || coinbase_branch || coinbase_index || chain_branch || chain_index || parent_header_80B`) and call `submitauxblock(hash, auxpow_hex)`.

### Local End-to-End Verification Before Connecting Production Hashrate

Verified live with `newsatoshi` v0.1.0's included simulator (`node/contrib/merge_mine_sim.py`):

```bash
# Terminal 1: local regtest node with AuxPoW RPC
./target/release/newsatoshi --net regtest --datadir /tmp/ns-aux-test node \
  --port 19644 --no-seeds --rpc 19656 --rpcbind 127.0.0.1

# Terminal 2: run Namecoin-format AuxPoW pool simulator
python3 contrib/merge_mine_sim.py 19656 /tmp/ns-aux-test/.cookie
# Expected output:
#   template {'hash': '...', 'chainid': 20051, 'height': 1, 'bits': '207fffff', 'coinbasevalue': 5000000000}
#   submit {'result': True, 'error': None, 'id': 'sim'}
#   count 1 best True
```

---

## 4. Direct Solo SHA-256d Mining (No Bitcoin Pool Required)

Anyone can mine Mainnet directly to their own wallet on CPU/ASIC via `--mine` (at initial difficulty `1.0`, `bits = 1d00ffff`, one block requires ~4.295 billion SHA-256d hashes on average):

```bash
./target/release/newsatoshi --net mainnet --datadir ~/.newsatoshi/mainnet node --mine
```

Check live block progress and peers:
```bash
./target/release/newsatoshi --net mainnet --datadir ~/.newsatoshi/mainnet info
curl -sS https://newsatoshi.org/mainnet/status
```

---

## 5. Anchoring `newsatoshi-commons` Once Mainnet Block `#1+` Is Active

Once Mainnet has spendable matured coins (after 100-block coinbase maturity):

1. Generate fresh offline hardware & trustee keys (`newsatoshi-hw-signer provision`) — never use development seeds on Mainnet.
2. Start `newsatoshi-commons serve` in strict `--verify-only` mode (`COMMONS_VERIFY_ONLY=1`) with your external public roots (`--deed-oracle-root`, `--hardware-root`, `--deed-oracle-slh-pk`, `--hardware-slh-pk`) and `--mainnet-seed-url https://newsatoshi.org/mainnet/status`.
3. Prepare a Mainnet state-root anchor (`POST /api/actions/anchor` with `{"network": "mainnet"}`), broadcast the real `newsatoshi lock 0.01 --hash <anchor_hash> --to <ns1_address>` transaction on Mainnet, and verify it publicly via `POST /api/actions/verify-anchor` (cross-checked live against `https://newsatoshi.org/mainnet/api/lock?hash=<anchor_hash>`).
