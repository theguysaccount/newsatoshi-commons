---
name: newsatoshi-bitcoin-auxpow
description: Use when deploying or configuring Bitcoin SHA-256 AuxPoW merge-mining for Newsatoshi Mainnet v2 (LOVE, v1.2.5, chainid=20051) on Umbrel OS, Start9, Bitaxe/NerdQAxe, Antminer/WhatsMiner, Public Pool, CKPool, or enterprise Bitcoin mining pools at $0 extra electricity.
---

# Newsatoshi Mainnet v2 (`v1.2.5`) — Bitcoin SHA-256 AuxPoW Merge-Mining (`chainid = 20051`)

## When to use this skill
Use whenever a user, home Bitcoin node runner (Umbrel / Start9 / Bitaxe / NerdQAxe), or enterprise SHA-256 pool wants to earn **50 LOVE/block** (`0%` premine, `0%` protocol tax, Bitcoin halving schedule + `0.1953125 LOVE` perpetual tail floor) alongside Bitcoin at **$0 extra electricity** using standard Namecoin `0xfabe6d6d` AuxPoW (`chainid = 20051` / `0x4e53`).

## 1. Fastest Path: 1-Click Umbrel Community App Store (Home Bitaxe / NerdQAxe)
Paste into **Umbrel → App Store → Community App Stores (`⋯`)**:
```text
https://github.com/theguysaccount/newsatoshi-umbrel-store
```
Install **Newsatoshi BTC+LOVE AuxPoW Merge-Miner** — auto-wires to Umbrel's `bitcoind` RPC and exposes `stratum+tcp://umbrel.local:3333`.

## 2. Prebuilt Multi-Arch Docker Images (`linux/amd64` + `linux/arm64` — Zero Rust Compile)
Public anonymous pull from GHCR:
- `ghcr.io/theguysaccount/newsatoshi-node:v1.2.5`
- `ghcr.io/theguysaccount/newsatoshi-auxpow-stratum:v1.2.5`

1-command dual `BTC + LOVE` merge-mining against any `bitcoind`:
```bash
git clone https://github.com/theguysaccount/newsatoshi-commons.git
cd newsatoshi-commons/auxpow-stratum
export BTC_RPC_URL="http://host.docker.internal:8332/"
export BTC_RPC_AUTH="bitcoinrpc:<BITCOIND_RPC_PASS>"
export BTC_PAYOUT_SCRIPT_HEX="0014<your_20byte_p2wpkh_hash>"
export NS_PAYOUT_ADDRESS="ns1<your_74char_shielded_mainnet_v2_address>"
export NS_RPC_AUTH="auxpow:$(openssl rand -hex 16)"
docker compose -f docker-compose.merge-mine.yml up -d
```

## 3. Direct 0%-Fee Solo Pool (Bitaxe, NerdQAxe, Antminer, WhatsMiner, MiningRigRentals)
- **Stratum URL:** `stratum+tcp://pool.newsatoshi.org:3333` (BIP-320 AsicBoost + integer VarDiff)
- **Worker / Username:** `<your_74char_ns1_address>.bitaxe1` (CRITICAL: use 74-char `ns1...`, NOT 1,800-char `nsd1...`)
- **Password:** `x` (Bitaxe / auto-VarDiff), `d=65536` (20–100 TH/s), or `d=524288` (100–300 TH/s Antminer S21 / WhatsMiner / MRR)
- **Live Status:** `https://pool.newsatoshi.org/status`

## 4. Enterprise Bitcoin Pool JSON-RPC Integration (`createauxblock` / `submitauxblock`)
Standard Namecoin/Dogecoin AuxPoW RPC interface on `127.0.0.1:19556`:
- `createauxblock <74char_ns1_address>` → returns `{ hash, chainid: 20051, target, bits }`
- Embed 44-byte `fabe6d6d || reverse_bytes(aux_hash) || merkle_size(1u32 LE) || merkle_nonce(0u32 LE)` in Bitcoin coinbase `scriptSig`
- `submitauxblock <aux_hash_be> <auxpow_hex>` whenever a BTC share meets Newsatoshi's target
- Full onboarding guide: `https://github.com/theguysaccount/newsatoshi-commons/blob/main/auxpow-stratum/MULTI_POOL_ONBOARDING.md`
- Additional kits: `auxpow-stratum/start9/`, `auxpow-stratum/public-pool-bridge/`, `auxpow-stratum/ckpool-sidecar.env.example`
