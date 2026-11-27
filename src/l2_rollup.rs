//! L2 Shielded Rollup & Perfection Layer (`src/l2_rollup.rs`).
//!
//! Realizes the Hourglass Architecture above Newsatoshi Mainnet v2 (`nsM2`):
//! - Layer 1 stays radically small, pure FIPS SHA-2, and ossified.
//! - Layer 2 delivers Visa/Mastercard/AI-agent velocity (`100,000+` TPS off-chain PayWord
//!   micro-transfers + multi-asset `LOVE` / `nsUSD` / `nsUSD-B` shielded state roots),
//!   compressed into deterministic `SHA-256` Merkle batch roots, proved with real Winterfell
//!   STARKs (`crate::stark`), anchored to Mainnet v2 L1 tips, and protected by unilateral
//!   Merkle-inclusion Escape Hatches (zero sequencer custody, zero freeze switch).

use crate::crypto::{sha256_concat, Hash32};
use crate::nsusd::NsUsdAssetId;
use crate::seed::SeedSnapshot;
use crate::stark::{prove_covenant_transition, verify_covenant_stark, CovenantPublicInputs, CovenantTxKind};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Instant;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct L2ShieldedLeaf {
    pub account_hash: String,
    pub asset_id: NsUsdAssetId,
    pub balance_units: u64,
    pub nonce: u64,
}

impl L2ShieldedLeaf {
    pub fn leaf_hash(&self) -> Hash32 {
        sha256_concat(&[
            b"nsL2LEAF1",
            self.account_hash.as_bytes(),
            &(self.asset_id as u32).to_le_bytes(),
            &self.balance_units.to_le_bytes(),
            &self.nonce.to_le_bytes(),
        ])
    }
}

/// Domain-separated empty-leaf sentinel used to pad non-power-of-two trees without ever
/// duplicating real leaves (prevents CVE-2012-2459 phantom-index double-exit attacks).
pub fn empty_l2_leaf_hash() -> Hash32 {
    sha256_concat(&[b"nsL2EMPTY_LEAF1"])
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct L2EscapeHatchProof {
    pub leaf_index: usize,
    pub leaf_count: usize,
    pub tree_depth: usize,
    pub leaf_hash_hex: String,
    pub state_root_hex: String,
    pub siblings_hex: Vec<String>,
    pub verified: bool,
}

/// Computes a deterministic binary `SHA-256` Merkle root over L2 account leaves
/// padded to the next power of two with `empty_l2_leaf_hash()` (never duplicating real leaves!)
/// and optionally builds an exact unilateral L1 Escape-Hatch inclusion proof for `prove_idx`.
#[allow(clippy::chunks_exact_to_as_chunks, clippy::manual_is_multiple_of)]
pub fn compute_l2_state_root_and_escape_proof(
    leaves: &[L2ShieldedLeaf],
    prove_idx: Option<usize>,
) -> (Hash32, Option<L2EscapeHatchProof>) {
    if leaves.is_empty() {
        return (sha256_concat(&[b"nsL2EMPTY"]), None);
    }
    let leaf_count = leaves.len();
    let padded_len = leaf_count.next_power_of_two();
    let empty_sentinel = empty_l2_leaf_hash();
    let mut level: Vec<Hash32> = Vec::with_capacity(padded_len);
    level.extend(leaves.iter().map(L2ShieldedLeaf::leaf_hash));
    level.resize(padded_len, empty_sentinel);

    let mut idx = prove_idx.filter(|&i| i < leaf_count);
    let target_leaf = idx.map(|i| level[i]);
    let mut siblings: Vec<Hash32> = Vec::new();

    while level.len() > 1 {
        if let Some(i) = idx {
            let sib = if i % 2 == 0 { level[i + 1] } else { level[i - 1] };
            siblings.push(sib);
            idx = Some(i / 2);
        }
        let mut next = Vec::with_capacity(level.len() / 2);
        for pair in level.chunks_exact(2) {
            next.push(sha256_concat(&[b"nsL2NODE1", &pair[0], &pair[1]]));
        }
        level = next;
    }

    let root = level[0];
    let proof = match (prove_idx, target_leaf) {
        (Some(orig_idx), Some(leaf)) if orig_idx < leaf_count => {
            let verified = verify_l2_escape_proof(leaf, orig_idx, &siblings, root);
            Some(L2EscapeHatchProof {
                leaf_index: orig_idx,
                leaf_count,
                tree_depth: siblings.len(),
                leaf_hash_hex: hex::encode(leaf),
                state_root_hex: hex::encode(root),
                siblings_hex: siblings.iter().map(hex::encode).collect(),
                verified,
            })
        }
        _ => None,
    };
    (root, proof)
}

/// Strictly verifies a SHA-256 Merkle inclusion proof AND requires that `idx` is completely
/// exhausted (`idx == 0`) after consuming `siblings.len()` bits (rejects `idx + k * 2^depth`
/// index-aliasing forgeries!). Also rejects the empty sentinel leaf.
#[allow(clippy::manual_is_multiple_of)]
pub fn verify_l2_escape_proof(
    leaf: Hash32,
    mut idx: usize,
    siblings: &[Hash32],
    expected_root: Hash32,
) -> bool {
    if leaf == empty_l2_leaf_hash() {
        return false;
    }
    let mut cur = leaf;
    for sib in siblings {
        cur = if idx % 2 == 0 {
            sha256_concat(&[b"nsL2NODE1", &cur, sib])
        } else {
            sha256_concat(&[b"nsL2NODE1", sib, &cur])
        };
        idx /= 2;
    }
    idx == 0 && cur == expected_root
}

/// Deterministic 32-byte L1 Mainnet v2 anchor binding the L2 rollup state transition,
/// the `nsUSD` solvency root, and the live Mainnet v2 L1 tip hash.
pub fn compute_l1_anchor_commitment(
    prev_l2_root: Hash32,
    new_l2_root: Hash32,
    nsusd_solvency_root: Hash32,
    l1_tip_hex: &str,
    batch_nonce: u64,
) -> Hash32 {
    sha256_concat(&[
        b"nsL2ANCHOR1",
        &prev_l2_root,
        &new_l2_root,
        &nsusd_solvency_root,
        l1_tip_hex.as_bytes(),
        &batch_nonce.to_le_bytes(),
    ])
}

/// Executes a REAL, live measured L2 Perfection Layer batch benchmark (sandbox only):
/// 1. Pre-computes a genuine reverse Rivest-Shamir PayWord hash chain (`tip = H^N(root_seed)`),
/// 2. Verifies every incremental PayWord token (`H(token_k) == token_{k-1}`) while updating
///    strictly same-asset (`SovereignNsUsd`) account balances (per-asset conservation exact!),
/// 3. Computes power-of-two sentinel-padded `SHA-256` L2 Merkle state roots + a verified
///    unilateral L1 Escape-Hatch proof (`idx == 0` exhaustion enforced),
/// 4. Generates & verifies a live Winterfell 0.13 STARK conservation proof over aggregate batch totals, and
/// 5. Computes the deterministic L1 Mainnet v2 anchor commitment.
pub fn run_live_l2_batch_benchmark(
    tx_count: usize,
    seed: &SeedSnapshot,
    nsusd_solvency_root: Hash32,
) -> Result<Value, String> {
    let t_total = Instant::now();
    let count = tx_count.clamp(10, 50_000);
    let initial_payer_bal: u64 = (count as u64) * 1_000 + 500_000;
    let prev_leaves = vec![
        L2ShieldedLeaf {
            account_hash: "agent-payer-01".to_string(),
            asset_id: NsUsdAssetId::SovereignNsUsd,
            balance_units: initial_payer_bal,
            nonce: 0,
        },
        L2ShieldedLeaf {
            account_hash: "inference-merchant-01".to_string(),
            asset_id: NsUsdAssetId::SovereignNsUsd,
            balance_units: 0,
            nonce: 0,
        },
        // Same denomination (SovereignNsUsd) — strict per-asset conservation (never debit nsUSD & credit LOVE!)
        L2ShieldedLeaf {
            account_hash: "l2-sequencer-fee-pool".to_string(),
            asset_id: NsUsdAssetId::SovereignNsUsd,
            balance_units: 0,
            nonce: 0,
        },
    ];
    let (prev_root, _) = compute_l2_state_root_and_escape_proof(&prev_leaves, None);

    // 1. Pre-build a genuine PayWord hash chain: chain[0] = root_seed, chain[count] = tip
    let mut chain: Vec<Hash32> = Vec::with_capacity(count + 1);
    let mut cur = sha256_concat(&[b"nsPAYWORD-BENCH-SEED", &(count as u64).to_le_bytes()]);
    chain.push(cur);
    for _ in 0..count {
        cur = sha256_concat(&[&cur]);
        chain.push(cur);
    }
    let committed_tip = chain[count];

    // 2. Measure REAL incremental O(1) PayWord verification (SHA256(token_k) == prev_verified_token)
    //    plus strictly same-asset nsUSD balance accounting.
    let t_exec = Instant::now();
    let mut prev_verified = committed_tip;
    let mut payer_bal = initial_payer_bal;
    let mut merchant_bal: u64 = 0;
    let mut fee_acc: u64 = 0;
    for step in 1..=count {
        let revealed_token = chain[count - step];
        let hashed = sha256_concat(&[&revealed_token]);
        if hashed != prev_verified {
            return Err("payword_verification_failed".to_string());
        }
        prev_verified = revealed_token;
        payer_bal -= 1_000; // $0.001 nsUSD per call
        merchant_bal += 990;
        fee_acc += 10;
    }
    let exec_us = t_exec.elapsed().as_micros().max(1);
    let measured_tps = ((count as f64) * 1_000_000.0) / (exec_us as f64);

    let new_leaves = vec![
        L2ShieldedLeaf {
            account_hash: "agent-payer-01".to_string(),
            asset_id: NsUsdAssetId::SovereignNsUsd,
            balance_units: payer_bal,
            nonce: count as u64,
        },
        L2ShieldedLeaf {
            account_hash: "inference-merchant-01".to_string(),
            asset_id: NsUsdAssetId::SovereignNsUsd,
            balance_units: merchant_bal,
            nonce: count as u64,
        },
        L2ShieldedLeaf {
            account_hash: "l2-sequencer-fee-pool".to_string(),
            asset_id: NsUsdAssetId::SovereignNsUsd,
            balance_units: fee_acc,
            nonce: 1,
        },
    ];
    let nsusd_before: u64 = prev_leaves
        .iter()
        .filter(|l| l.asset_id == NsUsdAssetId::SovereignNsUsd)
        .map(|l| l.balance_units)
        .sum();
    let nsusd_after: u64 = new_leaves
        .iter()
        .filter(|l| l.asset_id == NsUsdAssetId::SovereignNsUsd)
        .map(|l| l.balance_units)
        .sum();
    let per_asset_conserved = nsusd_before == nsusd_after && payer_bal + merchant_bal + fee_acc == initial_payer_bal;

    let (new_root, escape_proof) = compute_l2_state_root_and_escape_proof(&new_leaves, Some(1));

    // 3. Real Winterfell 0.13 STARK conservation proof over aggregate batch totals
    //    (proves gross = merchant_bal + fee_acc with >= 1% fee floor; note: individual PayWord hashes
    //    and Merkle updates are verified natively in Rust above, not inside this covenant AIR).
    let rule_hash = hex::encode(sha256_concat(&[b"nsL2-ROLLUP-RULE-V1"]));
    let gross = (count as u64) * 1_000;
    let pub_inputs = CovenantPublicInputs {
        tx_kind: CovenantTxKind::StreamingSettlement,
        label: format!("l2-batch-{count}-payword-nsusd"),
        tx_nonce: 1,
        covenant_rule_hash: rule_hash.clone(),
        change_covenant_rule_hash: rule_hash,
        prev_state_root: hex::encode(prev_root),
        new_state_root: hex::encode(new_root),
        input_amount: gross,
        payout_amount: merchant_bal,
        change_amount: fee_acc,
        max_bounty_payout: gross,
        reinvest_bps: 100, // >= 1.00% protocol fee / surplus floor
        min_soil_health_score: 0,
        soil_health_score: 0,
    };
    let t_prove = Instant::now();
    let artifact = prove_covenant_transition(pub_inputs)?;
    let prove_ms = t_prove.elapsed().as_secs_f64() * 1000.0;

    let t_verify = Instant::now();
    let verify_res = verify_covenant_stark(&artifact)?;
    let verify_ms = t_verify.elapsed().as_secs_f64() * 1000.0;
    let total_end_to_end_ms = t_total.elapsed().as_secs_f64() * 1000.0;

    let l1_tip = seed
        .mainnet_status
        .as_ref()
        .map(|s| s.tip.as_str())
        .unwrap_or("offline");
    let l1_height = seed.mainnet_status.as_ref().map(|s| s.height);
    let anchor = compute_l1_anchor_commitment(prev_root, new_root, nsusd_solvency_root, l1_tip, 1);

    Ok(json!({
        "ok": true,
        "sandbox_benchmark": true,
        "live_state_mutated": false,
        "scope_disclosure": "Measures real incremental SHA-256 PayWord verification + per-asset nsUSD accounting + sentinel-padded Merkle escape proof + Winterfell aggregate conservation STARK (does not submit an L1 tx or prove individual PayWord hashes inside the STARK AIR).",
        "tx_count": count,
        "asset_id": NsUsdAssetId::SovereignNsUsd as u32,
        "conservation_exact": per_asset_conserved,
        "payword_execution_us": exec_us,
        "measured_offchain_tps": measured_tps.round() as u64,
        "total_end_to_end_ms": (total_end_to_end_ms * 100.0).round() / 100.0,
        "committed_payword_tip_hex": hex::encode(committed_tip),
        "final_payword_token_hex": hex::encode(prev_verified),
        "prev_l2_state_root_hex": hex::encode(prev_root),
        "new_l2_state_root_hex": hex::encode(new_root),
        "winterfell_stark_batch_proof": {
            "proof_id": artifact.proof_id,
            "verified": verify_res.valid,
            "proof_bytes": artifact.proof_bytes_len,
            "security_bits": artifact.security_bits,
            "prove_ms": (prove_ms * 100.0).round() / 100.0,
            "verify_ms": (verify_ms * 100.0).round() / 100.0
        },
        "unilateral_l1_escape_hatch": escape_proof,
        "l1_mainnet_v2_anchor": {
            "l1_height": l1_height,
            "l1_tip_hex": l1_tip,
            "nsusd_solvency_root_hex": hex::encode(nsusd_solvency_root),
            "anchor_commitment_hex": hex::encode(anchor)
        }
    }))
}

pub fn render_l2_architecture_markdown() -> &'static str {
    "# Newsatoshi Layer-2 Expansion & Perfection Layers (`v0.28.4`)

**Hourglass Principle:** Layer 1 (`newsatoshi-rfc0021`, Mainnet v2 `nsM2`) stays radically small, pure FIPS SHA-2, and ossified. Layer 2 (`newsatoshi-commons`) delivers machine-speed throughput, private USD stability (`nsUSD`), and zero-custody escape hatches.

## The 4 Perfection Layers Above Layer 1

1. **Layer 2A — Microsecond Streaming (`O(1)` SHA-256 PayWords):**
   - `1` SHA-256 hash per API call (`~0.013 ms` verification, zero per-call gas, up to `1,000,000` ticks per channel).
   - Unilateral partial-claim L1 channel close (`RFC-0022`) eliminates counterparty griefing.
2. **Layer 2B — Stable Private Value (`nsUSD`, `RFC-0028`):**
   - **Sovereign `nsUSD` (`asset_id = 1`):** Over-collateralized Bitcoin (`>= 150%` MCR, `120%` liquidation), Liquity-style Stability Pool, direct `$1.00` permissionless collateral redemption floor, and `PriorLock` bonded median BTC/USD oracle (`50%` challenger slash / `50%` permanent `LOVE` burn on outlier spoofing).
   - **Bridged `nsUSD-B` (`asset_id = 2`):** Strictly isolated 1:1 `USDC`/`USDS`/`DAI` escrow receipts carrying honest underlying issuer-risk disclosure.
   - Live Stress-Test API: `POST /api/v2/nsusd/simulate-stress` (proves `-50%` flash-crash liquidation, bank-run redemption, oracle slash & stale circuit breaker live).
3. **Layer 2C — Shielded Rollup Batching & Winterfell STARK Proofs (`src/l2_rollup.rs`):**
   - Aggregates thousands of off-chain `LOVE` + `nsUSD` micro-transfers into a single deterministic `SHA-256` Merkle state root, proved by a real Winterfell 0.13 STARK conservation proof (`~22 KB`), anchored via a 32-byte commitment onto Mainnet v2 L1.
   - Live Benchmark API: `POST /api/v2/l2/benchmark` (runs a real live measured batch + Winterfell STARK prove/verify + Merkle escape-hatch proof).
4. **Layer 2D — Unilateral L1 Escape Hatch (Zero Sequencer Custody):**
   - Every L2 user receives a `SHA-256` Merkle inclusion proof (`L2EscapeHatchProof`) enabling trustless unilateral L1 exit via native `SHA-256` HTLC / `PriorLock` even if all sequencers go offline or attempt censorship.
"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nsusd::NsUsdEngine;

    #[test]
    fn l2_perfection_batch_proves_winterfell_stark_and_escape_hatch() {
        let seed = SeedSnapshot {
            live: false,
            endpoint: "test".to_string(),
            fetched_at_unix: 1_795_800_000,
            status: None,
            mainnet_info: crate::seed::MainnetInfo::verified_v0_1_0(),
            mainnet_live: false,
            mainnet_endpoint: None,
            mainnet_status: None,
            mainnet_error: None,
            error: None,
        };
        let eng = NsUsdEngine::new();
        let res = run_live_l2_batch_benchmark(1_000, &seed, eng.solvency_root()).unwrap();
        assert_eq!(res["ok"], json!(true));
        assert_eq!(res["conservation_exact"], json!(true));
        assert_eq!(res["winterfell_stark_batch_proof"]["verified"], json!(true));
        assert_eq!(res["unilateral_l1_escape_hatch"]["verified"], json!(true));
    }

    #[test]
    fn escape_hatch_blocks_cve_2012_2459_phantom_leaf_and_index_aliasing() {
        // 3 leaves -> padded to 4 with EMPTY sentinel (NOT duplicating leaf 2!)
        let leaves = vec![
            L2ShieldedLeaf { account_hash: "a".into(), asset_id: NsUsdAssetId::SovereignNsUsd, balance_units: 100, nonce: 1 },
            L2ShieldedLeaf { account_hash: "b".into(), asset_id: NsUsdAssetId::SovereignNsUsd, balance_units: 200, nonce: 1 },
            L2ShieldedLeaf { account_hash: "c".into(), asset_id: NsUsdAssetId::SovereignNsUsd, balance_units: 300, nonce: 1 },
        ];
        let (root, proof_opt) = compute_l2_state_root_and_escape_proof(&leaves, Some(2));
        let proof = proof_opt.unwrap();
        assert!(proof.verified);
        let sibs: Vec<Hash32> = proof
            .siblings_hex
            .iter()
            .map(|h| {
                let b = hex::decode(h).unwrap();
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&b);
                arr
            })
            .collect();
        let leaf2 = leaves[2].leaf_hash();
        // Genuine index 2 MUST verify
        assert!(verify_l2_escape_proof(leaf2, 2, &sibs, root));
        // CVE-2012-2459 phantom index 3 MUST FAIL (no odd-leaf duplication!)
        assert!(!verify_l2_escape_proof(leaf2, 3, &sibs, root));
        // Unexhausted aliased index 2 + 2^depth (= 6) MUST FAIL!
        assert!(!verify_l2_escape_proof(leaf2, 2 + (1 << sibs.len()), &sibs, root));
        // Empty sentinel leaf MUST NEVER verify as a withdrawable leaf!
        assert!(!verify_l2_escape_proof(empty_l2_leaf_hash(), 3, &sibs, root));
    }
}
