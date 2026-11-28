# Independent Bitcoin AuxPoW Pool Operator Outreach Kit (`v1.2.5` · `chainid = 20051`)

> **Goal:** Onboard independent Bitcoin SHA-256 mining pools, solo-pool operators (CKPool / Public Pool), and home node platforms (Umbrel / Start9) to merge-mine **Newsatoshi Mainnet v2 (`LOVE`)** alongside Bitcoin at **`$0` extra electricity**.

---

## 1. Ready-to-Send Technical Brief (OCEAN, Braiins, Public Pool, Solo CKPool, ViaBTC, F2Pool, Foundry)

```text
Subject: Zero-Cost Bitcoin SHA-256 Merge-Mining (Standard Namecoin AuxPoW RPC, chainid=20051) — Newsatoshi Mainnet v2 (LOVE)

Hi [Pool Engineering Team],

Newsatoshi Mainnet v2 (v1.2.5) is live — fair-launched (0% premine, Bitcoin block #977,738 anchored genesis), post-quantum (NIST FIPS 205 SLH-DSA + Winterfell STARK shielded ledger), and secured by standard Namecoin-compatible Bitcoin AuxPoW.

Why it takes <15 minutes for any pool already supporting Namecoin/RSK/Elastos/Syscoin AuxPoW:
- Exact Namecoin AuxPoW wire format & JSON-RPC (`createauxblock`, `getauxblock`, `submitauxblock`).
- `chainid = 20051` (`0x4e53`, "NS") — verified outside Bitcoin BIP-320 version-rolling space with zero collision against Namecoin (1), Syscoin (16), Dogecoin (98), etc.
- Zero extra electricity and zero Bitcoin stale-rate impact: standard `0xfabe6d6d` coinbase commitment alongside your existing BTC block template.
- 50 LOVE/block Bitcoin halving schedule + permanent 0.1953125 LOVE tail security floor, 0% protocol tax, 100% paid directly to your pool payout address.
- Prebuilt multi-arch Docker images (`linux/amd64` + `linux/arm64`) — zero local Rust compile needed:
  - `ghcr.io/theguysaccount/newsatoshi-node:v1.2.5`
  - `ghcr.io/theguysaccount/newsatoshi-auxpow-stratum:v1.2.5`
- 1-Command dual BTC+LOVE Docker Compose & onboarding guide:
  https://github.com/theguysaccount/newsatoshi-commons/blob/main/auxpow-stratum/docker-compose.merge-mine.yml
  https://github.com/theguysaccount/newsatoshi-commons/blob/main/auxpow-stratum/MULTI_POOL_ONBOARDING.md
- 1-Click Umbrel Community App Store (for home Bitaxe/NerdQAxe node runners):
  https://github.com/theguysaccount/newsatoshi-umbrel-store
- Live 0%-fee reference AuxPoW pool (`stratum+tcp://pool.newsatoshi.org:3333`):
  https://pool.newsatoshi.org/status
- Full open-source L1 & formal verification audit brief (`v1.2.5` + Lean 4 zero-sorry proofs):
  https://github.com/theguysaccount/newsatoshi-commons/blob/main/docs/EXTERNAL_AUDIT_BRIEF.md

Happy to spin up a dedicated staging peer or walk through `createauxblock` with your Stratum engineer this week.

Best,
Jack / Newsatoshi Contributors
contributors@newsatoshi.org
```

---

## 2. Live On-Chain Pool & AuxPoW Verification

Run [verify-auxpow-pool.sh](./verify-auxpow-pool.sh) against any live Mainnet v2 node status endpoint to inspect live difficulty, block height, and merge-mined AuxPoW blocks directly from on-chain data:

```bash
./auxpow-stratum/verify-auxpow-pool.sh https://newsatoshi.org/mainnet/status
```
