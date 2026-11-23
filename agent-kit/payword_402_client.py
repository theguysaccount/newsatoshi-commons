#!/usr/bin/env python3
"""
Zero-dependency AI Agent Transaction Client for Newsatoshi (v0.6.0+)

Demonstrates real, end-to-end machine-to-machine onboarding & payment verification:
  1. MCP Streamable HTTP handshake (`initialize`, `tools/list`, `tools/call`)
  2. HTTP `402 Payment Required` dual-rail challenge fetch (`/api/agent/402-challenge`)
  3. Post-quantum pure-SHA-256 PayWord hash-chain generation (`tip = SHA256^N(seed)`)
     and cryptographic verification of tick #k (`POST /api/agent/402-verify` + MCP)
"""

import argparse
import hashlib
import json
import os
import sys
import urllib.error
import urllib.request
from typing import Any, Dict, List, Tuple


def http_json(url: str, payload: Any = None, headers: Dict[str, str] = None) -> Tuple[int, Any]:
    hdrs = {"Accept": "application/json", "User-Agent": "newsatoshi-agent-client/0.6.0"}
    if headers:
        hdrs.update(headers)
    data = None
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
        hdrs["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=hdrs)
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            return resp.status, json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        try:
            return exc.code, json.loads(body)
        except Exception:
            return exc.code, {"raw": body}


def generate_payword_chain(max_ticks: int) -> Tuple[str, List[str]]:
    """Generate a CSPRNG 32-byte seed and compute SHA-256 hash chain w_0..w_N (tip = w_N)."""
    cur = os.urandom(32)
    chain = [cur.hex()]
    for _ in range(max_ticks):
        cur = hashlib.sha256(cur).digest()
        chain.append(cur.hex())
    return chain[-1], chain


def main() -> int:
    p = argparse.ArgumentParser(description="Newsatoshi AI Agent MCP + HTTP 402 PayWord Client")
    p.add_argument("--base-url", default="https://commons.newsatoshi.org")
    p.add_argument("--ticks", type=int, default=5, help="PayWord tick index to pay and verify")
    p.add_argument("--chain-len", type=int, default=100, help="Total pre-committed PayWord ticks")
    args = p.parse_args()
    base = args.base_url.rstrip("/")

    # 1. MCP Initialize
    status, init_res = http_json(
        f"{base}/mcp",
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
    )
    if status != 200 or "result" not in init_res:
        print(f"MCP initialize failed (HTTP {status}): {init_res}", file=sys.stderr)
        return 1
    info = init_res["result"]["serverInfo"]
    print(f"[1/4] MCP Connected: {info['name']} v{info['version']} ({init_res['result']['protocolVersion']})")

    # 2. MCP Tools List
    _, tools_res = http_json(
        f"{base}/mcp",
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
    )
    tool_names = [t["name"] for t in tools_res["result"]["tools"]]
    print(f"[2/4] Discovered {len(tool_names)} MCP Tools: {', '.join(tool_names)}")

    # 3. HTTP 402 Challenge
    code_402, challenge = http_json(
        f"{base}/api/agent/402-challenge?resource=/api/inference&amount_love=0.01&network=mainnet"
    )
    assert code_402 == 402, f"expected HTTP 402, got {code_402}"
    print(f"[3/4] Received HTTP {code_402} Challenge: payment_hash={challenge['payment_hash_hex'][:16]}...")

    # 4. Generate local PayWord hash-chain & verify tick #k via REST + MCP
    tip_hex, chain = generate_payword_chain(args.chain_len)
    token_hex = chain[args.chain_len - args.ticks]
    v_status, v_res = http_json(
        f"{base}/api/agent/402-verify",
        {
            "tip_hex": tip_hex,
            "token_hex": token_hex,
            "ticks": args.ticks,
            "unit_price_base_units": 1_000,
        },
    )
    assert v_status == 200 and v_res.get("valid") is True, f"PayWord verify failed: {v_res}"
    print(
        f"[4/4] Verified PayWord 402 Payment: valid={v_res['valid']} "
        f"ticks={v_res['ticks']} paid={v_res['total_paid_base_units']} base_units "
        f"next_required_ticks={v_res['next_required_ticks']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
