//! Standalone Hardware Telemetry Signer (`newsatoshi-hw-signer`)
//!
//! Designed for field-deployed Linux / Raspberry Pi solar inverter & soil-sensor gateways
//! (and cross-compilation to embedded targets such as `aarch64-unknown-linux-gnu`,
//! `armv7-unknown-linux-gnueabihf`, and `xtensa-esp32s3-espidf` for ESP32-S3 microcontrollers).
//!
//! Security Invariant:
//! - Before emitting ANY XMSS Merkle-Lamport signature, the incremented `next_leaf_index` is
//!   flushed and atomically renamed onto non-volatile storage (`fsync` + `rename`), guaranteeing
//!   that power loss or gateway reboots NEVER reuse a one-time Lamport leaf key.
//! - Also supports stateless NIST FIPS 205 `SLH-DSA-SHA2-128f` signatures with monotonic nonces.

use newsatoshi_commons::storage::{default_hardware_slh_seed, default_hardware_xmss_seed};
use newsatoshi_commons::{
    hash32_from_hex, sha256_concat, CheckedWorkProof, ExternalTelemetryPacket,
    HardwareDeviceConfig, HardwareTelemetrySigner, MerkleLamportKeypair, SlhDsaKeypair,
    DEFAULT_TREE_HEIGHT, PROTOCOL_VERSION, SLH_DSA_PARAM_SET,
};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_KEYFILE: &str = "./hw-signer-key.json";
const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8787/api/telemetry";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct HardwareKeyfile {
    protocol_version: String,
    device_id: String,
    parcel_id: u64,
    tree_height: u8,
    xmss_master_seed_hex: String,
    hardware_merkle_root: String,
    next_leaf_index: u32,
    tree_capacity: u32,
    slh_dsa_param_set: String,
    slh_dsa_seed_hex: String,
    slh_dsa_pk_hex: String,
    slh_dsa_pk_commitment: String,
    next_slh_nonce: u64,
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
newsatoshi-hw-signer v{} ({PROTOCOL_VERSION}) — Field Solar/Sensor Post-Quantum Telemetry Signer

TARGET PLATFORMS:
  - Linux / Raspberry Pi gateways (x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu, armv7)
  - Embedded ESP32-S3 gateways (cross-compile crypto kernel with xtensa-esp32s3-espidf;
    persist `next_leaf_index` in NVS flash / eFuse counter before transmitting packets)

USAGE:
  newsatoshi-hw-signer <SUBCOMMAND> [OPTIONS]

SUBCOMMANDS:
  provision    Generate persistent hardware keyfile with burned-in XMSS root & FIPS 205 SLH-DSA key
                 --keyfile <path>      Output keyfile path (default: {DEFAULT_KEYFILE})
                 --device-id <id>      Hardware device identifier (required)
                 --parcel <u64>        Assigned parcel ID (required)
                 --height <1..12>      XMSS Merkle tree height (default: 6 => 64 leaves)
                 --seed <passphrase>   Deterministic seed passphrase
                 --default-engine-seed Match default `newsatoshi-commons serve` hardware root
                 --json                Print machine-readable provisioning summary

  sign         Sign a single real telemetry reading and atomically persist monotonic leaf/nonce state
                 --keyfile <path>      Device keyfile path (default: {DEFAULT_KEYFILE})
                 --stdin-json          Read JSON reading from stdin ({{kwh_delivered, soil_health_score, payout_love, parcel_id?}})
                 --kwh <u64>           Measured energy output in kWh (required if not using --stdin-json)
                 --soil <u16>          Measured soil health score 0..100 (required if not using --stdin-json)
                 --payout <u64>        Requested bounty payout in LOVE (required if not using --stdin-json)
                 --parcel <u64>        Optional parcel ID override
                 --scheme <xmss|slh_dsa> Signature scheme (default: xmss)

  daemon       Read real JSONL sensor telemetry from stdin or a file and POST signed packets to Commons API
                 --keyfile <path>      Device keyfile path (default: {DEFAULT_KEYFILE})
                 --endpoint <url>      Telemetry ingest URL (default: {DEFAULT_ENDPOINT})
                 --stdin-jsonl         Stream newline-delimited JSON readings from stdin (one per sensor tick)
                 --reading-file <path> Read newline-delimited JSON readings from a file written by your meter/collector
                 --scheme <xmss|slh_dsa> Signature scheme (default: xmss)

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

fn run_provision(args: &[String]) -> Result<(), String> {
    let keyfile_path =
        PathBuf::from(find_flag_value(args, "--keyfile").unwrap_or_else(|| DEFAULT_KEYFILE.into()));
    let device_id = find_flag_value(args, "--device-id")
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "missing required --device-id <id>".to_string())?;
    let parcel_id = parse_u64_req(args, "--parcel")?;
    let height = parse_u8(args, "--height", DEFAULT_TREE_HEIGHT)?;
    let use_default_engine_seed = has_flag(args, "--default-engine-seed");
    let as_json = has_flag(args, "--json");

    let (xmss_seed, slh_seed) = if use_default_engine_seed {
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
        xmss_master_seed_hex: hex::encode(xmss_seed),
        hardware_merkle_root: hex::encode(xmss_kp.root()),
        next_leaf_index: 0,
        tree_capacity: xmss_kp.total_capacity(),
        slh_dsa_param_set: SLH_DSA_PARAM_SET.to_string(),
        slh_dsa_seed_hex: hex::encode(slh_seed),
        slh_dsa_pk_hex: slh_kp.pk.to_hex(),
        slh_dsa_pk_commitment: hex::encode(slh_kp.pk.commitment()),
        next_slh_nonce: 0,
        provisioned_at_unix: unix_now(),
    };

    save_keyfile_atomic(&keyfile_path, &keyfile)?;

    if as_json {
        let summary = serde_json::json!({
            "ok": true,
            "keyfile": keyfile_path.display().to_string(),
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
        println!("Keyfile               : {}", keyfile_path.display());
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

fn sign_and_persist_packet(
    keyfile_path: &Path,
    parcel_override: Option<u64>,
    kwh_delivered: u64,
    soil_health_score: u16,
    payout_love: u64,
    scheme: &str,
) -> Result<(ExternalTelemetryPacket, HardwareKeyfile), String> {
    let mut kf = load_keyfile(keyfile_path)?;
    let parcel_id = parcel_override.unwrap_or(kf.parcel_id);
    let ts = unix_now();

    let packet = match scheme {
        "slh_dsa" | "slh-dsa" | "fips205" => {
            let slh_seed = hash32_from_hex(&kf.slh_dsa_seed_hex)?;
            let slh_kp = SlhDsaKeypair::from_seed(slh_seed);
            let nonce = kf.next_slh_nonce;
            kf.next_slh_nonce = nonce
                .checked_add(1)
                .ok_or_else(|| "SLH-DSA nonce overflow".to_string())?;
            // Persist monotonic state BEFORE emitting packet
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
            let xmss_seed = hash32_from_hex(&kf.xmss_master_seed_hex)?;
            let xmss_kp = MerkleLamportKeypair::from_seed_at_index(
                xmss_seed,
                kf.tree_height,
                kf.next_leaf_index,
            )
            .map_err(|e| e.to_string())?;
            let mut signer = HardwareTelemetrySigner::from_keypair(
                kf.device_id.clone(),
                parcel_id,
                xmss_kp,
            );
            let signed = signer
                .sign_telemetry(kwh_delivered, soil_health_score, ts)
                .map_err(|e| e.to_string())?;
            if !signed.verify() {
                return Err("internal XMSS self-verification failed".to_string());
            }
            // Persist incremented monotonic leaf index atomically BEFORE returning packet
            kf.next_leaf_index = signer.next_leaf_index();
            save_keyfile_atomic(keyfile_path, &kf)?;

            let wire = signed.to_external_wire_packet(payout_love);
            // Sanity check message digest matches CheckedWorkProof
            debug_assert_eq!(
                wire.telemetry_msg_hash,
                hex::encode(CheckedWorkProof::energy_msg(
                    parcel_id,
                    kwh_delivered,
                    soil_health_score
                ))
            );
            wire
        }
    };

    Ok((packet, kf))
}

fn run_sign(args: &[String]) -> Result<(), String> {
    let keyfile_path =
        PathBuf::from(find_flag_value(args, "--keyfile").unwrap_or_else(|| DEFAULT_KEYFILE.into()));
    let scheme = find_flag_value(args, "--scheme").unwrap_or_else(|| "xmss".to_string());

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
    let use_stdin = has_flag(args, "--stdin-jsonl");
    let reading_file = find_flag_value(args, "--reading-file");

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
        )?;

        let payload_json = serde_json::to_string(&packet).map_err(|e| e.to_string())?;
        let mut resp = agent
            .post(&endpoint)
            .header("Content-Type", "application/json")
            .send(payload_json.as_str())
            .map_err(|e| format!("failed to POST signed telemetry to '{endpoint}': {e}"))?;

        let body = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("failed to read server response: {e}"))?;
        let parsed: serde_json::Value =
            serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!({ "raw": body }));

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
