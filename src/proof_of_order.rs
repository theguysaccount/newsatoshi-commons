//! Proof-of-Order / Good-Deed Mining Covenant Vault (`src/proof_of_order.rs`).
//!
//! Clean 2-Layer Monetary Physics:
//! - **Layer 1 (`nsM2`)** stays 100% pure, ungameable Bitcoin SHA-256 AuxPoW (`chainid=20051`,
//!   `$0` extra electricity, untouched `21,000,000` LOVE hard cap).
//! - **Layer 2 (`ProofOfOrderVault`)** lets Bitcoin/Bitaxe miners voluntarily pledge a percentage
//!   of merge-mined `$LOVE` into an ownerless Recursive Covenant Vault, and lets everyday humans
//!   and AI builders "mine" `$LOVE` from that vault by submitting verifiable proofs of real-world
//!   good deeds and order:
//!   1. `hardware_clean_energy`: Hardware-signed solar/microgrid kWh output (`src/hardware.rs`).
//!   2. `formal_math_or_open_source`: Verified Lean 4 (`0 sorry`) proof or open-source security patch.
//!   3. `zktls_civic_good_deed`: Privacy-preserving `zkTLS` receipt proof (`src/zktls.rs`) of
//!      verified charity, blood donation, environmental cleanup, or community service.
//!
//! Anti-Sybil & Anti-Fraud Game Theory:
//! - Every claim posts a refundable `PriorLock` bond (`>= 0.01 LOVE` = `1_000_000` base units)
//!   and enters a mandatory challenge window (`default 86,400s` = 24h).
//! - Fraudulent/duplicate claims challenged during the window are slashed: **50% of the slashed
//!   bond pays the honest challenger, and 50% is permanently burned** (making Sybil spam lose money).
//!
//! Zero fake data: fresh live vault starts at `0` pledges, `0` claims, `0` LOVE funded/distributed.

use crate::crypto::{sha256_concat, Hash32};
use crate::stark::{
    prove_covenant_transition, verify_covenant_stark, CovenantPublicInputs, CovenantTxKind,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Minimum refundable `PriorLock` anti-Sybil bond: `0.01 LOVE` (`1_000_000` base units).
pub const MIN_CLAIM_BOND_SATS: u64 = 1_000_000;
/// Default challenge window before covenant releases `$LOVE`: 24 hours.
pub const DEFAULT_CHALLENGE_WINDOW_SECS: u64 = 86_400;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoodDeedCategory {
    HardwareCleanEnergy,
    FormalMathOrOpenSource,
    ZkTlsCivicGoodDeed,
}

impl GoodDeedCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HardwareCleanEnergy => "hardware_clean_energy",
            Self::FormalMathOrOpenSource => "formal_math_or_open_source",
            Self::ZkTlsCivicGoodDeed => "zktls_civic_good_deed",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "hardware_clean_energy" | "clean_energy" | "solar" | "hardware" => {
                Ok(Self::HardwareCleanEnergy)
            }
            "formal_math_or_open_source" | "lean4" | "open_source" | "math" => {
                Ok(Self::FormalMathOrOpenSource)
            }
            "zktls_civic_good_deed" | "good_deed" | "civic" | "charity" | "zktls" => {
                Ok(Self::ZkTlsCivicGoodDeed)
            }
            other => Err(format!(
                "invalid category '{other}' (expected: hardware_clean_energy | formal_math_or_open_source | zktls_civic_good_deed)"
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeedClaimStatus {
    PendingChallengeWindow,
    SlashedFraud,
    FinalizedReleased,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuxPowMinerPledge {
    pub pledge_id_hex: String,
    pub miner_label: String,
    pub pledge_bps: u16, // 100..=10000 (1%..=100% of merge-mined LOVE routed to Good-Deed Vault)
    pub vault_deposit_sats: u64,
    pub l1_txid_hex: Option<String>,
    pub created_at_unix: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoodDeedClaim {
    pub claim_id_hex: String,
    pub category: GoodDeedCategory,
    pub claimant_handle: String,
    pub recipient_ns1: String,
    pub summary: String,
    pub proof_digest_hex: String,
    pub requested_reward_sats: u64,
    pub bond_love_sats: u64,
    pub submitted_at_unix: u64,
    pub challenge_ends_at_unix: u64,
    pub status: DeedClaimStatus,
    pub challenger_id: Option<String>,
    pub slash_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderSlashReceipt {
    pub claim_id_hex: String,
    pub challenger_id: String,
    pub total_slashed_sats: u64,
    pub challenger_bounty_sats: u64,
    pub permanently_burned_sats: u64,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProofOfOrderVault {
    pub vault_balance_sats: u64,
    pub total_distributed_sats: u64,
    pub total_burned_sats: u64,
    pub total_challenger_bounties_sats: u64,
    pub pledges: BTreeMap<String, AuxPowMinerPledge>,
    pub claims: BTreeMap<String, GoodDeedClaim>,
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn validate_hex64(s: &str, field: &str) -> Result<String, String> {
    let clean = s.trim().trim_start_matches("0x").to_ascii_lowercase();
    if clean.len() != 64 || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("{field} must be a 64-character SHA-256 hex digest"));
    }
    Ok(clean)
}

pub struct SubmitDeedClaimInput<'a> {
    pub category: GoodDeedCategory,
    pub claimant_handle: &'a str,
    pub recipient_ns1: &'a str,
    pub summary: &'a str,
    pub proof_digest_hex: &'a str,
    pub requested_reward_sats: u64,
    pub bond_love_sats: u64,
    pub challenge_window_secs: Option<u64>,
    pub now_unix: u64,
}

impl ProofOfOrderVault {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an optional AuxPoW miner pledge (and any verified LOVE deposit into the Good-Deed Vault).
    pub fn record_miner_pledge(
        &mut self,
        miner_label: &str,
        pledge_bps: u16,
        vault_deposit_sats: u64,
        l1_txid_hex: Option<&str>,
    ) -> Result<AuxPowMinerPledge, String> {
        let label = miner_label.trim();
        if label.len() < 2 || label.len() > 64 {
            return Err("miner_label must be 2..64 characters".to_string());
        }
        if !(100..=10_000).contains(&pledge_bps) {
            return Err("pledge_bps must be between 100 (1%) and 10000 (100%)".to_string());
        }
        let clean_tx = match l1_txid_hex.map(str::trim).filter(|s| !s.is_empty()) {
            Some(tx) => Some(validate_hex64(tx, "l1_txid_hex")?),
            None => None,
        };
        let created_at_unix = now_unix_secs();
        let pid = sha256_concat(&[
            b"nsORDER_PLEDGE1",
            label.as_bytes(),
            &pledge_bps.to_le_bytes(),
            &vault_deposit_sats.to_le_bytes(),
            &created_at_unix.to_le_bytes(),
        ]);
        let pledge_id_hex = hex::encode(pid);
        self.vault_balance_sats = self
            .vault_balance_sats
            .checked_add(vault_deposit_sats)
            .ok_or_else(|| "vault_balance overflow".to_string())?;
        let pledge = AuxPowMinerPledge {
            pledge_id_hex: pledge_id_hex.clone(),
            miner_label: label.to_string(),
            pledge_bps,
            vault_deposit_sats,
            l1_txid_hex: clean_tx,
            created_at_unix,
        };
        self.pledges.insert(pledge_id_hex, pledge.clone());
        Ok(pledge)
    }

    /// Submits a bonded verifiable Proof-of-Order / Good-Deed claim into the challenge window.
    pub fn submit_deed_claim(
        &mut self,
        input: SubmitDeedClaimInput<'_>,
    ) -> Result<GoodDeedClaim, String> {
        let handle = input.claimant_handle.trim();
        if handle.len() < 2 || handle.len() > 64 {
            return Err("claimant_handle must be 2..64 characters".to_string());
        }
        let recipient = input.recipient_ns1.trim();
        if recipient.len() < 8 || recipient.len() > 140 {
            return Err("recipient_ns1 must be a valid shielded address (8..140 chars)".to_string());
        }
        let summary = input.summary.trim();
        if summary.len() < 8 || summary.len() > 240 {
            return Err("summary must be 8..240 characters".to_string());
        }
        let proof_digest = validate_hex64(input.proof_digest_hex, "proof_digest_hex")?;
        if self
            .claims
            .values()
            .any(|c| c.proof_digest_hex == proof_digest)
        {
            return Err("duplicate_proof_digest: this deed proof was already submitted".to_string());
        }
        if input.bond_love_sats < MIN_CLAIM_BOND_SATS {
            return Err(format!(
                "PriorLock anti-Sybil bond must be >= {} base units (0.01 LOVE)",
                MIN_CLAIM_BOND_SATS
            ));
        }
        if input.requested_reward_sats == 0 {
            return Err("requested_reward_sats must be > 0".to_string());
        }

        let window = input
            .challenge_window_secs
            .unwrap_or(DEFAULT_CHALLENGE_WINDOW_SECS)
            .clamp(60, 7 * 86_400);
        let claim_id = sha256_concat(&[
            b"nsORDER_CLAIM1",
            input.category.as_str().as_bytes(),
            proof_digest.as_bytes(),
            recipient.as_bytes(),
            &input.requested_reward_sats.to_le_bytes(),
        ]);
        let claim_id_hex = hex::encode(claim_id);
        let claim = GoodDeedClaim {
            claim_id_hex: claim_id_hex.clone(),
            category: input.category,
            claimant_handle: handle.to_string(),
            recipient_ns1: recipient.to_string(),
            summary: summary.to_string(),
            proof_digest_hex: proof_digest,
            requested_reward_sats: input.requested_reward_sats,
            bond_love_sats: input.bond_love_sats,
            submitted_at_unix: input.now_unix,
            challenge_ends_at_unix: input.now_unix.saturating_add(window),
            status: DeedClaimStatus::PendingChallengeWindow,
            challenger_id: None,
            slash_reason: None,
        };
        self.claims.insert(claim_id_hex, claim.clone());
        Ok(claim)
    }

    /// Challenges and slashes a fraudulent claim during its challenge window:
    /// **50% of the PriorLock bond pays the honest challenger, 50% is permanently burned.**
    pub fn challenge_deed_claim(
        &mut self,
        claim_id_hex: &str,
        challenger_id: &str,
        reason: &str,
        now_unix: u64,
    ) -> Result<OrderSlashReceipt, String> {
        let cid = claim_id_hex.trim().to_ascii_lowercase();
        let ch = challenger_id.trim();
        if ch.len() < 2 {
            return Err("challenger_id required".to_string());
        }
        let rsn = reason.trim();
        if rsn.len() < 4 {
            return Err("challenge reason required".to_string());
        }
        let claim = self
            .claims
            .get_mut(&cid)
            .ok_or_else(|| format!("claim '{cid}' not found"))?;
        if claim.status != DeedClaimStatus::PendingChallengeWindow {
            return Err("claim is no longer in pending challenge window".to_string());
        }
        if now_unix >= claim.challenge_ends_at_unix {
            return Err("challenge window has already expired".to_string());
        }

        let total = claim.bond_love_sats;
        let bounty = total / 2;
        let burned = total - bounty;
        claim.status = DeedClaimStatus::SlashedFraud;
        claim.challenger_id = Some(ch.to_string());
        claim.slash_reason = Some(rsn.to_string());

        self.total_challenger_bounties_sats = self
            .total_challenger_bounties_sats
            .saturating_add(bounty);
        self.total_burned_sats = self.total_burned_sats.saturating_add(burned);

        Ok(OrderSlashReceipt {
            claim_id_hex: cid,
            challenger_id: ch.to_string(),
            total_slashed_sats: total,
            challenger_bounty_sats: bounty,
            permanently_burned_sats: burned,
            reason: rsn.to_string(),
        })
    }

    /// Finalizes an unchallenged claim after its challenge window expires and releases `$LOVE`
    /// from the Proof-of-Order covenant vault (enforcing strict conservation).
    pub fn finalize_deed_claim(
        &mut self,
        claim_id_hex: &str,
        now_unix: u64,
    ) -> Result<GoodDeedClaim, String> {
        let cid = claim_id_hex.trim().to_ascii_lowercase();
        let reward = {
            let claim = self
                .claims
                .get(&cid)
                .ok_or_else(|| format!("claim '{cid}' not found"))?;
            if claim.status != DeedClaimStatus::PendingChallengeWindow {
                return Err("claim is not pending".to_string());
            }
            if now_unix < claim.challenge_ends_at_unix {
                return Err(format!(
                    "challenge window active until unix {}",
                    claim.challenge_ends_at_unix
                ));
            }
            claim.requested_reward_sats
        };
        if self.vault_balance_sats < reward {
            return Err(format!(
                "insufficient vault balance ({} sats < {} sats requested)",
                self.vault_balance_sats, reward
            ));
        }
        self.vault_balance_sats -= reward;
        self.total_distributed_sats = self.total_distributed_sats.saturating_add(reward);
        let claim = self.claims.get_mut(&cid).unwrap();
        claim.status = DeedClaimStatus::FinalizedReleased;
        Ok(claim.clone())
    }

    /// Deterministic SHA-256 Merkle root committing to all vault balances, pledges, and deed claims.
    pub fn order_root(&self) -> Hash32 {
        let mut parts: Vec<Vec<u8>> = vec![
            b"nsORDER_ROOT1".to_vec(),
            self.vault_balance_sats.to_le_bytes().to_vec(),
            self.total_distributed_sats.to_le_bytes().to_vec(),
            self.total_burned_sats.to_le_bytes().to_vec(),
        ];
        for p in self.pledges.values() {
            parts.push(p.pledge_id_hex.as_bytes().to_vec());
        }
        for c in self.claims.values() {
            parts.push(c.claim_id_hex.as_bytes().to_vec());
            parts.push(vec![c.status as u8]);
        }
        let refs: Vec<&[u8]> = parts.iter().map(|v| v.as_slice()).collect();
        sha256_concat(&refs)
    }

    pub fn summary_json(&self) -> Value {
        json!({
            "honest_initial_state": self.pledges.is_empty() && self.claims.is_empty() && self.vault_balance_sats == 0,
            "architecture": "L1 Pure Bitcoin AuxPoW (0 extra watts) -> L2 Ownerless Proof-of-Order Covenant Vault",
            "order_root_hex": hex::encode(self.order_root()),
            "vault_balance_sats": self.vault_balance_sats,
            "vault_balance_love": format!("{:.8}", (self.vault_balance_sats as f64) / 1e8),
            "total_distributed_sats": self.total_distributed_sats,
            "total_distributed_love": format!("{:.8}", (self.total_distributed_sats as f64) / 1e8),
            "total_fraud_burned_sats": self.total_burned_sats,
            "total_challenger_bounties_sats": self.total_challenger_bounties_sats,
            "min_priorlock_bond_sats": MIN_CLAIM_BOND_SATS,
            "default_challenge_window_secs": DEFAULT_CHALLENGE_WINDOW_SECS,
            "pledges_count": self.pledges.len(),
            "claims_count": self.claims.len(),
            "pledges": self.pledges.values().collect::<Vec<_>>(),
            "claims": self.claims.values().collect::<Vec<_>>(),
            "supported_categories": [
                "hardware_clean_energy",
                "formal_math_or_open_source",
                "zktls_civic_good_deed"
            ]
        })
    }
}

/// Ephemeral isolated sandbox proving the entire Proof-of-Order lifecycle (never mutates live state):
/// 1. AuxPoW miner routes 50 LOVE (1 block) into the Good-Deed Vault at $0 extra watts.
/// 2. Honest claimant proves a zkTLS civic good deed + solar microgrid kWh -> unlocks LOVE via Winterfell STARK.
/// 3. Adversarial Sybil submits a forged deed -> challenged & slashed 50% bounty / 50% permanent LOVE burn!
pub fn run_order_sandbox_simulation() -> Result<Value, String> {
    let mut vault = ProofOfOrderVault::new();
    let t0: u64 = 1_795_850_000;

    // Step 1: AuxPoW miner pledges 50% of block rewards & deposits 50 LOVE (5_000_000_000 sats)
    let pledge = vault.record_miner_pledge("bitaxe-good-deed-pool-01", 5_000, 5_000_000_000, None)?;

    // Step 2: Honest zkTLS civic good deed (privacy-preserving verified charity/blood receipt)
    let good_digest = hex::encode(sha256_concat(&[b"zktls:verified-good-deed-receipt-01"]));
    let good_claim = vault.submit_deed_claim(SubmitDeedClaimInput {
        category: GoodDeedCategory::ZkTlsCivicGoodDeed,
        claimant_handle: "civic-builder-01",
        recipient_ns1: "ns1qgooddeedrecipient0000000000000001",
        summary: "zkTLS-verified Red Cross blood donation & community food-bank drive receipt",
        proof_digest_hex: &good_digest,
        requested_reward_sats: 1_000_000_000, // 10 LOVE
        bond_love_sats: 10_000_000,           // 0.1 LOVE refundable bond
        challenge_window_secs: Some(3_600),
        now_unix: t0,
    })?;

    // Step 3: Adversarial Sybil submits a forged claim -> caught and slashed 50/50!
    let fake_digest = hex::encode(sha256_concat(&[b"sybil:forged-receipt-attempt-99"]));
    let fake_claim = vault.submit_deed_claim(SubmitDeedClaimInput {
        category: GoodDeedCategory::HardwareCleanEnergy,
        claimant_handle: "sybil-attacker-99",
        recipient_ns1: "ns1qsybilattacker00000000000000000099",
        summary: "Forged solar telemetry packet with invalid hardware signature",
        proof_digest_hex: &fake_digest,
        requested_reward_sats: 2_500_000_000, // 25 LOVE attempted drain
        bond_love_sats: 50_000_000,           // 0.5 LOVE bond posted
        challenge_window_secs: Some(3_600),
        now_unix: t0 + 10,
    })?;
    let slash_receipt = vault.challenge_deed_claim(
        &fake_claim.claim_id_hex,
        "watchtower-verifier-07",
        "Hardware Ed25519/SLH-DSA meter attestation failed signature check",
        t0 + 120,
    )?;

    // Step 4: Honest claim passes challenge window -> releases 10 LOVE with real Winterfell STARK conservation proof!
    let prev_root = hex::encode(vault.order_root());
    let released = vault.finalize_deed_claim(&good_claim.claim_id_hex, t0 + 3_700)?;
    let next_root = hex::encode(vault.order_root());
    let rule_hash = hex::encode(sha256_concat(&[b"nsORDER-VAULT-RULE-V1"]));
    let pub_inputs = CovenantPublicInputs {
        tx_kind: CovenantTxKind::StreamingSettlement,
        label: "proof-of-order-good-deed-payout".to_string(),
        tx_nonce: 1,
        covenant_rule_hash: rule_hash.clone(),
        change_covenant_rule_hash: rule_hash,
        prev_state_root: prev_root,
        new_state_root: next_root,
        input_amount: 5_000_000_000,
        payout_amount: 1_000_000_000,
        change_amount: 4_000_000_000,
        max_bounty_payout: 1_500_000_000,
        reinvest_bps: 8_000,
        min_soil_health_score: 0,
        soil_health_score: 100,
    };
    let stark = prove_covenant_transition(pub_inputs)?;
    let t_v = Instant::now();
    let stark_v = verify_covenant_stark(&stark)?;
    let verify_ms = (t_v.elapsed().as_secs_f64() * 1000.0 * 100.0).round() / 100.0;

    Ok(json!({
        "ok": stark_v.valid && vault.vault_balance_sats == 4_000_000_000 && slash_receipt.permanently_burned_sats == 25_000_000,
        "sandbox": true,
        "mutated_live_state": false,
        "step_1_auxpow_pledge": pledge,
        "step_2_honest_deed_released": released,
        "step_3_sybil_fraud_slashed": slash_receipt,
        "step_4_stark_covenant_proof": {
            "valid": stark_v.valid,
            "proof_bytes": stark.proof_hex.len() / 2,
            "verify_ms": verify_ms
        },
        "final_sandbox_vault": {
            "remaining_vault_love": format!("{:.8}", (vault.vault_balance_sats as f64) / 1e8),
            "distributed_to_good_deeds_love": format!("{:.8}", (vault.total_distributed_sats as f64) / 1e8),
            "sybil_bond_burned_love": format!("{:.8}", (vault.total_burned_sats as f64) / 1e8),
            "honest_challenger_bounty_love": format!("{:.8}", (vault.total_challenger_bounties_sats as f64) / 1e8),
            "order_root_hex": hex::encode(vault.order_root())
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_of_order_vault_conservation_and_sybil_slashing() {
        let sim = run_order_sandbox_simulation().expect("order sandbox");
        assert_eq!(sim["ok"], json!(true));
        assert_eq!(sim["step_4_stark_covenant_proof"]["valid"], json!(true));
        assert_eq!(
            sim["step_3_sybil_fraud_slashed"]["challenger_bounty_sats"],
            json!(25_000_000)
        );
        assert_eq!(
            sim["step_3_sybil_fraud_slashed"]["permanently_burned_sats"],
            json!(25_000_000)
        );
        assert_eq!(
            sim["final_sandbox_vault"]["remaining_vault_love"],
            json!("40.00000000")
        );
    }
}
