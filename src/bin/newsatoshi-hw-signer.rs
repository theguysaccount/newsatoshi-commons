//! Standalone Hardware Telemetry & Offline Deed Signer (`newsatoshi-hw-signer`)
//!
//! Designed for field-deployed Linux / Raspberry Pi solar inverter & soil-sensor gateways
//! and air-gapped trustee workstations (compatible with `agent-vault env-run` zero-disk-secret mode).
//!
//! Security Invariants:
//! - Before emitting ANY XMSS Merkle-Lamport signature, the incremented `next_leaf_index` is
//!   flushed and atomically renamed onto non-volatile storage (`fsync` + `rename`), guaranteeing
//!   that power loss or gateway reboots NEVER reuse a one-time Lamport leaf key.
//! - Supports stateless NIST FIPS 205 `SLH-DSA-SHA2-128f` signatures with monotonic nonces.
//! - Supports `--env-seeds` zero-disk-secret mode: private seeds are read strictly from environment
//!   variables (`COMMONS_HARDWARE_XMSS_SEED`, `COMMONS_HARDWARE_SLH_SEED`,
//!   `COMMONS_DEED_ORACLE_XMSS_SEED`, `COMMONS_DEED_ORACLE_SLH_SEED`) and NEVER written to disk.
//! - Real physical smart-meter HTTP adapters (`shelly-em`, `enphase`, `home-assistant`, `generic-json`)
//!   with monotonic cumulative-kWh delta tracking — strictly refuses to emit synthetic readings.

use newsatoshi_commons::storage::{default_hardware_slh_seed, default_hardware_xmss_seed};
use newsatoshi_commons::{
    hash32_from_hex, sha256_concat, CheckedWorkProof, ExternalDeedPacket, ExternalTelemetryPacket,
    HardwareDeviceConfig, HardwareTelemetrySigner, Hash32, MerkleLamportKeypair, SlhDsaKeypair,
    DEFAULT_TREE_HEIGHT, PROTOCOL_VERSION, SLH_DSA_PARAM_SET,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_KEYFILE: &str = "./hw-signer-state.json";
const DEFAULT_ENDPOINT: &str = "https://commons.newsatoshi.org/api/telemetry";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct HardwareKeyfile {
    protocol_version: String,
    device_id: String,
    parcel_id: u64,
    tree_height: u8,
    #[serde(default)]
    env_seeds_only: bool,
    #[serde(default)]
    xmss_master_seed_hex: String,
    hardware_merkle_root: String,
    next_leaf_index: u32,
    tree_capacity: u32,
    slh_dsa_param_set: String,
    #[serde(default)]
    slh_dsa_seed_hex: String,
    slh_dsa_pk_hex: String,
    slh_dsa_pk_commitment: String,
    next_slh_nonce: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_cumulative_kwh: Option<f64>,
    provisioned_at_unix: u64,
}

#[derive(Debug, Deserialize)]
struct StdinReadingInput {
    kwh_delivered: u64,
    soil_health_score: u16,
    parcel_id: Option<u64>,
    payout_love: Option<u64>,
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("help");
    let rest = args.get(1..).unwrap_or(&[]);

    let res = match cmd {
        "provision" => run_provision(rest),
        "sign" => run_sign(rest),
        "daemon" => run_daemon(rest),
        "poll-meter" => run_poll_meter(rest),
        "sign-deed" => run_sign_deed(rest),
        "help" | "-h" | "--help" => {
            print_usage();
            Ok(())
        }
        other => {
            eprintln!("Unknown subcommand: '{other}'\n");
            print_usage();
            Err(format!("unknown subcommand '{other}'"))
        }
    };

    if let Err(err) = res {
        eprintln!("error: {err}");
        process::exit(1);
    }
}

fn print_usage() {
    println!(
        "\
newsatoshi-hw-signer v{} ({PROTOCOL_VERSION}) — Field Solar/Sensor Post-Quantum Signer & Meter Daemon

USAGE:
  newsatoshi-hw-signer <SUBCOMMAND> [OPTIONS]

SUBCOMMANDS:
  provision    Initialize hardware state file with burned-in XMSS root & FIPS 205 SLH-DSA public key
                 --keyfile <path>      Output state/keyfile path (default: {DEFAULT_KEYFILE})
                 --device-id <id>      Hardware device identifier (required)
                 --parcel <u64>        Assigned parcel ID (required)
                 --height <1..12>      XMSS Merkle tree height (default: 6 => 64 leaves)
                 --env-seeds           Zero-disk-secret mode: read COMMONS_HARDWARE_XMSS_SEED &
                                       COMMONS_HARDWARE_SLH_SEED from env (e.g. via agent-vault env-run);
                                       writes ZERO private seeds to disk
                 --seed <passphrase>   Deterministic seed passphrase (local dev/gateway keyfile)
                 --default-engine-seed Match default local dev hardware root
                 --json                Print machine-readable provisioning summary

  sign         Sign a single real telemetry reading and atomically persist monotonic leaf/nonce state
                 --keyfile <path>      Device state/keyfile path (default: {DEFAULT_KEYFILE})
                 --env-seeds           Read seeds from COMMONS_HARDWARE_*_SEED env vars
                 --stdin-json          Read JSON reading from stdin ({{kwh_delivered, soil_health_score, payout_love, parcel_id?}})
                 --kwh <u64>           Measured energy output in kWh
                 --soil <u16>          Measured soil health score 0..100
                 --payout <u64>        Requested bounty payout in LOVE
                 --parcel <u64>        Optional parcel ID override
                 --scheme <xmss|slh_dsa> Signature scheme (default: slh_dsa)

  daemon       Read real JSONL sensor telemetry from stdin or a file and POST signed packets to Commons API
                 --keyfile <path>      Device state/keyfile path (default: {DEFAULT_KEYFILE})
                 --env-seeds           Read seeds from COMMONS_HARDWARE_*_SEED env vars
                 --endpoint <url>      Telemetry ingest URL (default: {DEFAULT_ENDPOINT})
                 --bearer-env <VAR>    Optional env var holding Bearer token
                 --stdin-jsonl         Stream newline-delimited JSON readings from stdin
                 --reading-file <path> Read newline-delimited JSON readings from a file
                 --scheme <xmss|slh_dsa> Signature scheme (default: slh_dsa)

  poll-meter   Poll a real physical smart meter / solar inverter HTTP API and POST signed kWh deltas
                 --keyfile <path>      Device state/keyfile path (default: {DEFAULT_KEYFILE})
                 --env-seeds           Read seeds from COMMONS_HARDWARE_*_SEED env vars
                 --adapter <kind>      shelly-em | enphase | home-assistant | generic-json (required)
                 --meter-url <url>     Local LAN inverter / meter / Home Assistant URL (required)
                 --meter-token-env <V> Optional env var holding meter Authorization Bearer token
                 --kwh-pointer </a/b>  JSON pointer for generic-json adapter (e.g. /energy/total_kwh)
                 --unit <kwh|wh>       Override meter reading unit (default: wh for shelly/enphase, kwh otherwise)
                 --soil <u16>          Verified soil health score 0..100 (or use --soil-url + --soil-pointer)
                 --soil-url <url>      Optional live soil sensor HTTP JSON URL
                 --soil-pointer </a/b> JSON pointer for --soil-url
                 --love-per-kwh <u64>  LOVE bounty per verified whole kWh (required)
                 --endpoint <url>      Telemetry ingest URL (default: {DEFAULT_ENDPOINT})
                 --bearer-env <VAR>    Optional env var holding Commons Bearer token
                 --scheme <xmss|slh_dsa> Signature scheme (default: slh_dsa)
                 --interval <secs>     Poll interval in seconds (default: 300)
                 --once                Poll once and exit (for cron / launchd / systemd timers)

  sign-deed    Sign an offline trustee parcel deed (POST /api/deeds) from env seeds (zero server keys)
                 --parcel <u64>        Parcel ID (required)
                 --acres <f64>         Parcel acreage (required)
                 --price <u64>         Purchase price in LOVE (required)
                 --label <text>        Human-readable deed label (required)
                 --deed-hash <64hex>   SHA-256 hash of recorded deed PDF/instrument (required)
                 --scheme <slh_dsa|xmss> Signature scheme (default: slh_dsa)
                 --nonce <u64>         Monotonic SLH-DSA deed nonce (required for slh_dsa)
                 --leaf-index <u32>    Monotonic XMSS leaf index (required for xmss)
                 --height <1..12>      XMSS tree height (default: 6)
                 --post <url>          Optional: POST signed deed directly to Commons /api/deeds
                 --bearer-env <VAR>    Optional env var holding Commons Bearer token

  help         Show this help message",
        env!("CARGO_PKG_VERSION")
    );
}

fn find_flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].clone())
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn parse_u64_req(args: &[String], flag: &str) -> Result<u64, String> {
    let v = find_flag_value(args, flag)
        .ok_or_else(|| format!("missing required flag {flag}"))?;
    v.parse::<u64>()
        .map_err(|_| format!("invalid u64 for {flag}: '{v}'"))
}

fn parse_u16_req(args: &[String], flag: &str) -> Result<u16, String> {
    let v = find_flag_value(args, flag)
        .ok_or_else(|| format!("missing required flag {flag}"))?;
    v.parse::<u16>()
        .map_err(|_| format!("invalid u16 for {flag}: '{v}'"))
}

fn parse_u8(args: &[String], flag: &str, default: u8) -> Result<u8, String> {
    match find_flag_value(args, flag) {
        Some(v) => v
            .parse::<u8>()
            .map_err(|_| format!("invalid u8 for {flag}: '{v}'")),
        None => Ok(default),
    }
}

fn read_seed_env(var: &str) -> Result<Hash32, String> {
    let raw = env::var(var).map_err(|_| {
        format!("missing environment variable {var} (inject via `agent-vault env-run`)")
    })?;
    hash32_from_hex(raw.trim()).map_err(|e| format!("invalid 64-hex seed in {var}: {e}"))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn save_keyfile_atomic(path: &Path, kf: &HardwareKeyfile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create dir '{}': {e}", parent.display()))?;
        }
    }
    let data = serde_json::to_vec_pretty(kf)
        .map_err(|e| format!("failed to serialize keyfile: {e}"))?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_name = format!(
        ".{}.tmp.{}.{}",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("hw-key"),
        std::process::id(),
        nanos
    );
    let tmp_path = path.with_file_name(tmp_name);
    {
        let mut f = File::create(&tmp_path)
            .map_err(|e| format!("failed to create temp keyfile '{}': {e}", tmp_path.display()))?;
        f.write_all(&data)
            .map_err(|e| format!("failed to write temp keyfile: {e}"))?;
        f.sync_all()
            .map_err(|e| format!("failed to fsync temp keyfile: {e}"))?;
    }
    fs::rename(&tmp_path, path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        format!("failed to atomically replace keyfile '{}': {e}", path.display())
    })
}

fn load_keyfile(path: &Path) -> Result<HardwareKeyfile, String> {
    let raw = fs::read_to_string(path)
        .map_err(|e| format!("failed to read keyfile '{}': {e}", path.display()))?;
    serde_json::from_str::<HardwareKeyfile>(&raw)
        .map_err(|e| format!("failed to parse keyfile '{}': {e}", path.display()))
}

fn resolve_hw_seeds(kf: &HardwareKeyfile, force_env: bool) -> Result<(Hash32, Hash32), String> {
    if force_env || kf.env_seeds_only || kf.xmss_master_seed_hex.trim().is_empty() {
        let xmss = read_seed_env("COMMONS_HARDWARE_XMSS_SEED")?;
        let slh = read_seed_env("COMMONS_HARDWARE_SLH_SEED")?;
        Ok((xmss, slh))
    } else {
        let xmss = hash32_from_hex(&kf.xmss_master_seed_hex)?;
        let slh = hash32_from_hex(&kf.slh_dsa_seed_hex)?;
        Ok((xmss, slh))
    }
}

fn run_provision(args: &[String]) -> Result<(), String> {
    let keyfile_path =
        PathBuf::from(find_flag_value(args, "--keyfile").unwrap_or_else(|| DEFAULT_KEYFILE.into()));
    let device_id = find_flag_value(args, "--device-id")
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "missing required --device-id <id>".to_string())?;
    let parcel_id = parse_u64_req(args, "--parcel")?;
    let height = parse_u8(args, "--height", DEFAULT_TREE_HEIGHT)?;
    let use_env_seeds = has_flag(args, "--env-seeds");
    let use_default_engine_seed = has_flag(args, "--default-engine-seed");
    let as_json = has_flag(args, "--json");

    let (xmss_seed, slh_seed) = if use_env_seeds {
        (
            read_seed_env("COMMONS_HARDWARE_XMSS_SEED")?,
            read_seed_env("COMMONS_HARDWARE_SLH_SEED")?,
        )
    } else if use_default_engine_seed {
        (default_hardware_xmss_seed(), default_hardware_slh_seed())
    } else if let Some(passphrase) = find_flag_value(args, "--seed") {
        let cfg = HardwareDeviceConfig::from_passphrase(&device_id, parcel_id, height, &passphrase);
        let slh = sha256_concat(&[
            b"NEWSATOSHI_HW_SLH_SEED_V2_1",
            device_id.as_bytes(),
            &parcel_id.to_be_bytes(),
            passphrase.as_bytes(),
        ]);
        (cfg.master_seed, slh)
    } else {
        let entropy = format!("{}:{}:{}", device_id, parcel_id, unix_now());
        let xmss = sha256_concat(&[b"NEWSATOSHI_HW_XMSS_AUTO_V2_1", entropy.as_bytes()]);
        let slh = sha256_concat(&[b"NEWSATOSHI_HW_SLH_AUTO_V2_1", entropy.as_bytes()]);
        (xmss, slh)
    };

    let xmss_kp = MerkleLamportKeypair::from_seed(xmss_seed, height).map_err(|e| e.to_string())?;
    let slh_kp = SlhDsaKeypair::from_seed(slh_seed);

    let keyfile = HardwareKeyfile {
        protocol_version: PROTOCOL_VERSION.to_string(),
        device_id: device_id.clone(),
        parcel_id,
        tree_height: height,
        env_seeds_only: use_env_seeds,
        xmss_master_seed_hex: if use_env_seeds {
            String::new()
        } else {
            hex::encode(xmss_seed)
        },
        hardware_merkle_root: hex::encode(xmss_kp.root()),
        next_leaf_index: 0,
        tree_capacity: xmss_kp.total_capacity(),
        slh_dsa_param_set: SLH_DSA_PARAM_SET.to_string(),
        slh_dsa_seed_hex: if use_env_seeds {
            String::new()
        } else {
            hex::encode(slh_seed)
        },
        slh_dsa_pk_hex: slh_kp.pk.to_hex(),
        slh_dsa_pk_commitment: hex::encode(slh_kp.pk.commitment()),
        next_slh_nonce: 0,
        last_cumulative_kwh: None,
        provisioned_at_unix: unix_now(),
    };

    save_keyfile_atomic(&keyfile_path, &keyfile)?;

    if as_json {
        let summary = serde_json::json!({
            "ok": true,
            "keyfile": keyfile_path.display().to_string(),
            "env_seeds_only": keyfile.env_seeds_only,
            "device_id": keyfile.device_id,
            "parcel_id": keyfile.parcel_id,
            "tree_height": keyfile.tree_height,
            "tree_capacity": keyfile.tree_capacity,
            "hardware_merkle_root": keyfile.hardware_merkle_root,
            "slh_dsa_param_set": keyfile.slh_dsa_param_set,
            "slh_dsa_pk_hex": keyfile.slh_dsa_pk_hex,
            "slh_dsa_pk_commitment": keyfile.slh_dsa_pk_commitment,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?
        );
    } else {
        println!("=== NEWSATOSHI HARDWARE TELEMETRY SIGNER PROVISIONED ===");
        println!("State File            : {}", keyfile_path.display());
        println!(
            "Secret Mode           : {}",
            if keyfile.env_seeds_only {
                "ZERO-DISK-SECRETS (env / agent-vault only)"
            } else {
                "local keyfile"
            }
        );
        println!("Device ID             : {}", keyfile.device_id);
        println!("Parcel ID             : {}", keyfile.parcel_id);
        println!(
            "XMSS Merkle Root      : {} ({} leaves, height={})",
            keyfile.hardware_merkle_root, keyfile.tree_capacity, keyfile.tree_height
        );
        println!(
            "SLH-DSA ({}) PK : {}",
            keyfile.slh_dsa_param_set, keyfile.slh_dsa_pk_hex
        );
        println!("SLH-DSA PK Commitment : {}", keyfile.slh_dsa_pk_commitment);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn sign_and_persist_packet(
    keyfile_path: &Path,
    parcel_override: Option<u64>,
    kwh_delivered: u64,
    soil_health_score: u16,
    payout_love: u64,
    scheme: &str,
    force_env_seeds: bool,
    new_cumulative_kwh: Option<f64>,
) -> Result<(ExternalTelemetryPacket, HardwareKeyfile), String> {
    let mut kf = load_keyfile(keyfile_path)?;
    let (xmss_seed, slh_seed) = resolve_hw_seeds(&kf, force_env_seeds)?;
    let parcel_id = parcel_override.unwrap_or(kf.parcel_id);
    let ts = unix_now();
    if let Some(cum) = new_cumulative_kwh {
        kf.last_cumulative_kwh = Some(cum);
    }

    let packet = match scheme {
        "slh_dsa" | "slh-dsa" | "fips205" => {
            let slh_kp = SlhDsaKeypair::from_seed(slh_seed);
            if slh_kp.pk.to_hex() != kf.slh_dsa_pk_hex {
                return Err(
                    "SLH-DSA seed does not match public key recorded in state file".to_string(),
                );
            }
            let nonce = kf.next_slh_nonce;
            kf.next_slh_nonce = nonce
                .checked_add(1)
                .ok_or_else(|| "SLH-DSA nonce overflow".to_string())?;
            save_keyfile_atomic(keyfile_path, &kf)?;

            ExternalTelemetryPacket::sign_slh_dsa(
                &slh_kp,
                kf.device_id.clone(),
                parcel_id,
                kwh_delivered,
                soil_health_score,
                payout_love,
                nonce,
                ts,
            )
        }
        _ => {
            let xmss_kp = MerkleLamportKeypair::from_seed_at_index(
                xmss_seed,
                kf.tree_height,
                kf.next_leaf_index,
            )
            .map_err(|e| e.to_string())?;
            if hex::encode(xmss_kp.root()) != kf.hardware_merkle_root {
                return Err(
                    "XMSS seed does not match hardware_merkle_root recorded in state file"
                        .to_string(),
                );
            }
            let mut signer = HardwareTelemetrySigner::from_keypair(
                kf.device_id.clone(),
                parcel_id,
                xmss_kp,
            );
            let signed = signer
                .sign_telemetry(kwh_delivered, soil_health_score, payout_love, ts)
                .map_err(|e| e.to_string())?;
            if !signed.verify() {
                return Err("internal XMSS self-verification failed".to_string());
            }
            kf.next_leaf_index = signer.next_leaf_index();
            save_keyfile_atomic(keyfile_path, &kf)?;

            let wire = signed.to_external_wire_packet(payout_love);
            debug_assert_eq!(
                wire.telemetry_msg_hash,
                hex::encode(CheckedWorkProof::energy_msg(
                    parcel_id,
                    kwh_delivered,
                    soil_health_score,
                    payout_love
                ))
            );
            wire
        }
    };

    Ok((packet, kf))
}

fn post_json(
    agent: &ureq::Agent,
    endpoint: &str,
    payload_json: &str,
    bearer_token: Option<&str>,
) -> Result<Value, String> {
    let mut req = agent
        .post(endpoint)
        .header("Content-Type", "application/json")
        .header("User-Agent", "newsatoshi-hw-signer/0.5.0");
    if let Some(tok) = bearer_token {
        req = req.header("Authorization", &format!("Bearer {tok}"));
    }
    let mut resp = req
        .send(payload_json)
        .map_err(|e| format!("failed to POST to '{endpoint}': {e}"))?;
    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("failed reading response body: {e}"))?;
    Ok(serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!({ "raw": body })))
}

fn get_json(agent: &ureq::Agent, url: &str, bearer_token: Option<&str>) -> Result<Value, String> {
    let mut req = agent
        .get(url)
        .header("Accept", "application/json")
        .header("User-Agent", "newsatoshi-hw-signer/0.5.0");
    if let Some(tok) = bearer_token {
        req = req.header("Authorization", &format!("Bearer {tok}"));
    }
    let mut resp = req
        .call()
        .map_err(|e| format!("failed GET '{url}': {e}"))?;
    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("failed reading meter response from '{url}': {e}"))?;
    serde_json::from_str::<Value>(&body)
        .map_err(|e| format!("invalid JSON returned by meter '{url}': {e}"))
}

fn json_value_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Extracts a real cumulative kWh reading from supported physical inverter/meter APIs.
fn extract_meter_cumulative_kwh(
    adapter: &str,
    json: &Value,
    kwh_pointer: Option<&str>,
    unit_override: Option<&str>,
) -> Result<f64, String> {
    let (raw_val, default_unit) = match adapter {
        "shelly-em" => {
            // Shelly Pro EM / EM3 (`GET /rpc/EMData.GetStatus?id=0` or Gen1 `/status`)
            let v = json
                .pointer("/total_act")
                .or_else(|| json.pointer("/total_act_energy"))
                .or_else(|| json.pointer("/a_total_act_energy"))
                .or_else(|| json.pointer("/emeters/0/total"))
                .and_then(json_value_as_f64)
                .ok_or_else(|| {
                    "shelly-em response missing total_act / total_act_energy / emeters/0/total"
                        .to_string()
                })?;
            (v, "wh")
        }
        "enphase" => {
            // Enphase IQ Envoy (`GET /production.json`)
            let v = json
                .pointer("/production/1/whLifetime")
                .or_else(|| json.pointer("/production/0/whLifetime"))
                .or_else(|| json.pointer("/whLifetime"))
                .and_then(json_value_as_f64)
                .ok_or_else(|| {
                    "enphase response missing production/*/whLifetime field".to_string()
                })?;
            (v, "wh")
        }
        "home-assistant" => {
            // Home Assistant REST API (`GET /api/states/<entity_id>`)
            let v = json
                .pointer("/state")
                .and_then(json_value_as_f64)
                .ok_or_else(|| "home-assistant response missing numeric /state field".to_string())?;
            let unit = json
                .pointer("/attributes/unit_of_measurement")
                .and_then(|u| u.as_str())
                .map(|s| {
                    if s.eq_ignore_ascii_case("wh") {
                        "wh"
                    } else {
                        "kwh"
                    }
                })
                .unwrap_or("kwh");
            (v, unit)
        }
        "generic-json" => {
            let ptr = kwh_pointer
                .ok_or_else(|| "generic-json adapter requires --kwh-pointer </path>".to_string())?;
            let v = json
                .pointer(ptr)
                .and_then(json_value_as_f64)
                .ok_or_else(|| format!("generic-json pointer '{ptr}' missing or non-numeric"))?;
            (v, "kwh")
        }
        other => {
            return Err(format!(
                "unsupported meter adapter '{other}' (expected shelly-em, enphase, home-assistant, or generic-json)"
            ));
        }
    };

    if !raw_val.is_finite() || raw_val < 0.0 {
        return Err(format!("meter returned invalid energy value: {raw_val}"));
    }

    let unit = unit_override.unwrap_or(default_unit).to_ascii_lowercase();
    let kwh = if unit == "wh" {
        raw_val / 1000.0
    } else if unit == "kwh" {
        raw_val
    } else {
        return Err(format!("unsupported --unit '{unit}' (expected kwh or wh)"));
    };
    Ok(kwh)
}

fn run_sign(args: &[String]) -> Result<(), String> {
    let keyfile_path =
        PathBuf::from(find_flag_value(args, "--keyfile").unwrap_or_else(|| DEFAULT_KEYFILE.into()));
    let scheme = find_flag_value(args, "--scheme").unwrap_or_else(|| "xmss".to_string());
    let env_seeds = has_flag(args, "--env-seeds");

    let (parcel_override, kwh, soil, payout) = if has_flag(args, "--stdin-json") {
        let mut buf = String::new();
        io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| format!("failed to read stdin JSON: {e}"))?;
        let parsed: StdinReadingInput = serde_json::from_str(&buf)
            .map_err(|e| format!("invalid stdin JSON reading: {e}"))?;
        let payout = parsed
            .payout_love
            .ok_or_else(|| "stdin JSON reading must include 'payout_love'".to_string())?;
        (
            parsed.parcel_id,
            parsed.kwh_delivered,
            parsed.soil_health_score,
            payout,
        )
    } else {
        let parcel_override = match find_flag_value(args, "--parcel") {
            Some(v) => Some(
                v.parse::<u64>()
                    .map_err(|_| format!("invalid u64 for --parcel: '{v}'"))?,
            ),
            None => None,
        };
        let kwh = parse_u64_req(args, "--kwh")?;
        let soil = parse_u16_req(args, "--soil")?;
        let payout = parse_u64_req(args, "--payout")?;
        (parcel_override, kwh, soil, payout)
    };

    let (packet, _kf) = sign_and_persist_packet(
        &keyfile_path,
        parcel_override,
        kwh,
        soil,
        payout,
        &scheme,
        env_seeds,
        None,
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&packet).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn run_daemon(args: &[String]) -> Result<(), String> {
    use std::io::BufRead;

    let keyfile_path =
        PathBuf::from(find_flag_value(args, "--keyfile").unwrap_or_else(|| DEFAULT_KEYFILE.into()));
    let endpoint =
        find_flag_value(args, "--endpoint").unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());
    let scheme = find_flag_value(args, "--scheme").unwrap_or_else(|| "xmss".to_string());
    let env_seeds = has_flag(args, "--env-seeds");
    let use_stdin = has_flag(args, "--stdin-jsonl");
    let reading_file = find_flag_value(args, "--reading-file");
    let bearer_token = match find_flag_value(args, "--bearer-env") {
        Some(var) => Some(env::var(&var).map_err(|_| format!("env var {var} is not set"))?),
        None => None,
    };

    if !use_stdin && reading_file.is_none() {
        return Err(
            "daemon requires a real sensor source: pass --stdin-jsonl or --reading-file <path> (will not fabricate synthetic sensor readings)"
                .to_string(),
        );
    }

    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build();
    let agent: ureq::Agent = config.into();

    let lines: Vec<String> = if let Some(path) = reading_file {
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("failed to read sensor file '{path}': {e}"))?;
        content
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    } else {
        let stdin = io::stdin();
        stdin
            .lock()
            .lines()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("failed reading --stdin-jsonl: {e}"))?
            .into_iter()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    };

    if lines.is_empty() {
        return Err("sensor input stream contained 0 readings".to_string());
    }

    for (idx, line) in lines.iter().enumerate() {
        let reading: StdinReadingInput = serde_json::from_str(line)
            .map_err(|e| format!("invalid JSONL reading at line {}: {e}", idx + 1))?;
        let payout = reading
            .payout_love
            .ok_or_else(|| format!("line {} missing required 'payout_love'", idx + 1))?;

        let (packet, kf) = sign_and_persist_packet(
            &keyfile_path,
            reading.parcel_id,
            reading.kwh_delivered,
            reading.soil_health_score,
            payout,
            &scheme,
            env_seeds,
            None,
        )?;

        let payload_json = serde_json::to_string(&packet).map_err(|e| e.to_string())?;
        let parsed = post_json(&agent, &endpoint, &payload_json, bearer_token.as_deref())?;

        let ok = parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        let proof_id = parsed
            .get("stark_proof")
            .and_then(|p| p.get("proof_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("none");

        println!(
            "[hw-signer #{:03}] scheme={} kwh={} soil={} ok={} stark_proof={} xmss_next={}/{} slh_next={}",
            idx + 1,
            packet.signature_scheme,
            reading.kwh_delivered,
            reading.soil_health_score,
            ok,
            proof_id,
            kf.next_leaf_index,
            kf.tree_capacity,
            kf.next_slh_nonce
        );

        if !ok {
            let err = parsed
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(format!("server rejected hardware telemetry packet: {err}"));
        }
    }

    Ok(())
}

fn run_poll_meter(args: &[String]) -> Result<(), String> {
    let keyfile_path =
        PathBuf::from(find_flag_value(args, "--keyfile").unwrap_or_else(|| DEFAULT_KEYFILE.into()));
    let adapter = find_flag_value(args, "--adapter")
        .ok_or_else(|| "missing required --adapter <shelly-em|enphase|home-assistant|generic-json>".to_string())?;
    let meter_url = find_flag_value(args, "--meter-url")
        .ok_or_else(|| "missing required --meter-url <url>".to_string())?;
    let kwh_pointer = find_flag_value(args, "--kwh-pointer");
    let unit_override = find_flag_value(args, "--unit");
    let love_per_kwh = parse_u64_req(args, "--love-per-kwh")?;
    let endpoint =
        find_flag_value(args, "--endpoint").unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());
    let scheme = find_flag_value(args, "--scheme").unwrap_or_else(|| "xmss".to_string());
    let env_seeds = has_flag(args, "--env-seeds");
    let once = has_flag(args, "--once");
    let interval_secs: u64 = match find_flag_value(args, "--interval") {
        Some(v) => v.parse::<u64>().map_err(|_| format!("invalid --interval '{v}'"))?,
        None => 300,
    };

    let soil_static = match find_flag_value(args, "--soil") {
        Some(v) => Some(v.parse::<u16>().map_err(|_| format!("invalid --soil '{v}'"))?),
        None => None,
    };
    let soil_url = find_flag_value(args, "--soil-url");
    let soil_pointer = find_flag_value(args, "--soil-pointer");
    if soil_static.is_none() && soil_url.is_none() {
        return Err("provide either --soil <0..100> or --soil-url <url> + --soil-pointer </path>".to_string());
    }

    let meter_token = match find_flag_value(args, "--meter-token-env") {
        Some(var) => Some(env::var(&var).map_err(|_| format!("env var {var} is not set"))?),
        None => None,
    };
    let bearer_token = match find_flag_value(args, "--bearer-env") {
        Some(var) => Some(env::var(&var).map_err(|_| format!("env var {var} is not set"))?),
        None => None,
    };

    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build();
    let agent: ureq::Agent = config.into();

    loop {
        let meter_json = get_json(&agent, &meter_url, meter_token.as_deref())?;
        let current_kwh = extract_meter_cumulative_kwh(
            &adapter,
            &meter_json,
            kwh_pointer.as_deref(),
            unit_override.as_deref(),
        )?;

        let mut kf = load_keyfile(&keyfile_path)?;
        let prev_kwh = match kf.last_cumulative_kwh {
            Some(prev) => prev,
            None => {
                // First observation establishes real physical baseline without fabricating a delta.
                kf.last_cumulative_kwh = Some(current_kwh);
                save_keyfile_atomic(&keyfile_path, &kf)?;
                println!(
                    "[poll-meter] recorded initial baseline cumulative_kwh={current_kwh:.4} (0 delta emitted — waiting for real energy production)"
                );
                if once {
                    return Ok(());
                }
                thread::sleep(Duration::from_secs(interval_secs));
                continue;
            }
        };

        let delta_float = current_kwh - prev_kwh;
        if delta_float < 1.0 {
            println!(
                "[poll-meter] cumulative_kwh={current_kwh:.4} (delta={delta_float:.4} < 1 kWh — skipping; zero synthetic readings)"
            );
            if once {
                return Ok(());
            }
            thread::sleep(Duration::from_secs(interval_secs));
            continue;
        }

        let delta_kwh = delta_float.floor() as u64;
        let committed_cum_kwh = prev_kwh + (delta_kwh as f64);
        let payout_love = delta_kwh
            .checked_mul(love_per_kwh)
            .ok_or_else(|| "payout_love overflow".to_string())?;

        let soil_score = if let Some(ref s_url) = soil_url {
            let s_json = get_json(&agent, s_url, meter_token.as_deref())?;
            let ptr = soil_pointer.as_deref().unwrap_or("/state");
            let v = s_json
                .pointer(ptr)
                .and_then(json_value_as_f64)
                .ok_or_else(|| format!("soil sensor pointer '{ptr}' missing or non-numeric"))?;
            v.round().clamp(0.0, 100.0) as u16
        } else {
            soil_static.unwrap()
        };

        let (packet, updated_kf) = sign_and_persist_packet(
            &keyfile_path,
            None,
            delta_kwh,
            soil_score,
            payout_love,
            &scheme,
            env_seeds,
            Some(committed_cum_kwh),
        )?;

        let payload_json = serde_json::to_string(&packet).map_err(|e| e.to_string())?;
        let parsed = post_json(&agent, &endpoint, &payload_json, bearer_token.as_deref())?;
        let ok = parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        let proof_id = parsed
            .get("stark_proof")
            .and_then(|p| p.get("proof_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("none");

        println!(
            "[poll-meter] adapter={adapter} delta_kwh={delta_kwh} soil={soil_score} payout={payout_love} ok={ok} stark_proof={proof_id} slh_next={}",
            updated_kf.next_slh_nonce
        );

        if !ok {
            let err = parsed
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(format!("server rejected meter telemetry: {err}"));
        }

        if once {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(interval_secs));
    }
}

fn run_sign_deed(args: &[String]) -> Result<(), String> {
    let parcel_id = parse_u64_req(args, "--parcel")?;
    let acres_raw = find_flag_value(args, "--acres")
        .ok_or_else(|| "missing required --acres <f64>".to_string())?;
    let acres: f64 = acres_raw
        .parse()
        .map_err(|_| format!("invalid --acres '{acres_raw}'"))?;
    let price_love = parse_u64_req(args, "--price")?;
    let label = find_flag_value(args, "--label")
        .ok_or_else(|| "missing required --label <text>".to_string())?;
    let deed_hash_hex = find_flag_value(args, "--deed-hash")
        .ok_or_else(|| "missing required --deed-hash <64hex>".to_string())?;
    let deed_hash = hash32_from_hex(&deed_hash_hex)?;
    let scheme = find_flag_value(args, "--scheme").unwrap_or_else(|| "slh_dsa".to_string());

    let packet = match scheme.as_str() {
        "slh_dsa" | "slh-dsa" | "fips205" => {
            let seed = read_seed_env("COMMONS_DEED_ORACLE_SLH_SEED")?;
            let nonce = parse_u64_req(args, "--nonce")?;
            let kp = SlhDsaKeypair::from_seed(seed);
            ExternalDeedPacket::sign_slh_dsa(
                &kp, parcel_id, acres, price_love, label, deed_hash, nonce,
            )
        }
        _ => {
            let seed = read_seed_env("COMMONS_DEED_ORACLE_XMSS_SEED")?;
            let leaf_index: u32 = find_flag_value(args, "--leaf-index")
                .ok_or_else(|| "XMSS sign-deed requires --leaf-index <u32>".to_string())?
                .parse()
                .map_err(|_| "invalid --leaf-index".to_string())?;
            let height = parse_u8(args, "--height", DEFAULT_TREE_HEIGHT)?;
            let mut kp = MerkleLamportKeypair::from_seed_at_index(seed, height, leaf_index)
                .map_err(|e| e.to_string())?;
            ExternalDeedPacket::sign_xmss(
                &mut kp, parcel_id, acres, price_love, label, deed_hash,
            )
            .map_err(|e| e.to_string())?
        }
    };

    let json_str = serde_json::to_string_pretty(&packet).map_err(|e| e.to_string())?;
    if let Some(post_url) = find_flag_value(args, "--post") {
        let bearer_token = match find_flag_value(args, "--bearer-env") {
            Some(var) => Some(env::var(&var).map_err(|_| format!("env var {var} is not set"))?),
            None => None,
        };
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .build();
        let agent: ureq::Agent = config.into();
        let resp = post_json(&agent, &post_url, &json_str, bearer_token.as_deref())?;
        println!(
            "{}",
            serde_json::to_string_pretty(&resp).map_err(|e| e.to_string())?
        );
    } else {
        println!("{json_str}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_adapters_extract_real_kwh() {
        let shelly = serde_json::json!({"total_act_energy": 12450.0});
        assert!((extract_meter_cumulative_kwh("shelly-em", &shelly, None, None).unwrap() - 12.45).abs() < 1e-6);

        let enphase = serde_json::json!({"production": [{"type": "inverters"}, {"type": "eim", "whLifetime": 842000.0}]});
        assert!((extract_meter_cumulative_kwh("enphase", &enphase, None, None).unwrap() - 842.0).abs() < 1e-6);

        let ha = serde_json::json!({"state": "315.7", "attributes": {"unit_of_measurement": "kWh"}});
        assert!((extract_meter_cumulative_kwh("home-assistant", &ha, None, None).unwrap() - 315.7).abs() < 1e-6);

        let generic = serde_json::json!({"inverter": {"lifetime_kwh": 99.5}});
        assert!((extract_meter_cumulative_kwh("generic-json", &generic, Some("/inverter/lifetime_kwh"), None).unwrap() - 99.5).abs() < 1e-6);
    }
}
