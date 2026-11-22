#!/usr/bin/env python3
"""
Newsatoshi 2.1 Legal-Cryptographic Airlock Renderer (Zero-Dependency Python 3)

Populates all {{...}} cryptographic schedule placeholders in:
  - perpetual-purpose-trust-charter.md
  - wyoming-duna-clt-operating-agreement.md

Writes bound instruments into legal/rendered/ and emits legal/rendered/airlock-manifest.json
with SHA-256 digests for L1 OP_RETURN / hash-lock anchoring.
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import re
import struct
import sys
from pathlib import Path
from typing import Any, Dict, List, Tuple

MAINNET_GENESIS = "000000006ba30cc3c86ecaabcb962287e9d5fd0877c27f4b33661b4d1d3033f7"
TESTNET_GENESIS = "00000fd814d823a0bc64a84135868402998cff81e28a85b2bede5f1125d0a394"
AUXPOW_CHAINID = 20051

TEMPLATE_FILES = [
    "perpetual-purpose-trust-charter.md",
    "wyoming-duna-clt-operating-agreement.md",
]

PLACEHOLDER_RE = re.compile(r"\{\{([A-Z0-9_]+)\}\}")
ANY_MUSTACHE_RE = re.compile(r"\{\{|\}\}")


def sha256_bytes(data: bytes) -> bytes:
    return hashlib.sha256(data).digest()


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def build_default_schedule() -> Dict[str, Any]:
    """
    Deterministic built-in schedule matching newsatoshi-commons defaults:
      max_bounty_payout     = 20000 LOVE
      reinvest_bps          = 1500  (15.00%)
      min_soil_health_score = 80
    Computes covenant_rule_hash using the exact NEWSATOSHI_COVENANT_V2_1 byte layout.
    """
    max_bounty_payout = 20000
    reinvest_bps = 1500
    min_soil_health_score = 80

    # Deterministic 32-byte post-quantum placeholder roots/keys (replace via --schedule legal/schedule-generated.json)
    deed_oracle_merkle_root = sha256_bytes(b"NEWSATOSHI_V2_1_DEFAULT_DEED_ORACLE_XMSS_ROOT")
    hardware_merkle_root = sha256_bytes(b"NEWSATOSHI_V2_1_DEFAULT_HARDWARE_TEE_XMSS_ROOT")
    deed_oracle_slh_dsa_pk = sha256_bytes(b"NEWSATOSHI_V2_1_DEFAULT_DEED_ORACLE_SLH_DSA_SHA2_128F_PK")
    hardware_slh_dsa_pk = sha256_bytes(b"NEWSATOSHI_V2_1_DEFAULT_HARDWARE_TEE_SLH_DSA_SHA2_128F_PK")

    # Exact canonical NEWSATOSHI_COVENANT_V2_1 hash layout from src/lib.rs
    rule_preimage = (
        b"NEWSATOSHI_COVENANT_V2_1"
        + struct.pack(">QHH", max_bounty_payout, reinvest_bps, min_soil_health_score)
        + deed_oracle_merkle_root
        + hardware_merkle_root
    )
    covenant_rule_hash = sha256_hex(rule_preimage)

    return {
        "protocol_version": "NEWSATOSHI/2.1",
        "covenant_rule_hash": covenant_rule_hash,
        "covenant_rule": {
            "max_bounty_payout": max_bounty_payout,
            "reinvest_bps": reinvest_bps,
            "min_soil_health_score": min_soil_health_score,
        },
        "keys": {
            "deed_oracle_merkle_root": deed_oracle_merkle_root.hex(),
            "hardware_merkle_root": hardware_merkle_root.hex(),
            "deed_oracle_slh_dsa_pk_hex": deed_oracle_slh_dsa_pk.hex(),
            "hardware_slh_dsa_pk_hex": hardware_slh_dsa_pk.hex(),
        },
        "l1_anchors": {
            "mainnet_genesis": MAINNET_GENESIS,
            "testnet_genesis": TESTNET_GENESIS,
            "auxpow_chainid": AUXPOW_CHAINID,
        },
    }


def normalize_schedule(raw: Dict[str, Any]) -> Dict[str, Any]:
    """
    Accepts the canonical `newsatoshi-commons export-legal-schedule` JSON schema
    while tolerating minor flat/alternate key names gracefully.
    """
    defaults = build_default_schedule()
    rule = raw.get("covenant_rule") if isinstance(raw.get("covenant_rule"), dict) else {}
    keys = raw.get("keys") if isinstance(raw.get("keys"), dict) else {}
    l1 = raw.get("l1_anchors") if isinstance(raw.get("l1_anchors"), dict) else {}

    def pick(*candidates: Any) -> Any:
        for c in candidates:
            if c is not None and c != "":
                return c
        return None

    protocol_version = str(pick(raw.get("protocol_version"), defaults["protocol_version"]))
    max_bounty_payout = int(
        pick(
            rule.get("max_bounty_payout"),
            raw.get("max_bounty_payout"),
            defaults["covenant_rule"]["max_bounty_payout"],
        )
    )
    reinvest_bps = int(
        pick(
            rule.get("reinvest_bps"),
            raw.get("reinvest_bps"),
            defaults["covenant_rule"]["reinvest_bps"],
        )
    )
    if not (0 <= reinvest_bps <= 10000):
        raise ValueError(f"reinvest_bps out of range 0..10000: {reinvest_bps}")

    min_soil_health_score = int(
        pick(
            rule.get("min_soil_health_score"),
            raw.get("min_soil_health_score"),
            defaults["covenant_rule"]["min_soil_health_score"],
        )
    )
    if not (0 <= min_soil_health_score <= 100):
        raise ValueError(f"min_soil_health_score out of range 0..100: {min_soil_health_score}")

    deed_oracle_merkle_root = str(
        pick(
            keys.get("deed_oracle_merkle_root"),
            raw.get("deed_oracle_merkle_root"),
            rule.get("authorized_deed_oracle_pk"),
            defaults["keys"]["deed_oracle_merkle_root"],
        )
    )
    hardware_merkle_root = str(
        pick(
            keys.get("hardware_merkle_root"),
            raw.get("hardware_merkle_root"),
            rule.get("authorized_hardware_pk"),
            defaults["keys"]["hardware_merkle_root"],
        )
    )
    deed_oracle_slh_dsa_pk_hex = str(
        pick(
            keys.get("deed_oracle_slh_dsa_pk_hex"),
            keys.get("deed_oracle_slh_dsa_pk"),
            raw.get("deed_oracle_slh_dsa_pk_hex"),
            raw.get("deed_oracle_slh_dsa_pk"),
            defaults["keys"]["deed_oracle_slh_dsa_pk_hex"],
        )
    )
    hardware_slh_dsa_pk_hex = str(
        pick(
            keys.get("hardware_slh_dsa_pk_hex"),
            keys.get("hardware_slh_dsa_pk"),
            raw.get("hardware_slh_dsa_pk_hex"),
            raw.get("hardware_slh_dsa_pk"),
            defaults["keys"]["hardware_slh_dsa_pk_hex"],
        )
    )

    mainnet_genesis = str(
        pick(l1.get("mainnet_genesis"), raw.get("mainnet_genesis"), MAINNET_GENESIS)
    )
    testnet_genesis = str(
        pick(l1.get("testnet_genesis"), raw.get("testnet_genesis"), TESTNET_GENESIS)
    )
    auxpow_chainid = int(
        pick(l1.get("auxpow_chainid"), raw.get("auxpow_chainid"), AUXPOW_CHAINID)
    )

    covenant_rule_hash = str(pick(raw.get("covenant_rule_hash"), defaults["covenant_rule_hash"]))

    return {
        "protocol_version": protocol_version,
        "covenant_rule_hash": covenant_rule_hash,
        "covenant_rule": {
            "max_bounty_payout": max_bounty_payout,
            "reinvest_bps": reinvest_bps,
            "min_soil_health_score": min_soil_health_score,
        },
        "keys": {
            "deed_oracle_merkle_root": deed_oracle_merkle_root,
            "hardware_merkle_root": hardware_merkle_root,
            "deed_oracle_slh_dsa_pk_hex": deed_oracle_slh_dsa_pk_hex,
            "hardware_slh_dsa_pk_hex": hardware_slh_dsa_pk_hex,
        },
        "l1_anchors": {
            "mainnet_genesis": mainnet_genesis,
            "testnet_genesis": testnet_genesis,
            "auxpow_chainid": auxpow_chainid,
        },
    }


def build_placeholder_map(schedule: Dict[str, Any]) -> Dict[str, str]:
    rule = schedule["covenant_rule"]
    keys = schedule["keys"]
    l1 = schedule["l1_anchors"]

    reinvest_bps = int(rule["reinvest_bps"])
    operator_bps = 10000 - reinvest_bps

    return {
        "PROTOCOL_VERSION": str(schedule["protocol_version"]),
        "COVENANT_RULE_HASH": str(schedule["covenant_rule_hash"]),
        "MAX_BOUNTY_PAYOUT": f"{int(rule['max_bounty_payout']):,}",
        "REINVEST_BPS": str(reinvest_bps),
        "REINVEST_PCT": f"{reinvest_bps / 100:.2f}",
        "OPERATOR_SHARE_BPS": str(operator_bps),
        "OPERATOR_SHARE_PCT": f"{operator_bps / 100:.2f}",
        "MIN_SOIL_HEALTH_SCORE": str(int(rule["min_soil_health_score"])),
        "DEED_ORACLE_MERKLE_ROOT": str(keys["deed_oracle_merkle_root"]),
        "HARDWARE_MERKLE_ROOT": str(keys["hardware_merkle_root"]),
        "DEED_ORACLE_SLH_DSA_PK": str(keys["deed_oracle_slh_dsa_pk_hex"]),
        "HARDWARE_SLH_DSA_PK": str(keys["hardware_slh_dsa_pk_hex"]),
        "MAINNET_GENESIS": str(l1["mainnet_genesis"]),
        "TESTNET_GENESIS": str(l1["testnet_genesis"]),
        "AUXPOW_CHAINID": str(int(l1["auxpow_chainid"])),
    }


def render_template(content: str, mapping: Dict[str, str]) -> Tuple[str, int, List[str]]:
    replaced_count = 0
    unknown_keys: List[str] = []

    def _sub(match: re.Match[str]) -> str:
        nonlocal replaced_count
        key = match.group(1)
        if key in mapping:
            replaced_count += 1
            return mapping[key]
        unknown_keys.append(key)
        return match.group(0)

    rendered = PLACEHOLDER_RE.sub(_sub, content)
    leftover = ANY_MUSTACHE_RE.findall(rendered)
    if leftover and not unknown_keys:
        unknown_keys.append("<malformed-braces>")
    return rendered, replaced_count, unknown_keys


def main(argv: List[str] | None = None) -> int:
    script_dir = Path(__file__).resolve().parent

    parser = argparse.ArgumentParser(
        description="Render NEWSATOSHI/2.1 legal airlock templates and generate SHA-256 manifest."
    )
    parser.add_argument(
        "--schedule",
        type=Path,
        default=None,
        help="Path to legal schedule JSON (e.g., legal/schedule-generated.json). Uses deterministic defaults if omitted.",
    )
    parser.add_argument(
        "--templates-dir",
        type=Path,
        default=script_dir,
        help="Directory containing legal markdown templates (default: legal/).",
    )
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=script_dir / "rendered",
        help="Output directory for rendered instruments and airlock-manifest.json (default: legal/rendered/).",
    )
    args = parser.parse_args(argv)

    if args.schedule is not None:
        schedule_path = args.schedule.resolve()
        if not schedule_path.is_file():
            print(f"error: schedule file not found: {schedule_path}", file=sys.stderr)
            return 1
        with schedule_path.open("r", encoding="utf-8") as fh:
            raw_schedule = json.load(fh)
        schedule = normalize_schedule(raw_schedule)
        schedule_source = str(schedule_path)
    else:
        schedule = build_default_schedule()
        schedule_source = "built-in-deterministic-defaults"

    mapping = build_placeholder_map(schedule)
    out_dir: Path = args.out_dir.resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    documents_meta: List[Dict[str, Any]] = []
    bundle_hasher = hashlib.sha256()
    bundle_hasher.update(b"NEWSATOSHI_LEGAL_AIRLOCK_V2_1")
    bundle_hasher.update(schedule["covenant_rule_hash"].encode("ascii"))

    total_unresolved = 0
    total_replaced = 0

    for filename in TEMPLATE_FILES:
        src_path = (args.templates_dir / filename).resolve()
        if not src_path.is_file():
            print(f"error: template file not found: {src_path}", file=sys.stderr)
            return 1

        src_bytes = src_path.read_bytes()
        src_text = src_bytes.decode("utf-8")
        rendered_text, replaced_count, unknown_keys = render_template(src_text, mapping)
        unresolved = len(unknown_keys)
        total_unresolved += unresolved
        total_replaced += replaced_count

        if unresolved > 0:
            print(
                f"error: unresolved placeholders in {filename}: {sorted(set(unknown_keys))}",
                file=sys.stderr,
            )
            return 1

        dst_path = out_dir / filename
        rendered_bytes = rendered_text.encode("utf-8")
        dst_path.write_bytes(rendered_bytes)

        rendered_digest = sha256_hex(rendered_bytes)
        bundle_hasher.update(bytes.fromhex(rendered_digest))

        documents_meta.append(
            {
                "filename": filename,
                "template_path": str(src_path),
                "rendered_path": str(dst_path),
                "template_sha256": sha256_hex(src_bytes),
                "rendered_sha256": rendered_digest,
                "rendered_bytes": len(rendered_bytes),
                "rendered_lines": rendered_text.count("\n") + (0 if rendered_text.endswith("\n") else 1),
                "placeholders_replaced": replaced_count,
                "unresolved_placeholders": unresolved,
            }
        )

    airlock_bundle_sha256 = bundle_hasher.hexdigest()

    manifest = {
        "schema_version": "1.0.0",
        "protocol_version": schedule["protocol_version"],
        "rendered_at_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "schedule_source": schedule_source,
        "covenant_rule_hash": schedule["covenant_rule_hash"],
        "covenant_rule": schedule["covenant_rule"],
        "keys": schedule["keys"],
        "l1_anchors": schedule["l1_anchors"],
        "airlock_bundle_sha256": airlock_bundle_sha256,
        "l1_anchor_commands": {
            "testnet_lock": f"newsatoshi --net testnet lock 0.01 --hash {airlock_bundle_sha256} --to <TRUST_ADDRESS>",
            "mainnet_lock": f"newsatoshi lock 0.01 --hash {airlock_bundle_sha256} --to <TRUST_ADDRESS>",
            "testnet_memo": f"newsatoshi --net testnet send <TRUST_ADDRESS> 0.01 --memo AIRLOCK:{airlock_bundle_sha256}",
        },
        "documents": documents_meta,
        "verification": {
            "all_placeholders_resolved": total_unresolved == 0,
            "total_placeholders_replaced": total_replaced,
            "total_unresolved_placeholders": total_unresolved,
        },
    }

    manifest_path = out_dir / "airlock-manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    # Crisp verification summary
    print("=== Newsatoshi 2.1 Legal-Cryptographic Airlock Renderer ===")
    print(f"Protocol Version      : {schedule['protocol_version']}")
    print(f"Schedule Source       : {schedule_source}")
    print(f"Covenant Rule Hash    : {schedule['covenant_rule_hash']}")
    print(
        f"Covenant Parameters   : reinvest={mapping['REINVEST_BPS']} bps ({mapping['REINVEST_PCT']}%) | "
        f"max_bounty={mapping['MAX_BOUNTY_PAYOUT']} LOVE | min_soil>={mapping['MIN_SOIL_HEALTH_SCORE']}"
    )
    print(f"Mainnet Genesis (L1)  : {mapping['MAINNET_GENESIS']} (AuxPoW chainid={mapping['AUXPOW_CHAINID']})")
    print(f"Testnet Genesis (L1)  : {mapping['TESTNET_GENESIS']}")
    print("")
    print("Rendered Legal Instruments:")
    for doc in documents_meta:
        print(
            f"  [OK] {doc['filename']:<42} "
            f"replaced={doc['placeholders_replaced']:>2}  unresolved={doc['unresolved_placeholders']}  "
            f"sha256={doc['rendered_sha256'][:16]}... ({doc['rendered_bytes']} B)"
        )
    print("")
    print(f"Airlock Bundle SHA256 : {airlock_bundle_sha256}")
    print(f"Manifest Written      : {manifest_path}")
    print(f"Verification Status   : PASS (total_replaced={total_replaced}, total_unresolved={total_unresolved})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
