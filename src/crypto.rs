use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use slh_dsa::signature::{Keypair, Signer, Verifier};
use slh_dsa::{Sha2_128f, Signature as RawSlhSignature, SigningKey, VerifyingKey};
use std::fmt;

pub type Hash32 = [u8; 32];

/// Default Merkle tree height: 2^6 = 64 sequential one-time Lamport leaves.
pub const DEFAULT_TREE_HEIGHT: u8 = 6;
/// Large Merkle tree height: 2^10 = 1,024 sequential one-time Lamport leaves.
pub const LARGE_TREE_HEIGHT: u8 = 10;
/// Maximum supported in-memory tree height: 2^12 = 4,096 leaves.
pub const MAX_TREE_HEIGHT: u8 = 12;

/// Active NIST FIPS 205 parameter set (pure SHA-256 floor, fast interactive signing).
pub const SLH_DSA_PARAM_SET: &str = "SLH-DSA-SHA2-128f";
/// Serialized verifying key length for SLH-DSA-SHA2-128f (2 * 16 bytes).
pub const SLH_DSA_PK_BYTES: usize = 32;
/// Serialized signing key length for SLH-DSA-SHA2-128f (4 * 16 bytes).
pub const SLH_DSA_SK_BYTES: usize = 64;

pub fn sha256(data: &[u8]) -> Hash32 {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

pub fn sha256_concat(parts: &[&[u8]]) -> Hash32 {
    let mut hasher = Sha256::new();
    for p in parts {
        hasher.update(p);
    }
    hasher.finalize().into()
}

/// Decodes a 64-character hex string into a 32-byte `Hash32`.
pub fn hash32_from_hex(hex_str: &str) -> Result<Hash32, String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("invalid hex: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!(
            "expected 32 bytes (64 hex chars), got {} bytes",
            bytes.len()
        ));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Quantum-safe SHA-256 Lamport One-Time Signature (matching Newsatoshi's SHA-256-only floor).
#[derive(Clone, Debug)]
pub struct LamportKeypair {
    sk: Vec<[Hash32; 2]>,
    pub pk: LamportPubKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LamportPubKey {
    pub hashes: Vec<[Hash32; 2]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LamportSig {
    pub preimages: Vec<Hash32>,
}

impl LamportKeypair {
    pub fn from_seed(seed: Hash32) -> Self {
        let mut sk = Vec::with_capacity(256);
        let mut pk_hashes = Vec::with_capacity(256);

        for i in 0u16..256u16 {
            let idx = i.to_be_bytes();
            let s0 = sha256_concat(&[&seed, &idx, &[0u8]]);
            let s1 = sha256_concat(&[&seed, &idx, &[1u8]]);
            let p0 = sha256(&s0);
            let p1 = sha256(&s1);
            sk.push([s0, s1]);
            pk_hashes.push([p0, p1]);
        }

        Self {
            sk,
            pk: LamportPubKey { hashes: pk_hashes },
        }
    }

    pub fn sign(&self, msg_hash: Hash32) -> LamportSig {
        let mut preimages = Vec::with_capacity(256);
        for i in 0..256 {
            let bit = (msg_hash[i / 8] >> (7 - (i % 8))) & 1;
            preimages.push(self.sk[i][bit as usize]);
        }
        LamportSig { preimages }
    }
}

impl LamportPubKey {
    pub fn commitment(&self) -> Hash32 {
        let mut hasher = Sha256::new();
        for pair in &self.hashes {
            hasher.update(pair[0]);
            hasher.update(pair[1]);
        }
        hasher.finalize().into()
    }

    pub fn verify(&self, msg_hash: Hash32, sig: &LamportSig) -> bool {
        if self.hashes.len() != 256 || sig.preimages.len() != 256 {
            return false;
        }
        for i in 0..256 {
            let bit = (msg_hash[i / 8] >> (7 - (i % 8))) & 1;
            if sha256(&sig.preimages[i]) != self.hashes[i][bit as usize] {
                return false;
            }
        }
        true
    }
}

impl LamportSig {
    /// Compact SHA-256 fingerprint of the 8 KiB signature preimage vector.
    pub fn digest(&self) -> Hash32 {
        let mut hasher = Sha256::new();
        hasher.update(b"LAMPORT_SIG_DIGEST_V1");
        for preimage in &self.preimages {
            hasher.update(preimage);
        }
        hasher.finalize().into()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MerkleSignError {
    InvalidTreeHeight { requested: u8, max: u8 },
    TreeExhausted { capacity: u32 },
    LeafIndexOutOfBounds { index: u32, capacity: u32 },
}

impl fmt::Display for MerkleSignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTreeHeight { requested, max } => {
                write!(f, "invalid Merkle tree height {requested} (must be 1..={max})")
            }
            Self::TreeExhausted { capacity } => {
                write!(
                    f,
                    "Merkle Lamport keypair exhausted: all {capacity} one-time leaves have been spent"
                )
            }
            Self::LeafIndexOutOfBounds { index, capacity } => {
                write!(
                    f,
                    "leaf index {index} out of bounds for tree capacity {capacity}"
                )
            }
        }
    }
}

impl std::error::Error for MerkleSignError {}

/// Domain-separated leaf seed derivation from the hardware/oracle master seed.
pub fn derive_leaf_seed(master_seed: Hash32, leaf_index: u32) -> Hash32 {
    sha256_concat(&[
        b"NEWSATOSHI_XMSS_LEAF_SEED_V1",
        &master_seed,
        &leaf_index.to_be_bytes(),
    ])
}

/// Domain-separated leaf hash binding leaf index and Lamport public key commitment.
pub fn merkle_leaf_hash(leaf_index: u32, leaf_pk_commitment: Hash32) -> Hash32 {
    sha256_concat(&[
        b"NEWSATOSHI_XMSS_LEAF_V1",
        &leaf_index.to_be_bytes(),
        &leaf_pk_commitment,
    ])
}

/// Domain-separated internal node hash preventing second-preimage / leaf-node confusion attacks.
pub fn merkle_node_hash(level: u8, node_index: u32, left: Hash32, right: Hash32) -> Hash32 {
    sha256_concat(&[
        b"NEWSATOSHI_XMSS_NODE_V1",
        &[level],
        &node_index.to_be_bytes(),
        &left,
        &right,
    ])
}

/// Pure SHA-256 XMSS-style Merkleized Multi-Use Lamport Signature Tree.
///
/// A single burned-in 32-byte Merkle root can authenticate up to `2^height` sequential
/// one-time Lamport signatures (e.g., 64 leaves for height 6, 1,024 leaves for height 10)
/// with automatic monotonic leaf-index state to prevent catastrophic one-time key reuse.
///
/// Only the 32-byte `master_seed` and Merkle tree hashes are retained in memory; individual
/// Lamport keypairs are derived on demand when signing, keeping RAM footprint embedded-friendly.
#[derive(Clone, Debug)]
pub struct MerkleLamportKeypair {
    master_seed: Hash32,
    height: u8,
    next_leaf_index: u32,
    /// `layers[0]` holds leaf hashes (len = 2^height); `layers[height]` holds `[root]`.
    layers: Vec<Vec<Hash32>>,
}

/// Self-contained Merkleized Lamport signature verifiable against a single 32-byte Merkle root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleLamportSig {
    pub leaf_index: u32,
    pub leaf_pk: LamportPubKey,
    pub lamport_sig: LamportSig,
    pub auth_path: Vec<Hash32>,
}

impl MerkleLamportKeypair {
    /// Builds a Merkleized Lamport tree with `2^height` leaves (height `1..=12`).
    pub fn from_seed(master_seed: Hash32, height: u8) -> Result<Self, MerkleSignError> {
        if height == 0 || height > MAX_TREE_HEIGHT {
            return Err(MerkleSignError::InvalidTreeHeight {
                requested: height,
                max: MAX_TREE_HEIGHT,
            });
        }

        let num_leaves = 1u32 << height;
        let mut leaf_hashes = Vec::with_capacity(num_leaves as usize);
        for idx in 0..num_leaves {
            let leaf_seed = derive_leaf_seed(master_seed, idx);
            let kp = LamportKeypair::from_seed(leaf_seed);
            leaf_hashes.push(merkle_leaf_hash(idx, kp.pk.commitment()));
        }

        let mut layers = Vec::with_capacity((height + 1) as usize);
        layers.push(leaf_hashes);

        for level in 1..=height {
            let prev = &layers[(level - 1) as usize];
            let next_len = prev.len() / 2;
            let mut next_layer = Vec::with_capacity(next_len);
            for i in 0..next_len {
                let node = merkle_node_hash(level, i as u32, prev[2 * i], prev[2 * i + 1]);
                next_layer.push(node);
            }
            layers.push(next_layer);
        }

        Ok(Self {
            master_seed,
            height,
            next_leaf_index: 0,
            layers,
        })
    }

    /// Reconstructs a `MerkleLamportKeypair` from its master seed, height, and persisted leaf cursor.
    pub fn from_seed_at_index(
        master_seed: Hash32,
        height: u8,
        next_leaf_index: u32,
    ) -> Result<Self, MerkleSignError> {
        let mut kp = Self::from_seed(master_seed, height)?;
        if next_leaf_index > kp.total_capacity() {
            return Err(MerkleSignError::LeafIndexOutOfBounds {
                index: next_leaf_index,
                capacity: kp.total_capacity(),
            });
        }
        kp.next_leaf_index = next_leaf_index;
        Ok(kp)
    }

    /// Convenience constructor for a 64-leaf tree (`height = 6`).
    pub fn new_64(master_seed: Hash32) -> Self {
        Self::from_seed(master_seed, DEFAULT_TREE_HEIGHT).expect("height 6 is valid")
    }

    /// Convenience constructor for a 1,024-leaf tree (`height = 10`).
    pub fn new_1024(master_seed: Hash32) -> Self {
        Self::from_seed(master_seed, LARGE_TREE_HEIGHT).expect("height 10 is valid")
    }

    pub fn master_seed(&self) -> Hash32 {
        self.master_seed
    }

    /// The single 32-byte root public key burned into the `CovenantRule` / hardware TEE.
    pub fn root(&self) -> Hash32 {
        self.layers[self.height as usize][0]
    }

    pub fn height(&self) -> u8 {
        self.height
    }

    pub fn total_capacity(&self) -> u32 {
        1u32 << self.height
    }

    pub fn next_index(&self) -> u32 {
        self.next_leaf_index
    }

    pub fn remaining_signatures(&self) -> u32 {
        self.total_capacity().saturating_sub(self.next_leaf_index)
    }

    /// Computes the Merkle authentication path for `leaf_index` (length = `self.height`).
    pub fn auth_path(&self, leaf_index: u32) -> Result<Vec<Hash32>, MerkleSignError> {
        let capacity = self.total_capacity();
        if leaf_index >= capacity {
            return Err(MerkleSignError::LeafIndexOutOfBounds {
                index: leaf_index,
                capacity,
            });
        }

        let mut path = Vec::with_capacity(self.height as usize);
        let mut cursor = leaf_index as usize;
        for layer in self.layers.iter().take(self.height as usize) {
            let sibling = cursor ^ 1;
            path.push(layer[sibling]);
            cursor >>= 1;
        }
        Ok(path)
    }

    /// Signs `msg_hash` using the next unused one-time Lamport leaf and advances the state cursor.
    ///
    /// Guarantees a keypair instance will never reuse a Lamport leaf index.
    pub fn sign_next(&mut self, msg_hash: Hash32) -> Result<MerkleLamportSig, MerkleSignError> {
        let capacity = self.total_capacity();
        if self.next_leaf_index >= capacity {
            return Err(MerkleSignError::TreeExhausted { capacity });
        }

        let idx = self.next_leaf_index;
        self.next_leaf_index += 1;

        let leaf_seed = derive_leaf_seed(self.master_seed, idx);
        let leaf_kp = LamportKeypair::from_seed(leaf_seed);
        let lamport_sig = leaf_kp.sign(msg_hash);
        let auth_path = self.auth_path(idx)?;

        Ok(MerkleLamportSig {
            leaf_index: idx,
            leaf_pk: leaf_kp.pk,
            lamport_sig,
            auth_path,
        })
    }

    /// Advance the internal cursor without signing (useful for simulating out-of-order packets
    /// or restoring monotonic state from non-volatile hardware storage).
    pub fn skip_leaves(&mut self, count: u32) -> Result<(), MerkleSignError> {
        let new_idx = self.next_leaf_index.saturating_add(count);
        if new_idx > self.total_capacity() {
            return Err(MerkleSignError::TreeExhausted {
                capacity: self.total_capacity(),
            });
        }
        self.next_leaf_index = new_idx;
        Ok(())
    }

    /// Signs with an explicit leaf index WITHOUT checking whether it was already used.
    /// Exposed strictly for adversarial replay/key-reuse tests against `CommonsLedger`.
    pub fn sign_at_index_unchecked(
        &self,
        leaf_index: u32,
        msg_hash: Hash32,
    ) -> Result<MerkleLamportSig, MerkleSignError> {
        let auth_path = self.auth_path(leaf_index)?;
        let leaf_seed = derive_leaf_seed(self.master_seed, leaf_index);
        let leaf_kp = LamportKeypair::from_seed(leaf_seed);
        let lamport_sig = leaf_kp.sign(msg_hash);

        Ok(MerkleLamportSig {
            leaf_index,
            leaf_pk: leaf_kp.pk,
            lamport_sig,
            auth_path,
        })
    }
}

impl MerkleLamportSig {
    /// Recomputes the Merkle root implied by this signature's leaf public key and auth path.
    pub fn compute_implied_root(&self) -> Option<Hash32> {
        let height = self.auth_path.len();
        if height == 0 || height > MAX_TREE_HEIGHT as usize {
            return None;
        }
        let capacity = 1u32 << height;
        if self.leaf_index >= capacity {
            return None;
        }

        let mut node = merkle_leaf_hash(self.leaf_index, self.leaf_pk.commitment());
        let mut cursor = self.leaf_index;

        for (i, sibling) in self.auth_path.iter().enumerate() {
            let level = (i + 1) as u8;
            let parent_idx = cursor >> 1;
            let (left, right) = if (cursor & 1) == 0 {
                (node, *sibling)
            } else {
                (*sibling, node)
            };
            node = merkle_node_hash(level, parent_idx, left, right);
            cursor = parent_idx;
        }

        Some(node)
    }

    /// Verifies both the one-time Lamport signature over `msg_hash` and the Merkle inclusion
    /// proof against `expected_root`.
    pub fn verify(&self, expected_root: Hash32, msg_hash: Hash32) -> bool {
        let Some(implied_root) = self.compute_implied_root() else {
            return false;
        };
        if implied_root != expected_root {
            return false;
        }
        self.leaf_pk.verify(msg_hash, &self.lamport_sig)
    }
}

// ================================================================================================
// NIST FIPS 205 SLH-DSA (SLH-DSA-SHA2-128f) — Pure SHA-256 Stateless Post-Quantum Signatures
// ================================================================================================

/// NIST FIPS 205 SLH-DSA-SHA2-128f public verifying key (32 bytes).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlhDsaPubKey {
    pub bytes: [u8; SLH_DSA_PK_BYTES],
}

/// NIST FIPS 205 SLH-DSA-SHA2-128f detached signature (17,088 bytes).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlhDsaSig {
    pub bytes: Vec<u8>,
}

/// NIST FIPS 205 SLH-DSA-SHA2-128f keypair on Newsatoshi's pure SHA-256 post-quantum floor.
#[derive(Clone)]
pub struct SlhDsaKeypair {
    sk: SigningKey<Sha2_128f>,
    pub pk: SlhDsaPubKey,
}

impl fmt::Debug for SlhDsaKeypair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlhDsaKeypair")
            .field("param_set", &SLH_DSA_PARAM_SET)
            .field("pk_hex", &self.pk.to_hex())
            .finish()
    }
}

impl SlhDsaKeypair {
    /// Deterministically derives an SLH-DSA-SHA2-128f keypair from a 32-byte seed using SHA-256.
    pub fn from_seed(seed: Hash32) -> Self {
        let sk_seed_hash = sha256_concat(&[b"NEWSATOSHI_SLH_DSA_SK_SEED_V1", &seed]);
        let sk_prf_hash = sha256_concat(&[b"NEWSATOSHI_SLH_DSA_SK_PRF_V1", &seed]);
        let pk_seed_hash = sha256_concat(&[b"NEWSATOSHI_SLH_DSA_PK_SEED_V1", &seed]);

        let sk = SigningKey::<Sha2_128f>::slh_keygen_internal(
            &sk_seed_hash[..16],
            &sk_prf_hash[..16],
            &pk_seed_hash[..16],
        );
        let pk_bytes_arr = sk.verifying_key().to_bytes();
        let mut bytes = [0u8; SLH_DSA_PK_BYTES];
        bytes.copy_from_slice(pk_bytes_arr.as_slice());

        Self {
            sk,
            pk: SlhDsaPubKey { bytes },
        }
    }

    /// Serializes the 64-byte FIPS 205 signing key.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.sk.to_bytes().as_slice().to_vec()
    }

    /// Deserializes a 64-byte FIPS 205 signing key.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let sk = SigningKey::<Sha2_128f>::try_from(bytes)
            .map_err(|_| format!("invalid SLH-DSA-SHA2-128f signing key bytes (expected {SLH_DSA_SK_BYTES} bytes)"))?;
        let pk_arr = sk.verifying_key().to_bytes();
        let mut pk_bytes = [0u8; SLH_DSA_PK_BYTES];
        pk_bytes.copy_from_slice(pk_arr.as_slice());
        Ok(Self {
            sk,
            pk: SlhDsaPubKey { bytes: pk_bytes },
        })
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.to_bytes())
    }

    pub fn from_hex(hex_str: &str) -> Result<Self, String> {
        let bytes = hex::decode(hex_str).map_err(|e| format!("invalid hex for SLH-DSA sk: {e}"))?;
        Self::from_bytes(&bytes)
    }

    /// Produces a deterministic NIST FIPS 205 SLH-DSA-SHA2-128f signature over `msg`.
    pub fn sign(&self, msg: &[u8]) -> SlhDsaSig {
        let sig: RawSlhSignature<Sha2_128f> = self
            .sk
            .try_sign(msg)
            .expect("deterministic SLH-DSA-SHA2-128f signing is infallible");
        SlhDsaSig {
            bytes: sig.to_vec(),
        }
    }

    /// Convenience wrapper signing a 32-byte digest.
    pub fn sign_hash(&self, msg_hash: Hash32) -> SlhDsaSig {
        self.sign(&msg_hash)
    }
}

impl SlhDsaPubKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != SLH_DSA_PK_BYTES {
            return Err(format!(
                "invalid SLH-DSA-SHA2-128f public key length: expected {SLH_DSA_PK_BYTES}, got {}",
                bytes.len()
            ));
        }
        // Validate round-trip with slh-dsa VerifyingKey
        let _vk = VerifyingKey::<Sha2_128f>::try_from(bytes)
            .map_err(|_| "invalid SLH-DSA-SHA2-128f verifying key encoding".to_string())?;
        let mut out = [0u8; SLH_DSA_PK_BYTES];
        out.copy_from_slice(bytes);
        Ok(Self { bytes: out })
    }

    pub fn to_bytes(&self) -> [u8; SLH_DSA_PK_BYTES] {
        self.bytes
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.bytes)
    }

    pub fn from_hex(hex_str: &str) -> Result<Self, String> {
        let bytes = hex::decode(hex_str).map_err(|e| format!("invalid hex for SLH-DSA pk: {e}"))?;
        Self::from_bytes(&bytes)
    }

    /// Domain-separated 32-byte SHA-256 commitment used in `CovenantRule`.
    pub fn commitment(&self) -> Hash32 {
        sha256_concat(&[b"NEWSATOSHI_SLH_DSA_SHA2_128F_PK_V1", &self.bytes])
    }

    /// Verifies a NIST FIPS 205 SLH-DSA-SHA2-128f signature over `msg`.
    pub fn verify(&self, msg: &[u8], sig: &SlhDsaSig) -> bool {
        let Ok(vk) = VerifyingKey::<Sha2_128f>::try_from(self.bytes.as_slice()) else {
            return false;
        };
        let Ok(raw_sig) = RawSlhSignature::<Sha2_128f>::try_from(sig.bytes.as_slice()) else {
            return false;
        };
        vk.verify(msg, &raw_sig).is_ok()
    }

    /// Convenience wrapper verifying a signature over a 32-byte digest.
    pub fn verify_hash(&self, msg_hash: Hash32, sig: &SlhDsaSig) -> bool {
        self.verify(&msg_hash, sig)
    }
}

impl SlhDsaSig {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let _parsed = RawSlhSignature::<Sha2_128f>::try_from(bytes).map_err(|_| {
            format!(
                "invalid SLH-DSA-SHA2-128f signature encoding (got {} bytes)",
                bytes.len()
            )
        })?;
        Ok(Self {
            bytes: bytes.to_vec(),
        })
    }

    pub fn to_hex(&self) -> String {
        hex::encode(&self.bytes)
    }

    pub fn from_hex(hex_str: &str) -> Result<Self, String> {
        let bytes =
            hex::decode(hex_str).map_err(|e| format!("invalid hex for SLH-DSA sig: {e}"))?;
        Self::from_bytes(&bytes)
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Compact SHA-256 fingerprint of the SLH-DSA signature bytes.
    pub fn digest(&self) -> Hash32 {
        sha256_concat(&[b"SLH_DSA_SHA2_128F_SIG_DIGEST_V1", &self.bytes])
    }
}

/// Unified post-quantum attestation signature accepted by `CommonsLedger`.
///
/// Supports legacy single-use Lamport keys, XMSS-style Merkleized multi-use Lamport trees,
/// and NIST FIPS 205 SLH-DSA-SHA2-128f stateless signatures on Newsatoshi's SHA-256 floor.
#[derive(Clone, Debug)]
pub enum AttestationSignature {
    OneTimeLamport {
        pk: LamportPubKey,
        sig: LamportSig,
    },
    MerkleLamport(MerkleLamportSig),
    SlhDsa {
        pk: SlhDsaPubKey,
        sig: SlhDsaSig,
    },
}

impl AttestationSignature {
    pub fn one_time(pk: LamportPubKey, sig: LamportSig) -> Self {
        Self::OneTimeLamport { pk, sig }
    }

    pub fn merkle(sig: MerkleLamportSig) -> Self {
        Self::MerkleLamport(sig)
    }

    pub fn slh_dsa(pk: SlhDsaPubKey, sig: SlhDsaSig) -> Self {
        Self::SlhDsa { pk, sig }
    }
}
