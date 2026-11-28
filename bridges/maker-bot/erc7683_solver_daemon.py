#!/usr/bin/env python3
"""
Newsatoshi Zero-Custody ERC-7683 & Multi-Rail Preimage Solver Daemon (`v0.37.0`)

Strict Safety & Organic Price-Discovery Invariants:
  1. ZERO private keys loaded, ZERO automatic signing or custody.
  2. Enforces seller-defined `min_reserve_price_usd` per LOVE so solvers never dump LOVE
     below reserve price on thin pools.
  3. Enforces anti-race timelock ordering (`fill_deadline >= open_deadline + min_margin_secs`).
  4. Constant-time `SHA256(32-byte preimage) == payment_hash` verification across all rails.
  5. Emits exact, non-custodial settlement commands for:
     - EVM (`NewsatoshiErc7683Settler.claimWithPreimage` & `NewsatoshiHtlc.claim`)
     - Cosmos IBC (`wasmd tx wasm execute ... claim_htlc`)
     - Sui Move (`sui client call ... claim_with_preimage`)
     - Aptos Move (`aptos move run ... claim_coin`)
     - Newsatoshi Mainnet v2 (`newsatoshi --net mainnet-v2 claim`)
"""

import argparse
import hashlib
import hmac
import json
import sys
import urllib.request
from dataclasses import dataclass
from typing import Any, Dict, List, Optional


DEFAULT_COMMONS_URL = "https://commons.newsatoshi.org"
MIN_TIMELOCK_MARGIN_SECS = 3600


@dataclass(frozen=True)
class SolverPolicy:
    min_reserve_price_usd: float
    min_timelock_margin_secs: int = MIN_TIMELOCK_MARGIN_SECS
    allowed_origin_chains: tuple = ("ethereum", "base", "optimism", "arbitrum", "polygon")


def _clean_hex32(raw: str, field: str) -> str:
    clean = raw.strip().lower().removeprefix("0x")
    if len(clean) != 64:
        raise ValueError(f"{field} must be 32 bytes (64 hex characters)")
    bytes.fromhex(clean)
    return clean


def verify_preimage_constant_time(preimage_hex: str, payment_hash_hex: str) -> bool:
    """Returns True iff SHA256(32-byte preimage) == payment_hash using constant-time comparison."""
    pre_clean = _clean_hex32(preimage_hex, "preimage_hex")
    hash_clean = _clean_hex32(payment_hash_hex, "payment_hash_hex")
    derived = hashlib.sha256(bytes.fromhex(pre_clean)).hexdigest()
    return hmac.compare_digest(derived, hash_clean)


def evaluate_erc7683_order(order_payload: Dict[str, Any], policy: SolverPolicy) -> Dict[str, Any]:
    """
    Validates an ERC-7683 cross-chain intent compiled by `/api/v2/interop/erc7683-intent`
    against the solver's reserve-price floor and anti-race timelock policy.
    """
    reasons: List[str] = []
    origin = str(order_payload.get("origin_chain", "")).lower()
    if origin not in policy.allowed_origin_chains:
        reasons.append(f"origin_chain '{origin}' not in solver allowlist")

    msg = ((order_payload.get("eip712") or {}).get("typed_data") or {}).get("message") or {}
    open_dl = int(msg.get("openDeadline") or 0)
    fill_dl = int(msg.get("fillDeadline") or 0)
    if fill_dl - open_dl < policy.min_timelock_margin_secs:
        reasons.append(
            f"timelock margin {fill_dl - open_dl}s < required {policy.min_timelock_margin_secs}s"
        )

    decoded = order_payload.get("order_data_decoded") or {}
    in_raw = int(decoded.get("inputAmount") or 0)
    out_sats = int(decoded.get("outputAmountSats") or 0)
    if in_raw <= 0 or out_sats <= 0:
        reasons.append("inputAmount and outputAmountSats must both be > 0")
        implied_usd_per_love = 0.0
    else:
        # USDC 6 decimals -> USD; LOVE 8 decimals -> LOVE
        usd_in = in_raw / 1_000_000.0
        love_out = out_sats / 100_000_000.0
        implied_usd_per_love = round(usd_in / love_out, 6)
        if implied_usd_per_love < policy.min_reserve_price_usd:
            reasons.append(
                f"implied price ${implied_usd_per_love:.4f}/LOVE < reserve floor ${policy.min_reserve_price_usd:.4f}/LOVE"
            )

    payment_hash = str(order_payload.get("payment_hash_sha256") or "").removeprefix("0x")
    try:
        payment_hash = _clean_hex32(payment_hash, "payment_hash_sha256")
    except ValueError as exc:
        reasons.append(str(exc))

    return {
        "fillable": len(reasons) == 0,
        "origin_chain": origin,
        "implied_usd_per_love": implied_usd_per_love,
        "min_reserve_price_usd": policy.min_reserve_price_usd,
        "timelock_margin_secs": fill_dl - open_dl,
        "payment_hash_hex": payment_hash,
        "rejection_reasons": reasons,
    }


def emit_multirail_claim_commands(
    preimage_hex: str,
    payment_hash_hex: str,
    order_id_hex: str,
    settler_address: str = "0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987",
    cosmwasm_contract: str = "<COSMWASM_HTLC_ADDR>",
    sui_package: str = "<SUI_INTEROP_PKG>",
    aptos_module: str = "<APTOS_ADDR>::htlc",
) -> Dict[str, Any]:
    """Verifies preimage match and emits ready-to-sign claim commands for every supported ecosystem."""
    if not verify_preimage_constant_time(preimage_hex, payment_hash_hex):
        raise ValueError("INVALID_PREIMAGE: SHA256(preimage) does not match payment_hash")
    pre = _clean_hex32(preimage_hex, "preimage_hex")
    ph = _clean_hex32(payment_hash_hex, "payment_hash_hex")
    oid = _clean_hex32(order_id_hex, "order_id_hex")
    return {
        "verified": True,
        "payment_hash_hex": ph,
        "preimage_hex": pre,
        "commands": {
            "evm_erc7683_settler": (
                f'cast send {settler_address} "claimWithPreimage(bytes32,bytes32)" 0x{oid} 0x{pre}'
            ),
            "evm_newsatoshi_htlc": (
                f'cast send 0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987 "claim(bytes32,bytes32)" 0x{oid} 0x{pre}'
            ),
            "newsatoshi_mainnet_v2": (
                f"newsatoshi --net mainnet-v2 claim --hash {ph} --preimage {pre}"
            ),
            "cosmos_ibc_cosmwasm": (
                f"wasmd tx wasm execute {cosmwasm_contract} "
                f'\'{{"claim_htlc":{{"payment_hash_hex":"{ph}","preimage_hex":"{pre}"}}}}\''
            ),
            "sui_move": (
                f"sui client call --package {sui_package} --module htlc "
                f"--function claim_with_preimage --args <ESCROW_OBJ_ID> 0x{pre}"
            ),
            "aptos_move": (
                f"aptos move run --function-id {aptos_module}::claim_coin "
                f"--args address:<VAULT_OWNER> hex:0x{pre}"
            ),
        },
    }


def fetch_live_matrix(commons_url: str = DEFAULT_COMMONS_URL) -> Dict[str, Any]:
    url = f"{commons_url.rstrip('/')}/api/v2/interop/matrix"
    req = urllib.request.Request(url, headers={"Accept": "application/json", "User-Agent": "newsatoshi-erc7683-solver/0.37.0"})
    with urllib.request.urlopen(req, timeout=15) as resp:
        return json.loads(resp.read().decode("utf-8"))


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Newsatoshi Zero-Custody ERC-7683 & Multi-Rail Preimage Solver Daemon")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_mat = sub.add_parser("matrix", help="Probe live 6-chain interoperability RPC matrix")
    p_mat.add_argument("--commons-url", default=DEFAULT_COMMONS_URL)

    p_eval = sub.add_parser("evaluate", help="Evaluate an ERC-7683 intent JSON file against reserve floor policy")
    p_eval.add_argument("--order-json", required=True)
    p_eval.add_argument("--min-reserve-price-usd", type=float, required=True)

    p_claim = sub.add_parser("emit-claims", help="Verify preimage and emit non-custodial claim commands for all rails")
    p_claim.add_argument("--preimage", required=True)
    p_claim.add_argument("--payment-hash", required=True)
    p_claim.add_argument("--order-id", required=True)

    args = parser.parse_args(argv)
    if args.cmd == "matrix":
        print(json.dumps(fetch_live_matrix(args.commons_url), indent=2))
        return 0
    if args.cmd == "evaluate":
        with open(args.order_json, "r", encoding="utf-8") as fh:
            order_data = json.load(fh)
        policy = SolverPolicy(min_reserve_price_usd=args.min_reserve_price_usd)
        res = evaluate_erc7683_order(order_data, policy)
        print(json.dumps(res, indent=2))
        return 0 if res["fillable"] else 2
    if args.cmd == "emit-claims":
        res = emit_multirail_claim_commands(args.preimage, args.payment_hash, args.order_id)
        print(json.dumps(res, indent=2))
        return 0
    return 1


if __name__ == "__main__":
    sys.exit(main())
