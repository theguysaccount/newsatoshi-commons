//! Immutable, Ownerless CosmWasm SHA-256 HTLC Bridge (`bridges/cosmos/src/lib.rs`).
//!
//! Enables zero-custody atomic swaps between any Cosmos IBC (`ICS-20`) denom (USDC Noble, ATOM,
//! OSMO, TIA) and Newsatoshi Mainnet v2 (`LOVE` / `nsUSD` / custom RFC-0028 stablecoins) via
//! identical `SHA-256(32-byte preimage) == payment_hash`. Zero admin keys, zero custody.

use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{
    entry_point, to_json_binary, Addr, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult,
};
use cw_storage_plus::Map;
use sha2::{Digest, Sha256};

pub const MAX_TIMELOCK_SECS: u64 = 30 * 86_400;

#[cw_serde]
pub struct InstantiateMsg {}

#[cw_serde]
pub enum ExecuteMsg {
    LockHtlc {
        payment_hash_hex: String,
        recipient: String,
        timelock_unix: u64,
        ics20_memo: Option<String>,
    },
    ClaimHtlc {
        payment_hash_hex: String,
        preimage_hex: String,
    },
    RefundHtlc {
        payment_hash_hex: String,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(SwapLock)]
    GetSwap { payment_hash_hex: String },
}

#[cw_serde]
pub struct SwapLock {
    pub sender: Addr,
    pub recipient: Addr,
    pub funds: Vec<Coin>,
    pub payment_hash_hex: String,
    pub timelock_unix: u64,
    pub ics20_memo: Option<String>,
    pub claimed: bool,
    pub refunded: bool,
    pub preimage_hex: Option<String>,
}

const SWAPS: Map<&str, SwapLock> = Map::new("ns_htlc_swaps");

fn validate_hex32(raw: &str, field: &str) -> StdResult<[u8; 32]> {
    let clean = raw.trim().trim_start_matches("0x").to_ascii_lowercase();
    if clean.len() != 64 || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(StdError::generic_err(format!("{field} must be 64 hex chars")));
    }
    let bytes = hex::decode(&clean).map_err(|e| StdError::generic_err(e.to_string()))?;
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[entry_point]
pub fn instantiate(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> StdResult<Response> {
    Ok(Response::new().add_attribute("action", "instantiate_newsatoshi_htlc"))
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> StdResult<Response> {
    match msg {
        ExecuteMsg::LockHtlc {
            payment_hash_hex,
            recipient,
            timelock_unix,
            ics20_memo,
        } => {
            if info.funds.is_empty() {
                return Err(StdError::generic_err("must attach IBC/native funds"));
            }
            let hash_bytes = validate_hex32(&payment_hash_hex, "payment_hash_hex")?;
            let key = hex::encode(hash_bytes);
            if SWAPS.may_load(deps.storage, &key)?.is_some() {
                return Err(StdError::generic_err("swap already exists for payment_hash"));
            }
            let now = env.block.time.seconds();
            if timelock_unix <= now + 300 || timelock_unix > now + MAX_TIMELOCK_SECS {
                return Err(StdError::generic_err("timelock out of safe range (300s..30d)"));
            }
            let recipient_addr = deps.api.addr_validate(&recipient)?;
            if recipient_addr == info.sender {
                return Err(StdError::generic_err("recipient must differ from sender"));
            }
            let lock = SwapLock {
                sender: info.sender.clone(),
                recipient: recipient_addr,
                funds: info.funds,
                payment_hash_hex: key.clone(),
                timelock_unix,
                ics20_memo,
                claimed: false,
                refunded: false,
                preimage_hex: None,
            };
            SWAPS.save(deps.storage, &key, &lock)?;
            Ok(Response::new()
                .add_attribute("action", "lock_htlc")
                .add_attribute("payment_hash_hex", key))
        }
        ExecuteMsg::ClaimHtlc {
            payment_hash_hex,
            preimage_hex,
        } => {
            let expected = validate_hex32(&payment_hash_hex, "payment_hash_hex")?;
            let preimage = validate_hex32(&preimage_hex, "preimage_hex")?;
            let derived = Sha256::digest(preimage);
            if derived.as_slice() != expected.as_slice() {
                return Err(StdError::generic_err("INVALID_PREIMAGE"));
            }
            let key = hex::encode(expected);
            let mut lock = SWAPS.load(deps.storage, &key)?;
            if lock.claimed || lock.refunded {
                return Err(StdError::generic_err("swap already settled"));
            }
            lock.claimed = true;
            lock.preimage_hex = Some(hex::encode(preimage));
            let payout = BankMsg::Send {
                to_address: lock.recipient.to_string(),
                amount: lock.funds.clone(),
            };
            SWAPS.save(deps.storage, &key, &lock)?;
            Ok(Response::new()
                .add_message(payout)
                .add_attribute("action", "claim_htlc")
                .add_attribute("payment_hash_hex", key)
                .add_attribute("preimage_hex", hex::encode(preimage)))
        }
        ExecuteMsg::RefundHtlc { payment_hash_hex } => {
            let expected = validate_hex32(&payment_hash_hex, "payment_hash_hex")?;
            let key = hex::encode(expected);
            let mut lock = SWAPS.load(deps.storage, &key)?;
            if lock.claimed || lock.refunded {
                return Err(StdError::generic_err("swap already settled"));
            }
            if env.block.time.seconds() < lock.timelock_unix {
                return Err(StdError::generic_err("NOT_EXPIRED"));
            }
            lock.refunded = true;
            let refund = BankMsg::Send {
                to_address: lock.sender.to_string(),
                amount: lock.funds.clone(),
            };
            SWAPS.save(deps.storage, &key, &lock)?;
            Ok(Response::new()
                .add_message(refund)
                .add_attribute("action", "refund_htlc")
                .add_attribute("payment_hash_hex", key))
        }
    }
}

#[entry_point]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetSwap { payment_hash_hex } => {
            let key = hex::encode(validate_hex32(&payment_hash_hex, "payment_hash_hex")?);
            to_json_binary(&SWAPS.load(deps.storage, &key)?)
        }
    }
}
