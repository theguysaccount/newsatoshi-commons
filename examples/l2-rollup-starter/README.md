# Newsatoshi Sovereign L2 Rollup Starter Kit (RFC-0023)

Launch a private, post-quantum, hyperscale L2 rollup or AI clearinghouse on Newsatoshi L1 in 1 transaction — zero trusted multisig bridge, zero L1 consensus edits, and 128-bit pure-SHA-2 Winterfell STARK settlement.

## Run Locally (Zero Dependencies)

```bash
node examples/l2-rollup-starter/index.mjs
```

## Full L1 ↔ L2 Lifecycle
1. **Launch L2 on L1 (`Deposit::RollupRegister`):** Lock `initial_tvl` shielded LOVE against your immutable `vk_hash` and `genesis_root`.
2. **Shielded L1 → L2 Deposit (`Deposit::RollupDeposit`):** Users lock private LOVE into your vault; L1 folds deposits into `deposit_acc`.
3. **Hyperscale Batch Settlement (`Action::RollupBatch`):** Settle 1,000–1,000,000 off-chain L2 transactions in a single ~75 KB 128-bit pure-SHA-2 Winterfell STARK proof (`POST /api/tx`).
4. **Trustless L2 → L1 Shielded Exit (`Action::RollupWithdraw`):** Any L2 user generates a canonical `MerkleMap` proof (`encodeRollupWithdrawTx`) and claims native shielded LOVE directly on L1 (`GET /api/rollup?id=...`).
