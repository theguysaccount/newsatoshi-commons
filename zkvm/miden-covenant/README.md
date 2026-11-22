# `miden-covenant` — `NEWSATOSHI/2.1` Miden VM Native STARK Covenant Program

Native **Miden Assembly (MASM)** implementation of the `NEWSATOSHI/2.1` recursive covenant state-transition verifier for [`OP_VERIFY_COVENANT_STARK` (`0xb4`, Backend ID `0x03`)](../../docs/RFC-0021-OP_VERIFY_COVENANT_STARK.md).

Unlike RISC-V emulation zkVMs, Miden VM executes stack-based STARK constraints directly over the 64-bit Goldilocks prime field ($p = 2^{64} - 2^{32} + 1$) with built-in algebraic `u32` range proofs, `u64` arithmetic procedures, and native SHA-256 / Rescue Prime Optimized (RPO) hashing.

---

## Enforced Invariants (`covenant_transition.masm`)

| Procedure | Invariant Enforced |
|---|---|
| `verify_value_conservation` | `input_value == output_payout_value + change_value` via `u64::overflowing_add` (`carry == 0`) |
| `verify_covenant_continuity` | `input_covenant_vk_hash != 0x00..00` AND (`change_value > 0` $\implies$ `change_covenant_vk_hash == input_covenant_vk_hash`) |
| `verify_bounty_cap` | `0 < output_payout_value <= max_bounty_payout` via `u64::lte` |
| `verify_ecological_floor` | `min_soil_health_score <= soil_health_score <= 100` AND `kwh_delivered > 0` |
| `verify_xmss_watermark` | `leaf_index >= prev_watermark` AND `new_watermark == leaf_index + 1` (overflow-checked) |
| `verify_payword_reinvest_split_and_hashchain` | `gross == ticks * unit_price == input_value`, `gross * reinvest_bps == change_value * 10_000 + rem` (`0 <= rem < 10_000`), AND `H^{ticks}(final_preimage) == channel_tip` via `std::crypto::hashes::sha256::hash_1to1` |
| `fold_ivc_accumulator` | Deterministic SHA-256 IVC accumulator fold over `(prev_accumulator, prev_state_root, new_state_root, external_attestation_hash)` leaving the verified 8-limb `new_accumulator` on the output stack |

> **Soundness Guard:** Every `u64` limb pair and `u32` digest limb popped from public inputs or the prover advice tape is checked with `u32assert` / `u32assert2`, preventing Goldilocks field overflow attacks where a malicious prover injects a felt $\ge 2^{32}$.

---

## Public Input Tape & Advice Stack Schema (`inputs.json`)

Miden VM accepts up to 16 initial field elements on the public operand stack (`operand_stack`, top-of-stack last in JSON format depending on CLI version — listed below top-down) and arbitrary private witness elements on `advice_stack`:

### Public Operand Stack (16 `u32` limbs)

| Index (Top-Down) | Field | Description |
|---:|---|---|
| `0, 1` | `input_lo, input_hi` | 64-bit covenanted input LOVE (little-endian `u32` limbs) |
| `2, 3` | `payout_lo, payout_hi` | 64-bit uncovenanted payout LOVE |
| `4, 5` | `change_lo, change_hi` | 64-bit covenanted change / reinvested LOVE |
| `6, 7` | `max_bounty_lo, max_bounty_hi` | 64-bit covenant `max_bounty_payout` cap |
| `8` | `soil_health_score` | Hardware-attested soil health score (`0..=100`) |
| `9` | `min_soil_health_score` | Immutable ecological floor (`0..=100`) |
| `10, 11` | `kwh_lo, kwh_hi` | 64-bit hardware-attested renewable kWh delivered |
| `12` | `leaf_index` | Active XMSS Merkle-Lamport leaf index |
| `13` | `prev_watermark` | Prior monotonic XMSS leaf watermark |
| `14` | `new_watermark` | Post-transition monotonic XMSS leaf watermark (`leaf_index + 1`) |
| `15` | `transition_kind` | `1` = Land Deed, `2` = Energy Telemetry, `3` = PayWord Stream, `4` = Consolidation |

### Example `inputs.json` (Physical Energy Deployment, `kind = 2`)

```json
{
  "operand_stack": [
    "60000", "0",
    "15000", "0",
    "45000", "0",
    "20000", "0",
    "92", "80",
    "12500", "0",
    "0", "0", "1",
    "2"
  ],
  "advice_stack": [
    "<8 u32 limbs: input_covenant_vk_hash>",
    "<8 u32 limbs: change_covenant_vk_hash (identical to input)>",
    "<8 u32 limbs: prev_state_root>",
    "<8 u32 limbs: new_state_root>",
    "<8 u32 limbs: prev_accumulator>",
    "<8 u32 limbs: expected_new_accumulator>",
    "<8 u32 limbs: external_attestation_hash>"
  ]
}
```

When `transition_kind = 3` (PayWord Streaming Settlement), append to `advice_stack`:
1. `[ticks, max_ticks, unit_price, reinvest_bps]` (4 `u32` felts)
2. `[gross_lo, gross_hi, reinvest_lo, reinvest_hi]` (4 `u32` felts)
3. `channel_tip` (8 `u32` limbs)
4. `final_preimage` (8 `u32` limbs)
5. `rem` (1 `u32` felt: `(gross * reinvest_bps) mod 10_000`)

---

## Running, Proving & Verifying with Miden VM CLI

### 1. Install `miden-vm` CLI

```bash
cargo install miden-vm --locked
miden --version
```

### 2. Execute (Fast Dry-Run)

```bash
miden run \
  --assembly covenant_transition.masm \
  --input inputs.json \
  --num-outputs 8
```

### 3. Generate a 128-Bit Transparent STARK Proof

```bash
miden prove \
  --assembly covenant_transition.masm \
  --input inputs.json \
  --output covenant_transition.outputs \
  --proof covenant_transition.proof \
  --security 128
```

### 4. Verify the Proof (<10 ms Constant-Size Edge Verification)

```bash
# Compute the canonical program MAST root hash once and bind it as `program_vk_id`
miden compile --assembly covenant_transition.masm

# Verify proof against the compiled program hash, public inputs, and output accumulator
miden verify \
  --program-hash <PROGRAM_MAST_ROOT_HEX> \
  --input inputs.json \
  --output covenant_transition.outputs \
  --proof covenant_transition.proof
```

### 5. Recursive IVC Folding

To fold unbounded chains of transitions on Miden VM, wrap the `miden-verifier` recursive STARK verification procedure around `covenant_transition.masm` (verifying proof $P_{t-1}$ against `PROGRAM_MAST_ROOT` and enforcing `P_{t-1}.output_accumulator == P_t.prev_accumulator`), producing a single constant-size proof accepted by `OP_VERIFY_COVENANT_STARK` (`0xb4`, backend `0x03`).
