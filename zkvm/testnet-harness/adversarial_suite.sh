#!/usr/bin/env bash
# ==============================================================================
# NEWSATOSHI/2.1 — 90-Day Adversarial Testnet & zkVM Verification Harness
# Path: zkvm/testnet-harness/adversarial_suite.sh
# Spec: docs/RFC-0021-OP_VERIFY_COVENANT_STARK.md (Phase 1 Audit Gate)
#
# Verifies:
#   1. Live Newsatoshi L1 testnet seed health & block progression (newsatoshi-seed.fly.dev)
#   2. zkvm/covenant-circuit-core pure-Rust no_std circuit + adversarial tamper tests
#   3. newsatoshi-commons CLI simulator, telemetry-sign (--verify + adversarial low-soil),
#      and L1 OP_VERIFY_COVENANT_STARK (0xb4) anchor payload generation
#   4. Optional live HTTP Commons API adversarial attack suite (auto-detected on :8787)
#   5. Emits a SHA-256 fingerprinted JSON audit log + JSONL soak record
# ==============================================================================

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

export PATH="${HOME}/.cargo/bin:${PATH}"

SEED_URL="${SEED_URL:-https://newsatoshi-seed.fly.dev/status}"
LOCAL_API_URL="${LOCAL_API_URL:-http://127.0.0.1:8787}"
AUDIT_LOG_DIR="${AUDIT_LOG_DIR:-${SCRIPT_DIR}/audit-logs}"
WAIT_BLOCK_SECS="${WAIT_BLOCK_SECS:-0}"
SKIP_CLI="${SKIP_CLI:-0}"

EXPECTED_TESTNET_GENESIS="00000fd814d823a0bc64a84135868402998cff81e28a85b2bede5f1125d0a394"
EXPECTED_AUXPOW_CHAINID="20051"

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  --seed-url <url>         Live Newsatoshi status URL (default: ${SEED_URL})
  --api-url <url>          Local Commons HTTP API URL (default: ${LOCAL_API_URL})
  --log-dir <dir>          Output directory for soak audit logs (default: ${AUDIT_LOG_DIR})
  --wait-block-secs <sec>  Wait N seconds and re-poll seed for live height progression (default: 0)
  --skip-cli               Skip root newsatoshi-commons CLI checks (run seed + zkvm circuit only)
  -h, --help               Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --seed-url)
            SEED_URL="$2"
            shift 2
            ;;
        --api-url)
            LOCAL_API_URL="$2"
            shift 2
            ;;
        --log-dir)
            AUDIT_LOG_DIR="$2"
            shift 2
            ;;
        --wait-block-secs)
            WAIT_BLOCK_SECS="$2"
            shift 2
            ;;
        --skip-cli)
            SKIP_CLI=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

mkdir -p "${AUDIT_LOG_DIR}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

RESULTS_TSV="${TMP_DIR}/results.tsv"
SEED_JSON_1="${TMP_DIR}/seed_1.json"
SEED_JSON_2="${TMP_DIR}/seed_2.json"
: > "${RESULTS_TSV}"

PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0

now_ms() {
    python3 -c 'import time; print(int(time.time() * 1000))'
}

record_check() {
    local check_id="$1"
    local status="$2"
    local duration_ms="$3"
    local detail="$4"

    # Sanitize tabs/newlines for TSV
    local clean_detail
    clean_detail="$(printf '%s' "${detail}" | tr '\t\r\n' '   ')"
    printf '%s\t%s\t%s\t%s\n' "${check_id}" "${status}" "${duration_ms}" "${clean_detail}" >> "${RESULTS_TSV}"

    case "${status}" in
        PASS)
            PASS_COUNT=$((PASS_COUNT + 1))
            printf '  [PASS] %-40s (%5sms)  %s\n' "${check_id}" "${duration_ms}" "${clean_detail}"
            ;;
        FAIL)
            FAIL_COUNT=$((FAIL_COUNT + 1))
            printf '  [FAIL] %-40s (%5sms)  %s\n' "${check_id}" "${duration_ms}" "${clean_detail}"
            ;;
        SKIP)
            SKIP_COUNT=$((SKIP_COUNT + 1))
            printf '  [SKIP] %-40s (%5sms)  %s\n' "${check_id}" "${duration_ms}" "${clean_detail}"
            ;;
    esac
}

echo "================================================================================"
echo "  NEWSATOSHI/2.1 — 90-DAY ADVERSARIAL TESTNET & ZKVM VERIFICATION HARNESS"
echo "================================================================================"
echo "  Timestamp (UTC) : $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
echo "  Repository      : ${REPO_ROOT}"
echo "  L1 Seed URL     : ${SEED_URL}"
echo "  Local API URL   : ${LOCAL_API_URL}"
echo "  Audit Log Dir   : ${AUDIT_LOG_DIR}"
echo "--------------------------------------------------------------------------------"

# ------------------------------------------------------------------------------
# GATE 1: Live Newsatoshi L1 Testnet Seed Health & Block Progression
# ------------------------------------------------------------------------------
t0="$(now_ms)"
if curl -fsSL --max-time 10 "${SEED_URL}" -o "${SEED_JSON_1}" 2>"${TMP_DIR}/curl_seed.err"; then
    seed_check_out="$(python3 - "${SEED_JSON_1}" "${EXPECTED_TESTNET_GENESIS}" "${EXPECTED_AUXPOW_CHAINID}" <<'PY'
import json, sys

path, exp_genesis, exp_chainid = sys.argv[1], sys.argv[2], int(sys.argv[3])
with open(path, "r", encoding="utf-8") as f:
    data = json.load(f)

errors = []
if data.get("network") != "testnet":
    errors.append(f"network={data.get('network')} != testnet")
if data.get("genesis") != exp_genesis:
    errors.append(f"genesis={data.get('genesis')} != {exp_genesis}")
if int(data.get("auxpow_chainid", -1)) != exp_chainid:
    errors.append(f"auxpow_chainid={data.get('auxpow_chainid')} != {exp_chainid}")
height = int(data.get("height", 0))
if height <= 0:
    errors.append(f"height={height} <= 0")
tip = str(data.get("tip", ""))
if len(tip) != 64:
    errors.append(f"invalid tip hash len={len(tip)}")
peers = int(data.get("peers", 0))
if peers < 1:
    errors.append(f"peers={peers} < 1")

if errors:
    print("ERR:" + "; ".join(errors))
    sys.exit(1)

print(f"height={height} tip={tip[:16]}.. peers={peers} notes={data.get('notes')} chainid={exp_chainid}")
PY
    )"
    rc=$?
    dt=$(( $(now_ms) - t0 ))
    if [[ ${rc} -eq 0 ]]; then
        record_check "l1_seed_health" "PASS" "${dt}" "${seed_check_out}"
    else
        record_check "l1_seed_health" "FAIL" "${dt}" "${seed_check_out}"
    fi
else
    dt=$(( $(now_ms) - t0 ))
    err_msg="$(cat "${TMP_DIR}/curl_seed.err" 2>/dev/null || echo 'curl failed')"
    record_check "l1_seed_health" "FAIL" "${dt}" "unable to reach ${SEED_URL}: ${err_msg}"
fi

t0="$(now_ms)"
if [[ -s "${SEED_JSON_1}" ]]; then
    if [[ "${WAIT_BLOCK_SECS}" -gt 0 ]]; then
        sleep "${WAIT_BLOCK_SECS}"
        curl -fsSL --max-time 10 "${SEED_URL}" -o "${SEED_JSON_2}" 2>/dev/null || true
    fi
    prog_out="$(python3 - "${SEED_JSON_1}" "${SEED_JSON_2}" "${WAIT_BLOCK_SECS}" <<'PY'
import json, os, sys

p1, p2, wait_secs = sys.argv[1], sys.argv[2], int(sys.argv[3])
with open(p1, "r", encoding="utf-8") as f:
    d1 = json.load(f)

recent = d1.get("recent", [])
if not isinstance(recent, list) or len(recent) < 2:
    print("recent[] block history missing or < 2 entries")
    sys.exit(1)

heights = [int(b["height"]) for b in recent]
for i in range(len(heights) - 1):
    if heights[i] != heights[i + 1] + 1:
        print(f"non-contiguous recent heights: {heights[:5]}")
        sys.exit(1)

now_ts = int(d1.get("now", 0))
tip_ts = int(d1.get("tip_time", 0))
tip_age = max(0, now_ts - tip_ts)
span_blocks = heights[0] - heights[-1]
span_secs = max(1, int(recent[0]["time"]) - int(recent[-1]["time"]))
avg_block_sec = span_secs / max(1, span_blocks)

detail = f"recent_contiguous={len(heights)} tip_age={tip_age}s avg_interval={avg_block_sec:.1f}s"
if wait_secs > 0 and os.path.exists(p2) and os.path.getsize(p2) > 0:
    with open(p2, "r", encoding="utf-8") as f:
        d2 = json.load(f)
    h1, h2 = int(d1.get("height", 0)), int(d2.get("height", 0))
    if h2 < h1:
        print(f"height regressed: {h1} -> {h2}")
        sys.exit(1)
    detail += f" live_delta=+{h2 - h1} over {wait_secs}s"
print(detail)
PY
    )"
    rc=$?
    dt=$(( $(now_ms) - t0 ))
    if [[ ${rc} -eq 0 ]]; then
        record_check "l1_block_progression" "PASS" "${dt}" "${prog_out}"
    else
        record_check "l1_block_progression" "FAIL" "${dt}" "${prog_out}"
    fi
else
    record_check "l1_block_progression" "SKIP" "0" "skipped because l1_seed_health had no payload"
fi

# ------------------------------------------------------------------------------
# GATE 2: zkvm/covenant-circuit-core (Pure-Rust no_std Kernel + Adversarial Tests)
# ------------------------------------------------------------------------------
CORE_MANIFEST="${REPO_ROOT}/zkvm/covenant-circuit-core/Cargo.toml"
t0="$(now_ms)"
if cargo test --manifest-path "${CORE_MANIFEST}" >"${TMP_DIR}/circuit_test.log" 2>&1; then
    dt=$(( $(now_ms) - t0 ))
    summary_line="$(grep -E '^test result:' "${TMP_DIR}/circuit_test.log" | head -n 1 || echo 'passed')"
    record_check "zkvm_circuit_core_cargo_test" "PASS" "${dt}" "${summary_line}"
else
    dt=$(( $(now_ms) - t0 ))
    tail_err="$(tail -n 3 "${TMP_DIR}/circuit_test.log" | tr '\n' ' ')"
    record_check "zkvm_circuit_core_cargo_test" "FAIL" "${dt}" "${tail_err}"
fi

t0="$(now_ms)"
if cargo check --manifest-path "${CORE_MANIFEST}" --no-default-features >"${TMP_DIR}/circuit_nostd.log" 2>&1; then
    dt=$(( $(now_ms) - t0 ))
    record_check "zkvm_circuit_core_no_std_check" "PASS" "${dt}" "no_std zkVM guest target compiles cleanly"
else
    dt=$(( $(now_ms) - t0 ))
    tail_err="$(tail -n 3 "${TMP_DIR}/circuit_nostd.log" | tr '\n' ' ')"
    record_check "zkvm_circuit_core_no_std_check" "FAIL" "${dt}" "${tail_err}"
fi

# ------------------------------------------------------------------------------
# GATE 3: newsatoshi-commons CLI Simulator, Telemetry Signer & 0xb4 Anchor
# ------------------------------------------------------------------------------
if [[ "${SKIP_CLI}" -eq 1 ]]; then
    record_check "cli_simulate_abundance_loop" "SKIP" "0" "skipped via --skip-cli"
    record_check "cli_telemetry_sign_valid" "SKIP" "0" "skipped via --skip-cli"
    record_check "cli_telemetry_adversarial_low_soil" "SKIP" "0" "skipped via --skip-cli"
    record_check "cli_anchor_verify_stark" "SKIP" "0" "skipped via --skip-cli"
else
    ROOT_MANIFEST="${REPO_ROOT}/Cargo.toml"

    # 3a. Full multi-parcel simulate --json
    t0="$(now_ms)"
    if cargo run --quiet --manifest-path "${ROOT_MANIFEST}" --bin newsatoshi-commons -- simulate --height 6 --json >"${TMP_DIR}/sim.json" 2>"${TMP_DIR}/sim.err"; then
        sim_out="$(python3 - "${TMP_DIR}/sim.json" <<'PY'
import json, sys
with open(sys.argv[1], "r", encoding="utf-8") as f:
    d = json.load(f)
parcels = d.get("parcels_acquired", [])
blocked = d.get("adversarial_checks_blocked", [])
if len(parcels) < 2:
    print(f"expected >= 2 parcels, got {parcels}")
    sys.exit(1)
if len(blocked) < 3:
    print(f"expected >= 3 blocked adversarial checks, got {len(blocked)}")
    sys.exit(1)
print(f"parcels={parcels} kwh={d.get('total_kwh_verified')} blocked={len(blocked)} balance={d.get('final_covenant_balance_love')} LOVE")
PY
        )"
        rc=$?
        dt=$(( $(now_ms) - t0 ))
        if [[ ${rc} -eq 0 ]]; then
            record_check "cli_simulate_abundance_loop" "PASS" "${dt}" "${sim_out}"
        else
            record_check "cli_simulate_abundance_loop" "FAIL" "${dt}" "${sim_out}"
        fi
    else
        dt=$(( $(now_ms) - t0 ))
        record_check "cli_simulate_abundance_loop" "FAIL" "${dt}" "$(tail -n 2 "${TMP_DIR}/sim.err")"
    fi

    # 3b. Valid hardware telemetry-sign --verify --json
    t0="$(now_ms)"
    if cargo run --quiet --manifest-path "${ROOT_MANIFEST}" --bin newsatoshi-commons -- telemetry-sign --kwh 12500 --soil 92 --count 3 --verify --json >"${TMP_DIR}/tel.json" 2>"${TMP_DIR}/tel.err"; then
        tel_out="$(python3 - "${TMP_DIR}/tel.json" <<'PY'
import json, sys
with open(sys.argv[1], "r", encoding="utf-8") as f:
    d = json.load(f)
if not d.get("ledger_verified", False):
    print("ledger_verified != true")
    sys.exit(1)
pkts = d.get("packets", [])
if len(pkts) != 3:
    print(f"expected 3 signed packets, got {len(pkts)}")
    sys.exit(1)
print(f"verified_packets={len(pkts)} state_root={str(d.get('updated_state_root', ''))[:16]}..")
PY
        )"
        rc=$?
        dt=$(( $(now_ms) - t0 ))
        if [[ ${rc} -eq 0 ]]; then
            record_check "cli_telemetry_sign_valid" "PASS" "${dt}" "${tel_out}"
        else
            record_check "cli_telemetry_sign_valid" "FAIL" "${dt}" "${tel_out}"
        fi
    else
        dt=$(( $(now_ms) - t0 ))
        record_check "cli_telemetry_sign_valid" "FAIL" "${dt}" "$(tail -n 2 "${TMP_DIR}/tel.err")"
    fi

    # 3c. Adversarial hardware telemetry (soil_health_score 45 < floor 80 MUST fail!)
    t0="$(now_ms)"
    if cargo run --quiet --manifest-path "${ROOT_MANIFEST}" --bin newsatoshi-commons -- telemetry-sign --kwh 12500 --soil 45 --count 1 --verify >"${TMP_DIR}/adv_soil.out" 2>"${TMP_DIR}/adv_soil.err"; then
        dt=$(( $(now_ms) - t0 ))
        record_check "cli_telemetry_adversarial_low_soil" "FAIL" "${dt}" "CRITICAL: degraded soil=45 was unexpectedly accepted!"
    else
        dt=$(( $(now_ms) - t0 ))
        err_line="$(tail -n 1 "${TMP_DIR}/adv_soil.err" | tr '\n' ' ')"
        record_check "cli_telemetry_adversarial_low_soil" "PASS" "${dt}" "correctly rejected degraded soil=45 (${err_line})"
    fi

    # 3d. Deterministic L1 anchor + OP_VERIFY_COVENANT_STARK (0xb4) envelope
    t0="$(now_ms)"
    if cargo run --quiet --manifest-path "${ROOT_MANIFEST}" --bin newsatoshi-commons -- anchor --network testnet --balance 60000 --parcels 2 --kwh 42500 --verify-stark >"${TMP_DIR}/anchor.out" 2>"${TMP_DIR}/anchor.err"; then
        dt=$(( $(now_ms) - t0 ))
        if grep -q "newsatoshi_anchorCovenantRoot" "${TMP_DIR}/anchor.out" \
            && grep -q "OP_VERIFY_COVENANT_STARK" "${TMP_DIR}/anchor.out" \
            && grep -q "6a20" "${TMP_DIR}/anchor.out"; then
            record_check "cli_anchor_verify_stark" "PASS" "${dt}" "verified OP_RETURN (6a20) + OP_VERIFY_COVENANT_STARK (0xb4) JSON-RPC envelope"
        else
            record_check "cli_anchor_verify_stark" "FAIL" "${dt}" "missing expected OP_RETURN or OP_VERIFY_COVENANT_STARK fields"
        fi
    else
        dt=$(( $(now_ms) - t0 ))
        record_check "cli_anchor_verify_stark" "FAIL" "${dt}" "$(tail -n 2 "${TMP_DIR}/anchor.err")"
    fi
fi

# ------------------------------------------------------------------------------
# GATE 4: Optional Live HTTP API Adversarial Attack Suite (if server is running)
# ------------------------------------------------------------------------------
t0="$(now_ms)"
if curl -fsSL --max-time 3 "${LOCAL_API_URL}/api/health" -o "${TMP_DIR}/api_health.json" 2>/dev/null; then
    health_check="$(python3 - "${TMP_DIR}/api_health.json" <<'PY'
import json, sys
try:
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        d = json.load(f)
except Exception:
    print("NOT_JSON")
    sys.exit(2)
proto = str(d.get("protocol_version", ""))
if d.get("ok") is True and proto.startswith("NEWSATOSHI"):
    print(f"ok version={d.get('version')} protocol={proto}")
    sys.exit(0)
print(f"other_service={d.get('service', 'unknown')} protocol={proto or 'none'}")
sys.exit(2)
PY
    )"
    hrc=$?
    dt=$(( $(now_ms) - t0 ))
    if [[ ${hrc} -eq 0 ]]; then
        record_check "http_api_health" "PASS" "${dt}" "live NEWSATOSHI/2.1 server at ${LOCAL_API_URL} (${health_check})"

        for attack_spec in \
            "strip_covenant:{\"parcel_id\":404,\"kwh_delivered\":10000,\"soil_health_score\":92,\"payout_love\":10000,\"simulate_attack\":\"strip_covenant\"}" \
            "replay_leaf:{\"parcel_id\":404,\"kwh_delivered\":10000,\"soil_health_score\":92,\"payout_love\":10000,\"simulate_attack\":\"replay_leaf\"}" \
            "low_soil:{\"parcel_id\":404,\"kwh_delivered\":10000,\"soil_health_score\":42,\"payout_love\":10000,\"simulate_attack\":\"low_soil\"}" \
            "over_cap:{\"parcel_id\":404,\"kwh_delivered\":10000,\"soil_health_score\":92,\"payout_love\":999999,\"simulate_attack\":\"over_cap\"}"
        do
            attack_name="${attack_spec%%:*}"
            attack_body="${attack_spec#*:}"
            t0="$(now_ms)"
            http_code="$(curl -sS --max-time 5 -o "${TMP_DIR}/atk_${attack_name}.json" -w "%{http_code}" \
                -X POST -H "Content-Type: application/json" \
                -d "${attack_body}" \
                "${LOCAL_API_URL}/api/actions/telemetry" 2>/dev/null || echo "000")"
            atk_out="$(python3 - "${TMP_DIR}/atk_${attack_name}.json" "${http_code}" <<'PY'
import json, sys
path, code = sys.argv[1], int(sys.argv[2])
if code == 404:
    print("FAIL: endpoint returned HTTP 404 Not Found")
    sys.exit(1)
try:
    with open(path, "r", encoding="utf-8") as f:
        d = json.load(f)
except Exception as e:
    if code in (400, 403, 422):
        print(f"blocked with HTTP {code}")
        sys.exit(0)
    print(f"invalid response (HTTP {code}): {e}")
    sys.exit(1)

if (d.get("ok") is False and d.get("blocked") is True) or code in (400, 403, 422):
    print(f"blocked (HTTP {code}): {d.get('error', 'rejected')}")
    sys.exit(0)
print(f"CRITICAL: attack NOT blocked (HTTP {code}, ok={d.get('ok')}, blocked={d.get('blocked')})")
sys.exit(1)
PY
            )"
            rc=$?
            dt=$(( $(now_ms) - t0 ))
            if [[ ${rc} -eq 0 ]]; then
                record_check "http_adversarial_${attack_name}" "PASS" "${dt}" "${atk_out}"
            else
                record_check "http_adversarial_${attack_name}" "FAIL" "${dt}" "${atk_out}"
            fi
        done
    else
        record_check "http_api_adversarial_suite" "SKIP" "${dt}" "endpoint at ${LOCAL_API_URL} is not a NEWSATOSHI/2.1 server (${health_check})"
    fi
else
    dt=$(( $(now_ms) - t0 ))
    record_check "http_api_adversarial_suite" "SKIP" "${dt}" "local HTTP server not running at ${LOCAL_API_URL} (start with 'newsatoshi-commons serve')"
fi

# ------------------------------------------------------------------------------
# Emit Structured JSON Audit Log & Append to 90-Day Soak JSONL History
# ------------------------------------------------------------------------------
STAMP_UTC="$(date -u '+%Y%m%dT%H%M%SZ')"
AUDIT_JSON_FILE="${AUDIT_LOG_DIR}/soak-audit-${STAMP_UTC}.json"
SOAK_JSONL_FILE="${AUDIT_LOG_DIR}/soak-history.jsonl"

AUDIT_DIGEST="$(python3 - "${RESULTS_TSV}" "${SEED_JSON_1}" "${AUDIT_JSON_FILE}" "${SOAK_JSONL_FILE}" "${SEED_URL}" "${LOCAL_API_URL}" <<'PY'
import datetime, hashlib, json, os, sys

tsv_path, seed_path, out_json, out_jsonl, seed_url, api_url = sys.argv[1:7]

checks = []
passed = failed = skipped = 0
with open(tsv_path, "r", encoding="utf-8") as f:
    for line in f:
        line = line.rstrip("\n")
        if not line:
            continue
        cid, status, dur, detail = line.split("\t", 3)
        checks.append({
            "id": cid,
            "status": status,
            "duration_ms": int(dur),
            "detail": detail,
        })
        if status == "PASS":
            passed += 1
        elif status == "FAIL":
            failed += 1
        else:
            skipped += 1

l1_snapshot = None
if os.path.exists(seed_path) and os.path.getsize(seed_path) > 0:
    try:
        with open(seed_path, "r", encoding="utf-8") as f:
            s = json.load(f)
        l1_snapshot = {
            "seed_url": seed_url,
            "network": s.get("network"),
            "version": s.get("version"),
            "genesis": s.get("genesis"),
            "auxpow_chainid": s.get("auxpow_chainid"),
            "height": s.get("height"),
            "tip": s.get("tip"),
            "peers": s.get("peers"),
            "issued": s.get("issued"),
            "notes": s.get("notes"),
            "nullifiers": s.get("nullifiers"),
        }
    except Exception:
        l1_snapshot = {"seed_url": seed_url, "error": "unparseable"}

overall = "PASS" if failed == 0 else "FAIL"
payload = {
    "schema_version": "NEWSATOSHI/2.1-SOAK-AUDIT-V1",
    "timestamp_utc": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "soak_gate_days_required": 90,
    "api_url": api_url,
    "l1_snapshot": l1_snapshot,
    "summary": {
        "total": len(checks),
        "passed": passed,
        "failed": failed,
        "skipped": skipped,
        "overall_status": overall,
    },
    "checks": checks,
}

canonical = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
digest = hashlib.sha256(canonical).hexdigest()
payload["audit_record_sha256"] = digest

with open(out_json, "w", encoding="utf-8") as f:
    json.dump(payload, f, indent=2)
    f.write("\n")

with open(out_jsonl, "a", encoding="utf-8") as f:
    f.write(json.dumps(payload, sort_keys=True, separators=(",", ":")) + "\n")

print(digest)
PY
)"

echo "--------------------------------------------------------------------------------"
echo "  Summary         : PASS=${PASS_COUNT} | FAIL=${FAIL_COUNT} | SKIP=${SKIP_COUNT}"
echo "  Overall Status  : $([[ ${FAIL_COUNT} -eq 0 ]] && echo 'PASS' || echo 'FAIL')"
echo "  Audit Record    : ${AUDIT_JSON_FILE}"
echo "  Soak History    : ${SOAK_JSONL_FILE}"
echo "  Record SHA-256  : ${AUDIT_DIGEST}"
echo "================================================================================"

if [[ ${FAIL_COUNT} -ne 0 ]]; then
    exit 1
fi
exit 0
