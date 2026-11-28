# Newsatoshi External Security Audit Scope & Reproducibility Brief

> **Purpose:** Turnkey technical scope, live contract addresses, formal verification reproduction commands, and security invariant checklist for independent external cryptographic & smart-contract security auditors (Trail of Bits, Spearbit, Zellic, NCC Group, Quarkslab, OtterSec).

---

## 1. Two Independent Audit Tracks

### Track A — Layer-1 Consensus, Pure-SHA-2 Winterfell STARKs & Post-Quantum Cryptography (`Rust` + `Lean 4`)
- **Repository:** `https://github.com/theguysaccount/newsatoshi-rfc0021` (Canonical Mainnet v2 release tag: **`v1.2.5`**)
- **Cryptographic Primitive Surface (Zero Elliptic Curves, Zero Pairings, Zero Novel Algebraic Hashes):**
  - Commitment, nullifier, Merkle tree, and STARK FRI hash: **NIST FIPS 180-4 `SHA-256` / `SHA-384`**
  - Spending signatures: **NIST FIPS 205 `SLH-DSA`** (stateless hash-based signatures)
  - P2P transport encryption only (never protecting funds): **NIST FIPS 203 `ML-KEM-768`** (`nspq2`)
- **Primary Audit Targets:**
  1. **Shielded Spend & `BatchSpend` (`K=2..16`) Winterfell AIR + FRI verifier** (`node/src/proof.rs`, `node/src/air.rs`)
  2. **9,349-line Zero-Sorry Lean 4.36.0 AIR Soundness Proof** (`formal/` — verifying exact correspondence between Rust AIR constraints and Lean 4 IFF theorems)
  3. **Consensus State Machine & Replay/Inflation Prevention** (`node/src/state.rs`, `node/src/chain.rs`): nullifier set integrity, 100-block coinbase maturity, ASERT difficulty adjustment, and **Namecoin `0xfabe6d6d` AuxPoW (`chainid=20051`)** merge-mining verification (`auxpow.rs`)
  4. **Zero-Rust Independent Browser/Node Verifier** (`verify/`): confirming identical rejection behavior on malformed proofs/blocks.

### Track B — Multi-Chain Immutable Solidity Settlement Contracts & L2 Conservation (`Solidity ^0.8.24` + `Lean 4`)
- **Repository:** `https://github.com/theguysaccount/newsatoshi-commons` (`bridges/evm/src/` + `formal/NsUsdAndL2Perfection.lean`)
- **Live Mainnet Deployed & Sourcify `exact_match` Verified Contracts (All Ownerless, Non-Upgradeable, 0% Protocol Fee):**

| Contract | Networks | Canonical Address | Sourcify Verification |
|---|---|---|---|
| `NewsatoshiPayWordChannel.sol` | Base (`8453`) | `0x971BA814Df91c20DcA3aDd870C2B55d643965C50` | `https://repo.sourcify.dev/8453/0x971BA814Df91c20DcA3aDd870C2B55d643965C50` |
| `NewsatoshiPayWordChannel.sol` | Optimism (`10`), Arbitrum One (`42161`) | `0x3DeBB6d5ae490e6cF4F966585e000028111CF106` | `https://repo.sourcify.dev/10/0x3DeBB6d5ae490e6cF4F966585e000028111CF106` |
| `NewsatoshiHtlc.sol` | Base (`8453`), Optimism (`10`), Arbitrum (`42161`), Polygon (`137`) | `0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987` | `https://repo.sourcify.dev/8453/0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987` |
| `NewsatoshiBackingVault.sol` | Base (`8453`), Optimism (`10`), Arbitrum (`42161`), Polygon (`137`) | `0x37Ed94525DBE408fA72cDd156409875666679265` | `https://repo.sourcify.dev/8453/0x37Ed94525DBE408fA72cDd156409875666679265` |
| `NewsatoshiFairCurveFactory.sol` | Base (`8453`) | `0xF6bE782819a8eD0D1556A8f5DC3223f9eb13a933` | `https://repo.sourcify.dev/8453/0xF6bE782819a8eD0D1556A8f5DC3223f9eb13a933` |

- **Primary Solidity & L2 Audit Targets:**
  1. **`NewsatoshiPayWordChannel.sol`:** Hash-chain preimage loop gas griefing bounds (`MAX_STEPS`), monotonic `claimedSteps` / `currentRoot` updates, fee-on-transfer ERC-20 rejection (`balAfter - balBefore == totalDeposit`), `expiry` refund race conditions, and reentrancy guard.
  2. **`NewsatoshiHtlc.sol`:** `sha256` precompile (`0x02`) byte-compatibility with Bitcoin `OP_SHA256` & Newsatoshi L1 `lock --hash`, `MAX_TIMELOCK_DURATION` upper bound, `swapId` domain separation (`block.chainid`, `address(this)`), and `claim` vs `refund` timestamp boundary.
  3. **`NewsatoshiFairCurveFactory.sol` / `FairCurvePool`:** Constant-product integer rounding direction (`(V + R) * Y >= k0`), `REV_PRECISION = 1e30` dividend accumulator overflow / dust griefing, `injectRevenue()` dilution-proof invariants (Lean 4 Theorems 10–11).
  4. **L2 Rollup & `nsUSD` Engine (`src/l2_rollup.rs`, `src/nsusd.rs`, `formal/NsUsdAndL2Perfection.lean`):** CVE-2012-2459 Merkle second-preimage / phantom-leaf escape-hatch protection, `asset_id=1` vs `asset_id=2` isolation, and 50/50 bonded oracle slashing.

---

## 2. 60-Second Offline Reproduction Commands

```bash
# Track A: Newsatoshi L1 v1.2.5 (Rust tests + Lean 4.36.0 zero-sorry check)
git clone --branch v1.2.5 https://github.com/theguysaccount/newsatoshi-rfc0021.git
cd newsatoshi-rfc0021/node && cargo test --release
cd ../formal && lake build   # Verifies 0 sorry across all L1 AIR theorems

# Track B: Newsatoshi Commons + Multi-Chain Solidity Foundry Suite + SDKs
git clone https://github.com/theguysaccount/newsatoshi-commons.git
cd newsatoshi-commons
cargo test && cargo clippy --all-targets -- -D warnings
cd bridges/evm && forge test -vvv
cd ../../sdk/js && node --test test_newsatoshi_402.mjs
cd ../python && python3 -m unittest discover -p "test_*.py"
cd ../../auxpow-stratum && python3 -m unittest test_stratum_auxpow.py
```

---

## 3. Security Contact & Public Bug Bounty
- **Public Bug Bounty Terms:** `https://newsatoshi.org/bounty/` (`docs/BUG_BOUNTY.md`)
- **Public 5-Round AI Adversarial Review Log:** `https://newsatoshi.org/audit-log/`
- **Disclosure / Audit Inquiries:** `security@newsatoshi.org`
