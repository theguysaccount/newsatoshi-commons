use newsatoshi_commons::server::{run_server_blocking, ServerConfig};
use newsatoshi_commons::storage::{load_or_init_state, CommonsEngine, ExternalPublicKeysConfig};
use newsatoshi_commons::{
    prove_covenant_transition, sha256, AnchorCommitment, CheckedWorkProof, CommonsLedger,
    CovenantPublicInputs, CovenantRule, CovenantTxKind, HardwareDeviceConfig,
    HardwareTelemetrySigner, Hash32, JsonRpcAnchorClient, MerkleLamportKeypair, SeedStatusClient,
    SlhDsaKeypair, StreamingChannel, DEFAULT_MAINNET_SEED_STATUS_URL, DEFAULT_SEED_STATUS_URL,
    DEFAULT_TREE_HEIGHT, PROTOCOL_VERSION,
};
use serde::Serialize;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("simulate");
    let rest = args.get(1..).unwrap_or(&[]);

    let result = match cmd {
        "simulate" => run_simulate(rest),
        "telemetry-sign" => run_telemetry_sign(rest),
        "anchor" => run_anchor(rest),
        "serve" => run_serve(rest),
        "export-legal-schedule" => run_export_legal_schedule(rest),
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

    if let Err(err) = result {
        eprintln!("error: {err}");
        process::exit(1);
    }
}

fn print_usage() {
    println!(
        "\
newsatoshi-commons v{} ({PROTOCOL_VERSION}) — Autonomous Abundance Loops Engine & CLI

USAGE:
  newsatoshi-commons [SUBCOMMAND] [OPTIONS]

SUBCOMMANDS:
  serve            Start the live Commons Engine HTTP API server & optional static Web Cockpit
                     --port <u16>                 HTTP port (default: 8787)
                     --host <addr>                Bind address (default: 127.0.0.1)
                     --state-file <path>          Persistent JSON ledger path (default: ./data/commons-ledger.json)
                     --seed-url <url>             Live testnet /status URL (default: {DEFAULT_SEED_STATUS_URL})
                     --mainnet-seed-url <url>     Live mainnet /status URL (default: {DEFAULT_MAINNET_SEED_STATUS_URL})
                     --static-dir <dir>           Optional directory to serve compiled web UI (e.g. ./web/dist)
                     --admin-token <token>        Optional Bearer token (or COMMONS_ADMIN_TOKEN env var)
                     --verify-only                Strict Mainnet key-isolation mode (or COMMONS_VERIFY_ONLY=1): zero private keys in RAM/disk
                     --deed-oracle-root <64hex>   External Deed Oracle XMSS Merkle root (or COMMONS_DEED_ORACLE_ROOT)
                     --hardware-root <64hex>      External Hardware TEE XMSS Merkle root (or COMMONS_HARDWARE_ROOT)
                     --deed-oracle-slh-pk <64hex> External Deed Oracle FIPS 205 SLH-DSA pubkey (or COMMONS_DEED_ORACLE_SLH_PK)
                     --hardware-slh-pk <64hex>    External Hardware TEE FIPS 205 SLH-DSA pubkey (or COMMONS_HARDWARE_SLH_PK)

  export-legal-schedule
                   Export deterministic JSON schedule binding covenant_rule_hash, XMSS roots,
                   NIST FIPS 205 SLH-DSA public keys, and Newsatoshi L1 genesis hashes for legal/
                     --out <path>        Output JSON file path (default: stdout)
                     --state-file <path> Optional existing ledger state file to read active keys from

  simulate         Run full multi-parcel Autonomous Abundance Loop with XMSS, SLH-DSA & Winterfell STARKs
                     --height <1..12>    Merkle Lamport tree height (default: 6 => 64 leaves; 10 => 1024)
                     --endpoint <url>    JSON-RPC node endpoint (default: http://127.0.0.1:18332/)
                     --network <net>     L1 network tag: testnet | mainnet | regtest (default: testnet)
                     --json              Emit machine-readable simulation + STARK + anchor JSON

  telemetry-sign   Simulate solar inverter / greenhouse robot signing live telemetry packets
                     --device-id <id>    Hardware TEE device ID (default: solar-inverter-parcel-404)
                     --parcel <u64>      Parcel ID (default: 404)
                     --kwh <u64>         Verified energy output in kWh (default: 12500)
                     --soil <u16>        Soil health score 0..100 (default: 92)
                     --bounty <u64>      Bounty payout per packet in LOVE (default: 10000)
                     --height <1..12>    Merkle tree height (default: 6 => 64 leaves)
                     --count <u32>       Sequential packets to sign (default: 1)
                     --seed <str>        Deterministic hardware seed phrase
                     --verify            Verify & apply packet(s) into a live CommonsLedger
                     --json              Emit JSON packet(s) and anchor commitment

  anchor           Generate deterministic JSON-RPC 2.0 state-root anchor payload & newsatoshi CLI commands
                     --rule-hash <hex>   32-byte covenant rule hash (optional; defaults to demo rule)
                     --prev-root <hex>   32-byte previous state root (optional)
                     --state-root <hex>  32-byte new state root (optional)
                     --nonce <u64>       Ledger tx nonce (default: 1)
                     --balance <u64>     Covenant treasury balance in LOVE (default: 60000)
                     --parcels <u64>     Parcels acquired count (default: 1)
                     --kwh <u64>         Cumulative kWh verified (default: 12500)
                     --leaves <u64>      Merkle leaves consumed (default: 2)
                     --seed-url <url>    Bind live newsatoshi-seed L1 tip when --bind-seed is passed
                     --bind-seed         Poll live newsatoshi-seed /status and bind tip height/hash
                     --endpoint <url>    JSON-RPC endpoint (default: http://127.0.0.1:18332/)
                     --network <net>     Network tag (default: testnet)
                     --verify-stark      Also print OP_VERIFY_COVENANT_STARK JSON-RPC request
                     --live              POST JSON-RPC payload over HTTP to --endpoint

  help             Show this help message",
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

fn parse_u64(args: &[String], flag: &str, default: u64) -> Result<u64, String> {
    match find_flag_value(args, flag) {
        Some(v) => v
            .parse::<u64>()
            .map_err(|_| format!("invalid u64 for {flag}: '{v}'")),
        None => Ok(default),
    }
}

fn parse_u32(args: &[String], flag: &str, default: u32) -> Result<u32, String> {
    match find_flag_value(args, flag) {
        Some(v) => v
            .parse::<u32>()
            .map_err(|_| format!("invalid u32 for {flag}: '{v}'")),
        None => Ok(default),
    }
}

fn parse_u16(args: &[String], flag: &str, default: u16) -> Result<u16, String> {
    match find_flag_value(args, flag) {
        Some(v) => v
            .parse::<u16>()
            .map_err(|_| format!("invalid u16 for {flag}: '{v}'")),
        None => Ok(default),
    }
}

fn parse_u8(args: &[String], flag: &str, default: u8) -> Result<u8, String> {
    match find_flag_value(args, flag) {
        Some(v) => v
            .parse::<u8>()
            .map_err(|_| format!("invalid u8 for {flag}: '{v}'")),
        None => Ok(default),
    }
}

fn parse_hash32(hex_str: &str, field: &str) -> Result<Hash32, String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("invalid hex for {field}: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!(
            "{field} must be 32 bytes (64 hex chars), got {} bytes",
            bytes.len()
        ));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn run_serve(args: &[String]) -> Result<(), String> {
    let port = parse_u16(args, "--port", 8787)?;
    let host = find_flag_value(args, "--host").unwrap_or_else(|| "127.0.0.1".to_string());
    let state_file = PathBuf::from(
        find_flag_value(args, "--state-file")
            .unwrap_or_else(|| "./data/commons-ledger.json".to_string()),
    );
    let seed_url = find_flag_value(args, "--seed-url")
        .or_else(|| env::var("COMMONS_SEED_URL").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_SEED_STATUS_URL.to_string());
    let mainnet_seed_url = find_flag_value(args, "--mainnet-seed-url")
        .or_else(|| env::var("COMMONS_MAINNET_SEED_URL").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("none"))
        .or_else(|| Some(DEFAULT_MAINNET_SEED_STATUS_URL.to_string()));
    let static_dir = find_flag_value(args, "--static-dir").map(PathBuf::from);
    let admin_token = find_flag_value(args, "--admin-token")
        .or_else(|| env::var("COMMONS_ADMIN_TOKEN").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let verify_only_env = env::var("COMMONS_VERIFY_ONLY")
        .ok()
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    let verify_only = has_flag(args, "--verify-only") || verify_only_env;

    let deed_root = find_flag_value(args, "--deed-oracle-root")
        .or_else(|| env::var("COMMONS_DEED_ORACLE_ROOT").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let hw_root = find_flag_value(args, "--hardware-root")
        .or_else(|| env::var("COMMONS_HARDWARE_ROOT").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let deed_slh_pk = find_flag_value(args, "--deed-oracle-slh-pk")
        .or_else(|| env::var("COMMONS_DEED_ORACLE_SLH_PK").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let hw_slh_pk = find_flag_value(args, "--hardware-slh-pk")
        .or_else(|| env::var("COMMONS_HARDWARE_SLH_PK").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let external_public_keys = match (deed_root, hw_root) {
        (Some(d), Some(h)) => Some(ExternalPublicKeysConfig {
            deed_oracle_merkle_root: d,
            hardware_merkle_root: h,
            deed_oracle_slh_dsa_pk_hex: deed_slh_pk,
            hardware_slh_dsa_pk_hex: hw_slh_pk,
            max_bounty_payout: None,
            reinvest_bps: None,
            min_soil_health_score: None,
        }),
        (None, None) => None,
        _ => {
            return Err(
                "Both --deed-oracle-root and --hardware-root must be provided together"
                    .to_string(),
            );
        }
    };

    run_server_blocking(ServerConfig {
        host,
        port,
        state_file,
        seed_url,
        mainnet_seed_url,
        static_dir,
        admin_token,
        verify_only: verify_only || external_public_keys.is_some(),
        external_public_keys,
    })
}

fn run_export_legal_schedule(args: &[String]) -> Result<(), String> {
    let engine = match find_flag_value(args, "--state-file") {
        Some(path) => load_or_init_state(&PathBuf::from(path))?,
        None => CommonsEngine::new_empty(),
    };
    let schedule = engine.export_legal_schedule();
    let json = serde_json::to_string_pretty(&schedule)
        .map_err(|e| format!("failed to serialize legal schedule: {e}"))?;

    if let Some(out_path_str) = find_flag_value(args, "--out") {
        let out_path = PathBuf::from(&out_path_str);
        if let Some(parent) = out_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!("failed to create parent dir '{}': {e}", parent.display())
                })?;
            }
        }
        let mut f = File::create(&out_path)
            .map_err(|e| format!("failed to create '{}': {e}", out_path.display()))?;
        f.write_all(json.as_bytes())
            .map_err(|e| format!("failed to write '{}': {e}", out_path.display()))?;
        f.write_all(b"\n")
            .map_err(|e| format!("failed to finalize '{}': {e}", out_path.display()))?;
        println!(
            "Exported legal schedule -> {} (covenant_rule_hash: {})",
            out_path.display(),
            schedule.covenant_rule_hash
        );
    } else {
        println!("{json}");
    }
    Ok(())
}

#[derive(Serialize)]
struct SimulationReport {
    protocol: String,
    merkle_tree_height: u8,
    merkle_tree_capacity: u32,
    covenant_rule_hash: String,
    deed_oracle_merkle_root: String,
    hardware_merkle_root: String,
    deed_oracle_slh_dsa_pk: String,
    hardware_slh_dsa_pk: String,
    parcels_acquired: Vec<u64>,
    total_kwh_verified: u64,
    final_covenant_balance_love: u64,
    operator_payouts_love: u64,
    oracle_leaves_used: u32,
    hardware_leaves_used: u32,
    slh_dsa_signatures_verified: u64,
    stark_proof_bytes_len: usize,
    stark_prove_ms: u64,
    stark_verify_ms: u64,
    stark_security_bits: u32,
    adversarial_checks_blocked: Vec<String>,
    anchor_receipt: newsatoshi_commons::AnchorReceipt,
    anchor_rpc_request: newsatoshi_commons::JsonRpcRequest<newsatoshi_commons::AnchorRpcParams>,
}

fn run_simulate(args: &[String]) -> Result<(), String> {
    let height = parse_u8(args, "--height", DEFAULT_TREE_HEIGHT)?;
    let endpoint = find_flag_value(args, "--endpoint")
        .unwrap_or_else(|| "http://127.0.0.1:18332/".to_string());
    let network = find_flag_value(args, "--network").unwrap_or_else(|| "testnet".to_string());
    let as_json = has_flag(args, "--json");

    let mut ledger = CommonsLedger::new();

    // 1. Provision XMSS Merkleized Lamport Trees + NIST FIPS 205 SLH-DSA-SHA2-128f keys
    let mut deed_oracle =
        MerkleLamportKeypair::from_seed(sha256(b"perpetual_trust_merkle_oracle_v2_1"), height)
            .map_err(|e| e.to_string())?;
    let hw_config = HardwareDeviceConfig::from_passphrase(
        "solar-greenhouse-tee-01",
        404,
        height,
        "burned-in-silicon-seed-v2-1",
    );
    let mut hw_signer = HardwareTelemetrySigner::new(hw_config).map_err(|e| e.to_string())?;
    let deed_slh = SlhDsaKeypair::from_seed(sha256(b"perpetual_trust_slh_oracle_v2_1"));
    let hw_slh = SlhDsaKeypair::from_seed(sha256(b"solar_greenhouse_slh_tee_v2_1"));

    let rule = CovenantRule {
        max_bounty_payout: 20_000,
        reinvest_bps: 1_500, // 15% of streaming output compounds into buying adjacent land
        min_soil_health_score: 80,
        authorized_deed_oracle_pk: deed_oracle.root(),
        authorized_hardware_pk: hw_signer.root(),
        authorized_deed_oracle_slh_pk: Some(deed_slh.pk.commitment()),
        authorized_hardware_slh_pk: Some(hw_slh.pk.commitment()),
    };

    // 2. Mint Genesis Commons Treasury (100,000 LOVE)
    let treasury_ref = ledger.mint_genesis_covenant(100_000, &rule);

    // 3. Adversarial Guard #1: Attempt to strip the recursive covenant on change output
    let parcel1_deed = sha256(b"COUNTY_DEED_PARCEL_404_PERPETUAL_PURPOSE_TRUST_SEC_4_2");
    let parcel1_sig = deed_oracle
        .sign_next(CheckedWorkProof::deed_msg(404, parcel1_deed))
        .map_err(|e| e.to_string())?;

    let mut blocked = Vec::new();
    let strip_err = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            40_000,
            "PARCEL_404_SELLER",
            None,
            CheckedWorkProof::merkle_land_deed(404, parcel1_deed, parcel1_sig.clone()),
        )
        .expect_err("stripping covenant MUST fail");
    blocked.push(format!("covenant_strip: {strip_err}"));

    // 4. Legitimate Parcel #1 Acquisition (Oracle Leaf #0)
    let (_, change_ref) = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            40_000,
            "PARCEL_404_SELLER",
            Some(rule.rule_hash()),
            CheckedWorkProof::merkle_land_deed(404, parcel1_deed, parcel1_sig.clone()),
        )
        .map_err(|e| e.to_string())?;
    let mut treasury_ref = change_ref.expect("change UTXO expected");

    // 5. Adversarial Guard #2: Replay spent Oracle Leaf #0 MUST fail
    let replay_err = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            10_000,
            "ATTACKER",
            Some(rule.rule_hash()),
            CheckedWorkProof::merkle_land_deed(404, parcel1_deed, parcel1_sig),
        )
        .expect_err("replaying Merkle leaf #0 MUST fail");
    blocked.push(format!("merkle_leaf_replay: {replay_err}"));

    // 6. Adversarial Guard #3: Degraded soil health (62 < 80) MUST be rejected
    let degraded_packet = hw_signer
        .sign_telemetry(5_000, 62, 1_795_200_100)
        .map_err(|e| e.to_string())?;
    let soil_err = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            5_000,
            "DEGRADED_OPERATOR",
            Some(rule.rule_hash()),
            degraded_packet.to_checked_work_proof(),
        )
        .expect_err("soil health below floor MUST fail");
    blocked.push(format!("ecological_floor: {soil_err}"));

    // 7. Sequential Hardware Telemetry Attestations (XMSS Leaves #1, #2 + SLH-DSA Nonce #0)
    for (kwh, soil, bounty, ts) in [
        (12_500u64, 92u16, 10_000u64, 1_795_200_200u64),
        (14_200u64, 94u16, 10_000u64, 1_795_203_800u64),
    ] {
        let packet = hw_signer
            .sign_telemetry(kwh, soil, ts)
            .map_err(|e| e.to_string())?;
        assert!(packet.verify(), "signed hardware packet must self-verify");
        let (_, next_change) = ledger
            .spend_covenant(
                treasury_ref,
                &rule,
                bounty,
                "SOLAR_ROBOT_FLEET",
                Some(rule.rule_hash()),
                packet.to_checked_work_proof(),
            )
            .map_err(|e| e.to_string())?;
        treasury_ref = next_change.expect("remaining treasury change expected");
    }

    // Third telemetry bounty signed via NIST FIPS 205 SLH-DSA-SHA2-128f + proved via Winterfell STARK!
    let prev_root_before_slh = hex::encode(ledger.state_root);
    let balance_before_slh = ledger.total_covenant_balance(&rule);
    let slh_msg = CheckedWorkProof::energy_msg_with_nonce(404, 15_800, 95, 0);
    let slh_sig = hw_slh.sign_hash(slh_msg);
    let (_, next_change) = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            10_000,
            "SOLAR_ROBOT_FLEET",
            Some(rule.rule_hash()),
            CheckedWorkProof::slh_dsa_energy_deployment(
                404,
                15_800,
                95,
                0,
                hw_slh.pk.clone(),
                slh_sig,
            ),
        )
        .map_err(|e| e.to_string())?;
    let _treasury_ref = next_change.expect("remaining treasury change expected");

    let rule_hash_hex = hex::encode(rule.rule_hash());
    let stark_artifact = prove_covenant_transition(CovenantPublicInputs {
        tx_kind: CovenantTxKind::TelemetryBounty,
        label: "SLH-DSA Telemetry 15800 kWh / Soil 95".to_string(),
        tx_nonce: ledger.tx_nonce,
        covenant_rule_hash: rule_hash_hex.clone(),
        change_covenant_rule_hash: rule_hash_hex,
        prev_state_root: prev_root_before_slh,
        new_state_root: hex::encode(ledger.state_root),
        input_amount: balance_before_slh,
        payout_amount: 10_000,
        change_amount: balance_before_slh - 10_000,
        max_bounty_payout: rule.max_bounty_payout,
        reinvest_bps: rule.reinvest_bps,
        min_soil_health_score: rule.min_soil_health_score,
        soil_health_score: 95,
    })?;

    // 8. Machine & Local Grid Streaming Channels compound 15% spread back into the Commons
    let channel_runs = [
        (b"microgrid_ev_charger_ch1".as_slice(), 1_000u64, 200u64, 500u64),
        (b"greenhouse_produce_ch2".as_slice(), 1_000u64, 200u64, 500u64),
    ];
    for (seed_bytes, max_ticks, unit_price, ticks_consumed) in channel_runs {
        let (channel, secret_chain) =
            StreamingChannel::create_with_secret_chain(sha256(seed_bytes), max_ticks, unit_price);
        let preimage = secret_chain[(max_ticks - ticks_consumed) as usize];
        ledger
            .settle_streaming_channel(
                &channel,
                ticks_consumed,
                preimage,
                &rule,
                "SITE_MAINTENANCE_BOT",
            )
            .map_err(|e| e.to_string())?;
    }

    // 9. Consolidate compounded covenant UTXOs and acquire Parcel #2 using SLH-DSA Oracle Nonce #0
    let all_covenant_utxos = ledger.covenant_utxo_refs(&rule);
    let consolidated_ref = ledger
        .consolidate_covenant_utxos(&all_covenant_utxos, &rule)
        .map_err(|e| e.to_string())?;

    let parcel2_deed = sha256(b"COUNTY_DEED_PARCEL_405_ADJACENT_ACREAGE_TRUST");
    let parcel2_msg = CheckedWorkProof::deed_msg_with_nonce(405, parcel2_deed, 0);
    let parcel2_slh_sig = deed_slh.sign_hash(parcel2_msg);

    ledger
        .spend_covenant(
            consolidated_ref,
            &rule,
            45_000,
            "PARCEL_405_SELLER",
            Some(rule.rule_hash()),
            CheckedWorkProof::slh_dsa_land_deed(
                405,
                parcel2_deed,
                0,
                deed_slh.pk.clone(),
                parcel2_slh_sig,
            ),
        )
        .map_err(|e| e.to_string())?;

    // 10. Produce Deterministic JSON-RPC State Root Anchor Payload
    let commitment = ledger.build_anchor_commitment(&rule);
    let rpc_client = JsonRpcAnchorClient::new(endpoint, network);
    let anchor_rpc_request = rpc_client.build_anchor_request(&commitment, 1);
    let anchor_receipt = rpc_client.submit_dry_run(&commitment, 1);

    let report = SimulationReport {
        protocol: PROTOCOL_VERSION.to_string(),
        merkle_tree_height: height,
        merkle_tree_capacity: 1u32 << height,
        covenant_rule_hash: hex::encode(rule.rule_hash()),
        deed_oracle_merkle_root: hex::encode(deed_oracle.root()),
        hardware_merkle_root: hex::encode(hw_signer.root()),
        deed_oracle_slh_dsa_pk: deed_slh.pk.to_hex(),
        hardware_slh_dsa_pk: hw_slh.pk.to_hex(),
        parcels_acquired: ledger.acquired_parcels.clone(),
        total_kwh_verified: ledger.total_kwh_verified,
        final_covenant_balance_love: ledger.total_covenant_balance(&rule),
        operator_payouts_love: ledger.balance_of_owner("SOLAR_ROBOT_FLEET")
            + ledger.balance_of_owner("SITE_MAINTENANCE_BOT"),
        oracle_leaves_used: deed_oracle.next_index(),
        hardware_leaves_used: hw_signer.next_leaf_index(),
        slh_dsa_signatures_verified: ledger.total_slh_dsa_signatures_verified,
        stark_proof_bytes_len: stark_artifact.proof_bytes_len,
        stark_prove_ms: stark_artifact.prove_ms,
        stark_verify_ms: stark_artifact.verify_ms,
        stark_security_bits: stark_artifact.security_bits,
        adversarial_checks_blocked: blocked,
        anchor_receipt,
        anchor_rpc_request,
    };

    if as_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
    } else {
        println!("=== NEWSATOSHI 2.1 AUTONOMOUS ABUNDANCE LOOP SIMULATOR ===");
        println!(
            "Merkle Lamport Trees : height={} ({} one-time leaves per burned-in root)",
            report.merkle_tree_height, report.merkle_tree_capacity
        );
        println!("Covenant Rule Hash   : {}", report.covenant_rule_hash);
        println!("Deed Oracle Root     : {}", report.deed_oracle_merkle_root);
        println!("Hardware TEE Root    : {}", report.hardware_merkle_root);
        println!("Deed SLH-DSA PK      : {}", report.deed_oracle_slh_dsa_pk);
        println!("Hardware SLH-DSA PK  : {}", report.hardware_slh_dsa_pk);
        println!("----------------------------------------------------------");
        println!("Adversarial Checks Blocked:");
        for b in &report.adversarial_checks_blocked {
            println!("  [BLOCKED] {b}");
        }
        println!("----------------------------------------------------------");
        println!(
            "Parcels Acquired     : {:?} (Compounded into Parcel #2!)",
            report.parcels_acquired
        );
        println!("Total kWh Verified   : {} kWh", report.total_kwh_verified);
        println!(
            "Covenant Treasury    : {} LOVE (recursively locked)",
            report.final_covenant_balance_love
        );
        println!("Operator Payouts     : {} LOVE", report.operator_payouts_love);
        println!(
            "Winterfell 0.13 STARK: {} bytes | prove={}ms | verify={}ms | {}-bit security",
            report.stark_proof_bytes_len,
            report.stark_prove_ms,
            report.stark_verify_ms,
            report.stark_security_bits
        );
        println!(
            "Signatures Verified  : XMSS (Oracle {}, HW {}) | NIST FIPS 205 SLH-DSA ({})",
            report.oracle_leaves_used,
            report.hardware_leaves_used,
            report.slh_dsa_signatures_verified
        );
        println!("Final State Root     : {}", hex::encode(ledger.state_root));
        println!("----------------------------------------------------------");
        println!("L1 Anchor & Newsatoshi CLI:");
        println!("  Anchor Hash        : {}", report.anchor_receipt.anchor_hash);
        println!("  OP_RETURN Script   : {}", report.anchor_receipt.op_return_script_hex);
        println!("  CLI Lock Command   : {}", report.anchor_receipt.cli_lock_command);
    }

    Ok(())
}

#[derive(Serialize)]
struct TelemetryCliOutput {
    packets: Vec<newsatoshi_commons::TelemetryPacketJson>,
    ledger_verified: bool,
    updated_state_root: Option<String>,
    remaining_covenant_balance_love: Option<u64>,
    anchor_commitment: Option<AnchorCommitment>,
}

fn run_telemetry_sign(args: &[String]) -> Result<(), String> {
    let device_id = find_flag_value(args, "--device-id")
        .unwrap_or_else(|| "solar-inverter-parcel-404".to_string());
    let parcel_id = parse_u64(args, "--parcel", 404)?;
    let kwh = parse_u64(args, "--kwh", 12_500)?;
    let soil = parse_u16(args, "--soil", 92)?;
    let bounty = parse_u64(args, "--bounty", 10_000)?;
    let height = parse_u8(args, "--height", DEFAULT_TREE_HEIGHT)?;
    let count = parse_u32(args, "--count", 1)?;
    let seed_phrase =
        find_flag_value(args, "--seed").unwrap_or_else(|| "field-inverter-tee-key-01".to_string());
    let verify_on_ledger = has_flag(args, "--verify");
    let as_json = has_flag(args, "--json");

    if count == 0 {
        return Err("--count must be >= 1".to_string());
    }

    let config = HardwareDeviceConfig::from_passphrase(&device_id, parcel_id, height, &seed_phrase);
    let mut signer = HardwareTelemetrySigner::new(config).map_err(|e| e.to_string())?;
    let base_ts = current_unix_timestamp();

    let mut signed_packets = Vec::with_capacity(count as usize);
    for i in 0..count {
        let reading_kwh = kwh.saturating_add(u64::from(i) * 250);
        let packet = signer
            .sign_telemetry(reading_kwh, soil, base_ts + u64::from(i) * 60)
            .map_err(|e| e.to_string())?;
        if !packet.verify() {
            return Err("internal verification failure on freshly signed packet".to_string());
        }
        signed_packets.push(packet);
    }

    let mut updated_state_root = None;
    let mut remaining_balance = None;
    let mut anchor_commitment = None;

    if verify_on_ledger {
        let mut ledger = CommonsLedger::new();
        let rule = CovenantRule {
            max_bounty_payout: bounty.max(25_000),
            reinvest_bps: 1_500,
            min_soil_health_score: 80,
            authorized_deed_oracle_pk: sha256(b"placeholder_oracle_root"),
            authorized_hardware_pk: signer.root(),
            ..CovenantRule::default()
        };
        let initial_treasury = bounty.saturating_mul(u64::from(count)).saturating_add(50_000);
        let mut current_ref = ledger.mint_genesis_covenant(initial_treasury, &rule);

        for pkt in &signed_packets {
            let (_, next_ref) = ledger
                .spend_covenant(
                    current_ref,
                    &rule,
                    bounty,
                    &device_id,
                    Some(rule.rule_hash()),
                    pkt.to_checked_work_proof(),
                )
                .map_err(|e| format!("covenant engine rejected telemetry packet: {e}"))?;
            current_ref = next_ref.ok_or("unexpected treasury exhaustion")?;
        }

        updated_state_root = Some(hex::encode(ledger.state_root));
        remaining_balance = Some(ledger.total_covenant_balance(&rule));
        anchor_commitment = Some(ledger.build_anchor_commitment(&rule));
    }

    let json_packets: Vec<_> = signed_packets
        .iter()
        .map(|p| p.to_json_summary())
        .collect();

    if as_json {
        let out = TelemetryCliOutput {
            packets: json_packets,
            ledger_verified: verify_on_ledger,
            updated_state_root,
            remaining_covenant_balance_love: remaining_balance,
            anchor_commitment,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?
        );
    } else {
        println!("=== NEWSATOSHI HARDWARE TELEMETRY SIGNER (XMSS-LAMPORT) ===");
        println!("Device ID            : {device_id}");
        println!("Parcel ID            : {parcel_id}");
        println!(
            "Hardware Merkle Root : {} ({} leaves total)",
            hex::encode(signer.root()),
            signer.total_capacity()
        );
        for pkt in &json_packets {
            println!(
                "  [LEAF #{:04}] kwh={} | soil={}/100 | msg={}.. | remaining={}",
                pkt.leaf_index,
                pkt.kwh_delivered,
                pkt.soil_health_score,
                &pkt.telemetry_msg_hash[..16],
                pkt.remaining_leaves
            );
        }
        if let Some(root) = updated_state_root {
            println!("-----------------------------------------------------------");
            println!("Covenant Verification: PASSED ({count} packet(s) applied)");
            println!("Updated State Root   : {root}");
            if let Some(bal) = remaining_balance {
                println!("Remaining Treasury   : {bal} LOVE");
            }
            if let Some(anchor) = anchor_commitment {
                println!("L1 Anchor Hash       : {}", anchor.anchor_hash);
            }
        }
    }

    Ok(())
}

fn run_anchor(args: &[String]) -> Result<(), String> {
    let rule_hash = match find_flag_value(args, "--rule-hash") {
        Some(h) => parse_hash32(&h, "--rule-hash")?,
        None => sha256(b"NEWSATOSHI_DEMO_COVENANT_RULE_V2_1"),
    };
    let prev_root = match find_flag_value(args, "--prev-root") {
        Some(h) => parse_hash32(&h, "--prev-root")?,
        None => sha256(b"NEWSATOSHI_COMMONS_GENESIS"),
    };
    let state_root = match find_flag_value(args, "--state-root") {
        Some(h) => parse_hash32(&h, "--state-root")?,
        None => sha256(b"NEWSATOSHI_COMMONS_STATE_ROOT_1"),
    };

    let nonce = parse_u64(args, "--nonce", 1)?;
    let balance = parse_u64(args, "--balance", 60_000)?;
    let parcels = parse_u64(args, "--parcels", 1)?;
    let kwh = parse_u64(args, "--kwh", 12_500)?;
    let leaves = parse_u64(args, "--leaves", 2)?;
    let endpoint = find_flag_value(args, "--endpoint")
        .unwrap_or_else(|| "http://127.0.0.1:18332/".to_string());
    let network = find_flag_value(args, "--network").unwrap_or_else(|| "testnet".to_string());
    let seed_url = find_flag_value(args, "--seed-url")
        .unwrap_or_else(|| DEFAULT_SEED_STATUS_URL.to_string());
    let bind_seed = has_flag(args, "--bind-seed");
    let include_stark = has_flag(args, "--verify-stark");
    let live = has_flag(args, "--live");

    let mut commitment = AnchorCommitment::build(
        rule_hash, prev_root, state_root, nonce, balance, parcels, kwh, leaves,
    );
    if bind_seed {
        if network == "mainnet" {
            commitment = commitment.with_mainnet_genesis();
        } else {
            let snap = SeedStatusClient::new(seed_url, Duration::from_secs(5)).fetch_snapshot();
            let status = snap.status.ok_or_else(|| {
                format!(
                    "cannot bind live L1 seed tip: {}",
                    snap.error.unwrap_or_else(|| "seed unreachable".to_string())
                )
            })?;
            commitment = commitment.with_seed_status(&status);
        }
    }

    let client = JsonRpcAnchorClient::new(endpoint, network);

    println!("{}", client.anchor_payload_pretty(&commitment, 1));

    if include_stark {
        let stark_req = client.build_verify_stark_request(&commitment, 2);
        println!(
            "\n{}",
            serde_json::to_string_pretty(&stark_req).map_err(|e| e.to_string())?
        );
    }

    if live {
        let response = client.submit_live_http(&commitment, 1, Duration::from_secs(5))?;
        println!("\n=== LIVE NODE RESPONSE ===\n{response}");
    } else {
        let receipt = client.submit_dry_run(&commitment, 1);
        println!(
            "\n# Unbroadcast anchor_hash: {} | OP_RETURN: {}",
            receipt.anchor_hash, receipt.op_return_script_hex
        );
        println!("# Newsatoshi L1 CLI lock: {}", receipt.cli_lock_command);
        println!("# Newsatoshi L1 CLI memo: {}", receipt.cli_send_memo_command);
    }

    Ok(())
}
