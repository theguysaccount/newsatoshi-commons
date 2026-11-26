# Independent Bitcoin AuxPoW Pool Operator Outreach Kit (`chainid = 20051`)

> **Goal:** Onboard **≥ 3 independent Bitcoin SHA-256 mining pools** at real Bitcoin-grade difficulty (`>= 10,000`) to clear Gate 3 of Mainnet v2 activation (`GET https://commons.newsatoshi.org/api/v2/cutover-readiness`).

---

## 1. Ready-to-Send Technical Pitch (OCEAN, Braiins, ViaBTC, F2Pool, AntPool, Foundry, Public Pool / CKPool)

```text
Subject: Zero-Cost Bitcoin SHA-256 Merge-Mining Integration (Namecoin AuxPoW RPC, chainid=20051) — Newsatoshi (LOVE)

Hi [Pool Engineering Team],

We are onboarding the first 3+ independent Bitcoin SHA-256 mining pools ahead of Newsatoshi Mainnet v2 (fair launch, 0 premine, Bitcoin-block-anchored genesis, post-quantum 192-bit SHA-384 STARK privacy + native AI/clearinghouse rails).

Why it takes <15 minutes to add to any pool that already supports Namecoin/RSK/Elastos/Syscoin AuxPoW:
- Exact Namecoin AuxPoW wire format & JSON-RPC (`createauxblock`, `getauxblock`, `submitauxblock`).
- `chainid = 20051` (`0x4e53`, "NS") — verified outside Bitcoin BIP-320 version-rolling space and zero collision with Namecoin (1), Syscoin (16), Dogecoin (98), etc.
- Zero extra electricity or Bitcoin stale-rate impact: standard `0xfabe6d6d` coinbase commitment alongside your existing BTC template.
- 50 LOVE/block Bitcoin halving schedule + permanent <0.05%/yr tail security floor (never collapses to 0 subsidy), 0% protocol tax, 100% paid directly to your pool payout address.
- Ready-to-run Stratum bridge & onboarding guide:
  https://github.com/theguysaccount/newsatoshi-commons/blob/main/auxpow-stratum/MULTI_POOL_ONBOARDING.md
- Full open-source L1 & formal verification audit packet (`v0.9.0-frontier`):
  https://github.com/theguysaccount/newsatoshi-rfc0021/blob/main/docs/AUDIT_SCOPE_V2.md

Happy to spin up a dedicated staging peer or walk through `createauxblock` with your Stratum engineer this week.

Best,
Jack / Newsatoshi Contributors
```

---

## 2. Live Independent Pool Gate Verification

Run [verify-auxpow-pool.sh](./verify-auxpow-pool.sh) against any live L1 node status endpoint to inspect real on-chain difficulty and distinct merge-mined coinbase pool tags (no self-reported claims—strictly primary on-chain block data):

```bash
./auxpow-stratum/verify-auxpow-pool.sh https://newsatoshi.org/mainnet/status
```
