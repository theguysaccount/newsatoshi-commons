//! Trustless, Non-Custodial `BTC <-> LOVE` Atomic Swap Engine
//!
//! Connects Newsatoshi L1 native SHA-256 hash-locks (`newsatoshi lock --hash`, `lock.rs`)
//! directly to:
//! 1. **Bitcoin On-Chain SegWit v0 P2WSH HTLCs** (BIP-199 + BIP-173 Bech32 `bc1q…` / `tb1q…` / `bcrt1q…`)
//! 2. **Bitcoin Lightning Network BOLT-11 Invoices** (`lnbc…` / `lntb…` / `lnbcrt…` / `lntbs…`)
//!
//! Because both Bitcoin (`OP_SHA256` / BOLT-11) and Newsatoshi L1 (`lock.rs`) verify the exact
//! same `SHA256(32-byte preimage) == payment_hash` invariant, claiming one leg automatically
//! reveals the preimage needed to claim the other — zero custodians, zero multisig bridges,
//! zero wrapped tokens.

use crate::crypto::{hash32_from_hex, sha256, Hash32};
use crate::seed::{L1LockRecord, SeedStatusClient};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Matches `newsatoshi-0.1.0/node/src/lock.rs::MAX_LOCK_BLOCKS`.
pub const NEWSATOSHI_MAX_LOCK_BLOCKS: u32 = 20_160;

/// Minimum recommended Bitcoin locktime blocks for safe on-chain HTLC swaps.
pub const MIN_RECOMMENDED_BTC_BLOCKS: u32 = 12;

/// Minimum recommended Newsatoshi lock blocks.
pub const MIN_RECOMMENDED_LOVE_BLOCKS: u32 = 12;

const BECH32_CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
const BECH32_GEN: [u32; 5] = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3];

// ---------------------------------------------------------------------------
// BIP-173 Bech32 (pure Rust, zero external crypto deps)
// ---------------------------------------------------------------------------

fn bech32_polymod(values: &[u8]) -> u32 {
    let mut chk: u32 = 1;
    for &v in values {
        let top = chk >> 25;
        chk = ((chk & 0x01ff_ffff) << 5) ^ (v as u32);
        for (i, &g) in BECH32_GEN.iter().enumerate() {
            if (top >> i) & 1 != 0 {
                chk ^= g;
            }
        }
    }
    chk
}

fn bech32_hrp_expand(hrp: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(hrp.len() * 2 + 1);
    for b in hrp.bytes() {
        out.push(b >> 5);
    }
    out.push(0);
    for b in hrp.bytes() {
        out.push(b & 31);
    }
    out
}

fn bech32_create_checksum(hrp: &str, data: &[u8]) -> [u8; 6] {
    let mut values = bech32_hrp_expand(hrp);
    values.extend_from_slice(data);
    values.extend_from_slice(&[0u8; 6]);
    let polymod = bech32_polymod(&values) ^ 1;
    let mut out = [0u8; 6];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = ((polymod >> (5 * (5 - i))) & 31) as u8;
    }
    out
}

fn bech32_verify_checksum(hrp: &str, data: &[u8]) -> bool {
    let mut values = bech32_hrp_expand(hrp);
    values.extend_from_slice(data);
    bech32_polymod(&values) == 1
}

/// Encodes an HRP + 5-bit data payload into a lowercase BIP-173 Bech32 string.
pub fn bech32_encode(hrp: &str, data: &[u8]) -> Result<String, String> {
    if hrp.is_empty() || hrp.len() > 83 {
        return Err("bech32 HRP length out of range".to_string());
    }
    let hrp_lower = hrp.to_ascii_lowercase();
    let checksum = bech32_create_checksum(&hrp_lower, data);
    let mut out = String::with_capacity(hrp_lower.len() + 1 + data.len() + 6);
    out.push_str(&hrp_lower);
    out.push('1');
    for &d in data.iter().chain(checksum.iter()) {
        if d >= 32 {
            return Err(format!("invalid 5-bit bech32 value: {d}"));
        }
        out.push(BECH32_CHARSET[d as usize] as char);
    }
    Ok(out)
}

/// Decodes a BIP-173 Bech32 string (supports long BOLT-11 invoices when `max_len` permits).
pub fn bech32_decode(input: &str, max_len: usize) -> Result<(String, Vec<u8>), String> {
    let s = input.trim();
    if s.len() < 8 || s.len() > max_len {
        return Err(format!("bech32 string length {} out of bounds", s.len()));
    }
    let has_lower = s.bytes().any(|b| b.is_ascii_lowercase());
    let has_upper = s.bytes().any(|b| b.is_ascii_uppercase());
    if has_lower && has_upper {
        return Err("mixed-case bech32 string is invalid".to_string());
    }
    let lower = s.to_ascii_lowercase();
    let sep = lower
        .rfind('1')
        .ok_or_else(|| "missing bech32 separator '1'".to_string())?;
    if sep == 0 || sep + 7 > lower.len() {
        return Err("invalid bech32 separator position".to_string());
    }
    let hrp = &lower[..sep];
    for b in hrp.bytes() {
        if !(33..=126).contains(&b) {
            return Err("invalid ASCII character in bech32 HRP".to_string());
        }
    }
    let data_part = &lower[sep + 1..];
    let mut data = Vec::with_capacity(data_part.len());
    for b in data_part.bytes() {
        let idx = BECH32_CHARSET
            .iter()
            .position(|&c| c == b)
            .ok_or_else(|| format!("invalid bech32 character '{}'", b as char))?;
        data.push(idx as u8);
    }
    if !bech32_verify_checksum(hrp, &data) {
        return Err("bech32 checksum verification failed".to_string());
    }
    data.truncate(data.len() - 6);
    Ok((hrp.to_string(), data))
}

/// Converts bit-groups (`from_bits` -> `to_bits`), matching BIP-173 reference logic.
pub fn convert_bits(data: &[u8], from_bits: u32, to_bits: u32, pad: bool) -> Result<Vec<u8>, String> {
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    let mut ret = Vec::new();
    let maxv: u32 = (1 << to_bits) - 1;
    let max_acc: u32 = (1 << (from_bits + to_bits - 1)) - 1;

    for &value in data {
        let v = value as u32;
        if (v >> from_bits) != 0 {
            return Err(format!("invalid input value {value} for {from_bits}-bit group"));
        }
        acc = ((acc << from_bits) | v) & max_acc;
        bits += from_bits;
        while bits >= to_bits {
            bits -= to_bits;
            ret.push(((acc >> bits) & maxv) as u8);
        }
    }
    if pad {
        if bits > 0 {
            ret.push(((acc << (to_bits - bits)) & maxv) as u8);
        }
    } else if bits >= from_bits || ((acc << (to_bits - bits)) & maxv) != 0 {
        return Err("non-zero padding bits in convert_bits".to_string());
    }
    Ok(ret)
}

/// Encodes a 32-byte SegWit v0 P2WSH witness program into a BIP-173 Bech32 address.
pub fn encode_segwit_v0_p2wsh(btc_network: &str, witness_program: &Hash32) -> Result<String, String> {
    let hrp = match btc_network.trim().to_ascii_lowercase().as_str() {
        "mainnet" | "main" | "btc" => "bc",
        "testnet" | "testnet3" | "testnet4" | "signet" => "tb",
        "regtest" => "bcrt",
        other => {
            return Err(format!(
                "unsupported btc_network '{other}' (expected mainnet, testnet, signet, or regtest)"
            ))
        }
    };
    let mut payload = Vec::with_capacity(53);
    payload.push(0u8); // SegWit witness version 0
    payload.extend_from_slice(&convert_bits(witness_program, 8, 5, true)?);
    let addr = bech32_encode(hrp, &payload)?;
    if addr.len() > 90 {
        return Err("SegWit address exceeded 90 chars".to_string());
    }
    Ok(addr)
}

// ---------------------------------------------------------------------------
// Bitcoin Script (BIP-199 P2WSH HTLC)
// ---------------------------------------------------------------------------

/// Encodes a Bitcoin minimal `CScriptNum` push opcode (BIP-62 / Bitcoin Core `CScript::operator<<`).
pub fn encode_script_num_push(n: i64) -> Vec<u8> {
    if n == 0 {
        return vec![0x00]; // OP_0
    }
    if (1..=16).contains(&n) {
        return vec![0x50 + (n as u8)]; // OP_1..OP_16
    }
    if n == -1 {
        return vec![0x4f]; // OP_1NEGATE
    }
    let neg = n < 0;
    let mut abs_v: u64 = if neg { (-n) as u64 } else { n as u64 };
    let mut bytes = Vec::with_capacity(9);
    while abs_v > 0 {
        bytes.push((abs_v & 0xff) as u8);
        abs_v >>= 8;
    }
    if bytes.last().map(|&b| b & 0x80 != 0).unwrap_or(false) {
        bytes.push(if neg { 0x80 } else { 0x00 });
    } else if neg {
        if let Some(last) = bytes.last_mut() {
            *last |= 0x80;
        }
    }
    let mut out = Vec::with_capacity(1 + bytes.len());
    out.push(bytes.len() as u8);
    out.extend_from_slice(&bytes);
    out
}

/// Validates a 33-byte compressed secp256k1 public key (`02...` or `03...`).
pub fn parse_compressed_btc_pubkey(hex_str: &str, label: &str) -> Result<[u8; 33], String> {
    let clean = hex_str.trim().strip_prefix("0x").unwrap_or(hex_str.trim());
    let bytes = hex::decode(clean)
        .map_err(|_| format!("{label} must be valid hex (66 hex chars for compressed 33-byte secp256k1 pubkey)"))?;
    if bytes.len() != 33 {
        return Err(format!(
            "{label} must be exactly 33 bytes (66 hex chars), got {} bytes",
            bytes.len()
        ));
    }
    if bytes[0] != 0x02 && bytes[0] != 0x03 {
        return Err(format!(
            "{label} must be a compressed secp256k1 pubkey starting with 02 or 03"
        ));
    }
    let mut arr = [0u8; 33];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BitcoinHtlcArtifact {
    pub btc_network: String,
    pub btc_p2wsh_address: String,
    pub witness_script_hex: String,
    pub witness_script_asm: String,
    pub witness_program_hex: String,
    pub p2wsh_scriptpubkey_hex: String,
    pub payment_hash_hex: String,
    pub btc_claim_pubkey_hex: String,
    pub btc_refund_pubkey_hex: String,
    pub btc_locktime: u32,
    pub btc_locktime_kind: String,
}

/// Compiles a canonical BIP-199 Bitcoin SegWit v0 P2WSH HTLC script + Bech32 address.
///
/// Script:
/// ```text
/// OP_IF
///   OP_SHA256 <32-byte payment_hash> OP_EQUALVERIFY
///   <33-byte claim_pubkey> OP_CHECKSIG
/// OP_ELSE
///   <btc_locktime> OP_CHECKLOCKTIMEVERIFY OP_DROP
///   <33-byte refund_pubkey> OP_CHECKSIG
/// OP_ENDIF
/// ```
pub fn compile_btc_htlc(
    btc_network: &str,
    payment_hash_hex: &str,
    btc_claim_pubkey_hex: &str,
    btc_refund_pubkey_hex: &str,
    btc_locktime: u32,
) -> Result<BitcoinHtlcArtifact, String> {
    let hash = hash32_from_hex(payment_hash_hex)?;
    let claim_pk = parse_compressed_btc_pubkey(btc_claim_pubkey_hex, "btc_claim_pubkey")?;
    let refund_pk = parse_compressed_btc_pubkey(btc_refund_pubkey_hex, "btc_refund_pubkey")?;
    if btc_locktime == 0 {
        return Err("btc_locktime must be > 0 (block height < 500,000,000 or Unix timestamp >= 500,000,000)".to_string());
    }
    if claim_pk == refund_pk {
        return Err("btc_claim_pubkey and btc_refund_pubkey must be distinct counterparties".to_string());
    }

    let locktime_push = encode_script_num_push(btc_locktime as i64);

    let mut script = Vec::with_capacity(116);
    script.push(0x63); // OP_IF
    script.push(0xa8); // OP_SHA256
    script.push(0x20); // PUSHBYTES_32
    script.extend_from_slice(&hash);
    script.push(0x88); // OP_EQUALVERIFY
    script.push(0x21); // PUSHBYTES_33
    script.extend_from_slice(&claim_pk);
    script.push(0xac); // OP_CHECKSIG
    script.push(0x67); // OP_ELSE
    script.extend_from_slice(&locktime_push);
    script.push(0xb1); // OP_CHECKLOCKTIMEVERIFY
    script.push(0x75); // OP_DROP
    script.push(0x21); // PUSHBYTES_33
    script.extend_from_slice(&refund_pk);
    script.push(0xac); // OP_CHECKSIG
    script.push(0x68); // OP_ENDIF

    let witness_program = sha256(&script);
    let btc_p2wsh_address = encode_segwit_v0_p2wsh(btc_network, &witness_program)?;
    let witness_program_hex = hex::encode(witness_program);
    let p2wsh_scriptpubkey_hex = format!("0020{witness_program_hex}");

    let hash_hex = hex::encode(hash);
    let claim_hex = hex::encode(claim_pk);
    let refund_hex = hex::encode(refund_pk);
    let witness_script_asm = format!(
        "OP_IF OP_SHA256 {hash_hex} OP_EQUALVERIFY {claim_hex} OP_CHECKSIG OP_ELSE {btc_locktime} OP_CHECKLOCKTIMEVERIFY OP_DROP {refund_hex} OP_CHECKSIG OP_ENDIF"
    );

    let locktime_kind = if btc_locktime < 500_000_000 {
        "block_height".to_string()
    } else {
        "unix_timestamp".to_string()
    };

    Ok(BitcoinHtlcArtifact {
        btc_network: btc_network.trim().to_ascii_lowercase(),
        btc_p2wsh_address,
        witness_script_hex: hex::encode(&script),
        witness_script_asm,
        witness_program_hex,
        p2wsh_scriptpubkey_hex,
        payment_hash_hex: hash_hex,
        btc_claim_pubkey_hex: claim_hex,
        btc_refund_pubkey_hex: refund_hex,
        btc_locktime,
        btc_locktime_kind: locktime_kind,
    })
}

// ---------------------------------------------------------------------------
// Lightning BOLT-11 Invoice Decoder
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bolt11Decoded {
    pub network: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_msat: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_sats: Option<u64>,
    pub timestamp_unix: u64,
    pub expiry_secs: u64,
    pub min_final_cltv_expiry: u64,
    pub payment_hash_hex: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

fn parse_u64_be5(words: &[u8]) -> u64 {
    let mut v: u64 = 0;
    for &w in words {
        v = (v << 5) | (w as u64);
    }
    v
}

/// Decodes a Bitcoin Lightning BOLT-11 invoice (`lnbc…`, `lntb…`, `lnbcrt…`, `lntbs…`)
/// and extracts its exact 32-byte SHA-256 `payment_hash` for Newsatoshi L1 hash-lock swaps.
pub fn decode_bolt11_invoice(invoice: &str) -> Result<Bolt11Decoded, String> {
    let raw = invoice.trim();
    let stripped = raw
        .strip_prefix("lightning:")
        .or_else(|| raw.strip_prefix("LIGHTNING:"))
        .unwrap_or(raw);

    let (hrp, data) = bech32_decode(stripped, 2048)?;
    let (network, amount_part) = if let Some(rest) = hrp.strip_prefix("lnbcrt") {
        ("regtest", rest)
    } else if let Some(rest) = hrp.strip_prefix("lntbs") {
        ("signet", rest)
    } else if let Some(rest) = hrp.strip_prefix("lnbc") {
        ("mainnet", rest)
    } else if let Some(rest) = hrp.strip_prefix("lntb") {
        ("testnet", rest)
    } else {
        return Err(format!(
            "unrecognized BOLT-11 prefix in HRP '{hrp}' (expected lnbc, lntb, lntbs, or lnbcrt)"
        ));
    };

    let (amount_msat, amount_sats) = if amount_part.is_empty() {
        (None, None)
    } else {
        let last = amount_part.as_bytes()[amount_part.len() - 1];
        let (digits, mult_num, mult_den): (&str, u128, u128) = match last {
            b'm' => (&amount_part[..amount_part.len() - 1], 100_000_000, 1), // 10^-3 BTC = 10^8 msat
            b'u' => (&amount_part[..amount_part.len() - 1], 100_000, 1),     // 10^-6 BTC = 10^5 msat
            b'n' => (&amount_part[..amount_part.len() - 1], 100, 1),         // 10^-9 BTC = 10^2 msat
            b'p' => (&amount_part[..amount_part.len() - 1], 1, 10),          // 10^-12 BTC = 0.1 msat
            b'0'..=b'9' => (amount_part, 100_000_000_000, 1),                // whole BTC = 10^11 msat
            other => {
                return Err(format!(
                    "invalid BOLT-11 amount multiplier '{}'",
                    other as char
                ))
            }
        };
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(format!("invalid BOLT-11 amount digits '{digits}'"));
        }
        let num: u128 = digits
            .parse()
            .map_err(|_| format!("BOLT-11 amount overflow '{digits}'"))?;
        if mult_den > 1 && (num / mult_den) * mult_den != num {
            return Err("BOLT-11 pico-BTC amount must be a multiple of 10 (sub-msat invalid)".to_string());
        }
        let msat = num
            .checked_mul(mult_num)
            .ok_or_else(|| "BOLT-11 msat overflow".to_string())?
            / mult_den;
        let msat_u64 = u64::try_from(msat).map_err(|_| "BOLT-11 msat exceeds u64".to_string())?;
        (Some(msat_u64), Some(msat_u64 / 1_000))
    };

    // Minimum data: 7 words timestamp + 104 words (65 bytes) signature
    if data.len() < 7 + 104 {
        return Err("BOLT-11 payload too short (missing timestamp or signature)".to_string());
    }
    let timestamp_unix = parse_u64_be5(&data[..7]);
    let tagged_end = data.len() - 104;
    let mut cursor = 7usize;

    let mut payment_hash_hex: Option<String> = None;
    let mut description: Option<String> = None;
    let mut expiry_secs: u64 = 3600;
    let mut min_final_cltv_expiry: u64 = 18;

    while cursor + 3 <= tagged_end {
        let tag = data[cursor];
        let field_len = ((data[cursor + 1] as usize) << 5) | (data[cursor + 2] as usize);
        cursor += 3;
        if cursor + field_len > tagged_end {
            return Err("truncated BOLT-11 tagged field".to_string());
        }
        let field = &data[cursor..cursor + field_len];
        cursor += field_len;

        match tag {
            1 if field_len == 52 => {
                // 'p' — 256-bit SHA-256 payment hash (52 * 5 = 260 bits; 4 trailing zero bits)
                let raw_bytes = convert_bits(field, 5, 8, false)
                    .or_else(|_| {
                        // BOLT-11 allows converting 52 5-bit words to 32 bytes ignoring 4 pad bits
                        let mut acc: u32 = 0;
                        let mut bits: u32 = 0;
                        let mut out = Vec::with_capacity(32);
                        for &w in field {
                            acc = (acc << 5) | (w as u32);
                            bits += 5;
                            while bits >= 8 && out.len() < 32 {
                                bits -= 8;
                                out.push(((acc >> bits) & 0xff) as u8);
                            }
                        }
                        Ok::<Vec<u8>, String>(out)
                    })?;
                if raw_bytes.len() == 32 {
                    payment_hash_hex = Some(hex::encode(raw_bytes));
                }
            }
            13 => {
                // 'd' — UTF-8 description
                let mut acc: u32 = 0;
                let mut bits: u32 = 0;
                let mut out = Vec::new();
                for &w in field {
                    acc = (acc << 5) | (w as u32);
                    bits += 5;
                    while bits >= 8 {
                        bits -= 8;
                        out.push(((acc >> bits) & 0xff) as u8);
                    }
                }
                if let Ok(s) = String::from_utf8(out) {
                    description = Some(s);
                }
            }
            6 => {
                // 'x' — expiry seconds
                expiry_secs = parse_u64_be5(field);
            }
            24 => {
                // 'c' — min_final_cltv_expiry
                min_final_cltv_expiry = parse_u64_be5(field);
            }
            _ => {}
        }
    }

    let payment_hash_hex =
        payment_hash_hex.ok_or_else(|| "BOLT-11 invoice missing required 'p' payment_hash field".to_string())?;

    Ok(Bolt11Decoded {
        network: network.to_string(),
        amount_msat,
        amount_sats,
        timestamp_unix,
        expiry_secs,
        min_final_cltv_expiry,
        payment_hash_hex,
        description,
    })
}

// ---------------------------------------------------------------------------
// Preimage Generation & Verification
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwapSecretPair {
    pub preimage_hex: String,
    pub payment_hash_hex: String,
}

/// Generates a 32-byte cryptographically secure random swap preimage from OS entropy (`/dev/urandom`)
/// and its SHA-256 `payment_hash`.
pub fn generate_swap_secret() -> Result<SwapSecretPair, String> {
    let mut preimage = [0u8; 32];
    let mut f = std::fs::File::open("/dev/urandom")
        .map_err(|e| format!("failed to open OS CSPRNG /dev/urandom: {e}"))?;
    f.read_exact(&mut preimage)
        .map_err(|e| format!("failed reading 32 bytes from /dev/urandom: {e}"))?;
    let hash = sha256(&preimage);
    Ok(SwapSecretPair {
        preimage_hex: hex::encode(preimage),
        payment_hash_hex: hex::encode(hash),
    })
}

/// Verifies `SHA256(preimage) == payment_hash`.
pub fn verify_preimage(payment_hash_hex: &str, preimage_hex: &str) -> Result<bool, String> {
    let expected = hash32_from_hex(payment_hash_hex)?;
    let preimage = hash32_from_hex(preimage_hex)?;
    Ok(sha256(&preimage) == expected)
}

// ---------------------------------------------------------------------------
// Atomic Swap Contract Compilation & Persisted Offer Record
// ---------------------------------------------------------------------------

/// Canonical Solana USDC SPL mint (Mainnet-Beta).
pub const SOLANA_MAINNET_USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolanaHtlcArtifact {
    pub cluster: String,
    pub asset: String,
    pub amount_raw: u64,
    pub spl_mint: Option<String>,
    pub sender_pubkey: String,
    pub recipient_pubkey: String,
    pub program_id: String,
    pub pda_seeds_description: String,
    pub lock_instruction_hex: String,
    pub claim_instruction_template: String,
    pub timelock_unix: i64,
    pub jupiter_route_note: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvmHtlcArtifact {
    pub chain: String,
    pub chain_id: u64,
    pub asset: String,
    pub amount_raw: u64,
    pub token_address: String,
    pub recipient_address: String,
    pub contract_address: String,
    pub timelock_unix: u64,
    pub cast_lock_command: String,
    pub cast_claim_command: String,
    pub cast_refund_command: String,
}

fn validate_solana_base58_pubkey(s: &str, label: &str) -> Result<String, String> {
    let clean = s.trim();
    const BASE58_CHARS: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    if clean.len() < 32 || clean.len() > 44 || !clean.chars().all(|c| BASE58_CHARS.contains(c)) {
        return Err(format!(
            "{label} must be a valid Base58 Solana public key (32–44 chars)"
        ));
    }
    Ok(clean.to_string())
}

fn validate_evm_address(s: &str, label: &str) -> Result<String, String> {
    let clean = s.trim();
    let hex_part = clean
        .strip_prefix("0x")
        .or_else(|| clean.strip_prefix("0X"))
        .ok_or_else(|| format!("{label} must start with '0x' (42 chars)"))?;
    if hex_part.len() != 40 || !hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{label} must be a 20-byte 0x-prefixed hex address"));
    }
    Ok(format!("0x{}", hex_part.to_ascii_lowercase()))
}

fn evm_chain_metadata(chain: &str) -> Result<(u64, &'static str, &'static str), String> {
    match chain.trim().to_ascii_lowercase().as_str() {
        "base" => Ok((8453, "https://mainnet.base.org", "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913")),
        "ethereum" | "eth" => Ok((1, "https://ethereum-rpc.publicnode.com", "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48")),
        "arbitrum" | "arb" => Ok((42161, "https://arb1.arbitrum.io/rpc", "0xaf88d065e77c8cC2239327C5EDb3A432268e5831")),
        "optimism" | "op" => Ok((10, "https://mainnet.optimism.io", "0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85")),
        "polygon" | "matic" => Ok((137, "https://polygon-bor-rpc.publicnode.com", "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359")),
        other => Err(format!(
            "unsupported evm_chain '{other}' (expected base, ethereum, arbitrum, optimism, or polygon)"
        )),
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct CompileSwapRequest {
    /// `"btc_onchain"`, `"lightning_bolt11"`, `"solana_htlc"`, or `"evm_htlc"`.
    #[serde(default = "default_rail")]
    pub rail: String,
    /// `"btc_to_love"` (source asset → LOVE) or `"love_to_btc"` (LOVE → source asset).
    #[serde(default = "default_direction")]
    pub direction: String,
    /// `"mainnet"` or `"testnet"`.
    #[serde(default = "default_love_net")]
    pub love_network: String,
    /// `"mainnet"`, `"testnet"`, `"signet"`, or `"regtest"`.
    #[serde(default = "default_btc_net")]
    pub btc_network: String,
    /// Optional 64-hex SHA-256 payment hash (required for `btc_onchain`, `solana_htlc`, `evm_htlc`).
    #[serde(default)]
    pub payment_hash_hex: Option<String>,
    /// Optional Lightning BOLT-11 invoice (`lnbc…` / `lntb…`).
    #[serde(default)]
    pub bolt11_invoice: Option<String>,
    /// LOVE amount as decimal string (e.g. `"100"`, `"0.5"`) or numeric string.
    pub amount_love: String,
    /// Source asset base units (satoshis for BTC/LN, lamports or 6-dec USDC units for Solana/EVM).
    #[serde(default)]
    pub amount_btc_sats: Option<u64>,
    /// Newsatoshi claim address (`ns1…` or `nsd1…`).
    pub love_claim_address: String,
    /// Newsatoshi lock timeout in blocks from broadcast height (`1..=20160`).
    #[serde(default = "default_love_timeout_blocks")]
    pub love_timeout_blocks: u32,
    /// Required for `btc_onchain`: 33-byte compressed hex pubkey that claims BTC with preimage.
    #[serde(default)]
    pub btc_claim_pubkey_hex: Option<String>,
    /// Required for `btc_onchain`: 33-byte compressed hex pubkey that refunds BTC after `btc_locktime`.
    #[serde(default)]
    pub btc_refund_pubkey_hex: Option<String>,
    /// Required for `btc_onchain`: CLTV block height (or Unix timestamp) for Bitcoin refund path.
    #[serde(default)]
    pub btc_locktime: Option<u32>,
    /// Optional current Bitcoin block height (used only to compute exact remaining Bitcoin blocks for timelock safety check).
    #[serde(default)]
    pub btc_current_height: Option<u32>,
    /// Solana / EVM asset symbol (e.g. `"USDC"`, `"SOL"`, `"ETH"`, `"cbBTC"`).
    #[serde(default)]
    pub source_asset: Option<String>,
    /// Solana cluster (`"mainnet-beta"` or `"devnet"`).
    #[serde(default)]
    pub solana_cluster: Option<String>,
    /// Solana sender pubkey (Base58) for `solana_htlc`.
    #[serde(default)]
    pub solana_sender_pubkey: Option<String>,
    /// Solana recipient/maker pubkey (Base58) for `solana_htlc`.
    #[serde(default)]
    pub solana_recipient_pubkey: Option<String>,
    /// Optional deployed Solana HTLC program ID (Base58).
    #[serde(default)]
    pub solana_program_id: Option<String>,
    /// Optional custom SPL mint (defaults to canonical Solana USDC when `source_asset == "USDC"`).
    #[serde(default)]
    pub solana_spl_mint: Option<String>,
    /// EVM chain (`"base"`, `"ethereum"`, `"arbitrum"`, `"optimism"`, `"polygon"`).
    #[serde(default)]
    pub evm_chain: Option<String>,
    /// EVM recipient `0x…` address for `evm_htlc`.
    #[serde(default)]
    pub evm_recipient_address: Option<String>,
    /// Optional ERC-20 token address (defaults to canonical chain USDC or `0x000…000` for ETH).
    #[serde(default)]
    pub evm_token_address: Option<String>,
    /// Optional deployed `NewsatoshiHtlc.sol` address (`0x…`).
    #[serde(default)]
    pub evm_contract_address: Option<String>,
    /// Unix timestamp (seconds) expiry for `solana_htlc` or `evm_htlc`.
    #[serde(default)]
    pub timelock_unix: Option<u64>,
    /// Optional human note / maker tag.
    #[serde(default)]
    pub maker_label: Option<String>,
}

fn default_rail() -> String {
    "btc_onchain".to_string()
}
fn default_direction() -> String {
    "btc_to_love".to_string()
}
fn default_love_net() -> String {
    "mainnet".to_string()
}
fn default_btc_net() -> String {
    "mainnet".to_string()
}
fn default_love_timeout_blocks() -> u32 {
    144
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtomicSwapContract {
    pub swap_id: String,
    pub protocol: String,
    pub rail: String,
    pub direction: String,
    pub love_network: String,
    pub btc_network: String,
    pub payment_hash_hex: String,
    pub amount_love: String,
    pub amount_btc_sats: u64,
    pub love_claim_address: String,
    pub love_timeout_blocks: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub btc_htlc: Option<BitcoinHtlcArtifact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bolt11: Option<Bolt11Decoded>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solana_htlc: Option<SolanaHtlcArtifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evm_htlc: Option<EvmHtlcArtifact>,
    pub love_lock_command: String,
    pub love_unlock_command: String,
    pub love_expire_command: String,
    pub timelock_safe: bool,
    pub timelock_note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maker_label: Option<String>,
    pub created_at_unix: u64,
    /// Populated only by live on-chain verification (`POST /api/swaps/verify`).
    #[serde(default)]
    pub love_leg_verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub love_lock_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub love_lock_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub love_confirmed_height: Option<u64>,
    #[serde(default)]
    pub btc_leg_verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub btc_funding_txid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub btc_funded_sats: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_preimage_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_verified_unix: Option<u64>,
}

fn validate_love_amount(s: &str) -> Result<String, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err("amount_love is required".to_string());
    }
    let (w, f) = trimmed.split_once('.').unwrap_or((trimmed, ""));
    if f.len() > 8 {
        return Err("amount_love supports at most 8 decimal places".to_string());
    }
    let whole: u64 = w
        .parse()
        .map_err(|_| format!("invalid amount_love '{trimmed}'"))?;
    let frac: u64 = if f.is_empty() {
        0
    } else {
        format!("{f:0<8}")
            .parse()
            .map_err(|_| format!("invalid fractional amount_love '{trimmed}'"))?
    };
    if whole == 0 && frac == 0 {
        return Err("amount_love must be > 0".to_string());
    }
    Ok(trimmed.to_string())
}

fn validate_newsatoshi_address(addr: &str) -> Result<String, String> {
    let a = addr.trim();
    if !(a.starts_with("ns1") || a.starts_with("nsd1")) || a.len() < 20 {
        return Err(
            "love_claim_address must be a valid Newsatoshi address starting with 'ns1' or 'nsd1'"
                .to_string(),
        );
    }
    if !a
        .bytes()
        .all(|b| b.is_ascii_alphanumeric())
    {
        return Err("love_claim_address contains invalid characters".to_string());
    }
    Ok(a.to_string())
}

/// Compiles a complete, verifiable multi-chain (`BTC / Lightning / Solana / Base-EVM <-> LOVE`)
/// non-custodial atomic swap contract.
pub fn compile_atomic_swap(req: &CompileSwapRequest) -> Result<AtomicSwapContract, String> {
    let rail = req.rail.trim().to_ascii_lowercase();
    if !matches!(
        rail.as_str(),
        "btc_onchain" | "lightning_bolt11" | "solana_htlc" | "evm_htlc"
    ) {
        return Err(
            "rail must be 'btc_onchain', 'lightning_bolt11', 'solana_htlc', or 'evm_htlc'"
                .to_string(),
        );
    }
    let direction = req.direction.trim().to_ascii_lowercase();
    if direction != "btc_to_love" && direction != "love_to_btc" {
        return Err("direction must be 'btc_to_love' or 'love_to_btc'".to_string());
    }
    let love_network = req.love_network.trim().to_ascii_lowercase();
    if love_network != "mainnet" && love_network != "testnet" {
        return Err("love_network must be 'mainnet' or 'testnet'".to_string());
    }
    let btc_network = req.btc_network.trim().to_ascii_lowercase();
    let amount_love = validate_love_amount(&req.amount_love)?;
    let love_claim_address = validate_newsatoshi_address(&req.love_claim_address)?;

    if req.love_timeout_blocks == 0 || req.love_timeout_blocks > NEWSATOSHI_MAX_LOCK_BLOCKS {
        return Err(format!(
            "love_timeout_blocks must be between 1 and {NEWSATOSHI_MAX_LOCK_BLOCKS} (newsatoshi L1 MAX_LOCK_BLOCKS)"
        ));
    }

    let love_block_minutes: u64 = if love_network == "mainnet" { 10 } else { 1 };
    let love_window_mins = (req.love_timeout_blocks as u64) * love_block_minutes;

    let mut solana_htlc: Option<SolanaHtlcArtifact> = None;
    let mut evm_htlc: Option<EvmHtlcArtifact> = None;

    let (payment_hash_hex, amount_btc_sats, btc_htlc, bolt11, timelock_safe, timelock_note) =
        if rail == "solana_htlc" {
            let hash_in = req
                .payment_hash_hex
                .as_deref()
                .ok_or_else(|| "payment_hash_hex is required when rail='solana_htlc'".to_string())?;
            let hash_bytes = hash32_from_hex(hash_in)?;
            let hash_hex = hex::encode(hash_bytes);
            let cluster = req
                .solana_cluster
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("mainnet-beta")
                .to_string();
            let asset = req
                .source_asset
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("USDC")
                .to_ascii_uppercase();
            let amount_raw = req
                .amount_btc_sats
                .filter(|&v| v > 0)
                .ok_or_else(|| {
                    "amount_btc_sats (Solana raw base units: lamports or 6-dec USDC) must be > 0"
                        .to_string()
                })?;
            let sender = validate_solana_base58_pubkey(
                req.solana_sender_pubkey
                    .as_deref()
                    .ok_or_else(|| "solana_sender_pubkey (Base58) is required for solana_htlc".to_string())?,
                "solana_sender_pubkey",
            )?;
            let recipient = validate_solana_base58_pubkey(
                req.solana_recipient_pubkey
                    .as_deref()
                    .ok_or_else(|| "solana_recipient_pubkey (Base58) is required for solana_htlc".to_string())?,
                "solana_recipient_pubkey",
            )?;
            if sender == recipient {
                return Err("solana_sender_pubkey and solana_recipient_pubkey must be distinct".to_string());
            }
            let program_id = match req.solana_program_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                Some(p) => validate_solana_base58_pubkey(p, "solana_program_id")?,
                None => "<DEPLOYED_NEWSATOSHI_SOLANA_HTLC_PROGRAM_ID>".to_string(),
            };
            let is_spl = asset != "SOL";
            let spl_mint = if is_spl {
                Some(match req.solana_spl_mint.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    Some(m) => validate_solana_base58_pubkey(m, "solana_spl_mint")?,
                    None => SOLANA_MAINNET_USDC_MINT.to_string(),
                })
            } else {
                None
            };
            let now = unix_now();
            let timelock_unix = req.timelock_unix.unwrap_or(now + 4 * 3600);
            let sol_window_mins = timelock_unix.saturating_sub(now) / 60;

            // Exact 49-byte instruction matching `bridges/solana/src/lib.rs::encode_lock_instruction`
            let mut ix = [0u8; 49];
            ix[0] = if is_spl { 3 } else { 0 };
            ix[1..33].copy_from_slice(&hash_bytes);
            ix[33..41].copy_from_slice(&amount_raw.to_le_bytes());
            ix[41..49].copy_from_slice(&(timelock_unix as i64).to_le_bytes());

            let safe = req.love_timeout_blocks >= MIN_RECOMMENDED_LOVE_BLOCKS
                && timelock_unix > now + 1800
                && (if direction == "btc_to_love" {
                    love_window_mins >= sol_window_mins + 120
                } else {
                    sol_window_mins >= love_window_mins + 120
                });
            let note = if safe {
                format!(
                    "Safe Solana ↔ LOVE ordering: Solana HTLC expires in ~{sol_window_mins} min vs LOVE lock ~{love_window_mins} min (zero-custody SHA-256 PDA escrow)."
                )
            } else {
                format!(
                    "Check Solana ↔ LOVE timelock margin: Solana window ~{sol_window_mins} min vs LOVE window ~{love_window_mins} min (first locker should outlive second locker by ≥ 120 min)."
                )
            };

            solana_htlc = Some(SolanaHtlcArtifact {
                cluster,
                asset: asset.clone(),
                amount_raw,
                spl_mint,
                sender_pubkey: sender.clone(),
                recipient_pubkey: recipient,
                program_id,
                pda_seeds_description: format!(
                    "PDA seeds = [b\"ns_htlc\", bytes.fromhex(\"{hash_hex}\"), base58_decode(\"{sender}\")]"
                ),
                lock_instruction_hex: hex::encode(ix),
                claim_instruction_template: "0x01 || <32-byte preimage> (33 bytes total)".to_string(),
                timelock_unix: timelock_unix as i64,
                jupiter_route_note: "Any Solana SPL token can swap into USDC via Jupiter before locking into this SHA-256 HTLC PDA in one Phantom session.".to_string(),
            });

            (hash_hex, amount_raw, None, None, safe, note)
        } else if rail == "evm_htlc" {
            let hash_in = req
                .payment_hash_hex
                .as_deref()
                .ok_or_else(|| "payment_hash_hex is required when rail='evm_htlc'".to_string())?;
            let hash_hex = hex::encode(hash32_from_hex(hash_in)?);
            let chain = req
                .evm_chain
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("base")
                .to_ascii_lowercase();
            let (chain_id, rpc_url, default_usdc) = evm_chain_metadata(&chain)?;
            let asset = req
                .source_asset
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("USDC")
                .to_ascii_uppercase();
            let amount_raw = req
                .amount_btc_sats
                .filter(|&v| v > 0)
                .ok_or_else(|| {
                    "amount_btc_sats (EVM raw base units: wei or 6-dec USDC) must be > 0".to_string()
                })?;
            let recipient = validate_evm_address(
                req.evm_recipient_address
                    .as_deref()
                    .ok_or_else(|| "evm_recipient_address (0x...) is required for evm_htlc".to_string())?,
                "evm_recipient_address",
            )?;
            let contract = match req.evm_contract_address.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                Some(c) => validate_evm_address(c, "evm_contract_address")?,
                None => "<DEPLOYED_NEWSATOSHI_HTLC_SOL_ADDRESS>".to_string(),
            };
            let is_native_eth = asset == "ETH";
            let token_addr = if is_native_eth {
                "0x0000000000000000000000000000000000000000".to_string()
            } else {
                match req.evm_token_address.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    Some(t) => validate_evm_address(t, "evm_token_address")?,
                    None => default_usdc.to_string(),
                }
            };
            let now = unix_now();
            let timelock_unix = req.timelock_unix.unwrap_or(now + 4 * 3600);
            let evm_window_mins = timelock_unix.saturating_sub(now) / 60;

            let cast_lock_command = if is_native_eth {
                format!(
                    "cast send {contract} \"lockEth(address,bytes32,uint64)\" {recipient} 0x{hash_hex} {timelock_unix} --value {amount_raw} --rpc-url {rpc_url}"
                )
            } else {
                format!(
                    "cast send {token_addr} \"approve(address,uint256)\" {contract} {amount_raw} --rpc-url {rpc_url} && cast send {contract} \"lockErc20(address,address,uint256,bytes32,uint64)\" {recipient} {token_addr} {amount_raw} 0x{hash_hex} {timelock_unix} --rpc-url {rpc_url}"
                )
            };
            let cast_claim_command = format!(
                "cast send {contract} \"claim(bytes32,bytes32)\" <SWAP_ID_BYTES32> 0x<PREIMAGE_64HEX> --rpc-url {rpc_url}"
            );
            let cast_refund_command = format!(
                "cast send {contract} \"refund(bytes32)\" <SWAP_ID_BYTES32> --rpc-url {rpc_url}"
            );

            let safe = req.love_timeout_blocks >= MIN_RECOMMENDED_LOVE_BLOCKS
                && timelock_unix > now + 1800
                && (if direction == "btc_to_love" {
                    love_window_mins >= evm_window_mins + 120
                } else {
                    evm_window_mins >= love_window_mins + 120
                });
            let note = if safe {
                format!(
                    "Safe {chain} ({asset}) ↔ LOVE ordering: EVM HTLC expires in ~{evm_window_mins} min vs LOVE lock ~{love_window_mins} min (EVM 0x02 sha256 precompile)."
                )
            } else {
                format!(
                    "Check {chain} ↔ LOVE timelock margin: EVM window ~{evm_window_mins} min vs LOVE window ~{love_window_mins} min (first locker should outlive second locker by ≥ 120 min)."
                )
            };

            evm_htlc = Some(EvmHtlcArtifact {
                chain,
                chain_id,
                asset,
                amount_raw,
                token_address: token_addr,
                recipient_address: recipient,
                contract_address: contract,
                timelock_unix,
                cast_lock_command,
                cast_claim_command,
                cast_refund_command,
            });

            (hash_hex, amount_raw, None, None, safe, note)
        } else if rail == "lightning_bolt11" {
            let invoice = req
                .bolt11_invoice
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "bolt11_invoice is required when rail='lightning_bolt11'".to_string())?;
            let decoded = decode_bolt11_invoice(invoice)?;
            let sats = decoded
                .amount_sats
                .or(req.amount_btc_sats)
                .ok_or_else(|| "amount_btc_sats required for amountless BOLT-11 invoice".to_string())?;
            if sats == 0 {
                return Err("amount_btc_sats must be > 0".to_string());
            }
            let ln_window_mins = decoded.min_final_cltv_expiry * 10;
            let safe = req.love_timeout_blocks >= MIN_RECOMMENDED_LOVE_BLOCKS
                && love_window_mins >= ln_window_mins + 60;
            let note = if safe {
                format!(
                    "Safe Lightning margin: LOVE lock window ({} blocks ≈ {} min) exceeds BOLT-11 min_final_cltv_expiry ({} BTC blocks ≈ {} min) by {} min.",
                    req.love_timeout_blocks,
                    love_window_mins,
                    decoded.min_final_cltv_expiry,
                    ln_window_mins,
                    love_window_mins.saturating_sub(ln_window_mins)
                )
            } else {
                format!(
                    "Warning: LOVE lock window ({} blocks ≈ {} min) should exceed BOLT-11 CLTV window ({} BTC blocks ≈ {} min) by at least 60 min.",
                    req.love_timeout_blocks,
                    love_window_mins,
                    decoded.min_final_cltv_expiry,
                    ln_window_mins
                )
            };
            (
                decoded.payment_hash_hex.clone(),
                sats,
                None,
                Some(decoded),
                safe,
                note,
            )
        } else {
            let hash_in = req
                .payment_hash_hex
                .as_deref()
                .ok_or_else(|| "payment_hash_hex is required when rail='btc_onchain'".to_string())?;
            let claim_pk = req
                .btc_claim_pubkey_hex
                .as_deref()
                .ok_or_else(|| "btc_claim_pubkey_hex is required when rail='btc_onchain'".to_string())?;
            let refund_pk = req
                .btc_refund_pubkey_hex
                .as_deref()
                .ok_or_else(|| "btc_refund_pubkey_hex is required when rail='btc_onchain'".to_string())?;
            let btc_locktime = req
                .btc_locktime
                .ok_or_else(|| "btc_locktime (Bitcoin CLTV height or timestamp) is required when rail='btc_onchain'".to_string())?;
            let sats = req
                .amount_btc_sats
                .filter(|&v| v > 0)
                .ok_or_else(|| "amount_btc_sats must be > 0".to_string())?;

            let htlc = compile_btc_htlc(&btc_network, hash_in, claim_pk, refund_pk, btc_locktime)?;

            // Evaluate timelock safety margin honestly without guessing unknown Bitcoin block height.
            let (safe, note) = if let Some(cur_h) = req.btc_current_height {
                if btc_locktime <= cur_h {
                    (
                        false,
                        format!("Unsafe: btc_locktime ({btc_locktime}) is not ahead of btc_current_height ({cur_h})."),
                    )
                } else {
                    let btc_delta_blocks = btc_locktime - cur_h;
                    let btc_window_mins = (btc_delta_blocks as u64) * 10;
                    if direction == "btc_to_love" {
                        // Initiator locks LOVE first (must have LONGER timeout than taker's BTC HTLC).
                        let ok = btc_delta_blocks >= MIN_RECOMMENDED_BTC_BLOCKS
                            && love_window_mins >= btc_window_mins + 120;
                        let msg = if ok {
                            format!(
                                "Safe BTC→LOVE ordering: LOVE lock ({} blocks ≈ {} min) outlives BTC CLTV ({} blocks ≈ {} min) by {} min so taker cannot be raced.",
                                req.love_timeout_blocks, love_window_mins, btc_delta_blocks, btc_window_mins, love_window_mins - btc_window_mins
                            )
                        } else {
                            format!(
                                "Warning: For BTC→LOVE, LOVE lock window ({} min) should exceed BTC CLTV window ({} min) by ≥ 120 min (and BTC CLTV ≥ {MIN_RECOMMENDED_BTC_BLOCKS} blocks).",
                                love_window_mins, btc_window_mins
                            )
                        };
                        (ok, msg)
                    } else {
                        // Initiator locks BTC first (must have LONGER timeout than taker's LOVE lock).
                        let ok = req.love_timeout_blocks >= MIN_RECOMMENDED_LOVE_BLOCKS
                            && btc_window_mins >= love_window_mins + 120;
                        let msg = if ok {
                            format!(
                                "Safe LOVE→BTC ordering: BTC CLTV ({} blocks ≈ {} min) outlives LOVE lock ({} blocks ≈ {} min) by {} min so taker cannot be raced.",
                                btc_delta_blocks, btc_window_mins, req.love_timeout_blocks, love_window_mins, btc_window_mins - love_window_mins
                            )
                        } else {
                            format!(
                                "Warning: For LOVE→BTC, BTC CLTV window ({} min) should exceed LOVE lock window ({} min) by ≥ 120 min (and LOVE lock ≥ {MIN_RECOMMENDED_LOVE_BLOCKS} blocks).",
                                btc_window_mins, love_window_mins
                            )
                        };
                        (ok, msg)
                    }
                }
            } else {
                let ok = req.love_timeout_blocks >= MIN_RECOMMENDED_LOVE_BLOCKS && btc_locktime >= MIN_RECOMMENDED_BTC_BLOCKS;
                let msg = format!(
                    "HTLC compiled with BTC CLTV={btc_locktime} and LOVE timeout={} blocks (≈ {love_window_mins} min). Provide btc_current_height to verify exact relative anti-race margin (first-locker window must exceed second-locker window by ≥ 2h).",
                    req.love_timeout_blocks
                );
                (ok, msg)
            };

            (htlc.payment_hash_hex.clone(), sats, Some(htlc), None, safe, note)
        };

    let love_lock_command = format!(
        "newsatoshi --net {love_network} lock {amount_love} --hash {payment_hash_hex} --to {love_claim_address} --blocks {}",
        req.love_timeout_blocks
    );
    let love_unlock_command = format!(
        "newsatoshi --net {love_network} unlock <LOCK_ID> <PREIMAGE_HEX>"
    );
    let love_expire_command = format!(
        "newsatoshi --net {love_network} expire <LOCK_ID>"
    );

    let swap_id = format!(
        "swap-{}",
        &hex::encode(sha256(
            format!(
                "{rail}:{direction}:{love_network}:{btc_network}:{payment_hash_hex}:{amount_love}:{amount_btc_sats}:{love_claim_address}"
            )
            .as_bytes()
        ))[..16]
    );

    Ok(AtomicSwapContract {
        swap_id,
        protocol: "NEWSATOSHI-HTLC-SWAP/1.0".to_string(),
        rail,
        direction,
        love_network,
        btc_network,
        payment_hash_hex,
        amount_love,
        amount_btc_sats,
        love_claim_address,
        love_timeout_blocks: req.love_timeout_blocks,
        btc_htlc,
        bolt11,
        solana_htlc,
        evm_htlc,
        love_lock_command,
        love_unlock_command,
        love_expire_command,
        timelock_safe,
        timelock_note,
        maker_label: req
            .maker_label
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned),
        created_at_unix: unix_now(),
        love_leg_verified: false,
        love_lock_id: None,
        love_lock_state: None,
        love_confirmed_height: None,
        btc_leg_verified: false,
        btc_funding_txid: None,
        btc_funded_sats: None,
        verified_preimage_hex: None,
        last_verified_unix: None,
    })
}

// ---------------------------------------------------------------------------
// Live Dual-Chain On-Chain Verification (Newsatoshi L1 + mempool.space BTC)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BtcUtxoStatus {
    #[serde(default)]
    pub confirmed: bool,
    #[serde(default)]
    pub block_height: Option<u64>,
    #[serde(default)]
    pub block_hash: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BtcUtxoRecord {
    pub txid: String,
    pub vout: u32,
    pub value: u64,
    pub status: BtcUtxoStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DualChainSwapVerification {
    pub ok: bool,
    pub payment_hash_hex: String,
    pub love_network: String,
    pub love_endpoint: String,
    pub love_found: bool,
    pub love_confirmed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub love_lock: Option<L1LockRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub love_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub btc_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub btc_endpoint: Option<String>,
    pub btc_found: bool,
    pub btc_confirmed: bool,
    pub btc_total_sats: u64,
    pub btc_utxos: Vec<BtcUtxoRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub btc_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preimage_valid: Option<bool>,
    pub verified_at_unix: u64,
}

/// Fetches real unspent outputs at a Bitcoin P2WSH address from public `mempool.space` API.
pub fn fetch_btc_utxos_mempool(btc_address: &str) -> Result<(String, Vec<BtcUtxoRecord>), String> {
    let addr = btc_address.trim();
    let url = if addr.starts_with("bc1") {
        format!("https://mempool.space/api/address/{addr}/utxo")
    } else if addr.starts_with("tb1") {
        format!("https://mempool.space/testnet/api/address/{addr}/utxo")
    } else if addr.starts_with("bcrt1") {
        return Err("regtest addresses (bcrt1…) cannot be queried on public mempool.space".to_string());
    } else {
        return Err("btc_address must be a valid Bech32 SegWit address (bc1… or tb1…)".to_string());
    };

    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(6)))
        .build();
    let agent: ureq::Agent = config.into();
    let mut resp = agent
        .get(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "newsatoshi-commons/0.5.0")
        .call()
        .map_err(|e| format!("mempool.space fetch failed: {e}"))?;

    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("failed reading mempool.space body: {e}"))?;

    let utxos = serde_json::from_str::<Vec<BtcUtxoRecord>>(&body)
        .map_err(|e| format!("failed parsing mempool.space UTXO JSON: {e}"))?;
    Ok((url, utxos))
}

/// Performs live dual-chain verification against Newsatoshi L1 (`GET /api/lock?hash=`)
/// and optionally Bitcoin (`mempool.space/api/address/<addr>/utxo`) plus preimage check.
pub fn verify_swap_on_chain(
    love_client: &SeedStatusClient,
    love_network: &str,
    payment_hash_hex: &str,
    btc_address: Option<&str>,
    preimage_hex: Option<&str>,
) -> Result<DualChainSwapVerification, String> {
    let normalized_hash = hex::encode(hash32_from_hex(payment_hash_hex)?);
    let love_endpoint = love_client.lock_by_hash_url(&normalized_hash);

    let (love_found, love_confirmed, love_lock, love_error) =
        match love_client.fetch_locks_for_hash(&normalized_hash) {
            Ok(locks) => {
                let matched = locks
                    .iter()
                    .find(|l| l.found && l.confirmed)
                    .or_else(|| locks.iter().find(|l| l.found))
                    .cloned();
                match matched {
                    Some(m) => {
                        let conf = m.confirmed;
                        (true, conf, Some(m), None)
                    }
                    None => (false, false, None, None),
                }
            }
            Err(e) => (false, false, None, Some(e)),
        };

    let mut btc_endpoint = None;
    let mut btc_found = false;
    let mut btc_confirmed = false;
    let mut btc_total_sats = 0u64;
    let mut btc_utxos = Vec::new();
    let mut btc_error = None;

    let clean_btc_addr = btc_address
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);

    if let Some(ref addr) = clean_btc_addr {
        match fetch_btc_utxos_mempool(addr) {
            Ok((url, utxos)) => {
                btc_endpoint = Some(url);
                btc_found = !utxos.is_empty();
                btc_confirmed = utxos.iter().any(|u| u.status.confirmed);
                btc_total_sats = utxos.iter().map(|u| u.value).sum();
                btc_utxos = utxos;
            }
            Err(e) => {
                btc_error = Some(e);
            }
        }
    }

    let preimage_valid = match preimage_hex.map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => Some(verify_preimage(&normalized_hash, p)?),
        None => None,
    };

    Ok(DualChainSwapVerification {
        ok: true,
        payment_hash_hex: normalized_hash,
        love_network: love_network.to_string(),
        love_endpoint,
        love_found,
        love_confirmed,
        love_lock,
        love_error,
        btc_address: clean_btc_addr,
        btc_endpoint,
        btc_found,
        btc_confirmed,
        btc_total_sats,
        btc_utxos,
        btc_error,
        preimage_valid,
        verified_at_unix: unix_now(),
    })
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bip173_segwit_v0_p2wsh_official_vectors() {
        let prog = hash32_from_hex(
            "1863143c14c5166804bd19203356da136c985678cd4d27a1b8c6329604903262",
        )
        .unwrap();
        assert_eq!(
            encode_segwit_v0_p2wsh("mainnet", &prog).unwrap(),
            "bc1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qccfmv3"
        );
        assert_eq!(
            encode_segwit_v0_p2wsh("testnet", &prog).unwrap(),
            "tb1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3q0sl5k7"
        );
    }

    #[test]
    fn bolt11_official_test_vector_decodes_exact_payment_hash() {
        // Official Lightning BOLT #11 Appendix A test vector:
        // 2500 uBTC = 250,000 sats, description "1 cup coffee",
        // payment_hash = 0001020304050607080900010203040506070809000102030405060708090102
        let inv = "lnbc2500u1pvjluezpp5qqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqypqdq5xysxxatsyp3k7enxv4jsxqzpuaztrnwngzn3kdzw5hydlzf03qdgm2hdq27cqv3agm2awhz5se903vruatfhq77w3ls4evs3ch9zw97j25emudupq63nyw24cg27h2rspfj9srp";
        let dec = decode_bolt11_invoice(inv).unwrap();
        assert_eq!(dec.network, "mainnet");
        assert_eq!(dec.amount_sats, Some(250_000));
        assert_eq!(dec.amount_msat, Some(250_000_000));
        assert_eq!(dec.timestamp_unix, 1496314658);
        assert_eq!(dec.description.as_deref(), Some("1 cup coffee"));
        assert_eq!(
            dec.payment_hash_hex,
            "0001020304050607080900010203040506070809000102030405060708090102"
        );
    }

    #[test]
    fn script_num_minimal_encoding_matches_bitcoin_core() {
        assert_eq!(encode_script_num_push(0), vec![0x00]);
        assert_eq!(encode_script_num_push(1), vec![0x51]);
        assert_eq!(encode_script_num_push(16), vec![0x60]);
        assert_eq!(encode_script_num_push(127), vec![0x01, 0x7f]);
        assert_eq!(encode_script_num_push(128), vec![0x02, 0x80, 0x00]);
        assert_eq!(encode_script_num_push(850_000), vec![0x03, 0x50, 0xf8, 0x0c]);
    }

    #[test]
    fn compile_btc_htlc_produces_valid_p2wsh_and_rejects_bad_keys() {
        let secret = generate_swap_secret().unwrap();
        assert!(verify_preimage(&secret.payment_hash_hex, &secret.preimage_hex).unwrap());

        let claim_pk = "02".to_string() + &"11".repeat(32);
        let refund_pk = "03".to_string() + &"22".repeat(32);

        let htlc = compile_btc_htlc("mainnet", &secret.payment_hash_hex, &claim_pk, &refund_pk, 900_000).unwrap();
        assert!(htlc.btc_p2wsh_address.starts_with("bc1q"));
        assert_eq!(htlc.btc_p2wsh_address.len(), 62);
        assert!(htlc.p2wsh_scriptpubkey_hex.starts_with("0020"));
        assert_eq!(htlc.p2wsh_scriptpubkey_hex.len(), 68);
        assert!(htlc.witness_script_asm.contains("OP_SHA256"));
        assert!(htlc.witness_script_asm.contains("OP_CHECKLOCKTIMEVERIFY"));

        // Identical pubkeys rejected
        assert!(compile_btc_htlc("mainnet", &secret.payment_hash_hex, &claim_pk, &claim_pk, 900_000).is_err());
        // Uncompressed / invalid prefix rejected
        let bad_pk = "04".to_string() + &"11".repeat(32);
        assert!(compile_btc_htlc("mainnet", &secret.payment_hash_hex, &bad_pk, &refund_pk, 900_000).is_err());
    }

    #[test]
    fn compile_atomic_swap_enforces_anti_race_timelock_ordering() {
        let secret = generate_swap_secret().unwrap();
        let claim_pk = "02".to_string() + &"aa".repeat(32);
        let refund_pk = "03".to_string() + &"bb".repeat(32);

        // Safe BTC->LOVE: LOVE lock = 144 blocks (1440 min), BTC CLTV = +36 blocks (360 min)
        let req_safe = CompileSwapRequest {
            rail: "btc_onchain".into(),
            direction: "btc_to_love".into(),
            love_network: "mainnet".into(),
            btc_network: "mainnet".into(),
            payment_hash_hex: Some(secret.payment_hash_hex.clone()),
            bolt11_invoice: None,
            amount_love: "250".into(),
            amount_btc_sats: Some(500_000),
            love_claim_address: "ns1bc03485c35c661a5f6685a52f19351a143b44994df2943c20285b2e9876f68fa10452da1".into(),
            love_timeout_blocks: 144,
            btc_claim_pubkey_hex: Some(claim_pk.clone()),
            btc_refund_pubkey_hex: Some(refund_pk.clone()),
            btc_locktime: Some(900_036),
            btc_current_height: Some(900_000),
            maker_label: Some("mainnet-maker-1".into()),
            ..Default::default()
        };
        let contract = compile_atomic_swap(&req_safe).unwrap();
        assert!(contract.timelock_safe);
        assert!(contract.love_lock_command.contains("--net mainnet lock 250"));

        // Unsafe BTC->LOVE: BTC lock (144 blocks) outlives LOVE lock (24 blocks) -> flagged unsafe!
        let mut req_unsafe = req_safe.clone();
        req_unsafe.love_timeout_blocks = 24;
        req_unsafe.btc_locktime = Some(900_144);
        let unsafe_contract = compile_atomic_swap(&req_unsafe).unwrap();
        assert!(!unsafe_contract.timelock_safe);
    }

    #[test]
    fn compile_solana_and_base_evm_htlc_buy_roads() {
        let secret = generate_swap_secret().unwrap();
        let now = unix_now();

        // 1. Solana USDC -> LOVE HTLC
        let sol_req = CompileSwapRequest {
            rail: "solana_htlc".into(),
            direction: "btc_to_love".into(),
            love_network: "mainnet".into(),
            payment_hash_hex: Some(secret.payment_hash_hex.clone()),
            amount_love: "100".into(),
            amount_btc_sats: Some(50_000_000), // 50 USDC (6 decimals)
            love_claim_address: "ns1bc03485c35c661a5f6685a52f19351a143b44994df2943c20285b2e9876f68fa10452da1".into(),
            love_timeout_blocks: 144,
            source_asset: Some("USDC".into()),
            solana_cluster: Some("mainnet-beta".into()),
            solana_sender_pubkey: Some("7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU".into()),
            solana_recipient_pubkey: Some("9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin".into()),
            timelock_unix: Some(now + 4 * 3600),
            ..Default::default()
        };
        let sol_c = compile_atomic_swap(&sol_req).unwrap();
        assert!(sol_c.timelock_safe);
        let sol = sol_c.solana_htlc.unwrap();
        assert_eq!(sol.asset, "USDC");
        assert_eq!(sol.spl_mint.as_deref(), Some(SOLANA_MAINNET_USDC_MINT));
        assert_eq!(sol.lock_instruction_hex.len(), 98); // 49 bytes = 98 hex chars
        assert!(sol.lock_instruction_hex.starts_with("03")); // LockSpl discriminant = 3

        // 2. Base Mainnet USDC -> LOVE HTLC
        let evm_req = CompileSwapRequest {
            rail: "evm_htlc".into(),
            direction: "btc_to_love".into(),
            love_network: "mainnet".into(),
            payment_hash_hex: Some(secret.payment_hash_hex.clone()),
            amount_love: "100".into(),
            amount_btc_sats: Some(50_000_000), // 50 Base USDC
            love_claim_address: "ns1bc03485c35c661a5f6685a52f19351a143b44994df2943c20285b2e9876f68fa10452da1".into(),
            love_timeout_blocks: 144,
            source_asset: Some("USDC".into()),
            evm_chain: Some("base".into()),
            evm_recipient_address: Some("0x1111111111111111111111111111111111111111".into()),
            timelock_unix: Some(now + 4 * 3600),
            ..Default::default()
        };
        let evm_c = compile_atomic_swap(&evm_req).unwrap();
        assert!(evm_c.timelock_safe);
        let evm = evm_c.evm_htlc.unwrap();
        assert_eq!(evm.chain, "base");
        assert_eq!(evm.chain_id, 8453);
        assert_eq!(evm.token_address, "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913");
        assert!(evm.cast_lock_command.contains("lockErc20"));
    }
}
