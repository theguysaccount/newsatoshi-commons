//! # `sp1-covenant-host` — SP1 Hypercube Prover & Verifier Driver for `NEWSATOSHI/2.1`
//!
//! Builds a realistic 3-step `NEWSATOSHI/2.1` Autonomous Commons witness sequence
//! (Parcel #404 Land Deed -> Hardware TEE Telemetry Bounty -> PayWord Streaming Settlement),
//! pre-validates it natively via `covenant-circuit-core`, and runs Succinct's SP1 Hypercube
//! prover (`--execute`, `--prove`, `--verify`) to generate a compressed recursive STARK proof
//! ready for `OP_VERIFY_COVENANT_STARK` (`0xb4`).

use covenant_circuit_core::{
    build_xmss_commitment_tree, compute_covenant_vk_hash, compute_external_attestation_hash,
    compute_ivc_genesis, compute_ivc_step, compute_next_state_root, sha256, sha256_concat,
    verify_recursive_fold, xmss_auth_path, CovenantRuleParams, CovenantTransitionWitness, Hash32,
    PublicValues, TransitionKind, PUBLIC_VALUES_ENCODED_LEN,
};
use serde::{Deserialize, Serialize};
use sp1_sdk::{include_elf, HashableKey, ProverClient, SP1Stdin};
use std::env;
use std::time::Instant;

/// Embedded RISC-V ELF compiled from `zkvm/sp1-covenant-guest` by `build.rs`.
pub const COVENANT_GUEST_ELF: &[u8] = include_elf!("sp1-covenant-guest");

/// Backend identifier for SP1 Hypercube inside the RFC-0021 `0xb4` witness envelope.
const BACKEND_ID_SP1_HYPERCUBE: u8 = 0x02;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PriorIvcProof {
    prev_vkey_u32: [u32; 8],
    prev_public_values: PublicValues,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GuestInput {
    prior_proof: Option<PriorIvcProof>,
    witnesses: Vec<CovenantTransitionWitness>,
}

fn build_demo_witness_batch(program_vk_id: Hash32) -> Vec<CovenantTransitionWitness> {
    let (oracle_root, oracle_pks, oracle_layers) =
        build_xmss_commitment_tree(sha256(b"PERPETUAL_TRUST_DEED_ORACLE_V2_1"), 6);
    let (hw_root, hw_pks, hw_layers) =
        build_xmss_commitment_tree(sha256(b"SOLAR_INVERTER_TEE_PARCEL_404_V2_1"), 6);

    let rule = CovenantRuleParams {
        max_bounty_payout: 20_000,
        reinvest_bps: 1_500, // 15% reinvestment spread
        min_soil_health_score: 80,
        authorized_deed_oracle_pk: oracle_root,
        authorized_hardware_pk: hw_root,
    };
    let covenant_vk_hash = compute_covenant_vk_hash(program_vk_id, rule.rule_hash());
    let genesis_root = sha256(b"NEWSATOSHI_COMMONS_GENESIS");
    let genesis_acc = compute_ivc_genesis(covenant_vk_hash, genesis_root);

    // Step 0: Parcel #404 Land Deed Acquisition (100,000 -> 40,000 seller + 60,000 covenanted change)
    let kind0 = TransitionKind::LandDeedAcquisition {
        parcel_id: 404,
        deed_document_hash: sha256(b"COUNTY_DEED_PARCEL_404_PERPETUAL_PURPOSE_TRUST_SEC_4_2"),
        oracle_leaf_index: 0,
        oracle_leaf_pk_commitment: oracle_pks[0],
        oracle_auth_path: xmss_auth_path(&oracle_layers, 0),
        oracle_sig_digest: sha256(b"ORACLE_XMSS_SIG_0"),
    };
    let tx0 = sha256(b"L1_TX_SIGHASH_STEP_0");
    let att0 = compute_external_attestation_hash(&kind0);
    let root1 = compute_next_state_root(genesis_root, tx0, 0, 1, 0, att0);
    let acc1 = compute_ivc_step(genesis_acc, covenant_vk_hash, genesis_root, root1, tx0, 0, att0);
    let w0 = CovenantTransitionWitness {
        step_index: 0,
        tx_sighash: tx0,
        program_vk_id,
        rule: rule.clone(),
        covenant_vk_hash,
        input_value: 100_000,
        output_payout_value: 40_000,
        change_value: 60_000,
        change_covenant_vk_hash: Some(covenant_vk_hash),
        prev_state_root: genesis_root,
        new_state_root: root1,
        prev_accumulator: genesis_acc,
        new_accumulator: acc1,
        prev_oracle_watermark: 0,
        new_oracle_watermark: 1,
        prev_hw_watermark: 0,
        new_hw_watermark: 0,
        kind: kind0,
    };

    // Step 1: Hardware TEE Telemetry Bounty (60,000 -> 15,000 operator + 45,000 covenanted change)
    let kind1 = TransitionKind::PhysicalEnergyDeployment {
        parcel_id: 404,
        kwh_delivered: 12_500,
        soil_health_score: 92,
        hw_leaf_index: 0,
        hw_leaf_pk_commitment: hw_pks[0],
        hw_auth_path: xmss_auth_path(&hw_layers, 0),
        hw_sig_digest: sha256(b"HW_XMSS_SIG_0"),
    };
    let tx1 = sha256(b"L1_TX_SIGHASH_STEP_1");
    let att1 = compute_external_attestation_hash(&kind1);
    let root2 = compute_next_state_root(root1, tx1, 1, 1, 1, att1);
    let acc2 = compute_ivc_step(acc1, covenant_vk_hash, root1, root2, tx1, 1, att1);
    let w1 = CovenantTransitionWitness {
        step_index: 1,
        tx_sighash: tx1,
        program_vk_id,
        rule: rule.clone(),
        covenant_vk_hash,
        input_value: 60_000,
        output_payout_value: 15_000,
        change_value: 45_000,
        change_covenant_vk_hash: Some(covenant_vk_hash),
        prev_state_root: root1,
        new_state_root: root2,
        prev_accumulator: acc1,
        new_accumulator: acc2,
        prev_oracle_watermark: 1,
        new_oracle_watermark: 1,
        prev_hw_watermark: 0,
        new_hw_watermark: 1,
        kind: kind1,
    };

    // Step 2: PayWord Machine Streaming Settlement (500 ticks @ 200 LOVE = 100,000 gross)
    let max_ticks = 1_000u64;
    let ticks_claimed = 500u64;
    let unit_price = 200u64;
    let mut chain = Vec::with_capacity((max_ticks + 1) as usize);
    let mut cursor = sha256(b"MICROGRID_PAYWORD_CHANNEL_SEED_01");
    chain.push(cursor);
    for _ in 0..max_ticks {
        cursor = sha256(&cursor);
        chain.push(cursor);
    }
    let channel_tip = *chain.last().expect("non-empty hash chain");
    let final_preimage = chain[(max_ticks - ticks_claimed) as usize];

    let kind2 = TransitionKind::StreamingChannelSettlement {
        channel_tip,
        max_ticks,
        unit_price,
        ticks_claimed,
        final_preimage,
    };
    let tx2 = sha256(b"L1_TX_SIGHASH_STEP_2");
    let att2 = compute_external_attestation_hash(&kind2);
    let root3 = compute_next_state_root(root2, tx2, 2, 1, 1, att2);
    let acc3 = compute_ivc_step(acc2, covenant_vk_hash, root2, root3, tx2, 2, att2);
    let w2 = CovenantTransitionWitness {
        step_index: 2,
        tx_sighash: tx2,
        program_vk_id,
        rule,
        covenant_vk_hash,
        input_value: 100_000,
        output_payout_value: 85_000,
        change_value: 15_000,
        change_covenant_vk_hash: Some(covenant_vk_hash),
        prev_state_root: root2,
        new_state_root: root3,
        prev_accumulator: acc2,
        new_accumulator: acc3,
        prev_oracle_watermark: 1,
        new_oracle_watermark: 1,
        prev_hw_watermark: 1,
        new_hw_watermark: 1,
        kind: kind2,
    };

    vec![w0, w1, w2]
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mode = args.first().map(|s| s.as_str()).unwrap_or("--execute");

    let client = ProverClient::from_env();
    let (pk, vk) = client.setup(COVENANT_GUEST_ELF);

    // Derive deterministic 32-byte `program_vk_id` directly from the SP1 ELF verifying key.
    let vk_bytes32_hex = vk.bytes32();
    let vk_clean = vk_bytes32_hex.strip_prefix("0x").unwrap_or(&vk_bytes32_hex);
    let vk_raw = hex::decode(vk_clean).expect("SP1 vk.bytes32() must be valid 32-byte hex");
    let mut program_vk_id = [0u8; 32];
    program_vk_id.copy_from_slice(&vk_raw);

    let witnesses = build_demo_witness_batch(program_vk_id);

    // Native pre-flight check via `covenant-circuit-core` before spending prover cycles.
    let expected_fold =
        verify_recursive_fold(&witnesses).expect("native pre-flight fold verification failed");
    let expected_pv_bytes = expected_fold.to_canonical_bytes();

    let guest_input = GuestInput {
        prior_proof: None,
        witnesses,
    };

    let mut stdin = SP1Stdin::new();
    stdin.write(&guest_input);

    println!("=== NEWSATOSHI/2.1 SP1 HYPERCUBE COVENANT DRIVER ===");
    println!("Guest Program VK ID : {}", hex::encode(program_vk_id));
    println!(
        "Covenant VK Hash    : {}",
        hex::encode(expected_fold.covenant_vk_hash)
    );
    println!(
        "Initial State Root  : {}",
        hex::encode(expected_fold.initial_state_root)
    );
    println!(
        "Final State Root    : {}",
        hex::encode(expected_fold.final_state_root)
    );
    println!(
        "Final IVC Accum.    : {}",
        hex::encode(expected_fold.final_accumulator)
    );

    match mode {
        "--execute" => {
            let start = Instant::now();
            let (output, report) = client
                .execute(COVENANT_GUEST_ELF, &stdin)
                .run()
                .expect("SP1 guest execution failed");
            let elapsed_ms = start.elapsed().as_millis();
            let committed = output.as_slice();
            assert_eq!(committed.len(), PUBLIC_VALUES_ENCODED_LEN);
            assert_eq!(
                committed,
                expected_pv_bytes.as_slice(),
                "committed SP1 public values diverged from native kernel"
            );
            println!("Mode                : EXECUTE (cycle dry-run)");
            println!("Total RISC-V Cycles : {}", report.total_instruction_count());
            println!("Execution Time      : {elapsed_ms} ms");
            println!("Public Values Match : VERIFIED ({} bytes)", committed.len());
        }
        "--prove" | "--verify" => {
            // IMPORTANT: Always use `.compressed()` (recursive STARK) so deferred proofs
            // from `verify_sp1_proof` are fully resolved and proof size remains O(1).
            let prove_start = Instant::now();
            let proof = client
                .prove(&pk, &stdin)
                .compressed()
                .run()
                .expect("SP1 Hypercube compressed proof generation failed");
            let prove_ms = prove_start.elapsed().as_millis();

            let verify_start = Instant::now();
            client
                .verify(&proof, &vk)
                .expect("SP1 Hypercube proof verification failed");
            let verify_ms = verify_start.elapsed().as_millis();

            let committed = proof.public_values.as_slice();
            assert_eq!(committed, expected_pv_bytes.as_slice());

            let witness_digest = sha256_concat(&[
                b"OP_VERIFY_COVENANT_STARK_V2_1",
                &[BACKEND_ID_SP1_HYPERCUBE],
                &program_vk_id,
                committed,
            ]);

            println!("Mode                : PROVE + VERIFY (Compressed Recursive STARK)");
            println!("Prove Latency       : {prove_ms} ms");
            println!("Verify Latency      : {verify_ms} ms");
            println!("Proof Verified      : TRUE");
            println!(
                "0xb4 Witness Prefix : 20{}b4",
                hex::encode(witness_digest)
            );
        }
        other => {
            eprintln!("Usage: sp1-covenant-host [--execute | --prove | --verify]");
            eprintln!("Unknown flag: {other}");
            std::process::exit(1);
        }
    }
}
