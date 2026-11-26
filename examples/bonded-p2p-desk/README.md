# Zero-Custody Bonded zkTLS P2P Fiat/USDC <-> LOVE Desk

Run a borderless fiat/USDC <-> native shielded `LOVE` market-making desk with zero centralized exchange custody:
- **100%+ L1 Collateral Backing:** Maker posts an `RFC-0024` Trust Bond (`stake >= max_trade`) on Newsatoshi L1.
- **Real Cryptographic Settlement:** Taker proves fiat transfer completion via `reclaim_v1`, `tlsnotary_v1`, or `newsatoshi_pq_notary_v1` selective-disclosure zkTLS (`POST /api/zktls/verify` & `POST /api/p2p/verify-settlement`).
- **Instant Slashing on Default:** Any maker equivocation or verified non-delivery slashes the maker's L1 bond to compensate the taker directly in shielded `LOVE`.

```bash
node examples/bonded-p2p-desk/index.mjs
```
