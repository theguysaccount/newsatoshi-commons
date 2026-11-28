#!/usr/bin/env python3
"""
Newsatoshi AuxPoW Stratum v1 Merge-Mining Server (chainid = 20051 / 0x4e53)

Zero-dependency Python 3 Stratum v1 server enabling Bitcoin SHA-256 ASICs
(Bitaxe, NerdQAxe, Antminer S19/S21, WhatsMiner, CGMiner, BFGMiner) to
merge-mine Newsatoshi (LOVE) at zero extra energy cost:

  1. Standalone AuxPoW Mode (default):
     Polls a live `newsatoshi` full node (`getauxblock` / `createauxblock`),
     constructs a canonical Namecoin-compatible AuxPoW parent coinbase
     (`\xfa\xbemm` + 32-byte aux hash + size=1 + nonce=0), serves Stratum v1
     (with BIP-320 version-rolling support) on TCP :3333, and submits winning
     blocks via `submitauxblock`.

  2. Dual Bitcoin + Newsatoshi Merge-Mining Mode (`--btc-rpc-url`):
     Polls both `bitcoind` (`getblocktemplate`) and `newsatoshi` (`getauxblock`),
     embeds the Newsatoshi AuxPoW commitment inside the real Bitcoin coinbase,
     submits Bitcoin blocks via `submitblock`, and submits every share meeting
     Newsatoshi's target via `submitauxblock`.

Zero fake data: all worker, share, and block counters start at genuine 0.
"""

import argparse
import asyncio
import base64
import hashlib
import json
import os
import struct
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional, Tuple

AUXPOW_CHAIN_ID = 20051  # 0x4e53 ("NS")
MERGED_MINING_MAGIC = b"\xfa\xbemm"
BIP320_VERSION_MASK = 0x1FFFE000
DIFF1_TARGET = 0x00000000FFFF0000000000000000000000000000000000000000000000000000
EXTRANONCE1_SIZE = 4
EXTRANONCE2_SIZE = 8


def json_diff(diff: float) -> Any:
    """Format difficulty as JSON integer when >= 1 to keep strict C/jansson ASICs happy."""
    if diff >= 1.0:
        rounded = round(diff)
        if abs(diff - rounded) < 1e-6:
            return int(rounded)
    return round(diff, 6)
MAX_CLIENTS = 256
MAX_LINE_BYTES = 4096
CLIENT_IDLE_TIMEOUT = 300.0
HTTP_STATUS_TIMEOUT = 5.0
MAX_SHARES_PER_JOB = 50_000


def sha256d(data: bytes) -> bytes:
    return hashlib.sha256(hashlib.sha256(data).digest()).digest()


def encode_compact_size(n: int) -> bytes:
    if n < 0xFD:
        return bytes([n])
    if n <= 0xFFFF:
        return b"\xfd" + struct.pack("<H", n)
    if n <= 0xFFFFFFFF:
        return b"\xfe" + struct.pack("<I", n)
    return b"\xff" + struct.pack("<Q", n)


def encode_bip34_height(height: int) -> bytes:
    """Minimal BIP-34 scriptSig block height push."""
    if height == 0:
        return b"\x00"
    if 1 <= height <= 16:
        return bytes([0x50 + height])
    raw = bytearray()
    v = height
    while v > 0:
        raw.append(v & 0xFF)
        v >>= 8
    if raw[-1] & 0x80:
        raw.append(0x00)
    return bytes([len(raw)]) + bytes(raw)


def prevhash_to_stratum(display_hex: str) -> str:
    """Convert 32-byte display-order hex hash into Stratum v1 prevhash encoding."""
    internal = bytes.fromhex(display_hex)[::-1]
    swapped = b"".join(internal[i : i + 4][::-1] for i in range(0, 32, 4))
    return swapped.hex()


def stratum_prevhash_to_internal(stratum_hex: str) -> bytes:
    """Convert Stratum v1 prevhash back to 32-byte little-endian header order."""
    raw = bytes.fromhex(stratum_hex)
    if len(raw) != 32:
        raise ValueError("stratum prevhash must be 32 bytes")
    return b"".join(raw[i : i + 4][::-1] for i in range(0, 32, 4))


def merkle_branch_for_coinbase(txids_internal: List[bytes]) -> List[bytes]:
    """Compute the Bitcoin merkle branch for tx index 0 given txids[1:] in internal order."""
    if not txids_internal:
        return []
    branch: List[bytes] = []
    # Index 0 is the coinbase placeholder (we only need its sibling at each level).
    level: List[Optional[bytes]] = [None] + list(txids_internal)
    while len(level) > 1:
        if len(level) % 2 == 1:
            level.append(level[-1])
        sibling = level[1]
        assert sibling is not None
        branch.append(sibling)
        next_level: List[Optional[bytes]] = [None]
        for i in range(2, len(level), 2):
            left = level[i]
            right = level[i + 1]
            assert left is not None and right is not None
            next_level.append(sha256d(left + right))
        level = next_level
    return branch


def fold_merkle_root(coinbase_txid_internal: bytes, branch_internal: List[bytes]) -> bytes:
    cur = coinbase_txid_internal
    for sibling in branch_internal:
        cur = sha256d(cur + sibling)
    return cur


def difficulty_to_target(diff: float) -> int:
    if diff <= 0:
        return DIFF1_TARGET
    return max(1, int(DIFF1_TARGET / diff))


def build_aux_coinbase_parts(
    aux_hash_display_hex: str,
    bip34_height: int = 1,
    coinbase_tag: bytes = b"/newsatoshi-auxpow/",
    payout_script: bytes = b"\x51",
    coinbase_value_sats: int = 312_500_000,
    extra_outputs: Optional[List[Tuple[int, bytes]]] = None,
) -> Tuple[bytes, bytes]:
    """
    Construct Stratum `(coinb1, coinb2)` embedding the Namecoin/Newsatoshi AuxPoW commitment:
      `fabe6d6d || aux_hash_display(32) || merkle_size(1u32 LE) || merkle_nonce(0u32 LE)`
    immediately before `extranonce1 (4B) || extranonce2 (4B)` inside coinbase scriptSig.
    """
    aux_root = bytes.fromhex(aux_hash_display_hex)
    if len(aux_root) != 32:
        raise ValueError("aux_hash must be 32 bytes (64 hex chars)")

    bip34 = encode_bip34_height(bip34_height)
    mm_commit = MERGED_MINING_MAGIC + aux_root + struct.pack("<II", 1, 0)
    script_prefix = bip34 + mm_commit
    script_suffix = coinbase_tag[:24]
    total_script_len = len(script_prefix) + EXTRANONCE1_SIZE + EXTRANONCE2_SIZE + len(script_suffix)
    if total_script_len > 100:
        raise ValueError(f"coinbase scriptSig length {total_script_len} exceeds Bitcoin 100-byte consensus limit")

    coinb1 = (
        struct.pack("<I", 2)  # nVersion = 2
        + b"\x01"  # 1 input
        + b"\x00" * 32  # null prevout
        + b"\xff\xff\xff\xff"  # prevout index
        + encode_compact_size(total_script_len)
        + script_prefix
    )

    outputs: List[Tuple[int, bytes]] = [(coinbase_value_sats, payout_script)]
    if extra_outputs:
        outputs.extend(extra_outputs)

    out_bytes = bytearray()
    out_bytes.extend(encode_compact_size(len(outputs)))
    for value_sats, script_pk in outputs:
        out_bytes.extend(struct.pack("<Q", value_sats))
        out_bytes.extend(encode_compact_size(len(script_pk)))
        out_bytes.extend(script_pk)

    coinb2 = (
        script_suffix
        + b"\xff\xff\xff\xff"  # nSequence
        + bytes(out_bytes)
        + struct.pack("<I", 0)  # nLockTime
    )
    return coinb1, coinb2


def serialize_auxpow(
    coinbase: bytes,
    coinbase_branch_internal: List[bytes],
    parent_header_80: bytes,
) -> bytes:
    """Serialize Namecoin/Newsatoshi AuxPoW proof accepted by `newsatoshi submitauxblock`."""
    if len(parent_header_80) != 80:
        raise ValueError("parent header must be exactly 80 bytes")
    out = bytearray(coinbase)
    out.extend(b"\x00" * 32)  # unused_hash
    out.extend(encode_compact_size(len(coinbase_branch_internal)))
    for h in coinbase_branch_internal:
        out.extend(h)
    out.extend(struct.pack("<i", 0))  # coinbase_index = 0
    out.extend(b"\x00")  # chain_branch len = 0 (single-chain tree)
    out.extend(struct.pack("<i", 0))  # chain_index = 0
    out.extend(parent_header_80)
    return bytes(out)


class JsonRpcClient:
    def __init__(self, url: str, user_pass: str, timeout: float = 10.0) -> None:
        self.url = url
        self.auth_header = "Basic " + base64.b64encode(user_pass.encode("utf-8")).decode("ascii")
        self.timeout = timeout

    def call(self, method: str, params: Optional[List[Any]] = None) -> Any:
        payload = json.dumps(
            {"jsonrpc": "1.0", "id": "auxpow-stratum", "method": method, "params": params or []}
        ).encode("utf-8")
        req = urllib.request.Request(
            self.url,
            data=payload,
            headers={
                "Authorization": self.auth_header,
                "Content-Type": "application/json",
            },
        )
        try:
            with urllib.request.urlopen(req, timeout=self.timeout) as resp:
                data = json.loads(resp.read().decode("utf-8"))
        except urllib.error.HTTPError as exc:
            body = exc.read().decode("utf-8", errors="replace")
            try:
                data = json.loads(body)
            except Exception as parse_err:
                raise RuntimeError(f"RPC HTTP {exc.code}: {body}") from parse_err
        if data.get("error"):
            raise RuntimeError(f"RPC {method} error: {data['error']}")
        return data.get("result")


@dataclass
class StratumJob:
    job_id: str
    aux_hash: str
    aux_height: int
    aux_bits: str
    aux_target_int: int
    aux_coinbase_value: int
    parent_version: int
    parent_prevhash_stratum: str
    parent_nbits: int
    parent_ntime: int
    parent_target_int: Optional[int]
    coinb1: bytes
    coinb2: bytes
    merkle_branch_internal: List[bytes]
    btc_raw_txs: List[bytes] = field(default_factory=list)
    created_at: float = field(default_factory=time.time)
    submitted_shares: set = field(default_factory=set)
    clean_jobs: bool = True

    def notify_params(self, clean_jobs: bool = True) -> List[Any]:
        return [
            self.job_id,
            self.parent_prevhash_stratum,
            self.coinb1.hex(),
            self.coinb2.hex(),
            [h.hex() for h in self.merkle_branch_internal],
            f"{self.parent_version:08x}",
            f"{self.parent_nbits:08x}",
            f"{self.parent_ntime:08x}",
            clean_jobs,
        ]


def extract_payout_address(worker_name: str) -> Optional[str]:
    """Extract a valid Newsatoshi L1 payout address from a Stratum username (`<ns1...>.rig`)."""
    raw = (worker_name or "").strip()
    if not raw:
        return None
    prefix = raw.split(".", 1)[0].strip()
    if (
        prefix.startswith(("ns1", "nsd1", "nst1"))
        and 20 <= len(prefix) <= 140
        and prefix.isascii()
        and prefix.isalnum()
    ):
        return prefix
    return None


def mask_worker_label(worker_name: Optional[str]) -> str:
    raw = (worker_name or "worker").strip()
    if "." in raw:
        addr, rig = raw.split(".", 1)
        if extract_payout_address(addr):
            return f"{addr[:6]}…{addr[-4:]}.{rig[:24]}"
    if extract_payout_address(raw):
        return f"{raw[:6]}…{raw[-4:]}"
    return raw[:32]


def parse_password_difficulty(password: str) -> Optional[float]:
    """Parse standard Stratum password difficulty hint such as `d=4096` or `x,d=65536`."""
    raw = (password or "").strip()
    if not raw:
        return None
    for part in raw.replace(";", ",").split(","):
        tok = part.strip()
        if tok.lower().startswith("d="):
            try:
                val = float(tok[2:].strip())
                if val > 0 and val == val and val < 1e18:
                    return val
            except ValueError:
                continue
    return None


class AuxPowStratumServer:
    def __init__(
        self,
        ns_rpc: JsonRpcClient,
        payout_address: Optional[str] = None,
        btc_rpc: Optional[JsonRpcClient] = None,
        btc_payout_script: bytes = b"\x51",
        coinbase_tag: bytes = b"/newsatoshi-auxpow/",
        poll_interval: float = 2.0,
        worker_diff: Optional[float] = None,
        public_host: Optional[str] = None,
        stratum_port: int = 3333,
    ) -> None:
        self.ns_rpc = ns_rpc
        self.payout_address = payout_address
        self.btc_rpc = btc_rpc
        self.btc_payout_script = btc_payout_script
        self.coinbase_tag = coinbase_tag
        self.poll_interval = poll_interval
        self.configured_worker_diff = worker_diff
        self.public_host = public_host or "pool.newsatoshi.org"
        self.stratum_port = stratum_port

        self.jobs: Dict[str, StratumJob] = {}
        self.jobs_by_payout: Dict[Optional[str], StratumJob] = {}
        self.current_job: Optional[StratumJob] = None
        self.job_seq = 0
        self.extranonce_seq = 0
        self.clients: Dict[int, Dict[str, Any]] = {}

        # Genuine zero-initial-state telemetry counters (zero fake data)
        self.started_at = int(time.time())
        self.shares_accepted = 0
        self.shares_rejected = 0
        self.aux_blocks_submitted = 0
        self.aux_blocks_accepted = 0
        self.btc_blocks_accepted = 0
        self.recent_share_diffs: List[Tuple[float, float]] = []
        self.last_aux_hash: Optional[str] = None
        self.last_error: Optional[str] = None

    def _next_extranonce1(self) -> str:
        self.extranonce_seq += 1
        return f"{self.extranonce_seq:08x}"

    def _effective_worker_diff(self, job: StratumJob, client: Optional[Dict[str, Any]] = None) -> float:
        aux_diff = DIFF1_TARGET / max(1, job.aux_target_int)
        # Never exceed aux_diff (so every valid Newsatoshi block is always submitted)
        max_safe = max(aux_diff, 1e-6)
        if client is not None:
            requested = client.get("suggested_diff")
            if requested is not None and requested > 0:
                return max(1e-6, min(float(requested), max_safe))
            vardiff = client.get("vardiff")
            if vardiff is not None and vardiff > 0:
                return max(1e-6, min(float(vardiff), max_safe))
        if self.configured_worker_diff is not None and self.configured_worker_diff > 0:
            return min(self.configured_worker_diff, max_safe)
        # Default 16.0 for small Bitaxes; high-TH/s ASICs & MRR/NiceHash auto-scale via VarDiff or d=/suggest_difficulty
        return max(1e-6, min(aux_diff, 16.0))

    def _maybe_adjust_vardiff(self, client: Dict[str, Any], job: StratumJob) -> Optional[float]:
        """Auto-scale per-client difficulty targeting ~1 share every 5s (MRR / NiceHash / TH/s safe)."""
        if client.get("suggested_diff") is not None:
            return None
        now = time.time()
        times: List[float] = client.setdefault("share_times", [])
        times.append(now)
        # Keep a 15-second sliding window
        cutoff = now - 15.0
        while times and times[0] < cutoff:
            times.pop(0)
        last_adj = float(client.get("last_vardiff_at", 0.0))
        if len(times) < 8 or (now - last_adj) < 5.0:
            return None
        span = max(1.0, now - times[0])
        rate = len(times) / span  # shares/sec
        cur_diff = float(client.get("difficulty", 16.0))
        aux_diff = DIFF1_TARGET / max(1, job.aux_target_int)
        # Target ~0.2 shares/sec (1 share per 5s)
        if rate > 0.6:
            scale = min(16.0, max(2.0, rate / 0.2))
            new_diff = min(max(aux_diff, 16.0), round(cur_diff * scale, 4))
            if new_diff > cur_diff * 1.25:
                client["prev_difficulty"] = cur_diff
                client["prev_diff_until"] = now + 15.0
                client["vardiff"] = new_diff
                client["difficulty"] = new_diff
                client["last_vardiff_at"] = now
                times.clear()
                return new_diff
        return None

    def refresh_template_sync(self, payout_override: Optional[str] = None) -> Optional[StratumJob]:
        target_payout = payout_override or self.payout_address
        if target_payout:
            aux = self.ns_rpc.call("createauxblock", [target_payout])
        else:
            aux = self.ns_rpc.call("getauxblock", [])

        aux_hash = aux["hash"]
        aux_height = int(aux["height"])
        aux_bits = aux["bits"]
        aux_target_int = int.from_bytes(bytes.fromhex(aux["_target"]), "little")
        aux_coinbase_value = int(aux["coinbasevalue"])

        prev_job = self.jobs_by_payout.get(payout_override)
        same_tip = (
            prev_job is not None
            and prev_job.aux_height == aux_height
            and prev_job.aux_bits == aux_bits
        )
        # Keep the same job for 30s while tip height & bits are unchanged:
        # `createauxblock` bumps nTime on every call, so comparing aux_hash
        # directly would blast clean_jobs=True every 2s and abort ASIC pipelines.
        if same_tip and (time.time() - prev_job.created_at) < 30.0 and self.btc_rpc is None:
            return None
        clean_jobs = (not same_tip) or (self.btc_rpc is not None)

        btc_raw_txs: List[bytes] = []
        merkle_branch: List[bytes] = []
        parent_target_int: Optional[int] = None

        if self.btc_rpc is not None:
            gbt = self.btc_rpc.call("getblocktemplate", [{"rules": ["segwit"]}])
            parent_version = int(gbt["version"])
            parent_prevhash_stratum = prevhash_to_stratum(gbt["previousblockhash"])
            parent_nbits = int(gbt["bits"], 16)
            parent_ntime = int(gbt["curtime"])
            parent_target_int = int(gbt["target"], 16)
            bip34_height = int(gbt["height"])
            cb_val = int(gbt["coinbasevalue"])
            extra_outputs: List[Tuple[int, bytes]] = []
            if gbt.get("default_witness_commitment"):
                extra_outputs.append((0, bytes.fromhex(gbt["default_witness_commitment"])))
            txids_internal: List[bytes] = []
            for tx in gbt.get("transactions", []):
                tx_bytes = bytes.fromhex(tx["data"])
                btc_raw_txs.append(tx_bytes)
                txid_hex = tx.get("txid") or tx.get("hash")
                txids_internal.append(bytes.fromhex(txid_hex)[::-1])
            merkle_branch = merkle_branch_for_coinbase(txids_internal)
            coinb1, coinb2 = build_aux_coinbase_parts(
                aux_hash_display_hex=aux_hash,
                bip34_height=bip34_height,
                coinbase_tag=self.coinbase_tag,
                payout_script=self.btc_payout_script,
                coinbase_value_sats=cb_val,
                extra_outputs=extra_outputs,
            )
        else:
            # Standalone AuxPoW Solo-LOVE mode (parent header crafted solely for AuxPoW verification)
            parent_version = 0x20000000
            parent_prevhash_stratum = prevhash_to_stratum("11" * 32)
            parent_nbits = int(aux_bits, 16)
            parent_ntime = int(time.time())
            coinb1, coinb2 = build_aux_coinbase_parts(
                aux_hash_display_hex=aux_hash,
                bip34_height=max(1, aux_height),
                coinbase_tag=self.coinbase_tag,
                payout_script=self.btc_payout_script,
                coinbase_value_sats=312_500_000,
            )

        self.job_seq += 1
        job_id = f"{self.job_seq:08x}"
        job = StratumJob(
            job_id=job_id,
            aux_hash=aux_hash,
            aux_height=aux_height,
            aux_bits=aux_bits,
            aux_target_int=aux_target_int,
            aux_coinbase_value=aux_coinbase_value,
            parent_version=parent_version,
            parent_prevhash_stratum=parent_prevhash_stratum,
            parent_nbits=parent_nbits,
            parent_ntime=parent_ntime,
            parent_target_int=parent_target_int,
            coinb1=coinb1,
            coinb2=coinb2,
            merkle_branch_internal=merkle_branch,
            btc_raw_txs=btc_raw_txs,
            clean_jobs=clean_jobs,
        )
        self.jobs[job_id] = job
        # Keep the latest 256 jobs in RAM so in-flight ASIC shares across refreshes stay valid
        if len(self.jobs) > 256:
            oldest = sorted(self.jobs.keys())[:-256]
            for k in oldest:
                self.jobs.pop(k, None)
        self.jobs_by_payout[payout_override] = job
        if payout_override is None:
            self.current_job = job
        self.last_aux_hash = aux_hash
        self.last_error = None
        return job

    def estimated_hashrate_ghs(self) -> float:
        now = time.time()
        cutoff = now - 300.0
        self.recent_share_diffs = [(ts, d) for ts, d in self.recent_share_diffs if ts >= cutoff]
        if not self.recent_share_diffs:
            return 0.0
        window = max(30.0, min(300.0, now - self.started_at))
        total_diff = sum(d for _, d in self.recent_share_diffs)
        return round((total_diff * 4_294_967_296.0) / window / 1e9, 4)

    def status_dict(self) -> Dict[str, Any]:
        job = self.current_job
        now = time.time()
        unique_payouts = {
            c.get("payout_address")
            for c in self.clients.values()
            if c.get("payout_address")
        }
        workers_public = [
            {
                "worker": mask_worker_label(c.get("worker")),
                "custom_payout": bool(c.get("payout_address")),
                "difficulty": c.get("difficulty", 1.0),
                "shares_accepted": c.get("shares_accepted", 0),
                "connected_secs": max(0, int(now - c.get("connected_at", now))),
            }
            for c in list(self.clients.values())[:64]
        ]
        net_diff = round(DIFF1_TARGET / max(1, job.aux_target_int), 4) if job else None
        return {
            "ok": self.last_error is None,
            "online": True,
            "protocol": "NEWSATOSHI-AUXPOW-STRATUM/1.1",
            "stratum_url": f"stratum+tcp://{self.public_host}:{self.stratum_port}",
            "auxpow_chainid": AUXPOW_CHAIN_ID,
            "pool_fee_pct": 0.0,
            "payout_mode": "direct_solo_coinbase",
            "mode": "dual_btc_love" if self.btc_rpc is not None else "standalone_auxpow_solo",
            "uptime_secs": max(0, int(now) - self.started_at),
            "connected_workers": len(self.clients),
            "unique_payout_addresses": len(unique_payouts),
            "estimated_pool_hashrate_ghs": self.estimated_hashrate_ghs(),
            "shares_accepted": self.shares_accepted,
            "shares_rejected": self.shares_rejected,
            "aux_blocks_submitted": self.aux_blocks_submitted,
            "aux_blocks_accepted": self.aux_blocks_accepted,
            "btc_blocks_accepted": self.btc_blocks_accepted,
            "active_workers": workers_public,
            "current_template": {
                "job_id": job.job_id,
                "aux_hash": job.aux_hash,
                "aux_height": job.aux_height,
                "aux_bits": job.aux_bits,
                "network_difficulty": net_diff,
                "coinbase_value_love": round(job.aux_coinbase_value / 1e8, 4),
                "worker_difficulty": self._effective_worker_diff(job),
            }
            if job
            else None,
            "last_error": self.last_error,
        }

    async def _send_json(self, writer: asyncio.StreamWriter, obj: Dict[str, Any]) -> None:
        line = (json.dumps(obj, separators=(",", ":")) + "\n").encode("utf-8")
        writer.write(line)
        await writer.drain()

    async def _broadcast_job(
        self,
        job: StratumJob,
        clean_jobs: bool = True,
        payout_filter: Optional[str] = None,
    ) -> None:
        dead: List[int] = []
        for cid, c in list(self.clients.items()):
            if not c.get("subscribed"):
                continue
            if c.get("payout_address") != payout_filter:
                continue
            try:
                diff = self._effective_worker_diff(job, c)
                prev = float(c.get("difficulty", diff))
                if diff != prev:
                    c["prev_difficulty"] = prev
                    c["prev_diff_until"] = time.time() + 15.0
                c["difficulty"] = diff
                await self._send_json(
                    c["writer"], {"id": None, "method": "mining.set_difficulty", "params": [json_diff(diff)]}
                )
                await self._send_json(
                    c["writer"],
                    {"id": None, "method": "mining.notify", "params": job.notify_params(clean_jobs)},
                )
            except Exception:
                dead.append(cid)
        for cid in dead:
            self.clients.pop(cid, None)

    async def poll_loop(self) -> None:
        while True:
            try:
                payout_keys: List[Optional[str]] = [None]
                seen_custom = {
                    c.get("payout_address")
                    for c in list(self.clients.values())
                    if c.get("payout_address")
                }
                payout_keys.extend(sorted(seen_custom))
                # Prune cached jobs for disconnected payout addresses
                active_set = set(payout_keys)
                for stale_key in list(self.jobs_by_payout.keys()):
                    if stale_key not in active_set:
                        self.jobs_by_payout.pop(stale_key, None)
                for pkey in payout_keys:
                    try:
                        new_job = await asyncio.to_thread(self.refresh_template_sync, pkey)
                        if new_job is not None:
                            await self._broadcast_job(
                                new_job, clean_jobs=new_job.clean_jobs, payout_filter=pkey
                            )
                    except Exception as exc:
                        if pkey is None:
                            self.last_error = str(exc)
            except Exception as exc:
                self.last_error = str(exc)
            await asyncio.sleep(self.poll_interval)

    async def handle_stratum_client(
        self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter
    ) -> None:
        if len(self.clients) >= MAX_CLIENTS:
            try:
                writer.close()
                await writer.wait_closed()
            except Exception:
                pass
            return

        cid = id(writer)
        extranonce1 = self._next_extranonce1()
        client_state: Dict[str, Any] = {
            "writer": writer,
            "extranonce1": extranonce1,
            "subscribed": False,
            "authorized": False,
            "worker": None,
            "payout_address": None,
            "difficulty": 1.0,
            "version_mask": 0,
            "shares_accepted": 0,
            "connected_at": time.time(),
        }
        self.clients[cid] = client_state

        try:
            while not reader.at_eof():
                try:
                    raw_line = await asyncio.wait_for(
                        reader.readline(), timeout=CLIENT_IDLE_TIMEOUT
                    )
                except (asyncio.TimeoutError, ValueError):
                    break
                if not raw_line:
                    break
                if len(raw_line) > MAX_LINE_BYTES:
                    break
                line = raw_line.decode("utf-8", errors="replace").strip()
                if not line:
                    continue
                try:
                    msg = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if not isinstance(msg, dict):
                    continue

                req_id = msg.get("id")
                method = msg.get("method", "")
                params = msg.get("params") or []
                if not isinstance(method, str) or not isinstance(params, list):
                    await self._send_json(
                        writer, {"id": req_id, "result": None, "error": [20, "invalid stratum frame", None]}
                    )
                    continue

                if method == "mining.configure":
                    # BIP-310 / BIP-320 version-rolling negotiation for Bitaxe / Antminer / WhatsMiner / MRR
                    extensions = params[0] if len(params) > 0 and isinstance(params[0], list) else []
                    ext_params = params[1] if len(params) > 1 and isinstance(params[1], dict) else {}
                    res: Dict[str, Any] = {}
                    granted_mask: Optional[int] = None
                    if "version-rolling" in extensions:
                        try:
                            raw_mask = ext_params.get("version-rolling.mask", f"{BIP320_VERSION_MASK:08x}")
                            req_mask = int(str(raw_mask), 16)
                        except (ValueError, TypeError):
                            await self._send_json(
                                writer,
                                {"id": req_id, "result": None, "error": [20, "invalid version-rolling.mask", None]},
                            )
                            continue
                        granted_mask = req_mask & BIP320_VERSION_MASK
                        client_state["version_mask"] = granted_mask
                        res["version-rolling"] = True
                        res["version-rolling.mask"] = f"{granted_mask:08x}"
                    if "minimum-difficulty" in extensions:
                        res["minimum-difficulty"] = True
                        try:
                            min_d = float(ext_params.get("minimum-difficulty.value", 0))
                            if min_d > 0 and min_d == min_d and min_d < 1e18:
                                client_state["suggested_diff"] = max(
                                    float(client_state.get("suggested_diff") or 0.0), min_d
                                )
                        except (ValueError, TypeError):
                            pass
                    if "subscribe-extranonce" in extensions:
                        res["subscribe-extranonce"] = True
                    print(
                        f"[auxpow-stratum] configure cid={extranonce1} req={extensions} res={res}",
                        flush=True,
                    )
                    await self._send_json(writer, {"id": req_id, "result": res, "error": None})
                    if granted_mask is not None:
                        await self._send_json(
                            writer,
                            {"id": None, "method": "mining.set_version_mask", "params": [f"{granted_mask:08x}"]},
                        )

                elif method == "mining.subscribe":
                    client_state["subscribed"] = True
                    sub_id = f"ns-{extranonce1}"
                    ua = str(params[0])[:64] if params else ""
                    print(
                        f"[auxpow-stratum] subscribe cid={extranonce1} ua={ua!r} en2={EXTRANONCE2_SIZE}",
                        flush=True,
                    )
                    await self._send_json(
                        writer,
                        {
                            "id": req_id,
                            "result": [
                                [["mining.set_difficulty", sub_id], ["mining.notify", sub_id]],
                                extranonce1,
                                EXTRANONCE2_SIZE,
                            ],
                            "error": None,
                        },
                    )

                elif method == "mining.suggest_difficulty":
                    if params:
                        try:
                            s_diff = float(params[0])
                            if s_diff > 0 and s_diff == s_diff and s_diff < 1e18:
                                prev = float(client_state.get("difficulty", 16.0))
                                client_state["prev_difficulty"] = prev
                                client_state["prev_diff_until"] = time.time() + 35.0
                                client_state["suggested_diff"] = s_diff
                        except (ValueError, TypeError):
                            pass
                    print(
                        f"[auxpow-stratum] suggest_difficulty cid={extranonce1} "
                        f"worker={mask_worker_label(client_state.get('worker'))} params={params[:2]}",
                        flush=True,
                    )
                    await self._send_json(writer, {"id": req_id, "result": True, "error": None})
                    if client_state.get("authorized"):
                        active_job = self.jobs_by_payout.get(client_state.get("payout_address")) or self.current_job
                        if active_job is not None:
                            diff = self._effective_worker_diff(active_job, client_state)
                            client_state["difficulty"] = diff
                            await self._send_json(
                                writer, {"id": None, "method": "mining.set_difficulty", "params": [json_diff(diff)]}
                            )
                            await self._send_json(
                                writer,
                                {
                                    "id": None,
                                    "method": "mining.notify",
                                    "params": active_job.notify_params(False),
                                },
                            )

                elif method == "mining.authorize":
                    worker_name = str(params[0]) if params else "worker"
                    worker_pass = str(params[1]) if len(params) > 1 and params[1] is not None else ""
                    client_state["authorized"] = True
                    client_state["worker"] = worker_name
                    pass_diff = parse_password_difficulty(worker_pass)
                    if pass_diff is not None:
                        prev = float(client_state.get("difficulty", 16.0))
                        client_state["prev_difficulty"] = prev
                        client_state["prev_diff_until"] = time.time() + 15.0
                        client_state["suggested_diff"] = pass_diff
                    custom_addr = extract_payout_address(worker_name)
                    await self._send_json(writer, {"id": req_id, "result": True, "error": None})
                    target_job = self.current_job
                    if custom_addr:
                        try:
                            custom_job = self.jobs_by_payout.get(custom_addr)
                            if custom_job is None or (time.time() - custom_job.created_at) >= 15.0:
                                custom_job = await asyncio.to_thread(
                                    self.refresh_template_sync, custom_addr
                                ) or self.jobs_by_payout.get(custom_addr)
                            if custom_job is not None:
                                client_state["payout_address"] = custom_addr
                                target_job = custom_job
                        except Exception as exc:
                            print(
                                f"[auxpow-stratum] custom payout '{mask_worker_label(worker_name)}' fallback: {exc}",
                                flush=True,
                            )
                    if target_job is not None:
                        diff = self._effective_worker_diff(target_job, client_state)
                        client_state["difficulty"] = diff
                        print(
                            f"[auxpow-stratum] authorized worker={mask_worker_label(worker_name)} "
                            f"custom_payout={bool(client_state.get('payout_address'))} diff={diff}",
                            flush=True,
                        )
                        await self._send_json(
                            writer,
                            {"id": None, "method": "mining.set_difficulty", "params": [json_diff(diff)]},
                        )
                        await self._send_json(
                            writer,
                            {
                                "id": None,
                                "method": "mining.notify",
                                "params": target_job.notify_params(True),
                            },
                        )

                elif method == "mining.submit":
                    ok, err = await self._process_submit(client_state, params)
                    await self._send_json(writer, {"id": req_id, "result": ok, "error": err})
                    if ok:
                        active_job = self.jobs_by_payout.get(client_state.get("payout_address")) or self.current_job
                        if active_job is not None:
                            new_diff = self._maybe_adjust_vardiff(client_state, active_job)
                            if new_diff is not None:
                                await self._send_json(
                                    writer, {"id": None, "method": "mining.set_difficulty", "params": [json_diff(new_diff)]}
                                )

                elif method == "mining.extranonce.subscribe":
                    print(f"[auxpow-stratum] extranonce.subscribe cid={extranonce1}", flush=True)
                    await self._send_json(writer, {"id": req_id, "result": True, "error": None})

                else:
                    print(
                        f"[auxpow-stratum] unknown method={method!r} cid={extranonce1} params={str(params)[:120]}",
                        flush=True,
                    )
                    await self._send_json(
                        writer, {"id": req_id, "result": None, "error": [20, f"unknown method {method}", None]}
                    )
        except Exception:
            pass
        finally:
            self.clients.pop(cid, None)
            try:
                writer.close()
                await writer.wait_closed()
            except Exception:
                pass

    async def _process_submit(
        self, client: Dict[str, Any], params: List[Any]
    ) -> Tuple[bool, Optional[List[Any]]]:
        if len(params) < 5:
            self.shares_rejected += 1
            return False, [20, "invalid submit params", None]

        worker, job_id, extranonce2_hex, ntime_hex, nonce_hex = (
            str(params[0])[:128],
            str(params[1]),
            str(params[2]),
            str(params[3]),
            str(params[4]),
        )
        version_bits_hex = str(params[5]) if len(params) >= 6 and params[5] is not None else None

        job = self.jobs.get(job_id)
        if job is None:
            self.shares_rejected += 1
            print(
                f"[auxpow-stratum] REJECT worker={mask_worker_label(worker)} reason=stale_job job={job_id}",
                flush=True,
            )
            return False, [21, "job not found (stale)", None]

        try:
            en1 = bytes.fromhex(client["extranonce1"])
            en2 = bytes.fromhex(extranonce2_hex)
            if len(en2) != EXTRANONCE2_SIZE:
                raise ValueError("bad extranonce2 size")
            ntime = int(ntime_hex, 16)
            nonce = int(nonce_hex, 16)
            if not (0 <= ntime <= 0xFFFFFFFF and 0 <= nonce <= 0xFFFFFFFF):
                raise ValueError("ntime/nonce out of uint32 range")
            version = job.parent_version
            if version_bits_hex:
                vb = int(version_bits_hex, 16)
                mask = client.get("version_mask") or BIP320_VERSION_MASK
                version = (job.parent_version & ~mask) | (vb & mask)
            # Never allow parent header chain_id == AUXPOW_CHAIN_ID
            if (version >> 16) == AUXPOW_CHAIN_ID:
                raise ValueError("parent version collides with NEWSATOSHI chain_id")
        except Exception as exc:
            self.shares_rejected += 1
            print(
                f"[auxpow-stratum] REJECT worker={mask_worker_label(worker)} reason=malformed({exc}) "
                f"en2={extranonce2_hex} vb={version_bits_hex}",
                flush=True,
            )
            return False, [20, f"malformed share: {exc}", None]

        # Canonical per-job deduplication key (normalized lowercase + uint32 values;
        # evicted only when the job itself leaves the 16-job window, preventing intra-job replay).
        share_key = (client["extranonce1"], en2, ntime, nonce, version)
        if share_key in job.submitted_shares:
            self.shares_rejected += 1
            return False, [22, "duplicate share", None]
        if len(job.submitted_shares) >= MAX_SHARES_PER_JOB:
            self.shares_rejected += 1
            return False, [20, "job share cap reached", None]
        job.submitted_shares.add(share_key)

        coinbase = job.coinb1 + en1 + en2 + job.coinb2
        cb_txid = sha256d(coinbase)
        merkle_root = fold_merkle_root(cb_txid, job.merkle_branch_internal)
        prev_internal = stratum_prevhash_to_internal(job.parent_prevhash_stratum)
        parent_header = (
            struct.pack("<I", version)
            + prev_internal
            + merkle_root
            + struct.pack("<III", ntime, job.parent_nbits, nonce)
        )
        header_hash = sha256d(parent_header)
        hash_int = int.from_bytes(header_hash, "little")

        cur_diff = float(client.get("difficulty", 1.0))
        effective_accept_diff = cur_diff
        prev_diff = client.get("prev_difficulty")
        if prev_diff is not None and time.time() <= float(client.get("prev_diff_until", 0.0)):
            effective_accept_diff = min(cur_diff, float(prev_diff))
        worker_target = difficulty_to_target(effective_accept_diff)
        if hash_int > worker_target and hash_int > job.aux_target_int:
            self.shares_rejected += 1
            share_diff = round(DIFF1_TARGET / max(1, hash_int), 4)
            print(
                f"[auxpow-stratum] REJECT worker={mask_worker_label(worker)} reason=low_diff "
                f"share_diff={share_diff} req_diff={effective_accept_diff} vb={version_bits_hex} "
                f"ver={version:08x} job={job_id}",
                flush=True,
            )
            return False, [23, "low difficulty share", None]

        self.shares_accepted += 1
        client["shares_accepted"] = int(client.get("shares_accepted", 0)) + 1
        self.recent_share_diffs.append((time.time(), effective_accept_diff))

        # Check Newsatoshi AuxPoW target!
        if hash_int <= job.aux_target_int:
            auxpow_bytes = serialize_auxpow(coinbase, job.merkle_branch_internal, parent_header)
            self.aux_blocks_submitted += 1
            try:
                accepted = await asyncio.to_thread(
                    self.ns_rpc.call, "submitauxblock", [job.aux_hash, auxpow_bytes.hex()]
                )
                if accepted:
                    self.aux_blocks_accepted += 1
                    print(
                        f"[auxpow-stratum] ACCEPTED Newsatoshi block #{job.aux_height} "
                        f"hash={job.aux_hash[:16]}... worker={worker}",
                        flush=True,
                    )
                    # Immediately fetch next template for workers
                    pkey = client.get("payout_address")
                    new_job = await asyncio.to_thread(self.refresh_template_sync, pkey)
                    if new_job is not None:
                        await self._broadcast_job(new_job, clean_jobs=True, payout_filter=pkey)
                else:
                    print(
                        f"[auxpow-stratum] Newsatoshi node rejected aux block {job.aux_hash[:16]}...",
                        flush=True,
                    )
            except Exception as exc:
                self.last_error = f"submitauxblock failed: {exc}"
                print(f"[auxpow-stratum] {self.last_error}", flush=True)

        # Check Bitcoin parent block target (when running in dual BTC+LOVE mode)
        if self.btc_rpc is not None and job.parent_target_int is not None and hash_int <= job.parent_target_int:
            full_block = (
                parent_header
                + encode_compact_size(1 + len(job.btc_raw_txs))
                + coinbase
                + b"".join(job.btc_raw_txs)
            )
            try:
                btc_res = await asyncio.to_thread(self.btc_rpc.call, "submitblock", [full_block.hex()])
                if btc_res is None:
                    self.btc_blocks_accepted += 1
                    print(f"[auxpow-stratum] ACCEPTED Bitcoin parent block! worker={worker}", flush=True)
            except Exception as exc:
                print(f"[auxpow-stratum] bitcoind submitblock error: {exc}", flush=True)

        return True, None

    async def handle_http_status(
        self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter
    ) -> None:
        try:
            try:
                await asyncio.wait_for(reader.read(2048), timeout=HTTP_STATUS_TIMEOUT)
            except asyncio.TimeoutError:
                return
            body = json.dumps(self.status_dict(), indent=2).encode("utf-8")
            resp = (
                b"HTTP/1.1 200 OK\r\n"
                b"Content-Type: application/json\r\n"
                b"X-Content-Type-Options: nosniff\r\n"
                b"Access-Control-Allow-Origin: *\r\n"
                + f"Content-Length: {len(body)}\r\n".encode("ascii")
                + b"Connection: close\r\n\r\n"
                + body
            )
            writer.write(resp)
            await writer.drain()
        except Exception:
            pass
        finally:
            try:
                writer.close()
                await writer.wait_closed()
            except Exception:
                pass


def load_rpc_auth(auth_arg: Optional[str], cookie_path: Optional[str]) -> str:
    if auth_arg:
        return auth_arg.strip()
    if cookie_path:
        with open(cookie_path, "r", encoding="utf-8") as f:
            return f.read().strip()
    raise ValueError("provide either --ns-rpc-auth USER:PASS or --ns-cookie PATH")


async def run_server(args: argparse.Namespace) -> None:
    ns_auth = load_rpc_auth(args.ns_rpc_auth, args.ns_cookie)
    ns_rpc = JsonRpcClient(args.ns_rpc_url, ns_auth)

    btc_rpc = None
    if args.btc_rpc_url:
        btc_auth = load_rpc_auth(args.btc_rpc_auth, args.btc_cookie)
        btc_rpc = JsonRpcClient(args.btc_rpc_url, btc_auth)

    payout_script = bytes.fromhex(args.btc_payout_script_hex) if args.btc_payout_script_hex else b"\x51"
    server = AuxPowStratumServer(
        ns_rpc=ns_rpc,
        payout_address=args.payout_address or None,
        btc_rpc=btc_rpc,
        btc_payout_script=payout_script,
        coinbase_tag=args.tag.encode("utf-8"),
        poll_interval=args.poll_interval,
        worker_diff=args.worker_diff,
        public_host=args.public_host,
        stratum_port=args.stratum_port,
    )

    # Fetch initial template (non-fatal if node is still syncing)
    try:
        server.refresh_template_sync()
    except Exception as exc:
        server.last_error = str(exc)
        print(f"[auxpow-stratum] waiting for newsatoshi node RPC: {exc}", flush=True)

    stratum_srv = await asyncio.start_server(
        server.handle_stratum_client, args.bind, args.stratum_port, limit=MAX_LINE_BYTES
    )
    status_srv = await asyncio.start_server(
        server.handle_http_status, args.bind, args.status_port, limit=4096
    )

    print(
        f"[auxpow-stratum] listening stratum+tcp://{args.bind}:{args.stratum_port} "
        f"(status http://{args.bind}:{args.status_port}/status, chainid={AUXPOW_CHAIN_ID})",
        flush=True,
    )

    poll_task = asyncio.create_task(server.poll_loop())
    try:
        async with stratum_srv, status_srv:
            await asyncio.gather(stratum_srv.serve_forever(), status_srv.serve_forever(), poll_task)
    finally:
        poll_task.cancel()


def parse_args(argv: Optional[List[str]] = None) -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Newsatoshi AuxPoW Stratum v1 Merge-Mining Server (chainid = 20051)"
    )
    p.add_argument("--bind", default=os.environ.get("STRATUM_BIND", "0.0.0.0"))
    p.add_argument("--stratum-port", type=int, default=int(os.environ.get("STRATUM_PORT", "3333")))
    p.add_argument("--status-port", type=int, default=int(os.environ.get("STATUS_PORT", "3334")))
    p.add_argument("--ns-rpc-url", default=os.environ.get("NS_RPC_URL", "http://127.0.0.1:19556/"))
    p.add_argument("--ns-rpc-auth", default=os.environ.get("NS_RPC_AUTH"))
    p.add_argument("--ns-cookie", default=os.environ.get("NS_COOKIE"))
    p.add_argument("--payout-address", default=os.environ.get("NS_PAYOUT_ADDRESS"))
    p.add_argument("--btc-rpc-url", default=os.environ.get("BTC_RPC_URL"))
    p.add_argument("--btc-rpc-auth", default=os.environ.get("BTC_RPC_AUTH"))
    p.add_argument("--btc-cookie", default=os.environ.get("BTC_COOKIE"))
    p.add_argument("--btc-payout-script-hex", default=os.environ.get("BTC_PAYOUT_SCRIPT_HEX", "51"))
    p.add_argument("--tag", default=os.environ.get("COINBASE_TAG", "/newsatoshi-auxpow/"))
    p.add_argument("--poll-interval", type=float, default=float(os.environ.get("POLL_INTERVAL", "2.0")))
    p.add_argument(
        "--worker-diff",
        type=float,
        default=float(os.environ["WORKER_DIFF"]) if os.environ.get("WORKER_DIFF") else None,
    )
    p.add_argument(
        "--public-host",
        default=os.environ.get("PUBLIC_STRATUM_HOST", "pool.newsatoshi.org"),
    )
    return p.parse_args(argv)


if __name__ == "__main__":
    try:
        asyncio.run(run_server(parse_args()))
    except KeyboardInterrupt:
        sys.exit(0)
