#!/usr/bin/env python3
"""
newsatoshi_402 — Zero-Dependency Post-Quantum HTTP 402 PayWord & MCP Python SDK.

Enables any Python AI agent or API server (FastAPI, Flask, Django, Starlette, stdlib)
to pay or charge per request in Newsatoshi (LOVE) using pure-SHA-256 PayWord hash chains
with monotonic anti-replay watermarks, or call https://commons.newsatoshi.org/mcp directly.
"""

import hashlib
import hmac
import json
import os
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Tuple

DEFAULT_COMMONS_URL = "https://commons.newsatoshi.org"
MAX_PAYWORD_TICKS = 100_000


def _validate_hex32(val: str) -> bytes:
    clean = val.strip().lower().removeprefix("0x")
    if len(clean) != 64:
        raise ValueError("expected 64-character hex digest (32 bytes)")
    return bytes.fromhex(clean)


@dataclass
class PayWordWallet:
    tip_hex: str
    max_ticks: int
    unit_price_base_units: int
    _chain: List[bytes] = field(repr=False)
    spent_ticks: int = 0

    @property
    def remaining_ticks(self) -> int:
        return self.max_ticks - self.spent_ticks

    def next_payment(self, ticks_increment: int = 1) -> Dict[str, Any]:
        if ticks_increment < 1:
            raise ValueError("ticks_increment must be >= 1")
        if self.spent_ticks + ticks_increment > self.max_ticks:
            raise RuntimeError("PayWord channel exhausted")
        self.spent_ticks += ticks_increment
        token_hex = self._chain[self.max_ticks - self.spent_ticks].hex()
        return {
            "tip_hex": self.tip_hex,
            "token_hex": token_hex,
            "ticks": self.spent_ticks,
            "total_paid_base_units": self.spent_ticks * self.unit_price_base_units,
            "header_value": f"tip={self.tip_hex},token={token_hex},ticks={self.spent_ticks}",
        }


def generate_payword_chain(max_ticks: int, unit_price_base_units: int = 1_000) -> PayWordWallet:
    if not (1 <= max_ticks <= MAX_PAYWORD_TICKS):
        raise ValueError(f"max_ticks must be in 1..={MAX_PAYWORD_TICKS}")
    if unit_price_base_units < 1:
        raise ValueError("unit_price_base_units must be >= 1")
    seed = os.urandom(32)
    chain = [seed]
    cur = seed
    for _ in range(max_ticks):
        cur = hashlib.sha256(cur).digest()
        chain.append(cur)
    return PayWordWallet(
        tip_hex=chain[-1].hex(),
        max_ticks=max_ticks,
        unit_price_base_units=unit_price_base_units,
        _chain=chain,
    )


def verify_payword_token(
    tip_hex: str, token_hex: str, ticks: int, unit_price_base_units: int = 1_000
) -> Dict[str, Any]:
    if not isinstance(ticks, int) or not (1 <= ticks <= MAX_PAYWORD_TICKS):
        return {"valid": False, "reason": f"ticks must be in 1..={MAX_PAYWORD_TICKS}"}
    try:
        tip_bytes = _validate_hex32(tip_hex)
        cur = _validate_hex32(token_hex)
    except ValueError as exc:
        return {"valid": False, "reason": str(exc)}
    for _ in range(ticks):
        cur = hashlib.sha256(cur).digest()
    valid = hmac.compare_digest(cur, tip_bytes)
    return {
        "valid": valid,
        "ticks": ticks,
        "total_paid_base_units": ticks * unit_price_base_units if valid else 0,
        "computed_tip_hex": cur.hex(),
        "reason": None if valid else "SHA256^ticks(token) did not match committed tip",
    }


def parse_payword_header(header_value: Optional[str]) -> Optional[Tuple[str, str, int]]:
    if not header_value or not isinstance(header_value, str):
        return None
    fields: Dict[str, str] = {}
    for part in header_value.split(","):
        if "=" in part:
            k, v = part.split("=", 1)
            fields[k.strip()] = v.strip()
    if not all(k in fields for k in ("tip", "token", "ticks")):
        return None
    try:
        ticks = int(fields["ticks"])
    except ValueError:
        return None
    if ticks < 1:
        return None
    return fields["tip"], fields["token"], ticks


class PayWordGate:
    """Monotonic anti-replay HTTP 402 PayWord verifier for Python servers."""

    def __init__(self, min_ticks_increment: int = 1, unit_price_base_units: int = 1_000) -> None:
        self.min_ticks_increment = min_ticks_increment
        self.unit_price_base_units = unit_price_base_units
        self.high_water: Dict[str, int] = {}

    def verify_header(self, header_value: Optional[str]) -> Dict[str, Any]:
        parsed = parse_payword_header(header_value)
        if parsed is None:
            return {"paid": False, "reason": "missing or malformed X-Newsatoshi-PayWord header"}
        tip_hex, token_hex, ticks = parsed
        norm_tip = tip_hex.strip().lower()
        prev = self.high_water.get(norm_tip, 0)
        if ticks < prev + self.min_ticks_increment:
            return {
                "paid": False,
                "reason": f"replay or insufficient tick increment (got {ticks}, need >= {prev + self.min_ticks_increment})",
            }
        res = verify_payword_token(norm_tip, token_hex, ticks, self.unit_price_base_units)
        if not res["valid"]:
            return {"paid": False, "reason": res["reason"]}
        delta = ticks - prev
        self.high_water[norm_tip] = ticks
        return {
            "paid": True,
            "tip_hex": norm_tip,
            "ticks": ticks,
            "delta_ticks": delta,
            "delta_paid_base_units": delta * self.unit_price_base_units,
            "total_paid_base_units": res["total_paid_base_units"],
        }


class NewsatoshiMcpClient:
    """Zero-dependency Streamable HTTP MCP client for https://commons.newsatoshi.org/mcp."""

    def __init__(self, endpoint: str = f"{DEFAULT_COMMONS_URL}/mcp", timeout: float = 12.0) -> None:
        if not endpoint.startswith(("https://", "http://")):
            raise ValueError("MCP endpoint must use https:// or http://")
        self.endpoint = endpoint
        self.timeout = timeout
        self._seq = 0

    def rpc(self, method: str, params: Optional[Dict[str, Any]] = None) -> Any:
        self._seq += 1
        payload = json.dumps(
            {"jsonrpc": "2.0", "id": self._seq, "method": method, "params": params or {}}
        ).encode("utf-8")
        req = urllib.request.Request(
            self.endpoint,
            data=payload,
            headers={
                "Content-Type": "application/json",
                "Accept": "application/json",
                "User-Agent": "newsatoshi-402-py/0.8.0",
            },
        )
        with urllib.request.urlopen(req, timeout=self.timeout) as resp:
            data = json.loads(resp.read(2_097_152).decode("utf-8"))
        if data.get("error"):
            raise RuntimeError(f"MCP error: {data['error']}")
        return data.get("result")

    def list_tools(self) -> Any:
        return self.rpc("tools/list")

    def call_tool(self, name: str, arguments: Optional[Dict[str, Any]] = None) -> Any:
        return self.rpc("tools/call", {"name": name, "arguments": arguments or {}})


def build_402_challenge(
    resource: str = "/api/inference",
    amount_love: str = "0.01",
    network: str = "mainnet",
    recipient: str = "<RECIPIENT_NSD1_OR_NS1_ADDRESS>",
    unit_price_base_units: int = 1_000,
) -> Dict[str, Any]:
    net = "testnet" if network == "testnet" else "mainnet"
    ts = int(time.time())
    payment_hash = hashlib.sha256(
        f"NEWSATOSHI-402:{net}:{resource}:{amount_love}:{recipient}:{ts}".encode("utf-8")
    ).hexdigest()
    return {
        "status": 402,
        "protocol": "NEWSATOSHI-402/1.0",
        "resource": resource,
        "network": net,
        "amount_love": amount_love,
        "payment_hash_hex": payment_hash,
        "issued_at_unix": ts,
        "www_authenticate": f'Newsatoshi-402 realm="newsatoshi", network="{net}", amount="{amount_love}"',
        "unit_price_base_units": unit_price_base_units,
    }
