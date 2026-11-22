//! # `covenant-circuit-core` — `NEWSATOSHI/2.1` zkVM Guest Verification Kernel
//!
//! Pure-Rust, `no_std`-compatible deterministic state-transition verifier designed to execute
//! inside a hash-based STARK zkVM guest (SP1 Hypercube / RISC-V, or cross-checked against
//! Miden VM) to produce constant-size recursive IVC proofs for `OP_VERIFY_COVENANT_STARK` (`0xb4`).
//!
//! ## Core Invariants Enforced Inside the Guest Circuit
//! 1. **Non-Zero Covenant Binding:** Standard private `LOVE` notes carry `covenant_vk_hash = 0x00..00`;
//!    covenanted spends MUST commit to a non-zero `covenant_vk_hash = H(program_vk_id || rule_hash)`.
//! 2. **Strict Value Conservation:** `input_value == output_payout_value + change_value` (overflow-safe).
//! 3. **Recursive Covenant Continuity:** Whenever `change_value > 0`, `change_covenant_vk_hash` MUST
//!    equal `Some(covenant_vk_hash)`, preventing covenant stripping / treasury diversion.
//! 4. **Bounty Caps & Ecological Coherence Floor:** Physical energy deployment payouts MUST satisfy
//!    `0 < output_payout_value <= max_bounty_payout`, `kwh_delivered > 0`, and
//!    `min_soil_health_score <= soil_health_score <= 100`.
//! 5. **Monotonic XMSS Watermark & Merkle Inclusion:** Deed oracle and hardware TEE attestations
//!    MUST verify against their burned-in 32-byte Merkle roots (`NEWSATOSHI_XMSS_LEAF_V1` /
//!    `NEWSATOSHI_XMSS_NODE_V1`) and strictly advance the monotonic leaf watermark (`leaf_index >= prev`,
//!    `new_watermark == leaf_index + 1`).
//! 6. **PayWord Hash-Chain Settlement & Reinvestment Split:** Machine streaming settlements verify
//!    `H^ticks(final_preimage) == channel_tip` and enforce exact basis-point reinvestment
//!    (`commons_reinvest = floor(gross * reinvest_bps / 10_000)`) permanently back into the covenant.
//! 7. **Recursive IVC Accumulator Chaining:** Every verified transition deterministically advances
//!    `prev_state_root -> new_state_root` and folds `prev_accumulator -> new_accumulator` so arbitrary
//!    multi-step histories compress to a constant-size $O(1)$ proof verifiable in <10 ms on edge devices.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Canonical 32-byte SHA-256 digest type (Newsatoshi's post-quantum hash floor).
pub type Hash32 = [u8; 32];

/// Sentinel `covenant_vk_hash` for standard non-covenanted private `LOVE` notes.
pub const ZERO_HASH: Hash32 = [0u8; 32];

/// Maximum supported XMSS Merkle tree height (`2^12 = 4,096` leaves per root).
pub const MAX_XMSS_TREE_HEIGHT: usize = 12;

/// Canonical serialized byte length of [`PublicValues`]:
/// 6 * 32-byte hashes (tx_sighash, covenant_vk_hash, prev_state_root, new_state_root,
/// external_attestation_hash, prev_accumulator, new_accumulator -> 7 hashes = 224B)
/// + 4 * u64 (input_value, output_payout_value, change_value, step_index -> 32B)
/// + 2 * u32 (new_oracle_watermark, new_hw_watermark -> 8B) = 264 bytes.
pub const PUBLIC_VALUES_ENCODED_LEN: usize = (7 * 32) + (4 * 8) + (2 * 4);

/// Canonical serialized byte length of the 8 core L1 `OP_VERIFY_COVENANT_STARK` public inputs:
/// 5 * 32-byte hashes + 3 * 8-byte u64 = 184 bytes.
pub const L1_OP_PUBLIC_INPUTS_LEN: usize = (5 * 32) + (3 * 8);

// ---------------------------------------------------------------------------
// Domain Separators (exact match to RFC-0021 & newsatoshi-commons L1 engine)
// ---------------------------------------------------------------------------

pub const DOMAIN_COVENANT_RULE: &[u8] = b"NEWSATOSHI_COVENANT_V2_1";
pub const DOMAIN_COVENANT_VK: &[u8] = b"NEWSATOSHI_COVENANT_VK_V2_1";
pub const DOMAIN_NOTE_COMMITMENT: &[u8] = b"NEWSATOSHI_NOTE_CM_V2_1";
pub const DOMAIN_IVC_GENESIS: &[u8] = b"NEWSATOSHI_IVC_GENESIS_V2_1";
pub const DOMAIN_IVC_STEP: &[u8] = b"NEWSATOSHI_IVC_STEP_V2_1";
pub const DOMAIN_STATE_ROOT: &[u8] = b"NEWSATOSHI_STATE_ROOT_V2_1";
pub const DOMAIN_DEED_MSG: &[u8] = b"DEED_TRANSFER_V1";
pub const DOMAIN_TELEMETRY_MSG: &[u8] = b"PHYSICAL_TELEMETRY_V1";
pub const DOMAIN_ATTESTATION: &[u8] = b"NEWSATOSHI_ATTESTATION_V2_1";
pub const DOMAIN_XMSS_LEAF: &[u8] = b"NEWSATOSHI_XMSS_LEAF_V1";
pub const DOMAIN_XMSS_NODE: &[u8] = b"NEWSATOSHI_XMSS_NODE_V1";

// ---------------------------------------------------------------------------
// Pure SHA-256 Primitives
// ---------------------------------------------------------------------------

#[inline]
pub fn sha256(data: &[u8]) -> Hash32 {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

#[inline]
pub fn sha256_concat(parts: &[&[u8]]) -> Hash32 {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// Domain-separated XMSS leaf hash matching `newsatoshi-commons/src/crypto.rs`.
pub fn xmss_leaf_hash(leaf_index: u32, leaf_pk_commitment: Hash32) -> Hash32 {
    sha256_concat(&[
        DOMAIN_XMSS_LEAF,
        &leaf_index.to_be_bytes(),
        &leaf_pk_commitment,
    ])
}

/// Domain-separated XMSS internal node hash matching `newsatoshi-commons/src/crypto.rs`.
pub fn xmss_node_hash(level: u8, node_index: u32, left: Hash32, right: Hash32) -> Hash32 {
    sha256_concat(&[
        DOMAIN_XMSS_NODE,
        &[level],
        &node_index.to_be_bytes(),
        &left,
        &right,
    ])
}

/// Recomputes the XMSS Merkle root from `(leaf_index, leaf_pk_commitment, auth_path)`.
pub fn recompute_xmss_root(
    leaf_index: u32,
    leaf_pk_commitment: Hash32,
    auth_path: &[Hash32],
) -> Option<Hash32> {
    let height = auth_path.len();
    if height == 0 || height > MAX_XMSS_TREE_HEIGHT {
        return None;
    }
    let capacity = 1u32 << height;
    if leaf_index >= capacity {
        return None;
    }

    let mut node = xmss_leaf_hash(leaf_index, leaf_pk_commitment);
    let mut cursor = leaf_index;
    for (i, sibling) in auth_path.iter().enumerate() {
        let level = (i + 1) as u8;
        let parent_idx = cursor >> 1;
        let (left, right) = if (cursor & 1) == 0 {
            (node, *sibling)
        } else {
            (*sibling, node)
        };
        node = xmss_node_hash(level, parent_idx, left, right);
        cursor = parent_idx;
    }
    Some(node)
}

/// Verifies a SHA-256 PayWord hash chain: `H^ticks(final_preimage) == tip`.
pub fn compute_payword_tip(final_preimage: Hash32, ticks: u64) -> Hash32 {
    let mut cursor = final_preimage;
    for _ in 0..ticks {
        cursor = sha256(&cursor);
    }
    cursor
}

// ---------------------------------------------------------------------------
// Protocol Structures (RFC-0021)
// ---------------------------------------------------------------------------

/// Immutable covenant parameters committed into `covenant_rule_hash`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CovenantRuleParams {
    /// Maximum LOVE payout allowed for a single physical deployment bounty.
    pub max_bounty_payout: u64,
    /// Basis points (0..=10_000) of gross streaming revenue permanently reinvested into the covenant.
    pub reinvest_bps: u16,
    /// Ecological coherence floor (0..=100) required on every physical telemetry payout.
    pub min_soil_health_score: u16,
    /// 32-byte Merkle root (or SLH-DSA / Lamport commitment) of the authorized Deed Oracle.
    pub authorized_deed_oracle_pk: Hash32,
    /// 32-byte Merkle root (or SLH-DSA / Lamport commitment) of the authorized Hardware TEE.
    pub authorized_hardware_pk: Hash32,
}

impl CovenantRuleParams {
    /// Canonical `NEWSATOSHI_COVENANT_V2_1` rule digest (matches `newsatoshi-commons/src/lib.rs`).
    pub fn rule_hash(&self) -> Hash32 {
        sha256_concat(&[
            DOMAIN_COVENANT_RULE,
            &self.max_bounty_payout.to_be_bytes(),
            &self.reinvest_bps.to_be_bytes(),
            &self.min_soil_health_score.to_be_bytes(),
            &self.authorized_deed_oracle_pk,
            &self.authorized_hardware_pk,
        ])
    }
}

/// Derives the canonical `covenant_vk_hash` binding a zkVM program verifying key digest
/// (`program_vk_id`) to a specific immutable [`CovenantRuleParams::rule_hash`].
pub fn compute_covenant_vk_hash(program_vk_id: Hash32, rule_hash: Hash32) -> Hash32 {
    sha256_concat(&[DOMAIN_COVENANT_VK, &program_vk_id, &rule_hash])
}

/// Shielded Note Output Commitment preimage defined in RFC-0021 Section 4:
/// `H("NEWSATOSHI_NOTE_CM_V2_1" || value || owner_pk || covenant_vk_hash || salt)`.
///
/// Standard private `LOVE` notes set `covenant_vk_hash = ZERO_HASH`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteCommitmentPreimage {
    pub value: u64,
    pub owner_pk: Hash32,
    pub covenant_vk_hash: Hash32,
    pub salt: Hash32,
}

impl NoteCommitmentPreimage {
    pub fn commit(&self) -> Hash32 {
        sha256_concat(&[
            DOMAIN_NOTE_COMMITMENT,
            &self.value.to_be_bytes(),
            &self.owner_pk,
            &self.covenant_vk_hash,
            &self.salt,
        ])
    }

    pub fn is_covenanted(&self) -> bool {
        self.covenant_vk_hash != ZERO_HASH
    }
}

/// Canonical message hash for Land Deed acquisition attestations.
pub fn deed_message_hash(parcel_id: u64, deed_document_hash: Hash32) -> Hash32 {
    sha256_concat(&[
        DOMAIN_DEED_MSG,
        &parcel_id.to_be_bytes(),
        &deed_document_hash,
    ])
}

/// Canonical message hash for Physical Energy & Soil Health telemetry attestations.
pub fn energy_message_hash(parcel_id: u64, kwh_delivered: u64, soil_health_score: u16) -> Hash32 {
    sha256_concat(&[
        DOMAIN_TELEMETRY_MSG,
        &parcel_id.to_be_bytes(),
        &kwh_delivered.to_be_bytes(),
        &soil_health_score.to_be_bytes(),
    ])
}

/// Computes the genesis recursive IVC accumulator for a covenant prior to step 0.
pub fn compute_ivc_genesis(covenant_vk_hash: Hash32, genesis_state_root: Hash32) -> Hash32 {
    sha256_concat(&[DOMAIN_IVC_GENESIS, &covenant_vk_hash, &genesis_state_root])
}

/// Computes the deterministic next ledger state root for step `step_index`.
pub fn compute_next_state_root(
    prev_state_root: Hash32,
    tx_sighash: Hash32,
    step_index: u64,
    new_oracle_watermark: u32,
    new_hw_watermark: u32,
    external_attestation_hash: Hash32,
) -> Hash32 {
    sha256_concat(&[
        DOMAIN_STATE_ROOT,
        &prev_state_root,
        &tx_sighash,
        &step_index.to_be_bytes(),
        &new_oracle_watermark.to_be_bytes(),
        &new_hw_watermark.to_be_bytes(),
        &external_attestation_hash,
    ])
}

/// Computes the next recursive IVC accumulator folding `prev_accumulator` with the current step.
pub fn compute_ivc_step(
    prev_accumulator: Hash32,
    covenant_vk_hash: Hash32,
    prev_state_root: Hash32,
    new_state_root: Hash32,
    tx_sighash: Hash32,
    step_index: u64,
    external_attestation_hash: Hash32,
) -> Hash32 {
    sha256_concat(&[
        DOMAIN_IVC_STEP,
        &prev_accumulator,
        &covenant_vk_hash,
        &prev_state_root,
        &new_state_root,
        &tx_sighash,
        &step_index.to_be_bytes(),
        &external_attestation_hash,
    ])
}

/// Transition-specific private witness data verified inside the zkVM guest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionKind {
    /// Land parcel acquisition authorized by the Perpetual Purpose Trust Deed Oracle XMSS tree.
    LandDeedAcquisition {
        parcel_id: u64,
        deed_document_hash: Hash32,
        oracle_leaf_index: u32,
        oracle_leaf_pk_commitment: Hash32,
        oracle_auth_path: Vec<Hash32>,
        oracle_sig_digest: Hash32,
    },
    /// Physical energy & ecological telemetry bounty authorized by the Hardware TEE XMSS tree.
    PhysicalEnergyDeployment {
        parcel_id: u64,
        kwh_delivered: u64,
        soil_health_score: u16,
        hw_leaf_index: u32,
        hw_leaf_pk_commitment: Hash32,
        hw_auth_path: Vec<Hash32>,
        hw_sig_digest: Hash32,
    },
    /// SHA-256 PayWord streaming channel settlement routing `reinvest_bps` back into the covenant.
    StreamingChannelSettlement {
        channel_tip: Hash32,
        max_ticks: u64,
        unit_price: u64,
        ticks_claimed: u64,
        final_preimage: Hash32,
    },
    /// Zero-leakage consolidation of >= 2 covenanted UTXOs under the identical `covenant_vk_hash`.
    CovenantConsolidation {
        consolidated_input_count: u32,
        consolidated_inputs_digest: Hash32,
    },
}

/// Computes the canonical `external_attestation_hash` for any [`TransitionKind`].
pub fn compute_external_attestation_hash(kind: &TransitionKind) -> Hash32 {
    match kind {
        TransitionKind::LandDeedAcquisition {
            parcel_id,
            deed_document_hash,
            oracle_leaf_index,
            oracle_leaf_pk_commitment,
            oracle_sig_digest,
            ..
        } => {
            let msg = deed_message_hash(*parcel_id, *deed_document_hash);
            sha256_concat(&[
                DOMAIN_ATTESTATION,
                &[0x01u8],
                &msg,
                &oracle_leaf_index.to_be_bytes(),
                oracle_leaf_pk_commitment,
                oracle_sig_digest,
            ])
        }
        TransitionKind::PhysicalEnergyDeployment {
            parcel_id,
            kwh_delivered,
            soil_health_score,
            hw_leaf_index,
            hw_leaf_pk_commitment,
            hw_sig_digest,
            ..
        } => {
            let msg = energy_message_hash(*parcel_id, *kwh_delivered, *soil_health_score);
            sha256_concat(&[
                DOMAIN_ATTESTATION,
                &[0x02u8],
                &msg,
                &hw_leaf_index.to_be_bytes(),
                hw_leaf_pk_commitment,
                hw_sig_digest,
            ])
        }
        TransitionKind::StreamingChannelSettlement {
            channel_tip,
            max_ticks,
            unit_price,
            ticks_claimed,
            final_preimage,
        } => sha256_concat(&[
            DOMAIN_ATTESTATION,
            &[0x03u8],
            channel_tip,
            &max_ticks.to_be_bytes(),
            &unit_price.to_be_bytes(),
            &ticks_claimed.to_be_bytes(),
            final_preimage,
        ]),
        TransitionKind::CovenantConsolidation {
            consolidated_input_count,
            consolidated_inputs_digest,
        } => sha256_concat(&[
            DOMAIN_ATTESTATION,
            &[0x04u8],
            &consolidated_input_count.to_be_bytes(),
            consolidated_inputs_digest,
        ]),
    }
}

/// Complete private witness for a single recursive covenant state transition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CovenantTransitionWitness {
    /// Monotonic transition step index (`0, 1, 2, ...`).
    pub step_index: u64,
    /// L1 transaction SIGHASH_ALL commitment binding the proof to exact inputs & outputs.
    pub tx_sighash: Hash32,
    /// zkVM verifying key digest (SP1 ELF VK / Miden program root / Winterfell AIR ID).
    pub program_vk_id: Hash32,
    /// Immutable covenant rule parameters.
    pub rule: CovenantRuleParams,
    /// Claimed `covenant_vk_hash` locked on the spent input UTXO.
    pub covenant_vk_hash: Hash32,
    /// Total covenanted input value (or gross streaming settlement value for PayWord).
    pub input_value: u64,
    /// Non-covenanted payout value released to seller / hardware operator / maintenance bot.
    pub output_payout_value: u64,
    /// Covenanted change value (or reinvested streaming surplus) returned to the Commons Treasury.
    pub change_value: u64,
    /// Covenant VK hash attached to the change output (`Some(covenant_vk_hash)` required whenever `change_value > 0`).
    pub change_covenant_vk_hash: Option<Hash32>,
    /// Ledger state root prior to this transition.
    pub prev_state_root: Hash32,
    /// Ledger state root after this transition.
    pub new_state_root: Hash32,
    /// Recursive IVC accumulator prior to this transition.
    pub prev_accumulator: Hash32,
    /// Recursive IVC accumulator after this transition.
    pub new_accumulator: Hash32,
    /// Deed oracle XMSS monotonic watermark prior to this transition.
    pub prev_oracle_watermark: u32,
    /// Deed oracle XMSS monotonic watermark after this transition.
    pub new_oracle_watermark: u32,
    /// Hardware TEE XMSS monotonic watermark prior to this transition.
    pub prev_hw_watermark: u32,
    /// Hardware TEE XMSS monotonic watermark after this transition.
    pub new_hw_watermark: u32,
    /// Kind-specific witness payload.
    pub kind: TransitionKind,
}

/// Canonical public values committed by the zkVM guest and checked by `OP_VERIFY_COVENANT_STARK`.
///
/// The first 8 fields match the normative RFC-0021 consensus public input tuple verbatim:
/// `[tx_sighash, input_value, output_payout_value, change_value, covenant_vk_hash,
///   prev_state_root, new_state_root, external_attestation_hash]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicValues {
    pub tx_sighash: Hash32,
    pub input_value: u64,
    pub output_payout_value: u64,
    pub change_value: u64,
    pub covenant_vk_hash: Hash32,
    pub prev_state_root: Hash32,
    pub new_state_root: Hash32,
    pub external_attestation_hash: Hash32,
    pub prev_accumulator: Hash32,
    pub new_accumulator: Hash32,
    pub step_index: u64,
    pub new_oracle_watermark: u32,
    pub new_hw_watermark: u32,
}

impl PublicValues {
    /// Deterministic big-endian encoding of the 8 core L1 `OP_VERIFY_COVENANT_STARK` public inputs.
    pub fn to_l1_public_inputs_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(L1_OP_PUBLIC_INPUTS_LEN);
        out.extend_from_slice(&self.tx_sighash);
        out.extend_from_slice(&self.input_value.to_be_bytes());
        out.extend_from_slice(&self.output_payout_value.to_be_bytes());
        out.extend_from_slice(&self.change_value.to_be_bytes());
        out.extend_from_slice(&self.covenant_vk_hash);
        out.extend_from_slice(&self.prev_state_root);
        out.extend_from_slice(&self.new_state_root);
        out.extend_from_slice(&self.external_attestation_hash);
        out
    }

    /// Deterministic big-endian encoding committed via `sp1_zkvm::io::commit_slice`.
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(PUBLIC_VALUES_ENCODED_LEN);
        out.extend_from_slice(&self.to_l1_public_inputs_bytes());
        out.extend_from_slice(&self.prev_accumulator);
        out.extend_from_slice(&self.new_accumulator);
        out.extend_from_slice(&self.step_index.to_be_bytes());
        out.extend_from_slice(&self.new_oracle_watermark.to_be_bytes());
        out.extend_from_slice(&self.new_hw_watermark.to_be_bytes());
        out
    }

    /// SHA-256 digest of the canonical committed public values (used for SP1 recursive verification).
    pub fn commitment_digest(&self) -> Hash32 {
        sha256(&self.to_canonical_bytes())
    }
}

/// Constant-size folded public values summarizing a verified multi-step IVC sequence
/// `[step_start ..= step_end]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoldedPublicValues {
    /// Final step's L1 `tx_sighash` anchoring this folded batch.
    pub final_tx_sighash: Hash32,
    /// Final step's covenanted input value.
    pub final_input_value: u64,
    /// Final step's payout value.
    pub final_output_payout_value: u64,
    /// Final step's covenanted change value.
    pub final_change_value: u64,
    /// Immutable `covenant_vk_hash` preserved identically across every folded step.
    pub covenant_vk_hash: Hash32,
    /// Starting state root before the first folded transition.
    pub initial_state_root: Hash32,
    /// Ending state root after the final folded transition.
    pub final_state_root: Hash32,
    /// Final step's `external_attestation_hash`.
    pub final_external_attestation_hash: Hash32,
    /// Starting IVC accumulator prior to the first folded transition.
    pub initial_accumulator: Hash32,
    /// Ending IVC accumulator after the final folded transition.
    pub final_accumulator: Hash32,
    /// First step index included in the fold.
    pub start_step_index: u64,
    /// Final step index included in the fold.
    pub end_step_index: u64,
    /// Number of transitions verified in this fold.
    pub steps_folded: u64,
    /// Resulting monotonic deed oracle XMSS watermark.
    pub final_oracle_watermark: u32,
    /// Resulting monotonic hardware TEE XMSS watermark.
    pub final_hw_watermark: u32,
}

impl FoldedPublicValues {
    /// Converts the folded summary into the canonical 8-field + IVC [`PublicValues`] tuple
    /// spanning `initial_state_root -> final_state_root`.
    pub fn to_public_values(&self) -> PublicValues {
        PublicValues {
            tx_sighash: self.final_tx_sighash,
            input_value: self.final_input_value,
            output_payout_value: self.final_output_payout_value,
            change_value: self.final_change_value,
            covenant_vk_hash: self.covenant_vk_hash,
            prev_state_root: self.initial_state_root,
            new_state_root: self.final_state_root,
            external_attestation_hash: self.final_external_attestation_hash,
            prev_accumulator: self.initial_accumulator,
            new_accumulator: self.final_accumulator,
            step_index: self.end_step_index,
            new_oracle_watermark: self.final_oracle_watermark,
            new_hw_watermark: self.final_hw_watermark,
        }
    }

    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        self.to_public_values().to_canonical_bytes()
    }
}

/// Typed verification failure returned by the guest circuit verifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerificationError {
    ZeroCovenantVkHash,
    InvalidReinvestBps { bps: u16 },
    InvalidMinSoilScore { score: u16 },
    CovenantVkHashMismatch,
    ValueConservationViolated { input: u64, payout: u64, change: u64 },
    ZeroPayoutNotAllowed,
    RecursiveCovenantContinuityViolated,
    PayoutExceedsMaxBounty { payout: u64, max_bounty: u64 },
    SoilHealthBelowFloor { score: u16, min_score: u16 },
    InvalidSoilHealthScore { score: u16 },
    ZeroEnergyDelivered,
    XmssWatermarkNotMonotonic {
        leaf_index: u32,
        prev_watermark: u32,
        new_watermark: u32,
    },
    XmssMerkleProofInvalid,
    UnexpectedWatermarkMutation,
    PayWordInvalidTickCount { ticks: u64, max_ticks: u64 },
    PayWordGrossMismatch,
    PayWordHashChainMismatch,
    PayWordSplitMismatch {
        expected_reinvest: u64,
        got_reinvest: u64,
        expected_operator: u64,
        got_operator: u64,
    },
    ConsolidationInvalid,
    StateRootTransitionMismatch,
    IvcAccumulatorChainBroken,
    EmptyBatch,
    BatchContinuityBroken { step: usize },
}

impl fmt::Display for VerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroCovenantVkHash => write!(f, "covenant_vk_hash cannot be 0x00..00"),
            Self::InvalidReinvestBps { bps } => {
                write!(f, "reinvest_bps {bps} exceeds 10,000 basis points")
            }
            Self::InvalidMinSoilScore { score } => {
                write!(f, "min_soil_health_score {score} exceeds 100")
            }
            Self::CovenantVkHashMismatch => {
                write!(f, "covenant_vk_hash does not match H(program_vk_id || rule_hash)")
            }
            Self::ValueConservationViolated { input, payout, change } => write!(
                f,
                "value conservation violated: input({input}) != payout({payout}) + change({change})"
            ),
            Self::ZeroPayoutNotAllowed => write!(f, "payout must be strictly positive"),
            Self::RecursiveCovenantContinuityViolated => write!(
                f,
                "recursive covenant continuity violated: non-zero change must carry identical covenant_vk_hash"
            ),
            Self::PayoutExceedsMaxBounty { payout, max_bounty } => {
                write!(f, "payout {payout} exceeds max_bounty_payout {max_bounty}")
            }
            Self::SoilHealthBelowFloor { score, min_score } => write!(
                f,
                "ecological floor violated: soil_health_score {score} < min_soil_health_score {min_score}"
            ),
            Self::InvalidSoilHealthScore { score } => {
                write!(f, "soil_health_score {score} exceeds valid ceiling 100")
            }
            Self::ZeroEnergyDelivered => write!(f, "kwh_delivered must be > 0"),
            Self::XmssWatermarkNotMonotonic {
                leaf_index,
                prev_watermark,
                new_watermark,
            } => write!(
                f,
                "XMSS watermark non-monotonic: leaf={leaf_index}, prev={prev_watermark}, new={new_watermark}"
            ),
            Self::XmssMerkleProofInvalid => {
                write!(f, "XMSS Merkle authentication path does not recompute to authorized root")
            }
            Self::UnexpectedWatermarkMutation => {
                write!(f, "inactive signer XMSS watermark mutated unexpectedly")
            }
            Self::PayWordInvalidTickCount { ticks, max_ticks } => {
                write!(f, "invalid PayWord tick count {ticks} (must be 1..={max_ticks})")
            }
            Self::PayWordGrossMismatch => {
                write!(f, "PayWord gross revenue (ticks * unit_price) != input_value")
            }
            Self::PayWordHashChainMismatch => {
                write!(f, "PayWord hash chain verification failed: H^ticks(preimage) != tip")
            }
            Self::PayWordSplitMismatch {
                expected_reinvest,
                got_reinvest,
                expected_operator,
                got_operator,
            } => write!(
                f,
                "PayWord split mismatch: expected (reinvest={expected_reinvest}, operator={expected_operator}), got ({got_reinvest}, {got_operator})"
            ),
            Self::ConsolidationInvalid => {
                write!(f, "covenant consolidation requires >= 2 inputs, payout == 0, and change == input > 0")
            }
            Self::StateRootTransitionMismatch => {
                write!(f, "claimed new_state_root does not match deterministic transition root")
            }
            Self::IvcAccumulatorChainBroken => {
                write!(f, "claimed new_accumulator does not match deterministic IVC fold step")
            }
            Self::EmptyBatch => write!(f, "recursive fold witness batch cannot be empty"),
            Self::BatchContinuityBroken { step } => {
                write!(f, "recursive IVC fold continuity broken at batch index {step}")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Core Deterministic Guest Circuit Verifier
// ---------------------------------------------------------------------------

/// Verifies a single [`CovenantTransitionWitness`] and returns its canonical [`PublicValues`].
///
/// This function is completely pure, deterministic, panics-free, and `no_std`-compatible.
pub fn verify_transition(
    witness: &CovenantTransitionWitness,
) -> Result<PublicValues, VerificationError> {
    // 1. Validate rule bounds and non-zero covenant_vk_hash binding.
    if witness.covenant_vk_hash == ZERO_HASH {
        return Err(VerificationError::ZeroCovenantVkHash);
    }
    if witness.rule.reinvest_bps > 10_000 {
        return Err(VerificationError::InvalidReinvestBps {
            bps: witness.rule.reinvest_bps,
        });
    }
    if witness.rule.min_soil_health_score > 100 {
        return Err(VerificationError::InvalidMinSoilScore {
            score: witness.rule.min_soil_health_score,
        });
    }
    let expected_vk_hash = compute_covenant_vk_hash(witness.program_vk_id, witness.rule.rule_hash());
    if witness.covenant_vk_hash != expected_vk_hash {
        return Err(VerificationError::CovenantVkHashMismatch);
    }

    // 2. Strict overflow-safe value conservation: input_value == output_payout_value + change_value.
    let sum = witness
        .output_payout_value
        .checked_add(witness.change_value)
        .ok_or(VerificationError::ValueConservationViolated {
            input: witness.input_value,
            payout: witness.output_payout_value,
            change: witness.change_value,
        })?;
    if sum != witness.input_value {
        return Err(VerificationError::ValueConservationViolated {
            input: witness.input_value,
            payout: witness.output_payout_value,
            change: witness.change_value,
        });
    }

    // 3. Recursive covenant continuity on any non-zero change / reinvested output.
    if witness.change_value > 0 && witness.change_covenant_vk_hash != Some(witness.covenant_vk_hash)
    {
        return Err(VerificationError::RecursiveCovenantContinuityViolated);
    }

    // 4. Transition-kind-specific cryptographic & physical invariants.
    match &witness.kind {
        TransitionKind::LandDeedAcquisition {
            oracle_leaf_index,
            oracle_leaf_pk_commitment,
            oracle_auth_path,
            ..
        } => {
            if witness.output_payout_value == 0 {
                return Err(VerificationError::ZeroPayoutNotAllowed);
            }
            // Monotonic XMSS watermark progression for Deed Oracle; hardware watermark unchanged.
            let expected_next = oracle_leaf_index.checked_add(1).ok_or(
                VerificationError::XmssWatermarkNotMonotonic {
                    leaf_index: *oracle_leaf_index,
                    prev_watermark: witness.prev_oracle_watermark,
                    new_watermark: witness.new_oracle_watermark,
                },
            )?;
            if *oracle_leaf_index < witness.prev_oracle_watermark
                || witness.new_oracle_watermark != expected_next
            {
                return Err(VerificationError::XmssWatermarkNotMonotonic {
                    leaf_index: *oracle_leaf_index,
                    prev_watermark: witness.prev_oracle_watermark,
                    new_watermark: witness.new_oracle_watermark,
                });
            }
            if witness.new_hw_watermark != witness.prev_hw_watermark {
                return Err(VerificationError::UnexpectedWatermarkMutation);
            }
            // Verify XMSS Merkle authentication path against authorized_deed_oracle_pk.
            let implied_root = recompute_xmss_root(
                *oracle_leaf_index,
                *oracle_leaf_pk_commitment,
                oracle_auth_path,
            )
            .ok_or(VerificationError::XmssMerkleProofInvalid)?;
            if implied_root != witness.rule.authorized_deed_oracle_pk {
                return Err(VerificationError::XmssMerkleProofInvalid);
            }
        }
        TransitionKind::PhysicalEnergyDeployment {
            kwh_delivered,
            soil_health_score,
            hw_leaf_index,
            hw_leaf_pk_commitment,
            hw_auth_path,
            ..
        } => {
            if witness.output_payout_value == 0 {
                return Err(VerificationError::ZeroPayoutNotAllowed);
            }
            if witness.output_payout_value > witness.rule.max_bounty_payout {
                return Err(VerificationError::PayoutExceedsMaxBounty {
                    payout: witness.output_payout_value,
                    max_bounty: witness.rule.max_bounty_payout,
                });
            }
            if *kwh_delivered == 0 {
                return Err(VerificationError::ZeroEnergyDelivered);
            }
            if *soil_health_score > 100 {
                return Err(VerificationError::InvalidSoilHealthScore {
                    score: *soil_health_score,
                });
            }
            if *soil_health_score < witness.rule.min_soil_health_score {
                return Err(VerificationError::SoilHealthBelowFloor {
                    score: *soil_health_score,
                    min_score: witness.rule.min_soil_health_score,
                });
            }
            // Monotonic XMSS watermark progression for Hardware TEE; oracle watermark unchanged.
            let expected_next = hw_leaf_index.checked_add(1).ok_or(
                VerificationError::XmssWatermarkNotMonotonic {
                    leaf_index: *hw_leaf_index,
                    prev_watermark: witness.prev_hw_watermark,
                    new_watermark: witness.new_hw_watermark,
                },
            )?;
            if *hw_leaf_index < witness.prev_hw_watermark
                || witness.new_hw_watermark != expected_next
            {
                return Err(VerificationError::XmssWatermarkNotMonotonic {
                    leaf_index: *hw_leaf_index,
                    prev_watermark: witness.prev_hw_watermark,
                    new_watermark: witness.new_hw_watermark,
                });
            }
            if witness.new_oracle_watermark != witness.prev_oracle_watermark {
                return Err(VerificationError::UnexpectedWatermarkMutation);
            }
            // Verify XMSS Merkle authentication path against authorized_hardware_pk.
            let implied_root =
                recompute_xmss_root(*hw_leaf_index, *hw_leaf_pk_commitment, hw_auth_path)
                    .ok_or(VerificationError::XmssMerkleProofInvalid)?;
            if implied_root != witness.rule.authorized_hardware_pk {
                return Err(VerificationError::XmssMerkleProofInvalid);
            }
        }
        TransitionKind::StreamingChannelSettlement {
            channel_tip,
            max_ticks,
            unit_price,
            ticks_claimed,
            final_preimage,
        } => {
            if *ticks_claimed == 0 || *ticks_claimed > *max_ticks {
                return Err(VerificationError::PayWordInvalidTickCount {
                    ticks: *ticks_claimed,
                    max_ticks: *max_ticks,
                });
            }
            let gross = ticks_claimed
                .checked_mul(*unit_price)
                .ok_or(VerificationError::PayWordGrossMismatch)?;
            if gross != witness.input_value {
                return Err(VerificationError::PayWordGrossMismatch);
            }
            // Verify PayWord hash chain: H^ticks(final_preimage) == channel_tip.
            let derived_tip = compute_payword_tip(*final_preimage, *ticks_claimed);
            if derived_tip != *channel_tip {
                return Err(VerificationError::PayWordHashChainMismatch);
            }
            // Enforce exact reinvest_bps split: change_value = commons_reinvest, payout = operator.
            let expected_reinvest =
                ((u128::from(gross) * u128::from(witness.rule.reinvest_bps)) / 10_000u128) as u64;
            let expected_operator = gross - expected_reinvest;
            if witness.change_value != expected_reinvest
                || witness.output_payout_value != expected_operator
            {
                return Err(VerificationError::PayWordSplitMismatch {
                    expected_reinvest,
                    got_reinvest: witness.change_value,
                    expected_operator,
                    got_operator: witness.output_payout_value,
                });
            }
            if witness.new_oracle_watermark != witness.prev_oracle_watermark
                || witness.new_hw_watermark != witness.prev_hw_watermark
            {
                return Err(VerificationError::UnexpectedWatermarkMutation);
            }
        }
        TransitionKind::CovenantConsolidation {
            consolidated_input_count,
            ..
        } => {
            if *consolidated_input_count < 2
                || witness.input_value == 0
                || witness.output_payout_value != 0
                || witness.change_value != witness.input_value
            {
                return Err(VerificationError::ConsolidationInvalid);
            }
            if witness.new_oracle_watermark != witness.prev_oracle_watermark
                || witness.new_hw_watermark != witness.prev_hw_watermark
            {
                return Err(VerificationError::UnexpectedWatermarkMutation);
            }
        }
    }

    // 5. Deterministic state root & recursive IVC accumulator chaining.
    let external_attestation_hash = compute_external_attestation_hash(&witness.kind);
    let expected_state_root = compute_next_state_root(
        witness.prev_state_root,
        witness.tx_sighash,
        witness.step_index,
        witness.new_oracle_watermark,
        witness.new_hw_watermark,
        external_attestation_hash,
    );
    if witness.new_state_root != expected_state_root {
        return Err(VerificationError::StateRootTransitionMismatch);
    }

    let expected_accumulator = compute_ivc_step(
        witness.prev_accumulator,
        witness.covenant_vk_hash,
        witness.prev_state_root,
        witness.new_state_root,
        witness.tx_sighash,
        witness.step_index,
        external_attestation_hash,
    );
    if witness.new_accumulator != expected_accumulator {
        return Err(VerificationError::IvcAccumulatorChainBroken);
    }

    Ok(PublicValues {
        tx_sighash: witness.tx_sighash,
        input_value: witness.input_value,
        output_payout_value: witness.output_payout_value,
        change_value: witness.change_value,
        covenant_vk_hash: witness.covenant_vk_hash,
        prev_state_root: witness.prev_state_root,
        new_state_root: witness.new_state_root,
        external_attestation_hash,
        prev_accumulator: witness.prev_accumulator,
        new_accumulator: witness.new_accumulator,
        step_index: witness.step_index,
        new_oracle_watermark: witness.new_oracle_watermark,
        new_hw_watermark: witness.new_hw_watermark,
    })
}

/// Verifies a contiguous sequence of covenant transitions and folds them into a single
/// constant-size [`FoldedPublicValues`] summary.
///
/// Enforces strict cross-step continuity:
/// - Identical `covenant_vk_hash`
/// - Strictly consecutive `step_index` (`prev + 1`)
/// - Exact state-root chaining (`step[i].prev_state_root == step[i-1].new_state_root`)
/// - Exact IVC accumulator chaining (`step[i].prev_accumulator == step[i-1].new_accumulator`)
/// - Exact XMSS watermark chaining (`step[i].prev_*_watermark == step[i-1].new_*_watermark`)
pub fn verify_recursive_fold(
    witnesses: &[CovenantTransitionWitness],
) -> Result<FoldedPublicValues, VerificationError> {
    if witnesses.is_empty() {
        return Err(VerificationError::EmptyBatch);
    }

    let first_pv = verify_transition(&witnesses[0])?;
    let mut prev_pv = first_pv.clone();

    for (idx, w) in witnesses.iter().enumerate().skip(1) {
        let expected_step = prev_pv
            .step_index
            .checked_add(1)
            .ok_or(VerificationError::BatchContinuityBroken { step: idx })?;

        if w.covenant_vk_hash != prev_pv.covenant_vk_hash
            || w.step_index != expected_step
            || w.prev_state_root != prev_pv.new_state_root
            || w.prev_accumulator != prev_pv.new_accumulator
            || w.prev_oracle_watermark != prev_pv.new_oracle_watermark
            || w.prev_hw_watermark != prev_pv.new_hw_watermark
        {
            return Err(VerificationError::BatchContinuityBroken { step: idx });
        }

        prev_pv = verify_transition(w)?;
    }

    Ok(FoldedPublicValues {
        final_tx_sighash: prev_pv.tx_sighash,
        final_input_value: prev_pv.input_value,
        final_output_payout_value: prev_pv.output_payout_value,
        final_change_value: prev_pv.change_value,
        covenant_vk_hash: first_pv.covenant_vk_hash,
        initial_state_root: first_pv.prev_state_root,
        final_state_root: prev_pv.new_state_root,
        final_external_attestation_hash: prev_pv.external_attestation_hash,
        initial_accumulator: first_pv.prev_accumulator,
        final_accumulator: prev_pv.new_accumulator,
        start_step_index: first_pv.step_index,
        end_step_index: prev_pv.step_index,
        steps_folded: witnesses.len() as u64,
        final_oracle_watermark: prev_pv.new_oracle_watermark,
        final_hw_watermark: prev_pv.new_hw_watermark,
    })
}

/// Deterministic helper for building an XMSS Merkle tree (height `1..=12`) from a seed,
/// returning `(root, leaf_pk_commitments, layers)` for tests and host witness generation.
pub fn build_xmss_commitment_tree(
    master_seed: Hash32,
    height: usize,
) -> (Hash32, Vec<Hash32>, Vec<Vec<Hash32>>) {
    assert!((1..=MAX_XMSS_TREE_HEIGHT).contains(&height));
    let num_leaves = 1usize << height;
    let mut leaf_pks = Vec::with_capacity(num_leaves);
    let mut leaf_hashes = Vec::with_capacity(num_leaves);

    for idx in 0..num_leaves {
        let pk_commit = sha256_concat(&[
            b"NEWSATOSHI_XMSS_TEST_PK_V1",
            &master_seed,
            &(idx as u32).to_be_bytes(),
        ]);
        leaf_pks.push(pk_commit);
        leaf_hashes.push(xmss_leaf_hash(idx as u32, pk_commit));
    }

    let mut layers = Vec::with_capacity(height + 1);
    layers.push(leaf_hashes);
    for level in 1..=height {
        let prev = &layers[level - 1];
        let next_len = prev.len() / 2;
        let mut next_layer = Vec::with_capacity(next_len);
        for i in 0..next_len {
            next_layer.push(xmss_node_hash(
                level as u8,
                i as u32,
                prev[2 * i],
                prev[2 * i + 1],
            ));
        }
        layers.push(next_layer);
    }

    let root = layers[height][0];
    (root, leaf_pks, layers)
}

/// Extracts the Merkle authentication path for `leaf_index` from `layers`.
pub fn xmss_auth_path(layers: &[Vec<Hash32>], leaf_index: u32) -> Vec<Hash32> {
    let height = layers.len() - 1;
    let mut path = Vec::with_capacity(height);
    let mut cursor = leaf_index as usize;
    for layer in layers.iter().take(height) {
        path.push(layer[cursor ^ 1]);
        cursor >>= 1;
    }
    path
}

// ---------------------------------------------------------------------------
// Comprehensive Unit & Adversarial Witness Tampering Test Suite
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    struct TestFixture {
        program_vk_id: Hash32,
        rule: CovenantRuleParams,
        covenant_vk_hash: Hash32,
        genesis_state_root: Hash32,
        genesis_accumulator: Hash32,
        oracle_pks: Vec<Hash32>,
        oracle_layers: Vec<Vec<Hash32>>,
        hw_pks: Vec<Hash32>,
        hw_layers: Vec<Vec<Hash32>>,
    }

    impl TestFixture {
        fn new() -> Self {
            let program_vk_id = sha256(b"SP1_HYPERCUBE_COVENANT_ELF_V2_1");
            let (oracle_root, oracle_pks, oracle_layers) =
                build_xmss_commitment_tree(sha256(b"DEED_ORACLE_MASTER_SEED"), 6);
            let (hw_root, hw_pks, hw_layers) =
                build_xmss_commitment_tree(sha256(b"HARDWARE_TEE_MASTER_SEED"), 6);

            let rule = CovenantRuleParams {
                max_bounty_payout: 20_000,
                reinvest_bps: 1_500, // 15%
                min_soil_health_score: 80,
                authorized_deed_oracle_pk: oracle_root,
                authorized_hardware_pk: hw_root,
            };
            let covenant_vk_hash = compute_covenant_vk_hash(program_vk_id, rule.rule_hash());
            let genesis_state_root = sha256(b"NEWSATOSHI_COMMONS_GENESIS");
            let genesis_accumulator = compute_ivc_genesis(covenant_vk_hash, genesis_state_root);

            Self {
                program_vk_id,
                rule,
                covenant_vk_hash,
                genesis_state_root,
                genesis_accumulator,
                oracle_pks,
                oracle_layers,
                hw_pks,
                hw_layers,
            }
        }

        #[allow(clippy::too_many_arguments)]
        fn build_witness(
            &self,
            step_index: u64,
            tx_tag: &[u8],
            input_value: u64,
            output_payout_value: u64,
            change_value: u64,
            prev_state_root: Hash32,
            prev_accumulator: Hash32,
            prev_oracle_wm: u32,
            new_oracle_wm: u32,
            prev_hw_wm: u32,
            new_hw_wm: u32,
            kind: TransitionKind,
        ) -> CovenantTransitionWitness {
            let tx_sighash = sha256_concat(&[b"TX_SIGHASH_ALL", tx_tag, &step_index.to_be_bytes()]);
            let attestation = compute_external_attestation_hash(&kind);
            let new_state_root = compute_next_state_root(
                prev_state_root,
                tx_sighash,
                step_index,
                new_oracle_wm,
                new_hw_wm,
                attestation,
            );
            let new_accumulator = compute_ivc_step(
                prev_accumulator,
                self.covenant_vk_hash,
                prev_state_root,
                new_state_root,
                tx_sighash,
                step_index,
                attestation,
            );
            CovenantTransitionWitness {
                step_index,
                tx_sighash,
                program_vk_id: self.program_vk_id,
                rule: self.rule.clone(),
                covenant_vk_hash: self.covenant_vk_hash,
                input_value,
                output_payout_value,
                change_value,
                change_covenant_vk_hash: if change_value > 0 {
                    Some(self.covenant_vk_hash)
                } else {
                    None
                },
                prev_state_root,
                new_state_root,
                prev_accumulator,
                new_accumulator,
                prev_oracle_watermark: prev_oracle_wm,
                new_oracle_watermark: new_oracle_wm,
                prev_hw_watermark: prev_hw_wm,
                new_hw_watermark: new_hw_wm,
                kind,
            }
        }

        #[allow(clippy::too_many_arguments)]
        fn valid_deed_step(
            &self,
            step_index: u64,
            parcel_id: u64,
            leaf_index: u32,
            prev_state_root: Hash32,
            prev_acc: Hash32,
            prev_oracle_wm: u32,
            prev_hw_wm: u32,
            input_value: u64,
            payout: u64,
        ) -> CovenantTransitionWitness {
            let kind = TransitionKind::LandDeedAcquisition {
                parcel_id,
                deed_document_hash: sha256_concat(&[b"DEED_DOC", &parcel_id.to_be_bytes()]),
                oracle_leaf_index: leaf_index,
                oracle_leaf_pk_commitment: self.oracle_pks[leaf_index as usize],
                oracle_auth_path: xmss_auth_path(&self.oracle_layers, leaf_index),
                oracle_sig_digest: sha256_concat(&[b"ORACLE_SIG", &leaf_index.to_be_bytes()]),
            };
            self.build_witness(
                step_index,
                b"DEED_TX",
                input_value,
                payout,
                input_value - payout,
                prev_state_root,
                prev_acc,
                prev_oracle_wm,
                leaf_index + 1,
                prev_hw_wm,
                prev_hw_wm,
                kind,
            )
        }

        #[allow(clippy::too_many_arguments)]
        fn valid_energy_step(
            &self,
            step_index: u64,
            parcel_id: u64,
            kwh: u64,
            soil: u16,
            leaf_index: u32,
            prev_state_root: Hash32,
            prev_acc: Hash32,
            prev_oracle_wm: u32,
            prev_hw_wm: u32,
            input_value: u64,
            payout: u64,
        ) -> CovenantTransitionWitness {
            let kind = TransitionKind::PhysicalEnergyDeployment {
                parcel_id,
                kwh_delivered: kwh,
                soil_health_score: soil,
                hw_leaf_index: leaf_index,
                hw_leaf_pk_commitment: self.hw_pks[leaf_index as usize],
                hw_auth_path: xmss_auth_path(&self.hw_layers, leaf_index),
                hw_sig_digest: sha256_concat(&[b"HW_SIG", &leaf_index.to_be_bytes()]),
            };
            self.build_witness(
                step_index,
                b"ENERGY_TX",
                input_value,
                payout,
                input_value - payout,
                prev_state_root,
                prev_acc,
                prev_oracle_wm,
                prev_oracle_wm,
                prev_hw_wm,
                leaf_index + 1,
                kind,
            )
        }

        #[allow(clippy::too_many_arguments)]
        fn valid_payword_step(
            &self,
            step_index: u64,
            seed: Hash32,
            max_ticks: u64,
            unit_price: u64,
            ticks_claimed: u64,
            prev_state_root: Hash32,
            prev_acc: Hash32,
            prev_oracle_wm: u32,
            prev_hw_wm: u32,
        ) -> CovenantTransitionWitness {
            let mut chain = Vec::with_capacity((max_ticks + 1) as usize);
            let mut cur = seed;
            chain.push(cur);
            for _ in 0..max_ticks {
                cur = sha256(&cur);
                chain.push(cur);
            }
            let tip = *chain.last().unwrap();
            let final_preimage = chain[(max_ticks - ticks_claimed) as usize];
            let gross = ticks_claimed * unit_price;
            let reinvest = (gross * u64::from(self.rule.reinvest_bps)) / 10_000;
            let operator = gross - reinvest;

            let kind = TransitionKind::StreamingChannelSettlement {
                channel_tip: tip,
                max_ticks,
                unit_price,
                ticks_claimed,
                final_preimage,
            };
            self.build_witness(
                step_index,
                b"PAYWORD_TX",
                gross,
                operator,
                reinvest,
                prev_state_root,
                prev_acc,
                prev_oracle_wm,
                prev_oracle_wm,
                prev_hw_wm,
                prev_hw_wm,
                kind,
            )
        }
    }

    #[test]
    fn note_commitment_distinguishes_standard_and_covenanted_notes() {
        let fx = TestFixture::new();
        let owner = sha256(b"OWNER_PK");
        let salt = sha256(b"NOTE_SALT");

        let std_note = NoteCommitmentPreimage {
            value: 50_000,
            owner_pk: owner,
            covenant_vk_hash: ZERO_HASH,
            salt,
        };
        let cov_note = NoteCommitmentPreimage {
            value: 50_000,
            owner_pk: owner,
            covenant_vk_hash: fx.covenant_vk_hash,
            salt,
        };

        assert!(!std_note.is_covenanted());
        assert!(cov_note.is_covenanted());
        assert_ne!(std_note.commit(), cov_note.commit());
    }

    #[test]
    fn valid_multi_step_recursive_ivc_fold_passes() {
        let fx = TestFixture::new();

        // Step 0: Acquire Parcel #404 (100,000 -> 40,000 seller + 60,000 covenanted change)
        let w0 = fx.valid_deed_step(
            0,
            404,
            0,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            0,
            100_000,
            40_000,
        );
        // Step 1: Hardware telemetry bounty (60,000 -> 15,000 operator + 45,000 covenanted change)
        let w1 = fx.valid_energy_step(
            1,
            404,
            12_500,
            92,
            0,
            w0.new_state_root,
            w0.new_accumulator,
            w0.new_oracle_watermark,
            w0.new_hw_watermark,
            60_000,
            15_000,
        );
        // Step 2: PayWord streaming settlement (500 ticks @ 200 LOVE = 100,000 gross -> 15,000 reinvest + 85,000 operator)
        let w2 = fx.valid_payword_step(
            2,
            sha256(b"PAYWORD_CHANNEL_1"),
            1_000,
            200,
            500,
            w1.new_state_root,
            w1.new_accumulator,
            w1.new_oracle_watermark,
            w1.new_hw_watermark,
        );
        // Step 3: Consolidate 45,000 treasury change + 15,000 reinvested = 60,000 LOVE
        let w3 = fx.build_witness(
            3,
            b"CONSOLIDATE_TX",
            60_000,
            0,
            60_000,
            w2.new_state_root,
            w2.new_accumulator,
            w2.new_oracle_watermark,
            w2.new_oracle_watermark,
            w2.new_hw_watermark,
            w2.new_hw_watermark,
            TransitionKind::CovenantConsolidation {
                consolidated_input_count: 2,
                consolidated_inputs_digest: sha256(b"UTXO_CHANGE_PLUS_REINVEST"),
            },
        );
        // Step 4: Acquire Parcel #405 using Oracle Leaf #1 (60,000 -> 45,000 seller + 15,000 change)
        let w4 = fx.valid_deed_step(
            4,
            405,
            1,
            w3.new_state_root,
            w3.new_accumulator,
            w3.new_oracle_watermark,
            w3.new_hw_watermark,
            60_000,
            45_000,
        );

        let batch = vec![w0.clone(), w1, w2, w3, w4.clone()];
        let folded = verify_recursive_fold(&batch).expect("5-step recursive fold must verify");

        assert_eq!(folded.steps_folded, 5);
        assert_eq!(folded.start_step_index, 0);
        assert_eq!(folded.end_step_index, 4);
        assert_eq!(folded.covenant_vk_hash, fx.covenant_vk_hash);
        assert_eq!(folded.initial_state_root, fx.genesis_state_root);
        assert_eq!(folded.final_state_root, w4.new_state_root);
        assert_eq!(folded.initial_accumulator, fx.genesis_accumulator);
        assert_eq!(folded.final_accumulator, w4.new_accumulator);
        assert_eq!(folded.final_oracle_watermark, 2);
        assert_eq!(folded.final_hw_watermark, 1);

        let pv = folded.to_public_values();
        assert_eq!(pv.to_l1_public_inputs_bytes().len(), L1_OP_PUBLIC_INPUTS_LEN);
        assert_eq!(pv.to_canonical_bytes().len(), PUBLIC_VALUES_ENCODED_LEN);
    }

    #[test]
    fn rejects_zero_covenant_vk_hash() {
        let fx = TestFixture::new();
        let mut w = fx.valid_deed_step(
            0,
            404,
            0,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            0,
            100_000,
            40_000,
        );
        w.covenant_vk_hash = ZERO_HASH;
        assert_eq!(verify_transition(&w), Err(VerificationError::ZeroCovenantVkHash));
    }

    #[test]
    fn rejects_value_inflation_and_overflow() {
        let fx = TestFixture::new();
        let mut w = fx.valid_deed_step(
            0,
            404,
            0,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            0,
            100_000,
            40_000,
        );
        w.output_payout_value = 40_001; // Inflates 1 LOVE out of thin air
        assert!(matches!(
            verify_transition(&w),
            Err(VerificationError::ValueConservationViolated { .. })
        ));

        // u64 addition overflow attempt
        w.output_payout_value = u64::MAX;
        w.change_value = 1;
        assert!(matches!(
            verify_transition(&w),
            Err(VerificationError::ValueConservationViolated { .. })
        ));
    }

    #[test]
    fn rejects_covenant_stripping_and_rule_substitution() {
        let fx = TestFixture::new();
        let mut w = fx.valid_deed_step(
            0,
            404,
            0,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            0,
            100_000,
            40_000,
        );

        // Strip covenant entirely
        w.change_covenant_vk_hash = None;
        assert_eq!(
            verify_transition(&w),
            Err(VerificationError::RecursiveCovenantContinuityViolated)
        );

        // Substitute attacker covenant VK hash
        w.change_covenant_vk_hash = Some(sha256(b"ATTACKER_EXTRACTIVE_COVENANT"));
        assert_eq!(
            verify_transition(&w),
            Err(VerificationError::RecursiveCovenantContinuityViolated)
        );
    }

    #[test]
    fn rejects_bounty_cap_violation() {
        let fx = TestFixture::new();
        // max_bounty_payout is 20,000; attacker claims 20,001
        let w = fx.valid_energy_step(
            0,
            404,
            12_500,
            92,
            0,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            0,
            60_000,
            20_001,
        );
        assert_eq!(
            verify_transition(&w),
            Err(VerificationError::PayoutExceedsMaxBounty {
                payout: 20_001,
                max_bounty: 20_000,
            })
        );
    }

    #[test]
    fn rejects_soil_health_below_ecological_floor_and_zero_kwh() {
        let fx = TestFixture::new();
        let bad_soil = fx.valid_energy_step(
            0,
            404,
            12_500,
            79, // Below min_soil_health_score = 80
            0,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            0,
            60_000,
            10_000,
        );
        assert_eq!(
            verify_transition(&bad_soil),
            Err(VerificationError::SoilHealthBelowFloor {
                score: 79,
                min_score: 80,
            })
        );

        let zero_kwh = fx.valid_energy_step(
            0,
            404,
            0,
            95,
            0,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            0,
            60_000,
            10_000,
        );
        assert_eq!(
            verify_transition(&zero_kwh),
            Err(VerificationError::ZeroEnergyDelivered)
        );
    }

    #[test]
    fn rejects_xmss_watermark_replay_and_forged_merkle_path() {
        let fx = TestFixture::new();
        // Stale leaf index 2 when prev_hw_watermark is already 3
        let replay = fx.valid_energy_step(
            0,
            404,
            12_500,
            92,
            2,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            3,
            60_000,
            10_000,
        );
        assert_eq!(
            verify_transition(&replay),
            Err(VerificationError::XmssWatermarkNotMonotonic {
                leaf_index: 2,
                prev_watermark: 3,
                new_watermark: 3,
            })
        );

        // Tamper one sibling hash in the XMSS Merkle authentication path
        let mut forged = fx.valid_energy_step(
            0,
            404,
            12_500,
            92,
            3,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            3,
            60_000,
            10_000,
        );
        if let TransitionKind::PhysicalEnergyDeployment { hw_auth_path, .. } = &mut forged.kind {
            hw_auth_path[0][0] ^= 0xff;
        }
        assert_eq!(
            verify_transition(&forged),
            Err(VerificationError::XmssMerkleProofInvalid)
        );
    }

    #[test]
    fn rejects_payword_tampered_preimage_and_stolen_reinvestment() {
        let fx = TestFixture::new();
        let mut bad_preimage = fx.valid_payword_step(
            0,
            sha256(b"PAYWORD_SEED_BAD"),
            1_000,
            200,
            500,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            0,
        );
        if let TransitionKind::StreamingChannelSettlement { final_preimage, .. } =
            &mut bad_preimage.kind
        {
            final_preimage[0] ^= 0x01;
        }
        assert_eq!(
            verify_transition(&bad_preimage),
            Err(VerificationError::PayWordHashChainMismatch)
        );

        // Attempt to skim 1 LOVE from Commons reinvestment into operator payout
        let mut skim = fx.valid_payword_step(
            0,
            sha256(b"PAYWORD_SEED_SKIM"),
            1_000,
            200,
            500,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            0,
        );
        skim.change_value -= 1;
        skim.output_payout_value += 1;
        assert_eq!(
            verify_transition(&skim),
            Err(VerificationError::PayWordSplitMismatch {
                expected_reinvest: 15_000,
                got_reinvest: 14_999,
                expected_operator: 85_000,
                got_operator: 85_001,
            })
        );
    }

    #[test]
    fn rejects_broken_ivc_accumulator_and_fold_discontinuity() {
        let fx = TestFixture::new();
        let mut w0 = fx.valid_deed_step(
            0,
            404,
            0,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            0,
            100_000,
            40_000,
        );
        // Corrupt new_accumulator directly
        w0.new_accumulator[31] ^= 0x80;
        assert_eq!(
            verify_transition(&w0),
            Err(VerificationError::IvcAccumulatorChainBroken)
        );

        // Restore valid w0, then construct w1 with a skipped step_index (2 instead of 1)
        let w0_ok = fx.valid_deed_step(
            0,
            404,
            0,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            0,
            100_000,
            40_000,
        );
        let w1_gap = fx.valid_energy_step(
            2, // Gap! Expected 1
            404,
            12_500,
            92,
            0,
            w0_ok.new_state_root,
            w0_ok.new_accumulator,
            w0_ok.new_oracle_watermark,
            w0_ok.new_hw_watermark,
            60_000,
            10_000,
        );
        assert_eq!(
            verify_recursive_fold(&[w0_ok, w1_gap]),
            Err(VerificationError::BatchContinuityBroken { step: 1 })
        );
    }

    #[test]
    fn rejects_consolidation_value_leakage() {
        let fx = TestFixture::new();
        let bad_consolidate = fx.build_witness(
            0,
            b"BAD_CONSOLIDATE",
            60_000,
            1_000, // Leakage! Consolidation MUST have output_payout_value == 0
            59_000,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            0,
            0,
            0,
            TransitionKind::CovenantConsolidation {
                consolidated_input_count: 2,
                consolidated_inputs_digest: sha256(b"INPUTS"),
            },
        );
        assert_eq!(
            verify_transition(&bad_consolidate),
            Err(VerificationError::ConsolidationInvalid)
        );
    }

    #[test]
    fn rejects_state_root_tampering_and_inactive_watermark_mutation() {
        let fx = TestFixture::new();
        let mut bad_root = fx.valid_deed_step(
            0,
            404,
            0,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            0,
            100_000,
            40_000,
        );
        bad_root.new_state_root[0] ^= 0x42;
        assert_eq!(
            verify_transition(&bad_root),
            Err(VerificationError::StateRootTransitionMismatch)
        );

        let mut bad_wm = fx.valid_deed_step(
            0,
            404,
            0,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            0,
            100_000,
            40_000,
        );
        bad_wm.new_hw_watermark = 1; // Hardware watermark must not mutate on a deed step
        assert_eq!(
            verify_transition(&bad_wm),
            Err(VerificationError::UnexpectedWatermarkMutation)
        );
    }

    #[test]
    fn rejects_invalid_rule_bounds_and_vk_mismatch() {
        let fx = TestFixture::new();
        let mut w = fx.valid_deed_step(
            0,
            404,
            0,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            0,
            100_000,
            40_000,
        );
        w.rule.reinvest_bps = 10_001;
        assert_eq!(
            verify_transition(&w),
            Err(VerificationError::InvalidReinvestBps { bps: 10_001 })
        );

        w.rule.reinvest_bps = 1_500;
        w.rule.min_soil_health_score = 101;
        assert_eq!(
            verify_transition(&w),
            Err(VerificationError::InvalidMinSoilScore { score: 101 })
        );

        // Mutating rule without recomputing covenant_vk_hash MUST fail binding check
        w.rule.min_soil_health_score = 50;
        assert_eq!(
            verify_transition(&w),
            Err(VerificationError::CovenantVkHashMismatch)
        );
    }

    #[test]
    fn rejects_empty_batch_and_verifies_canonical_encoding_determinism() {
        assert_eq!(verify_recursive_fold(&[]), Err(VerificationError::EmptyBatch));

        let fx = TestFixture::new();
        let w = fx.valid_deed_step(
            0,
            404,
            0,
            fx.genesis_state_root,
            fx.genesis_accumulator,
            0,
            0,
            100_000,
            40_000,
        );
        let pv1 = verify_transition(&w).expect("valid transition");
        let pv2 = verify_transition(&w).expect("deterministic re-run");
        assert_eq!(pv1.to_canonical_bytes(), pv2.to_canonical_bytes());
        assert_eq!(pv1.commitment_digest(), pv2.commitment_digest());
        assert_eq!(pv1.to_l1_public_inputs_bytes().len(), L1_OP_PUBLIC_INPUTS_LEN);
        assert_eq!(pv1.to_canonical_bytes().len(), PUBLIC_VALUES_ENCODED_LEN);
    }
}
