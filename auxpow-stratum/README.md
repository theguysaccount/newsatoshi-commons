# Newsatoshi AuxPoW Stratum v1 Merge-Mining Kit (`chainid = 20051`)

1-command Docker & standalone Python 3 Stratum v1 server enabling any Bitcoin SHA-256 ASIC (**Bitaxe**, **NerdQAxe**, **Antminer S19/S21**, **WhatsMiner**, **CGMiner**) to merge-mine **Newsatoshi (`LOVE`)** on Mainnet at zero extra electricity cost.

## 1-Command Mainnet Launch (Docker)

```bash
cd auxpow-stratum
export NS_RPC_AUTH="auxpow:$(openssl rand -hex 16)"
# Optional: specify your own shielded payout address (ns1... or nsd1...)
# export NS_PAYOUT_ADDRESS="ns1..."
docker compose up -d
```

Point your **Bitaxe / NerdQAxe / Antminer** to:
- **Stratum URL:** `stratum+tcp://<YOUR_HOST>:3333`
- **Worker:** `bitaxe.1` (any name)
- **Password:** `x`

Inspect live real-time counters (strictly real zeros until miners connect):
```bash
curl http://127.0.0.1:3334/status
```

## Dual Bitcoin + Newsatoshi Merge-Mining (Zero Extra Energy)

Connect your existing `bitcoind` (Umbrel / Start9 / Bitcoin Core) so every SHA-256 hash mines **both** Bitcoin (`submitblock`) and Newsatoshi (`submitauxblock`) simultaneously:

```bash
python3 stratum_auxpow.py \
  --ns-rpc-url http://127.0.0.1:19556/ \
  --ns-cookie ~/.newsatoshi/mainnet/.cookie \
  --btc-rpc-url http://127.0.0.1:8332/ \
  --btc-rpc-auth "btcuser:btcpass" \
  --btc-payout-script-hex "<your_btc_scriptpubkey_hex>" \
  --payout-address "<your_ns1_or_nsd1_address>"
```

## Wire Protocol Verification

- **AuxPoW Chain ID:** `20051` (`0x4e53` — `"NS"`)
- **Coinbase Tag:** `fabe6d6d || aux_block_hash_display(32B) || merkle_size(1u32 LE) || merkle_nonce(0u32 LE)`
- **BIP-320 Version Rolling:** Negotiated automatically (`0x1fffe000`) with collision guard ensuring parent header upper 16 bits never equal `0x4e53`.
- **Unit & Stratum TCP Tests:** `python3 -m unittest test_stratum_auxpow.py`
