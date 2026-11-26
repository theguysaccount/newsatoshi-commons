//! Everyday Multi-Chain Crypto, Bitcoin, Lightning & EVM Security Tools (`src/crypto_tools.rs`).
//!
//! Zero-API-key, live-RPC developer & AI-agent utilities exposed via Streamable HTTP MCP (`/mcp`)
//! and dual-rail `x402 v2` + SHA-256 `PayWord` HTTP `402` endpoints:
//!   1. `inspect_evm_contract`        — Live multi-chain EVM security scan (bytecode, EIP-1967 proxy, owner, ERC-20, Sourcify)
//!   2. `fetch_bitcoin_mempool_oracle` — Live Bitcoin fee rates, tip height, mempool backlog & optional address UTXOs
//!   3. `decode_bolt11_tool`           — Sub-millisecond offline Lightning BOLT-11 invoice decoder
//!   4. `quote_base_uniswap_v3`        — Live Base Mainnet Uniswap V3 QuoterV2 quotes (ETH / USDC <-> wLOVE)
//!   5. `verify_tx_receipt`            — Live cross-chain tx finality & confirmation verifier (ETH/Base/Arb/OP/Polygon/BTC)
//!
//! Zero fake data: every figure is fetched live from public RPCs / mempool.space / Sourcify or computed deterministically.

use crate::swap::decode_bolt11_invoice;
use serde_json::{json, Value};
use std::time::Duration;

const EIP1967_IMPL_SLOT: &str =
    "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc";
const EIP1967_ADMIN_SLOT: &str =
    "0xb53127684a568b3173ae13b9f8a6016e243e63b6e8ee1178d6a717850b5d6103";

const BASE_WETH: &str = "0x4200000000000000000000000000000000000006";
const BASE_USDC: &str = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
const BASE_WLOVE: &str = "0x0A3ccFb3fF981a22A264Aa05CD8b77d06C2732A3";
const BASE_WLOVE_POOL: &str = "0x08ddc8C3fc09ACdab6802a13B26631f47ABcB888";
const BASE_QUOTER_V2: &str = "0x3d4e44Eb1374240CE5F1B871ab261CD16335B76a";
const BASE_SWAP_ROUTER: &str = "0x2626664c2603336E57B271c5C0b26F421741e481";

struct EvmNet {
    name: &'static str,
    chain_id: u64,
    native_symbol: &'static str,
    rpcs: &'static [&'static str],
    explorer_addr: &'static str,
    explorer_tx: &'static str,
}

const EVM_NETS: &[EvmNet] = &[
    EvmNet {
        name: "ethereum",
        chain_id: 1,
        native_symbol: "ETH",
        rpcs: &["https://ethereum-rpc.publicnode.com", "https://eth.drpc.org"],
        explorer_addr: "https://etherscan.io/address/",
        explorer_tx: "https://etherscan.io/tx/",
    },
    EvmNet {
        name: "base",
        chain_id: 8453,
        native_symbol: "ETH",
        rpcs: &["https://mainnet.base.org", "https://base-rpc.publicnode.com"],
        explorer_addr: "https://basescan.org/address/",
        explorer_tx: "https://basescan.org/tx/",
    },
    EvmNet {
        name: "arbitrum",
        chain_id: 42161,
        native_symbol: "ETH",
        rpcs: &["https://arb1.arbitrum.io/rpc", "https://arbitrum-one-rpc.publicnode.com"],
        explorer_addr: "https://arbiscan.io/address/",
        explorer_tx: "https://arbiscan.io/tx/",
    },
    EvmNet {
        name: "optimism",
        chain_id: 10,
        native_symbol: "ETH",
        rpcs: &["https://mainnet.optimism.io", "https://optimism-rpc.publicnode.com"],
        explorer_addr: "https://optimistic.etherscan.io/address/",
        explorer_tx: "https://optimistic.etherscan.io/tx/",
    },
    EvmNet {
        name: "polygon",
        chain_id: 137,
        native_symbol: "POL",
        rpcs: &["https://polygon.drpc.org", "https://polygon-bor-rpc.publicnode.com"],
        explorer_addr: "https://polygonscan.com/address/",
        explorer_tx: "https://polygonscan.com/tx/",
    },
];

fn resolve_evm_net(chain: &str) -> Result<&'static EvmNet, String> {
    let key = chain.trim().to_ascii_lowercase();
    for net in EVM_NETS {
        if net.name == key
            || (key == "eth" && net.chain_id == 1)
            || (key == "arb" && net.chain_id == 42161)
            || (key == "op" && net.chain_id == 10)
            || (key == "matic" && net.chain_id == 137)
            || key == net.chain_id.to_string()
        {
            return Ok(net);
        }
    }
    Err(format!(
        "unsupported EVM chain '{chain}' (supported: ethereum, base, arbitrum, optimism, polygon)"
    ))
}

fn normalize_evm_addr(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    let hex_part = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .ok_or_else(|| "address must start with 0x".to_string())?;
    if hex_part.len() != 40 || !hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("address must be 20-byte 0x hex (42 chars)".to_string());
    }
    Ok(format!("0x{}", hex_part.to_ascii_lowercase()))
}

fn normalize_hash32(raw: &str, label: &str) -> Result<String, String> {
    let s = raw.trim();
    let hex_part = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    if hex_part.len() != 64 || !hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{label} must be 32-byte hex (64 hex chars)"));
    }
    Ok(hex_part.to_ascii_lowercase())
}

fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .http_status_as_error(false)
        .build()
        .into()
}

fn evm_rpc_call(net: &EvmNet, method: &str, params: Value) -> Result<Value, String> {
    let agent = http_agent();
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    });
    let body_str = serde_json::to_string(&body).unwrap_or_default();
    let mut last_err = String::new();
    for url in net.rpcs {
        match agent
            .post(*url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "newsatoshi-commons/0.12")
            .send(body_str.as_bytes())
        {
            Ok(mut resp) => {
                let status = resp.status().as_u16();
                let txt = resp.body_mut().read_to_string().unwrap_or_default();
                if status >= 400 {
                    last_err = format!("RPC HTTP {status}");
                    continue;
                }
                if let Ok(parsed) = serde_json::from_str::<Value>(&txt) {
                    if let Some(err) = parsed.get("error") {
                        return Err(err
                            .get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("EVM RPC reverted")
                            .to_string());
                    }
                    if let Some(res) = parsed.get("result") {
                        return Ok(res.clone());
                    }
                }
                last_err = "malformed RPC JSON".to_string();
            }
            Err(e) => {
                last_err = e.to_string();
            }
        }
    }
    Err(format!("{} RPC failed: {last_err}", net.name))
}

fn eth_call_hex(net: &EvmNet, to: &str, data: &str) -> Option<String> {
    evm_rpc_call(net, "eth_call", json!([{ "to": to, "data": data }, "latest"]))
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
}

fn decode_u128_hex(hex_str: &str) -> u128 {
    let clean = hex_str
        .trim()
        .strip_prefix("0x")
        .or_else(|| hex_str.trim().strip_prefix("0X"))
        .unwrap_or(hex_str.trim());
    let trimmed = clean.trim_start_matches('0');
    if trimmed.is_empty() {
        return 0;
    }
    let slice = if trimmed.len() > 32 {
        &trimmed[trimmed.len() - 32..]
    } else {
        trimmed
    };
    u128::from_str_radix(slice, 16).unwrap_or(0)
}

fn decode_abi_address(hex_word: &str) -> Option<String> {
    let clean = hex_word.strip_prefix("0x").unwrap_or(hex_word);
    if clean.len() < 64 {
        return None;
    }
    let addr = &clean[24..64];
    if addr.bytes().all(|b| b == b'0') {
        return None;
    }
    Some(format!("0x{}", addr.to_ascii_lowercase()))
}

fn decode_abi_string(hex_data: &str) -> Option<String> {
    let clean = hex_data.strip_prefix("0x").unwrap_or(hex_data);
    let raw = hex::decode(clean).ok()?;
    if raw.is_empty() {
        return None;
    }
    // Standard dynamic ABI string: offset(32) + len(32) + bytes
    if raw.len() >= 64 {
        let offset = u64::from_be_bytes(raw[24..32].try_into().ok()?) as usize;
        if offset + 32 <= raw.len() {
            let len = u64::from_be_bytes(raw[offset + 24..offset + 32].try_into().ok()?) as usize;
            if len > 0 && len <= 128 && offset + 32 + len <= raw.len() {
                let s = String::from_utf8(raw[offset + 32..offset + 32 + len].to_vec()).ok()?;
                let trimmed = s.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
    }
    // Fallback bytes32 string (e.g., MKR-style tokens)
    if raw.len() == 32 {
        let stripped: Vec<u8> = raw.into_iter().take_while(|&b| b != 0).collect();
        if let Ok(s) = String::from_utf8(stripped) {
            let trimmed = s.trim().to_string();
            if !trimmed.is_empty() && trimmed.chars().all(|c| !c.is_control()) {
                return Some(trimmed);
            }
        }
    }
    None
}

fn format_units_u128(val: u128, decimals: u32, max_frac: usize) -> String {
    let base = 10u128.pow(decimals);
    let whole = val / base;
    let frac = val % base;
    if max_frac == 0 || frac == 0 {
        return whole.to_string();
    }
    let frac_full = format!("{:0width$}", frac, width = decimals as usize);
    let cut = &frac_full[..max_frac.min(frac_full.len())];
    let trimmed = cut.trim_end_matches('0');
    if trimmed.is_empty() {
        whole.to_string()
    } else {
        format!("{whole}.{trimmed}")
    }
}

fn parse_decimal_units(amount_str: &str, decimals: u32) -> Result<u128, String> {
    let clean = amount_str.trim().replace(',', "");
    if clean.is_empty() {
        return Err("amount cannot be empty".to_string());
    }
    let mut parts = clean.split('.');
    let whole_str = parts.next().unwrap_or("0");
    let frac_str = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return Err(format!("invalid decimal amount '{amount_str}'"));
    }
    if !whole_str.bytes().all(|b| b.is_ascii_digit())
        || !frac_str.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(format!("invalid numeric amount '{amount_str}'"));
    }
    let whole: u128 = if whole_str.is_empty() {
        0
    } else {
        whole_str
            .parse::<u128>()
            .map_err(|_| format!("amount overflow '{amount_str}'"))?
    };
    let mut frac_padded = frac_str.chars().take(decimals as usize).collect::<String>();
    while frac_padded.len() < decimals as usize {
        frac_padded.push('0');
    }
    let frac: u128 = if frac_padded.is_empty() {
        0
    } else {
        frac_padded
            .parse::<u128>()
            .map_err(|_| format!("fractional overflow '{amount_str}'"))?
    };
    let scale = 10u128.pow(decimals);
    whole
        .checked_mul(scale)
        .and_then(|v| v.checked_add(frac))
        .ok_or_else(|| format!("amount overflow '{amount_str}'"))
}

fn pad_addr_32(addr: &str) -> Result<String, String> {
    let clean = normalize_evm_addr(addr)?;
    Ok(format!("{:0>64}", &clean[2..]))
}

fn pad_u128_32(v: u128) -> String {
    format!("{v:064x}")
}

// ---------------------------------------------------------------------------
// Tool 1: Live Multi-Chain EVM Contract Security & Metadata Inspector
// ---------------------------------------------------------------------------

pub fn inspect_evm_contract(chain: &str, address: &str) -> Result<Value, String> {
    let net = resolve_evm_net(chain)?;
    let addr = normalize_evm_addr(address)?;

    let code_hex = evm_rpc_call(net, "eth_getCode", json!([&addr, "latest"]))?
        .as_str()
        .unwrap_or("0x")
        .to_string();
    let code_clean = code_hex.strip_prefix("0x").unwrap_or(&code_hex);
    let bytecode_bytes = code_clean.len() / 2;
    let is_contract = bytecode_bytes > 0;

    let balance_wei = evm_rpc_call(net, "eth_getBalance", json!([&addr, "latest"]))
        .ok()
        .and_then(|v| v.as_str().map(decode_u128_hex))
        .unwrap_or(0);

    let mut impl_addr: Option<String> = None;
    let mut admin_addr: Option<String> = None;
    let mut owner_addr: Option<String> = None;
    let mut erc20_name: Option<String> = None;
    let mut erc20_symbol: Option<String> = None;
    let mut erc20_decimals: Option<u64> = None;
    let mut erc20_total_supply: Option<String> = None;
    let mut sourcify_status = "unknown".to_string();

    if is_contract {
        // EIP-1967 proxy slots
        if let Ok(slot_val) = evm_rpc_call(
            net,
            "eth_getStorageAt",
            json!([&addr, EIP1967_IMPL_SLOT, "latest"]),
        ) {
            if let Some(s) = slot_val.as_str() {
                impl_addr = decode_abi_address(s);
            }
        }
        if let Ok(slot_val) = evm_rpc_call(
            net,
            "eth_getStorageAt",
            json!([&addr, EIP1967_ADMIN_SLOT, "latest"]),
        ) {
            if let Some(s) = slot_val.as_str() {
                admin_addr = decode_abi_address(s);
            }
        }
        // Ownable owner() -> 0x8da5cb5b
        if let Some(res) = eth_call_hex(net, &addr, "0x8da5cb5b") {
            owner_addr = decode_abi_address(&res);
        }
        // ERC-20 metadata probes
        if let Some(res) = eth_call_hex(net, &addr, "0x06fdde03") {
            erc20_name = decode_abi_string(&res);
        }
        if let Some(res) = eth_call_hex(net, &addr, "0x95d89b41") {
            erc20_symbol = decode_abi_string(&res);
        }
        if let Some(res) = eth_call_hex(net, &addr, "0x313ce567") {
            let dec = decode_u128_hex(&res);
            if dec <= 36 && res.len() >= 66 {
                erc20_decimals = Some(dec as u64);
            }
        }
        if let Some(res) = eth_call_hex(net, &addr, "0x18160ddd") {
            if res.len() >= 66 {
                erc20_total_supply = Some(decode_u128_hex(&res).to_string());
            }
        }

        // Sourcify live source verification check
        let agent = http_agent();
        let sc_url = format!(
            "https://sourcify.dev/server/check-by-addresses?addresses={addr}&chainIds={}",
            net.chain_id
        );
        if let Ok(mut resp) = agent
            .get(&sc_url)
            .header("User-Agent", "newsatoshi-commons/0.12")
            .call()
        {
            if let Ok(txt) = resp.body_mut().read_to_string() {
                if let Ok(arr) = serde_json::from_str::<Value>(&txt) {
                    if let Some(st) = arr
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(|o| o.get("status"))
                        .and_then(|s| s.as_str())
                    {
                        sourcify_status = st.to_string();
                    }
                }
            }
        }
    }

    let is_proxy = impl_addr.is_some();
    let is_ownerless = is_contract && owner_addr.is_none();
    let mut risk_flags: Vec<&'static str> = Vec::new();
    if !is_contract {
        risk_flags.push("EOA_NOT_A_CONTRACT");
    } else {
        if is_proxy {
            risk_flags.push("UPGRADEABLE_EIP1967_PROXY");
        }
        if owner_addr.is_some() {
            risk_flags.push("ACTIVE_OWNER_PRIVILEGE");
        }
        if sourcify_status == "false" {
            risk_flags.push("UNVERIFIED_ON_SOURCIFY");
        }
        if !is_proxy && is_ownerless && (sourcify_status == "perfect" || sourcify_status == "partial")
        {
            risk_flags.push("IMMUTABLE_OWNERLESS_VERIFIED");
        }
    }

    let erc20 = if erc20_symbol.is_some() || erc20_name.is_some() {
        Some(json!({
            "name": erc20_name,
            "symbol": erc20_symbol,
            "decimals": erc20_decimals,
            "total_supply_raw": erc20_total_supply
        }))
    } else {
        None
    };

    Ok(json!({
        "ok": true,
        "chain": net.name,
        "chain_id": net.chain_id,
        "address": addr,
        "is_contract": is_contract,
        "bytecode_bytes": bytecode_bytes,
        "native_balance_wei": balance_wei.to_string(),
        "native_balance_formatted": format!("{} {}", format_units_u128(balance_wei, 18, 6), net.native_symbol),
        "is_eip1967_proxy": is_proxy,
        "implementation_address": impl_addr,
        "proxy_admin_address": admin_addr,
        "owner_address": owner_addr,
        "is_ownerless": is_ownerless,
        "sourcify_status": sourcify_status,
        "sourcify_url": format!("https://repo.sourcify.dev/{}/{addr}", net.chain_id),
        "explorer_url": format!("{}{addr}", net.explorer_addr),
        "erc20": erc20,
        "risk_flags": risk_flags
    }))
}

// ---------------------------------------------------------------------------
// Tool 2: Live Bitcoin Fee & Mempool Oracle (Zero API Key)
// ---------------------------------------------------------------------------

pub fn fetch_bitcoin_mempool_oracle(address: Option<&str>) -> Result<Value, String> {
    let agent = http_agent();
    let fetch_json = |url: &str| -> Result<Value, String> {
        let mut resp = agent
            .get(url)
            .header("User-Agent", "newsatoshi-commons/0.12")
            .call()
            .map_err(|e| format!("mempool.space request failed: {e}"))?;
        let status = resp.status().as_u16();
        let body = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("failed reading response: {e}"))?;
        if status >= 400 {
            return Err(format!("mempool.space HTTP {status}: {body}"));
        }
        serde_json::from_str(&body).map_err(|e| format!("invalid JSON from {url}: {e}"))
    };

    let fees = fetch_json("https://mempool.space/api/v1/fees/recommended")?;
    let mempool = fetch_json("https://mempool.space/api/mempool").ok();
    let tip_height: Option<u64> = agent
        .get("https://mempool.space/api/blocks/tip/height")
        .header("User-Agent", "newsatoshi-commons/0.12")
        .call()
        .ok()
        .and_then(|mut r| r.body_mut().read_to_string().ok())
        .and_then(|s| s.trim().parse::<u64>().ok());

    let mut addr_info: Option<Value> = None;
    if let Some(raw_addr) = address.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if raw_addr.len() > 100 || !raw_addr.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err("invalid Bitcoin address format".to_string());
        }
        let stats = fetch_json(&format!("https://mempool.space/api/address/{raw_addr}"))?;
        let funded = stats["chain_stats"]["funded_txo_sum"].as_u64().unwrap_or(0);
        let spent = stats["chain_stats"]["spent_txo_sum"].as_u64().unwrap_or(0);
        let mem_funded = stats["mempool_stats"]["funded_txo_sum"].as_u64().unwrap_or(0);
        let mem_spent = stats["mempool_stats"]["spent_txo_sum"].as_u64().unwrap_or(0);
        let confirmed_sats = funded.saturating_sub(spent);
        let mempool_delta_sats = (mem_funded as i64) - (mem_spent as i64);
        addr_info = Some(json!({
            "address": raw_addr,
            "confirmed_sats": confirmed_sats,
            "confirmed_btc": format_units_u128(confirmed_sats as u128, 8, 8),
            "mempool_delta_sats": mempool_delta_sats,
            "tx_count": stats["chain_stats"]["tx_count"].as_u64().unwrap_or(0),
            "explorer_url": format!("https://mempool.space/address/{raw_addr}")
        }));
    }

    Ok(json!({
        "ok": true,
        "network": "bitcoin_mainnet",
        "source": "mempool.space",
        "tip_height": tip_height,
        "fees_sat_per_vb": {
            "fastest": fees["fastestFee"],
            "half_hour": fees["halfHourFee"],
            "hour": fees["hourFee"],
            "economy": fees["economyFee"],
            "minimum": fees["minimumFee"]
        },
        "mempool": mempool.map(|m| json!({
            "tx_count": m["count"],
            "vsize_bytes": m["vsize"],
            "total_fee_sats": m["total_fee"]
        })),
        "address_lookup": addr_info
    }))
}

// ---------------------------------------------------------------------------
// Tool 3: Offline Lightning BOLT-11 Invoice Decoder (<1ms)
// ---------------------------------------------------------------------------

pub fn decode_bolt11_tool(invoice: &str) -> Result<Value, String> {
    let decoded = decode_bolt11_invoice(invoice)?;
    Ok(json!({
        "ok": true,
        "protocol": "BOLT-11",
        "network": decoded.network,
        "payment_hash_hex": decoded.payment_hash_hex,
        "amount_sats": decoded.amount_sats,
        "amount_msat": decoded.amount_msat,
        "timestamp_unix": decoded.timestamp_unix,
        "expiry_secs": decoded.expiry_secs,
        "description": decoded.description
    }))
}

// ---------------------------------------------------------------------------
// Tool 4: Live Base Mainnet Uniswap V3 QuoterV2 (ETH / USDC <-> wLOVE)
// ---------------------------------------------------------------------------

pub fn quote_base_uniswap_v3(
    from_asset: &str,
    to_asset: &str,
    amount_str: &str,
) -> Result<Value, String> {
    let net = resolve_evm_net("base")?;
    let from = from_asset.trim().to_ascii_uppercase();
    let to = to_asset.trim().to_ascii_uppercase();
    if from == to {
        return Err("from_asset and to_asset must differ".to_string());
    }

    let decimals = |sym: &str| -> Result<u32, String> {
        match sym {
            "ETH" | "WETH" | "WLOVE" => Ok(18),
            "USDC" => Ok(6),
            _ => Err(format!("unsupported asset '{sym}' (supported: ETH, USDC, wLOVE)")),
        }
    };
    let in_dec = decimals(&from)?;
    let out_dec = decimals(&to)?;
    let amount_in = parse_decimal_units(amount_str, in_dec)?;
    if amount_in == 0 {
        return Err("amount must be greater than 0".to_string());
    }

    let (calldata, route_label) = match (from.as_str(), to.as_str()) {
        ("ETH" | "WETH", "WLOVE") => (
            format!(
                "0xc6a5026a{}{}{}{}{}",
                pad_addr_32(BASE_WETH)?,
                pad_addr_32(BASE_WLOVE)?,
                pad_u128_32(amount_in),
                pad_u128_32(10_000),
                pad_u128_32(0)
            ),
            "ETH -> wLOVE (1.00% Base Uniswap V3)",
        ),
        ("WLOVE", "ETH" | "WETH") => (
            format!(
                "0xc6a5026a{}{}{}{}{}",
                pad_addr_32(BASE_WLOVE)?,
                pad_addr_32(BASE_WETH)?,
                pad_u128_32(amount_in),
                pad_u128_32(10_000),
                pad_u128_32(0)
            ),
            "wLOVE -> ETH (1.00% Base Uniswap V3)",
        ),
        ("USDC", "WLOVE") => {
            // 2-hop packed path: USDC(20) + 500(3) + WETH(20) + 10000(3) + wLOVE(20) = 66 bytes (0x42)
            let path = format!(
                "{}0001f4{}002710{}",
                BASE_USDC[2..].to_ascii_lowercase(),
                BASE_WETH[2..].to_ascii_lowercase(),
                BASE_WLOVE[2..].to_ascii_lowercase()
            );
            let padded_path = format!("{path:0<192}");
            (
                format!(
                    "0xcdca1753{}{}{}{}",
                    pad_u128_32(64),
                    pad_u128_32(amount_in),
                    pad_u128_32(66),
                    padded_path
                ),
                "USDC -> WETH (0.05%) -> wLOVE (1.00%) Base Uniswap V3",
            )
        }
        ("WLOVE", "USDC") => {
            let path = format!(
                "{}002710{}0001f4{}",
                BASE_WLOVE[2..].to_ascii_lowercase(),
                BASE_WETH[2..].to_ascii_lowercase(),
                BASE_USDC[2..].to_ascii_lowercase()
            );
            let padded_path = format!("{path:0<192}");
            (
                format!(
                    "0xcdca1753{}{}{}{}",
                    pad_u128_32(64),
                    pad_u128_32(amount_in),
                    pad_u128_32(66),
                    padded_path
                ),
                "wLOVE -> WETH (1.00%) -> USDC (0.05%) Base Uniswap V3",
            )
        }
        _ => {
            return Err(format!(
                "unsupported pair {from} -> {to} (supported pairs: ETH<->wLOVE, USDC<->wLOVE)"
            ))
        }
    };

    let res_hex = evm_rpc_call(
        net,
        "eth_call",
        json!([{ "to": BASE_QUOTER_V2, "data": calldata }, "latest"]),
    )?
    .as_str()
    .unwrap_or("")
    .to_string();
    let clean = res_hex.strip_prefix("0x").unwrap_or(&res_hex);
    if clean.len() < 64 {
        return Err("pool liquidity exceeded for this trade size".to_string());
    }
    let amount_out = decode_u128_hex(&clean[..64]);

    // Live pool reserves snapshot
    let pool_weth = eth_call_hex(net, BASE_WETH, &format!("0x70a08231{}", pad_addr_32(BASE_WLOVE_POOL)?))
        .map(|h| decode_u128_hex(&h))
        .unwrap_or(0);
    let pool_wlove = eth_call_hex(net, BASE_WLOVE, &format!("0x70a08231{}", pad_addr_32(BASE_WLOVE_POOL)?))
        .map(|h| decode_u128_hex(&h))
        .unwrap_or(0);

    Ok(json!({
        "ok": true,
        "chain": "base",
        "chain_id": 8453,
        "from_asset": from,
        "to_asset": to,
        "amount_in": amount_str.trim(),
        "amount_in_raw": amount_in.to_string(),
        "amount_out_formatted": format_units_u128(amount_out, out_dec, if out_dec == 6 { 4 } else { 6 }),
        "amount_out_raw": amount_out.to_string(),
        "route": route_label,
        "swap_router_02": BASE_SWAP_ROUTER,
        "quoter_v2": BASE_QUOTER_V2,
        "wlove_token": BASE_WLOVE,
        "pool_address": BASE_WLOVE_POOL,
        "pool_reserves": {
            "weth": format_units_u128(pool_weth, 18, 6),
            "wlove": format_units_u128(pool_wlove, 18, 2)
        },
        "swap_ui_url": "https://commons.newsatoshi.org/?view=swap"
    }))
}

// ---------------------------------------------------------------------------
// Tool 5: Cross-Chain Transaction Finality & Receipt Verifier
// ---------------------------------------------------------------------------

pub fn verify_tx_receipt(chain: &str, tx_hash: &str) -> Result<Value, String> {
    let key = chain.trim().to_ascii_lowercase();
    let clean_hash = normalize_hash32(tx_hash, "tx_hash")?;

    if key == "bitcoin" || key == "btc" {
        let agent = http_agent();
        let url = format!("https://mempool.space/api/tx/{clean_hash}");
        let mut resp = agent
            .get(&url)
            .header("User-Agent", "newsatoshi-commons/0.12")
            .call()
            .map_err(|e| format!("mempool.space tx query failed: {e}"))?;
        let status_code = resp.status().as_u16();
        if status_code == 404 {
            return Ok(json!({
                "ok": true,
                "chain": "bitcoin",
                "tx_hash": clean_hash,
                "found": false,
                "status": "not_found"
            }));
        }
        let body = resp.body_mut().read_to_string().unwrap_or_default();
        let tx: Value = serde_json::from_str(&body).map_err(|e| format!("invalid tx JSON: {e}"))?;
        let confirmed = tx["status"]["confirmed"].as_bool().unwrap_or(false);
        let block_height = tx["status"]["block_height"].as_u64();
        let tip: Option<u64> = agent
            .get("https://mempool.space/api/blocks/tip/height")
            .header("User-Agent", "newsatoshi-commons/0.12")
            .call()
            .ok()
            .and_then(|mut r| r.body_mut().read_to_string().ok())
            .and_then(|s| s.trim().parse::<u64>().ok());
        let confirmations = match (block_height, tip) {
            (Some(bh), Some(t)) if t >= bh => Some(t - bh + 1),
            _ => Some(0),
        };
        return Ok(json!({
            "ok": true,
            "chain": "bitcoin",
            "tx_hash": clean_hash,
            "found": true,
            "status": if confirmed { "confirmed" } else { "mempool_unconfirmed" },
            "block_number": block_height,
            "confirmations": confirmations,
            "fee_sats": tx["fee"],
            "vsize": tx["weight"].as_u64().map(|w| w.div_ceil(4)),
            "explorer_url": format!("https://mempool.space/tx/{clean_hash}")
        }));
    }

    let net = resolve_evm_net(chain)?;
    let prefixed = format!("0x{clean_hash}");
    let receipt = evm_rpc_call(net, "eth_getTransactionReceipt", json!([&prefixed]))?;
    if receipt.is_null() {
        return Ok(json!({
            "ok": true,
            "chain": net.name,
            "chain_id": net.chain_id,
            "tx_hash": prefixed,
            "found": false,
            "status": "not_found_or_pending",
            "explorer_url": format!("{}{}", net.explorer_tx, prefixed)
        }));
    }

    let block_num = receipt["blockNumber"].as_str().map(decode_u128_hex).unwrap_or(0) as u64;
    let tip_num = evm_rpc_call(net, "eth_blockNumber", json!([]))
        .ok()
        .and_then(|v| v.as_str().map(decode_u128_hex))
        .unwrap_or(block_num as u128) as u64;
    let confirmations = if tip_num >= block_num {
        tip_num - block_num + 1
    } else {
        1
    };
    let status_hex = receipt["status"].as_str().unwrap_or("0x0");
    let status_str = if status_hex == "0x1" {
        "confirmed"
    } else {
        "reverted"
    };
    let gas_used = receipt["gasUsed"].as_str().map(decode_u128_hex).unwrap_or(0) as u64;

    Ok(json!({
        "ok": true,
        "chain": net.name,
        "chain_id": net.chain_id,
        "tx_hash": prefixed,
        "found": true,
        "status": status_str,
        "block_number": block_num,
        "confirmations": confirmations,
        "from": receipt["from"],
        "to": receipt["to"],
        "gas_used": gas_used,
        "logs_count": receipt["logs"].as_array().map(|a| a.len()).unwrap_or(0),
        "explorer_url": format!("{}{}", net.explorer_tx, prefixed)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_format_decimal_units_exact() {
        assert_eq!(parse_decimal_units("0.001", 18).unwrap(), 1_000_000_000_000_000);
        assert_eq!(parse_decimal_units("1.5", 6).unwrap(), 1_500_000);
        assert_eq!(format_units_u128(1_500_000, 6, 4), "1.5");
        assert_eq!(format_units_u128(1_000_000_000_000_000, 18, 6), "0.001");
    }

    #[test]
    fn abi_string_and_address_decoders_work() {
        let word = "0x0000000000000000000000000a3ccfb3ff981a22a264aa05cd8b77d06c2732a3";
        assert_eq!(
            decode_abi_address(word).as_deref(),
            Some("0x0a3ccfb3ff981a22a264aa05cd8b77d06c2732a3")
        );
        let zero_word = "0x0000000000000000000000000000000000000000000000000000000000000000";
        assert_eq!(decode_abi_address(zero_word), None);

        // ABI-encoded string "wLOVE"
        let abi_str = format!(
            "0x{:064x}{:064x}{:0<64}",
            32,
            5,
            hex::encode("wLOVE")
        );
        assert_eq!(decode_abi_string(&abi_str).as_deref(), Some("wLOVE"));
    }

    #[test]
    fn resolve_chains_and_validation() {
        assert_eq!(resolve_evm_net("base").unwrap().chain_id, 8453);
        assert_eq!(resolve_evm_net("eth").unwrap().chain_id, 1);
        assert_eq!(resolve_evm_net("polygon").unwrap().chain_id, 137);
        assert!(resolve_evm_net("unknown-chain").is_err());
        assert!(normalize_evm_addr("not-an-address").is_err());
    }
}
