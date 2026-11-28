# Newsatoshi Mainnet v2 AuxPoW Stratum v1 Merge-Mining Kit (`v1.2.5` · `chainid = 20051`)

Turnkey multi-arch Docker (`linux/amd64` + `linux/arm64`), 1-Click Umbrel Community App Store, Start9, Public Pool Bridge, and standalone Python 3 Stratum v1 bridge enabling any Bitcoin SHA-256 ASIC (**Bitaxe**, **NerdQAxe**, **Antminer S19/S21**, **WhatsMiner**, **CGMiner**, **MiningRigRentals**) to solo-mine or dual BTC+LOVE merge-mine **Newsatoshi Mainnet v2 (`LOVE`)** at **`$0` extra electricity**.

## 1-Click Umbrel Community App Store (Home Bitaxe / NerdQAxe)

Paste into **Umbrel → App Store → Community App Stores (`⋯`)**:
```text
https://github.com/theguysaccount/newsatoshi-umbrel-store
```

## 1-Command Dual Bitcoin + Newsatoshi Merge-Mining (Prebuilt Multi-Arch GHCR Docker)

Pulls prebuilt `ghcr.io/theguysaccount/newsatoshi-node:v1.2.5` + `ghcr.io/theguysaccount/newsatoshi-auxpow-stratum:v1.2.5` (works on Raspberry Pi 5, Umbrel, Start9, Apple Silicon, and x86_64 Linux):

```bash
cd auxpow-stratum
export BTC_RPC_URL="http://host.docker.internal:8332/"
export BTC_RPC_AUTH="bitcoinrpc:<YOUR_BITCOIND_RPC_PASS>"
export BTC_PAYOUT_SCRIPT_HEX="0014<your_20byte_p2wpkh_hash>"
export NS_PAYOUT_ADDRESS="ns1<your_74char_shielded_mainnet_v2_address>"
export NS_RPC_AUTH="auxpow:$(openssl rand -hex 16)"
docker compose -f docker-compose.merge-mine.yml up -d
```

Point your **Bitaxe / NerdQAxe / Antminer / WhatsMiner** to:
- **Stratum URL:** `stratum+tcp://<YOUR_HOST>:3333`
- **Worker / Username:** `<your_74char_ns1_address>.bitaxe1` (per-miner direct coinbase payouts)
- **Password:** `x` (Bitaxe / auto-VarDiff), `d=65536` (20–100 TH/s), or `d=524288` (100–300 TH/s)

Inspect live real-time counters (strictly real zeros until miners connect):
```bash
curl http://127.0.0.1:3334/status
```

## Standalone AuxPoW Solo Mode (No Local `bitcoind` Required)

```bash
cd auxpow-stratum
export NS_RPC_AUTH="auxpow:$(openssl rand -hex 16)"
export NS_PAYOUT_ADDRESS="ns1<your_74char_shielded_mainnet_v2_address>"
docker compose up -d
```

Or mine directly on the live 0%-fee community solo pool (`stratum+tcp://pool.newsatoshi.org:3333`, status at `https://pool.newsatoshi.org/status`).

## Wire Protocol & Test Verification

- **AuxPoW Chain ID:** `20051` (`0x4e53` — `"NS"`)
- **Coinbase Tag:** `fabe6d6d || aux_block_hash_display(32B) || merkle_size(1u32 LE) || merkle_nonce(0u32 LE)`
- **BIP-320 Version Rolling:** Negotiated automatically (`0x1fffe000`) with collision guard ensuring parent header upper 16 bits never equal `0x4e53`.
- **Unit & Stratum TCP Tests (`11/11`):** `PYTHONPATH=. python3 -m unittest test_stratum_auxpow.py`
- **Additional Kits:** [`umbrel/`](./umbrel/) · [`start9/`](./start9/) · [`public-pool-bridge/`](./public-pool-bridge/) · [`ckpool-sidecar.env.example`](./ckpool-sidecar.env.example) · [`MULTI_POOL_ONBOARDING.md`](./MULTI_POOL_ONBOARDING.md) · [`POOL_OPERATOR_OUTREACH.md`](./POOL_OPERATOR_OUTREACH.md)
