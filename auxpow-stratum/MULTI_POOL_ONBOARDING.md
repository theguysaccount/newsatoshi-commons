# Multi-Pool Bitcoin AuxPoW Onboarding Guide (`chainid = 20051`)

Mainnet v2 activation requires **≥ 3 independent Bitcoin SHA-256 mining pools** merge-mining Newsatoshi (`LOVE`, AuxPoW `chainid = 20051` / `0x4e53`) at real Bitcoin-grade difficulty so no single entity can reorganize the chain.

Every Bitcoin pool operator earns **50 LOVE/block** at **zero extra electricity or hashrate cost** by adding one `fabe6d6d` 44-byte AuxPoW commitment into their existing Bitcoin coinbase scriptSig.

## Live Cutover Gate Status

Check real-time pool diversity and audit cutover status anytime:
```bash
curl -sS https://commons.newsatoshi.org/api/v2/cutover-readiness
```

## Option A: 5-Minute Turnkey Docker Sidecar (Standalone / Solo / Public-Pool / Umbrel / Start9)

```bash
git clone https://github.com/theguysaccount/newsatoshi-commons.git
cd newsatoshi-commons/auxpow-stratum
export NS_RPC_AUTH="auxpow:$(openssl rand -hex 16)"
export NS_PAYOUT_ADDRESS="<YOUR_POOL_NS1_OR_NSD1_ADDRESS>"
docker compose up -d
```

Connect dual Bitcoin + Newsatoshi merge-mining against your pool's `bitcoind`:
```bash
python3 stratum_auxpow.py \
  --ns-rpc-url http://127.0.0.1:19556/ \
  --ns-cookie ~/.newsatoshi/mainnet/.cookie \
  --btc-rpc-url http://127.0.0.1:8332/ \
  --btc-rpc-auth "<BTC_RPC_USER>:<BTC_RPC_PASS>" \
  --btc-payout-script-hex "<YOUR_POOL_BTC_SCRIPTPUBKEY_HEX>" \
  --payout-address "<YOUR_POOL_NS1_OR_NSD1_ADDRESS>" \
  --pool-tag "/YourPoolName/"
```

## Option B: Direct Integration into Existing Bitcoin Pool Engines (CKPool / Braiins / Stratum V2 / NOMP)

Newsatoshi implements the standard Namecoin/Dogecoin Bitcoin Core AuxPoW JSON-RPC interface:

1. **Fetch work template** (`createauxblock`):
   ```bash
   curl -sS -u "$NS_RPC_AUTH" http://127.0.0.1:19556/ \
     -d '{"jsonrpc":"1.0","id":1,"method":"createauxblock","params":["<POOL_NS1_ADDRESS>"]}'
   ```
   Returns `{ "hash": "<aux_hash_be>", "chainid": 20051, "target": "<target_be>", "bits": "..." }`.

2. **Embed 44-byte commitment in Bitcoin coinbase scriptSig**:
   ```text
   fabe6d6d || reverse_bytes(aux_hash) [32B] || merkle_size (1u32 LE) || merkle_nonce (0u32 LE)
   ```
   *(If merge-mining multiple aux chains like Namecoin + Dogecoin + Newsatoshi, place `aux_hash` at slot `expected_index(20051, merkle_nonce, merkle_size)` using the exact standard Namecoin LCG formula.)*

3. **Submit winning parent shares** (`submitauxblock`):
   Whenever a miner finds a Bitcoin share meeting Newsatoshi's target (`share_hash <= aux_target`), assemble standard Namecoin-format `auxpow_hex` (`coinbase_tx || parent_block_hash || coinbase_branch || aux_branch || parent_80b_header`) and submit:
   ```bash
   curl -sS -u "$NS_RPC_AUTH" http://127.0.0.1:19556/ \
     -d '{"jsonrpc":"1.0","id":2,"method":"submitauxblock","params":["<aux_hash_be>","<auxpow_hex>"]}'
   ```

4. **BIP-9 v2 Upgrade Signaling (`RFC-0026`)**:
   Newsatoshi reserves header version bits `1..=7` (`0xfe`) strictly separate from the AuxPoW bit (`0x100`) and `chainid = 20051` (`0x4e53 << 16`). Pools can signal readiness for `RFC-0021..0027` without affecting Bitcoin BIP-320 version rolling.
