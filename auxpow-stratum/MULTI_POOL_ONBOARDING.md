# Multi-Pool Bitcoin AuxPoW Onboarding Guide (`v1.2.5` · `chainid = 20051`)

Every Bitcoin SHA-256 mining pool, solo pool (CKPool / Public Pool), and home Umbrel/Start9 node can merge-mine **Newsatoshi Mainnet v2 (`LOVE`, AuxPoW `chainid = 20051` / `0x4e53`)** at **`$0` extra electricity or hashrate cost** by adding one standard Namecoin `fabe6d6d` 44-byte AuxPoW commitment into their existing Bitcoin coinbase `scriptSig`.

- **Block Reward:** `50 LOVE/block` (Bitcoin halving schedule + `0.1953125 LOVE` perpetual tail security floor)
- **Protocol Tax / Premine:** `0%` (100% paid directly to your pool or miner shielded `ns1...` address)
- **Live Reference Pool:** `stratum+tcp://pool.newsatoshi.org:3333` (`https://pool.newsatoshi.org/status`)

## Option A: 1-Click Umbrel Community App Store (Home Bitaxe / NerdQAxe / ASICs)

Open **Umbrel → App Store → Community App Stores (`⋯`)** and add:
```text
https://github.com/theguysaccount/newsatoshi-umbrel-store
```
Install **Newsatoshi BTC+LOVE AuxPoW Merge-Miner** — auto-wires to Umbrel's built-in `bitcoind` and serves `stratum+tcp://umbrel.local:3333`.

## Option B: 1-Command Prebuilt Multi-Arch Docker Sidecar (`linux/amd64` + `linux/arm64`)

Prebuilt GHCR images (zero local Rust compile required):
- `ghcr.io/theguysaccount/newsatoshi-node:v1.2.5`
- `ghcr.io/theguysaccount/newsatoshi-auxpow-stratum:v1.2.5`

```bash
git clone https://github.com/theguysaccount/newsatoshi-commons.git
cd newsatoshi-commons/auxpow-stratum
export BTC_RPC_URL="http://host.docker.internal:8332/"
export BTC_RPC_AUTH="bitcoinrpc:<YOUR_BITCOIND_RPC_PASS>"
export BTC_PAYOUT_SCRIPT_HEX="0014<your_20byte_p2wpkh_hash>"
export NS_PAYOUT_ADDRESS="ns1<your_74char_shielded_mainnet_v2_address>"
export NS_RPC_AUTH="auxpow:$(openssl rand -hex 16)"
docker compose -f docker-compose.merge-mine.yml up -d
```

Or run `stratum_auxpow.py` directly against a local `v1.2.5` Mainnet v2 node + `bitcoind`:
```bash
python3 stratum_auxpow.py \
  --ns-rpc-url http://127.0.0.1:19556/ \
  --ns-rpc-auth "$NS_RPC_AUTH" \
  --btc-rpc-url http://127.0.0.1:8332/ \
  --btc-rpc-auth "<BTC_RPC_USER>:<BTC_RPC_PASS>" \
  --btc-payout-script-hex "<YOUR_POOL_BTC_SCRIPTPUBKEY_HEX>" \
  --payout-address "<YOUR_POOL_74CHAR_NS1_ADDRESS>" \
  --tag "/YourPoolName/"
```

Additional turnkey node kits in this folder:
- Start9 EmbassyOS: [`start9/`](./start9/)
- Public Pool / Bitaxe Bridge: [`public-pool-bridge/`](./public-pool-bridge/)
- CKPool Sidecar Env: [`ckpool-sidecar.env.example`](./ckpool-sidecar.env.example)

## Option C: Direct Integration into Existing Bitcoin Pool Engines (CKPool / Braiins / Stratum V2 / NOMP)

Newsatoshi Mainnet v2 (`v1.2.5`) implements the standard Namecoin/Dogecoin Bitcoin Core AuxPoW JSON-RPC interface:

1. **Fetch work template (`createauxblock`):**
   ```bash
   curl -sS -u "$NS_RPC_AUTH" http://127.0.0.1:19556/ \
     -d '{"jsonrpc":"1.0","id":1,"method":"createauxblock","params":["<POOL_74CHAR_NS1_ADDRESS>"]}'
   ```
   Returns `{ "hash": "<aux_hash_be>", "chainid": 20051, "target": "<target_be>", "bits": "..." }`.

2. **Embed 44-byte commitment in Bitcoin coinbase `scriptSig`:**
   ```text
   fabe6d6d || reverse_bytes(aux_hash) [32B] || merkle_size (1u32 LE) || merkle_nonce (0u32 LE)
   ```
   *(If merge-mining multiple aux chains like Namecoin + Dogecoin + Newsatoshi, place `aux_hash` at slot `expected_index(20051, merkle_nonce, merkle_size)` using the exact standard Namecoin LCG formula.)*

3. **Submit winning parent shares (`submitauxblock`):**
   Whenever a miner finds a Bitcoin share meeting Newsatoshi's target (`share_hash <= aux_target`), assemble standard Namecoin-format `auxpow_hex` (`coinbase_tx || parent_block_hash || coinbase_branch || aux_branch || parent_80b_header`) and submit:
   ```bash
   curl -sS -u "$NS_RPC_AUTH" http://127.0.0.1:19556/ \
     -d '{"jsonrpc":"1.0","id":2,"method":"submitauxblock","params":["<aux_hash_be>","<auxpow_hex>"]}'
   ```

4. **BIP-320 Version Rolling Compatibility:**
   Newsatoshi reserves AuxPoW `chainid = 20051` (`0x4e53 << 16`) strictly outside Bitcoin BIP-320 version-rolling space (`0x1fffe000`) with zero collision against Namecoin (`1`), Syscoin (`16`), or Dogecoin (`98`).
