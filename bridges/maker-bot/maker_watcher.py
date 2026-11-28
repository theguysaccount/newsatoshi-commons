#!/usr/bin/env python3
"""
Newsatoshi Multi-Chain Trustless Maker & Miner Liquidity Watcher (`v0.31.0`)
Supports: Newsatoshi Mainnet v2 (`nsM2`, `v1.2.5`) + Bitcoin + Lightning + Solana +
          4-Chain EVM HTLCs (Base, Optimism, Arbitrum, Polygon) +
          3-Chain EVM PayWord Channels (Base, Optimism, Arbitrum).

Zero-dependency Python 3 watcher that lets Mainnet v2 AuxPoW miners (50 LOVE/block)
and cross-chain liquidity makers:
  1. Monitor open SHA-256 HTLC swaps across Newsatoshi L1 + EVM + Solana (`--watch` supported).
  2. Inspect live `NewsatoshiPayWordChannel` settlement state across Base, Optimism, and Arbitrum.
  3. Verify `SHA256(preimage) == payment_hash` and emit exact non-custodial `newsatoshi unlock`
     and `cast send claim` settlement commands (`--emit-commands`).

Safety Invariant:
  - Runs strictly read-only (zero private keys loaded, zero automatic spends).
  - Zero fabricated prices or balances; only inspects live on-chain RPC responses.
"""

import argparse
import hashlib
import hmac
import json
import time
import urllib.request
from typing import Any, Dict, Optional

DEFAULT_EVM_RPCS = {
    "base": "https://mainnet.base.org",
    "optimism": "https://mainnet.optimism.io",
    "arbitrum": "https://arb1.arbitrum.io/rpc",
    "polygon": "https://polygon-bor-rpc.publicnode.com",
    "ethereum": "https://ethereum-rpc.publicnode.com",
}

CANONICAL_EVM_HTLC = "0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987"
DEFAULT_EVM_HTLC_CONTRACTS = {
    # All 4 live & Sourcify exact_match at identical nonce-0 address
    "base": CANONICAL_EVM_HTLC,
    "optimism": CANONICAL_EVM_HTLC,
    "arbitrum": CANONICAL_EVM_HTLC,
    "polygon": CANONICAL_EVM_HTLC,
}

DEFAULT_EVM_PAYWORD_CONTRACTS = {
    # All 3 live & Sourcify exact_match (0% protocol fee, ownerless)
    "base": "0x971BA814Df91c20DcA3aDd870C2B55d643965C50",
    "optimism": "0x3DeBB6d5ae490e6cF4F966585e000028111CF106",
    "arbitrum": "0x3DeBB6d5ae490e6cF4F966585e000028111CF106",
}

DEFAULT_SOLANA_RPCS = {
    "mainnet-beta": "https://api.mainnet-beta.solana.com",
    "devnet": "https://api.devnet.solana.com",
}


def _validate_hex(value: str, expected_bytes: int, field_name: str) -> str:
    clean = value.strip().lower().removeprefix("0x")
    if len(clean) != expected_bytes * 2:
        raise ValueError(f"{field_name} must be {expected_bytes * 2} hex chars")
    try:
        bytes.fromhex(clean)
    except ValueError as exc:
        raise ValueError(f"{field_name} contains invalid hex") from exc
    return clean


def http_json(url: str, payload: Optional[Dict[str, Any]] = None, timeout: float = 10.0) -> Any:
    if not url.startswith(("https://", "http://")):
        raise ValueError("RPC URL must use https:// or http:// scheme")
    data = json.dumps(payload).encode("utf-8") if payload is not None else None
    headers = {"Accept": "application/json", "User-Agent": "newsatoshi-maker-watcher/0.31.0"}
    if data is not None:
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers)
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read(2_097_152).decode("utf-8"))


def check_newsatoshi_lock(payment_hash: str, network: str = "mainnet-v2") -> Dict[str, Any]:
    clean_hash = _validate_hex(payment_hash, 32, "payment_hash")
    base = (
        "https://newsatoshi.org/mainnet"
        if network in ("mainnet", "mainnet-v2")
        else "https://newsatoshi-seed.fly.dev"
    )
    url = f"{base}/api/lock?hash={clean_hash}"
    res = http_json(url)
    locks = res.get("locks", [])
    matched = next((l for l in locks if l.get("found") and l.get("confirmed")), None) or next(
        (l for l in locks if l.get("found")), None
    )
    return {
        "network": network,
        "endpoint": url,
        "found": matched is not None,
        "confirmed": bool(matched and matched.get("confirmed")),
        "lock": matched,
    }


def check_evm_swap_status(rpc_url: str, contract_addr: str, swap_id_hex: str) -> Dict[str, Any]:
    """Read `swaps(bytes32)` directly via standard Ethereum `eth_call` (selector `0xeb84e7f2`)."""
    clean_addr = "0x" + _validate_hex(contract_addr, 20, "contract_addr")
    clean_id = _validate_hex(swap_id_hex, 32, "swap_id")
    calldata = "0xeb84e7f2" + clean_id
    res = http_json(
        rpc_url,
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_call",
            "params": [{"to": clean_addr, "data": calldata}, "latest"],
        },
    )
    raw = (res.get("result") or "0x").removeprefix("0x")
    if len(raw) < 64 * 9:
        return {"found": False, "rpc_url": rpc_url, "raw_len": len(raw)}
    sender = "0x" + raw[24:64]
    recipient = "0x" + raw[88:128]
    token = "0x" + raw[152:192]
    amount = int(raw[192:256], 16)
    payment_hash = raw[256:320]
    timelock = int(raw[320:384], 16)
    claimed = int(raw[384:448], 16) != 0
    refunded = int(raw[448:512], 16) != 0
    preimage = raw[512:576]
    found = sender != "0x" + ("0" * 40)
    return {
        "found": found,
        "contract": clean_addr,
        "sender": sender,
        "recipient": recipient,
        "token": token,
        "amount_raw": amount,
        "payment_hash_hex": payment_hash,
        "timelock_unix": timelock,
        "claimed": claimed,
        "refunded": refunded,
        "preimage_hex": preimage if claimed else None,
    }


def check_evm_payword_channel(rpc_url: str, contract_addr: str, channel_id_hex: str) -> Dict[str, Any]:
    """Read `channels(bytes32)` directly via standard `eth_call` (selector `0x7a7ebd7b`)."""
    # keccak256("channels(bytes32)")[:4] = 0x7a7ebd7b
    clean_addr = "0x" + _validate_hex(contract_addr, 20, "contract_addr")
    clean_id = _validate_hex(channel_id_hex, 32, "channel_id")
    res = http_json(
        rpc_url,
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_call",
            "params": [{"to": clean_addr, "data": "0x7a7ebd7b" + clean_id}, "latest"],
        },
    )
    raw = (res.get("result") or "0x").removeprefix("0x")
    if len(raw) < 64 * 10:
        return {"found": False, "rpc_url": rpc_url}
    payer = "0x" + raw[24:64]
    recipient = "0x" + raw[88:128]
    token = "0x" + raw[152:192]
    initial_root = "0x" + raw[192:256]
    current_root = "0x" + raw[256:320]
    unit_amount = int(raw[320:384], 16)
    max_steps = int(raw[384:448], 16)
    claimed_steps = int(raw[448:512], 16)
    expiry = int(raw[512:576], 16)
    closed = int(raw[576:640], 16) != 0
    return {
        "found": payer != "0x" + ("0" * 40),
        "contract": clean_addr,
        "payer": payer,
        "recipient": recipient,
        "token": token,
        "initial_root": initial_root,
        "current_root": current_root,
        "unit_amount_raw": unit_amount,
        "max_steps": max_steps,
        "claimed_steps": claimed_steps,
        "remaining_steps": max(0, max_steps - claimed_steps),
        "expiry_unix": expiry,
        "closed": closed,
    }


def check_solana_escrow_account(rpc_url: str, escrow_pda: str) -> Dict[str, Any]:
    """Query a deterministic Solana HTLC PDA via standard JSON-RPC `getAccountInfo`."""
    res = http_json(
        rpc_url,
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [escrow_pda, {"encoding": "base64"}],
        },
    )
    val = (res.get("result") or {}).get("value")
    if val is None:
        return {"found": False, "escrow_pda": escrow_pda, "rpc_url": rpc_url}
    return {
        "found": True,
        "escrow_pda": escrow_pda,
        "lamports": val.get("lamports", 0),
        "owner": val.get("owner"),
        "space": val.get("space"),
    }


def verify_preimage(payment_hash_hex: str, preimage_hex: str) -> bool:
    clean_hash = _validate_hex(payment_hash_hex, 32, "payment_hash")
    clean_preimage = _validate_hex(preimage_hex, 32, "preimage")
    digest = hashlib.sha256(bytes.fromhex(clean_preimage)).hexdigest()
    return hmac.compare_digest(digest, clean_hash)


def run_once(args: argparse.Namespace) -> Dict[str, Any]:
    report: Dict[str, Any] = {}
    evm_chain = args.evm_chain or "base"
    rpc = args.evm_rpc or DEFAULT_EVM_RPCS.get(evm_chain)

    if args.hash:
        h = args.hash.lower().removeprefix("0x")
        report["payment_hash_hex"] = h
        report["newsatoshi"] = check_newsatoshi_lock(h, args.love_net)

    evm_contract = args.evm_contract or DEFAULT_EVM_HTLC_CONTRACTS.get(evm_chain)
    if evm_contract and args.evm_swap_id and rpc:
        report["evm_htlc"] = check_evm_swap_status(rpc, evm_contract, args.evm_swap_id)

    if args.payword_channel_id and rpc:
        pw_contract = args.payword_contract or DEFAULT_EVM_PAYWORD_CONTRACTS.get(evm_chain)
        if pw_contract:
            report["evm_payword_channel"] = check_evm_payword_channel(
                rpc, pw_contract, args.payword_channel_id
            )

    if args.solana_pda:
        sol_rpc = args.solana_rpc or DEFAULT_SOLANA_RPCS.get(args.solana_cluster or "mainnet-beta")
        report["solana"] = check_solana_escrow_account(sol_rpc, args.solana_pda)

    revealed_preimage = args.preimage or (report.get("evm_htlc") or {}).get("preimage_hex")
    if args.hash and revealed_preimage:
        report["preimage_valid"] = verify_preimage(args.hash, revealed_preimage)
        if args.emit_commands and report["preimage_valid"]:
            cmds = {}
            lock_id = ((report.get("newsatoshi") or {}).get("lock") or {}).get("id")
            if lock_id:
                cmds["newsatoshi_unlock"] = (
                    f"newsatoshi --net {args.love_net} unlock {lock_id} {revealed_preimage.removeprefix('0x')}"
                )
            if args.evm_swap_id and evm_contract and rpc:
                cmds["evm_htlc_claim"] = (
                    f'cast send {evm_contract} "claim(bytes32,bytes32)" {args.evm_swap_id} '
                    f'0x{revealed_preimage.removeprefix("0x")} --rpc-url {rpc} --private-key $KEY'
                )
            report["settlement_commands"] = cmds
    return report


def main() -> int:
    p = argparse.ArgumentParser(description="Newsatoshi Multi-Chain HTLC & PayWords Maker Watcher (v0.31.0)")
    p.add_argument("--hash", help="64-hex SHA-256 payment_hash (for HTLC swap monitoring)")
    p.add_argument("--love-net", default="mainnet-v2", choices=["mainnet-v2", "mainnet", "testnet"])
    p.add_argument("--evm-chain", default="base", choices=list(DEFAULT_EVM_RPCS.keys()))
    p.add_argument("--evm-rpc")
    p.add_argument("--evm-contract", help="Override NewsatoshiHtlc contract address")
    p.add_argument("--evm-swap-id", help="0x-prefixed 32-byte EVM HTLC swapId")
    p.add_argument("--payword-channel-id", help="0x-prefixed 32-byte NewsatoshiPayWordChannel ID to inspect")
    p.add_argument("--payword-contract", help="Override NewsatoshiPayWordChannel contract address")
    p.add_argument("--solana-cluster", choices=list(DEFAULT_SOLANA_RPCS.keys()))
    p.add_argument("--solana-rpc")
    p.add_argument("--solana-pda")
    p.add_argument("--preimage", help="Optional 64-hex preimage to verify")
    p.add_argument("--emit-commands", action="store_true", help="Emit exact claim/unlock CLI commands when preimage matches")
    p.add_argument("--watch", type=float, default=0.0, help="Poll every N seconds until interrupted")
    args = p.parse_args()

    if not any([args.hash, args.evm_swap_id, args.payword_channel_id, args.solana_pda]):
        p.error("provide at least one of --hash, --evm-swap-id, --payword-channel-id, or --solana-pda")

    while True:
        print(json.dumps(run_once(args), indent=2), flush=True)
        if args.watch <= 0:
            return 0
        time.sleep(args.watch)


if __name__ == "__main__":
    raise SystemExit(main())
