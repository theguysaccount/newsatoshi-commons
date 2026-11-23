//! Newsatoshi Solana SHA-256 HTLC Escrow Program (SOL & SPL-USDC Atomic Swaps)
//!
//! Zero-custody, zero-admin, deterministic PDA escrow enabling Phantom / Backpack / Solflare
//! users (and Jupiter swap routes) to bridge SOL or USDC directly into native Newsatoshi (`LOVE`)
//! via the identical `SHA256(32-byte preimage) == payment_hash` invariant verified by Newsatoshi L1.
//!
//! PDA Seeds:
//!   - Escrow State PDA: `["ns_htlc", payment_hash(32), sender_pubkey(32)]`
//!   - Vault Authority : same PDA signs SPL token transfers or lamport releases on Claim/Refund.
//!
//! Instruction Tag (1-byte Borsh/raw discriminant):
//!   - `0`: LockSol   { payment_hash: [u8; 32], amount_lamports: u64, timelock_unix: i64 }
//!   - `1`: Claim     { preimage: [u8; 32] }   -> verifies `sha256(preimage) == payment_hash`
//!   - `2`: Refund    {}                       -> verifies `clock.unix_timestamp >= timelock_unix`
//!   - `3`: LockSpl   { payment_hash: [u8; 32], amount_raw: u64, timelock_unix: i64 } (USDC/SPL vault)

use sha2::{Digest, Sha256};

/// Canonical PDA seed prefix shared across Solana program, Rust swap compiler, and Maker Watcher.
pub const HTLC_PDA_SEED_PREFIX: &[u8] = b"ns_htlc";

/// Fixed binary layout size of an on-chain `SolanaHtlcEscrow` account (147 bytes).
pub const ESCROW_ACCOUNT_SIZE: usize = 1 + 32 + 32 + 32 + 32 + 8 + 8 + 1 + 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EscrowStatus {
    Open = 0,
    Claimed = 1,
    Refunded = 2,
}

/// On-chain account state stored in the deterministic PDA `["ns_htlc", payment_hash, sender]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SolanaHtlcEscrow {
    pub bump: u8,
    pub sender: [u8; 32],
    pub recipient: [u8; 32],
    /// `[0; 32]` for native SOL; SPL mint pubkey (e.g. Solana USDC `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`) for SPL swaps.
    pub mint: [u8; 32],
    pub payment_hash: [u8; 32],
    pub amount: u64,
    pub timelock_unix: i64,
    pub status: u8,
    pub is_spl: u8,
}

impl SolanaHtlcEscrow {
    pub fn to_bytes(&self) -> [u8; ESCROW_ACCOUNT_SIZE] {
        let mut out = [0u8; ESCROW_ACCOUNT_SIZE];
        out[0] = self.bump;
        out[1..33].copy_from_slice(&self.sender);
        out[33..65].copy_from_slice(&self.recipient);
        out[65..97].copy_from_slice(&self.mint);
        out[97..129].copy_from_slice(&self.payment_hash);
        out[129..137].copy_from_slice(&self.amount.to_le_bytes());
        out[137..145].copy_from_slice(&self.timelock_unix.to_le_bytes());
        out[145] = self.status;
        out[146] = self.is_spl;
        out
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self, &'static str> {
        if b.len() < ESCROW_ACCOUNT_SIZE {
            return Err("escrow account data truncated");
        }
        let status = b[145];
        if status > EscrowStatus::Refunded as u8 {
            return Err("invalid escrow status discriminant");
        }
        let is_spl = b[146];
        if is_spl > 1 {
            return Err("invalid is_spl boolean flag");
        }
        Ok(Self {
            bump: b[0],
            sender: b[1..33].try_into().unwrap(),
            recipient: b[33..65].try_into().unwrap(),
            mint: b[65..97].try_into().unwrap(),
            payment_hash: b[97..129].try_into().unwrap(),
            amount: u64::from_le_bytes(b[129..137].try_into().unwrap()),
            timelock_unix: i64::from_le_bytes(b[137..145].try_into().unwrap()),
            status,
            is_spl,
        })
    }

    /// Pure deterministic SHA-256 preimage verification matching Solana `sol_sha256` & Newsatoshi L1 `lock.rs`.
    pub fn verify_claim(&self, preimage: &[u8; 32], now_unix: i64) -> Result<(), &'static str> {
        if self.status != EscrowStatus::Open as u8 {
            return Err("escrow already settled");
        }
        if self.amount == 0 {
            return Err("escrow amount must be > 0");
        }
        if self.payment_hash == [0u8; 32] {
            return Err("escrow payment_hash cannot be all-zero");
        }
        if now_unix >= self.timelock_unix {
            return Err("escrow timelock expired");
        }
        let digest: [u8; 32] = Sha256::digest(preimage).into();
        let mut diff = 0u8;
        for i in 0..32 {
            diff |= digest[i] ^ self.payment_hash[i];
        }
        if diff != 0 {
            return Err("invalid SHA-256 preimage");
        }
        Ok(())
    }

    /// Pure deterministic refund check strictly after `timelock_unix`.
    pub fn verify_refund(&self, now_unix: i64) -> Result<(), &'static str> {
        if self.status != EscrowStatus::Open as u8 {
            return Err("escrow already settled");
        }
        if self.amount == 0 {
            return Err("escrow amount must be > 0");
        }
        if now_unix < self.timelock_unix {
            return Err("escrow timelock still active");
        }
        Ok(())
    }
}

/// Encodes instruction data for `LockSol` (tag `0`) or `LockSpl` (tag `3`).
pub fn encode_lock_instruction(
    is_spl: bool,
    payment_hash: [u8; 32],
    amount: u64,
    timelock_unix: i64,
) -> [u8; 49] {
    let mut out = [0u8; 49];
    out[0] = if is_spl { 3 } else { 0 };
    out[1..33].copy_from_slice(&payment_hash);
    out[33..41].copy_from_slice(&amount.to_le_bytes());
    out[41..49].copy_from_slice(&timelock_unix.to_le_bytes());
    out
}

/// Encodes instruction data for `Claim` (tag `1`, 33 bytes).
pub fn encode_claim_instruction(preimage: [u8; 32]) -> [u8; 33] {
    let mut out = [0u8; 33];
    out[0] = 1;
    out[1..33].copy_from_slice(&preimage);
    out
}

/// Encodes instruction data for `Refund` (tag `2`, 1 byte).
pub fn encode_refund_instruction() -> [u8; 1] {
    [2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solana_htlc_escrow_roundtrip_claim_and_refund_invariants() {
        let preimage = [0x42u8; 32];
        let hash: [u8; 32] = Sha256::digest(preimage).into();
        let escrow = SolanaHtlcEscrow {
            bump: 254,
            sender: [1u8; 32],
            recipient: [2u8; 32],
            mint: [3u8; 32],
            payment_hash: hash,
            amount: 50_000_000, // 50 USDC (6 decimals)
            timelock_unix: 1_800_000_000,
            status: EscrowStatus::Open as u8,
            is_spl: 1,
        };

        let raw = escrow.to_bytes();
        let decoded = SolanaHtlcEscrow::from_bytes(&raw).unwrap();
        assert_eq!(escrow, decoded);

        // Valid claim before expiry succeeds
        assert!(decoded.verify_claim(&preimage, 1_799_999_000).is_ok());
        // Wrong preimage rejected
        assert!(decoded.verify_claim(&[0x99u8; 32], 1_799_999_000).is_err());
        // Claim at/after expiry rejected
        assert!(decoded.verify_claim(&preimage, 1_800_000_000).is_err());
        // Refund before expiry rejected; refund at/after expiry succeeds
        assert!(decoded.verify_refund(1_799_999_999).is_err());
        assert!(decoded.verify_refund(1_800_000_000).is_ok());

        // Security audit guards: reject invalid discriminants, zero amount, zero payment_hash
        let mut bad_status = raw;
        bad_status[145] = 9;
        assert!(SolanaHtlcEscrow::from_bytes(&bad_status).is_err());

        let mut bad_flag = raw;
        bad_flag[146] = 2;
        assert!(SolanaHtlcEscrow::from_bytes(&bad_flag).is_err());

        let mut zero_amt = escrow.clone();
        zero_amt.amount = 0;
        assert!(zero_amt.verify_claim(&preimage, 1_799_999_000).is_err());
        assert!(zero_amt.verify_refund(1_800_000_000).is_err());

        let mut zero_hash = escrow;
        zero_hash.payment_hash = [0u8; 32];
        assert!(zero_hash.verify_claim(&preimage, 1_799_999_000).is_err());
    }
}
