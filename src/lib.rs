pub mod agent;
pub mod crypto;
pub mod hardware;
pub mod rpc;
pub mod seed;
pub mod server;
pub mod stark;
pub mod storage;
pub mod swap;

pub use crypto::{
    derive_leaf_seed, hash32_from_hex, merkle_leaf_hash, merkle_node_hash, sha256, sha256_concat,
    AttestationSignature, Hash32, LamportKeypair, LamportPubKey, LamportSig, MerkleLamportKeypair,
    MerkleLamportSig, MerkleSignError, SlhDsaKeypair, SlhDsaPubKey, SlhDsaSig, DEFAULT_TREE_HEIGHT,
    LARGE_TREE_HEIGHT, MAX_TREE_HEIGHT, SLH_DSA_PARAM_SET, SLH_DSA_PK_BYTES, SLH_DSA_SK_BYTES,
};
pub use hardware::{
    ExternalDeedPacket, ExternalTelemetryPacket, HardwareDeviceConfig, HardwareTelemetrySigner,
    SignedTelemetryPacket, TelemetryPacketJson, TelemetryReading,
};
pub use rpc::{
    format_newsatoshi_cli_commands, AnchorCommitment, AnchorReceipt, AnchorRpcParams,
    JsonRpcAnchorClient, JsonRpcRequest, VerifyStarkRpcParams, OP_VERIFY_COVENANT_STARK_HEX,
    PROTOCOL_VERSION,
};
pub use seed::{
    L1LockRecord, MainnetInfo, SeedRecentBlock, SeedSnapshot, SeedStatus, SeedStatusClient,
    AUXPOW_CHAIN_ID, DEFAULT_MAINNET_SEED_STATUS_URL, DEFAULT_SEED_STATUS_URL,
    MAINNET_GENESIS_HASH, TESTNET_GENESIS_HASH,
};
pub use stark::{
    prove_covenant_transition, verify_covenant_stark, CovenantProver, CovenantPublicInputs,
    CovenantTransitionAir, CovenantTxKind, StarkProofArtifact, StarkVerifyResult,
};
pub use storage::{
    compute_verified_love_per_kwh, load_or_init_state, load_or_init_state_with_mode, load_state,
    save_state_atomic, ApiStateView, CommonsEngine, ExternalPublicKeysConfig, FundingRecord,
    InProcessSigningKeys, LegalScheduleExport, ParcelRecord, PersistedEngineState, StreamingRecord,
    TelemetryRecord, TelemetrySeriesPoint,
};

use std::collections::{HashMap, HashSet};

/// Immutable predicate governing an Autonomous Commons Treasury UTXO.
///
/// `authorized_deed_oracle_pk` and `authorized_hardware_pk` may each be a single-use
/// `LamportPubKey::commitment()`, a multi-use `MerkleLamportKeypair::root()`, or a
/// `SlhDsaPubKey::commitment()`. Optional `authorized_*_slh_pk` fields allow simultaneously
/// authorizing NIST FIPS 205 SLH-DSA-SHA2-128f keys alongside XMSS Merkle roots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CovenantRule {
    pub max_bounty_payout: u64,
    pub reinvest_bps: u16, // Basis points (e.g., 1500 = 15% routed back to land covenant)
    pub min_soil_health_score: u16,
    pub authorized_deed_oracle_pk: Hash32,
    pub authorized_hardware_pk: Hash32,
    pub authorized_deed_oracle_slh_pk: Option<Hash32>,
    pub authorized_hardware_slh_pk: Option<Hash32>,
}

impl CovenantRule {
    pub fn rule_hash(&self) -> Hash32 {
        let base_parts: [&[u8]; 6] = [
            b"NEWSATOSHI_COVENANT_V2_1",
            &self.max_bounty_payout.to_be_bytes(),
            &self.reinvest_bps.to_be_bytes(),
            &self.min_soil_health_score.to_be_bytes(),
            &self.authorized_deed_oracle_pk,
            &self.authorized_hardware_pk,
        ];
        if self.authorized_deed_oracle_slh_pk.is_none() && self.authorized_hardware_slh_pk.is_none()
        {
            return sha256_concat(&base_parts);
        }

        let mut parts: Vec<&[u8]> = Vec::with_capacity(11);
        parts.extend_from_slice(&base_parts);
        parts.push(b"NEWSATOSHI_COVENANT_SLH_EXT_V1");
        if let Some(ref pk) = self.authorized_deed_oracle_slh_pk {
            parts.push(&[1u8]);
            parts.push(pk);
        } else {
            parts.push(&[0u8]);
        }
        if let Some(ref pk) = self.authorized_hardware_slh_pk {
            parts.push(&[1u8]);
            parts.push(pk);
        } else {
            parts.push(&[0u8]);
        }
        sha256_concat(&parts)
    }

    pub fn matches_deed_slh_pk(&self, commit: Hash32) -> bool {
        self.authorized_deed_oracle_slh_pk == Some(commit)
            || self.authorized_deed_oracle_pk == commit
    }

    pub fn matches_hardware_slh_pk(&self, commit: Hash32) -> bool {
        self.authorized_hardware_slh_pk == Some(commit) || self.authorized_hardware_pk == commit
    }
}

/// Outpoint reference `(txid, vout)` identifying a UTXO in `CommonsLedger`.
pub type UtxoRef = (Hash32, u32);
/// Result of a covenant spend: `(txid, optional_change_utxo_ref)`.
pub type SpendResult = (Hash32, Option<UtxoRef>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Utxo {
    pub txid: Hash32,
    pub vout: u32,
    pub amount: u64,
    pub owner_tag: String,
    pub covenant_hash: Option<Hash32>,
}

#[derive(Clone, Debug)]
pub enum CheckedWorkProof {
    /// Legacy one-time Lamport deed attestation.
    LandDeedAcquisition {
        parcel_id: u64,
        deed_document_hash: Hash32,
        oracle_pk: LamportPubKey,
        oracle_sig: LamportSig,
    },
    /// Legacy one-time Lamport physical telemetry attestation.
    PhysicalEnergyDeployment {
        parcel_id: u64,
        kwh_delivered: u64,
        soil_health_score: u16,
        device_pk: LamportPubKey,
        device_sig: LamportSig,
    },
    /// Upgraded XMSS-style Merkleized multi-use Lamport deed attestation.
    MerkleLandDeedAcquisition {
        parcel_id: u64,
        deed_document_hash: Hash32,
        oracle_sig: MerkleLamportSig,
    },
    /// Upgraded XMSS-style Merkleized multi-use Lamport physical telemetry attestation.
    MerklePhysicalEnergyDeployment {
        parcel_id: u64,
        kwh_delivered: u64,
        soil_health_score: u16,
        device_sig: MerkleLamportSig,
    },
    /// NIST FIPS 205 SLH-DSA-SHA2-128f deed attestation with monotonic nonce replay guard.
    SlhDsaLandDeedAcquisition {
        parcel_id: u64,
        deed_document_hash: Hash32,
        nonce: u64,
        oracle_pk: SlhDsaPubKey,
        oracle_sig: SlhDsaSig,
    },
    /// NIST FIPS 205 SLH-DSA-SHA2-128f hardware telemetry attestation with monotonic nonce guard.
    SlhDsaPhysicalEnergyDeployment {
        parcel_id: u64,
        kwh_delivered: u64,
        soil_health_score: u16,
        nonce: u64,
        device_pk: SlhDsaPubKey,
        device_sig: SlhDsaSig,
    },
}

impl CheckedWorkProof {
    pub fn deed_msg(parcel_id: u64, deed_document_hash: Hash32) -> Hash32 {
        sha256_concat(&[
            b"DEED_TRANSFER_V1",
            &parcel_id.to_be_bytes(),
            &deed_document_hash,
        ])
    }

    pub fn energy_msg(parcel_id: u64, kwh_delivered: u64, soil_health_score: u16) -> Hash32 {
        sha256_concat(&[
            b"PHYSICAL_TELEMETRY_V1",
            &parcel_id.to_be_bytes(),
            &kwh_delivered.to_be_bytes(),
            &soil_health_score.to_be_bytes(),
        ])
    }

    pub fn deed_msg_with_nonce(parcel_id: u64, deed_document_hash: Hash32, nonce: u64) -> Hash32 {
        sha256_concat(&[
            b"DEED_TRANSFER_SLH_V1",
            &parcel_id.to_be_bytes(),
            &deed_document_hash,
            &nonce.to_be_bytes(),
        ])
    }

    pub fn energy_msg_with_nonce(
        parcel_id: u64,
        kwh_delivered: u64,
        soil_health_score: u16,
        nonce: u64,
    ) -> Hash32 {
        sha256_concat(&[
            b"PHYSICAL_TELEMETRY_SLH_V1",
            &parcel_id.to_be_bytes(),
            &kwh_delivered.to_be_bytes(),
            &soil_health_score.to_be_bytes(),
            &nonce.to_be_bytes(),
        ])
    }

    pub fn merkle_land_deed(
        parcel_id: u64,
        deed_document_hash: Hash32,
        oracle_sig: MerkleLamportSig,
    ) -> Self {
        Self::MerkleLandDeedAcquisition {
            parcel_id,
            deed_document_hash,
            oracle_sig,
        }
    }

    pub fn merkle_energy_deployment(
        parcel_id: u64,
        kwh_delivered: u64,
        soil_health_score: u16,
        device_sig: MerkleLamportSig,
    ) -> Self {
        Self::MerklePhysicalEnergyDeployment {
            parcel_id,
            kwh_delivered,
            soil_health_score,
            device_sig,
        }
    }

    pub fn slh_dsa_land_deed(
        parcel_id: u64,
        deed_document_hash: Hash32,
        nonce: u64,
        oracle_pk: SlhDsaPubKey,
        oracle_sig: SlhDsaSig,
    ) -> Self {
        Self::SlhDsaLandDeedAcquisition {
            parcel_id,
            deed_document_hash,
            nonce,
            oracle_pk,
            oracle_sig,
        }
    }

    pub fn slh_dsa_energy_deployment(
        parcel_id: u64,
        kwh_delivered: u64,
        soil_health_score: u16,
        nonce: u64,
        device_pk: SlhDsaPubKey,
        device_sig: SlhDsaSig,
    ) -> Self {
        Self::SlhDsaPhysicalEnergyDeployment {
            parcel_id,
            kwh_delivered,
            soil_health_score,
            nonce,
            device_pk,
            device_sig,
        }
    }
}

enum ReplayRecord {
    OneTimeKey(Hash32),
    MerkleLeaf {
        root: Hash32,
        leaf_index: u32,
    },
    SlhDsaNonce {
        pk_commit: Hash32,
        nonce: u64,
        replay_digest: Hash32,
    },
}

enum WorkAccounting {
    LandDeed { parcel_id: u64 },
    EnergyDeployment { kwh_delivered: u64 },
}

/// Core Client-Side Validated UTXO ledger enforcing recursive covenants,
/// one-time Lamport non-reuse, XMSS-style Merkleized leaf-index replay protection,
/// and NIST FIPS 205 SLH-DSA monotonic nonce + digest replay protection.
#[derive(Clone, Debug)]
pub struct CommonsLedger {
    pub utxos: HashMap<(Hash32, u32), Utxo>,
    pub spent_one_time_keys: HashSet<Hash32>,
    /// Monotonic next minimum leaf index per Merkle root (O(1) STARK-friendly replay watermark).
    pub merkle_next_min_leaf: HashMap<Hash32, u32>,
    /// Audit set of all consumed Merkle leaf indices per root.
    pub spent_merkle_leaves: HashMap<Hash32, HashSet<u32>>,
    /// Replay-guard set of consumed SLH-DSA `(pk_commit, msg, sig_digest)` digests.
    pub spent_slh_digests: HashSet<Hash32>,
    /// Monotonic next minimum nonce per SLH-DSA public key commitment.
    pub slh_next_min_nonce: HashMap<Hash32, u64>,
    pub prev_state_root: Hash32,
    pub state_root: Hash32,
    pub tx_nonce: u64,
    pub acquired_parcels: Vec<u64>,
    pub total_kwh_verified: u64,
    pub total_merkle_leaves_consumed: u64,
    pub total_slh_dsa_signatures_verified: u64,
}

impl Default for CommonsLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl CommonsLedger {
    pub fn new() -> Self {
        let genesis = sha256(b"NEWSATOSHI_COMMONS_GENESIS");
        Self {
            utxos: HashMap::new(),
            spent_one_time_keys: HashSet::new(),
            merkle_next_min_leaf: HashMap::new(),
            spent_merkle_leaves: HashMap::new(),
            spent_slh_digests: HashSet::new(),
            slh_next_min_nonce: HashMap::new(),
            prev_state_root: genesis,
            state_root: genesis,
            tx_nonce: 0,
            acquired_parcels: Vec::new(),
            total_kwh_verified: 0,
            total_merkle_leaves_consumed: 0,
            total_slh_dsa_signatures_verified: 0,
        }
    }

    pub fn mint_genesis_covenant(&mut self, amount: u64, rule: &CovenantRule) -> (Hash32, u32) {
        let txid = sha256_concat(&[b"GENESIS", &amount.to_be_bytes(), &rule.rule_hash()]);
        let utxo = Utxo {
            txid,
            vout: 0,
            amount,
            owner_tag: "COMMONS_TREASURY".to_string(),
            covenant_hash: Some(rule.rule_hash()),
        };
        self.utxos.insert((txid, 0), utxo);
        self.recompute_state_root(txid);
        (txid, 0)
    }

    fn recompute_state_root(&mut self, latest_txid: Hash32) {
        self.prev_state_root = self.state_root;
        self.tx_nonce += 1;
        self.state_root = sha256_concat(&[
            &self.state_root,
            &latest_txid,
            &self.tx_nonce.to_be_bytes(),
        ]);
    }

    fn verify_merkle_leaf_fresh(
        &self,
        root: Hash32,
        leaf_index: u32,
    ) -> Result<(), &'static str> {
        if let Some(used) = self.spent_merkle_leaves.get(&root) {
            if used.contains(&leaf_index) {
                return Err("Merkle Lamport leaf index already spent (replay blocked)");
            }
        }
        let min_leaf = self.merkle_next_min_leaf.get(&root).copied().unwrap_or(0);
        if leaf_index < min_leaf {
            return Err("Merkle Lamport leaf index below monotonic watermark (stale replay blocked)");
        }
        Ok(())
    }

    fn verify_slh_fresh(
        &self,
        pk_commit: Hash32,
        nonce: u64,
        msg: Hash32,
        sig: &SlhDsaSig,
    ) -> Result<Hash32, &'static str> {
        let min_nonce = self.slh_next_min_nonce.get(&pk_commit).copied().unwrap_or(0);
        if nonce < min_nonce {
            return Err("SLH-DSA nonce below monotonic watermark (replay blocked)");
        }
        let replay_digest = sha256_concat(&[
            b"NEWSATOSHI_SLH_REPLAY_V1",
            &pk_commit,
            &nonce.to_be_bytes(),
            &msg,
            &sig.digest(),
        ]);
        if self.spent_slh_digests.contains(&replay_digest) {
            return Err("SLH-DSA signature digest already spent (replay blocked)");
        }
        Ok(replay_digest)
    }

    /// Executes a recursive covenant spend. Change MUST carry the identical `rule.rule_hash()`.
    pub fn spend_covenant(
        &mut self,
        input_ref: UtxoRef,
        rule: &CovenantRule,
        payout_amount: u64,
        recipient_tag: &str,
        proposed_change_covenant: Option<Hash32>,
        proof: CheckedWorkProof,
    ) -> Result<SpendResult, &'static str> {
        let input = self
            .utxos
            .get(&input_ref)
            .ok_or("Input UTXO does not exist or was already spent")?
            .clone();

        let expected_hash = rule.rule_hash();
        if input.covenant_hash != Some(expected_hash) {
            return Err("Input UTXO does not match covenant rule");
        }
        if payout_amount == 0 || payout_amount > input.amount {
            return Err("Invalid payout amount");
        }

        let change_amount = input.amount - payout_amount;
        if change_amount > 0 && proposed_change_covenant != Some(expected_hash) {
            return Err("Recursive covenant continuity violated: change must keep rule_hash");
        }

        // Verify cryptographic proof of checked physical work
        let (replay_record, accounting) = match proof {
            CheckedWorkProof::LandDeedAcquisition {
                parcel_id,
                deed_document_hash,
                oracle_pk,
                oracle_sig,
            } => {
                let commit = oracle_pk.commitment();
                if commit != rule.authorized_deed_oracle_pk {
                    return Err("Unauthorized deed oracle key");
                }
                if self.spent_one_time_keys.contains(&commit) {
                    return Err("Lamport key already spent (replay blocked)");
                }
                let msg = CheckedWorkProof::deed_msg(parcel_id, deed_document_hash);
                if !oracle_pk.verify(msg, &oracle_sig) {
                    return Err("Invalid quantum-safe deed attestation signature");
                }
                (
                    ReplayRecord::OneTimeKey(commit),
                    WorkAccounting::LandDeed { parcel_id },
                )
            }
            CheckedWorkProof::PhysicalEnergyDeployment {
                parcel_id,
                kwh_delivered,
                soil_health_score,
                device_pk,
                device_sig,
            } => {
                if payout_amount > rule.max_bounty_payout {
                    return Err("Payout exceeds covenant max_bounty_payout");
                }
                if soil_health_score < rule.min_soil_health_score {
                    return Err("Ecological coherence check failed: soil score below floor");
                }
                if kwh_delivered == 0 {
                    return Err("Zero physical output reported");
                }
                let commit = device_pk.commitment();
                if commit != rule.authorized_hardware_pk {
                    return Err("Unauthorized hardware attestation key");
                }
                if self.spent_one_time_keys.contains(&commit) {
                    return Err("Lamport key already spent (replay blocked)");
                }
                let msg = CheckedWorkProof::energy_msg(parcel_id, kwh_delivered, soil_health_score);
                if !device_pk.verify(msg, &device_sig) {
                    return Err("Invalid hardware telemetry signature");
                }
                (
                    ReplayRecord::OneTimeKey(commit),
                    WorkAccounting::EnergyDeployment { kwh_delivered },
                )
            }
            CheckedWorkProof::MerkleLandDeedAcquisition {
                parcel_id,
                deed_document_hash,
                oracle_sig,
            } => {
                let root = rule.authorized_deed_oracle_pk;
                self.verify_merkle_leaf_fresh(root, oracle_sig.leaf_index)?;
                let msg = CheckedWorkProof::deed_msg(parcel_id, deed_document_hash);
                if !oracle_sig.verify(root, msg) {
                    return Err("Invalid Merkleized deed attestation signature");
                }
                (
                    ReplayRecord::MerkleLeaf {
                        root,
                        leaf_index: oracle_sig.leaf_index,
                    },
                    WorkAccounting::LandDeed { parcel_id },
                )
            }
            CheckedWorkProof::MerklePhysicalEnergyDeployment {
                parcel_id,
                kwh_delivered,
                soil_health_score,
                device_sig,
            } => {
                if payout_amount > rule.max_bounty_payout {
                    return Err("Payout exceeds covenant max_bounty_payout");
                }
                if soil_health_score < rule.min_soil_health_score {
                    return Err("Ecological coherence check failed: soil score below floor");
                }
                if kwh_delivered == 0 {
                    return Err("Zero physical output reported");
                }
                let root = rule.authorized_hardware_pk;
                self.verify_merkle_leaf_fresh(root, device_sig.leaf_index)?;
                let msg = CheckedWorkProof::energy_msg(parcel_id, kwh_delivered, soil_health_score);
                if !device_sig.verify(root, msg) {
                    return Err("Invalid Merkleized hardware telemetry signature");
                }
                (
                    ReplayRecord::MerkleLeaf {
                        root,
                        leaf_index: device_sig.leaf_index,
                    },
                    WorkAccounting::EnergyDeployment { kwh_delivered },
                )
            }
            CheckedWorkProof::SlhDsaLandDeedAcquisition {
                parcel_id,
                deed_document_hash,
                nonce,
                oracle_pk,
                oracle_sig,
            } => {
                let pk_commit = oracle_pk.commitment();
                if !rule.matches_deed_slh_pk(pk_commit) {
                    return Err("Unauthorized SLH-DSA deed oracle key");
                }
                let msg =
                    CheckedWorkProof::deed_msg_with_nonce(parcel_id, deed_document_hash, nonce);
                let replay_digest = self.verify_slh_fresh(pk_commit, nonce, msg, &oracle_sig)?;
                if !oracle_pk.verify_hash(msg, &oracle_sig) {
                    return Err("Invalid NIST FIPS 205 SLH-DSA deed attestation signature");
                }
                (
                    ReplayRecord::SlhDsaNonce {
                        pk_commit,
                        nonce,
                        replay_digest,
                    },
                    WorkAccounting::LandDeed { parcel_id },
                )
            }
            CheckedWorkProof::SlhDsaPhysicalEnergyDeployment {
                parcel_id,
                kwh_delivered,
                soil_health_score,
                nonce,
                device_pk,
                device_sig,
            } => {
                if payout_amount > rule.max_bounty_payout {
                    return Err("Payout exceeds covenant max_bounty_payout");
                }
                if soil_health_score < rule.min_soil_health_score {
                    return Err("Ecological coherence check failed: soil score below floor");
                }
                if kwh_delivered == 0 {
                    return Err("Zero physical output reported");
                }
                let pk_commit = device_pk.commitment();
                if !rule.matches_hardware_slh_pk(pk_commit) {
                    return Err("Unauthorized SLH-DSA hardware attestation key");
                }
                let msg = CheckedWorkProof::energy_msg_with_nonce(
                    parcel_id,
                    kwh_delivered,
                    soil_health_score,
                    nonce,
                );
                let replay_digest = self.verify_slh_fresh(pk_commit, nonce, msg, &device_sig)?;
                if !device_pk.verify_hash(msg, &device_sig) {
                    return Err("Invalid NIST FIPS 205 SLH-DSA hardware telemetry signature");
                }
                (
                    ReplayRecord::SlhDsaNonce {
                        pk_commit,
                        nonce,
                        replay_digest,
                    },
                    WorkAccounting::EnergyDeployment { kwh_delivered },
                )
            }
        };

        // Commit state transition atomically
        self.utxos.remove(&input_ref);
        match replay_record {
            ReplayRecord::OneTimeKey(commit) => {
                self.spent_one_time_keys.insert(commit);
            }
            ReplayRecord::MerkleLeaf { root, leaf_index } => {
                self.spent_merkle_leaves
                    .entry(root)
                    .or_default()
                    .insert(leaf_index);
                self.merkle_next_min_leaf
                    .insert(root, leaf_index.saturating_add(1));
                self.total_merkle_leaves_consumed += 1;
            }
            ReplayRecord::SlhDsaNonce {
                pk_commit,
                nonce,
                replay_digest,
            } => {
                self.spent_slh_digests.insert(replay_digest);
                self.slh_next_min_nonce
                    .insert(pk_commit, nonce.saturating_add(1));
                self.total_slh_dsa_signatures_verified += 1;
            }
        }

        match accounting {
            WorkAccounting::LandDeed { parcel_id } => {
                self.acquired_parcels.push(parcel_id);
            }
            WorkAccounting::EnergyDeployment { kwh_delivered } => {
                self.total_kwh_verified += kwh_delivered;
            }
        }

        let txid = sha256_concat(&[
            &input_ref.0,
            &input_ref.1.to_be_bytes(),
            &payout_amount.to_be_bytes(),
            recipient_tag.as_bytes(),
            &self.state_root,
        ]);

        // Output 0: Payout to worker / land seller
        self.utxos.insert(
            (txid, 0),
            Utxo {
                txid,
                vout: 0,
                amount: payout_amount,
                owner_tag: recipient_tag.to_string(),
                covenant_hash: None,
            },
        );

        // Output 1: Recursive Covenant Change
        let change_ref = if change_amount > 0 {
            self.utxos.insert(
                (txid, 1),
                Utxo {
                    txid,
                    vout: 1,
                    amount: change_amount,
                    owner_tag: "COMMONS_TREASURY".to_string(),
                    covenant_hash: Some(expected_hash),
                },
            );
            Some((txid, 1))
        } else {
            None
        };

        self.recompute_state_root(txid);
        Ok((txid, change_ref))
    }

    /// Consolidates multiple UTXOs belonging to the exact same `CovenantRule` into a single
    /// covenanted UTXO (zero value leaves the covenant). Enables compounded streaming revenue
    /// to merge with treasury change to fund Parcel #2, #3, etc.
    pub fn consolidate_covenant_utxos(
        &mut self,
        input_refs: &[(Hash32, u32)],
        rule: &CovenantRule,
    ) -> Result<(Hash32, u32), &'static str> {
        if input_refs.len() < 2 {
            return Err("Consolidation requires at least two covenant UTXOs");
        }

        let expected_hash = rule.rule_hash();
        let mut seen = HashSet::with_capacity(input_refs.len());
        let mut total_amount: u64 = 0;

        for input_ref in input_refs {
            if !seen.insert(*input_ref) {
                return Err("Duplicate input UTXO in consolidation");
            }
            let utxo = self
                .utxos
                .get(input_ref)
                .ok_or("Input UTXO does not exist or was already spent")?;
            if utxo.covenant_hash != Some(expected_hash) {
                return Err("All consolidated UTXOs must carry the identical covenant rule_hash");
            }
            total_amount = total_amount
                .checked_add(utxo.amount)
                .ok_or("Consolidation balance overflow")?;
        }

        let mut txid_parts: Vec<u8> = Vec::from(&b"COVENANT_CONSOLIDATE_V2_1"[..]);
        for (txid, vout) in input_refs {
            txid_parts.extend_from_slice(txid);
            txid_parts.extend_from_slice(&vout.to_be_bytes());
        }
        txid_parts.extend_from_slice(&self.state_root);
        let new_txid = sha256(&txid_parts);

        for input_ref in input_refs {
            self.utxos.remove(input_ref);
        }

        self.utxos.insert(
            (new_txid, 0),
            Utxo {
                txid: new_txid,
                vout: 0,
                amount: total_amount,
                owner_tag: "COMMONS_TREASURY".to_string(),
                covenant_hash: Some(expected_hash),
            },
        );

        self.recompute_state_root(new_txid);
        Ok((new_txid, 0))
    }

    /// Settles a SHA-256 hash-chain streaming channel, routing protocol spread back to the Commons.
    pub fn settle_streaming_channel(
        &mut self,
        channel: &StreamingChannel,
        ticks_claimed: u64,
        final_preimage: Hash32,
        rule: &CovenantRule,
        operator_tag: &str,
    ) -> Result<(u64, u64), &'static str> {
        if ticks_claimed == 0 || ticks_claimed > channel.max_ticks {
            return Err("Invalid tick count");
        }
        // Verify SHA-256 hash chain: H^ticks(final_preimage) == channel.tip
        let mut cursor = final_preimage;
        for _ in 0..ticks_claimed {
            cursor = sha256(&cursor);
        }
        if cursor != channel.tip {
            return Err("Invalid streaming channel hash preimage");
        }

        let gross_revenue = ticks_claimed
            .checked_mul(channel.unit_price)
            .ok_or("Streaming gross revenue overflow")?;
        let commons_reinvest = ((gross_revenue as u128 * (rule.reinvest_bps as u128)) / 10_000u128) as u64;
        let operator_payout = gross_revenue - commons_reinvest;

        let txid = sha256_concat(&[
            b"CHANNEL_SETTLE",
            &channel.tip,
            &ticks_claimed.to_be_bytes(),
            &self.state_root,
        ]);

        // Lock reinvested surplus permanently into the recursive Land Covenant
        if commons_reinvest > 0 {
            self.utxos.insert(
                (txid, 0),
                Utxo {
                    txid,
                    vout: 0,
                    amount: commons_reinvest,
                    owner_tag: "COMMONS_TREASURY".to_string(),
                    covenant_hash: Some(rule.rule_hash()),
                },
            );
        }

        // Pay marginal operating cost to site operator / maintenance robot
        if operator_payout > 0 {
            self.utxos.insert(
                (txid, 1),
                Utxo {
                    txid,
                    vout: 1,
                    amount: operator_payout,
                    owner_tag: operator_tag.to_string(),
                    covenant_hash: None,
                },
            );
        }

        self.recompute_state_root(txid);
        Ok((commons_reinvest, operator_payout))
    }

    /// Returns all active UTXO references locked under `rule`, sorted deterministically.
    pub fn covenant_utxo_refs(&self, rule: &CovenantRule) -> Vec<(Hash32, u32)> {
        let target = Some(rule.rule_hash());
        let mut refs: Vec<(Hash32, u32)> = self
            .utxos
            .iter()
            .filter(|(_, u)| u.covenant_hash == target)
            .map(|(k, _)| *k)
            .collect();
        refs.sort();
        refs
    }

    pub fn total_covenant_balance(&self, rule: &CovenantRule) -> u64 {
        let target = Some(rule.rule_hash());
        self.utxos
            .values()
            .filter(|u| u.covenant_hash == target)
            .map(|u| u.amount)
            .sum()
    }

    pub fn balance_of_owner(&self, owner_tag: &str) -> u64 {
        self.utxos
            .values()
            .filter(|u| u.owner_tag == owner_tag)
            .map(|u| u.amount)
            .sum()
    }

    pub fn next_merkle_leaf_watermark(&self, root: Hash32) -> u32 {
        self.merkle_next_min_leaf.get(&root).copied().unwrap_or(0)
    }

    pub fn next_slh_nonce_watermark(&self, pk_commitment: Hash32) -> u64 {
        self.slh_next_min_nonce
            .get(&pk_commitment)
            .copied()
            .unwrap_or(0)
    }

    /// Produces a deterministic `NEWSATOSHI/2.1` L1 state-root anchor commitment.
    pub fn build_anchor_commitment(&self, rule: &CovenantRule) -> AnchorCommitment {
        AnchorCommitment::build(
            rule.rule_hash(),
            self.prev_state_root,
            self.state_root,
            self.tx_nonce,
            self.total_covenant_balance(rule),
            self.acquired_parcels.len() as u64,
            self.total_kwh_verified,
            self.total_merkle_leaves_consumed,
        )
    }
}

/// Sub-cent machine streaming channel using a pure SHA-256 PayWord hash chain.
pub struct StreamingChannel {
    pub tip: Hash32,
    pub max_ticks: u64,
    pub unit_price: u64,
}

impl StreamingChannel {
    pub fn create_with_secret_chain(
        seed: Hash32,
        max_ticks: u64,
        unit_price: u64,
    ) -> (Self, Vec<Hash32>) {
        let mut chain = Vec::with_capacity((max_ticks + 1) as usize);
        let mut current = seed;
        chain.push(current);
        for _ in 0..max_ticks {
            current = sha256(&current);
            chain.push(current);
        }
        let tip = *chain.last().unwrap();
        (
            Self {
                tip,
                max_ticks,
                unit_price,
            },
            chain,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_to_end_abundance_loop() {
        let mut ledger = CommonsLedger::new();

        // 1. Provision quantum-safe keys for County Deed Oracle and Solar/Robot Hardware
        let deed_oracle = LamportKeypair::from_seed(sha256(b"perpetual_trust_oracle_key_1"));
        let solar_device = LamportKeypair::from_seed(sha256(b"solar_inverter_tee_key_1"));

        let rule = CovenantRule {
            max_bounty_payout: 25_000,
            reinvest_bps: 1_500, // 15% of all output revenue compounds into buying more land
            min_soil_health_score: 80,
            authorized_deed_oracle_pk: deed_oracle.pk.commitment(),
            authorized_hardware_pk: solar_device.pk.commitment(),
            ..CovenantRule::default()
        };

        // 2. Seed initial Commons Treasury with 100,000 LOVE units
        let treasury_ref = ledger.mint_genesis_covenant(100_000, &rule);

        // 3. Adversarial test: attempting to strip the recursive covenant MUST fail
        let deed_hash = sha256(b"COUNTY_RECORD_PARCEL_404_PERPETUAL_PURPOSE_TRUST");
        let deed_sig = deed_oracle.sign(CheckedWorkProof::deed_msg(404, deed_hash));
        let rug_attempt = ledger.spend_covenant(
            treasury_ref,
            &rule,
            40_000,
            "LAND_SELLER",
            None, // Trying to drop covenant on change output
            CheckedWorkProof::LandDeedAcquisition {
                parcel_id: 404,
                deed_document_hash: deed_hash,
                oracle_pk: deed_oracle.pk.clone(),
                oracle_sig: deed_sig.clone(),
            },
        );
        assert!(rug_attempt.is_err());

        // 4. Legitimate Land Acquisition: 40,000 LOVE to seller, 60,000 locked recursively
        let (_, change_ref) = ledger
            .spend_covenant(
                treasury_ref,
                &rule,
                40_000,
                "LAND_SELLER",
                Some(rule.rule_hash()),
                CheckedWorkProof::LandDeedAcquisition {
                    parcel_id: 404,
                    deed_document_hash: deed_hash,
                    oracle_pk: deed_oracle.pk.clone(),
                    oracle_sig: deed_sig,
                },
            )
            .expect("Valid deed acquisition should succeed");

        let treasury_ref = change_ref.unwrap();
        assert_eq!(ledger.total_covenant_balance(&rule), 60_000);

        // 5. Physical Deployment Bounty: Solar + Greenhouse robot signs telemetry
        let energy_sig = solar_device.sign(CheckedWorkProof::energy_msg(404, 12_500, 92));
        let (_, change_ref) = ledger
            .spend_covenant(
                treasury_ref,
                &rule,
                15_000,
                "ROBOT_FLEET_OPERATOR",
                Some(rule.rule_hash()),
                CheckedWorkProof::PhysicalEnergyDeployment {
                    parcel_id: 404,
                    kwh_delivered: 12_500,
                    soil_health_score: 92,
                    device_pk: solar_device.pk.clone(),
                    device_sig: energy_sig,
                },
            )
            .expect("Valid hardware-attested bounty should unlock");

        assert!(change_ref.is_some());
        assert_eq!(ledger.total_covenant_balance(&rule), 45_000);

        // 6. Machine & Local Consumer Micropayment Streaming: 500 ticks @ 200 LOVE each
        let (channel, secret_chain) =
            StreamingChannel::create_with_secret_chain(sha256(b"consumer_wallet_seed"), 1_000, 200);
        let ticks_consumed = 500u64;
        // Reveal preimage 500 steps back from tip
        let revealed_preimage = secret_chain[(1_000 - ticks_consumed) as usize];

        let (reinvested, operator_paid) = ledger
            .settle_streaming_channel(
                &channel,
                ticks_consumed,
                revealed_preimage,
                &rule,
                "SITE_MAINTENANCE_BOT",
            )
            .expect("Valid hash-chain settlement should succeed");

        assert_eq!(reinvested, 15_000); // 15% of 100,000 gross revenue
        assert_eq!(operator_paid, 85_000);
        // Commons treasury automatically grew from 45,000 back to 60,000 to acquire Parcel #2
        assert_eq!(ledger.total_covenant_balance(&rule), 60_000);
        println!(
            "LOOP VERIFIED | State Root: {} | Commons Balance: {} LOVE",
            hex::encode(ledger.state_root),
            ledger.total_covenant_balance(&rule)
        );
    }
}
