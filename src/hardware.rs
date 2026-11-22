use crate::crypto::{
    hash32_from_hex, sha256_concat, Hash32, MerkleLamportKeypair, MerkleLamportSig,
    MerkleSignError, SlhDsaKeypair, SlhDsaPubKey, SlhDsaSig, SLH_DSA_PARAM_SET,
};
use crate::CheckedWorkProof;
use serde::{Deserialize, Serialize};

/// Configuration for a field-deployed physical hardware signer (solar inverter / greenhouse robot).
#[derive(Clone, Debug)]
pub struct HardwareDeviceConfig {
    pub device_id: String,
    pub parcel_id: u64,
    pub tree_height: u8,
    pub master_seed: Hash32,
}

impl HardwareDeviceConfig {
    pub fn from_passphrase(
        device_id: impl Into<String>,
        parcel_id: u64,
        tree_height: u8,
        seed_passphrase: &str,
    ) -> Self {
        let device_id = device_id.into();
        let master_seed = sha256_concat(&[
            b"NEWSATOSHI_HW_TEE_SEED_V2_1",
            device_id.as_bytes(),
            &parcel_id.to_be_bytes(),
            seed_passphrase.as_bytes(),
        ]);
        Self {
            device_id,
            parcel_id,
            tree_height,
            master_seed,
        }
    }
}

/// Raw physical sensor reading sampled by the solar inverter & soil telemetry bus.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryReading {
    pub device_id: String,
    pub parcel_id: u64,
    pub kwh_delivered: u64,
    pub soil_health_score: u16,
    pub timestamp_unix: u64,
}

/// Compact JSON-friendly summary of a signed hardware telemetry packet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryPacketJson {
    pub protocol: String,
    pub device_id: String,
    pub parcel_id: u64,
    pub kwh_delivered: u64,
    pub soil_health_score: u16,
    pub timestamp_unix: u64,
    pub hardware_merkle_root: String,
    pub leaf_index: u32,
    pub tree_capacity: u32,
    pub remaining_leaves: u32,
    pub telemetry_msg_hash: String,
    pub lamport_sig_digest: String,
    pub auth_path_hashes: Vec<String>,
}

/// Full signed hardware telemetry packet containing the verifiable `MerkleLamportSig`.
#[derive(Clone, Debug)]
pub struct SignedTelemetryPacket {
    pub reading: TelemetryReading,
    pub hardware_root: Hash32,
    pub tree_capacity: u32,
    pub remaining_leaves: u32,
    pub msg_hash: Hash32,
    pub signature: MerkleLamportSig,
}

impl SignedTelemetryPacket {
    /// Verifies the packet's Merkleized Lamport signature against its burned-in hardware root.
    pub fn verify(&self) -> bool {
        let expected_msg = CheckedWorkProof::energy_msg(
            self.reading.parcel_id,
            self.reading.kwh_delivered,
            self.reading.soil_health_score,
        );
        expected_msg == self.msg_hash && self.signature.verify(self.hardware_root, expected_msg)
    }

    /// Converts this hardware telemetry packet into a `CheckedWorkProof` accepted by `CommonsLedger`.
    pub fn to_checked_work_proof(&self) -> CheckedWorkProof {
        CheckedWorkProof::merkle_energy_deployment(
            self.reading.parcel_id,
            self.reading.kwh_delivered,
            self.reading.soil_health_score,
            self.signature.clone(),
        )
    }

    /// Produces a structured JSON-serializable view of the signed telemetry packet.
    pub fn to_json_summary(&self) -> TelemetryPacketJson {
        TelemetryPacketJson {
            protocol: "NEWSATOSHI/2.1-TELEMETRY".to_string(),
            device_id: self.reading.device_id.clone(),
            parcel_id: self.reading.parcel_id,
            kwh_delivered: self.reading.kwh_delivered,
            soil_health_score: self.reading.soil_health_score,
            timestamp_unix: self.reading.timestamp_unix,
            hardware_merkle_root: hex::encode(self.hardware_root),
            leaf_index: self.signature.leaf_index,
            tree_capacity: self.tree_capacity,
            remaining_leaves: self.remaining_leaves,
            telemetry_msg_hash: hex::encode(self.msg_hash),
            lamport_sig_digest: hex::encode(self.signature.lamport_sig.digest()),
            auth_path_hashes: self
                .signature
                .auth_path
                .iter()
                .map(hex::encode)
                .collect(),
        }
    }

    /// Converts this packet into a full wire-transmittable `ExternalTelemetryPacket` for `POST /api/telemetry`.
    pub fn to_external_wire_packet(&self, payout_love: u64) -> ExternalTelemetryPacket {
        ExternalTelemetryPacket {
            protocol: "NEWSATOSHI/2.1-HW-TELEMETRY".to_string(),
            signature_scheme: "xmss".to_string(),
            device_id: self.reading.device_id.clone(),
            parcel_id: self.reading.parcel_id,
            kwh_delivered: self.reading.kwh_delivered,
            soil_health_score: self.reading.soil_health_score,
            payout_love,
            timestamp_unix: self.reading.timestamp_unix,
            hardware_merkle_root: Some(hex::encode(self.hardware_root)),
            merkle_sig: Some(self.signature.clone()),
            slh_dsa_param_set: None,
            slh_dsa_pk_hex: None,
            slh_dsa_nonce: None,
            slh_dsa_sig_hex: None,
            telemetry_msg_hash: hex::encode(self.msg_hash),
        }
    }
}

/// Full wire-transmittable signed hardware telemetry packet emitted by `newsatoshi-hw-signer`
/// and accepted by `POST /api/telemetry` on `newsatoshi-commons serve`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalTelemetryPacket {
    pub protocol: String,
    /// `"xmss"` or `"slh_dsa"`.
    pub signature_scheme: String,
    pub device_id: String,
    pub parcel_id: u64,
    pub kwh_delivered: u64,
    pub soil_health_score: u16,
    #[serde(default = "default_hw_payout")]
    pub payout_love: u64,
    pub timestamp_unix: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hardware_merkle_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merkle_sig: Option<MerkleLamportSig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slh_dsa_param_set: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slh_dsa_pk_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slh_dsa_nonce: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slh_dsa_sig_hex: Option<String>,
    pub telemetry_msg_hash: String,
}

fn default_hw_payout() -> u64 {
    5_000
}

impl ExternalTelemetryPacket {
    /// Signs a telemetry packet using NIST FIPS 205 SLH-DSA-SHA2-128f with monotonic `nonce`.
    #[allow(clippy::too_many_arguments)]
    pub fn sign_slh_dsa(
        kp: &SlhDsaKeypair,
        device_id: impl Into<String>,
        parcel_id: u64,
        kwh_delivered: u64,
        soil_health_score: u16,
        payout_love: u64,
        nonce: u64,
        timestamp_unix: u64,
    ) -> Self {
        let msg_hash = CheckedWorkProof::energy_msg_with_nonce(
            parcel_id,
            kwh_delivered,
            soil_health_score,
            nonce,
        );
        let sig = kp.sign_hash(msg_hash);
        Self {
            protocol: "NEWSATOSHI/2.1-HW-TELEMETRY".to_string(),
            signature_scheme: "slh_dsa".to_string(),
            device_id: device_id.into(),
            parcel_id,
            kwh_delivered,
            soil_health_score,
            payout_love,
            timestamp_unix,
            hardware_merkle_root: None,
            merkle_sig: None,
            slh_dsa_param_set: Some(SLH_DSA_PARAM_SET.to_string()),
            slh_dsa_pk_hex: Some(kp.pk.to_hex()),
            slh_dsa_nonce: Some(nonce),
            slh_dsa_sig_hex: Some(sig.to_hex()),
            telemetry_msg_hash: hex::encode(msg_hash),
        }
    }

    /// Converts this external hardware wire packet into a `CheckedWorkProof` for `CommonsLedger`.
    pub fn to_checked_work_proof(&self) -> Result<CheckedWorkProof, String> {
        match self.signature_scheme.as_str() {
            "xmss" => {
                let sig = self
                    .merkle_sig
                    .clone()
                    .ok_or_else(|| "missing merkle_sig in xmss telemetry packet".to_string())?;
                Ok(CheckedWorkProof::merkle_energy_deployment(
                    self.parcel_id,
                    self.kwh_delivered,
                    self.soil_health_score,
                    sig,
                ))
            }
            "slh_dsa" => {
                let pk_hex = self
                    .slh_dsa_pk_hex
                    .as_deref()
                    .ok_or_else(|| "missing slh_dsa_pk_hex in slh_dsa telemetry packet".to_string())?;
                let sig_hex = self.slh_dsa_sig_hex.as_deref().ok_or_else(|| {
                    "missing slh_dsa_sig_hex in slh_dsa telemetry packet".to_string()
                })?;
                let nonce = self
                    .slh_dsa_nonce
                    .ok_or_else(|| "missing slh_dsa_nonce in slh_dsa telemetry packet".to_string())?;
                let device_pk = SlhDsaPubKey::from_hex(pk_hex)?;
                let device_sig = SlhDsaSig::from_hex(sig_hex)?;
                Ok(CheckedWorkProof::slh_dsa_energy_deployment(
                    self.parcel_id,
                    self.kwh_delivered,
                    self.soil_health_score,
                    nonce,
                    device_pk,
                    device_sig,
                ))
            }
            other => Err(format!("unsupported hardware signature_scheme '{other}'")),
        }
    }
}

/// Embedded Hardware Telemetry Signer (ESP32 / Raspberry Pi / Linux TEE simulator).
///
/// Holds a single stateful `MerkleLamportKeypair` whose 32-byte root is burned into the
/// Commons `CovenantRule`. Each call to `sign_telemetry` consumes one leaf index monotonically.
#[derive(Clone, Debug)]
pub struct HardwareTelemetrySigner {
    pub device_id: String,
    pub parcel_id: u64,
    keypair: MerkleLamportKeypair,
}

impl HardwareTelemetrySigner {
    pub fn new(config: HardwareDeviceConfig) -> Result<Self, MerkleSignError> {
        let keypair = MerkleLamportKeypair::from_seed(config.master_seed, config.tree_height)?;
        Ok(Self {
            device_id: config.device_id,
            parcel_id: config.parcel_id,
            keypair,
        })
    }

    pub fn from_keypair(
        device_id: impl Into<String>,
        parcel_id: u64,
        keypair: MerkleLamportKeypair,
    ) -> Self {
        Self {
            device_id: device_id.into(),
            parcel_id,
            keypair,
        }
    }

    pub fn root(&self) -> Hash32 {
        self.keypair.root()
    }

    pub fn next_leaf_index(&self) -> u32 {
        self.keypair.next_index()
    }

    pub fn remaining_leaves(&self) -> u32 {
        self.keypair.remaining_signatures()
    }

    pub fn total_capacity(&self) -> u32 {
        self.keypair.total_capacity()
    }

    /// Signs a live `kwh_delivered` & `soil_health_score` reading with the next unused leaf.
    pub fn sign_telemetry(
        &mut self,
        kwh_delivered: u64,
        soil_health_score: u16,
        timestamp_unix: u64,
    ) -> Result<SignedTelemetryPacket, MerkleSignError> {
        let msg_hash =
            CheckedWorkProof::energy_msg(self.parcel_id, kwh_delivered, soil_health_score);
        let signature = self.keypair.sign_next(msg_hash)?;

        Ok(SignedTelemetryPacket {
            reading: TelemetryReading {
                device_id: self.device_id.clone(),
                parcel_id: self.parcel_id,
                kwh_delivered,
                soil_health_score,
                timestamp_unix,
            },
            hardware_root: self.keypair.root(),
            tree_capacity: self.keypair.total_capacity(),
            remaining_leaves: self.keypair.remaining_signatures(),
            msg_hash,
            signature,
        })
    }
}

/// Externally signed land deed packet submitted by an offline Perpetual Purpose Trust / Deed Oracle
/// directly to `POST /api/deeds` (works in `--verify-only` mode with zero server-side private keys).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExternalDeedPacket {
    pub protocol: String,
    /// `"xmss"` or `"slh_dsa"`.
    pub signature_scheme: String,
    pub parcel_id: u64,
    pub acres: f64,
    pub price_love: u64,
    pub deed_label: String,
    pub deed_document_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merkle_sig: Option<MerkleLamportSig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slh_dsa_pk_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slh_dsa_nonce: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slh_dsa_sig_hex: Option<String>,
}

impl ExternalDeedPacket {
    /// Signs a land deed packet offline using NIST FIPS 205 SLH-DSA-SHA2-128f.
    #[allow(clippy::too_many_arguments)]
    pub fn sign_slh_dsa(
        kp: &SlhDsaKeypair,
        parcel_id: u64,
        acres: f64,
        price_love: u64,
        deed_label: impl Into<String>,
        deed_document_hash: Hash32,
        nonce: u64,
    ) -> Self {
        let msg = CheckedWorkProof::deed_msg_with_nonce(parcel_id, deed_document_hash, nonce);
        let sig = kp.sign_hash(msg);
        Self {
            protocol: "NEWSATOSHI/2.1-DEED-ORACLE".to_string(),
            signature_scheme: "slh_dsa".to_string(),
            parcel_id,
            acres,
            price_love,
            deed_label: deed_label.into(),
            deed_document_hash: hex::encode(deed_document_hash),
            merkle_sig: None,
            slh_dsa_pk_hex: Some(kp.pk.to_hex()),
            slh_dsa_nonce: Some(nonce),
            slh_dsa_sig_hex: Some(sig.to_hex()),
        }
    }

    /// Signs a land deed packet offline using stateful XMSS Merkle-Lamport.
    pub fn sign_xmss(
        kp: &mut MerkleLamportKeypair,
        parcel_id: u64,
        acres: f64,
        price_love: u64,
        deed_label: impl Into<String>,
        deed_document_hash: Hash32,
    ) -> Result<Self, MerkleSignError> {
        let msg = CheckedWorkProof::deed_msg(parcel_id, deed_document_hash);
        let sig = kp.sign_next(msg)?;
        Ok(Self {
            protocol: "NEWSATOSHI/2.1-DEED-ORACLE".to_string(),
            signature_scheme: "xmss".to_string(),
            parcel_id,
            acres,
            price_love,
            deed_label: deed_label.into(),
            deed_document_hash: hex::encode(deed_document_hash),
            merkle_sig: Some(sig),
            slh_dsa_pk_hex: None,
            slh_dsa_nonce: None,
            slh_dsa_sig_hex: None,
        })
    }

    /// Converts this external deed attestation into a verifiable `CheckedWorkProof`.
    pub fn to_checked_work_proof(&self) -> Result<(Hash32, CheckedWorkProof), String> {
        let doc_hash = hash32_from_hex(&self.deed_document_hash)
            .map_err(|e| format!("invalid deed_document_hash: {e}"))?;
        match self.signature_scheme.as_str() {
            "xmss" => {
                let sig = self
                    .merkle_sig
                    .clone()
                    .ok_or_else(|| "missing merkle_sig in xmss deed packet".to_string())?;
                Ok((
                    doc_hash,
                    CheckedWorkProof::merkle_land_deed(self.parcel_id, doc_hash, sig),
                ))
            }
            "slh_dsa" => {
                let pk_hex = self
                    .slh_dsa_pk_hex
                    .as_deref()
                    .ok_or_else(|| "missing slh_dsa_pk_hex in slh_dsa deed packet".to_string())?;
                let sig_hex = self
                    .slh_dsa_sig_hex
                    .as_deref()
                    .ok_or_else(|| "missing slh_dsa_sig_hex in slh_dsa deed packet".to_string())?;
                let nonce = self
                    .slh_dsa_nonce
                    .ok_or_else(|| "missing slh_dsa_nonce in slh_dsa deed packet".to_string())?;
                let oracle_pk = SlhDsaPubKey::from_hex(pk_hex)?;
                let oracle_sig = SlhDsaSig::from_hex(sig_hex)?;
                Ok((
                    doc_hash,
                    CheckedWorkProof::slh_dsa_land_deed(
                        self.parcel_id,
                        doc_hash,
                        nonce,
                        oracle_pk,
                        oracle_sig,
                    ),
                ))
            }
            other => Err(format!("unsupported deed signature_scheme '{other}'")),
        }
    }
}
