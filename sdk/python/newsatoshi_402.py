#!/usr/bin/env python3
"""
newsatoshi_402 — Zero-Dependency Post-Quantum HTTP 402 PayWord & MCP Python SDK (v0.14.0).

Implements incremental O(1) Rivest–Shamir (1996) PayWord SHA-256 hash-chain verification
(supporting up to 1,000,000 ticks per channel with sparse checkpoints), optional durable
watermark persistence across restarts, live Newsatoshi L1 on-chain lock funding verification,
and zero-API-key Streamable HTTP MCP access for https://commons.newsatoshi.org/mcp.
"""

import hashlib
import hmac
import json
import os
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from typing import Any, Callable, Dict, Optional, Tuple

DEFAULT_COMMONS_URL = "https://commons.newsatoshi.org"
DEFAULT_MAINNET_SEED_URL = "https://newsatoshi.org/mainnet"
DEFAULT_TESTNET_SEED_URL = "https://newsatoshi-seed.fly.dev"
MAX_PAYWORD_TICKS = 1_000_000
MAX_PAYWORD_STEP_JUMP = 100_000


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
    _checkpoints: Dict[int, bytes] = field(repr=False)
    _stride: int = 1024
    spent_ticks: int = 0

    @property
    def remaining_ticks(self) -> int:
        return self.max_ticks - self.spent_ticks

    def _derive_link(self, index: int) -> bytes:
        base_idx = (index // self._stride) * self._stride
        cur = self._checkpoints[base_idx]
        for _ in range(index - base_idx):
            cur = hashlib.sha256(cur).digest()
        return cur

    def next_payment(self, ticks_increment: int = 1) -> Dict[str, Any]:
        if ticks_increment < 1:
            raise ValueError("ticks_increment must be >= 1")
        if self.spent_ticks + ticks_increment > self.max_ticks:
            raise RuntimeError("PayWord channel exhausted")
        self.spent_ticks += ticks_increment
        token_hex = self._derive_link(self.max_ticks - self.spent_ticks).hex()
        return {
            "tip_hex": self.tip_hex,
            "token_hex": token_hex,
            "ticks": self.spent_ticks,
            "total_paid_base_units": self.spent_ticks * self.unit_price_base_units,
            "header_value": f"tip={self.tip_hex},token={token_hex},ticks={self.spent_ticks}",
        }


def generate_payword_chain(
    max_ticks: int,
    unit_price_base_units: int = 1_000,
    checkpoint_stride: int = 1024,
) -> PayWordWallet:
    """Generates a CSPRNG PayWord chain supporting up to 1,000,000 ticks with ~32 KB RAM."""
    if not (1 <= max_ticks <= MAX_PAYWORD_TICKS):
        raise ValueError(f"max_ticks must be in 1..={MAX_PAYWORD_TICKS}")
    if unit_price_base_units < 1:
        raise ValueError("unit_price_base_units must be >= 1")
    stride = 1 if max_ticks <= 4_096 else max(1, checkpoint_stride)
    seed = os.urandom(32)
    checkpoints: Dict[int, bytes] = {0: seed}
    cur = seed
    for i in range(1, max_ticks + 1):
        cur = hashlib.sha256(cur).digest()
        if i % stride == 0 or i == max_ticks:
            checkpoints[i] = cur
    return PayWordWallet(
        tip_hex=cur.hex(),
        max_ticks=max_ticks,
        unit_price_base_units=unit_price_base_units,
        _checkpoints=checkpoints,
        _stride=stride,
    )


def verify_payword_token(
    tip_hex: str,
    token_hex: str,
    ticks: int,
    unit_price_base_units: int = 1_000,
    prev_token_hex: Optional[str] = None,
    prev_ticks: int = 0,
) -> Dict[str, Any]:
    """
    Verifies a PayWord voucher:
    - Incremental O(delta) mode: when `prev_token_hex` & `prev_ticks > 0` are supplied,
      checks `SHA256^(ticks - prev_ticks)(token) == prev_token` (1 hash per sequential call).
    - Full-chain mode: checks `SHA256^ticks(token) == tip`.
    """
    if not isinstance(ticks, int) or not (1 <= ticks <= MAX_PAYWORD_TICKS):
        return {"valid": False, "reason": f"ticks must be in 1..={MAX_PAYWORD_TICKS}"}
    try:
        tip_bytes = _validate_hex32(tip_hex)
        cur = _validate_hex32(token_hex)
        if prev_token_hex and prev_ticks > 0:
            if ticks <= prev_ticks:
                return {
                    "valid": False,
                    "reason": f"ticks ({ticks}) must exceed prev_ticks ({prev_ticks})",
                }
            iterations = ticks - prev_ticks
            anchor_bytes = _validate_hex32(prev_token_hex)
            mode = "incremental"
        else:
            iterations = ticks
            anchor_bytes = tip_bytes
            mode = "full"
    except ValueError as exc:
        return {"valid": False, "reason": str(exc)}

    if iterations > MAX_PAYWORD_STEP_JUMP:
        return {
            "valid": False,
            "reason": f"single-call tick jump ({iterations}) exceeds anti-DoS cap ({MAX_PAYWORD_STEP_JUMP})",
        }

    for _ in range(iterations):
        cur = hashlib.sha256(cur).digest()
    valid = hmac.compare_digest(cur, anchor_bytes)
    return {
        "valid": valid,
        "mode": mode,
        "hashes_evaluated": iterations,
        "ticks": ticks,
        "total_paid_base_units": ticks * unit_price_base_units if valid else 0,
        "computed_tip_hex": cur.hex() if mode == "full" else tip_hex.strip().lower(),
        "reason": None
        if valid
        else (
            "SHA256^(ticks - prev_ticks)(token) did not match last accepted token"
            if mode == "incremental"
            else "SHA256^ticks(token) did not match committed tip"
        ),
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


def verify_l1_lock_funding(
    tip_hex: str,
    *,
    network: str = "mainnet",
    merchant_address: Optional[str] = None,
    required_base_units: int = 1,
    min_blocks_remaining: int = 6,
    seed_base_url: Optional[str] = None,
    timeout: float = 10.0,
) -> Dict[str, Any]:
    """Queries live Newsatoshi L1 (/api/lock?hash=<tip>) to confirm an open, solvent lock."""
    net = "testnet" if network == "testnet" else "mainnet"
    base = (
        seed_base_url or (DEFAULT_TESTNET_SEED_URL if net == "testnet" else DEFAULT_MAINNET_SEED_URL)
    ).rstrip("/")
    clean_tip = tip_hex.strip().lower().removeprefix("0x")
    try:
        with urllib.request.urlopen(f"{base}/api/lock?hash={clean_tip}", timeout=timeout) as r1:
            lock_data = json.loads(r1.read(524_288).decode("utf-8"))
        try:
            with urllib.request.urlopen(f"{base}/status", timeout=timeout) as r2:
                status_data = json.loads(r2.read(262_144).decode("utf-8"))
        except Exception:
            status_data = {}
        current_height = int(status_data.get("height", 0) or 0)
        open_raw = lock_data.get("open")
        if isinstance(open_raw, list):
            open_locks = open_raw
        elif isinstance(open_raw, dict):
            open_locks = [open_raw]
        elif isinstance(lock_data.get("lock"), dict) and lock_data.get("status") == "open":
            open_locks = [lock_data["lock"]]
        else:
            open_locks = []
        if not open_locks:
            return {
                "funded": False,
                "network": net,
                "current_height": current_height,
                "reason": f"no open funded L1 lock found on {net} for commitment {clean_tip[:16]}...",
            }
        for lock in open_locks:
            amount = int(lock.get("amount", lock.get("value", 0)) or 0)
            lock_timeout = int(lock.get("timeout", 0) or 0)
            claim = str(lock.get("claim_addr", lock.get("claim", lock.get("to", "")))).strip().lower()
            addr_ok = (
                not merchant_address
                or merchant_address.startswith("<")
                or claim == merchant_address.strip().lower()
            )
            amount_ok = amount >= required_base_units
            timeout_ok = current_height == 0 or lock_timeout >= current_height + min_blocks_remaining
            if addr_ok and amount_ok and timeout_ok:
                return {
                    "funded": True,
                    "network": net,
                    "lock_id": lock.get("id"),
                    "amount_base_units": amount,
                    "timeout_height": lock_timeout,
                    "current_height": current_height,
                    "claim_address": claim or None,
                }
        return {
            "funded": False,
            "network": net,
            "current_height": current_height,
            "reason": f"open lock on {net} fails merchant address, solvency, or timeout margin",
        }
    except Exception as exc:
        return {"funded": False, "network": net, "reason": f"L1 funding verification failed: {exc}"}


class PayWordGate:
    """
    Incremental O(1) anti-replay HTTP 402 PayWord verifier for Python servers.
    Tracks `(last_token, last_ticks)` per channel, supports atomic JSON file persistence
    across restarts (`state_file`), and enforces live on-chain funding verification when
    `require_funding=True` or a custom `funding_verifier` is configured.
    """

    def __init__(
        self,
        min_ticks_increment: int = 1,
        unit_price_base_units: int = 1_000,
        *,
        network: str = "mainnet",
        merchant_address: Optional[str] = None,
        require_funding: bool = False,
        allow_unfunded_demo: Optional[bool] = None,
        min_blocks_remaining: int = 6,
        funding_verifier: Optional[Callable[[Dict[str, Any]], Dict[str, Any]]] = None,
        state_file: Optional[str] = None,
    ) -> None:
        self.min_ticks_increment = min_ticks_increment
        self.unit_price_base_units = unit_price_base_units
        self.network = network
        self.merchant_address = merchant_address
        self.require_funding = require_funding
        self.allow_unfunded_demo = (not require_funding) if allow_unfunded_demo is None else allow_unfunded_demo
        self.min_blocks_remaining = min_blocks_remaining
        self.funding_verifier = funding_verifier
        self.state_file = state_file
        self.channels: Dict[str, Dict[str, Any]] = {}
        self.high_water: Dict[str, int] = {}
        if self.state_file and os.path.exists(self.state_file):
            try:
                with open(self.state_file, "r", encoding="utf-8") as fh:
                    loaded = json.load(fh)
                if isinstance(loaded, dict):
                    self.channels = loaded
                    self.high_water = {k: int(v.get("ticks", 0)) for k, v in loaded.items()}
            except Exception:
                pass

    def _persist(self) -> None:
        if not self.state_file:
            return
        tmp = f"{self.state_file}.tmp"
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(self.channels, fh, indent=2)
        os.replace(tmp, self.state_file)

    def verify_header(self, header_value: Optional[str]) -> Dict[str, Any]:
        parsed = parse_payword_header(header_value)
        if parsed is None:
            return {
                "paid": False,
                "collectible_on_chain": False,
                "funding_status": "rejected",
                "reason": "missing or malformed X-Newsatoshi-PayWord header",
            }
        tip_hex, token_hex, ticks = parsed
        norm_tip = tip_hex.strip().lower()
        norm_tok = token_hex.strip().lower()
        prev_state = self.channels.get(norm_tip)
        prev_ticks = int(prev_state["ticks"]) if prev_state else 0
        if ticks < prev_ticks + self.min_ticks_increment:
            return {
                "paid": False,
                "collectible_on_chain": False,
                "funding_status": "rejected",
                "reason": f"replay or insufficient tick increment (got {ticks}, need >= {prev_ticks + self.min_ticks_increment})",
            }

        required_total = ticks * self.unit_price_base_units
        funded_lock = prev_state.get("funded_lock") if prev_state else None
        funding_status = "verified_onchain_lock" if funded_lock else "unfunded_demo"

        if self.require_funding or not self.allow_unfunded_demo or self.funding_verifier is not None:
            cached_ok = (
                isinstance(funded_lock, dict)
                and funded_lock.get("funded") is True
                and int(funded_lock.get("amount_base_units", 0)) >= required_total
            )
            if not cached_ok:
                if self.funding_verifier is not None:
                    check_funding = self.funding_verifier(
                        {
                            "tip_hex": norm_tip,
                            "ticks": ticks,
                            "required_base_units": required_total,
                            "network": self.network,
                            "merchant_address": self.merchant_address,
                            "min_blocks_remaining": self.min_blocks_remaining,
                        }
                    )
                else:
                    check_funding = verify_l1_lock_funding(
                        norm_tip,
                        network=self.network,
                        merchant_address=self.merchant_address,
                        required_base_units=required_total,
                        min_blocks_remaining=self.min_blocks_remaining,
                    )
                if not check_funding or not check_funding.get("funded"):
                    return {
                        "paid": False,
                        "collectible_on_chain": False,
                        "funding_status": "rejected",
                        "reason": (check_funding or {}).get(
                            "reason", "unfunded PayWord channel: no live funded on-chain lock"
                        ),
                    }
                funded_lock = check_funding
                funding_status = "verified_onchain_lock"

        res = verify_payword_token(
            norm_tip,
            norm_tok,
            ticks,
            self.unit_price_base_units,
            prev_token_hex=prev_state["token_hex"] if prev_state else None,
            prev_ticks=prev_ticks,
        )
        if not res["valid"]:
            return {
                "paid": False,
                "collectible_on_chain": False,
                "funding_status": "rejected",
                "reason": res["reason"],
            }
        delta = ticks - prev_ticks
        self.high_water[norm_tip] = ticks
        self.channels[norm_tip] = {
            "ticks": ticks,
            "token_hex": norm_tok,
            "funded_lock": funded_lock,
            "updated_at_unix": int(time.time()),
        }
        self._persist()
        return {
            "paid": True,
            "collectible_on_chain": funding_status == "verified_onchain_lock",
            "funding_status": funding_status,
            "funded_lock": funded_lock,
            "verification_mode": res["mode"],
            "hashes_evaluated": res["hashes_evaluated"],
            "tip_hex": norm_tip,
            "token_hex": norm_tok,
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
                "User-Agent": "newsatoshi-402-py/0.14.0",
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

    def inspect_evm_contract(self, address: str, chain: str = "base") -> Any:
        return self.call_tool("crypto_inspect_evm_contract", {"address": address, "chain": chain})

    def bitcoin_mempool_oracle(self) -> Any:
        return self.call_tool("crypto_bitcoin_mempool_oracle", {})

    def decode_bolt11(self, invoice: str) -> Any:
        return self.call_tool("crypto_decode_bolt11_invoice", {"invoice": invoice})

    def quote_base_uniswap_v3(self, token_in: str, token_out: str, amount: str) -> Any:
        return self.call_tool(
            "crypto_quote_base_uniswap_v3",
            {"token_in": token_in, "token_out": token_out, "amount": str(amount)},
        )

    def verify_tx_receipt(self, tx_hash: str, chain: str = "base") -> Any:
        return self.call_tool("crypto_verify_tx_receipt", {"tx_hash": tx_hash, "chain": chain})


def fetch_with_newsatoshi_402(
    url: str,
    wallet: PayWordWallet,
    *,
    method: str = "GET",
    body: Optional[bytes] = None,
    headers: Optional[Dict[str, str]] = None,
    ticks_per_call: int = 1,
    timeout: float = 15.0,
) -> Tuple[int, bytes, Dict[str, str]]:
    """Autonomous Python HTTP client: automatically retries HTTP 402 with the next PayWord token."""
    if not url.startswith(("https://", "http://")):
        raise ValueError("URL must use https:// or http://")
    req_headers = dict(headers or {})
    req = urllib.request.Request(url, data=body, headers=req_headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return resp.status, resp.read(2_097_152), dict(resp.headers.items())
    except urllib.error.HTTPError as err:
        if err.code != 402:
            raise
        payment = wallet.next_payment(ticks_per_call)
        retry_headers = dict(req_headers)
        retry_headers["X-Newsatoshi-PayWord"] = payment["header_value"]
        retry_req = urllib.request.Request(url, data=body, headers=retry_headers, method=method)
        with urllib.request.urlopen(retry_req, timeout=timeout) as resp:
            return resp.status, resp.read(2_097_152), dict(resp.headers.items())


CANONICAL_EVM_HTLC = "0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987"
CANONICAL_BACKING_VAULT = "0x37Ed94525DBE408fA72cDd156409875666679265"


def build_402_challenge(
    resource: str = "/api/inference",
    amount_love: str = "0.01",
    network: str = "mainnet",
    recipient: str = "<RECIPIENT_NSD1_OR_NS1_ADDRESS>",
    unit_price_base_units: int = 1_000,
    evm_pay_to: Optional[str] = None,
    x402_usdc_micro: int = 1_000,
    charter_id: str = "",
    backing_bps: int = 2000,
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
        "max_payword_ticks": MAX_PAYWORD_TICKS,
        "x402": {
            "scheme": "exact",
            "network": "eip155:8453",
            "asset": "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
            "maxAmountRequired": str(x402_usdc_micro),
            "payTo": evm_pay_to or CANONICAL_BACKING_VAULT,
            "htlcBridge": CANONICAL_EVM_HTLC,
            "backingVault": CANONICAL_BACKING_VAULT,
            "charterId": charter_id,
            "backingBps": backing_bps,
        },
    }
