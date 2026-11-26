# Commercial Wedge Playbook — First Bonded zkTLS Makers & AI Agent Sellers

> **Honest Status:** Live counters start at genuine `0` (`GET https://commons.newsatoshi.org/api/p2p/offers` → `active_bonded_makers: 0`). This playbook is the zero-fluff 5-minute runbook for onboarding the first **real bonded zkTLS P2P market makers** and **real unsubsidized AI API / GPU inference sellers** settling natively in LOVE.

---

## Wedge 1 — First Bonded zkTLS P2P Market Makers (Fiat ↔ LOVE, Zero CEX Custody)

A market maker posts an `RFC-0024` Universal Trust Bond on L1 (150% collateral coverage) and publishes a signed offer on the Bonded P2P Desk. Buyers pay fiat via Wise / Revolut / Zelle / SEPA, prove settlement with an upstream Reclaim Protocol or PSE TLSNotary MPC-TLS receipt, and release LOVE atomically (if the maker defaults, the buyer submits the zkTLS proof to slash the maker's L1 bond and recover 100% + penalty).

### 5-Minute Maker Onboarding
1. **Verify live desk & cutover status:**
   ```bash
   curl -sS https://commons.newsatoshi.org/api/p2p/offers
   ```
2. **Compile a bonded offer (enforces `declared_bond_stake >= 150%` of `max_trade_love`):**
   ```bash
   curl -sS -X POST https://commons.newsatoshi.org/api/p2p/compile-offer \
     -H "Content-Type: application/json" \
     -d '{
       "maker_label": "Your-Desk-Name",
       "maker_bond_id_hex": "<64-HEX-L1-RFC0024-BOND-ID>",
       "maker_love_address": "<YOUR-nsd1-DELIVERY-ADDRESS>",
       "payment_rail": "wise",
       "price_fiat_cents_per_love": 50,
       "min_trade_love_base_units": 100000000,
       "max_trade_love_base_units": 500000000,
       "declared_bond_stake_base_units": 750000000,
       "expected_tls_server_name": "api.wise.com"
     }'
   ```
3. **Verify buyer's upstream MPC-TLS payment receipt (Reclaim or TLSNotary):**
   ```bash
   node zktls-prover/upstream-bridge.mjs reclaim ./buyer-reclaim-proof.json --verify-live
   # or:
   node zktls-prover/upstream-bridge.mjs tlsn ./buyer-tlsn-export.json --verify-live
   ```

---

## Wedge 2 — Unsubsidized AI API / GPU Sellers Settling via HTTP 402 PayWord Streams

AI agents cannot open Chase accounts or pass KYC, and sub-cent API calls (`\$0.0001/token`) fail on Visa/Stripe. With `RFC-0022` PayWord channels + `@newsatoshi/402`, an AI API or GPU endpoint verifies **1,000,000 sequential vouchers at ~0.009 ms per call (1 SHA-256 hash, zero per-call L1 gas)** and settles unilaterally on L1 in a single 70-byte `ChannelClaim` transaction.

### 5-Minute Seller Integration (Node.js / Express / Fastify / Next.js)
```bash
npm install github:theguysaccount/newsatoshi-commons#path:sdk/js
```
```js
import { createPayWordMiddleware } from "@newsatoshi/402";

app.use("/v1/inference", createPayWordMiddleware({
  requireFunding: true,               // checks live L1 RFC-0022 channel lock
  priceBaseUnitsPerCall: 10_000,      // 0.0001 LOVE per inference call
  sellerAddress: process.env.LOVE_SELLER_ADDR,
  l1RpcUrl: process.env.NEWSATOSHI_L1_RPC,
  watermarkPath: "./payword-watermarks.json",
}));
```

### 5-Minute Agent Buyer Integration (MCP or Python SDK)
- **Official MCP Registry (live):** `org.newsatoshi.commons/newsatoshi-commons` (`https://commons.newsatoshi.org/mcp`)
- **CLI / Zero-install:** `npx -y github:theguysaccount/newsatoshi-commons btc-fees`
- **Python SDK:** `sdk/python/newsatoshi_402.py` (`ClearinghouseClient`)
