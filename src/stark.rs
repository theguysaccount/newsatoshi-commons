use crate::crypto::{hash32_from_hex, sha256_concat, Hash32};
use serde::{Deserialize, Serialize};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use winterfell::crypto::hashers::Blake3_256;
use winterfell::crypto::{DefaultRandomCoin, MerkleTree};
use winterfell::math::fields::f128::BaseElement;
use winterfell::math::{FieldElement, StarkField, ToElements};
use winterfell::matrix::ColMatrix;
use winterfell::{
    AcceptableOptions, Air, AirContext, Assertion, AuxRandElements, BatchingMethod,
    CompositionPoly, CompositionPolyTrace, ConstraintCompositionCoefficients,
    DefaultConstraintCommitment, DefaultConstraintEvaluator, DefaultTraceLde, EvaluationFrame,
    FieldExtension, PartitionOptions, Proof, ProofOptions, Prover, StarkDomain, TraceInfo,
    TracePolyTable, TraceTable, TransitionConstraintDegree,
};

/// Power-of-2 execution trace length: 64 rows => 63 bit-decomposition transitions (exact 63-bit non-negative range checks).
pub const TRACE_LENGTH: usize = 64;
/// Number of algebraic columns in `CovenantTransitionAir`.
pub const TRACE_WIDTH: usize = 15;
/// Minimum conjectured security bits required by `verify_covenant_stark`.
pub const MIN_SECURITY_BITS: u32 = 95;
/// Maximum accepted serialized Winterfell STARK proof size (128 KiB; genuine proofs are ~22 KiB).
pub const MAX_PROOF_BYTES: usize = 131_072;

type StarkHashFn = Blake3_256<BaseElement>;
type StarkVC = MerkleTree<StarkHashFn>;
type StarkCoin = DefaultRandomCoin<StarkHashFn>;

/// Kind of `CommonsLedger` state transition proved by the Winterfell STARK AIR.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CovenantTxKind {
    /// Land deed acquisition spend (`input_amount == payout + change`, recursive rule continuity).
    LandDeed,
    /// Physical telemetry bounty spend (adds `payout <= max_bounty_payout` and `soil >= min_soil`).
    TelemetryBounty,
    /// PayWord streaming channel settlement (proves exact `reinvest_bps` floor split + conservation).
    StreamingSettlement,
}

impl CovenantTxKind {
    pub fn as_u64(self) -> u64 {
        match self {
            Self::LandDeed => 0,
            Self::TelemetryBounty => 1,
            Self::StreamingSettlement => 2,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::LandDeed => "land_deed",
            Self::TelemetryBounty => "telemetry_bounty",
            Self::StreamingSettlement => "stream_settle",
        }
    }
}

/// Public inputs committed to the Winterfell STARK verifier and Fiat-Shamir public coin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CovenantPublicInputs {
    pub tx_kind: CovenantTxKind,
    pub label: String,
    pub tx_nonce: u64,
    /// Hex-encoded 32-byte input covenant rule hash.
    pub covenant_rule_hash: String,
    /// Hex-encoded 32-byte change output covenant rule hash (must match `covenant_rule_hash` for continuity).
    pub change_covenant_rule_hash: String,
    /// Hex-encoded 32-byte previous ledger state root.
    pub prev_state_root: String,
    /// Hex-encoded 32-byte new ledger state root.
    pub new_state_root: String,
    /// Input UTXO amount (or streaming gross revenue for `StreamingSettlement`).
    pub input_amount: u64,
    /// Payout amount to seller/worker (or operator payout for `StreamingSettlement`).
    pub payout_amount: u64,
    /// Recursive covenant change amount (or reinvested commons surplus for `StreamingSettlement`).
    pub change_amount: u64,
    /// Covenant `max_bounty_payout` cap.
    pub max_bounty_payout: u64,
    /// Covenant `reinvest_bps` (0..=10_000).
    pub reinvest_bps: u16,
    /// Covenant `min_soil_health_score` ecological floor.
    pub min_soil_health_score: u16,
    /// Attested `soil_health_score` for telemetry transitions (0 for non-telemetry).
    pub soil_health_score: u16,
}

/// Packs a 32-byte digest injectively into three 88/88/80-bit field elements (`<< p = 2^128 - 45*2^40 + 1`).
fn hash32_to_three_limbs(h: &Hash32) -> [BaseElement; 3] {
    let mut b0 = [0u8; 16];
    b0[5..16].copy_from_slice(&h[0..11]);
    let mut b1 = [0u8; 16];
    b1[5..16].copy_from_slice(&h[11..22]);
    let mut b2 = [0u8; 16];
    b2[6..16].copy_from_slice(&h[22..32]);

    [
        BaseElement::new(u128::from_be_bytes(b0)),
        BaseElement::new(u128::from_be_bytes(b1)),
        BaseElement::new(u128::from_be_bytes(b2)),
    ]
}

fn hex_to_three_limbs(hex_str: &str) -> [BaseElement; 3] {
    let bytes = hash32_from_hex(hex_str).unwrap_or([0u8; 32]);
    hash32_to_three_limbs(&bytes)
}

impl CovenantPublicInputs {
    /// Expected field element for the first 63-bit range-checked slack accumulator.
    pub fn expected_bounty_slack_fe(&self) -> BaseElement {
        match self.tx_kind {
            CovenantTxKind::TelemetryBounty => {
                BaseElement::from(self.max_bounty_payout) - BaseElement::from(self.payout_amount)
            }
            CovenantTxKind::LandDeed => {
                BaseElement::from(self.input_amount) - BaseElement::from(self.payout_amount)
            }
            CovenantTxKind::StreamingSettlement => {
                BaseElement::from(self.input_amount) * BaseElement::from(self.reinvest_bps as u64)
                    - BaseElement::from(self.change_amount) * BaseElement::from(10_000u64)
            }
        }
    }

    /// Expected field element for the second 63-bit range-checked slack accumulator.
    pub fn expected_soil_slack_fe(&self) -> BaseElement {
        match self.tx_kind {
            CovenantTxKind::TelemetryBounty => {
                BaseElement::from(self.soil_health_score as u64)
                    - BaseElement::from(self.min_soil_health_score as u64)
            }
            CovenantTxKind::LandDeed => BaseElement::from(self.change_amount),
            CovenantTxKind::StreamingSettlement => {
                let rem = self.expected_bounty_slack_fe();
                BaseElement::from(9_999u64) - rem
            }
        }
    }

    /// Validates public-input domain bounds and strict 32-byte hex encodings before
    /// invoking Winterfell verification (prevents field aliasing, out-of-range parameters,
    /// and silent zero-hash fallback on malformed hex strings).
    pub fn validate_verifier_bounds(&self) -> Result<(), String> {
        let max_63 = (1u64 << 63) - 1;
        if self.input_amount > max_63
            || self.payout_amount > max_63
            || self.change_amount > max_63
            || self.max_bounty_payout > max_63
        {
            return Err("amounts exceed 63-bit STARK range bound".to_string());
        }
        if self.reinvest_bps > 10_000 {
            return Err(format!(
                "reinvest_bps {} exceeds 10000 (100%)",
                self.reinvest_bps
            ));
        }
        if self.soil_health_score > 100 || self.min_soil_health_score > 100 {
            return Err("soil_health_score and min_soil_health_score must be <= 100".to_string());
        }
        let _ = hash32_from_hex(&self.covenant_rule_hash)
            .map_err(|e| format!("invalid covenant_rule_hash: {e}"))?;
        let _ = hash32_from_hex(&self.change_covenant_rule_hash)
            .map_err(|e| format!("invalid change_covenant_rule_hash: {e}"))?;
        let _ = hash32_from_hex(&self.prev_state_root)
            .map_err(|e| format!("invalid prev_state_root: {e}"))?;
        let _ = hash32_from_hex(&self.new_state_root)
            .map_err(|e| format!("invalid new_state_root: {e}"))?;
        Ok(())
    }

    /// Validates all integer witness invariants before constructing the 63-bit decomposition trace.
    fn compute_checked_slacks(&self) -> Result<(u64, u64), String> {
        self.validate_verifier_bounds()?;
        if self.payout_amount.checked_add(self.change_amount) != Some(self.input_amount) {
            return Err(format!(
                "value conservation violated: input={} != payout={} + change={}",
                self.input_amount, self.payout_amount, self.change_amount
            ));
        }
        let rule_in = hash32_from_hex(&self.covenant_rule_hash)
            .map_err(|e| format!("invalid covenant_rule_hash: {e}"))?;
        let rule_out = hash32_from_hex(&self.change_covenant_rule_hash)
            .map_err(|e| format!("invalid change_covenant_rule_hash: {e}"))?;
        if rule_in != rule_out {
            return Err("recursive covenant continuity violated in STARK public inputs".to_string());
        }

        match self.tx_kind {
            CovenantTxKind::TelemetryBounty => {
                let bounty_slack = self
                    .max_bounty_payout
                    .checked_sub(self.payout_amount)
                    .ok_or_else(|| {
                        format!(
                            "bounty cap violated: payout={} > max_bounty={}",
                            self.payout_amount, self.max_bounty_payout
                        )
                    })?;
                let soil_slack = self
                    .soil_health_score
                    .checked_sub(self.min_soil_health_score)
                    .ok_or_else(|| {
                        format!(
                            "ecological floor violated: soil={} < min_soil={}",
                            self.soil_health_score, self.min_soil_health_score
                        )
                    })?;
                Ok((bounty_slack, u64::from(soil_slack)))
            }
            CovenantTxKind::LandDeed => {
                let slack = self
                    .input_amount
                    .checked_sub(self.payout_amount)
                    .ok_or_else(|| "deed payout exceeds input amount".to_string())?;
                Ok((slack, self.change_amount))
            }
            CovenantTxKind::StreamingSettlement => {
                let lhs = (self.input_amount as u128) * (self.reinvest_bps as u128);
                let rhs = (self.change_amount as u128) * 10_000u128;
                let rem = lhs
                    .checked_sub(rhs)
                    .ok_or_else(|| "streaming reinvest exceeds bps floor bound".to_string())?;
                if rem > 9_999 {
                    return Err(format!(
                        "streaming reinvest remainder {rem} >= 10000 (under-reinvested)"
                    ));
                }
                let rem_u64 = rem as u64;
                Ok((rem_u64, 9_999 - rem_u64))
            }
        }
    }
}

impl ToElements<BaseElement> for CovenantPublicInputs {
    fn to_elements(&self) -> Vec<BaseElement> {
        let rule_in = hex_to_three_limbs(&self.covenant_rule_hash);
        let rule_out = hex_to_three_limbs(&self.change_covenant_rule_hash);
        let prev_root = hex_to_three_limbs(&self.prev_state_root);
        let new_root = hex_to_three_limbs(&self.new_state_root);

        vec![
            BaseElement::from(self.tx_kind.as_u64()),
            BaseElement::from(self.tx_nonce),
            rule_in[0],
            rule_in[1],
            rule_in[2],
            rule_out[0],
            rule_out[1],
            rule_out[2],
            prev_root[0],
            prev_root[1],
            prev_root[2],
            new_root[0],
            new_root[1],
            new_root[2],
            BaseElement::from(self.input_amount),
            BaseElement::from(self.payout_amount),
            BaseElement::from(self.change_amount),
            BaseElement::from(self.max_bounty_payout),
            BaseElement::from(self.reinvest_bps as u64),
            BaseElement::from(self.min_soil_health_score as u64),
            BaseElement::from(self.soil_health_score as u64),
        ]
    }
}

/// Winterfell 0.13 AIR proving `CommonsLedger` recursive covenant invariants:
/// 1. Recursive covenant rule continuity across every trace step (`rule_in == rule_out`).
/// 2. Value conservation (`input_amount == payout_amount + change_amount`).
/// 3. 63-bit bit-decomposition non-negativity range proofs on `payout`, `change`,
///    bounty cap slack (`max_bounty - payout >= 0`), and ecological floor slack (`soil - min_soil >= 0`),
///    or exact `reinvest_bps` floor-division bounds for streaming settlements.
/// 4. Degree-3 nonlinear state-chain commitment mixing `prev_state_root`, `covenant_rule`, and `payout`.
pub struct CovenantTransitionAir {
    context: AirContext<BaseElement>,
    pub_inputs: CovenantPublicInputs,
}

impl Air for CovenantTransitionAir {
    type BaseField = BaseElement;
    type PublicInputs = CovenantPublicInputs;

    fn new(trace_info: TraceInfo, pub_inputs: Self::PublicInputs, options: ProofOptions) -> Self {
        assert_eq!(TRACE_WIDTH, trace_info.width());
        let degrees = vec![
            // Cols 0..3: covenant_rule_hash limbs constant across every step
            TransitionConstraintDegree::new(1),
            TransitionConstraintDegree::new(1),
            TransitionConstraintDegree::new(1),
            // Cols 3..6: input, payout, change amounts constant across every step
            TransitionConstraintDegree::new(1),
            TransitionConstraintDegree::new(1),
            TransitionConstraintDegree::new(1),
            // Value conservation: input_amount - payout_amount - change_amount == 0
            TransitionConstraintDegree::new(1),
            // Bounty/reinvest slack bit booleanity + MSB-first shift-add accumulation
            TransitionConstraintDegree::new(2),
            TransitionConstraintDegree::new(1),
            // Soil/reinvest upper slack bit booleanity + MSB-first shift-add accumulation
            TransitionConstraintDegree::new(2),
            TransitionConstraintDegree::new(1),
            // Change amount bit booleanity + accumulation
            TransitionConstraintDegree::new(2),
            TransitionConstraintDegree::new(1),
            // Payout amount bit booleanity + accumulation
            TransitionConstraintDegree::new(2),
            TransitionConstraintDegree::new(1),
            // Nonlinear degree-3 state chain step
            TransitionConstraintDegree::new(3),
        ];

        let num_assertions = 18;
        Self {
            context: AirContext::new(trace_info, degrees, num_assertions, options),
            pub_inputs,
        }
    }

    fn evaluate_transition<E: FieldElement + From<Self::BaseField>>(
        &self,
        frame: &EvaluationFrame<E>,
        _periodic_values: &[E],
        result: &mut [E],
    ) {
        let curr = frame.current();
        let next = frame.next();

        // 0..3: Recursive covenant rule_hash continuity
        result[0] = next[0] - curr[0];
        result[1] = next[1] - curr[1];
        result[2] = next[2] - curr[2];

        // 3..6: Constant transition amounts
        result[3] = next[3] - curr[3];
        result[4] = next[4] - curr[4];
        result[5] = next[5] - curr[5];

        // 6: Value conservation
        result[6] = curr[3] - curr[4] - curr[5];

        // 7..9: Bounty/reinvest slack bit booleanity + 63-bit accumulator
        result[7] = curr[6] * (curr[6] - E::ONE);
        result[8] = next[7] - (curr[7] + curr[7] + curr[6]);

        // 9..11: Soil/reinvest slack bit booleanity + 63-bit accumulator
        result[9] = curr[8] * (curr[8] - E::ONE);
        result[10] = next[9] - (curr[9] + curr[9] + curr[8]);

        // 11..13: Change amount bit booleanity + 63-bit accumulator
        result[11] = curr[10] * (curr[10] - E::ONE);
        result[12] = next[11] - (curr[11] + curr[11] + curr[10]);

        // 13..15: Payout amount bit booleanity + 63-bit accumulator
        result[13] = curr[12] * (curr[12] - E::ONE);
        result[14] = next[13] - (curr[13] + curr[13] + curr[12]);

        // 15: Degree-3 algebraic state-chain step
        let state_cubed = curr[14] * curr[14] * curr[14];
        result[15] = next[14] - (state_cubed + curr[0] + curr[4] + E::ONE);
    }

    fn get_assertions(&self) -> Vec<Assertion<Self::BaseField>> {
        let last = self.trace_length() - 1;
        let in_rule = hex_to_three_limbs(&self.pub_inputs.covenant_rule_hash);
        let out_rule = hex_to_three_limbs(&self.pub_inputs.change_covenant_rule_hash);
        let prev_root = hex_to_three_limbs(&self.pub_inputs.prev_state_root);

        vec![
            // Input covenant rule at step 0
            Assertion::single(0, 0, in_rule[0]),
            Assertion::single(1, 0, in_rule[1]),
            Assertion::single(2, 0, in_rule[2]),
            // Change output covenant rule at final step (proves recursive continuity)
            Assertion::single(0, last, out_rule[0]),
            Assertion::single(1, last, out_rule[1]),
            Assertion::single(2, last, out_rule[2]),
            // Conserved amounts at step 0
            Assertion::single(3, 0, BaseElement::from(self.pub_inputs.input_amount)),
            Assertion::single(4, 0, BaseElement::from(self.pub_inputs.payout_amount)),
            Assertion::single(5, 0, BaseElement::from(self.pub_inputs.change_amount)),
            // Bounty/reinvest slack accumulator: 0 -> expected_bounty_slack_fe
            Assertion::single(7, 0, BaseElement::ZERO),
            Assertion::single(7, last, self.pub_inputs.expected_bounty_slack_fe()),
            // Soil/reinvest slack accumulator: 0 -> expected_soil_slack_fe
            Assertion::single(9, 0, BaseElement::ZERO),
            Assertion::single(9, last, self.pub_inputs.expected_soil_slack_fe()),
            // Change amount accumulator: 0 -> change_amount
            Assertion::single(11, 0, BaseElement::ZERO),
            Assertion::single(11, last, BaseElement::from(self.pub_inputs.change_amount)),
            // Payout amount accumulator: 0 -> payout_amount
            Assertion::single(13, 0, BaseElement::ZERO),
            Assertion::single(13, last, BaseElement::from(self.pub_inputs.payout_amount)),
            // Initial state-chain seed bound to prev_state_root
            Assertion::single(14, 0, prev_root[0]),
        ]
    }

    fn context(&self) -> &AirContext<Self::BaseField> {
        &self.context
    }
}

/// Winterfell 0.13 STARK prover for `CovenantTransitionAir`.
pub struct CovenantProver {
    options: ProofOptions,
    pub_inputs: CovenantPublicInputs,
}

impl CovenantProver {
    pub fn new(pub_inputs: CovenantPublicInputs, options: ProofOptions) -> Self {
        Self {
            options,
            pub_inputs,
        }
    }

    /// Standard ProofOptions achieving >= 96 bits conjectured security with fast interactive prove/verify.
    pub fn default_options() -> ProofOptions {
        ProofOptions::new(
            32,                     // num_queries (32 * log2(8) = 96 bits + grinding)
            8,                      // blowup_factor (>= max degree 3)
            4,                      // grinding_factor (+4 bits => ~100 bits security, <1ms)
            FieldExtension::None,   // 128-bit base field provides full soundness without extension
            4,                      // FRI folding factor
            31,                     // FRI max remainder polynomial degree
            BatchingMethod::Linear, // constraint batching
            BatchingMethod::Linear, // DEEP batching
        )
    }

    /// Constructs the 64-row execution trace decomposing all range-checked values into 63 bits.
    pub fn build_trace(pub_inputs: &CovenantPublicInputs) -> Result<TraceTable<BaseElement>, String> {
        let (bounty_slack, soil_slack) = pub_inputs.compute_checked_slacks()?;
        let rule_limbs = hex_to_three_limbs(&pub_inputs.covenant_rule_hash);
        let prev_root_limbs = hex_to_three_limbs(&pub_inputs.prev_state_root);

        let input_fe = BaseElement::from(pub_inputs.input_amount);
        let payout_fe = BaseElement::from(pub_inputs.payout_amount);
        let change_fe = BaseElement::from(pub_inputs.change_amount);

        // Choose unused last-row bit (row 63, outside transitions 0..63) in {0, 1} so that the
        // interpolating polynomial of each bit column always has exact degree 63 (non-zero x^63
        // DFT coefficient `sum_{j=0..63} bit[j] * g^j`), even when `val == 0` or `val == 2^63 - 1`.
        let g = BaseElement::get_root_of_unity(6); // primitive 64th root of unity
        let final_bit_for_full_degree = |val: u64| -> BaseElement {
            let mut sum = BaseElement::ZERO;
            let mut g_pow = BaseElement::ONE;
            for step in 0..63 {
                if ((val >> (62 - step)) & 1) == 1 {
                    sum += g_pow;
                }
                g_pow *= g;
            }
            if sum == BaseElement::ZERO {
                BaseElement::ONE
            } else {
                BaseElement::ZERO
            }
        };

        let bounty_last_bit = final_bit_for_full_degree(bounty_slack);
        let soil_last_bit = final_bit_for_full_degree(soil_slack);
        let change_last_bit = final_bit_for_full_degree(pub_inputs.change_amount);
        let payout_last_bit = final_bit_for_full_degree(pub_inputs.payout_amount);

        let bit_at = |val: u64, last_bit: BaseElement, step: usize| -> BaseElement {
            if step < 63 {
                BaseElement::from((val >> (62 - step)) & 1)
            } else {
                last_bit
            }
        };

        let mut trace = TraceTable::new(TRACE_WIDTH, TRACE_LENGTH);
        trace.fill(
            |state| {
                state[0] = rule_limbs[0];
                state[1] = rule_limbs[1];
                state[2] = rule_limbs[2];
                state[3] = input_fe;
                state[4] = payout_fe;
                state[5] = change_fe;
                state[6] = bit_at(bounty_slack, bounty_last_bit, 0);
                state[7] = BaseElement::ZERO;
                state[8] = bit_at(soil_slack, soil_last_bit, 0);
                state[9] = BaseElement::ZERO;
                state[10] = bit_at(pub_inputs.change_amount, change_last_bit, 0);
                state[11] = BaseElement::ZERO;
                state[12] = bit_at(pub_inputs.payout_amount, payout_last_bit, 0);
                state[13] = BaseElement::ZERO;
                state[14] = prev_root_limbs[0];
            },
            |step, state| {
                // `step` is the current row index (0..63); compute row `step + 1` in `state`.
                let next_step = step + 1;
                state[7] = state[7] + state[7] + state[6];
                state[6] = bit_at(bounty_slack, bounty_last_bit, next_step);

                state[9] = state[9] + state[9] + state[8];
                state[8] = bit_at(soil_slack, soil_last_bit, next_step);

                state[11] = state[11] + state[11] + state[10];
                state[10] = bit_at(pub_inputs.change_amount, change_last_bit, next_step);

                state[13] = state[13] + state[13] + state[12];
                state[12] = bit_at(pub_inputs.payout_amount, payout_last_bit, next_step);

                let cubed = state[14] * state[14] * state[14];
                state[14] = cubed + state[0] + state[4] + BaseElement::ONE;
            },
        );

        Ok(trace)
    }
}

impl Prover for CovenantProver {
    type BaseField = BaseElement;
    type Air = CovenantTransitionAir;
    type Trace = TraceTable<Self::BaseField>;
    type HashFn = StarkHashFn;
    type VC = StarkVC;
    type RandomCoin = StarkCoin;
    type TraceLde<E: FieldElement<BaseField = Self::BaseField>> =
        DefaultTraceLde<E, Self::HashFn, Self::VC>;
    type ConstraintCommitment<E: FieldElement<BaseField = Self::BaseField>> =
        DefaultConstraintCommitment<E, Self::HashFn, Self::VC>;
    type ConstraintEvaluator<'a, E: FieldElement<BaseField = Self::BaseField>> =
        DefaultConstraintEvaluator<'a, Self::Air, E>;

    fn get_pub_inputs(&self, _trace: &Self::Trace) -> CovenantPublicInputs {
        self.pub_inputs.clone()
    }

    fn options(&self) -> &ProofOptions {
        &self.options
    }

    fn new_trace_lde<E: FieldElement<BaseField = Self::BaseField>>(
        &self,
        trace_info: &TraceInfo,
        main_trace: &ColMatrix<Self::BaseField>,
        domain: &StarkDomain<Self::BaseField>,
        partition_option: PartitionOptions,
    ) -> (Self::TraceLde<E>, TracePolyTable<E>) {
        DefaultTraceLde::new(trace_info, main_trace, domain, partition_option)
    }

    fn build_constraint_commitment<E: FieldElement<BaseField = Self::BaseField>>(
        &self,
        composition_poly_trace: CompositionPolyTrace<E>,
        num_constraint_composition_columns: usize,
        domain: &StarkDomain<Self::BaseField>,
        partition_options: PartitionOptions,
    ) -> (Self::ConstraintCommitment<E>, CompositionPoly<E>) {
        DefaultConstraintCommitment::new(
            composition_poly_trace,
            num_constraint_composition_columns,
            domain,
            partition_options,
        )
    }

    fn new_evaluator<'a, E: FieldElement<BaseField = Self::BaseField>>(
        &self,
        air: &'a Self::Air,
        aux_rand_elements: Option<AuxRandElements<E>>,
        composition_coefficients: ConstraintCompositionCoefficients<E>,
    ) -> Self::ConstraintEvaluator<'a, E> {
        DefaultConstraintEvaluator::new(air, aux_rand_elements, composition_coefficients)
    }
}

/// Serializable Winterfell 0.13 STARK proof artifact stored in ledger state and returned over HTTP.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarkProofArtifact {
    pub proof_id: String,
    pub tx_kind: String,
    pub label: String,
    pub public_inputs: CovenantPublicInputs,
    /// Full serialized Winterfell `Proof` bytes encoded as lowercase hex.
    pub proof_hex: String,
    pub proof_bytes_len: usize,
    pub prove_ms: u64,
    pub verify_ms: u64,
    pub security_bits: u32,
    pub verified: bool,
    pub created_at_unix: u64,
}

/// Result of re-running `winterfell::verify` on a stored `StarkProofArtifact`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarkVerifyResult {
    pub proof_id: String,
    pub valid: bool,
    pub verify_ms: u64,
    pub proof_bytes_len: usize,
    pub security_bits: u32,
    pub error: Option<String>,
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(1_795_320_000)
}

/// Generates and immediately self-verifies a real Winterfell 0.13 STARK proof for a covenant transition.
pub fn prove_covenant_transition(
    inputs: CovenantPublicInputs,
) -> Result<StarkProofArtifact, String> {
    let trace = CovenantProver::build_trace(&inputs)?;
    let options = CovenantProver::default_options();
    let prover = CovenantProver::new(inputs.clone(), options);

    let prove_start = Instant::now();
    let proof = prover
        .prove(trace)
        .map_err(|e| format!("Winterfell STARK proving failed: {e}"))?;
    let prove_ms = prove_start.elapsed().as_millis() as u64;

    let security_bits = proof.conjectured_security::<StarkHashFn>().bits();
    let proof_bytes = proof.to_bytes();
    let proof_bytes_len = proof_bytes.len();
    let proof_hex = hex::encode(&proof_bytes);

    let proof_id_hash = sha256_concat(&[
        b"NEWSATOSHI_WINTERFELL_STARK_V0_13",
        inputs.new_state_root.as_bytes(),
        &inputs.tx_nonce.to_be_bytes(),
        &proof_bytes[..proof_bytes.len().min(64)],
    ]);
    let proof_id = format!("stark-{}", &hex::encode(proof_id_hash)[..16]);

    let mut artifact = StarkProofArtifact {
        proof_id,
        tx_kind: inputs.tx_kind.as_str().to_string(),
        label: inputs.label.clone(),
        public_inputs: inputs,
        proof_hex,
        proof_bytes_len,
        prove_ms,
        verify_ms: 0,
        security_bits,
        verified: false,
        created_at_unix: unix_now(),
    };

    let check = verify_covenant_stark(&artifact)?;
    if !check.valid {
        return Err(format!(
            "newly generated STARK proof failed self-verification: {:?}",
            check.error
        ));
    }
    artifact.verify_ms = check.verify_ms;
    artifact.verified = true;
    Ok(artifact)
}

/// Deserializes and cryptographically verifies a Winterfell 0.13 STARK proof against its public inputs.
pub fn verify_covenant_stark(artifact: &StarkProofArtifact) -> Result<StarkVerifyResult, String> {
    if let Err(err) = artifact.public_inputs.validate_verifier_bounds() {
        return Ok(StarkVerifyResult {
            proof_id: artifact.proof_id.clone(),
            valid: false,
            verify_ms: 0,
            proof_bytes_len: 0,
            security_bits: 0,
            error: Some(err),
        });
    }
    if artifact.proof_hex.len() > MAX_PROOF_BYTES * 2 {
        return Err(format!(
            "STARK proof hex length {} exceeds maximum {} bytes",
            artifact.proof_hex.len() / 2,
            MAX_PROOF_BYTES
        ));
    }
    let proof_bytes = hex::decode(&artifact.proof_hex)
        .map_err(|e| format!("invalid proof_hex encoding: {e}"))?;
    let proof_bytes_len = proof_bytes.len();
    if proof_bytes_len > MAX_PROOF_BYTES {
        return Err(format!(
            "STARK proof byte length {proof_bytes_len} exceeds maximum {MAX_PROOF_BYTES}"
        ));
    }

    let proof = Proof::from_bytes(&proof_bytes)
        .map_err(|e| format!("failed to deserialize Winterfell proof bytes: {e}"))?;
    let security_bits = proof.conjectured_security::<StarkHashFn>().bits();
    let acceptable = AcceptableOptions::MinConjecturedSecurity(MIN_SECURITY_BITS);

    let verify_start = Instant::now();
    let verify_res = winterfell::verify::<CovenantTransitionAir, StarkHashFn, StarkCoin, StarkVC>(
        proof,
        artifact.public_inputs.clone(),
        &acceptable,
    );
    let verify_ms = verify_start.elapsed().as_millis() as u64;

    match verify_res {
        Ok(()) => Ok(StarkVerifyResult {
            proof_id: artifact.proof_id.clone(),
            valid: true,
            verify_ms,
            proof_bytes_len,
            security_bits,
            error: None,
        }),
        Err(err) => Ok(StarkVerifyResult {
            proof_id: artifact.proof_id.clone(),
            valid: false,
            verify_ms,
            proof_bytes_len,
            security_bits,
            error: Some(err.to_string()),
        }),
    }
}
