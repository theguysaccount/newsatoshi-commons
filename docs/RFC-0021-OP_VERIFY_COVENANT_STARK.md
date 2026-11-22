```text
RFC:            0021
Title:          OP_VERIFY_COVENANT_STARK & Recursive Shielded Covenants for NEWSATOSHI/2.1
Authors:        Newsatoshi Commons Protocol Working Group
Status:         Draft / Proposed Soft Fork
Category:       Consensus (Soft Fork — OP_NOP5 Redefinition)
Protocol:       NEWSATOSHI/2.1 (extending newsatoshi v0.1.0)
AuxPoW ChainID: 20051
Mainnet Genesis:000000006ba30cc3c86ecaabcb962287e9d5fd0877c27f4b33661b4d1d3033f7 (P2P :8444)
Testnet Genesis:00000fd814d823a0bc64a84135868402998cff81e28a85b2bede5f1125d0a394 (P2P :18444)
Live Testnet:   https://newsatoshi-seed.fly.dev/status
License:        MIT OR Apache-2.0
```

# RFC-0021: `OP_VERIFY_COVENANT_STARK` (`0xb4`) & Recursive Shielded Covenants

The key words **"MUST"**, **"MUST NOT"**, **"REQUIRED"**, **"SHALL"**, **"SHALL NOT"**, **"SHOULD"**, **"SHOULD NOT"**, **"RECOMMENDED"**, **"MAY"**, and **"OPTIONAL"** in this document are to be interpreted as described in RFC 2119.

---

## 1. Abstract

This RFC specifies **`NEWSATOSHI/2.1`**, a minimal, backwards-compatible soft-fork extension to the **`newsatoshi` v0.1.0** Layer-1 protocol (`LOVE`). It introduces two tightly scoped protocol upgrades:

1. **Shielded Note Covenant Commitment Extension:** Extends the shielded note commitment preimage to include a 32-byte `covenant_vk_hash` field (`0x00..00` for standard private `LOVE` notes), cryptographically binding covenanted UTXO notes to an immutable verification predicate without altering base-layer monetary issuance or note privacy for ordinary transfers.
2. **`OP_VERIFY_COVENANT_STARK` (`0xb4`):** Redefines the unused `OP_NOP5` opcode to verify a constant-size, post-quantum, transparent recursive STARK proof over an 8-element canonical public input vector, enforcing strict value conservation and **recursive change continuity** (`output_change.covenant_vk_hash == input.covenant_vk_hash`).

Together, these primitives enable **Autonomous Abundance Loops**—self-compounding physical Commons treasuries that acquire perpetual land title, enforce hardware-attested renewable energy and soil-health floors, and stream PayWord micropayment spreads back into acquiring adjacent acreage—while preserving **complete L1 blindness and ossification** over Newsatoshi's 50 LOVE/block SHA-256 merged-mined (`chainid = 20051`) base layer.

---

## 2. Motivation & Newsatoshi v0.1.0 Baseline

### 2.1 Verified Live L1 Ground Truth (`newsatoshi v0.1.0`)

`NEWSATOSHI/2.1` builds directly upon the live `newsatoshi v0.1.0` base layer without replacing any existing consensus rule:

| Parameter / Primitive | Mainnet (`v0.1.0`) | Testnet (`v0.1.0`, Live on Fly.io) |
|---|---|---|
| **Genesis Hash** | `000000006ba30cc3c86ecaabcb962287e9d5fd0877c27f4b33661b4d1d3033f7` | `00000fd814d823a0bc64a84135868402998cff81e28a85b2bede5f1125d0a394` |
| **P2P Port** | `8444` | `18444` |
| **Status / Seed Endpoint** | `https://newsatoshi.org/status` | `https://newsatoshi-seed.fly.dev/status` (`height > 21,180`, faucet active) |
| **Block Issuance** | `50 LOVE` (`5,000,000,000` base units), not self-mined | `50 LOVE`, testnet difficulty |
| **Consensus / Ordering** | SHA-256 AuxPoW merged mining (`chainid = 20051`) | SHA-256 AuxPoW (`chainid = 20051`) |
| **Shielded Notes** | `ns1` / `nsd1` shielded note pool + Winterfell 0.13 proofs | Same (`proofs = "test"` / production STARKs) |
| **Post-Quantum Signatures** | RustCrypto NIST FIPS 205 SLH-DSA (`signing-key`) | Same |
| **Programmable Primitives** | SHA-256 hash locks (`lock`/`unlock`), conditions (`cond`/`cond-sign`/`fulfil`/`program`), Freivalds checked-work `job` escrow | Same |

### 2.2 The Gap: Persistent State & Recursive Treasury Constraints

`newsatoshi v0.1.0` already provides shielded notes, SHA-256 hash locks (`lock`), SLH-DSA programmable conditions (`cond`), and single-shot matrix-work escrow (`job`). However, autonomous real-world Commons treasuries require an invariant that stateless spending scripts cannot enforce alone: **recursive output continuity across unbounded future spends**.

Without recursive covenant enforcement:
- A human trustee, multisig quorum, or compromised operator authorized to disburse a 15,000 LOVE deployment bounty from a 100,000 LOVE treasury UTXO can redirect the 85,000 LOVE change output to a private address, **stripping the covenant** and liquidating the Commons.
- Verifying cumulative physical history (monotonic XMSS leaf watermarks, ecological floor compliance across years of telemetry, and PayWord reinvestment ratios) would otherwise require L1 nodes to store unbounded off-chain state or rely on fallible human governance.

`OP_VERIFY_COVENANT_STARK` solves both problems cleanly: all complex application state, XMSS Merkle paths, PayWord hash chains, and soil-health predicates remain off-chain inside a client-side validated (CSV) zkVM guest, while L1 verifies a **single constant-size STARK proof** and enforces **one 32-byte equality check** on the change output commitment.

---

## 3. Non-Negotiable Design Invariants

Every implementation of `NEWSATOSHI/2.1` MUST preserve four foundational invariants:

### 3.1 Invariant 1 — L1 Blindness & Ossification (Anti-Budish / Anti-Sybil Axiom)

1. **Zero Coupling Between Issuance and Physical Work:** Newsatoshi L1 MUST NEVER mint `LOVE` block subsidies in exchange for solar telemetry, soil readings, land deeds, or any real-world oracle attestation. Block issuance MUST remain strictly bounded to the ossified 50 LOVE/block SHA-256 merged-mining schedule (`chainid = 20051`).
2. **Economic Proof (Budish 2018 & Wash-Farming Collapse):** As demonstrated by Budish (2018), PoW consensus security requires that the cost of a rented-hashpower 51% double-spend exceed extractable attack profit ($P_{\text{attack}} < C_{\text{rent}}$). If L1 minted currency directly for physical sensor reports ("Proof of Solar/Soil"), adversaries would either (a) rent SHA-256 hashpower on spot markets to reorg blocks and steal minted physical rewards, or (b) perform Sybil wash-farming by feeding closed-loop electrical heaters into compromised smart meters to print infinite base money—destroying `LOVE` monetary neutrality.
3. **Strict Separation of Layers:** All physical-world capital flows operate **strictly over pre-existing, already-mined `LOVE` UTXOs** locked voluntarily inside user-created covenants. If a hardware sensor fails or an oracle is corrupted, the blast radius is 100% isolated to that specific covenant UTXO tree; Newsatoshi L1 consensus, issuance, and every unrelated `LOVE` holder remain completely unaffected.

### 3.2 Invariant 2 — Pure SHA-256 Post-Quantum Cryptographic Floor

`NEWSATOSHI/2.1` MUST NOT introduce elliptic curves (ECDSA, Schnorr, Ed25519), bilinear pairings (BN254, BLS12-381), RSA accumulators, or trusted-setup ceremonies into consensus or covenant trust roots. Every cryptographic assumption rests exclusively on collision/preimage resistance of **SHA-256** and **transparent hash-based STARKs**:
- **Note & State Commitments:** Domain-separated SHA-256.
- **Deed Oracle & Hardware TEE Signatures:** Pure SHA-256 XMSS-style Merkleized Lamport OTS trees (`64`–`4,096` leaves under one 32-byte root) and NIST FIPS 205 SLH-DSA (`SLH-DSA-SHA2-128f` / `SLH-DSA-SHA2-256f`).
- **Machine Streaming Channels:** Pure SHA-256 PayWord hash chains ($H^k(\text{preimage}) = \text{tip}$).
- **Zero-Knowledge / Validity Proofs:** Transparent FRI-based STARKs (Winterfell 0.13 in Phase 0; SP1 Hypercube / Miden VM recursive STARKs in Phase 1+).

### 3.3 Invariant 3 — $O(1)$ Edge & Microcontroller Verification

L1 consensus verification time and proof size MUST be $O(1)$ with respect to the number of historical covenant transitions $T$ ($T \in [1, 10^9]$). Verification MUST complete in **$< 10\text{ ms}$** on commodity hardware and low-cost ($5–$15) off-grid edge controllers (ESP32-S3, ARM Cortex-M7, Raspberry Pi Zero 2 W) so local solar inverters, land-trust auditors, and community nodes can independently verify treasury solvency without trusting cloud RPC providers.

### 3.4 Invariant 4 — Strict Soft-Fork Backwards Compatibility

`OP_VERIFY_COVENANT_STARK` redefines `OP_NOP5` (`0xb4`). Under `newsatoshi v0.1.0` consensus rules, `OP_NOP5` performs no operation and leaves the stack unchanged. Therefore:
- Every valid `NEWSATOSHI/2.1` block is a valid `v0.1.0` block (strict tightening of validation rules).
- Existing `ns1`/`nsd1` private transfers, `lock`/`unlock` HTLCs, `cond` scripts, and `job` escrows require zero migration and incur zero extra witness overhead.

---

## 4. Shielded UTXO Note Commitment & Anchor Extensions

### 4.1 Shielded Note Output Commitment (`cm`)

In `NEWSATOSHI/2.1`, every shielded UTXO note commitment binds a 32-byte `covenant_vk_hash`:

```text
cm = SHA256(
    "NEWSATOSHI_NOTE_CM_V2_1" ||   // 23-byte ASCII domain separator
    be64(value)               ||   // 8-byte unsigned big-endian amount in base units
    owner_pk                  ||   // 32-byte shielded recipient public key commitment
    covenant_vk_hash          ||   // 32-byte covenant VK hash (0x00..00 for standard private LOVE)
    salt                           // 32-byte fresh uniform blinding randomness
)
```

Equivalently, in unprefixed notation: `H(value || owner_pk || covenant_vk_hash || salt)` under domain tag `NEWSATOSHI_NOTE_CM_V2_1`.

#### Sentinel Rule for Standard Private `LOVE`

- **Standard Private Note:** `covenant_vk_hash == 0x0000000000000000000000000000000000000000000000000000000000000000` (`ZERO_HASH`). Such notes spend via ordinary v0.1.0 shielded spend proofs with zero covenant overhead.
- **Covenanted Commons Note:** `covenant_vk_hash != ZERO_HASH`. Any transaction spending a note whose `covenant_vk_hash != ZERO_HASH` MUST satisfy `OP_VERIFY_COVENANT_STARK` (`0xb4`) against that exact `covenant_vk_hash`.

### 4.2 Immutable `covenant_rule_hash` & `covenant_vk_hash` Derivation

The domain-specific Commons rule digest matches `newsatoshi-commons/src/lib.rs` and `zkvm/covenant-circuit-core/src/lib.rs` byte-for-byte:

```text
covenant_rule_hash = SHA256(
    "NEWSATOSHI_COVENANT_V2_1"  || // 24-byte ASCII domain tag
    be64(max_bounty_payout)     || // 8-byte max single physical deployment bounty
    be16(reinvest_bps)          || // 2-byte basis points (0..=10_000, e.g., 1500 = 15%)
    be16(min_soil_health_score) || // 2-byte ecological coherence floor (0..=100)
    authorized_deed_oracle_pk   || // 32-byte Deed Oracle XMSS Merkle root / SLH-DSA commitment
    authorized_hardware_pk         // 32-byte Hardware TEE XMSS Merkle root / SLH-DSA commitment
)
```

To bind both the zkVM verifier circuit (`program_vk_id`, e.g., SP1 ELF `vk.bytes32()` or Miden MAST root) AND the immutable economic/ecological parameters into a single 32-byte UTXO lock:

```text
covenant_vk_hash = SHA256(
    "NEWSATOSHI_COVENANT_VK_V2_1" || // 27-byte ASCII domain tag
    program_vk_id                 || // 32-byte zkVM verifying key digest
    covenant_rule_hash               // 32-byte rule parameters digest
)
```

*Note:* In Phase 0 Client-Side Validation (pre-soft-fork), `program_vk_id` MAY be set to the Winterfell 0.13 `CovenantTransitionAir` identifier digest (`SHA256("WINTERFELL_0_13_COVENANT_AIR_V1")`), and transitioned cleanly to the audited SP1 Hypercube or Miden VM ELF/MAST digest in Phase 1.

### 4.3 Phase 0 L1 Anchor Commitment (`OP_RETURN` / `lock` / `--memo`)

Prior to Phase 2 soft-fork activation, CSV covenants anchor state roots onto live `newsatoshi v0.1.0` blocks using deterministic 32-byte commitments (compatible with `OP_RETURN`, `newsatoshi lock --hash`, and shielded `send --memo`):

```text
anchor_hash = SHA256(
    "NEWSATOSHI/2.1"             ||
    covenant_rule_hash           ||
    prev_state_root              ||
    new_state_root               ||
    be64(tx_nonce)               ||
    be64(covenant_balance_love)  ||
    be64(parcels_acquired)       ||
    be64(total_kwh_verified)     ||
    be64(merkle_leaves_consumed)
)

op_return_script         = 0x6a || 0x20 || anchor_hash
stark_witness_digest     = SHA256("OP_VERIFY_COVENANT_STARK_V2_1" || covenant_rule_hash || prev_state_root || new_state_root || anchor_hash)
stark_witness_commitment = 0x20 || stark_witness_digest || 0xb4
```

---

## 5. Consensus Specification: `OP_VERIFY_COVENANT_STARK` (`0xb4`)

### 5.1 Opcode Assignment & Activation Guard

- **Opcode Byte:** `0xb4` (redefinition of `OP_NOP5`).
- **Pre-Activation:** Treated as `OP_NOP5` (no-op; does not consume stack elements unless `DISCOURAGE_UPGRADABLE_NOPS` policy flag is set on standardness relay).
- **Post-Activation:** Executes the normative deterministic verification procedure in Section 5.4. Unlike standard `OP_NOP` redefinitions that leave the stack untouched on success, `OP_VERIFY_COVENANT_STARK` operates inside the dedicated covenant witness evaluation context (similar to Tapscript/shielded-condition extensions) and MUST abort script execution immediately (`SCRIPT_ERR_COVENANT_STARK_FAILED`) if any validation check returns `false`.

### 5.2 Stack & Witness Envelope Binary Layout

When spending a covenanted UTXO note (`input.covenant_vk_hash != ZERO_HASH`), the spending transaction witness MUST supply the following stack elements (top-to-bottom) prior to `0xb4`:

```text
Stack Top -> [ stark_witness_envelope ]   (variable bytes, <= 262,144 bytes / 256 KiB)
             [ external_attestation_hash ] (32 bytes)
             [ new_state_root ]            (32 bytes)
             [ prev_state_root ]           (32 bytes)
             [ covenant_vk_hash ]          (32 bytes)
```

*(Scalar transaction values `tx_sighash`, `input_value`, `output_payout_value`, and `change_value` are extracted deterministically from the spending transaction context by consensus rules—NOT trusted from unconstrained witness pushes—preventing transaction/proof decoupling!)*

#### `stark_witness_envelope` Serialization

| Offset | Size (Bytes) | Field | Normative Constraint |
|---:|---:|---|---|
| `0` | `4` | `magic` | MUST equal ASCII `"NS21"` (`0x4e 0x53 0x32 0x31`) |
| `4` | `1` | `version` | MUST equal `0x01` |
| `5` | `1` | `backend_id` | `0x01` = Winterfell 0.13 (Phase 0 / testnet only), `0x02` = SP1 Hypercube, `0x03` = Miden VM |
| `6` | `1` | `flags` | Bit `0`: `1` if consolidation spend (`output_payout_value == 0`); Bits `1..7` MUST be `0` |
| `7` | `1` | `reserved` | MUST be `0x00` |
| `8` | `32` | `program_vk_id` | Verifying key digest of the zkVM guest ELF / Miden MAST root |
| `40` | `32` | `covenant_rule_hash` | Immutable rule digest (`covenant_vk_hash == H(DOMAIN_VK \|\| program_vk_id \|\| covenant_rule_hash)`) |
| `72` | `32` | `prev_accumulator` | Prior recursive IVC accumulator |
| `104` | `32` | `new_accumulator` | Post-transition recursive IVC accumulator |
| `136` | `8` | `step_index` | Big-endian `u64` monotonic step counter |
| `144` | `4` | `new_oracle_watermark` | Big-endian `u32` Deed Oracle XMSS watermark |
| `148` | `4` | `new_hw_watermark` | Big-endian `u32` Hardware TEE XMSS watermark |
| `152` | `4` | `proof_len` | Big-endian `u32` length $L$ of `stark_proof_bytes` ($1 \le L \le 261,988$) |
| `156` | `L` | `stark_proof_bytes` | Raw serialized STARK proof for `backend_id` |

### 5.3 Canonical 8-Element Public Input Vector

Consensus constructs the exact 184-byte core L1 public input vector (and full 264-byte IVC public values slice matching `covenant_circuit_core::PublicValues::to_canonical_bytes()`) in strict big-endian order:

| # | Field | Type / Width | Source | Security Purpose |
|---:|---|---|---|---|
| 1 | `tx_sighash` | `[u8; 32]` | Computed by L1 node (`SIGHASH_ALL`) | Binds STARK proof to exact spending inputs, outputs, and locktime (blocks replay/front-running) |
| 2 | `input_value` | `u64` (8B BE) | Sum of covenanted input note value(s) | Establishes exact available covenanted principal |
| 3 | `output_payout_value` | `u64` (8B BE) | Non-covenanted payout output value | Caps funds exiting the covenant (must be `0` during consolidation) |
| 4 | `change_value` | `u64` (8B BE) | Covenanted change / reinvested output value | Establishes principal retained inside the recursive covenant |
| 5 | `covenant_vk_hash` | `[u8; 32]` | Input note commitment & stack | Binds proof to immutable program VK + economic/ecological rule |
| 6 | `prev_state_root` | `[u8; 32]` | Stack / prior anchor | Pre-transition ledger state commitment |
| 7 | `new_state_root` | `[u8; 32]` | Stack / new anchor | Post-transition ledger state commitment |
| 8 | `external_attestation_hash` | `[u8; 32]` | Stack | Binds Deed Oracle / Hardware TEE telemetry / PayWord settlement receipt |

*(The full 264-byte committed guest tape appends `prev_accumulator [32B]`, `new_accumulator [32B]`, `step_index [8B]`, `new_oracle_watermark [4B]`, and `new_hw_watermark [4B]` directly after the 8 core fields.)*

### 5.4 Deterministic Consensus Verification Algorithm

Upon executing `OP_VERIFY_COVENANT_STARK` (`0xb4`), a conforming `NEWSATOSHI/2.1` node MUST execute the following 7 steps in exact order, failing consensus immediately if any predicate evaluates to `false`:

1. **Envelope & DoS Bound Check:**
   - Assert stack depth $\ge 5$.
   - Assert `stark_witness_envelope.len() >= 156` and `<= 262_144` (256 KiB hard ceiling).
   - Assert `magic == "NS21"`, `version == 0x01`, `reserved == 0x00`, unused `flags` bits are `0`, `156 + proof_len == stark_witness_envelope.len()`.
   - Assert `backend_id` is enabled on the current network (`0x02` or `0x03` on mainnet; `0x01`/`0x02`/`0x03` on testnet).
2. **Non-Zero Input Covenant Binding:**
   - Assert `covenant_vk_hash != ZERO_HASH`.
   - Assert every covenanted input note in the spend commits to the identical `input.covenant_vk_hash == covenant_vk_hash`.
   - Assert `SHA256("NEWSATOSHI_COVENANT_VK_V2_1" || program_vk_id || covenant_rule_hash) == covenant_vk_hash`.
3. **Overflow-Safe Value Conservation:**
   - Let `tx_fee` be the explicit L1 transaction fee (or `0` when fee is paid by an auxiliary non-covenanted fee-funding input; RECOMMENDED pattern is paying L1 fees from a separate standard `LOVE` note so covenant value conservation is exact: `input_value == output_payout_value + change_value`).
   - Assert `output_payout_value.checked_add(change_value) == Some(input_value)`.
   - If `flags & 0x01 == 1` (consolidation), assert `output_payout_value == 0` and `change_value == input_value > 0`.
4. **Recursive Change Continuity Rule (Anti-Rug-Pull Predicate):**
   - If `change_value > 0`:
     - The spending transaction MUST contain a designated change output committing to `output_change.value == change_value` AND:
       $$\texttt{output\_change.covenant\_vk\_hash} \equiv \texttt{input.covenant\_vk\_hash}$$
   - If `change_value == 0`: no covenanted change output is required (full covenant depletion is permitted only if explicitly allowed by the guest circuit).
5. **Transaction Sighash Binding:**
   - Compute `tx_sighash = SHA256("NEWSATOSHI_SIGHASH_COVENANT_V2_1" || serialized_tx_inputs_and_outputs)`.
   - Assemble the 264-byte canonical `public_values` tape from `(tx_sighash, input_value, output_payout_value, change_value, covenant_vk_hash, prev_state_root, new_state_root, external_attestation_hash, prev_accumulator, new_accumulator, step_index, new_oracle_watermark, new_hw_watermark)`.
6. **Transparent STARK Verification:**
   - Invoke the consensus verifier for `backend_id`:
     $$\texttt{verify\_stark(backend\_id, program\_vk\_id, public\_values, stark\_proof\_bytes)} \stackrel{?}{=} \texttt{true}$$
   - Reject block/tx if verification fails or exceeds per-proof verification weight budget (`10 ms` reference CPU ceiling; `50,000` weight units).
7. **Clean Stack Completion:**
   - Pop the 5 covenant operands and push `OP_TRUE` (`0x01`) onto the stack.

### 5.5 Coexistence & Composition Matrix with `newsatoshi v0.1.0` Primitives

`OP_VERIFY_COVENANT_STARK` is strictly additive and composes cleanly with every existing `newsatoshi v0.1.0` primitive:

| `v0.1.0` Primitive | Interaction with `NEWSATOSHI/2.1` (`0xb4`) | Consensus Conflict? |
|---|---|---|
| **Shielded Notes (`ns1` / `nsd1`)** | Standard notes set `covenant_vk_hash = 0x00..00`; covenant payouts emit standard private `ns1` notes to land sellers & operators while change outputs retain `covenant_vk_hash`. | **None** — exact commitment extension |
| **SHA-256 Hash Locks (`lock` / `unlock`)** | Used in Phase 0 to anchor `anchor_hash` on live `v0.1.0` testnet/mainnet (`newsatoshi lock 0.01 --hash <anchor_hash> --to <addr>`) and for atomic PayWord channel funding. | **None** — orthogonal lock script |
| **Programmable Conditions (`cond` / `cond-sign` / `fulfil` / `program`)** | Can wrap a timelocked SLH-DSA emergency key-rotation or hardware-replacement fallback (e.g., if an inverter is destroyed by lightning, a timelocked `cond` threshold releases hardware root rotation without granting discretionary fund withdrawal!). | **None** — composable condition branch |
| **SLH-DSA (`signing-key`, FIPS 205)** | Complements XMSS Merkle-Lamport trees as stateless post-quantum signatures (`SLH-DSA-SHA2-128f`) for Deed Oracles and Trust Enforcers without leaf-exhaustion limits. | **None** — pure SHA-256 shared floor |
| **Freivalds Checked-Work (`job`)** | Remains specialized for matrix/compute escrow; covenants can fund `job` escrows as uncovenanted payout outputs when outsourcing AI/grid optimization. | **None** — strictly orthogonal |

---

## 6. Recursive IVC Prover Architecture

### 6.1 Why Winterfell 0.13 Serves Phase 0 — And Why Phase 1 Migrates to SP1 Hypercube / Miden VM

`newsatoshi v0.1.0` ships with **Winterfell 0.13** for shielded note STARK proofs, and `newsatoshi-commons` uses Winterfell 0.13 in Phase 0 (`src/stark.rs`, `CovenantTransitionAir`) to generate genuine single-step algebraic STARK proofs of value conservation, bounty caps, ecological floors, and covenant tag preservation.

However, **Winterfell 0.13 is unsuitable for unbounded multi-step recursive Incrementally Verifiable Computation (IVC)**:
1. **No Native Recursion Circuit:** Winterfell 0.13 is a bare-metal AIR framework without a production-audited STARK-in-STARK FRI verifier circuit. Verifying one Winterfell proof inside another Winterfell AIR requires hand-encoding hundreds of trace columns for extension-field arithmetic, Merkle authentication paths, and out-of-domain sampling—a high audit-risk surface.
2. **Rigid Control Flow:** Every new covenant predicate (XMSS Merkle path verification, variable-length PayWord SHA-256 chains, consolidation) requires re-designing and re-auditing custom AIR polynomial degree bounds.

Therefore, `NEWSATOSHI/2.1` adopts a disciplined two-stage prover evolution:

| Dimension | Phase 0: **Winterfell 0.13** (`src/stark.rs`) | Phase 1A: **SP1 Hypercube** (`zkvm/sp1-covenant-guest`) | Phase 1B: **Miden VM** (`zkvm/miden-covenant`) |
|---|---|---|---|
| **Role** | Single-step CSV transition proofs today | Primary recursive IVC zkVM (RISC-V `rv32im`) | Secondary independent STARK VM (dual-prover defense) |
| **Proof System** | Univariate FRI STARK (Winterfell 0.13) | Multilinear Hypercube STARK (KoalaBear + Poseidon2 / SHA-256 precompiles) | Native Goldilocks stack STARK (128-bit FRI) |
| **Guest Authoring** | Hand-written Rust AIR (`CovenantTransitionAir`) | Pure `no_std` Rust (`covenant-circuit-core`) | Native Miden Assembly (`covenant_transition.masm`) |
| **Recursive IVC** | Single-step only (no production IVC) | Native (`sp1_zkvm::lib::verify::verify_sp1_proof`, compressed mode) | Native (`miden-verifier` recursive STARK) |
| **Trusted Setup** | None (transparent) | None (transparent) | None (transparent) |
| **SHA-256 Efficiency** | Software / custom AIR | Hardware-accelerated SHA-256 AIR precompile (~100x speedup) | Built-in `std::crypto::hashes::sha256` stdlib |
| **Proof Size** | ~25–45 KiB (single step) | Constant ~60–180 KiB (compressed recursive STARK for $N$ steps) | Constant ~45–120 KiB (128-bit STARK for $N$ steps) |
| **Edge Verify Latency** | ~1–4 ms (x86/ARM) | **< 10 ms** (constant $O(1)$ regardless of history depth) | **< 10 ms** (constant $O(1)$ regardless of history depth) |

### 6.2 Recursive IVC Accumulator & State-Root Chaining Equations

Both SP1 Hypercube (`zkvm/covenant-circuit-core`) and Miden VM (`zkvm/miden-covenant`) enforce identical deterministic state-root and recursive accumulator chaining:

```text
acc_0 = SHA256("NEWSATOSHI_IVC_GENESIS_V2_1" || covenant_vk_hash || genesis_state_root)

state_root_{t+1} = SHA256(
    "NEWSATOSHI_STATE_ROOT_V2_1" ||
    state_root_t                 ||
    tx_sighash_t                 ||
    be64(step_index_t)           ||
    be32(new_oracle_watermark_t) ||
    be32(new_hw_watermark_t)     ||
    external_attestation_hash_t
)

acc_{t+1} = SHA256(
    "NEWSATOSHI_IVC_STEP_V2_1"   ||
    acc_t                        ||
    covenant_vk_hash             ||
    state_root_t                 ||
    state_root_{t+1}             ||
    tx_sighash_t                 ||
    be64(step_index_t)           ||
    external_attestation_hash_t
)
```

For an unbounded sequence of transitions $t = 0, 1, \dots, N-1$:
- At step $k$, the zkVM guest verifies either a contiguous batch of raw witnesses or recursively verifies the prior compressed proof $\pi_{k-1}$ (`verify_sp1_proof`) plus the new transition witness $w_k$, enforcing $\pi_{k-1}.\texttt{new\_accumulator} == w_k.\texttt{prev\_accumulator}$ and $\pi_{k-1}.\texttt{covenant\_vk\_hash} == w_k.\texttt{covenant\_vk\_hash}$.
- **Critical SP1 Recursion Invariant:** Whenever `verify_sp1_proof` is invoked inside `sp1-covenant-guest`, the host prover MUST generate a **compressed/recursive** STARK proof (never a raw Core proof) so that all deferred proof-verification claims are completely discharged prior to L1 submission.

### 6.3 $O(1)$ Edge & Microcontroller Verification Budget

| Target Hardware | Approximate BOM Cost | RAM Budget | Verifier Code Size | Target Verify Time (`0xb4`) |
|---|---:|---:|---:|---:|
| **ESP32-S3 (Dual-Core 240 MHz + PSRAM)** | ~$5 | 512 KiB SRAM + 2 MiB PSRAM | ~90 KiB (`no_std`) | 6–9 ms (native C/Rust FRI verifier) |
| **STM32H7 (ARM Cortex-M7 @ 480 MHz)** | ~$9 | 1 MiB SRAM | ~85 KiB | 4–7 ms |
| **Raspberry Pi Zero 2 W (Quad Cortex-A53)** | ~$15 | 512 MiB | Standard Rust binary | 1.5–3 ms |
| **Newsatoshi L1 Full Node (x86_64 / Apple Silicon)** | Server/Desktop | Minimal | Linked Rust crate | < 1.5 ms |

---

## 7. Four-Phase Rollout, 90-Day Adversarial Audit Gate & AuxPoW Signaling

### 7.1 Four-Phase Deployment Roadmap

```text
Phase 0 (Live Now)          Phase 1 (90-Day Soak)        Phase 2 (Soft Fork)         Phase 3 (Compounding)
┌──────────────────────┐    ┌──────────────────────┐    ┌──────────────────────┐    ┌──────────────────────┐
│ CSV Engine +         │    │ Testnet 0xb4 Active  │    │ AuxPoW ChainID 20051 │    │ Parcel #1 -> #N      │
│ Winterfell 0.13 +    ├───►│ SP1 & Miden Audits + ├───►│ Version-Bit Signal   ├───►│ Perpetual Purpose    │
│ OP_RETURN / lock L1  │    │ 100M Fuzz Mutations  │    │ (90% / 2016 blocks)  │    │ Trust Sec. 4.2 Live  │
└──────────────────────┘    └──────────────────────┘    └──────────────────────┘    └──────────────────────┘
```

1. **Phase 0 — Client-Side Validated (CSV) Launch (Operational Today):**
   - Deploy `newsatoshi-commons` with XMSS Merkle-Lamport trees (`64` / `1,024` leaves), NIST FIPS 205 SLH-DSA (`SHA2-128f`), real Winterfell 0.13 single-step STARK proofs, and live L1 anchoring onto `newsatoshi-seed.fly.dev` (`testnet`) and mainnet via `OP_RETURN`, `newsatoshi lock --hash`, and `--memo`.
2. **Phase 1 — 90-Day Adversarial Testnet & Dual-Audit Gate:**
   - Enable `OP_VERIFY_COVENANT_STARK` (`0xb4`) on the Newsatoshi testnet (`genesis: 00000fd814d823a0bc64a84135868402998cff81e28a85b2bede5f1125d0a394`, port `18444`).
   - Run automated daily soak & adversarial checks via `zkvm/testnet-harness/adversarial_suite.sh`.
   - **Mandatory Exit Criteria (ALL required before Phase 2 signaling starts):**
     - $\ge 90$ consecutive calendar days on live testnet with **0 consensus splits**, **0 unintended reorgs**, and **0 proof soundness breaks**.
     - Two independent cryptographic & formal-verification audits covering: (a) `zkvm/covenant-circuit-core` (`no_std` kernel), (b) SP1 Hypercube guest ELF / Miden MASM program & deterministic reproducible builds, and (c) Newsatoshi L1 `0xb4` script interpreter integration.
     - $\ge 10^8$ witness-mutation fuzzing iterations confirming zero false-accepts on covenant stripping, value overflow, soil-floor bypass, XMSS leaf replay, or PayWord skimming.
     - Cross-prover equivalence verification confirming SP1 Hypercube (`0x02`) and Miden VM (`0x03`) accept/reject identical state-transition test vectors.
     - Demonstrated `< 10 ms` verification benchmark on target embedded ARM / ESP32-S3 hardware.
3. **Phase 2 — Mainnet AuxPoW Soft-Fork Activation (`chainid = 20051`):**
   - Activate `0xb4` via conservative miner signaling on merged-mined Newsatoshi L1 blocks (parameters in Section 7.2).
4. **Phase 3 — Physical Parcel #1 $\rightarrow$ Parcel #N Autonomous Compounding:**
   - Bind Parcel #1's Perpetual Purpose Trust Instrument (Section 4.2 Legal-Cryptographic Airlock) to the live mainnet `covenant_vk_hash`, activate solar/greenhouse TEE telemetry and PayWord bill-killer microgrid streaming, and publish open verifiable thermodynamics (`LOVE/kWh`, kWh output, soil health score, compounding balance).

### 7.2 AuxPoW (`chainid = 20051`) Soft-Fork Activation Parameters

Activation uses a BIP-9-style version-bits state machine (`DEFINED -> STARTED -> LOCKED_IN -> ACTIVE` / `FAILED`) specifically engineered for **SHA-256 AuxPoW merged mining**:

| Parameter | Normative Value | Rationale |
|---|---|---|
| **Deployment Name** | `covenant_stark_v2_1` | Unique human-readable soft-fork identifier |
| **AuxPoW Chain ID** | `20051` (unchanged) | Preserves full compatibility with existing SHA-256 merged-mining pools |
| **Signaling Field** | Child block header `nVersion` (NOT parent PoW header!) | Critical: Parent Bitcoin/SHA-256 ASICs hash the parent header untouched; pools flip a single bit in the Newsatoshi AuxPoW child header with **zero ASIC firmware changes** |
| **Version Bit** | `bit = 4` (`0x00000010`, top bits `001` per BIP-9: `0x20000010`) | Non-overlapping with AuxPoW flag bit (`0x00000100`) and base version bits |
| **Signaling Window (`period`)** | `2,016` blocks | Standard difficulty-aligned evaluation window |
| **Activation Threshold** | `1,815 / 2,016` blocks (`90.03%`) | High supermajority prevents premature contentious activation |
| **Grace Period (`LOCKED_IN -> ACTIVE`)** | `1` full window (`2,016` blocks) | Guarantees economic nodes, wallets, and hardware signers time to upgrade before enforcement |
| **`start_time` / `timeout`** | Set post-Phase 1 audit sign-off (`timeout = start_time + 365 days`) | Prevents indefinite stale signaling states |
| **Non-Signaling Compatibility** | Mandatory `OP_NOP5` pre-activation | Unupgraded miners cannot cause chain splits during `STARTED` or `LOCKED_IN` |

---

## 8. Formal Threat Model & Security Analysis

| # | Threat Vector | Attack Mechanics | `NEWSATOSHI/2.1` Mitigation | Residual Risk & Operational Defense |
|---|---|---|---|---|
| 1 | **Budish (2018) Rented-Hashpower 51% Attack** | Adversary rents spot SHA-256 hashpower to reorg blocks and steal minted physical rewards or double-spend large land purchases. | **L1 Blindness Invariant:** L1 never mints `LOVE` for physical work, eliminating endogenous protocol-inflation attack profit. High-value land deed settlements require deep AuxPoW confirmations (e.g., 36–100 blocks) before county deed escrow release. | Standard PoW finality latency on large spends; mitigated via confirmation depth scaling proportional to `output_payout_value` and AuxPoW hashpower growth. |
| 2 | **Sybil Wash-Farming & Sensor Spoofing** | Operator loops solar output into dummy resistive loads or injects fake sensor packets to drain `max_bounty_payout` bounties without real community energy delivery. | (a) Zero L1 minting—treasury is strictly finite and pre-funded; (b) `max_bounty_payout` rate-limits every single bounty; (c) PayWord streaming revenue requires real paying consumers releasing preimages; (d) Ecological floor `soil_health_score >= min_soil_health_score` gates disbursement. | Local physical sensor tampering can still produce false telemetry inside a single parcel; defended by multi-sensor cross-checks (satellite irradiance vs inverter kWh, periodic independent agronomic audits, and Trust Enforcer hardware revocation). |
| 3 | **Covenant Stripping / Change Diversion (Rug-Pull)** | Malicious operator or compromised trustee spends a 100k LOVE covenant UTXO, pays 15k legitimately, and routes 85k change to an uncovenanted private address (`covenant_vk_hash = 0x00..00` or attacker rule). | **Consensus-Enforced Recursive Continuity:** Both the zkVM guest (`covenant-circuit-core`) and `OP_VERIFY_COVENANT_STARK` (`0xb4`) reject any spend where `change_value > 0` and `output_change.covenant_vk_hash != input.covenant_vk_hash`. | **Zero cryptographic residual risk** post-Phase 2 activation. In Phase 0 CSV mode, defended legally by Perpetual Purpose Trust Section 4.2 (*ultra vires* / void *ab initio*) and watchtower monitoring. |
| 4 | **Lamport / XMSS Leaf Reuse & Replay** | Adversary replays an old valid Merkle-Lamport signature or tricks a rebooted inverter into signing two messages with the same one-time Lamport leaf (which leaks half the secret key!). | (a) On-chain/circuit **monotonic watermark** (`leaf_index >= prev_watermark`, `new_watermark == leaf_index + 1`) blocks every replay or out-of-order leaf in $O(1)$ space; (b) Hardware TEE persists `next_leaf_index` in monotonic NVM/eFuse counter before releasing signature; (c) NIST FIPS 205 **SLH-DSA** provides a stateless PQ alternative immune to index-rollback errors. | Flash power-loss rollback on cheap MCUs if NVM counter isn't pre-incremented; mitigated by pre-incrementing NVM leaf counter *before* signing (accepting skipped leaves) or using stateless SLH-DSA. |
| 5 | **Deed Oracle Compromise or Collusion** | Attacker steals the County Deed Oracle private key and signs fraudulent land deeds to extract treasury funds to an accomplice seller. | (a) Oracle root SHOULD be instantiated as a **threshold M-of-N SLH-DSA / XMSS ceremony** across independent title company, Trust Enforcer, and community steward; (b) `output_payout_value` bounded by parcel escrow schedule; (c) Perpetual Purpose Trust Section 4.2 voids conveyances lacking recorded fee-simple title in Trust name. | Collusion of $\ge M$ threshold signers remains an off-chain governance risk; mitigated by timelocked `cond` dispute windows on large land acquisitions (>20% of treasury). |
| 6 | **Hardware TEE Side-Channel / Physical Extraction** | Attacker physically steals the solar inverter / ESP32-S3 from a rural parcel and performs power-analysis or fault-injection to extract `master_seed`. | (a) `max_bounty_payout` caps single-spend extraction; (b) Monotonic watermark limits total leaves and creates immediate on-chain anomaly telemetry; (c) Physical replacement & key rotation via timelocked `cond` + Trust Enforcer authorization allows rotating `authorized_hardware_pk` without unlocking discretionary fund extraction. | Physical device theft in remote rural sites; mitigated by tamper-evident enclosures, hardened secure elements (ATECC/SE050/TrustZone), and automated anomaly watchtowers. |
| 7 | **Legal-Title / On-Chain State Divergence** | A county recorder or court disputes title ownership, or a trustee attempts off-chain mortgage/encumbrance contradicting the on-chain covenant state. | **Perpetual Purpose Trust Airlock (Section 4.2):** Statutory non-charitable purpose trust strips trustee discretionary conveyance power, embeds immutable `covenant_rule_hash` in Schedule A, designates unauthorized encumbrances as *ultra vires* and void *ab initio*, and appoints an independent Trust Enforcer with standing to compel specific performance. | Jurisdictional statutory changes or eminent domain; mitigated by Wyoming DUNA / Delaware Purpose Trust statutory selection, recorded restrictive conservation covenants at the county level, and multi-parcel geographic diversification. |
| 8 | **Proof Malleability & L1 Verification DoS** | Adversary floods the mempool with huge invalid STARK witnesses or replays a valid STARK proof inside a modified transaction to stall full nodes. | (a) Strict `256 KiB` witness envelope ceiling + `50,000` weight unit cap; (b) `tx_sighash` is computed directly from the spending transaction by consensus (making cross-tx replay impossible); (c) Fast envelope header/binding checks execute in microseconds before invoking the STARK verifier. | Maliciously crafted near-ceiling invalid proofs still consume ~1–2 ms CPU during mempool admission; mitigated by standard `newsatoshi` fee-rate anti-spam (`fee >= 300,000` base units) and peer misbehavior scoring. |

---

## 9. Canonical Domain Separator Registry

All SHA-256 domain separators MUST match across `newsatoshi-commons`, `zkvm/covenant-circuit-core`, `zkvm/sp1-covenant-guest`, and `zkvm/miden-covenant`:

| Domain Tag (ASCII) | Purpose | Reference Implementation |
|---|---|---|
| `NEWSATOSHI/2.1` | L1 `anchor_hash` commitment prefix | `src/rpc.rs` |
| `NEWSATOSHI_COVENANT_V2_1` | Immutable `covenant_rule_hash` | `src/lib.rs`, `zkvm/covenant-circuit-core/src/lib.rs` |
| `NEWSATOSHI_COVENANT_VK_V2_1` | `covenant_vk_hash` binding `program_vk_id` + `rule_hash` | `zkvm/covenant-circuit-core/src/lib.rs` |
| `NEWSATOSHI_NOTE_CM_V2_1` | Shielded note commitment `H(value \|\| owner_pk \|\| covenant_vk_hash \|\| salt)` | `zkvm/covenant-circuit-core/src/lib.rs` |
| `NEWSATOSHI_IVC_GENESIS_V2_1` | Initial recursive IVC accumulator `acc_0` | `zkvm/covenant-circuit-core/src/lib.rs` |
| `NEWSATOSHI_IVC_STEP_V2_1` | Recursive IVC accumulator step `acc_{t+1}` | `zkvm/covenant-circuit-core/src/lib.rs` |
| `NEWSATOSHI_STATE_ROOT_V2_1` | Deterministic transition state root | `zkvm/covenant-circuit-core/src/lib.rs` |
| `NEWSATOSHI_ATTESTATION_V2_1` | Canonical `external_attestation_hash` | `zkvm/covenant-circuit-core/src/lib.rs` |
| `NEWSATOSHI_SIGHASH_COVENANT_V2_1` | Consensus transaction sighash binding | Section 5.4 |
| `OP_VERIFY_COVENANT_STARK_V2_1` | Soft-fork witness envelope digest (`0xb4`) | `src/rpc.rs`, `zkvm/sp1-covenant-guest/host/src/main.rs` |
| `NEWSATOSHI_XMSS_LEAF_V1` | XMSS Merkle leaf hash | `src/crypto.rs`, `zkvm/covenant-circuit-core/src/lib.rs` |
| `NEWSATOSHI_XMSS_NODE_V1` | XMSS Merkle internal node hash | `src/crypto.rs`, `zkvm/covenant-circuit-core/src/lib.rs` |
| `DEED_TRANSFER_V1` | Land Deed attestation message digest | `src/lib.rs`, `zkvm/covenant-circuit-core/src/lib.rs` |
| `PHYSICAL_TELEMETRY_V1` | Energy & Soil Health telemetry digest | `src/lib.rs`, `zkvm/covenant-circuit-core/src/lib.rs` |

---

## 10. Reference Implementations & Verification Suite

- **Pure-Rust `no_std` Verification Kernel:** [`zkvm/covenant-circuit-core/`](../zkvm/covenant-circuit-core/) — Compilable standalone crate enforcing every RFC-0021 invariant with unit & adversarial witness-tampering tests (`cargo test` & `cargo check --no-default-features`).
- **SP1 Hypercube Recursive Guest & Host Driver:** [`zkvm/sp1-covenant-guest/`](../zkvm/sp1-covenant-guest/) — RISC-V guest wrapping `covenant-circuit-core` with `sp1_zkvm::io::commit_slice` and `verify_sp1_proof` recursion support.
- **Miden VM Native STARK Program:** [`zkvm/miden-covenant/covenant_transition.masm`](../zkvm/miden-covenant/covenant_transition.masm) — Stack-native Goldilocks STARK implementation of value conservation, covenant continuity, bounty cap, ecological floor, XMSS watermark, PayWord reinvestment split + SHA-256 hash chain, and IVC folding.
- **90-Day Adversarial Testnet Harness:** [`zkvm/testnet-harness/adversarial_suite.sh`](../zkvm/testnet-harness/adversarial_suite.sh) — Automated soak & adversarial verification script checking live `newsatoshi-seed.fly.dev/status`, `covenant-circuit-core`, CLI adversarial guards, and structured audit-log generation.
