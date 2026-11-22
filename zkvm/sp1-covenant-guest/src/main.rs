//! # `sp1-covenant-guest` — `NEWSATOSHI/2.1` SP1 Hypercube zkVM Guest Program
//!
//! Executes the pure-Rust `no_std` verification kernel in `covenant-circuit-core` inside
//! Succinct's SP1 Hypercube RISC-V zkVM and commits canonical `OP_VERIFY_COVENANT_STARK`
//! [`PublicValues`] bytes to the STARK public values tape.
//!
//! Supports two operating modes:
//! 1. **Base / Batch Fold Mode (`prior_proof == None`):** Verifies 1..N contiguous
//!    [`CovenantTransitionWitness`] steps starting from genesis (or a checkpoint state root)
//!    and folds them into constant-size [`PublicValues`].
//! 2. **Recursive IVC Aggregation Mode (`prior_proof == Some(...)`, with `recursion` feature):**
//!    Recursively verifies a prior compressed SP1 proof via `sp1_zkvm::lib::verify::verify_sp1_proof`,
//!    checks exact continuity from the prior proof's committed `PublicValues` into the new
//!    witness batch, and emits a single constant-size proof spanning the entire covenant history.

#![no_main]
sp1_zkvm::entrypoint!(main);

extern crate alloc;

use alloc::vec::Vec;
use covenant_circuit_core::{
    verify_recursive_fold, CovenantTransitionWitness, Hash32, PublicValues,
    PUBLIC_VALUES_ENCODED_LEN,
};
use serde::{Deserialize, Serialize};

/// Optional prior compressed SP1 proof metadata for unbounded recursive IVC chaining.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PriorIvcProof {
    /// 8 x u32 SP1 verifying key digest (must match this guest ELF's own VK for self-recursion).
    pub prev_vkey_u32: [u32; 8],
    /// Prior proof's committed canonical [`PublicValues`] struct.
    pub prev_public_values: PublicValues,
}

/// Complete private input supplied by the SP1 host via `SP1Stdin::write`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GuestInput {
    /// Optional prior SP1 proof to recursively fold before verifying `witnesses`.
    pub prior_proof: Option<PriorIvcProof>,
    /// One or more contiguous covenant state transitions to verify & fold.
    pub witnesses: Vec<CovenantTransitionWitness>,
}

pub fn main() {
    // 1. Read private witness bundle from host stdin.
    let input: GuestInput = sp1_zkvm::io::read::<GuestInput>();

    assert!(
        !input.witnesses.is_empty(),
        "sp1-covenant-guest: witness batch cannot be empty"
    );

    // 2. Verify all transitions in the current batch via the audited `no_std` kernel.
    let folded = verify_recursive_fold(&input.witnesses)
        .expect("sp1-covenant-guest: covenant transition verification failed");

    // 3. If chaining onto a prior compressed SP1 proof, enforce strict recursive IVC continuity.
    let committed_pv: PublicValues = match input.prior_proof {
        Some(prior) => {
            let prev_pv = &prior.prev_public_values;

            // Enforce unbroken chain between prior proof tip and current batch head.
            assert_eq!(
                folded.covenant_vk_hash, prev_pv.covenant_vk_hash,
                "recursive IVC covenant_vk_hash mismatch"
            );
            assert_eq!(
                folded.initial_state_root, prev_pv.new_state_root,
                "recursive IVC state_root discontinuity"
            );
            assert_eq!(
                folded.initial_accumulator, prev_pv.new_accumulator,
                "recursive IVC accumulator discontinuity"
            );
            assert_eq!(
                folded.start_step_index,
                prev_pv
                    .step_index
                    .checked_add(1)
                    .expect("step_index overflow"),
                "recursive IVC step_index gap"
            );
            assert_eq!(
                input.witnesses[0].prev_oracle_watermark, prev_pv.new_oracle_watermark,
                "recursive IVC oracle watermark discontinuity"
            );
            assert_eq!(
                input.witnesses[0].prev_hw_watermark, prev_pv.new_hw_watermark,
                "recursive IVC hardware watermark discontinuity"
            );

            // Recursively verify the prior compressed SP1 proof inside the zkVM.
            // The host passes the prior SP1Proof via `stdin.write_proof(proof, vk.vk)`
            // and MUST prove this outer guest in `.compressed()` mode so deferred
            // proof verification is completely resolved.
            #[cfg(feature = "recursion")]
            {
                let prev_pv_digest: Hash32 = prev_pv.commitment_digest();
                sp1_zkvm::lib::verify::verify_sp1_proof(&prior.prev_vkey_u32, &prev_pv_digest);
            }
            #[cfg(not(feature = "recursion"))]
            {
                let _ = prior.prev_vkey_u32;
                let _prev_digest: Hash32 = prev_pv.commitment_digest();
                panic!(
                    "sp1-covenant-guest: prior_proof supplied but `recursion` feature is disabled"
                );
            }

            // Produce folded PublicValues spanning the prior proof's origin to the new tip.
            PublicValues {
                tx_sighash: folded.final_tx_sighash,
                input_value: folded.final_input_value,
                output_payout_value: folded.final_output_payout_value,
                change_value: folded.final_change_value,
                covenant_vk_hash: folded.covenant_vk_hash,
                prev_state_root: prev_pv.prev_state_root,
                new_state_root: folded.final_state_root,
                external_attestation_hash: folded.final_external_attestation_hash,
                prev_accumulator: prev_pv.prev_accumulator,
                new_accumulator: folded.final_accumulator,
                step_index: folded.end_step_index,
                new_oracle_watermark: folded.final_oracle_watermark,
                new_hw_watermark: folded.final_hw_watermark,
            }
        }
        None => folded.to_public_values(),
    };

    // 4. Commit canonical big-endian PublicValues bytes (exact 264-byte RFC-0021 layout).
    let canonical_bytes = committed_pv.to_canonical_bytes();
    assert_eq!(canonical_bytes.len(), PUBLIC_VALUES_ENCODED_LEN);
    sp1_zkvm::io::commit_slice(&canonical_bytes);
}
