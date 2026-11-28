# x402 v2 Extension Specification: `batch-settlement` (`paywords-sha256`)

- **Status:** Live Multi-Chain Reference Implementation (`v0.35.0`)
- **Author:** Newsatoshi Commons (`https://commons.newsatoshi.org`)
- **Tracking Issue:** `https://github.com/x402-foundation/x402/issues/4494`
- **Protocol Fee:** `0 bps` (`0%` — immutable, ownerless contracts)

---

## 1. Motivation & Problem Statement

Per-call on-chain HTTP `402` schemes (`scheme: "exact"` with EIP-3009 / ERC-20 transfer per request) incur `~$0.001–$0.01` of L2 gas + RPC/facilitator latency on **every individual API call**, making sub-cent autonomous AI agent calls (`$0.0001–$0.005`) economically unviable and exposing per-request customer call graphs on-chain.

The **`batch-settlement` (`paywords-sha256`)** scheme solves this with a single pure-SHA-256 hash chain (Rivest & Shamir, 1996) settled via the EVM native `sha256` precompile (`0x02`):

- **Per-call verification cost:** **1 SHA-256 hash (`~1–2 µs`)**, **`$0.000000` gas**, zero signatures, zero RPC calls after initial channel funding check.
- **On-chain settlement cost:** **1 single transaction** settles `1..4,096` accumulated API calls (`claim(channelId, steps, w_k)`).
- **Measured Mainnet Economics:**
  - Base Mainnet `claim(1000)` (`0xb79a3c…6256`): **`770,956` gas = `$0.0077`** total incl. L1 data fee (`97.7%` cheaper than a batched `$1.00` Stripe charge at `$0.329`, and `99.99%` cheaper than per-call cards).
  - Base Mainnet live commercial API `claim(5)` (`0xd45660…ea06`): **`75,608` gas** settling 5 real `$0.001` USDC calls.
  - Optimism Mainnet `claim(100)` (`0x05f72b…ef6c`): **`125,250` gas**.

---

## 2. Live Canonical Multi-Chain Contracts (Sourcify `exact_match`, Ownerless)

| Network | CAIP-2 | `NewsatoshiPayWordChannel` | Native USDC | Sourcify Verification |
|---|---|---|---|---|
| **Base Mainnet** | `eip155:8453` | `0x971BA814Df91c20DcA3aDd870C2B55d643965C50` | `0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913` | `https://repo.sourcify.dev/8453/0x971BA814Df91c20DcA3aDd870C2B55d643965C50` |
| **Optimism Mainnet** | `eip155:10` | `0x3DeBB6d5ae490e6cF4F966585e000028111CF106` | `0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85` | `https://repo.sourcify.dev/10/0x3DeBB6d5ae490e6cF4F966585e000028111CF106` |
| **Arbitrum One** | `eip155:42161` | `0x3DeBB6d5ae490e6cF4F966585e000028111CF106` | `0xaf88d065e77c8cC2239327C5EDb3A432268e5831` | `https://repo.sourcify.dev/42161/0x3DeBB6d5ae490e6cF4F966585e000028111CF106` |

---

## 3. Cryptographic Hash-Chain Construction

1. **Payer generates a 32-byte CSPRNG secret seed** `w_N` (`N = maxSteps`, e.g., `1000`).
2. **Payer computes the reverse SHA-256 chain:**
   - `w_{i-1} = SHA256(w_i)` for `i = N, N-1, ..., 1`
   - Public commitment root: `w_0 = SHA256^N(w_N)`
3. **Channel Funding (1 on-chain tx):**
   - Payer calls `openChannel(recipient, token, w_0, unitAmount, maxSteps, expiry)` depositing `unitAmount * maxSteps`.
4. **Off-Chain Per-Call Voucher (`$0` gas):**
   - For call `k` (`1 <= k <= N`), payer reveals `w_k` in the `PAYMENT-SIGNATURE` header.
5. **Server O(1) Verification + Replay Protection:**
   - Server maintains monotonic watermark `(highestIndex, highestToken)` initialized from on-chain `(claimedSteps, currentRoot)`.
   - On receipt of `(index = k, tokenHex = w_k)`:
     - Reject if `k <= highestIndex` (strict replay protection) or `k > maxSteps` or `now + 60 >= expiry`.
     - Compute `delta = k - highestIndex` (typically `delta = 1`).
     - Verify `SHA256^delta(w_k) == highestToken`.
     - Persist `(highestIndex = k, highestToken = w_k)` atomically and serve `HTTP 200`.
6. **Batch On-Chain Settlement (1 on-chain tx):**
   - Merchant calls `claim(channelId, k - claimedSteps, w_k)`.
   - Contract verifies `SHA256^steps(w_k) == currentRoot`, advances `currentRoot = w_k` and `claimedSteps += steps`, and transfers `steps * unitAmount` strictly to `recipient` (`0%` fee).
   - Unused balance after `expiry` (or early via merchant `closeAndRefund`) returns strictly to `payer`.

---

## 4. Wire Format

### 4.1 Server Challenge (`HTTP 402 Payment Required`)
```json
{
  "x402Version": 2,
  "scheme": "batch-settlement",
  "binding": "paywords-sha256",
  "protocol_fee_bps": 0,
  "merchant_recipient": "0x1744535521EB420be9700475400455a4Ad0EBEa2",
  "price_per_call_usdc": 0.001,
  "accepted_live_channels": [
    { "chain": "base", "caip2": "eip155:8453", "contract": "0x971BA814Df91c20DcA3aDd870C2B55d643965C50", "usdc": "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913" },
    { "chain": "optimism", "caip2": "eip155:10", "contract": "0x3DeBB6d5ae490e6cF4F966585e000028111CF106", "usdc": "0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85" },
    { "chain": "arbitrum", "caip2": "eip155:42161", "contract": "0x3DeBB6d5ae490e6cF4F966585e000028111CF106", "usdc": "0xaf88d065e77c8cC2239327C5EDb3A432268e5831" }
  ]
}
```

### 4.2 Client Voucher Header (`PAYMENT-SIGNATURE`)
Base64-encoded JSON:
```json
{
  "x402Version": 2,
  "scheme": "batch-settlement",
  "binding": "paywords-sha256",
  "network": "eip155:8453",
  "channelId": "0x91fbe151d270ed3b4953c48a068321dfdcf1cc74704040267fdafea63be11153",
  "index": 1,
  "tokenHex": "0x..."
}
```

---

## 5. Formal Verification & Live Test Endpoints

- **Lean 4.36.0 Theorems (`0 sorry`):** `payword_channel_exact_conservation` and `payword_incremental_claim_strict_monotonic` in `formal/NsUsdAndL2Perfection.lean`.
- **Live Paid Production APIs:**
  - `POST https://commons.newsatoshi.org/api/v2/paid/evm-audit-batch`
  - `POST https://commons.newsatoshi.org/api/v2/paid/tx-verify-batch`
  - `POST https://commons.newsatoshi.org/api/v2/paid/defi-quote-bundle`
- **Live Public Unfunded Sandbox:** `POST https://commons.newsatoshi.org/api/v2/paywords/try-402`
- **Zero-Dependency SDKs:**
  - JS/TS: `npm install github:theguysaccount/newsatoshi-commons` (`newsatoshi-402.mjs` + `.d.ts`)
  - Python: `pip install "git+https://github.com/theguysaccount/newsatoshi-commons.git#subdirectory=sdk/python"` (`newsatoshi_402.py`)
