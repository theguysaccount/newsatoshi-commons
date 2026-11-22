# `sp1-covenant-guest` — `NEWSATOSHI/2.1` SP1 Hypercube Recursive IVC Guest & Host

Succinct **SP1 Hypercube** (RISC-V `rv32im` multilinear STARK zkVM) guest program and host prover/verifier driver for [`OP_VERIFY_COVENANT_STARK` (`0xb4`)](../../docs/RFC-0021-OP_VERIFY_COVENANT_STARK.md).

This crate wraps the audited, pure-Rust `no_std` verification kernel in [`../covenant-circuit-core`](../covenant-circuit-core) so the exact same transition verifier runs natively in unit tests, inside SP1 Hypercube's RISC-V guest, and in edge audit tools without code drift.

---

## Architecture

```
covenant-circuit-core (no_std, pure SHA-256)
        │
        ▼  compiled to rv32im ELF via `cargo prove build`
sp1-covenant-guest/src/main.rs
   ├─ sp1_zkvm::io::read::<GuestInput>()
   ├─ covenant_circuit_core::verify_recursive_fold(&witnesses)
   ├─ [optional `recursion` feature] sp1_zkvm::lib::verify::verify_sp1_proof(&prev_vkey, &prev_pv_digest)
   └─ sp1_zkvm::io::commit_slice(&canonical_264b_public_values)
        │
        ▼  compressed recursive STARK proof (O(1) size, <10 ms edge verify)
sp1-covenant-host/src/main.rs  ──►  OP_VERIFY_COVENANT_STARK (0xb4) witness envelope
```

### Canonical Committed Public Values (`264 bytes`, Big-Endian)

| Offset | Length | Field | RFC-0021 Role |
|---:|---:|---|---|
| `0` | `32` | `tx_sighash` | L1 `SIGHASH_ALL` transaction binding |
| `32` | `8` | `input_value` | Total covenanted input LOVE |
| `40` | `8` | `output_payout_value` | Uncovenanted payout LOVE |
| `48` | `8` | `change_value` | Covenanted change / reinvested LOVE |
| `56` | `32` | `covenant_vk_hash` | `SHA256("NEWSATOSHI_COVENANT_VK_V2_1" \|\| program_vk_id \|\| rule_hash)` |
| `88` | `32` | `prev_state_root` | Pre-transition / batch-start ledger root |
| `120` | `32` | `new_state_root` | Post-transition / batch-end ledger root |
| `152` | `32` | `external_attestation_hash` | Deed / TEE telemetry / PayWord commitment |
| `184` | `32` | `prev_accumulator` | Starting IVC accumulator |
| `216` | `32` | `new_accumulator` | Folded IVC accumulator |
| `248` | `8` | `step_index` | Final verified step index |
| `256` | `4` | `new_oracle_watermark` | Post-fold Deed Oracle XMSS watermark |
| `260` | `4` | `new_hw_watermark` | Post-fold Hardware TEE XMSS watermark |

---

## Prerequisites & Build / Prove Commands

### 1. Install the SP1 Toolchain (`sp1up` / `cargo prove`)

```bash
curl -L https://sp1up.succinct.xyz | bash
sp1up
cargo prove --version
```

### 2. Verify the Underlying `no_std` Circuit Kernel First (No SP1 Toolchain Needed)

```bash
cd ../covenant-circuit-core
cargo test -- --nocapture
cargo check --no-default-features
```

### 3. Build the Deterministic RISC-V Guest ELF

```bash
cd ../sp1-covenant-guest
# Standard batch-folding guest
cargo prove build --locked

# Or with cross-proof SP1 recursion enabled (`verify_sp1_proof`):
cargo prove build --locked --features recursion
```

> **Performance Note:** Uncomment the `[patch.crates-io]` SHA-256 precompile patch in `Cargo.toml` matching your installed `sp1up` version to route `sha2::Sha256` directly to SP1's hardware-accelerated SHA-256 AIR precompile (~100x fewer RISC-V cycles).

### 4. Execute, Prove & Verify via the Host Driver

```bash
cd host

# Fast RISC-V cycle dry-run (verifies guest output matches native kernel)
RUST_LOG=info cargo run --release -- --execute

# Generate a compressed recursive STARK proof and verify it locally
SP1_PROVER=cpu RUST_LOG=info cargo run --release -- --prove

# Use GPU / Succinct Prover Network for sub-second Hypercube proving:
# SP1_PROVER=cuda cargo run --release -- --prove
```

> **Recursion Security Requirement:** Whenever `prior_proof` is populated (`--features recursion`), the host **MUST** invoke `.compressed()` (or higher recursive wrapping) rather than raw `.core()` proving, guaranteeing all deferred `verify_sp1_proof` calls are completely verified before emitting the `OP_VERIFY_COVENANT_STARK` witness.
