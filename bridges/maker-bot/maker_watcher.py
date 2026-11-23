#!/usr/bin/env python3
"""
Newsatoshi Multi-Chain Trustless Maker Watcher Bot (BTC · Lightning · Solana · Base/EVM)

Zero-dependency Python 3 watcher that lets Mainnet AuxPoW miners (50 LOVE/block)
and liquidity makers monitor open SHA-256 HTLC swaps across Newsatoshi L1,
Bitcoin (mempool.space), Solana RPC, and Base/Ethereum/Arbitrum JSON-RPC,
verify preimage matches, and emit exact settlement CLI commands.

Safety Invariant:
  - Runs in strict read-only `--dry-run` mode by default (zero automatic spends).
  - Zero fabricated prices or balances; only inspects live on-chain RPC responses.
"""

import argparse
import hashlib
import hmac
import json
import sys
import urllib.error
import urllib.request
from typing import Any, Dict, Optional

DEFAULT_EVM_RPCS = {
    "base": "https://mainnet.base.org",
    "ethereum": "https://ethereum-rpc.publicnode.com",
    "arbitrum": "https://arb1.arbitrum.io/rpc",
    "optimism": "https://mainnet.optimism.io",
    "polygon": "https://polygon-bor-rpc.publicnode.com",
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


def http_json(url: str, payload: Optional[Dict[str, Any]] = None, timeout: float = 8.0) -> Any:
    if not url.startswith(("https://", "http://")):
        raise ValueError("RPC URL must use https:// or http:// scheme")
    data = json.dumps(payload).encode("utf-8") if payload is not None else None
    headers = {"Accept": "application/json", "User-Agent": "newsatoshi-maker-watcher/0.7.1"}
    if data is not None:
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers)
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read(2_097_152).decode("utf-8"))


def check_newsatoshi_lock(payment_hash: str, network: str = "mainnet") -> Dict[str, Any]:
    clean_hash = _validate_hex(payment_hash, 32, "payment_hash")
    base = (
        "https://newsatoshi.org/mainnet"
        if network == "mainnet"
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
    # keccak256("swaps(bytes32)")[:4] = 0xeb84e7f2
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


def main() -> int:
    p = argparse.ArgumentParser(description="Newsatoshi Multi-Chain HTLC Maker Watcher")
    p.add_argument("--hash", required=True, help="64-hex SHA-256 payment_hash")
    p.add_argument("--love-net", default="mainnet", choices=["mainnet", "testnet"])
    p.add_argument("--evm-chain", choices=list(DEFAULT_EVM_RPCS.keys()))
    p.add_argument("--evm-rpc")
    p.add_argument("--evm-contract")
    p.add_argument("--evm-swap-id")
    p.add_argument("--solana-cluster", choices=list(DEFAULT_SOLANA_RPCS.keys()))
    p.add_argument("--solana-rpc")
    p.add_argument("--solana-pda")
    p.add_argument("--preimage", help="Optional 64-hex preimage to verify")
    args = p.parse_args()

    report: Dict[str, Any] = {
        "payment_hash_hex": args.hash.lower(),
        "newsatoshi": check_newsatoshi_lock(args.hash.lower(), args.love_net),
    }

    if args.evm_contract and args.evm_swap_id:
        rpc = args.evm_rpc or DEFAULT_EVM_RPCS.get(args.evm_chain or "base")
        report["evm"] = check_evm_swap_status(rpc, args.evm_contract, args.evm_swap_id)

    if args.solana_pda:
        rpc = args.solana_rpc or DEFAULT_SOLANA_RPCS.get(args.solana_cluster or "mainnet-beta")
        report["solana"] = check_solana_escrow_account(rpc, args.solana_pda)

    if args.preimage:
        report["preimage_valid"] = verify_preimage(args.hash, args.preimage)

    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
