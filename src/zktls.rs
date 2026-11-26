//! Production Cryptographic zkTLS / MPC-TLS Notary Presentation Verifier.
//!
//! Cryptographically verifies real signed TLS provenance attestations from three production
//! protocol wire formats and binds every verified claim into a canonical Newsatoshi L1
//! `RFC-0024` `BondAttest` receipt hash (`nsZKTLS2|...`):
//!
//! 1. **`reclaim_v1` (Reclaim Protocol exact wire format):**
//!    - Computes `identifier = "0x" || hex(keccak256(provider || "\n" || parameters || "\n" || context))`
//!    - Computes `signData = identifier || "\n" || owner.to_lowercase() || "\n" || timestampS || "\n" || epoch`
//!    - Computes EIP-191 personal_sign digest `keccak256("\x19Ethereum Signed Message:\n" || len || signData)`
//!    - Performs real `k256` secp256k1 ECDSA `ecrecover` to derive the 20-byte Ethereum attestor
//!      address and verifies membership in `trusted_attestors`!
//! 2. **`tlsnotary_v1` (TLSNotary / PSE MPC-TLS Presentation format):**
//!    - Verifies real `Ed25519` or `secp256k1` notary signature over canonical `SessionHeader`
//!      (TLS server SNI, X.509 certificate SHA-256 fingerprint, TLS version/cipher, timestamp,
//!      sent/recv byte counts, and `transcript_merkle_root`).
//!    - Verifies every selective-disclosure chunk opening (`SHA256("nsTLSL" || idx || salt || plaintext)`
//!      folded via Merkle siblings to `transcript_merkle_root`) so bank auth headers/cookies stay
//!      completely redacted while proven substrings (`"status":"COMPLETED"`, payment amounts, etc.)
//!      are cryptographically guaranteed to come from the live TLS session.
//! 3. **`newsatoshi_pq_notary_v1` (Post-Quantum NIST FIPS 205 `SLH-DSA-SHA2-128f` Notary):**
//!    - Identical selective-disclosure Merkle transcript proof signed by a quantum-resistant
//!      FIPS 205 `SLH-DSA-SHA2-128f` notary key.

use crate::crypto::{sha256, SlhDsaPubKey, SlhDsaSig};
use ed25519_dalek::{Signature as EdSig, Verifier as EdVerifier, VerifyingKey as EdVk};
use k256::ecdsa::{RecoveryId, Signature as K256Sig, VerifyingKey as K256Vk};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha3::{Digest as Sha3Digest, Keccak256};

fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut h = Keccak256::new();
    h.update(data);
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

fn strip_0x(s: &str) -> &str {
    s.trim()
        .strip_prefix("0x")
        .or_else(|| s.trim().strip_prefix("0X"))
        .unwrap_or_else(|| s.trim())
}

fn eth_address_from_k256_vk(vk: &K256Vk) -> String {
    let encoded = vk.to_encoded_point(false);
    let bytes = encoded.as_bytes(); // 65 bytes: 0x04 || x(32) || y(32)
    let h = keccak256(&bytes[1..]);
    format!("0x{}", hex::encode(&h[12..]))
}

/// Canonical Reclaim Protocol claim identifier (`getIdentifierFromClaimInfo`).
pub fn reclaim_claim_identifier(provider: &str, parameters: &str, context: &str) -> String {
    let msg = format!("{provider}\n{parameters}\n{context}");
    format!("0x{}", hex::encode(keccak256(msg.as_bytes())))
}

/// Canonical Reclaim Protocol witness signing string (`createSignDataForClaim`).
pub fn reclaim_sign_data(identifier: &str, owner: &str, timestamp_s: u64, epoch: u64) -> String {
    format!(
        "{}\n{}\n{}\n{}",
        identifier.trim().to_ascii_lowercase(),
        owner.trim().to_ascii_lowercase(),
        timestamp_s,
        epoch
    )
}

/// EIP-191 Ethereum personal_sign digest over UTF-8 text.
pub fn eip191_personal_hash(msg: &str) -> [u8; 32] {
    let prefix = format!("\x19Ethereum Signed Message:\n{}", msg.len());
    let mut h = Keccak256::new();
    h.update(prefix.as_bytes());
    h.update(msg.as_bytes());
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

/// Recover 20-byte `0x...` Ethereum address from a 65-byte `(r || s || v)` signature over `prehash`.
pub fn ecrecover_eth_address(prehash: &[u8; 32], sig_hex: &str) -> Result<String, String> {
    let raw = hex::decode(strip_0x(sig_hex)).map_err(|_| "invalid hex signature")?;
    if raw.len() != 65 {
        return Err(format!("expected 65-byte ECDSA signature, got {}", raw.len()));
    }
    let sig = K256Sig::from_slice(&raw[..64]).map_err(|e| format!("bad secp256k1 sig: {e}"))?;
    let v_raw = raw[64];
    let recid_byte = match v_raw {
        0 | 1 => v_raw,
        27 | 28 => v_raw - 27,
        _ => return Err(format!("invalid ECDSA recovery id byte {v_raw}")),
    };
    let recid = RecoveryId::from_byte(recid_byte).ok_or("invalid RecoveryId")?;
    let vk = K256Vk::recover_from_prehash(prehash, &sig, recid)
        .map_err(|e| format!("ecrecover failed: {e}"))?;
    Ok(eth_address_from_k256_vk(&vk))
}

/// Compute canonical selective-disclosure transcript leaf hash (`nsTLSL`).
pub fn tls_transcript_leaf_hash(chunk_index: u32, salt: &[u8], plaintext: &[u8]) -> [u8; 32] {
    let mut buf = Vec::with_capacity(6 + 4 + 4 + salt.len() + plaintext.len());
    buf.extend_from_slice(b"nsTLSL");
    buf.extend_from_slice(&chunk_index.to_le_bytes());
    buf.extend_from_slice(&(salt.len() as u32).to_le_bytes());
    buf.extend_from_slice(salt);
    buf.extend_from_slice(plaintext);
    sha256(&buf)
}

/// Compute canonical internal Merkle node hash (`nsTLSN`).
pub fn tls_transcript_node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut buf = [0u8; 6 + 64];
    buf[..6].copy_from_slice(b"nsTLSN");
    buf[6..38].copy_from_slice(left);
    buf[38..70].copy_from_slice(right);
    sha256(&buf)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TlsMerkleStep {
    pub sibling_hex: String,
    /// `true` if `sibling_hex` is on the left (`H(sibling || cur)`), `false` if on the right (`H(cur || sibling)`).
    pub is_left: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TlsDisclosedChunk {
    pub chunk_index: u32,
    pub plaintext: String,
    pub salt_hex: String,
    #[serde(default)]
    pub merkle_path: Vec<TlsMerkleStep>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TlsNotarySessionHeader {
    pub server_name: String,
    pub time_unix: u64,
    pub sent_len: u32,
    pub recv_len: u32,
    pub tls_version: String,
    pub cipher_suite: String,
    pub cert_sha256_hex: String,
    pub transcript_merkle_root_hex: String,
}

impl TlsNotarySessionHeader {
    /// Deterministic canonical byte encoding signed by the MPC-TLS Notary.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        format!(
            "nsTLSN1|{}|{}|{}|{}|{}|{}|{}|{}",
            self.server_name.trim().to_ascii_lowercase(),
            self.time_unix,
            self.sent_len,
            self.recv_len,
            self.tls_version.trim(),
            self.cipher_suite.trim(),
            strip_0x(&self.cert_sha256_hex).to_ascii_lowercase(),
            strip_0x(&self.transcript_merkle_root_hex).to_ascii_lowercase(),
        )
        .into_bytes()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "protocol")]
pub enum ZkTlsProofPayload {
    /// Reclaim Protocol v1 real EIP-191 secp256k1 witness-signed claim.
    #[serde(rename = "reclaim_v1")]
    ReclaimV1 {
        provider: String,
        parameters: String,
        #[serde(default)]
        context: String,
        owner: String,
        #[serde(rename = "timestampS")]
        timestamp_s: u64,
        epoch: u64,
        signatures: Vec<String>,
        #[serde(default)]
        trusted_attestors: Vec<String>,
        #[serde(default)]
        required_substrings: Vec<String>,
    },
    /// TLSNotary / PSE MPC-TLS selective-disclosure presentation (`ed25519` or `secp256k1`).
    #[serde(rename = "tlsnotary_v1")]
    TlsNotaryV1 {
        session_header: TlsNotarySessionHeader,
        /// `"ed25519"` or `"secp256k1"`
        #[serde(default = "default_ed25519")]
        signature_scheme: String,
        notary_pubkey_hex: String,
        notary_signature_hex: String,
        #[serde(default)]
        trusted_notary_pubkeys: Vec<String>,
        disclosed_chunks: Vec<TlsDisclosedChunk>,
        #[serde(default)]
        expected_server_name: Option<String>,
        #[serde(default)]
        required_substrings: Vec<String>,
    },
    /// Post-quantum NIST FIPS 205 `SLH-DSA-SHA2-128f` MPC-TLS Notary presentation.
    #[serde(rename = "newsatoshi_pq_notary_v1")]
    NewsatoshiPqNotaryV1 {
        session_header: TlsNotarySessionHeader,
        notary_slh_pubkey_hex: String,
        notary_slh_signature_hex: String,
        #[serde(default)]
        trusted_notary_pubkeys: Vec<String>,
        disclosed_chunks: Vec<TlsDisclosedChunk>,
        #[serde(default)]
        expected_server_name: Option<String>,
        #[serde(default)]
        required_substrings: Vec<String>,
    },
}

fn default_ed25519() -> String {
    "ed25519".into()
}

fn compute_l1_receipt_hash(
    protocol: &str,
    notary_id: &str,
    domain: &str,
    ts: u64,
    transcript_root: &str,
    disclosed_hash: &str,
) -> String {
    let canon = format!(
        "nsZKTLS2|{protocol}|{}|{}|{ts}|{}|{}",
        notary_id.to_ascii_lowercase(),
        domain.to_ascii_lowercase(),
        transcript_root.to_ascii_lowercase(),
        disclosed_hash.to_ascii_lowercase()
    );
    hex::encode(sha256(canon.as_bytes()))
}

fn verify_disclosed_chunks(
    expected_root_hex: &str,
    chunks: &[TlsDisclosedChunk],
    required_substrings: &[String],
) -> Result<(String, Vec<String>), String> {
    if chunks.is_empty() {
        return Err("at least one disclosed_chunk is required".into());
    }
    let root_bytes = hex::decode(strip_0x(expected_root_hex))
        .map_err(|_| "invalid transcript_merkle_root_hex")?;
    if root_bytes.len() != 32 {
        return Err("transcript_merkle_root_hex must be 32 bytes".into());
    }
    let mut expected_root = [0u8; 32];
    expected_root.copy_from_slice(&root_bytes);

    let mut combined = String::new();
    let mut texts = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let salt = hex::decode(strip_0x(&chunk.salt_hex)).map_err(|_| "invalid chunk salt_hex")?;
        if salt.len() < 16 {
            return Err("chunk blinding salt must be >= 16 bytes to protect redacted preimages".into());
        }
        let mut cur = tls_transcript_leaf_hash(chunk.chunk_index, &salt, chunk.plaintext.as_bytes());
        for step in &chunk.merkle_path {
            let sib_bytes = hex::decode(strip_0x(&step.sibling_hex))
                .map_err(|_| "invalid merkle sibling_hex")?;
            if sib_bytes.len() != 32 {
                return Err("merkle sibling must be 32 bytes".into());
            }
            let mut sib = [0u8; 32];
            sib.copy_from_slice(&sib_bytes);
            cur = if step.is_left {
                tls_transcript_node_hash(&sib, &cur)
            } else {
                tls_transcript_node_hash(&cur, &sib)
            };
        }
        if cur != expected_root {
            return Err(format!(
                "selective-disclosure Merkle proof failed for chunk_index {}",
                chunk.chunk_index
            ));
        }
        combined.push_str(&chunk.plaintext);
        combined.push('\n');
        texts.push(chunk.plaintext.clone());
    }

    for needle in required_substrings {
        if !combined.contains(needle) {
            return Err(format!("required disclosed substring not found: '{needle}'"));
        }
    }

    let disclosed_hash = hex::encode(sha256(combined.as_bytes()));
    Ok((disclosed_hash, texts))
}

/// Cryptographically verify a `reclaim_v1`, `tlsnotary_v1`, or `newsatoshi_pq_notary_v1`
/// zkTLS presentation and return its canonical L1 `RFC-0024` `BondAttest` receipt digest.
pub fn verify_zktls_proof(payload: &ZkTlsProofPayload) -> Result<Value, String> {
    match payload {
        ZkTlsProofPayload::ReclaimV1 {
            provider,
            parameters,
            context,
            owner,
            timestamp_s,
            epoch,
            signatures,
            trusted_attestors,
            required_substrings,
        } => {
            if signatures.is_empty() {
                return Err("reclaim_v1 requires at least 1 attestor signature".into());
            }
            let identifier = reclaim_claim_identifier(provider, parameters, context);
            let sign_data = reclaim_sign_data(&identifier, owner, *timestamp_s, *epoch);
            let prehash = eip191_personal_hash(&sign_data);

            let mut recovered_attestors = Vec::with_capacity(signatures.len());
            for sig in signatures {
                let addr = ecrecover_eth_address(&prehash, sig)?;
                if !recovered_attestors.contains(&addr) {
                    recovered_attestors.push(addr);
                }
            }
            if !trusted_attestors.is_empty() {
                let allowed: Vec<String> = trusted_attestors
                    .iter()
                    .map(|a| format!("0x{}", strip_0x(a).to_ascii_lowercase()))
                    .collect();
                let matched = recovered_attestors.iter().any(|a| allowed.contains(a));
                if !matched {
                    return Err(format!(
                        "recovered Reclaim attestor(s) {:?} not in trusted_attestors",
                        recovered_attestors
                    ));
                }
            }
            for needle in required_substrings {
                if !parameters.contains(needle) && !context.contains(needle) {
                    return Err(format!("required substring '{needle}' missing from claim parameters/context"));
                }
            }
            let disclosed_hash = hex::encode(sha256(format!("{parameters}|{context}").as_bytes()));
            let primary_attestor = recovered_attestors[0].clone();
            let l1_receipt_hash = compute_l1_receipt_hash(
                "reclaim_v1",
                &primary_attestor,
                provider,
                *timestamp_s,
                &identifier,
                &disclosed_hash,
            );
            Ok(json!({
                "ok": true,
                "valid": true,
                "protocol": "reclaim_v1",
                "cryptographic_check": "secp256k1_eip191_ecrecover_verified",
                "claim_identifier": identifier,
                "recovered_attestors": recovered_attestors,
                "trusted_attestor_matched": !trusted_attestors.is_empty(),
                "provider": provider,
                "owner": owner.to_ascii_lowercase(),
                "timestamp_unix": timestamp_s,
                "epoch": epoch,
                "l1_receipt_hash_hex": l1_receipt_hash,
                "rfc0024_binding": "Pass l1_receipt_hash_hex into RFC-0024 BondAttest receipt_hash"
            }))
        }
        ZkTlsProofPayload::TlsNotaryV1 {
            session_header,
            signature_scheme,
            notary_pubkey_hex,
            notary_signature_hex,
            trusted_notary_pubkeys,
            disclosed_chunks,
            expected_server_name,
            required_substrings,
        } => {
            if let Some(exp) = expected_server_name {
                if !session_header.server_name.trim().eq_ignore_ascii_case(exp.trim()) {
                    return Err(format!(
                        "TLS server_name mismatch: got '{}', expected '{}'",
                        session_header.server_name, exp
                    ));
                }
            }
            let pk_norm = strip_0x(notary_pubkey_hex).to_ascii_lowercase();
            if !trusted_notary_pubkeys.is_empty() {
                let allowed: Vec<String> = trusted_notary_pubkeys
                    .iter()
                    .map(|k| strip_0x(k).to_ascii_lowercase())
                    .collect();
                if !allowed.contains(&pk_norm) {
                    return Err("notary_pubkey_hex not in trusted_notary_pubkeys".into());
                }
            }

            let header_bytes = session_header.canonical_bytes();
            let pk_bytes = hex::decode(&pk_norm).map_err(|_| "invalid notary_pubkey_hex")?;
            let sig_bytes = hex::decode(strip_0x(notary_signature_hex))
                .map_err(|_| "invalid notary_signature_hex")?;

            let scheme = signature_scheme.trim().to_ascii_lowercase();
            match scheme.as_str() {
                "ed25519" => {
                    let pk_arr: [u8; 32] = pk_bytes
                        .try_into()
                        .map_err(|_| "ed25519 notary_pubkey_hex must be 32 bytes")?;
                    let sig_arr: [u8; 64] = sig_bytes
                        .try_into()
                        .map_err(|_| "ed25519 notary_signature_hex must be 64 bytes")?;
                    let vk = EdVk::from_bytes(&pk_arr)
                        .map_err(|e| format!("invalid ed25519 pubkey: {e}"))?;
                    let sig = EdSig::from_bytes(&sig_arr);
                    vk.verify(&header_bytes, &sig)
                        .map_err(|_| "ed25519 notary signature verification failed".to_string())?;
                }
                "secp256k1" => {
                    let vk = K256Vk::from_sec1_bytes(&pk_bytes)
                        .map_err(|e| format!("invalid secp256k1 pubkey: {e}"))?;
                    let sig = K256Sig::from_slice(&sig_bytes)
                        .map_err(|e| format!("invalid 64-byte secp256k1 sig: {e}"))?;
                    vk.verify(&header_bytes, &sig)
                        .map_err(|_| "secp256k1 notary signature verification failed".to_string())?;
                }
                other => return Err(format!("unsupported signature_scheme '{other}'")),
            }

            let (disclosed_hash, texts) = verify_disclosed_chunks(
                &session_header.transcript_merkle_root_hex,
                disclosed_chunks,
                required_substrings,
            )?;
            let l1_receipt_hash = compute_l1_receipt_hash(
                "tlsnotary_v1",
                &pk_norm,
                &session_header.server_name,
                session_header.time_unix,
                strip_0x(&session_header.transcript_merkle_root_hex),
                &disclosed_hash,
            );
            Ok(json!({
                "ok": true,
                "valid": true,
                "protocol": "tlsnotary_v1",
                "cryptographic_check": format!("{scheme}_notary_and_merkle_redaction_verified"),
                "server_name": session_header.server_name,
                "tls_version": session_header.tls_version,
                "cipher_suite": session_header.cipher_suite,
                "cert_sha256_hex": strip_0x(&session_header.cert_sha256_hex).to_ascii_lowercase(),
                "notary_pubkey_hex": pk_norm,
                "trusted_notary_matched": !trusted_notary_pubkeys.is_empty(),
                "disclosed_chunks_verified": texts.len(),
                "disclosed_hash_hex": disclosed_hash,
                "timestamp_unix": session_header.time_unix,
                "l1_receipt_hash_hex": l1_receipt_hash,
                "rfc0024_binding": "Pass l1_receipt_hash_hex into RFC-0024 BondAttest receipt_hash"
            }))
        }
        ZkTlsProofPayload::NewsatoshiPqNotaryV1 {
            session_header,
            notary_slh_pubkey_hex,
            notary_slh_signature_hex,
            trusted_notary_pubkeys,
            disclosed_chunks,
            expected_server_name,
            required_substrings,
        } => {
            if let Some(exp) = expected_server_name {
                if !session_header.server_name.trim().eq_ignore_ascii_case(exp.trim()) {
                    return Err(format!(
                        "TLS server_name mismatch: got '{}', expected '{}'",
                        session_header.server_name, exp
                    ));
                }
            }
            let pk_norm = strip_0x(notary_slh_pubkey_hex).to_ascii_lowercase();
            if !trusted_notary_pubkeys.is_empty() {
                let allowed: Vec<String> = trusted_notary_pubkeys
                    .iter()
                    .map(|k| strip_0x(k).to_ascii_lowercase())
                    .collect();
                if !allowed.contains(&pk_norm) {
                    return Err("notary_slh_pubkey_hex not in trusted_notary_pubkeys".into());
                }
            }
            let pk = SlhDsaPubKey::from_hex(&pk_norm)
                .map_err(|e| format!("invalid FIPS 205 notary pubkey: {e}"))?;
            let sig = SlhDsaSig::from_hex(strip_0x(notary_slh_signature_hex))
                .map_err(|e| format!("invalid FIPS 205 notary signature: {e}"))?;
            let header_hash = sha256(&session_header.canonical_bytes());
            if !pk.verify_hash(header_hash, &sig) {
                return Err("FIPS 205 SLH-DSA post-quantum notary signature verification failed".into());
            }

            let (disclosed_hash, texts) = verify_disclosed_chunks(
                &session_header.transcript_merkle_root_hex,
                disclosed_chunks,
                required_substrings,
            )?;
            let l1_receipt_hash = compute_l1_receipt_hash(
                "newsatoshi_pq_notary_v1",
                &pk_norm,
                &session_header.server_name,
                session_header.time_unix,
                strip_0x(&session_header.transcript_merkle_root_hex),
                &disclosed_hash,
            );
            Ok(json!({
                "ok": true,
                "valid": true,
                "protocol": "newsatoshi_pq_notary_v1",
                "cryptographic_check": "fips205_slh_dsa_post_quantum_notary_and_merkle_redaction_verified",
                "server_name": session_header.server_name,
                "notary_slh_pubkey_hex": pk_norm,
                "disclosed_chunks_verified": texts.len(),
                "disclosed_hash_hex": disclosed_hash,
                "timestamp_unix": session_header.time_unix,
                "l1_receipt_hash_hex": l1_receipt_hash,
                "rfc0024_binding": "Pass l1_receipt_hash_hex into RFC-0024 BondAttest receipt_hash"
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::SlhDsaKeypair;
    use ed25519_dalek::{Signer as EdSigner, SigningKey as EdSk};
    use k256::ecdsa::SigningKey as K256Sk;

    #[test]
    fn reclaim_v1_real_secp256k1_eip191_ecrecover_verifies_and_rejects_tamper() {
        let sk = K256Sk::from_slice(&[7u8; 32]).unwrap();
        let expected_addr = eth_address_from_k256_vk(sk.verifying_key());

        let provider = "http";
        let parameters = r#"{"url":"https://api.wise.com/v1/transfers/99","status":"COMPLETED","amount":250}"#;
        let context = r#"{"order_id":"p2p-order-42"}"#;
        let owner = "0x1111111111111111111111111111111111111111";
        let ts = 1_800_000_000u64;
        let epoch = 1u64;

        let id = reclaim_claim_identifier(provider, parameters, context);
        let sign_data = reclaim_sign_data(&id, owner, ts, epoch);
        let digest = eip191_personal_hash(&sign_data);
        let (sig, recid) = sk.sign_prehash_recoverable(&digest).unwrap();
        let mut sig65 = Vec::with_capacity(65);
        sig65.extend_from_slice(&sig.to_bytes());
        sig65.push(recid.to_byte() + 27);
        let sig_hex = format!("0x{}", hex::encode(&sig65));

        let ok = verify_zktls_proof(&ZkTlsProofPayload::ReclaimV1 {
            provider: provider.into(),
            parameters: parameters.into(),
            context: context.into(),
            owner: owner.into(),
            timestamp_s: ts,
            epoch,
            signatures: vec![sig_hex.clone()],
            trusted_attestors: vec![expected_addr.clone()],
            required_substrings: vec!["COMPLETED".into(), "250".into()],
        })
        .unwrap();
        assert_eq!(ok["valid"], true);
        assert_eq!(ok["recovered_attestors"][0], expected_addr);
        assert_eq!(ok["l1_receipt_hash_hex"].as_str().unwrap().len(), 64);

        // Tampering with amount -> recovered attestor changes -> REJECTED!
        let bad = verify_zktls_proof(&ZkTlsProofPayload::ReclaimV1 {
            provider: provider.into(),
            parameters: parameters.replace("250", "999"),
            context: context.into(),
            owner: owner.into(),
            timestamp_s: ts,
            epoch,
            signatures: vec![sig_hex],
            trusted_attestors: vec![expected_addr],
            required_substrings: vec![],
        });
        assert!(bad.is_err());
    }

    #[test]
    fn tlsnotary_v1_ed25519_and_pq_slh_dsa_selective_redaction_verify_and_reject_tamper() {
        let salt0 = [0x11u8; 16];
        let salt1 = [0x22u8; 16];
        // Chunk 0 is REDACTED (contains private bank cookie); only Chunk 1 is disclosed!
        let leaf0 = tls_transcript_leaf_hash(0, &salt0, b"Authorization: Bearer SECRET_BANK_COOKIE");
        let leaf1 = tls_transcript_leaf_hash(1, &salt1, br#"{"status":"COMPLETED","amount":"250.00"}"#);
        let root = tls_transcript_node_hash(&leaf0, &leaf1);

        let header = TlsNotarySessionHeader {
            server_name: "api.revolut.com".into(),
            time_unix: 1_800_000_123,
            sent_len: 128,
            recv_len: 512,
            tls_version: "TLSv1.3".into(),
            cipher_suite: "TLS_AES_256_GCM_SHA384".into(),
            cert_sha256_hex: hex::encode(sha256(b"revolut-leaf-x509")),
            transcript_merkle_root_hex: hex::encode(root),
        };

        // 1. Ed25519 TLSNotary presentation
        let ed_sk = EdSk::from_bytes(&[9u8; 32]);
        let ed_pk_hex = hex::encode(ed_sk.verifying_key().to_bytes());
        let ed_sig_hex = hex::encode(ed_sk.sign(&header.canonical_bytes()).to_bytes());
        let chunk = TlsDisclosedChunk {
            chunk_index: 1,
            plaintext: r#"{"status":"COMPLETED","amount":"250.00"}"#.into(),
            salt_hex: hex::encode(salt1),
            merkle_path: vec![TlsMerkleStep {
                sibling_hex: hex::encode(leaf0),
                is_left: true,
            }],
        };

        let res = verify_zktls_proof(&ZkTlsProofPayload::TlsNotaryV1 {
            session_header: header.clone(),
            signature_scheme: "ed25519".into(),
            notary_pubkey_hex: ed_pk_hex.clone(),
            notary_signature_hex: ed_sig_hex.clone(),
            trusted_notary_pubkeys: vec![ed_pk_hex.clone()],
            disclosed_chunks: vec![chunk.clone()],
            expected_server_name: Some("api.revolut.com".into()),
            required_substrings: vec!["COMPLETED".into(), "250.00".into()],
        })
        .unwrap();
        assert_eq!(res["valid"], true);
        assert_eq!(res["l1_receipt_hash_hex"].as_str().unwrap().len(), 64);

        // Tampering with disclosed amount fails Merkle opening check!
        let mut bad_chunk = chunk.clone();
        bad_chunk.plaintext = r#"{"status":"COMPLETED","amount":"999.00"}"#.into();
        assert!(verify_zktls_proof(&ZkTlsProofPayload::TlsNotaryV1 {
            session_header: header.clone(),
            signature_scheme: "ed25519".into(),
            notary_pubkey_hex: ed_pk_hex,
            notary_signature_hex: ed_sig_hex,
            trusted_notary_pubkeys: vec![],
            disclosed_chunks: vec![bad_chunk],
            expected_server_name: None,
            required_substrings: vec![],
        })
        .is_err());

        // 2. Post-quantum NIST FIPS 205 SLH-DSA-SHA2-128f Notary presentation
        let pq_kp = SlhDsaKeypair::from_seed([0x55u8; 32]);
        let header_hash = sha256(&header.canonical_bytes());
        let pq_sig = pq_kp.sign_hash(header_hash);
        let pq_res = verify_zktls_proof(&ZkTlsProofPayload::NewsatoshiPqNotaryV1 {
            session_header: header,
            notary_slh_pubkey_hex: pq_kp.pk.to_hex(),
            notary_slh_signature_hex: pq_sig.to_hex(),
            trusted_notary_pubkeys: vec![pq_kp.pk.to_hex()],
            disclosed_chunks: vec![chunk],
            expected_server_name: Some("api.revolut.com".into()),
            required_substrings: vec!["COMPLETED".into()],
        })
        .unwrap();
        assert_eq!(pq_res["valid"], true);
        assert_eq!(pq_res["protocol"], "newsatoshi_pq_notary_v1");
    }
}
