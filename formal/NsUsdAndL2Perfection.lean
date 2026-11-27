/-!
# RFC-0028 `nsUSD` Solvency & L2 Perfection Layer Formal Kernel (`formal/NsUsdAndL2Perfection.lean`)

Zero-dependency, zero-`sorry`, zero-`native_decide`, zero-custom-axiom Lean 4.36 machine-checked
soundness kernel for the exact integer arithmetic in:
  - `src/nsusd.rs` (`NsUsdEngine`: Sovereign BTC CDP `asset_id = 1`, Liquity Stability Pool,
    `$1.00` direct collateral redemption, `PriorLock` 50/50 slashable median oracle, and strictly
    isolated Bridged `nsUSD-B` `asset_id = 2`)
  - `src/l2_rollup.rs` (Hourglass L2 Shielded Rollup batch value conservation, PayWord hash-chain
    monotonicity, and deterministic unilateral SHA-256 Merkle L1 Escape Hatch)

Verify locally in < 1 second with stock Lean 4:
  `lean formal/NsUsdAndL2Perfection.lean`
-/

namespace Newsatoshi.NsUsdL2

/-- Basis-point denominator (`100.00% = 10_000 bps`). -/
def BPS : Nat := 10000

/-- Minimum Collateral Ratio for Sovereign `nsUSD` minting (`150.00%`). -/
def MCR_BPS : Nat := 15000

/-- Liquidation threshold (`120.00%`). -/
def LIQ_BPS : Nat := 12000

/-- Base direct `$1.00` redemption fee (`0.50% = 50 bps`). -/
def REDEMPTION_FEE_BPS : Nat := 50

/-- Liquidation keeper stipend (`0.50% = 50 bps`). -/
def KEEPER_STIPEND_BPS : Nat := 50

/-- Exact integer BTC value in micro-USD (`src/nsusd.rs::collateral_value_micro_usd`). -/
def collateralValueMicroUsd (btcSats btcUsdCents : Nat) : Nat :=
  (btcSats * btcUsdCents) / BPS

/-- Exact integer Collateral Ratio in bps (`src/nsusd.rs::vault_cr_bps`, for `debt > 0`). -/
def vaultCrBps (btcSats debtMicro btcUsdCents : Nat) : Nat :=
  (collateralValueMicroUsd btcSats btcUsdCents * BPS) / debtMicro

/-!
## Theorem 1: Liquidation Collateral Conservation (`src/nsusd.rs::liquidate_underwater_vault`)

When an underwater vault with `col` BTC satoshis is liquidated, the keeper receives
`stipend = floor(col * 50 / 10000)` and the Stability Pool receives `spGain = col - stipend`.
We prove `stipend ≤ col` and `spGain + stipend = col` (zero satoshis created or lost).
-/

theorem keeper_stipend_le_collateral (col : Nat) :
    (col * KEEPER_STIPEND_BPS) / BPS ≤ col := by
  unfold KEEPER_STIPEND_BPS BPS
  omega

theorem nsusd_liquidation_conserves_btc (col : Nat) :
    let stipend := (col * KEEPER_STIPEND_BPS) / BPS
    let spGain := col - stipend
    spGain + stipend = col := by
  intro stipend spGain
  have hle : stipend ≤ col := keeper_stipend_le_collateral col
  omega

/-!
## Theorem 2: Direct `$1.00` Redemption BTC Conservation (`src/nsusd.rs::redeem_nsusd_for_btc`)

When a redeemer burns `nsUSD` for `grossSats` BTC satoshis, the protocol retains
`fee = floor(grossSats * 50 / 10000)` and delivers `net = grossSats - fee`.
We prove `fee ≤ grossSats` and `net + fee = grossSats` (exact satoshi conservation).
-/

theorem redemption_fee_le_gross (grossSats : Nat) :
    (grossSats * REDEMPTION_FEE_BPS) / BPS ≤ grossSats := by
  unfold REDEMPTION_FEE_BPS BPS
  omega

theorem nsusd_redemption_conserves_btc (grossSats : Nat) :
    let fee := (grossSats * REDEMPTION_FEE_BPS) / BPS
    let net := grossSats - fee
    net + fee = grossSats := by
  intro fee net
  have hle : fee ≤ grossSats := redemption_fee_le_gross grossSats
  omega

/-!
## Theorem 3: `PriorLock` Oracle Slashing Exact 50/50 Conservation (`src/nsusd.rs::challenge_oracle_outlier`)

When an outlier oracle reporter's `bond` (in `LOVE` satoshis) is slashed, the challenger receives
`bounty = bond / 2` and `burn = bond - bounty` is permanently destroyed.
We prove:
  1. `bounty + burn = bond` (zero `LOVE` created or leaked),
  2. `burn ≥ bounty` (odd-satoshi rounding strictly favors permanent deflationary burn), and
  3. `burn - bounty ≤ 1` (exact 50/50 split within 1 base unit).
-/

theorem priorlock_oracle_slash_50_50_exact (bond : Nat) :
    let bounty := bond / 2
    let burn := bond - bounty
    bounty + burn = bond ∧ burn ≥ bounty ∧ burn - bounty ≤ 1 := by
  intro bounty burn
  omega

/-!
## Theorem 4: Strict Dual-Tier Asset Isolation (`asset_id = 1` Sovereign vs `asset_id = 2` Bridged)

Recording, minting, or redeeming Tier-1B bridged `nsUSD-B` (`asset_id = 2`, USDC/USDS/DAI escrow)
strictly preserves Tier-2 Sovereign `nsUSD` (`asset_id = 1`) BTC collateral, sovereign debt,
and Stability Pool balances — preventing any corporate bridge freeze from corrupting sovereign solvency.
-/

structure DualTierState where
  sovereignBtcSats : Nat
  sovereignDebtMicro : Nat
  stabilityPoolNsUsdMicro : Nat
  bridgedNsUsdBMicro : Nat

def recordBridgedMint (st : DualTierState) (deltaMicro : Nat) : DualTierState :=
  { st with bridgedNsUsdBMicro := st.bridgedNsUsdBMicro + deltaMicro }

def recordBridgedBurn (st : DualTierState) (deltaMicro : Nat) : DualTierState :=
  { st with bridgedNsUsdBMicro := st.bridgedNsUsdBMicro - deltaMicro }

theorem sovereign_isolated_from_bridged_mint (st : DualTierState) (delta : Nat) :
    let st' := recordBridgedMint st delta
    st'.sovereignBtcSats = st.sovereignBtcSats ∧
    st'.sovereignDebtMicro = st.sovereignDebtMicro ∧
    st'.stabilityPoolNsUsdMicro = st.stabilityPoolNsUsdMicro := by
  simp [recordBridgedMint]

theorem sovereign_isolated_from_bridged_burn (st : DualTierState) (delta : Nat) :
    let st' := recordBridgedBurn st delta
    st'.sovereignBtcSats = st.sovereignBtcSats ∧
    st'.sovereignDebtMicro = st.sovereignDebtMicro ∧
    st'.stabilityPoolNsUsdMicro = st.stabilityPoolNsUsdMicro := by
  simp [recordBridgedBurn]

/-!
## Theorem 5: Sovereign Opening Solvency Implies Strict Over-Collateralization (`> 100%`)

Any sovereign vault admitted under `vaultCrBps btcSats debtMicro price ≥ MCR_BPS` (`150%`)
satisfies `collateralValueMicroUsd btcSats price ≥ debtMicro` (in fact `≥ 150%` of debt),
ruling out under-collateralized or unbacked algorithmic issuance at genesis of every vault.
-/

theorem admitted_vault_strictly_overcollateralized
    (btcSats debtMicro price : Nat)
    (_hDebt : debtMicro > 0)
    (hMcr : vaultCrBps btcSats debtMicro price ≥ MCR_BPS) :
    collateralValueMicroUsd btcSats price * 2 ≥ debtMicro * 3 := by
  unfold vaultCrBps MCR_BPS BPS at hMcr
  have h1 : 15000 * debtMicro ≤ (collateralValueMicroUsd btcSats price * 10000) := by
    have hmul := Nat.mul_le_mul_right debtMicro hMcr
    have hdiv := Nat.div_mul_le_self (collateralValueMicroUsd btcSats price * 10000) debtMicro
    exact Nat.le_trans hmul hdiv
  omega

/-!
## Theorem 6: L2 Shielded Rollup Batch Value Conservation (`src/l2_rollup.rs::run_live_l2_batch_benchmark`)

Across an arbitrary batch of `n` L2 PayWord + `nsUSD` micro-transfers of `unitAmt` where each
transfer splits `unitAmt = merchantNet + fee` (`fee ≤ unitAmt`), total value across
`(payer, merchant, sequencerPool)` is strictly invariant for all `n : Nat`.
-/

structure L2BatchBalances where
  payer : Nat
  merchant : Nat
  feePool : Nat

def l2Total (b : L2BatchBalances) : Nat :=
  b.payer + b.merchant + b.feePool

def applySingleL2Transfer (b : L2BatchBalances) (merchantNet fee : Nat) : L2BatchBalances :=
  { payer := b.payer - (merchantNet + fee)
  , merchant := b.merchant + merchantNet
  , feePool := b.feePool + fee }

def applyL2Batch (b : L2BatchBalances) (merchantNet fee : Nat) : Nat → L2BatchBalances
  | 0 => b
  | n + 1 => applySingleL2Transfer (applyL2Batch b merchantNet fee n) merchantNet fee

theorem l2_batch_payer_balance (b : L2BatchBalances) (merchantNet fee n : Nat) :
    (applyL2Batch b merchantNet fee n).payer = b.payer - n * (merchantNet + fee) := by
  induction n with
  | zero => simp [applyL2Batch]
  | succ k ih =>
      simp only [applyL2Batch, applySingleL2Transfer, ih, Nat.succ_mul]
      omega

theorem l2_batch_conservation_exact
    (b : L2BatchBalances) (merchantNet fee n : Nat)
    (hSufficient : n * (merchantNet + fee) ≤ b.payer) :
    l2Total (applyL2Batch b merchantNet fee n) = l2Total b := by
  induction n with
  | zero => simp [applyL2Batch, l2Total]
  | succ k ih =>
      have hk : k * (merchantNet + fee) ≤ b.payer := by
        have hstep : k * (merchantNet + fee) ≤ (k + 1) * (merchantNet + fee) := by
          simp only [Nat.succ_mul]
          omega
        exact Nat.le_trans hstep hSufficient
      have hPrev := ih hk
      have hPayerPrev := l2_batch_payer_balance b merchantNet fee k
      have hCanPay : merchantNet + fee ≤ (applyL2Batch b merchantNet fee k).payer := by
        rw [hPayerPrev]
        simp only [Nat.succ_mul] at hSufficient
        omega
      simp only [applyL2Batch, applySingleL2Transfer, l2Total] at *
      omega

/-!
## Theorem 7: Unilateral L1 Escape Hatch Merkle Verification Determinism (`src/l2_rollup.rs::verify_l2_escape_proof`)

Given any deterministic node-compression function `nodeHash : α → α → α` (instantiated with
domain-separated `SHA-256("nsL2NODE1" || left || right)` in Rust), unilateral L1 escape-hatch
verification is pure, total, and invariant under identical Merkle paths — and if `nodeHash` is
collision-resistant (injective on distinct pairs), no two distinct leaf digests can produce the
same root at the same leaf index and sibling path.
-/

def verifyEscapeRoot {α : Type} (nodeHash : α → α → α) : α → Nat → List α → α
  | cur, _, [] => cur
  | cur, idx, sib :: rest =>
      let next := if idx % 2 = 0 then nodeHash cur sib else nodeHash sib cur
      verifyEscapeRoot nodeHash next (idx / 2) rest

theorem l2_escape_hatch_leaf_binding
    {α : Type} (nodeHash : α → α → α)
    (hCR : ∀ x₁ y₁ x₂ y₂, nodeHash x₁ y₁ = nodeHash x₂ y₂ → x₁ = x₂ ∧ y₁ = y₂)
    (siblings : List α) :
    ∀ (leaf₁ leaf₂ : α) (idx : Nat),
      verifyEscapeRoot nodeHash leaf₁ idx siblings = verifyEscapeRoot nodeHash leaf₂ idx siblings →
      leaf₁ = leaf₂ := by
  induction siblings with
  | nil =>
      intro leaf₁ leaf₂ idx h
      simpa [verifyEscapeRoot] using h
  | cons sib rest ih =>
      intro leaf₁ leaf₂ idx h
      simp only [verifyEscapeRoot] at h
      have hNext := ih _ _ (idx / 2) h
      by_cases heven : idx % 2 = 0
      · simp only [heven, ↓reduceIte] at hNext
        exact (hCR leaf₁ sib leaf₂ sib hNext).1
      · simp only [heven, ↓reduceIte] at hNext
        exact (hCR sib leaf₁ sib leaf₂ hNext).2

/-!
## Theorem 8: Escape-Hatch Index Exhaustion Prevents `idx + k * 2^depth` Aliasing (`src/l2_rollup.rs::verify_l2_escape_proof`)

When `verify_l2_escape_proof` consumes `d = siblings.length` bits by dividing `idx` by `2`
at each level and requires `idx / 2^d = 0`, we prove `idx < 2^d` — ruling out every aliased
index `idx + k * 2^d` (`k ≥ 1`) for the same Merkle sibling path.
-/

theorem index_exhausted_implies_lt_pow2 (idx d : Nat) (h : idx / 2^d = 0) :
    idx < 2^d := by
  have hpos : 0 < 2^d := Nat.pow_pos (by decide : 0 < 2)
  cases Nat.lt_or_ge idx (2^d) with
  | inl hlt => exact hlt
  | inr hle =>
      have hdiv : 1 ≤ idx / 2^d := by
        rw [Nat.le_div_iff_mul_le hpos]
        omega
      omega

theorem aliased_index_rejected (idx k d : Nat) (hk : k ≥ 1) :
    (idx + k * 2^d) / 2^d ≠ 0 := by
  have hge : 2^d ≤ idx + k * 2^d := by
    have h1 : 1 * 2^d ≤ k * 2^d := Nat.mul_le_mul_right (2^d) hk
    omega
  intro hzero
  have hlt := index_exhausted_implies_lt_pow2 (idx + k * 2^d) d hzero
  omega

end Newsatoshi.NsUsdL2
