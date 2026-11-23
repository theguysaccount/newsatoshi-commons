//! Standalone Trustless `BTC <-> LOVE` Atomic Swap CLI (`newsatoshi-swap`)
//!
//! Compiles BIP-199 Bitcoin SegWit v0 P2WSH HTLCs, decodes Lightning BOLT-11 invoices,
//! generates CSPRNG 32-byte preimages, emits exact `newsatoshi lock/unlock/expire` commands,
//! and verifies both chains live (`newsatoshi` L1 `/api/lock?hash=` + `mempool.space`).

use newsatoshi_commons::seed::{SeedStatusClient, DEFAULT_MAINNET_SEED_STATUS_URL, DEFAULT_SEED_STATUS_URL};
use newsatoshi_commons::swap::{
    compile_atomic_swap, decode_bolt11_invoice, generate_swap_secret, verify_preimage,
    verify_swap_on_chain, CompileSwapRequest,
};
use std::env;
use std::process;
use std::time::Duration;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("help");
    let rest = args.get(1..).unwrap_or(&[]);

    let res = match cmd {
        "keygen" => run_keygen(),
        "htlc" => run_htlc(rest),
        "decode-bolt11" => run_decode_bolt11(rest),
        "verify-preimage" => run_verify_preimage(rest),
        "watch" => run_watch(rest),
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
newsatoshi-swap v{} — Trustless Non-Custodial BTC <-> LOVE HTLC Atomic Swap CLI

USAGE:
  newsatoshi-swap <SUBCOMMAND> [OPTIONS]

SUBCOMMANDS:
  keygen           Generate CSPRNG 32-byte swap preimage + SHA-256 payment_hash (keep preimage secret!)

  htlc             Compile a canonical BIP-199 Bitcoin P2WSH HTLC + Newsatoshi L1 lock contract
                     --rail <btc_onchain|lightning_bolt11>  Swap rail (default: btc_onchain)
                     --direction <btc_to_love|love_to_btc>  Swap direction (default: btc_to_love)
                     --love-net <mainnet|testnet>           Newsatoshi network (default: mainnet)
                     --btc-net <mainnet|testnet|signet|regtest> Bitcoin network (default: mainnet)
                     --hash <64hex>                         SHA-256 payment hash (for btc_onchain)
                     --bolt11 <invoice>                     Lightning BOLT-11 invoice (for lightning_bolt11)
                     --love <decimal>                       LOVE amount (e.g. 250 or 0.5)
                     --sats <u64>                           BTC amount in satoshis
                     --love-to <ns1...>                     Newsatoshi claim address
                     --love-blocks <u32>                    Newsatoshi lock timeout in blocks (default: 144)
                     --btc-claim-pk <66hex>                 Compressed 33-byte BTC claim pubkey
                     --btc-refund-pk <66hex>                Compressed 33-byte BTC refund pubkey
                     --btc-locktime <u32>                   Bitcoin CLTV height or Unix timestamp
                     --btc-height <u32>                     Optional current BTC height for exact margin check

  decode-bolt11    Decode a Lightning BOLT-11 invoice and print its SHA-256 payment_hash & amount
                     <invoice>

  verify-preimage  Verify SHA256(preimage) == payment_hash
                     --hash <64hex> --preimage <64hex>

  watch            Verify live on-chain lock status on Newsatoshi L1 (+ optional Bitcoin P2WSH UTXO)
                     --hash <64hex>
                     --net <mainnet|testnet>                Newsatoshi network (default: mainnet)
                     --btc-address <bc1q...|tb1q...>        Optional Bitcoin P2WSH address (mempool.space)
                     --preimage <64hex>                     Optional preimage to check against hash
                     --seed-url <url>                       Optional custom Newsatoshi /status URL

  help             Show this help message",
        env!("CARGO_PKG_VERSION")
    );
}

fn find_flag(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == flag)
        .map(|w| w[1].clone())
}

fn run_keygen() -> Result<(), String> {
    let pair = generate_swap_secret()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&pair).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn run_htlc(args: &[String]) -> Result<(), String> {
    let req = CompileSwapRequest {
        rail: find_flag(args, "--rail").unwrap_or_else(|| "btc_onchain".to_string()),
        direction: find_flag(args, "--direction").unwrap_or_else(|| "btc_to_love".to_string()),
        love_network: find_flag(args, "--love-net").unwrap_or_else(|| "mainnet".to_string()),
        btc_network: find_flag(args, "--btc-net").unwrap_or_else(|| "mainnet".to_string()),
        payment_hash_hex: find_flag(args, "--hash"),
        bolt11_invoice: find_flag(args, "--bolt11"),
        amount_love: find_flag(args, "--love")
            .ok_or_else(|| "missing required --love <amount>".to_string())?,
        amount_btc_sats: match find_flag(args, "--sats") {
            Some(v) => Some(v.parse::<u64>().map_err(|_| format!("invalid --sats '{v}'"))?),
            None => None,
        },
        love_claim_address: find_flag(args, "--love-to")
            .ok_or_else(|| "missing required --love-to <ns1...>".to_string())?,
        love_timeout_blocks: match find_flag(args, "--love-blocks") {
            Some(v) => v
                .parse::<u32>()
                .map_err(|_| format!("invalid --love-blocks '{v}'"))?,
            None => 144,
        },
        btc_claim_pubkey_hex: find_flag(args, "--btc-claim-pk"),
        btc_refund_pubkey_hex: find_flag(args, "--btc-refund-pk"),
        btc_locktime: match find_flag(args, "--btc-locktime") {
            Some(v) => Some(
                v.parse::<u32>()
                    .map_err(|_| format!("invalid --btc-locktime '{v}'"))?,
            ),
            None => None,
        },
        btc_current_height: match find_flag(args, "--btc-height") {
            Some(v) => Some(
                v.parse::<u32>()
                    .map_err(|_| format!("invalid --btc-height '{v}'"))?,
            ),
            None => None,
        },
        source_asset: find_flag(args, "--asset"),
        solana_cluster: find_flag(args, "--solana-cluster"),
        solana_sender_pubkey: find_flag(args, "--sol-sender"),
        solana_recipient_pubkey: find_flag(args, "--sol-recipient"),
        solana_program_id: find_flag(args, "--sol-program"),
        solana_spl_mint: find_flag(args, "--sol-mint"),
        evm_chain: find_flag(args, "--evm-chain"),
        evm_recipient_address: find_flag(args, "--evm-recipient"),
        evm_token_address: find_flag(args, "--evm-token"),
        evm_contract_address: find_flag(args, "--evm-contract"),
        timelock_unix: match find_flag(args, "--timelock-unix") {
            Some(v) => Some(
                v.parse::<u64>()
                    .map_err(|_| format!("invalid --timelock-unix '{v}'"))?,
            ),
            None => None,
        },
        maker_label: find_flag(args, "--label"),
    };

    let contract = compile_atomic_swap(&req)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&contract).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn run_decode_bolt11(args: &[String]) -> Result<(), String> {
    let inv = args
        .first()
        .ok_or_else(|| "usage: newsatoshi-swap decode-bolt11 <bolt11_invoice>".to_string())?;
    let decoded = decode_bolt11_invoice(inv)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&decoded).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn run_verify_preimage(args: &[String]) -> Result<(), String> {
    let hash = find_flag(args, "--hash").ok_or_else(|| "missing --hash <64hex>".to_string())?;
    let preimage =
        find_flag(args, "--preimage").ok_or_else(|| "missing --preimage <64hex>".to_string())?;
    let valid = verify_preimage(&hash, &preimage)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "ok": true,
            "valid": valid,
            "payment_hash_hex": hash,
        }))
        .map_err(|e| e.to_string())?
    );
    if !valid {
        process::exit(2);
    }
    Ok(())
}

fn run_watch(args: &[String]) -> Result<(), String> {
    let hash = find_flag(args, "--hash").ok_or_else(|| "missing --hash <64hex>".to_string())?;
    let net = find_flag(args, "--net").unwrap_or_else(|| "mainnet".to_string());
    let seed_url = find_flag(args, "--seed-url").unwrap_or_else(|| {
        if net == "testnet" {
            DEFAULT_SEED_STATUS_URL.to_string()
        } else {
            DEFAULT_MAINNET_SEED_STATUS_URL.to_string()
        }
    });
    let btc_address = find_flag(args, "--btc-address");
    let preimage = find_flag(args, "--preimage");

    let client = SeedStatusClient::new(seed_url, Duration::from_secs(6));
    let report = verify_swap_on_chain(
        &client,
        &net,
        &hash,
        btc_address.as_deref(),
        preimage.as_deref(),
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
    );
    Ok(())
}
