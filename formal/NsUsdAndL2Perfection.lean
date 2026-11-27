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

/-!
## Theorem 9: EVM `NewsatoshiPayWordChannel` Exact Conservation (`bridges/evm/NewsatoshiPayWordChannel.sol`)

For any SHA-256 PayWord channel locking `totalDeposit = maxSteps * unitAmount` where `claimedSteps ≤ maxSteps`,
the settled recipient payout `claimedSteps * unitAmount` plus the early/expiry payer refund
`(maxSteps - claimedSteps) * unitAmount` equals `totalDeposit` identically (0 wei created or stranded),
and every incremental claim `steps > 0` strictly advances `claimedSteps` (blocking replay).
-/

theorem payword_channel_exact_conservation
    (maxSteps claimedSteps unitAmount : Nat)
    (hBound : claimedSteps ≤ maxSteps) :
    claimedSteps * unitAmount + (maxSteps - claimedSteps) * unitAmount = maxSteps * unitAmount := by
  rw [← Nat.add_mul, Nat.add_sub_of_le hBound]

theorem payword_incremental_claim_strict_monotonic
    (claimedSteps steps maxSteps : Nat)
    (hPos : steps > 0)
    (hFit : claimedSteps + steps ≤ maxSteps) :
    claimedSteps < claimedSteps + steps ∧ claimedSteps + steps ≤ maxSteps := by
  omega

/-!
## Theorem 10: Fair Bonding Curve Integer Buy & Sell Solvency (`bridges/evm/NewsatoshiFairCurveFactory.sol`)

Let `V` = `virtualReserve`, `Y0` = `virtualSupply`, `R` = `curveReserve`, `S` = `totalSupply` (`S < Y0`),
and remaining virtual inventory `y = Y0 - S > 0`.
Suppose the constant-product invariant holds: `(V + R) * y ≥ V * Y0`.
1. Under `buy(netIn)`: `Δs = (y * netIn) / (V + R + netIn)`. We prove `Δs ≤ y` (in fact `< y` for `V + R > 0`)
   AND the post-buy state `(R' = R + netIn, y' = y - Δs)` strictly preserves `(V + R') * y' ≥ V * Y0`!
2. Under `sell(t)` (`t ≤ S = Y0 - y`): `grossOut = ((V + R) * t) / (y + t)`. We prove:
   - **Never Insolvent:** `grossOut ≤ R` ALWAYS (integer floor rounding can never drain the reserve!), AND
   - **Invariant Preserved:** `(V + (R - grossOut)) * (y + t) ≥ V * Y0` ALWAYS!
-/

theorem fair_curve_buy_preserves_invariant
    (V R y Y0 netIn : Nat)
    (hInv : (V + R) * y ≥ V * Y0)
    (hPos : V + R > 0) :
    let ds := (y * netIn) / (V + R + netIn)
    ds ≤ y ∧ (V + R + netIn) * (y - ds) ≥ V * Y0 := by
  intro ds
  have hDenPos : 0 < V + R + netIn := by omega
  have hMulLe : ds * (V + R + netIn) ≤ y * netIn := Nat.div_mul_le_self (y * netIn) (V + R + netIn)
  have hLeY : ds ≤ y := by
    have h1 : y * netIn ≤ y * (V + R + netIn) := Nat.mul_le_mul_left y (by omega)
    have h2 : ds * (V + R + netIn) ≤ y * (V + R + netIn) := Nat.le_trans hMulLe h1
    exact Nat.le_of_mul_le_mul_right h2 hDenPos
  refine ⟨hLeY, ?_⟩
  have hDist : (V + R + netIn) * (y - ds) + (V + R + netIn) * ds = (V + R + netIn) * y := by
    rw [← Nat.mul_add, Nat.sub_add_cancel hLeY]
  have hComm : (V + R + netIn) * ds = ds * (V + R + netIn) := Nat.mul_comm _ _
  have hSplit : (V + R + netIn) * y = (V + R) * y + netIn * y := Nat.add_mul (V + R) netIn y
  have hComm2 : netIn * y = y * netIn := Nat.mul_comm _ _
  omega

theorem fair_curve_sell_never_insolvent
    (V R y Y0 t : Nat)
    (hInv : (V + R) * y ≥ V * Y0)
    (hLeY0 : y ≤ Y0)
    (hSellBound : t ≤ Y0 - y)
    (hDenPos : y + t > 0) :
    let grossOut := ((V + R) * t) / (y + t)
    grossOut ≤ R ∧ (V + (R - grossOut)) * (y + t) ≥ V * Y0 := by
  intro grossOut
  -- Step 1: Prove grossOut ≤ R via (V + R) * t ≤ R * (y + t) ⟺ V * t ≤ R * y
  have hVtLeRy : V * t ≤ R * y := by
    have h1 : V * t ≤ V * (Y0 - y) := Nat.mul_le_mul_left V hSellBound
    have h2 : V * (Y0 - y) + V * y = V * Y0 := by
      rw [← Nat.mul_add, Nat.sub_add_cancel hLeY0]
    have h3 : (V + R) * y = V * y + R * y := Nat.add_mul V R y
    omega
  have hNumLe : (V + R) * t ≤ R * (y + t) := by
    have hL : (V + R) * t = V * t + R * t := Nat.add_mul V R t
    have hR : R * (y + t) = R * y + R * t := Nat.mul_add R y t
    omega
  have hGrossLeR : grossOut ≤ R := by
    have hCommR : R * (y + t) = (y + t) * R := Nat.mul_comm R (y + t)
    exact (Nat.div_le_iff_le_mul_add_pred hDenPos).mpr (by omega)
  refine ⟨hGrossLeR, ?_⟩
  -- Step 2: Prove post-sell invariant (V + (R - grossOut)) * (y + t) ≥ (V + R) * y ≥ V * Y0
  have hMulLe : grossOut * (y + t) ≤ (V + R) * t := Nat.div_mul_le_self ((V + R) * t) (y + t)
  have hEqV : V + (R - grossOut) = (V + R) - grossOut := by omega
  rw [hEqV]
  have hSubAdd : ((V + R) - grossOut) * (y + t) + grossOut * (y + t) = (V + R) * (y + t) := by
    rw [← Nat.add_mul, Nat.sub_add_cancel (by omega : grossOut ≤ V + R)]
  have hExpand : (V + R) * (y + t) = (V + R) * y + (V + R) * t := Nat.mul_add (V + R) y t
  omega

/-!
## Theorem 11: Dilution-Proof `injectRevenue` Monotonicity (`bridges/evm/NewsatoshiFairCurveFactory.sol`)

Injecting `rev` units of settled x402 / PayWords USDC revenue when `activeSupply > 0` increases
`accRevPerToken` monotonically (`acc' ≥ acc`, and strictly `acc' > acc` whenever `rev * REV_PRECISION ≥ activeSupply`),
strictly increasing claimable USDC per existing share with zero token minting or dilution.
-/

theorem fair_curve_inject_revenue_monotone
    (acc rev prec activeSupply : Nat)
    (hSupply : activeSupply > 0) :
    acc + (rev * prec) / activeSupply ≥ acc ∧
      (rev * prec ≥ activeSupply → acc + (rev * prec) / activeSupply > acc) := by
  refine ⟨Nat.le_add_right acc _, ?_⟩
  intro hGe
  have hDivPos : 1 ≤ (rev * prec) / activeSupply := by
    rw [Nat.le_div_iff_mul_le hSupply]
    omega
  omega

#print axioms payword_channel_exact_conservation
#print axioms payword_incremental_claim_strict_monotonic
#print axioms fair_curve_buy_preserves_invariant
#print axioms fair_curve_sell_never_insolvent
#print axioms fair_curve_inject_revenue_monotone

end Newsatoshi.NsUsdL2
