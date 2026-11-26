#!/usr/bin/env python3
"""
newsatoshi_402 — Zero-Dependency Post-Quantum HTTP 402 PayWord & MCP Python SDK (v0.15.0).

Implements incremental O(1) Rivest–Shamir (1996) PayWord SHA-256 hash-chain verification
(supporting up to 1,000,000 ticks per channel with sparse checkpoints), optional durable
watermark persistence across restarts, live Newsatoshi L1 RFC-0022 PayWord Channel funding
verification (`/api/channel`, canonical SHA-256 Merkle inclusion proof check, and 70-byte
unilateral `ChannelClaim` broadcast via `POST /api/tx`), legacy L1 HTLC lock checks
(`/api/lock`), and zero-API-key Streamable HTTP MCP access for https://commons.newsatoshi.org/mcp.
"""

import hashlib
import hmac
import json
import os
import struct
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from typing import Any, Callable, Dict, List, Optional, Tuple

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


def verify_channel_merkle_proof(proof: Optional[Dict[str, Any]]) -> bool:
    """Verifies a canonical Newsatoshi L1 MerkleMap inclusion proof using pure SHA-256."""
    if not isinstance(proof, dict):
        return False
    try:
        root_bytes = _validate_hex32(str(proof.get("root", "")))
        if all(b == 0 for b in root_bytes):
            return False
        key_bytes = _validate_hex32(str(proof.get("key", "")))
        val_bytes = _validate_hex32(str(proof.get("value_hash", proof.get("valueHash", ""))))
        siblings = proof.get("siblings")
        if not isinstance(siblings, list) or len(siblings) > 256:
            return False
        cur = hashlib.sha256(b"nsMKL" + key_bytes + val_bytes).digest()
        for depth in range(len(siblings) - 1, -1, -1):
            sib = _validate_hex32(str(siblings[depth]))
            bit = ((key_bytes[depth >> 3] >> (7 - (depth & 7))) & 1) == 1
            if bit:
                cur = hashlib.sha256(b"nsMKB" + sib + cur).digest()
            else:
                cur = hashlib.sha256(b"nsMKB" + cur + sib).digest()
        return hmac.compare_digest(cur, root_bytes)
    except Exception:
        return False


def channel_claim_message(channel_id_hex: str, ticks: int, token_hex: str) -> str:
    """Computes SHA256('nsCHANCLAIM' || channel_id || ticks_u32_le || token) signed by seller_vk."""
    if not isinstance(ticks, int) or not (1 <= ticks <= MAX_PAYWORD_TICKS):
        raise ValueError(f"ticks must be in 1..={MAX_PAYWORD_TICKS}")
    ch_bytes = _validate_hex32(channel_id_hex)
    tok_bytes = _validate_hex32(token_hex)
    return hashlib.sha256(b"nsCHANCLAIM" + ch_bytes + struct.pack("<I", ticks) + tok_bytes).hexdigest()


def encode_channel_claim_tx(
    channel_id_hex: str,
    ticks: int,
    token_hex: str,
    seller_sig_hex: str,
) -> bytes:
    """Encodes a seller-signed RFC-0022 unilateral Action::ChannelClaim L1 transaction (7,930 bytes with SLH-DSA-128s)."""
    if not isinstance(ticks, int) or not (1 <= ticks <= MAX_PAYWORD_TICKS):
        raise ValueError(f"ticks must be in 1..={MAX_PAYWORD_TICKS}")
    ch_bytes = _validate_hex32(channel_id_hex)
    tok_bytes = _validate_hex32(token_hex)
    clean_sig = (seller_sig_hex or "").strip().lower().removeprefix("0x")
    sig_bytes = bytes.fromhex(clean_sig) if clean_sig else b""
    if len(sig_bytes) > 7856:
        raise ValueError("seller signature exceeds 7,856 bytes")
    return (
        bytes([2, 11])
        + ch_bytes
        + struct.pack("<I", ticks)
        + tok_bytes
        + struct.pack("<I", len(sig_bytes))
        + sig_bytes
    )


def encode_channel_expire_tx(channel_id_hex: str) -> bytes:
    """Encodes the exact 34-byte RFC-0022 Action::ChannelExpire L1 transaction."""
    ch_bytes = _validate_hex32(channel_id_hex)
    return bytes([2, 12]) + ch_bytes


def submit_l1_channel_claim(
    channel_id_hex: str,
    ticks: int,
    token_hex: str,
    seller_sig_hex: str,
    *,
    network: str = "mainnet",
    seed_base_url: Optional[str] = None,
    timeout: float = 10.0,
) -> Dict[str, Any]:
    """Broadcasts a seller-signed unilateral RFC-0022 ChannelClaim tx to any Newsatoshi L1 node."""
    net = "testnet" if network == "testnet" else "mainnet"
    base = (
        seed_base_url or (DEFAULT_TESTNET_SEED_URL if net == "testnet" else DEFAULT_MAINNET_SEED_URL)
    ).rstrip("/")
    tx_bytes = encode_channel_claim_tx(channel_id_hex, ticks, token_hex, seller_sig_hex)
    req = urllib.request.Request(
        f"{base}/api/tx",
        data=tx_bytes,
        headers={"Content-Type": "application/octet-stream"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            data = json.loads(resp.read(65_536).decode("utf-8"))
            return {
                "ok": bool(data.get("ok")),
                "status": resp.status,
                "txid": data.get("txid"),
                "tx_bytes": len(tx_bytes),
                "error": data.get("error"),
            }
    except urllib.error.HTTPError as err:
        try:
            data = json.loads(err.read(65_536).decode("utf-8"))
        except Exception:
            data = {"ok": False, "error": f"HTTP {err.code}"}
        return {
            "ok": False,
            "status": err.code,
            "txid": None,
            "tx_bytes": len(tx_bytes),
            "error": data.get("error", f"HTTP {err.code}"),
        }


def verify_l1_channel_funding(
    tip_hex: Optional[str] = None,
    *,
    channel_id: Optional[str] = None,
    network: str = "mainnet",
    seller_address: Optional[str] = None,
    seller_vk: Optional[str] = None,
    merchant_address: Optional[str] = None,
    required_ticks: int = 1,
    unit_price_base_units: int = 1,
    min_blocks_remaining: int = 6,
    require_confirmed: bool = True,
    seed_base_url: Optional[str] = None,
    timeout: float = 10.0,
) -> Dict[str, Any]:
    """Queries live Newsatoshi L1 (/api/channel?tip=<hex> or ?id=<hex>) for an open RFC-0022 channel."""
    net = "testnet" if network == "testnet" else "mainnet"
    base = (
        seed_base_url or (DEFAULT_TESTNET_SEED_URL if net == "testnet" else DEFAULT_MAINNET_SEED_URL)
    ).rstrip("/")
    expected_seller = seller_address or merchant_address
    try:
        if channel_id:
            clean_id = channel_id.strip().lower().removeprefix("0x")
            url = f"{base}/api/channel?id={clean_id}"
        elif tip_hex:
            clean_tip = tip_hex.strip().lower().removeprefix("0x")
            url = f"{base}/api/channel?tip={clean_tip}"
        else:
            return {"funded": False, "network": net, "reason": "tip_hex or channel_id is required"}

        with urllib.request.urlopen(url, timeout=timeout) as resp:
            data = json.loads(resp.read(524_288).decode("utf-8"))
        current_height = int(data.get("height", 0) or 0)
        if isinstance(data.get("channels"), list):
            candidates = data["channels"]
        elif data.get("found"):
            candidates = [data]
        else:
            candidates = []
        open_channels = [
            ch for ch in candidates if isinstance(ch, dict) and ch.get("found") is not False and ch.get("state") == "open"
        ]
        if not open_channels:
            return {
                "funded": False,
                "network": net,
                "current_height": current_height,
                "reason": f"no open funded RFC-0022 L1 channel found on {net}",
            }

        for ch in open_channels:
            ch_height = int(ch.get("height", current_height) or current_height or 0)
            max_ticks = int(ch.get("max_ticks", 0) or 0)
            unit_price = int(ch.get("unit_price", 0) or 0)
            ch_timeout = int(ch.get("timeout", 0) or 0)
            seller = str(ch.get("seller", "")).strip().lower()
            ch_vk = str(ch.get("seller_vk", "")).strip().lower()
            confirmed_ok = (not require_confirmed) or (ch.get("confirmed") is True)
            addr_ok = (
                not expected_seller
                or expected_seller.startswith("<")
                or seller == expected_seller.strip().lower()
            )
            vk_ok = (not seller_vk) or (ch_vk == seller_vk.strip().lower().removeprefix("0x"))
            ticks_ok = max_ticks >= required_ticks
            price_ok = unit_price >= unit_price_base_units
            timeout_ok = ch_height == 0 or ch_timeout >= ch_height + min_blocks_remaining
            if confirmed_ok and addr_ok and vk_ok and ticks_ok and price_ok and timeout_ok:
                merkle_verified = False
                if ch.get("merkle_proof"):
                    merkle_verified = verify_channel_merkle_proof(ch["merkle_proof"])
                    if not merkle_verified:
                        return {
                            "funded": False,
                            "network": net,
                            "current_height": ch_height,
                            "reason": "RFC-0022 channel Merkle inclusion proof failed local SHA-256 verification",
                        }
                budget = int(ch.get("budget", max_ticks * unit_price) or (max_ticks * unit_price))
                return {
                    "funded": True,
                    "rail": "rfc0022_payword_channel",
                    "network": net,
                    "channel_id": ch.get("id"),
                    "tip_hex": ch.get("tip") or tip_hex,
                    "max_ticks": max_ticks,
                    "unit_price_base_units": unit_price,
                    "claim_fee_base_units": int(ch.get("claim_fee", 0) or 0),
                    "budget_base_units": budget,
                    "total_deposit_base_units": int(ch.get("total_deposit", budget) or budget),
                    "amount_base_units": budget,
                    "timeout_height": ch_timeout,
                    "current_height": ch_height,
                    "confirmations": int(ch.get("confirmations", 0) or 0),
                    "confirmed": bool(ch.get("confirmed")),
                    "seller_address": ch.get("seller"),
                    "seller_vk": ch.get("seller_vk"),
                    "refund_address": ch.get("refund"),
                    "merkle_proof_verified": merkle_verified,
                }
        return {
            "funded": False,
            "network": net,
            "current_height": current_height,
            "reason": f"open RFC-0022 channel on {net} fails confirmation, seller, ticks, unit_price, or timeout check",
        }
    except Exception as exc:
        return {"funded": False, "network": net, "reason": f"L1 channel verification failed: {exc}"}


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
        if isinstance(lock_data.get("locks"), list):
            open_locks = [
                lk for lk in lock_data["locks"] if isinstance(lk, dict) and lk.get("found") is not False and lk.get("state") == "open"
            ]
        else:
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
        funding_rail: str = "auto",
        require_confirmed: bool = True,
        min_blocks_remaining: int = 6,
        seed_base_url: Optional[str] = None,
        funding_verifier: Optional[Callable[[Dict[str, Any]], Dict[str, Any]]] = None,
        state_file: Optional[str] = None,
    ) -> None:
        self.min_ticks_increment = min_ticks_increment
        self.unit_price_base_units = unit_price_base_units
        self.network = network
        self.merchant_address = merchant_address
        self.require_funding = require_funding
        self.allow_unfunded_demo = (not require_funding) if allow_unfunded_demo is None else allow_unfunded_demo
        self.funding_rail = funding_rail
        self.require_confirmed = require_confirmed
        self.min_blocks_remaining = min_blocks_remaining
        self.seed_base_url = seed_base_url
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
        if funded_lock:
            funding_status = (
                "verified_onchain_channel"
                if funded_lock.get("rail") == "rfc0022_payword_channel"
                else "verified_onchain_lock"
            )
        else:
            funding_status = "unfunded_demo"

        if self.require_funding or not self.allow_unfunded_demo or self.funding_verifier is not None:
            if isinstance(funded_lock, dict) and funded_lock.get("funded") is True:
                if funded_lock.get("rail") == "rfc0022_payword_channel":
                    cached_ok = (
                        int(funded_lock.get("max_ticks", 0)) >= ticks
                        and int(funded_lock.get("unit_price_base_units", 0)) >= self.unit_price_base_units
                    )
                else:
                    cached_ok = int(funded_lock.get("amount_base_units", 0)) >= required_total
            else:
                cached_ok = False
            if not cached_ok:
                if self.funding_verifier is not None:
                    check_funding = self.funding_verifier(
                        {
                            "tip_hex": norm_tip,
                            "ticks": ticks,
                            "unit_price_base_units": self.unit_price_base_units,
                            "required_base_units": required_total,
                            "network": self.network,
                            "merchant_address": self.merchant_address,
                            "min_blocks_remaining": self.min_blocks_remaining,
                        }
                    )
                elif self.funding_rail == "lock":
                    check_funding = verify_l1_lock_funding(
                        norm_tip,
                        network=self.network,
                        merchant_address=self.merchant_address,
                        required_base_units=required_total,
                        min_blocks_remaining=self.min_blocks_remaining,
                        seed_base_url=self.seed_base_url,
                    )
                else:
                    check_funding = verify_l1_channel_funding(
                        norm_tip,
                        network=self.network,
                        seller_address=self.merchant_address,
                        required_ticks=ticks,
                        unit_price_base_units=self.unit_price_base_units,
                        min_blocks_remaining=self.min_blocks_remaining,
                        require_confirmed=self.require_confirmed,
                        seed_base_url=self.seed_base_url,
                    )
                    if not check_funding.get("funded") and self.funding_rail == "auto":
                        lock_check = verify_l1_lock_funding(
                            norm_tip,
                            network=self.network,
                            merchant_address=self.merchant_address,
                            required_base_units=required_total,
                            min_blocks_remaining=self.min_blocks_remaining,
                            seed_base_url=self.seed_base_url,
                        )
                        if lock_check.get("funded"):
                            check_funding = lock_check
                if not check_funding or not check_funding.get("funded"):
                    return {
                        "paid": False,
                        "collectible_on_chain": False,
                        "funding_status": "rejected",
                        "reason": (check_funding or {}).get(
                            "reason", "unfunded PayWord channel: no live funded on-chain channel or lock"
                        ),
                    }
                funded_lock = check_funding
                funding_status = (
                    "verified_onchain_channel"
                    if funded_lock.get("rail") == "rfc0022_payword_channel"
                    else "verified_onchain_lock"
                )

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
        collectible = funding_status in ("verified_onchain_channel", "verified_onchain_lock")
        return {
            "paid": True,
            "collectible_on_chain": collectible,
            "unilateral_claim_ready": funding_status == "verified_onchain_channel",
            "channel_id": funded_lock.get("channel_id") if isinstance(funded_lock, dict) else None,
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
                "User-Agent": "newsatoshi-402-py/0.15.0",
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


# =============================================================================
# Verifiable Economic Clearinghouse & Sovereign L2 Substrate SDK (v0.16.0)
# RFC-0023 (Sovereign L2 Rollup Vaults) · RFC-0024 (Universal Trust Bonds) ·
# RFC-0025 (Scoped Agent Session Policies) · RFC-0026 (Collective Signaling) ·
# Tier-1 zkTLS Attestation Verification & Tier-2 Optimistic Spot-Checks
# =============================================================================


def _resolve_seed_url(network: str, seed_base_url: Optional[str]) -> str:
    if seed_base_url:
        return seed_base_url.rstrip("/")
    return (DEFAULT_TESTNET_SEED_URL if network == "testnet" else DEFAULT_MAINNET_SEED_URL).rstrip("/")


def verify_merkle_map_proof(
    merkle_proof: Dict[str, Any], expected_root_hex: Optional[str] = None
) -> bool:
    """Verify a canonical Newsatoshi L1 MerkleMap inclusion proof (channels, bonds, rollups, withdrawals)."""
    if not isinstance(merkle_proof, dict):
        return False
    root = expected_root_hex or merkle_proof.get("root")
    if not root:
        return False
    if expected_root_hex and merkle_proof.get("root") and expected_root_hex.lower() != str(merkle_proof["root"]).lower():
        return False
    return verify_channel_merkle_proof({**merkle_proof, "root": root})


def fetch_l1_trust_bond(
    bond_id_hex: str,
    network: str = "mainnet",
    seed_base_url: Optional[str] = None,
    expected_bonds_root_hex: Optional[str] = None,
    timeout: int = 8,
) -> Dict[str, Any]:
    """Query and cryptographically verify a live RFC-0024 Universal Trust Bond (`GET /api/bond?id=...`)."""
    bid = str(bond_id_hex or "").strip().lower()
    if len(bid) != 64:
        raise ValueError("bond_id_hex must be 64 hex characters")
    base = _resolve_seed_url(network, seed_base_url)
    req = urllib.request.Request(f"{base}/api/bond?id={bid}", headers={"Accept": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            data = json.loads(resp.read(1_048_576).decode("utf-8"))
    except Exception as exc:
        return {"found": False, "reason": str(exc)}
    if not data or not data.get("found"):
        return {"found": False, "height": data.get("height") if isinstance(data, dict) else None, "reason": "trust bond not found on L1"}
    mp = data.get("merkle_proof")
    merkle_verified = verify_merkle_map_proof(mp, expected_bonds_root_hex) if mp else False
    return {
        "found": True,
        "confirmed": bool(data.get("confirmed")),
        "merkle_verified": merkle_verified,
        "height": int(data.get("height", 0)),
        "id": data.get("id"),
        "owner_vk": data.get("owner_vk"),
        "owner_address": data.get("owner_address"),
        "stake": int(data.get("stake", 0)),
        "slash_burn_bps": int(data.get("slash_burn_bps", 0)),
        "unbond_delay": int(data.get("unbond_delay", 0)),
        "quorum_m": int(data.get("quorum_m", 0)),
        "settled_count": int(data.get("settled_count", 0)),
        "settled_volume": int(data.get("settled_volume", "0")),
        "slash_count": int(data.get("slash_count", 0)),
        "slashed_total": int(data.get("slashed_total", "0")),
        "action_nonce": int(data.get("action_nonce", 0)),
        "merkle_proof": mp,
    }


def fetch_l1_rollup_vault(
    rollup_id_hex: str,
    network: str = "mainnet",
    seed_base_url: Optional[str] = None,
    expected_rollups_root_hex: Optional[str] = None,
    timeout: int = 8,
) -> Dict[str, Any]:
    """Query and cryptographically verify a live RFC-0023 Sovereign L2 Rollup Vault (`GET /api/rollup?id=...`)."""
    rid = str(rollup_id_hex or "").strip().lower()
    if len(rid) != 64:
        raise ValueError("rollup_id_hex must be 64 hex characters")
    base = _resolve_seed_url(network, seed_base_url)
    req = urllib.request.Request(f"{base}/api/rollup?id={rid}", headers={"Accept": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            data = json.loads(resp.read(1_048_576).decode("utf-8"))
    except Exception as exc:
        return {"found": False, "reason": str(exc)}
    if not data or not data.get("found"):
        return {"found": False, "height": data.get("height") if isinstance(data, dict) else None, "reason": "rollup vault not found on L1"}
    mp = data.get("merkle_proof")
    merkle_verified = verify_merkle_map_proof(mp, expected_rollups_root_hex) if mp else False
    return {
        "found": True,
        "confirmed": bool(data.get("confirmed")),
        "merkle_verified": merkle_verified,
        "height": int(data.get("height", 0)),
        "id": data.get("id"),
        "vk_hash": data.get("vk_hash"),
        "genesis_root": data.get("genesis_root"),
        "state_root": data.get("state_root"),
        "deposit_acc": data.get("deposit_acc"),
        "deposit_count": int(data.get("deposit_count", 0)),
        "batch_seq": int(data.get("batch_seq", 0)),
        "total_l2_txs": int(data.get("total_l2_txs", 0)),
        "tvl": int(data.get("tvl", 0)),
        "pending_withdrawals": int(data.get("pending_withdrawals", 0)),
        "withdraw_roots": data.get("withdraw_roots") or [],
        "merkle_proof": mp,
    }


def fetch_l1_signaling(
    network: str = "mainnet",
    seed_base_url: Optional[str] = None,
    timeout: int = 8,
) -> Dict[str, Any]:
    """Query live RFC-0026 BIP-9 collective protocol signaling & ossification telemetry (`GET /api/signaling`)."""
    base = _resolve_seed_url(network, seed_base_url)
    req = urllib.request.Request(f"{base}/api/signaling", headers={"Accept": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read(1_048_576).decode("utf-8"))


def bond_commit_message_hex(bond_id_hex: str, context_nonce: int, payload_hash_hex: str) -> str:
    """Compute exact L1 RFC-0024 commitment digest (`SHA256("nsBONDCOMMIT" || bond || nonce_u64_le || payload)`)."""
    bond = bytes.fromhex(bond_id_hex)
    payload = bytes.fromhex(payload_hash_hex)
    if len(bond) != 32 or len(payload) != 32:
        raise ValueError("bond_id_hex and payload_hash_hex must be 32 bytes")
    return hashlib.sha256(b"nsBONDCOMMIT" + bond + int(context_nonce).to_bytes(8, "little") + payload).hexdigest()


def encode_rollup_withdraw_tx(
    rollup_id_hex: str,
    batch_seq: int,
    withdrawal_id_hex: str,
    recipient_apk_hex: str,
    amount: int,
    miner_fee: int,
    merkle_proof: Dict[str, Any],
) -> bytes:
    """Encode an L1 RFC-0023 `Tx::Act(Action::RollupWithdraw)` (tx tag 2, action tag 19)."""
    rollup = bytes.fromhex(rollup_id_hex)
    wid = bytes.fromhex(withdrawal_id_hex)
    apk = bytes.fromhex(recipient_apk_hex)
    key = bytes.fromhex(merkle_proof["key"])
    vhash = bytes.fromhex(merkle_proof.get("value_hash") or merkle_proof["valueHash"])
    siblings = [bytes.fromhex(s) for s in merkle_proof.get("siblings", [])]
    if any(len(x) != 32 for x in (rollup, wid, apk, key, vhash)) or any(len(s) != 32 for s in siblings):
        raise ValueError("all hashes and Merkle siblings must be 32 bytes")
    out = bytearray([2, 19])
    out += rollup
    out += int(batch_seq).to_bytes(8, "little")
    out += wid
    out += apk
    out += int(amount).to_bytes(8, "little")
    out += int(miner_fee).to_bytes(8, "little")
    out += key
    out += vhash
    out += len(siblings).to_bytes(2, "little")
    for s in siblings:
        out += s
    return bytes(out)


class ScopedAgentSession:
    """RFC-0025: Policy-constrained autonomous AI agent session key guard."""

    def __init__(
        self,
        session_id: str,
        max_per_call: int,
        max_per_hour: int,
        max_total: int,
        min_counterparty_bond: int = 0,
        max_counterparty_slashes: int = 0,
        allowed_endpoints: Optional[List[str]] = None,
        profit_sweep_address: str = "",
        expiry_unix: Optional[int] = None,
    ):
        self.session_id = session_id
        self.max_per_call = int(max_per_call)
        self.max_per_hour = int(max_per_hour)
        self.max_total = int(max_total)
        self.min_counterparty_bond = int(min_counterparty_bond)
        self.max_counterparty_slashes = int(max_counterparty_slashes)
        self.allowed_endpoints = allowed_endpoints
        self.profit_sweep_address = profit_sweep_address
        self.expiry_unix = int(expiry_unix) if expiry_unix is not None else int(time.time()) + 86400
        self.total_spent = 0
        self._hourly: List[Tuple[int, int]] = []

    def authorize_spend(
        self,
        amount: int,
        endpoint: str = "",
        counterparty_bond: Optional[Dict[str, Any]] = None,
        now_unix: Optional[int] = None,
    ) -> Dict[str, Any]:
        now = int(now_unix if now_unix is not None else time.time())
        if now > self.expiry_unix:
            return {"allowed": False, "reason": "agent session policy expired"}
        if amount <= 0:
            return {"allowed": False, "reason": "spend amount must be positive"}
        if amount > self.max_per_call:
            return {"allowed": False, "reason": f"amount {amount} exceeds max_per_call {self.max_per_call}"}
        if self.allowed_endpoints and not any(endpoint.startswith(p) for p in self.allowed_endpoints):
            return {"allowed": False, "reason": f"endpoint '{endpoint}' not in session allowlist"}
        if self.min_counterparty_bond > 0:
            if not counterparty_bond or not counterparty_bond.get("found"):
                return {"allowed": False, "reason": "counterparty lacks a verified L1 Trust Bond"}
            if int(counterparty_bond.get("stake", 0)) < self.min_counterparty_bond:
                return {"allowed": False, "reason": "counterparty bond stake below policy minimum"}
            if int(counterparty_bond.get("slash_count", 0)) > self.max_counterparty_slashes:
                return {"allowed": False, "reason": "counterparty slash_count exceeds policy maximum"}
        cutoff = now - 3600
        self._hourly = [(t, a) for (t, a) in self._hourly if t >= cutoff]
        hour_spent = sum(a for _, a in self._hourly)
        if hour_spent + amount > self.max_per_hour:
            return {"allowed": False, "reason": "hourly spend limit exceeded"}
        if self.total_spent + amount > self.max_total:
            return {"allowed": False, "reason": "total session budget exceeded"}
        self._hourly.append((now, amount))
        self.total_spent += amount
        digest = hashlib.sha256(
            f"nsSESS|{self.session_id}|{now}|{amount}|{self.total_spent}|{endpoint}".encode("utf-8")
        ).hexdigest()
        return {
            "allowed": True,
            "auth_digest_hex": digest,
            "total_spent": self.total_spent,
            "spent_last_hour": hour_spent + amount,
            "profit_sweep_address": self.profit_sweep_address,
        }


def verify_zktls_attestation(
    attestation: Dict[str, Any],
    expected_domain: Optional[str] = None,
    expected_status: int = 200,
    max_age_sec: int = 300,
    required_fields: Optional[Dict[str, Any]] = None,
    now_unix: Optional[int] = None,
) -> Dict[str, Any]:
    """Tier-1 Truth Engine: Canonical zkTLS Web2 attestation verifier -> L1 `receipt_hash_hex`."""
    domain = str(attestation.get("domain", "")).lower()
    if expected_domain and domain != expected_domain.lower():
        return {"valid": False, "reason": f"domain mismatch: {domain} != {expected_domain}"}
    if int(attestation.get("http_status", attestation.get("httpStatus", 0))) != int(expected_status):
        return {"valid": False, "reason": "unexpected HTTP status"}
    ts = int(attestation.get("timestamp_unix", attestation.get("timestampUnix", 0)))
    now = int(now_unix if now_unix is not None else time.time())
    if ts <= 0 or abs(now - ts) > max_age_sec:
        return {"valid": False, "reason": "attestation outside freshness window"}
    proof_digest = str(attestation.get("proof_digest_hex", attestation.get("proofDigestHex", ""))).lower()
    if len(proof_digest) != 64:
        return {"valid": False, "reason": "missing 32-byte zkTLS proof_digest_hex"}
    body_obj = attestation.get("response_body", attestation.get("responseBody", {}))
    raw_body = body_obj if isinstance(body_obj, str) else json.dumps(body_obj, separators=(",", ":"))
    body_hash = hashlib.sha256(raw_body.encode("utf-8")).hexdigest()
    want_hash = attestation.get("body_sha256_hex", attestation.get("bodySha256Hex"))
    if want_hash and str(want_hash).lower() != body_hash:
        return {"valid": False, "reason": "response body SHA-256 mismatch"}
    parsed = json.loads(raw_body) if isinstance(body_obj, str) else body_obj
    for k, expected_val in (required_fields or {}).items():
        actual = parsed.get(k) if isinstance(parsed, dict) else None
        if callable(expected_val):
            if not expected_val(actual):
                return {"valid": False, "reason": f"predicate failed for '{k}'"}
        elif actual != expected_val:
            return {"valid": False, "reason": f"field '{k}' mismatch"}
    status_val = int(attestation.get("http_status", attestation.get("httpStatus", 0)))
    receipt_hash = hashlib.sha256(
        f"nsZKTLS1|{domain}|{status_val}|{ts}|{body_hash}|{proof_digest}".encode("utf-8")
    ).hexdigest()
    return {"valid": True, "domain": domain, "timestamp_unix": ts, "body_hash_hex": body_hash, "receipt_hash_hex": receipt_hash}


# =============================================================================
# RFC-0027: Decentralized Agent Swarm Launchpads & Bonding-Curve Fee Routing
# =============================================================================


def quote_mission_curve_buy(
    curve_reserve: int, total_shares: int, curve_fee_bps: int, amount: int
) -> Dict[str, Any]:
    """Exact integer bonding-curve buy quote matching L1 `node/src/mission.rs` 1:1 (0% founder cut)."""
    import math
    if amount <= 0 or curve_fee_bps > 1000:
        raise ValueError("amount must be > 0 and curve_fee_bps <= 1000")
    fee_to_bounty = (amount * curve_fee_bps) // 10_000
    net_to_curve = amount - fee_to_bounty
    new_reserve = curve_reserve + net_to_curve
    target_shares = math.isqrt(new_reserve * 1_000)
    minted = max(1, target_shares - total_shares)
    return {
        "amount": amount,
        "fee_to_bounty_pool": fee_to_bounty,
        "founder_cut": 0,
        "net_to_curve": net_to_curve,
        "shares_minted": minted,
        "new_curve_reserve": new_reserve,
        "new_total_shares": total_shares + minted,
    }


def quote_mission_curve_redeem(
    curve_reserve: int, total_shares: int, curve_fee_bps: int, shares: int, miner_fee: int = 0
) -> Dict[str, Any]:
    """Exact integer bonding-curve share redemption quote matching L1 `node/src/mission.rs` 1:1."""
    if shares <= 0 or total_shares <= 0 or shares > total_shares:
        raise ValueError("shares must be in 1..=total_shares")
    gross = (curve_reserve * shares) // total_shares
    fee_to_bounty = (gross * curve_fee_bps) // 10_000
    after_bounty = gross - fee_to_bounty
    if miner_fee > after_bounty:
        raise ValueError("miner_fee exceeds payout")
    user_payout = after_bounty - miner_fee
    return {
        "shares_burned": shares,
        "gross_exit": gross,
        "fee_to_bounty_pool": fee_to_bounty,
        "founder_cut": 0,
        "miner_fee": miner_fee,
        "user_payout": user_payout,
        "new_curve_reserve": curve_reserve - gross,
        "new_total_shares": total_shares - shares,
    }


def mission_post_task_message_hex(mission_id_hex: str, task_id: int, spec_hash_hex: str, reward: int) -> str:
    """Compute exact L1 RFC-0027 `post_task_message` SHA-256 digest (`nsMISSTASK`)."""
    mid = bytes.fromhex(mission_id_hex)
    spec = bytes.fromhex(spec_hash_hex)
    return hashlib.sha256(
        b"nsMISSTASK" + mid + int(task_id).to_bytes(4, "little") + spec + int(reward).to_bytes(8, "little")
    ).hexdigest()


def mission_claim_task_message_hex(
    mission_id_hex: str,
    task_id: int,
    spec_hash_hex: str,
    proof_hash_hex: str,
    contributor_apk_hex: str,
    miner_fee: int,
) -> str:
    """Compute exact L1 RFC-0027 `claim_task_message` SHA-256 digest (`nsMISSCLAIM`)."""
    return hashlib.sha256(
        b"nsMISSCLAIM"
        + bytes.fromhex(mission_id_hex)
        + int(task_id).to_bytes(4, "little")
        + bytes.fromhex(spec_hash_hex)
        + bytes.fromhex(proof_hash_hex)
        + bytes.fromhex(contributor_apk_hex)
        + int(miner_fee).to_bytes(8, "little")
    ).hexdigest()


# =============================================================================
# v0.19.0: Cryptographic zkTLS Verifier, Cutover Gate & Bonded P2P Desk Helpers
# =============================================================================


def verify_zktls_proof_remote(
    payload: Dict[str, Any], commons_base_url: str = DEFAULT_COMMONS_URL, timeout: int = 12
) -> Dict[str, Any]:
    """POST a `reclaim_v1`, `tlsnotary_v1`, or `newsatoshi_pq_notary_v1` proof to `/api/zktls/verify`."""
    url = f"{commons_base_url.rstrip('/')}/api/zktls/verify"
    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode("utf-8"),
        headers={"Content-Type": "application/json", "Accept": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read(1_048_576).decode("utf-8"))


def fetch_v2_cutover_readiness(
    commons_base_url: str = DEFAULT_COMMONS_URL, timeout: int = 12
) -> Dict[str, Any]:
    """Fetch honest live Mainnet v2 cutover readiness status (`GET /api/v2/cutover-readiness`)."""
    url = f"{commons_base_url.rstrip('/')}/api/v2/cutover-readiness"
    req = urllib.request.Request(url, headers={"Accept": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read(1_048_576).decode("utf-8"))
