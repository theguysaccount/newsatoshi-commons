//! `nsUSD` Dual-Tier Stablecoin Engine (`src/nsusd.rs` — RFC-0028).
//!
//! Strict Integer Arithmetic (`u64`/`u128`, zero floating-point drift) & Strict Asset Isolation:
//! - **Asset ID 1 (`nsUSD` — Sovereign Unfreezable Dollar):** Over-collateralized Bitcoin (`BTC`)
//!   CDPs (`>= 150%` Min Collateral Ratio, `120%` Liquidation Threshold), Liquity-style Stability
//!   Pool, direct permissionless `$1.00` collateral redemption floor, and a `PriorLock`-bonded
//!   median BTC/USD oracle (50% challenger bounty / 50% permanent `LOVE` burn on outlier spoofing,
//!   automatic mint halt on stale feeds). Zero corporate freeze switch, zero algorithmic death spiral.
//! - **Asset ID 2 (`nsUSD-B` — 1:1 Bridged Shielded Dollar):** Explicitly separated 1:1 USDC/USDS/DAI
//!   escrow receipts carrying honest underlying issuer-risk disclosure.
//!
//! Zero fake data: fresh live state starts at `0` vaults, `0` debt, `0` collateral. Adversarial
//! stress tests execute inside an isolated ephemeral sandbox (`run_adversarial_stress_suite`)
//! without ever mutating live balances.

use crate::crypto::{sha256_concat, Hash32};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// `1 BTC = 100,000,000` satoshis.
pub const SATS_PER_BTC: u128 = 100_000_000;
/// `1 nsUSD = 1,000,000` micro-nsUSD (6 decimals, matching USDC precision).
pub const MICRO_PER_USD: u128 = 1_000_000;
/// `1 USD = 100` cents (oracle quote precision).
pub const CENTS_PER_USD: u128 = 100;
/// Minimum Collateral Ratio for opening/minting Sovereign `nsUSD`: `150%` (`15_000` bps).
pub const MIN_COLLATERAL_RATIO_BPS: u64 = 15_000;
/// Liquidation threshold: `< 120%` (`12_000` bps).
pub const LIQUIDATION_RATIO_BPS: u64 = 12_000;
/// Base permissionless `$1.00` redemption fee: `0.50%` (`50` bps).
pub const REDEMPTION_FEE_BPS: u64 = 50;
/// Keeper gas stipend on liquidation: `0.50%` (`50` bps) of liquidated collateral.
pub const KEEPER_STIPEND_BPS: u64 = 50;
/// Maximum allowed oracle quote deviation before `PriorLock` slashing triggers: `5.00%` (`500` bps).
pub const ORACLE_SLASH_DEVIATION_BPS: u64 = 500;
/// Minimum independent fresh `PriorLock`-bonded oracle reporters required to mint.
pub const MIN_FRESH_ORACLE_REPORTERS: usize = 3;
/// Maximum oracle quote age before staleness circuit breaker halts new minting (`900` s = 15 min).
pub const MAX_ORACLE_AGE_SECS: u64 = 900;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum NsUsdAssetId {
    /// Native Mainnet v2 Layer-1 `LOVE`.
    NativeLove = 0,
    /// Sovereign unfreezable BTC-collateralized `nsUSD` (zero corporate freeze switch).
    SovereignNsUsd = 1,
    /// 1:1 Bridged `USDC`/`USDS`/`DAI` shielded dollar (`nsUSD-B` — inherits underlying issuer risk at bridge exit).
    BridgedNsUsdB = 2,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleQuote {
    pub reporter_id: String,
    pub bond_love_sats: u64,
    pub btc_usd_cents: u64,
    pub quoted_at_unix: u64,
    pub slashed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleSlashReceipt {
    pub reporter_id: String,
    pub challenger_id: String,
    pub median_btc_usd_cents: u64,
    pub spoofed_btc_usd_cents: u64,
    pub deviation_bps: u64,
    pub total_slashed_love_sats: u64,
    pub challenger_bounty_love_sats: u64,
    pub burned_love_sats: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SovereignVault {
    pub vault_id: String,
    pub owner: String,
    pub asset_id: NsUsdAssetId,
    pub btc_collateral_sats: u64,
    pub debt_nsusd_micro: u64,
    pub closed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedemptionReceipt {
    pub redeemer: String,
    pub nsusd_burned_micro: u64,
    pub oracle_btc_usd_cents: u64,
    pub gross_btc_sats: u64,
    pub fee_btc_sats: u64,
    pub net_btc_delivered_sats: u64,
    pub vaults_touched: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiquidationReceipt {
    pub vault_id: String,
    pub keeper: String,
    pub pre_cr_bps: u64,
    pub debt_absorbed_micro: u64,
    pub collateral_seized_sats: u64,
    pub stability_pool_btc_gain_sats: u64,
    pub keeper_stipend_sats: u64,
    pub leftover_bad_debt_micro: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgedEscrowReceipt {
    pub receipt_id: String,
    pub asset_id: NsUsdAssetId,
    pub backing_token: String, // "USDC" | "USDS" | "DAI"
    pub chain: String,
    pub amount_micro: u64,
    pub issuer_freeze_risk_disclosed: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NsUsdEngine {
    pub quotes: BTreeMap<String, OracleQuote>,
    pub vaults: BTreeMap<String, SovereignVault>,
    pub stability_pool_nsusd_micro: u64,
    pub stability_pool_btc_sats: u64,
    pub protocol_fee_btc_sats: u64,
    pub oracle_love_burned_sats: u64,
    pub oracle_challenger_bounties_sats: u64,
    pub bridged_escrows: BTreeMap<String, BridgedEscrowReceipt>,
    pub total_bridged_nsusd_b_micro: u64,
}

impl NsUsdEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Submit or update a `PriorLock`-bonded BTC/USD price quote.
    /// Permanently slashed reporters cannot un-slash themselves by resubmitting.
    pub fn submit_oracle_quote(
        &mut self,
        reporter_id: &str,
        bond_love_sats: u64,
        btc_usd_cents: u64,
        quoted_at_unix: u64,
    ) -> Result<(), String> {
        if reporter_id.trim().is_empty() {
            return Err("reporter_id required".to_string());
        }
        if let Some(prev) = self.quotes.get(reporter_id) {
            if prev.slashed {
                return Err("reporter_slashed_permanently: cannot resubmit after PriorLock slash".to_string());
            }
        }
        if bond_love_sats < 1_000_000 {
            return Err("PriorLock oracle bond must be >= 0.01 LOVE (1,000,000 sats)".to_string());
        }
        if btc_usd_cents == 0 {
            return Err("btc_usd_cents must be > 0".to_string());
        }
        self.quotes.insert(
            reporter_id.to_string(),
            OracleQuote {
                reporter_id: reporter_id.to_string(),
                bond_love_sats,
                btc_usd_cents,
                quoted_at_unix,
                slashed: false,
            },
        );
        Ok(())
    }

    /// Computes exact integer median BTC/USD price across non-slashed, non-stale, non-future reporters.
    /// Rejects future-dated timestamps (> 60 s clock drift) and fails closed if fewer than
    /// `MIN_FRESH_ORACLE_REPORTERS` fresh quotes exist.
    pub fn fresh_median_btc_usd_cents(&self, now_unix: u64) -> Result<u64, String> {
        let max_future = now_unix.saturating_add(60);
        let mut fresh: Vec<u64> = self
            .quotes
            .values()
            .filter(|q| {
                !q.slashed
                    && q.quoted_at_unix <= max_future
                    && now_unix.saturating_sub(q.quoted_at_unix) <= MAX_ORACLE_AGE_SECS
            })
            .map(|q| q.btc_usd_cents)
            .collect();
        if fresh.len() < MIN_FRESH_ORACLE_REPORTERS {
            return Err(format!(
                "oracle_circuit_breaker_stale: only {} fresh reporters (need >= {MIN_FRESH_ORACLE_REPORTERS})",
                fresh.len()
            ));
        }
        fresh.sort_unstable();
        let mid = fresh.len() / 2;
        if fresh.len() % 2 == 1 {
            Ok(fresh[mid])
        } else {
            Ok(((fresh[mid - 1] as u128 + fresh[mid] as u128) / 2) as u64)
        }
    }

    /// Permissionless `PriorLock` challenge: slashes any oracle reporter deviating > `500` bps
    /// from the verified fresh median — exact `50%` challenger bounty + `50%` permanent `LOVE` burn.
    pub fn challenge_oracle_outlier(
        &mut self,
        reporter_id: &str,
        challenger_id: &str,
        now_unix: u64,
    ) -> Result<OracleSlashReceipt, String> {
        let median = self.fresh_median_btc_usd_cents(now_unix)?;
        let q = self
            .quotes
            .get_mut(reporter_id)
            .ok_or_else(|| "unknown oracle reporter".to_string())?;
        if q.slashed {
            return Err("reporter already slashed".to_string());
        }
        let diff = q.btc_usd_cents.abs_diff(median) as u128;
        let dev_bps = ((diff * 10_000) / (median as u128)) as u64;
        if dev_bps <= ORACLE_SLASH_DEVIATION_BPS {
            return Err(format!(
                "quote within tolerance ({dev_bps} bps <= {ORACLE_SLASH_DEVIATION_BPS} bps)"
            ));
        }
        let bond = q.bond_love_sats;
        let bounty = bond / 2;
        let burn = bond - bounty;
        q.slashed = true;
        q.bond_love_sats = 0;
        self.oracle_challenger_bounties_sats =
            self.oracle_challenger_bounties_sats.saturating_add(bounty);
        self.oracle_love_burned_sats = self.oracle_love_burned_sats.saturating_add(burn);

        Ok(OracleSlashReceipt {
            reporter_id: reporter_id.to_string(),
            challenger_id: challenger_id.to_string(),
            median_btc_usd_cents: median,
            spoofed_btc_usd_cents: q.btc_usd_cents,
            deviation_bps: dev_bps,
            total_slashed_love_sats: bond,
            challenger_bounty_love_sats: bounty,
            burned_love_sats: burn,
        })
    }

    /// Exact integer collateral value in micro-USD (`1 USD = 1,000,000 micro-USD`).
    pub fn collateral_value_micro_usd(btc_sats: u64, btc_usd_cents: u64) -> u128 {
        // btc_sats * (btc_usd_cents / 100) * 1_000_000 / 100_000_000
        // = btc_sats * btc_usd_cents / 10_000
        ((btc_sats as u128) * (btc_usd_cents as u128)) / 10_000
    }

    /// Exact collateral ratio in basis points (`15_000 = 150.00%`). Returns `u64::MAX` when debt == 0.
    pub fn vault_cr_bps(btc_sats: u64, debt_micro: u64, btc_usd_cents: u64) -> u64 {
        if debt_micro == 0 {
            return u64::MAX;
        }
        let col_usd = Self::collateral_value_micro_usd(btc_sats, btc_usd_cents);
        let bps = (col_usd * 10_000) / (debt_micro as u128);
        bps.min(u64::MAX as u128) as u64
    }

    /// Open a Sovereign BTC-collateralized `nsUSD` vault (`>= 150%` MCR enforced, fresh oracle required).
    pub fn open_sovereign_vault(
        &mut self,
        vault_id: &str,
        owner: &str,
        btc_collateral_sats: u64,
        mint_nsusd_micro: u64,
        now_unix: u64,
    ) -> Result<SovereignVault, String> {
        if self.vaults.contains_key(vault_id) {
            return Err("vault_id already exists".to_string());
        }
        if btc_collateral_sats == 0 || mint_nsusd_micro == 0 {
            return Err("collateral and mint amount must be > 0".to_string());
        }
        let price = self.fresh_median_btc_usd_cents(now_unix)?;
        let cr_bps = Self::vault_cr_bps(btc_collateral_sats, mint_nsusd_micro, price);
        if cr_bps < MIN_COLLATERAL_RATIO_BPS {
            return Err(format!(
                "undercollateralized_vault: CR {cr_bps} bps < minimum {MIN_COLLATERAL_RATIO_BPS} bps (150%)"
            ));
        }
        let v = SovereignVault {
            vault_id: vault_id.to_string(),
            owner: owner.to_string(),
            asset_id: NsUsdAssetId::SovereignNsUsd,
            btc_collateral_sats,
            debt_nsusd_micro: mint_nsusd_micro,
            closed: false,
        };
        self.vaults.insert(vault_id.to_string(), v.clone());
        Ok(v)
    }

    pub fn deposit_stability_pool(&mut self, nsusd_micro: u64) -> Result<u64, String> {
        if nsusd_micro == 0 {
            return Err("deposit must be > 0".to_string());
        }
        if self.stability_pool_nsusd_micro.saturating_add(nsusd_micro) > self.total_sovereign_debt_micro() {
            return Err("stability pool deposit cannot exceed outstanding sovereign nsUSD supply".to_string());
        }
        self.stability_pool_nsusd_micro += nsusd_micro;
        Ok(self.stability_pool_nsusd_micro)
    }

    /// Permissionless Liquity-style `$1.00` direct collateral redemption against the lowest-CR vault(s).
    pub fn redeem_nsusd_for_btc(
        &mut self,
        redeemer: &str,
        nsusd_micro: u64,
        now_unix: u64,
    ) -> Result<RedemptionReceipt, String> {
        if nsusd_micro == 0 {
            return Err("redemption amount must be > 0".to_string());
        }
        let price = self.fresh_median_btc_usd_cents(now_unix)?;
        let total_debt = self.total_sovereign_debt_micro();
        if nsusd_micro > total_debt {
            return Err("redemption exceeds total outstanding sovereign nsUSD debt".to_string());
        }

        // Sort open vaults ascending by collateral ratio (lowest CR redeemed first, Liquity rule)
        let mut order: Vec<(u64, String)> = self
            .vaults
            .values()
            .filter(|v| !v.closed && v.debt_nsusd_micro > 0)
            .map(|v| (Self::vault_cr_bps(v.btc_collateral_sats, v.debt_nsusd_micro, price), v.vault_id.clone()))
            .collect();
        order.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

        let mut remaining_micro = nsusd_micro;
        let mut gross_btc_sats: u64 = 0;
        let mut vaults_touched = Vec::new();

        for (_, vid) in order {
            if remaining_micro == 0 {
                break;
            }
            let Some(v) = self.vaults.get_mut(&vid) else {
                continue;
            };
            let take_debt = remaining_micro.min(v.debt_nsusd_micro);
            // Exact BTC satoshis worth `take_debt` micro-USD at `price` cents/BTC:
            // sats = take_debt * 10_000 / price (checked conversion, no u64 truncation).
            let btc_sats_needed_u128 = ((take_debt as u128) * 10_000) / (price as u128);
            let btc_sats_needed = u64::try_from(btc_sats_needed_u128)
                .map_err(|_| "redemption btc_sats overflow".to_string())?;
            let take_sats = btc_sats_needed.min(v.btc_collateral_sats);
            v.debt_nsusd_micro -= take_debt;
            v.btc_collateral_sats -= take_sats;
            // Only close the vault if BOTH debt AND residual collateral are zero (never strand owner BTC!).
            if v.debt_nsusd_micro == 0 && v.btc_collateral_sats == 0 {
                v.closed = true;
            }
            remaining_micro -= take_debt;
            gross_btc_sats = gross_btc_sats.saturating_add(take_sats);
            vaults_touched.push(vid);
        }

        let fee_btc_sats = (((gross_btc_sats as u128) * (REDEMPTION_FEE_BPS as u128)) / 10_000) as u64;
        let net_btc_delivered_sats = gross_btc_sats.saturating_sub(fee_btc_sats);
        self.protocol_fee_btc_sats = self.protocol_fee_btc_sats.saturating_add(fee_btc_sats);

        Ok(RedemptionReceipt {
            redeemer: redeemer.to_string(),
            nsusd_burned_micro: nsusd_micro - remaining_micro,
            oracle_btc_usd_cents: price,
            gross_btc_sats,
            fee_btc_sats,
            net_btc_delivered_sats,
            vaults_touched,
        })
    }

    /// Permissionless liquidation of an underwater vault (`CR < 120%`) absorbed by the Stability Pool.
    pub fn liquidate_underwater_vault(
        &mut self,
        vault_id: &str,
        keeper: &str,
        now_unix: u64,
    ) -> Result<LiquidationReceipt, String> {
        let price = self.fresh_median_btc_usd_cents(now_unix)?;
        let v = self
            .vaults
            .get(vault_id)
            .ok_or_else(|| "vault not found".to_string())?
            .clone();
        if v.closed || v.debt_nsusd_micro == 0 {
            return Err("vault already closed".to_string());
        }
        let cr_bps = Self::vault_cr_bps(v.btc_collateral_sats, v.debt_nsusd_micro, price);
        if cr_bps >= LIQUIDATION_RATIO_BPS {
            return Err(format!(
                "vault is healthy: CR {cr_bps} bps >= liquidation threshold {LIQUIDATION_RATIO_BPS} bps"
            ));
        }

        let absorbed = v.debt_nsusd_micro.min(self.stability_pool_nsusd_micro);
        let leftover_bad_debt = v.debt_nsusd_micro - absorbed;
        self.stability_pool_nsusd_micro -= absorbed;

        let keeper_stipend =
            (((v.btc_collateral_sats as u128) * (KEEPER_STIPEND_BPS as u128)) / 10_000) as u64;
        let sp_btc_gain = v.btc_collateral_sats.saturating_sub(keeper_stipend);
        self.stability_pool_btc_sats = self.stability_pool_btc_sats.saturating_add(sp_btc_gain);

        if let Some(vm) = self.vaults.get_mut(vault_id) {
            vm.debt_nsusd_micro = 0;
            vm.btc_collateral_sats = 0;
            vm.closed = true;
        }

        Ok(LiquidationReceipt {
            vault_id: vault_id.to_string(),
            keeper: keeper.to_string(),
            pre_cr_bps: cr_bps,
            debt_absorbed_micro: absorbed,
            collateral_seized_sats: v.btc_collateral_sats,
            stability_pool_btc_gain_sats: sp_btc_gain,
            keeper_stipend_sats: keeper_stipend,
            leftover_bad_debt_micro: leftover_bad_debt,
        })
    }

    /// Record an explicitly separated Tier-1B `nsUSD-B` 1:1 bridged escrow receipt (never mixed with `asset_id = 1`).
    pub fn record_bridged_nsusd_b(
        &mut self,
        receipt_id: &str,
        backing_token: &str,
        chain: &str,
        amount_micro: u64,
    ) -> Result<BridgedEscrowReceipt, String> {
        if !matches!(backing_token, "USDC" | "USDS" | "DAI") {
            return Err("unsupported Tier-1B bridged dollar asset (expected USDC, USDS, or DAI)".to_string());
        }
        if amount_micro == 0 || self.bridged_escrows.contains_key(receipt_id) {
            return Err("invalid amount or duplicate receipt_id".to_string());
        }
        let rec = BridgedEscrowReceipt {
            receipt_id: receipt_id.to_string(),
            asset_id: NsUsdAssetId::BridgedNsUsdB,
            backing_token: backing_token.to_string(),
            chain: chain.to_string(),
            amount_micro,
            issuer_freeze_risk_disclosed: true,
        };
        self.bridged_escrows.insert(receipt_id.to_string(), rec.clone());
        self.total_bridged_nsusd_b_micro = self.total_bridged_nsusd_b_micro.saturating_add(amount_micro);
        Ok(rec)
    }

    pub fn total_sovereign_btc_sats(&self) -> u64 {
        self.vaults
            .values()
            .filter(|v| !v.closed)
            .fold(0u64, |acc, v| acc.saturating_add(v.btc_collateral_sats))
    }

    pub fn total_sovereign_debt_micro(&self) -> u64 {
        self.vaults
            .values()
            .filter(|v| !v.closed)
            .fold(0u64, |acc, v| acc.saturating_add(v.debt_nsusd_micro))
    }

    /// Deterministic 32-byte SHA-256 solvency commitment root for L1 anchoring.
    pub fn solvency_root(&self) -> Hash32 {
        sha256_concat(&[
            b"nsUSD-SOLVENCY-V1",
            &self.total_sovereign_btc_sats().to_le_bytes(),
            &self.total_sovereign_debt_micro().to_le_bytes(),
            &self.stability_pool_nsusd_micro.to_le_bytes(),
            &self.stability_pool_btc_sats.to_le_bytes(),
            &self.total_bridged_nsusd_b_micro.to_le_bytes(),
            &self.oracle_love_burned_sats.to_le_bytes(),
        ])
    }

    pub fn summary_json(&self, now_unix: u64) -> Value {
        let median_res = self.fresh_median_btc_usd_cents(now_unix);
        let btc_sats = self.total_sovereign_btc_sats();
        let debt_micro = self.total_sovereign_debt_micro();
        let global_cr_bps = median_res
            .as_ref()
            .ok()
            .map(|&p| Self::vault_cr_bps(btc_sats, debt_micro, p));
        json!({
            "spec": "RFC-0028",
            "solvency_root_hex": hex::encode(self.solvency_root()),
            "sovereign_nsusd": {
                "asset_id": NsUsdAssetId::SovereignNsUsd as u32,
                "freeze_switch": false,
                "algorithmic_unbacked_mint": false,
                "min_collateral_ratio_bps": MIN_COLLATERAL_RATIO_BPS,
                "liquidation_ratio_bps": LIQUIDATION_RATIO_BPS,
                "redemption_fee_bps": REDEMPTION_FEE_BPS,
                "active_vaults": self.vaults.values().filter(|v| !v.closed).count(),
                "total_btc_collateral_sats": btc_sats,
                "total_debt_nsusd_micro": debt_micro,
                "global_cr_bps": global_cr_bps,
                "stability_pool_nsusd_micro": self.stability_pool_nsusd_micro,
                "stability_pool_btc_sats": self.stability_pool_btc_sats,
                "protocol_fee_btc_sats": self.protocol_fee_btc_sats
            },
            "priorlock_oracle": {
                "min_fresh_reporters": MIN_FRESH_ORACLE_REPORTERS,
                "max_age_secs": MAX_ORACLE_AGE_SECS,
                "slash_deviation_bps": ORACLE_SLASH_DEVIATION_BPS,
                "fresh_median_btc_usd_cents": median_res.as_ref().ok().copied(),
                "circuit_breaker_status": match &median_res {
                    Ok(_) => "LIVE_HEALTHY".to_string(),
                    Err(e) => e.clone(),
                },
                "oracle_love_burned_sats": self.oracle_love_burned_sats,
                "oracle_challenger_bounties_sats": self.oracle_challenger_bounties_sats
            },
            "bridged_nsusd_b": {
                "asset_id": NsUsdAssetId::BridgedNsUsdB as u32,
                "issuer_freeze_risk_disclosed": true,
                "total_escrowed_micro": self.total_bridged_nsusd_b_micro,
                "escrow_receipts_count": self.bridged_escrows.len()
            }
        })
    }
}

/// Runs a deterministic, reproducible 4-scenario adversarial stress-test suite inside an isolated
/// ephemeral sandbox (never mutates live balances) — proving sovereign `nsUSD` liquidation,
/// `$1.00` redemption floor, `PriorLock` oracle slashing, and stale-feed circuit breakers.
pub fn run_adversarial_stress_suite(crash_pct: u64) -> Value {
    let crash = crash_pct.clamp(10, 75);
    let t0: u64 = 1_795_800_000;
    let initial_price: u64 = 10_000_000; // $100,000.00 / BTC
    let crashed_price: u64 = (initial_price * (100 - crash)) / 100;

    // ── Scenario 1: BTC Flash Crash Liquidation + Stability Pool Conservation ──
    let mut eng = NsUsdEngine::new();
    for (rep, p) in [("rep-1", 10_000_000), ("rep-2", 10_010_000), ("rep-3", 9_990_000)] {
        eng.submit_oracle_quote(rep, 500_000_000, p, t0).unwrap();
    }
    // Vault A: 1.0 BTC ($100k) minting 60,000 nsUSD (166.67% CR -> drops below 120% on >=30% crash!)
    eng.open_sovereign_vault("vault-a", "alice", 100_000_000, 60_000_000_000, t0)
        .unwrap();
    // Vault B: 2.0 BTC ($200k) minting 60,000 nsUSD (333.33% CR -> stays healthy even on -50% crash!)
    eng.open_sovereign_vault("vault-b", "bob", 200_000_000, 60_000_000_000, t0)
        .unwrap();
    // Stability Pool holds 70,000 nsUSD ready to absorb underwater vaults
    eng.deposit_stability_pool(70_000_000_000).unwrap();

    // Crash BTC price across all 3 oracle reporters
    let t1 = t0 + 60;
    for rep in ["rep-1", "rep-2", "rep-3"] {
        eng.submit_oracle_quote(rep, 500_000_000, crashed_price, t1).unwrap();
    }
    let liq = eng.liquidate_underwater_vault("vault-a", "keeper-1", t1);
    let post_crash_cr = NsUsdEngine::vault_cr_bps(
        eng.total_sovereign_btc_sats(),
        eng.total_sovereign_debt_micro(),
        crashed_price,
    );
    let liq_conserved = liq
        .as_ref()
        .map(|r| {
            r.stability_pool_btc_gain_sats + r.keeper_stipend_sats == r.collateral_seized_sats
                && r.leftover_bad_debt_micro == 0
        })
        .unwrap_or(false);

    // ── Scenario 2: Bank-Run Direct $1.00 Collateral Redemption ──
    let mut eng_red = NsUsdEngine::new();
    for rep in ["rep-1", "rep-2", "rep-3"] {
        eng_red.submit_oracle_quote(rep, 500_000_000, initial_price, t0).unwrap();
    }
    eng_red
        .open_sovereign_vault("vault-low", "maker-1", 100_000_000, 65_000_000_000, t0)
        .unwrap();
    let red = eng_red
        .redeem_nsusd_for_btc("arb-1", 25_000_000_000, t0)
        .unwrap();
    let red_conserved = red.net_btc_delivered_sats + red.fee_btc_sats == red.gross_btc_sats
        && red.nsusd_burned_micro == 25_000_000_000
        && eng_red.total_sovereign_debt_micro() == 40_000_000_000;

    // ── Scenario 3: Malicious Oracle Spoof -> 50% Challenger Bounty / 50% LOVE Burn ──
    let mut eng_orc = NsUsdEngine::new();
    eng_orc.submit_oracle_quote("honest-1", 500_000_000, 10_000_000, t0).unwrap();
    eng_orc.submit_oracle_quote("honest-2", 500_000_000, 10_020_000, t0).unwrap();
    eng_orc.submit_oracle_quote("honest-3", 500_000_000, 9_980_000, t0).unwrap();
    eng_orc.submit_oracle_quote("attacker", 1_000_000_000, 14_000_000, t0).unwrap(); // +40% spoof!
    let slash = eng_orc
        .challenge_oracle_outlier("attacker", "challenger-1", t0)
        .unwrap();
    let slash_exact = slash.challenger_bounty_love_sats == 500_000_000
        && slash.burned_love_sats == 500_000_000
        && slash.challenger_bounty_love_sats + slash.burned_love_sats == slash.total_slashed_love_sats;

    // ── Scenario 4: Stale Oracle Circuit Breaker Halts Minting ──
    let stale_err = eng_orc
        .open_sovereign_vault("vault-stale", "mallory", 100_000_000, 50_000_000_000, t0 + 3600)
        .unwrap_err();
    let stale_halted = stale_err.contains("oracle_circuit_breaker_stale");

    let liq_expected = crash >= 29; // Vault A starts at 166.67% CR -> drops < 120% when crash >= 29%
    let scenario_1_ok = if liq_expected {
        liq.is_ok() && liq_conserved
    } else {
        liq.is_err() // Healthy vault MUST NOT be liquidated
    };

    json!({
        "ok": true,
        "sandbox_only": true,
        "live_state_mutated": false,
        "note": "In-memory accounting & adversarial state-machine stress test (no live L1 custody or real BTC moved).",
        "crash_pct_simulated": crash,
        "all_invariants_passed": scenario_1_ok && red_conserved && slash_exact && stale_halted,
        "scenarios": {
            "1_flash_crash_liquidation": {
                "initial_btc_usd": initial_price / 100,
                "crashed_btc_usd": crashed_price / 100,
                "vault_a_liquidated": liq.is_ok(),
                "receipt": liq.ok(),
                "satoshi_conservation_exact": liq_conserved,
                "post_liquidation_system_cr_bps": post_crash_cr
            },
            "2_bank_run_redemption_floor": {
                "receipt": red,
                "remaining_debt_nsusd_micro": eng_red.total_sovereign_debt_micro(),
                "exact_conservation": red_conserved
            },
            "3_priorlock_oracle_spoof_slash": {
                "receipt": slash,
                "exact_50_50_bounty_burn": slash_exact
            },
            "4_stale_oracle_circuit_breaker": {
                "mint_halted": stale_halted,
                "error_returned": stale_err
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nsusd_adversarial_stress_suite_passes_all_invariants() {
        let res = run_adversarial_stress_suite(50);
        assert_eq!(res["all_invariants_passed"], json!(true));
        assert_eq!(res["scenarios"]["1_flash_crash_liquidation"]["vault_a_liquidated"], json!(true));
        assert_eq!(res["scenarios"]["1_flash_crash_liquidation"]["satoshi_conservation_exact"], json!(true));
        assert_eq!(res["scenarios"]["2_bank_run_redemption_floor"]["exact_conservation"], json!(true));
        assert_eq!(res["scenarios"]["3_priorlock_oracle_spoof_slash"]["exact_50_50_bounty_burn"], json!(true));
        assert_eq!(res["scenarios"]["4_stale_oracle_circuit_breaker"]["mint_halted"], json!(true));
    }

    #[test]
    fn nsusd_rejects_undercollateralized_mint_and_isolates_bridged_asset_id() {
        let mut eng = NsUsdEngine::new();
        let t0 = 1_795_800_000;
        for rep in ["r1", "r2", "r3"] {
            eng.submit_oracle_quote(rep, 100_000_000, 10_000_000, t0).unwrap();
        }
        // 1 BTC = $100k, minting $70k => 142.8% CR < 150% MCR -> MUST FAIL
        let err = eng
            .open_sovereign_vault("v-bad", "eve", 100_000_000, 70_000_000_000, t0)
            .unwrap_err();
        assert!(err.contains("undercollateralized_vault"));

        // Bridged nsUSD-B lives under AssetId 2 and NEVER alters Sovereign AssetId 1 debt
        let br = eng.record_bridged_nsusd_b("br-1", "USDC", "base", 50_000_000).unwrap();
        assert_eq!(br.asset_id, NsUsdAssetId::BridgedNsUsdB);
        assert_eq!(eng.total_sovereign_debt_micro(), 0);
        assert_eq!(eng.total_bridged_nsusd_b_micro, 50_000_000);
    }

    #[test]
    fn nsusd_blocks_slashed_reporter_resubmit_and_future_timestamps() {
        let mut eng = NsUsdEngine::new();
        let t0 = 1_795_800_000;
        eng.submit_oracle_quote("h1", 500_000_000, 10_000_000, t0).unwrap();
        eng.submit_oracle_quote("h2", 500_000_000, 10_010_000, t0).unwrap();
        eng.submit_oracle_quote("h3", 500_000_000, 9_990_000, t0).unwrap();
        // Future-dated quote (+10 hours) MUST NOT be counted as fresh
        eng.submit_oracle_quote("future-bot", 500_000_000, 99_999_999, t0 + 36_000)
            .unwrap();
        let med = eng.fresh_median_btc_usd_cents(t0).unwrap();
        assert_eq!(med, 10_000_000);

        // Slashed attacker MUST be permanently locked out from un-slashing via resubmit
        eng.submit_oracle_quote("bad", 500_000_000, 15_000_000, t0).unwrap();
        eng.challenge_oracle_outlier("bad", "chal", t0).unwrap();
        let err = eng.submit_oracle_quote("bad", 500_000_000, 10_000_000, t0 + 10).unwrap_err();
        assert!(err.contains("reporter_slashed_permanently"));

        // Non-liquidation crash (<29%) passes strictly because healthy vault rejects liquidation
        let mild = run_adversarial_stress_suite(10);
        assert_eq!(mild["all_invariants_passed"], json!(true));
        assert_eq!(mild["scenarios"]["1_flash_crash_liquidation"]["vault_a_liquidated"], json!(false));
    }
}
